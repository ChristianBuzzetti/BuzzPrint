"""Genera los fondos de Boberto de BuzzPrint: escenas planas hechas con formas simples y SOLO colores del
tema (acento, texto, superficies), así combinan con cualquier paleta y acento.

Salida (un solo catálogo para los dos motores):
  assets/boberto/fondos-app.json  -> lo lee la banda nativa (src/fondos.rs, include_str!)
  assets/boberto/fondos-app.js    -> lo lee el motor web (Boberto Studio) antes de boberto.js

Caja de 240 x 135 (la misma del motor web), el piso cerca de y = 118 y el tercio del medio (x 82..158) libre
para Boberto. Formas (listas JSON):
  ["r", x, y, w, h, rx, color, alfa]               rectángulo
  ["s", x, y, w, h, rx, ancho, color, alfa]        rectángulo de borde
  ["c", cx, cy, r, color, alfa]                    círculo
  ["o", cx, cy, r, ancho, color, alfa]             aro
  ["l", x1, y1, x2, y2, ancho, color, alfa]        línea
  ["p", [x, y, ...], color, alfa]                  polígono CONVEXO
  ["g", x, y, w, h, color1, alfa1, color2, alfa2]  degradé vertical
  ["rg", cx, cy, rx, ry, color, alfa]              halo radial (alfa en el centro, 0 en el borde)
Colores: accent, ink, muted, dim, canvas, header, surface, raised.

Uso:  python tools/fondos.py
"""
import json
import math
import os
import random

W, H = 240, 135
FLOOR = 118
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def r(x, y, w, h, col, a=1.0, rx=0):
    return ["r", round(x, 2), round(y, 2), round(w, 2), round(h, 2), rx, col, a]


def s(x, y, w, h, col, a=1.0, width=1.0, rx=0):
    return ["s", round(x, 2), round(y, 2), round(w, 2), round(h, 2), rx, width, col, a]


def c(cx, cy, rad, col, a=1.0):
    return ["c", round(cx, 2), round(cy, 2), round(rad, 2), col, a]


def o(cx, cy, rad, col, a=1.0, width=1.0):
    return ["o", round(cx, 2), round(cy, 2), round(rad, 2), width, col, a]


def l(x1, y1, x2, y2, col, a=1.0, width=1.0):
    return ["l", round(x1, 2), round(y1, 2), round(x2, 2), round(y2, 2), width, col, a]


def p(pts, col, a=1.0):
    return ["p", [round(v, 2) for v in pts], col, a]


def g(x, y, w, h, c1, a1, c2, a2):
    return ["g", x, y, w, h, c1, a1, c2, a2]


def rg(cx, cy, rx, ry, col, a):
    return ["rg", cx, cy, rx, ry, col, a]


def dashed(x1, y1, x2, y2, col, a, width=1.0, dash=4, gap=3):
    out = []
    length = math.hypot(x2 - x1, y2 - y1)
    if length == 0:
        return out
    ux, uy = (x2 - x1) / length, (y2 - y1) / length
    t = 0.0
    while t < length:
        e = min(t + dash, length)
        out.append(l(x1 + ux * t, y1 + uy * t, x1 + ux * e, y1 + uy * e, col, a, width))
        t += dash + gap
    return out


def room(top_col="surface", wall_a=0.9):
    """Pared con un degradé suave y el piso: la base de las escenas de interior."""
    return [
        g(0, 0, W, FLOOR, "header", 1.0, "surface", 1.0),
        r(0, FLOOR, W, H - FLOOR, "canvas", 1.0),
        l(0, FLOOR, W, FLOOR, "ink", 0.10, 1),
    ]


# ------------------------------------------------------------------ escenas

