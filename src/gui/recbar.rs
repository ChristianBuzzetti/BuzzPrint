//! Barra flotante de grabación (estilo DBMON): punto REC que titila, reloj, origen, nivel de audio, Detener y
//! Descartar. Antes de un video, cuenta 3-2-1 (Empezar arranca ya, ✕ o Esc cancelan). La barra y el marco punteado
//! del área no salen en el video: se excluyen de la captura de pantalla.

use std::any::Any;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, Key, RichText, Sense, Stroke, Vec2};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::host::{self, Options, View, WinCtl};
use super::widgets::{self as w};
use super::{bold, colors, mono};
use crate::app::{post, Cmd};
use crate::i18n::t;
use crate::record::{self, Kind, Status};
use crate::win::wide;

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;

enum Phase {
    Countdown(Instant),
    Recording,
    Saving,
}

pub struct RecBar {
    kind: Kind,
    area: RECT,
    source: String,
    phase: Phase,
    frame: HWND,
    start_requested: bool,
}

/// Abre la barra. Video: con cuenta regresiva y marco del área. Audio: empieza enseguida.
pub fn open(kind: Kind, area: RECT, source: String) {
    if host::find("recbar").is_some() {
        return;
    }
    let frame = if kind == Kind::Video { show_frame(area) } else { null_mut() };
    let phase = if kind == Kind::Video { Phase::Countdown(Instant::now()) } else { Phase::Recording };
    let (wd, hd) = (560.0, 74.0);
    let at = unsafe { place(kind, area, wd, hd) };
    let view = RecBar { kind, area, source: source.clone(), phase, frame, start_requested: false };
    if let Some(h) = host::open(Box::new(view), Options { width: wd, height: hd, min_width: 300.0, min_height: 60.0, popup: true, topmost: true, at: Some(at), scale: false }) {
        unsafe { SetWindowDisplayAffinity(h, WDA_EXCLUDEFROMCAPTURE) };
        if kind == Kind::Audio {
            if let Err(e) = record::start(kind, area, source) {
                post(Cmd::Toast(t("No se pudo grabar"), e));
                host::close(h);
            }
        }
    } else {
        destroy_frame(frame);
    }
}

/// Debajo del área grabada; si no entra, arriba; si no, adentro del borde inferior. Solo audio: arriba al centro.
unsafe fn place(kind: Kind, area: RECT, wd: f32, hd: f32) -> (i32, i32) {
    let s = GetDpiForSystem().max(96) as f32 / 96.0;
    let (bw, bh) = ((wd * s) as i32, (hd * s) as i32);
    let pt = if kind == Kind::Video {
        windows_sys::Win32::Foundation::POINT { x: (area.left + area.right) / 2, y: (area.top + area.bottom) / 2 }
    } else {
        let mut p = zeroed();
        GetCursorPos(&mut p);
        p
    };
    let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
    let mut mi: MONITORINFO = zeroed();
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(mon, &mut mi);
    let wa = mi.rcWork;
    let (x, y) = if kind == Kind::Audio {
        (wa.left + (wa.right - wa.left - bw) / 2, wa.top + 8)
    } else {
        let x = (area.left + area.right) / 2 - bw / 2;
        let y = if area.bottom + bh <= wa.bottom {
            area.bottom + 4
        } else if area.top - bh >= wa.top {
            area.top - bh - 4
        } else {
            area.bottom - bh - 12
        };
        (x, y)
    };
    (x.clamp(wa.left, (wa.right - bw).max(wa.left)), y.clamp(wa.top, (wa.bottom - bh).max(wa.top)))
}

impl RecBar {
    fn start_now(&mut self, win: &mut WinCtl) {
        if self.start_requested {
            return;
        }
        self.start_requested = true;
        match record::start(self.kind, self.area, self.source.clone()) {
            Ok(()) => self.phase = Phase::Recording,
            Err(e) => {
                post(Cmd::Toast(t("No se pudo grabar"), e));
                win.close = true;
            }
        }
    }

    pub fn stop(&mut self, keep: bool) {
        if matches!(self.phase, Phase::Saving) {
            return;
        }
        destroy_frame(self.frame);
        self.frame = null_mut();
        self.phase = Phase::Saving;
        post(Cmd::FinishRecording(keep));
    }
}

