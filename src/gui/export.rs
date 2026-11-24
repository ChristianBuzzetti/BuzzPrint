//! Ventana Exportar: las capturas elegidas en orden (arrastrar, Alt+↑/↓ o por fecha), título y nota de cada
//! una (vuelven a la biblioteca), formato, plantilla de diseño (incluidas y propias, con editor) y opciones.
//! La exportación corre en un hilo aparte: la ventana sigue respondiendo.

use std::any::Any;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

use egui::{Color32, CornerRadius, Key, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::Value;

use super::host::{self, Options, View, WinCtl};
use super::thumbs::Thumbs;
use super::{bold, caption, colors, mono, muted};
use crate::app::{post, Cmd};
use crate::export::layout::Src;
use crate::export::{self, templates, Format, Opts, Page};
use crate::i18n::{count, t};
use crate::store;
use crate::theme::alpha;

struct Row {
    page: Page,
    /// Tamaño de la captura en píxeles (para el diseño, sin cargar la imagen en cada cuadro).
    size: (u32, u32),
    orig_title: String,
    orig_note: String,
}

enum Busy {
    Idle,
    Working(Receiver<Result<PathBuf, String>>),
}

pub struct ExportView {
    rows: Vec<Row>,
    selected: usize,
    format: Format,
    template: String,
    title: String,
    intro: String,
    ai_drafted: bool,
    cover: bool,
    numbers: bool,
    captions: bool,
    meta: bool,
    one_per_page: bool,
    landscape: bool,
    open_after: bool,
    save_text: bool,
    folder: String,
    status: String,
    busy: Busy,
    thumbs: Option<Thumbs>,
    editing: Option<super::templates::Editor>,
    logos: super::templates::Logos,
    drag_from: Option<usize>,
    /// Página que muestra la vista previa.
    page: usize,
    /// Al elegir una captura de la lista, la vista previa va a su página.
    follow: bool,
}

pub fn open(ids: Vec<String>) {
    let items: Vec<store::Item> = ids.iter().filter_map(|id| store::with(|s| s.find(id))).filter(|i| !i.is_recording()).collect();
    if let Some(h) = host::find("export") {
        host::activate(h);
        if !items.is_empty() {
            host::with_view::<ExportView, _>(h, |v| v.set_items(items));
        }
        return;
    }
    let st = store::settings();
    let format = Format::parse(&st.export_format);
    let saved = st.export_templates.get(templates::kind_of(format)).cloned().unwrap_or_else(|| st.export_template.clone());
    let mut v = ExportView {
        rows: Vec::new(),
        selected: 0,
        format,
        template: templates::get_for(&saved, format).id,
        title: String::new(),
        intro: String::new(),
        ai_drafted: false,
        cover: st.export_cover,
        numbers: st.export_numbers,
        captions: st.export_captions,
        meta: st.export_meta,
        one_per_page: st.export_one_per_page,
        landscape: st.export_landscape,
        open_after: st.export_open,
        save_text: st.export_save_text,
        folder: st.export_folder.clone(),
        status: String::new(),
        busy: Busy::Idle,
        thumbs: None,
        editing: None,
        logos: super::templates::Logos::default(),
        drag_from: None,
        page: 0,
        follow: false,
    };
    v.set_items(items);
    host::open(Box::new(v), Options { width: 1360.0, height: 840.0, min_width: 1040.0, min_height: 620.0, ..Default::default() });
}

/// Abre Exportar con las capturas y deja que la IA escriba la guía paso a paso (título, introducción y pasos).
pub fn open_with_ai(ids: Vec<String>) {
    open(ids);
    if let Some(h) = host::find("export") {
        host::with_view::<ExportView, _>(h, |v| v.ai_draft());
    }
}

impl ExportView {
    /// La IA redacta el documento con las capturas en el orden actual. El resultado se puede corregir antes de exportar.
    fn ai_draft(&mut self) {
        let items: Vec<store::Item> = self.rows.iter().map(|r| r.page.item.clone()).collect();
        if items.is_empty() {
            return;
        }
        let Some(c) = crate::ai::ready() else {
            crate::gui::ai::open_settings();
            return;
        };
        let title = t("Documento paso a paso");
        let what = crate::i18n::f("Se van a enviar {0} captura(s) y su texto, en orden, para escribir el título, la introducción y un título y una nota por paso.", &[&items.len()]);
        let text = format!("{what}

{}", crate::i18n::f("Destino: {0}. Nada se envía hasta que tocás Enviar.", &[&crate::ai::display_name(&c)]));
        let current = self.title.clone();
        let tt = title.clone();
        crate::gui::prompt::confirm(&title, &text, &t("Enviar"), &t("Cancelar"), move || {
            let n = items.len();
            crate::gui::ai::background(&tt, &crate::i18n::f("Escribiendo la guía con {0} captura(s)…", &[&n]), move |p| crate::ai::draft_document(&items, &current, &p.cancel), |d: crate::ai::DocDraft| {
                if let Some(h) = host::find("export") {
                    host::with_view::<ExportView, _>(h, |v| v.apply_draft(d));
                    host::wake(h);
                }
            });
        });
    }

    fn apply_draft(&mut self, d: crate::ai::DocDraft) {
        self.ai_drafted = true;
        if !d.title.is_empty() {
            self.title = d.title;
        }
        if !d.intro.is_empty() {
            self.intro = d.intro;
        }
        for (r, (ti, note)) in self.rows.iter_mut().zip(d.steps) {
            if !ti.is_empty() {
                r.page.title = ti;
            }
            if !note.is_empty() {
                r.page.note = note;
            }
        }
        self.captions = true;
        self.status = t("La IA escribió la guía. Revisala antes de exportar.");
    }

    fn set_items(&mut self, items: Vec<store::Item>) {
        self.rows = items
            .into_iter()
            .map(|it| {
                let title = it.display();
                let note = it.note.clone();
                let size = export::size_of(&it);
                Row { orig_title: title.clone(), orig_note: note.clone(), size, page: Page { title, note, item: it } }
            })
            .collect();
        self.selected = 0;
        if self.title.is_empty() {
            let now = crate::clock::Stamp::now();
            self.title = format!("{} {}", t("Capturas"), now.date_text());
        }
    }

    fn save_settings(&self) {
        let (f, tpl, folder) = (self.format, self.template.clone(), self.folder.clone());
        let v = (self.cover, self.numbers, self.captions, self.meta, self.one_per_page, self.landscape, self.open_after, self.save_text);
        store::with(|s| {
            let st = &mut s.settings;
            st.export_format = f.ext().into();
            st.export_templates.insert(templates::kind_of(f).to_string(), tpl.clone());
            st.export_template = tpl;
            st.export_folder = folder;
            (st.export_cover, st.export_numbers, st.export_captions, st.export_meta, st.export_one_per_page, st.export_landscape, st.export_open, st.export_save_text) = v;
            s.save_settings();
        });
    }

    /// Títulos y notas editados vuelven a la biblioteca.
    fn save_text_back(&mut self) {
        if !self.save_text {
            return;
        }
        store::with(|s| {
            for r in &mut self.rows {
                if r.page.title.trim() != r.orig_title.trim() {
                    let _ = s.update_meta(&r.page.item, "title", Value::String(r.page.title.trim().into()));
                    r.orig_title = r.page.title.clone();
                }
                if r.page.note.trim() != r.orig_note.trim() {
                    let _ = s.update_meta(&r.page.item, "note", Value::String(r.page.note.trim().into()));
                    r.orig_note = r.page.note.clone();
                }
            }
        });
    }

    fn opts(&self) -> Opts {
        let tpl = templates::get_for(&self.template, self.format);
        let accent = tpl.accent_hex(&export::app_accent());
        Opts {
            title: self.title.trim().to_string(),
            intro: self.intro.clone(),
            cover: self.cover,
            numbers: self.numbers,
            captions: self.captions,
            meta: self.meta,
            one_per_page: self.one_per_page,
            landscape: self.landscape,
            accent,
            t: tpl,
        }
    }

    fn start_export(&mut self, hwnd: windows_sys::Win32::Foundation::HWND) {
        if self.rows.is_empty() || matches!(self.busy, Busy::Working(_)) {
            return;
        }
        self.save_settings();
        self.save_text_back();
        let pages: Vec<Page> = self.rows.iter().map(|r| r.page.clone()).collect();
        let opts = self.opts();
        let format = self.format;
        let folder = self.folder.clone();
        let name = sanitize(&opts.title);
        let hwnd = hwnd as isize;
        post(Cmd::Run(Box::new(move || {
            let dir = if folder.trim().is_empty() { None } else { Some(folder.as_str()) };
            let (label, pat) = match format {
                Format::Pptx => ("PowerPoint", "*.pptx"),
                Format::Docx => ("Word", "*.docx"),
                Format::Pdf => ("PDF", "*.pdf"),
                Format::Png => ("PNG", "*.png"),
            };
            let Some((path, _)) = crate::dialogs::save_file(hwnd as _, &t("Exportar"), &name, &[(label, pat)], dir) else { return };
            let (tx, rx) = channel();
            let folder = path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            host::with_view::<ExportView, _>(hwnd as _, |v| {
                v.busy = Busy::Working(rx);
                v.status = t("Exportando…");
                v.folder = folder;
                v.save_settings();
            });
            std::thread::spawn(move || {
                let r = export::write(&path, format, &pages, &opts).map(|_| path).map_err(|e| e.to_string());
                let _ = tx.send(r);
                host::wake(hwnd as _);
            });
        })));
    }

    fn poll(&mut self) {
        let Busy::Working(rx) = &self.busy else { return };
        if let Ok(r) = rx.try_recv() {
            self.busy = Busy::Idle;
            match r {
                Ok(path) => {
                    self.status = format!("{} · {}", t("Exportado"), path.display());
                    if self.open_after {
                        crate::output::shell_open(&path);
                    } else {
                        crate::output::reveal(&path);
                    }
                }
                Err(e) => self.status = format!("{}: {e}", t("No se pudo exportar")),
            }
            crate::app::trim_memory();
        }
    }

    /// Las capturas tal como las ve el diseño (con títulos y notas como están escritos ahora).
    fn shots(&self) -> Vec<export::layout::Shot> {
        self.rows
            .iter()
            .map(|r| export::layout::Shot {
                title: if self.captions { r.page.title.clone() } else { String::new() },
                note: r.page.note.clone(),
                meta: export::meta_line(&r.page),
                w: r.size.0,
                h: r.size.1,
            })
            .collect()
    }

    /// La página del documento donde aparece la captura `i`.
    fn page_of(doc: &export::layout::Doc, i: usize) -> Option<usize> {
        // En las portadas puede aparecer la primera captura: se busca desde el final.
        doc.pages.iter().rposition(|els| els.iter().any(|e| matches!(e, export::layout::El::Img { src: Src::Cap(k), .. } if *k == i)))
    }

    fn list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, thumbs: &mut Thumbs) {
        let c = colors();
        ui.horizontal(|ui| {
            caption(ui, &count(self.rows.len(), "captura", "capturas"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(t("Ordenar por fecha")).on_hover_text(t("De la más vieja a la más nueva")).clicked() {
                    self.rows.sort_by(|a, b| a.page.item.created.cmp(&b.page.item.created));
                }
            });
        });
        muted(ui, t("Arrastrá para reordenar · Alt+↑/↓ mueve la elegida"));
        ui.add_space(8.0);
        let mut move_to: Option<(usize, usize)> = None;
        let mut remove: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt("ex_list").auto_shrink([false, false]).show(ui, |ui| {
            let row_w = ui.available_width() - 6.0;
            for i in 0..self.rows.len() {
                let on = i == self.selected;
                let frame = egui::Frame::default()
                    .fill(if on { alpha(c.accent, 0.10) } else { c.raised })
                    .stroke(Stroke::new(1.0, if on { alpha(c.accent, 0.7) } else { c.faint_border }))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin { left: 6, right: 10, top: 8, bottom: 8 });
                let mut handle_resp = None;
                let resp = frame
                    .show(ui, |ui| {
                        ui.set_width(row_w - 16.0);
                        ui.horizontal(|ui| {
                            let (h, hr) = ui.allocate_exact_size(Vec2::new(16.0, 58.0), Sense::drag());
                            super::grip_dots(ui.painter(), h.center(), if hr.hovered() { c.accent } else { c.muted });
                            handle_resp = Some(hr.on_hover_cursor(egui::CursorIcon::Grab));
                            let (r, _) = ui.allocate_exact_size(Vec2::new(92.0, 58.0), Sense::hover());
                            ui.painter().rect_filled(r, CornerRadius::same(6), c.canvas);
                            if let Some((tex, size)) = thumbs.get(ctx, &self.rows[i].page.item.path, 1200) {
                                let inner = r.shrink(3.0);
                                let s = (inner.width() / size.x).min(inner.height() / size.y);
                                ui.painter().image(tex, egui::Rect::from_center_size(inner.center(), size * s), egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
                            }
                            // Número del paso sobre la miniatura.
                            let badge = egui::Rect::from_min_size(r.min + Vec2::new(-4.0, -4.0), Vec2::splat(20.0));
                            ui.painter().circle_filled(badge.center(), 10.0, c.accent);
                            ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, (i + 1).to_string(), bold(11.0), c.on_accent);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    let te = ui.add(super::field(&mut self.rows[i].page.title).hint_text(t("Título")).desired_width((ui.available_width() - 44.0).max(60.0)));
                                    if te.gained_focus() {
                                        self.selected = i;
                                    }
                                    if ui.add(egui::Button::new(super::widgets::icon('\u{E711}', 10.0).color(c.muted)).frame(false)).on_hover_text(t("Quitar de la exportación")).clicked() {
                                        remove = Some(i);
                                    }
                                });
                                let ne = ui.add(super::area(&mut self.rows[i].page.note).hint_text(t("Nota (opcional)")).desired_rows(1).desired_width((ui.available_width() - 16.0).max(60.0)));
                                if ne.gained_focus() {
                                    self.selected = i;
                                }
                            });
                        })
                    })
                    .response;
                let resp = ui.interact(resp.rect, ui.id().with(("row", i)), Sense::click());
                if resp.clicked() {
                    self.selected = i;
                    self.follow = true;
                }
                if handle_resp.as_ref().is_some_and(|h| h.drag_started()) {
                    self.drag_from = Some(i);
                }
                if let Some(from) = self.drag_from {
                    if from != i && ui.rect_contains_pointer(resp.rect) && ctx.input(|inp| inp.pointer.any_down()) {
                        move_to = Some((from, i));
                    }
                }
                ui.add_space(6.0);
            }
        });
        if ctx.input(|i| !i.pointer.any_down()) {
            self.drag_from = None;
        }
        if let Some((a, b)) = move_to {
            let r = self.rows.remove(a);
            self.rows.insert(b, r);
            self.drag_from = Some(b);
            self.selected = b;
        }
        if let Some(i) = remove {
            self.rows.remove(i);
            self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        }
        // Alt+↑/↓.
        let (up, down) = ctx.input(|i| (i.modifiers.alt && i.key_pressed(Key::ArrowUp), i.modifiers.alt && i.key_pressed(Key::ArrowDown)));
        if up && self.selected > 0 {
            self.rows.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
        if down && self.selected + 1 < self.rows.len() {
            self.rows.swap(self.selected, self.selected + 1);
            self.selected += 1;
        }
    }

    /// Vista previa real: las páginas del documento con las capturas, títulos y notas de ahora.
    fn preview(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, thumbs: &mut Thumbs) {
        let c = colors();
        let o = self.opts();
        let logo = self.logos.aspect(ctx, &o.t);
        let doc = export::layout::design(&self.shots(), &o, self.format, logo);
        let n = doc.pages.len().max(1);
        if self.follow {
            self.follow = false;
            if let Some(p) = Self::page_of(&doc, self.selected) {
                self.page = p;
            }
        }
        self.page = self.page.min(n - 1);
        let paths: Vec<PathBuf> = self.rows.iter().map(|r| r.page.item.path.clone()).collect();
        let mut tex = |s: Src| match s {
            Src::Cap(i) => paths.get(i).and_then(|p| thumbs.get(ctx, p, 1200)),
            Src::Logo => self.logos.get(ctx, &o.t),
        };

        // Encabezado: qué se ve y navegación.
        ui.horizontal(|ui| {
            caption(ui, &t("Vista previa"));
            let kind = match self.format {
                Format::Pptx => t("diapositiva"),
                Format::Png => t("imagen"),
                _ => t("página"),
            };
            ui.label(RichText::new(format!("{} · {}", o.t.display_name(), kind)).color(c.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(self.page + 1 < n, egui::Button::new(super::widgets::icon('\u{E76C}', 11.0))).on_hover_text(t("Siguiente")).clicked() {
                    self.page += 1;
                }
                ui.label(RichText::new(format!("{} / {n}", self.page + 1)).font(mono(12.0)).color(c.ink));
                if ui.add_enabled(self.page > 0, egui::Button::new(super::widgets::icon('\u{E76B}', 11.0))).on_hover_text(t("Anterior")).clicked() {
                    self.page -= 1;
                }
            });
        });
        ui.add_space(8.0);
        if ui.input(|i| i.key_pressed(Key::PageDown)) && self.page + 1 < n {
            self.page += 1;
        }
        if ui.input(|i| i.key_pressed(Key::PageUp)) && self.page > 0 {
            self.page -= 1;
        }

        let strip_h = if self.format == Format::Png { 0.0 } else { 118.0 };
        let avail = ui.available_size() - Vec2::new(0.0, strip_h);
        if self.format == Format::Png {
            // Una sola imagen larga: se ve a lo ancho, con desplazamiento.
            egui::ScrollArea::vertical().id_salt("ex_png").auto_shrink([false, false]).show(ui, |ui| {
                let w = (ui.available_width() - 24.0).min(760.0);
                let h = w * doc.h / doc.w;
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h + 20.0), Sense::hover());
                let page = Rect::from_min_size(egui::pos2(r.center().x - w / 2.0, r.top() + 6.0), Vec2::new(w, h));
                super::templates::paint_page(ui.painter(), page, &doc, 0, &o.t, &o.accent, &mut tex);
            });
            return;
        }
        let s = ((avail.x - 40.0) / doc.w).min((avail.y - 24.0) / doc.h).max(0.05);
        let size = Vec2::new(doc.w, doc.h) * s;
        let (area, _) = ui.allocate_exact_size(avail, Sense::hover());
        let page = Rect::from_center_size(area.center(), size);
        super::templates::paint_page(ui.painter(), page, &doc, self.page, &o.t, &o.accent, &mut tex);

        // Tira con todas las páginas.
        ui.add_space(6.0);
        egui::ScrollArea::horizontal().id_salt("ex_pages").auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                let th = 92.0;
                let tw = th * doc.w / doc.h;
                for k in 0..doc.pages.len() {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(tw + 8.0, th + 18.0), Sense::click());
                    let pr = Rect::from_min_size(r.min + Vec2::new(4.0, 4.0), Vec2::new(tw, th));
                    super::templates::paint_page(ui.painter(), pr, &doc, k, &o.t, &o.accent, &mut tex);
                    if k == self.page {
                        ui.painter().rect_stroke(pr.expand(2.5), CornerRadius::same(4), Stroke::new(2.0, c.accent), egui::StrokeKind::Middle);
                    } else if resp.hovered() {
                        ui.painter().rect_stroke(pr.expand(2.0), CornerRadius::same(4), Stroke::new(1.0, c.strong_border), egui::StrokeKind::Middle);
                    }
                    ui.painter().text(egui::pos2(pr.center().x, pr.bottom() + 9.0), egui::Align2::CENTER_CENTER, (k + 1).to_string(), mono(10.0), c.muted);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.page = k;
                    }
                }
            });
        });
    }

    fn side(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, thumbs: &mut Thumbs, hwnd: windows_sys::Win32::Foundation::HWND) {
        let c = colors();
        let working = matches!(self.busy, Busy::Working(_));
        let footer_h = 78.0;
        let scroll_h = (ui.available_height() - footer_h).max(120.0);
        egui::ScrollArea::vertical().id_salt("ex_side").max_height(scroll_h).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(ui.available_width() - 8.0);
            caption(ui, &t("Formato"));
            ui.add_space(2.0);
            let fmts = [
                (Format::Pptx, "PowerPoint", t("Diapositivas 16:9"), "P", "D24726"),
                (Format::Docx, "Word", t("Documento editable"), "W", "2B579A"),
                (Format::Pdf, "PDF", t("Listo para compartir"), "PDF", "C62828"),
                (Format::Png, &*t("Una imagen"), t("Todo en un PNG"), "PNG", "0F766E"),
            ];
            let gap = 8.0;
            let tile_w = (ui.available_width() - gap) / 2.0;
            for row in fmts.chunks(2) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (f, name, sub, badge, col) in row {
                        let on = self.format == *f;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(tile_w, 54.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(r, CornerRadius::same(10), if on { alpha(c.accent, 0.14) } else { c.raised });
                        p.rect_stroke(r, CornerRadius::same(10), Stroke::new(if on { 2.0 } else { 1.0 }, if on { c.accent } else if resp.hovered() { c.strong_border } else { c.faint_border }), egui::StrokeKind::Inside);
                        let b = egui::Rect::from_min_size(r.min + Vec2::new(10.0, 12.0), Vec2::new(30.0, 30.0));
                        p.rect_filled(b, CornerRadius::same(6), crate::theme::parse_hex(&format!("#{col}")));
                        p.text(b.center(), egui::Align2::CENTER_CENTER, *badge, bold(if badge.len() > 1 { 9.0 } else { 13.0 }), Color32::WHITE);
                        p.text(egui::pos2(b.right() + 9.0, r.top() + 18.0), egui::Align2::LEFT_CENTER, *name, bold(13.0), c.ink);
                        p.text(egui::pos2(b.right() + 9.0, r.top() + 36.0), egui::Align2::LEFT_CENTER, sub, super::prop(11.0), c.muted);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                            self.set_format(*f);
                        }
                    }
                });
                ui.add_space(gap);
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                caption(ui, &t("Diseño"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(format!("{}…", t("Todas"))).on_hover_text(t("Ver, crear, duplicar o editar plantillas")).clicked() {
                        post(Cmd::Run(Box::new(super::templates::open)));
                    }
                });
            });
            ui.add_space(2.0);
            // Galería del formato elegido, con las capturas reales.
            let list = templates::for_format(self.format);
            let shots = self.shots();
            let paths: Vec<PathBuf> = self.rows.iter().map(|r| r.page.item.path.clone()).collect();
            let col_w = (ui.available_width() - gap) / 2.0;
            let portrait = self.format != Format::Pptx;
            for row in list.chunks(2) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for tp in row {
                        let on = tp.id == self.template;
                        let mut o = self.opts();
                        o.accent = tp.accent_hex(&export::app_accent());
                        o.t = tp.normalized();
                        let doc = export::layout::design(&shots, &o, if self.format == Format::Png { Format::Pdf } else { self.format }, self.logos.aspect(ctx, &o.t));
                        let pw = if portrait { (col_w - 16.0) * 0.6 } else { col_w - 16.0 };
                        let ph = pw * doc.h / doc.w;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(col_w, ph + 40.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(r, CornerRadius::same(10), if on { alpha(c.accent, 0.12) } else { c.raised });
                        p.rect_stroke(r, CornerRadius::same(10), Stroke::new(if on { 2.0 } else { 1.0 }, if on { c.accent } else if resp.hovered() { c.strong_border } else { c.faint_border }), egui::StrokeKind::Inside);
                        let shown = if o.cover && doc.pages.len() > 1 { 1 } else { 0 };
                        let pr = Rect::from_min_size(egui::pos2(r.center().x - pw / 2.0, r.top() + 8.0), Vec2::new(pw, ph));
                        let mut tex = |s: Src| match s {
                            Src::Cap(i) => paths.get(i).and_then(|p| thumbs.get(ctx, p, 1200)),
                            Src::Logo => self.logos.get(ctx, &o.t),
                        };
                        super::templates::paint_page(ui.painter(), pr, &doc, shown, &o.t, &o.accent, &mut tex);
                        let name = tp.display_name();
                        ui.painter().text(egui::pos2(r.left() + 10.0, r.bottom() - 15.0), egui::Align2::LEFT_CENTER, if tp.built_in() { name } else { format!("★ {name}") }, bold(12.0), c.ink);
                        let desc = if tp.built_in() { t(&tp.description) } else { tp.description.clone() };
                        if resp.on_hover_text(desc).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.template = tp.id.clone();
                        }
                    }
                });
                ui.add_space(gap);
            }
            let current = templates::get_for(&self.template, self.format);
            ui.horizontal(|ui| {
                if ui.button(t("Duplicar y ajustar")).on_hover_text(t("Crea una plantilla propia a partir de esta")).clicked() {
                    self.editing = Some(super::templates::Editor::duplicate(&current));
                }
                if !current.built_in() && ui.button(t("Editar")).clicked() {
                    self.editing = Some(super::templates::Editor::edit(&current));
                }
            });

            ui.add_space(12.0);
            caption(ui, &t("Documento"));
            ui.label(t("Título"));
            ui.add(super::field(&mut self.title).desired_width(f32::INFINITY));
            ui.label(t("Introducción (opcional)"));
            ui.add(super::area(&mut self.intro).desired_rows(3).desired_width(f32::INFINITY));
            let ai_ready = crate::ai::ready().is_some();
            if ai_ready && ui
                    .button(super::widgets::icon_label(super::widgets::glyph::AI, &t("Escribir la guía con IA"), 13.0, c.ink))
                    .on_hover_text(t("Título, introducción y un título y una nota por captura"))
                    .clicked()
            {
                crate::app::post(crate::app::Cmd::Run(Box::new(|| {
                    if let Some(h) = host::find("export") {
                        host::with_view::<ExportView, _>(h, |v| v.ai_draft());
                    }
                })));
            }
            if ai_ready || self.ai_drafted {
                super::ai::report_button(ui, hwnd);
            }

            ui.add_space(10.0);
            caption(ui, &t("Opciones"));
            let w = &super::widgets::option_row;
            w(ui, &t("Portada"), &t("Título, fecha y la introducción"), &mut self.cover, false);
            w(ui, &t("Números de paso"), &t("Paso 1, 2, 3… según la plantilla"), &mut self.numbers, false);
            w(ui, &t("Títulos"), &t("El título de cada captura"), &mut self.captions, false);
            w(ui, &t("Origen y fecha"), &t("De qué ventana es y cuándo se tomó"), &mut self.meta, false);
            if self.format == Format::Docx {
                w(ui, &t("Una por página"), &t("Cada paso empieza en una hoja nueva"), &mut self.one_per_page, false);
            }
            if matches!(self.format, Format::Docx | Format::Pdf) {
                w(ui, &t("Horizontal"), &t("Hojas apaisadas"), &mut self.landscape, false);
            }
            w(ui, &t("Abrir al terminar"), &t("Si no, se muestra en la carpeta"), &mut self.open_after, false);
            w(ui, &t("Guardar títulos y notas"), &t("Vuelven a la biblioteca"), &mut self.save_text, true);
        });

        // Acción principal abajo, siempre visible.
        ui.add_space(8.0);
        let label = match self.format {
            Format::Pptx => t("Exportar PowerPoint"),
            Format::Docx => t("Exportar Word"),
            Format::Pdf => t("Exportar PDF"),
            Format::Png => t("Exportar imagen"),
        };
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!working && !self.rows.is_empty(), |ui| {
                let w = ui.available_width() - if working { 44.0 } else { 0.0 };
                if ui.add_sized([w, 40.0], egui::Button::new(RichText::new(format!("⤓  {label}…")).font(bold(15.0)).color(c.on_accent)).fill(c.accent).corner_radius(CornerRadius::same(10))).clicked() {
                    self.start_export(hwnd);
                }
            });
            if working {
                super::mark::working(ui, 36.0, c.accent);
            }
        });
        if !self.status.is_empty() {
            ui.label(RichText::new(&self.status).font(mono(10.5)).color(c.muted));
        }
    }

    /// Cambia de formato y elige la plantilla de ese formato (la última usada o la primera).
    fn set_format(&mut self, f: Format) {
        let kind = templates::kind_of(self.format).to_string();
        let id = self.template.clone();
        store::with(|s| {
            s.settings.export_templates.insert(kind, id);
        });
        self.format = f;
        let saved = store::with(|s| s.settings.export_templates.get(templates::kind_of(f)).cloned()).unwrap_or_default();
        self.template = templates::get_for(&saved, f).id;
        self.page = 0;
    }

    fn template_editor(&mut self, ctx: &egui::Context, hwnd: windows_sys::Win32::Foundation::HWND) {
        let Some(ed) = self.editing.as_mut() else { return };
        match ed.show(ctx, hwnd, &mut self.logos) {
            super::templates::EditResult::Open => {}
            super::templates::EditResult::Cancelled => self.editing = None,
            super::templates::EditResult::Saved(tp) => {
                self.template = tp.id;
                self.editing = None;
            }
        }
    }

    /// Plantilla elegida desde la galería.
    pub fn set_template(&mut self, id: &str) {
        // Una plantilla de otro formato cambia también el formato (Word, PowerPoint o PDF).
        if let Some(tp) = templates::all().into_iter().find(|x| x.id == id) {
            if !tp.format.is_empty() && tp.format != templates::kind_of(self.format) {
                self.format = Format::parse(&tp.format);
            }
        }
        self.template = id.to_string();
        self.page = 0;
    }
}


