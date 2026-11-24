//! Ventanas y flujo de la IA (port de AiSettingsWindow.cs, AiWindows.cs, AiBusy.cs y los clics de MainWindow):
//! antes de enviar se muestra qué se manda y a quién; el trabajo corre en un hilo con una ventana de progreso
//! cancelable; los resultados se revisan antes de aplicarse.

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, Stroke, Vec2};
use serde_json::Value;
use windows_sys::Win32::Foundation::HWND;

use super::host::{self, Options, View, WinCtl};
use super::{bold, caption, colors, mono, muted, primary};
use crate::ai::{self, keys, Client, Conversation};
use crate::app::{post, post_from_thread, Cmd};
use crate::gui::prompt;
use crate::i18n::{f, t};
use crate::report_format::Report;
use crate::store::{self, AiConnection, Item};

// ------------------------------------------------------------------ trabajo en segundo plano

/// Corre `work` en un hilo con la ventana de progreso; `done` vuelve al hilo de la interfaz.
pub fn background<T: Send + 'static>(title: &str, status: &str, work: impl FnOnce(&prompt::Progress) -> Result<T, String> + Send + 'static, done: impl FnOnce(T) + Send + 'static) {
    let p = prompt::progress(title);
    p.status(status);
    let title = title.to_string();
    std::thread::spawn(move || {
        let r = work(&p);
        let cancelled = p.cancelled();
        p.finish();
        post_from_thread(move || {
            match r {
                Ok(v) if !cancelled => done(v),
                Ok(_) => {}
                Err(_) if cancelled => {}
                Err(e) if e == "cancelado" => {}
                Err(e) => prompt::info(&title, &e),
            }
            crate::app::trim_memory();
        });
    });
}

/// IA encendida y con conexión: pregunta si se envía. Si no, abre la configuración.
fn ready_then(title: &str, what: &str, go: impl FnOnce() + 'static) {
    match ai::ready() {
        Some(c) => {
            let text = format!("{what}\n\n{}", f("Destino: {0}. Nada se envía hasta que tocás Enviar.", &[&ai::display_name(&c)]));
            prompt::confirm(title, &text, &t("Enviar"), &t("Cancelar"), go);
        }
        None => open_settings(),
    }
}

fn images(items: &[Item]) -> Vec<Item> {
    items.iter().filter(|i| !i.is_recording()).cloned().collect()
}

fn need(items: &[Item], max: usize, title: &str) -> bool {
    if items.is_empty() {
        prompt::info(title, &t("Elegí una o más capturas en la biblioteca (Ctrl+clic o Shift+clic)."));
        return false;
    }
    if items.len() > max {
        prompt::info(title, &f("Elegí hasta {0} capturas para esta función.", &[&max]));
        return false;
    }
    true
}

/// Punto de entrada de las acciones de IA ("caption", "document", "redact", "ask", "ticket", "organize", "settings").
pub fn run(action: &str, selected: Vec<Item>) {
    let sel = images(&selected);
    match action {
        "settings" => open_settings(),
        "caption" => {
            let title = t("Título y resumen");
            if !need(&sel, 50, &title) {
                return;
            }
            let what = f("Se van a enviar {0} captura(s) y su texto para escribir un título corto y un resumen. Los títulos que escribiste vos no se tocan.", &[&sel.len()]);
            let tt = title.clone();
            ready_then(&title, &what, move || caption_items(tt, sel));
        }
        "document" => {
            let title = t("Documento paso a paso");
            if !need(&sel, 40, &title) {
                return;
            }
            if ai::ready().is_none() {
                open_settings();
                return;
            }
            crate::gui::export::open_with_ai(sel.iter().map(|i| i.id.clone()).collect());
        }
        "redact" => {
            let title = t("Proteger datos sensibles");
            if !need(&sel, 1, &title) {
                return;
            }
            let it = sel[0].clone();
            match ai::ready() {
                Some(c) => {
                    let text = f("BuzzPrint busca mails, IPs, tokens y números largos en tu PC. Para encontrar también nombres, usuarios y cuentas, envía SOLO el texto de la captura (no la imagen) a {0}.", &[&ai::display_name(&c)]);
                    let tt = title.clone();
                    let it2 = it.clone();
                    prompt::confirm_or(&title, &text, &t("Buscar con IA"), &t("Solo en la PC"), move || redact(tt, it, true), move || redact(t("Proteger datos sensibles"), it2, false));
                }
                None => redact(title, it, false),
            }
        }
        "ask" => {
            let title = t("Preguntar sobre la captura");
            if !need(&sel, 1, &title) {
                return;
            }
            if ai::ready().is_none() {
                open_settings();
                return;
            }
            open_ask(sel[0].clone());
        }
        "ticket" => {
            let title = t("Reporte de incidente");
            let mut sel = sel;
            sel.sort_by_key(|i| i.id.clone());
            if !need(&sel, 6, &title) {
                return;
            }
            let what = f("Se van a enviar {0} captura(s) y su texto para armar un reporte listo para pegar en Jira o un mail.", &[&sel.len()]);
            let tt = title.clone();
            ready_then(&title, &what, move || {
                let n = sel.len();
                background(&tt.clone(), &f("Armando el reporte con {0} captura(s)…", &[&n]), move |p| ai::ticket(&sel, &p.cancel), move |text| {
                    open_text(&tt, &t("Revisalo antes de enviarlo: la IA solo sabe lo que muestran las capturas."), text)
                });
            });
        }
        "organize" => {
            let title = t("Ordenar la biblioteca");
            let mut sel = sel;
            if sel.is_empty() {
                sel = store::with(|s| s.all().iter().filter(|i| ai::has_default_title(i)).take(20).cloned().collect());
            }
            if sel.is_empty() {
                prompt::info(&title, &t("Elegí las capturas a ordenar. Sin selección, se toman las que todavía tienen nombre automático, y no quedan."));
                return;
            }
            sel.truncate(30);
            let what = f("Se van a enviar {0} captura(s) juntas, en una sola consulta, para sugerir un nombre corto y una sección para cada una. Vas a poder revisar todo antes de aplicarlo.", &[&sel.len()]);
            let tt = title.clone();
            ready_then(&title, &what, move || organize(tt, sel));
        }
        _ => {}
    }
}

