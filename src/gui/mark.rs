//! El logo de BuzzPrint (BobertoMark): cabeza con volumen sobre una placa de acento y ojos que siguen al cursor.
//! Port del dibujo de BobertoMark.cs: mismas formas, sombra del cuerpo, luz y ojos que contrastan con la cabeza.

use egui::{Color32, Pos2, Rect, Shape, Vec2};

use crate::store;

const N: usize = 72;

fn polar(r: impl Fn(f32) -> f32, sx: f32, sy: f32) -> Vec<(f32, f32)> {
    (0..N)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / N as f32;
            let v = r(a);
            (32.0 + a.cos() * v * sx, 32.0 + a.sin() * v * sy)
        })
        .collect()
}

/// Contorno de una figura dada por "está adentro": desde el centro, cada rayo avanza hasta salir de la figura.
fn ray(cx: f32, cy: f32, inside: impl Fn(f32, f32) -> bool) -> Vec<(f32, f32)> {
    (0..N)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / N as f32;
            let (dx, dy) = (a.cos(), a.sin());
            let mut r = 0.0;
            while r < 32.0 && inside(cx + dx * (r + 0.25), cy + dy * (r + 0.25)) {
                r += 0.25;
            }
            (cx + dx * r, cy + dy * r)
        })
        .collect()
}

/// Las 14 formas, los 6 ojos y los colores que ofrece Personalizar (los mismos que MarkStyle de .NET).
pub const SHAPES: [&str; 14] = ["hex", "circle", "squircle", "diamond", "ghost", "blob", "pebble", "cloud", "drop", "clover", "flower", "star", "burst", "heart"];
pub const EYES: [&str; 6] = ["oval", "pill", "round", "happy", "line", "pixel"];
pub const HEADS: [&str; 7] = ["#0E1015", "#F4F4F5", "#2684FF", "#10B981", "#F59E0B", "#8B5CF6", "#F43F5E"];
pub const EYE_COLORS: [&str; 9] = ["auto", "accent", "#F4F4F5", "#0E1015", "#22D3EE", "#39FF14", "#F59E0B", "#F43F5E", "#A78BFA"];
pub const TILES: [&str; 9] = ["accent", "#F43F5E", "#0E1015", "#F4F4F5", "#2684FF", "#10B981", "#F59E0B", "#8B5CF6", "none"];

/// Polígono redondeado (como el trazo de 7 unidades de la versión .NET): se infla cada vértice.
fn rounded(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let n = pts.len();
    for i in 0..n {
        let (px, py) = pts[i];
        let (ax, ay) = pts[(i + n - 1) % n];
        let (bx, by) = pts[(i + 1) % n];
        let a1 = (py - ay).atan2(px - ax) - std::f32::consts::FRAC_PI_2;
        let a2 = (by - py).atan2(bx - px) - std::f32::consts::FRAC_PI_2;
        let mut d = a2 - a1;
        while d < 0.0 {
            d += std::f32::consts::TAU;
        }
        for k in 0..=6 {
            let a = a1 + d * k as f32 / 6.0;
            out.push((px + a.cos() * 3.5, py + a.sin() * 3.5));
        }
    }
    out
}

