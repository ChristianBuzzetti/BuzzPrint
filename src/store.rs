//! Biblioteca y configuración, con los mismos archivos que BuzzPrint .NET:
//!
//! - Capturas: `Pictures\BuzzPrint\capturas\<yyyyMMdd_HHmmss_fff>.png`
//! - Metadatos y original sin anotar: `capturas\.orig\<id>.json` / `<id>.png` (carpeta oculta)
//! - Grabaciones: `Videos\BuzzPrint\<id>.mp4|mp3` con `.meta\<id>.json|png`
//! - Configuración: `%LOCALAPPDATA%\BuzzPrint\capture-settings.json`
//!
//! Al guardar se conservan las claves que esta versión no conoce, así las dos versiones conviven.

use std::cell::RefCell;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use windows_sys::core::{GUID, PWSTR};
use windows_sys::Win32::Storage::FileSystem::{GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, INVALID_FILE_ATTRIBUTES};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Pictures, FOLDERID_Videos, SHFileOperationW, SHGetKnownFolderPath, FOF_ALLOWUNDO,
    FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
};

use crate::clock::Stamp;
use crate::i18n::{es, t};
use crate::win::wide_os;

// ------------------------------------------------------------------ configuración

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct AiConnection {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
}

impl Default for AiConnection {
    fn default() -> Self {
        AiConnection { id: new_id(), name: String::new(), provider: "anthropic".into(), model: String::new(), base_url: String::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub hotkey_capture: String,
    pub hotkey_full: String,
    pub hotkey_pick: String,
    pub hotkey_capture_alt: String,
    pub copy_after_capture: bool,
    pub open_editor_after_capture: bool,
    pub edit_keep_original: bool,
    pub snipping_prompt_answered: bool,
    pub guide_offered: bool,
    #[serde(rename = "WebView2Declined")]
    pub webview2_declined: bool,
    pub paste_on_pick: bool,
    pub delay_seconds: i32,
    pub quick_pick_count: i32,
    pub hotkey_record: String,
    pub record_system_audio: bool,
    pub record_microphone: bool,
    pub record_cursor: bool,
    pub record_fps: i32,
    pub capture_folder: String,
    pub record_folder: String,
    pub language: String,
    pub export_format: String,
    pub export_template: String,
    /// Última plantilla usada en cada formato ("pptx", "docx", "pdf").
    pub export_templates: std::collections::BTreeMap<String, String>,
    pub export_cover: bool,
    pub export_numbers: bool,
    pub export_captions: bool,
    pub export_meta: bool,
    pub export_one_per_page: bool,
    pub export_landscape: bool,
    pub export_open: bool,
    pub export_save_text: bool,
    pub export_folder: String,
    pub sections: Vec<String>,
    pub library_group_by: String,
    /// Orden de la biblioteca: "new" (más recientes), "old", "name" o "custom" (el que arma el usuario).
    pub library_sort: String,
    /// Orden personalizado: ids de las capturas, de la primera a la última.
    pub library_order: Vec<String>,
    pub mark_shape: String,
    pub mark_head: String,
    pub mark_eyes: String,
    pub mark_tile: String,
    /// Color de los ojos del logo: "auto" (claros en cabezas oscuras y al revés), "accent" o #RRGGBB.
    pub mark_eye_color: String,
    /// Al cambiar de paleta: "ask" (preguntar si se usan también su acento y su logo), "always" o "never".
    pub palette_extras: String,
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_base_url: String,
    pub ai_auto_caption: bool,
    pub ai_connections: Vec<AiConnection>,
    pub ai_active_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey_capture: "PrintScreen".into(),
            hotkey_full: "Ctrl+PrintScreen".into(),
            hotkey_pick: "Ctrl+Alt+V".into(),
            hotkey_capture_alt: "Ctrl+Shift+X".into(),
            copy_after_capture: true,
            open_editor_after_capture: false,
            edit_keep_original: true,
            snipping_prompt_answered: false,
            guide_offered: false,
            webview2_declined: false,
            paste_on_pick: true,
            delay_seconds: 3,
            quick_pick_count: 12,
            hotkey_record: "Ctrl+Alt+R".into(),
            record_system_audio: true,
            record_microphone: true,
            record_cursor: true,
            record_fps: 30,
            capture_folder: String::new(),
            record_folder: String::new(),
            language: "en".into(),
            export_format: "pptx".into(),
            export_template: "ppt-pizarra".into(),
            export_templates: std::collections::BTreeMap::new(),
            export_cover: true,
            export_numbers: true,
            export_captions: true,
            export_meta: true,
            export_one_per_page: true,
            export_landscape: false,
            export_open: true,
            export_save_text: true,
            export_folder: String::new(),
            sections: Vec::new(),
            library_group_by: "none".into(),
            library_sort: "new".into(),
            library_order: Vec::new(),
            mark_shape: "hex".into(),
            mark_head: "#0E1015".into(),
            mark_eyes: "oval".into(),
            // Sin placa: solo se pone si la persona la elige en Personalizar → Logo.
            mark_tile: "none".into(),
            mark_eye_color: "auto".into(),
            palette_extras: "ask".into(),
            ai_enabled: false,
            ai_provider: "anthropic".into(),
            ai_model: String::new(),
            ai_base_url: String::new(),
            ai_auto_caption: false,
            ai_connections: Vec::new(),
            ai_active_id: String::new(),
        }
    }
}

