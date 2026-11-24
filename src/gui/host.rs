//! Host de egui sobre ventanas Win32 con OpenGL (WGL), sin winit ni frameworks.
//!
//! Cada ventana crea su contexto OpenGL al abrirse y lo destruye al cerrarse: con todas las ventanas
//! cerradas el proceso vuelve a pesar lo mismo que en la bandeja. Solo se repinta ante eventos o cuando
//! egui pide una animación; no hay bucle de render permanente.

use std::any::Any;
use std::cell::RefCell;
use std::ffi::{c_void, CString};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, ViewportId};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetDC, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, ReleaseDC, ScreenToClient, HDC,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
};
use windows_sys::Win32::Graphics::OpenGL::{
    wglCreateContext, wglDeleteContext, wglGetProcAddress, wglMakeCurrent, ChoosePixelFormat, SetPixelFormat, SwapBuffers,
    HGLRC, PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW, PFD_MAIN_PLANE, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR,
};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryA};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::win::wide;

pub const WM_WAKE: u32 = WM_APP + 40;
/// Aviso del despertador (cursor o repintado diferido): pinta en el acto.
const WM_TICK: u32 = WM_APP + 41;
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_REPAINT: usize = 1;
/// Mientras una vista dibuja el logo que sigue al cursor, se mira la posición del mouse cada 40 ms y se
/// repinta solo si se movió. Sin logo visible (o con la ventana minimizada) el temporizador no existe.
const CF_UNICODETEXT: u32 = 13;

/// Lo que una ventana puede pedir al host durante un frame.
pub struct WinCtl {
    pub hwnd: HWND,
    pub close: bool,
    pub title: Option<String>,
    pub ppp: f32,
}

pub trait View: Any {
    fn kind(&self) -> &'static str;
    fn title(&self) -> String;
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl);
    /// Se pidió cerrar (X, Alt+F4). `true` = cerrar ya.
    fn on_close_request(&mut self) -> bool {
        true
    }
    /// La ventana perdió el foco (para menús emergentes que se cierran solos).
    fn on_deactivate(&mut self) -> bool {
        false
    }
    /// La ventana se minimizó y va a soltar su contexto OpenGL: liberar `TextureHandle`s.
    fn on_suspend(&mut self) {}
    fn as_any(&mut self) -> &mut dyn Any;
}

pub struct Options {
    pub width: f32,
    pub height: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub popup: bool,
    pub topmost: bool,
    /// Posición en píxeles de pantalla (esquina superior izquierda). `None` = centrada en el monitor del cursor.
    pub at: Option<(i32, i32)>,
    /// Diálogo de tamaño fijo (Acerca de, avisos, progreso): se redimensiona solo desde las esquinas,
    /// manteniendo la proporción, y todo el contenido se agranda o achica en conjunto (nunca se corta).
    pub scale: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { width: 1100.0, height: 720.0, min_width: 480.0, min_height: 320.0, popup: false, topmost: false, at: None, scale: false }
    }
}

struct GlState {
    hglrc: HGLRC,
    gl: Arc<glow::Context>,
    painter: egui_glow::Painter,
    ctx: egui::Context,
    theme_rev: u64,
}

struct Win {
    hwnd: HWND,
    hdc: HDC,
    gl: Option<GlState>,
    events: Vec<Event>,
    view: Box<dyn View>,
    size: (u32, u32),
    ppp: f32,
    start: Instant,
    cursor: egui::CursorIcon,
    painting: bool,
    tracking: bool,
    focused: bool,
    surrogate: Option<u16>,
    min: (f32, f32),
    last_cursor: (i32, i32),
    watching: bool,
    last_shot: Option<Instant>,
    /// Cuándo empezó el último cuadro (tope de 60 cuadros por segundo para las animaciones).
    frame_start: Instant,
    /// Veces seguidas que la ventana creció para que entre el contenido (tope contra contenidos que
    /// crecen con la ventana).
    fit_grows: u8,
    /// Tamaño y sobrante del último ajuste (para detectar contenido elástico) y ejes que no se ajustan.
    fit_last: Option<(f32, f32, f32, f32)>,
    fit_elastic: (bool, bool),
    /// Diálogos que escalan: tamaño natural del contenido (en puntos) y el factor actual.
    scale_base: Option<(f32, f32)>,
    zoom: f32,
}

impl Win {
    /// Píxeles por punto efectivos: el DPI del monitor por el factor de escala del diálogo.
    fn px(&self) -> f32 {
        self.ppp * self.zoom
    }
}

/// Escala mínima y máxima de los diálogos que se agrandan o achican en conjunto.
const SCALE_MIN: f32 = 0.6;
const SCALE_MAX: f32 = 2.0;

thread_local! {
    static OPEN: RefCell<Vec<(HWND, &'static str)>> = const { RefCell::new(Vec::new()) };
    static CURSOR: std::cell::Cell<Option<Pos2>> = const { std::cell::Cell::new(None) };
    static WANT_CURSOR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static ICONS: std::cell::Cell<(HICON, HICON)> = const { std::cell::Cell::new((null_mut(), null_mut())) };
    /// Alguna ventana se está arrastrando o cambiando de tamaño (entre WM_ENTERSIZEMOVE y WM_EXITSIZEMOVE).
    static MOVING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Íconos de las ventanas (grande y chico) con el logo actual; se crean con la primera ventana.
fn window_icons() -> (HICON, HICON) {
    let have = ICONS.with(|i| i.get());
    if !have.0.is_null() {
        return have;
    }
    unsafe {
        let dpi = GetDpiForSystem().max(96);
        let big = super::markicon::current_icon(GetSystemMetricsForDpi(SM_CXICON, dpi));
        let small = super::markicon::current_icon(GetSystemMetricsForDpi(SM_CXSMICON, dpi));
        ICONS.with(|i| i.set((big, small)));
        (big, small)
    }
}

unsafe fn apply_icons(hwnd: HWND) {
    let (big, small) = window_icons();
    if !big.is_null() {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big as isize);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small as isize);
    }
}

