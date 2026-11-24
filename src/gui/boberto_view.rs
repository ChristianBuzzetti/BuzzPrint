//! Renderizador nativo de Boberto en egui para el Dock del encabezado y el Stage (biblioteca vacía).
//!
//! Usa el atlas 4×4 de poses horneado por Boberto Studio (`boberto-sheet.png` + `boberto-look.json`, con
//! respaldo a `boberto-avatar.png` o al `default-sheet.png` empaquetado), los 42 fondos `.jpg` decodificados
//! vía GDI+ y los 19 marcos de `boberto-estudio.css` dibujados con mallas `egui::Mesh`.
//! Cero procesos WebView2 en Dock y Stage; WebView2 solo se abre en la pestaña Boberto Studio.

use std::f32::consts::{PI, TAU};
use std::io::Cursor;
use std::time::Duration;

use egui::epaint::Vertex;
use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Shape, Stroke, TextureHandle, TextureOptions, Vec2};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

use crate::boberto::{self, Look, SHEET_COLS, SHEET_H, SHEET_ROWS, SHEET_W};
use crate::i18n::t;
use crate::theme::{self, alpha, mix, Colors};

use super::host;

// ------------------------------------------------------------------ gestos y partículas

#[derive(Clone, Copy, PartialEq, Eq)]
enum GestureKind {
    Wave,
    Celebrate,
    Dance,
    Think,
    Coffee,
    Happy,
}

impl GestureKind {
    const ALL: [GestureKind; 6] = [
        GestureKind::Wave,
        GestureKind::Celebrate,
        GestureKind::Dance,
        GestureKind::Think,
        GestureKind::Coffee,
        GestureKind::Happy,
    ];

    fn duration(self) -> f64 {
        match self {
            GestureKind::Wave => 2.2,
            GestureKind::Celebrate => 2.1,
            GestureKind::Dance => 2.6,
            GestureKind::Think => 2.5,
            GestureKind::Coffee => 2.8,
            GestureKind::Happy => 1.6,
        }
    }

    fn label(self) -> String {
        t(match self {
            GestureKind::Wave => "Saluda",
            GestureKind::Celebrate => "Festeja",
            GestureKind::Dance => "Baila",
            GestureKind::Think => "Piensa",
            GestureKind::Coffee => "Café",
            GestureKind::Happy => "Feliz",
        })
    }
}

#[derive(Clone, Copy)]
enum ParticleShape {
    Sparkle,
    Star,
    Note,
    Puff,
    Heart,
    Dot,
}

#[derive(Clone)]
struct Particle {
    /// Posición relativa al centro del personaje (en fracción del alto del personaje).
    ox: f32,
    oy: f32,
    vx: f32,
    vy: f32,
    born: f64,
    dur: f64,
    size: f32,
    shape: ParticleShape,
    color: Color32,
}

#[derive(Clone, Copy, Default)]
struct PoseXform {
    dx: f32,
    dy: f32,
    sx: f32,
    sy: f32,
    rot: f32,
    /// Hacia dónde mira (-1..1 en cada eje): la cabeza se inclina hacia ahí y el cuerpo la sigue, en 2.5D.
    tx: f32,
    ty: f32,
}

impl PoseXform {
    fn identity() -> Self {
        Self { dx: 0.0, dy: 0.0, sx: 1.0, sy: 1.0, rot: 0.0, tx: 0.0, ty: 0.0 }
    }
}

/// Poses de mirada (entre estas se funde en lugar de saltar).
fn is_gaze_pose(p: &str) -> bool {
    matches!(p, "base" | "lookL" | "lookR" | "lookU")
}

struct AnimState {
    seed: u32,
    blink_start: f64,
    next_blink: f64,
    gesture: Option<(GestureKind, f64, u32)>,
    next_gesture: f64,
    last_emit_step: u32,
    gaze: &'static str,
    lean_x: f32,
    /// Mirada suavizada (-1..1): sigue al cursor en las dos direcciones.
    gx: f32,
    gy: f32,
    last_now: f64,
    last_cursor: Option<Pos2>,
    still_since: f64,
    /// Pose anterior y cuándo se cambió (fundido de 140 ms entre poses de mirada).
    fade_from: &'static str,
    fade_t0: f64,
    was_hovered: bool,
    greeted: bool,
    mount_time: Option<f64>,
    particles: Vec<Particle>,
}

