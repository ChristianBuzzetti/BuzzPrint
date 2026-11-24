//! Plantillas de documento: galería con vista previa (portada y una página de paso), editor con vista previa
//! en vivo e importación de logo. Port de TemplatesWindow.cs, TemplatePreview.cs y TemplateEditWindow.cs.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Vec2};
use windows_sys::Win32::Foundation::HWND;

use super::host::{self, Options, View, WinCtl};
use super::{bold, caption, colors, muted, primary};
use crate::app::{post, Cmd};
use crate::export::layout::Src;
use crate::export::{self, templates, Format, Template};
use crate::i18n::{f, t};
use crate::store;

// ------------------------------------------------------------------ vista previa

/// Logos de las plantillas ya cargados como textura (por nombre de archivo).
#[derive(Default)]
pub struct Logos(HashMap<String, Option<(TextureHandle, Vec2)>>);

impl Logos {
    pub fn get(&mut self, ctx: &egui::Context, tp: &Template) -> Option<(egui::TextureId, Vec2)> {
        let path = tp.logo_path()?;
        let entry = self.0.entry(tp.logo.clone()).or_insert_with(|| {
            let img = crate::image::load_png(&path).ok()?;
            let size = Vec2::new(img.w as f32, img.h as f32);
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.w as usize, img.h as usize], &img.px);
            Some((ctx.load_texture(format!("tpl-logo-{}", tp.logo), ci, egui::TextureOptions::LINEAR), size))
        });
        entry.as_ref().map(|(h, s)| (h.id(), *s))
    }
    /// Proporción ancho/alto del logo (lo que necesita el diseño).
    pub fn aspect(&mut self, ctx: &egui::Context, tp: &Template) -> Option<f32> {
        self.get(ctx, tp).map(|(_, s)| s.x / s.y.max(1.0))
    }
}

fn hex(h: &str) -> Color32 {
    crate::theme::parse_hex(&format!("#{}", h.trim_start_matches('#')))
}

fn font_for(name: &str, size: f32, heading: bool) -> FontId {
    let n = name.to_ascii_lowercase();
    if n.contains("consolas") || n.contains("courier") || n.contains("mono") {
        FontId::new(size, FontFamily::Monospace)
    } else if heading {
        FontId::new(size, FontFamily::Name("bold".into()))
    } else {
        FontId::new(size, FontFamily::Proportional)
    }
}

/// Una captura de mentira (cuando no hay imagen real, como en la galería): barra, menú lateral y contenido.
fn mock_shot(p: &egui::Painter, r: Rect, accent: Color32) {
    let u = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(r.min + Vec2::new(x * r.width(), y * r.height()), Vec2::new(w * r.width(), h * r.height()));
    p.rect_filled(r, 0.0, Color32::from_rgb(0xE8, 0xEC, 0xF2));
    p.rect_filled(u(0.0, 0.0, 1.0, 0.12), 0.0, Color32::from_rgb(0xCD, 0xD5, 0xE0));
    p.rect_filled(u(0.03, 0.2, 0.24, 0.72), 0.0, Color32::from_rgb(0xD9, 0xDF, 0xE8));
    p.rect_filled(u(0.33, 0.2, 0.6, 0.08), 0.0, Color32::from_rgb(0xC2, 0xCB, 0xD8));
    p.rect_filled(u(0.33, 0.34, 0.5, 0.05), 0.0, Color32::from_rgb(0xD0, 0xD7, 0xE2));
    p.rect_filled(u(0.33, 0.43, 0.55, 0.05), 0.0, Color32::from_rgb(0xD0, 0xD7, 0xE2));
    p.rect_filled(u(0.33, 0.55, 0.2, 0.09), 0.0, accent);
}