/// El logo cambió: íconos nuevos para todas las ventanas abiertas.
pub fn refresh_icons() {
    let old = ICONS.with(|i| i.replace((null_mut(), null_mut())));
    for h in open_kinds() {
        unsafe { apply_icons(h) };
    }
    unsafe {
        for i in [old.0, old.1] {
            if !i.is_null() {
                DestroyIcon(i);
            }
        }
    }
}

/// Posición del cursor en puntos de la ventana que se está pintando, aunque esté fuera de ella
/// (los ojos del logo). Llamarla pide que la ventana siga mirando el cursor mientras se dibuje.
pub fn global_cursor() -> Option<Pos2> {
    WANT_CURSOR.with(|w| w.set(true));
    CURSOR.with(|c| c.get())
}

// ------------------------------------------------------------------ despertador

/// Despierta ventanas desde un hilo aparte con un mensaje (WM_WAKE). Antes se usaban temporizadores de
/// Windows, pero WM_TIMER es el mensaje de menor prioridad: con otra ventana ocupada (Boberto animándose en
/// la biblioteca) se atrasaba o se salteaba y los ojos del logo dejaban de seguir al cursor fuera de la ventana.
mod waker {
    use std::collections::HashMap;
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible, PostMessageW};

    #[derive(Default)]
    struct State {
        /// Por ventana: si mira el cursor y cuándo repintarla.
        entries: HashMap<isize, (bool, Option<Instant>)>,
        last: (i32, i32),
    }

    fn shared() -> &'static (Mutex<State>, Condvar) {
        static S: OnceLock<(Mutex<State>, Condvar)> = OnceLock::new();
        S.get_or_init(|| {
            std::thread::Builder::new().name("buzzprint-waker".into()).spawn(run).ok();
            (Mutex::new(State::default()), Condvar::new())
        })
    }

    /// `watch`: avisar cuando se mueva el cursor; `at`: avisar a esa hora (repintado pedido por egui).
    pub fn set(hwnd: HWND, watch: bool, at: Option<Instant>) {
        let (m, cv) = shared();
        let mut g = m.lock().unwrap();
        let e = g.entries.entry(hwnd as isize).or_insert((false, None));
        e.0 = watch;
        if let Some(t) = at {
            e.1 = Some(e.1.map_or(t, |x| x.min(t)));
        }
        if !e.0 && e.1.is_none() {
            g.entries.remove(&(hwnd as isize));
        }
        cv.notify_one();
    }

    pub fn forget(hwnd: HWND) {
        let (m, _) = shared();
        m.lock().unwrap().entries.remove(&(hwnd as isize));
    }

    fn run() {
        // Si el hilo arranca antes de que termine la inicialización, `shared()` espera a que termine.
        let (m, cv) = shared();
        let mut g = m.lock().unwrap();
        // Con el reloj de Windows por defecto (15,6 ms) una espera de 9 ms dura 15 o 31: las animaciones
        // (scroll suave, ojos del logo, arrastrar la ventana) salían a tirones, a unos 30 cuadros por segundo.
        // Mientras haya algo animándose se pide resolución de 1 ms; en reposo se devuelve.
        let mut fine = false;
        let precise = |on: bool, fine: &mut bool| {
            if on != *fine {
                *fine = on;
                unsafe {
                    if on {
                        windows_sys::Win32::Media::timeBeginPeriod(1);
                    } else {
                        windows_sys::Win32::Media::timeEndPeriod(1);
                    }
                }
            }
        };
        loop {
            if g.entries.is_empty() {
                precise(false, &mut fine);
                g = cv.wait(g).unwrap();
                continue;
            }
            let now = Instant::now();
            let mut pt = POINT { x: 0, y: 0 };
            let moved = unsafe { super::cursor_pos(&mut pt) } && (pt.x, pt.y) != g.last;
            if moved {
                g.last = (pt.x, pt.y);
            }
            let mut next: Option<Instant> = None;
            let mut watching = false;
            for (h, (watch, at)) in g.entries.iter_mut() {
                let hwnd = *h as HWND;
                // Minimizada u oculta: apaga tanto el seguimiento del cursor como cualquier repintado diferido
                // (si no, `at` seguía disparando WM_TICK y quemando ~2-3 % de CPU + timeBeginPeriod(1) en reposo).
                if unsafe { IsIconic(hwnd) != 0 || IsWindowVisible(hwnd) == 0 } {
                    *watch = false;
                    *at = None;
                    continue;
                }
                let mut wake = false;
                if *watch {
                    watching = true;
                    wake |= moved;
                }
                if let Some(t) = *at {
                    if t <= now {
                        wake = true;
                        *at = None;
                    } else {
                        next = Some(next.map_or(t, |n| n.min(t)));
                    }
                }
                if wake {
                    unsafe {
                        PostMessageW(hwnd, super::WM_TICK, 0, 0);
                    }
                }
            }
            g.entries.retain(|_, (w, at)| *w || at.is_some());
            let mut wait = next.map(|t| t.saturating_duration_since(now)).unwrap_or(Duration::from_secs(3600));
            if watching {
                wait = wait.min(Duration::from_millis(16));
            }
            precise(wait < Duration::from_millis(100), &mut fine);
            g = cv.wait_timeout(g, wait.max(Duration::from_millis(1))).unwrap().0;
        }
    }
}