impl AnimState {
    fn new(seed_salt: u32) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(1337);
        Self {
            seed: (nanos ^ seed_salt.wrapping_mul(2654435761)) | 1,
            blink_start: -1.0,
            next_blink: -1.0,
            gesture: None,
            next_gesture: -1.0,
            last_emit_step: 0,
            gaze: "base",
            lean_x: 0.0,
            gx: 0.0,
            gy: 0.0,
            last_now: -1.0,
            last_cursor: None,
            still_since: 0.0,
            fade_from: "base",
            fade_t0: -1.0,
            was_hovered: false,
            greeted: false,
            mount_time: None,
            particles: Vec::new(),
        }
    }

    fn rand(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f64) / (u32::MAX as f64)
    }

    fn pick_gesture(&mut self) -> GestureKind {
        let i = (self.rand() * GestureKind::ALL.len() as f64) as usize;
        GestureKind::ALL[i.min(GestureKind::ALL.len() - 1)]
    }

    fn start_gesture(&mut self, kind: GestureKind, now: f64) {
        self.gesture = Some((kind, now, 0));
        self.last_emit_step = 0;
        self.blink_start = -1.0;
    }

    fn step(
        &mut self,
        now: f64,
        motion: bool,
        is_stage: bool,
        hovered: bool,
        clicked: bool,
        cursor: Option<Pos2>,
        char_rect: Rect,
        accent: Color32,
    ) -> (&'static str, PoseXform, bool) {
        if !motion {
            self.gesture = None;
            self.particles.clear();
            self.lean_x = 0.0;
            return ("base", PoseXform::identity(), false);
        }

        if self.mount_time.is_none() {
            self.mount_time = Some(now);
            self.next_blink = now + 1.1 + self.rand() * 1.1;
            self.next_gesture = now + if is_stage { 6.5 } else { 9.0 + self.rand() * 7.0 };
        }

        // En el Stage saluda medio segundo después de aparecer (como stage.js).
        if is_stage && !self.greeted {
            if let Some(mt) = self.mount_time {
                if now - mt >= 0.45 {
                    self.greeted = true;
                    self.start_gesture(GestureKind::Wave, now);
                }
            }
        }

        // Clic en el Stage dispara un gesto al azar.
        if is_stage && clicked {
            let g = self.pick_gesture();
            self.start_gesture(g, now);
            self.next_gesture = now + 6.5;
        }

        // Al salir del hover en el dock hace un parpadeo corto (como onDockLeave).
        if !is_stage && self.was_hovered && !hovered && self.gesture.is_none() {
            self.blink_start = now;
        }
        self.was_hovered = hovered;

        // Mirada hacia el cursor en 2D: un vector continuo (arriba, abajo, diagonales) que se suaviza con un
        // resorte; de él salen la pose (base / lookL / lookR / lookU, con histéresis para no titilar) y la
        // inclinación 2.5D de la cabeza, que es la que da la fluidez. Si el mouse queda quieto unos segundos,
        // vuelve despacio a mirar al frente.
        let dt = if self.last_now < 0.0 { 0.0 } else { (now - self.last_now).clamp(0.0, 0.1) as f32 };
        self.last_now = now;
        let (mut tx, mut ty) = (0.0f32, 0.0f32);
        if let Some(pt) = cursor {
            if self.last_cursor.map(|l| (l - pt).length() > 0.5).unwrap_or(true) {
                self.still_since = now;
            }
            self.last_cursor = Some(pt);
            let head = Pos2::new(char_rect.center().x, char_rect.min.y + char_rect.height() * 0.38);
            let d = pt - head;
            let len = d.length();
            if len > 1.0 {
                let reach = char_rect.height() * if is_stage { 2.4 } else { 2.0 };
                let k = (len / reach).min(1.0);
                let k = k * (2.0 - k); // sale rápido y se asienta en el borde
                tx = d.x / len * k;
                ty = d.y / len * k;
            }
            if now - self.still_since > 6.0 {
                let calm = (1.0 - ((now - self.still_since - 6.0) / 1.5) as f32).clamp(0.25, 1.0);
                tx *= calm;
                ty *= calm;
            }
        }
        let k = 1.0 - (-dt * 8.0).exp();
        self.gx += (tx - self.gx) * k;
        self.gy += (ty - self.gy) * k;
        let gaze_moving = (tx - self.gx).abs() > 0.004 || (ty - self.gy).abs() > 0.004;
        let ax = self.gx.abs();
        let side = matches!(self.gaze, "lookL" | "lookR");
        let target_gaze = if self.gy < -0.5 && ax < 0.55 {
            "lookU"
        } else if ax > if side { 0.2 } else { 0.32 } {
            if self.gx > 0.0 {
                "lookR"
            } else {
                "lookL"
            }
        } else if hovered && !is_stage {
            "lookU"
        } else {
            "base"
        };
        if target_gaze != self.gaze {
            self.fade_from = self.gaze;
            self.fade_t0 = now;
        }
        self.gaze = target_gaze;
        let target_lean = self.gx;
        let lean_diff = target_lean - self.lean_x;
        let lean_moving = lean_diff.abs() > 0.01 || gaze_moving;
        self.lean_x = if lean_diff.abs() > 0.01 { self.lean_x + lean_diff * 0.22 } else { target_lean };

        // Gesto periódico en reposo.
        if self.gesture.is_none() && !hovered && now >= self.next_gesture {
            let g = self.pick_gesture();
            self.start_gesture(g, now);
            self.next_gesture = now + if is_stage { 6.5 } else { 9.0 + self.rand() * 7.0 };
        }

        // Limpia partículas vencidas.
        self.particles.retain(|pt| now - pt.born < pt.dur);

        // Si hay un gesto activo, evalúa pose + transformación + partículas.
        if let Some((kind, start, _)) = self.gesture {
            let elapsed = now - start;
            let dur = kind.duration();
            if elapsed >= dur {
                self.gesture = None;
            } else {
                let (pose, xf) = self.eval_gesture(kind, elapsed, dur, now, accent);
                return (pose, xf, true);
            }
        }

        // Parpadeo (cada 1.0–2.2 s, 75 ms cerrado, como startBlinkTimer en boberto.js).
        let mut blinking = false;
        if !hovered && self.blink_start < 0.0 && now >= self.next_blink {
            self.blink_start = now;
        }
        if self.blink_start >= 0.0 {
            if now - self.blink_start < 0.085 {
                blinking = true;
            } else {
                self.blink_start = -1.0;
                self.next_blink = now + 1.2 + self.rand() * 1.4;
            }
        }

        let pose = if blinking { "blink" } else { self.gaze };
        // Respiración idle: plt-breathe (3.2 s ease-in-out).
        let ph = ((now * TAU as f64 / 3.2).sin() * 0.5 + 0.5) as f32;
        let xf = PoseXform {
            dx: self.lean_x * 0.022,
            dy: -ph * 0.015,
            sx: 1.0 - 0.015 * ph,
            sy: 1.0 + 0.035 * ph,
            rot: self.lean_x * 0.03,
            tx: self.gx,
            ty: self.gy,
        };
        let fast = blinking || lean_moving || !self.particles.is_empty() || (self.fade_t0 >= 0.0 && now - self.fade_t0 < 0.2);
        (pose, xf, fast)
    }

    fn emit(&mut self, now: f64, ox: f32, oy: f32, vx: f32, vy: f32, dur: f64, size: f32, shape: ParticleShape, color: Color32) {
        if self.particles.len() >= 14 {
            return;
        }
        self.particles.push(Particle { ox, oy, vx, vy, born: now, dur, size, shape, color });
    }

    fn eval_gesture(&mut self, kind: GestureKind, t: f64, dur: f64, now: f64, accent: Color32) -> (&'static str, PoseXform) {
        let p = (t / dur).clamp(0.0, 1.0) as f32;
        let step = (t / 0.35) as u32 + 1;
        let gold = Color32::from_rgb(0xFF, 0xD1, 0x66);
        let cyan = Color32::from_rgb(0x7D, 0xF9, 0xFF);
        let pink = Color32::from_rgb(0xFF, 0x6B, 0x9D);
        let steam = Color32::from_rgb(0xE8, 0xEE, 0xF7);

        match kind {
            GestureKind::Wave => {
                // Saludo: alterna waveUp / waveDown + guiño al final; vaivén plt-saludo (0.66 s).
                let pose = if p > 0.84 {
                    "wink"
                } else if ((t / 0.24) as u32) % 2 == 0 {
                    "waveUp"
                } else {
                    "waveDown"
                };
                if step != self.last_emit_step && step <= 3 {
                    self.last_emit_step = step;
                    self.emit(now, 0.30, -0.32, 0.10, -0.22, 0.85, 4.2, ParticleShape::Sparkle, accent);
                }
                let s = (t as f32 * TAU / 0.66).sin();
                (pose, PoseXform { dx: 0.0, dy: -0.006 * s.abs(), sx: 1.0, sy: 1.0, rot: s * 0.045, tx: 0.0, ty: 0.0 })
            }
            GestureKind::Celebrate => {
                // Festejo: salto elástico + puños arriba + estrellas.
                let pose = if p < 0.35 || (p > 0.58 && p < 0.78) { "festejaSalto" } else { "festejaPuno" };
                if step != self.last_emit_step && step <= 4 {
                    self.last_emit_step = step;
                    let side = if step % 2 == 0 { 1.0 } else { -1.0 };
                    self.emit(now, side * 0.24, -0.35, side * 0.18, -0.28, 0.95, 4.6, ParticleShape::Star, if step % 2 == 0 { gold } else { accent });
                }
                let hop = (t as f32 * PI / 0.36).sin().abs();
                (pose, PoseXform { dx: 0.0, dy: -0.055 * hop, sx: 1.0 - 0.05 * hop, sy: 1.0 + 0.06 * hop, rot: 0.0, tx: 0.0, ty: 0.0 })
            }
            GestureKind::Dance => {
                // Baile: danceR / danceL al compás de 0.56 s (plt-baila) + notas musicales.
                let pose = if ((t / 0.28) as u32) % 2 == 0 { "danceR" } else { "danceL" };
                if step != self.last_emit_step && step <= 5 {
                    self.last_emit_step = step;
                    let side = if step % 2 == 0 { 1.0 } else { -1.0 };
                    self.emit(now, side * 0.26, -0.28, side * 0.14, -0.24, 1.0, 4.5, ParticleShape::Note, if step % 2 == 0 { cyan } else { accent });
                }
                let cyc = (t as f32 * TAU / 0.56).sin();
                let bounce = (t as f32 * TAU / 0.28).sin().abs();
                (pose, PoseXform { dx: cyc * 0.025, dy: -0.02 * bounce, sx: 1.0, sy: 1.0 + 0.025 * bounce, rot: cyc * 0.075, tx: 0.0, ty: 0.0 })
            }
            GestureKind::Think => {
                // Piensa: cabeza ladeada suavemente (plt-piensa 1.4 s) + burbujitas de idea.
                let pose = if p > 0.78 { "happy" } else { "think" };
                if step != self.last_emit_step && step <= 3 {
                    self.last_emit_step = step;
                    self.emit(now, 0.22 + (step as f32) * 0.04, -0.32 - (step as f32) * 0.05, 0.04, -0.14, 1.0, 2.5 + step as f32, ParticleShape::Dot, accent);
                }
                let s = (t as f32 * TAU / 1.4).sin() * 0.5 + 0.5;
                (pose, PoseXform { dx: 0.0, dy: -0.005 * s, sx: 1.0, sy: 1.0, rot: -0.04 * s, tx: 0.0, ty: 0.0 })
            }
            GestureKind::Coffee => {
                // Café: coffeeHold → coffeeSip → happy + vapor tibio (plt-cafecito 1.5 s).
                let pose = if p < 0.34 {
                    "coffeeHold"
                } else if p < 0.76 {
                    "coffeeSip"
                } else {
                    "happy"
                };
                if step != self.last_emit_step && step <= 4 {
                    self.last_emit_step = step;
                    self.emit(now, 0.18, -0.16, 0.03, -0.22, 1.05, 4.0, ParticleShape::Puff, steam);
                }
                let s = (t as f32 * TAU / 1.5).sin() * 0.5 + 0.5;
                (pose, PoseXform { dx: 0.0, dy: -0.008 * s, sx: 1.0, sy: 1.0, rot: -0.028 * s, tx: 0.0, ty: 0.0 })
            }
            GestureKind::Happy => {
                // Feliz: salto breve + corazones/chispas.
                let pose = if p > 0.72 { "wink" } else { "happy" };
                if step != self.last_emit_step && step <= 2 {
                    self.last_emit_step = step;
                    let side = if step == 1 { -1.0 } else { 1.0 };
                    self.emit(now, side * 0.22, -0.30, side * 0.10, -0.24, 0.95, 4.4, ParticleShape::Heart, pink);
                }
                let hop = (p * PI).sin();
                (pose, PoseXform { dx: 0.0, dy: -0.035 * hop, sx: 1.0 - 0.03 * hop, sy: 1.0 + 0.045 * hop, rot: 0.0, tx: 0.0, ty: 0.0 })
            }
        }
    }

    /// Fundido entre poses de mirada: la pose anterior y cuánto le queda (1 = recién cambió, 0 = terminado).
    fn fade(&self, now: f64, pose: &str) -> Option<(&'static str, f32)> {
        if self.fade_t0 < 0.0 || self.gesture.is_some() || !is_gaze_pose(pose) || !is_gaze_pose(self.fade_from) || self.fade_from == pose {
            return None;
        }
        let u = ((now - self.fade_t0) / 0.14) as f32;
        (u < 1.0).then_some((self.fade_from, 1.0 - u))
    }

    fn active_gesture_label(&self) -> Option<String> {
        self.gesture.map(|(k, _, _)| k.label())
    }
}