def window_card(x, y, w, h, accent_bar=False, lines=3, seed=0):
    """Ventana de app en miniatura: barra de título, tres puntos y renglones."""
    rnd = random.Random(seed)
    out = [
        r(x + 1.5, y + 2.5, w, h, "canvas", 0.55, 3),
        r(x, y, w, h, "raised", 1.0, 3),
        s(x, y, w, h, "ink", 0.14, 0.7, 3),
        r(x, y, w, 7, "header", 1.0, 3),
        r(x, y + 4, w, 3, "header", 1.0),
    ]
    for i, col in enumerate(["accent", "muted", "muted"]):
        out.append(c(x + 4.5 + i * 4, y + 3.5, 1.2, col, 0.9 if i == 0 else 0.5))
    if accent_bar:
        out.append(r(x + 4, y + 11, w * 0.45, 3, "accent", 0.9, 1.5))
    yy = y + (17 if accent_bar else 12)
    for _ in range(lines):
        if yy > y + h - 4:
            break
        out.append(r(x + 4, yy, (w - 8) * rnd.uniform(0.45, 0.95), 2, "ink", 0.22, 1))
        yy += 5
    return out


def scene_ventanas():
    out = room()
    out += [rg(120, 112, 120, 40, "accent", 0.16)]
    out += window_card(8, 30, 62, 44, True, 4, 1)
    out += window_card(30, 62, 50, 40, False, 4, 2)
    out += window_card(170, 22, 60, 40, False, 4, 3)
    out += window_card(160, 58, 66, 46, True, 4, 4)
    # Selección de captura punteada alrededor de una ventana, con su medida.
    x0, y0, x1, y1 = 156, 54, 230, 108
    for seg in [(x0, y0, x1, y0), (x1, y0, x1, y1), (x1, y1, x0, y1), (x0, y1, x0, y0)]:
        out += dashed(*seg, "accent", 0.95, 0.9)
    for (cx, cy) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]:
        out.append(r(cx - 1.6, cy - 1.6, 3.2, 3.2, "accent", 1.0))
    out.append(r(x0, y0 - 9, 30, 7, "accent", 1.0, 2))
    out.append(r(x0 + 3, y0 - 6.5, 24, 2, "header", 0.85, 1))
    return out


def scene_escritorio():
    out = room()
    out += [rg(120, 110, 110, 34, "accent", 0.14)]
    # Mesa.
    out.append(r(0, 100, W, 4, "raised", 1.0))
    out.append(l(0, 100, W, 100, "ink", 0.18, 0.8))
    for (mx, mw, mh, seed) in [(10, 66, 42, 11), (166, 64, 40, 12)]:
        top = 100 - 14 - mh
        out.append(r(mx, top, mw, mh, "canvas", 1.0, 2.5))
        out.append(s(mx, top, mw, mh, "ink", 0.2, 0.9, 2.5))
        out += window_card(mx + 4, top + 4, mw - 8, mh - 8, seed % 2 == 1, 4, seed)
        # Pie del monitor.
        out.append(r(mx + mw / 2 - 2.5, top + mh, 5, 10, "raised", 1.0))
        out.append(r(mx + mw / 2 - 12, 98, 24, 2.5, "raised", 1.0, 1))
    # Taza y lámpara.
    out.append(r(84, 90, 8, 10, "raised", 1.0, 1.5))
    out.append(o(93, 95, 2.5, "raised", 1.0, 1.4))
    out.append(l(150, 100, 156, 72, "muted", 0.6, 1.2))
    out.append(l(156, 72, 146, 66, "muted", 0.6, 1.2))
    out.append(p([140, 66, 152, 66, 149, 61, 143, 61], "accent", 0.95))
    out.append(rg(146, 72, 16, 14, "accent", 0.35))
    return out


