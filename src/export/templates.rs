//! Plantillas de documento: las 12 incluidas y hasta 12 propias en `%LOCALAPPDATA%\BuzzPrint\templates`
//! (mismo JSON que la versión .NET).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::t;
use crate::store::data_dir;

pub const MAX_CUSTOM: usize = 12;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    pub accent: String,
    pub ink: String,
    pub muted: String,
    pub faint: String,
    pub rule: String,
    pub page: String,
    pub cover_fill: String,
    pub cover_ink: String,
    pub frame: String,
    pub heading_font: String,
    pub body_font: String,
    pub cover: String,
    pub steps: String,
    pub top_rule: bool,
    pub footer: String,
    pub org: String,
    pub header: String,
    pub footer_style: String,
    pub layout: String,
    pub logo: String,
    /// Para qué formato es: "pptx", "docx", "pdf" (también sirve para "Una imagen") o "" (todos).
    pub format: String,
    /// Cómo se enmarca cada captura: "line", "shadow" o "window" (ventana de navegador).
    pub frame_style: String,
}

impl Default for Template {
    fn default() -> Self {
        Template {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            accent: "app".into(),
            ink: "1F2328".into(),
            muted: "6B7280".into(),
            faint: "9CA3AF".into(),
            rule: "E5E7EB".into(),
            page: "FFFFFF".into(),
            cover_fill: String::new(),
            cover_ink: "FFFFFF".into(),
            frame: "D1D5DB".into(),
            heading_font: "Segoe UI Semibold".into(),
            body_font: "Segoe UI".into(),
            cover: "bar".into(),
            steps: "inline".into(),
            top_rule: false,
            footer: "BuzzPrint".into(),
            org: String::new(),
            header: "rule".into(),
            footer_style: "text".into(),
            layout: "stack".into(),
            logo: String::new(),
            format: String::new(),
            frame_style: "line".into(),
        }
    }
}

pub fn dir() -> PathBuf {
    data_dir().join("templates")
}

impl Template {
    pub fn logo_path(&self) -> Option<PathBuf> {
        if self.logo.is_empty() {
            return None;
        }
        let p = dir().join(&self.logo);
        p.exists().then_some(p)
    }
    /// Nombre para mostrar: las incluidas se traducen.
    pub fn display_name(&self) -> String {
        if self.built_in() { t(&self.name) } else { self.name.clone() }
    }
    pub fn built_in(&self) -> bool {
        builtins().iter().any(|b| b.id == self.id)
    }
    pub fn accent_hex(&self, app: &str) -> String {
        if self.accent == "app" { app.to_string() } else { self.accent.clone() }
    }
    pub fn cover_fill_hex(&self, accent: &str) -> String {
        if self.cover_fill.len() == 6 { self.cover_fill.clone() } else { accent.to_string() }
    }
    pub fn frame_hex(&self, accent: &str) -> String {
        if self.frame == "accent" { accent.to_string() } else { self.frame.clone() }
    }

