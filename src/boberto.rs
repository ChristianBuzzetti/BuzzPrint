//! Boberto: sincronización del aspecto con el motor DBMON (`boberto.js` en WebView2 solo para Studio o
//! sincronización puntual con .NET) y datos locales para el render nativo en egui (Dock y Stage).
//!
//! Liviano: Dock y Stage se dibujan 100% en egui usando el atlas 4x4 (`boberto-sheet.png`) y `boberto-look.json`.
//! WebView2 solo existe mientras está abierta la pestaña Boberto Studio (o unos ~0,6 s si .NET cambió el aspecto)
//! y al cerrarse la última vista sus procesos terminan.
//!
//! Seguro: solo carga los archivos empaquetados (`boberto\` junto al .exe) en el origen
//! `https://boberto.desktop.invalid/`; navegación, ventanas nuevas, permisos y pedidos a otros orígenes se
//! rechazan. Las herramientas de desarrollo y los menús del navegador están apagados.
//!
//! El perfil (`%LOCALAPPDATA%\BuzzPrint\BobertoRuntime`) y `boberto-avatar.png` se comparten con .NET.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING, PWSTR};
use windows::Win32::Foundation::{BOOL, HWND as WHWND, RECT as WRECT};
use windows::Win32::System::WinRT::EventRegistrationToken;
use windows_sys::Win32::Foundation::HWND;

use crate::theme::Colors;

pub const HOST: &str = "boberto.desktop.invalid";
const ORIGIN: &str = "https://boberto.desktop.invalid/";

// ------------------------------------------------------------------ Atlas nativo y estado del aspecto

pub const SHEET_COLS: usize = 4;
pub const SHEET_ROWS: usize = 4;
pub const CELL_W: u32 = 160; // BOX.W (20) * scale 8
pub const CELL_H: u32 = 168; // BOX.H (21) * scale 8
pub const SHEET_W: u32 = CELL_W * SHEET_COLS as u32;
pub const SHEET_H: u32 = CELL_H * SHEET_ROWS as u32;

/// Orden fila por fila del atlas 4x4 (col = i % 4, row = i / 4).
pub const POSE_NAMES: [&str; 16] = [
    "base", "blink", "wink", "lookL",
    "lookR", "lookU", "happy", "waveUp",
    "waveDown", "festejaSalto", "festejaPuno", "danceR",
    "danceL", "think", "coffeeHold", "coffeeSip",
];

pub fn pose_index(name: &str) -> Option<usize> {
    POSE_NAMES.iter().position(|&p| p == name)
}

static LOOK_REV: AtomicU64 = AtomicU64::new(1);

/// Contador monotónico: sube cada vez que Studio o Sync guardan un aspecto/atlas nuevo.
pub fn look_revision() -> u64 {
    LOOK_REV.load(Ordering::Relaxed)
}

fn bump_look_revision() {
    LOOK_REV.fetch_add(1, Ordering::Relaxed);
    crate::gui::host::wake_all();
}

/// Estado del aspecto de Boberto (espejo de las claves `bzp_boberto_*` de localStorage).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub skin: String,
    pub skin_color: String,
    pub eye: String,
    pub hat: String,
    pub hat_color: String,
    pub hat_color2: String,
    pub character: String,
    pub fx_body: Vec<String>,
    pub fx_acc: Vec<String>,
    pub glow_body: String,
    pub fondo: String,
    pub fondo_op: HashMap<String, u8>,
    pub borde: String,
    pub label: String,
}

impl Default for Look {
    /// Aspecto inicial de BuzzPrint (igual que `desktop-start.js`: Samurái, cuerpo negro, ojos azules, Mirai noche, cruce).
    fn default() -> Self {
        Look {
            skin: "custom".into(),
            skin_color: "#000000".into(),
            eye: "#2684ff".into(),
            hat: "samurai".into(),
            hat_color: "#2a2c34".into(),
            hat_color2: "auto".into(),
            character: String::new(),
            fx_body: Vec::new(),
            fx_acc: Vec::new(),
            glow_body: "auto".into(),
            fondo: crate::fondos::default_id().into(),
            fondo_op: HashMap::new(),
            borde: "cruce".into(),
            label: String::new(),
        }
    }
}

impl Look {
    fn normalize(&mut self) {
        if self.character == "none" {
            self.character.clear();
        }
        if self.fondo.is_empty() {
            self.fondo = "ninguno".into();
        }
        // Las láminas anteriores ya no están: quien tenía una pasa al fondo por defecto de BuzzPrint.
        if self.fondo != "ninguno" && !crate::fondos::exists(&self.fondo) {
            self.fondo = crate::fondos::default_id().into();
        }
        if self.borde.is_empty() {
            self.borde = "ninguno".into();
        }
        if self.glow_body.is_empty() {
            self.glow_body = "auto".into();
        }
    }