def scene_recorte():
    out = [g(0, 0, W, H, "header", 1.0, "canvas", 1.0)]
    # Grilla fina de guías.
    for x in range(0, W + 1, 12):
        out.append(l(x, 0, x, H, "ink", 0.05, 0.5))
    for y in range(0, H + 1, 12):
        out.append(l(0, y, W, y, "ink", 0.05, 0.5))
    # Reglas arriba e izquierda.
    out.append(r(0, 0, W, 8, "raised", 0.95))
    out.append(r(0, 0, 8, H, "raised", 0.95))
    for x in range(8, W, 6):
        hgt = 5 if (x - 8) % 30 == 0 else 2.5
        out.append(l(x, 8 - hgt, x, 8, "muted", 0.7, 0.5))
    for y in range(8, H, 6):
        wid = 5 if (y - 8) % 30 == 0 else 2.5
        out.append(l(8 - wid, y, 8, y, "muted", 0.7, 0.5))
    # Esquinas de recorte grandes alrededor del centro, guías de tercios.
    x0, y0, x1, y1 = 62, 22, 178, 116
    k = 12
    for (cx, cy, dx, dy) in [(x0, y0, 1, 1), (x1, y0, -1, 1), (x0, y1, 1, -1), (x1, y1, -1, -1)]:
        out.append(l(cx, cy, cx + dx * k, cy, "accent", 1.0, 2.2))
        out.append(l(cx, cy, cx, cy + dy * k, "accent", 1.0, 2.2))
    # Etiqueta de medida y mira.
    out.append(r(x0, y0 - 11, 40, 8, "accent", 1.0, 2))
    out.append(r(x0 + 4, y0 - 8, 32, 2, "header", 0.85, 1))
    for (mx, my) in [(32, 70), (208, 52)]:
        out.append(o(mx, my, 7, "muted", 0.55, 0.8))
        out.append(l(mx - 11, my, mx + 11, my, "muted", 0.55, 0.6))
        out.append(l(mx, my - 11, mx, my + 11, "muted", 0.55, 0.6))
        out.append(c(mx, my, 1.3, "accent", 1.0))
    return out


def scene_plano():
    out = [g(0, 0, W, H, "header", 1.0, "surface", 1.0)]
    for x in range(0, W + 1, 8):
        out.append(l(x, 0, x, H, "accent", 0.06 if x % 40 else 0.14, 0.5))
    for y in range(-1, H + 1, 8):
        out.append(l(0, y, W, y, "accent", 0.06 if (y + 1) % 40 else 0.14, 0.5))
    # Piezas técnicas a los costados.
    out.append(o(40, 62, 24, "accent", 0.55, 1))
    out.append(o(40, 62, 14, "accent", 0.4, 0.8))
    out.append(l(10, 62, 70, 62, "accent", 0.45, 0.6))
    out.append(l(40, 32, 40, 92, "accent", 0.45, 0.6))
    out.append(s(16, 96, 48, 14, "accent", 0.5, 0.8, 1))
    out += dashed(16, 112, 64, 112, "accent", 0.5, 0.6, 2, 2)
    out.append(s(176, 26, 50, 34, "accent", 0.55, 1, 1))
    out.append(l(176, 26, 226, 60, "accent", 0.3, 0.6))
    out.append(l(226, 26, 176, 60, "accent", 0.3, 0.6))
    out.append(o(200, 88, 16, "accent", 0.5, 0.9))
    for k in range(12):
        a = k * math.pi / 6
        out.append(l(200 + 16 * math.cos(a), 88 + 16 * math.sin(a), 200 + 20 * math.cos(a), 88 + 20 * math.sin(a), "accent", 0.5, 0.7))
    out.append(r(178, 112, 46, 8, "accent", 0.18, 1))
    out.append(r(181, 115, 28, 2, "accent", 0.7, 1))
    return out


def scene_galeria():
    out = room()
    # Conos de luz desde el techo.
    for (x, w) in [(36, 46), (204, 46)]:
        out.append(p([x - 4, 0, x + 4, 0, x + w / 2, 92, x - w / 2, 92], "accent", 0.07))
        out.append(r(x - 6, 0, 12, 3, "raised", 1.0, 1))
    # Cuadros colgados (capturas enmarcadas).
    frames = [(16, 26, 42, 30, 21), (60, 40, 16, 22, 22), (166, 34, 20, 26, 23), (190, 22, 40, 34, 24)]
    for (x, y, w, h, seed) in frames:
        out.append(r(x + 1.5, y + 2, w, h, "canvas", 0.5, 1))
        out.append(r(x, y, w, h, "raised", 1.0, 1))
        out.append(s(x, y, w, h, "ink", 0.25, 0.9, 1))
        rnd = random.Random(seed)
        out.append(r(x + 3, y + 3, w - 6, h - 6, "header", 1.0))
        out.append(r(x + 3, y + 3, w - 6, 3, "accent", 0.85))
        yy = y + 9
        while yy < y + h - 5:
            out.append(r(x + 5, yy, (w - 10) * rnd.uniform(0.4, 0.95), 1.6, "ink", 0.25, 0.8))
            yy += 4
    # Banco y placas.
    out.append(r(18, 104, 52, 4, "raised", 1.0, 1))
    out.append(r(22, 108, 3, 10, "raised", 1.0))
    out.append(r(63, 108, 3, 10, "raised", 1.0))
    out.append(r(188, 64, 12, 3, "muted", 0.5, 1))
    return out


