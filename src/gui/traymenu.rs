//! Menú del ícono de la bandeja (clic derecho junto al reloj): una ventanita propia con el tema de la app
//! (mismas entradas, íconos y atajos que los menús de la biblioteca) en lugar del menú blanco de Windows.
//! Se abre pegada al cursor, del lado de la barra de tareas, con el tamaño justo de su contenido, y se
//! cierra al elegir algo, con Esc o al hacer clic afuera.

use std::any::Any;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};

use egui::{CornerRadius, Sense, Stroke, Vec2};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

use super::host::{self, Options, View, WinCtl};
use super::widgets::glyph as g;
use super::{colors, mono};
use crate::app::{key_text, post, Cmd};
use crate::i18n::t;

/// Ancho inicial estimado: en el primer cuadro se mide el contenido y se ajusta al tamaño exacto.
const W: f32 = 340.0;

pub struct TrayMenu {
    recording: bool,
    /// Esquina inferior derecha donde quedó anclado (píxeles de pantalla): al ajustar el tamaño al
    /// contenido, el menú crece hacia arriba y a la izquierda, del lado contrario a la barra de tareas.
    anchor: (i32, i32),
    /// Último tamaño pedido (lógico), para no reajustar en cada cuadro.
    fitted: (f32, f32),
    styled: bool,
}

/// Abre el menú junto al cursor (o lo cierra si ya estaba abierto).
pub fn toggle() {
    if let Some(h) = host::find("traymenu") {
        host::close(h);
        return;
    }
    let recording = crate::record::is_recording();
    let h = if recording { 470.0 } else { 590.0 };
    let (at, anchor) = unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(mon, &mut mi);
        let s = GetDpiForSystem().max(96) as f32 / 96.0;
        let (pw, ph) = ((W * s) as i32, (h * s) as i32);
        let wa = mi.rcWork;
        // Como el menú de Windows: arriba y a la izquierda del cursor (la bandeja está abajo a la derecha),
        // siempre dentro del área de trabajo.
        let x = (pt.x - pw + 8).clamp(wa.left, (wa.right - pw).max(wa.left));
        let y = (pt.y - ph - 6).clamp(wa.top, (wa.bottom - ph).max(wa.top));
        ((x, y), (x + pw, y + ph))
    };
    host::open(Box::new(TrayMenu { recording, anchor, fitted: (W, h), styled: false }), Options { width: W, height: h, min_width: 200.0, min_height: 120.0, popup: true, topmost: true, at: Some(at), scale: false });
}

impl TrayMenu {
    fn heading(ui: &mut egui::Ui, text: &str) {
        ui.add_space(6.0);
        super::menu_heading(ui, text);
    }
}

