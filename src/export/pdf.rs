//! PDF (A4, imágenes sin pérdida, fuentes estándar), escrito a mano como en Exporter.cs.

use std::fmt::Write as _;
use std::io;

use super::layout::{self, Al, El, Src, Va};
use super::{Format, Opts, Page, Template};
use crate::image::Rgba;

fn f(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn rgb(hex: &str) -> (f64, f64, f64) {
    let c = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
    (c(0), c(2), c(4))
}

fn col(hex: &str) -> String {
    let (r, g, b) = rgb(hex);
    format!("{} {} {}", f(r), f(g), f(b))
}

/// Anchos promedio de Helvetica (por 1000 em): alcanza para cortar y ajustar texto.
fn width(s: &str, size: f64, family: &str) -> f64 {
    if family == "Courier" {
        return s.chars().count() as f64 * 600.0 * size / 1000.0;
    }
    let sum: f64 = s
        .chars()
        .map(|ch| {
            if ch == ' ' {
                260.0
            } else if ch.is_uppercase() {
                667.0
            } else if "il.,:;'|!".contains(ch) {
                260.0
            } else if "mwMW".contains(ch) {
                850.0
            } else if ch.is_ascii_digit() {
                556.0
            } else {
                520.0
            }
        })
        .sum();
    sum * size / 1000.0 * if family == "Times" { 0.92 } else { 1.0 }
}

fn clip(s: &str, size: f64, max: f64, family: &str) -> String {
    if width(s, size, family) <= max {
        return s.to_string();
    }
    let mut v: Vec<char> = s.chars().collect();
    while v.len() > 1 && width(&(v.iter().collect::<String>() + "…"), size, family) > max {
        v.pop();
    }
    v.iter().collect::<String>().trim_end().to_string() + "…"
}

fn wrap(text: &str, size: f64, max: f64, family: &str) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.replace('\r', "").split('\n') {
        let mut line = String::new();
        // Las palabras más anchas que el renglón (nombres de archivo con "_") se parten por letras, como en Office.
        let mut words: Vec<String> = Vec::new();
        for word in para.split(' ') {
            if width(word, size, family) <= max {
                words.push(word.to_string());
                continue;
            }
            let mut chunk = String::new();
            for ch in word.chars() {
                if !chunk.is_empty() && width(&format!("{chunk}{ch}"), size, family) > max {
                    words.push(std::mem::take(&mut chunk));
                }
                chunk.push(ch);
            }
            words.push(chunk);
        }
        for word in words.iter().map(|w| w.as_str()) {
            let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if width(&next, size, family) <= max || line.is_empty() {
                line = next;
            } else {
                out.push(std::mem::replace(&mut line, word.to_string()));
            }
        }
        out.push(line);
    }
    out
}

fn round_rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let r = r.min(w.min(h) / 2.0);
    let k = r * 0.5523;
    format!(
        "{} {} m {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c h",
        f(x + r), f(y), f(x + w - r), f(y), f(x + w - r + k), f(y), f(x + w), f(y + r - k), f(x + w), f(y + r),
        f(x + w), f(y + h - r), f(x + w), f(y + h - r + k), f(x + w - r + k), f(y + h), f(x + w - r), f(y + h),
        f(x + r), f(y + h), f(x + r - k), f(y + h), f(x), f(y + h - r + k), f(x), f(y + h - r),
        f(x), f(y + r), f(x), f(y + r - k), f(x + r - k), f(y), f(x + r), f(y)
    )
}

/// Cadena literal en WinAnsi (Latin-1 más los signos tipográficos de Windows-1252).
fn pstr(text: &str) -> Vec<u8> {
    let mut v = vec![b'('];
    for ch in text.chars() {
        let c: u32 = match ch {
            '—' => 0x97,
            '–' => 0x96,
            '“' => 0x93,
            '”' => 0x94,
            '‘' => 0x91,
            '’' => 0x92,
            '•' => 0x95,
            '…' => 0x85,
            '€' => 0x80,
            c => c as u32,
        };
        match c {
            0x28 | 0x29 | 0x5C => {
                v.push(b'\\');
                v.push(c as u8);
            }
            c if c < 32 => v.push(b' '),
            c if c > 255 => v.push(b'?'),
            c => v.push(c as u8),
        }
    }
    v.push(b')');
    v
}

fn unicode(text: &str) -> String {
    let mut s = String::from("<FEFF");
    for u in text.encode_utf16() {
        let _ = write!(s, "{u:04X}");
    }
    s + ">"
}

struct Writer {
    out: Vec<u8>,
    offsets: Vec<i64>,
    pending: Vec<(usize, String)>,
}

