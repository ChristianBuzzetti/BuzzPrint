//! BuzzPrint (Rust): capturas de pantalla, biblioteca, editor y exportación para Windows.
//!
//! Principios: un solo proceso nativo sin runtime; en reposo cero trabajo (el hilo duerme en
//! `GetMessageW`); cada ventana crea su contexto gráfico al abrirse y lo libera al cerrarse; la memoria
//! grande de una captura se libera al terminar. Comparte biblioteca y configuración con BuzzPrint .NET.

#![windows_subsystem = "windows"]

mod ai;
mod app;
mod boberto;
mod audio;
mod capture;
mod clock;
mod crypto;
mod deps;
mod dialogs;
mod dragdrop;
mod export;
mod fondos;
mod gdip;
mod gui;
mod i18n;
mod image;
mod marks;
mod net;
mod ocr;
mod output;
mod overlay;
mod record;
mod report_format;
mod store;
mod theme;
mod unzip;
mod watch;
mod win;

use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |a: &str| args.iter().any(|x| x == a);
    if has("--capture-screen") {
        std::process::exit(app::capture_screen_headless());
    }
    if let Some(i) = args.iter().position(|a| a == "--selftest") {
        let dir = args.get(i + 1).cloned().unwrap_or_else(|| std::env::temp_dir().join("buzzprint-selftest").display().to_string());
        std::process::exit(app::selftest(std::path::Path::new(&dir)));
    }
    if let Some(i) = args.iter().position(|a| a == "--audiotest") {
        // Graba N segundos del audio del sistema y del micrófono a WAV y reporta tamaños (diagnóstico).
        let secs: u64 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(3);
        let dir = std::env::temp_dir().join("buzzprint-audiotest");
        let _ = std::fs::create_dir_all(&dir);
        let mut out = String::new();
        for (lb, name) in [(true, "system.wav"), (false, "mic.wav")] {
            let t0 = std::time::Instant::now();
            let mut a = audio::AudioCapture::start(lb, &dir.join(name));
            out += &format!("{name}: abrir {:?} ok={} err={:?}\n", t0.elapsed(), a.ok(), a.error);
            std::thread::sleep(std::time::Duration::from_secs(secs));
            let t1 = std::time::Instant::now();
            a.stop();
            let len = std::fs::metadata(dir.join(name)).map(|m| m.len()).unwrap_or(0);
            out += &format!("{name}: detener {:?} bytes={len}\n{}\n", t1.elapsed(), audio::last_log());
        }
        let _ = std::fs::write(dir.join("result.txt"), &out);
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--ffmpegtest") {
        // Baja y verifica FFmpeg en una carpeta de prueba, sin interfaz (diagnóstico de red y ZIP).
        let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| std::env::temp_dir().join("buzzprint-ffmpegtest").display().to_string()));
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let t0 = std::time::Instant::now();
        let mut last = 0u64;
        let _ = std::fs::create_dir_all(&dir);
        let r = deps::fetch_ffmpeg(&dir.join("ffmpeg.exe"), &dir.join("pkg.zip"), &cancel, &mut |d, _| last = d, &mut |_| {});
        let _ = std::fs::remove_file(dir.join("pkg.zip"));
        let _ = std::fs::write(dir.join("result.txt"), format!("{r:?} bajados={last} en {:?}", t0.elapsed()));
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--ocrtest") {
        // Lee con OCR la captura más nueva (o la indicada) y cuenta cuántas del caché de .NET siguen válidas.
        let items: Vec<store::Item> = store::with(|s| s.all().iter().filter(|i| !i.is_recording()).cloned().collect());
        let log = std::env::temp_dir().join("buzzprint-ocrtest.txt");
        let _ = std::fs::write(&log, format!("items={}\n", items.len()));
        let mut out = format!("soportado={}\n", ocr::supported());
        let _ = std::fs::write(&log, &out);
        let hits = items.iter().filter(|i| ocr::cached(i).is_some()).count();
        out += &format!("caché válido {hits} de {}\n", items.len());
        let target = args.get(i + 1).cloned();
        if let Some(it) = items.iter().find(|i| target.as_deref().map_or(true, |t| i.id == t)) {
            let t0 = std::time::Instant::now();
            let r = std::thread::spawn({
                let p = it.path.clone();
                move || ocr::engine_recognize(&p)
            })
            .join()
            .ok()
            .flatten();
            out += &format!("{} en {:?}: {:?}\n", it.id, t0.elapsed(), r.map(|r| (r.language, r.words.len(), r.text.chars().take(300).collect::<String>())));
        }
        let _ = std::fs::write(std::env::temp_dir().join("buzzprint-ocrtest.txt"), out);
        return;
    }
    if has("--aitest") {
        // Prueba la conexión activa con un pedido de texto (sin capturas) y lista sus modelos.
        let mut out = String::new();
        let mut s = store::settings();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        match ai::active(&mut s) {
            Some(c) => {
                out += &format!("conexión: {} key={}\n", ai::display_name(&c), ai::keys::load(&c.id, &c.provider).map(|k| ai::keys::masked(&k)).unwrap_or("-".into()));
                let r = ai::Client::from_connection(&c, None).and_then(|cl| {
                    cl.complete(&ai::AiRequest { system: "Reply with one short sentence.".into(), prompt: "Say hello.".into(), images: vec![], max_tokens: 1500 }, &cancel)
                });
                out += &format!("respuesta: {r:?}\n");
                out += &format!("modelos: {:?}\n", ai::Client::list_models(&c, None, &cancel));
            }
            None => out += "sin conexión\n",
        }
        let _ = std::fs::write(std::env::temp_dir().join("buzzprint-aitest.txt"), out);
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--marktest") {
        let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| std::env::temp_dir().join("buzzprint-marktest").display().to_string()));
        let _ = gui::markicon::selftest(&dir);
        return;
    }
    if has("--version") {
        println!("buzzprint {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // Al reiniciar (cambio de idioma) se espera a que la instancia anterior termine de cerrarse.
    let mut tries = if has("--relaunch") { 40 } else { 0 };
    let mut first = win::single_instance("Local\\BuzzPrintRs.SingleInstance");
    while !first && tries > 0 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        tries -= 1;
        first = win::single_instance("Local\\BuzzPrintRs.SingleInstance");
    }
    if !first {
        // Ya está abierto: que muestre su biblioteca.
        unsafe {
            let h = FindWindowW(win::wide("BuzzPrintRsMain").as_ptr(), std::ptr::null());
            if !h.is_null() {
                PostMessageW(h, app::WM_SHOW_LIBRARY, 0, 0);
            }
        }
        return;
    }
    let lang = store::settings().language;
    i18n::set_language(if lang == "es" { "es" } else { "en" });
    app::run(has("--tray"));
}
