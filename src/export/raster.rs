//! "Una imagen" (PNG y Copiar como una sola imagen): dibuja el diseño compartido ([`super::layout::tall`])
//! con GDI+, en píxeles.

use std::collections::HashMap;
use std::io;

use super::layout::{self, Al, El, Src, Va};
use super::{load, Opts, Page};
use crate::gdip::{hex_argb, Canvas, Font};
use crate::image::Rgba;

pub fn render(pages: &[Page], o: &Opts) -> io::Result<Rgba> {
    let tp = &o.t;
    let logo = tp.logo_path().and_then(|p| crate::image::load_png(&p).ok());
    let doc = layout::tall(&super::shots(pages, o), o, logo.as_ref().map(|l| l.w as f32 / l.h.max(1) as f32));
    let (cw, ch) = (doc.w.ceil() as i32, doc.h.ceil().max(1.0) as i32);
    let mut buf = vec![0u8; cw as usize * ch as usize * 4];
    {
        let g = unsafe { Canvas::on(&mut buf, cw, ch) }.ok_or_else(|| io::Error::other("GDI+"))?;
        g.clear(hex_argb(&doc.page));
        let mut cache: HashMap<usize, Rgba> = HashMap::new();
        for el in doc.pages.first().map(|p| p.as_slice()).unwrap_or(&[]) {
            match el {
                El::Fill { r, color, radius } => {
                    if *radius > 0.5 {
                        g.fill_round(hex_argb(color), r.x, r.y, r.w, r.h, *radius);
                    } else {
                        g.fill_rect(hex_argb(color), r.x, r.y, r.w, r.h);
                    }
                }
                El::Line { r, color, width, .. } => g.draw_rect(hex_argb(color), *width, r.x, r.y, r.w, r.h),
                El::Dot { r, color } => g.fill_round(hex_argb(color), r.x, r.y, r.w, r.h, r.w.min(r.h) / 2.0),
                El::Text { r, text, size, head, color, al, va, lines } => {
                    let font = Font::new(if *head { &tp.heading_font } else { &tp.body_font }, *size, *head);
                    let align = match al {
                        Al::L => 0,
                        Al::C => 1,
                        Al::R => 2,
                    };
                    let single = *lines <= 1;
                    let th = if single { size * 1.3 } else { g.measure(text, &font, r.w).1.min(r.h) };
                    let y = match va {
                        Va::T => r.y,
                        Va::M => r.y + (r.h - th) / 2.0,
                        Va::B => r.bottom() - th,
                    };
                    g.text_in(text, &font, hex_argb(color), r.x, y, r.w, th.max(size * 1.3), align, single);
                }
                El::Img { r, src } => {
                    let img = match src {
                        Src::Logo => logo.as_ref(),
                        Src::Cap(i) => {
                            if !cache.contains_key(i) {
                                if let Some(p) = pages.get(*i) {
                                    cache.insert(*i, load(&p.item)?);
                                }
                            }
                            cache.get(i)
                        }
                    };
                    if let Some(img) = img {
                        g.image(img, r.x.round() as i32, r.y.round() as i32, r.w.round() as i32, r.h.round() as i32);
                    }
                }
            }
        }
    }
    let mut img = Rgba { w: cw as u32, h: ch as u32, px: buf };
    for p in img.px.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
    Ok(img)
}
