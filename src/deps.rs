//! Componentes opcionales que la app baja sola, como la versión .NET (Dependencies.cs): FFmpeg para grabar, de las
//! compilaciones oficiales de gyan.dev, verificado contra el SHA-256 publicado y guardado en
//! %LOCALAPPDATA%\BuzzPrint\tools. WebView2 solo se detecta (Boberto lo usa).

use std::path::PathBuf;

use crate::app::post_from_thread;
use crate::gui::prompt;
use crate::i18n::t;
use crate::store;

const FFMPEG_ZIP: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

pub fn ffmpeg_home() -> PathBuf {
    store::data_dir().join("tools").join("ffmpeg.exe")
}

/// Versión del runtime WebView2 instalada (registro de EdgeUpdate), si hay.
pub fn webview2_version() -> Option<String> {
    use windows_sys::Win32::System::Registry::*;
    let guid = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let keys = [
        (HKEY_LOCAL_MACHINE, format!("SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{guid}")),
        (HKEY_LOCAL_MACHINE, format!("SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{guid}")),
        (HKEY_CURRENT_USER, format!("Software\\Microsoft\\EdgeUpdate\\Clients\\{guid}")),
    ];
    for (root, key) in keys {
        let k = crate::win::wide(&key);
        let name = crate::win::wide("pv");
        let mut buf = [0u16; 64];
        let mut size = (buf.len() * 2) as u32;
        let rc = unsafe { RegGetValueW(root, k.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _, &mut size) };
        if rc == 0 {
            let v = String::from_utf16_lossy(&buf).trim_matches('\0').to_string();
            if !v.is_empty() && v != "0.0.0.0" {
                return Some(v);
            }
        }
    }
    None
}

/// Pregunta y baja FFmpeg. `then` corre en el hilo de la interfaz si quedó instalado.
pub fn offer_ffmpeg(then: impl FnOnce() + Send + 'static) {
    prompt::confirm(
        &t("Falta FFmpeg"),
        &t("Para grabar video y audio, BuzzPrint usa FFmpeg (gratuito y de código abierto). Lo puede descargar ahora desde gyan.dev, las compilaciones oficiales para Windows que recomienda ffmpeg.org (unos 110 MB), y guardarlo en su carpeta. Las capturas, el editor y la exportación funcionan sin él."),
        &t("Descargar FFmpeg"),
        &t("Ahora no"),
        move || download_ffmpeg(then),
    );
}

/// Baja el paquete a `temp`, lo verifica y deja `ffmpeg.exe` (y su licencia) en `home`. `stage`: 0 verificando,
/// 1 descomprimiendo. Error "sha" si la firma no coincide.
pub fn fetch_ffmpeg(
    home: &std::path::Path,
    temp: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &mut dyn FnMut(u64, Option<u64>),
    stage: &mut dyn FnMut(u8),
) -> Result<(), String> {
    let expected = crate::net::get_text(&format!("{FFMPEG_ZIP}.sha256"), Some(cancel))?;
    let expected = expected.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    crate::net::download(FFMPEG_ZIP, temp, Some(cancel), progress)?;
    stage(0);
    let actual = crate::crypto::sha256_file(temp)?;
    if expected.len() != 64 || actual != expected {
        return Err("sha".into());
    }
    stage(1);
    let mut zip = crate::unzip::Zip::open(temp)?;
    let exe = zip.find(|n| n.to_ascii_lowercase().ends_with("/bin/ffmpeg.exe")).ok_or("ffmpeg.exe not found in the package.")?;
    let bytes = zip.read(exe)?;
    std::fs::create_dir_all(home.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(home, bytes).map_err(|e| e.to_string())?;
    // Su licencia viaja con él (FFmpeg es GPL; BuzzPrint solo lo baja para el usuario).
    if let Some(l) = zip.find(|n| n.to_ascii_lowercase().ends_with("/license")) {
        if let Ok(b) = zip.read(l) {
            let _ = std::fs::write(home.with_file_name("FFmpeg-LICENSE.txt"), b);
        }
    }
    Ok(())
}

pub fn download_ffmpeg(then: impl FnOnce() + Send + 'static) {
    let p = prompt::progress(&t("Descargando FFmpeg"));
    let (s_verify, s_unzip) = (t("Verificando…"), t("Descomprimiendo…"));
    let (e_sha, e_title, e_manual) = (
        t("La descarga no coincide con la firma SHA-256 publicada. Probá de nuevo."),
        t("No se pudo descargar FFmpeg"),
        t("Podés instalarlo a mano con:"),
    );
    std::thread::spawn(move || {
        let temp = std::env::temp_dir().join(format!("BuzzPrint-ffmpeg-{}.zip", std::process::id()));
        let result = fetch_ffmpeg(&ffmpeg_home(), &temp, &p.cancel, &mut |d, tot| p.report(d, tot), &mut |st| {
            p.status(if st == 0 { &s_verify } else { &s_unzip })
        })
        .map_err(|e| if e == "sha" { e_sha.clone() } else { e });
        let _ = std::fs::remove_file(&temp);
        let cancelled = p.cancelled();
        p.finish();
        post_from_thread(move || {
            crate::app::trim_memory();
            match result {
                Ok(()) => then(),
                Err(_) if cancelled => {}
                Err(e) => prompt::info(&e_title, &format!("{e}\n\n{e_manual} winget install Gyan.FFmpeg")),
            }
        });
    });
}
