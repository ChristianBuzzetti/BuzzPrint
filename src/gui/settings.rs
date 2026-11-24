//! Opciones (atajos, comportamiento, carpetas, idioma, inicio con Windows) y Personalizar (paleta, acento,
//! tipografía, forma de paneles y controles), en una ventana con dos pestañas.

use std::any::Any;

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Vec2};
use windows::ApplicationModel::{Activation::ActivationKind, AppInstance, StartupTask, StartupTaskState};
use windows::core::HSTRING;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};

use super::colorpick::{ColorPicker, Outcome};
use super::host::{self, Options, View, WinCtl};
use super::widgets::glyph as G;
use super::{appearance, bold, caption, colors, muted, set_appearance};
use crate::app::{post, Cmd};
use crate::i18n::t;
use crate::store;
use crate::theme::{self, alpha, palettes, ACCENTS};
use crate::win::wide;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "BuzzPrintRs";
const STARTUP_TASK: &str = "BuzzPrintStartup";

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Options,
    Personalize,
    /// Boberto Studio (el motor web, en WebView2 solo mientras esta pestaña se ve).
    Boberto,
}

pub struct SettingsView {
    tab: Tab,
    st: store::Settings,
    keys: [String; 5],
    key_error: String,
    startup: bool,
    status: String,
    move_files: bool,
    studio: Option<crate::boberto::Web>,
    /// Tamaño de letra mientras se arrastra el deslizador: se aplica al soltar (aplicarlo en cada cuadro
    /// cambia el tamaño de todo y la perilla salta bajo el mouse).
    size_drag: Option<f64>,
    /// Selector de color abierto ("Otro color…" del acento o del logo).
    picker: Option<(PickTarget, ColorPicker)>,
    /// Solapa elegida dentro de Personalizar (Colores, Interfaz, Logo, Boberto).
    ptab: usize,
    /// Paleta recién elegida: se ofrece usar también su acento y su logo.
    palette_offer: Option<String>,
    /// "No volver a preguntar" en ese aviso.
    palette_remember: bool,
}

/// Personalizar directo en Boberto Studio (clic en Boberto del encabezado).
pub fn open_studio() {
    open_tab(Tab::Boberto);
}

pub fn open(personalize: bool) {
    open_tab(if personalize { Tab::Personalize } else { Tab::Options });
}

fn open_tab(tab: Tab) {
    if let Some(h) = host::find("settings") {
        host::activate(h);
        host::with_view::<SettingsView, _>(h, |v| v.tab = tab);
        return;
    }
    let st = store::settings();
    let keys = [st.hotkey_capture.clone(), st.hotkey_full.clone(), st.hotkey_pick.clone(), st.hotkey_record.clone(), st.hotkey_capture_alt.clone()];
    let v = SettingsView { tab, st, keys, key_error: String::new(), startup: startup_enabled(), status: String::new(), move_files: true, studio: None, size_drag: None, picker: None, ptab: 0, palette_offer: None, palette_remember: false };
    host::open(Box::new(v), Options { width: if tab == Tab::Boberto { 1180.0 } else { 960.0 }, height: 760.0, min_width: 680.0, min_height: 520.0, ..Default::default() });
}


/// Consulta, activa o desactiva la tarea sin esperar una operación WinRT en el apartamento STA.
fn packaged_startup_state(enable: Option<bool>) -> windows::core::Result<StartupTaskState> {
    crate::win::ensure_mta();
    std::thread::spawn(move || {
        let task = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK))?.get()?;
        match (enable, task.State()?) {
            (Some(true), StartupTaskState::Disabled) => task.RequestEnableAsync()?.get(),
            (Some(false), StartupTaskState::Enabled) => {
                task.Disable()?;
                task.State()
            }
            (_, state) => Ok(state),
        }
    })
    .join()
    .expect("startup task thread")
}

fn task_enabled(state: StartupTaskState) -> bool {
    matches!(state, StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy)
}

pub fn launched_at_startup() -> bool {
    crate::win::is_packaged()
        && AppInstance::GetActivatedEventArgs().and_then(|args| args.Kind())
            .is_ok_and(|kind| kind == ActivationKind::StartupTask)
}

pub fn startup_enabled() -> bool {
    if crate::win::is_packaged() {
        return packaged_startup_state(None).is_ok_and(task_enabled);
    }
    let key = wide(RUN_KEY);
    let name = wide(RUN_NAME);
    let mut size = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) == 0 }
}

