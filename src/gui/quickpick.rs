//! Menú rápido junto al cursor con las últimas capturas.
//! 1-9 / clic = copiar (y pegar en la ventana que estaba activa), Shift = solo copiar,
//! E / clic derecho = editar, flechas para moverse, Esc cierra. Se cierra solo al perder el foco.

use std::any::Any;
use std::mem::{size_of, zeroed};

use egui::{Align2, Color32, CornerRadius, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::host::{self, Options, View, WinCtl};
use super::thumbs::Thumbs;
use super::{bold, colors, mono};
use crate::app::{post, Cmd};
use crate::i18n::t;
use crate::store::{self, Item};
use crate::theme::alpha;

const COLS: usize = 4;
const CELL_W: f32 = 196.0;
const PIC_H: f32 = 118.0;
const CELL_H: f32 = PIC_H + 58.0;
const GAP: f32 = 10.0;

pub struct QuickPick {
    previous: isize,
    items: Vec<Item>,
    index: usize,
    thumbs: Option<Thumbs>,
}

pub fn toggle(previous: HWND) {
    if let Some(h) = host::find("quickpick") {
        host::close(h);
        return;
    }
    let n = store::settings().quick_pick_count.clamp(1, 20) as usize;
    let items: Vec<Item> = store::with(|s| s.all().iter().take(n).cloned().collect());
    let cols = items.len().clamp(1, COLS);
    let rows = items.len().div_ceil(COLS).max(1);
    let w = 2.0 * 30.0 + cols as f32 * (CELL_W + GAP);
    let h = if items.is_empty() { 190.0 } else { 120.0 + rows as f32 * (CELL_H + GAP) };

    // Junto al cursor, dentro del área de trabajo de ese monitor.
    let at = unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(mon, &mut mi);
        let s = GetDpiForSystem().max(96) as f32 / 96.0;
        let (pw, ph) = ((w * s) as i32, (h * s) as i32);
        let wa = mi.rcWork;
        let x = (pt.x - pw / 2).clamp(wa.left, (wa.right - pw).max(wa.left));
        let y = (pt.y - 30).clamp(wa.top, (wa.bottom - ph).max(wa.top));
        (x, y)
    };
    let view = QuickPick { previous: previous as isize, items, index: 0, thumbs: None };
    host::open(Box::new(view), Options { width: w, height: h, min_width: 200.0, min_height: 120.0, popup: true, topmost: true, at: Some(at), scale: false });
}

impl QuickPick {
    fn pick(&mut self, i: usize, paste: bool, win: &mut WinCtl) {
        let Some(it) = self.items.get(i) else { return };
        let ids = vec![it.id.clone()];
        if paste && store::settings().paste_on_pick && self.previous != 0 {
            post(Cmd::PasteInto(ids, self.previous));
        } else {
            post(Cmd::CopyItems(ids));
        }
        win.close = true;
    }

    fn edit(&mut self, i: usize, win: &mut WinCtl) {
        if let Some(it) = self.items.get(i) {
            post(Cmd::OpenEditor(Some(it.id.clone())));
        }
        win.close = true;
    }
}

