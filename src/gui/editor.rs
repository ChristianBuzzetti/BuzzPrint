//! Editor de anotaciones: seleccionar, flecha, línea, rectángulo, elipse, resaltador, lápiz, texto, pasos
//! numerados, pixelado y recorte (V A L R E H P T N B C); colores, grosor, tamaño de letra, deshacer y
//! rehacer, Ctrl+rueda para zoom y una bandeja de capturas. Al guardar por primera vez se conserva la
//! original y la versión anotada queda como copia "_editada" (como la versión .NET).

use std::any::Any;
use std::rc::Rc;

use egui::{Align2, Color32, ColorImage, CornerRadius, Key, Pos2, Rect, RichText, Sense, Shape, Stroke, TextureHandle, TextureOptions, Vec2};
use serde_json::Value;

use super::host::{self, Options, View, WinCtl};
use super::thumbs::Thumbs;
use super::{bold, caption, colors, mono, primary};
use crate::app::{post, Cmd};
use crate::i18n::t;
use crate::i18n::t as t_;
use crate::image::{load_png, Rgba};
use crate::marks::{self, Argb, Mark, Tool, P};
use crate::store::{self, Item, Store};
use crate::theme::alpha;

const TOOLS: &[(Tool, &str, &str, Key)] = &[
    (Tool::Select, "⬚", "Seleccionar y mover", Key::V),
    (Tool::Arrow, "➜", "Flecha", Key::A),
    (Tool::Line, "╱", "Línea", Key::L),
    (Tool::Rect, "▭", "Rectángulo", Key::R),
    (Tool::Ellipse, "◯", "Elipse", Key::E),
    (Tool::Highlight, "▮", "Resaltador", Key::H),
    (Tool::Pen, "✎", "Lápiz", Key::P),
    (Tool::Text, "T", "Texto", Key::T),
    (Tool::Step, "①", "Paso numerado", Key::N),
    (Tool::Blur, "▦", "Pixelar / ocultar datos", Key::B),
    (Tool::Crop, "⛶", "Recortar", Key::C),
];

const SWATCHES: [(u8, u8, u8); 8] = [(230, 30, 40), (255, 140, 0), (255, 214, 0), (30, 170, 60), (20, 110, 230), (150, 60, 200), (0, 0, 0), (255, 255, 255)];
const WIDTHS: [f32; 7] = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0];
const FONTS: [f32; 8] = [12.0, 14.0, 16.0, 18.0, 22.0, 28.0, 36.0, 48.0];
const HANDLE: f32 = 9.0;
/// Al acercar se ven los píxeles nítidos (como en un editor de imágenes); al alejar, suavizado.
const TEX: TextureOptions = TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Linear, ..TextureOptions::LINEAR };

#[derive(Clone)]
struct Snapshot {
    base: Rc<Rgba>,
    marks: Vec<Mark>,
}

struct TextEdit {
    mark: Mark,
    buffer: String,
    focus: bool,
}

enum Ask {
    /// Cambios sin guardar: después se abre `next` (None = cerrar la ventana).
    Unsaved { next: Option<String> },
}

pub struct Editor {
    item: Option<Item>,
    base: Option<Rc<Rgba>>,
    marks: Vec<Mark>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    tool: Tool,
    color: Argb,
    width: f32,
    font_size: f32,
    zoom: f32,
    fit_pending: bool,
    /// Desplazamiento de la imagen respecto del centro del área (en pantalla), para moverse con zoom.
    pan: Vec2,
    /// Herramienta Mano: arrastrar mueve la vista en lugar de dibujar.
    hand: bool,
    /// Lienzo: color (0 blanco, 1 negro, 2 transparente) y tamaño (0 chico, 1 mediano, 2 igual a la captura).
    canvas_color: usize,
    canvas_size: usize,
    selected: Option<usize>,
    current: Option<Mark>,
    drag_start: Option<P>,
    drag_handle: i32,
    moving: bool,
    crop: Option<(P, P)>,
    text: Option<TextEdit>,
    dirty: bool,
    note: String,
    note_dirty: bool,
    title: String,
    status: String,
    committed: Option<TextureHandle>,
    committed_rev: u64,
    rev: u64,
    hidden: Option<usize>, // marca que se dibuja en vivo (no en la textura) mientras se mueve
    thumbs: Option<Thumbs>,
    ask: Option<Ask>,
    close_now: bool,
    store_rev: u64,
    tray: Vec<Item>,
    /// Selector de "Otro color…" abierto.
    picker: Option<super::colorpick::ColorPicker>,
}

pub fn open(id: Option<String>) {
    let id = id.or_else(|| store::with(|s| s.all().first().map(|i| i.id.clone())));
    if let Some(h) = host::find("editor") {
        host::activate(h);
        if let Some(id) = id {
            host::with_view::<Editor, _>(h, |e| e.request_load(id));
        }
        return;
    }
    let mut e = Editor {
        item: None,
        base: None,
        marks: Vec::new(),
        undo: Vec::new(),
        redo: Vec::new(),
        tool: Tool::Arrow,
        color: Argb::from_rgb(230, 30, 40),
        width: 4.0,
        font_size: 22.0,
        zoom: 1.0,
        fit_pending: true,
        pan: Vec2::ZERO,
        hand: false,
        canvas_color: 0,
        canvas_size: 1,
        selected: None,
        current: None,
        drag_start: None,
        drag_handle: -1,
        moving: false,
        crop: None,
        text: None,
        dirty: false,
        note: String::new(),
        note_dirty: false,
        title: t("Sin captura"),
        status: String::new(),
        committed: None,
        committed_rev: 0,
        rev: 1,
        hidden: None,
        thumbs: None,
        ask: None,
        close_now: false,
        store_rev: 0,
        tray: Vec::new(),
        picker: None,
    };
    if let Some(id) = id {
        e.load(&id);
    }
    host::open(Box::new(e), Options { width: 1280.0, height: 820.0, min_width: 800.0, min_height: 520.0, ..Default::default() });
}

fn to_p(v: Pos2, origin: Pos2, zoom: f32) -> P {
    P { x: (v.x - origin.x) / zoom, y: (v.y - origin.y) / zoom }
}

fn to_s(p: P, origin: Pos2, zoom: f32) -> Pos2 {
    Pos2::new(origin.x + p.x * zoom, origin.y + p.y * zoom)
}

impl Editor {
    fn request_load(&mut self, id: String) {
        if self.item.as_ref().is_some_and(|i| i.id == id) {
            return;
        }
        self.commit_text();
        if self.dirty {
            self.ask = Some(Ask::Unsaved { next: Some(id) });
        } else {
            self.load(&id);
        }
    }

    fn load(&mut self, id: &str) {
        self.save_note();
        let Some(it) = store::with(|s| s.find(id)) else { return };
        if it.is_recording() {
            return;
        }
        let (base, marks) = if it.orig_path.exists() {
            match load_png(&it.orig_path) {
                Ok(b) => (b, marks::marks_from_meta(&Store::read_meta(&it))),
                Err(e) => {
                    self.status = format!("{}: {e}", t("No se pudo abrir"));
                    return;
                }
            }
        } else {
            match load_png(&it.path) {
                Ok(b) => (b, Vec::new()),
                Err(e) => {
                    self.status = format!("{}: {e}", t("No se pudo abrir"));
                    return;
                }
            }
        };
        self.title = it.display();
        self.note = it.note.clone();
        self.note_dirty = false;
        self.base = Some(Rc::new(base));
        self.marks = marks;
        self.item = Some(it);
        self.undo.clear();
        self.redo.clear();
        self.selected = None;
        self.current = None;
        self.text = None;
        self.crop = None;
        self.dirty = false;
        self.fit_pending = true;
        self.changed_visual();
        self.status.clear();
    }

