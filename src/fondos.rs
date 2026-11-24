//! Fondos de Boberto de BuzzPrint: escenas de formas simples con SOLO colores del tema (acento, texto,
//! superficies), así combinan con cualquier paleta. El catálogo lo genera `tools/fondos.py` y es el mismo que
//! usa el motor web de Boberto Studio (`assets/boberto/fondos-app.js`).

use std::sync::OnceLock;

use egui::{epaint::Vertex, Color32, Mesh, Pos2, Rect, Shape, Stroke, Vec2};
use serde_json::Value;

use crate::theme::{alpha, Colors};

const JSON: &str = include_str!("../assets/boberto/fondos-app.json");

struct Catalog {
    w: f32,
    h: f32,
    default: String,
    scenes: Vec<(String, Vec<Value>)>,
}

fn catalog() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| {
        let v: Value = serde_json::from_str(JSON).unwrap_or(Value::Null);
        let scenes = v["scenes"]
            .as_array()
            .map(|a| a.iter().filter_map(|s| Some((s["id"].as_str()?.to_string(), s["shapes"].as_array()?.clone()))).collect())
            .unwrap_or_default();
        Catalog {
            w: v["w"].as_f64().unwrap_or(240.0) as f32,
            h: v["h"].as_f64().unwrap_or(135.0) as f32,
            default: v["default"].as_str().unwrap_or("ninguno").to_string(),
            scenes,
        }
    })
}

/// ¿Es un fondo del catálogo?
pub fn exists(id: &str) -> bool {
    catalog().scenes.iter().any(|(s, _)| s == id)
}

/// El fondo por defecto (el que reemplaza a las láminas viejas).
pub fn default_id() -> &'static str {
    &catalog().default
}

fn color(c: &Colors, name: &str) -> Color32 {
    match name {
        "accent" => c.accent,
        "ink" => c.ink,
        "muted" => c.muted,
        "dim" => c.line_number,
        "canvas" => c.canvas,
        "header" => c.header,
        "surface" => c.surface,
        "raised" => c.raised,
        _ => Color32::GRAY,
    }
}

/// Dibuja la escena `id` ajustada a `rect`: entera, de borde a borde (sin recortar ni correrse), con `opacity`
/// y un desvanecido abajo hacia `fade_to` (para que se lea la lectura "BOBERTO"). Si la caja no es 16:9 la
/// escena se estira apenas: son formas planas y así siempre se ve completa.
pub fn draw(p: &egui::Painter, rect: Rect, id: &str, c: &Colors, opacity: f32, fade_to: Color32) {
    let cat = catalog();
    let Some((_, shapes)) = cat.scenes.iter().find(|(s, _)| s == id) else { return };
    let (kx, ky) = (rect.width() / cat.w, rect.height() / cat.h);
    // Radios y grosores con la escala menor (un círculo sigue siendo círculo).
    let k = kx.min(ky);
    let pt = |x: f64, y: f64| Pos2::new(rect.min.x + x as f32 * kx, rect.min.y + y as f32 * ky);
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let f = |v: &Value, i: usize| v[i].as_f64().unwrap_or(0.0);
    let col = |v: &Value, i: usize, ai: usize| alpha(color(c, v[i].as_str().unwrap_or("")), (f(v, ai) as f32 * opacity).clamp(0.0, 1.0));
    for sh in shapes {
        match sh[0].as_str().unwrap_or("") {
            "r" => {
                let r = Rect::from_min_max(pt(f(sh, 1), f(sh, 2)), pt(f(sh, 1) + f(sh, 3), f(sh, 2) + f(sh, 4)));
                p.rect_filled(r, (f(sh, 5) as f32 * k).min(255.0), col(sh, 6, 7));
            }
            "s" => {
                let r = Rect::from_min_max(pt(f(sh, 1), f(sh, 2)), pt(f(sh, 1) + f(sh, 3), f(sh, 2) + f(sh, 4)));
                p.rect_stroke(r, (f(sh, 5) as f32 * k).min(255.0), Stroke::new(f(sh, 6) as f32 * k, col(sh, 7, 8)), egui::StrokeKind::Middle);
            }
            "c" => {
                p.circle_filled(pt(f(sh, 1), f(sh, 2)), f(sh, 3) as f32 * k, col(sh, 4, 5));
            }
            "o" => {
                p.circle_stroke(pt(f(sh, 1), f(sh, 2)), f(sh, 3) as f32 * k, Stroke::new(f(sh, 4) as f32 * k, col(sh, 5, 6)));
            }
            "l" => {
                p.line_segment([pt(f(sh, 1), f(sh, 2)), pt(f(sh, 3), f(sh, 4))], Stroke::new(f(sh, 5) as f32 * k, col(sh, 6, 7)));
            }
            "p" => {
                let pts: Vec<Pos2> = sh[1].as_array().map(|a| a.chunks(2).map(|q| pt(q[0].as_f64().unwrap_or(0.0), q.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0))).collect()).unwrap_or_default();
                if pts.len() >= 3 {
                    p.add(Shape::convex_polygon(pts, col(sh, 2, 3), Stroke::NONE));
                }
            }
            "g" => {
                let r = Rect::from_min_max(pt(f(sh, 1), f(sh, 2)), pt(f(sh, 1) + f(sh, 3), f(sh, 2) + f(sh, 4)));
                let (top, bot) = (col(sh, 5, 6), col(sh, 7, 8));
                let mut m = Mesh::default();
                for (q, cc) in [(r.left_top(), top), (r.right_top(), top), (r.left_bottom(), bot), (r.right_bottom(), bot)] {
                    m.vertices.push(Vertex { pos: q, uv: egui::epaint::WHITE_UV, color: cc });
                }
                m.indices = vec![0, 1, 2, 1, 2, 3];
                p.add(Shape::mesh(m));
            }
            "rg" => {
                // Halo radial: abanico de triángulos del color pleno en el centro a transparente en el borde.
                let ctr = pt(f(sh, 1), f(sh, 2));
                let (rx, ry) = (f(sh, 3) as f32 * kx, f(sh, 4) as f32 * ky);
                let inner = col(sh, 5, 6);
                let outer = Color32::TRANSPARENT;
                let mut m = Mesh::default();
                m.vertices.push(Vertex { pos: ctr, uv: egui::epaint::WHITE_UV, color: inner });
                let n = 40;
                for i in 0..=n {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    m.vertices.push(Vertex { pos: ctr + Vec2::new(a.cos() * rx, a.sin() * ry), uv: egui::epaint::WHITE_UV, color: outer });
                    if i > 0 {
                        m.indices.extend([0, i as u32, i as u32 + 1]);
                    }
                }
                p.add(Shape::mesh(m));
            }
            _ => {}
        }
    }
    // Desvanecido inferior (como la máscara 72 % → 45 % de las láminas).
    let y0 = rect.min.y + rect.height() * 0.70;
    let mut m = Mesh::default();
    let bottom = alpha(fade_to, 0.55);
    for (q, cc) in [
        (Pos2::new(rect.min.x, y0), Color32::TRANSPARENT),
        (Pos2::new(rect.max.x, y0), Color32::TRANSPARENT),
        (rect.left_bottom(), bottom),
        (rect.right_bottom(), bottom),
    ] {
        m.vertices.push(Vertex { pos: q, uv: egui::epaint::WHITE_UV, color: cc });
    }
    m.indices = vec![0, 1, 2, 1, 2, 3];
    p.add(Shape::mesh(m));
}