fn caption_items(title: String, sel: Vec<Item>) {
    let n = sel.len();
    background(
        &title,
        &f("Describiendo {0} captura(s)…", &[&n]),
        move |p| {
            let client = Client::from_settings()?;
            let mut out = Vec::new();
            for (i, it) in sel.iter().enumerate() {
                if p.cancelled() {
                    break;
                }
                p.report(i as u64, Some(n as u64));
                p.status(&f("Describiendo capturas… {0}/{1}", &[&i, &n]));
                let (ti, cap) = ai::describe(&client, it, &p.cancel)?;
                out.push((it.clone(), ti, cap));
            }
            Ok(out)
        },
        |out: Vec<(Item, String, String)>| {
            let n = apply_captions(&out);
            crate::app::notify(&t("Título y resumen"), &f("IA: {0} captura(s) con título y resumen", &[&n]), None);
        },
    );
}

fn apply_captions(out: &[(Item, String, String)]) -> usize {
    store::with(|s| {
        let mut n = 0;
        for (it, title, cap) in out {
            if !cap.is_empty() {
                let _ = s.update_meta(it, "note", Value::String(cap.clone()));
                n += 1;
            }
            if !title.is_empty() && ai::has_default_title(it) {
                let _ = s.update_meta(it, "title", Value::String(title.clone()));
            }
        }
        s.changed();
        n
    });
    host::wake_all();
    out.len()
}

/// Título y resumen automáticos para una captura nueva (opción "Describir cada captura nueva").
pub fn auto_caption(it: Item) {
    if !store::settings().ai_auto_caption || ai::ready().is_none() {
        return;
    }
    std::thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        let r = Client::from_settings().and_then(|c| ai::describe(&c, &it, &cancel));
        if let Ok((ti, cap)) = r {
            post_from_thread(move || {
                apply_captions(&[(it, ti, cap)]);
            });
        }
    });
}

fn redact(title: String, it: Item, use_ai: bool) {
    let it2 = it.clone();
    background(
        &title.clone(),
        &t("Buscando datos sensibles en la captura…"),
        move |p| ai::find_sensitive(&it, use_ai, &p.cancel),
        move |spots| {
            if spots.is_empty() {
                prompt::info(&title, &t("No se encontraron datos sensibles en esta captura."));
                return;
            }
            let rows: Vec<(String, String)> = spots.iter().map(|s| (format!("{}: {}", ai::kind_name(&s.kind), ai::mask(&s.text)), String::new())).collect();
            open_review(
                &title,
                &t("Marcá lo que querés tapar. Se guarda una copia pixelada; la captura original no se modifica."),
                rows,
                &t("Guardar copia protegida"),
                Box::new(move |keep: Vec<usize>| {
                    if keep.is_empty() {
                        return;
                    }
                    let chosen: Vec<ai::Spot> = keep.iter().map(|&i| spots[i].clone()).collect();
                    match ai::save_redacted_copy(&it2, &chosen) {
                        Ok(copy) => {
                            host::wake_all();
                            crate::app::notify(&t("Proteger datos sensibles"), &f("Copia protegida: {0} dato(s) tapado(s) · {1}", &[&keep.len(), &copy.display()]), Some(copy.id));
                        }
                        Err(e) => prompt::info(&t("Proteger datos sensibles"), &e),
                    }
                }),
            );
        },
    );
}

