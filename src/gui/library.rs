//! Biblioteca: la ventana principal, con la misma estructura que MainWindow.xaml de la versión .NET:
//! encabezado (logo, menús, buscador) con la barra luminosa, sub-barra de acciones, panel lateral (vistas,
//! secciones, opciones), tarjeta central (título, filtros de día/sección/agrupar, tarjetas de capturas), panel de
//! opciones a la derecha y pie con la doble regla y los atajos.

use std::any::Any;
use std::collections::HashSet;

use egui::{Align2, Color32, CornerRadius, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::Value;

use super::host::{self, Options, View, WinCtl};
use super::thumbs::Thumbs;
use super::widgets::{self as w, glyph as g};
use super::{bold, colors, mono};
use crate::app::{post, Cmd};
use crate::clock::{day_label, Stamp};
use crate::i18n::{count, t};
use crate::store::{self, Item, Kind};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    All,
    Today,
    Pinned,
    Edited,
    Recordings,
}

#[derive(Clone, PartialEq)]
enum DayFilter {
    Any,
    Today,
    Yesterday,
    Week,
    Month,
    Day(i64),
}

enum Dialog {
    Rename { ids: Vec<String>, text: String },
    NewSection { ids: Vec<String>, text: String },
    RenameSection { from: String, text: String },
    ConfirmDelete { ids: Vec<String> },
}

pub struct Library {
    tab: Tab,
    search: String,
    day: DayFilter,
    section: Option<String>,
    group_by: String,
    /// "new", "old", "name" o "custom".
    sort: String,
    /// Reordenar a mano: la tarjeta que se arrastra por su manija.
    reorder: Option<String>,
    selected: Vec<String>, // en orden de selección
    anchor: Option<usize>,
    collapsed: HashSet<String>,
    thumbs: Option<Thumbs>,
    dialog: Option<Dialog>,
    focus_search: bool,
    revision: u64,
    ocr_revision: u64,
    visible: Vec<Item>,
    status: String,
    show_options: bool,
    startup: bool,
    /// Boberto nativo en egui para la banda del encabezado (Dock) y la biblioteca vacía (Stage).
    boberto: super::boberto_view::BobertoView,
    /// Alto de encabezado + sub-barra del último cuadro (la banda de Boberto es 16:9 sobre ese alto).
    band_h: f32,
    /// Lo que ocupan de verdad el encabezado y la barra (medido en el cuadro anterior): la banda de Boberto
    /// aparece solo si entera entra al lado, así nunca tapa botones.
    /// Dónde terminan los menús (con texto y solo íconos), medido en el cuadro en que se dibujaron así.
    menus_end: [f32; 2],
    /// Ancho de la barra de botones: completa y con Menú rápido/Exportar en ícono.
    sub_need: [f32; 2],
    /// 0: todo completo; 1: el buscador es un botón; 2: además, menús y Menú rápido/Exportar solo con íconos.
    head_level: usize,
}

pub fn show() {
    if let Some(h) = host::find("library") {
        host::activate(h);
        return;
    }
    let st = store::settings();
    let view = Library {
        tab: Tab::All,
        search: String::new(),
        day: DayFilter::Any,
        section: None,
        group_by: st.library_group_by.clone(),
        sort: st.library_sort.clone(),
        reorder: None,
        selected: Vec::new(),
        anchor: None,
        collapsed: HashSet::new(),
        thumbs: None,
        dialog: None,
        focus_search: false,
        revision: 0,
        ocr_revision: 0,
        visible: Vec::new(),
        status: t("Listo"),
        show_options: false,
        startup: super::settings::startup_enabled(),
        boberto: super::boberto_view::BobertoView::new(),
        band_h: 112.0,
        menus_end: [900.0, 560.0],
        sub_need: [830.0, 690.0],
        head_level: 0,
    };
    host::open(Box::new(view), Options { width: 1440.0, height: 900.0, min_width: 860.0, min_height: 560.0, ..Default::default() });
}

/// Abre la biblioteca en la pestaña de grabaciones.
pub fn show_recordings() {
    show();
    if let Some(h) = host::find("library") {
        host::with_view::<Library, _>(h, |l| {
            l.tab = Tab::Recordings;
            l.refresh();
        });
    }
}

fn today() -> i64 {
    Stamp::now().day_number()
}

impl Library {
    fn source_items(&self) -> Vec<Item> {
        store::with(|s| if self.tab == Tab::Recordings { s.recordings().clone() } else { s.all().clone() })
    }

    fn refresh(&mut self) {
        let now = today();
        let q = self.search.trim().to_lowercase();
        let mut list = self.source_items();
        list.retain(|it| {
            let d = it.created.day_number();
            let tab_ok = match self.tab {
                Tab::All | Tab::Recordings => true,
                Tab::Today => d == now,
                Tab::Pinned => it.pinned,
                Tab::Edited => it.edited,
            };
            let day_ok = match self.day {
                DayFilter::Any => true,
                DayFilter::Today => d == now,
                DayFilter::Yesterday => d == now - 1,
                DayFilter::Week => d > now - 7,
                DayFilter::Month => {
                    let n = Stamp::now();
                    it.created.year == n.year && it.created.month == n.month
                }
                DayFilter::Day(x) => d == x,
            };
            let sec_ok = match &self.section {
                None => true,
                Some(s) if s.is_empty() => it.section.is_empty(),
                Some(s) => &it.section == s,
            };
            let q_ok = q.is_empty()
                || [&it.title, &it.source, &it.note, &it.section, &it.id].iter().any(|f| f.to_lowercase().contains(&q))
                || it.ocr_text.as_ref().is_some_and(|o| o.to_lowercase().contains(&q))
                || crate::ocr::matches(&it.id, &q);
            tab_ok && day_ok && sec_ok && q_ok
        });
        sort_items(&mut list, &self.sort);
        self.visible = list;
        let ids: HashSet<&String> = self.visible.iter().map(|i| &i.id).collect();
        self.selected.retain(|id| ids.contains(id));
    }

    fn selected_items(&self) -> Vec<Item> {
        self.selected.iter().filter_map(|id| self.visible.iter().find(|i| &i.id == id).cloned()).collect()
    }

    /// Orden de exportación: el de Ctrl+clic si se eligió así; si no, cronológico.
    fn export_ids(&self) -> Vec<String> {
        let mut items = self.selected_items();
        if self.sort == "custom" {
            // Orden propio: se exporta tal cual se ve en la biblioteca.
            items.sort_by_key(|it| self.visible.iter().position(|v| v.id == it.id).unwrap_or(usize::MAX));
        } else if !(self.selected.len() > 1 && self.anchor.is_none()) {
            items.sort_by(|a, b| a.created.cmp(&b.created));
        }
        items.into_iter().filter(|i| !i.is_recording()).map(|i| i.id).collect()
    }

    fn click(&mut self, idx: usize, ctrl: bool, shift: bool) {
        let id = self.visible[idx].id.clone();
        if ctrl {
            if let Some(p) = self.selected.iter().position(|s| *s == id) {
                self.selected.remove(p);
            } else {
                self.selected.push(id);
            }
            self.anchor = None;
        } else if shift {
            let a = self.anchor.unwrap_or(idx);
            let (lo, hi) = (a.min(idx), a.max(idx));
            self.selected = self.visible[lo..=hi].iter().map(|i| i.id.clone()).collect();
        } else {
            self.selected = vec![id];
            self.anchor = Some(idx);
        }
    }

    fn first_selected(&self) -> Option<Item> {
        self.selected_items().into_iter().next()
    }

    fn open_item(&self, it: &Item) {
        if it.is_recording() {
            crate::output::shell_open(&it.path);
        } else {
            post(Cmd::OpenEditor(Some(it.id.clone())));
        }
    }

    // ---- acciones (las mismas de los menús y la barra de .NET)
    fn act(&mut self, a: &str) {
        let need = |s: &mut Self| -> bool {
            if s.selected.is_empty() {
                s.status = t("Elegí una captura primero.");
                return false;
            }
            true
        };
        match a {
            "capture" => post(Cmd::CaptureRegion),
            "full" => post(Cmd::CaptureFull),
            "delayed" => post(Cmd::CaptureDelayed),
            "quick" => post(Cmd::QuickPick),
            "move-first" | "move-last" => {
                if need(self) {
                    self.move_selection(a == "move-first");
                }
            }
            "record" => post(Cmd::RecordVideo),
            "record-silent" | "record-system" | "record-mic" | "record-both" => {
                let (sys, mic) = match a {
                    "record-silent" => (false, false),
                    "record-system" => (true, false),
                    "record-mic" => (false, true),
                    _ => (true, true),
                };
                crate::record::set_next_audio(sys, mic);
                post(Cmd::RecordVideo);
            }
            "record-audio" => post(Cmd::RecordAudio),
            "stop" => post(Cmd::StopRecording(true)),
            "recordings" => {
                self.tab = Tab::Recordings;
                self.refresh();
            }
            "record-folder" => {
                let d = store::with(|s| s.record_dir.clone());
                crate::output::shell_open(&d);
            }
            "copy" if need(self) => {
                post(Cmd::CopyItems(self.selected.clone()));
                self.status = t("Copiada al portapapeles.");
            }
            "copy-file" if need(self) => {
                let files: Vec<_> = self.selected_items().into_iter().map(|i| i.path).collect();
                crate::output::copy_files(crate::app::main_hwnd(), &files);
                self.status = t("Copiado como archivo.");
            }
            a if a.starts_with("ai-") => crate::gui::ai::run(&a[3..], self.selected_items()),
            "copy-text" if need(self) => {
                crate::ocr::copy_text(self.selected_items());
                self.status = t("Leyendo el texto…");
            }
            "edit" => match self.first_selected() {
                Some(it) => self.open_item(&it),
                None => post(Cmd::OpenEditor(None)),
            },
            "rename" if need(self) => {
                if let Some(it) = self.first_selected() {
                    self.dialog = Some(Dialog::Rename { ids: vec![it.id.clone()], text: it.display() });
                }
            }
            "pin" if need(self) => {
                let items = self.selected_items();
                let pin = !items.iter().all(|i| i.pinned);
                store::with(|s| {
                    for it in &items {
                        let _ = s.update_meta(it, "pinned", Value::Bool(pin));
                    }
                });
                self.status = if pin { t("Fijada arriba") } else { t("Soltada") };
            }
            "export" => post(Cmd::OpenExport(self.export_ids())),
            "stitch" if need(self) => {
                crate::export::copy_as_one_image(&self.export_ids());
                self.status = t("Copiadas como una sola imagen.");
            }
            "delete" if need(self) => self.dialog = Some(Dialog::ConfirmDelete { ids: self.selected.clone() }),
            "search" => self.focus_search = true,
            "select-all" => {
                self.selected = self.visible.iter().map(|i| i.id.clone()).collect();
                self.anchor = Some(0);
            }
            "select-today" => {
                let now = today();
                self.selected = self.visible.iter().filter(|i| i.created.day_number() == now).map(|i| i.id.clone()).collect();
                self.anchor = Some(0);
            }
            "export-today" => {
                let now = today();
                let mut items: Vec<Item> = store::with(|s| s.all().iter().filter(|i| i.created.day_number() == now).cloned().collect());
                items.sort_by(|a, b| a.created.cmp(&b.created));
                post(Cmd::OpenExport(items.into_iter().map(|i| i.id).collect()));
            }
            "reveal" if need(self) => {
                if let Some(it) = self.first_selected() {
                    crate::output::reveal(&it.path);
                }
            }
            "folder" => {
                let d = store::with(|s| s.capture_dir.clone());
                crate::output::shell_open(&d);
            }
            "new-section" => self.dialog = Some(Dialog::NewSection { ids: self.selected.clone(), text: String::new() }),
            "personalize" => post(Cmd::OpenPersonalize),
            "templates" => post(Cmd::Run(Box::new(super::templates::open))),
            "settings" => post(Cmd::OpenSettings),
            "options" => self.show_options = !self.show_options,
            "guide" => post(Cmd::Run(Box::new(|| crate::gui::guide::open(None)))),
            "components" => post(Cmd::Run(Box::new(crate::gui::guide::components))),
            "about" => post(Cmd::Run(Box::new(crate::gui::guide::about))),
            "lang-en" | "lang-es" => {
                let lang = if a == "lang-es" { "es" } else { "en" };
                store::with(|s| {
                    s.settings.language = lang.into();
                    s.save_settings();
                });
                self.status = t("El idioma cambia al reiniciar BuzzPrint.");
            }
            "exit" => post(Cmd::Quit),
            _ => {}
        }
    }