// ------------------------------------------------------------------ Vista principal

pub struct BobertoView {
    look: Look,
    loaded_rev: u64,
    checked_stale: bool,
    sheet_tex: Option<TextureHandle>,
    is_atlas: bool,
    dock_anim: AnimState,
    stage_anim: AnimState,
}

impl Default for BobertoView {
    fn default() -> Self {
        Self::new()
    }
}

impl BobertoView {
    pub fn new() -> Self {
        Self {
            look: Look::default(),
            loaded_rev: u64::MAX,
            checked_stale: false,
            sheet_tex: None,
            is_atlas: true,
            dock_anim: AnimState::new(17),
            stage_anim: AnimState::new(53),
        }
    }

    /// Libera las texturas GPU del atlas y del fondo (para minimizar / Parte 2).
    /// Se vuelven a cargar solas en el próximo cuadro visible.
    pub fn drop_textures(&mut self) {
        self.sheet_tex = None;
    }

    /// Asegura que el aspecto, el atlas y el fondo estén al día con `boberto::look_revision()`.
    fn sync(&mut self, ctx: &egui::Context, hwnd: HWND) {
        if !self.checked_stale {
            self.checked_stale = true;
            boberto::trigger_sync_if_stale(hwnd);
        }
        let rev = boberto::look_revision();
        let rev_changed = rev != self.loaded_rev;
        if rev_changed {
            self.look = boberto::load_look();
            self.loaded_rev = rev;
            self.sheet_tex = None;
        }
        if self.sheet_tex.is_none() {
            if let Some(data) = boberto::load_sheet_png() {
                if let Some(img) = decode_png_bytes(&data.png) {
                    self.is_atlas = data.is_atlas;
                    self.sheet_tex = Some(ctx.load_texture("boberto-sheet", img, TextureOptions::LINEAR));
                }
            }
        }
    }

    fn pose_uv(&self, pose: &str) -> Rect {
        if !self.is_atlas {
            return Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        }
        let idx = boberto::pose_index(pose).unwrap_or(0);
        let col = (idx % SHEET_COLS) as f32;
        let row = (idx / SHEET_COLS) as f32;
        let cw = 1.0 / SHEET_COLS as f32;
        let ch = 1.0 / SHEET_ROWS as f32;
        let eps_u = 0.5 / SHEET_W as f32;
        let eps_v = 0.5 / SHEET_H as f32;
        Rect::from_min_max(
            Pos2::new(col * cw + eps_u, row * ch + eps_v),
            Pos2::new((col + 1.0) * cw - eps_u, (row + 1.0) * ch - eps_v),
        )
    }

