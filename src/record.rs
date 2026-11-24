//! Grabación de pantalla y/o audio. Video: FFmpeg (gdigrab) codifica el área a H.264 mientras WASAPI graba el
//! audio del sistema (loopback) y el micrófono a WAV; al detener, FFmpeg mezcla las pistas en un MP4. Solo audio:
//! las pistas WAV se mezclan en un MP3. Todo va a Videos\BuzzPrint con su archivo de metadatos (título, origen,
//! duración) y una miniatura (primer cuadro, o la forma de onda del audio). Port de Recorder.cs.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::RECT;

use crate::audio::AudioCapture;
use crate::clock::Stamp;
use crate::i18n::t;
use crate::store;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Video,
    Audio,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    Starting,
    Running,
    Failed(String),
}

pub struct Recorder {
    pub kind: Kind,
    pub area: RECT,
    pub source: String,
    id: String,
    work: PathBuf,
    child: Option<Child>,
    ff_state: Arc<AtomicU8>, // 0 arrancando, 1 grabando, 2 terminó
    log: Arc<Mutex<VecDeque<String>>>,
    sys: Option<AudioCapture>,
    mic: Option<AudioCapture>,
    started: Option<Instant>,
    asked: Instant,
    pub warning: Option<String>,
    status: Status,
    /// Audio de esta grabación: (audio de la PC, micrófono).
    audio: (bool, bool),
}

thread_local! {
    static CURRENT: RefCell<Option<Recorder>> = const { RefCell::new(None) };
}

/// Resultado del guardado (lo deja el hilo que arma el archivo final).
pub static DONE: Mutex<Option<Result<String, String>>> = Mutex::new(None);

pub fn ffmpeg() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = exe_dir {
        candidates.push(d.join("tools").join("ffmpeg.exe"));
    }
    candidates.push(store::data_dir().join("tools").join("ffmpeg.exe")); // la descarga de la versión .NET
    for dir in std::env::var("PATH").unwrap_or_default().split(';') {
        if !dir.trim().is_empty() {
            candidates.push(PathBuf::from(dir.trim()).join("ffmpeg.exe"));
        }
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(l).join("Microsoft").join("WinGet").join("Links").join("ffmpeg.exe"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn command(exe: &Path, args: &[String]) -> Command {
    let mut c = Command::new(exe);
    c.args(args).creation_flags(CREATE_NO_WINDOW).env("__COMPAT_LAYER", "HighDpiAware");
    c
}

pub fn is_recording() -> bool {
    CURRENT.with(|c| c.borrow().is_some())
}

pub fn with<R>(f: impl FnOnce(&mut Recorder) -> R) -> Option<R> {
    CURRENT.with(|c| c.borrow_mut().as_mut().map(f))
}

thread_local! {
    /// Audio elegido para la próxima grabación desde el menú (sin audio, PC, micrófono o ambos).
    static NEXT_AUDIO: std::cell::Cell<Option<(bool, bool)>> = const { std::cell::Cell::new(None) };
}

/// La próxima grabación de video usa este audio (audio de la PC, micrófono) en lugar de lo que diga Opciones.
pub fn set_next_audio(system: bool, mic: bool) {
    NEXT_AUDIO.with(|n| n.set(Some((system, mic))));
}

pub fn clear_next_audio() {
    NEXT_AUDIO.with(|n| n.set(None));
}

/// Empieza a grabar. El video confirma el arranque de FFmpeg de a poco (ver `poll`).
pub fn start(kind: Kind, area: RECT, source: String) -> Result<(), String> {
    if is_recording() {
        return Err(t("Ya hay una grabación en curso."));
    }
    let exe = ffmpeg().ok_or_else(|| t("No se encontró ffmpeg.exe. Instalalo con: winget install Gyan.FFmpeg"))?;
    let mut st = store::settings();
    if let Some((sys, mic)) = NEXT_AUDIO.with(|n| n.take()) {
        st.record_system_audio = sys;
        st.record_microphone = mic;
    }
    if kind == Kind::Audio && !st.record_system_audio && !st.record_microphone {
        return Err(t("Activá el audio del sistema o el micrófono para grabar solo audio."));
    }
    // H.264 necesita lados pares.
    let area = RECT { left: area.left, top: area.top, right: area.left + ((area.right - area.left) & !1), bottom: area.top + ((area.bottom - area.top) & !1) };
    let id = Stamp::now().id();
    let work = std::env::temp_dir().join("BuzzPrint").join(format!("rec-{id}"));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let mut rec = Recorder {
        kind,
        area,
        source,
        id,
        work,
        child: None,
        ff_state: Arc::new(AtomicU8::new(0)),
        log: Arc::new(Mutex::new(VecDeque::new())),
        sys: None,
        mic: None,
        started: None,
        asked: Instant::now(),
        warning: None,
        status: Status::Starting,
        audio: (st.record_system_audio, st.record_microphone),
    };
    if kind == Kind::Video {
        let (w, h) = (area.right - area.left, area.bottom - area.top);
        if w < 16 || h < 16 {
            return Err(t("El área es demasiado chica para grabar."));
        }
        let args: Vec<String> = format!(
            "-hide_banner -y -f gdigrab -framerate {} -draw_mouse {} -offset_x {} -offset_y {} -video_size {}x{} -i desktop -c:v libx264 -preset ultrafast -crf 23 -pix_fmt yuv420p",
            st.record_fps.clamp(5, 60), st.record_cursor as i32, area.left, area.top, w, h
        )
        .split(' ')
        .map(String::from)
        .chain(std::iter::once(rec.work.join("video.mkv").display().to_string()))
        .collect();
        let mut child = command(&exe, &args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("{}{e}", t("No se pudo iniciar ffmpeg: ")))?;
        let stderr = child.stderr.take().unwrap();
        let (state, log) = (rec.ff_state.clone(), rec.log.clone());
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).split(b'\r').flat_map(|l| l.ok()) {
                let line = String::from_utf8_lossy(&line).trim().to_string();
                if line.contains("Press [q]") || line.starts_with("frame=") {
                    let _ = state.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst);
                }
                let mut l = log.lock().unwrap();
                l.push_back(line);
                if l.len() > 30 {
                    l.pop_front();
                }
            }
            state.store(2, Ordering::SeqCst);
        });
        rec.child = Some(child);
    } else {
        rec.begin_audio(&st);
        if rec.sys.is_none() && rec.mic.is_none() {
            let w = rec.warning.clone().unwrap_or_default();
            let _ = std::fs::remove_dir_all(&rec.work);
            return Err(format!("{}\n{w}", t("No se pudo abrir ningún dispositivo de audio.")));
        }
        rec.started = Some(Instant::now());
        rec.status = Status::Running;
    }
    CURRENT.with(|c| *c.borrow_mut() = Some(rec));
    Ok(())
}