pub fn set_startup(on: bool) -> bool {
    if crate::win::is_packaged() {
        let state = packaged_startup_state(Some(on));
        let message = match &state {
            Ok(StartupTaskState::DisabledByUser) if on => {
                Some(t("Lo desactivaste desde Windows. Para volver a activarlo, abrí Configuración → Aplicaciones → Inicio y encendé BuzzPrint."))
            }
            Ok(StartupTaskState::DisabledByPolicy) if on => {
                Some(t("Una directiva de tu organización no permite que BuzzPrint arranque con Windows."))
            }
            Ok(StartupTaskState::EnabledByPolicy) if !on => {
                Some(t("Una directiva de tu organización requiere que BuzzPrint arranque con Windows."))
            }
            Err(err) => Some(format!("{} · {err}", t("No se pudo cambiar el inicio con Windows."))),
            _ => None,
        };
        if let Some(message) = message {
            post(Cmd::Run(Box::new(move || super::prompt::info(&t("Iniciar con Windows"), &message))));
        }
        return state.is_ok_and(|state| task_enabled(state) == on);
    }
    unsafe {
        let mut hk: HKEY = std::ptr::null_mut();
        if RegCreateKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, std::ptr::null(), &mut hk, std::ptr::null_mut()) != 0 {
            return false;
        }
        let name = wide(RUN_NAME);
        let ok = if on {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let v = wide(&format!("\"{exe}\" --tray"));
            RegSetValueExW(hk, name.as_ptr(), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32) == 0
        } else {
            RegDeleteValueW(hk, name.as_ptr()) == 0
        };
        RegCloseKey(hk);
        ok
    }
}

/// "Otro color…": aro con el círculo cromático y un + en el centro (el mismo en el acento y en el logo).
fn more_colors_button(ui: &mut egui::Ui, side: f32, radius: f32) -> egui::Response {
    let c = colors();
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    let p = ui.painter();
    let ctr = r.center();
    let n = 36;
    let inner = radius * 2.0 / 3.0;
    for i in 0..n {
        let (a0, a1) = (i as f32 / n as f32 * std::f32::consts::TAU, (i + 1) as f32 / n as f32 * std::f32::consts::TAU);
        let hue = egui::ecolor::Hsva::new(i as f32 / n as f32, 0.85, 1.0, 1.0);
        let pt = |a: f32, rad: f32| ctr + Vec2::new(a.cos(), a.sin()) * rad;
        p.add(egui::Shape::convex_polygon(vec![pt(a0, inner), pt(a0, radius), pt(a1 + 0.01, radius), pt(a1 + 0.01, inner)], Color32::from(hue), Stroke::NONE));
    }
    p.line_segment([ctr - Vec2::new(3.5, 0.0), ctr + Vec2::new(3.5, 0.0)], Stroke::new(1.6_f32, c.ink));
    p.line_segment([ctr - Vec2::new(0.0, 3.5), ctr + Vec2::new(0.0, 3.5)], Stroke::new(1.6_f32, c.ink));
    if resp.hovered() {
        p.circle_stroke(ctr, radius + 3.0, Stroke::new(1.0, alpha(c.ink, 0.4)));
    }
    resp.on_hover_text(t("Otro color…"))
}

/// Qué color se está eligiendo con "Otro color…".
#[derive(Clone, Copy)]
enum PickTarget {
    Accent,
    Head,
    Eyes,
    Tile,
}

impl SettingsView {
    /// Aplica el color elegido en el selector.
    fn apply_picked(&mut self, target: PickTarget, col: Color32) {
        let hex = theme::to_hex(col);
        let field = match target {
            PickTarget::Accent => {
                let mut a = appearance();
                a.accent_hex = Some(hex);
                set_appearance(a);
                return;
            }
            PickTarget::Head => &mut self.st.mark_head,
            PickTarget::Eyes => &mut self.st.mark_eye_color,
            PickTarget::Tile => &mut self.st.mark_tile,
        };
        *field = hex;
        self.save();
        host::wake_all();
        post(Cmd::Run(Box::new(crate::app::refresh_icons)));
    }

    /// Pone un logo completo (restablecer, o el de una paleta) y refresca los íconos.
    fn set_logo(&mut self, s: super::mark::Style) {
        self.st.mark_shape = s.mark_shape;
        self.st.mark_eyes = s.mark_eyes;
        self.st.mark_head = s.mark_head;
        self.st.mark_tile = s.mark_tile;
        self.st.mark_eye_color = s.mark_eye_color;
        self.save();
        host::wake_all();
        post(Cmd::Run(Box::new(crate::app::refresh_icons)));
    }

    /// El acento y el logo propios de la paleta `id`.
    fn apply_palette_extras(&mut self, id: &str, a: &mut crate::theme::Appearance) {
        a.accent_hex = None;
        self.set_logo(super::mark::for_palette(id));
    }

    fn save(&mut self) {
        let st = self.st.clone();
        store::with(|s| {
            s.settings = st;
            s.save_settings();
        });
    }