    fn changed_visual(&mut self) {
        self.rev += 1;
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.changed_visual();
    }

    fn push_undo(&mut self) {
        if let Some(b) = &self.base {
            self.undo.push(Snapshot { base: b.clone(), marks: self.marks.clone() });
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
            // Limita la cantidad de imágenes base distintas retenidas en el historial (evita crecer sin tope tras recortes).
            const MAX_BASES: usize = 10;
            let mut seen: Vec<*const Rgba> = vec![Rc::as_ptr(b)];
            let mut cut = None;
            for i in (0..self.undo.len()).rev() {
                let ptr = Rc::as_ptr(&self.undo[i].base);
                if !seen.contains(&ptr) {
                    if seen.len() >= MAX_BASES {
                        cut = Some(i + 1);
                        break;
                    }
                    seen.push(ptr);
                }
            }
            if let Some(k) = cut {
                self.undo.drain(..k);
            }
            self.redo.clear();
        }
    }

    fn undo(&mut self) {
        if let (Some(s), Some(b)) = (self.undo.pop(), &self.base) {
            self.redo.push(Snapshot { base: b.clone(), marks: std::mem::take(&mut self.marks) });
            self.base = Some(s.base);
            self.marks = s.marks;
            self.selected = None;
            self.changed();
        }
    }

    fn redo(&mut self) {
        if let (Some(s), Some(b)) = (self.redo.pop(), &self.base) {
            self.undo.push(Snapshot { base: b.clone(), marks: std::mem::take(&mut self.marks) });
            self.base = Some(s.base);
            self.marks = s.marks;
            self.selected = None;
            self.changed();
        }
    }

    fn render_final(&self) -> Option<Rgba> {
        self.base.as_ref().map(|b| marks::render(b, &self.marks))
    }

    fn commit_text(&mut self) {
        if let Some(te) = self.text.take() {
            let text = te.buffer.trim_end().to_string();
            if !text.is_empty() {
                let mut m = te.mark;
                m.text = text;
                self.marks.push(m);
                if self.tool == Tool::Select {
                    self.selected = Some(self.marks.len() - 1);
                }
            }
            self.changed();
        }
    }

    fn begin_text(&mut self, mark: Mark, existing: Option<usize>) {
        self.commit_text();
        self.push_undo();
        if let Some(i) = existing {
            self.marks.remove(i);
            self.selected = None;
            self.changed_visual();
        }
        let buffer = mark.text.clone();
        self.text = Some(TextEdit { mark, buffer, focus: true });
    }

    fn hit(&self, p: P) -> Option<usize> {
        (0..self.marks.len()).rev().find(|&i| self.marks[i].hit(p, 5.0 / self.zoom))
    }

    fn hit_handle(&self, p: P) -> i32 {
        let Some(m) = self.selected.and_then(|i| self.marks.get(i)) else { return -1 };
        if !m.is_two_point() {
            return -1;
        }
        let tol = HANDLE / self.zoom;
        let near = |q: P| (p.x - q.x).abs() <= tol && (p.y - q.y).abs() <= tol;
        if near(m.p0()) {
            0
        } else if near(m.p1()) {
            1
        } else {
            -1
        }
    }

    fn save(&mut self) {
        self.commit_text();
        self.save_note();
        let (Some(item), Some(base)) = (self.item.clone(), self.base.clone()) else { return };
        let st = store::settings();
        let result = (|| -> std::io::Result<String> {
            let final_img = marks::render(&base, &self.marks);
            if st.edit_keep_original && !item.orig_path.exists() {
                // Primera vez: la original queda intacta y la anotada es una captura nueva "_editada".
                let png = crate::image::png_bytes(&final_img)?;
                let copy = store::with(|s| s.add_png(&png, &item.source))?;
                crate::image::save_png(&base, &copy.orig_path)?;
                let mut meta = Store::read_meta(&copy);
                meta.insert("title".into(), Value::String(format!("{}{}", item.display(), if crate::i18n::es() { "_editada" } else { "_edited" })));
                meta.insert("editedFrom".into(), Value::String(item.id.clone()));
                meta.insert("shapes".into(), marks::marks_to_meta(&self.marks));
                if !item.section.is_empty() {
                    meta.insert("section".into(), Value::String(item.section.clone()));
                }
                Store::write_meta(&copy, &meta)?;
                store::with(|s| s.changed());
                let fresh = store::with(|s| s.find(&copy.id)).unwrap_or(copy);
                self.title = fresh.display();
                self.item = Some(fresh);
                return Ok(format!("{}{}", t("Guardada como copia · la original quedó intacta · "), self.title));
            }
            crate::image::save_png(&base, &item.orig_path)?;
            let mut meta = Store::read_meta(&item);
            meta.insert("shapes".into(), marks::marks_to_meta(&self.marks));
            Store::write_meta(&item, &meta)?;
            crate::image::save_png(&final_img, &item.path)?;
            store::with(|s| s.changed());
            Ok(format!("{}{}", t("Guardada · "), item.path.display()))
        })();
        match result {
            Ok(msg) => {
                self.dirty = false;
                self.status = msg;
            }
            Err(e) => self.status = format!("{}: {e}", t("No se pudo guardar")),
        }
        crate::app::trim_memory();
        post(Cmd::LibraryChanged);
    }

    fn save_note(&mut self) {
        if !self.note_dirty {
            return;
        }
        self.note_dirty = false;
        if let Some(it) = &self.item {
            let note = self.note.trim().to_string();
            let _ = store::with(|s| s.update_meta(it, "note", Value::String(note)));
            self.status = t("Resumen guardado");
        }
    }

    fn copy(&mut self) {
        self.commit_text();
        if let Some(img) = self.render_final() {
            crate::output::copy_image(crate::app::main_hwnd(), &img);
            self.status = t("Copiada al portapapeles.");
        }
    }

    fn export_file(&mut self, hwnd: windows_sys::Win32::Foundation::HWND) {
        self.commit_text();
        let Some(img) = self.render_final() else { return };
        let name = self.item.as_ref().map(|i| i.display()).unwrap_or_else(|| t("captura"));
        let hwnd = hwnd as isize;
        post(Cmd::Run(Box::new(move || {
            if let Some((path, idx)) = crate::dialogs::save_file(hwnd as _, &t("Exportar imagen"), &name, &[("PNG", "*.png"), ("JPEG", "*.jpg"), ("BMP", "*.bmp")], None) {
                let r = match idx {
                    2 => marks::save_with_gdiplus(&img, &path, "jpg"),
                    3 => marks::save_with_gdiplus(&img, &path, "bmp"),
                    _ => crate::image::save_png(&img, &path),
                };
                let msg = match r {
                    Ok(()) => format!("{}{}", t("Guardada · "), path.display()),
                    Err(e) => format!("{}: {e}", t("No se pudo guardar")),
                };
                host::with_view::<Editor, _>(hwnd as _, |e| e.status = msg);
            }
        })));
    }