/// Dibuja la página `n` de un documento diseñado en `rect` (con su fondo y sombra). `tex`: la imagen de
/// cada captura o del logo; sin imagen, la captura se dibuja de mentira.
pub fn paint_page(p: &egui::Painter, rect: Rect, doc: &export::layout::Doc, n: usize, tp: &Template, accent: &str, tex: &mut dyn FnMut(Src) -> Option<(egui::TextureId, Vec2)>) {
    use export::layout::{Al, El, Va};
    let s = rect.width() / doc.w;
    p.rect_filled(rect.translate(Vec2::new(0.0, 2.0)).expand(1.0), CornerRadius::same(3), Color32::from_black_alpha(60));
    p.rect_filled(rect, CornerRadius::same(2), hex(&doc.page));
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let at = |r: &export::layout::R| Rect::from_min_size(rect.min + Vec2::new(r.x, r.y) * s, Vec2::new(r.w, r.h) * s);
    let Some(els) = doc.pages.get(n) else { return };
    for el in els {
        match el {
            El::Fill { r, color, radius } => {
                p.rect_filled(at(r), CornerRadius::same((radius * s).round().min(120.0) as u8), hex(color));
            }
            El::Line { r, color, width, radius } => {
                p.rect_stroke(at(r), CornerRadius::same((radius * s).round().min(120.0) as u8), Stroke::new((width * s).max(0.6), hex(color)), egui::StrokeKind::Middle);
            }
            El::Dot { r, color } => {
                let rr = at(r);
                p.circle_filled(rr.center(), rr.width().min(rr.height()) / 2.0, hex(color));
            }
            El::Text { r, text, size, head, color, al, va, lines } => {
                let rr = at(r);
                let px = size * s;
                let col = hex(color);
                if px < 3.2 {
                    // Muy chico para leerse: rayitas donde va el texto.
                    let n_lines = export::layout::est_lines(text, *size, r.w).clamp(1, *lines.max(&1)) as f32;
                    let lh = (px * 1.3).max(1.2);
                    let total = n_lines * lh;
                    let top = match va {
                        Va::T => rr.top(),
                        Va::M => rr.center().y - total / 2.0,
                        Va::B => rr.bottom() - total,
                    };
                    for k in 0..n_lines as usize {
                        let w = (export::layout::est(text, *size) * s).min(rr.width()) * if k + 1 == n_lines as usize && n_lines > 1.0 { 0.6 } else { 1.0 };
                        let x = match al {
                            Al::L => rr.left(),
                            Al::C => rr.center().x - w / 2.0,
                            Al::R => rr.right() - w,
                        };
                        p.rect_filled(Rect::from_min_size(egui::pos2(x, top + k as f32 * lh), Vec2::new(w, (px * 0.7).max(0.8))), 0.0, crate::theme::alpha(col, 0.55));
                    }
                    continue;
                }
                let font = font_for(if *head { &tp.heading_font } else { &tp.body_font }, px, *head);
                let mut job = egui::text::LayoutJob::simple(text.clone(), font, col, rr.width().max(1.0));
                job.wrap.max_rows = (*lines).max(1);
                job.wrap.overflow_character = Some('…');
                job.halign = match al {
                    Al::L => egui::Align::LEFT,
                    Al::C => egui::Align::Center,
                    Al::R => egui::Align::RIGHT,
                };
                let g = p.layout_job(job);
                let gh = g.size().y.min(rr.height().max(px * 1.3));
                let y = match va {
                    Va::T => rr.top(),
                    Va::M => rr.center().y - gh / 2.0,
                    Va::B => rr.bottom() - gh,
                };
                let x = match al {
                    Al::L => rr.left(),
                    Al::C => rr.center().x,
                    Al::R => rr.right(),
                };
                p.galley(egui::pos2(x, y), g, col);
            }
            El::Img { r, src } => {
                let rr = at(r);
                match tex(*src) {
                    Some((id, _)) => {
                        p.image(id, rr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    None if matches!(src, Src::Cap(_)) => mock_shot(&p, rr, hex(accent)),
                    None => {}
                }
            }
        }
    }
}

/// Tres capturas de ejemplo para la galería (sin imágenes reales).
pub fn sample_shots() -> Vec<export::layout::Shot> {
    let mk = |title: &str, note: &str, w: u32, h: u32| export::layout::Shot { title: t(title), note: t(note), meta: "BuzzPrint · 20/11/2026 10:24 · 1280×760".into(), w, h };
    vec![
        mk("Abrir la consola", "Entrá con tu usuario y elegí el ambiente de prueba.", 1280, 760),
        mk("Revisar la configuración", "Confirmá que los valores coincidan con el pedido.", 1280, 800),
        mk("Guardar los cambios", "Tocá Guardar y esperá el aviso de confirmación.", 1180, 700),
    ]
}

/// Opciones de ejemplo para las vistas previas de la galería.
pub fn sample_opts(tp: &Template, app_accent: &str) -> export::Opts {
    export::Opts {
        title: t("Título del documento"),
        intro: t("Guía paso a paso con las capturas de la tarea."),
        cover: true,
        numbers: true,
        captions: true,
        meta: true,
        one_per_page: true,
        landscape: false,
        accent: tp.accent_hex(app_accent),
        t: tp.normalized(),
    }
}

/// El formato con que se muestra una plantilla en la galería (las propias sin formato, como PDF).
pub fn format_of(tp: &Template) -> Format {
    match tp.format.as_str() {
        "pptx" => Format::Pptx,
        "docx" => Format::Docx,
        _ => Format::Pdf,
    }
}

/// Miniatura de una plantilla: portada y primera página, con capturas de ejemplo. `page_w`: ancho de cada página.
pub fn preview(ui: &mut egui::Ui, tp: &Template, app_accent: &str, page_w: f32, logos: &mut Logos) -> egui::Response {
    let format = format_of(tp);
    let ctx = ui.ctx().clone();
    let o = sample_opts(tp, app_accent);
    let doc = export::layout::design(&sample_shots(), &o, format, logos.aspect(&ctx, &o.t));
    let ph = page_w * doc.h / doc.w;
    let gap = (page_w * 0.06).max(6.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(page_w * 2.0 + gap, ph + 4.0), Sense::click());
    let mut tex = |s: Src| if s == Src::Logo { logos.get(&ctx, &o.t) } else { None };
    for k in 0..2.min(doc.pages.len()) {
        let r = Rect::from_min_size(rect.min + Vec2::new(k as f32 * (page_w + gap), 0.0), Vec2::new(page_w, ph));
        paint_page(ui.painter(), r, &doc, k, &o.t, &o.accent, &mut tex);
    }
    resp
}

// ------------------------------------------------------------------ editor

thread_local! {
    /// Logo recién importado (el diálogo de Windows corre fuera del frame y deja el resultado acá).
    static PICKED_LOGO: RefCell<Option<Result<String, String>>> = const { RefCell::new(None) };
}

pub enum EditResult {
    Open,
    Cancelled,
    Saved(Template),
}

/// Editor de una plantilla propia: formulario a la izquierda y vista previa en vivo a la derecha.
pub struct Editor {
    pub tp: Template,
    heading: String,
    intro: String,
    error: String,
    logo_note: String,
}

impl Editor {
    pub fn new(tp: Template, heading: &str, intro: &str) -> Editor {
        let note = if tp.logo_path().is_some() { t("Logo cargado.") } else { t("Sin logo.") };
        Editor { tp, heading: heading.into(), intro: intro.into(), error: String::new(), logo_note: note }
    }

    /// Crear desde cero (a partir de "Informe ejecutivo"), como .NET.
    pub fn create(kind: &str) -> Editor {
        let mut tp = templates::builtins().into_iter().find(|b| b.format == kind).unwrap_or_else(|| templates::builtins().remove(0));
        tp.id.clear();
        tp.name = t("Mi plantilla");
        tp.description = t("Plantilla propia.");
        Editor::new(tp, &t("Crear plantilla"), &t("Elegí colores, tipografías, portada, diseño de página, marco, pie y logo. La vista previa muestra cómo queda."))
    }

    pub fn duplicate(of: &Template) -> Editor {
        let mut tp = of.clone();
        let base = if of.built_in() { t(&of.name) } else { of.name.clone() };
        tp.name = format!("{base} {}", t("(copia)"));
        if of.built_in() {
            tp.description = t(&of.description);
        }
        tp.id.clear();
        Editor::new(tp, &t("Nueva plantilla"), &t("Ajustá colores, tipografías y estilo. Se guarda como plantilla propia."))
    }

    pub fn edit(tp: &Template) -> Editor {
        Editor::new(tp.clone(), &t("Editar plantilla"), &t("Los cambios se aplican a los próximos exports con esta plantilla."))
    }

    pub fn show(&mut self, ctx: &egui::Context, hwnd: HWND, logos: &mut Logos) -> EditResult {
        if let Some(r) = PICKED_LOGO.with(|p| p.borrow_mut().take()) {
            match r {
                Ok(name) => {
                    self.tp.logo = name;
                    self.logo_note = t("Logo cargado.");
                }
                Err(e) => self.logo_note = e,
            }
        }
        let mut result = EditResult::Open;
        let app_accent = export::app_accent();
        let modal = egui::Modal::new(egui::Id::new("tpl-editor")).show(ctx, |ui| {
            let screen = ctx.screen_rect();
            ui.set_width((screen.width() - 80.0).clamp(560.0, 1000.0));
            ui.label(RichText::new(&self.heading).font(bold(18.0)));
            muted(ui, &self.intro);
            ui.add_space(8.0);
            let max_h = (screen.height() - 200.0).max(300.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(470.0);
                    egui::ScrollArea::vertical().id_salt("tpl_form").max_height(max_h).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).show(ui, |ui| self.form(ui, hwnd));
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    caption(ui, &t("Vista previa"));
                    let w = ((ui.available_width() - 12.0) / 2.05).clamp(120.0, 230.0);
                    preview(ui, &self.tp, &app_accent, w, logos);
                    ui.add_space(6.0);
                    muted(ui, t("Portada y una página de paso, con tus colores, tipografías y logo."));
                });
            });
            if !self.error.is_empty() {
                ui.colored_label(colors().error, &self.error);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary(ui, &t("Guardar")).clicked() {
                    match templates::save(&self.tp) {
                        Ok(saved) => result = EditResult::Saved(saved),
                        Err(e) => self.error = e,
                    }
                }
                if ui.button(t("Cancelar")).clicked() {
                    result = EditResult::Cancelled;
                }
            });
        });
        if modal.should_close() && matches!(result, EditResult::Open) {
            result = EditResult::Cancelled;
        }
        result
    }

    fn form(&mut self, ui: &mut egui::Ui, hwnd: HWND) {
        let tp = &mut self.tp;
        egui::Grid::new("tpl-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(t("Nombre"));
            ui.add(super::field(&mut tp.name).desired_width(320.0));
            ui.end_row();
            ui.label(t("Descripción"));
            ui.add(super::field(&mut tp.description).desired_width(320.0));
            ui.end_row();
            let mut app = tp.accent == "app";
            ui.label(t("Acento"));
            ui.horizontal(|ui| {
                if ui.checkbox(&mut app, t("el de la app")).changed() {
                    tp.accent = if app { "app".into() } else { export::app_accent() };
                }
                if !app {
                    color_field(ui, &mut tp.accent);
                }
            });
            ui.end_row();
            for (label, field) in [("Texto", &mut tp.ink), ("Texto suave", &mut tp.muted), ("Texto tenue", &mut tp.faint), ("Líneas", &mut tp.rule), ("Página", &mut tp.page), ("Texto de portada", &mut tp.cover_ink)] {
                ui.label(t(label));
                color_field(ui, field);
                ui.end_row();
            }
            ui.label(t("Relleno de portada"));
            ui.horizontal(|ui| {
                let mut acc = tp.cover_fill.is_empty();
                if ui.checkbox(&mut acc, t("acento")).changed() {
                    tp.cover_fill = if acc { String::new() } else { "13233A".into() };
                }
                if !acc {
                    color_field(ui, &mut tp.cover_fill);
                }
            });
            ui.end_row();
            ui.label(t("Marco de las capturas"));
            ui.horizontal(|ui| {
                for (v, l) in [("none", "Sin marco"), ("accent", "Acento")] {
                    if ui.selectable_label(tp.frame == v, t(l)).clicked() {
                        tp.frame = v.into();
                    }
                }
                if ui.selectable_label(tp.frame.len() == 6, t("Color")).clicked() && tp.frame.len() != 6 {
                    tp.frame = "D1D5DB".into();
                }
                if tp.frame.len() == 6 {
                    color_field(ui, &mut tp.frame);
                }
            });
            ui.end_row();
            font_field(ui, &t("Fuente de títulos"), &mut tp.heading_font);
            font_field(ui, &t("Fuente del texto"), &mut tp.body_font);
            choice(ui, &t("Formato"), &mut tp.format, &[("pptx", "PowerPoint"), ("docx", "Word"), ("pdf", "PDF e imagen"), ("", "Todos")]);
            let word = tp.format == "docx";
            if word {
                choice(ui, &t("Portada"), &mut tp.cover, &[("bar", "Barra"), ("band", "Banda"), ("full", "Completa"), ("minimal", "Mínima"), ("hero", "Con imagen")]);
                choice(ui, &t("Diseño de página"), &mut tp.layout, &[("stack", "Apilado"), ("split", "Texto a la izquierda"), ("split-right", "Texto a la derecha"), ("card", "Recuadro")]);
            } else {
                choice(ui, &t("Portada"), &mut tp.cover, &[("bar", "Barra"), ("band", "Banda"), ("full", "Completa"), ("minimal", "Mínima"), ("hero", "Con imagen"), ("split", "Índice"), ("frame", "Con marco")]);
                choice(ui, &t("Diseño de página"), &mut tp.layout, &[("stack", "Apilado"), ("split", "Texto a la izquierda"), ("split-right", "Texto a la derecha"), ("full", "Captura a lo ancho"), ("card", "Tarjeta"), ("sidebar", "Columna lateral")]);
            }
            choice(ui, &t("Pasos"), &mut tp.steps, &[("inline", "En línea"), ("pill", "Etiqueta"), ("number", "Número grande"), ("circle", "Círculo")]);
            choice(ui, &t("Títulos de paso"), &mut tp.header, &[("rule", "Con línea"), ("band", "Banda"), ("plain", "Simple")]);
            choice(ui, &t("Pie"), &mut tp.footer_style, &[("text", "Texto"), ("bar", "Barra")]);
            choice(ui, &t("Estilo del marco"), &mut tp.frame_style, &[("line", "Línea"), ("shadow", "Sombra"), ("window", "Ventana")]);
            ui.label(t("Línea superior"));
            ui.checkbox(&mut tp.top_rule, "");
            ui.end_row();
            ui.label(t("Pie de página"));
            ui.add(super::field(&mut tp.footer).desired_width(320.0));
            ui.end_row();
            ui.label(t("Organización"));
            ui.add(super::field(&mut tp.org).desired_width(320.0));
            ui.end_row();
            ui.label(t("Logo"));
            ui.horizontal(|ui| {
                if ui.button(t("Elegir logo…")).clicked() {
                    let owner = hwnd as isize;
                    post(Cmd::Run(Box::new(move || {
                        let filter = t("Imágenes");
                        if let Some(p) = crate::dialogs::open_file(owner as HWND, &t("Elegir logo"), &[(&filter, "*.png;*.jpg;*.jpeg;*.bmp;*.gif")]) {
                            let r = templates::import_logo(&p);
                            PICKED_LOGO.with(|x| *x.borrow_mut() = Some(r));
                            host::wake(owner as HWND);
                        }
                    })));
                }
                if ui.add_enabled(!tp.logo.is_empty(), egui::Button::new(t("Quitar"))).clicked() {
                    tp.logo.clear();
                    self.logo_note = t("Sin logo.");
                }
                ui.label(RichText::new(&self.logo_note).color(colors().muted));
            });
            ui.end_row();
        });
    }
}

