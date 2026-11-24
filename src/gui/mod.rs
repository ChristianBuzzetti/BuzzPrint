//! Interfaz: host de ventanas, tema compartido y piezas comunes.

pub mod ai;
pub mod boberto_view;
pub mod colorpick;
pub mod editor;
pub mod export;
pub mod guide;
mod guide_content;
pub mod host;
pub mod library;
pub mod mark;
pub mod markicon;
pub mod prompt;
pub mod quickpick;
pub mod recbar;
pub mod settings;
pub mod templates;
pub mod traymenu;
pub mod thumbs;
pub mod widgets;

use std::cell::RefCell;

use egui::{Color32, CornerRadius, FontFamily, FontId, RichText, Stroke};

use crate::theme::{self, Appearance, Colors};

struct ThemeState {
    appearance: Appearance,
    colors: Colors,
    rev: u64,
}

thread_local! {
    static THEME: RefCell<Option<ThemeState>> = const { RefCell::new(None) };
}

fn with_theme<R>(f: impl FnOnce(&mut ThemeState) -> R) -> R {
    THEME.with(|t| {
        let mut t = t.borrow_mut();
        if t.is_none() {
            let appearance = Appearance::load();
            let colors = Colors::resolve(&appearance);
            *t = Some(ThemeState { appearance, colors, rev: 1 });
        }
        f(t.as_mut().unwrap())
    })
}

pub fn appearance() -> Appearance {
    with_theme(|t| t.appearance.clone())
}

pub fn colors() -> Colors {
    with_theme(|t| t.colors.clone())
}

pub fn theme_rev() -> u64 {
    with_theme(|t| t.rev)
}

/// Cambia la apariencia, la guarda y repinta todas las ventanas abiertas.
pub fn set_appearance(a: Appearance) {
    let accent_changed = with_theme(|t| {
        if t.appearance == a {
            return false;
        }
        let before = t.colors.accent;
        t.colors = Colors::resolve(&a);
        t.appearance = a;
        t.appearance.save();
        t.rev += 1;
        before != t.colors.accent
    });
    host::wake_all();
    if accent_changed {
        // El ícono de la bandeja y de las ventanas lleva el acento en la placa.
        crate::app::post(crate::app::Cmd::Run(Box::new(crate::app::refresh_icons)));
    }
}

pub fn style_context(ctx: &egui::Context) {
    let a = appearance();
    theme::apply(ctx, &a);
}

// ------------------------------------------------------------------ piezas comunes

/// Tamaño de letra de la interfaz: los tamaños del código están pensados para 13 pt y siguen al tamaño
/// elegido en Personalizar, en píxeles enteros y nunca por debajo de 11 (más chico no se lee).
pub fn px(size: f32) -> f32 {
    let ui = with_theme(|t| t.appearance.ui_size) as f32;
    (size + ui - 13.0).round().max(11.0)
}