    fn texture(&mut self, ctx: &egui::Context) -> Option<(egui::TextureId, Vec2)> {
        let base = self.base.clone()?;
        if self.committed.is_none() || self.committed_rev != self.rev {
            self.committed_rev = self.rev;
            let marks: Vec<Mark> = self.marks.iter().enumerate().filter(|(i, _)| Some(*i) != self.hidden).map(|(_, m)| m.clone()).collect();
            let img = marks::render(&base, &marks);
            let ci = ColorImage::from_rgba_unmultiplied([img.w as usize, img.h as usize], &img.px);
            match &mut self.committed {
                Some(t) => t.set(ci, TEX),
                None => self.committed = Some(ctx.load_texture("editor", ci, TEX)),
            }
        }
        self.committed.as_ref().map(|t| (t.id(), Vec2::new(base.w as f32, base.h as f32)))
    }

    fn keys(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        // Con el selector de color abierto, las teclas son suyas (Esc, Enter, el campo hex).
        if self.ask.is_some() || self.picker.is_some() {
            return;
        }
        if self.text.is_some() {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.text = None;
                self.changed_visual();
            }
            return;
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        let (m, events) = ctx.input(|i| (i.modifiers, i.events.clone()));
        for e in events {
            match e {
                egui::Event::Copy => self.copy(),
                egui::Event::Key { key, pressed: true, .. } if m.ctrl => match key {
                    Key::S => self.save(),
                    Key::Z => self.undo(),
                    Key::Y => self.redo(),
                    Key::Num0 => self.fit_pending = true,
                    Key::Num1 => {
                        self.zoom = 1.0;
                        self.pan = Vec2::ZERO;
                    }
                    Key::Plus => self.zoom = (self.zoom * 1.25).min(16.0),
                    Key::Minus => self.zoom = (self.zoom / 1.25).max(0.05),
                    _ => {}
                },
                egui::Event::Key { key, pressed: true, .. } => match key {
                    Key::Delete => {
                        if let Some(i) = self.selected.take() {
                            self.push_undo();
                            self.marks.remove(i);
                            self.changed();
                        }
                    }
                    Key::Escape => {
                        self.selected = None;
                        self.current = None;
                        self.crop = None;
                    }
                    k => {
                        if let Some((tool, ..)) = TOOLS.iter().find(|d| d.3 == k) {
                            self.tool = *tool;
                            self.selected = None;
                        }
                    }
                },
                _ => {}
            }
        }
        let _ = win;
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, hwnd: windows_sys::Win32::Foundation::HWND) {
        let c = colors();
        ui.horizontal_wrapped(|ui| {
            // Barra compacta: entra en un renglón aun con Lienzo y Mover.
            ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
            ui.spacing_mut().item_spacing.x = 5.0;
            for (tool, _glyph, name, key) in TOOLS {
                let on = self.tool == *tool;
                // Íconos dibujados (no caracteres de fuentes distintas): todos del mismo tamaño y centrados.
                let b = egui::Button::new("").fill(if on { c.accent } else { c.raised }).min_size(Vec2::new(34.0, 30.0));
                let resp = ui.add(b);
                paint_tool_icon(ui.painter(), resp.rect.center(), *tool, if on { c.on_accent } else { c.ink });
                if resp.on_hover_text(format!("{} · {:?}", t(name), key)).clicked() {
                    self.commit_text();
                    self.tool = *tool;
                    self.selected = None;
                }
            }
            ui.separator();
            for (r, g, b) in SWATCHES {
                let col = Argb::from_rgb(r, g, b);
                let on = self.color == col;
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                ui.painter().rect_filled(rect.shrink(2.0), CornerRadius::same(4), col.to_egui());
                ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(if on { 2.0 } else { 1.0 }, if on { c.accent } else { alpha(c.ink, 0.2) }), egui::StrokeKind::Inside);
                if resp.clicked() {
                    self.set_color(col);
                }
            }
            let more = tool_btn(ui, '\u{E790}', true).on_hover_text(t("Otro color…"));
            if more.clicked() {
                self.picker = Some(super::colorpick::ColorPicker::new(self.color.to_egui(), more.rect));
            }
            ui.separator();
            egui::ComboBox::from_id_salt("width").width(70.0).selected_text(egui::RichText::new(format!("{} px", self.width)).size(super::combo_size())).show_ui(ui, |ui| {
                super::menu_style(ui);
                for w in WIDTHS {
                    if ui.selectable_label(self.width == w, format!("{w} px")).clicked() {
                        self.width = w;
                        self.apply_to_selected(|m| m.width = w);
                    }
                }
            });
            egui::ComboBox::from_id_salt("font").width(70.0).selected_text(egui::RichText::new(format!("Aa {}", self.font_size)).size(super::combo_size())).show_ui(ui, |ui| {
                super::menu_style(ui);
                for f in FONTS {
                    if ui.selectable_label(self.font_size == f, format!("{f} px")).clicked() {
                        self.font_size = f;
                        self.apply_to_selected(|m| {
                            if matches!(m.kind, Tool::Text | Tool::Step) {
                                m.font_size = f;
                            }
                        });
                    }
                }
            });
            ui.separator();
            if tool_btn(ui, '\u{E7A7}', !self.undo.is_empty()).on_hover_text(t("Deshacer · Ctrl+Z")).clicked() {
                self.undo();
            }
            if tool_btn(ui, '\u{E7A6}', !self.redo.is_empty()).on_hover_text(t("Rehacer · Ctrl+Y")).clicked() {
                self.redo();
            }
            ui.separator();
            self.canvas_menu(ui);
            ui.separator();
            // Mano: arrastrar para moverse por la imagen (también: rueda, Espacio + arrastrar o botón del medio).
            let b = egui::Button::new("").fill(if self.hand { c.accent } else { c.raised }).min_size(Vec2::new(34.0, 30.0));
            let hr = ui.add(b);
            paint_hand_icon(ui.painter(), hr.rect.center(), if self.hand { c.on_accent } else { c.ink });
            if hr.on_hover_text(t_("Mover la vista · también rueda, Shift+rueda, Espacio + arrastrar o botón del medio")).clicked() {
                self.hand = !self.hand;
            }
            if tool_btn(ui, '\u{E9A6}', true).on_hover_text(t("Ajustar a la ventana · Ctrl+0")).clicked() {
                self.fit_pending = true;
            }
            if ui.add(egui::Button::new(RichText::new("1:1").font(mono(11.5))).min_size(Vec2::new(34.0, 30.0))).on_hover_text("Ctrl+1").clicked() {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            ui.label(RichText::new(format!("{:.0} %", self.zoom * 100.0)).font(mono(11.0)).color(c.muted));
        });
        let _ = hwnd;
    }