fn organize(title: String, sel: Vec<Item>) {
    let n = sel.len();
    let tt = title.clone();
    background(
        &title,
        &f("Ordenando {0} capturas en una sola consulta…", &[&n]),
        move |p| {
            let client = Client::from_settings()?;
            let mut sections = store::with(|s| s.section_names());
            let mut results = Vec::new();
            for (k, chunk) in sel.chunks(ai::ORGANIZE_BATCH).enumerate() {
                if k > 0 {
                    p.report((k * ai::ORGANIZE_BATCH) as u64, Some(n as u64));
                    p.status(&f("Ordenando… {0}/{1}", &[&(k * ai::ORGANIZE_BATCH), &n]));
                }
                match ai::organize(&client, chunk, &sections, &p.cancel) {
                    Ok(rs) => {
                        for r in rs {
                            if !r.2.is_empty() && !sections.contains(&r.2) {
                                sections.push(r.2.clone());
                            }
                            results.push(r);
                        }
                    }
                    Err(e) if results.is_empty() => return Err(e),
                    Err(_) => break,
                }
            }
            Ok(results)
        },
        move |results: Vec<(Item, String, String)>| {
            let rows: Vec<(String, String)> = results
                .iter()
                .map(|(it, ti, sec)| (ti.clone(), f("Sección: {0}   ·   antes: {1}", &[&if sec.is_empty() { "—".to_string() } else { sec.clone() }, &it.display()])))
                .collect();
            open_review(
                &tt,
                &t("Destildá lo que no quieras cambiar. Se renombran las capturas y se mueven a la sección sugerida."),
                rows,
                &t("Aplicar"),
                Box::new(move |keep: Vec<usize>| {
                    store::with(|s| {
                        for &i in &keep {
                            let (it, ti, sec) = &results[i];
                            if !ti.is_empty() {
                                let _ = s.update_meta(it, "title", Value::String(ti.clone()));
                            }
                            if !sec.is_empty() {
                                s.add_section(sec);
                                s.set_section(std::slice::from_ref(it), sec);
                            }
                        }
                        s.changed();
                    });
                    host::wake_all();
                    crate::app::notify(&t("Ordenar la biblioteca"), &f("IA: {0} captura(s) ordenadas", &[&keep.len()]), None);
                }),
            );
        },
    );
}

// ------------------------------------------------------------------ marco común

fn header(ui: &mut egui::Ui, title: &str, purpose: &str) {
    let c = colors();
    ui.horizontal(|ui| {
        super::corner_tri(ui, 10.0, c.accent);
        ui.label(RichText::new(title).font(bold(16.0)).color(c.ink));
    });
    if !purpose.is_empty() {
        muted(ui, purpose);
    }
    ui.add_space(8.0);
}

fn panel() -> egui::CentralPanel {
    egui::CentralPanel::default().frame(egui::Frame::default().fill(colors().surface).inner_margin(egui::Margin::same(20)))
}

pub fn report_button(ui: &mut egui::Ui, owner: HWND) {
    const URL: &str = "https://github.com/ChristianBuzzetti/BuzzPrint/issues/new?template=ai-content-report.md";
    if ui.button(t("Reportar contenido inapropiado"))
        .on_hover_text(t("Abre un reporte en GitHub. No se envía nada solo: pegá ahí lo que quieras mostrar."))
        .clicked()
    {
        post(Cmd::Run(Box::new(move || {
            if !crate::output::open_url(URL) {
                let message = if crate::output::copy_text(owner, URL) {
                    t("El enlace al reporte se copió al portapapeles.")
                } else {
                    format!("{}\n{URL}", t("No se pudo abrir el enlace ni copiarlo al portapapeles."))
                };
                prompt::info(&t("Reportar contenido inapropiado"), &message);
            }
        })));
    }
}

// ------------------------------------------------------------------ texto de resultado

struct TextWin {
    title: String,
    purpose: String,
    text: String,
    status: String,
}