/// Contorno de la cabeza en un cuadro de 64 unidades y la altura de los ojos.
fn outline(shape: &str) -> (Vec<(f32, f32)>, f32) {
    use std::f32::consts::PI;
    match shape {
        "circle" => (polar(|_| 19.5, 1.0, 1.0), 32.0),
        "blob" => (polar(|a| 19.2 * (1.0 + 0.055 * (3.0 * a + 0.5).cos() + 0.035 * (2.0 * a).sin()), 1.0, 1.0), 32.0),
        "pebble" => (polar(|a| 19.0 * (1.0 + 0.09 * (2.0 * a + 0.9).cos() + 0.045 * (3.0 * a + 2.1).cos()), 1.08, 0.92), 33.0),
        "clover" => (polar(|a| 16.8 + 4.4 * (4.0 * a + PI / 4.0).cos(), 1.0, 1.0), 32.0),
        "flower" => (polar(|a| 17.2 + 3.0 * (6.0 * a).cos(), 1.0, 1.0), 32.0),
        "star" => (polar(|a| 13.6 + 9.0 * ((1.0 + (4.0 * a).cos()) / 2.0).powf(2.4), 1.0, 1.0), 32.0),
        "burst" => (polar(|a| 17.6 + 2.4 * (9.0 * a).cos(), 1.0, 1.0), 32.0),
        "squircle" => (polar(|a| 19.0 / (a.cos().abs().powf(4.0) + a.sin().abs().powf(4.0)).powf(0.25), 1.0, 1.0), 32.0),
        "diamond" => (rounded(&[(32.0, 12.0), (52.0, 32.0), (32.0, 52.0), (12.0, 32.0)]), 32.0),
        "ghost" => (
            ray(32.0, 32.0, |x, y| {
                let bottom = 49.0 + 1.75 * (PI * (49.5 - x) / 5.8).sin();
                (y <= 32.0 && (x - 32.0).hypot(y - 32.0) <= 17.5) || (y > 32.0 && (14.5..=49.5).contains(&x) && y <= bottom)
            }),
            31.0,
        ),
        "cloud" => (
            ray(32.0, 32.0, |x, y| {
                (x - 23.0).hypot(y - 37.0) <= 10.5 || (x - 33.0).hypot(y - 28.5) <= 13.0 || (x - 43.0).hypot(y - 36.0) <= 10.0 || ((22.0..=44.0).contains(&x) && (34.0..=47.0).contains(&y))
            }),
            35.0,
        ),
        "drop" => (
            ray(32.0, 32.0, |x, y| {
                if y >= 38.5 {
                    (x - 32.0).hypot(y - 38.5) <= 18.5
                } else {
                    y >= 11.0 && (x - 32.0).abs() <= 18.5 * ((y - 11.0) / 27.5).powf(0.62)
                }
            }),
            36.0,
        ),
        "heart" => (
            (0..N)
                .map(|i| {
                    let t = i as f32 * std::f32::consts::TAU / N as f32;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    (32.0 + x * 1.22, 30.5 - y * 1.2)
                })
                .collect(),
            31.0,
        ),
        _ => (rounded(&[(32.0, 13.0), (48.5, 22.5), (48.5, 41.5), (32.0, 51.0), (15.5, 41.5), (15.5, 22.5)]), 31.0), // hex
    }
}