    /// Opacidad efectiva del fondo actual (0.0..=1.0). 0.0 si no hay fondo.
    pub fn fondo_opacity(&self, dark: bool) -> f32 {
        if self.fondo.is_empty() || self.fondo == "ninguno" {
            return 0.0;
        }
        if let Some(&pct) = self.fondo_op.get(&self.fondo) {
            return (pct.min(100) as f32) / 100.0;
        }
        if dark { 0.85 } else { 0.60 }
    }

    /// Rótulo del aspecto (material o accesorio activo), en el idioma actual de la interfaz.
    pub fn display_label(&self) -> String {
        let es = crate::i18n::es();
        if !self.character.is_empty() {
            if let Some(name) = catalog_character_name(&self.character, es) {
                return name.to_string();
            }
        } else if !self.hat.is_empty() {
            if let Some(name) = catalog_hat_name(&self.hat, es) {
                return name.to_string();
            }
        }
        if !self.label.is_empty() {
            return self.label.clone();
        }
        if es { "En línea".into() } else { "Online".into() }
    }
}

fn catalog_character_name(id: &str, es: bool) -> Option<&'static str> {
    let pair = match id {
        "madera" => ("Madera y lianas", "Wood and vines"),
        "talla" => ("Madera tallada", "Carved wood"),
        "gemas" => ("Gemas", "Gemstones"),
        "piedra" => ("Piedra y musgo", "Stone and moss"),
        "kintsugi" => ("Kintsugi", "Kintsugi"),
        "lava" => ("Obsidiana y lava", "Obsidian and lava"),
        "escarcha" => ("Escarcha", "Frost"),
        "nebulosa" => ("Nebulosa", "Nebula"),
        "celestial" => ("Celestial", "Celestial"),
        "bronce" => ("Bronce y pátina", "Bronze and patina"),
        "marmol" => ("Mármol", "Marble"),
        "vitral" => ("Vitral", "Stained glass"),
        "circuito" => ("Circuito", "Circuit"),
        "ambar" => ("Ámbar", "Amber"),
        _ => return None,
    };
    Some(if es { pair.0 } else { pair.1 })
}

fn catalog_hat_name(id: &str, es: bool) -> Option<&'static str> {
    let pair = match id {
        "auriculares" => ("Auriculares", "Headphones"),
        "antena" => ("Antena", "Antenna"),
        "copete" => ("Copete", "Quiff"),
        "cresta" => ("Cresta LED", "LED crest"),
        "gorro" => ("Gorro", "Cap"),
        "mago" => ("Gorro Mago", "Wizard hat"),
        "corona" => ("Corona", "Crown"),
        "vikingo" => ("Casco Vikingo", "Viking helmet"),
        "chef" => ("Sombrero Chef", "Chef hat"),
        "halo" => ("Halo Ángel", "Angel halo"),
        "lentes" => ("Lentes Sol", "Sunglasses"),
        "gato" => ("Orejas Gato", "Cat ears"),
        "ovni" => ("Mini OVNI", "Mini UFO"),
        "santa" => ("Gorro Santa", "Santa hat"),
        "copa" => ("Sombrero Copa", "Top hat"),
        "pirata" => ("Pirata", "Pirate"),
        "samurai" => ("Samurái", "Samurai"),
        "caballero" => ("Caballero medieval", "Medieval knight"),
        "steampunk" => ("Explorador steampunk", "Steampunk explorer"),
        "faraon" => ("Faraón", "Pharaoh"),
        "vaquero" => ("Vaquero", "Cowboy"),
        "guardian" => ("Guardián del bosque", "Forest guardian"),
        "bombero" => ("Bombero", "Firefighter"),
        "detective" => ("Detective", "Detective"),
        "mosquetero" => ("Mosquetero", "Musketeer"),
        "bufon" => ("Bufón", "Jester"),
        "marinero" => ("Marinero", "Sailor"),
        "pintor" => ("Pintor", "Painter"),
        "mariachi" => ("Mariachi", "Mariachi"),
        "ninja" => ("Ninja", "Ninja"),
        "gaitero" => ("Gaitero escocés", "Scottish piper"),
        "polar" => ("Expedición polar", "Polar expedition"),
        "astronauta" => ("Astronauta", "Astronaut"),
        "buzo" => ("Escafandra", "Diving helmet"),
        "alas" => ("Alas", "Wings"),
        "jetpack" => ("Jetpack", "Jetpack"),
        "demonio" => ("Demonio", "Demon"),
        "capa" => ("Capa", "Cape"),
        "arquero" => ("Arquero", "Archer"),
        "bufanda" => ("Bufanda", "Scarf"),
        "gladiador" => ("Gladiador", "Gladiator"),
        "helice" => ("Gorro hélice", "Propeller cap"),
        "yukata" => ("Yukata", "Yukata"),
        "murcielago" => ("Murciélago", "Bat"),
        "alasmurcielago" => ("Alas Murciélago", "Bat wings"),
        "bata" => ("Bata", "Lab coat"),
        "brasita" => ("Disfraz Brasita", "Ember costume"),
        "caparazon" => ("Disfraz Caparazón", "Shell costume"),
        "brotito" => ("Disfraz Brotito", "Sprout costume"),
        "chispa" => ("Disfraz Chispa", "Spark costume"),
        "sombrita" => ("Disfraz Sombrita", "Shadow costume"),
        "limpio" => ("Sin accesorio", "No accessory"),
        _ => return None,
    };
    Some(if es { pair.0 } else { pair.1 })
}

