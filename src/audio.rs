//! Captura WASAPI en modo compartido a un WAV: los parlantes predeterminados en loopback (audio del sistema) o el
//! micrófono predeterminado. Cuando el loopback no entrega paquetes (no suena nada) se escribe silencio, así la pista
//! mantiene el tiempo real y queda sincronizada con el video. Port de AudioCapture.cs, con COM llamado a mano.

use std::ffi::c_void;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::core::GUID;
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

const CLSID_MMDEVICE_ENUMERATOR: GUID = GUID::from_u128(0xBCDE0395_E52F_467C_8E3D_C4579291692E);
const IID_IMMDEVICE_ENUMERATOR: GUID = GUID::from_u128(0xA95664D2_9614_4F35_A746_DE8DB63617E6);
const IID_IAUDIO_CLIENT: GUID = GUID::from_u128(0x1CB9AD4C_DBFA_4c32_B178_C2F568A703B2);
const IID_IAUDIO_CAPTURE_CLIENT: GUID = GUID::from_u128(0xC8ADBD64_E71E_48a0_A4DE_185C395CD317);
const STREAMFLAGS_LOOPBACK: u32 = 0x0002_0000;
const BUFFERFLAGS_SILENT: u32 = 2;

/// Puntero a una función de la tabla virtual de un objeto COM.
unsafe fn vfn<T: Copy>(obj: *mut c_void, index: usize) -> T {
    let vtbl = *(obj as *const *const usize);
    std::mem::transmute_copy(&*vtbl.add(index))
}

unsafe fn release(obj: *mut c_void) {
    if !obj.is_null() {
        let f: unsafe extern "system" fn(*mut c_void) -> u32 = vfn(obj, 2);
        f(obj);
    }
}

fn hr(code: i32, what: &str) -> Result<(), String> {
    if code < 0 { Err(format!("{what} (0x{:08X})", code as u32)) } else { Ok(()) }
}

/// Último diagnóstico de cada captura (formato, paquetes, silencio agregado): para `--audiotest` y errores.
static LOG: Mutex<String> = Mutex::new(String::new());

pub fn last_log() -> String {
    LOG.lock().map(|l| l.clone()).unwrap_or_default()
}

fn log(line: String) {
    if let Ok(mut l) = LOG.lock() {
        if l.len() > 8000 {
            l.clear();
        }
        l.push_str(&line);
        l.push_str("\n");
    }
}

pub struct AudioCapture {
    running: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    data: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
    pub error: Option<String>,
}

impl AudioCapture {
    /// Abre el dispositivo y empieza a grabar en un hilo propio. Devuelve el error si no se puede abrir.
    pub fn start(loopback: bool, path: &Path) -> AudioCapture {
        let running = Arc::new(AtomicBool::new(true));
        let level = Arc::new(AtomicU32::new(0));
        let data = Arc::new(AtomicU64::new(0));
        let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let (r2, l2, d2, e2, rd2, p2) = (running.clone(), level.clone(), data.clone(), error.clone(), ready.clone(), path.to_path_buf());
        let worker = std::thread::Builder::new()
            .name(if loopback { "loopback".into() } else { "mic".into() })
            .spawn(move || {
                let result = unsafe { run(loopback, &p2, &r2, &l2, &d2, &rd2) };
                if let Err(e) = result {
                    *e2.lock().unwrap() = Some(e);
                    r2.store(false, Ordering::SeqCst);
                }
                let (m, cv) = &*rd2;
                *m.lock().unwrap() = true;
                cv.notify_all();
            })
            .ok();
        // Esperar a que el dispositivo abra (o falle), como mucho 5 s.
        let (m, cv) = &*ready;
        let guard = m.lock().unwrap();
        let _ = cv.wait_timeout_while(guard, Duration::from_secs(5), |done| !*done);
        let err = error.lock().unwrap().clone();
        AudioCapture { running, level, data, worker, error: err }
    }

