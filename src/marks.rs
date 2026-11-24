//! Anotaciones del editor. Mismo modelo y mismo JSON ("shapes" en los metadatos) que BuzzPrint .NET, y el
//! render final con GDI+ (el motor de System.Drawing), así una captura editada en una versión se ve igual en
//! la otra.

use std::ptr::null_mut;

use serde_json::{Map, Value};
use windows_sys::Win32::Graphics::GdiPlus::{
    GdipCreateAdjustableArrowCap, GdipCreateStringFormat, GdipDeleteCustomLineCap, GdipDeletePath, GdipDeleteStringFormat,
    GdipDrawEllipse, GdipDrawLine, GdipDrawLines, GdipDrawPath, GdipDrawRectangle, GdipDrawString, GdipFillEllipse, GdipFillPath,
    GdipSetPenCustomEndCap, GdipSetPenEndCap, GdipSetPenLineJoin, GdipSetPenStartCap, GdipSetStringFormatAlign,
    GdipSetStringFormatLineAlign, GpCustomLineCap, LineCapRound, LineJoinRound, PointF, StringAlignmentCenter,
};

use crate::gdip;
use crate::image::Rgba;
use crate::win::wide;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Arrow,
    Line,
    Rect,
    Ellipse,
    Highlight,
    Pen,
    Text,
    Step,
    Blur,
    Crop,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Arrow => "Arrow",
            Tool::Line => "Line",
            Tool::Rect => "Rect",
            Tool::Ellipse => "Ellipse",
            Tool::Highlight => "Highlight",
            Tool::Pen => "Pen",
            Tool::Text => "Text",
            Tool::Step => "Step",
            Tool::Blur => "Blur",
            Tool::Crop => "Crop",
        }
    }
    pub fn parse(s: &str) -> Option<Tool> {
        Some(match s {
            "Select" => Tool::Select,
            "Arrow" => Tool::Arrow,
            "Line" => Tool::Line,
            "Rect" => Tool::Rect,
            "Ellipse" => Tool::Ellipse,
            "Highlight" => Tool::Highlight,
            "Pen" => Tool::Pen,
            "Text" => Tool::Text,
            "Step" => Tool::Step,
            "Blur" => Tool::Blur,
            "Crop" => Tool::Crop,
            _ => return None,
        })
    }
}

/// Color ARGB como `System.Drawing.Color`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Argb(pub u32);