fn shade(c: Color32, k: f32) -> Color32 {
    // Oscurece; las cabezas muy oscuras se aclaran para que el cuerpo se lea sobre la placa.
    let lum = (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0;
    if lum < 0.12 {
        let add = |v: u8, n: f32| (v as f32 + n * (1.2 - k)).min(255.0) as u8;
        return Color32::from_rgb(add(c.r(), 70.0), add(c.g(), 70.0), add(c.b(), 76.0));
    }
    Color32::from_rgb((c.r() as f32 * k) as u8, (c.g() as f32 * k) as u8, (c.b() as f32 * k) as u8)
}

/// Mezcla `top` con opacidad `a` (0..1) sobre `base`.
fn over(base: [f32; 3], top: [f32; 3], a: f32) -> [f32; 3] {
    [base[0] + (top[0] - base[0]) * a, base[1] + (top[1] - base[1]) * a, base[2] + (top[2] - base[2]) * a]
}

/// El logo original de BuzzPrint (hexágono negro, ojos ovalados, sin placa).
pub fn original() -> Style {
    Style { mark_shape: "hex".into(), mark_eyes: "oval".into(), mark_head: "#0E1015".into(), mark_tile: "none".into(), mark_eye_color: "auto".into() }
}

/// El logo propio de cada paleta: forma, ojos y colores elegidos para que combinen con ella. Sin placa: la
/// placa solo aparece si la persona la elige.
pub fn for_palette(id: &str) -> Style {
    let (shape, eyes, head, eye) = match id {
        "light" => ("squircle", "round", "#0E1015", "auto"),
        "dark" => ("hex", "oval", "#0E1015", "#22D3EE"),
        "cosmos" => ("star", "round", "#0E1015", "#A78BFA"),
        "stellar" => ("circle", "pill", "#0E1015", "accent"),
        "abyss" => ("drop", "oval", "#0E1015", "#22D3EE"),
        "teal" => ("pebble", "happy", "#0E1015", "accent"),
        "nord" => ("squircle", "line", "#F4F4F5", "auto"),
        "dracula" => ("ghost", "round", "#0E1015", "#A78BFA"),
        "catppuccin" => ("cloud", "happy", "#F4F4F5", "auto"),
        "synthwave" => ("diamond", "pixel", "#0E1015", "#F43F5E"),
        "phosphor" => ("squircle", "pixel", "#0E1015", "#39FF14"),
        "forest" => ("clover", "oval", "#0E1015", "#39FF14"),
        "ember" => ("flower", "happy", "#0E1015", "#F59E0B"),
        "rose" => ("heart", "round", "#0E1015", "accent"),
        "dragon" => ("burst", "line", "#0E1015", "#F59E0B"),
        "pergamino" => ("blob", "oval", "#0E1015", "auto"),
        "neomirai" => ("hex", "oval", "#0E1015", "auto"),
        "kanagawa" => ("drop", "line", "#0E1015", "auto"),
        "sandstone" => ("pebble", "round", "#0E1015", "auto"),
        "papel" => ("circle", "oval", "#F4F4F5", "auto"),
        "sepia" => ("blob", "happy", "#0E1015", "auto"),
        "glaciar" => ("cloud", "oval", "#F4F4F5", "auto"),
        "contraste" => ("squircle", "pill", "#0E1015", "#F4F4F5"),
        _ => return original(),
    };
    Style { mark_shape: shape.into(), mark_eyes: eyes.into(), mark_head: head.into(), mark_tile: "none".into(), mark_eye_color: eye.into() }
}

/// El estilo del logo guardado en la configuración.
pub fn current() -> Style {
    let s = store::settings();
    Style { mark_shape: s.mark_shape, mark_head: s.mark_head, mark_eyes: s.mark_eyes, mark_tile: s.mark_tile, mark_eye_color: s.mark_eye_color }
}

/// Forma, cabeza, ojos y placa del logo (claves MarkShape, MarkHead, MarkEyes, MarkTile de la configuración).
#[derive(Clone, PartialEq, Default)]
pub struct Style {
    pub mark_shape: String,
    pub mark_head: String,
    pub mark_eyes: String,
    pub mark_tile: String,
    /// "auto" (o vacío), "accent" o #RRGGBB.
    pub mark_eye_color: String,
}

pub fn draw_with(p: &egui::Painter, rect: Rect, accent: Color32, look: Vec2, st: &Style) {
    draw_full(p, rect, accent, st, Vec2::new(look.x * 3.4, look.y * 2.6), (1.0, 1.0), 0.0);
}

/// Contorno ya girado (en unidades de 64).
fn turned(shape: &str, spin: f32) -> (Vec<(f32, f32)>, f32) {
    let (mut pts, eye_y) = outline(shape);
    if spin.rem_euclid(360.0) > 0.01 {
        let (sn, cs) = spin.to_radians().sin_cos();
        for q in pts.iter_mut() {
            let (dx, dy) = (q.0 - 32.0, q.1 - 32.0);
            *q = (32.0 + dx * cs - dy * sn, 32.0 + dx * sn + dy * cs);
        }
    }
    (pts, eye_y)
}

/// Todo lo que hace falta para pintar el logo con cualquier "pincel" (egui en pantalla, o el rasterizador
/// del ícono): triángulos con color por vértice, en unidades de 64.
pub struct Tris {
    pub verts: Vec<(f32, f32, Color32)>,
    pub idx: Vec<u32>,
}

impl Tris {
    fn new() -> Tris {
        Tris { verts: Vec::new(), idx: Vec::new() }
    }
    fn fan(&mut self, pts: &[(f32, f32)], center: (f32, f32), col: Color32) {
        let b = self.verts.len() as u32;
        self.verts.push((center.0, center.1, col));
        for q in pts {
            self.verts.push((q.0, q.1, col));
        }
        let n = pts.len() as u32;
        for i in 0..n {
            self.idx.extend([b, b + 1 + i, b + 1 + (i + 1) % n]);
        }
    }
    fn ellipse(&mut self, c: (f32, f32), rx: f32, ry: f32, col: Color32) {
        let pts: Vec<(f32, f32)> = (0..28).map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 28.0;
            (c.0 + a.cos() * rx, c.1 + a.sin() * ry)
        }).collect();
        self.fan(&pts, c, col);
    }
    fn round_rect(&mut self, c: (f32, f32), w: f32, h: f32, r: f32, col: Color32) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let mut pts = Vec::new();
        let corners = [(c.0 + w / 2.0 - r, c.1 + h / 2.0 - r, 0.0f32), (c.0 - w / 2.0 + r, c.1 + h / 2.0 - r, 90.0), (c.0 - w / 2.0 + r, c.1 - h / 2.0 + r, 180.0), (c.0 + w / 2.0 - r, c.1 - h / 2.0 + r, 270.0)];
        for (x, y, a0) in corners {
            for k in 0..=6 {
                let a = (a0 + 90.0 * k as f32 / 6.0).to_radians();
                pts.push((x + a.cos() * r, y + a.sin() * r));
            }
        }
        self.fan(&pts, c, col);
    }
    fn thick_line(&mut self, pts: &[(f32, f32)], w: f32, col: Color32) {
        for s in pts.windows(2) {
            let (dx, dy) = (s[1].0 - s[0].0, s[1].1 - s[0].1);
            let l = (dx * dx + dy * dy).sqrt().max(1e-4);
            let (nx, ny) = (-dy / l * w / 2.0, dx / l * w / 2.0);
            let b = self.verts.len() as u32;
            self.verts.extend([(s[0].0 + nx, s[0].1 + ny, col), (s[1].0 + nx, s[1].1 + ny, col), (s[1].0 - nx, s[1].1 - ny, col), (s[0].0 - nx, s[0].1 - ny, col)]);
            self.idx.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        for q in pts {
            self.ellipse(*q, w / 2.0, w / 2.0, col);
        }
    }
}

