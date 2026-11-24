//! Paletas DBMON (las mismas 23 de la versión .NET) y `appearance.json`.

use std::collections::HashMap;
use std::sync::OnceLock;

use egui::Color32;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::store::{data_dir, write_atomic};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Palette {
    pub id: String,
    pub label: String,
    pub dark_chrome: bool,
    pub colors: HashMap<String, String>,
}

pub fn palettes() -> &'static Vec<Palette> {
    static P: OnceLock<Vec<Palette>> = OnceLock::new();
    P.get_or_init(|| serde_json::from_str(include_str!("../assets/dbmon-palettes.json")).unwrap_or_default())
}

pub const ACCENTS: &[(&str, Option<&str>)] = &[
    ("Acento del tema", None),
    ("Cyan", Some("#00E5FF")),
    ("Blue", Some("#5B8CFF")),
    ("Violet", Some("#818CF8")),
    ("Emerald", Some("#10B981")),
    ("Teal", Some("#2DD4BF")),
    ("Amber", Some("#F59E0B")),
    ("Red", Some("#F43F5E")),
    ("Pink", Some("#EC4899")),
    ("Copper", Some("#CB6A3E")),
    ("Ice", Some("#88C0D0")),
    ("Lavender", Some("#BD93F9")),
    ("Magenta", Some("#FF2E97")),
    ("Lime", Some("#39FF14")),
    ("Rose", Some("#EB6F92")),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "PascalCase")]
pub struct Appearance {
    pub theme_id: String,
    pub accent_hex: Option<String>,
    pub ui_font: String,
    pub editor_font: String,
    pub ui_size: f64,
    pub editor_size: f64,
    pub compact: bool,
    pub rounded: bool,
    pub show_messages: bool,
    pub boberto_visible: bool,
    pub boberto_size: f64,
    pub motion: bool,
    pub boberto_background: String,
    pub boberto_motion: bool,
    pub sql_completion: bool,
    pub rounded_controls: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            theme_id: "neomirai".into(),
            accent_hex: Some("#F43F5E".into()),
            ui_font: "inter".into(),
            editor_font: "consolas".into(),
            ui_size: 13.0,
            editor_size: 15.0,
            compact: false,
            rounded: true,
            show_messages: true,
            boberto_visible: true,
            boberto_size: 84.0,
            motion: false,
            boberto_background: "theme".into(),
            boberto_motion: true,
            sql_completion: true,
            rounded_controls: true,
        }
    }
}

impl Appearance {
    pub fn load() -> Appearance {
        let path = data_dir().join("appearance.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Appearance>(t.trim_start_matches('\u{feff}')).ok())
            .map(|mut a| {
                if !palettes().iter().any(|p| p.id == a.theme_id) {
                    a.theme_id = "neomirai".into();
                }
                a.ui_size = a.ui_size.round().clamp(12.0, 18.0);
                a
            })
            .unwrap_or_default()
    }

    pub fn save(&self) {
        // Mismo esquema estricto que .NET: solo estas claves, con su forma.
        if let Ok(Value::Object(m)) = serde_json::to_value(self) {
            let m: Map<String, Value> = m;
            if let Ok(text) = serde_json::to_string_pretty(&Value::Object(m)) {
                let _ = write_atomic(&data_dir().join("appearance.json"), text.as_bytes());
            }
        }
    }
}

pub fn parse_hex(s: &str) -> Color32 {
    let h = s.trim().trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    match h.len() {
        8 => Color32::from_rgba_unmultiplied(v(2), v(4), v(6), v(0)), // #AARRGGBB de WPF
        6 => Color32::from_rgb(v(0), v(2), v(4)),
        _ => Color32::GRAY,
    }
}

pub fn to_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// Colores resueltos del tema actual (con el acento personalizado aplicado).
#[derive(Clone, Debug)]
pub struct Colors {
    pub dark: bool,
    pub canvas: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub alt_row: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub rule: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub header: Color32,
    pub header_ink: Color32,
    pub badge: Color32,
    pub badge_ink: Color32,
    pub selection: Color32,
    pub ok: Color32,
    pub error: Color32,
    pub line_number: Color32,
    // Derivados de DBMON (mismas mezclas que Themes.ApplyDerived de .NET).
    pub faint_border: Color32,
    pub strong_border: Color32,
    pub accent_ring: Color32,
    pub accent_card_ring: Color32,
    pub accent_rule: Color32,
    pub accent_row_rule: Color32,
    pub accent_weak: Color32,
    pub accent_tint: Color32,
    pub topbar_rule: Color32,
    pub topbar_glow: Color32,
    pub ok_glow: Color32,
}

fn luminance(c: Color32) -> f64 {
    let lin = |b: u8| {
        let v = b as f64 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

fn contrast(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 * t + y as f32 * (1.0 - t)).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0).round() as u8)
}

