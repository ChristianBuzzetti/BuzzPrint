//! El logo como imagen: rasterizador por software de los triángulos de [`super::mark::build`]
//! (con supermuestreo 4×4) y conversión a HICON para la bandeja y las ventanas.
//! No necesita OpenGL ni ventanas: sirve desde la bandeja, donde no hay ningún contexto abierto.

use std::ptr::null_mut;

use egui::{Color32, Vec2};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

use super::mark::{self, Style};
use crate::image::Rgba;

const SS: usize = 4;

/// El logo quieto, de `size`×`size` píxeles, con fondo transparente.
pub fn raster(size: u32, accent: Color32, st: &Style) -> Rgba {
    raster_full(size, accent, st, Vec2::ZERO, (1.0, 1.0), 0.0)
}

pub fn raster_full(size: u32, accent: Color32, st: &Style, eye: Vec2, blink: (f32, f32), spin: f32) -> Rgba {
    let n = size as usize * SS;
    let tris = mark::build(accent, st, eye, blink, spin, size <= 40);
    let k = n as f32 / 64.0;
    // Muestra: color (lineal en sRGB, como egui) y si está cubierta.
    let mut col = vec![[0f32; 3]; n * n];
    let mut cov = vec![false; n * n];
    for t in tris.idx.chunks_exact(3) {
        let v: Vec<(f32, f32, [f32; 3])> = t
            .iter()
            .map(|&i| {
                let (x, y, c) = tris.verts[i as usize];
                (x * k, y * k, [c.r() as f32, c.g() as f32, c.b() as f32])
            })
            .collect();
        let (a, b, c) = (v[0], v[1], v[2]);
        let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if area.abs() < 1e-6 {
            continue;
        }
        let x0 = a.0.min(b.0).min(c.0).floor().max(0.0) as usize;
        let x1 = (a.0.max(b.0).max(c.0).ceil() as usize).min(n);
        let y0 = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
        let y1 = (a.1.max(b.1).max(c.1).ceil() as usize).min(n);
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let w0 = ((b.0 - px) * (c.1 - py) - (b.1 - py) * (c.0 - px)) / area;
                let w1 = ((c.0 - px) * (a.1 - py) - (c.1 - py) * (a.0 - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                    continue;
                }
                let i = y * n + x;
                for ch in 0..3 {
                    col[i][ch] = a.2[ch] * w0 + b.2[ch] * w1 + c.2[ch] * w2;
                }
                cov[i] = true;
            }
        }
    }
    let mut out = Rgba::new(size, size);
    for y in 0..size as usize {
        for x in 0..size as usize {
            let (mut sum, mut hits) = ([0f32; 3], 0);
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = (y * SS + sy) * n + x * SS + sx;
                    if cov[i] {
                        hits += 1;
                        for ch in 0..3 {
                            sum[ch] += col[i][ch];
                        }
                    }
                }
            }
            if hits > 0 {
                let o = (y * size as usize + x) * 4;
                for ch in 0..3 {
                    out.px[o + ch] = (sum[ch] / hits as f32).round().clamp(0.0, 255.0) as u8;
                }
                out.px[o + 3] = (hits * 255 / (SS * SS)) as u8;
            }
        }
    }
    out
}

/// HICON con transparencia a partir de una imagen RGBA. Quien lo recibe lo libera con `DestroyIcon`.
pub fn hicon(img: &Rgba) -> HICON {
    unsafe {
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: img.w as i32,
            biHeight: -(img.h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut core::ffi::c_void = null_mut();
        let color = CreateDIBSection(null_mut(), &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if color.is_null() || bits.is_null() {
            return null_mut();
        }
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, img.px.len());
        for (d, s) in dst.chunks_exact_mut(4).zip(img.px.chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
        let mask = CreateBitmap(img.w as i32, img.h as i32, 1, 1, null_mut());
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}

/// El logo actual (forma, colores y acento de Personalizar) como ícono de `size` píxeles.
/// El ícono de la app (bandeja, barra de tareas y ventanas): siempre el mismo, el que va incrustado en el
/// .exe (`resources\buzzprint.ico`). No sigue al logo ni al acento de Personalizar: la app se reconoce igual.
pub fn current_icon(size: i32) -> HICON {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_DEFAULTCOLOR};
    let size = size.max(16);
    let h = unsafe { LoadImageW(GetModuleHandleW(std::ptr::null()), 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) };
    if !h.is_null() {
        return h as HICON;
    }
    // Sin recurso (no debería pasar): el logo de siempre, con su placa.
    hicon(&raster(size as u32, egui::Color32::from_rgb(0xF4, 0x3F, 0x5E), &mark::Style { mark_shape: "hex".into(), mark_eyes: "oval".into(), mark_head: "#0E1015".into(), mark_tile: "accent".into(), ..Default::default() }))
}

/// Escribe el logo en todas sus formas y ojos como PNG (diagnóstico: `--marktest <carpeta>`).
pub fn selftest(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let accent = Color32::from_rgb(0xF4, 0x3F, 0x5E);
    let st = |sh: &str, ey: &str, hd: &str, tl: &str| Style { mark_shape: sh.into(), mark_eyes: ey.into(), mark_head: hd.into(), mark_tile: tl.into(), ..Default::default() };
    // Una hoja con todas las formas (fila 1), ojos (fila 2), colores de cabeza (fila 3) y miradas (fila 4).
    let cell = 128u32;
    let mut sheet = Rgba::new(cell * 14, cell * 4);
    let mut put = |img: &Rgba, cx: u32, cy: u32| {
        for y in 0..img.h {
            for x in 0..img.w {
                let s = ((y * img.w + x) * 4) as usize;
                let d = (((cy * cell + y) * sheet.w + cx * cell + x) * 4) as usize;
                sheet.px[d..d + 4].copy_from_slice(&img.px[s..s + 4]);
            }
        }
    };
    for (i, s) in mark::SHAPES.iter().enumerate() {
        put(&raster(cell, accent, &st(s, "oval", "#0E1015", "accent")), i as u32, 0);
    }
    for (i, e) in mark::EYES.iter().enumerate() {
        put(&raster(cell, accent, &st("hex", e, "#0E1015", "accent")), i as u32, 1);
    }
    for (i, h) in mark::HEADS.iter().enumerate() {
        put(&raster(cell, accent, &st("circle", "oval", h, "accent")), i as u32, 2);
    }
    put(&raster(cell, accent, &st("ghost", "oval", "#F4F4F5", "none")), 7, 2);
    let looks = [(-3.4, 0.0), (3.4, 0.0), (0.0, -2.6), (0.0, 2.6), (2.4, 1.8), (-2.4, -1.8)];
    for (i, (x, y)) in looks.iter().enumerate() {
        put(&raster_full(cell, accent, &st("hex", "oval", "#0E1015", "accent"), Vec2::new(*x, *y), (1.0, 1.0), 0.0), i as u32, 3);
    }
    put(&raster_full(cell, accent, &st("hex", "oval", "#0E1015", "accent"), Vec2::ZERO, (0.3, 0.3), 0.0), 6, 3);
    put(&raster_full(cell, accent, &st("hex", "oval", "#0E1015", "accent"), Vec2::ZERO, (1.0, 1.0), 40.0), 7, 3);
    crate::image::save_png(&sheet, &dir.join("marks.png"))?;
    crate::image::save_png(&raster(16, accent, &st("hex", "oval", "#0E1015", "accent")), &dir.join("mark16.png"))?;
    crate::image::save_png(&raster(32, accent, &st("hex", "oval", "#0E1015", "accent")), &dir.join("mark32.png"))?;
    Ok(())
}
