//! Selector de color propio (reemplaza al cuadro "Color" de Windows): cuadro de saturación y brillo,
//! barra de tono, color anterior contra el nuevo, campo hexadecimal y colores recientes, con el tema de la app.
//! Es una ventanita flotante dentro de la misma ventana: se abre junto al botón que la pidió.

use std::cell::RefCell;

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::{colors, mono, primary};
use crate::i18n::t;
use crate::theme::{alpha, parse_hex, to_hex};

const W: f32 = 240.0;
const SV_H: f32 = 150.0;
const HUE_H: f32 = 14.0;
const MAX_RECENT: usize = 9;

thread_local! {
    /// Últimos colores aplicados con el selector (en esta sesión, en todas las ventanas).
    static RECENT: RefCell<Vec<Color32>> = const { RefCell::new(Vec::new()) };
}

/// Qué pasó en este cuadro.
pub enum Outcome {
    /// Sigue abierto.
    Open,
    Apply(Color32),
    Cancel,
}

pub struct ColorPicker {
    id: egui::Id,
    /// Tono (0..1), saturación y brillo, en sRGB como los selectores de las apps de diseño.
    h: f32,
    s: f32,
    v: f32,
    hex: String,
    original: Color32,
    /// El botón que lo abrió: se abre debajo, o arriba si abajo no entra.
    button: Rect,
    /// Alto real del cuadro anterior (para decidir si entra abajo).
    height: f32,
    /// El clic que lo abrió no cuenta como "clic afuera".
    fresh: bool,
}

impl ColorPicker {
    /// `button`: el rectángulo del botón que lo abrió.
    pub fn new(start: Color32, button: Rect) -> ColorPicker {
        let (h, s, v) = rgb_to_hsv(start);
        ColorPicker { id: egui::Id::new("buzzprint-color-picker"), h, s, v, hex: hex_text(start), original: start, button, height: 300.0, fresh: true }
    }

    fn color(&self) -> Color32 {
        hsv_to_rgb(self.h, self.s, self.v)
    }

    fn set_color(&mut self, c: Color32) {
        let (h, s, v) = rgb_to_hsv(c);
        // Grises: se conserva el tono que ya estaba, así la barra no salta al rojo.
        if s > 0.0 && v > 0.0 {
            self.h = h;
        }
        self.s = s;
        self.v = v;
        self.hex = hex_text(c);
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let c = colors();
        let mut out = Outcome::Open;
        let frame = egui::Frame::default()
            .fill(c.surface)
            .stroke(Stroke::new(1.0_f32, alpha(c.ink, 0.14)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(12))
            .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(90) });
        // Debajo del botón; si no entra, arriba (sin taparlo).
        let below = self.button.bottom() + 6.0 + self.height <= ctx.screen_rect().bottom() - 4.0;
        let (pos, pivot) = if below { (self.button.left_bottom() + Vec2::new(0.0, 6.0), egui::Align2::LEFT_TOP) } else { (self.button.left_top() - Vec2::new(0.0, 6.0), egui::Align2::LEFT_BOTTOM) };
        let area = egui::Area::new(self.id).order(egui::Order::Foreground).fixed_pos(pos).pivot(pivot).constrain(true).fade_in(false).show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(W);
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
                self.sv_square(ui);
                self.hue_bar(ui);
                ui.horizontal(|ui| {
                    // Anterior | nuevo: un clic en el anterior lo recupera.
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(64.0, 30.0), Sense::click());
                    let p = ui.painter();
                    let (l, rr) = r.split_left_right_at_fraction(0.5);
                    p.rect_filled(l, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, self.original);
                    p.rect_filled(rr, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, self.color());
                    p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0_f32, alpha(c.ink, 0.2)), egui::StrokeKind::Inside);
                    if resp.on_hover_text(t("Color anterior")).clicked() {
                        self.set_color(self.original);
                    }
                    ui.label(RichText::new("#").font(mono(13.0)).color(c.muted));
                    let edit = ui.add(egui::TextEdit::singleline(&mut self.hex).font(mono(13.0)).desired_width(84.0).char_limit(7).margin(egui::Margin::symmetric(8, 6)));
                    if edit.changed() {
                        let raw = self.hex.trim().trim_start_matches('#').to_string();
                        if raw.len() == 6 && raw.chars().all(|ch| ch.is_ascii_hexdigit()) {
                            let col = parse_hex(&format!("#{raw}"));
                            let keep = self.hex.clone();
                            self.set_color(col);
                            self.hex = keep;
                        }
                    }
                    if edit.lost_focus() {
                        self.hex = hex_text(self.color());
                    }
                });
                let recent = RECENT.with(|r| r.borrow().clone());
                if !recent.is_empty() {
                    ui.label(RichText::new(t("Recientes")).font(mono(10.5)).color(c.muted));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for col in recent {
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                            ui.painter().circle_filled(r.center(), 9.0, col);
                            ui.painter().circle_stroke(r.center(), 9.0, Stroke::new(1.0_f32, alpha(c.ink, 0.2)));
                            if resp.hovered() {
                                ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(1.0_f32, alpha(c.ink, 0.45)));
                            }
                            if resp.on_hover_text(to_hex(col)).clicked() {
                                self.set_color(col);
                            }
                        }
                    });
                }
                ui.add_space(2.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary(ui, &t("Aplicar")).clicked() {
                        out = Outcome::Apply(self.color());
                    }
                    if ui.button(t("Cancelar")).clicked() {
                        out = Outcome::Cancel;
                    }
                });
            });
        });
        let rect = area.response.rect;
        self.height = rect.height();
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                out = Outcome::Cancel;
            }
            if i.key_pressed(egui::Key::Enter) {
                out = Outcome::Apply(self.color());
            }
            if !self.fresh && i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)) {
                out = Outcome::Cancel;
            }
        });
        self.fresh = false;
        if let Outcome::Apply(col) = out {
            RECENT.with(|r| {
                let mut r = r.borrow_mut();
                r.retain(|x| *x != col);
                r.insert(0, col);
                r.truncate(MAX_RECENT);
            });
        }
        out
    }

    /// Cuadro de saturación (horizontal) y brillo (vertical) para el tono elegido.
    fn sv_square(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(W, SV_H), Sense::click_and_drag());
        let n = 16;
        let mut mesh = egui::Mesh::default();
        for j in 0..=n {
            for i in 0..=n {
                let (s, v) = (i as f32 / n as f32, 1.0 - j as f32 / n as f32);
                mesh.colored_vertex(Pos2::new(r.left() + r.width() * i as f32 / n as f32, r.top() + r.height() * j as f32 / n as f32), hsv_to_rgb(self.h, s, v));
            }
        }
        for j in 0..n {
            for i in 0..n {
                let a = (j * (n + 1) + i) as u32;
                let b = a + n as u32 + 1;
                mesh.add_triangle(a, a + 1, b + 1);
                mesh.add_triangle(a, b + 1, b);
            }
        }
        let p = ui.painter();
        p.add(egui::Shape::mesh(mesh));
        // Esquinas redondeadas: se tapan con el color del panel y se repasa el borde.
        round_corners(p, r, 8.0, c.surface);
        p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0_f32, alpha(c.ink, 0.14)), egui::StrokeKind::Inside);
        if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on()) {
            self.s = ((pos.x - r.left()) / r.width()).clamp(0.0, 1.0);
            self.v = 1.0 - ((pos.y - r.top()) / r.height()).clamp(0.0, 1.0);
            self.hex = hex_text(self.color());
        }
        let at = Pos2::new(r.left() + self.s * r.width(), r.top() + (1.0 - self.v) * r.height());
        handle(ui.painter(), at, self.color());
    }

    fn hue_bar(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(W, HUE_H), Sense::click_and_drag());
        let n = 36;
        let mut mesh = egui::Mesh::default();
        for i in 0..=n {
            let k = i as f32 / n as f32;
            let col = hsv_to_rgb(k, 1.0, 1.0);
            let x = r.left() + r.width() * k;
            mesh.colored_vertex(Pos2::new(x, r.top()), col);
            mesh.colored_vertex(Pos2::new(x, r.bottom()), col);
        }
        for i in 0..n as u32 {
            let a = i * 2;
            mesh.add_triangle(a, a + 1, a + 3);
            mesh.add_triangle(a, a + 3, a + 2);
        }
        let p = ui.painter();
        p.add(egui::Shape::mesh(mesh));
        round_corners(p, r, HUE_H / 2.0, c.surface);
        if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on()) {
            self.h = ((pos.x - r.left()) / r.width()).clamp(0.0, 0.9999);
            self.hex = hex_text(self.color());
        }
        handle(ui.painter(), Pos2::new(r.left() + self.h * r.width(), r.center().y), hsv_to_rgb(self.h, 1.0, 1.0));
    }
}