pub fn prop(size: f32) -> FontId {
    FontId::new(px(size), FontFamily::Proportional)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(px(size), FontFamily::Name("bold".into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::new(px(size), FontFamily::Monospace)
}

/// La marca de BuzzPrint en todas las ventanas: el logo vivo (ojos que siguen al cursor) y "BUZZPRINT"
/// (BUZZ en tinta, PRINT en acento). Al hacerle clic, el logo da una vuelta y una ola recorre las letras:
/// cada una salta 3 px y se enciende, 55 ms después de la anterior, mientras una línea del acento se
/// dibuja debajo de izquierda a derecha y se apaga. Sin animaciones (Personalizar), queda quieta.
pub fn brand(ui: &mut egui::Ui, mark_size: f32, text_size: f32) -> egui::Response {
    brand_with(ui, mark_size, text_size, false)
}

/// La marca de la biblioteca, como en .NET: las letras en tinta tenue con la ola del acento.
pub fn brand_neon(ui: &mut egui::Ui, mark_size: f32, text_size: f32) -> egui::Response {
    brand_with(ui, mark_size, text_size, true)
}

fn brand_with(ui: &mut egui::Ui, mark_size: f32, text_size: f32, neon: bool) -> egui::Response {
    let c = colors();
    let motion = appearance().boberto_motion;
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let m = mark::live(ui, mark_size, c.accent, true);
        ui.add_space((mark_size * 0.16).max(6.0));
        let font = bold(text_size);
        let galleys: Vec<_> = "BUZZPRINT"
            .chars()
            .enumerate()
            .map(|(i, ch)| ui.painter().layout_no_wrap(ch.to_string(), font.clone(), if i < 4 { c.ink } else { c.accent }))
            .collect();
        let w: f32 = galleys.iter().map(|g| g.size().x).sum::<f32>() + 1.0;
        let h = galleys.first().map(|g| g.size().y).unwrap_or(text_size + 4.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h + 4.0), egui::Sense::click());
        let id = m.id.with("brand-wave");
        let now = ui.input(|i| i.time);
        if motion && (resp.clicked() || m.clicked()) {
            ui.ctx().data_mut(|d| d.insert_temp(id, now));
            mark::spin_now(ui.ctx(), m.id);
        }
        let started = ui.ctx().data(|d| d.get_temp::<f64>(id));
        let el = started.map(|t0| (now - t0) as f32).filter(|e| *e < 1.1);
        let p = ui.painter();
        let ease = |t: f32| 1.0 - (1.0 - t).powi(3);
        let mut x = rect.min.x;
        let base_y = rect.min.y + 2.0;
        // La ola de .NET (.logo-wave de DBMON): letras en tinta tenue; cada una se enciende con el acento al 30 %
        // de un ciclo de 3 s, 0,18 s después de la anterior.
        let wave_t = (neon && motion).then(|| ui.input(|i| i.time));
        for (i, g) in galleys.into_iter().enumerate() {
            let gw = g.size().x;
            let base = if i < 4 { c.ink } else { c.accent };
            let (mut col, mut lift) = (base, 0.0);
            if let Some(e) = el {
                let t = ((e - i as f32 * 0.055) / 0.42).clamp(0.0, 1.0);
                let bump = (std::f32::consts::PI * ease(t)).sin();
                lift = -3.0 * bump;
                let hi = if i < 4 { c.accent } else { theme::mix(egui::Color32::WHITE, c.accent, 0.55) };
                col = theme::mix(hi, base, bump);
            }
            if let Some(now) = wave_t {
                if el.is_none() {
                    let start = i as f64 * 0.18;
                    let f = if now < start { 0.0 } else { (((now - start) / 3.0).fract()) as f32 };
                    let s3 = |u: f32| u * u * (3.0 - 2.0 * u);
                    let k = if f < 0.3 { s3(f / 0.3) } else { 1.0 - s3((f - 0.3) / 0.7) };
                    col = theme::mix(c.accent, c.muted, k);
                }
            }
            p.galley_with_override_text_color(egui::pos2(x, base_y + lift), g, col);
            x += gw;
        }
        if wave_t.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }
        if let Some(e) = el {
            // Línea que se dibuja debajo y se apaga.
            let grow = ease((e / 0.55).min(1.0));
            let fade = 1.0 - ((e - 0.55) / 0.5).clamp(0.0, 1.0);
            let y = rect.max.y - 1.0;
            p.line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.min.x + w * grow, y)], Stroke::new(1.5, theme::alpha(c.accent, 0.9 * fade)));
            ui.ctx().request_repaint();
        } else if started.is_some() {
            ui.ctx().data_mut(|d| d.remove::<f64>(id));
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("BuzzPrint")
    });
    row.inner
}

pub fn theme_alpha(c: Color32, a: f32) -> Color32 {
    theme::alpha(c, a)
}