pub struct SheetData {
    pub png: Vec<u8>,
    /// `true` = atlas 4x4 (640x672); `false` = respaldo de un solo cuadro (`boberto-avatar.png`).
    pub is_atlas: bool,
}

/// Carpeta con el motor: `boberto\` junto al .exe; en desarrollo, `assets\boberto` del proyecto.
pub fn assets_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    let packaged = dir.join("boberto");
    if packaged.join("boberto.js").is_file() {
        return Some(packaged);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("boberto");
    dev.join("boberto.js").is_file().then_some(dev)
}

pub fn profile_dir() -> PathBuf {
    crate::store::data_dir().join("BobertoRuntime")
}

/// Último cuadro que mandó Studio (compartido con .NET: <=512x512 PNG).
pub fn avatar_path() -> PathBuf {
    crate::store::data_dir().join("boberto-avatar.png")
}

/// Aspecto exportado por Studio/Sync para el render nativo en egui (archivo propio de Rust, ignorado por .NET).
pub fn look_path() -> PathBuf {
    crate::store::data_dir().join("boberto-look.json")
}

/// Atlas 4x4 (640x672) exportado por Studio/Sync para el render nativo en egui.
pub fn sheet_path() -> PathBuf {
    crate::store::data_dir().join("boberto-sheet.png")
}

/// Carga `boberto-look.json` o devuelve el aspecto por defecto de BuzzPrint.
pub fn load_look() -> Look {
    let mut look = std::fs::read_to_string(look_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Look>(&s).ok())
        .unwrap_or_default();
    look.normalize();
    look
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= 24 && bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
}

fn png_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    if !is_png(bytes) {
        return None;
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((w, h))
}

fn file_mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn leveldb_mtime() -> Option<SystemTime> {
    let dir = profile_dir().join("EBWebView").join("Default").join("Local Storage").join("leveldb");
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut newest: Option<SystemTime> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("log") || ext.eq_ignore_ascii_case("ldb") {
            if let Some(m) = entry.metadata().ok().and_then(|md| md.modified().ok()) {
                newest = Some(newest.map_or(m, |cur| cur.max(m)));
            }
        }
    }
    newest
}

/// Carga el atlas 4x4 más reciente, o cae en `boberto-avatar.png` (si .NET lo actualizó), o en `default-sheet.png`.
pub fn load_sheet_png() -> Option<SheetData> {
    let sheet_p = sheet_path();
    let avatar_p = avatar_path();
    let avatar_newer = match (file_mtime(&avatar_p), file_mtime(&sheet_p)) {
        (Some(av), Some(sh)) => av > sh + Duration::from_secs(5),
        (Some(_), None) => true,
        _ => false,
    };
    if !avatar_newer {
        if let Ok(bytes) = std::fs::read(&sheet_p) {
            if png_dims(&bytes) == Some((SHEET_W, SHEET_H)) {
                return Some(SheetData { png: bytes, is_atlas: true });
            }
        }
    }
    if let Ok(bytes) = std::fs::read(&avatar_p) {
        if is_png(&bytes) {
            return Some(SheetData { png: bytes, is_atlas: false });
        }
    }
    if let Ok(bytes) = std::fs::read(&sheet_p) {
        if png_dims(&bytes) == Some((SHEET_W, SHEET_H)) {
            return Some(SheetData { png: bytes, is_atlas: true });
        }
    }
    let bundled = assets_dir()?.join("default-sheet.png");
    let bytes = std::fs::read(bundled).ok()?;
    is_png(&bytes).then_some(SheetData { png: bytes, is_atlas: true })
}

/// ¿El perfil compartido de WebView2 o `boberto-avatar.png` fue modificado (p. ej. por BuzzPrint .NET)
/// después de nuestra última sincronización nativa?
pub fn is_stale_from_dotnet() -> bool {
    let look_m = file_mtime(&look_path());
    let sheet_m = file_mtime(&sheet_path());
    let ldb_m = leveldb_mtime();
    let av_m = file_mtime(&avatar_path());
    let source_m = match (ldb_m, av_m) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    };
    let Some(src) = source_m else {
        // Instalación limpia: no existe BobertoRuntime ni avatar previo; usar default-sheet.png sin WebView2.
        return false;
    };
    let (Some(lm), Some(sm)) = (look_m, sheet_m) else {
        return true;
    };
    let synced = lm.min(sm);
    // Margen de 5 s: cerrar WebView2 tras sync.html / Studio puede tocar leveldb unos milisegundos después.
    src > synced + Duration::from_secs(5)
}