pub fn new_id() -> String {
    let mut g: GUID = unsafe { std::mem::zeroed() };
    unsafe { windows_sys::Win32::System::Com::CoCreateGuid(&mut g) };
    format!("{:08x}{:04x}{:04x}{}", g.data1, g.data2, g.data3, g.data4.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

// ------------------------------------------------------------------ elementos

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Image,
    Video,
    Audio,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub path: PathBuf,
    pub orig_path: PathBuf,
    pub meta_path: PathBuf,
    pub thumb_path: Option<PathBuf>,
    pub created: Stamp,
    pub title: String,
    pub source: String,
    pub note: String,
    pub section: String,
    pub pinned: bool,
    pub width: u32,
    pub height: u32,
    pub kind: Kind,
    pub duration: f64,
    pub edited: bool,
    pub ocr_text: Option<String>,
}

impl Item {
    pub fn is_recording(&self) -> bool {
        self.kind != Kind::Image
    }
    pub fn display(&self) -> String {
        if !self.title.is_empty() {
            self.title.clone()
        } else if !self.source.is_empty() {
            self.source.clone()
        } else {
            self.created.short()
        }
    }
}

// ------------------------------------------------------------------ la tienda

pub struct Store {
    pub settings: Settings,
    raw_settings: Map<String, Value>,
    pub capture_dir: PathBuf,
    pub orig_dir: PathBuf,
    pub record_dir: PathBuf,
    pub record_meta_dir: PathBuf,
    cache: Option<Vec<Item>>,
    rec_cache: Option<Vec<Item>>,
    /// Sube con cada cambio: las ventanas abiertas se refrescan cuando lo ven cambiar.
    pub revision: u64,
}

thread_local! {
    static STORE: RefCell<Option<Store>> = const { RefCell::new(None) };
}

/// Acceso a la tienda (un solo hilo: el de la interfaz).
pub fn with<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    STORE.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_none() {
            *s = Some(Store::load());
        }
        f(s.as_mut().unwrap())
    })
}

pub fn settings() -> Settings {
    with(|s| s.settings.clone())
}

pub fn data_dir() -> PathBuf {
    known_folder(&FOLDERID_LocalAppData)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()))
        .join("BuzzPrint")
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let mut p: PWSTR = null_mut();
        let hr = SHGetKnownFolderPath(id, 0, null_mut(), &mut p);
        if hr < 0 || p.is_null() {
            if !p.is_null() {
                CoTaskMemFree(p as _);
            }
            return None;
        }
        let mut len = 0;
        while *p.add(len) != 0 {
            len += 1;
        }
        let s = OsString::from_wide(std::slice::from_raw_parts(p, len));
        CoTaskMemFree(p as _);
        Some(PathBuf::from(s))
    }
}