impl TextWin {
    fn save(&self, owner: HWND, image: bool, accent: u32) {
        let title = self.title.clone();
        let text = self.text.clone();
        post(Cmd::Run(Box::new(move || {
            let label = t(if image { "Guardar imagen" } else { "Guardar texto" });
            let valid_title = title.replace(|c: char| c.is_control() || "\\/:*?\"<>|".contains(c), "_");
            let stem = format!("{valid_title} {}", crate::clock::Stamp::now().name_part());
            let plain = t("Texto");
            let filters: &[(&str, &str)] = if image {
                &[("PNG", "*.png")]
            } else {
                &[(&plain, "*.txt"), ("Markdown", "*.md"), ("HTML", "*.html")]
            };
            let Some((path, _)) = crate::dialogs::save_file(owner, &label, &stem, filters, None) else { return };
            let report = Report::new(&title, &text, accent);
            let result = if image {
                report.image().and_then(|img| crate::image::save_png(&img, &path))
            } else {
                let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_ascii_lowercase();
                match extension.as_str() {
                    "md" => std::fs::write(&path, report.markdown()),
                    "html" | "htm" => std::fs::write(&path, report.html_page()),
                    _ => std::fs::write(&path, &text),
                }
            };
            host::with_view::<TextWin, _>(owner, |view| {
                view.status = match result {
                    Ok(()) => format!("{}{}", t(if image { "Imagen guardada: " } else { "Archivo guardado: " }), path.file_name().unwrap_or_default().to_string_lossy()),
                    Err(err) => format!("{} · {err}", t("No se pudo guardar")),
                };
            });
            host::wake(owner);
            if image {
                crate::app::trim_memory();
            }
        })));
    }
}