def scene_cuartooscuro():
    out = [g(0, 0, W, H, "canvas", 1.0, "header", 1.0)]
    out.append(rg(120, 20, 150, 60, "accent", 0.20))
    # Dos cuerdas con fotos colgadas con broches (a los costados).
    for (x0, x1, y, seed) in [(0, 84, 24, 31), (156, 240, 30, 32)]:
        out.append(l(x0, y, x1, y + 4, "muted", 0.55, 0.6))
        rnd = random.Random(seed)
        x = x0 + 6
        while x < x1 - 14:
            w = rnd.uniform(14, 20)
            h = w * rnd.uniform(1.1, 1.35)
            yy = y + (x - x0) / (x1 - x0) * 4
            out.append(r(x, yy + 2, w, h, "ink", 0.85, 0.5))
            out.append(r(x + 1.5, yy + 3.5, w - 3, h - 7, "accent", 0.35))
            out.append(r(x + 1.5, yy + 3.5 + (h - 7) * 0.6, w - 3, (h - 7) * 0.4, "header", 0.5))
            out.append(r(x + w / 2 - 1.2, yy - 1, 2.4, 4.5, "raised", 1.0, 0.6))
            x += w + rnd.uniform(4, 8)
    # Cubetas en la mesa.
    out.append(r(0, 104, W, 14, "raised", 1.0))
    out.append(l(0, 104, W, 104, "ink", 0.14, 0.8))
    for x in (14, 44, 172, 202):
        out.append(r(x, 98, 26, 7, "surface", 1.0, 1.5))
        out.append(r(x + 3, 99.5, 20, 2.5, "accent", 0.35, 1))
    out.append(r(0, 118, W, 17, "canvas", 1.0))
    return out


def scene_terminal():
    out = [g(0, 0, W, H, "canvas", 1.0, "header", 1.0)]
    rnd = random.Random(41)
    for (x0, x1) in [(8, 78), (162, 232)]:
        y = 10
        while y < 108:
            indent = rnd.choice([0, 0, 6, 12])
            parts = rnd.randint(1, 3)
            x = x0 + indent
            for k in range(parts):
                w = rnd.uniform(6, 22)
                if x + w > x1:
                    break
                col = rnd.choice(["accent", "ink", "ink", "muted"])
                out.append(r(x, y, w, 2.6, col, 0.75 if col == "accent" else 0.32, 1))
                x += w + 3
            y += 6.5
    # Prompt con cursor.
    out.append(r(8, 112, 5, 2.6, "accent", 1.0, 1))
    out.append(r(16, 112, 30, 2.6, "ink", 0.4, 1))
    out.append(r(49, 110.5, 4, 5.5, "accent", 1.0))
    out.append(g(0, 0, W, 26, "canvas", 0.9, "canvas", 0.0))
    return out


def scene_biblioteca():
    out = [g(0, 0, W, H, "header", 1.0, "canvas", 1.0)]
    rnd = random.Random(51)
    cw, ch, gap = 22, 16, 4
    for row in range(6):
        for col in range(9):
            x = 4 + col * (cw + gap)
            y = 6 + row * (ch + gap + 2)
            cx = x + cw / 2
            dist = abs(cx - 120)
            if dist < 64:
                continue
            fade = min(1.0, (dist - 64) / 40)
            a = 0.25 + 0.75 * fade
            on = rnd.random() < 0.12
            out.append(r(x, y, cw, ch, "raised", a, 2))
            out.append(s(x, y, cw, ch, "accent" if on else "ink", (0.9 if on else 0.14) * a, 0.8 if on else 0.6, 2))
            out.append(r(x + 2, y + 2, cw - 4, ch - 7, "header", a, 1))
            out.append(r(x + 3, y + 3, (cw - 6) * rnd.uniform(0.3, 0.8), 1.6, "accent" if on else "muted", 0.8 * a, 0.8))
            out.append(r(x + 2, y + ch - 4, (cw - 4) * rnd.uniform(0.5, 0.9), 1.4, "ink", 0.35 * a, 0.7))
    return out