pub fn default_capture_dir() -> PathBuf {
    known_folder(&FOLDERID_Pictures).unwrap_or_default().join("BuzzPrint").join("capturas")
}

pub fn default_record_dir() -> PathBuf {
    known_folder(&FOLDERID_Videos).unwrap_or_default().join("BuzzPrint")
}

/// Escribe sin dejar nunca un archivo a medias: temporal al lado y reemplazo.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

fn set_hidden(dir: &Path) {
    let w = wide_os(dir.as_os_str());
    unsafe {
        let a = GetFileAttributesW(w.as_ptr());
        if a != INVALID_FILE_ATTRIBUTES && a & FILE_ATTRIBUTE_HIDDEN == 0 {
            SetFileAttributesW(w.as_ptr(), a | FILE_ATTRIBUTE_HIDDEN);
        }
    }
}

fn read_json(path: &Path) -> Option<Map<String, Value>> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok()? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

fn png_size(path: &Path) -> (u32, u32) {
    use std::io::Read;
    let mut h = [0u8; 24];
    match fs::File::open(path).and_then(|mut f| f.read_exact(&mut h)) {
        Ok(()) if &h[1..4] == b"PNG" => (
            u32::from_be_bytes([h[16], h[17], h[18], h[19]]),
            u32::from_be_bytes([h[20], h[21], h[22], h[23]]),
        ),
        _ => (0, 0),
    }
}

fn str_of(m: &Map<String, Value>, k: &str) -> String {
    match m.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(v) => v.to_string(),
    }
}

impl Store {
    fn load() -> Store {
        let path = data_dir().join("capture-settings.json");
        let raw = read_json(&path).unwrap_or_default();
        let settings: Settings = serde_json::from_value(Value::Object(raw.clone())).unwrap_or_default();
        let mut s = Store {
            settings,
            raw_settings: raw,
            capture_dir: PathBuf::new(),
            orig_dir: PathBuf::new(),
            record_dir: PathBuf::new(),
            record_meta_dir: PathBuf::new(),
            cache: None,
            rec_cache: None,
            revision: 1,
        };
        s.apply_folders();
        s
    }

    pub fn save_settings(&mut self) {
        if let Ok(Value::Object(ours)) = serde_json::to_value(&self.settings) {
            for (k, v) in ours {
                self.raw_settings.insert(k, v);
            }
        }
        if let Ok(text) = serde_json::to_string_pretty(&Value::Object(self.raw_settings.clone())) {
            let _ = write_atomic(&data_dir().join("capture-settings.json"), text.as_bytes());
        }
    }

    pub fn apply_folders(&mut self) {
        let usable = |p: &str| -> Option<PathBuf> {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            fs::create_dir_all(p).ok().map(|_| PathBuf::from(p))
        };
        self.capture_dir = usable(&self.settings.capture_folder).unwrap_or_else(default_capture_dir);
        self.record_dir = usable(&self.settings.record_folder).unwrap_or_else(default_record_dir);
        self.orig_dir = self.capture_dir.join(".orig");
        self.record_meta_dir = self.record_dir.join(".meta");
        for d in [&self.capture_dir, &self.orig_dir, &self.record_dir, &self.record_meta_dir] {
            let _ = fs::create_dir_all(d);
        }
        set_hidden(&self.orig_dir);
        set_hidden(&self.record_meta_dir);
        self.changed();
    }

    pub fn changed(&mut self) {
        self.cache = None;
        self.rec_cache = None;
        self.revision += 1;
    }