    /// Banda 16:9 de Boberto a la derecha del encabezado (~220×124).
    /// Devuelve `true` si el usuario hizo clic para abrir Boberto Studio.
    pub fn dock(&mut self, ui: &mut egui::Ui, rect: Rect, resp: &egui::Response, hwnd: HWND) -> bool {
        let ctx = ui.ctx();
        self.sync(ctx, hwnd);
        let c = super::colors();
        let motion = super::appearance().boberto_motion;
        let minimized = unsafe { IsIconic(hwnd) != 0 };
        let live_motion = motion && !minimized;
        let now = ctx.input(|i| i.time);
        let now_ms = now * 1000.0;
        let hovered = resp.hovered();

        let p = ui.painter().with_clip_rect(rect);

        // 1. Fondo base + cielo ambiental del tema (.dock-sky).
        p.rect_filled(rect, 0.0, c.header);
        draw_radial_glow(
            &p,
            Pos2::new(rect.center().x, rect.min.y + rect.height() * 0.86),
            rect.width() * 0.55,
            rect.height() * 0.48,
            c.accent,
            if c.dark { 0.12 } else { 0.08 },
        );

        // 2. Escena de fondo (catálogo de BuzzPrint, colores del tema) con su opacidad y desvanecido inferior.
        let fondo_op = self.look.fondo_opacity(c.dark);
        if fondo_op > 0.005 {
            crate::fondos::draw(&p, rect, &self.look.fondo, &c, fondo_op, c.header);
        }

        // 3. Geometría del horizonte y del personaje (como shots_web / dock.css:33-34).
        let has_border = !self.look.borde.is_empty() && self.look.borde != "ninguno";
        let floor_y = if has_border {
            (rect.max.y - 6.0).max(rect.min.y + 44.0)
        } else {
            (rect.max.y - 18.0).max(rect.min.y + 40.0)
        };
        // La celda del atlas trae aire arriba: se agranda un poco para que Boberto ocupe la banda como en .NET.
        let char_h = ((floor_y - rect.min.y) * 1.1).clamp(48.0, 150.0);
        let char_w = char_h * 20.0 / 21.0;
        // Las suelas en el lienzo 20×21 terminan ~5 % arriba del borde inferior de la celda.
        let base_y = floor_y + char_h * 0.04;
        let char_rect = Rect::from_min_size(Pos2::new(rect.center().x - char_w * 0.5, base_y - char_h), Vec2::new(char_w, char_h));

        // 4. Cursor + animación de poses.
        let cursor = if live_motion { host::global_cursor() } else { None };
        let (pose, xf, fast) = self.dock_anim.step(now, live_motion, false, hovered, false, cursor, char_rect, c.accent);

        // 5. Luz del piso subiendo detrás de Boberto (#plt-wrap::before).
        let light_boost = if hovered { 1.0 } else { 0.78 };
        draw_radial_glow(
            &p,
            Pos2::new(rect.center().x, floor_y),
            char_w * 0.95,
            char_h * 0.50,
            c.accent,
            (if c.dark { 0.13 } else { 0.08 }) * light_boost,
        );

        // 6. Reflejo en el piso (anclado al horizonte, espejado verticalmente, sin saltar con el cuerpo).
        if let Some(tex) = &self.sheet_tex {
            let uv = self.pose_uv(pose);
            let refl_clip = Rect::from_min_max(Pos2::new(rect.min.x, floor_y), rect.max);
            let rp = p.with_clip_rect(refl_clip);
            let refl_rect = Rect::from_min_size(Pos2::new(char_rect.min.x, floor_y - char_h * 0.05), Vec2::new(char_w, char_h));
            draw_reflection(&rp, refl_rect, tex.id(), uv, if c.dark { 0.24 } else { 0.14 });
        }

        // 7. Barra neón del horizonte: solo sin marco (con marco, el borde inferior cierra la escena y
        //    la barra no cruza la lectura "BOBERTO · ...").
        if !has_border {
            draw_radial_glow(
                &p,
                Pos2::new(rect.center().x, floor_y + 2.0),
                rect.width() * 0.48,
                16.0,
                c.accent,
                (if c.dark { 0.15 } else { 0.09 }) * light_boost,
            );
            draw_floor_bar(&p, rect, floor_y, c.accent, if hovered { 1.0 } else { 0.78 }, live_motion, now);
        }

        // 8. Brillo del cuerpo / FX (pulso, latido, parpadeo, halo, rgb).
        let (glow_col, glow_mul) = self.resolve_body_glow(&c, live_motion, now_ms);
        if self.look.character.is_empty() && glow_mul > 0.04 {
            let r_base = char_w * (0.44 + 0.16 * glow_mul.min(2.2));
            let a_base = (0.24 * glow_mul.min(1.6)).clamp(0.03, 0.42);
            draw_radial_glow(&p, char_rect.center(), r_base, r_base * 1.05, glow_col, a_base);
            if glow_mul > 1.15 {
                draw_radial_glow(&p, char_rect.center(), r_base * 1.55, r_base * 1.55, glow_col, a_base * 0.45);
            }
        }

        // 9. Sprite de Boberto con pivote en los pies (o logo de respaldo si falta el atlas).
        if let Some(tex) = &self.sheet_tex {
            let uv = self.pose_uv(pose);
            let tint = self.sprite_tint(live_motion, now_ms);
            if let Some((from, left)) = self.dock_anim.fade(now, pose) {
                // La pose anterior se apaga debajo mientras la nueva aparece encima.
                draw_sprite_xform(&p, char_rect, tex.id(), self.pose_uv(from), xf, tint.gamma_multiply(left));
                draw_sprite_xform(&p, char_rect, tex.id(), uv, xf, tint.gamma_multiply((1.0 - left).sqrt()));
            } else {
                draw_sprite_xform(&p, char_rect, tex.id(), uv, xf, tint);
            }
        } else {
            let m = Rect::from_center_size(char_rect.center(), Vec2::splat(52.0));
            super::mark::draw_with(&p, m, c.accent, Vec2::ZERO, &super::mark::current());
        }

        // 10. Partículas vectoriales de gestos.
        draw_particles(&p, &self.dock_anim.particles, char_rect, now);

        // 11. Marco elegido (los 19 BORDES de boberto-estudio.css).
        draw_border(&p, rect, &self.look.borde, c.accent, live_motion, now);

        // 12. Lectura mono inferior ("BOBERTO" a la izquierda, accesorio/gesto o "ESTUDIO ›" al pasar).
        let reading_y = rect.max.y - 4.0;
        let font = FontId::new(9.5, egui::FontFamily::Monospace);
        let right_text = if hovered {
            format!("{} ›", t("Estudio").to_uppercase())
        } else if let Some(g) = self.dock_anim.active_gesture_label() {
            g.to_uppercase()
        } else {
            self.look.display_label().to_uppercase()
        };
        let right_col = if hovered {
            c.accent
        } else if self.dock_anim.gesture.is_some() {
            c.ink
        } else {
            c.muted
        };
        let shadow = if c.dark { Color32::from_black_alpha(220) } else { alpha(c.header, 0.85) };
        let left_pos = Pos2::new(rect.min.x + 8.0, reading_y);
        let right_pos = Pos2::new(rect.max.x - 8.0, reading_y);
        // Sombra fina para que se lea sobre cualquier lámina.
        p.text(left_pos + Vec2::new(0.0, 1.0), Align2::LEFT_BOTTOM, "BOBERTO", font.clone(), shadow);
        p.text(left_pos, Align2::LEFT_BOTTOM, "BOBERTO", font.clone(), c.muted);
        let max_right_w = (rect.width() - 78.0).max(32.0);
        let clipped_right = fit_mono(&p, &right_text, &font, max_right_w);
        p.text(right_pos + Vec2::new(0.0, 1.0), Align2::RIGHT_BOTTOM, &clipped_right, font.clone(), shadow);
        p.text(right_pos, Align2::RIGHT_BOTTOM, &clipped_right, font, right_col);

        // Divisor izquierdo fino contra el encabezado.
        p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(1.0, rect.height())), 0.0, c.faint_border);

        // 13. Cadencia de repintado: 60 fps en gestos/parpadeos/FX rápidos, 30 fps al pasar el cursor,
        //     ~15 fps (66 ms) en reposo animado (baja a la mitad el CPU de la biblioteca), 0 si Motion=off.
        if live_motion {
            let has_fast_fx = self.look.fx_body.iter().any(|e| matches!(e.as_str(), "latido" | "parpadeo" | "rgb"))
                || self.look.fx_acc.iter().any(|e| e == "rgb");
            let ms = if fast || has_fast_fx {
                16
            } else if hovered {
                33
            } else {
                66
            };
            ctx.request_repaint_after(Duration::from_millis(ms));
        }

        resp.clicked()
    }

    /// Peana en la biblioteca vacía (`300×230`): fondo transparente, saluda al aparecer, sigue al cursor y hace gestos al clic.
    pub fn stage(&mut self, ui: &mut egui::Ui, rect: Rect, resp: &egui::Response, hwnd: HWND) {
        let ctx = ui.ctx();
        self.sync(ctx, hwnd);
        let c = super::colors();
        let motion = super::appearance().boberto_motion;
        let minimized = unsafe { IsIconic(hwnd) != 0 };
        let live_motion = motion && !minimized;
        let now = ctx.input(|i| i.time);
        let now_ms = now * 1000.0;

        let p = ui.painter().with_clip_rect(rect);

        let char_h = (rect.height() - 28.0).clamp(96.0, 178.0);
        let char_w = char_h * 20.0 / 21.0;
        let floor_y = rect.center().y + char_h * 0.48;
        let char_rect = Rect::from_min_size(Pos2::new(rect.center().x - char_w * 0.5, floor_y - char_h + char_h * 0.05), Vec2::new(char_w, char_h));

        let cursor = if live_motion { host::global_cursor() } else { None };
        let (pose, xf, fast) = self.stage_anim.step(now, live_motion, true, resp.hovered(), resp.clicked(), cursor, char_rect, c.accent);

        // Sombra suave de contacto en la peana (se achica un poco cuando salta).
        let lift = (-xf.dy).max(0.0);
        let sh_scale = (1.0 - lift * 2.2).clamp(0.55, 1.0);
        draw_radial_glow(
            &p,
            Pos2::new(rect.center().x, floor_y + 2.0),
            char_w * 0.44 * sh_scale,
            8.0 * sh_scale,
            if c.dark { Color32::BLACK } else { c.ink },
            if c.dark { 0.38 } else { 0.14 },
        );

        // Brillo / FX del cuerpo.
        let (glow_col, glow_mul) = self.resolve_body_glow(&c, live_motion, now_ms);
        if self.look.character.is_empty() && glow_mul > 0.04 {
            let r_base = char_w * (0.42 + 0.15 * glow_mul.min(2.2));
            let a_base = (0.20 * glow_mul.min(1.5)).clamp(0.03, 0.36);
            draw_radial_glow(&p, char_rect.center(), r_base, r_base * 1.05, glow_col, a_base);
        }

        // Sprite de Boberto.
        if let Some(tex) = &self.sheet_tex {
            let uv = self.pose_uv(pose);
            let tint = self.sprite_tint(live_motion, now_ms);
            if let Some((from, left)) = self.stage_anim.fade(now, pose) {
                // La pose anterior se apaga debajo mientras la nueva aparece encima.
                draw_sprite_xform(&p, char_rect, tex.id(), self.pose_uv(from), xf, tint.gamma_multiply(left));
                draw_sprite_xform(&p, char_rect, tex.id(), uv, xf, tint.gamma_multiply((1.0 - left).sqrt()));
            } else {
                draw_sprite_xform(&p, char_rect, tex.id(), uv, xf, tint);
            }
        } else {
            let m = Rect::from_center_size(char_rect.center(), Vec2::splat(96.0));
            super::mark::live_at(ui, m, resp, c.accent, true, &super::mark::current());
        }

        draw_particles(&p, &self.stage_anim.particles, char_rect, now);

        if live_motion {
            let ms = if fast { 16 } else { 66 };
            ctx.request_repaint_after(Duration::from_millis(ms));
        }
    }

    fn resolve_body_glow(&self, c: &Colors, motion: bool, now_ms: f64) -> (Color32, f32) {
        let rgb = self.look.fx_body.iter().any(|e| e == "rgb");
        let col = if rgb && motion {
            hsl_to_color32(((now_ms * 0.06) % 360.0) as f32, 0.90, 0.62)
        } else if !self.look.glow_body.is_empty() && self.look.glow_body != "auto" {
            theme::parse_hex(&self.look.glow_body)
        } else if self.look.skin == "custom" && !self.look.skin_color.is_empty() {
            let base = theme::parse_hex(&self.look.skin_color);
            let lum = (0.2126 * base.r() as f32 + 0.7152 * base.g() as f32 + 0.0722 * base.b() as f32) / 255.0;
            if lum < 0.14 { c.accent } else { base }
        } else {
            c.accent
        };
        let mul = if motion { fx_mul(&self.look.fx_body, now_ms).unwrap_or(0.55) } else { 0.55 };
        (col, mul)
    }

    fn sprite_tint(&self, motion: bool, now_ms: f64) -> Color32 {
        if !motion {
            return Color32::WHITE;
        }
        let rgb = self.look.fx_body.iter().any(|e| e == "rgb") || self.look.fx_acc.iter().any(|e| e == "rgb");
        if rgb {
            let c = hsl_to_color32(((now_ms * 0.06) % 360.0) as f32, 0.65, 0.84);
            mix(Color32::WHITE, c, 0.35)
        } else {
            Color32::WHITE
        }
    }
}

