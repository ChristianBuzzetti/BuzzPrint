//! Presentación del reporte de IA para el portapapeles y los archivos exportados.

use std::fmt::Write;
use std::io;

use crate::gdip::{Argb, Canvas, Font};
use crate::image::Rgba;

pub struct Report<'a> {
    title: &'a str,
    sections: Vec<(&'a str, Vec<&'a str>)>,
    accent: Argb,
}

impl<'a> Report<'a> {
    pub fn new(title: &'a str, text: &'a str, accent: Argb) -> Self {
        let mut sections = Vec::new();
        let mut lines = Vec::new();
        for line in text.lines().chain(std::iter::once("")) {
            let line = line.trim_end();
            if !line.is_empty() {
                lines.push(line);
            } else if !lines.is_empty() {
                let first = lines[0].trim().trim_end_matches(':');
                let heading = if lines.len() > 1 && first.chars().count() <= 48 && !first.ends_with('.') {
                    lines.remove(0);
                    first
                } else {
                    ""
                };
                sections.push((heading, std::mem::take(&mut lines)));
            }
        }
        Self { title, sections, accent }
    }

    pub fn html(&self) -> String {
        let accent = self.accent & 0xff_ffff;
        let title = escape(self.title);
        let mut html = format!(r#"<div style="font-family:'Segoe UI',Arial,sans-serif;font-size:14px;line-height:1.5;color:#1f2328;background:#fff;border:1px solid #e5e7eb;border-top:4px solid #{accent:06x};padding:20px 24px;max-width:780px"><h1 style="color:#{accent:06x};font-size:20px;margin:0 0 12px">{title}</h1>"#);
        for (heading, lines) in &self.sections {
            html.push_str(r#"<div style="margin-top:16px">"#);
            if !heading.is_empty() {
                let _ = write!(html, r#"<h2 style="font-size:13px;color:#4b5563;border-bottom:1px solid #e5e7eb;margin:0 0 6px">{}</h2>"#, escape(heading));
            }
            for (i, line) in lines.iter().enumerate() {
                let line = line.trim_start();
                if i == 0 && is_severity(heading) {
                    let (word, rest) = severity_parts(line);
                    let _ = write!(html, r#"<p style="margin:4px 0"><strong style="background:#{:06x};color:#fff;padding:2px 8px;margin-right:6px">{}</strong> {}</p>"#, severity_color(word) & 0xff_ffff, escape(word), escape(rest));
                } else if is_log(line) {
                    let _ = write!(html, r#"<p style="background:#f6f8fa;font-family:Consolas,monospace;white-space:pre-wrap;overflow-wrap:anywhere;padding:6px 10px;margin:4px 0">{}</p>"#, escape(line));
                } else {
                    let _ = write!(html, r#"<p style="margin:4px 0;white-space:pre-wrap">{}</p>"#, escape(line));
                }
            }
            html.push_str("</div>");
        }
        html.push_str("</div>");
        html
    }

    pub fn html_page(&self) -> String {
        format!(
            r#"<!doctype html><html><head><meta charset="utf-8"><title>{}</title></head><body style="background:#f3f4f6;padding:24px">{}</body></html>"#,
            escape(self.title),
            self.html()
        )
    }

    pub fn markdown(&self) -> String {
        let mut out = format!("# {}\n\n", self.title);
        for (heading, lines) in &self.sections {
            if !heading.is_empty() {
                let _ = write!(out, "## {heading}\n\n");
            }
            for line in lines {
                if is_log(line.trim_start()) {
                    out.push_str("    ");
                }
                let _ = writeln!(out, "{line}");
            }
            out.push('\n');
        }
        out
    }

    pub fn image(&self) -> io::Result<Rgba> {
        const WIDTH: i32 = 1200;
        const PAD: f32 = 36.0;
        const BODY_WIDTH: f32 = WIDTH as f32 - 2.0 * PAD;
        let unavailable = || io::Error::other(crate::i18n::t("No se pudo dibujar la imagen."));
        let mut scratch = [0u8; 4];
        let measure = unsafe { Canvas::on(&mut scratch, 1, 1) }.ok_or_else(unavailable)?;
        let title = Font::new("Segoe UI", 28.0, true);
        let heading = Font::new("Segoe UI", 18.0, true);
        let body = Font::new("Segoe UI", 21.0, false);
        let mono = Font::new("Consolas", 19.0, false);
        let mut rows = Vec::new();
        let mut y = PAD;
        let mut add = |text, font, ink, background, gap| {
            y += gap;
            let height = measure.measure(text, font, BODY_WIDTH - 20.0).1.ceil() + 8.0;
            rows.push((text, font, ink, background, y, height));
            y += height;
        };
        add(self.title, &title, self.accent, None, 0.0);
        for (name, lines) in &self.sections {
            if !name.is_empty() {
                add(name, &heading, 0xff4b5563, None, 14.0);
            }
            for (i, line) in lines.iter().enumerate() {
                let line = line.trim_start();
                let (font, ink, background) = if i == 0 && is_severity(name) {
                    (&heading, severity_color(severity_parts(line).0), Some(0xfff6f8fa))
                } else if is_log(line) {
                    (&mono, 0xff1f2328, Some(0xfff6f8fa))
                } else {
                    (&body, 0xff1f2328, None)
                };
                add(line, font, ink, background, if i == 0 && name.is_empty() { 14.0 } else { 3.0 });
            }
        }
        drop(measure);
        let height = (y + PAD).ceil() as i32;
        if height > 16_384 {
            return Err(io::Error::other(crate::i18n::t("El resultado es demasiado largo para guardarlo como imagen.")));
        }
        let mut pixels = vec![255; WIDTH as usize * height as usize * 4];
        {
            let canvas = unsafe { Canvas::on(&mut pixels, WIDTH, height) }.ok_or_else(unavailable)?;
            canvas.fill_rect(self.accent, 0.0, 0.0, WIDTH as f32, 6.0);
            for (text, font, ink, background, y, h) in rows {
                if let Some(color) = background {
                    canvas.fill_rect(color, PAD, y, BODY_WIDTH, h);
                }
                canvas.text_in(text, font, ink, PAD + 10.0, y + 3.0, BODY_WIDTH - 20.0, h, 0, false);
            }
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        Ok(Rgba { w: WIDTH as u32, h: height as u32, px: pixels })
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn is_log(line: &str) -> bool {
    line.starts_with(['"', '`'])
}

fn is_severity(heading: &str) -> bool {
    let heading = heading.to_lowercase();
    ["severity", "severidad", "prioridad", "priority"].iter().any(|term| heading.contains(term))
}

fn severity_parts(line: &str) -> (&str, &str) {
    let delimiters = [' ', '—', '-', ':', ','];
    let end = line.find(delimiters).unwrap_or(line.len());
    (&line[..end], line[end..].trim_start_matches(delimiters))
}

fn severity_color(word: &str) -> Argb {
    let word = word.to_lowercase();
    if word.starts_with("crit") || word.starts_with("high") || word.starts_with("alt") {
        0xffb91c1c
    } else if word.starts_with("med") {
        0xff92400e
    } else if word.starts_with("low") || word.starts_with("baj") {
        0xff047857
    } else {
        0xff4b5563
    }
}
