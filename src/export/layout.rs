//! Diseño de páginas compartido: cada plantilla arma sus páginas como una lista de piezas (rellenos, bordes,
//! círculos, textos e imágenes) en puntos, con origen arriba a la izquierda. PowerPoint, PDF y la imagen PNG
//! dibujan exactamente esas piezas, y la vista previa de la app también: lo que se ve es lo que se exporta.
//! Word fluye (el texto se acomoda solo), así que [`flow_docx`] simula ese flujo con las mismas reglas que el
//! escritor de .docx, para que su vista previa muestre los saltos de página reales.

use super::{step_label, subtitle, Format, Opts, Template};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> R {
        R { x, y, w: w.max(0.0), h: h.max(0.0) }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn inset(&self, d: f32) -> R {
        R::new(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Al {
    L,
    C,
    R,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Va {
    T,
    M,
    B,
}

/// De dónde sale una imagen: la captura número `n` (en el orden de exportación) o el logo de la plantilla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Src {
    Cap(usize),
    Logo,
}

#[derive(Clone, Debug)]
pub enum El {
    Fill { r: R, color: String, radius: f32 },
    Line { r: R, color: String, width: f32, radius: f32 },
    Dot { r: R, color: String },
    Text { r: R, text: String, size: f32, head: bool, color: String, al: Al, va: Va, lines: usize },
    Img { r: R, src: Src },
}

/// Lo que el diseño necesita saber de cada captura (sin cargar la imagen).
#[derive(Clone, Debug)]
pub struct Shot {
    pub title: String,
    pub note: String,
    pub meta: String,
    pub w: u32,
    pub h: u32,
}

/// Un documento ya diseñado: tamaño de página en puntos, color de fondo y las piezas de cada página.
pub struct Doc {
    pub w: f32,
    pub h: f32,
    pub page: String,
    pub pages: Vec<Vec<El>>,
}

pub const SLIDE: (f32, f32) = (960.0, 540.0);
pub const A4: (f32, f32) = (595.3, 841.9);

/// Tamaño de página de cada formato.
pub fn page_size(format: Format, landscape: bool) -> (f32, f32) {
    match format {
        Format::Pptx => SLIDE,
        _ if landscape => (A4.1, A4.0),
        _ => A4,
    }
}

/// Mezcla dos colores hex (`t` = cuánto de `b`).
pub fn mix(a: &str, b: &str, t: f32) -> String {
    let p = |h: &str, i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32;
    let c = |i: usize| (p(a, i) + (p(b, i) - p(a, i)) * t).round().clamp(0.0, 255.0) as u8;
    format!("{:02X}{:02X}{:02X}", c(0), c(2), c(4))
}

/// ¿Es oscuro? (para elegir texto claro u oscuro encima).
pub fn dark(hex: &str) -> bool {
    let p = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32;
    0.2126 * p(0) + 0.7152 * p(2) + 0.0722 * p(4) < 140.0
}

/// Ancho aproximado de un texto (promedio de letras de las fuentes sans): alcanza para ubicar etiquetas.
pub fn est(text: &str, size: f32) -> f32 {
    text.chars().map(|c| if c == ' ' { 0.28 } else if c.is_uppercase() || c.is_ascii_digit() { 0.62 } else if "il.,:;'|!".contains(c) { 0.27 } else if "mwMW".contains(c) { 0.85 } else { 0.53 }).sum::<f32>() * size
}

/// Cuántas líneas ocupa un texto en `w` puntos (estimado).
pub fn est_lines(text: &str, size: f32, w: f32) -> usize {
    if text.trim().is_empty() {
        return 0;
    }
    text.replace('\r', "").split('\n').map(|p| ((est(p, size) / w.max(10.0)).ceil() as usize).max(1)).sum()
}

/// Encaja una imagen de `iw`×`ih` px en `area`, centrada, sin agrandarla más de `max` puntos por píxel.
pub fn fit(iw: u32, ih: u32, area: R, max: f32) -> R {
    let (iw, ih) = (iw.max(1) as f32, ih.max(1) as f32);
    let s = (area.w / iw).min(area.h / ih).min(max).max(0.0);
    let (w, h) = (iw * s, ih * s);
    R::new(area.x + (area.w - w) / 2.0, area.y + (area.h - h) / 2.0, w, h)
}

struct B {
    els: Vec<El>,
}

impl B {
    fn new() -> B {
        B { els: Vec::new() }
    }
    fn fill(&mut self, r: R, color: &str) {
        self.fill_r(r, color, 0.0);
    }
    fn fill_r(&mut self, r: R, color: &str, radius: f32) {
        if r.w > 0.0 && r.h > 0.0 {
            self.els.push(El::Fill { r, color: color.into(), radius });
        }
    }
    fn line(&mut self, r: R, color: &str, width: f32, radius: f32) {
        self.els.push(El::Line { r, color: color.into(), width, radius });
    }
    fn dot(&mut self, r: R, color: &str) {
        self.els.push(El::Dot { r, color: color.into() });
    }
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, r: R, text: &str, size: f32, head: bool, color: &str, al: Al, va: Va, lines: usize) {
        if !text.trim().is_empty() && r.w > 4.0 {
            self.els.push(El::Text { r, text: text.trim().to_string(), size, head, color: color.into(), al, va, lines });
        }
    }
    fn img(&mut self, r: R, src: Src) {
        self.els.push(El::Img { r, src });
    }
}

/// Medidas y colores comunes a todas las páginas de un documento.
struct Kit<'a> {
    o: &'a Opts,
    t: &'a Template,
    w: f32,
    h: f32,
    m: f32,
    slides: bool,
    brand: String,
    on_brand: String,
    logo: Option<f32>,
    n: usize,
}

impl Kit<'_> {
    /// Tamaño para diapositiva o para hoja.
    fn z(&self, slide: f32, doc: f32) -> f32 {
        if self.slides { slide } else { doc }
    }

    /// Logo de `h` puntos de alto con su ancho; `plate`: fondo blanco (sobre colores).
    fn logo(&self, b: &mut B, x: f32, y: f32, h: f32, plate: bool, right: bool) -> f32 {
        let Some(aspect) = self.logo else { return 0.0 };
        let w = (h * aspect).min(h * 5.0);
        let x = if right { x - w } else { x };
        if plate {
            b.fill_r(R::new(x - h * 0.22, y - h * 0.22, w + h * 0.44, h * 1.44), "FFFFFF", h * 0.18);
        }
        b.img(R::new(x, y, w, h), Src::Logo);
        w
    }

    /// Etiqueta del paso (según la plantilla) a partir de `x`, centrada en una franja de alto `hh`.
    /// Devuelve dónde sigue el título.
    fn label(&self, b: &mut B, x: f32, y: f32, hh: f32, i: usize, on_band: bool, size: f32) -> f32 {
        if !self.o.numbers {
            return x;
        }
        let accent = &self.o.accent;
        let (fill, ink) = if on_band { (self.on_brand.as_str(), self.brand.as_str()) } else { (accent.as_str(), "FFFFFF") };
        match self.t.steps.as_str() {
            "pill" => {
                let ls = size * 0.66;
                let text = step_label(i);
                let pw = est(&text, ls) + ls * 1.6;
                let ph = ls * 1.9;
                b.fill_r(R::new(x, y + (hh - ph) / 2.0, pw, ph), fill, ph / 2.0);
                b.text(R::new(x, y + (hh - ph) / 2.0, pw, ph), &text, ls, true, ink, Al::C, Va::M, 1);
                x + pw + size * 0.5
            }
            "number" => {
                let s = size * 1.55;
                let text = format!("{:02}", i + 1);
                let w = est(&text, s) + 2.0;
                b.text(R::new(x, y, w + 4.0, hh), &text, s, true, if on_band { &self.on_brand } else { accent }, Al::L, Va::M, 1);
                x + w + size * 0.6
            }
            "circle" => {
                let d = size * 1.75;
                let r = R::new(x, y + (hh - d) / 2.0, d, d);
                b.dot(r, fill);
                b.text(r, &(i + 1).to_string(), size * 0.8, true, ink, Al::C, Va::M, 1);
                x + d + size * 0.55
            }
            _ => {
                let text = step_label(i);
                let w = est(&text, size) + 2.0;
                b.text(R::new(x, y, w + 6.0, hh), &text, size, true, if on_band { &self.on_brand } else { accent }, Al::L, Va::M, 1);
                x + w + size * 0.7
            }
        }
    }

    /// La captura con su marco (línea, sombra o ventana) dentro de `area`.
    fn shot(&self, b: &mut B, i: usize, s: &super::layout::Shot, area: R) -> R {
        let t = self.t;
        let max = self.z(1.5, 1.125);
        let style = t.frame_style.as_str();
        let frame_col = t.frame_hex(&self.o.accent);
        let none = t.frame == "none";
        if style == "window" {
            let bar = self.z(20.0, 14.0);
            let r = fit(s.w, s.h, R::new(area.x, area.y + bar, area.w, area.h - bar), max);
            let chrome = if none { mix(&t.page, "000000", 0.12) } else { frame_col.clone() };
            let top = R::new(r.x, r.y - bar, r.w, bar);
            b.fill_r(R::new(r.x + 2.0, r.y - bar + 3.0, r.w, r.h + bar), &mix(&t.page, "000000", 0.16), 4.0);
            b.fill_r(top, &chrome, 4.0);
            b.fill(R::new(r.x, r.y - bar * 0.4, r.w, bar * 0.4), &chrome);
            let d = bar * 0.36;
            for (k, c) in ["F87171", "FBBF24", "34D399"].iter().enumerate() {
                b.dot(R::new(r.x + bar * 0.45 + k as f32 * d * 1.6, r.y - bar / 2.0 - d / 2.0, d, d), c);
            }
            let addr = R::new(r.x + bar * 2.6, r.y - bar * 0.72, (r.w * 0.45).min(r.w - bar * 3.0), bar * 0.44);
            b.fill_r(addr, &mix(&chrome, "FFFFFF", 0.55), bar * 0.22);
            b.img(r, Src::Cap(i));
            b.line(R::new(r.x, r.y - bar, r.w, r.h + bar), &chrome, 0.8, 4.0);
            return r;
        }
        let r = fit(s.w, s.h, area, max);
        if style == "shadow" {
            let off = self.z(5.0, 3.5);
            b.fill_r(R::new(r.x + off * 0.4, r.y + off, r.w, r.h), &mix(&t.page, "000000", 0.10), 3.0);
            b.fill_r(R::new(r.x + off * 0.2, r.y + off * 0.5, r.w, r.h), &mix(&t.page, "000000", 0.16), 2.0);
        }
        b.img(r, Src::Cap(i));
        if !none {
            b.line(r, &frame_col, if t.frame == "accent" { self.z(2.0, 1.4) } else { self.z(1.0, 0.75) }, 0.0);
        }
        r
    }
}