def scene_horizonte():
    out = [g(0, 0, W, 86, "canvas", 1.0, "surface", 1.0), r(0, 86, W, H - 86, "canvas", 1.0)]
    # Sol de bandas del acento a la izquierda (el centro es de Boberto).
    sx, sy, sr = 46, 54, 22
    out.append(rg(sx, sy + 6, 46, 30, "accent", 0.22))
    out.append(c(sx, sy, sr, "accent", 0.85))
    for k, y in enumerate([sy + 2, sy + 7, sy + 12, sy + 17]):
        out.append(r(sx - sr - 1, y, 2 * sr + 2, 1.0 + k * 0.6, "canvas", 1.0))
    # Luna chica a la derecha.
    out.append(c(204, 30, 6, "ink", 0.55))
    out.append(c(207, 28, 6, "surface", 1.0))
    out.append(r(0, 86, W, 49, "canvas", 1.0))
    # Montes bajos a los costados.
    out.append(p([0, 86, 0, 72, 24, 64, 52, 76, 66, 86], "header", 1.0))
    out.append(p([174, 86, 196, 68, 222, 60, 240, 68, 240, 86], "header", 1.0))
    # Piso en perspectiva solo a los costados: el medio queda libre para Boberto.
    for k in list(range(-12, -4)) + list(range(5, 13)):
        out.append(l(120 + k * 6, 86, 120 + k * 34, H, "accent", 0.32, 0.6))
    y = 87.0
    step = 2.0
    while y < H:
        out.append(l(0, y, 66, y, "accent", 0.32, 0.6))
        out.append(l(174, y, W, y, "accent", 0.32, 0.6))
        step *= 1.35
        y += step
    out.append(l(0, 86, 70, 86, "accent", 0.8, 1))
    out.append(l(170, 86, W, 86, "accent", 0.8, 1))
    return out


def scene_pixeles():
    out = [g(0, 0, W, H, "header", 1.0, "canvas", 1.0)]
    rnd = random.Random(61)
    size = 6
    for gx in range(0, W, size):
        for gy in range(0, H, size):
            cx = gx + size / 2
            d = abs(cx - 120) / 120
            if d < 0.5:
                continue
            chance = (d - 0.5) * 1.1
            if rnd.random() < chance:
                tone = rnd.random()
                col = "accent" if tone < 0.35 else ("ink" if tone < 0.55 else "raised")
                a = rnd.uniform(0.25, 0.9) if col == "accent" else rnd.uniform(0.06, 0.18) if col == "ink" else 1.0
                out.append(r(gx + 0.4, gy + 0.4, size - 0.8, size - 0.8, col, round(a, 2), 0.8))
    return out


def scene_estudio():
    out = room()
    out.append(rg(120, 112, 56, 16, "accent", 0.16))
    # Softboxes con trípodes.
    for (x, flip) in [(30, 1), (210, -1)]:
        out.append(l(x, 58, x, FLOOR, "muted", 0.7, 1.2))
        out.append(l(x, FLOOR, x - 10, FLOOR, "muted", 0.7, 1))
        out.append(l(x, FLOOR, x + 10, FLOOR, "muted", 0.7, 1))
        out.append(l(x, 104, x - 8, FLOOR, "muted", 0.7, 0.9))
        out.append(l(x, 104, x + 8, FLOOR, "muted", 0.7, 0.9))
        box = [x - 16, 26, x + 16, 26, x + 10, 60, x - 10, 60] if flip else []
        out.append(p([x - 18, 24, x + 18, 24, x + 11, 58, x - 11, 58], "raised", 1.0))
        out.append(p([x - 14, 27, x + 14, 27, x + 9, 54, x - 9, 54], "ink", 0.85))
        out.append(rg(x, 40, 30, 26, "ink", 0.12))
        out.append(p([x + 14 * flip, 30, x + 60 * flip, 76, x + 50 * flip, 96, x + 9 * flip, 52] if flip > 0
                     else [x + 9 * flip, 52, x + 50 * flip, 96, x + 60 * flip, 76, x + 14 * flip, 30], "ink", 0.035))
    # Cámara en trípode chica a la derecha baja.
    return out