    pub fn ok(&self) -> bool {
        self.error.is_none() && self.running.load(Ordering::SeqCst)
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    pub fn has_data(&self) -> bool {
        self.data.load(Ordering::SeqCst) > 0
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

unsafe fn run(loopback: bool, path: &PathBuf, running: &AtomicBool, level: &AtomicU32, data: &AtomicU64, ready: &(Mutex<bool>, std::sync::Condvar)) -> Result<(), String> {
    CoInitializeEx(null(), COINIT_MULTITHREADED as u32);
    let mut enumerator: *mut c_void = null_mut();
    let mut device: *mut c_void = null_mut();
    let mut client: *mut c_void = null_mut();
    let mut capture: *mut c_void = null_mut();
    let mut format: *mut u8 = null_mut();
    let result = (|| -> Result<(), String> {
        hr(CoCreateInstance(&CLSID_MMDEVICE_ENUMERATOR, null_mut(), CLSCTX_ALL, &IID_IMMDEVICE_ENUMERATOR, &mut enumerator), "MMDeviceEnumerator")?;
        // IMMDeviceEnumerator::GetDefaultAudioEndpoint(dataFlow: 0 render / 1 capture, role: 0 console)
        let get_default: unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> i32 = vfn(enumerator, 4);
        hr(get_default(enumerator, if loopback { 0 } else { 1 }, 0, &mut device), if loopback { "Sin parlantes" } else { "Sin micrófono" })?;
        // IMMDevice::Activate
        let activate: unsafe extern "system" fn(*mut c_void, *const GUID, u32, *const c_void, *mut *mut c_void) -> i32 = vfn(device, 3);
        hr(activate(device, &IID_IAUDIO_CLIENT, CLSCTX_ALL, null(), &mut client), "IAudioClient")?;
        // IAudioClient::GetMixFormat
        let get_mix: unsafe extern "system" fn(*mut c_void, *mut *mut u8) -> i32 = vfn(client, 8);
        hr(get_mix(client, &mut format), "GetMixFormat")?;
        let rd16 = |o: usize| u16::from_le_bytes([*format.add(o), *format.add(o + 1)]);
        let rd32 = |o: usize| u32::from_le_bytes([*format.add(o), *format.add(o + 1), *format.add(o + 2), *format.add(o + 3)]);
        let tag = rd16(0);
        let rate = rd32(4) as u64;
        let block = rd16(12) as usize;
        let bits = rd16(14);
        let extra = if tag == 0xFFFE { rd16(16) as usize } else { 0 };
        let is_float = tag == 3 || (tag == 0xFFFE && rd32(24) == 3);
        let channels = rd16(2).max(1) as usize;
        log(format!("{}: tag=0x{tag:04X} canales={channels} rate={rate} block={block} bits={bits} float={is_float}", if loopback { "sistema" } else { "mic" }));
        if block == 0 || rate == 0 {
            return Err(format!("Formato de audio no válido (block={block}, rate={rate})"));
        }
        // IAudioClient::Initialize(shared, flags, 1 s de búfer, 0, formato, null)
        let init: unsafe extern "system" fn(*mut c_void, i32, u32, i64, i64, *const u8, *const GUID) -> i32 = vfn(client, 3);
        hr(init(client, 0, if loopback { STREAMFLAGS_LOOPBACK } else { 0 }, 10_000_000, 0, format, null()), "Initialize")?;
        let get_service: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32 = vfn(client, 14);
        hr(get_service(client, &IID_IAUDIO_CAPTURE_CLIENT, &mut capture), "IAudioCaptureClient")?;

        // WAV con el mismo bloque de formato que entrega Windows.
        let fmt = std::slice::from_raw_parts(format, 18 + extra).to_vec();
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(b"RIFF\0\0\0\0WAVEfmt ").map_err(|e| e.to_string())?;
        file.write_all(&(fmt.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&fmt).map_err(|e| e.to_string())?;
        file.write_all(b"data\0\0\0\0").map_err(|e| e.to_string())?;
        let mut file = std::io::BufWriter::with_capacity(1 << 16, file);

        let start: unsafe extern "system" fn(*mut c_void) -> i32 = vfn(client, 10);
        hr(start(client), "Start")?;
        {
            let (m, cv) = ready;
            *m.lock().unwrap() = true;
            cv.notify_all();
        }
        let get_next: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32 = vfn(capture, 5);
        let get_buffer: unsafe extern "system" fn(*mut c_void, *mut *mut u8, *mut u32, *mut u32, *mut u64, *mut u64) -> i32 = vfn(capture, 3);
        let release_buffer: unsafe extern "system" fn(*mut c_void, u32) -> i32 = vfn(capture, 4);
        let clock = Instant::now();
        let mut frames: u64 = 0;
        let mut written: u64 = 0;
        let zeros = vec![0u8; (block * rate as usize / 10).max(block)];
        let zero_frames = (zeros.len() / block).max(1) as u64;
        let (mut packets, mut padded) = (0u64, 0u64);
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
            let mut got = false;
            let mut packet = 0u32;
            // Como mucho 64 paquetes por vuelta: si el dispositivo responde raro, no se queda girando.
            let mut spins = 0;
            while spins < 64 && get_next(capture, &mut packet) >= 0 && packet > 0 {
                spins += 1;
                let (mut ptr, mut count, mut flags) = (null_mut(), 0u32, 0u32);
                let code = get_buffer(capture, &mut ptr, &mut count, &mut flags, null_mut(), null_mut());
                // AUDCLNT_S_BUFFER_EMPTY (0x08890001) es un código de éxito sin datos.
                if code < 0 || code == 0x0889_0001 {
                    break;
                }
                if count == 0 {
                    release_buffer(capture, 0);
                    break;
                }
                packets += 1;
                let bytes = count as usize * block;
                if flags & BUFFERFLAGS_SILENT != 0 || ptr.is_null() {
                    let mut left = bytes;
                    while left > 0 {
                        let n = left.min(zeros.len());
                        let _ = file.write_all(&zeros[..n]);
                        left -= n;
                    }
                } else {
                    let slice = std::slice::from_raw_parts(ptr, bytes);
                    let _ = file.write_all(slice);
                    if is_float && bits == 32 {
                        let mut peak = 0f32;
                        for c in slice.chunks_exact(block.max(4)) {
                            peak = peak.max(f32::from_le_bytes([c[0], c[1], c[2], c[3]]).abs());
                        }
                        level.store(peak.min(1.0).to_bits(), Ordering::Relaxed);
                    }
                }
                release_buffer(capture, count);
                frames += count as u64;
                written += bytes as u64;
                got = true;
            }
            // El loopback se calla si no suena nada: completar con silencio para mantener el tiempo real.
            let due = (clock.elapsed().as_secs_f64() * rate as f64) as u64;
            if !got && due > frames + rate / 10 {
                let mut pad = due - frames - rate / 50;
                while pad > 0 {
                    let n = pad.min(zero_frames);
                    let _ = file.write_all(&zeros[..n as usize * block]);
                    frames += n;
                    written += n * block as u64;
                    pad -= n;
                    padded += n;
                }
                level.store(0f32.to_bits(), Ordering::Relaxed);
            }
            data.store(written, Ordering::SeqCst);
        }
        let stop: unsafe extern "system" fn(*mut c_void) -> i32 = vfn(client, 11);
        stop(client);
        log(format!("  paquetes={packets} cuadros={frames} silencio={padded} bytes={written} en {:.1}s", clock.elapsed().as_secs_f64()));
        file.flush().map_err(|e| e.to_string())?;
        // Completar los tamaños del encabezado RIFF.
        let mut f = file.into_inner().map_err(|e| e.to_string())?;
        let total = f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        let data_pos = total - written - 4;
        let _ = f.seek(SeekFrom::Start(4)).and_then(|_| f.write_all(&((total - 8).min(u32::MAX as u64) as u32).to_le_bytes()));
        let _ = f.seek(SeekFrom::Start(data_pos)).and_then(|_| f.write_all(&(written.min(u32::MAX as u64) as u32).to_le_bytes()));
        Ok(())
    })();
    if !format.is_null() {
        CoTaskMemFree(format as _);
    }
    release(capture);
    release(client);
    release(device);
    release(enumerator);
    CoUninitialize();
    result
}