impl View for TextWin {
    fn kind(&self) -> &'static str {
        "ai-text"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", self.title)
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let color = colors().accent;
        let accent = u32::from_be_bytes([255, color.r(), color.g(), color.b()]);
        egui::TopBottomPanel::bottom("ai_text_actions").frame(egui::Frame::default().fill(colors().surface).inner_margin(egui::Margin::same(20))).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if primary(ui, &t("Copiar")).clicked() {
                    let ok = crate::output::copy_text(win.hwnd, &self.text);
                    self.status = t(if ok { "Texto copiado al portapapeles." } else { "No se pudo copiar al portapapeles." });
                }
                if ui.button(t("Copiar con formato")).clicked() {
                    let report = Report::new(&self.title, &self.text, accent);
                    let ok = crate::output::copy_html(win.hwnd, &report.html(), &self.text);
                    self.status = t(if ok { "Copiado con formato: pegalo en un mail, Teams, Jira o Word." } else { "No se pudo copiar al portapapeles." });
                }
                if ui.button(t("Guardar imagen")).clicked() {
                    self.save(win.hwnd, true, accent);
                }
                if ui.button(t("Guardar texto")).clicked() {
                    self.save(win.hwnd, false, accent);
                }
                if ui.button(t("Cerrar")).clicked() {
                    win.close = true;
                }
                report_button(ui, win.hwnd);
            });
            let status = if self.status.is_empty() { "\u{00a0}" } else { &self.status };
            ui.add(egui::Label::new(RichText::new(status).color(colors().muted)).truncate());
        });
        panel().show(ctx, |ui| {
            header(ui, &self.title, &self.purpose);
            egui::ScrollArea::vertical().id_salt("ai_a").show(ui, |ui| {
                ui.add(super::area(&mut self.text).desired_width(f32::INFINITY).desired_rows(20).font(egui::TextStyle::Body));
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            win.close = true;
        }
    }
}

pub fn open_text(title: &str, purpose: &str, text: String) {
    host::open(Box::new(TextWin { title: title.into(), purpose: purpose.into(), text, status: String::new() }), Options { width: 900.0, height: 640.0, ..Default::default() });
}

// ------------------------------------------------------------------ revisar y aplicar

struct ReviewWin {
    title: String,
    purpose: String,
    rows: Vec<(String, String, bool)>,
    apply: String,
    on_apply: Option<Box<dyn FnOnce(Vec<usize>)>>,
}

impl View for ReviewWin {
    fn kind(&self) -> &'static str {
        "ai-review"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", self.title)
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        panel().show(ctx, |ui| {
            header(ui, &self.title, &self.purpose);
            report_button(ui, win.hwnd);
            let h = ui.available_height() - 46.0;
            egui::ScrollArea::vertical().id_salt("ai_b").max_height(h).show(ui, |ui| {
                for (main, sub, on) in &mut self.rows {
                    ui.horizontal(|ui| {
                        ui.checkbox(on, "");
                        ui.vertical(|ui| {
                            ui.label(RichText::new(main.as_str()).color(c.ink));
                            if !sub.is_empty() {
                                ui.label(RichText::new(sub.as_str()).small().color(c.muted));
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary(ui, &self.apply).clicked() {
                    let keep: Vec<usize> = self.rows.iter().enumerate().filter(|r| r.1 .2).map(|r| r.0).collect();
                    if let Some(cb) = self.on_apply.take() {
                        crate::app::post(crate::app::Cmd::Run(Box::new(move || cb(keep))));
                    }
                    win.close = true;
                }
                if ui.button(t("Cancelar")).clicked() {
                    win.close = true;
                }
                let n = self.rows.iter().filter(|r| r.2).count();
                ui.label(RichText::new(format!("{n} / {}", self.rows.len())).font(mono(11.0)).color(c.muted));
            });
        });
    }
}

pub fn open_review(title: &str, purpose: &str, rows: Vec<(String, String)>, apply: &str, on_apply: Box<dyn FnOnce(Vec<usize>)>) {
    let rows = rows.into_iter().map(|(a, b)| (a, b, true)).collect();
    host::open(Box::new(ReviewWin { title: title.into(), purpose: purpose.into(), rows, apply: apply.into(), on_apply: Some(on_apply) }), Options { width: 760.0, height: 600.0, ..Default::default() });
}

// ------------------------------------------------------------------ preguntar sobre la captura

struct AskWin {
    conv: Arc<Mutex<Conversation>>,
    item_name: String,
    transcript: Arc<Mutex<Vec<(bool, String)>>>, // (es pregunta, texto)
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    input: String,
    target: String,
    hwnd: isize,
}

impl View for AskWin {
    fn kind(&self) -> &'static str {
        "ai-ask"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", t("Preguntar sobre la captura"))
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn on_close_request(&mut self) -> bool {
        self.cancel.store(true, Ordering::SeqCst);
        true
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        self.hwnd = win.hwnd as isize;
        let c = colors();
        let busy = self.busy.load(Ordering::SeqCst);
        panel().show(ctx, |ui| {
            header(ui, &t("Preguntar sobre la captura"), &f("Cada pregunta envía la captura y su texto a {0}. Preguntá qué significa un error, qué revisar o cómo seguir.", &[&self.target]));
            report_button(ui, win.hwnd);
            ui.label(RichText::new(&self.item_name).font(mono(11.0)).color(c.accent));
            ui.add_space(6.0);
            let h = ui.available_height() - 90.0;
            egui::ScrollArea::vertical().id_salt("ai_chat").max_height(h).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                if self.transcript.lock().unwrap().is_empty() && !busy {
                    // Sin preguntas todavía: ejemplos en vez de un área vacía.
                    ui.add_space((h * 0.28).max(12.0));
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(t("Todavía no preguntaste nada")).font(bold(14.0)).color(c.ink));
                        ui.add_space(6.0);
                        for ex in ["¿Qué significa este error?", "¿Qué debería revisar primero?", "Resumí lo que se ve en la captura"] {
                            ui.label(RichText::new(format!("“{}”", t(ex))).color(c.muted));
                        }
                    });
                }
                for (q, text) in self.transcript.lock().unwrap().iter() {
                    let (fill, ink) = if *q { (c.accent_weak, c.ink) } else { (c.raised, c.ink) };
                    egui::Frame::default().fill(fill).corner_radius(6).inner_margin(10).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(text.as_str()).color(ink)).selectable(true));
                    });
                    ui.add_space(8.0);
                }
                if busy {
                    ui.horizontal(|ui| {
                        super::mark::working(ui, 34.0, c.accent);
                        ui.add_space(6.0);
                        ui.label(RichText::new(t("Mirando la captura para responder…")).color(c.muted).italics());
                    });
                }
            });
            ui.add_space(8.0);
            let mut send = false;
            ui.horizontal(|ui| {
                let r = ui.add_sized([ui.available_width() - 110.0, 60.0], super::area(&mut self.input).hint_text(t("Escribí tu pregunta… (Enter envía, Shift+Enter baja de línea)")));
                if r.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) {
                    send = true;
                }
                ui.vertical(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(RichText::new(t("Preguntar")).color(c.on_accent).font(bold(13.0))).fill(c.accent).min_size(Vec2::new(100.0, 28.0))).clicked() {
                        send = true;
                    }
                    if busy && ui.add(egui::Button::new(t("Detener")).min_size(Vec2::new(100.0, 26.0))).clicked() {
                        self.cancel.store(true, Ordering::SeqCst);
                    }
                });
            });
            if send && !busy && !self.input.trim().is_empty() {
                let q = self.input.trim().to_string();
                self.input.clear();
                self.transcript.lock().unwrap().push((true, q.clone()));
                self.busy.store(true, Ordering::SeqCst);
                self.cancel.store(false, Ordering::SeqCst);
                let (conv, tr, b, cancel, h) = (self.conv.clone(), self.transcript.clone(), self.busy.clone(), self.cancel.clone(), self.hwnd);
                std::thread::spawn(move || {
                    let r = conv.lock().unwrap().ask(&q, &cancel);
                    let text = match r {
                        Ok(a) => a,
                        Err(e) if e == "cancelado" => crate::i18n::t("Se detuvo la consulta."),
                        Err(e) => format!("⚠ {e}"),
                    };
                    tr.lock().unwrap().push((false, text));
                    b.store(false, Ordering::SeqCst);
                    host::wake(h as windows_sys::Win32::Foundation::HWND);
                });
            }
        });
    }
}