/// Posición del cursor en la pantalla. Diagnóstico: con `BUZZPRINT_FAKE_CURSOR=<archivo>` se lee "x,y" de
/// ese archivo (para probar la mirada del logo sin mover el mouse de nadie).
unsafe fn cursor_pos(pt: &mut POINT) -> bool {
    if let Some(f) = std::env::var_os("BUZZPRINT_FAKE_CURSOR") {
        if let Ok(t) = std::fs::read_to_string(f) {
            let v: Vec<i32> = t.trim().split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() == 2 {
                (pt.x, pt.y) = (v[0], v[1]);
                return true;
            }
        }
    }
    GetCursorPos(pt) != 0
}

fn class_name() -> Vec<u16> {
    wide("BuzzPrintRsWindow")
}

/// Crea contexto WGL + glow + painter + egui::Context sobre un HDC cuyo formato de píxel ya fue fijado.
/// (Nunca vuelve a llamar a `SetPixelFormat`: Win32 solo lo permite una vez por HWND.)
unsafe fn create_gl(hdc: HDC) -> Option<GlState> {
    let hglrc = wglCreateContext(hdc);
    if hglrc.is_null() || wglMakeCurrent(hdc, hglrc) == 0 {
        if !hglrc.is_null() {
            wglDeleteContext(hglrc);
        }
        return None;
    }
    let opengl32 = LoadLibraryA(c"opengl32.dll".as_ptr() as _);
    let gl = Arc::new(glow::Context::from_loader_function(|name| {
        let c = CString::new(name).unwrap();
        let p = wglGetProcAddress(c.as_ptr() as _).map(|f| f as usize).unwrap_or(0);
        if p > 3 && p != usize::MAX {
            return p as *const c_void;
        }
        GetProcAddress(opengl32, c.as_ptr() as _).map(|f| f as *const c_void).unwrap_or(null())
    }));
    let painter = match egui_glow::Painter::new(gl.clone(), "", None, false) {
        Ok(p) => p,
        Err(_) => {
            wglMakeCurrent(null_mut(), null_mut());
            wglDeleteContext(hglrc);
            return None;
        }
    };
    let ctx = egui::Context::default();
    let theme_rev = crate::gui::theme_rev();
    crate::gui::style_context(&ctx);
    Some(GlState { hglrc, gl, painter, ctx, theme_rev })
}

/// Suelta el contexto OpenGL, el painter y las texturas de la vista (al minimizar o cerrar).
unsafe fn destroy_gl(w: &mut Win) {
    waker::forget(w.hwnd);
    w.watching = false;
    w.events.clear();
    if let Some(mut g) = w.gl.take() {
        w.view.on_suspend();
        wglMakeCurrent(w.hdc, g.hglrc);
        g.painter.destroy();
        wglMakeCurrent(null_mut(), null_mut());
        wglDeleteContext(g.hglrc);
    }
}

/// Abre una ventana con su vista. Devuelve el HWND o `None` si no hay OpenGL disponible.
pub fn open(view: Box<dyn View>, opts: Options) -> Option<HWND> {
    unsafe {
        let hinst = GetModuleHandleW(null());
        let class = class_name();
        let icon = LoadImageW(hinst, 1usize as *const u16, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE) as HICON;
        let small = LoadImageW(hinst, 1usize as *const u16, IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), 0) as HICON;
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_OWNDC,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hIcon: icon,
            hIconSm: small,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassExW(&wc);

        let dpi = GetDpiForSystem().max(96) as f32;
        let scale = dpi / 96.0;
        let (w, h) = ((opts.width * scale) as i32, (opts.height * scale) as i32);
        let (x, y) = match opts.at {
            Some(p) => p,
            None => {
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
                let mut mi: MONITORINFO = zeroed();
                mi.cbSize = size_of::<MONITORINFO>() as u32;
                GetMonitorInfoW(mon, &mut mi);
                let wa = mi.rcWork;
                (wa.left + ((wa.right - wa.left) - w).max(0) / 2, wa.top + ((wa.bottom - wa.top) - h).max(0) / 2)
            }
        };
        let (style, ex) = if opts.popup {
            (WS_POPUP, WS_EX_TOOLWINDOW | if opts.topmost { WS_EX_TOPMOST } else { 0 })
        } else {
            (if opts.scale { WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX } else { WS_OVERLAPPEDWINDOW }, if opts.topmost { WS_EX_TOPMOST } else { 0 })
        };
        let title = wide(&view.title());
        let hwnd = CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), style, x, y, w, h, null_mut(), null_mut(), hinst, null());
        if hwnd.is_null() {
            return None;
        }
        let dark: i32 = crate::gui::colors().dark as i32;
        DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const c_void, 4);
        apply_icons(hwnd);

        let hdc = GetDC(hwnd);
        let pfd = PIXELFORMATDESCRIPTOR {
            nSize: size_of::<PIXELFORMATDESCRIPTOR>() as u16,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
            iPixelType: PFD_TYPE_RGBA as _,
            cColorBits: 32,
            cAlphaBits: 8,
            iLayerType: PFD_MAIN_PLANE as _,
            ..zeroed()
        };
        let fmt = ChoosePixelFormat(hdc, &pfd);
        let gl_state = if fmt != 0 && SetPixelFormat(hdc, fmt, &pfd) != 0 { create_gl(hdc) } else { None };
        let Some(gl_state) = gl_state else {
            ReleaseDC(hwnd, hdc);
            DestroyWindow(hwnd);
            return None;
        };

        let mut rc: RECT = zeroed();
        GetClientRect(hwnd, &mut rc);
        let dpi_ppp = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        let kind = view.kind();
        let win = Box::new(Win {
            hwnd,
            hdc,
            gl: Some(gl_state),
            events: Vec::new(),
            view,
            size: ((rc.right - rc.left).max(1) as u32, (rc.bottom - rc.top).max(1) as u32),
            ppp: GetDpiForWindow(hwnd).max(96) as f32 / 96.0,
            start: Instant::now(),
            cursor: egui::CursorIcon::Default,
            painting: false,
            last_shot: None,
            frame_start: Instant::now(),
            tracking: false,
            focused: true,
            surrogate: None,
            min: (opts.min_width, opts.min_height),
            last_cursor: (i32::MIN, 0),
            watching: false,
            fit_grows: 0,
            fit_last: None,
            fit_elastic: (false, false),
            scale_base: opts.scale.then(|| ((rc.right - rc.left).max(1) as f32 / dpi_ppp, (rc.bottom - rc.top).max(1) as f32 / dpi_ppp)),
            zoom: 1.0,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(win) as isize);
        OPEN.with(|o| o.borrow_mut().push((hwnd, kind)));
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        Some(hwnd)
    }
}