impl Writer {
    fn new() -> Writer {
        let mut out = b"%PDF-1.4\n%".to_vec();
        out.extend_from_slice(&[0xE2, 0xE3, 0xCF, 0xD3, b'\n']);
        Writer { out, offsets: vec![0], pending: Vec::new() }
    }
    fn reserve(&mut self) -> usize {
        self.offsets.push(-1);
        self.offsets.len() - 1
    }
    fn write(&mut self, id: usize, dict: &str) {
        self.offsets[id] = self.out.len() as i64;
        self.out.extend_from_slice(format!("{id} 0 obj\n{dict}\nendobj\n").as_bytes());
    }
    fn add(&mut self, dict: &str) -> usize {
        let id = self.reserve();
        self.write(id, dict);
        id
    }
    fn add_stream(&mut self, dict: &str, data: &[u8]) -> usize {
        let id = self.reserve();
        let body = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
        self.offsets[id] = self.out.len() as i64;
        self.out.extend_from_slice(format!("{id} 0 obj\n<< {dict} /Length {} /Filter /FlateDecode >>\nstream\n", body.len()).as_bytes());
        self.out.extend_from_slice(&body);
        self.out.extend_from_slice(b"\nendstream\nendobj\n");
        id
    }
    fn finish(mut self, catalog: usize, info: usize) -> Vec<u8> {
        for (id, dict) in std::mem::take(&mut self.pending) {
            self.write(id, &dict);
        }
        let xref = self.out.len();
        let mut s = format!("xref\n0 {}\n0000000000 65535 f \n", self.offsets.len());
        for o in &self.offsets[1..] {
            let _ = write!(s, "{:010} 00000 n \n", o);
        }
        let _ = write!(s, "trailer\n<< /Size {} /Root {catalog} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n", self.offsets.len());
        self.out.extend_from_slice(s.as_bytes());
        self.out
    }

    fn image(&mut self, img: &Rgba) -> (usize, u32, u32) {
        let mut raw = Vec::with_capacity(img.px.len() / 4 * 3);
        for p in img.px.chunks_exact(4) {
            // Sobre blanco, como el original (24 bpp).
            let a = p[3] as u32;
            for k in 0..3 {
                raw.push(((p[k] as u32 * a + 255 * (255 - a)) / 255) as u8);
            }
        }
        let id = self.add_stream(&format!("/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8", img.w, img.h), &raw);
        (id, img.w, img.h)
    }
}

/// Contenido de página como bytes (los textos van en WinAnsi).
#[derive(Default)]
struct Content(Vec<u8>);
impl Content {
    fn s(&mut self, s: &str) {
        self.0.extend_from_slice(s.as_bytes());
    }
    fn txt(&mut self, font: &str, size: f64, x: f64, y: f64, color: &str, text: &str) {
        self.s(&format!("BT /{font} {} Tf {color} rg {} {} Td ", f(size), f(x), f(y)));
        self.0.extend_from_slice(&pstr(text));
        self.s(" Tj ET\n");
    }
    fn tj(&mut self, color: &str, text: &str) {
        self.s(&format!("{color} rg "));
        self.0.extend_from_slice(&pstr(text));
        self.s(" Tj ");
    }
}

fn ellipse(x: f64, y: f64, w: f64, h: f64) -> String {
    let (rx, ry) = (w / 2.0, h / 2.0);
    let (cx, cy) = (x + rx, y + ry);
    let (kx, ky) = (rx * 0.5523, ry * 0.5523);
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h",
        f(cx + rx), f(cy),
        f(cx + rx), f(cy + ky), f(cx + kx), f(cy + ry), f(cx), f(cy + ry),
        f(cx - kx), f(cy + ry), f(cx - rx), f(cy + ky), f(cx - rx), f(cy),
        f(cx - rx), f(cy - ky), f(cx - kx), f(cy - ry), f(cx), f(cy - ry),
        f(cx + kx), f(cy - ry), f(cx + rx), f(cy - ky), f(cx + rx), f(cy)
    )
}