pub fn open_ask(it: Item) {
    let target = ai::ready().map(|c| ai::display_name(&c)).unwrap_or_else(|| "IA".into());
    let view = AskWin {
        item_name: it.display(),
        conv: Arc::new(Mutex::new(Conversation::new(it))),
        transcript: Arc::new(Mutex::new(Vec::new())),
        busy: Arc::new(AtomicBool::new(false)),
        cancel: Arc::new(AtomicBool::new(false)),
        input: String::new(),
        target,
        hwnd: 0,
    };
    host::open(Box::new(view), Options { width: 960.0, height: 700.0, ..Default::default() });
}

// ------------------------------------------------------------------ configurar IA

struct Edit {
    conn: AiConnection,
    key: String,
    stored_key: Option<String>,
    is_new: bool,
}

struct SettingsWin {
    enabled: bool,
    auto_caption: bool,
    conns: Vec<AiConnection>,
    active: String,
    edit: Option<Edit>,
    models: Arc<Mutex<Option<Result<Vec<String>, String>>>>,
    test: Arc<Mutex<Option<Result<String, String>>>>,
    working: Arc<AtomicBool>,
    hwnd: isize,
}

impl SettingsWin {
    fn save(&self) {
        let (en, auto, conns, active) = (self.enabled, self.auto_caption, self.conns.clone(), self.active.clone());
        store::with(|s| {
            s.settings.ai_enabled = en;
            s.settings.ai_auto_caption = auto;
            s.settings.ai_connections = conns;
            s.settings.ai_active_id = active;
            s.save_settings();
        });
        host::wake_all();
    }

    fn begin_edit(&mut self, conn: AiConnection, is_new: bool) {
        let stored = keys::load(&conn.id, &conn.provider);
        self.edit = Some(Edit { conn, key: String::new(), stored_key: stored, is_new });
        *self.models.lock().unwrap() = None;
        *self.test.lock().unwrap() = None;
    }

