//! Exportación: capturas en orden a PowerPoint, Word, PDF o una sola imagen, con las plantillas de diseño.
//! Port directo de Exporter.cs: los Office son paquetes Open XML escritos a mano y el PDF también, así no
//! hace falta nada más que el propio ejecutable.

pub mod layout;
mod office;
mod pdf;
mod raster;
pub mod templates;
mod zip;

use std::fs;
use std::io;
use std::path::Path;

pub use templates::Template;

use crate::clock::Stamp;
use crate::i18n::{count, es, t};
use crate::image::Rgba;
use crate::store::Item;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Pptx,
    Docx,
    Pdf,
    Png,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Pptx => "pptx",
            Format::Docx => "docx",
            Format::Pdf => "pdf",
            Format::Png => "png",
        }
    }
    pub fn parse(s: &str) -> Format {
        match s {
            "docx" => Format::Docx,
            "pdf" => Format::Pdf,
            "png" => Format::Png,
            _ => Format::Pptx,
        }
    }
}

#[derive(Clone)]
pub struct Page {
    pub item: Item,
    pub title: String,
    pub note: String,
}

#[derive(Clone)]
pub struct Opts {
    pub title: String,
    pub intro: String,
    pub cover: bool,
    pub numbers: bool,
    pub captions: bool,
    pub meta: bool,
    pub one_per_page: bool,
    pub landscape: bool,
    /// Acento resuelto, seis dígitos hex sin "#".
    pub accent: String,
    pub t: Template,
}

pub fn write(path: &Path, format: Format, pages: &[Page], o: &Opts) -> io::Result<()> {
    if pages.is_empty() {
        return Err(io::Error::other(t("No hay capturas para exportar.")));
    }
    let mut o = o.clone();
    o.t = o.t.normalized();
    let bytes = match format {
        Format::Pptx => office::pptx(pages, &o)?,
        Format::Docx => office::docx(pages, &o)?,
        Format::Pdf => pdf::pdf(pages, &o)?,
        Format::Png => crate::image::png_bytes(&raster::render(pages, &o)?)?,
    };
    let tmp = path.with_extension(format!("{}.tmp", format.ext()));
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Ctrl+Shift+C: las capturas elegidas, numeradas y con título, apiladas en una imagen al portapapeles.
pub fn copy_as_one_image(ids: &[String]) {
    let pages: Vec<Page> = ids
        .iter()
        .filter_map(|id| crate::store::with(|s| s.find(id)))
        .filter(|i| !i.is_recording())
        .map(|i| Page { title: i.display(), note: String::new(), item: i })
        .collect();
    if pages.is_empty() {
        return;
    }
    let o = Opts {
        title: String::new(),
        intro: String::new(),
        cover: false,
        numbers: true,
        captions: true,
        meta: false,
        one_per_page: false,
        landscape: false,
        accent: app_accent(),
        t: templates::builtins()[0].clone().normalized(),
    };
    if let Ok(img) = raster::render(&pages, &o) {
        crate::output::copy_image(crate::app::main_hwnd(), &img);
    }
}

pub fn app_accent() -> String {
    let c = crate::gui::colors().accent;
    format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

// ------------------------------------------------------------------ textos compartidos

pub(crate) fn step_label(i: usize) -> String {
    format!("{} {}", t("Paso"), i + 1)
}

/// Fecha corta ("g") en el idioma de la app.
fn short_date(s: &Stamp) -> String {
    if es() {
        format!("{:02}/{:02}/{:04} {:02}:{:02}", s.day, s.month, s.year, s.hour, s.minute)
    } else {
        let h12 = if s.hour % 12 == 0 { 12 } else { s.hour % 12 };
        format!("{}/{}/{} {}:{:02} {}", s.month, s.day, s.year, h12, s.minute, if s.hour < 12 { "AM" } else { "PM" })
    }
}

/// Fecha larga ("D") en el idioma de la app.
fn long_date(s: &Stamp) -> String {
    let w = crate::clock::weekday_name(s.weekday());
    let m = crate::clock::month_name(s.month);
    if es() {
        format!("{w}, {} de {m} de {}", s.day, s.year)
    } else {
        format!("{w}, {m} {}, {}", s.day, s.year)
    }
}

pub(crate) fn meta_line(p: &Page) -> String {
    let mut parts = Vec::new();
    if !p.item.source.trim().is_empty() && p.item.source != p.title {
        parts.push(p.item.source.trim().to_string());
    }
    parts.push(short_date(&p.item.created));
    if p.item.width > 0 {
        parts.push(format!("{}×{}", p.item.width, p.item.height));
    }
    parts.join("  ·  ")
}

pub(crate) fn subtitle(n: usize) -> String {
    format!("{}  ·  {}", count(n, "captura", "capturas"), long_date(&Stamp::now()))
}

/// Lo que el diseño necesita de cada página (título, nota, origen y tamaño de la imagen).
pub fn shots(pages: &[Page], o: &Opts) -> Vec<layout::Shot> {
    pages
        .iter()
        .map(|p| {
            let (w, h) = size_of(&p.item);
            layout::Shot { title: if o.captions { p.title.clone() } else { String::new() }, note: p.note.clone(), meta: meta_line(p), w, h }
        })
        .collect()
}

pub(crate) fn load(item: &Item) -> io::Result<Rgba> {
    crate::image::load_png(&item.path)
}

pub(crate) fn size_of(item: &Item) -> (u32, u32) {
    if item.width > 0 && item.height > 0 {
        return (item.width, item.height);
    }
    load(item).map(|i| (i.w, i.h)).unwrap_or((1, 1))
}

pub(crate) fn fit(iw: i64, ih: i64, max_w: i64, max_h: i64) -> (i64, i64) {
    let s = (max_w as f64 / iw.max(1) as f64).min(max_h as f64 / ih.max(1) as f64);
    ((iw as f64 * s) as i64, (ih as f64 * s) as i64)
}

pub(crate) fn xml(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}
