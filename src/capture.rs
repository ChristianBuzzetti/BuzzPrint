//! Lectura de la pantalla con GDI: una sola copia, directo a un DIB de 32 bits.

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC,
    GetMonitorInfoW, MonitorFromPoint, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, MONITORINFO, MONITOR_DEFAULTTONEAREST, SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Imagen recortada en memoria, BGRA de arriba hacia abajo.
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

/// Toda la pantalla virtual (todos los monitores) congelada en un DIB.
/// Se libera sola al salir de alcance.
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub dib: HBITMAP,
    bits: *const u8,
}

pub fn dib_info(w: i32, h: i32) -> BITMAPINFO {
    let mut bmi: BITMAPINFO = unsafe { zeroed() };
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h; // de arriba hacia abajo
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as _;
    bmi
}

impl Frame {
    pub fn grab() -> Option<Frame> {
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            if w <= 0 || h <= 0 {
                return None;
            }
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return None;
            }
            let mem = CreateCompatibleDC(screen);
            let bmi = dib_info(w, h);
            let mut bits = null_mut();
            let dib = CreateDIBSection(screen, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            let ok = !mem.is_null() && !dib.is_null() && !bits.is_null();
            if ok {
                let old = SelectObject(mem, dib);
                BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT);
                SelectObject(mem, old);
                GdiFlush();
            }
            DeleteDC(mem);
            ReleaseDC(null_mut(), screen);
            if !ok {
                DeleteObject(dib);
                return None;
            }
            Some(Frame { x, y, w, h, dib, bits: bits as *const u8 })
        }
    }

    pub fn pixels(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bits, (self.w as usize) * (self.h as usize) * 4) }
    }

    /// Rectángulo en coordenadas de pantalla -> coordenadas de este frame.
    pub fn to_local(&self, r: RECT) -> RECT {
        RECT { left: r.left - self.x, top: r.top - self.y, right: r.right - self.x, bottom: r.bottom - self.y }
    }

    /// Recorte como RGBA (el formato común del resto de la app).
    pub fn crop_rgba(&self, r: RECT) -> Option<crate::image::Rgba> {
        let img = self.crop(r)?;
        Some(crate::image::Rgba::from_bgra(img.w, img.h, &img.bgra))
    }

    pub fn full_rect(&self) -> RECT {
        RECT { left: 0, top: 0, right: self.w, bottom: self.h }
    }

    /// Copia un rectángulo (coordenadas del frame), recortado a los bordes.
    pub fn crop(&self, r: RECT) -> Option<Image> {
        let l = r.left.clamp(0, self.w);
        let t = r.top.clamp(0, self.h);
        let rr = r.right.clamp(l, self.w);
        let b = r.bottom.clamp(t, self.h);
        let (cw, ch) = ((rr - l) as usize, (b - t) as usize);
        if cw == 0 || ch == 0 {
            return None;
        }
        let src = self.pixels();
        let stride = self.w as usize * 4;
        let mut out = Vec::with_capacity(cw * ch * 4);
        for y in t as usize..b as usize {
            let s = y * stride + l as usize * 4;
            out.extend_from_slice(&src[s..s + cw * 4]);
        }
        Some(Image { w: cw as u32, h: ch as u32, bgra: out })
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.dib);
        }
    }
}

/// Monitor donde está el cursor, en coordenadas de pantalla.
pub fn cursor_monitor() -> RECT {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) != 0 {
            mi.rcMonitor
        } else {
            RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) }
        }
    }
}