impl Argb {
    pub const RED: Argb = Argb(0xFFFF0000);
    pub fn a(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn b(self) -> u8 {
        self.0 as u8
    }
    pub fn with_alpha(self, a: u8) -> Argb {
        Argb((self.0 & 0x00FF_FFFF) | ((a as u32) << 24))
    }
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Argb {
        Argb(0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32)
    }
    pub fn to_egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.r(), self.g(), self.b(), self.a())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct P {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct Mark {
    pub kind: Tool,
    pub color: Argb,
    pub width: f32,
    pub pts: Vec<P>,
    pub text: String,
    pub font_size: f32,
    pub number: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub fn ltrb(l: f32, t: f32, r: f32, b: f32) -> RectF {
        RectF { x: l, y: t, w: r - l, h: b - t }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn inflate(&self, d: f32) -> RectF {
        RectF { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }
    pub fn contains(&self, p: P) -> bool {
        p.x >= self.x && p.y >= self.y && p.x <= self.right() && p.y <= self.bottom()
    }
}

impl Mark {
    pub fn new(kind: Tool, color: Argb, width: f32, at: P) -> Mark {
        Mark { kind, color, width, pts: vec![at, at], text: String::new(), font_size: 18.0, number: 0 }
    }
    pub fn p0(&self) -> P {
        self.pts[0]
    }
    pub fn p1(&self) -> P {
        *self.pts.last().unwrap()
    }
    pub fn set_p1(&mut self, p: P) {
        if let Some(l) = self.pts.last_mut() {
            *l = p;
        }
    }
    pub fn is_two_point(&self) -> bool {
        matches!(self.kind, Tool::Arrow | Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Highlight | Tool::Blur)
    }
    pub fn norm(&self) -> RectF {
        let (a, b) = (self.p0(), self.p1());
        RectF::ltrb(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
    }
    pub fn step_radius(&self) -> f32 {
        14f32.max(self.font_size * 0.9)
    }

    pub fn bounds(&self) -> RectF {
        match self.kind {
            Tool::Text => {
                let (w, h) = measure_text(if self.text.is_empty() { " " } else { &self.text }, self.font_size);
                RectF { x: self.p0().x, y: self.p0().y, w: w + 12.0, h: h + 8.0 }
            }
            Tool::Step => {
                let r = self.step_radius();
                RectF { x: self.p0().x - r, y: self.p0().y - r, w: 2.0 * r, h: 2.0 * r }
            }
            Tool::Pen => {
                let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for p in &self.pts {
                    l = l.min(p.x);
                    t = t.min(p.y);
                    r = r.max(p.x);
                    b = b.max(p.y);
                }
                RectF::ltrb(l, t, r, b)
            }
            _ => self.norm(),
        }
    }

    pub fn hit(&self, p: P, tol: f32) -> bool {
        let tol = tol + self.width / 2.0;
        match self.kind {
            Tool::Arrow | Tool::Line => dist_seg(p, self.p0(), self.p1()) <= tol,
            Tool::Pen => self.pts.windows(2).any(|w| dist_seg(p, w[0], w[1]) <= tol),
            Tool::Rect => {
                let r = self.norm();
                let inner = r.inflate(-tol);
                r.inflate(tol).contains(p) && !(inner.w > 0.0 && inner.h > 0.0 && inner.contains(p))
            }
            Tool::Ellipse => {
                let r = self.norm();
                let (rx, ry) = (r.w / 2.0, r.h / 2.0);
                if rx < 1.0 || ry < 1.0 {
                    return r.inflate(tol).contains(p);
                }
                let (cx, cy) = (r.x + rx, r.y + ry);
                let d = (((p.x - cx) / rx).powi(2) + ((p.y - cy) / ry).powi(2)).sqrt();
                (d - 1.0).abs() * rx.min(ry) <= tol
            }
            _ => self.bounds().inflate(tol / 2.0).contains(p),
        }
    }

    pub fn offset(&mut self, dx: f32, dy: f32) {
        for p in &mut self.pts {
            p.x += dx;
            p.y += dy;
        }
    }

    // ---- persistencia, idéntica a Mark.ToDict / FromDict de .NET
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("k".into(), Value::String(self.kind.name().into()));
        m.insert("c".into(), Value::from(self.color.0 as i32 as i64));
        m.insert("w".into(), Value::String(fmt_f(self.width)));
        m.insert("p".into(), Value::String(self.pts.iter().map(|p| format!("{},{}", fmt_f(p.x), fmt_f(p.y))).collect::<Vec<_>>().join(" ")));
        m.insert("t".into(), Value::String(self.text.clone()));
        m.insert("f".into(), Value::String(fmt_f(self.font_size)));
        m.insert("n".into(), Value::from(self.number));
        Value::Object(m)
    }

    pub fn from_json(v: &Value) -> Option<Mark> {
        let m = v.as_object()?;
        let num = |k: &str| -> Option<f64> {
            match m.get(k)? {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => s.parse().ok(),
                _ => None,
            }
        };
        let pts = m.get("p")?.as_str()?.split_whitespace().filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some(P { x: x.parse().ok()?, y: y.parse().ok()? })
        }).collect::<Vec<_>>();
        if pts.is_empty() {
            return None;
        }
        Some(Mark {
            kind: Tool::parse(m.get("k")?.as_str()?)?,
            color: Argb(num("c")? as i64 as i32 as u32),
            width: num("w")? as f32,
            pts,
            text: m.get("t").and_then(Value::as_str).unwrap_or("").to_string(),
            font_size: num("f").unwrap_or(18.0) as f32,
            number: num("n").unwrap_or(0.0) as i32,
        })
    }
}

fn fmt_f(v: f32) -> String {
    // Igual que float.ToString(InvariantCulture): sin ceros de más.
    let s = format!("{v}");
    if s.contains('e') { format!("{v:.3}") } else { s }
}

fn dist_seg(p: P, a: P, b: P) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) };
    let (px, py) = (a.x + t * dx - p.x, a.y + t * dy - p.y);
    (px * px + py * py).sqrt()
}