fn kit<'a>(o: &'a Opts, slides: bool, w: f32, h: f32, logo: Option<f32>, n: usize) -> Kit<'a> {
    let t = &o.t;
    let brand = t.cover_fill_hex(&o.accent);
    let on_brand = if dark(&brand) { "FFFFFF".to_string() } else { "111827".to_string() };
    Kit { o, t, w, h, m: if slides { 38.0 } else { 42.0 }, slides, brand, on_brand, logo, n }
}

/// PowerPoint y PDF: páginas fijas. `logo`: proporción ancho/alto del logo, si la plantilla tiene.
pub fn fixed(shots: &[Shot], o: &Opts, format: Format, logo: Option<f32>) -> Doc {
    let (w, h) = page_size(format, o.landscape);
    let k = kit(o, format == Format::Pptx, w, h, logo, shots.len());
    let mut pages = Vec::new();
    if o.cover {
        pages.push(cover(&k, shots));
    }
    for (i, s) in shots.iter().enumerate() {
        pages.push(step(&k, i, s));
    }
    Doc { w, h, page: o.t.page.clone(), pages }
}

fn cover(k: &Kit, shots: &[Shot]) -> Vec<El> {
    let (o, t, w, h, m) = (k.o, k.t, k.w, k.h, k.m);
    let mut b = B::new();
    let ts = k.z(40.0, 28.0);
    let ss = k.z(16.0, 12.5);
    let is = k.z(13.0, 10.5);
    let th = ts * 1.22 * 2.0;
    let sub = subtitle(k.n);
    let intro = o.intro.trim();
    let accent = o.accent.as_str();
    let (mut foot_ink, mut org_ink) = (t.faint.clone(), t.muted.clone());
    let mut logo_at: (f32, f32, bool, bool) = (m, m * 0.75, false, false); // x, y, placa, centrado
    match t.cover.as_str() {
        "band" => {
            let bh = h * 0.46;
            b.fill(R::new(0.0, 0.0, w, bh), &k.brand);
            let tx = m + k.z(18.0, 14.0);
            b.text(R::new(tx, bh - m - ss * 1.6 - th, w - tx - m, th), &o.title, ts, true, &t.cover_ink, Al::L, Va::B, 2);
            b.text(R::new(tx, bh - m - ss * 1.5, w - tx - m, ss * 1.5), &sub, ss, false, &t.cover_ink, Al::L, Va::T, 1);
            b.text(R::new(tx, bh + m * 0.9, (w - tx - m).min(w * 0.7), h - bh - m * 2.6), intro, is, false, &t.muted, Al::L, Va::T, 12);
            logo_at.2 = true;
        }
        "full" => {
            b.fill(R::new(0.0, 0.0, w, h), &k.brand);
            let tx = m + k.z(18.0, 14.0);
            let y = h * 0.36;
            b.fill(R::new(tx, y - 16.0, k.z(64.0, 48.0), 3.0), if k.brand.eq_ignore_ascii_case(accent) { &t.cover_ink } else { accent });
            b.text(R::new(tx, y, w - tx - m, th), &o.title, ts, true, &t.cover_ink, Al::L, Va::T, 2);
            b.text(R::new(tx, y + th + 6.0, w - tx - m, ss * 1.5), &sub, ss, false, &t.cover_ink, Al::L, Va::T, 1);
            b.text(R::new(tx, y + th + ss * 1.5 + 26.0, (w - tx - m).min(w * 0.68), h * 0.3), intro, is, false, &t.cover_ink, Al::L, Va::T, 10);
            foot_ink = t.cover_ink.clone();
            org_ink = t.cover_ink.clone();
            logo_at.2 = true;
        }
        "minimal" => {
            let y = h * 0.30;
            b.text(R::new(m * 2.0, y, w - 4.0 * m, th), &o.title, ts, true, &t.ink, Al::C, Va::B, 2);
            b.fill(R::new(w / 2.0 - 30.0, y + th + 12.0, 60.0, 2.0), accent);
            b.text(R::new(m, y + th + 24.0, w - 2.0 * m, ss * 1.5), &sub, ss, false, &t.muted, Al::C, Va::T, 1);
            b.text(R::new(w * 0.16, y + th + ss * 1.5 + 44.0, w * 0.68, h * 0.3), intro, is, false, &t.muted, Al::C, Va::T, 10);
            logo_at = (w / 2.0, m * 0.9, false, true);
        }
        "hero" => {
            if w > h {
                let split = w * 0.45;
                b.fill(R::new(split, 0.0, w - split, h), &k.brand);
                if let Some(s) = shots.first() {
                    let pad = k.z(34.0, 24.0);
                    let r = fit(s.w, s.h, R::new(split + pad, pad, w - split - 2.0 * pad, h - 2.0 * pad), 3.0);
                    b.fill(R::new(r.x + 4.0, r.y + 6.0, r.w, r.h), &mix(&k.brand, "000000", 0.25));
                    b.img(r, Src::Cap(0));
                    b.line(r, "FFFFFF", 1.5, 0.0);
                }
                b.fill(R::new(split, 0.0, k.z(7.0, 5.0), h), accent);
                let cw = split - 2.0 * m - 10.0;
                let y = h * 0.30;
                b.fill(R::new(m, y - 14.0, k.z(56.0, 44.0), 3.0), accent);
                let tl = est_lines(&o.title, ts * 0.9, cw).clamp(1, 3) as f32;
                b.text(R::new(m, y, cw, ts * 1.12 * tl + 4.0), &o.title, ts * 0.9, true, &t.ink, Al::L, Va::T, 3);
                let sy = y + ts * 1.12 * tl + 10.0;
                b.text(R::new(m, sy, cw, ss * 1.5), &sub, ss, false, &t.muted, Al::L, Va::T, 1);
                b.text(R::new(m, sy + ss * 1.5 + 18.0, cw, h - sy - m * 2.6), intro, is, false, &t.muted, Al::L, Va::T, 12);
            } else {
                let top = h * 0.5;
                b.fill(R::new(0.0, top, w, h - top), &k.brand);
                if let Some(s) = shots.first() {
                    let pad = 28.0;
                    let r = fit(s.w, s.h, R::new(pad, top + pad, w - 2.0 * pad, h - top - 2.0 * pad - 20.0), 3.0);
                    b.fill(R::new(r.x + 4.0, r.y + 6.0, r.w, r.h), &mix(&k.brand, "000000", 0.25));
                    b.img(r, Src::Cap(0));
                    b.line(r, "FFFFFF", 1.5, 0.0);
                }
                b.fill(R::new(0.0, top, w, 5.0), accent);
                let y = h * 0.17;
                b.fill(R::new(m, y - 14.0, 44.0, 3.0), accent);
                b.text(R::new(m, y, w - 2.0 * m, th), &o.title, ts, true, &t.ink, Al::L, Va::T, 2);
                b.text(R::new(m, y + th + 6.0, w - 2.0 * m, ss * 1.5), &sub, ss, false, &t.muted, Al::L, Va::T, 1);
                b.text(R::new(m, y + th + ss * 1.5 + 20.0, w - 2.0 * m, top - (y + th + ss * 1.5 + 20.0) - 16.0), intro, is, false, &t.muted, Al::L, Va::T, 6);
                foot_ink = t.cover_ink.clone();
                org_ink = t.cover_ink.clone();
            }
        }
        "split" => {
            // Índice: panel de color con el título y, al lado, la lista de pasos.
            let land = w > h;
            let panel = if land { R::new(0.0, 0.0, w * 0.42, h) } else { R::new(0.0, 0.0, w, h * 0.40) };
            b.fill(panel, &k.brand);
            let cw = panel.w - 2.0 * m;
            let y = if land { h * 0.34 } else { panel.h * 0.36 };
            b.fill(R::new(m, y - 14.0, k.z(50.0, 40.0), 3.0), if k.brand.eq_ignore_ascii_case(accent) { &t.cover_ink } else { accent });
            let tl = est_lines(&o.title, ts * 0.9, cw).clamp(1, 3) as f32;
            b.text(R::new(m, y, cw, ts * 1.12 * tl + 4.0), &o.title, ts * 0.9, true, &t.cover_ink, Al::L, Va::T, 3);
            let sy = y + ts * 1.12 * tl + 10.0;
            b.text(R::new(m, sy, cw, ss * 1.5), &sub, ss, false, &t.cover_ink, Al::L, Va::T, 1);
            let lx = if land { panel.right() + m } else { m };
            let mut ly = if land { h * 0.2 } else { panel.bottom() + m * 0.9 };
            let lw = if land { w - lx - m } else { w - 2.0 * m };
            let hs = k.z(12.0, 9.5);
            b.text(R::new(lx, ly, lw, hs * 1.6), &crate::i18n::t("CONTENIDO"), hs, true, accent, Al::L, Va::T, 1);
            ly += hs * 2.4;
            let es = k.z(15.0, 11.0);
            let room = (if land { h - m * 1.6 } else { h - m * 2.2 } - ly).max(0.0);
            let rows = ((room / (es * 2.2)) as usize).max(1);
            for (i, s) in shots.iter().enumerate().take(rows) {
                let nr = R::new(lx, ly, es * 2.0, es * 1.6);
                b.text(nr, &format!("{:02}", i + 1), es, true, accent, Al::L, Va::M, 1);
                let title = if s.title.trim().is_empty() { step_label(i) } else { s.title.clone() };
                b.text(R::new(lx + es * 2.4, ly, lw - es * 2.4, es * 1.6), &title, es, false, &t.ink, Al::L, Va::M, 1);
                b.fill(R::new(lx, ly + es * 1.9, lw, 0.6), &t.rule);
                ly += es * 2.2;
            }
            if !intro.is_empty() && land {
                b.text(R::new(m, sy + ss * 1.5 + 18.0, cw, h - sy - m * 3.0), intro, is, false, &t.cover_ink, Al::L, Va::T, 8);
            }
            logo_at.2 = true;
            if land {
                foot_ink = t.cover_ink.clone();
            }
        }
        "frame" => {
            let f1 = R::new(m * 0.5, m * 0.5, w - m, h - m);
            b.line(f1, accent, k.z(3.0, 2.5), 0.0);
            b.line(f1.inset(6.0), &t.rule, 0.6, 0.0);
            let y = h * 0.20;
            b.text(R::new(m * 1.5, y, w - 3.0 * m, th), &o.title, ts, true, &t.ink, Al::C, Va::B, 2);
            b.fill(R::new(w / 2.0 - 26.0, y + th + 10.0, 52.0, 2.0), accent);
            b.text(R::new(m, y + th + 20.0, w - 2.0 * m, ss * 1.5), &sub, ss, false, &t.muted, Al::C, Va::T, 1);
            // Tres capturas de muestra en fila.
            let row_y = y + th + ss * 1.5 + k.z(40.0, 36.0);
            let row_h = (h * if w > h { 0.32 } else { 0.22 }).min(h - row_y - m * 3.2);
            let count = shots.len().min(3);
            if count > 0 && row_h > 30.0 {
                let gap = k.z(18.0, 12.0);
                let total_w = w - 4.0 * m;
                let cw = (total_w - gap * (count as f32 - 1.0)) / count as f32;
                for (i, s) in shots.iter().take(count).enumerate() {
                    let cell = R::new(2.0 * m + i as f32 * (cw + gap), row_y, cw, row_h);
                    let r = fit(s.w, s.h, cell, 3.0);
                    b.fill(R::new(r.x + 2.0, r.y + 3.0, r.w, r.h), &mix(&t.page, "000000", 0.12));
                    b.img(r, Src::Cap(i));
                    b.line(r, &t.rule, 0.6, 0.0);
                }
                if !intro.is_empty() {
                    b.text(R::new(w * 0.15, row_y + row_h + 18.0, w * 0.7, h - row_y - row_h - m * 3.0), intro, is, false, &t.muted, Al::C, Va::T, 4);
                }
            } else {
                b.text(R::new(w * 0.15, row_y, w * 0.7, h * 0.3), intro, is, false, &t.muted, Al::C, Va::T, 8);
            }
            logo_at = (w / 2.0, m * 1.1, false, true);
        }
        _ => {
            // "bar": barra vertical del acento al lado del título.
            let tx = m + k.z(20.0, 16.0);
            let y = h * 0.34;
            b.fill(R::new(m, y + 4.0, k.z(6.0, 4.5), th + ss * 1.5), accent);
            b.text(R::new(tx, y, w - tx - m, th), &o.title, ts, true, &t.ink, Al::L, Va::B, 2);
            b.text(R::new(tx, y + th + 4.0, w - tx - m, ss * 1.5), &sub, ss, false, &t.muted, Al::L, Va::T, 1);
            b.text(R::new(tx, y + th + ss * 1.5 + 26.0, (w - tx - m).min(w * 0.66), h * 0.3), intro, is, false, &t.muted, Al::L, Va::T, 10);
        }
    }
    let fs = k.z(10.0, 8.0);
    b.text(R::new(m, h - m * 0.65 - fs * 1.4, w / 2.0 - m, fs * 1.4), &t.footer, fs, false, &foot_ink, Al::L, Va::B, 1);
    b.text(R::new(w / 2.0, h - m * 0.65 - fs * 1.6, w / 2.0 - m, fs * 1.6), &t.org, fs * 1.15, true, &org_ink, Al::R, Va::B, 1);
    let lh = k.z(30.0, 24.0);
    if logo_at.3 {
        let lw = k.logo.map(|a| (lh * a).min(lh * 5.0)).unwrap_or(0.0);
        k.logo(&mut b, logo_at.0 - lw / 2.0, logo_at.1, lh, logo_at.2, false);
    } else {
        k.logo(&mut b, logo_at.0, logo_at.1, lh, logo_at.2, false);
    }
    b.els
}