impl Recorder {
    fn begin_audio(&mut self, st: &store::Settings) {
        if st.record_system_audio {
            let a = AudioCapture::start(true, &self.work.join("system.wav"));
            if a.ok() {
                self.sys = Some(a);
            } else {
                self.warning = Some(format!("{}{}", t("Audio del sistema no disponible: "), a.error.clone().unwrap_or_default()));
            }
        }
        if st.record_microphone {
            let a = AudioCapture::start(false, &self.work.join("mic.wav"));
            if a.ok() {
                self.mic = Some(a);
            } else {
                let m = format!("{}{}", t("Micrófono no disponible: "), a.error.clone().unwrap_or_default());
                self.warning = Some(match self.warning.take() {
                    Some(w) => format!("{w}\n{m}"),
                    None => m,
                });
            }
        }
    }

    fn tail(&self) -> String {
        let l = self.log.lock().unwrap();
        l.iter().rev().take(6).rev().cloned().collect::<Vec<_>>().join("\n")
    }

    pub fn elapsed(&self) -> Duration {
        self.started.map(|s| s.elapsed()).unwrap_or_default()
    }

    pub fn level(&self) -> f32 {
        self.sys.as_ref().map(|a| a.level()).unwrap_or(0.0).max(self.mic.as_ref().map(|a| a.level()).unwrap_or(0.0))
    }

    pub fn with_system(&self) -> bool {
        self.sys.is_some()
    }

    pub fn with_mic(&self) -> bool {
        self.mic.is_some()
    }

    /// Avanza el arranque: cuando FFmpeg ya graba, arranca el audio y el reloj.
    pub fn poll(&mut self) -> Status {
        if self.status == Status::Starting {
            match self.ff_state.load(Ordering::SeqCst) {
                1 => {
                    let mut st = store::settings();
                    (st.record_system_audio, st.record_microphone) = self.audio;
                    self.begin_audio(&st);
                    self.started = Some(Instant::now());
                    self.status = Status::Running;
                }
                2 => self.status = Status::Failed(format!("{}\n{}", t("ffmpeg no pudo empezar a grabar."), self.tail())),
                _ if self.asked.elapsed() > Duration::from_secs(15) => {
                    if let Some(c) = self.child.as_mut() {
                        let _ = c.kill();
                    }
                    self.status = Status::Failed(format!("{}\n{}", t("ffmpeg no pudo empezar a grabar."), self.tail()));
                }
                _ => {}
            }
        }
        self.status.clone()
    }
}