impl View for TrayMenu {
    fn kind(&self) -> &'static str {
        "traymenu"
    }
    fn title(&self) -> String {
        "BuzzPrint".into()
    }
    fn on_deactivate(&mut self) -> bool {
        true
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        if !self.styled {
            self.styled = true;
            // Windows 11: esquinas redondeadas y borde fino del tema, como los menús del sistema.
            unsafe {
                let round: i32 = 2;
                DwmSetWindowAttribute(win.hwnd, 33, &round as *const i32 as *const c_void, 4);
                let b = c.strong_border;
                let colorref: u32 = b.r() as u32 | (b.g() as u32) << 8 | (b.b() as u32) << 16;
                DwmSetWindowAttribute(win.hwnd, 34, &colorref as *const u32 as *const c_void, 4);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            win.close = true;
        }
        let mut cmd: Option<i32> = None;
        let frame = egui::Frame::default().fill(c.raised).inner_margin(egui::Margin { left: 8, right: 8, top: 12, bottom: 8 });
        let mut content = Vec2::ZERO;
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            super::menu_style(ui);
            let top = ui.cursor().top();
            // Encabezado: la marca viva y una pastilla con el estado.
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                super::brand(ui, 26.0, 14.0);
                ui.add_space(24.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    let (txt, col) = if self.recording { (t("GRABANDO"), c.error) } else { (t("ACTIVO"), c.ok) };
                    let galley = ui.painter().layout_no_wrap(txt, mono(10.0), col);
                    let (r, _) = ui.allocate_exact_size(Vec2::new(galley.size().x + 28.0, 22.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, CornerRadius::same(11), crate::theme::alpha(col, 0.12));
                    p.rect_stroke(r, CornerRadius::same(11), Stroke::new(1.0, crate::theme::alpha(col, 0.35)), egui::StrokeKind::Inside);
                    let d = egui::pos2(r.left() + 11.0, r.center().y);
                    if super::appearance().boberto_motion {
                        // Latido del punto de estado.
                        let ph = ((ui.input(|i| i.time) * 2.2).sin() * 0.5 + 0.5) as f32;
                        p.circle_filled(d, 3.0 + 2.5 * ph, crate::theme::alpha(col, 0.35 * (1.0 - ph)));
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
                    }
                    p.circle_filled(d, 3.0, col);
                    p.galley_with_override_text_color(egui::pos2(r.left() + 19.0, r.center().y - galley.size().y / 2.0), galley, col);
                });
            });
            ui.add_space(10.0);
            // Regla con el acento que se desvanece hacia los costados.
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            let mut mesh = egui::Mesh::default();
            for (f, a) in [(0.0, 0.0), (0.2, 0.55), (0.5, 0.8), (0.8, 0.55), (1.0, 0.0)] {
                let x = r.left() + r.width() * f;
                let col = crate::theme::alpha(c.accent, a);
                mesh.colored_vertex(egui::pos2(x, r.top()), col);
                mesh.colored_vertex(egui::pos2(x, r.bottom()), col);
            }
            for i in 0..4u32 {
                let b = i * 2;
                mesh.indices.extend([b, b + 2, b + 3, b, b + 3, b + 1]);
            }
            ui.painter().add(egui::Shape::mesh(mesh));

            let delay = format!("{} s", crate::store::settings().delay_seconds);
            Self::heading(ui, &t("Capturar"));
            for (gl, label, key, id) in [
                (g::REGION, t("Región o ventana"), crate::app::capture_key(), 10),
                (g::SCREEN, t("Pantalla completa"), key_text(2), 11),
                (g::TIMER, t("Con demora"), delay, 12),
            ] {
                if super::menu_item_icon(ui, gl, &label, "", &key, true).clicked() {
                    cmd = Some(id);
                }
            }

            Self::heading(ui, &t("Grabar"));
            if self.recording {
                if super::menu_item_icon_tinted(ui, g::RECORD, &t("Detener grabación"), &key_text(4), c.error).clicked() {
                    cmd = Some(30);
                }
            } else {
                if super::menu_item_icon(ui, g::RECORD, &t("Grabar video"), "", &key_text(4), true).clicked() {
                    cmd = Some(31);
                }
                // Con qué audio: cuatro pastillas en un renglón, alineadas con el texto de las entradas.
                ui.horizontal(|ui| {
                    ui.add_space(42.0);
                    ui.spacing_mut().item_spacing.x = 5.0;
                    for (gl, label, tip, id) in [
                        ('\u{E74F}', t("Mudo"), t("Video sin audio"), 33),
                        (g::SCREEN, "PC".to_string(), t("Video con audio de la PC"), 34),
                        ('\u{E720}', t("Mic"), t("Video con micrófono"), 35),
                        (g::AUDIO, t("PC + Mic"), t("Video con audio de la PC y micrófono"), 36),
                    ] {
                        let galley = ui.painter().layout_no_wrap(label, super::prop(11.5), c.muted);
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 32.0, 26.0), Sense::click());
                        let hot = resp.hovered();
                        let p = ui.painter();
                        p.rect_filled(rect, CornerRadius::same(13), if hot { crate::theme::alpha(c.accent, 0.18) } else { c.canvas });
                        p.rect_stroke(rect, CornerRadius::same(13), Stroke::new(1.0, if hot { c.accent } else { c.faint_border }), egui::StrokeKind::Inside);
                        p.text(egui::pos2(rect.left() + 13.0, rect.center().y), egui::Align2::CENTER_CENTER, gl.to_string(), super::widgets::icon_font(10.5), if hot { c.accent } else { c.muted });
                        p.galley_with_override_text_color(egui::pos2(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0), galley, if hot { c.ink } else { c.muted });
                        if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            cmd = Some(id);
                        }
                    }
                    ui.add_space(10.0);
                });
                ui.add_space(4.0);
                if super::menu_item_icon(ui, g::AUDIO, &t("Grabar solo audio"), "", "", true).clicked() {
                    cmd = Some(32);
                }
            }

            Self::heading(ui, &t("Abrir"));
            for (gl, label, key, id) in [
                (g::LIST, t("Menú rápido"), key_text(3), 20),
                (g::LIBRARY, t("Biblioteca"), String::new(), 21),
                (g::EDITOR, t("Editor"), String::new(), 22),
                (g::PERSONALIZE, t("Personalizar"), String::new(), 23),
                (g::OPTIONS, t("Opciones"), String::new(), 24),
            ] {
                if super::menu_item_icon(ui, gl, &label, "", &key, true).clicked() {
                    cmd = Some(id);
                }
            }
            ui.add_space(4.0);
            ui.separator();
            let quit = super::menu_item_icon_tinted(ui, '\u{E7E8}', &t("Salir"), "", c.error);
            // Versión en el pie, a la derecha de "Salir".
            ui.painter().text(
                egui::pos2(quit.rect.right() - 12.0, quit.rect.center().y),
                egui::Align2::RIGHT_CENTER,
                format!("v{}", env!("CARGO_PKG_VERSION")),
                mono(10.0),
                c.line_number,
            );
            if quit.clicked() {
                cmd = Some(99);
            }
            content = Vec2::new(ui.min_rect().width(), ui.cursor().top() - top);
        });
        // Ajuste exacto al contenido, anclado del lado del reloj: el alto a la medida y el ancho solo crece
        // (el contenido ocupa siempre todo el ancho disponible, así que no puede medirse más angosto).
        let want_h = content.y + 12.0 + 8.0;
        let want_w = (content.x + 16.0).max(self.fitted.0);
        if want_w - self.fitted.0 > 1.0 || (want_h - self.fitted.1).abs() > 1.0 {
            self.fitted = (want_w, want_h);
            let (hwnd, s, anchor) = (win.hwnd as isize, win.ppp, self.anchor);
            post(Cmd::Run(Box::new(move || unsafe {
                let (pw, ph) = ((want_w * s).ceil() as i32, (want_h * s).ceil() as i32);
                let mon = MonitorFromPoint(POINT { x: anchor.0 - 1, y: anchor.1 - 1 }, MONITOR_DEFAULTTONEAREST);
                let mut mi: MONITORINFO = zeroed();
                mi.cbSize = size_of::<MONITORINFO>() as u32;
                GetMonitorInfoW(mon, &mut mi);
                let wa = mi.rcWork;
                let x = (anchor.0 - pw).clamp(wa.left, (wa.right - pw).max(wa.left));
                let y = (anchor.1 - ph).clamp(wa.top, (wa.bottom - ph).max(wa.top));
                SetWindowPos(hwnd as _, std::ptr::null_mut(), x, y, pw, ph, SWP_NOZORDER | SWP_NOACTIVATE);
            })));
        }
        if let Some(id) = cmd {
            win.close = true;
            post(Cmd::Run(Box::new(move || {
                // Las capturas esperan a que el menú termine de cerrarse (si no, sale en la foto).
                if (10..=12).contains(&id) {
                    std::thread::sleep(std::time::Duration::from_millis(160));
                }
                crate::app::menu_command(id);
            })));
        }
    }
}