// ------------------------------------------------------------------ fórmulas de boberto.js

/// Exact port de `fxMul(effects, t)` de `boberto.js` (L521–555).
fn fx_mul(effects: &[String], t_ms: f64) -> Option<f32> {
    if effects.is_empty() {
        return None;
    }
    let mut m = 1.0f64;
    let mut any = false;
    for e in effects {
        match e.as_str() {
            "rgb" => continue,
            "pulso" => {
                any = true;
                m *= 0.30 + 0.70 * (0.5 + 0.5 * (t_ms / 1600.0).sin());
            }
            "latido" => {
                any = true;
                let p = (t_ms % 1100.0) / 1100.0;
                let b1 = (-((p - 0.10) / 0.055).powi(2)).exp();
                let b2 = 0.72 * (-((p - 0.30) / 0.055).powi(2)).exp();
                m *= 0.22 + 1.45 * (b1 + b2).min(1.0);
            }
            "parpadeo" => {
                any = true;
                let p = (t_ms % 900.0) / 900.0;
                m *= if p < 0.10 || (p > 0.20 && p < 0.28) { 1.15 } else { 0.10 };
            }
            "halo" => {
                any = true;
                m *= 2.10;
            }
            _ => {}
        }
    }
    any.then_some(m.max(0.06) as f32)
}

fn hsl_to_color32(h_deg: f32, s: f32, l: f32) -> Color32 {
    let h = h_deg.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let ch = |mut t: f32| -> u8 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(ch(h + 1.0 / 3.0), ch(h), ch(h - 1.0 / 3.0))
}

// ------------------------------------------------------------------ utilidades de dibujo

