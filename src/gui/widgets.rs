//! Piezas con el estilo de DBMON (el mismo de la versión .NET): íconos de Segoe Fluent, interruptores, filas de
//! navegación, tarjetas "Neo", grilla de fondo, barra luminosa y segmentos de la barra de estado.

use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Ui, Vec2};

use super::{appearance, colors, mono};
use crate::theme::alpha;

/// Códigos de Segoe Fluent Icons / MDL2 (los mismos del XAML de .NET).
pub mod glyph {
    pub const CAMERA: char = '\u{E722}';
    pub const RECORD: char = '\u{E7C8}';
    pub const EDIT: char = '\u{E70F}';
    pub const LIBRARY: char = '\u{E8F1}';
    pub const VIEW: char = '\u{E890}';
    pub const AI: char = '\u{EA80}';
    pub const HELP: char = '\u{E897}';
    pub const SEARCH: char = '\u{E721}';
    pub const CHEVRON_DOWN: char = '\u{E70D}';
    pub const CHEVRON_RIGHT: char = '\u{E76C}';
    pub const LIST: char = '\u{E8FD}';
    pub const EXPORT: char = '\u{EDE1}';
    pub const COPY: char = '\u{E8C8}';
    pub const RENAME: char = '\u{E8AC}';
    pub const PIN: char = '\u{E718}';
    pub const DELETE: char = '\u{E74D}';
    pub const EDITOR: char = '\u{E7E6}';
    pub const FOLDER: char = '\u{E838}';
    pub const ALL: char = '\u{E8B9}';
    pub const TODAY: char = '\u{E787}';
    pub const VIDEO: char = '\u{E714}';
    pub const ADD: char = '\u{E710}';
    pub const OPTIONS: char = '\u{E9E9}';
    pub const PERSONALIZE: char = '\u{E790}';
    pub const GUIDE: char = '\u{E82D}';
    pub const OPEN_FOLDER: char = '\u{E8DA}';
    pub const IMAGES: char = '\u{EB9F}';
    pub const PLAY: char = '\u{E768}';
    pub const SECTION: char = '\u{E8B7}';
    pub const NO_SECTION: char = '\u{E8B6}';
    pub const AUDIO: char = '\u{E8D6}';
    pub const CAPTION: char = '\u{E8D2}';
    pub const DOCUMENT: char = '\u{E8A5}';
    pub const SHIELD: char = '\u{E72E}';
    pub const TICKET: char = '\u{E8C3}';
    pub const SORT: char = '\u{E8CB}';
    pub const REGION: char = '\u{F407}';
    pub const SCREEN: char = '\u{E7F4}';
    pub const TIMER: char = '\u{E916}';
    pub const PALETTE: char = '\u{E790}';
    pub const FONT: char = '\u{E8D2}';
    pub const ROBOT: char = '\u{E99A}';
    pub const SETTINGS: char = '\u{E713}';
    pub const CHECK: char = '\u{E73E}';
    pub const SPARKLE: char = '\u{E945}';
    pub const UP: char = '\u{E74A}';
    pub const DOWN: char = '\u{E74B}';
}