    fn delete(&mut self, ids: &[String]) {
        let items: Vec<Item> = ids.iter().filter_map(|id| self.visible.iter().find(|i| &i.id == id).cloned()).collect();
        store::with(|s| s.delete_many(&items));
        if let Some(t) = self.thumbs.as_mut() {
            t.forget_all();
        }
        self.selected.clear();
        self.status = count(items.len(), "captura enviada a la Papelera", "capturas enviadas a la Papelera");
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        let typing = ctx.wants_keyboard_input();
        let (m, events) = ctx.input(|i| (i.modifiers, i.events.clone()));
        for e in events {
            match e {
                egui::Event::Key { key, pressed: true, .. } => match key {
                    Key::F if m.ctrl => self.act("search"),
                    Key::E if m.ctrl => self.act("export"),
                    Key::P if m.ctrl && !typing => self.act("pin"),
                    Key::A if m.ctrl && !typing => self.act("select-all"),
                    Key::T if m.ctrl && m.shift && !typing => self.act("copy-text"),
                    Key::F2 if !typing => self.act("rename"),
                    Key::Delete if !typing => self.act("delete"),
                    Key::Enter if !typing => self.act("edit"),
                    Key::F1 => self.act("guide"),
                    Key::Escape if !typing => self.selected.clear(),
                    _ => {}
                },
                egui::Event::Copy if !typing => self.act(if m.shift { "stitch" } else { "copy" }),
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------ encabezado

    fn header(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        ui.horizontal(|ui| {
            // Sin espacio extra: el logo queda alineado con el botón Capturar de abajo (el margen ya es de 16).
            // Logo con los ojos siguiendo al cursor y BUZZPRINT (la misma marca de todas las ventanas).
            super::brand_neon(ui, 40.0, 18.0);
            // Indicador de estado (verde con halo).
            let (d, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
            let pulse = if super::appearance().boberto_motion {
                let ph = (ui.input(|i| i.time) / 1.2) % 2.0;
                let k = if ph < 1.0 { ph } else { 2.0 - ph } as f32;
                1.0 - 0.55 * (k * k * (3.0 - 2.0 * k))
            } else {
                1.0
            };
            ui.painter().circle_filled(d.center(), 6.0, crate::theme::alpha(c.ok_glow, c.ok_glow.a() as f32 / 255.0 * pulse));
            ui.painter().circle_filled(d.center(), 3.0, crate::theme::alpha(c.ok, pulse));
            ui.add_space(10.0);
            let icons_only = self.head_level >= 2;
            self.menus(ui, icons_only);
            let end = ui.cursor().min.x;
            let slot = &mut self.menus_end[icons_only as usize];
            if (end - *slot).abs() > 1.0 {
                *slot = end;
                ui.ctx().request_repaint();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if self.head_level >= 1 {
                    self.search_button(ui);
                } else {
                    self.search_box(ui);
                }
            });
        });
    }

    fn menus(&mut self, ui: &mut egui::Ui, icons_only: bool) {
        let c = colors();
        let mut chosen: Option<&'static str> = None;
        let keys = |id: i32| crate::app::key_text(id);
        // En ventanas medianas los menús quedan con su ícono solo (el nombre va en el tooltip).
        let menu = |ui: &mut egui::Ui, gl: char, title: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            let r = ui.menu_button(w::icon_label(gl, if icons_only { "" } else { title }, if icons_only { 15.0 } else { 13.5 }, c.ink), |ui| {
                super::menu_style(ui);
                ui.set_min_width(250.0);
                add(ui);
            });
            if icons_only {
                r.response.on_hover_text(title);
            }
        };
        let mut item = |ui: &mut egui::Ui, label: &str, key: &str, a: &'static str, chosen: &mut Option<&'static str>| {
            if super::menu_item(ui, &t(label), key, true).clicked() {
                *chosen = Some(a);
            }
        };
        let ai = store::settings().ai_enabled;
        let lang = store::settings().language;
        ui.spacing_mut().item_spacing.x = 2.0;
        // Menús planos, como el Menu de WPF de la versión .NET: sin marco hasta pasar el mouse.
        {
            let v = ui.visuals_mut();
            v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            v.widgets.inactive.bg_stroke = Stroke::NONE;
            v.widgets.hovered.weak_bg_fill = c.accent_weak;
            v.widgets.hovered.bg_stroke = Stroke::NONE;
            v.widgets.open.weak_bg_fill = c.accent_weak;
        }
        menu(ui, g::CAMERA, &t("Captura"), &mut |ui| {
            item(ui, "Región o ventana", &keys(1), "capture", &mut chosen);
            item(ui, "Pantalla completa", &keys(2), "full", &mut chosen);
            item(ui, "Con demora", "", "delayed", &mut chosen);
            ui.separator();
            item(ui, "Menú rápido", &keys(3), "quick", &mut chosen);
            ui.separator();
            item(ui, "Salir", "", "exit", &mut chosen);
        });
        menu(ui, g::RECORD, &t("Grabación"), &mut |ui| {
            item(ui, "Grabar video", &keys(4), "record", &mut chosen);
            ui.separator();
            item(ui, "Video sin audio", "", "record-silent", &mut chosen);
            item(ui, "Video con audio de la PC", "", "record-system", &mut chosen);
            item(ui, "Video con micrófono", "", "record-mic", &mut chosen);
            item(ui, "Video con audio de la PC y micrófono", "", "record-both", &mut chosen);
            ui.separator();
            item(ui, "Grabar solo audio", "", "record-audio", &mut chosen);
            if super::menu_item(ui, &t("Detener grabación"), "", crate::record::is_recording()).clicked() {
                chosen = Some("stop");
            }
            ui.separator();
            item(ui, "Ver grabaciones", "", "recordings", &mut chosen);
            item(ui, "Abrir carpeta de grabaciones", "", "record-folder", &mut chosen);
        });
        menu(ui, g::EDIT, &t("Edición"), &mut |ui| {
            item(ui, "Copiar imagen", "Ctrl+C", "copy", &mut chosen);
            item(ui, "Copiar como archivo", "", "copy-file", &mut chosen);
            item(ui, "Copiar el texto de la imagen", "Ctrl+Shift+T", "copy-text", &mut chosen);
            item(ui, "Editar y anotar", "Enter", "edit", &mut chosen);
            item(ui, "Renombrar", "F2", "rename", &mut chosen);
            item(ui, "Fijar / soltar", "Ctrl+P", "pin", &mut chosen);
            ui.separator();
            item(ui, "Exportar a PowerPoint, Word o PDF…", "Ctrl+E", "export", &mut chosen);
            item(ui, "Copiar como una sola imagen", "Ctrl+Shift+C", "stitch", &mut chosen);
            item(ui, "Plantillas de documento…", "", "templates", &mut chosen);
            ui.separator();
            item(ui, "Eliminar", "Supr", "delete", &mut chosen);
        });
        menu(ui, g::LIBRARY, &t("Biblioteca"), &mut |ui| {
            item(ui, "Buscar", "Ctrl+F", "search", &mut chosen);
            item(ui, "Seleccionar todo", "Ctrl+A", "select-all", &mut chosen);
            item(ui, "Seleccionar las de hoy", "", "select-today", &mut chosen);
            item(ui, "Exportar las de hoy…", "", "export-today", &mut chosen);
            ui.separator();
            item(ui, "Nueva sección", "", "new-section", &mut chosen);
            item(ui, "Mostrar en carpeta", "", "reveal", &mut chosen);
            item(ui, "Abrir carpeta de capturas", "", "folder", &mut chosen);
        });
        let show_options = self.show_options;
        menu(ui, g::VIEW, &t("Vista"), &mut |ui| {
            item(ui, "Personalizar…", "", "personalize", &mut chosen);
            ui.separator();
            if super::menu_check(ui, &t("Panel de opciones"), show_options).clicked() {
                chosen = Some("options");
            }
            ui.separator();
            ui.menu_button(format!("      {}", t("Idioma")), |ui| {
                super::menu_style(ui);
                if super::menu_check(ui, "English", lang != "es").clicked() {
                    chosen = Some("lang-en");
                }
                if super::menu_check(ui, "Español", lang == "es").clicked() {
                    chosen = Some("lang-es");
                }
            });
        });
        if ai {
            menu(ui, g::AI, &t("IA"), &mut |ui| {
                item(ui, "Título y resumen", "", "ai-caption", &mut chosen);
                item(ui, "Documento paso a paso…", "", "ai-document", &mut chosen);
                item(ui, "Proteger datos sensibles", "", "ai-redact", &mut chosen);
                item(ui, "Preguntar sobre la captura…", "", "ai-ask", &mut chosen);
                item(ui, "Reporte de incidente", "", "ai-ticket", &mut chosen);
                item(ui, "Ordenar la biblioteca", "", "ai-organize", &mut chosen);
                ui.separator();
                item(ui, "Configurar IA…", "", "ai-settings", &mut chosen);
            });
        }
        menu(ui, g::HELP, &t("Ayuda"), &mut |ui| {
            item(ui, "Guía de BuzzPrint", "F1", "guide", &mut chosen);
            ui.separator();
            item(ui, "Revisar componentes…", "", "components", &mut chosen);
            item(ui, "Acerca de BuzzPrint", "", "about", &mut chosen);
        });
        if let Some(a) = chosen {
            self.act(a);
        }
    }

    fn search_box(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        egui::Frame::default()
            .fill(c.surface)
            .stroke(Stroke::new(1.0, c.faint_border))
            .corner_radius(w::radius())
            .inner_margin(egui::Margin { left: 10, right: 6, top: 4, bottom: 4 })
            .show(ui, |ui| {
                // Se achica si no entra (con Boberto en el encabezado), sin tapar los menús.
                let width = (ui.available_width() - 24.0).clamp(150.0, 340.0);
                ui.set_width(width);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(w::icon(g::SEARCH, 13.0).color(c.muted));
                    let r = ui.add(
                        super::field(&mut self.search)
                            .hint_text(t("Buscar capturas y texto"))
                            .frame(false)
                            .desired_width(width - 90.0),
                    );
                    if self.focus_search {
                        r.request_focus();
                        self.focus_search = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| w::kbd(ui, "Ctrl F"));
                });
            });
    }