/// Detiene la grabación. Con `keep`, arma el archivo final en un hilo aparte y avisa al núcleo al terminar.
pub fn stop(keep: bool) {
    let Some(mut rec) = CURRENT.with(|c| c.borrow_mut().take()) else { return };
    let seconds = rec.elapsed().as_secs_f64();
    let main = crate::app::main_hwnd() as isize;
    let (record_dir, meta_dir) = store::with(|s| (s.record_dir.clone(), s.record_meta_dir.clone()));
    let accent = crate::export::app_accent();
    let lang_es = crate::i18n::es();
    std::thread::spawn(move || {
        if let Some(mut c) = rec.child.take() {
            if let Some(mut stdin) = c.stdin.take() {
                let _ = stdin.write_all(b"q");
                let _ = stdin.flush();
            }
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_secs(10) {
                if let Ok(Some(_)) = c.try_wait() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(a) = rec.sys.as_mut() {
            a.stop();
        }
        if let Some(a) = rec.mic.as_mut() {
            a.stop();
        }
        let result = if keep { finish(&rec, seconds, &record_dir, &meta_dir, &accent, lang_es) } else { Ok(String::new()) };
        let _ = std::fs::remove_dir_all(&rec.work);
        *DONE.lock().unwrap() = Some(result);
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(main as _, crate::app::WM_RECORD_DONE, keep as usize, 0);
        }
    });
}

fn finish(rec: &Recorder, seconds: f64, record_dir: &Path, meta_dir: &Path, accent: &str, _es: bool) -> Result<String, String> {
    let exe = ffmpeg().ok_or_else(|| t("No se encontró ffmpeg.exe."))?;
    std::fs::create_dir_all(record_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(meta_dir).map_err(|e| e.to_string())?;
    let mut tracks: Vec<PathBuf> = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    if rec.sys.as_ref().is_some_and(|a| a.has_data()) {
        tracks.push(rec.work.join("system.wav"));
        names.push("system");
    }
    if rec.mic.as_ref().is_some_and(|a| a.has_data()) {
        tracks.push(rec.work.join("mic.wav"));
        names.push("mic");
    }
    let video = rec.kind == Kind::Video;
    if !video && tracks.is_empty() {
        return Err(t("La grabación no tiene audio."));
    }
    let output = record_dir.join(format!("{}.{}", rec.id, if video { "mp4" } else { "mp3" }));
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-y".into()];
    if video {
        args.push("-i".into());
        args.push(rec.work.join("video.mkv").display().to_string());
    }
    for tr in &tracks {
        args.push("-i".into());
        args.push(tr.display().to_string());
    }
    let a0 = if video { 1 } else { 0 };
    if tracks.len() == 2 {
        args.push("-filter_complex".into());
        args.push(format!("[{a0}:a][{}:a]amix=inputs=2:duration=longest:normalize=0[a]", a0 + 1));
    }
    if video {
        args.extend(["-map".into(), "0:v".into()]);
        if tracks.len() == 2 {
            args.extend(["-map".into(), "[a]".into()]);
        } else if tracks.len() == 1 {
            args.extend(["-map".into(), "1:a".into()]);
        }
        args.extend(["-c:v".into(), "copy".into()]);
        if !tracks.is_empty() {
            args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-shortest".into()]);
        }
        args.extend(["-movflags".into(), "+faststart".into()]);
    } else {
        if tracks.len() == 2 {
            args.extend(["-map".into(), "[a]".into()]);
        }
        args.extend(["-c:a".into(), "libmp3lame".into(), "-b:a".into(), "192k".into()]);
    }
    args.push(output.display().to_string());
    let out = command(&exe, &args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
    if !out.status.success() || !output.exists() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
        return Err(format!("{}\n{}", t("No se pudo guardar la grabación."), tail[tail.len().saturating_sub(5)..].join("\n")));
    }
    // Miniatura: primer cuadro, o la forma de onda del audio en el color de acento.
    let thumb = meta_dir.join(format!("{}.png", rec.id));
    let thumb_args: Vec<String> = if video {
        vec![
            "-hide_banner".into(), "-y".into(), "-ss".into(), format!("{:.2}", (seconds / 2.0).min(0.5)), "-i".into(), output.display().to_string(),
            "-frames:v".into(), "1".into(), "-vf".into(), "scale='min(640,iw)':-2".into(), thumb.display().to_string(),
        ]
    } else {
        vec![
            "-hide_banner".into(), "-y".into(), "-i".into(), output.display().to_string(), "-filter_complex".into(),
            format!("showwavespic=s=640x260:split_channels=0:colors=0x{accent}"), "-frames:v".into(), "1".into(), thumb.display().to_string(),
        ]
    };
    let _ = command(&exe, &thumb_args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();

    let mut meta = serde_json::Map::new();
    meta.insert("source".into(), rec.source.clone().into());
    meta.insert("duration".into(), ((seconds * 10.0).round() / 10.0).into());
    meta.insert("kind".into(), if video { "video" } else { "audio" }.into());
    meta.insert("title".into(), store::default_name(&rec.source, &Stamp::now(), Some(if video { "Video" } else { "Audio" })).into());
    meta.insert("audio".into(), names.join("+").into());
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(meta)).map_err(|e| e.to_string())?;
    std::fs::write(meta_dir.join(format!("{}.json", rec.id)), text).map_err(|e| e.to_string())?;
    Ok(rec.id.clone())
}

pub fn clock_text(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}