fn decode_png_bytes(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let n = w * h;
    let mut rgba = vec![255u8; n * 4];
    match info.color_type {
        png::ColorType::Rgba => rgba.copy_from_slice(&buf[..n * 4]),
        png::ColorType::Rgb => {
            for (d, s) in rgba.chunks_exact_mut(4).zip(buf.chunks_exact(3)) {
                d[..3].copy_from_slice(s);
            }
        }
        png::ColorType::Grayscale => {
            for (d, &s) in rgba.chunks_exact_mut(4).zip(buf.iter()) {
                d[0] = s;
                d[1] = s;
                d[2] = s;
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for (d, s) in rgba.chunks_exact_mut(4).zip(buf.chunks_exact(2)) {
                d[0] = s[0];
                d[1] = s[0];
                d[2] = s[0];
                d[3] = s[1];
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}

fn fit_mono(p: &egui::Painter, text: &str, font: &FontId, max_w: f32) -> String {
    if p.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size().x <= max_w {
        return text.to_string();
    }
    let mut s = String::new();
    for ch in text.chars() {
        let mut try_s = s.clone();
        try_s.push(ch);
        try_s.push('…');
        if p.layout_no_wrap(try_s, font.clone(), Color32::WHITE).size().x > max_w {
            break;
        }
        s.push(ch);
    }
    s.push('…');
    s
}

/// Elipse con degradado radial suave en anillos concéntricos.
fn draw_radial_glow(p: &egui::Painter, center: Pos2, rx: f32, ry: f32, color: Color32, peak_alpha: f32) {
    if peak_alpha <= 0.004 || rx <= 0.5 || ry <= 0.5 {
        return;
    }
    const SEG: u32 = 28;
    const RINGS: u32 = 4;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, alpha(color, peak_alpha));
    for r in 1..=RINGS {
        let t = r as f32 / RINGS as f32;
        let a = peak_alpha * (1.0 - t).powi(2);
        let col = alpha(color, a);
        for i in 0..SEG {
            let ang = i as f32 * TAU / SEG as f32;
            mesh.colored_vertex(Pos2::new(center.x + ang.cos() * rx * t, center.y + ang.sin() * ry * t), col);
        }
    }
    for i in 0..SEG {
        mesh.indices.extend([0, 1 + i, 1 + (i + 1) % SEG]);
    }
    for r in 1..RINGS {
        let a0 = 1 + (r - 1) * SEG;
        let b0 = 1 + r * SEG;
        for i in 0..SEG {
            let j = (i + 1) % SEG;
            mesh.indices.extend([a0 + i, b0 + i, b0 + j, a0 + i, b0 + j, a0 + j]);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// Dibuja el sprite con escala/traslación/rotación alrededor del pivote inferior-central (pies).
fn draw_sprite_xform(p: &egui::Painter, rect: Rect, tex_id: egui::TextureId, uv: Rect, xf: PoseXform, tint: Color32) {
    let pivot = Pos2::new(rect.center().x, rect.max.y);
    let (sn, cs) = xf.rot.sin_cos();
    let (w, h) = (rect.width(), rect.height());
    // Malla de 6×8: la parte de arriba (cabeza) se dobla hacia donde mira y los pies quedan quietos. Mirando
    // abajo, la cabeza asiente y la parte de arriba se acerca (más ancha); mirando arriba, se aleja.
    let xform = |u: f32, v: f32| -> Pos2 {
        let up = 1.0 - v;
        let bend = up.powf(1.6);
        let qx = rect.min.x + u * w + xf.tx * w * 0.05 * bend + (u - 0.5) * w * xf.ty * 0.06 * up;
        let qy = rect.min.y + v * h + xf.ty * h * 0.03 * bend - xf.tx.abs() * h * 0.006 * bend;
        let lx = (qx - pivot.x) * xf.sx;
        let ly = (qy - pivot.y) * xf.sy;
        let rx = lx * cs - ly * sn;
        let ry = lx * sn + ly * cs;
        Pos2::new(pivot.x + rx + xf.dx * w, pivot.y + ry + xf.dy * h)
    };
    const NX: u32 = 6;
    const NY: u32 = 8;
    let mut mesh = Mesh::with_texture(tex_id);
    for j in 0..=NY {
        for i in 0..=NX {
            let (u, v) = (i as f32 / NX as f32, j as f32 / NY as f32);
            let t = Pos2::new(uv.min.x + uv.width() * u, uv.min.y + uv.height() * v);
            mesh.vertices.push(Vertex { pos: xform(u, v), uv: t, color: tint });
        }
    }
    for j in 0..NY {
        for i in 0..NX {
            let a = j * (NX + 1) + i;
            let b = a + NX + 1;
            mesh.indices.extend([a, a + 1, b + 1, a, b + 1, b]);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// Reflejo espejado verticalmente debajo del piso, desvanecido en el primer 38 % del alto.
fn draw_reflection(p: &egui::Painter, rect: Rect, tex_id: egui::TextureId, uv: Rect, top_alpha: f32) {
    let fade_frac = 0.38f32;
    let y_end = rect.min.y + rect.height() * fade_frac;
    // Espejado vertical: arriba del reflejo va el borde inferior del personaje (uv.max.y).
    let v_top = uv.max.y;
    let v_end = uv.max.y - (uv.max.y - uv.min.y) * fade_frac;
    let c_top = Color32::from_white_alpha((top_alpha * 255.0).round().clamp(0.0, 255.0) as u8);
    let c_end = Color32::TRANSPARENT;
    let mut mesh = Mesh::with_texture(tex_id);
    mesh.vertices = vec![
        Vertex { pos: Pos2::new(rect.min.x, rect.min.y), uv: Pos2::new(uv.min.x, v_top), color: c_top },
        Vertex { pos: Pos2::new(rect.max.x, rect.min.y), uv: Pos2::new(uv.max.x, v_top), color: c_top },
        Vertex { pos: Pos2::new(rect.max.x, y_end), uv: Pos2::new(uv.max.x, v_end), color: c_end },
        Vertex { pos: Pos2::new(rect.min.x, y_end), uv: Pos2::new(uv.min.x, v_end), color: c_end },
    ];
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    p.add(Shape::mesh(mesh));
}

/// Barra horizontal neón del horizonte (.plt-dock-barra) + barrido de 7 s (`bzp-edge-sweep`).
fn draw_floor_bar(p: &egui::Painter, rect: Rect, y: f32, accent: Color32, opacity: f32, motion: bool, now: f64) {
    let x0 = rect.min.x + 6.0;
    let x1 = rect.max.x - 6.0;
    let w = (x1 - x0).max(1.0);
    let h = 2.0;
    let stops: [(f32, f32); 5] = [(0.0, 0.0), (0.14, 0.45), (0.50, 0.85), (0.86, 0.45), (1.0, 0.0)];
    let mut mesh = Mesh::default();
    for &(f, a) in &stops {
        let x = x0 + w * f;
        let col = alpha(accent, a * opacity);
        mesh.colored_vertex(Pos2::new(x, y), col);
        mesh.colored_vertex(Pos2::new(x, y + h), col);
    }
    for i in 0..(stops.len() as u32 - 1) {
        let b = i * 2;
        mesh.indices.extend([b, b + 2, b + 3, b, b + 3, b + 1]);
    }
    p.add(Shape::mesh(mesh));

    if motion {
        let sw_w = w * 0.26;
        let phase = ((now / 7.0).fract()) as f32;
        let sx = x0 - sw_w * 1.2 + phase * (w + sw_w * 2.4);
        let clip = Rect::from_min_max(Pos2::new(x0, y - 1.0), Pos2::new(x1, y + h + 1.0));
        let sp = p.with_clip_rect(clip);
        let bright = mix(accent, Color32::WHITE, 0.30);
        let mut sm = Mesh::default();
        let spts = [(sx, Color32::TRANSPARENT), (sx + sw_w * 0.5, alpha(bright, 0.95 * opacity)), (sx + sw_w, Color32::TRANSPARENT)];
        for &(x, col) in &spts {
            sm.colored_vertex(Pos2::new(x, y), col);
            sm.colored_vertex(Pos2::new(x, y + h), col);
        }
        sm.indices.extend([0, 2, 3, 0, 3, 1, 2, 4, 5, 2, 5, 3]);
        sp.add(Shape::mesh(sm));
    }
}

/// Dibuja partículas vectoriales (sin depender de fuentes de símbolos).
fn draw_particles(p: &egui::Painter, list: &[Particle], char_rect: Rect, now: f64) {
    let ctr = char_rect.center();
    let scale = char_rect.height();
    for pt in list {
        let t = ((now - pt.born) / pt.dur).clamp(0.0, 1.0) as f32;
        let fade = if t < 0.15 { t / 0.15 } else { 1.0 - (t - 0.15) / 0.85 };
        let col = alpha(pt.color, fade.clamp(0.0, 1.0));
        let pos = Pos2::new(ctr.x + (pt.ox + pt.vx * t) * scale, ctr.y + (pt.oy + pt.vy * t) * scale);
        let r = pt.size * (0.75 + 0.35 * (1.0 - t));
        match pt.shape {
            ParticleShape::Sparkle => {
                let st = Stroke::new(1.3_f32, col);
                p.line_segment([pos - Vec2::new(r, 0.0), pos + Vec2::new(r, 0.0)], st);
                p.line_segment([pos - Vec2::new(0.0, r), pos + Vec2::new(0.0, r)], st);
            }
            ParticleShape::Star => {
                let st = Stroke::new(1.2_f32, col);
                p.line_segment([pos - Vec2::new(r, 0.0), pos + Vec2::new(r, 0.0)], st);
                p.line_segment([pos - Vec2::new(0.0, r), pos + Vec2::new(0.0, r)], st);
                let d = r * 0.65;
                p.line_segment([pos - Vec2::new(d, d), pos + Vec2::new(d, d)], st);
                p.line_segment([pos + Vec2::new(-d, d), pos + Vec2::new(d, -d)], st);
            }
            ParticleShape::Note => {
                p.circle_filled(pos + Vec2::new(-r * 0.35, r * 0.45), r * 0.42, col);
                p.line_segment([pos + Vec2::new(0.0, r * 0.40), pos + Vec2::new(0.0, -r * 0.85)], Stroke::new(1.4_f32, col));
                p.line_segment([pos + Vec2::new(0.0, -r * 0.85), pos + Vec2::new(r * 0.70, -r * 0.55)], Stroke::new(1.6_f32, col));
            }
            ParticleShape::Puff => {
                p.circle_stroke(pos, r * (0.7 + 0.6 * t), Stroke::new(1.2_f32, alpha(pt.color, fade * 0.65)));
            }
            ParticleShape::Heart => {
                let hr = r * 0.52;
                p.circle_filled(pos + Vec2::new(-hr * 0.55, -hr * 0.2), hr * 0.65, col);
                p.circle_filled(pos + Vec2::new(hr * 0.55, -hr * 0.2), hr * 0.65, col);
                p.add(Shape::convex_polygon(
                    vec![pos + Vec2::new(-hr * 1.15, 0.0), pos + Vec2::new(hr * 1.15, 0.0), pos + Vec2::new(0.0, hr * 1.35)],
                    col,
                    Stroke::NONE,
                ));
            }
            ParticleShape::Dot => {
                p.circle_filled(pos, r * 0.55, col);
            }
        }
    }
}

// ------------------------------------------------------------------ los 19 BORDES (boberto-estudio.css L651–868)

fn draw_border(p: &egui::Painter, rect: Rect, model: &str, accent: Color32, motion: bool, now: f64) {
    let inset_rect = |r: Rect, px: f32, col: Color32| {
        p.rect_stroke(r, 0.0, Stroke::new(px, col), egui::StrokeKind::Inside);
    };
    match model {
        "" | "ninguno" => {}
        "filete" => {
            inset_rect(rect, 1.0, mix(accent, Color32::WHITE, 0.18));
        }
        "doble" => {
            inset_rect(rect, 1.0, mix(accent, Color32::WHITE, 0.12));
            inset_rect(rect.shrink(5.0), 1.0, alpha(accent, 0.50));
        }
        "visor" => {
            draw_corners(p, rect, 18.0, 18.0, 2.0, accent);
        }
        "vidrio" => {
            inset_rect(rect, 1.0, Color32::from_white_alpha(82));
            p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)), 0.0, Color32::from_white_alpha(140));
            draw_inner_glow(p, rect, 14.0, Color32::WHITE, 0.08);
        }
        "pulso" => {
            let k = if motion { 0.45 + 0.55 * (0.5 + 0.5 * (now as f32 * TAU / 2.8).sin()) } else { 0.85 };
            inset_rect(rect, 1.0, alpha(accent, 0.40 * k));
            draw_inner_glow(p, rect, 16.0, accent, 0.30 * k);
        }
        "arcade" => {
            inset_rect(rect, 5.0, Color32::from_rgb(0x14, 0x16, 0x1E));
            inset_rect(rect.shrink(5.0), 1.0, mix(accent, Color32::WHITE, 0.20));
            inset_rect(rect.shrink(6.0), 2.0, Color32::from_rgb(0x0C, 0x0E, 0x14));
        }
        "banda" => {
            inset_rect(rect, 3.0, accent);
        }
        "foto" => {
            inset_rect(rect, 1.0, mix(accent, Color32::WHITE, 0.08));
            inset_rect(rect.shrink(5.0), 1.0, alpha(accent, 0.70));
        }
        "guion" => {
            draw_dashed_rect(p, rect.shrink(1.0), 1.5, 6.0, 4.0, mix(accent, Color32::WHITE, 0.12));
        }
        "cine" => {
            p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), 2.0)), 0.0, accent);
            p.rect_filled(Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y - 2.0), Vec2::new(rect.width(), 2.0)), 0.0, accent);
        }
        "esquina" => {
            draw_corners(p, rect, rect.width() * 0.28, rect.height() * 0.34, 2.0, accent);
        }
        "neon" => {
            let phase = if motion { (now / 4.2).fract() as f32 } else { 0.0 };
            let stops = [
                (0.00, Color32::from_rgb(0xFF, 0x3D, 0xF0)),
                (0.33, Color32::from_rgb(0x3D, 0xE1, 0xFF)),
                (0.66, Color32::from_rgb(0xE4, 0xFF, 0x4A)),
                (1.00, Color32::from_rgb(0xFF, 0x3D, 0xF0)),
            ];
            draw_conic_frame(p, rect, 2.0, phase, &stops);
        }
        "barrido" => {
            let phase = if motion { (now / 3.2).fract() as f32 } else { 0.0 };
            let stops = [
                (0.00, Color32::TRANSPARENT),
                (0.72, Color32::TRANSPARENT),
                (0.82, alpha(accent, 0.15)),
                (0.90, Color32::WHITE),
                (0.94, accent),
                (0.98, Color32::TRANSPARENT),
                (1.00, Color32::TRANSPARENT),
            ];
            draw_conic_frame(p, rect, 2.0, phase, &stops);
        }
        "estela" => {
            inset_rect(rect, 2.0, alpha(accent, 0.58));
            let phase = if motion { (now / 5.4).fract() as f32 } else { 0.0 };
            let stops = [
                (0.00, Color32::TRANSPARENT),
                (0.22, alpha(accent, 0.12)),
                (0.46, alpha(accent, 0.45)),
                (0.62, accent),
                (0.70, Color32::WHITE),
                (0.78, Color32::TRANSPARENT),
                (1.00, Color32::TRANSPARENT),
            ];
            draw_conic_frame(p, rect, 3.0, phase, &stops);
        }
        "gemelos" => {
            inset_rect(rect, 2.0, alpha(accent, 0.58));
            let phase = if motion { (now / 3.6).fract() as f32 } else { 0.0 };
            let stops = [
                (0.00, Color32::TRANSPARENT),
                (0.06, alpha(accent, 0.30)),
                (0.12, accent),
                (0.16, Color32::WHITE),
                (0.24, Color32::TRANSPARENT),
                (0.50, Color32::TRANSPARENT),
                (0.56, alpha(accent, 0.30)),
                (0.62, accent),
                (0.66, Color32::WHITE),
                (0.74, Color32::TRANSPARENT),
                (1.00, Color32::TRANSPARENT),
            ];
            draw_conic_frame(p, rect, 3.0, phase, &stops);
        }
        "chispa" => {
            inset_rect(rect, 2.0, alpha(accent, 0.58));
            let phase = if motion { (now / 1.7).fract() as f32 } else { 0.0 };
            let stops = [
                (0.00, Color32::TRANSPARENT),
                (0.90, Color32::TRANSPARENT),
                (0.94, alpha(accent, 0.55)),
                (0.97, Color32::WHITE),
                (1.00, Color32::TRANSPARENT),
            ];
            draw_conic_frame(p, rect, 3.0, phase, &stops);
        }
        "flujo" => {
            let phase = if motion { (now / 7.5).fract() as f32 } else { 0.0 };
            let stops = [
                (0.00, alpha(accent, 0.28)),
                (0.25, alpha(accent, 0.55)),
                (0.50, Color32::WHITE),
                (0.75, accent),
                (1.00, alpha(accent, 0.28)),
            ];
            draw_conic_frame(p, rect, 3.0, phase, &stops);
        }
        "cruce" => {
            inset_rect(rect, 2.0, alpha(accent, 0.58));
            let phase_a = if motion { (now / 3.4).fract() as f32 } else { 0.0 };
            let phase_b = if motion { (1.0 - (now / 4.8).fract() as f32 + 0.5).fract() } else { 0.5 };
            let cyan = Color32::from_rgb(0x7E, 0xF9, 0xFF);
            let stops_a = [
                (0.00, Color32::TRANSPARENT),
                (0.08, Color32::TRANSPARENT),
                (0.16, alpha(accent, 0.40)),
                (0.24, accent),
                (0.28, Color32::WHITE),
                (0.36, Color32::TRANSPARENT),
                (1.00, Color32::TRANSPARENT),
            ];
            let stops_b = [
                (0.00, Color32::TRANSPARENT),
                (0.08, Color32::TRANSPARENT),
                (0.16, alpha(cyan, 0.45)),
                (0.24, cyan),
                (0.28, Color32::WHITE),
                (0.36, Color32::TRANSPARENT),
                (1.00, Color32::TRANSPARENT),
            ];
            draw_conic_frame(p, rect, 3.0, phase_a, &stops_a);
            draw_conic_frame(p, rect, 3.0, phase_b, &stops_b);
        }
        _ => {}
    }
}