    /// Corrige lo que esté fuera de rango (archivos editados a mano, respuestas de IA).
    pub fn normalized(&self) -> Template {
        let d = Template::default();
        let hex = |v: &str, fallback: &str| -> String {
            let v = v.trim().trim_start_matches('#').to_ascii_uppercase();
            if v.len() == 6 && v.chars().all(|c| c.is_ascii_hexdigit()) { v } else { fallback.to_string() }
        };
        let font = |f: &str, fallback: &str| -> String {
            let f = f.trim();
            if !f.is_empty() && f.chars().count() <= 40 && f.chars().all(|c| c.is_alphanumeric() || " .-".contains(c)) { f.to_string() } else { fallback.to_string() }
        };
        let pick = |v: &str, allowed: &[&str], fallback: &str| -> String { if allowed.contains(&v) { v.to_string() } else { fallback.to_string() } };
        let mut t2 = self.clone();
        t2.name = t2.name.trim().chars().take(40).collect();
        if t2.name.is_empty() {
            t2.name = t("Plantilla");
        }
        t2.description = t2.description.trim().chars().take(120).collect();
        t2.accent = if t2.accent.trim().eq_ignore_ascii_case("app") { "app".into() } else { hex(&t2.accent, "app") };
        t2.ink = hex(&t2.ink, &d.ink);
        t2.muted = hex(&t2.muted, &d.muted);
        t2.faint = hex(&t2.faint, &d.faint);
        t2.rule = hex(&t2.rule, &d.rule);
        t2.page = hex(&t2.page, &d.page);
        t2.cover_fill = if t2.cover_fill.trim().is_empty() { String::new() } else { hex(&t2.cover_fill, "") };
        t2.cover_ink = hex(&t2.cover_ink, &d.cover_ink);
        let fr = t2.frame.trim().to_ascii_lowercase();
        t2.frame = if fr == "none" || fr == "accent" { fr } else { hex(&t2.frame, &d.frame) };
        t2.heading_font = font(&t2.heading_font, &d.heading_font);
        t2.body_font = font(&t2.body_font, &d.body_font);
        t2.cover = pick(&t2.cover, &["bar", "band", "full", "minimal", "hero", "split", "frame"], &d.cover);
        t2.header = pick(&t2.header, &["rule", "band", "plain"], &d.header);
        t2.footer_style = pick(&t2.footer_style, &["text", "bar"], &d.footer_style);
        t2.layout = pick(&t2.layout, &["stack", "split", "split-right", "full", "card", "sidebar"], &d.layout);
        t2.steps = pick(&t2.steps, &["inline", "pill", "number", "circle"], &d.steps);
        t2.format = pick(&t2.format, &["pptx", "docx", "pdf"], "");
        t2.frame_style = pick(&t2.frame_style, &["line", "shadow", "window"], &d.frame_style);
        // Word fluye: no tiene portadas de índice ni de marco, ni páginas a todo color o con columna.
        if t2.format == "docx" {
            if t2.cover == "split" || t2.cover == "frame" {
                t2.cover = "bar".into();
            }
            if matches!(t2.layout.as_str(), "full" | "sidebar") {
                t2.layout = "stack".into();
            }
        }
        t2.org = t2.org.trim().chars().take(80).collect();
        t2.footer = t2.footer.trim().chars().take(60).collect();
        let ok_logo = t2.logo.ends_with(".png") && t2.logo[..t2.logo.len() - 4].chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') && t2.logo.len() > 4;
        if !ok_logo {
            t2.logo = String::new();
        }
        t2
    }

    /// PDF solo tiene las fuentes estándar: serif = Times, monoespaciada = Courier, el resto Helvetica.
    pub fn pdf_family(font: &str) -> &'static str {
        let f = font.to_lowercase();
        if ["consolas", "courier", "mono", "code", "cascadia"].iter().any(|k| f.contains(k)) {
            return "Courier";
        }
        if ["georgia", "times", "garamond", "cambria", "serif", "book", "palatino", "baskerville", "constantia"].iter().any(|k| f.contains(k)) && !f.contains("sans") {
            return "Times";
        }
        "Helvetica"
    }
}

/// Una plantilla incluida: parte de la predeterminada y cambia lo que haga falta.
fn mk(id: &str, format: &str, name: &str, desc: &str, f: impl FnOnce(&mut Template)) -> Template {
    let mut t = Template { id: id.into(), name: name.into(), description: desc.into(), format: format.into(), ..Template::default() };
    f(&mut t);
    t
}

/// Colores de un tema oscuro (texto claro sobre fondo `page`).
fn dark_page(t: &mut Template, page: &str, accent: &str) {
    t.page = page.into();
    t.ink = "F1F5F9".into();
    t.muted = "94A3B8".into();
    t.faint = "64748B".into();
    t.rule = "1E293B".into();
    t.accent = accent.into();
}

