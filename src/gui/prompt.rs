//! Ventanas chicas estilo DBMON: confirmar, informar y progreso con Cancelar. No son modales: la respuesta llega
//! por un callback que corre fuera del frame (con `Cmd::Run`).

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{RichText, Stroke, Vec2};
use windows_sys::Win32::Foundation::HWND;

use super::host::{self, Options, View, WinCtl};
use super::{bold, colors, mono, primary};
use crate::app::{post, Cmd};
use crate::i18n::t;

type Callback = Box<dyn FnOnce()>;

struct Prompt {
    title: String,
    text: String,
    ok: String,
    cancel: Option<String>,
    on_ok: Option<Callback>,
    on_cancel: Option<Callback>,
}

impl View for Prompt {
    fn kind(&self) -> &'static str {
        "prompt"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", self.title)
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn on_close_request(&mut self) -> bool {
        if let Some(f) = self.on_cancel.take() {
            post(Cmd::Run(f));
        }
        true
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        egui::CentralPanel::default().frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin { left: 24, right: 24, top: 20, bottom: 18 })).show(ctx, |ui| {
            ui.horizontal(|ui| {
                super::corner_tri(ui, 9.5, c.accent);
                ui.label(RichText::new(&self.title).font(bold(15.0)).color(c.ink));
            });
            ui.add_space(10.0);
            egui::ScrollArea::vertical().id_salt("pr_body").max_height(ui.available_height() - 48.0).show(ui, |ui| {
                ui.label(RichText::new(&self.text).color(c.ink));
            });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, &self.ok).clicked() || ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.on_cancel = None;
                            if let Some(f) = self.on_ok.take() {
                                post(Cmd::Run(f));
                            }
                            win.close = true;
                        }
                        if let Some(cn) = &self.cancel {
                            if ui.button(cn).clicked() {
                                win.close = true;
                            }
                        }
                    });
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            win.close = true;
        }
    }
}

fn height_for(text: &str) -> f32 {
    let lines: usize = text.lines().map(|l| 1 + l.chars().count() / 62).sum();
    (130.0 + lines as f32 * 19.0).clamp(170.0, 560.0)
}

/// Pregunta con dos botones. `on_ok` corre si se acepta.
pub fn confirm(title: &str, text: &str, ok: &str, cancel: &str, on_ok: impl FnOnce() + 'static) {
    let view = Prompt { title: title.into(), text: text.into(), ok: ok.into(), cancel: Some(cancel.into()), on_ok: Some(Box::new(on_ok)), on_cancel: None };
    open(view);
}

/// Igual que `confirm`, con algo que correr también si se cancela o se cierra.
pub fn confirm_or(title: &str, text: &str, ok: &str, cancel: &str, on_ok: impl FnOnce() + 'static, on_cancel: impl FnOnce() + 'static) {
    let view = Prompt { title: title.into(), text: text.into(), ok: ok.into(), cancel: Some(cancel.into()), on_ok: Some(Box::new(on_ok)), on_cancel: Some(Box::new(on_cancel)) };
    open(view);
}

pub fn info(title: &str, text: &str) {
    let view = Prompt { title: title.into(), text: text.into(), ok: t("Aceptar"), cancel: None, on_ok: None, on_cancel: None };
    open(view);
}

fn open(view: Prompt) {
    let h = height_for(&view.text);
    host::open(Box::new(view), Options { width: 500.0, height: h, min_width: 360.0, min_height: 150.0, popup: false, topmost: true, at: None, scale: true });
}

// ------------------------------------------------------------------ progreso

#[derive(Default)]
pub struct ProgressState {
    pub done: u64,
    pub total: Option<u64>,
    pub status: Option<String>,
    pub finished: bool,
}

/// Lo que comparte el hilo que trabaja con la ventana: avance, estado y el pedido de cancelar.
#[derive(Clone)]
pub struct Progress {
    pub state: Arc<Mutex<ProgressState>>,
    pub cancel: Arc<AtomicBool>,
    hwnd: Arc<Mutex<isize>>,
}