    /// Logo (Personalizar → Efectos y logo de .NET): forma, ojos, color de la cabeza y de la placa, con vista previa.
    fn logo_editor(&mut self, ui: &mut egui::Ui) {
        use super::mark::{self, Style, EYES, EYE_COLORS, HEADS, SHAPES, TILES};
        let c = colors();
        let eye_color = self.st.mark_eye_color.clone();
        let style = |sh: &str, ey: &str, hd: &str, tl: &str| Style { mark_shape: sh.into(), mark_eyes: ey.into(), mark_head: hd.into(), mark_tile: tl.into(), mark_eye_color: eye_color.clone() };
        let cur = style(&self.st.mark_shape, &self.st.mark_eyes, &self.st.mark_head, &self.st.mark_tile);
        let mut changed = false;
        // Restablecer: el logo de la paleta actual o el original de BuzzPrint.
        let theme_id = appearance().theme_id;
        let mut reset = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let pal = palettes().iter().find(|p| p.id == theme_id).map(|p| p.label.replace(" · ", " ")).unwrap_or_default();
            if super::widgets::bar_button(ui, super::widgets::icon_label(G::PALETTE, &crate::i18n::f("Logo de la paleta {0}", &[&pal]), 13.5, c.ink), false, &t("Forma, ojos y colores pensados para la paleta actual")).clicked() {
                reset = Some(mark::for_palette(&theme_id));
            }
            if super::widgets::bar_button(ui, super::widgets::icon_label('\u{E777}', &t("Restablecer el original"), 13.5, c.ink), false, &t("El logo de BuzzPrint de fábrica")).clicked() {
                reset = Some(mark::original());
            }
        });
        if let Some(s) = reset {
            self.set_logo(s);
            return;
        }
        ui.add_space(10.0);
        // Casilla de cada opción: fondo suave (el logo oscuro se lee sobre el tema oscuro), aro al elegir.
        let cell = |ui: &egui::Ui, r: egui::Rect, on: bool, hover: bool| {
            let p = ui.painter();
            p.rect_filled(r.shrink(1.0), CornerRadius::same(8), if on { alpha(c.accent, 0.16) } else { alpha(c.ink, if hover { 0.10 } else { 0.05 }) });
            if on {
                p.rect_stroke(r.shrink(1.0), CornerRadius::same(8), Stroke::new(2.0, c.accent), egui::StrokeKind::Inside);
            }
        };
        ui.horizontal(|ui| {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::click());
            mark::live_at(ui, r, &resp, c.accent, true, &cur);
            ui.vertical(|ui| {
                muted(ui, t("Forma"));
                ui.horizontal_wrapped(|ui| {
                    for s in SHAPES {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::click());
                        cell(ui, r, self.st.mark_shape == s, resp.hovered());
                        mark::draw_with(ui.painter(), r.shrink(5.0), c.accent, Vec2::ZERO, &style(s, &cur.mark_eyes, &cur.mark_head, &cur.mark_tile));
                        if resp.on_hover_text(s).clicked() {
                            self.st.mark_shape = s.into();
                            changed = true;
                        }
                    }
                });
                muted(ui, t("Ojos"));
                ui.horizontal_wrapped(|ui| {
                    for e in EYES {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::click());
                        cell(ui, r, self.st.mark_eyes == e, resp.hovered());
                        mark::draw_with(ui.painter(), r.shrink(5.0), c.accent, Vec2::ZERO, &style(&cur.mark_shape, e, &cur.mark_head, &cur.mark_tile));
                        if resp.on_hover_text(e).clicked() {
                            self.st.mark_eyes = e.into();
                            changed = true;
                        }
                    }
                });
            });
        });
        // Colores: círculos como los del acento, en tres filas alineadas (cabeza, ojos y placa).
        let swatch = |ui: &mut egui::Ui, value: &str, on: bool| -> bool {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
            let p = ui.painter();
            let ctr = r.center();
            match value {
                "auto" => {
                    // Mitad clara y mitad oscura: elige solo según la cabeza.
                    p.circle_filled(ctr, 10.0, Color32::from_rgb(0x0E, 0x10, 0x15));
                    let half: Vec<egui::Pos2> = (0..=16).map(|k| ctr + Vec2::angled(-std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * k as f32 / 16.0) * 10.0).collect();
                    p.add(egui::Shape::convex_polygon(half, Color32::from_rgb(0xF4, 0xF4, 0xF5), Stroke::NONE));
                }
                "none" => {
                    p.circle_stroke(ctr, 9.5, Stroke::new(1.0, alpha(c.ink, 0.35)));
                    p.line_segment([ctr + Vec2::new(-6.5, 6.5), ctr + Vec2::new(6.5, -6.5)], Stroke::new(1.5, c.muted));
                }
                v => {
                    p.circle_filled(ctr, 10.0, if v == "accent" { c.accent } else { theme::parse_hex(v) });
                }
            }
            if value != "none" {
                p.circle_stroke(ctr, 10.0, Stroke::new(1.0, alpha(c.ink, 0.2)));
            }
            if on {
                p.circle_stroke(ctr, 13.0, Stroke::new(2.0, c.accent));
            } else if resp.hovered() {
                p.circle_stroke(ctr, 13.0, Stroke::new(1.0, alpha(c.ink, 0.4)));
            }
            let tip = match value {
                "accent" => t("Color de acento"),
                "none" => t("Sin placa"),
                "auto" => t("Automático: claros en cabezas oscuras y oscuros en las claras"),
                h => h.to_string(),
            };
            resp.on_hover_text(tip).clicked()
        };
        // Después de cada fila: el color propio (si el elegido no está en la lista) y "Otro color…".
        // Devuelve el selector a abrir si se tocó "Otro color…".
        let extra = |ui: &mut egui::Ui, current: &str, list: &[&str], target: PickTarget| -> Option<(PickTarget, ColorPicker)> {
            if current.starts_with('#') && !list.iter().any(|v| v.eq_ignore_ascii_case(current)) {
                swatch(ui, current, true);
            }
            let resp = more_colors_button(ui, 28.0, 10.0);
            resp.clicked().then(|| {
                let start = if current.starts_with('#') { theme::parse_hex(current) } else { c.accent };
                (target, ColorPicker::new(start, resp.rect))
            })
        };
        let mut open = None;
        ui.add_space(6.0);
        egui::Grid::new("logo-colors").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
            ui.label(t("Cabeza"));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for h in HEADS {
                    if swatch(ui, h, self.st.mark_head.eq_ignore_ascii_case(h)) {
                        self.st.mark_head = h.into();
                        changed = true;
                    }
                }
                open = open.take().or(extra(ui, &self.st.mark_head, &HEADS, PickTarget::Head));
            });
            ui.end_row();
            ui.label(t("Color de ojos"));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let cur = if self.st.mark_eye_color.is_empty() { "auto".to_string() } else { self.st.mark_eye_color.clone() };
                for e in EYE_COLORS {
                    if swatch(ui, e, cur.eq_ignore_ascii_case(e)) {
                        self.st.mark_eye_color = e.into();
                        changed = true;
                    }
                }
                open = open.take().or(extra(ui, &cur, &EYE_COLORS, PickTarget::Eyes));
            });
            ui.end_row();
            ui.label(t("Placa"));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for tl in TILES {
                    if swatch(ui, tl, self.st.mark_tile.eq_ignore_ascii_case(tl)) {
                        self.st.mark_tile = tl.into();
                        changed = true;
                    }
                }
                open = open.take().or(extra(ui, &self.st.mark_tile, &TILES, PickTarget::Tile));
            });
            ui.end_row();
        });
        if open.is_some() {
            self.picker = open;
        }
        if changed {
            self.save();
            host::wake_all();
            crate::app::post(crate::app::Cmd::Run(Box::new(crate::app::refresh_icons)));
        }
    }

    fn options(&mut self, ui: &mut egui::Ui, hwnd: windows_sys::Win32::Foundation::HWND) {
        let c = colors();
        caption(ui, &t("Atajos de teclado"));
        egui::Grid::new("keys").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
            let labels = ["Capturar región o ventana", "Capturar pantalla completa", "Menú rápido", "Grabar video", "Atajo alternativo de captura"];
            for (i, l) in labels.iter().enumerate() {
                ui.label(t(l));
                ui.add(super::field(&mut self.keys[i]).desired_width(200.0).font(egui::TextStyle::Monospace));
                ui.end_row();
            }
        });
        muted(ui, t("Formato: Ctrl+Alt+V, Ctrl+Shift+X, PrintScreen, F9…"));
        if !self.key_error.is_empty() {
            ui.colored_label(c.error, &self.key_error);
        }
        if ui.button(t("Aplicar atajos")).clicked() {
            match self.keys.iter().find(|k| crate::app::parse_hotkey(k).is_none()) {
                Some(bad) => self.key_error = format!("{}: {bad}", t("Atajo no válido")),
                None => {
                    self.key_error.clear();
                    self.st.hotkey_capture = self.keys[0].clone();
                    self.st.hotkey_full = self.keys[1].clone();
                    self.st.hotkey_pick = self.keys[2].clone();
                    self.st.hotkey_record = self.keys[3].clone();
                    self.st.hotkey_capture_alt = self.keys[4].clone();
                    self.save();
                    post(Cmd::Run(Box::new(|| crate::app::register_hotkeys(crate::app::main_hwnd(), true))));
                    self.status = t("Atajos aplicados");
                }
            }
        }
        ui.add_space(12.0);

        caption(ui, &t("Al capturar"));
        let mut changed = false;
        changed |= ui.checkbox(&mut self.st.copy_after_capture, t("Copiar al portapapeles")).changed();
        changed |= ui.checkbox(&mut self.st.open_editor_after_capture, t("Abrir el editor")).changed();
        changed |= ui.checkbox(&mut self.st.edit_keep_original, t("Conservar la original al editar")).changed();
        changed |= ui.checkbox(&mut self.st.paste_on_pick, t("Menú rápido: pegar en la ventana activa")).changed();
        ui.horizontal(|ui| {
            ui.label(t("Capturas en el menú rápido"));
            changed |= ui.add(egui::DragValue::new(&mut self.st.quick_pick_count).range(1..=20)).changed();
        });
        ui.horizontal(|ui| {
            ui.label(t("Demora (segundos)"));
            changed |= ui.add(egui::DragValue::new(&mut self.st.delay_seconds).range(1..=30)).changed();
        });
        ui.add_space(12.0);

        caption(ui, &t("Grabación"));
        changed |= ui.checkbox(&mut self.st.record_system_audio, t("Audio del sistema")).changed();
        changed |= ui.checkbox(&mut self.st.record_microphone, t("Micrófono")).changed();
        changed |= ui.checkbox(&mut self.st.record_cursor, t("Mostrar el cursor")).changed();
        ui.horizontal(|ui| {
            ui.label(t("Cuadros por segundo"));
            changed |= ui.add(egui::DragValue::new(&mut self.st.record_fps).range(10..=60)).changed();
        });
        if changed {
            self.save();
        }
        ui.add_space(12.0);

        caption(ui, &t("Almacenamiento"));
        let (cap, rec) = store::with(|s| (s.capture_dir.clone(), s.record_dir.clone()));
        for (label, dir, recordings) in [(t("Capturas"), cap, false), (t("Grabaciones"), rec, true)] {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).strong());
                ui.label(RichText::new(dir.display().to_string()).font(super::mono(11.0)).color(c.muted));
            });
            ui.horizontal(|ui| {
                if ui.button(t("Cambiar…")).clicked() {
                    let mv = self.move_files;
                    let hwnd = hwnd as isize;
                    post(Cmd::Run(Box::new(move || {
                        if let Some(dir) = crate::dialogs::pick_folder(hwnd as _, &t("Elegí la carpeta")) {
                            let n = store::with(|s| s.change_folder(recordings, &dir, mv));
                            let msg = format!("{} · {n} {}", t("Carpeta cambiada"), t("archivos movidos"));
                            host::with_view::<SettingsView, _>(hwnd as _, |v| {
                                v.st = store::settings();
                                v.status = msg;
                            });
                            host::wake_all();
                        }
                    })));
                }
                if ui.button(t("Abrir")).clicked() {
                    crate::output::shell_open(&dir);
                }
            });
        }
        ui.checkbox(&mut self.move_files, t("Mover lo que ya tengo a la carpeta nueva"));
        ui.add_space(12.0);

        caption(ui, &t("Sistema"));
        if ui.checkbox(&mut self.startup, t("Iniciar con Windows")).changed() && !set_startup(self.startup) {
            self.startup = startup_enabled();
        }
        ui.horizontal(|ui| {
            ui.label(t("Idioma"));
            let mut lang = self.st.language.clone();
            egui::ComboBox::from_id_salt("lang").selected_text(egui::RichText::new(if lang == "es" { "Español" } else { "English" }).size(super::combo_size())).show_ui(ui, |ui| {
                super::menu_style(ui);
                ui.selectable_value(&mut lang, "en".to_string(), "English");
                ui.selectable_value(&mut lang, "es".to_string(), "Español");
            });
            if lang != self.st.language {
                self.st.language = lang;
                self.save();
                self.status = t("El idioma cambia al reiniciar BuzzPrint.");
            }
        });
        if ui.button(t("Reiniciar BuzzPrint")).clicked() {
            relaunch();
        }
    }

    /// Personalizar, en solapas: Colores (paleta y acento), Interfaz, Logo y Boberto.
    fn personalize(&mut self, ui: &mut egui::Ui) {
        let tabs = [
            (G::PALETTE, t("Colores")),
            (G::FONT, t("Interfaz")),
            (G::SPARKLE, t("Logo")),
            (G::ROBOT, "Boberto".to_string()),
        ];
        if let Some(i) = super::widgets::segmented(ui, &tabs, self.ptab) {
            self.ptab = i;
        }
        ui.add_space(14.0);
        egui::ScrollArea::vertical().id_salt(("st_personalize", self.ptab)).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width() - 10.0);
            let mut a = appearance();
            let before = a.clone();
            match self.ptab {
                0 => self.colors_tab(ui, &mut a),
                1 => self.interface_tab(ui, &mut a),
                2 => group(ui, &t("Logo"), &t("El logo de BuzzPrint en la barra, la bandeja y las ventanas: forma, ojos y colores."), |ui| self.logo_editor(ui)),
                _ => self.boberto_tab(ui, &mut a),
            }
            if a != before {
                set_appearance(a);
            }
        });
    }

    fn interface_tab(&mut self, ui: &mut egui::Ui, a: &mut crate::theme::Appearance) {
        let c = colors();
        group(ui, &t("Texto"), &t("Letra y tamaño de toda la interfaz."), |ui| {
            egui::Grid::new("iface").num_columns(2).spacing([24.0, 14.0]).min_col_width(140.0).show(ui, |ui| {
                ui.label(RichText::new(t("Fuente")).strong());
                let fonts = [(G::FONT, "Inter".to_string()), (G::FONT, "Segoe UI".to_string())];
                let cur = if a.ui_font == "inter" { 0 } else { 1 };
                if let Some(i) = super::widgets::segmented(ui, &fonts, cur) {
                    a.ui_font = if i == 0 { "inter" } else { "system" }.to_string();
                }
                ui.end_row();
                ui.label(RichText::new(t("Tamaño del texto")).strong());
                ui.horizontal(|ui| {
                    let shown = self.size_drag.unwrap_or(a.ui_size).round() as i32;
                    let (v, dragging) = super::widgets::int_slider(ui, shown, 12, 18, 220.0);
                    // Se aplica al soltar: aplicarlo en cada cuadro cambia el tamaño de todo y la perilla salta.
                    if dragging {
                        self.size_drag = Some(v as f64);
                    } else {
                        self.size_drag = None;
                        a.ui_size = v as f64;
                    }
                    ui.add_space(6.0);
                    egui::Frame::default()
                        .fill(c.canvas)
                        .stroke(Stroke::new(1.0, c.strong_border))
                        .corner_radius(super::widgets::radius())
                        .inner_margin(egui::Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("{v} pt")).font(super::mono(12.0)).color(c.ink));
                        });
                });
                ui.end_row();
            });
        });
        ui.add_space(12.0);
        group(ui, &t("Forma"), &t("Densidad y esquinas de paneles y controles."), |ui| {
            super::widgets::option_row(ui, &t("Compacto"), &t("Menos aire entre filas y controles más bajos"), &mut a.compact, false);
            super::widgets::option_row(ui, &t("Paneles redondeados"), &t("Esquinas suaves en tarjetas y paneles"), &mut a.rounded, false);
            super::widgets::option_row(ui, &t("Controles redondeados"), &t("Esquinas suaves en botones, campos e interruptores"), &mut a.rounded_controls, true);
        });
    }

    fn boberto_tab(&mut self, ui: &mut egui::Ui, a: &mut crate::theme::Appearance) {
        group(ui, "Boberto", &t("Boberto vive en la banda del encabezado, junto a los menús. Al hacerle clic se abre Boberto Studio."), |ui| {
            super::widgets::option_row(ui, &t("Mostrar a Boberto"), &t("La banda de la derecha del encabezado"), &mut a.boberto_visible, false);
            super::widgets::option_row(
                ui,
                &t("Animar a Boberto y el logo"),
                &t("Gestos en reposo, respiración, parpadeo y la vuelta del logo. Los gestos que elijas en Studio siempre se reproducen."),
                &mut a.boberto_motion,
                true,
            );
        });
        ui.add_space(12.0);
        group(ui, "Boberto Studio", &t("Fondos, marcos, gestos y vestuario de Boberto."), |ui| {
            let c = colors();
            if super::widgets::bar_button(ui, super::widgets::icon_label(G::ROBOT, &format!("{}  →", t("Abrir Boberto Studio")), 13.5, c.on_accent), true, "").clicked() {
                self.tab = Tab::Boberto;
            }
        });
    }

    fn colors_tab(&mut self, ui: &mut egui::Ui, a: &mut crate::theme::Appearance) {
        let c = colors();
        group(ui, &t("Paleta"), &t("Los colores de fondo, texto y superficies."), |ui| {
            self.palettes(ui, a);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Al cambiar de paleta")).strong());
                ui.add_space(8.0);
                let opts = [(G::HELP, t("Preguntar")), (G::PALETTE, t("Usar su acento y su logo")), (G::CHECK, t("Solo la paleta"))];
                let cur = match self.st.palette_extras.as_str() { "always" => 1, "never" => 2, _ => 0 };
                if let Some(i) = super::widgets::segmented(ui, &opts, cur) {
                    self.st.palette_extras = ["ask", "always", "never"][i].into();
                    self.save();
                }
            });
        });
        if let Some(id) = self.palette_offer.clone() {
            ui.add_space(10.0);
            let label = palettes().iter().find(|p| p.id == id).map(|p| p.label.replace(" · ", " ")).unwrap_or_default();
            let mut close = None;
            egui::Frame::default()
                .fill(super::theme_alpha(c.accent, 0.10))
                .stroke(Stroke::new(1.0, super::theme_alpha(c.accent, 0.5)))
                .corner_radius(super::widgets::panel_radius())
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        // El logo que vendría con la paleta.
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
                        super::mark::draw_with(ui.painter(), r, c.accent, Vec2::ZERO, &super::mark::for_palette(&id));
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(crate::i18n::f("¿Usar también el acento y el logo de {0}?", &[&label])).strong().color(c.ink));
                            ui.checkbox(&mut self.palette_remember, t("No volver a preguntar"));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if super::widgets::confirm_button(ui, &t("Aplicar")).clicked() {
                                close = Some(true);
                            }
                            ui.add_space(6.0);
                            if super::widgets::bar_button(ui, super::widgets::icon_label(G::PALETTE, &t("Solo la paleta"), 13.5, c.ink), false, "").clicked() {
                                close = Some(false);
                            }
                        });
                    });
                });
            if let Some(apply) = close {
                if apply {
                    self.apply_palette_extras(&id, a);
                }
                if self.palette_remember {
                    self.st.palette_extras = if apply { "always" } else { "never" }.into();
                    self.save();
                }
                self.palette_offer = None;
                self.palette_remember = false;
            }
        }
        ui.add_space(12.0);
        group(ui, &t("Acento"), &t("El color de los botones principales, las selecciones y los detalles."), |ui| {
            let pals = palettes();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                // Todos los colores son círculos del mismo tamaño; el elegido lleva un aro y una tilde.
                let dot = |ui: &mut egui::Ui, col: Color32, on: bool| -> egui::Response {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                    let p = ui.painter();
                    let ctr = r.center();
                    p.circle_filled(ctr, 12.0, col);
                    p.circle_stroke(ctr, 12.0, Stroke::new(1.0, alpha(c.ink, 0.18)));
                    if on {
                        p.circle_stroke(ctr, 15.5, Stroke::new(2.0, c.ink));
                        let lum = 0.2126 * col.r() as f32 + 0.7152 * col.g() as f32 + 0.0722 * col.b() as f32;
                        let ink = if lum > 150.0 { Color32::from_rgb(0x10, 0x10, 0x14) } else { Color32::WHITE };
                        p.line_segment([ctr + Vec2::new(-4.5, 0.2), ctr + Vec2::new(-1.4, 3.3)], Stroke::new(2.0, ink));
                        p.line_segment([ctr + Vec2::new(-1.4, 3.3), ctr + Vec2::new(4.6, -3.2)], Stroke::new(2.0, ink));
                    } else if resp.hovered() {
                        p.circle_stroke(ctr, 15.5, Stroke::new(1.0, alpha(c.ink, 0.4)));
                    }
                    resp
                };
                let mut known = false;
                for (name, hex) in ACCENTS {
                    let col = hex.map(theme::parse_hex).unwrap_or_else(|| {
                        pals.iter().find(|p| p.id == a.theme_id).and_then(|p| p.colors.get("Accent")).map(|s| theme::parse_hex(s)).unwrap_or(Color32::GRAY)
                    });
                    let on = match (a.accent_hex.as_deref(), hex) {
                        (None, None) => true,
                        (Some(x), Some(y)) => x.eq_ignore_ascii_case(y),
                        _ => false,
                    };
                    known |= on;
                    if dot(ui, col, on).on_hover_text(t(name)).clicked() {
                        a.accent_hex = hex.map(|s| s.to_string());
                    }
                }
                // Un color propio (elegido con el selector) se muestra como un círculo más, ya elegido.
                if !known {
                    if let Some(h) = a.accent_hex.clone() {
                        dot(ui, theme::parse_hex(&h), true).on_hover_text(format!("{} · {h}", t("Color propio")));
                    }
                }
                // "Otro color…": abre el selector de color.
                let resp = more_colors_button(ui, 34.0, 12.0);
                if resp.clicked() {
                    self.picker = Some((PickTarget::Accent, ColorPicker::new(c.accent, resp.rect)));
                }
            });
        });
    }

    fn palettes(&mut self, ui: &mut egui::Ui, a: &mut crate::theme::Appearance) {
        let c = colors();
        let pals = palettes();
        let w = 150.0;
        let cols = ((ui.available_width() + 8.0) / (w + 8.0)).floor().max(1.0) as usize;
        egui::Grid::new("pals").spacing([8.0, 8.0]).show(ui, |ui| {
            for (i, p) in pals.iter().enumerate() {
                let get = |k: &str| p.colors.get(k).map(|s| theme::parse_hex(s)).unwrap_or(Color32::GRAY);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 54.0), Sense::click());
                let on = a.theme_id == p.id;
                let painter = ui.painter();
                painter.rect_filled(r, CornerRadius::same(8), get("Canvas"));
                painter.rect_filled(egui::Rect::from_min_size(r.min + Vec2::new(8.0, 8.0), Vec2::new(w - 16.0, 18.0)), CornerRadius::same(4), get("Surface"));
                painter.rect_filled(egui::Rect::from_min_size(r.min + Vec2::new(12.0, 13.0), Vec2::new(28.0, 8.0)), CornerRadius::same(3), get("Accent"));
                painter.text(r.left_bottom() + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM, p.label.replace(" · ", " "), super::prop(11.5), get("Ink"));
                painter.rect_stroke(r, CornerRadius::same(8), Stroke::new(if on { 2.5 } else { 1.0 }, if on { c.accent } else { alpha(c.ink, 0.15) }), egui::StrokeKind::Inside);
                if resp.clicked() && a.theme_id != p.id {
                    a.theme_id = p.id.clone();
                    match self.st.palette_extras.as_str() {
                        "always" => self.apply_palette_extras(&p.id, a),
                        "never" => {}
                        _ => self.palette_offer = Some(p.id.clone()),
                    }
                }
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
    }
}