    /// Capturas, fijadas primero y luego de la más nueva a la más vieja.
    pub fn all(&mut self) -> &Vec<Item> {
        if self.cache.is_none() {
            let mut list = Vec::new();
            if let Ok(rd) = fs::read_dir(&self.capture_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")) {
                        continue;
                    }
                    let id = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    let created = Stamp::parse_id(&id).unwrap_or_else(|| {
                        e.metadata().and_then(|m| m.created()).map(Stamp::from_system).unwrap_or_default()
                    });
                    let orig_path = self.orig_dir.join(format!("{id}.png"));
                    let meta_path = self.orig_dir.join(format!("{id}.json"));
                    let (width, height) = png_size(&p);
                    let mut it = Item {
                        edited: orig_path.exists(),
                        id,
                        path: p,
                        orig_path,
                        meta_path,
                        thumb_path: None,
                        created,
                        title: String::new(),
                        source: String::new(),
                        note: String::new(),
                        section: String::new(),
                        pinned: false,
                        width,
                        height,
                        kind: Kind::Image,
                        duration: 0.0,
                        ocr_text: None,
                    };
                    if let Some(m) = read_json(&it.meta_path) {
                        it.title = str_of(&m, "title");
                        it.section = str_of(&m, "section");
                        it.source = str_of(&m, "source");
                        it.note = str_of(&m, "note");
                        it.pinned = m.get("pinned").and_then(Value::as_bool).unwrap_or(false);
                        if let Some(Value::String(o)) = m.get("ocr") {
                            it.ocr_text = Some(o.clone());
                        }
                    }
                    list.push(it);
                }
            }
            list.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.created.cmp(&a.created)));
            self.cache = Some(list);
        }
        self.cache.as_ref().unwrap()
    }

    pub fn recordings(&mut self) -> &Vec<Item> {
        if self.rec_cache.is_none() {
            let mut list = Vec::new();
            if let Ok(rd) = fs::read_dir(&self.record_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                    if ext != "mp4" && ext != "mp3" {
                        continue;
                    }
                    let id = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    let created = Stamp::parse_id(&id).unwrap_or_else(|| {
                        e.metadata().and_then(|m| m.created()).map(Stamp::from_system).unwrap_or_default()
                    });
                    let mut it = Item {
                        orig_path: self.record_meta_dir.join(format!("{id}.none")),
                        meta_path: self.record_meta_dir.join(format!("{id}.json")),
                        thumb_path: Some(self.record_meta_dir.join(format!("{id}.png"))),
                        id,
                        path: p,
                        created,
                        title: String::new(),
                        source: String::new(),
                        note: String::new(),
                        section: String::new(),
                        pinned: false,
                        width: 0,
                        height: 0,
                        kind: if ext == "mp4" { Kind::Video } else { Kind::Audio },
                        duration: 0.0,
                        edited: false,
                        ocr_text: None,
                    };
                    if let Some(m) = read_json(&it.meta_path) {
                        it.title = str_of(&m, "title");
                        it.section = str_of(&m, "section");
                        it.source = str_of(&m, "source");
                        it.pinned = m.get("pinned").and_then(Value::as_bool).unwrap_or(false);
                        it.duration = m.get("duration").and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok())).unwrap_or(0.0);
                    }
                    list.push(it);
                }
            }
            list.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.created.cmp(&a.created)));
            self.rec_cache = Some(list);
        }
        self.rec_cache.as_ref().unwrap()
    }

    pub fn find(&mut self, id: &str) -> Option<Item> {
        if let Some(i) = self.all().iter().find(|i| i.id == id) {
            return Some(i.clone());
        }
        self.recordings().iter().find(|i| i.id == id).cloned()
    }

    // ---- metadatos (se conservan claves desconocidas, como "shapes")
    pub fn read_meta(item: &Item) -> Map<String, Value> {
        read_json(&item.meta_path).unwrap_or_default()
    }

    pub fn write_meta(item: &Item, meta: &Map<String, Value>) -> io::Result<()> {
        let text = serde_json::to_string_pretty(&Value::Object(meta.clone())).map_err(io::Error::other)?;
        write_atomic(&item.meta_path, text.as_bytes())
    }

    pub fn update_meta(&mut self, item: &Item, key: &str, value: Value) -> io::Result<()> {
        let mut m = Self::read_meta(item);
        m.insert(key.into(), value);
        let r = Self::write_meta(item, &m);
        self.changed();
        r
    }

    /// Agrega una captura nueva (PNG ya codificado o imagen BGRA) con su título por defecto.
    pub fn add_png(&mut self, png_bytes: &[u8], source: &str) -> io::Result<Item> {
        let now = Stamp::now();
        let mut id = now.id();
        let mut path = self.capture_dir.join(format!("{id}.png"));
        let mut n = 1;
        while path.exists() {
            id = format!("{}_{n}", now.id());
            path = self.capture_dir.join(format!("{id}.png"));
            n += 1;
        }
        fs::create_dir_all(&self.capture_dir)?;
        fs::write(&path, png_bytes)?;
        let item = Item {
            id: id.clone(),
            path,
            orig_path: self.orig_dir.join(format!("{id}.png")),
            meta_path: self.orig_dir.join(format!("{id}.json")),
            thumb_path: None,
            created: now,
            title: default_name(source, &now, None),
            source: source.to_string(),
            note: String::new(),
            section: String::new(),
            pinned: false,
            width: 0,
            height: 0,
            kind: Kind::Image,
            duration: 0.0,
            edited: false,
            ocr_text: None,
        };
        let mut meta = Map::new();
        meta.insert("title".into(), Value::String(item.title.clone()));
        if !source.is_empty() {
            meta.insert("source".into(), Value::String(source.into()));
        }
        Self::write_meta(&item, &meta)?;
        self.changed();
        Ok(self.find(&id).unwrap_or(item))
    }

    /// Manda a la Papelera la captura con su original, metadatos y miniatura, en una sola operación del shell.
    pub fn delete_many(&mut self, items: &[Item]) {
        let mut files: Vec<PathBuf> = Vec::new();
        for it in items {
            for p in [Some(&it.path), Some(&it.orig_path), Some(&it.meta_path), it.thumb_path.as_ref()].into_iter().flatten() {
                if p.exists() && !files.contains(p) {
                    files.push(p.clone());
                }
            }
        }
        if !files.is_empty() && !recycle(&files) {
            for f in &files {
                let _ = fs::remove_file(f);
            }
        }
        self.changed();
    }

    /// Capturas y grabaciones juntas.
    pub fn everything(&mut self) -> Vec<Item> {
        let mut v = self.all().clone();
        v.extend(self.recordings().iter().cloned());
        v
    }

    // ---- secciones
    pub fn section_names(&mut self) -> Vec<String> {
        let mut names = self.settings.sections.clone();
        let extra: Vec<String> = self.everything().into_iter().map(|i| i.section).collect();
        for s in extra {
            if !s.is_empty() && !names.contains(&s) {
                names.push(s);
            }
        }
        names
    }

    pub fn add_section(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.section_names().iter().any(|n| n == name) {
            return false;
        }
        self.settings.sections.push(name.to_string());
        self.save_settings();
        true
    }

    pub fn set_section(&mut self, items: &[Item], name: &str) {
        let name = name.trim();
        if !name.is_empty() {
            self.add_section(name);
        }
        for it in items {
            let _ = self.update_meta(it, "section", Value::String(name.into()));
        }
        self.changed();
    }

    pub fn rename_section(&mut self, from: &str, to: &str) {
        let to = to.trim();
        if to.is_empty() || from == to {
            return;
        }
        let list = &mut self.settings.sections;
        if let Some(i) = list.iter().position(|s| s == from) {
            if list.iter().any(|s| s == to) {
                list.remove(i);
            } else {
                list[i] = to.into();
            }
        } else if !list.iter().any(|s| s == to) {
            list.push(to.into());
        }
        let affected: Vec<Item> = self.everything().into_iter().filter(|i| i.section == from).collect();
        for it in &affected {
            let _ = self.update_meta(it, "section", Value::String(to.into()));
        }
        self.save_settings();
        self.changed();
    }

    pub fn delete_section(&mut self, name: &str) {
        self.settings.sections.retain(|s| s != name);
        let affected: Vec<Item> = self.everything().into_iter().filter(|i| i.section == name).collect();
        for it in &affected {
            let _ = self.update_meta(it, "section", Value::String(String::new()));
        }
        self.save_settings();
        self.changed();
    }

    /// Mueve capturas o grabaciones a otra carpeta. Devuelve cuántos archivos se movieron.
    pub fn change_folder(&mut self, recordings: bool, folder: &Path, move_files: bool) -> usize {
        let (old_dir, old_side) = if recordings {
            (self.record_dir.clone(), self.record_meta_dir.clone())
        } else {
            (self.capture_dir.clone(), self.orig_dir.clone())
        };
        let default = if recordings { default_record_dir() } else { default_capture_dir() };
        let value = if same_path(folder, &default) { String::new() } else { folder.to_string_lossy().into_owned() };
        if recordings {
            self.settings.record_folder = value;
        } else {
            self.settings.capture_folder = value;
        }
        self.save_settings();
        self.apply_folders();
        let (new_dir, new_side) = if recordings {
            (self.record_dir.clone(), self.record_meta_dir.clone())
        } else {
            (self.capture_dir.clone(), self.orig_dir.clone())
        };
        let mut moved = 0;
        if move_files && !same_path(&old_dir, &new_dir) {
            let exts: &[&str] = if recordings { &["mp4", "mp3"] } else { &["png"] };
            if let Ok(rd) = fs::read_dir(&old_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    let ok = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).is_some_and(|x| exts.contains(&x.as_str()));
                    if ok {
                        let to = new_dir.join(p.file_name().unwrap());
                        if !to.exists() && move_file(&p, &to) {
                            moved += 1;
                        }
                    }
                }
            }
            if let Ok(rd) = fs::read_dir(&old_side) {
                for e in rd.flatten() {
                    let p = e.path();
                    let to = new_side.join(p.file_name().unwrap());
                    if !to.exists() {
                        move_file(&p, &to);
                    }
                }
            }
        }
        self.changed();
        moved
    }
}