fn draw_corners(p: &egui::Painter, r: Rect, lw: f32, lh: f32, t: f32, col: Color32) {
    // Top-left
    p.rect_filled(Rect::from_min_size(r.left_top(), Vec2::new(lw, t)), 0.0, col);
    p.rect_filled(Rect::from_min_size(r.left_top(), Vec2::new(t, lh)), 0.0, col);
    // Top-right
    p.rect_filled(Rect::from_min_size(Pos2::new(r.max.x - lw, r.min.y), Vec2::new(lw, t)), 0.0, col);
    p.rect_filled(Rect::from_min_size(Pos2::new(r.max.x - t, r.min.y), Vec2::new(t, lh)), 0.0, col);
    // Bottom-left
    p.rect_filled(Rect::from_min_size(Pos2::new(r.min.x, r.max.y - t), Vec2::new(lw, t)), 0.0, col);
    p.rect_filled(Rect::from_min_size(Pos2::new(r.min.x, r.max.y - lh), Vec2::new(t, lh)), 0.0, col);
    // Bottom-right
    p.rect_filled(Rect::from_min_size(Pos2::new(r.max.x - lw, r.max.y - t), Vec2::new(lw, t)), 0.0, col);
    p.rect_filled(Rect::from_min_size(Pos2::new(r.max.x - t, r.max.y - lh), Vec2::new(t, lh)), 0.0, col);
}