pub fn builtins() -> Vec<Template> {
    vec![
        // ---------------------------------------------------------------- PowerPoint
        mk("ppt-pizarra", "pptx", "Pizarra", "Limpia: barra del acento en la portada, título con línea y la captura grande.", |_| {}),
        mk("ppt-ejecutivo", "pptx", "Ejecutivo", "Portada con la primera captura, banda oscura arriba y el texto al costado.", |t| {
            t.accent = "0A84C6".into();
            t.ink = "13233A".into();
            t.muted = "4A5A70".into();
            t.faint = "8A97A8".into();
            t.rule = "D7E0EA".into();
            t.cover_fill = "13233A".into();
            t.cover = "hero".into();
            t.header = "band".into();
            t.steps = "pill".into();
            t.footer_style = "bar".into();
            t.layout = "split".into();
            t.frame = "none".into();
            t.frame_style = "shadow".into();
        }),
        mk("ppt-escenario", "pptx", "Escenario", "Fondo oscuro, la captura a lo ancho y el título en una franja, como un keynote.", |t| {
            dark_page(t, "0F172A", "38BDF8");
            t.cover_fill = "020617".into();
            t.cover_ink = "F8FAFC".into();
            t.cover = "full".into();
            t.layout = "full".into();
            t.steps = "number".into();
            t.frame = "none".into();
            t.frame_style = "shadow".into();
            t.heading_font = "Segoe UI Black".into();
        }),
        mk("ppt-tarjetas", "pptx", "Tarjetas", "Índice en la portada y cada paso en una tarjeta blanca con sombra.", |t| {
            t.page = "EEF1F5".into();
            t.accent = "7C3AED".into();
            t.ink = "1F1235".into();
            t.cover_fill = "2E1065".into();
            t.cover = "split".into();
            t.layout = "card".into();
            t.steps = "circle".into();
            t.frame = "E5E7EB".into();
        }),
        mk("ppt-lateral", "pptx", "Lateral", "Columna de color con el número grande y la nota; la captura ocupa el resto.", |t| {
            t.accent = "F59E0B".into();
            t.ink = "111827".into();
            t.cover_fill = "111827".into();
            t.cover = "band".into();
            t.layout = "sidebar".into();
            t.steps = "number".into();
            t.frame = "E5E7EB".into();
            t.frame_style = "shadow".into();
            t.heading_font = "Segoe UI Black".into();
        }),
        mk("ppt-navegador", "pptx", "Navegador", "Cada captura en una ventana de navegador, con el texto a la derecha.", |t| {
            t.accent = "2563EB".into();
            t.ink = "0F172A".into();
            t.cover = "minimal".into();
            t.layout = "split-right".into();
            t.header = "plain".into();
            t.steps = "pill".into();
            t.frame = "CBD5E1".into();
            t.frame_style = "window".into();
            t.heading_font = "Georgia".into();
        }),
        // ---------------------------------------------------------------- Word
        mk("doc-manual", "docx", "Manual", "Documento clásico: título con barra, pasos numerados y capturas con borde fino.", |_| {}),
        mk("doc-informe", "docx", "Informe", "Portada con banda de color, títulos en banda y pie con número de página.", |t| {
            t.accent = "0F766E".into();
            t.ink = "102A2A".into();
            t.muted = "4B6363".into();
            t.faint = "8FA3A3".into();
            t.rule = "D6E6E4".into();
            t.cover_fill = "0F766E".into();
            t.cover = "band".into();
            t.header = "band".into();
            t.steps = "pill".into();
            t.footer_style = "bar".into();
            t.frame = "none".into();
            t.frame_style = "shadow".into();
        }),
        mk("doc-dos-columnas", "docx", "Dos columnas", "Texto a la izquierda y captura a la derecha, en tabla; tipografía con serif.", |t| {
            t.accent = "B45309".into();
            t.ink = "1C1917".into();
            t.muted = "78716C".into();
            t.rule = "E7DCCB".into();
            t.page = "FFFBF5".into();
            t.cover = "minimal".into();
            t.layout = "split".into();
            t.header = "plain".into();
            t.steps = "number".into();
            t.frame = "E7DCCB".into();
            t.heading_font = "Georgia".into();
            t.body_font = "Georgia".into();
        }),
        mk("doc-tarjetas", "docx", "Recuadros", "Cada paso dentro de un recuadro con fondo suave y borde.", |t| {
            t.accent = "7C3AED".into();
            t.ink = "1F1235".into();
            t.cover_fill = "4C1D95".into();
            t.cover = "full".into();
            t.layout = "card".into();
            t.header = "plain".into();
            t.steps = "circle".into();
            t.frame = "none".into();
        }),
        mk("doc-tecnico", "docx", "Técnico", "Títulos monoespaciados y portada con la primera captura; para documentación.", |t| {
            t.accent = "0EA5E9".into();
            t.ink = "0F172A".into();
            t.muted = "475569".into();
            t.faint = "94A3B8".into();
            t.rule = "E2E8F0".into();
            t.cover = "hero".into();
            t.top_rule = true;
            t.steps = "pill".into();
            t.frame = "94A3B8".into();
            t.heading_font = "Consolas".into();
        }),
        mk("doc-minimal", "docx", "Minimal", "Blanco y negro, mucho aire y sin bordes.", |t| {
            t.accent = "111827".into();
            t.muted = "6B7280".into();
            t.rule = "EEEEEE".into();
            t.cover = "minimal".into();
            t.header = "plain".into();
            t.steps = "number".into();
            t.frame = "none".into();
            t.heading_font = "Segoe UI Semilight".into();
            t.footer = String::new();
        }),
        // ---------------------------------------------------------------- PDF (y una sola imagen)
        mk("pdf-clasico", "pdf", "Clásico", "Barra del acento en la portada, título con línea y la captura centrada.", |_| {}),
        mk("pdf-ejecutivo", "pdf", "Ejecutivo", "Portada con imagen, banda en cada página y el texto al costado de la captura.", |t| {
            t.accent = "0A84C6".into();
            t.ink = "13233A".into();
            t.muted = "4A5A70".into();
            t.faint = "8A97A8".into();
            t.rule = "D7E0EA".into();
            t.cover_fill = "13233A".into();
            t.cover = "hero".into();
            t.header = "band".into();
            t.steps = "pill".into();
            t.footer_style = "bar".into();
            t.layout = "split".into();
            t.frame = "none".into();
            t.frame_style = "shadow".into();
        }),
        mk("pdf-revista", "pdf", "Revista", "Portada con marco y tres capturas; cada página con la captura sobre un panel oscuro.", |t| {
            t.accent = "B45309".into();
            t.ink = "1C1917".into();
            t.muted = "78716C".into();
            t.faint = "A8A29E".into();
            t.rule = "E7DCCB".into();
            t.page = "FFFBF5".into();
            t.cover_fill = "1C1917".into();
            t.cover = "frame".into();
            t.layout = "full".into();
            t.steps = "number".into();
            t.frame = "none".into();
            t.frame_style = "shadow".into();
            t.heading_font = "Georgia".into();
            t.body_font = "Georgia".into();
        }),
        mk("pdf-tarjetas", "pdf", "Tarjetas", "Índice en la portada y cada paso en una tarjeta blanca sobre fondo gris.", |t| {
            t.page = "EEF1F5".into();
            t.accent = "2563EB".into();
            t.ink = "0F172A".into();
            t.cover_fill = "1E3A8A".into();
            t.cover = "split".into();
            t.layout = "card".into();
            t.steps = "circle".into();
            t.frame = "E5E7EB".into();
        }),
        mk("pdf-lateral", "pdf", "Lateral", "Columna de color con el número grande; la captura ocupa el resto de la hoja.", |t| {
            t.accent = "10B981".into();
            t.ink = "052E2B".into();
            t.cover_fill = "064E3B".into();
            t.cover = "full".into();
            t.layout = "sidebar".into();
            t.steps = "number".into();
            t.footer_style = "bar".into();
            t.frame = "D1D5DB".into();
            t.frame_style = "shadow".into();
        }),
        mk("pdf-oscuro", "pdf", "Oscuro", "Hoja oscura, títulos monoespaciados y cada captura en una ventana.", |t| {
            dark_page(t, "0B1220", "22D3EE");
            t.cover_fill = "020617".into();
            t.cover = "hero".into();
            t.top_rule = true;
            t.steps = "pill".into();
            t.frame = "none".into();
            t.frame_style = "window".into();
            t.heading_font = "Consolas".into();
        }),
    ]
}