/// Arma el logo como triángulos (port de BobertoMark.Draw). `eye`: desvío de los ojos en unidades de 64
/// (±3,4 × ±2,6); `blink`: apertura de cada ojo (1 abierto, 0 cerrado); `spin`: giro de la cabeza en grados.
/// `tiny`: ojos un poco más grandes para tamaños chicos.
pub fn build(accent: Color32, st: &Style, eye: Vec2, blink: (f32, f32), spin: f32, tiny: bool) -> Tris {
    let mut out = Tris::new();
    let bare = st.mark_tile == "none";
    if !bare {
        let tile = if st.mark_tile == "accent" { accent } else { crate::theme::parse_hex(&st.mark_tile) };
        // Ocupa todo el cuadro: la cara va al mismo tamaño que sin placa y no debe salirse.
        out.round_rect((32.0, 32.0), 64.0, 64.0, 15.0, tile);
    }
    // La cara tiene siempre el mismo tamaño: la placa va detrás, no la achica.
    let scale = 1.25;
    let head = crate::theme::parse_hex(&st.mark_head);
    let (shape, eye_y) = turned(&st.mark_shape, spin);

    // Giro 2.5D, como una cabeza de juguete sólida: hacia donde miran los ojos, gira la cabeza.
    let yaw = (eye.x / 3.4).clamp(-1.0, 1.0);
    let pitch = (eye.y / 2.6).clamp(-1.0, 1.0);
    let (ay, ap) = (yaw.abs(), pitch.abs());
    let (sx, sy) = (1.0 - 0.18 * ay, 1.0 - 0.11 * ap);
    let (fx, fy) = (yaw * 1.8, pitch * 1.3);
    // Unidades de la cara -> unidades del cuadro (agrandada alrededor del centro, con o sin placa).
    let face = |x: f32, y: f32, ox: f32, oy: f32| {
        let (x, y) = (32.0 + (x - 32.0) * sx + ox, 32.0 + (y - 32.0) * sy + oy);
        (32.0 + (x - 32.0) * scale, 32.0 + (y - 32.0) * scale)
    };

    // Cuerpo detrás (la extrusión que da volumen).
    let back = (-yaw * 4.4, -pitch * 3.2 + 1.8 * (1.0 - ap));
    let side = shade(head, 0.62);
    for i in (1..=5).rev() {
        let k = i as f32 / 5.0;
        let col = if i == 5 { shade(head, 0.5) } else { side };
        let (ox, oy) = (fx + back.0 * k, fy + back.1 * k);
        let pts: Vec<(f32, f32)> = shape.iter().map(|&(x, y)| face(x, y, ox, oy)).collect();
        out.fan(&pts, face(32.0, 32.0, ox, oy), col);
    }

    // La cara en anillos concéntricos: cada vértice lleva su color con la luz y la sombra ya mezcladas,
    // así el brillo queda suave y recortado exactamente a la cabeza (el degradado radial de .NET).
    let base = [head.r() as f32, head.g() as f32, head.b() as f32];
    let (lx, ly) = (24.0 - fx * 2.2, eye_y - 12.0 - fy * 2.0);
    let far_a = 95.0 / 255.0 * ay.max(ap);
    let (fs, fe) = ((32.0 - yaw * 26.0, eye_y - pitch * 22.0), (32.0 + yaw * 4.0, eye_y + pitch * 4.0));
    let fd = ((fe.0 - fs.0).powi(2) + (fe.1 - fs.1).powi(2)).max(1e-3);
    let lit = |x: f32, y: f32| -> Color32 {
        let d = (((x - lx) / 24.0).powi(2) + ((y - ly) / 20.0).powi(2)).sqrt();
        let mut c = over(base, [255.0; 3], (84.0 / 255.0) * (1.0 - d).max(0.0));
        if far_a > 0.0 {
            let t = (((x - fs.0) * (fe.0 - fs.0) + (y - fs.1) * (fe.1 - fs.1)) / fd).clamp(0.0, 1.0);
            c = over(c, [0.0; 3], far_a * (1.0 - t));
        }
        Color32::from_rgb(c[0].round() as u8, c[1].round() as u8, c[2].round() as u8)
    };
    const RINGS: u32 = 10;
    let n = shape.len() as u32;
    let b = out.verts.len() as u32;
    let (cx, cy) = face(32.0, 32.0, fx, fy);
    out.verts.push((cx, cy, lit(32.0, 32.0)));
    for ring in 1..=RINGS {
        let t = ring as f32 / RINGS as f32;
        for &(x, y) in &shape {
            let (x, y) = (32.0 + (x - 32.0) * t, 32.0 + (y - 32.0) * t);
            let (px, py) = face(x, y, fx, fy);
            out.verts.push((px, py, lit(x, y)));
        }
    }
    for i in 0..n {
        out.idx.extend([b, b + 1 + i, b + 1 + (i + 1) % n]);
    }
    for ring in 1..RINGS {
        let (a0, b0) = (b + 1 + (ring - 1) * n, b + 1 + ring * n);
        for i in 0..n {
            let j = (i + 1) % n;
            out.idx.extend([a0 + i, b0 + i, b0 + j, a0 + i, b0 + j, a0 + j]);
        }
    }

    // Ojos: blancos en cabezas oscuras, tinta en las claras. Van dentro de la cara (siguen su corrimiento)
    // y además se deslizan más que el contorno (paralaje); el del lado lejano es un poco más chico.
    let lum = (0.2126 * head.r() as f32 + 0.7152 * head.g() as f32 + 0.0722 * head.b() as f32) / 255.0;
    let ink = match st.mark_eye_color.as_str() {
        "" | "auto" => if lum > 0.62 { Color32::from_rgb(0x0E, 0x10, 0x15) } else { Color32::from_rgb(0xF4, 0xF4, 0xF5) },
        "accent" => accent,
        h => crate::theme::parse_hex(h),
    };
    let gap = if tiny { 7.0 } else { 6.5 } * (1.0 - 0.28 * ay);
    let (ex, ey) = (32.0 + yaw * 3.8, eye_y + pitch * 3.0);
    let eyes = [(ex - gap, blink.0, 1.0 - 0.16 * yaw.max(0.0)), (ex + gap, blink.1, 1.0 - 0.16 * (-yaw).max(0.0))];
    for (x, open, far) in eyes {
        let c = face(x, ey, fx, fy);
        let k = if tiny { 1.2 } else { 1.0 } * scale * far;
        let open = open.clamp(0.08, 1.0);
        match st.mark_eyes.as_str() {
            "round" => out.ellipse(c, 4.6 * k, 4.6 * k * open, ink),
            "pill" => {
                let (w, h) = (5.2 * k, 11.5 * k * open);
                out.round_rect(c, w, h, w.min(h) / 2.0, ink);
            }
            "line" => out.round_rect(c, 9.2 * k, 3.2 * k * open, 1.6 * k * open, ink),
            "pixel" => out.round_rect(c, 7.0 * k, 7.0 * k * open, 1.2, ink),
            "happy" => {
                let (a, b) = ((c.0 - 4.2 * k, c.1 + 2.0 * k), (c.0 + 4.2 * k, c.1 + 2.0 * k));
                let ctrl = (c.0, c.1 - 5.5 * k * open);
                let pts: Vec<(f32, f32)> = (0..=10)
                    .map(|i| {
                        let t = i as f32 / 10.0;
                        let u = 1.0 - t;
                        (u * u * a.0 + 2.0 * u * t * ctrl.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * ctrl.1 + t * t * b.1)
                    })
                    .collect();
                out.thick_line(&pts, 3.2 * k, ink);
            }
            _ => out.ellipse(c, 3.6 * k, 5.4 * k * open, ink),
        }
    }
    out
}