fn draw_inner_glow(p: &egui::Painter, r: Rect, depth: f32, col: Color32, peak_a: f32) {
    let inner = r.shrink(depth.min(r.width() * 0.4).min(r.height() * 0.4));
    let outer_c = alpha(col, peak_a);
    let inner_c = Color32::TRANSPARENT;
    let mut mesh = Mesh::default();
    let o = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    let i = [inner.left_top(), inner.right_top(), inner.right_bottom(), inner.left_bottom()];
    for k in 0..4 {
        mesh.colored_vertex(o[k], outer_c);
        mesh.colored_vertex(i[k], inner_c);
    }
    for k in 0..4u32 {
        let n = (k + 1) % 4;
        mesh.indices.extend([k * 2, n * 2, n * 2 + 1, k * 2, n * 2 + 1, k * 2 + 1]);
    }
    p.add(Shape::mesh(mesh));
}

fn draw_dashed_rect(p: &egui::Painter, r: Rect, width: f32, dash: f32, gap: f32, col: Color32) {
    let st = Stroke::new(width, col);
    let step = dash + gap;
    let draw_edge = |a: Pos2, b: Pos2| {
        let d = b - a;
        let len = d.length();
        if len < 0.5 {
            return;
        }
        let dir = d / len;
        let mut t = 0.0;
        while t < len {
            let e = (t + dash).min(len);
            p.line_segment([a + dir * t, a + dir * e], st);
            t += step;
        }
    };
    draw_edge(r.left_top(), r.right_top());
    draw_edge(r.right_top(), r.right_bottom());
    draw_edge(r.right_bottom(), r.left_bottom());
    draw_edge(r.left_bottom(), r.left_top());
}

/// Dibuja un marco rectangular con gradiente cónico giratorio (réplica de `conic-gradient` + `mask-composite: exclude`).
fn draw_conic_frame(p: &egui::Painter, rect: Rect, thickness: f32, phase: f32, stops: &[(f32, Color32)]) {
    let inner = rect.shrink(thickness);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }
    const PER_EDGE: usize = 16;
    let ctr = rect.center();
    let mut outer_pts = Vec::with_capacity(PER_EDGE * 4);
    let mut inner_pts = Vec::with_capacity(PER_EDGE * 4);
    let edges = [
        (rect.left_top(), rect.right_top(), inner.left_top(), inner.right_top()),
        (rect.right_top(), rect.right_bottom(), inner.right_top(), inner.right_bottom()),
        (rect.right_bottom(), rect.left_bottom(), inner.right_bottom(), inner.left_bottom()),
        (rect.left_bottom(), rect.left_top(), inner.left_bottom(), inner.left_top()),
    ];
    for &(oa, ob, ia, ib) in &edges {
        for k in 0..PER_EDGE {
            let t = k as f32 / PER_EDGE as f32;
            outer_pts.push(oa.lerp(ob, t));
            inner_pts.push(ia.lerp(ib, t));
        }
    }
    let n = outer_pts.len();
    let mut mesh = Mesh::default();
    for k in 0..n {
        let q = outer_pts[k];
        // Ángulo en sentido horario desde arriba (0 = 12 en punto, igual que `conic-gradient(from 0deg)`).
        let ang = ((q.x - ctr.x).atan2(-(q.y - ctr.y)).rem_euclid(TAU)) / TAU;
        let t = (ang - phase).rem_euclid(1.0);
        let col = sample_stops(stops, t);
        mesh.colored_vertex(q, col);
        mesh.colored_vertex(inner_pts[k], col);
    }
    for k in 0..n as u32 {
        let next = (k + 1) % n as u32;
        mesh.indices.extend([k * 2, next * 2, next * 2 + 1, k * 2, next * 2 + 1, k * 2 + 1]);
    }
    p.add(Shape::mesh(mesh));
}

fn sample_stops(stops: &[(f32, Color32)], t: f32) -> Color32 {
    if stops.is_empty() {
        return Color32::TRANSPARENT;
    }
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t <= t1 {
            let f = if (t1 - t0).abs() < 1e-5 { 0.0 } else { (t - t0) / (t1 - t0) };
            return lerp_rgba(c0, c1, f);
        }
    }
    stops[stops.len() - 1].1
}

fn lerp_rgba(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix_u8 = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8 };
    Color32::from_rgba_unmultiplied(mix_u8(a.r(), b.r()), mix_u8(a.g(), b.g()), mix_u8(a.b(), b.b()), mix_u8(a.a(), b.a()))
}