    /// Guardar, copiar y exportar: arriba del panel derecho, siempre visibles.
    fn actions(&mut self, ui: &mut egui::Ui, hwnd: windows_sys::Win32::Foundation::HWND) {
        let w = ui.available_width();
        if ui.add_sized([w, 32.0], egui::Button::new(RichText::new(t("Guardar")).font(bold(14.0)).color(colors().on_accent)).fill(colors().accent)).on_hover_text("Ctrl+S").clicked() {
            self.save();
        }
        ui.horizontal(|ui| {
            let half = (w - 8.0) / 2.0;
            if ui.add_sized([half, 28.0], egui::Button::new(t("Copiar"))).on_hover_text("Ctrl+C").clicked() {
                self.copy();
            }
            if ui.add_sized([half, 28.0], egui::Button::new(t("Exportar…"))).clicked() {
                self.export_file(hwnd);
            }
        });
        // ✦ IA: las mismas acciones del menú IA de la biblioteca, para la captura abierta.
        if store::settings().ai_enabled && self.item.as_ref().is_some_and(|i| !i.is_recording()) {
            ui.add_space(6.0);
            let c = colors();
            let mut job = super::widgets::icon_label(super::widgets::glyph::AI, &t("IA"), 13.0, c.ink);
            job.append("   ▾", 0.0, egui::TextFormat { font_id: super::prop(11.0), color: c.muted, ..Default::default() });
            let r = ui.add_sized([w, 28.0], egui::Button::new(job)).on_hover_text(t("Título y resumen, proteger datos, preguntar o reporte de incidente con IA"));
            let mut chosen: Option<&'static str> = None;
            super::widgets::dropdown(ui, &r, |ui| {
                for (label, a) in [
                    ("Título y resumen", "caption"),
                    ("Proteger datos sensibles", "redact"),
                    ("Preguntar sobre la captura…", "ask"),
                    ("Reporte de incidente", "ticket"),
                    ("Documento paso a paso…", "document"),
                ] {
                    if super::menu_item(ui, &t(label), "", true).clicked() {
                        chosen = Some(a);
                    }
                }
                ui.separator();
                if super::menu_item(ui, &t("Configurar IA…"), "", true).clicked() {
                    chosen = Some("settings");
                }
            });
            if let Some(a) = chosen {
                // La IA trabaja sobre el archivo: si hay anotaciones sin guardar, se guardan antes.
                if self.dirty && a != "settings" {
                    self.save();
                }
                if let Some(it) = self.item.clone() {
                    post(Cmd::Run(Box::new(move || crate::gui::ai::run(a, vec![it]))));
                }
            }
        }
        ui.add_space(14.0);
    }

    /// Agrega lienzo alrededor de la captura (para dibujar al lado): `side` 0 derecha, 1 abajo, 2 izquierda,
    /// 3 arriba, 4 alrededor. Las marcas se corren con la imagen y se puede deshacer.
    fn extend_canvas(&mut self, side: usize) {
        self.commit_text();
        let Some(base) = self.base.clone() else { return };
        let frac = [0.25, 0.5, 1.0][self.canvas_size.min(2)];
        let (w, h) = (base.w as f32, base.h as f32);
        let (l, t, r, b) = match side {
            0 => (0.0, 0.0, w * frac, 0.0),
            1 => (0.0, 0.0, 0.0, h * frac),
            2 => (w * frac, 0.0, 0.0, 0.0),
            3 => (0.0, h * frac, 0.0, 0.0),
            _ => {
                let m = w.min(h) * frac * 0.5;
                (m, m, m, m)
            }
        };
        let (l, t, r, b) = (l.round() as u32, t.round() as u32, r.round() as u32, b.round() as u32);
        let (nw, nh) = (base.w + l + r, base.h + t + b);
        if nw > 16000 || nh > 16000 {
            self.status = t_("El lienzo no puede pasar de 16000 px.");
            return;
        }
        let fill: [u8; 4] = match self.canvas_color {
            1 => [0, 0, 0, 255],
            2 => [0, 0, 0, 0],
            _ => [255, 255, 255, 255],
        };
        let mut out = Rgba::new(nw, nh);
        for px in out.px.chunks_exact_mut(4) {
            px.copy_from_slice(&fill);
        }
        let (sw, dw) = (base.w as usize * 4, nw as usize * 4);
        for row in 0..base.h as usize {
            let d = (row + t as usize) * dw + l as usize * 4;
            out.px[d..d + sw].copy_from_slice(&base.px[row * sw..(row + 1) * sw]);
        }
        self.push_undo();
        self.base = Some(Rc::new(out));
        for m in &mut self.marks {
            m.offset(l as f32, t as f32);
        }
        self.selected = None;
        self.changed();
        self.fit_pending = true;
        self.status = format!("{} · {nw}×{nh}", t_("Lienzo agregado"));
    }

    /// Botón "Lienzo ▾": color, tamaño y de qué lado agregar.
    fn canvas_menu(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        let mut job = super::widgets::icon_label('\u{E9F5}', &t_("Lienzo"), 13.5, c.ink);
        job.append("  ▾", 0.0, egui::TextFormat { font_id: super::prop(11.0), color: c.muted, valign: egui::Align::Center, ..Default::default() });
        let r = ui.add(egui::Button::new(job).min_size(Vec2::new(0.0, 30.0))).on_hover_text(t_("Agregar lienzo blanco, negro o transparente al lado de la captura para dibujar"));
        let mut side = None;
        super::widgets::dropdown(ui, &r, |ui| {
            ui.set_min_width(280.0);
            super::menu_heading(ui, &t_("Color"));
            for (i, l) in [t_("Blanco"), t_("Negro"), t_("Transparente")].iter().enumerate() {
                if super::menu_check(ui, l, self.canvas_color == i).clicked() {
                    self.canvas_color = i;
                }
            }
            ui.separator();
            super::menu_heading(ui, &t_("Tamaño"));
            for (i, l) in [t_("Chico (¼ de la captura)"), t_("Mediano (½ de la captura)"), t_("Igual a la captura")].iter().enumerate() {
                if super::menu_check(ui, l, self.canvas_size == i).clicked() {
                    self.canvas_size = i;
                }
            }
            ui.separator();
            super::menu_heading(ui, &t_("Agregar"));
            for (i, g, l) in [
                (0, '\u{E72A}', t_("A la derecha")),
                (1, '\u{E74B}', t_("Abajo")),
                (2, '\u{E72B}', t_("A la izquierda")),
                (3, '\u{E74A}', t_("Arriba")),
                (4, '\u{E740}', t_("Alrededor (margen)")),
            ] {
                if super::menu_item_icon(ui, g, &l, "", "", self.base.is_some()).clicked() {
                    side = Some(i);
                }
            }
        });
        if let Some(s) = side {
            self.extend_canvas(s);
        }
    }

    fn set_color(&mut self, c: Argb) {
        self.color = c;
        self.apply_to_selected(|m| m.color = c);
    }