/// Triángulo de esquina superior-izquierda (reemplaza `◤` sin depender de seguisym.ttf).
pub fn corner_tri(ui: &mut egui::Ui, size: f32, color: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().add(egui::Shape::convex_polygon(vec![r.left_top(), r.right_top(), r.left_bottom()], color, Stroke::NONE));
}

/// Manija de arrastre de 2×3 puntos (reemplaza `⠿` sin depender de seguisym.ttf).
pub fn grip_dots(p: &egui::Painter, center: egui::Pos2, color: Color32) {
    let (dx, dy) = (2.4_f32, 3.2_f32);
    for row in -1..=1 {
        for col in [-1.0_f32, 1.0_f32] {
            p.circle_filled(center + egui::vec2(col * dx, row as f32 * dy), 1.15, color);
        }
    }
}

/// Rótulo de sección estilo DBMON: triángulo + TÍTULO en mono y acento.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    let c = colors();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        corner_tri(ui, 7.5, c.accent);
        ui.label(RichText::new(text.to_uppercase()).font(mono(11.0)).color(c.accent).strong());
    });
}

pub fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(colors().muted));
}

/// Botón principal (relleno de acento).
pub fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let c = colors();
    // Mismo tamaño de letra que los demás botones (en negrita): así el principal y "Cancelar" quedan parejos.
    let size = egui::TextStyle::Button.resolve(ui.style()).size;
    ui.add(
        egui::Button::new(RichText::new(text).color(c.on_accent).font(FontId::new(size, FontFamily::Name("bold".into()))))
            .fill(c.accent)
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(0.0, 34.0)),
    )
}

pub fn card_frame() -> egui::Frame {
    let c = colors();
    let r = if appearance().rounded { 12 } else { 0 };
    egui::Frame::default()
        .fill(c.surface)
        .stroke(Stroke::new(1.0, theme::alpha(c.accent, 0.32)))
        .corner_radius(CornerRadius::same(r))
        .inner_margin(egui::Margin::same(14))
}

pub fn chip(ui: &mut egui::Ui, text: &str, on: bool) -> egui::Response {
    let c = colors();
    let (fill, ink) = if on { (c.accent, c.on_accent) } else { (c.raised, c.ink) };
    ui.add(
        egui::Button::new(RichText::new(text).font(if on { bold(13.5) } else { prop(13.5) }).color(ink))
            .fill(fill)
            .min_size(egui::vec2(0.0, 30.0))
            .stroke(Stroke::new(1.0, if on { c.accent } else { theme::alpha(c.ink, 0.12) }))
            .corner_radius(CornerRadius::same(if appearance().rounded_controls { 14 } else { 2 })),
    )
}

pub fn accent_rule(ui: &mut egui::Ui) {
    let c = colors();
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.5), egui::Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::alpha(c.accent, 0.45));
}

pub fn badge_color(c: &Colors) -> Color32 {
    c.badge
}

/// Letra y espacio de los menús (barra, clic derecho y desplegables): más grandes que el cuerpo, como en .NET.
pub fn menu_style(ui: &mut egui::Ui) {
    let size = appearance().ui_size as f32 + 1.0;
    let s = ui.style_mut();
    for st in [egui::TextStyle::Body, egui::TextStyle::Button] {
        s.text_styles.insert(st, FontId::new(size, FontFamily::Proportional));
    }
    s.spacing.button_padding = egui::vec2(12.0, 7.0);
    s.spacing.item_spacing.y = 1.0;
    s.spacing.interact_size.y = 32.0;
    // Reglas (`ui.separator()`) tenues, como el Separator de .NET.
    let c = colors();
    s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c.faint_border);
    // Submenús (Idioma, Mover a sección): fila plana con tinte al pasar, igual que las entradas.
    let tint = theme::alpha(c.accent, 0.14);
    s.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    s.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    s.visuals.widgets.hovered.weak_bg_fill = tint;
    s.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    s.visuals.widgets.open.weak_bg_fill = tint;
    s.visuals.widgets.open.bg_stroke = Stroke::NONE;
    ui.set_min_width(200.0);
}

