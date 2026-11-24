//! Texto de las capturas con el OCR que trae Windows (Windows.Media.Ocr): sin internet, gratis, nada sale de la PC.
//! Port de Ocr.cs: los resultados se guardan por captura en %LOCALAPPDATA%\BuzzPrint\ocr\<id>.json con el mismo
//! formato que .NET (las dos versiones comparten el caché) y se renuevan si la captura cambia (sello = escritura
//! UTC en ticks de .NET XOR tamaño). Un hilo de fondo lee de a una las capturas que falten, con pausas.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::store::{self, Item};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Word {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct OcrResult {
    pub text: String,
    pub words: Vec<Word>,
    pub language: String,
    pub stamp: i64,
}

fn cache_dir() -> PathBuf {
    store::data_dir().join("ocr")
}

fn cache_path(id: &str) -> PathBuf {
    cache_dir().join(format!("{id}.json"))
}

/// `LastWriteTimeUtc.Ticks ^ Length`, igual que .NET.
fn stamp_of(path: &Path) -> i64 {
    let Ok(m) = std::fs::metadata(path) else { return 0 };
    let ticks = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64 * 10_000_000 + d.subsec_nanos() as i64 / 100 + 621_355_968_000_000_000)
        .unwrap_or(0);
    ticks ^ m.len() as i64
}

/// Resultado guardado si sigue al día con la captura.
pub fn cached(it: &Item) -> Option<OcrResult> {
    let text = std::fs::read_to_string(cache_path(&it.id)).ok()?;
    let r: OcrResult = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    (r.stamp == stamp_of(&it.path)).then_some(r)
}

/// Lo guardado, o lee la captura ahora y lo guarda. Llamar desde un hilo de fondo.
pub fn get(it: &Item) -> Option<OcrResult> {
    if it.is_recording() {
        return None;
    }
    if let Some(c) = cached(it) {
        return Some(c);
    }
    let stamp = stamp_of(&it.path);
    let mut r = engine::recognize(&it.path)?;
    r.stamp = stamp;
    let _ = std::fs::create_dir_all(cache_dir());
    if let Ok(s) = serde_json::to_string(&r) {
        let _ = std::fs::write(cache_path(&it.id), s);
    }
    Some(r)
}

pub fn supported() -> bool {
    static S: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *S.get_or_init(|| std::thread::spawn(engine::available).join().unwrap_or(false))
}

// ------------------------------------------------------------------ índice para la búsqueda

static INDEX: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static RUN: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
static PROGRESS: Mutex<(usize, usize)> = Mutex::new((0, 0));
static REV: AtomicU64 = AtomicU64::new(0);

/// Cambia cada vez que entra texto nuevo al índice (la búsqueda se vuelve a filtrar).
pub fn rev() -> u64 {
    REV.load(Ordering::SeqCst)
}

fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Sin distinguir mayúsculas ni acentos.
pub fn matches(id: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let q = fold(query.trim());
    INDEX.lock().unwrap().as_ref().and_then(|m| m.get(id)).is_some_and(|t| t.contains(&q))
}

pub fn text_of(id: &str) -> Option<String> {
    std::fs::read_to_string(cache_path(id)).ok().and_then(|s| serde_json::from_str::<OcrResult>(s.trim_start_matches('\u{feff}')).ok()).map(|r| r.text)
}

fn remember(id: &str, text: &str) {
    INDEX.lock().unwrap().get_or_insert_with(HashMap::new).insert(id.to_string(), fold(text));
    REV.fetch_add(1, Ordering::SeqCst);
}

/// (hechas, total) del pase en curso; total 0 = nada pendiente.
pub fn progress() -> (usize, usize) {
    *PROGRESS.lock().unwrap()
}

/// Copia al portapapeles el texto de las capturas (lee con OCR las que falten, en segundo plano).
pub fn copy_text(items: Vec<Item>) {
    let (none, done, unsupported) = (crate::i18n::t("No se encontró texto en la imagen."), crate::i18n::t("Texto copiado"), crate::i18n::t("El OCR de Windows no está disponible: falta un idioma con reconocimiento de texto (Configuración → Hora e idioma → Idioma)."));
    std::thread::spawn(move || {
        if !supported() {
            crate::app::post_from_thread(move || crate::app::notify(&crate::i18n::t("Copiar el texto de la imagen"), &unsupported, None));
            return;
        }
        let texts: Vec<String> = items.iter().filter_map(|it| get(it).map(|r| { remember(&it.id, &r.text); r.text })).filter(|t| !t.trim().is_empty()).collect();
        let text = texts.join("\n\n");
        crate::app::post_from_thread(move || {
            if text.is_empty() {
                crate::app::notify(&crate::i18n::t("Copiar el texto de la imagen"), &none, None);
            } else {
                crate::gui::host::set_clipboard_text(crate::app::main_hwnd(), &text);
                let lines = text.lines().count();
                crate::app::notify(&done, &format!("{lines} {}", crate::i18n::t("líneas")), None);
            }
            crate::app::trim_memory();
        });
    });
}