    fn apply_to_selected(&mut self, f: impl FnOnce(&mut Mark)) {
        if let Some(i) = self.selected {
            self.push_undo();
            if let Some(m) = self.marks.get_mut(i) {
                f(m);
            }
            self.changed();
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let c = colors();
        let Some((tex, size)) = self.texture(ctx) else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(t("Elegí una captura de la bandeja")).color(c.muted));
            });
            return;
        };
        let area = ui.max_rect();
        let avail = area.size();
        if self.fit_pending {
            self.fit_pending = false;
            self.zoom = ((avail.x - 40.0) / size.x).min((avail.y - 40.0) / size.y).clamp(0.05, 1.0);
            self.pan = Vec2::ZERO;
        }
        let resp = ui.allocate_rect(area, Sense::click_and_drag());
        let hover = ui.rect_contains_pointer(area);
        let (zd, wheel, space, pointer) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.key_down(Key::Space), i.pointer.hover_pos()));
        let origin_for = |zoom: f32, pan: Vec2| area.center() - size * zoom / 2.0 + pan;
        // Ctrl+rueda: zoom hacia el puntero (el punto bajo el mouse queda quieto).
        if zd != 1.0 && hover {
            let z2 = (self.zoom * zd).clamp(0.05, 16.0);
            if let Some(ptr) = pointer {
                let o = origin_for(self.zoom, self.pan);
                let img = (ptr - o) / self.zoom;
                let want = ptr - img * z2;
                self.pan = want - (area.center() - size * z2 / 2.0);
            }
            self.zoom = z2;
        }
        // Rueda: mover arriba/abajo; Shift+rueda: de costado (egui ya lo pasa a x).
        if hover && wheel != Vec2::ZERO && zd == 1.0 && self.text.is_none() {
            self.pan += wheel;
        }
        // Mano, Espacio + arrastrar o botón del medio: arrastrar mueve la vista.
        let panning = self.hand || space || resp.dragged_by(egui::PointerButton::Middle);
        if panning && (resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Middle)) {
            self.pan += resp.drag_delta();
        }
        if hover && (self.hand || space) {
            ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        // La imagen no se pierde de vista: si entra entera queda centrada; si no, se puede llevar hasta 60 px del borde.
        let disp = size * self.zoom;
        let lim = |d: f32, a: f32| ((d - a) / 2.0 + 60.0).max(0.0);
        let (lx, ly) = (lim(disp.x, avail.x), lim(disp.y, avail.y));
        self.pan.x = if disp.x <= avail.x - 40.0 { 0.0 } else { self.pan.x.clamp(-lx, lx) };
        self.pan.y = if disp.y <= avail.y - 40.0 { 0.0 } else { self.pan.y.clamp(-ly, ly) };
        let origin = origin_for(self.zoom, self.pan);
        let img_rect = Rect::from_min_size(origin, disp);
        let p = ui.painter_at(area);
        // Damero debajo: se nota dónde el lienzo es transparente.
        let vis = img_rect.intersect(area);
        if vis.is_positive() {
            p.rect_filled(vis, 0.0, Color32::from_gray(200));
            let cell = 10.0;
            let (x0, y0) = (((vis.min.x - img_rect.min.x) / cell).floor(), ((vis.min.y - img_rect.min.y) / cell).floor());
            let mut yy = y0;
            while img_rect.min.y + yy * cell < vis.max.y {
                let mut xx = x0;
                while img_rect.min.x + xx * cell < vis.max.x {
                    if (xx as i64 + yy as i64) % 2 == 0 {
                        let r = Rect::from_min_size(img_rect.min + Vec2::new(xx * cell, yy * cell), Vec2::splat(cell)).intersect(vis);
                        p.rect_filled(r, 0.0, Color32::from_gray(160));
                    }
                    xx += 1.0;
                }
                yy += 1.0;
            }
        }
        p.rect_filled(img_rect.expand(1.0), 0.0, Color32::TRANSPARENT);
        p.image(tex, img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        p.rect_stroke(img_rect.expand(1.0), 0.0, Stroke::new(1.0, Color32::from_black_alpha(90)), egui::StrokeKind::Outside);

        // Marca que se mueve en vivo y la que se está dibujando.
        if let Some(i) = self.hidden {
            if let Some(m) = self.marks.get(i) {
                draw_live(&p, m, origin, self.zoom);
            }
        }
        if let Some(m) = &self.current {
            draw_live(&p, m, origin, self.zoom);
        }
        self.selection_overlay(&p, origin);
        if let Some((a, b)) = self.crop {
            let r = Rect::from_two_pos(to_s(a, origin, self.zoom), to_s(b, origin, self.zoom));
            for side in [
                Rect::from_min_max(img_rect.min, Pos2::new(img_rect.max.x, r.min.y)),
                Rect::from_min_max(Pos2::new(img_rect.min.x, r.max.y), img_rect.max),
                Rect::from_min_max(Pos2::new(img_rect.min.x, r.min.y), Pos2::new(r.min.x, r.max.y)),
                Rect::from_min_max(Pos2::new(r.max.x, r.min.y), Pos2::new(img_rect.max.x, r.max.y)),
            ] {
                p.rect_filled(side, 0.0, Color32::from_black_alpha(150));
            }
            p.rect_stroke(r, 0.0, Stroke::new(1.5, c.accent), egui::StrokeKind::Middle);
            let label = format!("{} × {}", (r.width() / self.zoom) as i32, (r.height() / self.zoom) as i32);
            let lr = Rect::from_min_size(r.min - Vec2::new(0.0, 22.0), Vec2::new(90.0, 20.0));
            p.rect_filled(lr, 0.0, c.header);
            p.text(lr.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, label, mono(11.0), c.accent);
        }

        // Barras de posición (solo si la imagen no entra): muestran qué parte se está viendo.
        let bar = |p: &egui::Painter, horiz: bool| {
            let (d, a, off) = if horiz { (disp.x, avail.x, area.min.x - origin.x) } else { (disp.y, avail.y, area.min.y - origin.y) };
            if d <= a {
                return;
            }
            let frac = (a / d).clamp(0.05, 1.0);
            let start = (off / d).clamp(0.0, 1.0 - frac);
            let track = if horiz {
                Rect::from_min_max(Pos2::new(area.min.x + 8.0, area.max.y - 9.0), Pos2::new(area.max.x - 16.0, area.max.y - 4.0))
            } else {
                Rect::from_min_max(Pos2::new(area.max.x - 9.0, area.min.y + 8.0), Pos2::new(area.max.x - 4.0, area.max.y - 16.0))
            };
            p.rect_filled(track, CornerRadius::same(3), Color32::from_black_alpha(70));
            let thumb = if horiz {
                Rect::from_min_max(Pos2::new(track.min.x + track.width() * start, track.min.y), Pos2::new(track.min.x + track.width() * (start + frac), track.max.y))
            } else {
                Rect::from_min_max(Pos2::new(track.min.x, track.min.y + track.height() * start), Pos2::new(track.max.x, track.min.y + track.height() * (start + frac)))
            };
            p.rect_filled(thumb, CornerRadius::same(3), alpha(c.accent, 0.85));
        };
        bar(&p, true);
        bar(&p, false);
        if (disp.x > avail.x || disp.y > avail.y) && !self.hand {
            let hint = t_("Rueda o Espacio + arrastrar para moverte");
            let g = p.layout_no_wrap(hint, mono(10.5), c.muted);
            let r = Rect::from_min_size(Pos2::new(area.min.x + 10.0, area.max.y - 34.0), g.size() + Vec2::new(14.0, 8.0));
            p.rect_filled(r, CornerRadius::same(5), alpha(c.header, 0.88));
            p.galley(r.min + Vec2::new(7.0, 4.0), g, c.muted);
        }

        if !(panning || self.hand) {
            self.mouse(ui, &resp, origin);
        }
        self.text_box(ui, ctx, origin);
    }

    fn selection_overlay(&self, p: &egui::Painter, origin: Pos2) {
        let c = colors();
        let Some(m) = self.selected.and_then(|i| self.marks.get(i)) else { return };
        let b = m.bounds();
        let r = Rect::from_min_max(to_s(P { x: b.x, y: b.y }, origin, self.zoom), to_s(P { x: b.right(), y: b.bottom() }, origin, self.zoom)).expand(4.0);
        dashed_rect(p, r, Stroke::new(1.0, c.accent));
        if m.is_two_point() {
            for q in [m.p0(), m.p1()] {
                let s = to_s(q, origin, self.zoom);
                let h = Rect::from_center_size(s, Vec2::splat(HANDLE));
                p.rect_filled(h, 0.0, Color32::WHITE);
                p.rect_stroke(h, 0.0, Stroke::new(1.0, c.accent), egui::StrokeKind::Inside);
            }
        }
    }

    fn mouse(&mut self, ui: &egui::Ui, resp: &egui::Response, origin: Pos2) {
        let Some(base) = self.base.clone() else { return };
        let pointer = resp.interact_pointer_pos().or_else(|| ui.ctx().pointer_hover_pos());
        let Some(pos) = pointer else { return };
        let p = to_p(pos, origin, self.zoom);
        let shift = ui.input(|i| i.modifiers.shift);

        if self.tool == Tool::Select && !resp.dragged() {
            let cursor = if self.hit_handle(p) >= 0 { egui::CursorIcon::Move } else if self.hit(p).is_some() { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default };
            if resp.hovered() {
                ui.ctx().set_cursor_icon(cursor);
            }
        } else if resp.hovered() && self.tool != Tool::Select {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }

        if resp.double_clicked() && self.tool == Tool::Select {
            if let Some(i) = self.hit(p).filter(|&i| self.marks[i].kind == Tool::Text) {
                let m = self.marks[i].clone();
                self.begin_text(m, Some(i));
                return;
            }
        }

        let pressed = resp.drag_started() || (resp.clicked() && self.drag_start.is_none() && matches!(self.tool, Tool::Text | Tool::Step | Tool::Select));
        if pressed && self.drag_start.is_none() {
            if self.text.is_some() {
                self.commit_text();
                return;
            }
            self.drag_start = Some(p);
            match self.tool {
                Tool::Select => {
                    let h = self.hit_handle(p);
                    if h >= 0 {
                        self.drag_handle = h;
                        self.push_undo();
                    } else {
                        self.drag_handle = -1;
                        self.selected = self.hit(p);
                        if self.selected.is_some() {
                            self.push_undo();
                        } else {
                            self.drag_start = None;
                        }
                    }
                    if self.selected.is_some() && resp.drag_started() {
                        self.hidden = self.selected;
                        self.moving = true;
                        self.changed_visual();
                    }
                }
                Tool::Text => {
                    self.drag_start = None;
                    if let Some(i) = self.hit(p).filter(|&i| self.marks[i].kind == Tool::Text) {
                        let m = self.marks[i].clone();
                        self.begin_text(m, Some(i));
                    } else {
                        let mut m = Mark::new(Tool::Text, self.color, self.width, p);
                        m.pts.truncate(1);
                        m.font_size = self.font_size;
                        self.begin_text(m, None);
                    }
                    return;
                }
                Tool::Step => {
                    self.drag_start = None;
                    self.push_undo();
                    let n = self.marks.iter().filter(|m| m.kind == Tool::Step).map(|m| m.number).max().unwrap_or(0) + 1;
                    let mut m = Mark::new(Tool::Step, self.color, self.width, p);
                    m.pts.truncate(1);
                    m.font_size = self.font_size;
                    m.number = n;
                    self.marks.push(m);
                    self.changed();
                    return;
                }
                Tool::Crop => self.crop = Some((p, p)),
                tool => {
                    let mut m = Mark::new(tool, self.color, self.width, p);
                    if tool == Tool::Pen {
                        m.pts.truncate(1);
                    }
                    self.current = Some(m);
                }
            }
        }

        if resp.dragged() {
            if let Some(start) = self.drag_start {
                match self.tool {
                    Tool::Select => {
                        if let Some(i) = self.selected {
                            let m = &mut self.marks[i];
                            match self.drag_handle {
                                0 => m.pts[0] = p,
                                1 => m.set_p1(p),
                                _ => {
                                    m.offset(p.x - start.x, p.y - start.y);
                                    self.drag_start = Some(p);
                                }
                            }
                            self.dirty = true;
                        }
                    }
                    Tool::Crop => self.crop = Some((start, p)),
                    _ => {
                        if let Some(m) = &mut self.current {
                            if m.kind == Tool::Pen {
                                m.pts.push(p);
                            } else {
                                m.set_p1(if shift { constrain(m, p) } else { p });
                            }
                        }
                    }
                }
            }
        }

        if resp.drag_stopped() || (resp.clicked() && self.drag_start.is_some()) {
            self.drag_start = None;
            match self.tool {
                Tool::Select => {
                    if self.moving {
                        self.moving = false;
                        self.hidden = None;
                        self.changed();
                    }
                }
                Tool::Crop => {
                    if let Some((a, b)) = self.crop.take() {
                        let (x0, y0) = (a.x.min(b.x).round().max(0.0) as i32, a.y.min(b.y).round().max(0.0) as i32);
                        let (x1, y1) = ((a.x.max(b.x).round() as i32).min(base.w as i32), (a.y.max(b.y).round() as i32).min(base.h as i32));
                        if x1 - x0 > 4 && y1 - y0 > 4 {
                            self.push_undo();
                            self.base = Some(Rc::new(base.crop(x0, y0, x1 - x0, y1 - y0)));
                            for m in &mut self.marks {
                                m.offset(-x0 as f32, -y0 as f32);
                            }
                            self.changed();
                            self.fit_pending = true;
                            self.tool = Tool::Select;
                        }
                    }
                }
                _ => {
                    if let Some(m) = self.current.take() {
                        let b = m.bounds();
                        let tiny = if m.kind == Tool::Pen { m.pts.len() < 2 } else { b.w < 3.0 && b.h < 3.0 };
                        if !tiny {
                            self.push_undo();
                            self.marks.push(m);
                            self.changed();
                        }
                    }
                }
            }
        }
    }

    fn text_box(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, origin: Pos2) {
        let zoom = self.zoom;
        let Some(te) = self.text.as_mut() else { return };
        let pos = to_s(te.mark.p0(), origin, zoom);
        let col = te.mark.color.to_egui();
        let size = (te.mark.font_size * zoom).max(8.0);
        let mut commit = false;
        egui::Area::new(egui::Id::new("editor-text")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::default()
                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 235))
                .stroke(Stroke::new(2.0, col))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut te.buffer)
                            .font(egui::FontId::new(size, egui::FontFamily::Name("bold".into())))
                            .text_color(col)
                            .frame(false)
                            .desired_rows(1)
                            .desired_width(220.0f32.max(size * 8.0)),
                    );
                    if te.focus {
                        r.request_focus();
                        te.focus = false;
                    }
                    let enter = ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
                    if enter {
                        // El TextEdit ya agregó el salto de línea: se quita y se confirma.
                        while te.buffer.ends_with('\n') {
                            te.buffer.pop();
                        }
                        commit = true;
                    }
                    if r.lost_focus() && !enter {
                        commit = true;
                    }
                });
        });
        let _ = ui;
        if commit {
            self.commit_text();
        }
    }

    fn side(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        caption(ui, &t("Captura"));
        ui.label(RichText::new(&self.title).font(bold(15.0)).color(c.ink));
        if let (Some(it), Some(b)) = (&self.item, &self.base) {
            ui.label(RichText::new(format!("{}\n{}×{} · {}.png", it.created.long(), b.w, b.h, it.id)).font(mono(10.5)).color(c.muted));
        }
        ui.add_space(12.0);
        caption(ui, &t("Resumen"));
        let r = ui.add(super::area(&mut self.note).hint_text(t("Nota para la exportación (Word, PowerPoint, PDF)")).desired_rows(8).desired_width(f32::INFINITY));
        if r.changed() {
            self.note_dirty = true;
        }
        if r.lost_focus() {
            self.save_note();
        }
        ui.add_space(12.0);
        caption(ui, &t("Atajos"));
        for line in ["V A L R E H P T N B C", "Ctrl+Z / Ctrl+Y", "Ctrl+S · Ctrl+C", "Ctrl+rueda: zoom", "Rueda / Espacio: mover", "Shift: ángulo recto", "Supr: borrar marca"] {
            ui.label(RichText::new(line).font(mono(10.5)).color(c.muted));
        }
    }

    fn tray(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, thumbs: &mut Thumbs) {
        let c = colors();
        let rev = store::with(|s| s.revision);
        if rev != self.store_rev {
            self.store_rev = rev;
            self.tray = store::with(|s| s.all().iter().take(40).cloned().collect());
        }
        let mut pick = None;
        egui::ScrollArea::horizontal().id_salt("ed_strip").show(ui, |ui| {
            ui.horizontal(|ui| {
                for it in &self.tray {
                    let on = self.item.as_ref().is_some_and(|x| x.id == it.id);
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(120.0, 72.0), Sense::click());
                    if !ui.is_rect_visible(r) {
                        continue;
                    }
                    let p = ui.painter();
                    p.rect_filled(r, CornerRadius::same(4), c.canvas);
                    if let Some((tex, size)) = thumbs.get(ctx, &it.path, 240) {
                        let inner = r.shrink(3.0);
                        let s = (inner.width() / size.x).min(inner.height() / size.y);
                        p.image(tex, Rect::from_center_size(inner.center(), size * s), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    p.rect_stroke(r, CornerRadius::same(4), Stroke::new(if on { 2.0 } else { 1.0 }, if on { c.accent } else { alpha(c.ink, 0.1) }), egui::StrokeKind::Inside);
                    if resp.on_hover_text(it.display()).clicked() {
                        pick = Some(it.id.clone());
                    }
                }
            });
        });
        if let Some(id) = pick {
            self.request_load(id);
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let Some(Ask::Unsaved { next }) = &self.ask else { return };
        let next = next.clone();
        let mut choice = 0; // 1 guardar, 2 descartar, 3 cancelar
        egui::Modal::new(egui::Id::new("editor-unsaved")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new(t("¿Guardar los cambios de la captura actual?")).font(bold(15.0)));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if primary(ui, &t("Guardar")).clicked() {
                    choice = 1;
                }
                if ui.button(t("No guardar")).clicked() {
                    choice = 2;
                }
                if ui.button(t("Cancelar")).clicked() {
                    choice = 3;
                }
            });
        });
        if choice == 0 {
            return;
        }
        self.ask = None;
        if choice == 3 {
            return;
        }
        if choice == 1 {
            self.save();
        }
        self.dirty = false;
        match next {
            Some(id) => self.load(&id),
            None => {
                self.close_now = true;
                win.close = true;
            }
        }
    }
}

