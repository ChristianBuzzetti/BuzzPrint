//! GDI+ (el motor de System.Drawing) sobre buffers propios: anotaciones e imagen única de exportación salen
//! igual que en la versión .NET. GDI+ se inicia la primera vez que hace falta.

use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::Win32::Graphics::GdiPlus::*;

use crate::image::Rgba;
use crate::win::wide;

pub fn startup() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: 0, SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
        GdiplusStartup(&mut token, &input, null_mut()) == 0
    })
}

const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026200A;

/// Color ARGB.
pub type Argb = u32;

pub fn hex_argb(hex: &str) -> Argb {
    let h = hex.trim().trim_start_matches('#');
    let v = u32::from_str_radix(h.get(..6).unwrap_or("000000"), 16).unwrap_or(0);
    0xFF00_0000 | v
}

/// Lienzo sobre un buffer BGRA (el buffer debe vivir más que el lienzo).
pub struct Canvas {
    pub bmp: *mut GpBitmap,
    pub g: *mut GpGraphics,
}

impl Canvas {
    /// # Safety
    /// `bgra` debe medir `w*h*4` y no moverse mientras exista el lienzo.
    pub unsafe fn on(bgra: &mut [u8], w: i32, h: i32) -> Option<Canvas> {
        if !startup() {
            return None;
        }
        let mut bmp = null_mut();
        if GdipCreateBitmapFromScan0(w, h, w * 4, PIXEL_FORMAT_32BPP_ARGB as _, bgra.as_mut_ptr(), &mut bmp) != 0 {
            return None;
        }
        let mut g = null_mut();
        if GdipGetImageGraphicsContext(bmp as *mut GpImage, &mut g) != 0 {
            GdipDisposeImage(bmp as *mut GpImage);
            return None;
        }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        GdipSetTextRenderingHint(g, TextRenderingHintAntiAliasGridFit);
        GdipSetInterpolationMode(g, InterpolationModeHighQualityBicubic);
        Some(Canvas { bmp, g })
    }

    pub fn smoothing(&self, on: bool) {
        unsafe { GdipSetSmoothingMode(self.g, if on { SmoothingModeAntiAlias } else { SmoothingModeNone }) };
    }

    pub fn clear(&self, c: Argb) {
        unsafe { GdipGraphicsClear(self.g, c) };
    }

    pub fn fill_rect(&self, c: Argb, x: f32, y: f32, w: f32, h: f32) {
        let b = Brush::new(c);
        unsafe { GdipFillRectangle(self.g, b.raw(), x, y, w, h) };
    }

    pub fn draw_rect(&self, c: Argb, width: f32, x: f32, y: f32, w: f32, h: f32) {
        let p = Pen::new(c, width);
        unsafe { GdipDrawRectangle(self.g, p.0, x, y, w, h) };
    }

    pub fn line(&self, c: Argb, width: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        let p = Pen::new(c, width);
        unsafe { GdipDrawLine(self.g, p.0, x1, y1, x2, y2) };
    }

    pub fn fill_round(&self, c: Argb, x: f32, y: f32, w: f32, h: f32, r: f32) {
        unsafe {
            let path = round_path(x, y, w, h, r);
            let b = Brush::new(c);
            GdipFillPath(self.g, b.raw(), path);
            GdipDeletePath(path);
        }
    }

    /// Texto en un punto (como `DrawString(text, font, brush, x, y)`).
    pub fn text(&self, s: &str, f: &Font, c: Argb, x: f32, y: f32) {
        self.text_in(s, f, c, x, y, 0.0, 0.0, 0, false);
    }

    /// Texto en un rectángulo. `align`: 0 izquierda, 1 centro, 2 derecha. `single`: una línea con "…".
    #[allow(clippy::too_many_arguments)]
    pub fn text_in(&self, s: &str, f: &Font, c: Argb, x: f32, y: f32, w: f32, h: f32, align: i32, single: bool) {
        unsafe {
            let b = Brush::new(c);
            let mut fmt = null_mut();
            GdipCreateStringFormat(if single { StringFormatFlagsNoWrap } else { 0 }, 0, &mut fmt);
            GdipSetStringFormatAlign(fmt, align);
            if single {
                GdipSetStringFormatTrimming(fmt, StringTrimmingEllipsisCharacter);
            }
            let t = wide(s);
            let r = RectF { X: x, Y: y, Width: w, Height: h };
            GdipDrawString(self.g, t.as_ptr(), (t.len() - 1) as i32, f.font, &r, fmt, b.raw());
            GdipDeleteStringFormat(fmt);
        }
    }

    /// Mide un texto; `width` > 0 = con ajuste de línea a ese ancho.
    pub fn measure(&self, s: &str, f: &Font, width: f32) -> (f32, f32) {
        unsafe {
            let mut fmt = null_mut();
            GdipCreateStringFormat(0, 0, &mut fmt);
            let t = wide(if s.is_empty() { " " } else { s });
            let layout = RectF { X: 0.0, Y: 0.0, Width: width, Height: 0.0 };
            let mut out = RectF { X: 0.0, Y: 0.0, Width: 0.0, Height: 0.0 };
            GdipMeasureString(self.g, t.as_ptr(), (t.len() - 1) as i32, f.font, &layout, fmt, &mut out, null_mut(), null_mut());
            GdipDeleteStringFormat(fmt);
            (out.Width, out.Height)
        }
    }