    fn spawn<T: Send + 'static>(&self, slot: Arc<Mutex<Option<T>>>, work: impl FnOnce() -> T + Send + 'static) {
        if self.working.swap(true, Ordering::SeqCst) {
            return;
        }
        let (w, h) = (self.working.clone(), self.hwnd);
        std::thread::spawn(move || {
            let r = work();
            *slot.lock().unwrap() = Some(r);
            w.store(false, Ordering::SeqCst);
            host::wake(h as windows_sys::Win32::Foundation::HWND);
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let _c = colors();
        let working = self.working.load(Ordering::SeqCst);
        let Some(e) = self.edit.as_mut() else { return };
        caption(ui, &if e.is_new { t("Nueva conexión") } else { t("Editar conexión") });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for p in ai::PROVIDERS {
                let on = e.conn.provider == p.id;
                let txt = format!("{}  {}", t(p.name), t(p.tag));
                if super::chip(ui, &txt, on).clicked() && !on {
                    e.conn.provider = p.id.into();
                    e.conn.model.clear();
                    e.stored_key = keys::load(&e.conn.id, p.id);
                }
            }
        });
        let p = ai::provider(&e.conn.provider);
        ui.add_space(4.0);
        muted(ui, t(p.hint));
        ui.add_space(8.0);
        egui::Grid::new("aiedit").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(t("Nombre (opcional)"));
            let auto = ai::display_name(&AiConnection { name: String::new(), ..e.conn.clone() });
            ui.add(super::field(&mut e.conn.name).desired_width(360.0).hint_text(auto));
            ui.end_row();
            if e.conn.provider == "custom" {
                ui.label(t("URL del servicio"));
                ui.add(super::field(&mut e.conn.base_url).desired_width(360.0).hint_text("https://…/v1"));
                ui.end_row();
            }
            if p.needs_key {
                ui.label(t("API key"));
                ui.horizontal(|ui| {
                    let hint = e.stored_key.as_deref().map(|k| format!("{} {}", t("Guardada:"), keys::masked(k))).unwrap_or_else(|| t("Pegá la key acá"));
                    ui.add(super::field(&mut e.key).password(true).desired_width(300.0).hint_text(hint));
                    if !p.key_url.is_empty() && ui.link(t("Crear una key")).clicked() {
                        crate::output::open_url(p.key_url);
                    }
                });
                ui.end_row();
            }
            ui.label(t("Modelo"));
            ui.horizontal(|ui| {
                let hint = p.models.first().map(|m| m.0).unwrap_or("");
                ui.add(super::field(&mut e.conn.model).desired_width(300.0).hint_text(hint));
                if ui.add_enabled(!working, egui::Button::new(t("Ver modelos"))).clicked() {
                    let (conn, key) = (e.conn.clone(), e.key.clone());
                    let slot = self.models.clone();
                    let (w, h) = (self.working.clone(), self.hwnd);
                    if !w.swap(true, Ordering::SeqCst) {
                        std::thread::spawn(move || {
                            let cancel = AtomicBool::new(false);
                            *slot.lock().unwrap() = Some(Client::list_models(&conn, Some(&key), &cancel));
                            w.store(false, Ordering::SeqCst);
                            host::wake(h as windows_sys::Win32::Foundation::HWND);
                        });
                    }
                }
            });
            ui.end_row();
        });
        // Sugeridos del proveedor y los que devolvió el servicio.
        let mut pick: Option<String> = None;
        if !p.models.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for (id, note) in p.models {
                    if super::chip(ui, id, e.conn.model == *id).on_hover_text(t(note)).clicked() {
                        pick = Some(id.to_string());
                    }
                }
            });
        }
        match &*self.models.lock().unwrap() {
            Some(Ok(list)) if list.is_empty() => muted(ui, t("El servicio no devolvió modelos.")),
            Some(Ok(list)) => {
                egui::ScrollArea::vertical().id_salt("models").max_height(110.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for id in list {
                            if super::chip(ui, id, &e.conn.model == id).clicked() {
                                pick = Some(id.clone());
                            }
                        }
                    });
                });
            }
            Some(Err(err)) => {
                ui.label(RichText::new(err.as_str()).color(Color32::from_rgb(0xF4, 0x3F, 0x5E)));
            }
            None => {}
        }
        if let Some(m) = pick {
            e.conn.model = m;
        }
        if working {
            ui.horizontal(|ui| {
                super::mark::working(ui, 28.0, colors().accent);
                ui.add_space(4.0);
                muted(ui, t("Consultando el servicio…"));
            });
        }
        match &*self.test.lock().unwrap() {
            Some(Ok(a)) => {
                ui.label(RichText::new(format!("✓ {} {}", t("Funciona:"), ai::one_line(a, 120))).color(Color32::from_rgb(0x22, 0xC5, 0x5E)));
            }
            Some(Err(err)) => {
                ui.label(RichText::new(format!("✕ {err}")).color(Color32::from_rgb(0xF4, 0x3F, 0x5E)));
            }
            None => {}
        }
        ui.add_space(8.0);
        let mut action = None;
        ui.horizontal(|ui| {
            if primary(ui, &t("Guardar conexión")).clicked() {
                action = Some("save");
            }
            if ui.add_enabled(!working, egui::Button::new(t("Probar"))).on_hover_text(t("Envía un pedido de prueba, sin capturas")).clicked() {
                action = Some("test");
            }
            if ui.button(t("Cancelar")).clicked() {
                action = Some("cancel");
            }
        });
        let _ = Stroke::NONE;
        match action {
            Some("save") => {
                let e = self.edit.take().unwrap();
                if !e.key.trim().is_empty() {
                    keys::save(&e.conn.id, &e.conn.provider, &e.key);
                }
                match self.conns.iter_mut().find(|x| x.id == e.conn.id) {
                    Some(x) => *x = e.conn.clone(),
                    None => self.conns.push(e.conn.clone()),
                }
                if self.active.is_empty() || e.is_new {
                    self.active = e.conn.id.clone();
                }
                self.enabled = true;
                self.save();
            }
            Some("test") => {
                let e = self.edit.as_ref().unwrap();
                let (conn, key) = (e.conn.clone(), e.key.clone());
                self.spawn(self.test.clone(), move || {
                    let cancel = AtomicBool::new(false);
                    let client = Client::from_connection(&conn, Some(&key))?;
                    client.complete(&ai::AiRequest { system: "Reply with one short sentence.".into(), prompt: "Say hello and name the model you are.".into(), images: vec![], max_tokens: 1500 }, &cancel)
                });
            }
            Some("cancel") => self.edit = None,
            _ => {}
        }
    }
}