pub fn marks_from_meta(meta: &Map<String, Value>) -> Vec<Mark> {
    meta.get("shapes").and_then(Value::as_array).map(|a| a.iter().filter_map(Mark::from_json).collect()).unwrap_or_default()
}

pub fn marks_to_meta(marks: &[Mark]) -> Value {
    Value::Array(marks.iter().map(Mark::to_json).collect())
}

// ------------------------------------------------------------------ render final con GDI+

/// Ancho y alto de un texto como lo mide `Graphics.MeasureString` (Segoe UI Bold, píxeles).
pub fn measure_text(text: &str, size: f32) -> (f32, f32) {
    let mut px = [0u8; 4];
    unsafe {
        match gdip::Canvas::on(&mut px, 1, 1) {
            Some(c) => c.measure(text, &gdip::Font::new("Segoe UI", size, true), 0.0),
            None => (text.chars().count() as f32 * size * 0.6, size * 1.35),
        }
    }
}

/// Imagen final: la base con todas las anotaciones encima, en orden.
pub fn render(base: &Rgba, marks: &[Mark]) -> Rgba {
    let (w, h) = (base.w as i32, base.h as i32);
    let base_bgra = base.to_bgra();
    let mut out = base_bgra.clone();
    unsafe {
        let canvas = gdip::Canvas::on(&mut out, w, h);
        for m in marks {
            if m.kind == Tool::Blur {
                // El pixelado toma la base sin anotar (como .NET) y se pinta en su lugar del orden.
                pixelate_raw(&mut out, &base_bgra, base.w, base.h, m);
            } else if let Some(c) = &canvas {
                draw(c, m);
            }
        }
    }
    let mut img = Rgba { w: base.w, h: base.h, px: out };
    for p in img.px.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
    img
}