/// PDF desde el diseño compartido ([`layout::fixed`]): cada pieza se dibuja tal cual la muestra la vista previa.
pub fn pdf(pages: &[Page], o: &Opts) -> io::Result<Vec<u8>> {
    let tp = &o.t;
    let logo_img = tp.logo_path().and_then(|p| crate::image::load_png(&p).ok());
    let logo_aspect = logo_img.as_ref().map(|l| l.w as f32 / l.h.max(1) as f32);
    let doc = layout::fixed(&super::shots(pages, o), o, Format::Pdf, logo_aspect);
    let (w_, h_) = (doc.w as f64, doc.h as f64);

    let mut pdf = Writer::new();
    let body_fam = Template::pdf_family(&tp.body_font);
    let head_fam = Template::pdf_family(&tp.heading_font);
    let regular = |fam: &str| if fam == "Times" { "Times-Roman".to_string() } else { fam.to_string() };
    let font_r = pdf.add(&format!("<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>", regular(body_fam)));
    let font_b = pdf.add(&format!("<< /Type /Font /Subtype /Type1 /BaseFont /{head_fam}-Bold /Encoding /WinAnsiEncoding >>"));
    let pages_id = pdf.reserve();
    let mut kids: Vec<usize> = Vec::new();
    // Cada captura entra una sola vez aunque aparezca en varias páginas (portada, índice).
    let mut images: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let logo_id = logo_img.as_ref().map(|l| pdf.image(l).0);

    for els in &doc.pages {
        let mut c = Content::default();
        if tp.page != "FFFFFF" {
            c.s(&format!("{} rg 0 0 {} {} re f\n", col(&tp.page), f(w_), f(h_)));
        }
        let mut used: Vec<(String, usize)> = Vec::new();
        for el in els {
            match el {
                El::Fill { r, color, radius } => {
                    let (x, y, w, h) = (r.x as f64, h_ - r.bottom() as f64, r.w as f64, r.h as f64);
                    if *radius > 0.2 {
                        c.s(&format!("{} rg {} f\n", col(color), round_rect(x, y, w, h, *radius as f64)));
                    } else {
                        c.s(&format!("{} rg {} {} {} {} re f\n", col(color), f(x), f(y), f(w), f(h)));
                    }
                }
                El::Line { r, color, width, radius } => {
                    let (x, y, w, h) = (r.x as f64, h_ - r.bottom() as f64, r.w as f64, r.h as f64);
                    if *radius > 0.2 {
                        c.s(&format!("{} RG {} w {} S\n", col(color), f(*width as f64), round_rect(x, y, w, h, *radius as f64)));
                    } else {
                        c.s(&format!("{} RG {} w {} {} {} {} re S\n", col(color), f(*width as f64), f(x), f(y), f(w), f(h)));
                    }
                }
                El::Dot { r, color } => {
                    c.s(&format!("{} rg {} f\n", col(color), ellipse(r.x as f64, h_ - r.bottom() as f64, r.w as f64, r.h as f64)));
                }
                El::Text { r, text, size, head, color, al, va, lines } => {
                    let fam = if *head { head_fam } else { body_fam };
                    let size = *size as f64;
                    let mut ls = wrap(text, size, r.w as f64, fam);
                    if ls.len() > *lines {
                        ls.truncate(*lines);
                        if let Some(last) = ls.last_mut() {
                            *last = clip(&format!("{last} …"), size, r.w as f64, fam);
                        }
                    }
                    let lh = size * 1.25;
                    let total = ls.len() as f64 * lh;
                    let top = match va {
                        Va::T => r.y as f64,
                        Va::M => r.y as f64 + (r.h as f64 - total) / 2.0,
                        Va::B => r.bottom() as f64 - total,
                    };
                    for (k, line) in ls.iter().enumerate() {
                        let line = clip(line, size, r.w as f64, fam);
                        let lw = width(&line, size, fam);
                        let x = match al {
                            Al::L => r.x as f64,
                            Al::C => r.x as f64 + (r.w as f64 - lw) / 2.0,
                            Al::R => r.right() as f64 - lw,
                        };
                        let baseline = top + k as f64 * lh + (lh - size) / 2.0 + size * 0.8;
                        c.txt(if *head { "F2" } else { "F1" }, size, x, h_ - baseline, &col(color), &line);
                    }
                }
                El::Img { r, src } => {
                    let name = match src {
                        Src::Logo => {
                            let Some(id) = logo_id else { continue };
                            ("Lg".to_string(), id)
                        }
                        Src::Cap(i) => {
                            let Some(p) = pages.get(*i) else { continue };
                            let id = match images.get(i) {
                                Some(id) => *id,
                                None => {
                                    let img = super::load(&p.item)?;
                                    let id = pdf.image(&img).0;
                                    images.insert(*i, id);
                                    id
                                }
                            };
                            (format!("Im{i}"), id)
                        }
                    };
                    c.s(&format!("q {} 0 0 {} {} {} cm /{} Do Q\n", f(r.w as f64), f(r.h as f64), f(r.x as f64), f(h_ - r.bottom() as f64), name.0));
                    if !used.iter().any(|u| u.0 == name.0) {
                        used.push(name);
                    }
                }
            }
        }
        let xobj: String = used.iter().map(|(n, id)| format!(" /{n} {id} 0 R")).collect();
        let stream = pdf.add_stream("", &c.0);
        let id = pdf.add(&format!(
            "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {} {}] /Contents {stream} 0 R /Resources << /Font << /F1 {font_r} 0 R /F2 {font_b} 0 R >> /XObject <<{xobj} >> >> >>",
            f(w_),
            f(h_)
        ));
        kids.push(id);
    }

    let kids_s = kids.iter().map(|k| format!("{k} 0 R")).collect::<Vec<_>>().join(" ");
    pdf.pending.push((pages_id, format!("<< /Type /Pages /Kids [{kids_s}] /Count {} >>", kids.len())));
    let catalog = pdf.add(&format!("<< /Type /Catalog /Pages {pages_id} 0 R >>"));
    let now = crate::clock::Stamp::now();
    let info = pdf.add(&format!(
        "<< /Title {} /Producer (BuzzPrint) /CreationDate (D:{:04}{:02}{:02}{:02}{:02}{:02}) >>",
        unicode(&o.title), now.year, now.month, now.day, now.hour, now.minute, now.second
    ));
    Ok(pdf.finish(catalog, info))
}