fn constrain(m: &Mark, p: P) -> P {
    let (dx, dy) = (p.x - m.p0().x, p.y - m.p0().y);
    if matches!(m.kind, Tool::Arrow | Tool::Line) {
        let step = std::f32::consts::FRAC_PI_4;
        let ang = (dy.atan2(dx) / step).round() * step;
        let len = (dx * dx + dy * dy).sqrt();
        return P { x: m.p0().x + ang.cos() * len, y: m.p0().y + ang.sin() * len };
    }
    let s = dx.abs().max(dy.abs());
    P { x: m.p0().x + dx.signum() * s, y: m.p0().y + dy.signum() * s }
}

fn dashed_rect(p: &egui::Painter, r: Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(Shape::dashed_line(&pts, stroke, 4.0, 3.0));
}

/// Dibujo aproximado en pantalla de una marca mientras se crea o se mueve (la versión final la pinta GDI+).
fn draw_live(p: &egui::Painter, m: &Mark, origin: Pos2, zoom: f32) {
    let s = |q: P| to_s(q, origin, zoom);
    let col = m.color.to_egui();
    let stroke = Stroke::new((m.width * zoom).max(1.0), col);
    match m.kind {
        Tool::Arrow => {
            let (a, b) = (s(m.p0()), s(m.p1()));
            let dir = (b - a).normalized();
            let cap = 3f32.max(12.0 / m.width.sqrt()) * m.width * zoom;
            let back = b - dir * cap;
            let n = Vec2::new(-dir.y, dir.x) * cap * 0.75 * 0.5 * 2.0 / 2.0;
            p.line_segment([a, back], stroke);
            p.add(Shape::convex_polygon(vec![b, back + n, back - n], col, Stroke::NONE));
        }
        Tool::Line => {
            p.line_segment([s(m.p0()), s(m.p1())], stroke);
        }
        Tool::Rect => {
            p.rect_stroke(Rect::from_two_pos(s(m.p0()), s(m.p1())), 0.0, stroke, egui::StrokeKind::Middle);
        }
        Tool::Ellipse => {
            let r = Rect::from_two_pos(s(m.p0()), s(m.p1()));
            p.add(Shape::ellipse_stroke(r.center(), r.size() / 2.0, stroke));
        }
        Tool::Highlight => {
            p.rect_filled(Rect::from_two_pos(s(m.p0()), s(m.p1())), 0.0, Color32::from_rgba_unmultiplied(m.color.r(), m.color.g(), m.color.b(), 105));
        }
        Tool::Pen => {
            p.add(Shape::line(m.pts.iter().map(|q| s(*q)).collect(), stroke));
        }
        Tool::Blur => {
            dashed_rect(p, Rect::from_two_pos(s(m.p0()), s(m.p1())), Stroke::new(1.5, Color32::WHITE));
        }
        Tool::Step => {
            let r = m.step_radius() * zoom;
            p.circle(s(m.p0()), r, col, Stroke::new(2.0 * zoom, Color32::WHITE));
            p.text(s(m.p0()), Align2::CENTER_CENTER, m.number.to_string(), bold(r * 1.1), Color32::WHITE);
        }
        Tool::Text => {
            let b = m.bounds();
            let r = Rect::from_min_size(s(m.p0()), Vec2::new(b.w, b.h) * zoom);
            p.rect_filled(r, CornerRadius::same((6.0 * zoom) as u8), Color32::from_rgba_unmultiplied(255, 255, 255, 235));
            p.rect_stroke(r, CornerRadius::same((6.0 * zoom) as u8), Stroke::new(2.0 * zoom, col), egui::StrokeKind::Middle);
            p.text(r.min + Vec2::new(6.0, 4.0) * zoom, Align2::LEFT_TOP, &m.text, bold(m.font_size * zoom), col);
        }
        Tool::Select | Tool::Crop => {}
    }
}