/// Pixelado por bloques: promedia cada bloque de la base y lo pinta en `dst` (mismo orden de canales).
fn pixelate_raw(dst: &mut [u8], base: &[u8], bw: u32, bh: u32, m: &Mark) {
    let r = m.norm();
    let x0 = (r.x.round() as i32).clamp(0, bw as i32);
    let y0 = (r.y.round() as i32).clamp(0, bh as i32);
    let x1 = (r.right().round() as i32).clamp(0, bw as i32);
    let y1 = (r.bottom().round() as i32).clamp(0, bh as i32);
    let (rw, rh) = (x1 - x0, y1 - y0);
    if rw < 2 || rh < 2 {
        return;
    }
    let block = 6.max((m.width * 2.5) as i32);
    let (sw, sh) = ((rw / block).max(1), (rh / block).max(1));
    let stride = bw as usize * 4;
    for by in 0..sh {
        let (ya, yb) = (y0 + by * rh / sh, y0 + (by + 1) * rh / sh);
        for bx in 0..sw {
            let (xa, xb) = (x0 + bx * rw / sw, x0 + (bx + 1) * rw / sw);
            let mut acc = [0u64; 3];
            let mut n = 0u64;
            for y in ya..yb {
                for x in xa..xb {
                    let o = y as usize * stride + x as usize * 4;
                    acc[0] += base[o] as u64;
                    acc[1] += base[o + 1] as u64;
                    acc[2] += base[o + 2] as u64;
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let c = [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8];
            for y in ya..yb {
                for x in xa..xb {
                    let o = y as usize * stride + x as usize * 4;
                    dst[o..o + 3].copy_from_slice(&c);
                    dst[o + 3] = 255;
                }
            }
        }
    }
}

unsafe fn draw(c: &gdip::Canvas, m: &Mark) {
    let g = c.g;
    c.smoothing(true);
    match m.kind {
        Tool::Arrow => {
            let p = gdip::Pen::new(m.color.0, m.width);
            // AdjustableArrowCap(ancho = cap·0,75, alto = cap, relleno), en unidades del grosor.
            let cap_size = 3f32.max(12.0 / m.width.sqrt());
            let mut cap = null_mut();
            GdipCreateAdjustableArrowCap(cap_size, cap_size * 0.75, 1, &mut cap);
            GdipSetPenCustomEndCap(p.0, cap as *mut GpCustomLineCap);
            GdipSetPenStartCap(p.0, LineCapRound);
            GdipDrawLine(g, p.0, m.p0().x, m.p0().y, m.p1().x, m.p1().y);
            GdipDeleteCustomLineCap(cap as *mut GpCustomLineCap);
        }
        Tool::Line => {
            let p = gdip::Pen::new(m.color.0, m.width);
            GdipSetPenStartCap(p.0, LineCapRound);
            GdipSetPenEndCap(p.0, LineCapRound);
            GdipDrawLine(g, p.0, m.p0().x, m.p0().y, m.p1().x, m.p1().y);
        }
        Tool::Rect => {
            let p = gdip::Pen::new(m.color.0, m.width);
            GdipSetPenLineJoin(p.0, LineJoinRound);
            let r = m.norm();
            GdipDrawRectangle(g, p.0, r.x, r.y, r.w, r.h);
        }
        Tool::Ellipse => {
            let p = gdip::Pen::new(m.color.0, m.width);
            let r = m.norm();
            GdipDrawEllipse(g, p.0, r.x, r.y, r.w, r.h);
        }
        Tool::Highlight => {
            c.smoothing(false);
            let r = m.norm();
            c.fill_rect(m.color.with_alpha(105).0, r.x, r.y, r.w, r.h);
        }
        Tool::Pen => {
            if m.pts.len() > 1 {
                let p = gdip::Pen::new(m.color.0, m.width);
                GdipSetPenStartCap(p.0, LineCapRound);
                GdipSetPenEndCap(p.0, LineCapRound);
                GdipSetPenLineJoin(p.0, LineJoinRound);
                let pts: Vec<PointF> = m.pts.iter().map(|q| PointF { X: q.x, Y: q.y }).collect();
                GdipDrawLines(g, p.0, pts.as_ptr(), pts.len() as i32);
            }
        }
        Tool::Text => {
            let b = m.bounds();
            let path = gdip::round_path(b.x, b.y, b.w, b.h, 6.0);
            let bg = gdip::Brush::new(0xEBFFFFFF);
            GdipFillPath(g, bg.raw(), path);
            let p = gdip::Pen::new(m.color.0, 2.0);
            GdipDrawPath(g, p.0, path);
            GdipDeletePath(path);
            let f = gdip::Font::new("Segoe UI", m.font_size, true);
            c.text(&m.text, &f, m.color.0, m.p0().x + 6.0, m.p0().y + 4.0);
        }
        Tool::Step => {
            let r = m.step_radius();
            let (x, y) = (m.p0().x - r, m.p0().y - r);
            let fill = gdip::Brush::new(m.color.0);
            GdipFillEllipse(g, fill.raw(), x, y, 2.0 * r, 2.0 * r);
            let ring = gdip::Pen::new(0xFFFFFFFF, 2.0);
            GdipDrawEllipse(g, ring.0, x, y, 2.0 * r, 2.0 * r);
            let f = gdip::Font::new("Segoe UI", r * 1.1, true);
            let white = gdip::Brush::new(0xFFFFFFFF);
            let mut fmt = null_mut();
            GdipCreateStringFormat(0, 0, &mut fmt);
            GdipSetStringFormatAlign(fmt, StringAlignmentCenter);
            GdipSetStringFormatLineAlign(fmt, StringAlignmentCenter);
            let t = wide(&m.number.to_string());
            let layout = windows_sys::Win32::Graphics::GdiPlus::RectF { X: x, Y: y + 1.0, Width: 2.0 * r, Height: 2.0 * r };
            GdipDrawString(g, t.as_ptr(), (t.len() - 1) as i32, f.font, &layout, fmt, white.raw());
            GdipDeleteStringFormat(fmt);
        }
        Tool::Blur | Tool::Select | Tool::Crop => {}
    }
}

pub fn save_with_gdiplus(img: &Rgba, path: &std::path::Path, fmt: &str) -> std::io::Result<()> {
    gdip::save(img, path, fmt)
}