fn sanitize(s: &str) -> String {
    let s: String = s.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let s = s.trim();
    if s.is_empty() { "BuzzPrint".into() } else { s.to_string() }
}

impl View for ExportView {
    fn kind(&self) -> &'static str {
        "export"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", t("Exportar"))
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn on_close_request(&mut self) -> bool {
        self.save_settings();
        self.save_text_back();
        true
    }
    fn on_suspend(&mut self) {
        if let Some(t) = self.thumbs.as_mut() {
            t.clear();
        }
        self.logos = super::templates::Logos::default();
    }

    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        self.poll();
        let c = colors();
        let mut thumbs = self.thumbs.take().unwrap_or_else(|| Thumbs::with_cap(win.hwnd, 20));
        egui::SidePanel::left("ex_rows")
            .resizable(false)
            .exact_width(340.0)
            .frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin { left: 16, right: 10, top: 16, bottom: 12 }))
            .show(ctx, |ui| {
                if self.rows.is_empty() {
                    ui.add_space(40.0);
                    muted(ui, t("Elegí capturas en la biblioteca y volvé a Exportar (Ctrl+E)."));
                } else {
                    self.list(ui, ctx, &mut thumbs);
                }
            });
        egui::SidePanel::right("ex_opts")
            .resizable(false)
            .exact_width(360.0)
            .frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin { left: 16, right: 12, top: 16, bottom: 14 }))
            .show(ctx, |ui| self.side(ui, ctx, &mut thumbs, win.hwnd));
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(c.canvas).inner_margin(egui::Margin::same(18)))
            .show(ctx, |ui| {
                if !self.rows.is_empty() {
                    self.preview(ui, ctx, &mut thumbs);
                }
            });
        self.thumbs = Some(thumbs);
        self.template_editor(ctx, win.hwnd);
    }
}