/// Manija: círculo con el color, aro blanco y un borde oscuro fino (se lee sobre cualquier color).
fn handle(p: &egui::Painter, at: Pos2, fill: Color32) {
    p.circle_filled(at, 7.0, fill);
    p.circle_stroke(at, 7.0, Stroke::new(2.5_f32, Color32::WHITE));
    p.circle_stroke(at, 8.5, Stroke::new(1.0_f32, Color32::from_black_alpha(110)));
}

/// Tapa las cuatro esquinas de `r` con `bg` para que un degradado (malla) parezca redondeado.
fn round_corners(p: &egui::Painter, r: Rect, rad: f32, bg: Color32) {
    let steps = 8;
    let corners = [(r.left_top(), Vec2::new(1.0, 1.0)), (r.right_top(), Vec2::new(-1.0, 1.0)), (r.right_bottom(), Vec2::new(-1.0, -1.0)), (r.left_bottom(), Vec2::new(1.0, -1.0))];
    for (corner, dir) in corners {
        let ctr = corner + Vec2::new(dir.x * rad, dir.y * rad);
        let mut pts = vec![corner];
        for k in 0..=steps {
            // De la dirección horizontal a la vertical del cuarto de círculo que mira a la esquina.
            let a = std::f32::consts::FRAC_PI_2 * k as f32 / steps as f32;
            pts.push(ctr + Vec2::new(-dir.x * a.cos() * rad, -dir.y * a.sin() * rad));
        }
        // Abanico desde la esquina: forma convexa por tramos, se pinta triángulo por triángulo.
        for w in pts[1..].windows(2) {
            p.add(egui::Shape::convex_polygon(vec![corner, w[0], w[1]], bg, Stroke::NONE));
        }
    }
}

fn hex_text(c: Color32) -> String {
    to_hex(c).trim_start_matches('#').to_uppercase()
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color32 {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let u = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(u(r), u(g), u(b))
}

pub fn rgb_to_hsv(c: Color32) -> (f32, f32, f32) {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trip() {
        for c in [Color32::from_rgb(0xD8, 0x8A, 0x4E), Color32::from_rgb(0x0E, 0x10, 0x15), Color32::WHITE, Color32::BLACK, Color32::from_rgb(0x26, 0x84, 0xFF), Color32::from_rgb(0x39, 0xFF, 0x14)] {
            let (h, s, v) = rgb_to_hsv(c);
            assert_eq!(hsv_to_rgb(h, s, v), c);
        }
    }
}