thread_local! {
    static SYNC_WEB: RefCell<Option<Web>> = const { RefCell::new(None) };
    static SYNC_ATTEMPTED: Cell<bool> = const { Cell::new(false) };
}

/// Si .NET o una versión anterior cambió el perfil de Boberto, lanza una sincronización oculta única (~0,6 s)
/// que hornea `boberto-look.json` + `boberto-sheet.png` y cierra WebView2 de inmediato.
pub fn trigger_sync_if_stale(parent: HWND) {
    if !available() || SYNC_ATTEMPTED.with(|c| c.get()) || !is_stale_from_dotnet() {
        return;
    }
    if LIVE.with(|l| l.get()) > 0 {
        // Studio ya está abierto y va a emitir `native-sync`.
        return;
    }
    SYNC_ATTEMPTED.with(|c| c.set(true));
    let w = Web::new(parent, Page::Sync, egui::Color32::TRANSPARENT, true);
    SYNC_WEB.with(|s| *s.borrow_mut() = Some(w));
}

fn finish_sync() {
    SYNC_WEB.with(|s| {
        s.borrow_mut().take();
    });
}

fn save_bytes(path: &PathBuf, bytes: &[u8]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return;
    }
    // Escritura directa (como boberto-avatar.png y .NET): renombrar temporales sobre imágenes molesta a algunos antivirus.
    let _ = std::fs::write(path, bytes);
}

/// ¿Se puede mostrar Boberto Studio (WebView2 instalado y motor empaquetado)?
pub fn available() -> bool {
    thread_local! {
        static OK: Cell<Option<bool>> = const { Cell::new(None) };
    }
    OK.with(|ok| {
        if let Some(v) = ok.get() {
            return v;
        }
        let v = crate::deps::webview2_version().is_some() && assets_dir().is_some();
        ok.set(Some(v));
        v
    })
}

fn page_url(page: &str) -> String {
    let es = crate::i18n::es();
    format!("{ORIGIN}{page}{}", if es { "?lang=es" } else { "" })
}

fn local(uri: &str) -> bool {
    uri.starts_with(ORIGIN) || uri == ORIGIN.trim_end_matches('/')
}

fn css(c: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}{:02X}", c.r(), c.g(), c.b(), c.a())
}

/// Colores del tema para el motor (los mismos tokens CSS que arma BobertoRuntime.ThemeTokens en .NET).
fn theme_tokens() -> String {
    let c: Colors = crate::gui::colors();
    let a = crate::gui::appearance();
    let mut m = serde_json::Map::new();
    for (k, v) in [
        ("--bg", c.canvas),
        ("--bg-elev-0", c.header),
        ("--bg-elev-1", c.surface),
        ("--bg-elev-2", c.raised),
        ("--bg-hover", c.alt_row),
        ("--text", c.ink),
        ("--text-muted", c.muted),
        ("--text-dim", c.line_number),
        ("--border", c.strong_border),
        ("--border-strong", c.strong_border),
        ("--border-subtle", c.faint_border),
        ("--accent", c.accent),
    ] {
        m.insert(k.into(), css(v).into());
    }
    m.insert("--ui-size".into(), format!("{}px", a.ui_size).into());
    m.insert("--bzp-text".into(), format!("{}", a.ui_size / 13.0).into());
    m.insert("--font-ui".into(), (if a.ui_font == "inter" { "Inter, Segoe UI, sans-serif" } else { "Segoe UI, sans-serif" }).into());
    serde_json::Value::Object(m).to_string()
}

fn theme_name() -> String {
    let id = crate::gui::appearance().theme_id;
    let name = match id.as_str() {
        "light" => "alba".to_string(),
        "dark" => "midnight".to_string(),
        _ => id,
    };
    serde_json::Value::String(name).to_string()
}

/// La llamada que aplica el tema en cada página (Studio).
pub fn theme_script(page: Page) -> String {
    let motion = if crate::gui::appearance().boberto_motion { "true" } else { "false" };
    let (tokens, name) = (theme_tokens(), theme_name());
    match page {
        Page::Studio => format!("window.desktopStudio&&window.desktopStudio.applyTheme({tokens},{name},{motion})"),
        Page::Sync => String::new(),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    /// Boberto Studio (aspecto, accesorios, fondos, bordes y gestos).
    Studio,
    /// Sincronización oculta única para hornear el atlas tras cambios en .NET.
    Sync,
}

impl Page {
    fn file(self) -> &'static str {
        match self {
            Page::Studio => "studio.html",
            Page::Sync => "sync.html",
        }
    }
}

/// Lo que avisa la página.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Event {
    Ready,
    Studio,
    Error(String),
}

#[derive(Default)]
struct Inner {
    parent: isize,
    page: Option<Page>,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    bounds: Option<(i32, i32, i32, i32)>,
    visible: bool,
    ready: bool,
    failed: Option<String>,
    closed: bool,
    events: Vec<Event>,
    theme_rev: u64,
    background: u32,
    transparent: bool,
    last_shot: Option<std::time::Instant>,
}