impl View for RecBar {
    fn kind(&self) -> &'static str {
        "recbar"
    }
    fn title(&self) -> String {
        "BuzzPrint · REC".into()
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn on_close_request(&mut self) -> bool {
        destroy_frame(self.frame);
        self.frame = null_mut();
        true
    }

    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        // Avance del estado.
        let mut label = "REC".to_string();
        let mut clock = "00:00".to_string();
        let mut level = 0.0;
        let (mut sys, mut mic) = (false, false);
        match self.phase {
            Phase::Countdown(t0) => {
                let left = 3 - t0.elapsed().as_secs() as i64;
                if left <= 0 {
                    self.start_now(win);
                } else {
                    label = t("EN");
                    clock = left.to_string();
                }
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Phase::Recording => {
                let status = record::with(|r| r.poll());
                match status {
                    Some(Status::Failed(e)) => {
                        record::stop(false);
                        post(Cmd::Toast(t("No se pudo grabar"), e));
                        destroy_frame(self.frame);
                        self.frame = null_mut();
                        win.close = true;
                    }
                    Some(Status::Starting) => label = "…".into(),
                    Some(Status::Running) => {}
                    None => {}
                }
                if let Some((e, l, s, m)) = record::with(|r| (r.elapsed(), r.level(), r.with_system(), r.with_mic())) {
                    clock = record::clock_text(e);
                    level = l;
                    sys = s;
                    mic = m;
                }
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Phase::Saving => label = t("GUARDANDO"),
        }

        let frame = egui::Frame::default().fill(c.header).stroke(Stroke::new(1.0, c.accent_card_ring)).corner_radius(w::radius()).inner_margin(egui::Margin { left: 12, right: 8, top: 8, bottom: 8 });
        let resp = egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Regla de acento arriba.
            let r = ui.max_rect().expand2(Vec2::new(12.0, 8.0));
            ui.painter().rect_filled(egui::Rect::from_min_size(r.min, Vec2::new(r.width(), 2.0)), 0.0, c.accent);
            ui.horizontal_centered(|ui| {
                // Punto que titila.
                let on = matches!(self.phase, Phase::Recording) && (ctx.input(|i| i.time) * 1.6).fract() < 0.6;
                let (d, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                let red = Color32::from_rgb(0xF4, 0x3F, 0x5E);
                ui.painter().circle_filled(d.center(), 4.5, if on || !matches!(self.phase, Phase::Recording) { red } else { crate::theme::alpha(red, 0.25) });
                ui.label(RichText::new(&label).font(mono(11.0)).strong().color(c.accent));
                ui.add_space(4.0);
                ui.add_sized([62.0, 24.0], egui::Label::new(RichText::new(&clock).font(mono(16.0)).strong().color(c.ink)));
                let (s, _) = ui.allocate_exact_size(Vec2::new(1.0, 24.0), Sense::hover());
                ui.painter().rect_filled(s, 0.0, c.faint_border);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    let caption = if self.kind == Kind::Video {
                        format!("{}×{} · {}", self.area.right - self.area.left, self.area.bottom - self.area.top, self.source)
                    } else {
                        self.source.clone()
                    };
                    ui.add(egui::Label::new(RichText::new(caption).small().color(c.muted)).truncate());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if sys {
                            ui.label(w::icon('\u{E767}', 10.0).color(c.muted)).on_hover_text(t("Audio del sistema"));
                        }
                        if mic {
                            ui.label(w::icon('\u{E720}', 10.0).color(c.muted)).on_hover_text(t("Micrófono"));
                        }
                        if !sys && !mic && matches!(self.phase, Phase::Recording) {
                            ui.label(w::icon('\u{E74F}', 10.0).color(c.muted)).on_hover_text(t("Sin audio"));
                        }
                        let (m, _) = ui.allocate_exact_size(Vec2::new(120.0, 4.0), Sense::hover());
                        ui.painter().rect_filled(m, 0.0, c.canvas);
                        let wl = (level.sqrt() * 120.0).min(120.0);
                        ui.painter().rect_filled(egui::Rect::from_min_size(m.min, Vec2::new(wl, 4.0)), 0.0, c.accent);
                    });
                });
                let saving = matches!(self.phase, Phase::Saving);
                let counting = matches!(self.phase, Phase::Countdown(_));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let discard = ui.add_enabled(!saving, egui::Button::new(w::icon('\u{E711}', 12.0).color(c.ink)).fill(c.raised).stroke(Stroke::new(1.0, c.faint_border)).corner_radius(w::radius()).min_size(Vec2::new(32.0, 32.0)));
                    if discard.on_hover_text(t("Descartar la grabación")).clicked() {
                        if counting {
                            destroy_frame(self.frame);
                            self.frame = null_mut();
                            win.close = true;
                        } else {
                            self.stop(false);
                        }
                    }
                    let (gl, text) = if counting { ('\u{E768}', t("Empezar")) } else { ('\u{E71A}', t("Detener")) };
                    let stop = ui.add_enabled(
                        !saving,
                        egui::Button::new(w::icon_label(gl, &text, 13.0, c.on_accent)).fill(c.accent).stroke(Stroke::new(1.0, c.accent)).corner_radius(w::radius()).min_size(Vec2::new(0.0, 32.0)),
                    );
                    if stop.on_hover_text(format!("{} · {}", t("Detener y guardar"), crate::app::key_text(4))).clicked() {
                        if counting {
                            self.start_now(win);
                        } else {
                            self.stop(true);
                        }
                    }
                });
            });
        });
        // Arrastrar la barra desde el fondo.
        let bg = resp.response.interact(Sense::drag());
        if bg.dragged() {
            let d = bg.drag_delta() * win.ppp;
            unsafe {
                let mut r: RECT = zeroed();
                GetWindowRect(win.hwnd, &mut r);
                SetWindowPos(win.hwnd, HWND_TOPMOST, r.left + d.x as i32, r.top + d.y as i32, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) && matches!(self.phase, Phase::Countdown(_)) {
            destroy_frame(self.frame);
            self.frame = null_mut();
            win.close = true;
        }
        let _ = (bold(1.0), CornerRadius::ZERO);
    }
}