impl View for Editor {
    fn kind(&self) -> &'static str {
        "editor"
    }
    fn title(&self) -> String {
        format!("{} · Editor · BuzzPrint", self.title)
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn on_close_request(&mut self) -> bool {
        if self.close_now {
            return true;
        }
        self.commit_text();
        self.save_note();
        if self.dirty {
            self.ask = Some(Ask::Unsaved { next: None });
            return false;
        }
        true
    }

    fn on_suspend(&mut self) {
        self.committed = None;
        if let Some(t) = self.thumbs.as_mut() {
            t.clear();
        }
    }

    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        self.keys(ctx, win);
        let mut thumbs = self.thumbs.take().unwrap_or_else(|| Thumbs::with_cap(win.hwnd, 48));
        egui::TopBottomPanel::top("tools")
            .frame(egui::Frame::default().fill(c.header).inner_margin(egui::Margin::symmetric(12, 8)))
            .show(ctx, |ui| self.toolbar(ui, win.hwnd));
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::default().fill(c.header).inner_margin(egui::Margin::symmetric(12, 4)))
            .show(ctx, |ui| {
                let dirty = if self.dirty { format!("● {}  ", t("Sin guardar")) } else { String::new() };
                ui.label(RichText::new(format!("{dirty}{}", self.status)).font(mono(11.0)).color(c.muted));
            });
        egui::TopBottomPanel::bottom("tray")
            .resizable(false)
            .exact_height(88.0)
            .frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin::symmetric(12, 8)))
            .show(ctx, |ui| self.tray(ui, ctx, &mut thumbs));
        egui::SidePanel::right("side")
            .resizable(false)
            .exact_width(260.0)
            .frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin::same(14)))
            .show(ctx, |ui| {
                self.actions(ui, win.hwnd);
                self.side(ui);
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(c.canvas))
            .show(ctx, |ui| self.canvas(ui, ctx));
        self.thumbs = Some(thumbs);
        self.dialogs(ctx, win);
        if let Some(p) = self.picker.as_mut() {
            match p.show(ctx) {
                super::colorpick::Outcome::Open => {}
                super::colorpick::Outcome::Cancel => self.picker = None,
                super::colorpick::Outcome::Apply(col) => {
                    self.picker = None;
                    self.set_color(Argb::from_rgb(col.r(), col.g(), col.b()));
                }
            }
        }
        win.title = Some(self.title());
    }
}

