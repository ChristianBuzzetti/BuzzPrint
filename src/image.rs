//! Imágenes en memoria (RGBA de 8 bits) y PNG.

use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;

#[derive(Clone)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Rgba {
    pub fn new(w: u32, h: u32) -> Rgba {
        Rgba { w, h, px: vec![0; w as usize * h as usize * 4] }
    }

    pub fn from_bgra(w: u32, h: u32, bgra: &[u8]) -> Rgba {
        let mut px = bgra.to_vec();
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        Rgba { w, h, px }
    }

    pub fn to_bgra(&self) -> Vec<u8> {
        let mut v = self.px.clone();
        for p in v.chunks_exact_mut(4) {
            p.swap(0, 2);
        }
        v
    }

    /// Recorte (clampeado a los bordes).
    pub fn crop(&self, x: i32, y: i32, w: i32, h: i32) -> Rgba {
        let x0 = x.clamp(0, self.w as i32) as usize;
        let y0 = y.clamp(0, self.h as i32) as usize;
        let x1 = (x + w).clamp(x0 as i32, self.w as i32) as usize;
        let y1 = (y + h).clamp(y0 as i32, self.h as i32) as usize;
        let (cw, ch) = (x1 - x0, y1 - y0);
        let mut out = Rgba::new(cw as u32, ch as u32);
        let stride = self.w as usize * 4;
        for row in 0..ch {
            let s = (y0 + row) * stride + x0 * 4;
            out.px[row * cw * 4..(row + 1) * cw * 4].copy_from_slice(&self.px[s..s + cw * 4]);
        }
        out
    }

    /// Reducción por promedio de área: nítida y sin dependencias. Nunca agranda.
    pub fn fit(&self, max_w: u32, max_h: u32) -> Rgba {
        let s = (max_w as f64 / self.w.max(1) as f64).min(max_h as f64 / self.h.max(1) as f64).min(1.0);
        let (nw, nh) = (((self.w as f64 * s).round() as u32).max(1), ((self.h as f64 * s).round() as u32).max(1));
        if nw == self.w && nh == self.h {
            return self.clone();
        }
        let mut out = Rgba::new(nw, nh);
        let (sx, sy) = (self.w as f64 / nw as f64, self.h as f64 / nh as f64);
        let stride = self.w as usize * 4;
        for oy in 0..nh as usize {
            let y0 = (oy as f64 * sy) as usize;
            let y1 = (((oy + 1) as f64 * sy).ceil() as usize).min(self.h as usize).max(y0 + 1);
            for ox in 0..nw as usize {
                let x0 = (ox as f64 * sx) as usize;
                let x1 = (((ox + 1) as f64 * sx).ceil() as usize).min(self.w as usize).max(x0 + 1);
                let mut acc = [0u32; 4];
                for y in y0..y1 {
                    let row = &self.px[y * stride + x0 * 4..y * stride + x1 * 4];
                    for p in row.chunks_exact(4) {
                        acc[0] += p[0] as u32;
                        acc[1] += p[1] as u32;
                        acc[2] += p[2] as u32;
                        acc[3] += p[3] as u32;
                    }
                }
                let n = ((y1 - y0) * (x1 - x0)) as u32;
                let o = (oy * nw as usize + ox) * 4;
                for k in 0..4 {
                    out.px[o + k] = (acc[k] / n) as u8;
                }
            }
        }
        out
    }
}

pub fn load_png(path: &Path) -> io::Result<Rgba> {
    let file = fs::File::open(path)?;
    let mut dec = png::Decoder::new(io::BufReader::new(file));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(io::Error::other)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(io::Error::other)?;
    let (w, h) = (info.width, info.height);
    let n = w as usize * h as usize;
    let mut px = vec![255u8; n * 4];
    match info.color_type {
        png::ColorType::Rgba => px.copy_from_slice(&buf[..n * 4]),
        png::ColorType::Rgb => {
            for (d, s) in px.chunks_exact_mut(4).zip(buf.chunks_exact(3)) {
                d[..3].copy_from_slice(s);
            }
        }
        png::ColorType::Grayscale => {
            for (d, s) in px.chunks_exact_mut(4).zip(buf.iter()) {
                d[0] = *s;
                d[1] = *s;
                d[2] = *s;
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for (d, s) in px.chunks_exact_mut(4).zip(buf.chunks_exact(2)) {
                d[0] = s[0];
                d[1] = s[0];
                d[2] = s[0];
                d[3] = s[1];
            }
        }
        png::ColorType::Indexed => return Err(io::Error::other("PNG indexado sin expandir")),
    }
    Ok(Rgba { w, h, px })
}

/// PNG RGB de 8 bits (las capturas no necesitan alfa), escrito fila por fila.
pub fn save_png(img: &Rgba, path: &Path) -> io::Result<()> {
    let file = fs::File::create(path)?;
    write_png(img, BufWriter::with_capacity(256 * 1024, file))
}

pub fn png_bytes(img: &Rgba) -> io::Result<Vec<u8>> {
    let mut v = Vec::with_capacity(img.px.len() / 4);
    write_png(img, &mut v)?;
    Ok(v)
}

fn write_png<W: Write>(img: &Rgba, out: W) -> io::Result<()> {
    let mut enc = png::Encoder::new(out, img.w, img.h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(io::Error::other)?;
    {
        let mut stream = writer.stream_writer().map_err(io::Error::other)?;
        let mut row = vec![0u8; img.w as usize * 3];
        for src in img.px.chunks_exact(img.w as usize * 4) {
            for (d, s) in row.chunks_exact_mut(3).zip(src.chunks_exact(4)) {
                d.copy_from_slice(&s[..3]);
            }
            stream.write_all(&row)?;
        }
        stream.finish().map_err(io::Error::other)?;
    }
    writer.finish().map_err(io::Error::other)
}