/// Ventana abierta de un tipo ("library", "editor"…), si hay.
pub fn find(kind: &str) -> Option<HWND> {
    OPEN.with(|o| o.borrow().iter().find(|(_, k)| *k == kind).map(|(h, _)| *h))
}

pub fn open_kinds() -> Vec<HWND> {
    OPEN.with(|o| o.borrow().iter().map(|(h, _)| *h).collect())
}

/// Ejecuta algo sobre la vista de una ventana abierta (fuera de su frame).
pub fn with_view<T: 'static, R>(hwnd: HWND, f: impl FnOnce(&mut T) -> R) -> Option<R> {
    unsafe {
        let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Win;
        if p.is_null() || (*p).painting {
            return None;
        }
        let r = (*p).view.as_any().downcast_mut::<T>().map(f);
        wake(hwnd);
        r
    }
}

pub fn activate(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
}

pub fn close(hwnd: HWND) {
    unsafe {
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

/// Tras un arrastre modal del shell, Windows se queda con el WM_LBUTTONUP: se le avisa a egui que el botón se soltó.
pub fn release_pointer(hwnd: HWND) {
    unsafe {
        if let Some(w) = win_of(hwnd) {
            w.events.push(Event::PointerButton { pos: Pos2::ZERO, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            w.events.push(Event::PointerGone);
            ReleaseCapture();
            InvalidateRect(hwnd, null(), 0);
        }
    }
}

/// Pide un repintado. Se puede llamar desde cualquier hilo.
pub fn wake(hwnd: HWND) {
    unsafe {
        PostMessageW(hwnd, WM_WAKE, 0, 0);
    }
}

pub fn wake_all() {
    for h in open_kinds() {
        // Una ventana minimizada no tiene contexto OpenGL vivo; al restaurarse tomará el tema y estado nuevos.
        if unsafe { IsIconic(h) == 0 } {
            wake(h);
        }
    }
}

unsafe fn win_of(hwnd: HWND) -> Option<&'static mut Win> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Win;
    (!p.is_null()).then(|| &mut *p)
}

fn modifiers() -> Modifiers {
    let down = |vk: u16| unsafe { GetKeyState(vk as i32) as u16 & 0x8000 != 0 };
    let ctrl = down(VK_CONTROL);
    Modifiers { alt: down(VK_MENU), ctrl, shift: down(VK_SHIFT), mac_cmd: false, command: ctrl }
}

fn pos(w: &Win, lp: LPARAM) -> Pos2 {
    let p = crate::win::point_from_lparam(lp);
    Pos2::new(p.x as f32 / w.px(), p.y as f32 / w.px())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(w) = win_of(hwnd) else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            paint(w);
            0
        }
        WM_ERASEBKGND => 1,
        WM_WAKE => {
            if IsIconic(hwnd) == 0 {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_TICK => {
            if IsIconic(hwnd) != 0 || IsWindowVisible(hwnd) == 0 {
                return 0;
            }
            InvalidateRect(hwnd, null(), 0);
            // Mientras se arrastra o se agranda una ventana no se pinta en el acto: WM_PAINT espera a que
            // Windows atienda el mouse. Si no, las animaciones (ojos del logo, Boberto) se adelantaban a cada
            // movimiento y un arrastre rápido quedaba más de un segundo atrás del cursor.
            if !MOVING.with(|m| m.get()) {
                windows_sys::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
            0
        }
        WM_ENTERSIZEMOVE => {
            MOVING.with(|m| m.set(true));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_EXITSIZEMOVE => {
            MOVING.with(|m| m.set(false));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_TIMER if wp == TIMER_REPAINT => {
            KillTimer(hwnd, TIMER_REPAINT);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_SIZE => {
            if wp == SIZE_MINIMIZED as usize {
                w.size = (0, 0);
                if !w.painting && w.gl.is_some() {
                    destroy_gl(w);
                    crate::app::trim_memory();
                }
                return 0;
            }
            let nw = (lp as u32) & 0xFFFF;
            let nh = (lp as u32 >> 16) & 0xFFFF;
            w.size = (nw, nh);
            if nw > 0 && nh > 0 {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        // Diálogos que escalan: solo las esquinas cambian el tamaño (los bordes no) ...
        WM_NCHITTEST if w.scale_base.is_some() => {
            let hit = DefWindowProcW(hwnd, msg, wp, lp) as u32;
            if matches!(hit, HTLEFT | HTRIGHT | HTTOP | HTBOTTOM) {
                HTBORDER as LRESULT
            } else {
                hit as LRESULT
            }
        }
        // ... y siempre con la proporción natural: ancho y alto crecen o se achican juntos.
        WM_SIZING if w.scale_base.is_some() => {
            let (bw, bh) = w.scale_base.unwrap();
            let r = &mut *(lp as *mut RECT);
            let (ncw, nch) = nonclient(hwnd);
            let (bpw, bph) = (bw * w.ppp, bh * w.ppp);
            let cw = (r.right - r.left - ncw) as f32;
            let ch = (r.bottom - r.top - nch) as f32;
            let s = ((cw / bpw + ch / bph) / 2.0).clamp(SCALE_MIN, SCALE_MAX);
            let (nw, nh) = ((bpw * s).round() as i32 + ncw, (bph * s).round() as i32 + nch);
            let edge = wp as u32;
            if matches!(edge, WMSZ_LEFT | WMSZ_TOPLEFT | WMSZ_BOTTOMLEFT) {
                r.left = r.right - nw;
            } else {
                r.right = r.left + nw;
            }
            if matches!(edge, WMSZ_TOP | WMSZ_TOPLEFT | WMSZ_TOPRIGHT) {
                r.top = r.bottom - nh;
            } else {
                r.bottom = r.top + nh;
            }
            1
        }
        WM_GETMINMAXINFO => {
            let mmi = lp as *mut MINMAXINFO;
            if let Some((bw, bh)) = w.scale_base {
                let (ncw, nch) = nonclient(hwnd);
                (*mmi).ptMinTrackSize = POINT { x: (bw * SCALE_MIN * w.ppp) as i32 + ncw, y: (bh * SCALE_MIN * w.ppp) as i32 + nch };
            } else {
                (*mmi).ptMinTrackSize = POINT { x: (w.min.0 * w.ppp) as i32, y: (w.min.1 * w.ppp) as i32 };
            }
            0
        }
        WM_DPICHANGED => {
            w.ppp = ((wp as u32) & 0xFFFF).max(96) as f32 / 96.0;
            let r = &*(lp as *const RECT);
            SetWindowPos(hwnd, null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_SETCURSOR if (lp as u32 & 0xFFFF) == HTCLIENT => {
            SetCursor(LoadCursorW(null_mut(), cursor_id(w.cursor)));
            1
        }
        WM_MOUSEMOVE => {
            if !w.tracking {
                let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                TrackMouseEvent(&mut t);
                w.tracking = true;
            }
            w.events.push(Event::PointerMoved(pos(w, lp)));
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_MOUSELEAVE => {
            w.tracking = false;
            w.events.push(Event::PointerGone);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_LBUTTONDBLCLK => {
            let (button, pressed) = match msg {
                WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => (PointerButton::Primary, true),
                WM_LBUTTONUP => (PointerButton::Primary, false),
                WM_RBUTTONDOWN => (PointerButton::Secondary, true),
                WM_RBUTTONUP => (PointerButton::Secondary, false),
                WM_MBUTTONDOWN => (PointerButton::Middle, true),
                _ => (PointerButton::Middle, false),
            };
            if pressed {
                SetCapture(hwnd);
            } else {
                ReleaseCapture();
            }
            w.events.push(Event::PointerButton { pos: pos(w, lp), button, pressed, modifiers: modifiers() });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let delta = ((wp >> 16) as u16 as i16) as f32 / 120.0;
            let mods = modifiers();
            let v = if msg == WM_MOUSEHWHEEL { egui::vec2(-delta, 0.0) } else if mods.shift { egui::vec2(delta, 0.0) } else { egui::vec2(0.0, delta) };
            // La posición del mensaje viene en coordenadas de pantalla.
            let mut pt = crate::win::point_from_lparam(lp);
            ScreenToClient(hwnd, &mut pt);
            w.events.push(Event::PointerMoved(Pos2::new(pt.x as f32 / w.px(), pt.y as f32 / w.px())));
            w.events.push(Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: v, modifiers: mods });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
            let pressed = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let mods = modifiers();
            if let Some(key) = map_key(wp as u16) {
                if pressed && mods.ctrl && !mods.alt {
                    match key {
                        Key::C => w.events.push(Event::Copy),
                        Key::X => w.events.push(Event::Cut),
                        Key::V => {
                            if let Some(t) = clipboard_text() {
                                w.events.push(Event::Paste(t));
                            }
                        }
                        _ => {}
                    }
                }
                let repeat = pressed && (lp >> 30) & 1 == 1;
                w.events.push(Event::Key { key, physical_key: None, pressed, repeat, modifiers: mods });
            }
            InvalidateRect(hwnd, null(), 0);
            if msg == WM_SYSKEYDOWN && wp as u16 == VK_F4 {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            0
        }
        WM_CHAR => {
            let u = wp as u16;
            if (0xD800..0xDC00).contains(&u) {
                w.surrogate = Some(u);
                return 0;
            }
            let s = if (0xDC00..0xE000).contains(&u) {
                w.surrogate.take().and_then(|hi| String::from_utf16(&[hi, u]).ok())
            } else {
                char::from_u32(u as u32).map(|c| c.to_string())
            };
            if let Some(s) = s {
                if s.chars().all(|c| !c.is_control()) {
                    w.events.push(Event::Text(s));
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_SETFOCUS => {
            w.focused = true;
            w.events.push(Event::WindowFocused(true));
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_KILLFOCUS => {
            w.focused = false;
            w.events.push(Event::WindowFocused(false));
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_ACTIVATE => {
            if (wp & 0xFFFF) as u32 == WA_INACTIVE && !w.painting && w.view.on_deactivate() {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CLOSE => {
            if !w.painting && w.view.on_close_request() {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            OPEN.with(|o| o.borrow_mut().retain(|(h, _)| *h != hwnd));
            let mut b = Box::from_raw(w as *mut Win);
            destroy_gl(&mut b);
            ReleaseDC(hwnd, b.hdc);
            drop(b);
            crate::app::post(crate::app::Cmd::WindowClosed);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Diagnóstico: con `BUZZPRINT_SHOT=<carpeta>` cada ventana se guarda como `<título>.png` leyendo el búfer
/// de OpenGL (sirve aunque la sesión de Windows esté bloqueada; no incluye los WebView2, que son ventanas hijas).
unsafe fn shot(hwnd: HWND, size: (u32, u32), last_shot: &mut Option<Instant>, gl: &glow::Context) {
    use glow::HasContext;
    let Some(dir) = std::env::var_os("BUZZPRINT_SHOT") else { return };
    let now = std::time::Instant::now();
    if last_shot.is_some_and(|t| now.duration_since(t) < Duration::from_millis(400)) {
        // Se vuelve a pintar en un rato para que el último estado también quede guardado.
        SetTimer(hwnd, TIMER_REPAINT, 450, None);
        return;
    }
    *last_shot = Some(now);
    let (pw, ph) = size;
    if pw == 0 || ph == 0 {
        return;
    }
    let mut buf = vec![0u8; pw as usize * ph as usize * 4];
    gl.read_pixels(0, 0, pw as i32, ph as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut buf)));
    let row = pw as usize * 4;
    let mut img = crate::image::Rgba::new(pw, ph);
    for y in 0..ph as usize {
        img.px[y * row..(y + 1) * row].copy_from_slice(&buf[(ph as usize - 1 - y) * row..(ph as usize - y) * row]);
    }
    for p in img.px.chunks_exact_mut(4) {
        p[3] = 255;
    }
    let mut name = [0u16; 256];
    let n = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, name.as_mut_ptr(), 256);
    let title: String = String::from_utf16_lossy(&name[..n.max(0) as usize]).chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let _ = crate::image::save_png(&img, &std::path::Path::new(&dir).join(format!("{}.png", title.trim())));
}

unsafe fn paint(w: &mut Win) {
    if w.painting || IsIconic(w.hwnd) != 0 || IsWindowVisible(w.hwnd) == 0 || w.size.0 == 0 || w.size.1 == 0 {
        return;
    }
    // Al restaurar desde minimizada, recrea el contexto OpenGL, el painter y el contexto de egui.
    if w.gl.is_none() {
        w.gl = create_gl(w.hdc);
        if w.gl.is_none() {
            return;
        }
        let dark: i32 = crate::gui::colors().dark as i32;
        DwmSetWindowAttribute(w.hwnd, 20, &dark as *const i32 as *const c_void, 4);
    }
    let Some(g) = w.gl.as_mut() else {
        return;
    };
    w.painting = true;
    wglMakeCurrent(w.hdc, g.hglrc);

    // Si cambió la apariencia (Personalizar), se vuelve a estilizar el contexto.
    let rev = crate::gui::theme_rev();
    if rev != g.theme_rev {
        g.theme_rev = rev;
        crate::gui::style_context(&g.ctx);
        let dark: i32 = crate::gui::colors().dark as i32;
        DwmSetWindowAttribute(w.hwnd, 20, &dark as *const i32 as *const c_void, 4);
    }

    let (pw, ph) = w.size;
    if let Some((bw, bh)) = w.scale_base {
        // El contenido se dibuja siempre a su tamaño natural y se escala al de la ventana.
        w.zoom = ((pw as f32 / w.ppp) / bw).min((ph as f32 / w.ppp) / bh).clamp(SCALE_MIN * 0.9, SCALE_MAX * 1.1);
    }
    let px = w.ppp * w.zoom;
    // Tope del atlas de texturas: 4096 en el editor (capturas 4K) y 2048 en las demás ventanas
    // (achica el atlas de fuentes de egui de 16384×32 [2 MB] a 2048×64 [0,5 MB]).
    let tex_cap = if w.view.kind() == "editor" { 4096 } else { 2048 };
    let mut raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(pw as f32 / px, ph as f32 / px))),
        time: Some(w.start.elapsed().as_secs_f64()),
        modifiers: modifiers(),
        focused: w.focused,
        max_texture_side: Some(g.painter.max_texture_side().min(tex_cap)),
        events: std::mem::take(&mut w.events),
        ..Default::default()
    };
    raw.viewports.entry(ViewportId::ROOT).or_default().native_pixels_per_point = Some(px);

    {
        let mut pt = POINT { x: 0, y: 0 };
        let cur = if cursor_pos(&mut pt) {
            w.last_cursor = (pt.x, pt.y);
            ScreenToClient(w.hwnd, &mut pt);
            Some(Pos2::new(pt.x as f32 / px, pt.y as f32 / px))
        } else {
            None
        };
        CURSOR.with(|c| c.set(cur));
        WANT_CURSOR.with(|c| c.set(false));
    }
    let frame_start = Instant::now();
    let prev_frame = std::mem::replace(&mut w.frame_start, frame_start);
    let mut ctl = WinCtl { hwnd: w.hwnd, close: false, title: None, ppp: w.ppp };
    let ctx = g.ctx.clone();
    let view = &mut w.view;
    let debug = std::env::var_os("BUZZPRINT_DEBUG").is_some();
    let logged: Vec<String> = if debug { raw.events.iter().map(|e| format!("{e:?}")).collect() } else { Vec::new() };
    let out = ctx.run(raw, |c| view.ui(c, &mut ctl));
    if debug && !logged.is_empty() {
        let clicks = ctx.input(|i| (i.pointer.any_click(), i.pointer.any_pressed(), i.pointer.any_released(), i.pointer.latest_pos()));
        let line = format!("{:.3} {:?} -> click/press/release/pos {:?}\n", w.start.elapsed().as_secs_f64(), logged, clicks);
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("buzzprint-events.log")).and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }

    let used = ctx.used_rect();

    w.cursor = out.platform_output.cursor_icon;
    for cmd in &out.platform_output.commands {
        if let egui::OutputCommand::CopyText(t) = cmd {
            set_clipboard_text(w.hwnd, t);
        }
    }
    let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
    let bg = crate::gui::colors().canvas;
    egui_glow::painter::clear(&g.gl, [pw, ph], [bg.r() as f32 / 255.0, bg.g() as f32 / 255.0, bg.b() as f32 / 255.0, 1.0]);
    g.painter.paint_and_update_textures([pw, ph], out.pixels_per_point, &prims, &out.textures_delta);
    shot(w.hwnd, w.size, &mut w.last_shot, &g.gl);
    SwapBuffers(w.hdc);

    let mut repaint_at: Option<Instant> = None;
    if let Some(vo) = out.viewport_output.get(&ViewportId::ROOT) {
        let d = vo.repaint_delay;
        // egui le resta al pedido de las animaciones lo que tardó el cuadro anterior y suele pedir "ya":
        // el siguiente cuadro va siempre 16 ms después del comienzo de este (máximo ~60 por segundo, parejos).
        // Antes, si el anterior había sido hace más de 16 ms, se pintaba otro en el acto y los cuadros salían
        // de a pares (8 ms + 24 ms): el scroll y los ojos del logo se veían a los tirones.
        // El mouse y el teclado no pasan por acá: siguen pintando en el acto.
        let _ = prev_frame;
        let frame = Duration::from_millis(16);
        if d.is_zero() {
            repaint_at = Some(frame_start + frame);
        } else if d < Duration::from_secs(3600) {
            // Repintado diferido (animaciones del logo): lo pide el despertador, no un WM_TIMER.
            repaint_at = Some((Instant::now() + d).max(frame_start + frame));
        }
    }
    let want = WANT_CURSOR.with(|c| c.replace(false));
    if want || w.watching || repaint_at.is_some() {
        waker::set(w.hwnd, want, repaint_at);
        w.watching = want;
    }
    w.painting = false;
    fit_content(w, used);
    if let Some(t) = ctl.title {
        SetWindowTextW(w.hwnd, wide(&t).as_ptr());
    }
    if ctl.close {
        PostMessageW(w.hwnd, WM_CLOSE, 0, 0);
    }
}

/// Ancho y alto del marco de la ventana (borde y barra de título), en píxeles.
unsafe fn nonclient(hwnd: HWND) -> (i32, i32) {
    let mut wr: RECT = zeroed();
    let mut cr: RECT = zeroed();
    GetWindowRect(hwnd, &mut wr);
    GetClientRect(hwnd, &mut cr);
    ((wr.right - wr.left) - (cr.right - cr.left), (wr.bottom - wr.top) - (cr.bottom - cr.top))
}

/// Ninguna ventana abre cortada: si lo que dibujó la vista no entra (botones o textos afuera del borde), la
/// ventana crece hasta que entre, sin salirse del área de trabajo del monitor. La biblioteca y el editor
/// acomodan su propio contenido (paneles con scroll) y quedan afuera.
unsafe fn fit_content(w: &mut Win, used: Rect) {
    // Solo al abrir (mientras la ventana se acomoda) y nunca mientras el usuario la arrastra: después, el
    // tamaño es del usuario. Los diálogos chicos no se cortan porque escalan (`Options::scale`).
    if matches!(w.view.kind(), "library" | "editor") || IsZoomed(w.hwnd) != 0 || MOVING.with(|m| m.get()) || w.start.elapsed() > Duration::from_millis(1500) {
        return;
    }
    let (sw, sh) = (w.size.0 as f32 / w.px(), w.size.1 as f32 / w.px());
    let (ox, oy) = (used.max.x - sw, used.max.y - sh);
    // Contenido elástico (un layout que llena todo y sobra siempre lo mismo, por el espaciado): si después
    // de crecer sobra igual que antes, agrandar no sirve y ese eje deja de ajustarse.
    if let Some((lw, lh, lox, loy)) = w.fit_last {
        if sw > lw + 0.5 && ox > 1.5 && (ox - lox).abs() < 1.0 {
            w.fit_elastic.0 = true;
        }
        if sh > lh + 0.5 && oy > 1.5 && (oy - loy).abs() < 1.0 {
            w.fit_elastic.1 = true;
        }
    }
    let over_x = ox > 1.5 && !w.fit_elastic.0;
    let over_y = oy > 1.5 && !w.fit_elastic.1;
    if std::env::var_os("BUZZPRINT_FIT_LOG").is_some() && (ox > 1.5 || oy > 1.5) {
        let line = format!("{} screen={sw}x{sh} used={:?} grows={} elastic={:?}\n", w.view.kind(), used, w.fit_grows, w.fit_elastic);
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("buzzprint-fit.log")).and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
    if !over_x && !over_y {
        return;
    }
    // Tope de seguridad por ventana.
    if w.fit_grows >= 40 {
        return;
    }
    w.fit_grows += 1;
    w.fit_last = Some((sw, sh, ox, oy));
    let need = (if over_x { used.max.x.ceil() } else { sw }, if over_y { used.max.y.ceil() } else { sh });
    if let Some((bw, bh)) = w.scale_base {
        // El tamaño natural del diálogo pasa a ser el que hace entrar todo.
        w.scale_base = Some((bw.max(need.0), bh.max(need.1)));
    }
    let mut wr: RECT = zeroed();
    let mut cr: RECT = zeroed();
    GetWindowRect(w.hwnd, &mut wr);
    GetClientRect(w.hwnd, &mut cr);
    let (ncw, nch) = ((wr.right - wr.left) - (cr.right - cr.left), (wr.bottom - wr.top) - (cr.bottom - cr.top));
    let mon = MonitorFromPoint(POINT { x: (wr.left + wr.right) / 2, y: (wr.top + wr.bottom) / 2 }, MONITOR_DEFAULTTONEAREST);
    let mut mi: MONITORINFO = zeroed();
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(mon, &mut mi);
    let wa = mi.rcWork;
    let nw = ((need.0 * w.px()).ceil() as i32 + ncw).min(wa.right - wa.left);
    let nh = ((need.1 * w.px()).ceil() as i32 + nch).min(wa.bottom - wa.top);
    // Crece hacia la derecha y abajo; si se sale del monitor, se corre hacia adentro.
    let x = wr.left.min(wa.right - nw).max(wa.left);
    let y = wr.top.min(wa.bottom - nh).max(wa.top);
    SetWindowPos(w.hwnd, null_mut(), x, y, nw, nh, SWP_NOZORDER | SWP_NOACTIVATE);
    InvalidateRect(w.hwnd, null(), 0);
}

fn cursor_id(c: egui::CursorIcon) -> *const u16 {
    use egui::CursorIcon as C;
    match c {
        C::PointingHand => IDC_HAND,
        C::Text | C::VerticalText => IDC_IBEAM,
        C::Crosshair | C::Cell => IDC_CROSS,
        C::Move | C::Grab | C::Grabbing | C::AllScroll => IDC_SIZEALL,
        C::ResizeHorizontal | C::ResizeColumn | C::ResizeEast | C::ResizeWest => IDC_SIZEWE,
        C::ResizeVertical | C::ResizeRow | C::ResizeNorth | C::ResizeSouth => IDC_SIZENS,
        C::ResizeNeSw | C::ResizeNorthEast | C::ResizeSouthWest => IDC_SIZENESW,
        C::ResizeNwSe | C::ResizeNorthWest | C::ResizeSouthEast => IDC_SIZENWSE,
        C::NotAllowed | C::NoDrop => IDC_NO,
        C::Wait => IDC_WAIT,
        C::Progress => IDC_APPSTARTING,
        C::Help => IDC_HELP,
        _ => IDC_ARROW,
    }
}

fn map_key(vk: u16) -> Option<Key> {
    Some(match vk {
        VK_LEFT => Key::ArrowLeft,
        VK_RIGHT => Key::ArrowRight,
        VK_UP => Key::ArrowUp,
        VK_DOWN => Key::ArrowDown,
        VK_ESCAPE => Key::Escape,
        VK_TAB => Key::Tab,
        VK_BACK => Key::Backspace,
        VK_RETURN => Key::Enter,
        VK_SPACE => Key::Space,
        VK_INSERT => Key::Insert,
        VK_DELETE => Key::Delete,
        VK_HOME => Key::Home,
        VK_END => Key::End,
        VK_PRIOR => Key::PageUp,
        VK_NEXT => Key::PageDown,
        VK_OEM_PLUS | VK_ADD => Key::Plus,
        VK_OEM_MINUS | VK_SUBTRACT => Key::Minus,
        VK_F1 => Key::F1,
        VK_F2 => Key::F2,
        VK_F3 => Key::F3,
        VK_F4 => Key::F4,
        VK_F5 => Key::F5,
        VK_F6 => Key::F6,
        VK_F7 => Key::F7,
        VK_F8 => Key::F8,
        VK_F9 => Key::F9,
        VK_F10 => Key::F10,
        VK_F11 => Key::F11,
        VK_F12 => Key::F12,
        0x30..=0x39 => [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9][(vk - 0x30) as usize],
        0x60..=0x69 => [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9][(vk - 0x60) as usize],
        0x41..=0x5A => {
            const K: [Key; 26] = [
                Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L, Key::M,
                Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
            ];
            K[(vk - 0x41) as usize]
        }
        _ => return None,
    })
}

pub fn clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return None;
        }
        let h = GetClipboardData(CF_UNICODETEXT);
        let mut out = None;
        if !h.is_null() {
            let p = GlobalLock(h) as *const u16;
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)).replace("\r\n", "\n"));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
        out
    }
}

pub fn set_clipboard_text(owner: HWND, text: &str) {
    crate::output::copy_text(owner, text);
}