impl View for SettingsWin {
    fn kind(&self) -> &'static str {
        "ai-settings"
    }
    fn title(&self) -> String {
        format!("{} · BuzzPrint", t("Configurar IA"))
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        self.hwnd = win.hwnd as isize;
        let c = colors();
        panel().show(ctx, |ui| {
            header(ui, &t("Configurar IA"), &t("Elegí con qué servicio trabaja la IA. Nada se envía sin que lo pidas: cada función muestra antes qué manda y a quién. Las keys se guardan cifradas para tu usuario de Windows."));
            egui::ScrollArea::vertical().id_salt("ai_cfg").auto_shrink([false, false]).show(ui, |ui| {
                let mut changed = false;
                changed |= super::widgets::option_row(ui, &t("Usar IA"), &t("Muestra el menú IA y sus acciones en la biblioteca y el editor"), &mut self.enabled, false);
                changed |= super::widgets::option_row(ui, &t("Describir cada captura nueva"), &t("Título y resumen automáticos: envía cada captura nueva al servicio activo"), &mut self.auto_caption, true);
                if changed {
                    self.save();
                }
                ui.add_space(12.0);
                caption(ui, &t("Conexiones"));
                ui.add_space(4.0);
                let mut remove: Option<String> = None;
                let mut edit: Option<AiConnection> = None;
                let mut activate: Option<String> = None;
                if self.conns.is_empty() {
                    muted(ui, t("Todavía no hay ninguna conexión."));
                }
                for conn in &self.conns {
                    let on = conn.id == self.active;
                    egui::Frame::default().fill(if on { c.accent_weak } else { c.raised }).corner_radius(6).inner_margin(10).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let p = ai::provider(&conn.provider);
                            ui.label(RichText::new(p.letter).font(bold(15.0)).color(c.accent));
                            ui.vertical(|ui| {
                                ui.label(RichText::new(ai::display_name(conn)).color(c.ink).strong());
                                let has_key = !p.needs_key || keys::load(&conn.id, &conn.provider).is_some();
                                let state = if has_key { t(p.tag) } else { t("Falta la API key") };
                                ui.label(RichText::new(state).small().color(c.muted));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(t("Quitar")).clicked() {
                                    remove = Some(conn.id.clone());
                                }
                                if ui.button(t("Editar")).clicked() {
                                    edit = Some(conn.clone());
                                }
                                if !on && ui.button(t("Usar esta")).clicked() {
                                    activate = Some(conn.id.clone());
                                }
                                if on {
                                    ui.label(RichText::new(t("ACTIVA")).font(mono(10.5)).color(c.accent));
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
                if let Some(id) = activate {
                    self.active = id;
                    self.save();
                }
                if let Some(id) = remove {
                    keys::delete(&id);
                    self.conns.retain(|x| x.id != id);
                    if self.active == id {
                        self.active = self.conns.first().map(|x| x.id.clone()).unwrap_or_default();
                    }
                    self.save();
                }
                if let Some(conn) = edit {
                    self.begin_edit(conn, false);
                }
                if self.edit.is_none() && ui.button(format!("+ {}", t("Agregar conexión"))).clicked() {
                    self.begin_edit(AiConnection::default(), true);
                }
                ui.add_space(12.0);
                if self.edit.is_some() {
                    egui::Frame::default().stroke(Stroke::new(1.0, c.faint_border)).corner_radius(6).inner_margin(14).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.editor(ui);
                    });
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.edit.is_none() {
            win.close = true;
        }
    }
}

pub fn open_settings() {
    if let Some(h) = host::find("ai-settings") {
        host::activate(h);
        return;
    }
    let mut s = store::settings();
    let _ = ai::active(&mut s);
    let view = SettingsWin {
        enabled: s.ai_enabled,
        auto_caption: s.ai_auto_caption,
        conns: s.ai_connections.clone(),
        active: s.ai_active_id.clone(),
        edit: None,
        models: Arc::new(Mutex::new(None)),
        test: Arc::new(Mutex::new(None)),
        working: Arc::new(AtomicBool::new(false)),
        hwnd: 0,
    };
    host::open(Box::new(view), Options { width: 820.0, height: 720.0, min_width: 640.0, min_height: 480.0, ..Default::default() });
}