/// Arranca (o reinicia) el pase de fondo sobre la biblioteca. Barato si todo está en caché.
pub fn refresh() {
    let items: Vec<Item> = store::with(|s| s.all().iter().filter(|i| !i.is_recording()).cloned().collect());
    let run = RUN.fetch_add(1, Ordering::SeqCst) + 1;
    let alive = Arc::new(items);
    std::thread::Builder::new()
        .name("ocr".into())
        .spawn(move || {
            while BUSY.swap(true, Ordering::SeqCst) {
                if RUN.load(Ordering::SeqCst) != run {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            pass(run, &alive);
            BUSY.store(false, Ordering::SeqCst);
        })
        .ok();
}

fn pass(run: u64, items: &[Item]) {
    if !supported() {
        return;
    }
    let mut todo = Vec::new();
    for it in items {
        match cached(it) {
            Some(c) => remember(&it.id, &c.text),
            None => todo.push(it.clone()),
        }
    }
    prune(items);
    crate::app::post_from_thread(crate::gui::host::wake_all);
    let mut since = 0;
    for (n, it) in todo.iter().enumerate() {
        if RUN.load(Ordering::SeqCst) != run {
            return;
        }
        *PROGRESS.lock().unwrap() = (n, todo.len());
        if let Some(r) = get(it) {
            remember(&it.id, &r.text);
            since += 1;
        }
        if since >= 5 {
            since = 0;
            crate::app::post_from_thread(crate::gui::host::wake_all);
        }
        std::thread::sleep(std::time::Duration::from_millis(150)); // liviano: de a una, con pausa
    }
    *PROGRESS.lock().unwrap() = (0, 0);
    crate::app::post_from_thread(|| {
        crate::gui::host::wake_all();
        crate::app::trim_memory();
    });
}

/// Borra el caché de capturas que ya no existen.
fn prune(alive: &[Item]) {
    let ids: std::collections::HashSet<&str> = alive.iter().map(|i| i.id.as_str()).collect();
    if let Ok(rd) = std::fs::read_dir(cache_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if !ids.contains(stem) && !stem.contains("_editada") {
                        let _ = std::fs::remove_file(&p);
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ motor de Windows (WinRT)

mod engine {
    use super::{OcrResult, Word};
    use std::path::Path;
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode, ExifOrientationMode};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    pub fn available() -> bool {
        crate::win::ensure_mta();
        OcrEngine::TryCreateFromUserProfileLanguages().is_ok()
    }

    pub fn recognize(path: &Path) -> Option<OcrResult> {
        crate::win::ensure_mta();
        run(path).ok()
    }

    fn trace(s: &str) {
        if std::env::var_os("BUZZPRINT_DEBUG").is_some() {
            use std::io::Write;
            let _ = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("buzzprint-ocr.log")).and_then(|mut f| writeln!(f, "{s}"));
        }
    }

    fn run(path: &Path) -> windows::core::Result<OcrResult> {
        trace("engine");
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
        let bytes = std::fs::read(path).map_err(|_| windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_0002u32 as i32)))?;
        trace("stream");
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(&bytes)?;
        trace("store");
        writer.StoreAsync()?.get()?;
        writer.FlushAsync()?.get()?;
        writer.DetachStream()?;
        drop(bytes);
        stream.Seek(0)?;
        trace("decoder");
        let decoder = BitmapDecoder::CreateAsync(&stream)?.get()?;
        let (w, h) = (decoder.PixelWidth()? as f64, decoder.PixelHeight()? as f64);
        // El texto chico de las pantallas se lee mejor agrandado; el motor rechaza imágenes más grandes que su máximo.
        let max = OcrEngine::MaxImageDimension()? as f64;
        let mut scale = if w.max(h) < 1600.0 { 2.0 } else { 1.0 };
        scale = f64::min(scale, max / w.max(h));
        trace("transform");
        let transform = BitmapTransform::new()?;
        transform.SetScaledWidth((w * scale).round().max(1.0) as u32)?;
        transform.SetScaledHeight((h * scale).round().max(1.0) as u32)?;
        transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
        let bmp = decoder
            .GetSoftwareBitmapTransformedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied, &transform, ExifOrientationMode::IgnoreExifOrientation, ColorManagementMode::DoNotColorManage)?
            .get()?;
        trace("recognize");
        let res = engine.RecognizeAsync(&bmp)?.get()?;
        let mut out = OcrResult { language: engine.RecognizerLanguage().and_then(|l| l.LanguageTag()).map(|s| s.to_string()).unwrap_or_default(), ..Default::default() };
        let mut lines = Vec::new();
        for line in res.Lines()? {
            lines.push(line.Text()?.to_string());
            for word in line.Words()? {
                let b = word.BoundingRect()?;
                out.words.push(Word { text: word.Text()?.to_string(), x: b.X as f64 / scale, y: b.Y as f64 / scale, w: b.Width as f64 / scale, h: b.Height as f64 / scale });
            }
        }
        trace("done");
        out.text = lines.join("\n");
        bmp.Close().ok();
        Ok(out)
    }
}

/// Lectura directa, sin caché (diagnóstico `--ocrtest`).
pub fn engine_recognize(path: &Path) -> Option<OcrResult> {
    engine::recognize(path)
}