    /// El buscador compacto: un botón con la lupa (y "Ctrl F") que abre el campo en un desplegable. Con una
    /// búsqueda activa el botón queda resaltado y muestra lo buscado.
    fn search_button(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        let active = !self.search.trim().is_empty();
        let mut job = w::icon_label(g::SEARCH, "", 14.0, if active { c.accent } else { c.ink });
        if active {
            let mut q: String = self.search.trim().chars().take(14).collect();
            if self.search.trim().chars().count() > 14 {
                q.push('…');
            }
            job.append(&q, 7.0, egui::TextFormat { font_id: super::prop(12.5), color: c.accent, valign: egui::Align::Center, ..Default::default() });
        }
        job.append("Ctrl F", 9.0, egui::TextFormat { font_id: mono(10.5), color: c.muted, valign: egui::Align::Center, ..Default::default() });
        let b = egui::Button::new(job)
            .fill(if active { c.accent_weak } else { c.surface })
            .stroke(Stroke::new(1.0, if active { c.accent_card_ring } else { c.faint_border }))
            .corner_radius(w::radius())
            .min_size(Vec2::new(0.0, 34.0));
        let r = ui.add(b).on_hover_text(t("Buscar capturas y texto"));
        let id = r.id.with("search-pop");
        if r.clicked() || self.focus_search {
            ui.memory_mut(|m| m.open_popup(id));
        }
        egui::popup::popup_below_widget(ui, id, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(320.0);
            ui.horizontal(|ui| {
                ui.label(w::icon(g::SEARCH, 13.0).color(c.muted));
                let f = ui.add(super::field(&mut self.search).hint_text(t("Buscar capturas y texto")).desired_width(270.0));
                if self.focus_search {
                    f.request_focus();
                    self.focus_search = false;
                }
                if f.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)) {
                    ui.memory_mut(|m| m.close_popup());
                }
            });
        });
    }

    fn subbar(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        let mut a: Option<&'static str> = None;
        let row = ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.spacing_mut().item_spacing.x = 1.0;
            // Capturar (acento) + flecha de otras formas.
            let mut job = w::icon_label(g::CAMERA, &t("Capturar"), 13.5, c.on_accent);
            if w::bar_button(ui, job, true, &format!("{} · {}", t("Arrastrá una región o hacé clic en una ventana"), crate::app::capture_key())).clicked() {
                a = Some("capture");
            }
            let arrow = w::bar_button(ui, w::icon_label(g::CHEVRON_DOWN, "", 10.0, c.on_accent), true, &t("Otras formas de capturar: pantalla completa, con demora"));
            w::dropdown(ui, &arrow, |ui| {
                ui.set_min_width(300.0);
                super::menu_heading(ui, &t("Capturar"));
                if super::menu_item_icon(ui, g::REGION, &t("Región o ventana"), &t("Arrastrá un área o hacé clic en una ventana"), &crate::app::capture_key(), true).clicked() {
                    a = Some("capture");
                }
                if super::menu_item_icon(ui, g::SCREEN, &t("Pantalla completa"), &t("Todo el monitor donde está el mouse"), &crate::app::key_text(2), true).clicked() {
                    a = Some("full");
                }
                let delay = store::settings().delay_seconds;
                if super::menu_item_icon(ui, g::TIMER, &t("Con demora"), &crate::i18n::f("Espera {0} s: tiempo para abrir menús o tooltips", &[&delay]), "", true).clicked() {
                    a = Some("delayed");
                }
                ui.separator();
                if super::menu_item_icon(ui, g::LIST, &t("Menú rápido"), &t("Pegar una captura reciente"), &crate::app::key_text(3), true).clicked() {
                    a = Some("quick");
                }
            });
            ui.add_space(7.0);
            // Grabar (punto rojo) + flecha.
            let mut job = egui::text::LayoutJob::default();
            job.append("●", 0.0, egui::TextFormat { font_id: super::prop(13.0), color: Color32::from_rgb(0xF4, 0x3F, 0x5E), valign: egui::Align::Center, ..Default::default() });
            job.append(&t("Grabar"), 7.0, egui::TextFormat { font_id: super::prop(13.5), color: c.ink, valign: egui::Align::Center, ..Default::default() });
            if w::bar_button(ui, job, false, &format!("{} · {}", t("Grabar video de una región, ventana o pantalla"), crate::app::key_text(4))).clicked() {
                a = Some("record");
            }
            let arrow = w::bar_button(ui, w::icon_label(g::CHEVRON_DOWN, "", 10.0, c.ink), false, &t("Grabar video o solo audio"));
            w::dropdown(ui, &arrow, |ui| {
                let st = store::settings();
                let now = match (st.record_system_audio, st.record_microphone) {
                    (true, true) => t("audio de la PC y micrófono"),
                    (true, false) => t("audio de la PC"),
                    (false, true) => t("micrófono"),
                    _ => t("sin audio"),
                };
                if super::menu_item(ui, &format!("{} ({now})", t("Grabar video")), &crate::app::key_text(4), true).clicked() {
                    a = Some("record");
                }
                ui.separator();
                for (label, act) in [("Video sin audio", "record-silent"), ("Video con audio de la PC", "record-system"), ("Video con micrófono", "record-mic"), ("Video con audio de la PC y micrófono", "record-both")] {
                    if super::menu_item(ui, &t(label), "", true).clicked() {
                        a = Some(act);
                    }
                }
                ui.separator();
                if super::menu_item(ui, &t("Grabar solo audio"), "", true).clicked() {
                    a = Some("record-audio");
                }
            });
            ui.add_space(7.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            let compact = self.head_level >= 2;
            let lbl = |s: &str| if compact { String::new() } else { t(s) };
            if w::bar_button(ui, w::icon_label(g::LIST, &lbl("Menú rápido"), 13.5, c.ink), false, &t("Menú rápido de capturas · Ctrl+Alt+V")).clicked() {
                a = Some("quick");
            }
            if w::bar_button(ui, w::icon_label(g::EXPORT, &lbl("Exportar"), 13.5, c.ink), false, &t("Exportar las capturas elegidas, en orden, a PowerPoint, Word, PDF o una sola imagen · Ctrl+E")).clicked() {
                a = Some("export");
            }
            separator(ui);
            for (gl, tip, act) in [
                (g::COPY, "Copiar imagen · Ctrl+C", "copy"),
                (g::EDIT, "Editar y anotar · Enter", "edit"),
                (g::RENAME, "Renombrar · F2", "rename"),
                (g::PIN, "Fijar arriba · Ctrl+P", "pin"),
                (g::DELETE, "Eliminar · Supr", "delete"),
            ] {
                if w::icon_button(ui, gl, &t(tip)).clicked() {
                    a = Some(act);
                }
            }
            separator(ui);
            if w::icon_button(ui, g::FOLDER, &t("Abrir la carpeta de capturas")).clicked() {
                a = Some("folder");
            }
        });
        // Lo que ocupó la fila (aunque se haya salido del lugar): es lo que la banda tiene que respetar.
        let need = row.response.rect.max.x + 16.0;
        let slot = &mut self.sub_need[(self.head_level >= 2) as usize];
        if (need - *slot).abs() > 1.0 {
            *slot = need;
            ui.ctx().request_repaint();
        }
        if let Some(a) = a {
            self.act(a);
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(RichText::new(&self.status).font(mono(11.0)).color(c.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(14.0);
                let dir = store::with(|s| s.capture_dir.clone());
                let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if w::status_segment(ui, "DIR", &name, c.muted, &t("Abrir la carpeta de capturas")).clicked() {
                    self.act("folder");
                }
                separator(ui);
                if w::status_segment(ui, &crate::app::key_text(3), &t("menú"), c.ink, &t("Menú rápido")).clicked() {
                    self.act("quick");
                }
                if w::status_segment(ui, &crate::app::key_text(4), &t("grabar"), c.ink, &t("Grabar video")).clicked() {
                    self.act("record");
                }
                if w::status_segment(ui, &crate::app::capture_key(), &t("capturar"), c.accent, &t("Capturar región o ventana")).clicked() {
                    self.act("capture");
                }
                separator(ui);
                let n = self.selected.len();
                if n > 0 {
                    ui.label(RichText::new(count(n, "seleccionada", "seleccionadas")).font(mono(11.0)).color(c.muted));
                }
            });
        });
    }

    // ------------------------------------------------------------------ panel lateral

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let now = today();
        let (all, recs) = store::with(|s| (s.all().clone(), s.recordings().len()));
        let mut bottom = None;
        let ai_on = store::settings().ai_enabled;
        // Pie fijo, como en .NET: Opciones, Personalizar, Guía y el estado de la IA (con su punto).
        egui::TopBottomPanel::bottom("side-foot").frame(egui::Frame::NONE).show_separator_line(false).show_inside(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(r.shrink2(Vec2::new(6.0, 0.0)), 0.0, colors().faint_border);
            ui.add_space(6.0);
            if w::nav_row(ui, g::OPTIONS, &t("Opciones"), None, self.show_options).clicked() {
                bottom = Some("options");
            }
            if w::nav_row(ui, g::PERSONALIZE, &t("Personalizar"), None, false).clicked() {
                bottom = Some("personalize");
            }
            if w::nav_row(ui, g::GUIDE, &t("Guía"), None, false).on_hover_text(t("Todo lo que puede hacer BuzzPrint, paso a paso · F1")).clicked() {
                bottom = Some("guide");
            }
            let conn = if ai_on { crate::ai::ready() } else { None };
            let label = match (&conn, ai_on) {
                // Corto como en .NET ("IA · Gemini"): el modelo va en el tooltip.
                (Some(cfg), _) => format!("IA · {}", crate::ai::display_name(cfg).split(" · ").next().unwrap_or_default()),
                (None, true) => format!("IA · {}", t("Sin conexión")),
                (None, false) => t("IA apagada"),
            };
            let tip = match &conn {
                Some(cfg) => format!("{} · {}", crate::ai::display_name(cfg), t("Configurar IA")),
                None => t("Configurar IA"),
            };
            let r = w::nav_row(ui, g::AI, &label, None, false).on_hover_text(tip);
            // Punto de estado: verde con halo si hay conexión, gris si no.
            let dot = Pos2::new(r.rect.max.x - 16.0, r.rect.center().y);
            if conn.is_some() {
                ui.painter().circle_filled(dot, 7.0, colors().ok_glow);
                ui.painter().circle_filled(dot, 4.0, colors().ok);
            } else {
                ui.painter().circle_filled(dot, 4.0, colors().muted);
            }
            if r.clicked() {
                bottom = Some("ai-settings");
            }
        });
        let scroll = egui::ScrollArea::vertical().id_salt("lib_side").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.set_max_width(ui.available_width() - 10.0); // lugar para la barra de desplazamiento
            w::nav_label(ui, &t("BIBLIOTECA"));
            let tabs = [
                (Tab::All, g::ALL, t("Todas"), all.len()),
                (Tab::Today, g::TODAY, t("Hoy"), all.iter().filter(|i| i.created.day_number() == now).count()),
                (Tab::Pinned, g::PIN, t("Fijadas"), all.iter().filter(|i| i.pinned).count()),
                (Tab::Edited, g::EDIT, t("Editadas"), all.iter().filter(|i| i.edited).count()),
                (Tab::Recordings, g::VIDEO, t("Grabaciones"), recs),
            ];
            for (tab, gl, label, n) in tabs {
                if w::nav_row(ui, gl, &label, Some(n), self.tab == tab).clicked() {
                    self.tab = tab;
                    self.selected.clear();
                }
            }
            w::nav_label(ui, &t("SECCIONES"));
            let names = store::with(|s| s.section_names());
            for n in &names {
                let k = all.iter().filter(|i| &i.section == n).count();
                let on = self.section.as_deref() == Some(n.as_str());
                let r = w::nav_row(ui, g::SECTION, n, Some(k), on);
                if r.clicked() {
                    self.section = if on { None } else { Some(n.clone()) };
                }
                r.context_menu(|ui| {
                    super::menu_style(ui);
                    if ui.button(t("Renombrar sección…")).clicked() {
                        self.dialog = Some(Dialog::RenameSection { from: n.clone(), text: n.clone() });
                        ui.close_menu();
                    }
                    if ui.button(t("Eliminar sección")).clicked() {
                        store::with(|s| s.delete_section(n));
                        if self.section.as_deref() == Some(n.as_str()) {
                            self.section = None;
                        }
                        ui.close_menu();
                    }
                });
            }
            let none = all.iter().filter(|i| i.section.is_empty()).count();
            let on = self.section.as_deref() == Some("");
            if w::nav_row(ui, g::NO_SECTION, &t("Sin sección"), Some(none), on).clicked() {
                self.section = if on { None } else { Some(String::new()) };
            }
            if w::nav_row(ui, g::ADD, &t("Nueva sección"), None, false).clicked() {
                self.act("new-section");
            }
            if ai_on {
                w::nav_label(ui, &t("INTELIGENCIA ARTIFICIAL"));
                for (gl, label, act) in [
                    (g::CAPTION, "Título y resumen", "ai-caption"),
                    (g::DOCUMENT, "Guía paso a paso", "ai-document"),
                    (g::SHIELD, "Proteger datos", "ai-redact"),
                    (g::HELP, "Consultar la captura", "ai-ask"),
                    (g::TICKET, "Reporte de incidente", "ai-ticket"),
                    (g::SORT, "Ordenar la biblioteca", "ai-organize"),
                ] {
                    if w::nav_row(ui, gl, &t(label), None, false).clicked() {
                        bottom = Some(act);
                    }
                }
            }
            ui.add_space(10.0);
        });
        // Desvanecido abajo de la lista cuando sigue más contenido: la última fila no queda "cortada" por la regla.
        let more_below = scroll.state.offset.y + scroll.inner_rect.height() < scroll.content_size.y - 1.0;
        if more_below {
            let r = scroll.inner_rect;
            let fade = Rect::from_min_max(Pos2::new(r.min.x, r.max.y - 28.0), r.max);
            let col = colors().header;
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(fade.left_top(), Color32::TRANSPARENT);
            mesh.colored_vertex(fade.right_top(), Color32::TRANSPARENT);
            mesh.colored_vertex(fade.left_bottom(), col);
            mesh.colored_vertex(fade.right_bottom(), col);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(1, 2, 3);
            ui.painter().add(egui::Shape::mesh(mesh));
        }
        if let Some(b) = bottom {
            self.act(b);
        }
    }

    // ------------------------------------------------------------------ tarjeta central

    fn page_header(&mut self, ui: &mut egui::Ui) {
        let c = colors();
        let title = match self.tab {
            Tab::All => t("Todas las capturas"),
            Tab::Today => t("Capturas de hoy"),
            Tab::Pinned => t("Fijadas"),
            Tab::Edited => t("Editadas"),
            Tab::Recordings => t("Grabaciones"),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(bold(super::appearance().ui_size as f32 + 9.0)).color(c.ink));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = self.visible.len();
                let what = if self.tab == Tab::Recordings { count(n, "grabación", "grabaciones") } else { count(n, "captura", "capturas") };
                let r = w::bar_button(ui, w::icon_label(g::LIST, &t("Secciones"), 13.0, c.ink), false, &t("Crear, renombrar o eliminar secciones"));
                w::dropdown(ui, &r, |ui| {
                    if ui.button(t("Nueva sección…")).clicked() {
                        self.act("new-section");
                    }
                    if let Some(s) = self.section.clone().filter(|s| !s.is_empty()) {
                        if ui.button(format!("{} «{s}»…", t("Renombrar"))).clicked() {
                            self.dialog = Some(Dialog::RenameSection { from: s.clone(), text: s.clone() });
                        }
                        if ui.button(format!("{} «{s}»", t("Eliminar"))).clicked() {
                            store::with(|st| st.delete_section(&s));
                            self.section = None;
                        }
                    }
                });
                ui.add_space(10.0);
                ui.label(RichText::new(what).font(mono(11.0)).color(c.muted));
            });
        });
        ui.add_space(8.0);
        let now = today();
        let all = store::with(|s| s.all().clone());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t("DÍA")).size(super::px(10.0)).strong().color(c.muted));
            let mut days: Vec<i64> = all.iter().map(|i| i.created.day_number()).collect();
            days.sort_unstable_by(|a, b| b.cmp(a));
            days.dedup();
            let label = match &self.day {
                DayFilter::Any => t("Cualquier día"),
                DayFilter::Today => t("Hoy"),
                DayFilter::Yesterday => t("Ayer"),
                DayFilter::Week => t("Últimos 7 días"),
                DayFilter::Month => t("Este mes"),
                DayFilter::Day(d) => day_label(*d, now),
            };
            egui::ComboBox::from_id_salt("day").width(130.0).selected_text(egui::RichText::new(label).size(super::combo_size())).show_ui(ui, |ui| {
                super::menu_style(ui);
                ui.selectable_value(&mut self.day, DayFilter::Any, t("Cualquier día"));
                ui.selectable_value(&mut self.day, DayFilter::Today, t("Hoy"));
                ui.selectable_value(&mut self.day, DayFilter::Yesterday, t("Ayer"));
                ui.selectable_value(&mut self.day, DayFilter::Week, t("Últimos 7 días"));
                ui.selectable_value(&mut self.day, DayFilter::Month, t("Este mes"));
                ui.separator();
                for d in days.iter().take(60) {
                    ui.selectable_value(&mut self.day, DayFilter::Day(*d), day_label(*d, now));
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new(t("SECCIÓN")).size(super::px(10.0)).strong().color(c.muted));
            let names = store::with(|s| s.section_names());
            let sel = match &self.section {
                None => t("Todas"),
                Some(s) if s.is_empty() => t("Sin sección"),
                Some(s) => s.clone(),
            };
            egui::ComboBox::from_id_salt("section").width(130.0).selected_text(egui::RichText::new(sel).size(super::combo_size())).show_ui(ui, |ui| {
                super::menu_style(ui);
                ui.selectable_value(&mut self.section, None, t("Todas"));
                ui.selectable_value(&mut self.section, Some(String::new()), t("Sin sección"));
                for n in names {
                    ui.selectable_value(&mut self.section, Some(n.clone()), n);
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new(t("ORDEN")).size(super::px(10.0)).strong().color(c.muted));
            let before_sort = self.sort.clone();
            egui::ComboBox::from_id_salt("sort").width(130.0).selected_text(egui::RichText::new(sort_label(&self.sort)).size(super::combo_size())).show_ui(ui, |ui| {
                super::menu_style(ui);
                for so in ["new", "old", "name", "custom"] {
                    ui.selectable_value(&mut self.sort, so.to_string(), sort_label(so));
                }
            })
            .response
            .on_hover_text(t("Personalizado: arrastrá cada tarjeta por su manija para ordenarlas a tu gusto"));
            if self.sort != before_sort {
                self.set_sort(&self.sort.clone());
            }
            ui.add_space(8.0);
            ui.label(RichText::new(t("AGRUPAR")).size(super::px(10.0)).strong().color(c.muted));
            let before = self.group_by.clone();
            egui::ComboBox::from_id_salt("group").width(120.0).selected_text(egui::RichText::new(group_label(&self.group_by)).size(super::combo_size())).show_ui(ui, |ui| {
                super::menu_style(ui);
                for gb in ["none", "day", "section", "source"] {
                    ui.selectable_value(&mut self.group_by, gb.to_string(), group_label(gb));
                }
            });
            if self.group_by != before {
                let gb = self.group_by.clone();
                store::with(|s| {
                    s.settings.library_group_by = gb;
                    s.save_settings();
                });
            }
        });
    }

    fn groups(&self) -> Vec<(String, Vec<usize>)> {
        let now = today();
        let key = |it: &Item| -> String {
            match self.group_by.as_str() {
                "day" => day_label(it.created.day_number(), now),
                "section" => if it.section.is_empty() { t("Sin sección") } else { it.section.clone() },
                "source" => if it.source.is_empty() { t("Región") } else { it.source.clone() },
                _ => String::new(),
            }
        };
        let mut out: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, it) in self.visible.iter().enumerate() {
            let k = key(it);
            if let Some(gr) = out.iter_mut().find(|gr| gr.0 == k) {
                gr.1.push(i);
            } else {
                out.push((k, vec![i]));
            }
        }
        out
    }

    fn grid(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, thumbs: &mut Thumbs, hwnd: windows_sys::Win32::Foundation::HWND) {
        let c = colors();
        if self.visible.is_empty() {
            let live_stage = self.search.is_empty() && super::appearance().boberto_visible;
            ui.add_space(if live_stage { 30.0 } else { 90.0 });
            ui.vertical_centered(|ui| {
                if live_stage {
                    // Boberto nativo en egui (biblioteca vacía, con gestos al clic y mirada al cursor).
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(300.0, 230.0), Sense::click());
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    self.boberto.stage(ui, r, &resp, hwnd);
                } else {
                    super::mark::live(ui, 96.0, c.accent, true);
                }
                ui.add_space(12.0);
                // El mensaje depende de qué se está mirando: una pestaña o un filtro vacíos no son una biblioteca vacía.
                let total = store::with(|s| s.all().len());
                let filtered = self.day != DayFilter::Any || self.section.is_some();
                let (title, hint) = if !self.search.is_empty() {
                    (t("No hay resultados"), t("Probá con otras palabras: también se busca en el texto de las capturas."))
                } else if total == 0 {
                    (t("Todavía no hay capturas"), format!("{} {}", crate::app::capture_key(), t("para capturar")))
                } else {
                    match self.tab {
                        Tab::Pinned => (t("No hay capturas fijadas"), t("Fijá una captura con el botón del alfiler para tenerla siempre a mano.")),
                        Tab::Edited => (t("No hay capturas editadas"), t("Las capturas que anotes en el editor aparecen acá.")),
                        Tab::Recordings => (t("No hay grabaciones"), t("Grabá video o audio con el botón Grabar.")),
                        Tab::Today if !filtered => (t("Hoy no hay capturas"), format!("{} {}", crate::app::capture_key(), t("para capturar"))),
                        _ => (t("No hay capturas con estos filtros"), t("Cambiá el día o la sección para ver más.")),
                    }
                };
                ui.label(RichText::new(title).font(bold(18.0)).color(c.ink));
                ui.add_space(4.0);
                ui.label(RichText::new(hint).color(c.muted));
            });
            return;
        }
        let (ctrl, shift) = ui.input(|i| (i.modifiers.ctrl, i.modifiers.shift));
        let avail = ui.available_width() - 4.0;
        let (min_w, gap) = (220.0, 14.0);
        let cols = (((avail + gap) / (min_w + gap)).floor() as usize).max(1);
        let card_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        let pic_h = (card_w * 0.56).round();

        let mut clicked: Option<(usize, bool, bool)> = None;
        let mut open: Option<usize> = None;
        let mut menu_action: Option<(usize, &'static str)> = None;
        let mut section_pick: Option<String> = None;
        let mut drag: Option<usize> = None;
        let custom = self.sort == "custom" && self.tab != Tab::Recordings;
        let mut card_rects: Vec<(usize, Rect)> = Vec::new();
        let mut grip_started: Option<usize> = None;
        for (name, idxs) in self.groups() {
            if !name.is_empty() {
                let folded = self.collapsed.contains(&name);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(avail, 30.0), Sense::click());
                let p = ui.painter();
                p.text(r.left_center() + Vec2::new(2.0, 0.0), Align2::LEFT_CENTER, if folded { g::CHEVRON_RIGHT } else { g::CHEVRON_DOWN }.to_string(), w::icon_font(10.0), c.muted);
                p.text(r.left_center() + Vec2::new(20.0, 0.0), Align2::LEFT_CENTER, "▚", super::prop(10.0), c.accent);
                let nr = p.text(r.left_center() + Vec2::new(35.0, 0.0), Align2::LEFT_CENTER, &name, bold(13.0), c.ink);
                p.text(Pos2::new(nr.max.x + 10.0, r.center().y), Align2::LEFT_CENTER, idxs.len().to_string(), mono(11.0), c.muted);
                p.rect_filled(Rect::from_min_size(Pos2::new(r.min.x, r.max.y - 1.0), Vec2::new(r.width(), 1.0)), 0.0, c.accent_row_rule);
                if resp.clicked() {
                    if folded {
                        self.collapsed.remove(&name);
                    } else {
                        self.collapsed.insert(name.clone());
                    }
                }
                ui.add_space(8.0);
                if folded {
                    continue;
                }
            }
            // Altura de cada fila según si alguna tarjeta tiene nota (línea extra).
            let rows = idxs.len().div_ceil(cols);
            let mut y = 0.0;
            let start = ui.cursor().min;
            for row in 0..rows {
                let row_items = &idxs[row * cols..((row + 1) * cols).min(idxs.len())];
                let has_note = row_items.iter().any(|&i| !self.visible[i].note.trim().is_empty());
                let card_h = pic_h + 8.0 + if has_note { 74.0 } else { 56.0 };
                for (n, &i) in row_items.iter().enumerate() {
                    let r = Rect::from_min_size(start + Vec2::new(n as f32 * (card_w + gap), y), Vec2::new(card_w, card_h));
                    if !ui.is_rect_visible(r) {
                        continue;
                    }
                    let it = &self.visible[i];
                    card_rects.push((i, r));
                    let resp = ui.interact(r, ui.id().with(("card", &it.id)), Sense::click_and_drag());
                    let on = self.selected.contains(&it.id);
                    let dragging_me = self.reorder.as_deref() == Some(it.id.as_str());
                    self.card(ui, ctx, thumbs, r, it, on, resp.hovered(), pic_h);
                    if dragging_me {
                        ui.painter().rect_filled(r, w::strip_radius(), crate::theme::alpha(c.canvas, 0.55));
                    }
                    // Orden personalizado: manija ⠿ arriba a la izquierda para reordenar dentro de la biblioteca
                    // (arrastrar la tarjeta en sí la sigue llevando a otras apps).
                    if custom {
                        let grip = Rect::from_min_size(Pos2::new(r.max.x - 38.0, r.min.y + 12.0), Vec2::new(26.0, 26.0));
                        let g_resp = ui.interact(grip, ui.id().with(("grip", &it.id)), Sense::drag());
                        if resp.hovered() || g_resp.hovered() || dragging_me {
                            let p = ui.painter();
                            p.rect_filled(grip, CornerRadius::same(6), if g_resp.hovered() || dragging_me { c.accent } else { crate::theme::alpha(c.raised, 0.92) });
                            super::grip_dots(p, grip.center(), if g_resp.hovered() || dragging_me { c.on_accent } else { c.ink });
                        }
                        let g_resp = g_resp.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(t("Arrastrá para cambiar el orden"));
                        if g_resp.drag_started() {
                            grip_started = Some(i);
                        }
                    }
                    if resp.double_clicked() {
                        open = Some(i);
                    } else if resp.clicked() {
                        clicked = Some((i, ctrl, shift));
                    }
                    if resp.drag_started() && grip_started.is_none() && self.reorder.is_none() {
                        drag = Some(i);
                    }
                    if resp.secondary_clicked() && !on {
                        clicked = Some((i, false, false));
                    }
                    let is_rec = it.is_recording();
                    resp.context_menu(|ui| {
                        super::menu_style(ui);
                        ui.set_min_width(290.0);
                        let mut act: Option<&'static str> = None;
                        let mut entries: Vec<(char, String, &str, &'static str)> = vec![
                            (g::COPY, t("Copiar imagen"), "Ctrl+C", "copy"),
                            (g::DOCUMENT, t("Copiar como archivo"), "", "copy-file"),
                            (g::CAPTION, t("Copiar el texto de la imagen"), "Ctrl+Shift+T", "copy-text"),
                        ];
                        entries.push((if is_rec { g::PLAY } else { g::EDIT }, if is_rec { t("Abrir") } else { t("Editar y anotar") }, "Enter", "edit"));
                        entries.push((g::RENAME, t("Renombrar"), "F2", "rename"));
                        entries.push((g::PIN, t("Fijar / soltar"), "Ctrl+P", "pin"));
                        entries.push((g::OPEN_FOLDER, t("Mostrar en carpeta"), "", "reveal"));
                        for (gl, label, key, a) in &entries {
                            if super::menu_item_icon(ui, *gl, label, "", key, true).clicked() {
                                act = Some(a);
                            }
                        }
                        if !is_rec {
                            ui.separator();
                            super::submenu(ui, g::AI, &t("Inteligencia artificial"), |ui| {
                                super::menu_style(ui);
                                ui.set_min_width(250.0);
                                for (gl, label, a) in [
                                    (g::CAPTION, "Título y resumen", "ai-caption"),
                                    (g::DOCUMENT, "Guía paso a paso…", "ai-document"),
                                    (g::SHIELD, "Proteger datos sensibles", "ai-redact"),
                                    (g::HELP, "Preguntar sobre la captura…", "ai-ask"),
                                    (g::TICKET, "Reporte de incidente", "ai-ticket"),
                                    (g::SORT, "Ordenar con IA", "ai-organize"),
                                ] {
                                    if super::menu_item_icon(ui, gl, &t(label), "", "", true).clicked() {
                                        act = Some(a);
                                    }
                                }
                                ui.separator();
                                if super::menu_item_icon(ui, g::SETTINGS, &t("Configurar IA…"), "", "", true).clicked() {
                                    act = Some("ai-settings");
                                }
                            });
                        }
                        ui.separator();
                        super::submenu(ui, g::SECTION, &t("Mover a sección"), |ui| {
                            super::menu_style(ui);
                            for n in store::with(|s| s.section_names()) {
                                if super::menu_item_icon(ui, g::SECTION, &n, "", "", true).clicked() {
                                    section_pick = Some(n);
                                }
                            }
                            ui.separator();
                            if super::menu_item_icon(ui, g::ADD, &format!("{}…", t("Nueva sección")), "", "", true).clicked() {
                                act = Some("new-section");
                            }
                            if super::menu_item_icon(ui, g::NO_SECTION, &t("Quitar de la sección"), "", "", true).clicked() {
                                section_pick = Some(String::new());
                            }
                        });
                        if super::menu_item_icon(ui, g::UP, &t("Mover al principio"), "", "", true).clicked() {
                            act = Some("move-first");
                        }
                        if super::menu_item_icon(ui, g::DOWN, &t("Mover al final"), "", "", true).clicked() {
                            act = Some("move-last");
                        }
                        if !is_rec {
                            ui.separator();
                            if super::menu_item_icon(ui, g::EXPORT, &t("Exportar a PowerPoint, Word o PDF…"), "", "Ctrl+E", true).clicked() {
                                act = Some("export");
                            }
                            if super::menu_item_icon(ui, g::IMAGES, &t("Copiar como una sola imagen"), "", "Ctrl+Shift+C", true).clicked() {
                                act = Some("stitch");
                            }
                        }
                        ui.separator();
                        if super::menu_item_icon_tinted(ui, g::DELETE, &t("Eliminar"), "Supr", colors().error).clicked() {
                            act = Some("delete");
                        }
                        if let Some(a) = act {
                            menu_action = Some((i, a));
                        }
                        if section_pick.is_some() && menu_action.is_none() {
                            menu_action = Some((i, "section"));
                        }
                    });
                }
                y += card_h + gap;
            }
            ui.allocate_space(Vec2::new(avail, y));
        }

        if let Some(i) = grip_started {
            self.reorder = Some(self.visible[i].id.clone());
        }
        if let Some(id) = self.reorder.clone() {
            let (pos, down) = ui.input(|inp| (inp.pointer.interact_pos(), inp.pointer.primary_down()));
            // La tarjeta más cercana al puntero y de qué lado cae (antes o después).
            let target = pos.and_then(|p| {
                card_rects.iter().min_by(|a, b| a.1.center().distance(p).total_cmp(&b.1.center().distance(p))).map(|(i, r)| (*i, p.x > r.center().x, *r))
            });
            if let Some((_, after, r)) = target {
                let x = if after { r.max.x + 7.0 } else { r.min.x - 7.0 };
                ui.painter().rect_filled(Rect::from_min_size(Pos2::new(x - 2.0, r.min.y + 6.0), Vec2::new(4.0, r.height() - 12.0)), CornerRadius::same(2), c.accent);
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if !down {
                self.reorder = None;
                if let Some((ti, after, _)) = target {
                    let target_id = self.visible[ti].id.clone();
                    if target_id != id {
                        self.move_item(&id, &target_id, after);
                    }
                }
            } else {
                ui.ctx().request_repaint();
            }
        }
        if let Some((i, ctrl, shift)) = clicked {
            self.click(i, ctrl, shift);
        }
        if let Some(i) = open {
            let it = self.visible[i].clone();
            self.selected = vec![it.id.clone()];
            self.open_item(&it);
        }
        if let Some(i) = drag {
            // Arrastrar a otras apps (Outlook, Teams, Word, el Explorador): la selección, o la tarjeta tomada.
            let id = self.visible[i].id.clone();
            if !self.selected.contains(&id) {
                self.selected = vec![id];
            }
            let files: Vec<_> = self.selected_items().into_iter().map(|i| i.path).collect();
            let h = hwnd as isize;
            post(Cmd::Run(Box::new(move || {
                crate::dragdrop::drag_files(h as _, &files);
                host::release_pointer(h as _);
            })));
        }
        if let Some((i, a)) = menu_action {
            let it = self.visible[i].clone();
            if !self.selected.contains(&it.id) {
                self.selected = vec![it.id.clone()];
            }
            if a == "section" {
                if let Some(n) = section_pick.take() {
                    let items = self.selected_items();
                    store::with(|s| s.set_section(&items, &n));
                }
            } else {
                self.act(a);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn card(&self, ui: &egui::Ui, ctx: &egui::Context, thumbs: &mut Thumbs, r: Rect, it: &Item, on: bool, hover: bool, pic_h: f32) {
        let c = colors();
        let p = ui.painter();
        let rad = w::strip_radius();
        p.rect_filled(r, rad, if on { c.accent_tint } else { c.raised });
        p.rect_stroke(r, rad, Stroke::new(1.0, if on { c.accent_ring } else if hover { c.strong_border } else { c.faint_border }), egui::StrokeKind::Inside);
        if on {
            p.rect_filled(Rect::from_min_size(r.min + Vec2::new(0.0, 10.0), Vec2::new(3.0, r.height() - 20.0)), CornerRadius::same(2), c.accent);
        }
        let pic = Rect::from_min_size(r.min + Vec2::splat(8.0), Vec2::new(r.width() - 16.0, pic_h));
        p.rect_filled(pic, w::radius(), c.canvas);
        let thumb_path = match it.kind {
            Kind::Image => Some(it.path.clone()),
            _ => it.thumb_path.clone().filter(|p| p.exists()),
        };
        let mut has_thumb = false;
        if let Some(tp) = thumb_path {
            if let Some((tex, size)) = thumbs.get(ctx, &tp, 520) {
                let inner = pic.shrink(6.0);
                let s = (inner.width() / size.x).min(inner.height() / size.y).min(1.0);
                p.image(tex, Rect::from_center_size(inner.center(), size * s), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                has_thumb = true;
            }
        }
        if !has_thumb && it.is_recording() {
            let gl = if it.kind == Kind::Video { g::VIDEO } else { g::AUDIO };
            p.text(pic.center(), Align2::CENTER_CENTER, gl.to_string(), w::icon_font(34.0), c.muted);
        }
        if it.kind == Kind::Video {
            p.circle_filled(pic.center(), 20.0, Color32::from_black_alpha(153));
            p.text(pic.center() + Vec2::new(2.0, 0.0), Align2::CENTER_CENTER, g::PLAY.to_string(), w::icon_font(16.0), Color32::WHITE);
        }
        if it.pinned {
            w::badge(p, pic.right_top() + Vec2::new(-6.0, 6.0), Align2::RIGHT_TOP, &t("FIJADA"), true);
        }
        if it.edited {
            w::badge(p, pic.left_top() + Vec2::new(6.0, 6.0), Align2::LEFT_TOP, &t("ANOTADA"), false);
        }
        if it.is_recording() {
            let kind = if it.kind == Kind::Video { "VIDEO" } else { "AUDIO" };
            w::badge(p, pic.left_bottom() + Vec2::new(6.0, -6.0), Align2::LEFT_BOTTOM, kind, false);
            if it.duration > 0.0 {
                let d = it.duration as u64;
                w::badge(p, pic.right_bottom() + Vec2::new(-6.0, -6.0), Align2::RIGHT_BOTTOM, &format!("{:02}:{:02}", d / 60, d % 60), false);
            }
        }
        let tx = r.min.x + 12.0;
        let clip_w = r.width() - 24.0;
        let mut y = pic.max.y + 10.0;
        let line = |p: &egui::Painter, y: f32, text: &str, font: egui::FontId, col: Color32| {
            let gal = p.layout_no_wrap(text.to_string(), font, col);
            p.with_clip_rect(Rect::from_min_size(Pos2::new(tx, y), Vec2::new(clip_w, 20.0))).galley(Pos2::new(tx, y), gal, col);
        };
        line(p, y, &it.display(), bold(13.0), c.ink);
        y += 20.0;
        let note = it.note.trim();
        if !note.is_empty() {
            line(p, y, note.lines().next().unwrap_or(""), super::prop(11.5), c.muted);
            y += 18.0;
        }
        let mut meta = it.created.short();
        if it.width > 0 {
            meta.push_str(&format!(" · {}×{}", it.width, it.height));
        }
        if !it.section.is_empty() {
            meta.push_str(&format!(" · {}", it.section));
        }
        line(p, y, &meta, mono(10.5), c.line_number);
    }

    // ------------------------------------------------------------------ panel de opciones

    fn options_panel(&mut self, ui: &mut egui::Ui, hwnd: windows_sys::Win32::Foundation::HWND) {
        let c = colors();
        egui::ScrollArea::vertical().id_salt("lib_grid").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width() - 12.0); // lugar para la barra de desplazamiento
            ui.add_space(4.0);
            ui.label(RichText::new(t("Opciones")).font(bold(super::appearance().ui_size as f32 + 4.0)).color(c.ink));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(RichText::new(t("Cómo trabaja BuzzPrint: capturas, grabación, atajos y carpetas. Colores, letra y bordes están en")).small().color(c.muted));
                if ui.link(RichText::new(t("Personalizar")).small()).clicked() {
                    post(Cmd::OpenPersonalize);
                }
            });
            ui.add_space(14.0);
            let mut st = store::settings();
            let mut changed = false;
            w::section_label(ui, &t("Después de capturar"));
            changed |= w::option_row(ui, &t("Copiar al portapapeles"), &t("Lista para pegar con Ctrl+V"), &mut st.copy_after_capture, false);
            changed |= w::option_row(ui, &t("Abrir en el editor"), &t("Para anotar apenas capturás"), &mut st.open_editor_after_capture, false);
            changed |= w::option_row(ui, &t("Conservar la original al editar"), &t("La versión anotada se guarda como otra captura"), &mut st.edit_keep_original, false);
            changed |= w::option_row(ui, &t("Pegar al elegir del menú"), &t("El menú rápido pega en la ventana donde estabas"), &mut st.paste_on_pick, false);
            if w::option_row(ui, &t("Iniciar con Windows"), &t("Arranca en la bandeja del sistema"), &mut self.startup, true) && !super::settings::set_startup(self.startup) {
                self.startup = super::settings::startup_enabled();
            }
            ui.add_space(18.0);
            w::section_label(ui, &t("Grabación"));
            changed |= w::option_row(ui, &t("Audio del sistema"), &t("Lo que suena en la PC: videos, llamadas, alertas"), &mut st.record_system_audio, false);
            changed |= w::option_row(ui, &t("Micrófono"), &t("Tu voz, con el micrófono predeterminado de Windows"), &mut st.record_microphone, false);
            changed |= w::option_row(ui, &t("Mostrar el cursor"), &t("El puntero del mouse aparece en el video"), &mut st.record_cursor, true);
            if changed {
                store::with(|s| {
                    s.settings = st.clone();
                    s.save_settings();
                });
            }
            ui.add_space(18.0);
            w::section_label(ui, &t("Atajos"));
            ui.add_space(4.0);
            let shortcuts = [
                (crate::app::capture_key(), t("Capturar región o ventana")),
                (crate::app::key_text(2), t("Pantalla completa")),
                (crate::app::key_text(3), t("Menú rápido")),
                (crate::app::key_text(4), t("Grabar video")),
                ("Ctrl+E".to_string(), t("Exportar")),
                ("Ctrl+Shift+C".to_string(), t("Copiar como una sola imagen")),
                ("F1".to_string(), t("Guía de BuzzPrint")),
            ];
            for (k, text) in shortcuts {
                if k.is_empty() {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.allocate_ui(Vec2::new(108.0, 22.0), |ui| w::kbd(ui, &k));
                    ui.label(RichText::new(text).color(c.muted));
                });
                ui.add_space(4.0);
            }
            if ui.link(RichText::new(t("Cambiar atajos…")).small()).clicked() {
                post(Cmd::OpenSettings);
            }
            ui.add_space(18.0);
            w::section_label(ui, &t("Almacenamiento"));
            ui.add_space(4.0);
            let (cap, rec) = store::with(|s| (s.capture_dir.clone(), s.record_dir.clone()));
            for (gl, title, dir, recordings) in [(g::IMAGES, t("Imágenes"), cap, false), (g::VIDEO, t("Videos y audio"), rec, true)] {
                egui::Frame::default()
                    .fill(c.raised)
                    .stroke(Stroke::new(1.0, c.faint_border))
                    .corner_radius(w::strip_radius())
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (b, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                            ui.painter().rect_filled(b, w::radius(), c.accent_weak);
                            ui.painter().text(b.center(), Align2::CENTER_CENTER, gl.to_string(), w::icon_font(14.0), c.accent);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(title).strong().color(c.ink));
                                let s = dir.display().to_string();
                                ui.add(egui::Label::new(RichText::new(&s).font(mono(10.5)).color(c.muted)).truncate()).on_hover_text(&s);
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(42.0);
                            if w::bar_button(ui, w::icon_label(g::OPEN_FOLDER, &t("Cambiar…"), 12.5, c.ink), false, "").clicked() {
                                let h = hwnd as isize;
                                post(Cmd::Run(Box::new(move || {
                                    if let Some(d) = crate::dialogs::pick_folder(h as _, &t("Elegí la carpeta")) {
                                        let n = store::with(|s| s.change_folder(recordings, &d, true));
                                        let msg = format!("{} · {n} {}", t("Carpeta cambiada"), t("archivos movidos"));
                                        host::with_view::<Library, _>(h as _, |l| l.status = msg);
                                        crate::watch::restart();
                                    }
                                })));
                            }
                            if w::bar_button(ui, w::icon_label(g::FOLDER, &t("Abrir"), 12.5, c.ink), false, "").clicked() {
                                crate::output::shell_open(&dir);
                            }
                        });
                    });
                ui.add_space(8.0);
            }
        });
    }

    // ------------------------------------------------------------------ diálogos

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(d) = self.dialog.as_mut() else { return };
        let mut close = false;
        let mut apply = false;
        let c = colors();
        let frame = egui::Frame::default()
            .fill(c.raised)
            .stroke(Stroke::new(1.0, c.strong_border))
            .corner_radius(w::panel_radius())
            .inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 18 })
            .shadow(egui::epaint::Shadow { offset: [0, 12], blur: 36, spread: 0, color: Color32::from_black_alpha(if c.dark { 150 } else { 60 }) });
        let modal = egui::Modal::new(egui::Id::new("library-dialog")).frame(frame).backdrop_color(Color32::from_black_alpha(110)).show(ctx, |ui| {
            ui.set_width(400.0);
            let (gl, title, hint) = match d {
                Dialog::Rename { .. } => (g::RENAME, t("Renombrar"), t("El título que se ve en la tarjeta y al exportar.")),
                Dialog::NewSection { .. } => (g::SECTION, t("Nueva sección"), t("Las secciones agrupan capturas por tema, ticket o proyecto.")),
                Dialog::RenameSection { .. } => (g::RENAME, t("Renombrar sección"), t("Las capturas de la sección quedan en ella.")),
                Dialog::ConfirmDelete { .. } => (g::DELETE, t("Eliminar"), String::new()),
            };
            let danger = matches!(d, Dialog::ConfirmDelete { .. });
            ui.horizontal(|ui| {
                // Placa con el ícono, como las tarjetas de .NET.
                let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                let tint = if danger { c.error } else { c.accent };
                ui.painter().rect_filled(r, w::radius(), crate::theme::alpha(tint, 0.14));
                ui.painter().rect_stroke(r, w::radius(), Stroke::new(1.0, crate::theme::alpha(tint, 0.35)), egui::StrokeKind::Inside);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, gl.to_string(), w::icon_font(15.0), tint);
                ui.add_space(4.0);
                ui.label(RichText::new(title).font(bold(16.0)).color(c.ink));
            });
            if !hint.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(hint).color(c.muted));
            }
            ui.add_space(12.0);
            match d {
                Dialog::Rename { text, .. } | Dialog::NewSection { text, .. } | Dialog::RenameSection { text, .. } => {
                    let r = ui.add(super::field(text).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(10, 8)));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        apply = true;
                    }
                }
                Dialog::ConfirmDelete { ids } => {
                    ui.label(RichText::new(count(ids.len(), "captura va a la Papelera de reciclaje.", "capturas van a la Papelera de reciclaje.")).color(c.ink));
                }
            }
            ui.add_space(18.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let ok_label = if danger { t("Eliminar") } else { t("Aceptar") };
                if w::confirm_button_with(ui, if danger { g::DELETE } else { g::CHECK }, &ok_label).clicked() {
                    apply = true;
                }
                if w::bar_button(ui, w::icon_label('\u{E711}', &t("Cancelar"), 13.5, c.ink), false, "").clicked() {
                    close = true;
                }
            });
        });
        if modal.should_close() {
            close = true;
        }
        if apply {
            match self.dialog.take().unwrap() {
                Dialog::Rename { ids, text } => {
                    let items: Vec<Item> = ids.iter().filter_map(|id| self.visible.iter().find(|i| &i.id == id).cloned()).collect();
                    store::with(|s| {
                        for it in &items {
                            let _ = s.update_meta(it, "title", Value::String(text.trim().to_string()));
                        }
                    });
                }
                Dialog::NewSection { ids, text } => {
                    let items: Vec<Item> = ids.iter().filter_map(|id| self.visible.iter().find(|i| &i.id == id).cloned()).collect();
                    store::with(|s| {
                        if items.is_empty() {
                            s.add_section(&text);
                        } else {
                            s.set_section(&items, &text);
                        }
                    });
                }
                Dialog::RenameSection { from, text } => {
                    store::with(|s| s.rename_section(&from, &text));
                    if self.section.as_deref() == Some(from.as_str()) {
                        self.section = Some(text.trim().to_string());
                    }
                }
                Dialog::ConfirmDelete { ids } => self.delete(&ids),
            }
        } else if close {
            self.dialog = None;
        }
    }
}