/// Un WebView2 hijo de una ventana de BuzzPrint. Se crea en segundo plano; se cierra al soltarlo.
pub struct Web(Rc<RefCell<Inner>>);

thread_local! {
    static ENV: RefCell<Option<ICoreWebView2Environment>> = const { RefCell::new(None) };
    static WAITING: RefCell<Vec<Rc<RefCell<Inner>>>> = const { RefCell::new(Vec::new()) };
    static CREATING: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<usize> = const { Cell::new(0) };
}

impl Web {
    /// Empieza a crear el WebView para `page` dentro de `parent`. `background`: color mientras carga.
    pub fn new(parent: HWND, page: Page, background: egui::Color32, transparent: bool) -> Web {
        if page == Page::Studio {
            SYNC_ATTEMPTED.with(|c| c.set(true));
            finish_sync();
        }
        let inner = Rc::new(RefCell::new(Inner {
            parent: parent as isize,
            page: Some(page),
            background: u32::from_be_bytes([0, background.r(), background.g(), background.b()]),
            transparent,
            ..Default::default()
        }));
        LIVE.with(|l| l.set(l.get() + 1));
        if !available() {
            inner.borrow_mut().failed = Some(crate::i18n::t("Boberto necesita Microsoft Edge WebView2."));
        } else {
            // Nunca dentro de un frame de egui: se crea desde la cola de la app.
            let pending = inner.clone();
            crate::app::post(crate::app::Cmd::Run(Box::new(move || start(pending))));
        }
        Web(inner)
    }

    /// Ubica el WebView en píxeles del área cliente de la ventana; `None` lo oculta (deja de pintar).
    pub fn place(&self, px: Option<(i32, i32, i32, i32)>) {
        let mut s = self.0.borrow_mut();
        let want_visible = px.is_some() && s.ready && s.page != Some(Page::Sync);
        if let Some(b) = px {
            if s.bounds != Some(b) {
                s.bounds = Some(b);
                if let Some(c) = &s.controller {
                    unsafe {
                        let _ = c.SetBounds(WRECT { left: b.0, top: b.1, right: b.2, bottom: b.3 });
                    }
                }
            }
        }
        if s.visible != want_visible {
            s.visible = want_visible;
            if let Some(c) = &s.controller {
                unsafe {
                    let _ = c.SetIsVisible(BOOL::from(want_visible));
                }
            }
        }
        // Diagnóstico (`BUZZPRINT_SHOT=<carpeta>`): guarda lo que muestra la página como `boberto-<página>.png`.
        if s.ready && s.visible {
            if let Some(dir) = std::env::var_os("BUZZPRINT_SHOT") {
                let now = std::time::Instant::now();
                if s.last_shot.is_none_or(|t| now.duration_since(t).as_secs() >= 2) {
                    s.last_shot = Some(now);
                    if let Some(wv) = s.webview.clone() {
                        let path = std::path::Path::new(&dir).join(format!("boberto-{:?}.png", s.page.unwrap_or(Page::Studio)));
                        let done = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |_, json: String| {
                            if let Some(b64) = serde_json::from_str::<serde_json::Value>(&json).ok().and_then(|v| v.get("data").and_then(|d| d.as_str()).map(String::from)) {
                                if let Some(bytes) = crate::crypto::base64_decode(&b64) {
                                    let _ = std::fs::write(&path, bytes);
                                }
                            }
                            Ok(())
                        }));
                        unsafe {
                            let _ = wv.CallDevToolsProtocolMethod(&HSTRING::from("Page.captureScreenshot"), &HSTRING::from(r#"{"format":"png"}"#), &done);
                        }
                    }
                }
            }
        }
        // Tema nuevo (Personalizar): se aplica a la página viva.
        let rev = crate::gui::theme_rev();
        if s.ready && s.theme_rev != rev {
            s.theme_rev = rev;
            let page = s.page.unwrap_or(Page::Studio);
            drop(s);
            let script = theme_script(page);
            if !script.is_empty() {
                self.run(&script);
            }
        }
    }

    pub fn ready(&self) -> bool {
        self.0.borrow().ready
    }

    pub fn failed(&self) -> Option<String> {
        self.0.borrow().failed.clone()
    }

    pub fn take_events(&self) -> Vec<Event> {
        std::mem::take(&mut self.0.borrow_mut().events)
    }

    /// Ejecuta JavaScript en la página (sin esperar resultado).
    pub fn run(&self, js: &str) {
        let wv = self.0.borrow().webview.clone();
        if let Some(wv) = wv {
            unsafe {
                let _ = wv.ExecuteScript(&HSTRING::from(js), None);
            }
        }
    }
}

impl Drop for Web {
    fn drop(&mut self) {
        let mut s = self.0.borrow_mut();
        s.closed = true;
        s.webview = None;
        if let Some(c) = s.controller.take() {
            unsafe {
                let _ = c.Close();
            }
        }
        drop(s);
        let left = LIVE.with(|l| {
            let n = l.get().saturating_sub(1);
            l.set(n);
            n
        });
        if left == 0 {
            // Sin vistas, se suelta el entorno: los procesos del navegador terminan solos.
            ENV.with(|e| e.borrow_mut().take());
            crate::app::trim_memory();
        }
    }
}