def scene_red():
    out = [g(0, 0, W, H, "canvas", 1.0, "header", 1.0)]
    rnd = random.Random(71)
    nodes = []
    for side in (0, 1):
        for _ in range(9):
            x = rnd.uniform(8, 74) if side == 0 else rnd.uniform(166, 232)
            y = rnd.uniform(10, 108)
            nodes.append((x, y, side))
    for i, (x, y, sd) in enumerate(nodes):
        near = sorted(((math.hypot(x - x2, y - y2), j) for j, (x2, y2, s2) in enumerate(nodes) if s2 == sd and j != i))[:2]
        for _, j in near:
            if j > i:
                out.append(l(x, y, nodes[j][0], nodes[j][1], "ink", 0.16, 0.6))
    for i, (x, y, _) in enumerate(nodes):
        hot = i % 4 == 0
        if hot:
            out.append(rg(x, y, 10, 10, "accent", 0.35))
        # Cada nodo es una captura chiquita.
        out.append(r(x - 5, y - 3.5, 10, 7, "raised", 1.0, 1))
        out.append(s(x - 5, y - 3.5, 10, 7, "accent" if hot else "ink", 0.9 if hot else 0.25, 0.6, 1))
        out.append(r(x - 3.5, y - 2, 7 if hot else 5, 1.4, "accent" if hot else "muted", 0.9, 0.6))
    return out


def scene_sobrio():
    """Casi nada: la luz del acento en el piso, un horizonte fino a los costados y unos puntos lejanos."""
    out = [g(0, 0, W, H, "header", 1.0, "canvas", 1.0)]
    out.append(rg(120, 120, 120, 30, "accent", 0.13))
    out.append(l(0, 118, 64, 118, "accent", 0.28, 0.6))
    out.append(l(176, 118, W, 118, "accent", 0.28, 0.6))
    out.append(l(0, 118, 30, 118, "accent", 0.45, 0.8))
    out.append(l(210, 118, W, 118, "accent", 0.45, 0.8))
    rnd = random.Random(81)
    for _ in range(9):
        for x0, x1 in ((6, 52), (188, 234)):
            out.append(c(rnd.uniform(x0, x1), rnd.uniform(8, 80), rnd.uniform(0.4, 0.9), "ink", round(rnd.uniform(0.10, 0.28), 2)))
    out.append(g(10, 0, 1.2, 96, "accent", 0.0, "accent", 0.16))
    out.append(g(229, 0, 1.2, 96, "accent", 0.0, "accent", 0.16))
    return out


def _bbox(sh):
    t = sh[0]
    if t in ("r", "s", "g"):
        return sh[1], sh[1] + sh[3]
    if t in ("c", "o"):
        return sh[1] - sh[3], sh[1] + sh[3]
    if t == "l":
        return min(sh[1], sh[3]), max(sh[1], sh[3])
    if t == "p":
        xs = sh[1][0::2]
        return min(xs), max(xs)
    if t == "rg":
        return sh[1] - sh[3], sh[1] + sh[3]
    return 0, W


def clear_center(shapes, k=0.74):
    """Deja libre el centro (donde está Boberto): lo que vive de un solo lado se achica y se corre hacia su
    borde, apoyado en el piso. Lo que cruza el medio (fondos, pisos, halos) queda igual."""
    out = []
    for sh in shapes:
        x0, x1 = _bbox(sh)
        if x0 < 120 < x1 or sh[0] in ("g",):
            out.append(sh)
            continue
        left = x1 <= 120
        fx = (lambda x: x * k) if left else (lambda x: W - (W - x) * k)
        fy = lambda y: FLOOR - (FLOOR - y) * k
        t = sh[0]
        sh = list(sh)
        if t in ("r", "s"):
            nx0, nx1 = sorted((fx(sh[1]), fx(sh[1] + sh[3])))
            sh[1], sh[2], sh[3], sh[4] = round(nx0, 2), round(fy(sh[2]), 2), round(nx1 - nx0, 2), round(sh[4] * k, 2)
            if t == "s":
                sh[6] = round(sh[6] * k, 2)
        elif t in ("c", "o"):
            sh[1], sh[2], sh[3] = round(fx(sh[1]), 2), round(fy(sh[2]), 2), round(sh[3] * k, 2)
            if t == "o":
                sh[4] = round(sh[4] * k, 2)
        elif t == "l":
            sh[1], sh[2], sh[3], sh[4] = round(fx(sh[1]), 2), round(fy(sh[2]), 2), round(fx(sh[3]), 2), round(fy(sh[4]), 2)
        elif t == "p":
            pts = sh[1]
            sh[1] = [round(fx(v), 2) if i % 2 == 0 else round(fy(v), 2) for i, v in enumerate(pts)]
        elif t == "rg":
            sh[1], sh[2], sh[3], sh[4] = round(fx(sh[1]), 2), round(fy(sh[2]), 2), round(sh[3] * k, 2), round(sh[4] * k, 2)
        out.append(sh)
    return out