fn separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(1.0, 22.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, colors().faint_border);
    ui.add_space(6.0);
}


fn group_label(gb: &str) -> String {
    match gb {
        "day" => t("Día"),
        "section" => t("Sección"),
        "source" => t("Origen"),
        _ => t("Sin agrupar"),
    }
}

impl View for Library {
    fn kind(&self) -> &'static str {
        "library"
    }
    fn title(&self) -> String {
        "BuzzPrint".into()
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn on_suspend(&mut self) {
        if let Some(t) = self.thumbs.as_mut() {
            t.clear();
        }
        self.drop_boberto_textures();
    }

    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        // Al volver a la ventana se relee la carpeta (el vigilante de carpetas cubre el resto).
        if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(true)))) {
            store::with(|s| s.changed());
        }
        let rev = store::with(|s| s.revision);
        let ocr_rev = crate::ocr::rev();
        if rev != self.revision || (ocr_rev != self.ocr_revision && !self.search.trim().is_empty()) {
            if rev != self.revision {
                crate::ocr::refresh();
            }
            self.revision = rev;
            self.ocr_revision = ocr_rev;
            self.refresh();
        }
        let before = (self.tab, self.search.clone(), self.day.clone(), self.section.clone());
        self.keys(ctx);
        let c = colors();
        let mut thumbs = self.thumbs.take().unwrap_or_else(|| Thumbs::with_cap(win.hwnd, 64));

        // Banda de Boberto a la derecha del encabezado (ocupa las dos filas, como en .NET).
        let band = self.band_width(ctx);
        // Encabezado + barra luminosa + sub-barra.
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::default().fill(c.header).inner_margin(egui::Margin { left: 16, right: 16, top: 16, bottom: 8 }))
            .show_separator_line(false)
            .show(ctx, |ui| {
                let mut r = ui.max_rect();
                r.max.x -= band;
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(r), |ui| self.header(ui));
            });
        egui::TopBottomPanel::top("edge").exact_height(3.0).frame(egui::Frame::NONE).show_separator_line(false).show(ctx, |ui| {
            let r = ui.max_rect();
            // El barrido no pasa por la columna de Boberto.
            w::paint_edge_line(ui.ctx(), ui.painter(), r, r.max.x - band);
        });
        let subbar = egui::TopBottomPanel::top("subbar")
            .frame(egui::Frame::default().fill(c.header).inner_margin(egui::Margin::symmetric(0, 12)))
            .show_separator_line(false)
            .show(ctx, |ui| {
                let mut r = ui.max_rect();
                r.max.x -= band;
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(r), |ui| self.subbar(ui));
                let r = ui.max_rect().expand2(Vec2::new(0.0, 12.0));
                ui.painter().rect_filled(Rect::from_min_size(Pos2::new(r.min.x, r.max.y - 1.0), Vec2::new(r.width(), 1.0)), 0.0, c.strong_border);
            });
        if (subbar.response.rect.max.y - self.band_h).abs() > 0.5 {
            self.band_h = subbar.response.rect.max.y;
            ctx.request_repaint();
        }
        let band_rect = (band > 0.0).then(|| {
            let full = ctx.screen_rect();
            Rect::from_min_max(Pos2::new(full.max.x - band, 0.0), Pos2::new(full.max.x, subbar.response.rect.max.y))
        });
        self.boberto_dock(ctx, band_rect, win);
        // Pie.
        egui::TopBottomPanel::bottom("footer")
            .frame(egui::Frame::default().fill(c.header).inner_margin(egui::Margin { left: 0, right: 0, top: 12, bottom: 7 }))
            .show_separator_line(false)
            .show(ctx, |ui| {
                let r = ui.max_rect();
                // La misma línea que la del encabezado (regla + barra luminosa + barrido), a la par con ella.
                let line = Rect::from_min_size(Pos2::new(r.min.x, r.min.y - 12.0), Vec2::new(r.width(), 3.0));
                w::paint_edge_line(ui.ctx(), ui.painter(), line, line.max.x);
                self.footer(ui);
            });
        // Área de trabajo sobre la grilla Neo Mirai.
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let area = ui.max_rect();
            w::paint_grid(ui.painter(), area);
            let inner = Rect::from_min_max(area.min + Vec2::new(14.0, 12.0), area.max - Vec2::new(14.0, 12.0));
            let side_w = 248.0;
            let opt_w = if self.show_options { 320.0 } else { 0.0 };
            let side = Rect::from_min_size(inner.min, Vec2::new(side_w, inner.height()));
            let center = Rect::from_min_max(Pos2::new(side.max.x + 12.0, inner.min.y), Pos2::new(inner.max.x - if opt_w > 0.0 { opt_w + 12.0 } else { 0.0 }, inner.max.y));
            // Panel lateral.
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(side), |ui| {
                egui::Frame::default()
                    .fill(c.header)
                    .stroke(Stroke::new(1.0, c.faint_border))
                    .corner_radius(w::panel_radius())
                    .inner_margin(egui::Margin { left: 8, right: 8, top: 2, bottom: 8 })
                    .show(ui, |ui| {
                        ui.set_min_size(side.size() - Vec2::new(16.0, 10.0));
                        self.sidebar(ui);
                    });
            });
            // Tarjeta central.
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(center), |ui| {
                w::neo_frame().show(ui, |ui| {
                    ui.set_min_size(center.size() - Vec2::splat(2.0));
                    egui::Frame::default().inner_margin(egui::Margin { left: 20, right: 18, top: 16, bottom: 12 }).show(ui, |ui| self.page_header(ui));
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().rect_filled(r, 0.0, c.faint_border);
                    egui::Frame::default().inner_margin(egui::Margin { left: 14, right: 6, top: 14, bottom: 4 }).show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("lib_panel").auto_shrink([false, false]).show(ui, |ui| {
                            ui.set_width(ui.available_width() - 8.0);
                            self.grid(ui, ctx, &mut thumbs, win.hwnd);
                        });
                    });
                });
            });
            // Panel de opciones.
            if self.show_options {
                let opt = Rect::from_min_max(Pos2::new(inner.max.x - opt_w, inner.min.y), inner.max);
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(opt), |ui| {
                    w::neo_frame().inner_margin(egui::Margin::symmetric(18, 16)).show(ui, |ui| {
                        ui.set_min_size(opt.size() - Vec2::new(36.0, 32.0));
                        self.options_panel(ui, win.hwnd);
                    });
                });
            }
        });
        self.thumbs = Some(thumbs);
        self.dialogs(ctx);

        if before != (self.tab, self.search.clone(), self.day.clone(), self.section.clone()) {
            self.refresh();
        }
        let _ = Color32::TRANSPARENT;
    }
}