impl View for QuickPick {
    fn kind(&self) -> &'static str {
        "quickpick"
    }
    fn title(&self) -> String {
        t("BuzzPrint · Menú rápido")
    }
    fn on_deactivate(&mut self) -> bool {
        true
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        let mut thumbs = self.thumbs.take().unwrap_or_else(|| Thumbs::new(win.hwnd));

        // Teclado.
        let (shift, keys) = ctx.input(|i| (i.modifiers.shift, i.events.clone()));
        let mut action: Option<(usize, bool, bool)> = None; // (índice, pegar, editar)
        for e in keys {
            if let egui::Event::Key { key, pressed: true, .. } = e {
                let digits = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9, Key::Num0];
                if let Some(n) = digits.iter().position(|k| *k == key) {
                    action = Some((n, !shift, false));
                }
                match key {
                    Key::Escape => win.close = true,
                    Key::ArrowRight => self.index = (self.index + 1).min(self.items.len().saturating_sub(1)),
                    Key::ArrowLeft => self.index = self.index.saturating_sub(1),
                    Key::ArrowDown => self.index = (self.index + COLS).min(self.items.len().saturating_sub(1)),
                    Key::ArrowUp => self.index = self.index.saturating_sub(COLS),
                    Key::Enter => action = Some((self.index, !shift, false)),
                    Key::E => action = Some((self.index, false, true)),
                    _ => {}
                }
            }
        }

        let frame = egui::Frame::default()
            .fill(c.surface)
            .stroke(Stroke::new(1.5, alpha(c.accent, 0.42)))
            .inner_margin(egui::Margin::same(16));
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.horizontal(|ui| {
                super::corner_tri(ui, 8.5, c.accent);
                ui.label(RichText::new(t("CAPTURAS RECIENTES")).font(mono(12.0)).color(c.accent).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.items.len();
                    ui.label(RichText::new(format!("{n}{}", t(if n == 1 { " CAPTURA" } else { " CAPTURAS" }))).font(mono(11.0)).color(c.muted));
                });
            });
            super::accent_rule(ui);
            ui.add_space(10.0);

            if self.items.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new(t("Todavía no hay capturas. Apretá PrtSc para tomar una.")).color(c.muted));
                ui.add_space(20.0);
            } else {
                let start = ui.cursor().min;
                let mut hovered = None;
                for (i, it) in self.items.iter().enumerate() {
                    let (col, row) = (i % COLS, i / COLS);
                    let r = Rect::from_min_size(start + Vec2::new(col as f32 * (CELL_W + GAP), row as f32 * (CELL_H + GAP)), Vec2::new(CELL_W, CELL_H));
                    let resp = ui.interact(r, ui.id().with(("cell", i)), Sense::click());
                    if resp.hovered() {
                        hovered = Some(i);
                    }
                    let on = i == self.index;
                    let p = ui.painter();
                    p.rect_filled(r, CornerRadius::same(7), if on { alpha(c.accent, 0.07) } else { c.surface });
                    p.rect_stroke(r, CornerRadius::same(7), Stroke::new(1.0, if on { alpha(c.accent, 0.6) } else { alpha(c.ink, 0.08) }), egui::StrokeKind::Inside);
                    if on {
                        p.rect_filled(Rect::from_min_size(r.min, Vec2::new(3.0, r.height())), 0.0, c.accent);
                    }
                    let pic = Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::new(CELL_W - 12.0, PIC_H));
                    p.rect_filled(pic, 0.0, c.canvas);
                    if let Some((tex, size)) = thumbs.get(ctx, &it.path, 400) {
                        let inner = pic.shrink(5.0);
                        let s = (inner.width() / size.x).min(inner.height() / size.y);
                        let img = Rect::from_center_size(inner.center(), size * s);
                        p.image(tex, img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    if i < 10 {
                        let b = Rect::from_min_size(pic.min + Vec2::splat(5.0), Vec2::splat(20.0));
                        p.rect_filled(b, CornerRadius::same(5), c.header);
                        p.rect_stroke(b, CornerRadius::same(5), Stroke::new(1.0, alpha(c.accent, 0.6)), egui::StrokeKind::Inside);
                        p.text(b.center(), Align2::CENTER_CENTER, ((i + 1) % 10).to_string(), mono(11.0), c.accent);
                    }
                    let name = it.display();
                    let name_rect = Rect::from_min_size(Pos2::new(r.min.x + 10.0, pic.max.y + 8.0), Vec2::new(CELL_W - 20.0, 18.0));
                    let galley = ui.painter().layout(name, bold(13.0), c.ink, name_rect.width());
                    let clipped = ui.painter().with_clip_rect(name_rect);
                    clipped.galley(name_rect.min, galley, c.ink);
                    let meta = format!("{}{}", it.created.short(), if it.width > 0 { format!(" · {}×{}", it.width, it.height) } else { String::new() });
                    p.text(Pos2::new(r.min.x + 10.0, pic.max.y + 30.0), Align2::LEFT_TOP, meta, mono(10.5), c.muted);

                    if resp.clicked() {
                        action = Some((i, !shift, false));
                    }
                    if resp.secondary_clicked() {
                        action = Some((i, false, true));
                    }
                }
                if let Some(h) = hovered {
                    self.index = h;
                }
                let rows = self.items.len().div_ceil(COLS);
                ui.allocate_space(Vec2::new(COLS.min(self.items.len()) as f32 * (CELL_W + GAP), rows as f32 * (CELL_H + GAP)));
            }

            ui.separator();
            let hint = if store::settings().paste_on_pick {
                t("1-9 / CLIC  COPIAR Y PEGAR   ·   SHIFT  SOLO COPIAR   ·   E  EDITAR   ·   ESC")
            } else {
                t("1-9 / CLIC  COPIAR   ·   E  EDITAR   ·   ESC")
            };
            ui.label(RichText::new(hint).font(mono(10.5)).color(c.muted));
        });
        self.thumbs = Some(thumbs);

        if let Some((i, paste, edit)) = action {
            if edit {
                self.edit(i, win);
            } else {
                self.pick(i, paste, win);
            }
        }
    }
}
