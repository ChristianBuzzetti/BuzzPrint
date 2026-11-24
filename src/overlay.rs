//! Captura "todo en uno" sobre la pantalla congelada: arrastrar una región o hacer clic en la ventana
//! resaltada. Enter = ventana, F = pantalla completa, Esc o clic derecho = cancelar. Con lupa, cruz y
//! coordenadas, como la versión .NET.
//!
//! GDI con doble buffer; todos los bitmaps se crean al abrir y se liberan al cerrar.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE, VK_F, VK_RETURN};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::capture::{dib_info, Frame};
use crate::i18n::t;
use crate::win::{point_from_lparam, wide};

pub struct Pick {
    /// Rectángulo en coordenadas del frame.
    pub rect: RECT,
    /// "Región", "Pantalla completa", "Ventana" o el título de la ventana.
    pub source: String,
}

struct WinInfo {
    hwnd: HWND,
    rect: RECT, // pantalla
}

/// Ventanas visibles de nivel superior, de la de más arriba a la de más abajo, con su borde visual.
fn visible_windows() -> Vec<WinInfo> {
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> BOOL {
        let list = &mut *(lp as *mut Vec<WinInfo>);
        if IsWindowVisible(h) == 0 || IsIconic(h) != 0 {
            return 1;
        }
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(h, DWMWA_CLOAKED as u32, &mut cloaked as *mut u32 as *mut c_void, 4) == 0 && cloaked != 0 {
            return 1;
        }
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TRANSPARENT != 0 {
            return 1;
        }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(h, cls.as_mut_ptr(), 64) as usize;
        let c = String::from_utf16_lossy(&cls[..n]);
        if c != "Progman" && c != "WorkerW" && ex & WS_EX_TOOLWINDOW != 0 && GetWindowTextLengthW(h) == 0 {
            return 1;
        }
        let mut r: RECT = zeroed();
        if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS as u32, &mut r as *mut RECT as *mut c_void, size_of::<RECT>() as u32) != 0
            && GetWindowRect(h, &mut r) == 0
        {
            return 1;
        }
        if r.right - r.left < 8 || r.bottom - r.top < 8 {
            return 1;
        }
        list.push(WinInfo { hwnd: h, rect: r });
        1
    }
    let mut list: Vec<WinInfo> = Vec::new();
    unsafe {
        EnumWindows(Some(cb), &mut list as *mut Vec<WinInfo> as LPARAM);
    }
    list
}

pub fn window_title(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h);
        if n <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

fn monitors() -> Vec<RECT> {
    unsafe extern "system" fn cb(_: HMONITOR, _: HDC, r: *mut RECT, lp: LPARAM) -> BOOL {
        (*(lp as *mut Vec<RECT>)).push(*r);
        1
    }
    let mut v: Vec<RECT> = Vec::new();
    unsafe {
        EnumDisplayMonitors(null_mut(), null(), Some(cb), &mut v as *mut Vec<RECT> as LPARAM);
    }
    v
}

const MAG: i32 = 128;
const ZOOM: i32 = 8;

struct Ov {
    w: i32,
    h: i32,
    ox: i32,
    oy: i32,
    frame_dc: HDC,
    frame_old: HGDIOBJ,
    frame_bits: *const u8,
    dim_dc: HDC,
    dim_bmp: HBITMAP,
    dim_old: HGDIOBJ,
    back_dc: HDC,
    back_bmp: HBITMAP,
    back_old: HGDIOBJ,
    pen: HPEN,
    dash: HPEN,
    font: HFONT,
    pill: HBRUSH,
    accent: u32,
    pad: i32,
    windows: Vec<WinInfo>,
    monitors: Vec<RECT>,
    mouse: POINT,
    start: POINT,
    dragging: bool,
    moved: bool,
    sel: RECT,
    hover: RECT,
    hover_hwnd: HWND,
    record: bool,
    result: Option<Pick>,
    done: bool,
}

impl Drop for Ov {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.frame_dc, self.frame_old);
            DeleteDC(self.frame_dc);
            SelectObject(self.dim_dc, self.dim_old);
            DeleteDC(self.dim_dc);
            DeleteObject(self.dim_bmp);
            SelectObject(self.back_dc, self.back_old);
            DeleteDC(self.back_dc);
            DeleteObject(self.back_bmp);
            DeleteObject(self.pen);
            DeleteObject(self.dash);
            DeleteObject(self.font);
            DeleteObject(self.pill);
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