impl Colors {
    pub fn resolve(app: &Appearance) -> Colors {
        let pals = palettes();
        let p = pals.iter().find(|p| p.id == app.theme_id).or_else(|| pals.first());
        let get = |k: &str| p.and_then(|p| p.colors.get(k)).map(|s| parse_hex(s)).unwrap_or(Color32::GRAY);
        let dark = p.map(|p| p.dark_chrome).unwrap_or(true);
        let surface = get("Surface");
        let mut accent = app.accent_hex.as_deref().map(parse_hex).unwrap_or_else(|| get("Accent"));
        // Paletas claras: oscurecer un acento muy brillante hasta que se lea (como .NET).
        while !dark && contrast(accent, surface) < 4.5 {
            accent = Color32::from_rgb((accent.r() as f32 * 0.9) as u8, (accent.g() as f32 * 0.9) as u8, (accent.b() as f32 * 0.9) as u8);
        }
        let on_accent = if contrast(accent, Color32::WHITE) >= contrast(accent, Color32::from_rgb(7, 16, 31)) {
            Color32::WHITE
        } else {
            Color32::from_rgb(7, 16, 31)
        };
        Colors {
            dark,
            canvas: get("Canvas"),
            surface,
            raised: get("SurfaceRaised"),
            alt_row: get("AlternateRow"),
            ink: get("Ink"),
            muted: get("Muted"),
            rule: get("Rule"),
            accent,
            on_accent,
            header: get("Header"),
            header_ink: get("HeaderInk"),
            badge: get("Badge"),
            badge_ink: get("BadgeInk"),
            selection: mix(accent, surface, 0.45),
            ok: get("SqlString"),
            error: get("ErrorInk"),
            line_number: get("LineNumber"),
            faint_border: alpha(get("Ink"), 0.08),
            strong_border: alpha(get("Ink"), 0.16),
            accent_ring: alpha(accent, 0.60),
            accent_card_ring: alpha(accent, 0.32),
            accent_rule: alpha(accent, 0.45),
            accent_row_rule: alpha(accent, 0.10),
            accent_weak: alpha(accent, 0.13),
            accent_tint: alpha(accent, 0.07),
            topbar_rule: alpha(accent, 0.37),
            topbar_glow: alpha(accent, 0.14),
            ok_glow: alpha(get("SqlString"), 0.25),
        }
    }
}