/// Rótulo chico dentro de un menú, en mayúsculas y gris.
pub fn menu_heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(text.to_uppercase()).font(bold(10.0)).color(colors().line_number));
    });
    ui.add_space(2.0);
}

/// Entrada de menú: texto a la izquierda y atajo a la derecha, en gris y monoespaciado.
pub fn menu_item(ui: &mut egui::Ui, label: &str, key: &str, enabled: bool) -> egui::Response {
    let ink = if enabled { colors().ink } else { theme::alpha(colors().ink, 0.4) };
    menu_item_with(ui, None, label, "", key, enabled, ink, false)
}

/// Entrada con ícono de Segoe Fluent a la izquierda y, si se pasa, una línea de ayuda debajo.
pub fn menu_item_icon(ui: &mut egui::Ui, glyph: char, label: &str, hint: &str, key: &str, enabled: bool) -> egui::Response {
    let ink = if enabled { colors().ink } else { theme::alpha(colors().ink, 0.4) };
    menu_item_with(ui, Some(glyph), label, hint, key, enabled, ink, false)
}

/// Entrada con ícono y otro color (Eliminar en rojo): el ícono toma el mismo color.
pub fn menu_item_icon_tinted(ui: &mut egui::Ui, glyph: char, label: &str, key: &str, ink: Color32) -> egui::Response {
    menu_item_with(ui, Some(glyph), label, "", key, true, ink, false)
}

/// Entrada con tilde (Panel de opciones, idioma): la tilde de acento va en el margen izquierdo.
pub fn menu_check(ui: &mut egui::Ui, label: &str, on: bool) -> egui::Response {
    menu_item_with(ui, None, label, "", "", true, colors().ink, on)
}

#[allow(clippy::too_many_arguments)]
fn menu_item_with(ui: &mut egui::Ui, glyph: Option<char>, label: &str, hint: &str, key: &str, enabled: bool, ink: Color32, checked: bool) -> egui::Response {
    let c = colors();
    let font = egui::TextStyle::Button.resolve(ui.style());
    let label_g = ui.painter().layout_no_wrap(label.to_string(), font.clone(), ink);
    let hint_g = (!hint.is_empty()).then(|| ui.painter().layout_no_wrap(hint.to_string(), FontId::proportional((font.size - 2.5).max(11.0)), c.muted));
    let key_g = ui.painter().layout_no_wrap(key.to_string(), mono(font.size - 2.0), c.muted);
    // Margen izquierdo para el ícono o la tilde (la columna de 20 px del MenuItem de .NET).
    let gutter = if glyph.is_some() { 42.0 } else { 30.0 };
    let text_w = label_g.size().x.max(hint_g.as_ref().map(|g| g.size().x).unwrap_or(0.0));
    let need = gutter + text_w + if key.is_empty() { 0.0 } else { key_g.size().x + 44.0 } + 18.0;
    ui.set_min_width(need);
    let base_h = ui.spacing().interact_size.y.max(font.size + 14.0);
    let h = match &hint_g {
        Some(g) => (label_g.size().y + g.size().y + 16.0).max(base_h),
        None => base_h,
    };
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(need), h), sense);
    let hot = enabled && resp.hovered();
    let p = ui.painter();
    if hot {
        let r = CornerRadius::same(if appearance().rounded_controls { 6 } else { 0 });
        p.rect_filled(rect, r, theme::alpha(c.accent, 0.14));
        // Barrita de acento a la izquierda: marca cuál se va a elegir.
        let bar = egui::Rect::from_min_size(egui::pos2(rect.left() + 2.0, rect.top() + 8.0), egui::vec2(3.0, rect.height() - 16.0));
        p.rect_filled(bar, CornerRadius::same(2), c.accent);
    }
    let x = rect.left() + gutter;
    let (label_y, hint_y) = match &hint_g {
        Some(hg) => {
            let top = rect.top() + (h - label_g.size().y - hg.size().y - 2.0) / 2.0;
            (top, top + label_g.size().y + 2.0)
        }
        None => (rect.center().y - label_g.size().y / 2.0, 0.0),
    };
    if let Some(g) = glyph {
        let gi = if ink != c.ink && enabled { ink } else if hot { c.accent } else if enabled { c.muted } else { theme::alpha(c.muted, 0.5) };
        p.text(egui::pos2(rect.left() + 21.0, label_y + label_g.size().y / 2.0), egui::Align2::CENTER_CENTER, g.to_string(), widgets::icon_font(15.0), gi);
    }
    if checked {
        p.text(egui::pos2(rect.left() + 16.0, rect.center().y), egui::Align2::CENTER_CENTER, '\u{E73E}'.to_string(), widgets::icon_font(12.0), c.accent);
    }
    let label_mid = label_y + label_g.size().y / 2.0;
    p.galley(egui::pos2(x, label_y), label_g, ink);
    if let Some(hg) = hint_g {
        p.galley(egui::pos2(x, hint_y), hg, c.muted);
    }
    if !key.is_empty() {
        // Atajo como tecla (kbd): placa del lienzo con borde fino.
        let ks = key_g.size() + egui::vec2(12.0, 6.0);
        let kr = egui::Rect::from_min_size(egui::pos2(rect.right() - 10.0 - ks.x, label_mid - ks.y / 2.0), ks);
        p.rect_filled(kr, CornerRadius::same(4), c.canvas);
        p.rect_stroke(kr, CornerRadius::same(4), Stroke::new(1.0, c.faint_border), egui::StrokeKind::Inside);
        p.galley(kr.min + egui::vec2(6.0, 3.0), key_g, c.muted);
    }
    let resp = if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    if resp.clicked() {
        ui.close_menu();
    }
    resp
}