/// El formato de plantilla que corresponde a cada formato de exportación ("Una imagen" usa las de PDF).
pub fn kind_of(format: super::Format) -> &'static str {
    match format {
        super::Format::Pptx => "pptx",
        super::Format::Docx => "docx",
        _ => "pdf",
    }
}

/// Las plantillas que sirven para un formato (las propias sin formato aparecen en todos).
pub fn for_format(format: super::Format) -> Vec<Template> {
    let k = kind_of(format);
    all().into_iter().filter(|t| t.format.is_empty() || t.format == k).collect()
}

/// La plantilla para un formato: la pedida si sirve, si no la primera de ese formato.
pub fn get_for(id: &str, format: super::Format) -> Template {
    let list = for_format(format);
    list.iter().find(|x| x.id == id).cloned().unwrap_or_else(|| list[0].clone())
}

pub fn custom() -> Vec<Template> {
    let mut list = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                if let Some(mut tp) = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<Template>(s.trim_start_matches('\u{feff}')).ok()) {
                    tp.id = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    list.push(tp.normalized());
                }
            }
        }
    }
    list.sort_by_key(|a| a.name.to_lowercase());
    list
}

pub fn all() -> Vec<Template> {
    let mut v = builtins();
    v.extend(custom());
    v
}

pub fn get(id: &str) -> Template {
    all().into_iter().find(|x| x.id == id).unwrap_or_else(|| builtins().remove(0))
}