// ------------------------------------------------------------------ orden

fn sort_label(s: &str) -> String {
    match s {
        "old" => t("Más antiguas"),
        "name" => t("Por nombre"),
        "custom" => t("Personalizado"),
        _ => t("Más recientes"),
    }
}

/// Ordena según el modo de la biblioteca. En el personalizado, las capturas nuevas (que todavía no tienen
/// lugar) van primero, de la más reciente a la más vieja.
fn sort_items(list: &mut [Item], sort: &str) {
    match sort {
        "old" => list.sort_by(|a, b| a.created.cmp(&b.created)),
        "name" => list.sort_by_key(|a| a.display().to_lowercase()),
        "custom" => {
            let order = store::with(|s| s.settings.library_order.clone());
            let pos: std::collections::HashMap<&str, usize> = order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
            list.sort_by(|a, b| match (pos.get(a.id.as_str()), pos.get(b.id.as_str())) {
                (Some(x), Some(y)) => x.cmp(y),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, None) => b.created.cmp(&a.created),
            });
        }
        _ => list.sort_by(|a, b| b.created.cmp(&a.created)),
    }
}

impl Library {
    fn set_sort(&mut self, sort: &str) {
        self.sort = sort.to_string();
        let so = self.sort.clone();
        store::with(|s| {
            s.settings.library_sort = so;
            s.save_settings();
        });
        self.refresh();
    }