/// Pestaña de la barra superior de una ventana (Opciones / Personalizar / Boberto Studio): ícono + texto, misma
/// altura que los botones; la elegida lleva placa de acento suave y una regla de acento abajo.
pub fn top_tab(ui: &mut Ui, g: char, label: &str, on: bool) -> Response {
    let c = colors();
    let job = icon_label(g, label, 13.5, if on { c.accent } else { c.muted });
    let galley = ui.painter().layout_job(job);
    let size = Vec2::new(galley.size().x + 28.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hot = resp.hovered();
    let p = ui.painter();
    if on {
        p.rect_filled(rect, radius(), c.accent_weak);
        p.rect_stroke(rect, radius(), Stroke::new(1.0, c.accent_card_ring), egui::StrokeKind::Inside);
    } else if hot {
        p.rect_filled(rect, radius(), c.accent_tint);
    }
    let ink = if on { c.accent } else if hot { c.ink } else { c.muted };
    p.galley(Pos2::new(rect.min.x + 14.0, rect.center().y - galley.size().y / 2.0), galley, ink);
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Control segmentado (solapas dentro de una página): una tira con fondo hundido y la opción elegida en relieve.
/// Devuelve el índice tocado.
pub fn segmented(ui: &mut Ui, items: &[(char, String)], current: usize) -> Option<usize> {
    let c = colors();
    let mut hit = None;
    let r_out = CornerRadius::same(if appearance().rounded_controls { 9 } else { 0 });
    let r_in = CornerRadius::same(if appearance().rounded_controls { 7 } else { 0 });
    let galleys: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, (g, l))| ui.painter().layout_job(icon_label(*g, l, 13.0, if i == current { c.ink } else { c.muted })))
        .collect();
    let pad = 18.0;
    let total: f32 = galleys.iter().map(|g| g.size().x + pad * 2.0).sum::<f32>() + 8.0;
    let (outer, _) = ui.allocate_exact_size(Vec2::new(total, 40.0), Sense::hover());
    ui.painter().rect_filled(outer, r_out, c.canvas);
    ui.painter().rect_stroke(outer, r_out, Stroke::new(1.0, c.faint_border), egui::StrokeKind::Inside);
    let mut x = outer.min.x + 4.0;
    for (i, galley) in galleys.into_iter().enumerate() {
        let w = galley.size().x + pad * 2.0;
        let rect = Rect::from_min_size(Pos2::new(x, outer.min.y + 4.0), Vec2::new(w, outer.height() - 8.0));
        let resp = ui.interact(rect, ui.id().with(("seg", i)), Sense::click());
        let on = i == current;
        let p = ui.painter();
        if on {
            p.rect_filled(rect, r_in, c.raised);
            p.rect_stroke(rect, r_in, Stroke::new(1.0, c.strong_border), egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_min_size(Pos2::new(rect.center().x - 10.0, rect.max.y - 3.0), Vec2::new(20.0, 2.0)), CornerRadius::same(1), c.accent);
        } else if resp.hovered() {
            p.rect_filled(rect, r_in, c.accent_tint);
        }
        let ink = if on { c.ink } else if resp.hovered() { c.ink } else { c.muted };
        p.galley(Pos2::new(rect.min.x + pad, rect.center().y - galley.size().y / 2.0), galley, ink);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            hit = Some(i);
        }
        x += w;
    }
    hit
}

/// Deslizador entero sólido (riel opaco, relleno de acento y perilla con aro): el de egui usa colores con
/// transparencia y se ve "lavado" sobre las tarjetas. Devuelve (valor, se_está_arrastrando).
pub fn int_slider(ui: &mut Ui, value: i32, min: i32, max: i32, width: f32) -> (i32, bool) {
    let c = colors();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click_and_drag());
    let rail = Rect::from_center_size(rect.center(), Vec2::new(width - 16.0, 6.0));
    let span = (max - min).max(1) as f32;
    let mut v = value;
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on() {
            let t = ((pos.x - rail.min.x) / rail.width()).clamp(0.0, 1.0);
            v = min + (t * span).round() as i32;
        }
    }
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let t = (v - min) as f32 / span;
    let x = rail.min.x + rail.width() * t;
    let p = ui.painter();
    p.rect_filled(rail, CornerRadius::same(3), crate::theme::mix(c.ink, c.surface, 0.18));
    p.rect_filled(Rect::from_min_max(rail.min, Pos2::new(x, rail.max.y)), CornerRadius::same(3), c.accent);
    // Marcas de cada paso.
    for k in 0..=(max - min) {
        let mx = rail.min.x + rail.width() * k as f32 / span;
        if (mx - x).abs() > 6.0 {
            p.circle_filled(Pos2::new(mx, rail.center().y), 1.5, if mx < x { c.on_accent } else { c.muted });
        }
    }
    let knob = Pos2::new(x, rail.center().y);
    let active = resp.hovered() || resp.dragged();
    p.circle_filled(knob, if active { 10.0 } else { 9.0 }, c.accent);
    p.circle_filled(knob, 4.0, c.surface);
    p.circle_stroke(knob, if active { 10.0 } else { 9.0 }, Stroke::new(1.0, crate::theme::mix(c.accent, Color32::BLACK, 0.7)));
    (v, resp.dragged() || resp.is_pointer_button_down_on())
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(super::px(size), FontFamily::Name("icons".into()))
}

pub fn icon(g: char, size: f32) -> RichText {
    RichText::new(g.to_string()).font(icon_font(size))
}

/// Texto con un ícono adelante, en un solo trabajo de texto (ícono en su fuente, texto en la normal).
pub fn icon_label(g: char, text: &str, size: f32, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(&g.to_string(), 0.0, egui::TextFormat { font_id: icon_font(size - 1.0), color, valign: egui::Align::Center, ..Default::default() });
    if !text.is_empty() {
        job.append(text, 7.0, egui::TextFormat { font_id: FontId::proportional(size), color, valign: egui::Align::Center, ..Default::default() });
    }
    job
}