fn empty(r: &RECT) -> bool {
    r.right <= r.left || r.bottom <= r.top
}

fn contains(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

fn intersect(a: &RECT, b: &RECT) -> RECT {
    RECT { left: a.left.max(b.left), top: a.top.max(b.top), right: a.right.min(b.right), bottom: a.bottom.min(b.bottom) }
}

impl Ov {
    fn to_local(&self, r: &RECT) -> RECT {
        RECT { left: r.left - self.ox, top: r.top - self.oy, right: r.right - self.ox, bottom: r.bottom - self.oy }
    }

    fn update_hover(&mut self) {
        let sp = POINT { x: self.mouse.x + self.ox, y: self.mouse.y + self.oy };
        let mut found = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let mut hw: HWND = null_mut();
        if let Some(w) = self.windows.iter().find(|w| contains(&w.rect, sp)) {
            found = w.rect;
            hw = w.hwnd;
        } else if let Some(m) = self.monitors.iter().find(|m| contains(m, sp)) {
            found = *m;
        }
        let local = intersect(&self.to_local(&found), &RECT { left: 0, top: 0, right: self.w, bottom: self.h });
        self.hover = local;
        self.hover_hwnd = hw;
    }

    fn window_source(&self) -> String {
        let t = if self.hover_hwnd.is_null() { String::new() } else { window_title(self.hover_hwnd) };
        if t.is_empty() { t_("Ventana") } else { t }
    }
}

fn t_(s: &str) -> String {
    t(s)
}

/// Muestra el selector y bloquea (con su propio bucle de mensajes) hasta que se elige o cancela.
/// `record`: modo "elegir qué grabar" (otro rótulo; F toma la pantalla completa).
pub fn select(frame: &Frame, record: bool) -> Option<Pick> {
    unsafe {
        let hinst = GetModuleHandleW(null());
        let class = wide("BuzzPrintRsOverlay");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: LoadCursorW(null_mut(), IDC_CROSS),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassExW(&wc);

        let (w, h) = (frame.w, frame.h);
        let accent = crate::gui::colors().accent;
        let accent = rgb(accent.r(), accent.g(), accent.b());
        let header = crate::gui::colors().header;
        let screen = GetDC(null_mut());
        let mut ov = Box::new(Ov {
            w,
            h,
            ox: frame.x,
            oy: frame.y,
            frame_dc: CreateCompatibleDC(screen),
            frame_old: null_mut(),
            frame_bits: frame.pixels().as_ptr(),
            dim_dc: CreateCompatibleDC(screen),
            dim_bmp: null_mut(),
            dim_old: null_mut(),
            back_dc: CreateCompatibleDC(screen),
            back_bmp: CreateCompatibleBitmap(screen, w, h),
            back_old: null_mut(),
            pen: null_mut(),
            dash: CreatePen(PS_DOT, 1, rgb(255, 255, 255)),
            font: null_mut(),
            pill: CreateSolidBrush(rgb(header.r(), header.g(), header.b())),
            accent,
            pad: 8,
            windows: visible_windows(),
            monitors: monitors(),
            mouse: POINT { x: 0, y: 0 },
            start: POINT { x: 0, y: 0 },
            dragging: false,
            moved: false,
            sel: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            hover: RECT { left: 0, top: 0, right: 0, bottom: 0 },
            hover_hwnd: null_mut(),
            record,
            result: None,
            done: false,
        });

        // Copia oscurecida (≈ 43 % de negro encima, como .NET).
        let bmi = dib_info(w, h);
        let mut bits = null_mut();
        ov.dim_bmp = CreateDIBSection(screen, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        ReleaseDC(null_mut(), screen);
        if ov.dim_bmp.is_null() || bits.is_null() || ov.back_bmp.is_null() {
            return None;
        }
        let dim = std::slice::from_raw_parts_mut(bits as *mut u8, frame.pixels().len());
        for (d, s) in dim.chunks_exact_mut(4).zip(frame.pixels().chunks_exact(4)) {
            d[0] = (s[0] as u16 * 145 / 255) as u8;
            d[1] = (s[1] as u16 * 145 / 255) as u8;
            d[2] = (s[2] as u16 * 145 / 255) as u8;
        }
        ov.frame_old = SelectObject(ov.frame_dc, frame.dib);
        ov.dim_old = SelectObject(ov.dim_dc, ov.dim_bmp);
        ov.back_old = SelectObject(ov.back_dc, ov.back_bmp);

        let p: *mut Ov = &mut *ov;
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("BuzzPrint").as_ptr(),
            WS_POPUP,
            frame.x,
            frame.y,
            w,
            h,
            null_mut(),
            null_mut(),
            hinst,
            p as *const c_void,
        );
        if hwnd.is_null() {
            return None;
        }
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let px = |v: i32| v * dpi / 96;
        ov.pen = CreatePen(PS_SOLID, px(2).max(2), accent);
        ov.font = CreateFontW(-px(13), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, wide("Consolas").as_ptr());
        ov.pad = px(5);
        let mut cur = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cur);
        ov.mouse = POINT { x: cur.x - frame.x, y: cur.y - frame.y };
        ov.update_hover();

        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);

        let mut msg: MSG = zeroed();
        while !(*p).done {
            let r = GetMessageW(&mut msg, null_mut(), 0, 0);
            if r == 0 {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if r == -1 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        DestroyWindow(hwnd);
        ov.result.take()
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = lp as *const CREATESTRUCTW;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ov;
    if p.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let ov = &mut *p;
    let full = RECT { left: 0, top: 0, right: ov.w, bottom: ov.h };
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint(ov, hdc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            ov.mouse = point_from_lparam(lp);
            if ov.dragging {
                if (ov.mouse.x - ov.start.x).abs() > 3 || (ov.mouse.y - ov.start.y).abs() > 3 {
                    ov.moved = true;
                }
                ov.sel = RECT {
                    left: ov.start.x.min(ov.mouse.x),
                    top: ov.start.y.min(ov.mouse.y),
                    right: ov.start.x.max(ov.mouse.x) + 1,
                    bottom: ov.start.y.max(ov.mouse.y) + 1,
                };
            } else {
                ov.update_hover();
            }
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONDOWN => {
            SetCapture(hwnd);
            ov.dragging = true;
            ov.moved = false;
            ov.start = point_from_lparam(lp);
            ov.sel = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            0
        }
        WM_LBUTTONUP => {
            if ov.dragging {
                ReleaseCapture();
                ov.dragging = false;
                let pick = if ov.moved && ov.sel.right - ov.sel.left > 1 && ov.sel.bottom - ov.sel.top > 1 {
                    Pick { rect: intersect(&ov.sel, &full), source: t("Región") }
                } else {
                    Pick { rect: ov.hover, source: ov.window_source() }
                };
                if !empty(&pick.rect) {
                    ov.result = Some(pick);
                }
                ov.done = true;
            }
            0
        }
        WM_RBUTTONDOWN => {
            ov.result = None;
            ov.done = true;
            0
        }
        WM_KEYDOWN => {
            match wp as u16 {
                VK_ESCAPE => {
                    ov.result = None;
                    ov.done = true;
                }
                VK_RETURN if !empty(&ov.hover) => {
                    ov.result = Some(Pick { rect: ov.hover, source: ov.window_source() });
                    ov.done = true;
                }
                VK_F => {
                    ov.result = Some(Pick { rect: full, source: t("Pantalla completa") });
                    ov.done = true;
                }
                _ => {}
            }
            0
        }
        // Si se cambia de app, el selector se cierra solo: nunca queda tapando la pantalla.
        WM_ACTIVATEAPP if wp == 0 => {
            ov.done = true;
            0
        }
        WM_CLOSE => {
            ov.done = true;
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn paint(ov: &Ov, hdc: HDC) {
    let b = ov.back_dc;
    BitBlt(b, 0, 0, ov.w, ov.h, ov.dim_dc, 0, 0, SRCCOPY);
    let active = if ov.dragging && ov.moved { ov.sel } else { ov.hover };
    if !empty(&active) {
        BitBlt(b, active.left, active.top, active.right - active.left, active.bottom - active.top, ov.frame_dc, active.left, active.top, SRCCOPY);
    }

    // Cruz punteada.
    SelectObject(b, ov.dash);
    SetBkMode(b, TRANSPARENT as _);
    MoveToEx(b, 0, ov.mouse.y, null_mut());
    LineTo(b, ov.w, ov.mouse.y);
    MoveToEx(b, ov.mouse.x, 0, null_mut());
    LineTo(b, ov.mouse.x, ov.h);

    SelectObject(b, ov.font);
    SetTextColor(b, rgb(235, 238, 245));
    if !empty(&active) {
        SelectObject(b, ov.pen);
        SelectObject(b, GetStockObject(NULL_BRUSH));
        Rectangle(b, active.left, active.top, active.right, active.bottom);
        let (aw, ah) = (active.right - active.left, active.bottom - active.top);
        let mut txt = String::new();
        if ov.record {
            txt.push_str("● REC  ");
        }
        txt.push_str(&format!("{aw} × {ah}"));
        if !(ov.dragging && ov.moved) {
            txt.push_str(&format!("   {}", t("clic = ventana")));
            if ov.record {
                txt.push_str(&format!("  ·  F = {}", t("pantalla")));
            }
        }
        let tw: Vec<u16> = txt.encode_utf16().collect();
        let mut sz = SIZE { cx: 0, cy: 0 };
        GetTextExtentPoint32W(b, tw.as_ptr(), tw.len() as i32, &mut sz);
        let lh = sz.cy + ov.pad;
        let mut y = active.top - lh - 2;
        if y < 0 {
            y = active.top + 4;
        }
        let rc = RECT { left: active.left, top: y, right: active.left + sz.cx + ov.pad * 2, bottom: y + lh };
        FillRect(b, &rc, ov.pill);
        TextOutW(b, active.left + ov.pad, y + ov.pad / 2, tw.as_ptr(), tw.len() as i32);
    }
    magnifier(ov, b);
    BitBlt(hdc, 0, 0, ov.w, ov.h, b, 0, 0, SRCCOPY);
}

unsafe fn magnifier(ov: &Ov, b: HDC) {
    let info_h = 22;
    let mut x = ov.mouse.x + 24;
    let mut y = ov.mouse.y + 24;
    if x + MAG > ov.w {
        x = ov.mouse.x - 24 - MAG;
    }
    if y + MAG + info_h > ov.h {
        y = ov.mouse.y - 24 - MAG - info_h;
    }
    let n = MAG / ZOOM;
    SetStretchBltMode(b, COLORONCOLOR as _);
    StretchBlt(b, x, y, MAG, MAG, ov.frame_dc, ov.mouse.x - n / 2, ov.mouse.y - n / 2, n, n, SRCCOPY);
    let frame = CreatePen(PS_SOLID, 1, ov.accent);
    let old = SelectObject(b, frame);
    SelectObject(b, GetStockObject(NULL_BRUSH));
    let cx = x + (n / 2) * ZOOM;
    let cy = y + (n / 2) * ZOOM;
    Rectangle(b, cx, cy, cx + ZOOM + 1, cy + ZOOM + 1);
    Rectangle(b, x, y, x + MAG, y + MAG);
    SelectObject(b, old);
    DeleteObject(frame);

    // Coordenadas de pantalla y color del píxel bajo el cursor.
    let (mx, my) = (ov.mouse.x.clamp(0, ov.w - 1), ov.mouse.y.clamp(0, ov.h - 1));
    let o = (my as usize * ov.w as usize + mx as usize) * 4;
    let px = std::slice::from_raw_parts(ov.frame_bits.add(o), 3);
    let info = format!("{},{}  #{:02X}{:02X}{:02X}", ov.mouse.x + ov.ox, ov.mouse.y + ov.oy, px[2], px[1], px[0]);
    let rc = RECT { left: x, top: y + MAG, right: x + MAG, bottom: y + MAG + info_h };
    FillRect(b, &rc, ov.pill);
    let tw: Vec<u16> = info.encode_utf16().collect();
    let mut rc2 = rc;
    DrawTextW(b, tw.as_ptr(), tw.len() as i32, &mut rc2, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}