    /// Dibuja una imagen escalada (bicúbica de alta calidad).
    pub fn image(&self, img: &Rgba, x: i32, y: i32, w: i32, h: i32) {
        let mut bgra = img.to_bgra();
        unsafe {
            let mut src = null_mut();
            if GdipCreateBitmapFromScan0(img.w as i32, img.h as i32, img.w as i32 * 4, PIXEL_FORMAT_32BPP_ARGB as _, bgra.as_mut_ptr(), &mut src) == 0 {
                GdipDrawImageRectI(self.g, src as *mut GpImage, x, y, w, h);
                GdipDisposeImage(src as *mut GpImage);
            }
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteGraphics(self.g);
            GdipDisposeImage(self.bmp as *mut GpImage);
        }
    }
}

pub struct Pen(pub *mut GpPen);
impl Pen {
    pub fn new(c: Argb, w: f32) -> Pen {
        let mut p = null_mut();
        unsafe { GdipCreatePen1(c, w, UnitWorld, &mut p) };
        Pen(p)
    }
}
impl Drop for Pen {
    fn drop(&mut self) {
        unsafe { GdipDeletePen(self.0) };
    }
}

pub struct Brush(pub *mut GpSolidFill);
impl Brush {
    pub fn new(c: Argb) -> Brush {
        let mut b = null_mut();
        unsafe { GdipCreateSolidFill(c, &mut b) };
        Brush(b)
    }
    pub fn raw(&self) -> *mut GpBrush {
        self.0 as *mut GpBrush
    }
}
impl Drop for Brush {
    fn drop(&mut self) {
        unsafe { GdipDeleteBrush(self.0 as *mut GpBrush) };
    }
}

pub struct Font {
    family: *mut GpFontFamily,
    pub font: *mut GpFont,
}
impl Font {
    /// Fuente en píxeles. Si la familia no existe, cae en Segoe UI.
    pub fn new(name: &str, size: f32, bold: bool) -> Font {
        unsafe {
            let mut family = null_mut();
            if GdipCreateFontFamilyFromName(wide(name).as_ptr(), null_mut(), &mut family) != 0 {
                GdipCreateFontFamilyFromName(wide("Segoe UI").as_ptr(), null_mut(), &mut family);
            }
            let mut font = null_mut();
            let style = if bold { FontStyleBold } else { FontStyleRegular };
            if GdipCreateFont(family, size.max(1.0), style, UnitPixel, &mut font) != 0 {
                GdipCreateFont(family, size.max(1.0), FontStyleRegular, UnitPixel, &mut font);
            }
            Font { family, font }
        }
    }
}
impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteFont(self.font);
            GdipDeleteFontFamily(self.family);
        }
    }
}

pub unsafe fn round_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> *mut GpPath {
    let mut p = null_mut();
    GdipCreatePath(FillModeAlternate, &mut p);
    let d = r * 2.0;
    GdipAddPathArc(p, x, y, d, d, 180.0, 90.0);
    GdipAddPathArc(p, x + w - d, y, d, d, 270.0, 90.0);
    GdipAddPathArc(p, x + w - d, y + h - d, d, d, 0.0, 90.0);
    GdipAddPathArc(p, x, y + h - d, d, d, 90.0, 90.0);
    GdipClosePathFigure(p);
    p
}

/// Abre cualquier imagen que entienda Windows (PNG, JPG, BMP, GIF…) y la achica a `max_w` de ancho como mucho.
/// Conserva la transparencia.
pub fn load_scaled(path: &std::path::Path, max_w: u32) -> Option<Rgba> {
    if !startup() {
        return None;
    }
    unsafe {
        let p = crate::win::wide_os(path.as_os_str());
        let mut img = null_mut();
        if GdipLoadImageFromFile(p.as_ptr(), &mut img) != 0 || img.is_null() {
            return None;
        }
        let (mut w, mut h) = (0u32, 0u32);
        GdipGetImageWidth(img, &mut w);
        GdipGetImageHeight(img, &mut h);
        if w == 0 || h == 0 || w > 20000 || h > 20000 {
            GdipDisposeImage(img);
            return None;
        }
        let scale = (max_w as f64 / w as f64).min(1.0);
        let (dw, dh) = (((w as f64 * scale) as i32).max(1), ((h as f64 * scale) as i32).max(1));
        let mut bgra = vec![0u8; dw as usize * dh as usize * 4];
        let ok = match Canvas::on(&mut bgra, dw, dh) {
            Some(c) => GdipDrawImageRectI(c.g, img, 0, 0, dw, dh) == 0,
            None => false,
        };
        GdipDisposeImage(img);
        if !ok {
            return None;
        }
        let mut out = Rgba::new(dw as u32, dh as u32);
        for (d, s) in out.px.chunks_exact_mut(4).zip(bgra.chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
        Some(out)
    }
}

/// Guarda con los codificadores de Windows ("png", "jpg", "bmp").
pub fn save(img: &Rgba, path: &std::path::Path, fmt: &str) -> std::io::Result<()> {
    use windows_sys::core::GUID;
    let clsid = match fmt {
        "jpg" => GUID::from_u128(0x557cf401_1a04_11d3_9a73_0000f81ef32e),
        "bmp" => GUID::from_u128(0x557cf400_1a04_11d3_9a73_0000f81ef32e),
        _ => GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e),
    };
    let mut bgra = img.to_bgra();
    unsafe {
        let Some(c) = Canvas::on(&mut bgra, img.w as i32, img.h as i32) else {
            return Err(std::io::Error::other("GDI+"));
        };
        let p = crate::win::wide_os(path.as_os_str());
        let st = GdipSaveImageToFile(c.bmp as *mut GpImage, p.as_ptr(), &clsid, null());
        if st != 0 {
            return Err(std::io::Error::other(format!("GDI+ {st}")));
        }
    }
    std::result::Result::Ok(())
}