fn step(k: &Kit, i: usize, s: &Shot) -> Vec<El> {
    let (o, t, w, h, m) = (k.o, k.t, k.w, k.h, k.m);
    let mut b = B::new();
    let title = if o.captions { s.title.trim().to_string() } else { String::new() };
    let note = s.note.trim().to_string();
    let accent = o.accent.as_str();
    let ts = k.z(22.0, 15.0);
    let ns = k.z(14.0, 10.5);
    let fs = k.z(10.0, 8.0);
    let layout = t.layout.as_str();
    let band = t.header == "band";
    let head = o.numbers || !title.is_empty();
    let num = format!("{} / {}", i + 1, k.n);
    let sidebar = layout == "sidebar";
    let col_w = if sidebar { w * k.z(0.28, 0.30) } else { 0.0 };

    // Columna lateral de color (diseño "sidebar"): número grande, título y nota.
    if sidebar {
        b.fill(R::new(0.0, 0.0, col_w, h), &k.brand);
        let cx = m * 0.8;
        let cw = col_w - cx - m * 0.6;
        let mut y = h * 0.16;
        if o.numbers {
            let big = k.z(64.0, 44.0);
            b.text(R::new(cx, y, cw, big * 1.15), &format!("{:02}", i + 1), big, true, &k.on_brand, Al::L, Va::T, 1);
            y += big * 1.2;
            b.fill(R::new(cx, y, k.z(40.0, 30.0), 3.0), if k.brand.eq_ignore_ascii_case(accent) { &k.on_brand } else { accent });
            y += 14.0;
        }
        let tl = est_lines(&title, ts * 0.92, cw).clamp(1, 5) as f32;
        b.text(R::new(cx, y, cw, ts * 1.25 * tl), &title, ts * 0.92, true, &k.on_brand, Al::L, Va::T, 5);
        y += ts * 1.25 * tl + 12.0;
        b.text(R::new(cx, y, cw, h - y - m * 1.6), &note, ns * 0.92, false, &mix(&k.on_brand, &k.brand, 0.18), Al::L, Va::T, 14);
        k.logo(&mut b, cx, h - m * 0.8 - k.z(22.0, 16.0), k.z(22.0, 16.0), true, false);
    }

    // Pie: barra de color o línea de texto, con el número de página.
    let x0 = col_w + if sidebar { m * 0.8 } else { m };
    let mut bottom;
    if t.footer_style == "bar" {
        let bh = k.z(28.0, 22.0);
        b.fill(R::new(col_w, h - bh, w - col_w, bh), &k.brand);
        let left = [t.footer.as_str(), t.org.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join("   ·   ");
        b.text(R::new(x0, h - bh, w - x0 - m - 70.0, bh), &left, fs, true, &k.on_brand, Al::L, Va::M, 1);
        b.text(R::new(w - m - 70.0, h - bh, 70.0, bh), &num, fs, true, &k.on_brand, Al::R, Va::M, 1);
        bottom = h - bh - k.z(10.0, 8.0);
        if o.meta {
            b.text(R::new(x0, bottom - fs * 1.5, w - x0 - m, fs * 1.5), &s.meta, fs * 0.95, false, &t.faint, Al::L, Va::B, 1);
            bottom -= fs * 1.5 + 6.0;
        }
    } else {
        let fh = fs * 1.5;
        let fy = h - m * 0.55 - fh;
        if o.meta {
            b.text(R::new(x0, fy, w - x0 - m - 70.0, fh), &s.meta, fs, false, &t.faint, Al::L, Va::M, 1);
        }
        b.text(R::new(w - m - 70.0, fy, 70.0, fh), &num, fs, false, &t.faint, Al::R, Va::M, 1);
        bottom = fy - k.z(10.0, 8.0);
    }

    let note_lines = |width: f32| est_lines(&note, ns, width).min(5);
    match layout {
        "full" => {
            // La captura grande sobre un panel de color, con el título en una franja abajo.
            let nl = note_lines(w - 2.0 * m);
            let note_h = if nl == 0 { 0.0 } else { nl as f32 * ns * 1.35 + 10.0 };
            let panel = R::new(0.0, 0.0, w, bottom - note_h - if nl == 0 { 0.0 } else { 6.0 });
            b.fill(panel, &k.brand);
            let cap_h = if head { k.z(54.0, 40.0) } else { 0.0 };
            let pad = k.z(30.0, 22.0);
            k.shot(&mut b, i, s, R::new(pad, pad, w - 2.0 * pad, panel.h - cap_h - pad * 1.4));
            if head {
                let strip = R::new(0.0, panel.bottom() - cap_h, w, cap_h);
                b.fill(strip, &mix(&k.brand, "000000", 0.35));
                let x = k.label(&mut b, m, strip.y, cap_h, i, true, ts * 0.85);
                b.text(R::new(x, strip.y, w - x - m - 40.0, cap_h), &title, ts * 0.85, true, "FFFFFF", Al::L, Va::M, 1);
            }
            k.logo(&mut b, w - m, m * 0.6, k.z(22.0, 16.0), true, true);
            b.text(R::new(m, panel.bottom() + 8.0, w - 2.0 * m, note_h), &note, ns, false, &t.ink, Al::L, Va::T, 5);
        }
        "card" => {
            // Tarjeta blanca con sombra sobre el fondo de la página.
            let card = R::new(m, m * 0.75, w - 2.0 * m, bottom - m * 0.75);
            let rad = k.z(14.0, 10.0);
            b.fill_r(R::new(card.x + 2.0, card.y + 5.0, card.w, card.h), &mix(&t.page, "000000", 0.10), rad);
            b.fill_r(card, "FFFFFF", rad);
            if t.top_rule {
                b.fill_r(R::new(card.x, card.y, card.w, 5.0), accent, 2.0);
            }
            let p = k.z(24.0, 18.0);
            let mut top = card.y + p;
            if head {
                let hh = k.z(34.0, 24.0);
                let x = k.label(&mut b, card.x + p, top, hh, i, false, ts);
                let lr = k.logo.map(|_| k.z(110.0, 80.0)).unwrap_or(0.0);
                b.text(R::new(x, top, card.right() - p - x - lr, hh), &title, ts, true, &t.ink, Al::L, Va::M, 1);
                k.logo(&mut b, card.right() - p, top + (hh - k.z(20.0, 14.0)) / 2.0, k.z(20.0, 14.0), false, true);
                top += hh + k.z(14.0, 10.0);
            }
            let nl = note_lines(card.w - 2.0 * p);
            let note_h = if nl == 0 { 0.0 } else { nl as f32 * ns * 1.35 + 12.0 };
            k.shot(&mut b, i, s, R::new(card.x + p, top, card.w - 2.0 * p, card.bottom() - p - top - note_h));
            b.text(R::new(card.x + p, card.bottom() - p - note_h + 10.0, card.w - 2.0 * p, note_h), &note, ns, false, &t.ink, Al::L, Va::T, 5);
        }
        "sidebar" => {
            let area = R::new(col_w + m * 0.8, m * 0.8, w - col_w - m * 1.6, bottom - m * 0.8);
            if t.top_rule {
                b.fill(R::new(col_w, 0.0, w - col_w, k.z(5.0, 4.0)), accent);
            }
            k.shot(&mut b, i, s, area);
        }
        _ => {
            // Encabezado arriba (banda, línea o simple) y luego "stack", "split" o "split-right".
            let split = layout == "split" || layout == "split-right";
            let mut top = m * 0.8;
            if band {
                let bh = k.z(64.0, 46.0);
                b.fill(R::new(0.0, 0.0, w, bh), &k.brand);
                let lw = k.logo.map(|_| k.z(120.0, 90.0)).unwrap_or(0.0);
                let x = k.label(&mut b, m, 0.0, bh, i, true, ts);
                b.text(R::new(x, 0.0, w - x - m - lw, bh), &title, ts, true, &k.on_brand, Al::L, Va::M, 1);
                k.logo(&mut b, w - m, (bh - k.z(24.0, 18.0)) / 2.0, k.z(24.0, 18.0), true, true);
                top = bh + k.z(20.0, 16.0);
            } else {
                if t.top_rule {
                    b.fill(R::new(0.0, 0.0, w, k.z(5.0, 4.0)), accent);
                }
                k.logo(&mut b, w - m, m * 0.7, k.z(22.0, 16.0), false, true);
                if head && !split {
                    let plain = t.header == "plain";
                    let hs = if plain { ts * 1.18 } else { ts };
                    let hh = hs * 1.6;
                    let lw = k.logo.map(|_| k.z(120.0, 90.0)).unwrap_or(0.0);
                    let x = k.label(&mut b, m, top, hh, i, false, hs);
                    b.text(R::new(x, top, w - x - m - lw, hh), &title, hs, true, &t.ink, Al::L, Va::M, 1);
                    if plain {
                        b.fill(R::new(m, top + hh + 4.0, k.z(48.0, 36.0), 3.0), accent);
                    } else {
                        b.fill(R::new(m, top + hh + 6.0, w - 2.0 * m, 0.75), &t.rule);
                    }
                    top += hh + k.z(22.0, 16.0);
                }
            }
            if split {
                let land = w > h;
                let cw = w * if land { 0.30 } else { 0.36 };
                let right = layout == "split-right";
                let tx = if right { w - m - cw + 10.0 } else { m };
                let tw = cw - 10.0;
                let mut y = top + 4.0;
                if !band && head {
                    if o.numbers {
                        let hh = ts * 1.4;
                        k.label(&mut b, tx, y, hh, i, false, ts * 0.8);
                        y += hh + 6.0;
                    }
                    let tl = est_lines(&title, ts, tw).clamp(1, 4) as f32;
                    b.text(R::new(tx, y, tw, ts * 1.25 * tl), &title, ts, true, &t.ink, Al::L, Va::T, 4);
                    y += ts * 1.25 * tl + 8.0;
                    b.fill(R::new(tx, y, k.z(40.0, 30.0), 3.0), accent);
                    y += 16.0;
                }
                b.text(R::new(tx, y, tw, bottom - y), &note, ns, false, &t.ink, Al::L, Va::T, 20);
                let ax = if right { m } else { m + cw };
                k.shot(&mut b, i, s, R::new(ax, top, w - m - cw - m + if right { -6.0 } else { 0.0 }, bottom - top));
            } else {
                let nl = note_lines(w - 2.0 * m);
                let note_h = if nl == 0 { 0.0 } else { nl as f32 * ns * 1.35 + 12.0 };
                let area = R::new(m, top, w - 2.0 * m, bottom - top - note_h);
                let r = k.shot(&mut b, i, s, area);
                b.text(R::new(m, r.bottom().max(area.bottom() - 4.0) + 10.0, w - 2.0 * m, note_h), &note, ns, false, &t.ink, Al::L, Va::T, 5);
            }
        }
    }
    b.els
}

// ------------------------------------------------------------------ Word (flujo simulado)

/// Medidas de Word en puntos (las mismas que usa el escritor de .docx).
pub const DOCX_MARGIN: f32 = 56.7;

/// Simula cómo Word acomoda el documento: portada en párrafos, y por paso encabezado, imagen, epígrafe y
/// nota (o tabla en "split"/"card"), con saltos de página cuando no entra o si se pidió una por página.
pub fn flow_docx(shots: &[Shot], o: &Opts, logo: Option<f32>) -> Doc {
    let t = &o.t;
    let (w, h) = page_size(Format::Docx, o.landscape);
    let m = DOCX_MARGIN;
    let tw = w - 2.0 * m;
    let foot_y = h - 28.35 - 12.0;
    let limit = h - m;
    let accent = o.accent.as_str();
    let fill = t.cover_fill_hex(accent);
    let mut pages: Vec<Vec<El>> = vec![Vec::new()];
    let mut y = m;
    let mut b = B::new();
    let new_page = |pages: &mut Vec<Vec<El>>, b: &mut B, y: &mut f32| {
        pages.last_mut().unwrap().append(&mut b.els);
        pages.push(Vec::new());
        *y = m;
    };
    let max_img_h = h - 2.0 * m - 130.0;

    if o.cover {
        if let Some(a) = logo {
            let lh = 45.0;
            let lw = (lh * a).min(158.0);
            let x = if t.cover == "minimal" { (w - lw) / 2.0 } else { m };
            b.img(R::new(x, y, lw, lh.min(lw / a.max(0.01))), Src::Logo);
            y += lh + 24.0;
        }
        let ts = 26.0;
        let tl = est_lines(&o.title, ts, tw - 14.0).clamp(1, 3) as f32;
        let th = ts * 1.2 * tl;
        match t.cover.as_str() {
            "band" | "full" => {
                if t.cover == "full" {
                    y += 120.0;
                }
                b.fill(R::new(m, y, tw, th + 12.0 + 12.5 * 1.4 + 6.0), &fill);
                b.text(R::new(m + 4.0, y + 4.0, tw - 8.0, th), &o.title, ts, true, &t.cover_ink, Al::L, Va::T, 3);
                y += th + 8.0;
                b.text(R::new(m + 4.0, y, tw - 8.0, 12.5 * 1.4), &subtitle(shots.len()), 12.0, false, &t.cover_ink, Al::L, Va::T, 1);
                y += 12.5 * 1.4 + 24.0 + 10.0;
            }
            "minimal" => {
                y += 90.0;
                b.text(R::new(m, y, tw, th), &o.title, ts, true, &t.ink, Al::C, Va::T, 3);
                y += th + 6.0;
                b.fill(R::new(m, y, tw, 1.0), accent);
                y += 14.0;
                b.text(R::new(m, y, tw, 16.0), &subtitle(shots.len()), 12.0, false, &t.muted, Al::C, Va::T, 1);
                y += 16.0 + 24.0;
            }
            _ => {
                b.fill(R::new(m, y - 2.0, 4.5, th + 4.0), accent);
                b.text(R::new(m + 16.5, y, tw - 16.5, th), &o.title, ts, true, &t.ink, Al::L, Va::T, 3);
                y += th + 3.0;
                b.text(R::new(m + 12.0, y, tw - 12.0, 16.0), &subtitle(shots.len()), 12.0, false, &t.muted, Al::L, Va::T, 1);
                y += 16.0 + 24.0;
            }
        }
        if t.cover == "hero" {
            if let Some(s) = shots.first() {
                let r = fit(s.w, s.h, R::new(m, y, tw, max_img_h / 2.0), 0.75);
                let r = R::new(m, y, r.w, r.h);
                b.img(r, Src::Cap(0));
                b.line(r, accent, 2.25, 0.0);
                y += r.h + 12.0;
            }
        }
        let il = est_lines(o.intro.trim(), 10.5, tw).min(30) as f32;
        if il > 0.0 {
            b.text(R::new(m, y, tw, il * 10.5 * 1.38), o.intro.trim(), 10.5, false, &t.ink, Al::L, Va::T, 30);
            y += il * 10.5 * 1.38 + 6.0;
        }
        if !t.org.is_empty() {
            y += 24.0;
            b.text(R::new(m, y, tw, 14.0), &t.org, 10.5, true, &t.muted, Al::R, Va::T, 1);
            y += 20.0;
        }
    }

    for (i, s) in shots.iter().enumerate() {
        let title = if o.captions { s.title.trim().to_string() } else { String::new() };
        let head = o.numbers || !title.is_empty();
        let hs: f32 = if t.header == "plain" { 17.0 } else { 15.0 };
        let head_h: f32 = if head { hs * 1.35 + 18.0 + if t.header == "band" { 6.0 } else { 0.0 } } else { 0.0 };
        let layout = t.layout.as_str();
        let split = layout == "split" || layout == "split-right";
        let card = layout == "card";
        let img_w = if split { tw * 0.62 } else if card { tw - 24.0 } else { tw };
        let img = fit(s.w, s.h, R::new(0.0, 0.0, img_w, if split { max_img_h * 0.7 } else { max_img_h }), 0.75);
        let nl = est_lines(&s.note, 10.5, if split { tw * 0.34 } else { tw }) as f32;
        let note_h = nl * 10.5 * 1.38;
        let block_h = if split {
            head_h.max(0.0) + img.h.max(note_h + 40.0) + 10.0
        } else {
            head_h + img.h + if o.meta { 16.0 } else { 10.0 } + note_h + if card { 28.0 } else { 0.0 }
        };
        let break_before = o.one_per_page && (i > 0 || o.cover);
        if break_before || (y + block_h > limit && y > m + 1.0) {
            new_page(&mut pages, &mut b, &mut y);
        }
        let x0 = m;
        if card {
            b.fill(R::new(x0, y, tw, block_h - 6.0), &mix(&t.page, accent, 0.05));
            b.line(R::new(x0, y, tw, block_h - 6.0), &t.rule, 0.75, 0.0);
            y += 12.0;
        }
        let ix = if card { x0 + 12.0 } else { x0 };
        let iw = if card { tw - 24.0 } else { tw };
        // Encabezado del paso (estilo Título 2).
        let heading = |b: &mut B, x: f32, y: f32, width: f32| {
            let band = t.header == "band";
            if band {
                b.fill(R::new(x, y + 12.0, width, hs * 1.35 + 6.0), &fill);
            } else if t.top_rule {
                b.fill(R::new(x, y + 6.0, width, 2.25), accent);
            }
            let ink = if band { "FFFFFF".to_string() } else { t.ink.clone() };
            let step_ink = if band { "FFFFFF" } else { accent };
            let mut lx = x + if band { 4.0 } else { 0.0 };
            let ly = y + 12.0 + if band { 3.0 } else { 0.0 };
            if o.numbers {
                let label = match t.steps.as_str() {
                    "number" => format!("{:02}", i + 1),
                    "circle" => format!(" {} ", i + 1),
                    _ => step_label(i),
                };
                let size = if t.steps == "number" { 24.0 } else { hs };
                let lw = est(&label, size) + if t.steps == "pill" || t.steps == "circle" { 8.0 } else { 0.0 };
                if t.steps == "pill" || t.steps == "circle" {
                    b.fill(R::new(lx, ly, lw, hs * 1.3), if band { "FFFFFF" } else { accent });
                    b.text(R::new(lx, ly, lw, hs * 1.3), &label, hs, true, if band { &fill } else { "FFFFFF" }, Al::C, Va::M, 1);
                } else {
                    b.text(R::new(lx, ly - if t.steps == "number" { 4.0 } else { 0.0 }, lw + 4.0, size * 1.3), &label, size, t.steps == "number", step_ink, Al::L, Va::T, 1);
                }
                lx += lw + 8.0;
            }
            b.text(R::new(lx, ly, x + width - lx, hs * 1.3), &title, hs, false, &ink, Al::L, Va::T, 1);
        };
        if split {
            let cw = tw * 0.34;
            let (tx, ax) = if layout == "split-right" { (x0 + tw - cw, x0) } else { (x0, x0 + tw - img.w) };
            let mut ty = y;
            if head {
                heading(&mut b, tx, ty - 6.0, cw);
                ty += head_h - 6.0;
            }
            b.text(R::new(tx, ty, cw, note_h + 4.0), s.note.trim(), 10.5, false, &t.ink, Al::L, Va::T, 40);
            let r = R::new(ax, y + 6.0, img.w, img.h);
            b.img(r, Src::Cap(i));
            if t.frame != "none" {
                b.line(r, &t.frame_hex(accent), 0.5, 0.0);
            }
            y += block_h;
            continue;
        }
        if head {
            heading(&mut b, ix, y, iw);
            y += head_h;
        }
        let r = R::new(ix + (iw - img.w) / 2.0, y, img.w, img.h);
        if t.frame_style == "shadow" {
            b.fill(R::new(r.x + 2.5, r.y + 3.0, r.w, r.h), &mix(&t.page, "000000", 0.14));
        }
        b.img(r, Src::Cap(i));
        if t.frame != "none" {
            b.line(r, &t.frame_hex(accent), 0.5, 0.0);
        }
        y += img.h + 3.0;
        if o.meta {
            b.text(R::new(ix, y, iw, 11.0), &s.meta, 8.5, false, &t.faint, Al::C, Va::T, 1);
            y += 13.0;
        } else {
            y += 8.0;
        }
        if nl > 0.0 {
            b.text(R::new(ix, y, iw, note_h), s.note.trim(), 10.5, false, &t.ink, Al::L, Va::T, 60);
            y += note_h + 6.0;
        }
        if card {
            y += 22.0;
        }
    }
    pages.last_mut().unwrap().append(&mut b.els);
    // Pie en todas las páginas.
    let foot = [t.footer.as_str(), t.org.as_str(), o.title.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join("  ·  ");
    let n = pages.len();
    for (k, p) in pages.iter_mut().enumerate() {
        let mut fb = B::new();
        let bar = t.footer_style == "bar";
        if bar {
            fb.fill(R::new(m, foot_y - 2.0, tw, 13.0), &fill);
        }
        let ink = if bar { "FFFFFF" } else { t.faint.as_str() };
        fb.text(R::new(m + 2.0, foot_y, tw - 60.0, 11.0), &foot, 8.0, false, ink, Al::L, Va::T, 1);
        fb.text(R::new(w - m - 60.0, foot_y, 58.0, 11.0), &format!("{} / {n}", k + 1), 8.0, false, ink, Al::R, Va::T, 1);
        p.append(&mut fb.els);
    }
    Doc { w, h, page: t.page.clone(), pages }
}

// ------------------------------------------------------------------ una sola imagen

/// "Una imagen": una hoja alta con la portada y los pasos uno debajo del otro. `width` en píxeles.
pub fn tall(shots: &[Shot], o: &Opts, logo: Option<f32>) -> Doc {
    let t = &o.t;
    let accent = o.accent.as_str();
    let content = shots.iter().map(|s| s.w).max().unwrap_or(640).clamp(640, 1600) as f32;
    let pad = 36.0;
    let w = content + 2.0 * pad;
    let k = kit(o, true, w, 10_000.0, logo, shots.len());
    let mut b = B::new();
    let mut y = 0.0;
    if o.cover {
        let ch = 190.0;
        let filled = matches!(t.cover.as_str(), "band" | "full" | "hero" | "split");
        let (ink, sub_ink) = if filled { (t.cover_ink.clone(), t.cover_ink.clone()) } else { (t.ink.clone(), t.muted.clone()) };
        if filled {
            b.fill(R::new(0.0, 0.0, w, ch), &k.brand);
        } else if t.cover == "frame" {
            b.line(R::new(12.0, 12.0, w - 24.0, ch - 12.0), accent, 2.5, 0.0);
        }
        let centered = matches!(t.cover.as_str(), "minimal" | "frame");
        let al = if centered { Al::C } else { Al::L };
        let tx = if t.cover == "bar" || t.cover.is_empty() { pad + 20.0 } else { pad };
        if !filled && !centered {
            b.fill(R::new(pad, 56.0, 6.0, 74.0), accent);
        }
        b.text(R::new(tx, 50.0, w - tx - pad - 160.0, 46.0), &o.title, 34.0, true, &ink, al, Va::B, 1);
        if centered {
            b.fill(R::new(w / 2.0 - 30.0, 104.0, 60.0, 2.0), accent);
        }
        b.text(R::new(tx, 112.0, w - tx - pad, 24.0), &subtitle(shots.len()), 15.0, false, &sub_ink, al, Va::T, 1);
        b.text(R::new(tx, 140.0, w - tx - pad - 40.0, 40.0), o.intro.trim(), 13.0, false, &sub_ink, al, Va::T, 2);
        k.logo(&mut b, w - pad, 40.0, 40.0, filled, true);
        if filled {
            b.fill(R::new(0.0, ch, w, 5.0), accent);
        }
        y = ch + 30.0;
    } else {
        y += pad;
    }
    for (i, s) in shots.iter().enumerate() {
        let title = if o.captions { s.title.trim().to_string() } else { String::new() };
        if o.numbers || !title.is_empty() {
            let hh = 46.0;
            let band = t.header == "band";
            if band {
                b.fill(R::new(0.0, y, w, hh), &k.brand);
            }
            let x = k.label(&mut b, pad, y, hh, i, band, 18.0);
            b.text(R::new(x, y, w - x - pad, hh), &title, 18.0, true, if band { &k.on_brand } else { &t.ink }, Al::L, Va::M, 1);
            if !band {
                b.fill(R::new(pad, y + hh, content, 1.0), &t.rule);
            }
            y += hh + 16.0;
        }
        let ih = s.h as f32 * (content / s.w.max(1) as f32).min(1.0);
        let area = R::new(pad, y, content, ih);
        let r = k.shot(&mut b, i, s, if t.frame_style == "window" { R::new(pad, y, content, ih + 20.0) } else { area });
        y = r.bottom() + 10.0;
        if o.meta {
            b.text(R::new(pad, y, content, 18.0), &s.meta, 12.0, false, &t.faint, Al::L, Va::T, 1);
            y += 22.0;
        }
        let nl = est_lines(&s.note, 14.0, content) as f32;
        if nl > 0.0 {
            b.text(R::new(pad, y, content, nl * 14.0 * 1.4 + 4.0), s.note.trim(), 14.0, false, &t.ink, Al::L, Va::T, 200);
            y += nl * 14.0 * 1.4 + 10.0;
        }
        y += 34.0;
    }
    if t.footer_style == "bar" {
        b.fill(R::new(0.0, y, w, 30.0), &k.brand);
        let left = [t.footer.as_str(), t.org.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join("   ·   ");
        b.text(R::new(pad, y, w - 2.0 * pad, 30.0), &left, 11.0, true, &k.on_brand, Al::L, Va::M, 1);
        y += 30.0;
    } else {
        y += pad - 20.0;
    }
    Doc { w, h: y, page: t.page.clone(), pages: vec![b.els] }
}

/// El documento diseñado para cualquier formato (lo que dibuja la vista previa).
pub fn design(shots: &[Shot], o: &Opts, format: Format, logo: Option<f32>) -> Doc {
    match format {
        Format::Docx => flow_docx(shots, o, logo),
        Format::Png => tall(shots, o, logo),
        _ => fixed(shots, o, format, logo),
    }
}