fn fail(inner: &Rc<RefCell<Inner>>, msg: String) {
    let (parent, page) = {
        let mut s = inner.borrow_mut();
        s.failed = Some(msg);
        (s.parent, s.page)
    };
    if page == Some(Page::Sync) {
        crate::app::post(crate::app::Cmd::Run(Box::new(finish_sync)));
    }
    crate::gui::host::wake(parent as HWND);
}

/// Crea (o reutiliza) el entorno y después el controlador de esta vista.
fn start(inner: Rc<RefCell<Inner>>) {
    if inner.borrow().closed {
        return;
    }
    if let Some(env) = ENV.with(|e| e.borrow().clone()) {
        create_controller(env, inner);
        return;
    }
    WAITING.with(|w| w.borrow_mut().push(inner));
    if CREATING.with(|c| c.replace(true)) {
        return;
    }
    let folder = profile_dir();
    let _ = std::fs::create_dir_all(&folder);
    let options = CoreWebView2EnvironmentOptions::default();
    unsafe {
        options.set_language("en-US".into());
    }
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |hr: windows::core::Result<()>, env: Option<ICoreWebView2Environment>| {
        CREATING.with(|c| c.set(false));
        let waiting: Vec<_> = WAITING.with(|w| std::mem::take(&mut *w.borrow_mut()));
        match (hr, env) {
            (Ok(()), Some(env)) => {
                // Si mientras tanto se cerraron todas las vistas, no se guarda el entorno.
                if waiting.iter().any(|w| !w.borrow().closed) {
                    ENV.with(|e| *e.borrow_mut() = Some(env.clone()));
                }
                for w in waiting {
                    create_controller(env.clone(), w);
                }
            }
            (r, _) => {
                let msg = format!("{} ({:?})", crate::i18n::t("No se pudo iniciar WebView2"), r.err().map(|e| e.code()));
                for w in waiting {
                    fail(&w, msg.clone());
                }
            }
        }
        Ok(())
    }));
    let folder_h = HSTRING::from(folder.as_os_str());
    let r = unsafe { CreateCoreWebView2EnvironmentWithOptions(None, &folder_h, &options, &handler) };
    if let Err(e) = r {
        CREATING.with(|c| c.set(false));
        let waiting: Vec<_> = WAITING.with(|w| std::mem::take(&mut *w.borrow_mut()));
        for w in waiting {
            fail(&w, format!("{} ({e})", crate::i18n::t("No se pudo iniciar WebView2")));
        }
    }
}

fn create_controller(env: ICoreWebView2Environment, inner: Rc<RefCell<Inner>>) {
    if inner.borrow().closed {
        return;
    }
    let parent = WHWND(inner.borrow().parent as *mut core::ffi::c_void);
    let env2 = env.clone();
    let cb_inner = inner.clone();
    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |hr: windows::core::Result<()>, controller: Option<ICoreWebView2Controller>| {
        let inner = cb_inner;
        let controller = match (hr, controller) {
            (Ok(()), Some(c)) => c,
            (r, _) => {
                fail(&inner, format!("{} ({:?})", crate::i18n::t("No se pudo iniciar WebView2"), r.err().map(|e| e.code())));
                return Ok(());
            }
        };
        if inner.borrow().closed {
            unsafe {
                let _ = controller.Close();
            }
            return Ok(());
        }
        if let Err(e) = unsafe { setup(&env2, &controller, &inner) } {
            unsafe {
                let _ = controller.Close();
            }
            fail(&inner, format!("{} ({e})", crate::i18n::t("No se pudo iniciar WebView2")));
        }
        Ok(())
    }));
    if let Err(e) = unsafe { env.CreateCoreWebView2Controller(parent, &handler) } {
        fail(&inner, format!("{} ({e})", crate::i18n::t("No se pudo iniciar WebView2")));
    }
}