/// Submenú con ícono (Mover a sección, IA). egui dibuja la flecha con "⏵", que nuestras fuentes no tienen
/// (sale "?"): se tapa y se pinta el chevron de Segoe Fluent en su lugar.
pub fn submenu<R>(ui: &mut egui::Ui, glyph: char, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> Option<R> {
    let c = colors();
    let size = egui::TextStyle::Button.resolve(ui.style()).size;
    let ir = ui.menu_button(widgets::icon_label(glyph, &format!("  {label}"), size, c.ink), add);
    let r = ir.response.rect;
    let hot = ir.response.hovered() || ir.inner.is_some();
    let pad = ui.spacing().button_padding.x;
    let cover = egui::Rect::from_min_max(egui::pos2(r.right() - pad - 18.0, r.top() + 2.0), egui::pos2(r.right() - pad + 3.0, r.bottom() - 2.0));
    let p = ui.painter();
    p.rect_filled(cover, 0.0, ui.visuals().window_fill);
    if hot {
        p.rect_filled(cover, 0.0, theme::alpha(c.accent, 0.14));
    }
    p.text(egui::pos2(r.right() - pad - 2.0, r.center().y), egui::Align2::RIGHT_CENTER, widgets::glyph::CHEVRON_RIGHT.to_string(), widgets::icon_font(11.0), if hot { c.accent } else { c.muted });
    ir.inner
}

/// Tamaño de letra del texto elegido en los desplegables.
pub fn combo_size() -> f32 {
    appearance().ui_size as f32 + 1.0
}

/// Campo de texto de una línea con relleno cómodo (el de egui, 4×2 px, queda finito al lado de los botones).
pub fn field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).margin(egui::Margin::symmetric(8, 6))
}

/// Campo de texto de varias líneas con el mismo relleno.
pub fn area(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::multiline(text).margin(egui::Margin::symmetric(8, 6))
}