fn move_file(from: &Path, to: &Path) -> bool {
    fs::rename(from, to).is_ok() || (fs::copy(from, to).is_ok() && fs::remove_file(from).is_ok())
}

fn same_path(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    n(a) == n(b)
}

/// Papelera de reciclaje con una sola operación del shell (como el Explorador).
pub fn recycle(files: &[PathBuf]) -> bool {
    let mut from: Vec<u16> = Vec::new();
    for f in files {
        from.extend(wide_os(f.as_os_str())); // ya trae su 0
    }
    from.push(0); // doble 0 al final
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = from.as_ptr();
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as _;
    unsafe { SHFileOperationW(&mut op) == 0 && op.fAnyOperationsAborted == 0 }
}

/// Nombre por defecto: tipo (o título de la ventana) + fecha, solo ASCII y guiones bajos.
pub fn default_name(source: &str, when: &Stamp, prefix: Option<&str>) -> String {
    let prefix = match prefix {
        Some(p) => p.to_string(),
        None => {
            if source.is_empty() || source == "Región" || source == "Region" {
                "Region".into()
            } else if source == "Pantalla completa" || source == "Full screen" {
                if es() { "Pantalla".into() } else { "Screen".into() }
            } else if source == "Ventana" || source == "Window" {
                if es() { "Ventana".into() } else { "Window".into() }
            } else {
                source.to_string()
            }
        }
    };
    let mut plain = String::new();
    for c in strip_accents(&prefix).chars() {
        if c.is_ascii_alphanumeric() {
            plain.push(c);
        } else if !plain.is_empty() && !plain.ends_with('_') {
            plain.push('_');
        }
    }
    let mut head = plain.trim_matches('_').to_string();
    if head.len() > 32 {
        head.truncate(32);
        head = head.trim_end_matches('_').to_string();
    }
    if head.is_empty() {
        head = "Captura".into();
    }
    format!("{head}_{}", when.name_part())
}

fn strip_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'A',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'É' | 'È' | 'Ë' | 'Ê' => 'E',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'O',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ç' => 'c',
            'Ç' => 'C',
            other => other,
        })
        .collect()
}

/// Fuente legible para mostrar ("Región", "Pantalla completa", título de ventana).
pub fn source_label(kind: &str) -> String {
    t(kind)
}