const FONTS: [&str; 14] = ["Segoe UI", "Segoe UI Semibold", "Segoe UI Semilight", "Segoe UI Black", "Calibri", "Arial", "Verdana", "Tahoma", "Trebuchet MS", "Georgia", "Times New Roman", "Cambria", "Consolas", "Courier New"];

fn font_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    egui::ComboBox::from_id_salt(label).width(220.0).selected_text(RichText::new(value.as_str()).size(super::combo_size())).show_ui(ui, |ui| {
        super::menu_style(ui);
        for f in FONTS {
            ui.selectable_value(value, f.to_string(), f);
        }
    });
    ui.end_row();
}

fn choice(ui: &mut egui::Ui, label: &str, value: &mut String, options: &[(&str, &str)]) {
    ui.label(label);
    ui.horizontal_wrapped(|ui| {
        for (v, l) in options {
            if ui.selectable_label(value == v, t(l)).clicked() {
                *value = v.to_string();
            }
        }
    });
    ui.end_row();
}

pub fn color_field(ui: &mut egui::Ui, hexv: &mut String) {
    ui.horizontal(|ui| {
        let mut c = hex(hexv);
        if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
            *hexv = crate::theme::to_hex(c).trim_start_matches('#').to_string();
        }
        ui.label(RichText::new(format!("#{}", hexv.trim_start_matches('#'))).font(super::mono(11.0)).color(colors().muted));
    });
}