/// Guarda una plantilla propia (nueva si no tiene id o es una incluida). Devuelve la guardada.
pub fn save(tp: &Template) -> Result<Template, String> {
    let mut tp = tp.normalized();
    if tp.id.is_empty() || tp.built_in() {
        if custom().len() >= MAX_CUSTOM {
            return Err(crate::i18n::f("Ya tenés {0} plantillas propias. Eliminá alguna para crear otra.", &[&MAX_CUSTOM]));
        }
        tp.id = format!("custom-{}", &crate::store::new_id()[..10]);
    }
    let _ = std::fs::create_dir_all(dir());
    let text = serde_json::to_string_pretty(&tp).map_err(|e| e.to_string())?;
    std::fs::write(dir().join(format!("{}.json", tp.id)), text).map_err(|e| e.to_string())?;
    Ok(tp)
}

/// Copia un logo a la carpeta de plantillas como PNG de hasta 800 px de ancho (igual que .NET). Devuelve el nombre.
pub fn import_logo(path: &std::path::Path) -> Result<String, String> {
    let img = crate::gdip::load_scaled(path, 800).ok_or_else(|| t("No se pudo abrir la imagen."))?;
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let name = format!("logo-{}.png", &crate::store::new_id()[..10]);
    crate::image::save_png(&img, &dir().join(&name)).map_err(|e| e.to_string())?;
    Ok(name)
}

pub fn delete(id: &str) {
    if builtins().iter().any(|b| b.id == id) {
        return;
    }
    if let Some(tp) = custom().into_iter().find(|x| x.id == id) {
        if let Some(l) = tp.logo_path() {
            let _ = std::fs::remove_file(l);
        }
    }
    let _ = std::fs::remove_file(dir().join(format!("{id}.json")));
}