pub fn radius() -> CornerRadius {
    CornerRadius::same(if appearance().rounded_controls { 7 } else { 0 })
}

pub fn panel_radius() -> CornerRadius {
    CornerRadius::same(if appearance().rounded { 14 } else { 0 })
}

pub fn strip_radius() -> CornerRadius {
    CornerRadius::same(if appearance().rounded { 10 } else { 0 })
}

/// Rótulo de sección: "▚ TEXTO" (acento + gris, negrita chica).
pub fn section_label(ui: &mut Ui, text: &str) {
    let c = colors();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        ui.label(RichText::new("▚").size(super::px(10.0)).color(c.accent));
        ui.label(RichText::new(text.to_uppercase()).size(super::px(10.0)).strong().color(c.muted));
    });
}

/// Rótulo del panel lateral (.dbm-side-label).
pub fn nav_label(ui: &mut Ui, text: &str) {
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(text.to_uppercase()).size(super::px(10.0)).strong().color(colors().line_number));
    });
    ui.add_space(6.0);
}

/// Fila de navegación (.dbm-nav-link): ícono, texto y contador; la elegida lleva placa de acento.
pub fn nav_row(ui: &mut Ui, g: char, label: &str, count: Option<usize>, on: bool) -> Response {
    let c = colors();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, radius(), c.accent_weak);
        p.rect_stroke(rect, radius(), Stroke::new(1.0, c.accent_card_ring), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_filled(rect, radius(), c.accent_tint);
    }
    let ink = if on { c.accent } else if resp.hovered() { c.ink } else { c.muted };
    let y = rect.center().y;
    p.text(Pos2::new(rect.min.x + 12.0, y), Align2::LEFT_CENTER, g.to_string(), icon_font(14.0), ink);
    let right = rect.max.x - 11.0;
    if let Some(n) = count {
        p.text(Pos2::new(right, y), Align2::RIGHT_CENTER, n.to_string(), mono(10.5), if on { c.accent } else { c.line_number });
    }
    let text_rect = Rect::from_min_max(Pos2::new(rect.min.x + 37.0, rect.min.y), Pos2::new(right - 26.0, rect.max.y));
    p.with_clip_rect(text_rect).text(Pos2::new(rect.min.x + 37.0, y), Align2::LEFT_CENTER, label, super::bold(13.0), ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Interruptor (.toggle de DBMON).
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let c = colors();
    let size = Vec2::new(38.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let r = CornerRadius::same(if appearance().rounded_controls { 11 } else { 3 });
    let fill = if *on { c.accent } else { c.raised };
    ui.painter().rect_filled(rect, r, fill);
    ui.painter().rect_stroke(rect, r, Stroke::new(1.0, if *on { c.accent } else { c.strong_border }), egui::StrokeKind::Inside);
    let knob_r = 8.0;
    let x = egui::lerp((rect.min.x + 11.0)..=(rect.max.x - 11.0), t);
    let knob = Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::splat(knob_r * 2.0));
    ui.painter().rect_filled(knob, CornerRadius::same(if appearance().rounded_controls { 8 } else { 2 }), if *on { c.on_accent } else { c.muted });
    resp
}

/// Fila de opción: título, subtítulo y un interruptor a la derecha, con regla inferior de acento.
pub fn option_row(ui: &mut Ui, title: &str, hint: &str, on: &mut bool, last: bool) -> bool {
    let c = colors();
    let mut changed = false;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let w = ui.available_width() - 50.0;
        ui.vertical(|ui| {
            ui.set_width(w);
            ui.label(RichText::new(title).strong().color(c.ink));
            ui.label(RichText::new(hint).small().color(c.muted));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = toggle(ui, on).changed();
        });
    });
    ui.add_space(8.0);
    if !last {
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, c.accent_row_rule);
    }
    changed
}

/// Atajo como tecla (`kbd`).
pub fn kbd(ui: &mut Ui, text: &str) {
    let c = colors();
    egui::Frame::default()
        .fill(c.canvas)
        .stroke(Stroke::new(1.0, c.strong_border))
        .corner_radius(radius())
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).font(mono(11.0)).color(c.ink));
        });
}

/// Botón normal de la barra (.btn): relleno elevado y borde fino; `accent` = botón principal.
pub fn bar_button(ui: &mut Ui, job: egui::text::LayoutJob, accent: bool, tip: &str) -> Response {
    let c = colors();
    let (fill, stroke) = if accent { (c.accent, c.accent) } else { (c.raised, c.faint_border) };
    let b = egui::Button::new(job).fill(fill).stroke(Stroke::new(1.0, stroke)).corner_radius(radius()).min_size(Vec2::new(0.0, 34.0));
    let r = ui.add(b);
    if tip.is_empty() { r } else { r.on_hover_text(tip) }
}