// ------------------------------------------------------------------ galería

pub struct TemplatesView {
    selected: String,
    /// Pestaña: "pptx", "docx" o "pdf".
    kind: String,
    editor: Option<Editor>,
    logos: Logos,
    confirm_delete: Option<Template>,
}

/// Abre la galería. "Usar esta plantilla" la deja elegida para los próximos exports (y en Exportar si está abierto).
pub fn open() {
    if let Some(h) = host::find("templates") {
        host::activate(h);
        return;
    }
    let st = store::settings();
    let format = export::Format::parse(&st.export_format);
    let kind = templates::kind_of(format).to_string();
    let selected = st.export_templates.get(&kind).cloned().unwrap_or(st.export_template);
    host::open(Box::new(TemplatesView { selected, kind, editor: None, logos: Logos::default(), confirm_delete: None }), Options { width: 1060.0, height: 780.0, min_width: 760.0, min_height: 520.0, ..Default::default() });
}

fn use_template(id: &str) {
    let id = id.to_string();
    let kind = templates::all().into_iter().find(|x| x.id == id).map(|x| x.format).unwrap_or_default();
    store::with(|s| {
        if !kind.is_empty() {
            s.settings.export_templates.insert(kind.clone(), id.clone());
        }
        s.settings.export_template = id.clone();
        s.save_settings();
    });
    if let Some(h) = host::find("export") {
        host::with_view::<super::export::ExportView, _>(h, |v| v.set_template(&id));
    }
}