/// Grupo de ajustes: tarjeta con título, una línea de ayuda y el contenido.
fn group(ui: &mut egui::Ui, title: &str, hint: &str, add: impl FnOnce(&mut egui::Ui)) {
    let c = colors();
    egui::Frame::default()
        .fill(c.header)
        .stroke(Stroke::new(1.0, c.faint_border))
        .corner_radius(super::widgets::panel_radius())
        .inner_margin(egui::Margin { left: 20, right: 20, top: 16, bottom: 18 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            caption(ui, title);
            if !hint.is_empty() {
                ui.add_space(2.0);
                muted(ui, hint);
            }
            ui.add_space(12.0);
            add(ui);
        });
}

fn relaunch() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--relaunch").spawn();
    }
    post(Cmd::Quit);
}

impl SettingsView {
    /// Boberto Studio: el WebView ocupa toda el área; mientras carga (o si falta WebView2) se ve un aviso.
    fn studio(&mut self, ui: &mut egui::Ui, win: &mut WinCtl) {
        let c = colors();
        let area = ui.max_rect();
        if self.studio.is_none() {
            self.studio = Some(crate::boberto::Web::new(win.hwnd, crate::boberto::Page::Studio, c.canvas, false));
        }
        let web = self.studio.as_ref().unwrap();
        web.take_events();
        let ready = web.ready();
        if !ready {
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(area.shrink(40.0)), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(area.height() * 0.22);
                    super::mark::live(ui, 84.0, c.accent, true);
                    ui.add_space(12.0);
                    match web.failed() {
                        Some(e) => {
                            ui.label(RichText::new(t("Boberto Studio no está disponible")).font(bold(16.0)));
                            muted(ui, e);
                            ui.add_space(8.0);
                            muted(ui, t("Las capturas, la biblioteca y el editor siguen disponibles. Instalá WebView2 desde Ayuda → Revisar componentes."));
                        }
                        None => {
                            ui.label(RichText::new(t("Abriendo Boberto Studio…")).color(c.muted));
                        }
                    }
                });
            });
        }
        let p = win.ppp;
        web.place(Some(((area.min.x * p).round() as i32, (area.min.y * p).round() as i32, (area.max.x * p).round() as i32, (area.max.y * p).round() as i32)));
    }
}

