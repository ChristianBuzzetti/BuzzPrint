//! Guía (F1) con búsqueda y botones que abren cada función, Acerca de y Revisar componentes. Port de
//! GuideWindow.cs, AboutWindow.cs y la parte de Dependencies.cs que informa el estado.

use std::any::Any;

use egui::{RichText, Stroke};

use super::guide_content::{self as content, Topic};
use super::host::{self, Options, View, WinCtl};
use super::{bold, caption, colors, mono, muted, primary};
use crate::app::{post, Cmd};
use crate::i18n::t;

struct Guide {
    topics: Vec<Topic>,
    current: usize,
    search: String,
}

fn fold(s: &str) -> String {
    s.to_lowercase().replace(['á', 'à'], "a").replace('é', "e").replace('í', "i").replace('ó', "o").replace('ú', "u").replace('ñ', "n")
}

fn matches(tp: &Topic, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let mut all = format!("{} {}", tp.title, tp.lead);
    for b in &tp.blocks {
        all.push(' ');
        all.push_str(&b.heading);
        for (a, c) in &b.items {
            all.push(' ');
            all.push_str(a);
            all.push(' ');
            all.push_str(c);
        }
    }
    fold(&all).contains(&fold(q))
}

/// Acciones de los botones "Probalo" de la guía.
fn try_action(a: &str) {
    match a {
        "capture" => post(Cmd::CaptureRegion),
        "full" => post(Cmd::CaptureFull),
        "quick" => post(Cmd::QuickPick),
        "editor" => post(Cmd::OpenEditor(None)),
        "export" | "templates" => post(Cmd::OpenExport(Vec::new())),
        "personalize" => post(Cmd::OpenPersonalize),
        "ai" => post(Cmd::Run(Box::new(super::ai::open_settings))),
        "recordings" => post(Cmd::Run(Box::new(|| {
            super::library::show();
            super::library::show_recordings();
        }))),
        "folder" => {
            let d = crate::store::with(|s| s.capture_dir.clone());
            crate::output::shell_open(&d);
        }
        _ => {}
    }
}

impl View for Guide {
    fn kind(&self) -> &'static str {
        "guide"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", t("Guía de BuzzPrint"))
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        let ai_on = crate::store::settings().ai_enabled;
        egui::SidePanel::left("topics").exact_width(270.0).resizable(false).frame(egui::Frame::default().fill(c.header).inner_margin(egui::Margin::same(14))).show(ctx, |ui| {
            super::brand(ui, 28.0, 15.0);
            ui.add_space(8.0);
            ui.label(RichText::new(t("GUÍA")).font(mono(11.0)).color(c.accent).strong());
            ui.add_space(6.0);
            ui.add(super::field(&mut self.search).hint_text(t("Buscar en la guía")).desired_width(f32::INFINITY));
            ui.add_space(8.0);
            egui::ScrollArea::vertical().id_salt("gd_toc").show(ui, |ui| {
                let q = self.search.trim().to_string();
                for (i, tp) in self.topics.iter().enumerate() {
                    if (tp.ai_only && !ai_on) || !matches(tp, &q) {
                        continue;
                    }
                    if super::widgets::nav_row(ui, '\u{E8A5}', &tp.title, None, i == self.current).clicked() {
                        self.current = i;
                    }
                }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin::same(26))).show(ctx, |ui| {
            let Some(tp) = self.topics.get(self.current) else { return };
            egui::ScrollArea::vertical().id_salt("gd_body").auto_shrink([false, false]).show(ui, |ui| {
                ui.label(RichText::new(&tp.title).font(bold(24.0)).color(c.ink));
                ui.add_space(6.0);
                ui.label(RichText::new(&tp.lead).size(super::px(14.5)).color(c.muted));
                ui.add_space(16.0);
                for b in &tp.blocks {
                    caption(ui, &b.heading);
                    ui.add_space(4.0);
                    for (n, (a, w)) in b.items.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| match b.kind {
                            "steps" => {
                                ui.label(RichText::new(format!("{:>2}", n + 1)).font(mono(13.0)).color(c.accent).strong());
                                ui.label(RichText::new(a).color(c.ink));
                            }
                            "keys" => {
                                super::widgets::kbd(ui, a);
                                ui.label(RichText::new(w).color(c.ink));
                            }
                            _ => {
                                ui.label(RichText::new("•").color(c.accent));
                                ui.label(RichText::new(a).color(c.ink));
                            }
                        });
                        ui.add_space(4.0);
                    }
                    ui.add_space(12.0);
                }
                if !tp.tries.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (label, act) in &tp.tries {
                            if primary(ui, label).clicked() {
                                try_action(act);
                            }
                        }
                    });
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            win.close = true;
        }
    }
}

pub fn open(topic: Option<&str>) {
    if let Some(h) = host::find("guide") {
        host::activate(h);
        return;
    }
    let topics = content::all();
    let current = topic.and_then(|id| topics.iter().position(|x| x.id == id)).unwrap_or(0);
    host::open(Box::new(Guide { topics, current, search: String::new() }), Options { width: 1040.0, height: 720.0, min_width: 720.0, min_height: 480.0, ..Default::default() });
}