// ------------------------------------------------------------------ marco punteado alrededor del área

unsafe extern "system" fn frame_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_PAINT {
        let mut ps: PAINTSTRUCT = zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc: RECT = zeroed();
        GetClientRect(hwnd, &mut rc);
        let key = CreateSolidBrush(0x00FF00FF);
        FillRect(hdc, &rc, key);
        DeleteObject(key);
        let a = colors().accent;
        let col = a.r() as u32 | (a.g() as u32) << 8 | (a.b() as u32) << 16;
        let lb = LOGBRUSH { lbStyle: BS_SOLID, lbColor: col, lbHatch: 0 };
        let pen = ExtCreatePen((PS_GEOMETRIC as u32 | PS_DASH as u32) as _, 2, &lb, 0, null());
        let old = SelectObject(hdc, pen);
        SelectObject(hdc, GetStockObject(NULL_BRUSH));
        Rectangle(hdc, 1, 1, rc.right - 1, rc.bottom - 1);
        SelectObject(hdc, old);
        DeleteObject(pen);
        EndPaint(hwnd, &ps);
        return 0;
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn show_frame(area: RECT) -> HWND {
    unsafe {
        let hinst = GetModuleHandleW(null());
        let class = wide("BuzzPrintRsRecFrame");
        let wc = WNDCLASSEXW { cbSize: size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: Some(frame_proc), hInstance: hinst, lpszClassName: class.as_ptr(), ..zeroed() };
        RegisterClassExW(&wc);
        let pad = 3;
        // Sin activar, transparente al mouse y con el color mágico como transparente: no estorba lo que se graba.
        let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
        let h = CreateWindowExW(ex, class.as_ptr(), null(), WS_POPUP, area.left - pad, area.top - pad, area.right - area.left + 2 * pad, area.bottom - area.top + 2 * pad, null_mut(), null_mut(), hinst, null());
        if h.is_null() {
            return h;
        }
        SetLayeredWindowAttributes(h, 0x00FF00FF, 0, LWA_COLORKEY);
        SetWindowDisplayAffinity(h, WDA_EXCLUDEFROMCAPTURE);
        ShowWindow(h, SW_SHOWNOACTIVATE);
        h
    }
}

fn destroy_frame(h: HWND) {
    if !h.is_null() {
        unsafe { DestroyWindow(h) };
    }
}