unsafe fn setup(env: &ICoreWebView2Environment, controller: &ICoreWebView2Controller, inner: &Rc<RefCell<Inner>>) -> windows::core::Result<()> {
    let (bg, transparent, bounds, page) = {
        let s = inner.borrow();
        (s.background, s.transparent, s.bounds, s.page.unwrap_or(Page::Studio))
    };
    // Oculto hasta que la página avise que está lista (Page::Sync permanece siempre oculto).
    controller.SetIsVisible(false)?;
    if let Some(b) = bounds {
        controller.SetBounds(WRECT { left: b.0, top: b.1, right: b.2, bottom: b.3 })?;
    }
    if let Ok(c2) = controller.cast::<ICoreWebView2Controller2>() {
        let [_, r, g, b] = bg.to_be_bytes();
        let color = if transparent { COREWEBVIEW2_COLOR { A: 0, R: 0, G: 0, B: 0 } } else { COREWEBVIEW2_COLOR { A: 255, R: r, G: g, B: b } };
        let _ = c2.SetDefaultBackgroundColor(color);
    }
    let wv = controller.CoreWebView2()?;

    // Endurecido: sin herramientas, menús, atajos del navegador, barra de estado ni zoom.
    let st = wv.Settings()?;
    st.SetAreDevToolsEnabled(false)?;
    st.SetAreDefaultContextMenusEnabled(false)?;
    st.SetIsStatusBarEnabled(false)?;
    st.SetIsZoomControlEnabled(false)?;
    st.SetAreDefaultScriptDialogsEnabled(false)?;
    if let Ok(s3) = st.cast::<ICoreWebView2Settings3>() {
        let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
    }
    if let Ok(s4) = st.cast::<ICoreWebView2Settings4>() {
        let _ = s4.SetIsPasswordAutosaveEnabled(false);
        let _ = s4.SetIsGeneralAutofillEnabled(false);
    }
    let Some(dir) = assets_dir() else {
        return Err(windows::core::Error::from(windows::Win32::Foundation::E_FAIL));
    };
    let wv3: ICoreWebView2_3 = wv.cast()?;
    wv3.SetVirtualHostNameToFolderMapping(&HSTRING::from(HOST), &HSTRING::from(dir.as_os_str()), COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS)?;

    let mut token = EventRegistrationToken::default();
    wv.add_NavigationStarting(
        &NavigationStartingEventHandler::create(Box::new(|_, args: Option<ICoreWebView2NavigationStartingEventArgs>| {
            if let Some(a) = args {
                let mut uri = PWSTR::null();
                a.Uri(&mut uri)?;
                if !local(&take_pwstr(uri)) {
                    a.SetCancel(true)?;
                }
            }
            Ok(())
        })),
        &mut token,
    )?;
    wv.add_NewWindowRequested(
        &NewWindowRequestedEventHandler::create(Box::new(|_, args: Option<ICoreWebView2NewWindowRequestedEventArgs>| {
            if let Some(a) = args {
                a.SetHandled(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    wv.add_PermissionRequested(
        &PermissionRequestedEventHandler::create(Box::new(|_, args: Option<ICoreWebView2PermissionRequestedEventArgs>| {
            if let Some(a) = args {
                a.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    wv.AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
    let env_r = env.clone();
    wv.add_WebResourceRequested(
        &WebResourceRequestedEventHandler::create(Box::new(move |_, args: Option<ICoreWebView2WebResourceRequestedEventArgs>| {
            if let Some(a) = args {
                let req = a.Request()?;
                let mut uri = PWSTR::null();
                req.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                if !local(&uri) && !uri.starts_with("data:") {
                    let resp = env_r.CreateWebResourceResponse(None, 403, &HSTRING::from("Local content only"), &HSTRING::new())?;
                    a.SetResponse(&resp)?;
                }
            }
            Ok(())
        })),
        &mut token,
    )?;
    let msg_inner = Rc::downgrade(inner);
    wv.add_WebMessageReceived(
        &WebMessageReceivedEventHandler::create(Box::new(move |_, args: Option<ICoreWebView2WebMessageReceivedEventArgs>| {
            let (Some(a), Some(inner)) = (args, msg_inner.upgrade()) else {
                return Ok(());
            };
            let mut src = PWSTR::null();
            a.Source(&mut src)?;
            if !local(&take_pwstr(src)) {
                return Ok(());
            }
            let mut json = PWSTR::null();
            a.WebMessageAsJson(&mut json)?;
            let json = take_pwstr(json);
            on_message(&inner, &json);
            Ok(())
        })),
        &mut token,
    )?;
    let fail_inner = Rc::downgrade(inner);
    wv.add_ProcessFailed(
        &ProcessFailedEventHandler::create(Box::new(move |sender: Option<ICoreWebView2>, args: Option<ICoreWebView2ProcessFailedEventArgs>| {
            let kind = match &args {
                Some(a) => {
                    let mut k = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    a.ProcessFailedKind(&mut k)?;
                    k
                }
                None => return Ok(()),
            };
            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED {
                if let Some(s) = sender {
                    let _ = s.Reload();
                }
            } else if kind != COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED {
                if let Some(inner) = fail_inner.upgrade() {
                    fail(&inner, crate::i18n::t("Boberto se detuvo."));
                }
            }
            Ok(())
        })),
        &mut token,
    )?;

    {
        let mut s = inner.borrow_mut();
        s.controller = Some(controller.clone());
        s.webview = Some(wv.clone());
    }
    let url = page_url(page.file());
    // El perfil se comparte con .NET: su caché puede tener archivos del motor de otra versión (por ejemplo un
    // english.js anterior). La primera vista de cada arranque la vacía antes de cargar.
    thread_local! {
        static CLEARED: Cell<bool> = const { Cell::new(false) };
    }
    if !CLEARED.with(|c| c.replace(true)) {
        let nav = wv.clone();
        let url2 = url.clone();
        let done = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |_, _| {
            let _ = nav.Navigate(&HSTRING::from(url2));
            Ok(())
        }));
        if wv.CallDevToolsProtocolMethod(&HSTRING::from("Network.clearBrowserCache"), &HSTRING::from("{}"), &done).is_ok() {
            return Ok(());
        }
    }
    wv.Navigate(&HSTRING::from(url))?;
    Ok(())
}

fn on_message(inner: &Rc<RefCell<Inner>>, json: &str) {
    if json.len() > 6 * 1024 * 1024 {
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return;
    };
    let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let (parent, page) = {
        let s = inner.borrow();
        (s.parent, s.page.unwrap_or(Page::Studio))
    };
    match kind {
        "ready" => {
            if page == Page::Sync {
                return;
            }
            let (controller, wv, show) = {
                let mut s = inner.borrow_mut();
                s.ready = true;
                s.failed = None;
                s.theme_rev = crate::gui::theme_rev();
                s.events.push(Event::Ready);
                (s.controller.clone(), s.webview.clone(), s.bounds.is_some())
            };
            // Primero el tema y el movimiento, después se muestra (sin destello del aspecto por defecto).
            if let Some(wv) = wv {
                if std::env::var_os("BUZZPRINT_DEBUG").is_some() {
                    let h = ExecuteScriptCompletedHandler::create(Box::new(move |_, r: String| {
                        let _ = std::fs::write(std::env::temp_dir().join(format!("buzzprint-boberto-{page:?}-state.txt")), r);
                        Ok(())
                    }));
                    unsafe {
                        let _ = wv.ExecuteScript(&HSTRING::from("location.href+' | '+location.search+' | '+(document.body.innerText||'').slice(0,200)"), &h);
                    }
                }
                let script = theme_script(page);
                unsafe {
                    if !script.is_empty() {
                        let _ = wv.ExecuteScript(&HSTRING::from(script), None);
                    }
                    let motion = crate::gui::appearance().boberto_motion;
                    let media = format!("{{\"features\":[{{\"name\":\"prefers-reduced-motion\",\"value\":\"{}\"}}]}}", if motion { "no-preference" } else { "reduce" });
                    let _ = wv.CallDevToolsProtocolMethod(&HSTRING::from("Emulation.setEmulatedMedia"), &HSTRING::from(media), None);
                }
            }
            if let (Some(c), true) = (controller, show) {
                unsafe {
                    let _ = c.SetIsVisible(true);
                }
                inner.borrow_mut().visible = true;
            }
        }
        "studio" => inner.borrow_mut().events.push(Event::Studio),
        "avatar" => {
            // Studio manda su cuadro como PNG: queda compartido con .NET (<=512x512).
            if let Some(png) = v.get("png").and_then(|p| p.as_str()).and_then(|d| d.strip_prefix("data:image/png;base64,")) {
                if png.len() < 1024 * 1024 {
                    if let Some(bytes) = crate::crypto::base64_decode(png) {
                        if is_png(&bytes) {
                            let path = avatar_path();
                            if std::fs::read(&path).ok().as_deref() != Some(&bytes[..]) {
                                let _ = std::fs::write(&path, &bytes);
                            }
                        }
                    }
                }
            }
        }
        "native-sync" => {
            // Studio o Sync mandan el aspecto completo + atlas 4x4 (640x672) para el render nativo en egui.
            let mut updated = false;
            if let Some(look_val) = v.get("look") {
                if let Ok(mut look) = serde_json::from_value::<Look>(look_val.clone()) {
                    look.normalize();
                    if let Ok(pretty) = serde_json::to_vec_pretty(&look) {
                        save_bytes(&look_path(), &pretty);
                        updated = true;
                    }
                }
            }
            if let Some(b64) = v.get("sheet").and_then(|p| p.as_str()).and_then(|d| d.strip_prefix("data:image/png;base64,")) {
                if b64.len() <= 5 * 1024 * 1024 {
                    if let Some(bytes) = crate::crypto::base64_decode(b64) {
                        if png_dims(&bytes) == Some((SHEET_W, SHEET_H)) {
                            save_bytes(&sheet_path(), &bytes);
                            updated = true;
                        }
                    }
                }
            }
            if updated {
                bump_look_revision();
            }
        }
        "sync-done" => {
            if page == Page::Sync {
                crate::app::post(crate::app::Cmd::Run(Box::new(finish_sync)));
            }
        }
        "error" => {
            let m = v.get("message").and_then(|m| m.as_str()).unwrap_or("Boberto").chars().take(300).collect::<String>();
            if page == Page::Sync {
                crate::app::post(crate::app::Cmd::Run(Box::new(finish_sync)));
                return;
            }
            let live = inner.borrow().ready;
            // Con la escena viva, un error de script no la reemplaza.
            if !live {
                inner.borrow_mut().failed = Some(m.clone());
            }
            inner.borrow_mut().events.push(Event::Error(m));
        }
        _ => {}
    }
    crate::gui::host::wake(parent as HWND);
}