/// Botón cuadrado solo con ícono (.icon-btn).
pub fn icon_button(ui: &mut Ui, g: char, tip: &str) -> Response {
    let c = colors();
    let b = egui::Button::new(icon(g, 13.0).color(c.ink)).fill(c.raised).stroke(Stroke::new(1.0, c.faint_border)).corner_radius(radius()).min_size(Vec2::new(34.0, 34.0));
    ui.add(b).on_hover_text(tip)
}

/// Segmento clickeable de la barra de estado ("PrtSc capturar").
pub fn status_segment(ui: &mut Ui, key: &str, text: &str, key_color: Color32, tip: &str) -> Response {
    let c = colors();
    let mut job = egui::text::LayoutJob::default();
    job.append(key, 0.0, egui::TextFormat { font_id: mono(11.0), color: key_color, ..Default::default() });
    job.append(text, 6.0, egui::TextFormat { font_id: mono(11.0), color: c.muted, ..Default::default() });
    let r = ui.add(egui::Button::new(job).frame(false));
    if r.hovered() {
        ui.painter().rect_stroke(r.rect.expand(2.0), radius(), Stroke::new(1.0, c.faint_border), egui::StrokeKind::Inside);
    }
    r.on_hover_text(tip)
}

/// Tarjeta "Neo": superficie con sombra suave y borde tenue.
pub fn neo_frame() -> egui::Frame {
    let c = colors();
    egui::Frame::default()
        .fill(c.surface)
        .stroke(Stroke::new(1.0, c.faint_border))
        .corner_radius(panel_radius())
        .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(70) })
}

/// Fondo del área de trabajo: liso, del color del lienzo (las líneas de la grilla Neo Mirai asomaban entre
/// los paneles como rayas sueltas).
pub fn paint_grid(p: &egui::Painter, rect: Rect) {
    p.rect_filled(rect, 0.0, colors().canvas);
}

/// Botón de confirmar (Listo, Aceptar, Guardar…): relleno de acento, tilde dentro de un círculo, texto en
/// negrita y 36 px de alto; al pasar el mouse se aclara y suma un halo del acento.
pub fn confirm_button(ui: &mut Ui, label: &str) -> Response {
    confirm_button_with(ui, glyph::CHECK, label)
}

pub fn confirm_button_with(ui: &mut Ui, g: char, label: &str) -> Response {
    let c = colors();
    let font = super::bold(13.5);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, c.accent);
    let icon_w = 14.0;
    let size = Vec2::new(16.0 + icon_w + 8.0 + galley.size().x + 18.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let down = resp.is_pointer_button_down_on();
    // 0 en reposo, 1 con el mouse encima (transición de 150 ms).
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || down, 0.15);
    let p = ui.painter();
    let r = radius();
    // En reposo: placa suave del acento con borde y texto del acento (como la pestaña elegida de al lado).
    // Al pasar: se llena del acento y el texto pasa al color sobre acento.
    let rest_fill = alpha(c.accent, 0.14);
    let fill = if down { crate::theme::mix(Color32::BLACK, c.accent, 0.18) } else { crate::theme::mix(c.accent, rest_fill_opaque(&c, rest_fill), t) };
    p.rect_filled(rect, r, fill);
    p.rect_stroke(rect, r, Stroke::new(1.0, alpha(c.accent, 0.55 + 0.45 * t)), egui::StrokeKind::Inside);
    let ink = crate::theme::mix(c.on_accent, c.accent, t);
    let x = rect.min.x + 16.0;
    p.text(Pos2::new(x + icon_w / 2.0, rect.center().y + 0.5), Align2::CENTER_CENTER, g.to_string(), icon_font(14.5), ink);
    p.galley_with_override_text_color(Pos2::new(x + icon_w + 8.0, rect.center().y - galley.size().y / 2.0), galley, ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// El tinte translúcido del acento ya compuesto sobre el encabezado (para poder mezclarlo con el acento pleno).
fn rest_fill_opaque(c: &crate::theme::Colors, tint: Color32) -> Color32 {
    let a = tint.a() as f32 / 255.0;
    crate::theme::mix(Color32::from_rgb(c.accent.r(), c.accent.g(), c.accent.b()), c.header, a)
}

/// La línea del borde (la de abajo del encabezado y la de arriba del pie, idénticas): una regla de 1 px y la
/// barra luminosa de 2 px, con el barrido de .NET encima. `sweep_until` corta el barrido (en el encabezado
/// no pasa por la columna de Boberto); la posición se calcula sobre el ancho entero de `rect`, así las dos
/// líneas van exactamente a la par.
pub fn paint_edge_line(ctx: &egui::Context, p: &egui::Painter, rect: Rect, sweep_until: f32) {
    let c = colors();
    p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)), 0.0, c.topbar_rule);
    let bar = Rect::from_min_max(rect.min + Vec2::new(0.0, 1.0), Pos2::new(rect.max.x, rect.min.y + 3.0));
    paint_edge_bar(p, bar);
    paint_edge_sweep(ctx, p, bar, Rect::from_min_max(bar.min, Pos2::new(sweep_until.min(bar.max.x), bar.max.y)));
}