SCENES = [
    ("bp-sobrio", "", "Sobrio", "Solo la luz del acento en el piso y un horizonte fino a los costados.", scene_sobrio),
    ("bp-apps", "trabajo", "Ventanas", "Ventanas de apps a los costados y una selección de captura punteada.", scene_ventanas),
    ("bp-escritorio", "trabajo", "Escritorio", "Dos monitores sobre la mesa y una lámpara del acento.", scene_escritorio),
    ("bp-terminal", "trabajo", "Consola", "Renglones de código a los dos lados y el cursor abajo.", scene_terminal),
    ("bp-biblioteca", "trabajo", "Biblioteca", "Una grilla de capturas que se apaga hacia el centro.", scene_biblioteca),
    ("bp-recorte", "taller", "Recorte", "Reglas, miras y las esquinas de recorte alrededor de Boberto.", scene_recorte),
    ("bp-plano", "taller", "Plano", "Cuadrícula técnica y piezas acotadas en el color del acento.", scene_plano),
    ("bp-pixeles", "taller", "Píxeles", "Bloques de píxeles en los bordes, como el pixelado del editor.", scene_pixeles),
    ("bp-galeria", "estudio", "Galería", "Capturas enmarcadas en la pared y conos de luz.", scene_galeria),
    ("bp-estudio", "estudio", "Estudio de fotos", "Dos softboxes que iluminan a Boberto desde los costados.", scene_estudio),
    ("bp-cuartooscuro", "estudio", "Cuarto oscuro", "Fotos colgadas de una cuerda bajo la luz del acento.", scene_cuartooscuro),
    ("bp-horizonte", "ambiente", "Horizonte", "Un sol de bandas a la izquierda y el piso en perspectiva a los costados.", scene_horizonte),
    ("bp-red", "ambiente", "Red", "Capturas conectadas como nodos, a los dos lados.", scene_red),
]

# Escenas que ya dejan el centro libre a su manera (grillas que se abren, perspectiva): no se achican.
KEEP = {"bp-sobrio", "bp-biblioteca", "bp-pixeles", "bp-horizonte", "bp-plano"}

GROUPS = [("trabajo", "Trabajo"), ("taller", "Taller"), ("estudio", "Estudio"), ("ambiente", "Ambiente")]