/// Pinta el logo en pantalla.
pub fn draw_full(p: &egui::Painter, rect: Rect, accent: Color32, st: &Style, eye: Vec2, blink: (f32, f32), spin: f32) {
    let size = rect.width().min(rect.height());
    let s = size / 64.0;
    let origin = rect.center() - Vec2::splat(size / 2.0);
    let tris = build(accent, st, eye, blink, spin, size <= 40.0);
    let mut mesh = egui::Mesh::default();
    for (x, y, c) in &tris.verts {
        mesh.colored_vertex(Pos2::new(origin.x + x * s, origin.y + y * s), *c);
    }
    mesh.indices = tris.idx;
    p.add(Shape::mesh(mesh));
}

// ------------------------------------------------------------------ logo vivo

#[derive(Clone)]
struct Anim {
    eye: Vec2,
    target: Vec2,
    blink_start: f64,
    blink_kind: u8,
    next_blink: f64,
    spin_start: f64,
    next_spin: f64,
    next_glance: f64,
    seed: u32,
}

impl Anim {
    fn rand(&mut self) -> f64 {
        // xorshift: alcanza para parpadeos.
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f64) / (u32::MAX as f64)
    }
}

/// Apertura del ojo durante un parpadeo de una unidad: se cierra y se vuelve a abrir.
fn open_at(t: f64) -> f32 {
    if !(0.0..1.0).contains(&t) {
        1.0
    } else {
        (1.0 - 2.0 * t).abs() as f32
    }
}