impl View for TemplatesView {
    fn kind(&self) -> &'static str {
        "templates"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", t("Plantillas de documento"))
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        let all: Vec<Template> = templates::all().into_iter().filter(|x| x.format.is_empty() || x.format == self.kind).collect();
        if !all.iter().any(|x| x.id == self.selected) {
            self.selected = all[0].id.clone();
        }
        let sel = all.iter().find(|x| x.id == self.selected).cloned().unwrap_or_else(|| all[0].clone());
        let custom = templates::custom().len();
        let app_accent = export::app_accent();
        let mut action: Option<&'static str> = None;

        egui::TopBottomPanel::top("tpl-head").frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin { left: 24, right: 24, top: 20, bottom: 10 })).show_separator_line(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                super::mark::live(ui, 48.0, c.accent, true);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(t("Plantillas de documento")).font(bold(20.0)));
                    muted(ui, t("El diseño de los PowerPoint, Word, PDF e imágenes que exportás. Elegí una, duplicala para ajustarla, o creá la tuya con tus colores, tipografías y logo."));
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                for (k, label) in [("pptx", "PowerPoint"), ("docx", "Word"), ("pdf", "PDF e imagen")] {
                    if super::chip(ui, &t(label), self.kind == k).clicked() && self.kind != k {
                        self.kind = k.to_string();
                        self.selected = store::with(|s| s.settings.export_templates.get(k).cloned()).unwrap_or_default();
                    }
                }
            });
        });
        egui::TopBottomPanel::bottom("tpl-foot").frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin { left: 24, right: 24, top: 10, bottom: 16 })).show_separator_line(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(f("{0} de {1} plantillas propias", &[&custom, &templates::MAX_CUSTOM])).color(c.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("Cerrar")).clicked() {
                        action = Some("close");
                    }
                    if primary(ui, &t("Usar esta plantilla")).clicked() {
                        action = Some("use");
                    }
                    ui.add_space(12.0);
                    let can_add = custom < templates::MAX_CUSTOM;
                    if ui.add_enabled(!sel.built_in(), egui::Button::new(t("Eliminar"))).clicked() {
                        action = Some("delete");
                    }
                    if ui.add_enabled(!sel.built_in(), egui::Button::new(t("Editar"))).clicked() {
                        action = Some("edit");
                    }
                    if ui.add_enabled(can_add, egui::Button::new(t("Duplicar"))).clicked() {
                        action = Some("duplicate");
                    }
                    if ui.add_enabled(can_add, egui::Button::new(t("Crear plantilla"))).clicked() {
                        action = Some("create");
                    }
                });
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin::symmetric(24, 6))).show(ctx, |ui| {
            egui::ScrollArea::vertical().id_salt("tpl_grid").auto_shrink([false, false]).show(ui, |ui| {
                let avail = ui.available_width() - 4.0;
                let cols = if avail > 900.0 { 2 } else { 1 };
                let portrait = self.kind != "pptx";
                let card_w = (avail - 14.0 * (cols - 1) as f32) / cols as f32;
                let page_w = ((card_w - 28.0 - 10.0) / 2.05).min(if portrait { 150.0 } else { 230.0 });
                ui.spacing_mut().item_spacing = Vec2::new(14.0, 14.0);
                for row in all.chunks(cols) {
                  ui.horizontal_top(|ui| {
                    for tp in row {
                        let on = tp.id == self.selected;
                        let frame = egui::Frame::default()
                            .fill(if on { c.accent_weak } else { c.raised })
                            .stroke(Stroke::new(if on { 2.0 } else { 1.0 }, if on { c.accent } else { c.faint_border }))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(egui::Margin::same(14));
                        let shown = frame.show(ui, |ui| {
                            ui.set_width(card_w - 28.0);
                            ui.vertical(|ui| {
                                preview(ui, tp, &app_accent, page_w, &mut self.logos);
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    let name = if tp.built_in() { t(&tp.name) } else { tp.name.clone() };
                                    ui.label(RichText::new(name).font(bold(14.0)));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let (tag, col) = if tp.built_in() { (t("INCLUIDA"), c.muted) } else { (t("PROPIA"), c.accent) };
                                        ui.label(RichText::new(tag).font(super::mono(10.0)).color(col));
                                    });
                                });
                                let desc = if tp.built_in() { t(&tp.description) } else { tp.description.clone() };
                                ui.label(RichText::new(desc).size(super::px(11.5)).color(c.muted));
                            });
                        });
                        // Toda la tarjeta responde al clic (uno elige, dos usan la plantilla).
                        let resp = ui.interact(shown.response.rect, egui::Id::new(("tpl-card", &tp.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        if resp.double_clicked() {
                            self.selected = tp.id.clone();
                            action = Some("use");
                        } else if resp.clicked() {
                            self.selected = tp.id.clone();
                        }
                    }
                  });
                }
            });
        });

        // Confirmar antes de eliminar.
        if let Some(d) = self.confirm_delete.clone() {
            let mut close = false;
            egui::Modal::new(egui::Id::new("tpl-del")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new(t("Eliminar plantilla")).font(bold(16.0)));
                ui.label(f("Se elimina «{0}». Los documentos que ya exportaste no cambian.", &[&d.name]));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(t("Eliminar")).color(c.error)).clicked() {
                        templates::delete(&d.id);
                        self.selected = templates::builtins()[0].id.clone();
                        close = true;
                    }
                    if ui.button(t("Cancelar")).clicked() {
                        close = true;
                    }
                });
            });
            if close {
                self.confirm_delete = None;
            }
        }

        if let Some(ed) = self.editor.as_mut() {
            match ed.show(ctx, win.hwnd, &mut self.logos) {
                EditResult::Open => {}
                EditResult::Cancelled => self.editor = None,
                EditResult::Saved(tp) => {
                    self.selected = tp.id;
                    self.editor = None;
                }
            }
        }

        match action {
            Some("close") => win.close = true,
            Some("use") => {
                use_template(&self.selected);
                win.close = true;
            }
            Some("create") => self.editor = Some(Editor::create(&self.kind.clone())),
            Some("duplicate") => self.editor = Some(Editor::duplicate(&sel)),
            Some("edit") if !sel.built_in() => self.editor = Some(Editor::edit(&sel)),
            Some("delete") if !sel.built_in() => self.confirm_delete = Some(sel.clone()),
            _ => {}
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.editor.is_none() && self.confirm_delete.is_none() {
            win.close = true;
        }
    }
}