    /// El orden completo de la biblioteca tal como se ve ahora (base para mover dentro del personalizado).
    fn full_order(&self) -> Vec<String> {
        let mut all = store::with(|s| s.all().clone());
        sort_items(&mut all, &self.sort);
        all.into_iter().map(|i| i.id).collect()
    }

    fn save_order(&mut self, order: Vec<String>) {
        store::with(|s| {
            s.settings.library_order = order;
            s.settings.library_sort = "custom".into();
            s.save_settings();
        });
        self.sort = "custom".into();
        self.refresh();
    }

    /// Pone `id` antes (o después) de `target` y pasa al orden personalizado.
    fn move_item(&mut self, id: &str, target: &str, after: bool) {
        let mut order = self.full_order();
        order.retain(|x| x != id);
        let at = order.iter().position(|x| x == target).map(|p| if after { p + 1 } else { p }).unwrap_or(order.len());
        order.insert(at, id.to_string());
        self.save_order(order);
    }

    /// Mueve la selección al principio o al final, en el orden en que se ve.
    fn move_selection(&mut self, first: bool) {
        let mut order = self.full_order();
        let sel: Vec<String> = order.iter().filter(|id| self.selected.contains(id)).cloned().collect();
        order.retain(|x| !sel.contains(x));
        if first {
            order.splice(0..0, sel);
        } else {
            order.extend(sel);
        }
        self.save_order(order);
    }
}