impl Progress {
    pub fn report(&self, done: u64, total: Option<u64>) {
        if let Ok(mut s) = self.state.lock() {
            s.done = done;
            s.total = total;
            s.status = None;
        }
        self.wake();
    }
    pub fn status(&self, text: &str) {
        if let Ok(mut s) = self.state.lock() {
            s.status = Some(text.into());
        }
        self.wake();
    }
    /// Cierra la ventana (desde cualquier hilo).
    pub fn finish(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.finished = true;
        }
        self.wake();
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    fn wake(&self) {
        let h = *self.hwnd.lock().unwrap();
        if h != 0 {
            host::wake(h as HWND);
        }
    }
}

struct ProgressView {
    title: String,
    p: Progress,
}

impl View for ProgressView {
    fn kind(&self) -> &'static str {
        "progress"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", self.title)
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn on_close_request(&mut self) -> bool {
        self.p.cancel.store(true, Ordering::SeqCst);
        true
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        let (done, total, status, finished) = {
            let s = self.p.state.lock().unwrap();
            (s.done, s.total, s.status.clone(), s.finished)
        };
        if finished {
            win.close = true;
        }
        egui::CentralPanel::default().frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin { left: 20, right: 24, top: 18, bottom: 16 })).show(ctx, |ui| {
          ui.horizontal_top(|ui| {
            // Boberto trabajando mientras se espera.
            super::mark::working(ui, 58.0, c.accent);
            ui.add_space(14.0);
            ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                super::corner_tri(ui, 9.0, c.accent);
                ui.label(RichText::new(&self.title).font(bold(14.0)).color(c.ink));
            });
            ui.add_space(12.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 3.0, c.canvas);
            match total {
                // Con total conocido la barra muestra el avance, aunque haya un texto de estado.
                Some(tot) if tot > 0 => {
                    let f = (done as f32 / tot as f32).clamp(0.0, 1.0);
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, Vec2::new(r.width() * f, 6.0)), 3.0, c.accent);
                }
                _ => {
                    // Indeterminado: un tramo que va y viene.
                    let tt = ctx.input(|i| i.time) as f32;
                    let x = (tt * 0.8).fract();
                    let seg = egui::Rect::from_min_size(r.min + Vec2::new((r.width() - r.width() * 0.25) * x, 0.0), Vec2::new(r.width() * 0.25, 6.0));
                    ui.painter().rect_filled(seg, 3.0, c.accent);
                    ctx.request_repaint_after(Duration::from_millis(33));
                }
            }
            ui.add_space(8.0);
            let detail = status.unwrap_or_else(|| {
                if done == 0 {
                    t("Conectando…")
                } else {
                    let mb = |b: u64| format!("{:.1} MB", b as f64 / 1_048_576.0);
                    match total {
                        Some(tot) if tot > 0 => format!("{} / {}", mb(done), mb(tot)),
                        _ => mb(done),
                    }
                }
            });
            ui.label(RichText::new(detail).font(mono(11.5)).color(c.muted));
            });
          });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(t("Cancelar")).stroke(Stroke::new(1.0, c.faint_border)).min_size(Vec2::new(96.0, 28.0))).clicked() {
                    self.p.cancel.store(true, Ordering::SeqCst);
                    win.close = true;
                }
            });
        });
    }
}

/// Abre la ventana de progreso y devuelve el control para el hilo que trabaja.
pub fn progress(title: &str) -> Progress {
    let p = Progress { state: Arc::new(Mutex::new(ProgressState::default())), cancel: Arc::new(AtomicBool::new(false)), hwnd: Arc::new(Mutex::new(0)) };
    let view = ProgressView { title: title.into(), p: p.clone() };
    if let Some(h) = host::open(Box::new(view), Options { width: 480.0, height: 200.0, min_width: 380.0, min_height: 180.0, popup: false, topmost: true, at: None, scale: true }) {
        *p.hwnd.lock().unwrap() = h as isize;
    }
    p
}