/// Caché de fuentes por proceso: las fuentes estáticas son cero copia y las de Windows (`SegoeIcons.ttf`,
/// `segoeui.ttf`) se leen del disco una sola vez y se comparten por `Arc` entre todas las ventanas.
fn cached_inter() -> std::sync::Arc<egui::FontData> {
    static F: OnceLock<std::sync::Arc<egui::FontData>> = OnceLock::new();
    F.get_or_init(|| std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")))).clone()
}

fn cached_inter_semibold() -> std::sync::Arc<egui::FontData> {
    static F: OnceLock<std::sync::Arc<egui::FontData>> = OnceLock::new();
    F.get_or_init(|| std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")))).clone()
}

fn cached_jetbrains() -> std::sync::Arc<egui::FontData> {
    static F: OnceLock<std::sync::Arc<egui::FontData>> = OnceLock::new();
    F.get_or_init(|| std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")))).clone()
}

fn cached_segoe_ui() -> Option<std::sync::Arc<egui::FontData>> {
    static F: OnceLock<Option<std::sync::Arc<egui::FontData>>> = OnceLock::new();
    F.get_or_init(|| {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        std::fs::read(format!("{windir}\\Fonts\\segoeui.ttf")).ok().map(|b| std::sync::Arc::new(egui::FontData::from_owned(b)))
    })
    .clone()
}

fn cached_icons() -> Option<std::sync::Arc<egui::FontData>> {
    static F: OnceLock<Option<std::sync::Arc<egui::FontData>>> = OnceLock::new();
    F.get_or_init(|| {
        // Misma fuente de íconos que .NET: Segoe Fluent Icons en Windows 11, MDL2 en Windows 10.
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        for file in ["SegoeIcons.ttf", "segmdl2.ttf"] {
            if let Ok(bytes) = std::fs::read(format!("{windir}\\Fonts\\{file}")) {
                return Some(std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            }
        }
        None
    })
    .clone()
}

/// Aplica tema y tipografía a un contexto de egui.
pub fn apply(ctx: &egui::Context, app: &Appearance) -> Colors {
    use egui::{CornerRadius, FontFamily, FontId, Stroke, TextStyle};
    let c = Colors::resolve(app);

    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert("inter".into(), cached_inter());
    fonts.font_data.insert("inter-semibold".into(), cached_inter_semibold());
    fonts.font_data.insert("jetbrains".into(), cached_jetbrains());

    let mut prop = vec!["inter".to_string(), "jetbrains".to_string()];
    if app.ui_font != "inter" {
        if let Some(segoe) = cached_segoe_ui() {
            fonts.font_data.insert("segoeui".into(), segoe);
            prop.insert(0, "segoeui".into());
        }
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("bold".into()), vec!["inter-semibold".into(), "inter".into(), "jetbrains".into()]);
    fonts.families.insert(FontFamily::Monospace, vec!["jetbrains".into(), "inter".into()]);

    if let Some(icons) = cached_icons() {
        fonts.font_data.insert("icons".into(), icons);
        fonts.families.insert(FontFamily::Name("icons".into()), vec!["icons".into(), "inter".into()]);
    } else {
        fonts.families.insert(FontFamily::Name("icons".into()), vec!["inter".into()]);
    }
    ctx.set_fonts(fonts);

    let size = app.ui_size as f32;
    let radius = if app.rounded_controls { 7 } else { 0 };
    ctx.style_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::new((size - 1.0).max(11.0), FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(size, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(size + 1.5, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(size + 6.0, FontFamily::Name("bold".into()))),
            (TextStyle::Monospace, FontId::new((size - 1.0).max(11.0), FontFamily::Monospace)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, if app.compact { 4.0 } else { 6.0 });
        s.spacing.button_padding = egui::vec2(14.0, if app.compact { 5.0 } else { 7.0 });
        s.spacing.interact_size.y = if app.compact { 28.0 } else { 32.0 };

        let v = &mut s.visuals;
        *v = if c.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.override_text_color = Some(c.ink);
        v.panel_fill = c.surface;
        v.window_fill = c.raised;
        v.extreme_bg_color = c.canvas;
        v.faint_bg_color = c.alt_row;
        v.code_bg_color = c.canvas;
        v.hyperlink_color = c.accent;
        v.selection.bg_fill = c.selection;
        v.selection.stroke = Stroke::new(1.0, c.ink);
        // Flotantes (menús, desplegables, diálogos) como el ContextMenu de .NET: superficie elevada, borde
        // fuerte, radio de tira y una sombra amplia hacia abajo.
        v.window_stroke = Stroke::new(1.0, c.strong_border);
        v.window_corner_radius = CornerRadius::same(if app.rounded { 12 } else { 0 });
        v.menu_corner_radius = CornerRadius::same(if app.rounded { 10 } else { 0 });
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(if c.dark { 120 } else { 50 }) };
        v.window_shadow = egui::epaint::Shadow { offset: [0, 10], blur: 32, spread: 0, color: Color32::from_black_alpha(if c.dark { 130 } else { 55 }) };
        s.spacing.menu_margin = egui::Margin::same(6);
        let ws = &mut v.widgets;
        for w in [&mut ws.noninteractive, &mut ws.inactive, &mut ws.hovered, &mut ws.active, &mut ws.open] {
            w.corner_radius = CornerRadius::same(radius);
            w.fg_stroke.color = c.ink;
        }
        ws.noninteractive.bg_fill = c.surface;
        ws.noninteractive.weak_bg_fill = c.surface;
        ws.noninteractive.bg_stroke = Stroke::new(1.0, alpha(c.ink, 0.08));
        ws.inactive.bg_fill = c.raised;
        ws.inactive.weak_bg_fill = c.raised;
        ws.inactive.bg_stroke = Stroke::new(1.0, alpha(c.ink, 0.12));
        ws.hovered.bg_fill = mix(c.accent, c.raised, 0.18);
        ws.hovered.weak_bg_fill = mix(c.accent, c.raised, 0.18);
        ws.hovered.bg_stroke = Stroke::new(1.0, alpha(c.accent, 0.6));
        ws.active.bg_fill = mix(c.accent, c.raised, 0.32);
        ws.active.weak_bg_fill = mix(c.accent, c.raised, 0.32);
        ws.active.bg_stroke = Stroke::new(1.0, c.accent);
        ws.open.bg_fill = mix(c.accent, c.raised, 0.22);
        ws.open.weak_bg_fill = mix(c.accent, c.raised, 0.22);
    });
    c
}