/// El barrido del borde, igual que en .NET (ApplyShellMotion): un degradé que ocupa el 26 % del ancho
/// (transparente en 37 % y 63 %, pleno en 50 %), del acento aclarado un 26 % con alfa 240, que cruza de -65 % a
/// +65 % en 7 s a velocidad constante, sin pausa. `span` da la posición; se dibuja recortado a `clip`.
pub fn paint_edge_sweep(ctx: &egui::Context, p: &egui::Painter, span: Rect, clip: Rect) {
    if !appearance().boberto_motion || span.width() <= 0.0 || clip.width() <= 0.0 {
        return;
    }
    let a = colors().accent;
    let lift = |c: u8| (c as f32 + (255.0 - c as f32) * 0.26).min(255.0) as u8;
    let peak = Color32::from_rgba_unmultiplied(lift(a.r()), lift(a.g()), lift(a.b()), 240);
    let t = (ctx.input(|i| i.time) / 7.0).fract() as f32;
    let shift = (-0.65 + 1.3 * t) * span.width();
    let x = |f: f32| span.min.x + span.width() * f + shift;
    let mut m = egui::Mesh::default();
    for (i, (f, col)) in [(0.37, Color32::TRANSPARENT), (0.5, peak), (0.63, Color32::TRANSPARENT)].into_iter().enumerate() {
        m.colored_vertex(Pos2::new(x(f), span.min.y), col);
        m.colored_vertex(Pos2::new(x(f), span.max.y), col);
        if i > 0 {
            let k = (i as u32) * 2;
            m.add_triangle(k - 2, k - 1, k);
            m.add_triangle(k - 1, k, k + 1);
        }
    }
    p.with_clip_rect(clip).add(Shape::mesh(m));
    // 7 s para cruzar: a 30 cuadros por segundo se ve continuo.
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
}

/// Barra luminosa bajo el encabezado: transparente → acento 45 % → 85 % → 45 % → transparente.
pub fn paint_edge_bar(p: &egui::Painter, rect: Rect) {
    let c = colors().accent;
    let stops = [(0.0, 0.0), (0.14, 0.45), (0.5, 0.85), (0.86, 0.45), (1.0, 0.0)];
    let mut mesh = egui::Mesh::default();
    for (i, (t, a)) in stops.iter().enumerate() {
        let x = rect.min.x + rect.width() * t;
        let col = alpha(c, a * 0.75);
        mesh.colored_vertex(Pos2::new(x, rect.min.y), col);
        mesh.colored_vertex(Pos2::new(x, rect.max.y), col);
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b, b + 1);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// Insignia chica en mono ("FIJADA", "ANOTADA", "VIDEO").
pub fn badge(p: &egui::Painter, at: Pos2, align: Align2, text: &str, filled: bool) -> Rect {
    let c = colors();
    let galley = p.layout_no_wrap(text.to_string(), super::bold(9.0), if filled { c.on_accent } else { c.accent });
    let size = galley.size() + Vec2::new(12.0, 4.0);
    let rect = align.anchor_size(at, size);
    if filled {
        p.rect_filled(rect, radius(), c.accent);
    } else {
        p.rect_filled(rect, radius(), c.header);
        p.rect_stroke(rect, radius(), Stroke::new(1.0, c.accent_card_ring), egui::StrokeKind::Inside);
    }
    p.galley(rect.min + Vec2::new(6.0, 2.0), galley, Color32::WHITE);
    rect
}

/// Menú desplegable bajo un botón (las flechas de Capturar y Grabar, el botón Secciones).
pub fn dropdown(ui: &Ui, resp: &Response, add: impl FnOnce(&mut Ui)) {
    let id = resp.id.with("dropdown");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup::popup_below_widget(ui, id, resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        super::menu_style(ui);
        ui.set_min_width(220.0);
        add(ui);
    });
}