/// Primer arranque: ofrece la guía una sola vez (GuideOffered, compartido con .NET).
pub fn offer_once() {
    if crate::store::settings().guide_offered {
        return;
    }
    crate::store::with(|s| {
        s.settings.guide_offered = true;
        s.save_settings();
    });
    super::prompt::confirm(
        &t("Bienvenido a BuzzPrint"),
        &t("¿Querés ver la guía? Explica en pocos minutos cómo capturar, anotar, grabar y exportar. La abrís cuando quieras con F1."),
        &t("Ver la guía"),
        &t("Ahora no"),
        || open(None),
    );
}

// ------------------------------------------------------------------ Acerca de

struct About;

impl View for About {
    fn kind(&self) -> &'static str {
        "about"
    }
    fn title(&self) -> String {
        t("Acerca de BuzzPrint")
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        egui::CentralPanel::default().frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin { left: 32, right: 32, top: 26, bottom: 20 })).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                super::mark::live(ui, 84.0, c.accent, true);
                ui.add_space(10.0);
                ui.label(RichText::new("BuzzPrint").font(bold(24.0)).color(c.ink));
                ui.label(RichText::new(format!("v{} · Rust", env!("CARGO_PKG_VERSION"))).font(mono(11.0)).color(c.muted));
                ui.add_space(12.0);
                ui.label(RichText::new(t("Capturas de región, ventana y pantalla completa, grabación de video y audio, biblioteca, menú rápido para copiar y pegar, editor de anotaciones y exportación a PowerPoint, Word y PDF.")).color(c.muted));
            });
            ui.add_space(16.0);
            egui::Frame::default().fill(c.raised).stroke(Stroke::new(1.0, c.faint_border)).corner_radius(6).inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("about").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                    for (k, v) in [
                        (t("LICENCIA"), t("MIT · © 2026 Christian Buzzetti")),
                        (t("CÓDIGO"), "github.com/ChristianBuzzetti/BuzzPrint".to_string()),
                        (t("FUENTES"), t("Inter y JetBrains Mono (SIL Open Font License)")),
                        (t("VIDEO Y AUDIO"), "FFmpeg · ffmpeg.org".to_string()),
                    ] {
                        ui.label(RichText::new(k).font(mono(10.5)).color(c.accent));
                        ui.label(RichText::new(v).color(c.ink));
                        ui.end_row();
                    }
                });
            });
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button(t("Ver en GitHub")).clicked() {
                    crate::output::open_url("https://github.com/ChristianBuzzetti/BuzzPrint");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary(ui, &t("Listo")).clicked() {
                        win.close = true;
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            win.close = true;
        }
    }
}

pub fn about() {
    if let Some(h) = host::find("about") {
        host::activate(h);
        return;
    }
    host::open(Box::new(About), Options { width: 480.0, height: 520.0, min_width: 420.0, min_height: 420.0, scale: true, ..Default::default() });
}

// ------------------------------------------------------------------ Revisar componentes

struct Components;

impl View for Components {
    fn kind(&self) -> &'static str {
        "components"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", t("Revisar componentes"))
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        let ok = egui::Color32::from_rgb(0x22, 0xC5, 0x5E);
        let warn = egui::Color32::from_rgb(0xF5, 0x9E, 0x0B);
        egui::CentralPanel::default().frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin::same(24))).show(ctx, |ui| {
            ui.label(RichText::new(t("Revisar componentes")).font(bold(18.0)).color(c.ink));
            muted(ui, t("BuzzPrint funciona solo. Estos componentes opcionales suman funciones."));
            ui.add_space(12.0);
            let ffmpeg = crate::record::ffmpeg();
            let wv = crate::deps::webview2_version();
            let ocr = crate::ocr::supported();
            let rows: [(String, String, bool, Option<&str>); 3] = [
                (
                    "FFmpeg".into(),
                    match &ffmpeg {
                        Some(p) => format!("{} · {}", t("Instalado"), p.display()),
                        None => t("No está: hace falta para grabar video y audio."),
                    },
                    ffmpeg.is_some(),
                    ffmpeg.is_none().then_some("ffmpeg"),
                ),
                (
                    "Microsoft Edge WebView2".into(),
                    match &wv {
                        Some(v) => format!("{} · {v}", t("Instalado")),
                        None => t("No está: lo usa Boberto, la mascota animada."),
                    },
                    wv.is_some(),
                    None,
                ),
                (
                    t("Reconocimiento de texto de Windows (OCR)"),
                    if ocr { t("Disponible: busca por el texto de las capturas.") } else { t("Falta un idioma con reconocimiento de texto (Configuración → Hora e idioma → Idioma).") },
                    ocr,
                    None,
                ),
            ];
            for (name, detail, good, action) in rows {
                egui::Frame::default().fill(c.raised).corner_radius(6).inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(if good { "✓" } else { "!" }).font(bold(16.0)).color(if good { ok } else { warn }));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(name).strong().color(c.ink));
                            ui.label(RichText::new(detail).small().color(c.muted));
                        });
                        if let Some("ffmpeg") = action {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if primary(ui, &t("Descargar FFmpeg")).clicked() {
                                    post(Cmd::Run(Box::new(|| crate::deps::download_ffmpeg(|| crate::gui::host::wake_all()))));
                                }
                            });
                        }
                    });
                });
                ui.add_space(8.0);
            }
            ui.add_space(8.0);
            if primary(ui, &t("Listo")).clicked() {
                win.close = true;
            }
        });
    }
}

pub fn components() {
    if let Some(h) = host::find("components") {
        host::activate(h);
        return;
    }
    host::open(Box::new(Components), Options { width: 620.0, height: 470.0, min_width: 480.0, min_height: 380.0, ..Default::default() });
}