/// El logo vivo: con `follow` los ojos siguen al cursor en toda la pantalla (aunque esté fuera de la
/// ventana); sin él miran alrededor. Parpadea y de vez en cuando da una vuelta; un clic lo hace parpadear.
/// Solo pide repintar mientras algo se mueve o cuando toca el próximo parpadeo: al cerrar la ventana
/// no queda nada corriendo.
pub fn live(ui: &mut egui::Ui, size: f32, accent: Color32, follow: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    live_at(ui, rect, &resp, accent, follow, &current());
    resp
}

/// El logo "trabajando" para las esperas (IA, descargas): la cabeza rueda mirando en círculo como quien
/// piensa, rebota apenas sobre su sombra, parpadea y cada tanto da una vuelta entera. Corre a 60 fps solo
/// mientras está en pantalla; sin animaciones (Personalizar) queda quieto.
pub fn working(ui: &mut egui::Ui, size: f32, accent: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    working_at(ui.painter(), ui.ctx(), rect, accent);
    resp
}

pub fn working_at(p: &egui::Painter, ctx: &egui::Context, rect: Rect, accent: Color32) {
    let st = current();
    if !crate::gui::appearance().boberto_motion {
        draw_full(p, rect, accent, &st, Vec2::ZERO, (1.0, 1.0), 0.0);
        return;
    }
    let t = ctx.input(|i| i.time) as f32;
    // Ciclo de 3,6 s: 2,6 s mirando en círculo (la cabeza gira en 2.5D con los ojos) y 1 s de vuelta entera.
    let cyc = 3.6;
    let k = t % cyc;
    let roll = t * 2.6;
    let fade_in = (k / 0.35).min(1.0);
    let mut eye = Vec2::new(roll.cos() * 3.4, roll.sin() * 2.4) * fade_in;
    let mut spin = 0.0;
    if k > 2.6 {
        let u = ((k - 2.6) / 1.0).min(1.0);
        let e = if u < 0.5 { 4.0 * u * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(3) / 2.0 };
        spin = 360.0 * e;
        eye *= 1.0 - (u * 3.0).min(1.0);
    }
    // Parpadeo a mitad de la ronda.
    let b = (k - 1.3) / 0.16;
    let blink = if (0.0..1.0).contains(&b) { (1.0 - 2.0 * b).abs() } else { 1.0 };
    // Rebote suave sobre una sombra que se achica cuando sube.
    let hop = ((t * std::f32::consts::TAU / 0.9).sin() * 0.5 + 0.5).powf(1.6);
    let lift = rect.height() * 0.07 * hop;
    let shadow_c = egui::pos2(rect.center().x, rect.max.y - rect.height() * 0.02);
    let sw = rect.width() * (0.34 - 0.08 * hop);
    p.add(Shape::ellipse_filled(shadow_c, Vec2::new(sw, rect.height() * 0.05), crate::theme::alpha(Color32::BLACK, 0.28 - 0.12 * hop)));
    let face = Rect::from_center_size(rect.center() - Vec2::new(0.0, lift + rect.height() * 0.04), rect.size() * 0.92);
    draw_full(p, face, accent, &st, eye, (blink, blink), spin);
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
}