// ------------------------------------------------------------------ Boberto (nativo en egui)

/// Ancho fijo de la banda de Boberto.
const BAND_W: f32 = 300.0;

impl Library {
    /// Ancho de la banda de Boberto: 300 o nada (y de paso, cuánto se compacta el encabezado).
    fn band_width(&mut self, ctx: &egui::Context) -> f32 {
        if !super::appearance().boberto_visible {
            return 0.0;
        }
        // Siempre del mismo tamaño (300, como la columna de .NET). Para que entre, el encabezado se compacta de
        // a pasos: primero el buscador pasa a botón y después los menús quedan con su ícono. Solo si ni así
        // entra (ventana muy chica) Boberto no se muestra y el encabezado vuelve a estar completo.
        let w = ctx.screen_rect().width();
        let need = |level: usize| {
            let menus = self.menus_end[(level >= 2) as usize];
            let search = if level >= 1 { 150.0 } else { 150.0 + 24.0 + 16.0 + 24.0 + 40.0 };
            (menus + search).max(self.sub_need[(level >= 2) as usize])
        };
        for level in 0..3 {
            if w - need(level) >= BAND_W {
                self.head_level = level;
                return BAND_W;
            }
        }
        self.head_level = (0..3).find(|&l| w - need(l) >= 0.0).unwrap_or(2);
        0.0
    }

    /// La banda 16:9 del encabezado dibujada de forma nativa en egui; clic = Boberto Studio.
    fn boberto_dock(&mut self, ctx: &egui::Context, band: Option<Rect>, win: &WinCtl) {
        let Some(r) = band else {
            return;
        };
        let mut open = false;
        egui::Area::new(egui::Id::new("boberto-band")).fixed_pos(r.min).order(egui::Order::Middle).show(ctx, |ui| {
            let resp = ui
                .allocate_rect(r, Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(t("Boberto · clic para abrir Boberto Studio"));
            if self.boberto.dock(ui, r, &resp, win.hwnd) {
                open = true;
            }
        });
        if open {
            post(Cmd::Run(Box::new(super::settings::open_studio)));
        }
    }

    /// Libera las texturas GPU de Boberto (listo para minimizar en Parte 2).
    #[allow(dead_code)]
    pub fn drop_boberto_textures(&mut self) {
        self.boberto.drop_textures();
    }
}