impl View for SettingsView {
    fn kind(&self) -> &'static str {
        "settings"
    }
    fn title(&self) -> String {
        let name = match self.tab {
            Tab::Options => t("Opciones"),
            Tab::Personalize => t("Personalizar"),
            Tab::Boberto => t("Boberto Studio"),
        };
        format!("{name} · BuzzPrint")
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn ui(&mut self, ctx: &egui::Context, win: &mut WinCtl) {
        let c = colors();
        egui::TopBottomPanel::top("tabs")
            .frame(egui::Frame::default().fill(c.header).inner_margin(egui::Margin::symmetric(16, 10)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    super::brand(ui, 30.0, 16.0);
                    ui.add_space(18.0);
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (tab, gl, label) in [(Tab::Options, G::SETTINGS, t("Opciones")), (Tab::Personalize, G::PALETTE, t("Personalizar")), (Tab::Boberto, G::ROBOT, t("Boberto Studio"))] {
                        if super::widgets::top_tab(ui, gl, &label, self.tab == tab).clicked() {
                            self.tab = tab;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Mismo alto que las pestañas (34 px), con ícono: antes quedaba más chico que el resto.
                        if super::widgets::confirm_button(ui, &t("Listo")).clicked() {
                            win.close = true;
                        }
                        ui.label(RichText::new(&self.status).font(super::mono(11.0)).color(c.muted));
                    });
                });
            });
        if self.tab == Tab::Boberto {
            egui::CentralPanel::default().frame(egui::Frame::default().fill(c.canvas)).show(ctx, |ui| self.studio(ui, win));
        } else {
            // Fuera de su pestaña, Studio se cierra (y con él WebView2, si no hay otro Boberto a la vista).
            self.studio = None;
            egui::CentralPanel::default()
                .frame(egui::Frame::default().fill(c.surface).inner_margin(egui::Margin::same(18)))
                .show(ctx, |ui| {
                    if self.tab == Tab::Options {
                        egui::ScrollArea::vertical().id_salt("st_body").auto_shrink([false, false]).show(ui, |ui| self.options(ui, win.hwnd));
                    } else {
                        self.personalize(ui);
                    }
                });
        }
        if let Some((target, picker)) = self.picker.as_mut() {
            let target = *target;
            match picker.show(ctx) {
                Outcome::Open => {}
                Outcome::Cancel => self.picker = None,
                Outcome::Apply(col) => {
                    self.picker = None;
                    self.apply_picked(target, col);
                }
            }
        }
        win.title = Some(self.title());
    }
}