/// Ícono de "mover la vista": cruz con cuatro puntas de flecha.
fn paint_hand_icon(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    let r = 7.5;
    p.line_segment([c - Vec2::new(r, 0.0), c + Vec2::new(r, 0.0)], s);
    p.line_segment([c - Vec2::new(0.0, r), c + Vec2::new(0.0, r)], s);
    for d in [Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(0.0, -1.0)] {
        let tip = c + d * r;
        let n = Vec2::new(-d.y, d.x);
        p.add(Shape::convex_polygon(vec![tip + d * 1.5, tip - d * 2.5 + n * 3.0, tip - d * 2.5 - n * 3.0], col, Stroke::NONE));
    }
}

/// Botón de la barra con un ícono de Segoe Fluent, del mismo tamaño que las herramientas.
fn tool_btn(ui: &mut egui::Ui, g: char, enabled: bool) -> egui::Response {
    let c = colors();
    let b = egui::Button::new(super::widgets::icon(g, 14.0).color(if enabled { c.ink } else { c.muted })).fill(c.raised).min_size(Vec2::new(34.0, 30.0));
    ui.add_enabled(enabled, b)
}

/// Ícono de una herramienta en un cuadro de 16×16 centrado en `c` (trazos de 1,6 px, como Fluent).
fn paint_tool_icon(p: &egui::Painter, c: Pos2, tool: Tool, col: Color32) {
    let s = Stroke::new(1.6, col);
    let at = |x: f32, y: f32| Pos2::new(c.x + x, c.y + y);
    let box8 = Rect::from_center_size(c, Vec2::splat(14.0));
    match tool {
        Tool::Select => {
            // Rectángulo de trazos.
            let r = Rect::from_center_size(c, Vec2::new(14.0, 12.0));
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            for w in pts.windows(2) {
                p.extend(Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.3, col), 2.5, 2.0));
            }
        }
        Tool::Arrow => {
            p.line_segment([at(-7.0, 0.0), at(5.0, 0.0)], Stroke::new(2.0, col));
            p.add(Shape::convex_polygon(vec![at(7.5, 0.0), at(2.0, -5.0), at(2.0, 5.0)], col, Stroke::NONE));
        }
        Tool::Line => {
            p.line_segment([at(-6.5, 6.5), at(6.5, -6.5)], s);
        }
        Tool::Rect => {
            p.rect_stroke(Rect::from_center_size(c, Vec2::new(15.0, 11.0)), CornerRadius::same(1), s, egui::StrokeKind::Middle);
        }
        Tool::Ellipse => {
            p.add(egui::epaint::EllipseShape::stroke(c, Vec2::new(7.5, 6.0), s));
        }
        Tool::Highlight => {
            // Trazo de marcador: barra ancha semitransparente con una línea de texto encima.
            p.rect_filled(Rect::from_center_size(at(0.0, 1.5), Vec2::new(15.0, 7.0)), CornerRadius::same(2), crate::theme::alpha(col, 0.45));
            p.line_segment([at(-5.0, -4.5), at(5.0, -4.5)], Stroke::new(1.4, col));
        }
        Tool::Pen => {
            // Lápiz inclinado: cuerpo, punta y goma.
            let (d, n) = (Vec2::new(0.7071, -0.7071), Vec2::new(0.7071, 0.7071));
            let tip = at(-6.5, 6.5);
            let body0 = tip + d * 4.0;
            let body1 = tip + d * 15.5;
            p.add(Shape::convex_polygon(vec![body0 + n * 2.6, body1 + n * 2.6, body1 - n * 2.6, body0 - n * 2.6], Color32::TRANSPARENT, Stroke::new(1.4, col)));
            p.add(Shape::convex_polygon(vec![tip, body0 + n * 2.6, body0 - n * 2.6], col, Stroke::NONE));
            p.line_segment([tip + d * 13.0 + n * 2.6, tip + d * 13.0 - n * 2.6], Stroke::new(1.2, col));
        }
        Tool::Text => {
            p.line_segment([at(-6.0, -6.0), at(6.0, -6.0)], Stroke::new(2.0, col));
            p.line_segment([at(0.0, -6.0), at(0.0, 7.0)], Stroke::new(2.0, col));
        }
        Tool::Step => {
            p.circle_stroke(c, 7.5, s);
            p.text(at(0.0, 0.5), Align2::CENTER_CENTER, "1", egui::FontId::proportional(10.5), col);
        }
        Tool::Blur => {
            // Cuadrícula de 3×3 con celdas alternadas (pixelado).
            let cell = box8.width() / 3.0;
            for i in 0..3 {
                for j in 0..3 {
                    let r = Rect::from_min_size(box8.min + Vec2::new(i as f32 * cell, j as f32 * cell), Vec2::splat(cell)).shrink(0.6);
                    let a = if (i + j) % 2 == 0 { 1.0 } else { 0.4 };
                    p.rect_filled(r, CornerRadius::same(1), crate::theme::alpha(col, a));
                }
            }
        }
        Tool::Crop => {
            // Cuatro esquinas.
            let r = box8;
            let k = 4.5;
            for (corner, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.right_bottom(), -1.0, -1.0), (r.left_bottom(), 1.0, -1.0)] {
                p.line_segment([corner, corner + Vec2::new(k * dx, 0.0)], s);
                p.line_segment([corner, corner + Vec2::new(0.0, k * dy)], s);
            }
        }
    }
}