JS_RENDER = r"""
/* Formas -> SVG. Los colores son variables CSS del tema (las pone BuzzPrint), por eso van en style="". */
window.BZP_FONDO_SVG = (function () {
  var VARS = { accent: '--accent', ink: '--text', muted: '--text-muted', dim: '--text-dim', canvas: '--bg',
               header: '--bg-elev-0', surface: '--bg-elev-1', raised: '--bg-elev-2' };
  function col(name) { return VARS[name] ? 'var(' + VARS[name] + ')' : '#888'; }
  function n(v) { return Math.round(v * 100) / 100; }
  return function (id) {
    var sc = (window.BZP_FONDOS.scenes || []).filter(function (s) { return s.id === id; })[0];
    if (!sc) return '';
    var defs = '', body = '', k = 0;
    sc.shapes.forEach(function (sh) {
      var t = sh[0];
      if (t === 'r') {
        body += '<rect x="' + n(sh[1]) + '" y="' + n(sh[2]) + '" width="' + n(sh[3]) + '" height="' + n(sh[4]) + '" rx="' + sh[5] + '" style="fill:' + col(sh[6]) + ';fill-opacity:' + sh[7] + '"/>';
      } else if (t === 's') {
        body += '<rect x="' + n(sh[1]) + '" y="' + n(sh[2]) + '" width="' + n(sh[3]) + '" height="' + n(sh[4]) + '" rx="' + sh[5] + '" style="fill:none;stroke:' + col(sh[7]) + ';stroke-width:' + sh[6] + ';stroke-opacity:' + sh[8] + '"/>';
      } else if (t === 'c') {
        body += '<circle cx="' + n(sh[1]) + '" cy="' + n(sh[2]) + '" r="' + n(sh[3]) + '" style="fill:' + col(sh[4]) + ';fill-opacity:' + sh[5] + '"/>';
      } else if (t === 'o') {
        body += '<circle cx="' + n(sh[1]) + '" cy="' + n(sh[2]) + '" r="' + n(sh[3]) + '" style="fill:none;stroke:' + col(sh[5]) + ';stroke-width:' + sh[4] + ';stroke-opacity:' + sh[6] + '"/>';
      } else if (t === 'l') {
        body += '<line x1="' + n(sh[1]) + '" y1="' + n(sh[2]) + '" x2="' + n(sh[3]) + '" y2="' + n(sh[4]) + '" style="stroke:' + col(sh[6]) + ';stroke-width:' + sh[5] + ';stroke-opacity:' + sh[7] + ';stroke-linecap:round"/>';
      } else if (t === 'p') {
        var pts = [];
        for (var i = 0; i < sh[1].length; i += 2) pts.push(n(sh[1][i]) + ',' + n(sh[1][i + 1]));
        body += '<polygon points="' + pts.join(' ') + '" style="fill:' + col(sh[2]) + ';fill-opacity:' + sh[3] + '"/>';
      } else if (t === 'g') {
        var gid = 'bzpg-' + id + '-' + (k++);
        defs += '<linearGradient id="' + gid + '" x1="0" y1="0" x2="0" y2="1"><stop offset="0" style="stop-color:' + col(sh[5]) + ';stop-opacity:' + sh[6] + '"/><stop offset="1" style="stop-color:' + col(sh[7]) + ';stop-opacity:' + sh[8] + '"/></linearGradient>';
        body += '<rect x="' + sh[1] + '" y="' + sh[2] + '" width="' + sh[3] + '" height="' + sh[4] + '" fill="url(#' + gid + ')"/>';
      } else if (t === 'rg') {
        var rid = 'bzpr-' + id + '-' + (k++);
        defs += '<radialGradient id="' + rid + '"><stop offset="0" style="stop-color:' + col(sh[5]) + ';stop-opacity:' + sh[6] + '"/><stop offset="1" style="stop-color:' + col(sh[5]) + ';stop-opacity:0"/></radialGradient>';
        body += '<ellipse cx="' + sh[1] + '" cy="' + sh[2] + '" rx="' + sh[3] + '" ry="' + sh[4] + '" fill="url(#' + rid + ')"/>';
      }
    });
    return (defs ? '<defs>' + defs + '</defs>' : '') + body;
  };
})();
"""


def main():
    data = {
        "w": W,
        "h": H,
        "default": "bp-sobrio",
        "groups": [{"id": gid, "name": name} for gid, name in GROUPS],
        "scenes": [
            {"id": sid, "grupo": grp, "name": name, "desc": desc, "shapes": fn() if sid in KEEP else clear_center(fn())}
            for sid, grp, name, desc, fn in SCENES
        ],
    }
    text = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    out_dir = os.path.join(ROOT, "assets", "boberto")
    with open(os.path.join(out_dir, "fondos-app.json"), "w", encoding="utf-8") as f:
        f.write(text)
    with open(os.path.join(out_dir, "fondos-app.js"), "w", encoding="utf-8") as f:
        f.write("/* Generado por tools/fondos.py: los fondos de Boberto de BuzzPrint (formas + colores del tema). */\n")
        f.write("window.BZP_FONDOS = " + text + ";\n")
        f.write(JS_RENDER)
    total = sum(len(s["shapes"]) for s in data["scenes"])
    print(f"{len(SCENES)} escenas, {total} formas, {len(text) // 1024} KB")


if __name__ == "__main__":
    main()