/// Hace que el logo vivo de `resp_id` dé una vuelta ya (clic en la marca BUZZPRINT).
pub fn spin_now(ctx: &egui::Context, resp_id: egui::Id) {
    let id = resp_id.with("mark-anim");
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| {
        if let Some(mut a) = d.get_temp::<Anim>(id) {
            if a.spin_start < 0.0 {
                a.spin_start = now;
                d.insert_temp(id, a);
            }
        }
    });
    ctx.request_repaint();
}

pub fn live_at(ui: &egui::Ui, rect: Rect, resp: &egui::Response, accent: Color32, follow: bool, st: &Style) {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let id = resp.id.with("mark-anim");
    if !crate::gui::appearance().boberto_motion {
        draw_full(ui.painter(), rect, accent, st, Vec2::ZERO, (1.0, 1.0), 0.0);
        return;
    }
    let mut a = ctx.data_mut(|d| d.get_temp::<Anim>(id)).unwrap_or_else(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
        let seed = ((rect.min.x as u32).wrapping_mul(2654435761) ^ nanos) | 1;
        Anim { eye: Vec2::ZERO, target: Vec2::ZERO, blink_start: -1.0, blink_kind: 0, next_blink: now + 2.0, spin_start: -1.0, next_spin: now + 3.0 + (seed % 6000) as f64 / 1000.0, next_glance: now + 0.6, seed }
    });
    // Ojos: hacia el cursor (en cualquier lugar de la pantalla) o, si no sigue, miradas al azar.
    let cursor = if follow { crate::gui::host::global_cursor() } else { None };
    if let Some(p) = cursor {
        let c = Pos2::new(rect.center().x, rect.top() + rect.height() * 0.48);
        let d = p - c;
        let len = d.length();
        let t = if len < 1.0 { Vec2::ZERO } else { d / len * (len / 220.0).min(1.0) };
        a.target = Vec2::new(t.x * 3.4, t.y * 2.6);
    } else if now >= a.next_glance {
        let r = a.rand();
        a.target = if r < 0.3 { Vec2::ZERO } else { Vec2::new((a.rand() as f32 * 2.0 - 1.0) * 3.4, (a.rand() as f32 * 2.0 - 1.0) * 2.2) };
        a.next_glance = now + 1.4 + a.rand() * 2.6;
    }
    let mut busy = false;
    let next = a.eye + (a.target - a.eye) * 0.2;
    if (next - a.eye).length_sq() > 0.0004 {
        a.eye = next;
        busy = true;
    } else {
        a.eye = a.target;
    }
    // Parpadeo: los dos, alternado o doble. Un clic también.
    if resp.clicked() {
        a.blink_start = now;
        a.blink_kind = (a.rand() * 3.0) as u8;
    }
    if a.blink_start < 0.0 && now >= a.next_blink {
        a.blink_start = now;
        a.blink_kind = if a.rand() < 0.15 { 2 } else { 0 };
    }
    let mut blink = (1.0, 1.0);
    if a.blink_start >= 0.0 {
        busy = true;
        let t = (now - a.blink_start) / 0.15;
        let length = match a.blink_kind {
            1 => {
                blink = (open_at(t), open_at(t - 0.6));
                1.6
            }
            2 => {
                let o = open_at(t).min(open_at(t - 1.3));
                blink = (o, o);
                2.3
            }
            _ => {
                blink = (open_at(t), open_at(t));
                1.0
            }
        };
        if t >= length {
            a.blink_start = -1.0;
            blink = (1.0, 1.0);
            a.next_blink = now + 2.5 + a.rand() * 3.5;
        }
    }
    // Una vuelta de vez en cuando: arranca lento, gira rápido y frena.
    let mut spin = 0.0;
    if a.spin_start < 0.0 && now >= a.next_spin {
        a.spin_start = now;
    }
    if a.spin_start >= 0.0 {
        busy = true;
        let t = ((now - a.spin_start) / 1.15).min(1.0);
        let e = if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 };
        spin = 360.0 * e as f32;
        if t >= 1.0 {
            spin = 0.0;
            a.spin_start = -1.0;
            a.next_spin = now + 8.0 + a.rand() * 7.0;
        }
    }
    if std::env::var_os("BUZZPRINT_EYES_LOG").is_some() {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let line = format!("{ms} {now:.3} rect={:?} cursor={cursor:?} target={:?} eye={:?} busy={busy}\n", rect.center(), a.target, a.eye);
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("buzzprint-eyes.log")).and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
    draw_full(ui.painter(), rect, accent, st, a.eye, blink, spin);
    let wait = if busy {
        0.016
    } else {
        let mut w = a.next_blink.min(a.next_spin) - now;
        if !follow {
            w = w.min(a.next_glance - now);
        }
        w.max(0.016)
    };
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
    ctx.data_mut(|d| d.insert_temp(id, a));
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_shape_has_a_sensible_outline() {
        for s in super::SHAPES {
            let (pts, eye_y) = super::outline(s);
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (x, y) in &pts {
                x0 = x0.min(*x);
                y0 = y0.min(*y);
                x1 = x1.max(*x);
                y1 = y1.max(*y);
            }
            assert!(pts.len() >= 24, "{s}: pocos puntos");
            assert!(x1 - x0 > 28.0 && x1 - x0 < 49.0, "{s}: ancho {}", x1 - x0);
            assert!(y1 - y0 > 28.0 && y1 - y0 < 49.0, "{s}: alto {}", y1 - y0);
            assert!(x0 > 4.0 && y0 > 4.0 && x1 < 60.0 && y1 < 60.0, "{s}: se sale del cuadro");
            assert!(eye_y > y0 + 8.0 && eye_y < y1 - 8.0, "{s}: ojos fuera de la cabeza");
        }
    }
}
