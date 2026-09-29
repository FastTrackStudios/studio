//! The heads' sculpted fronts, drawn procedurally (a seeded generator, so
//! every build draws the same art):
//!
//! - [`hammered_glass`] — violet hammered glass lit from below by the
//!   tubes (the lead head);
//! - [`relief_white`] — a white plaster relief: moulded frame, a shield
//!   with a bead strand, C-scrolls and acanthus sprays, a haloed cherub at
//!   one end and a horned, bat-winged one at the other (the rhythm head);
//! - [`floral_black`] — a black floral relief: roses, five-petal flowers,
//!   leaves and wheat (the acoustic head);
//! - [`flourish`] — the maker's scribbled mark on a faceplate.

use std::f64::consts::{PI, TAU};

use document_proto::{Node, NodeId};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use crate::reverb::ui;

/// A small deterministic generator (an LCG), so the art is the same on
/// every build.
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }
    /// 0..1.
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1_u64 << 53) as f64)
    }
    /// `lo..hi`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// A closed shape through `pts` (parent coordinates) as a vector whose box
/// is the points' extent — so its gradients light this shape alone.
fn shape(name: &str, pts: &[(f64, f64)], fill: document_proto::Paint) -> Node {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in pts {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let local: Vec<(f64, f64)> = pts.iter().map(|(x, y)| (x - x0, y - y0)).collect();
    draw::path(name, x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0), draw::polygon(&local), fill)
}

/// An ellipse's outline, turned `rot` degrees about its centre.
fn oval(cx: f64, cy: f64, rx: f64, ry: f64, rot: f64) -> Vec<(f64, f64)> {
    let (s, c) = rot.to_radians().sin_cos();
    (0..28)
        .map(|i| {
            let a = TAU * f64::from(i) / 28.0;
            let (x, y) = (rx * a.cos(), ry * a.sin());
            (cx + x * c - y * s, cy + x * s + y * c)
        })
        .collect()
}

/// A pointed leaf from `(x, y)` along `deg` (0 = up), `len` long, `wid` wide.
fn leaf(x: f64, y: f64, len: f64, wid: f64, deg: f64) -> Vec<(f64, f64)> {
    let a = deg.to_radians();
    let (ux, uy) = (a.sin(), -a.cos());
    let (nx, ny) = (-uy, ux);
    let mut pts = Vec::new();
    for i in 0..=12 {
        let t = f64::from(i) / 12.0;
        let half = wid / 2.0 * (PI * t).sin().powf(0.8);
        pts.push((x + ux * len * t + nx * half, y + uy * len * t + ny * half));
    }
    for i in (0..=12).rev() {
        let t = f64::from(i) / 12.0;
        let half = wid / 2.0 * (PI * t).sin().powf(0.8);
        pts.push((x + ux * len * t - nx * half, y + uy * len * t - ny * half));
    }
    pts
}

// ── the lead head's hammered glass ───────────────────────────────────────

/// Hammered violet glass filling `w × h` of `parent`, lit from below.
pub fn hammered_glass(b: &mut Builder<'_>, parent: NodeId, w: f64, h: f64) {
    b.add(
        parent,
        draw::rect(
            "glass",
            0.0,
            0.0,
            w,
            h,
            draw::linear(180.0, &[(0.0, "#c93fd0"), (0.14, "#9a2fb8"), (0.42, "#55288f"), (0.7, "#4f3a9e"), (0.9, "#7f62d6"), (1.0, "#cbb2ff")]),
        ),
    );
    // The light behind the glass: brighter columns low in the middle.
    let mut rng = Rng::new(7);
    for i in 0..4 {
        let cx = w * (0.34 + 0.1 * f64::from(i)) + rng.range(-10.0, 10.0);
        let col = draw::rect("tube light", cx - 48.0, h * 0.3, 96.0, h * 0.7, draw::radial_at(0.5, 0.8, 0.6, &[(0.0, "#f1e8ffc8"), (0.45, "#c7a8ff55"), (1.0, "#b99cff00")]));
        b.add(parent, col);
    }
    let glow = draw::rect("tube glow", w * 0.08, h * 0.8, w * 0.84, h * 0.2, draw::radial_at(0.5, 1.0, 0.55, &[(0.0, "#d9c2ff80"), (1.0, "#b58cff00")]));
    let glow = b.add(parent, draw::emissive(glow, draw::hex("#b98cff"), 0.25, 18.0));
    motion::add(b, glow, motion::flicker(0.08));

    // The hammered cells: a brick lattice of jittered shared vertices, so
    // the cells tile; each dimple outlined dark, its upper rim catching light.
    let (cw, ch) = (66.0, 50.0);
    let cols = (w / (cw / 2.0)).ceil() as usize + 3;
    let rows = (h / ch).ceil() as usize + 2;
    let mut rng = Rng::new(42);
    let mut v = vec![vec![(0.0, 0.0); cols]; rows];
    for (j, row) in v.iter_mut().enumerate() {
        for (i, p) in row.iter_mut().enumerate() {
            *p = (i as f64 * cw / 2.0 - cw + rng.range(-cw * 0.26, cw * 0.26), j as f64 * ch - ch * 0.5 + rng.range(-ch * 0.36, ch * 0.36));
        }
    }
    let (mut dark, mut lit, mut dimple, mut cells) = (Vec::new(), Vec::new(), String::new(), Vec::new());
    for j in 0..rows - 1 {
        let mut i = j % 2;
        while i + 2 < cols {
            let poly = [v[j][i], v[j][i + 1], v[j][i + 2], v[j + 1][i + 2], v[j + 1][i + 1], v[j + 1][i]];
            let mut ring = poly.to_vec();
            ring.push(poly[0]);
            dark.push(ring);
            lit.push([poly[5], poly[0], poly[1], poly[2]].iter().map(|(x, y)| (x + 1.2, y + 1.6)).collect());
            let (cx, cy) = poly.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / 6.0, a.1 + p.1 / 6.0));
            cells.push(shape("dimple", &poly, draw::radial_at(rng.range(0.55, 0.7), rng.range(0.6, 0.75), 0.75, &[(0.0, "#ffffff2e"), (0.55, "#ffffff08"), (1.0, "#1a063a40")])));
            if rng.next() < 0.35 {
                dimple.push_str(&draw::polygon(&oval(cx + cw * 0.05, cy + ch * 0.08, cw * 0.2, ch * 0.16, rng.range(-30.0, 30.0))));
                dimple.push(' ');
            }
            i += 2;
        }
    }
    for c in cells {
        b.add(parent, c);
    }
    b.add(parent, draw::path("dimples", 0.0, 0.0, w, h, dimple, draw::solid("#f0e0ff14")));
    b.add(parent, ui::strokes("cell rims", &lit, 2.2, draw::solid("#f3dcff2a")));
    b.add(parent, ui::strokes("cell edges", &dark, 2.6, draw::solid("#1d0b3f4d")));
    // Glass sheen and the recess shadow round the opening.
    b.add(parent, draw::rect("sheen", 0.0, 0.0, w, h, draw::linear(110.0, &[(0.0, "#ffffff00"), (0.4, "#ffffff10"), (0.6, "#ffffff00")])));
    let mut rim = draw::frame("recess", 0.0, 0.0, w, h);
    rim.effects = vec![draw::inner_shadow("#2a0a3acc", 0.0, 3.0, 8.0)];
    b.add(parent, rim);
}

// ── the rhythm head's white relief ───────────────────────────────────────

fn plaster(n: Node, depth: f64) -> Node {
    draw::with_effects(n, vec![draw::drop_shadow("#00000059", depth * 0.5, depth, depth * 1.4), draw::inner_shadow("#00000026", -depth * 0.3, -depth * 0.5, depth * 0.6), draw::inner_shadow("#ffffffcc", depth * 0.2, depth * 0.3, depth * 0.3)])
}

const WHITE: &str = "#eeeeec";

fn white_fill() -> document_proto::Paint {
    draw::radial_at(0.35, 0.3, 0.9, &[(0.0, "#fbfbfa"), (0.6, "#ececea"), (1.0, "#d2d2d0")])
}

/// A spiral scroll about (`cx`, `cy`): `turns` turns from `r0` out to `r1`,
/// `dir` ±1 for its hand.
fn scroll(cx: f64, cy: f64, r0: f64, r1: f64, turns: f64, start: f64, dir: f64) -> Vec<(f64, f64)> {
    (0..=90)
        .map(|i| {
            let t = f64::from(i) / 90.0;
            let a = start + dir * t * turns * TAU;
            let r = r0 + (r1 - r0) * t;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// A spray of leaflets along a quadratic curve `p0 → p2` bowed through `p1`.
#[allow(clippy::too_many_arguments)]
fn spray(b: &mut Builder<'_>, parent: NodeId, (p0, p1, p2): ((f64, f64), (f64, f64), (f64, f64)), n: usize, size: f64, rng: &mut Rng, depth: f64, name: &str) {
    let q = |t: f64| {
        let u = 1.0 - t;
        (u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0, u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1)
    };
    let stem: Vec<(f64, f64)> = (0..=24).map(|i| q(f64::from(i) / 24.0)).collect();
    b.add(parent, draw::with_effects(ui::line_node("stem", &stem, size * 0.18, "#dcdcda"), vec![draw::drop_shadow("#00000040", depth * 0.4, depth * 0.8, depth)]));
    let mut d = String::new();
    for k in 0..n {
        let t = (k as f64 + 0.5) / n as f64;
        let (x, y) = q(t);
        let (x2, y2) = q((t + 0.02).min(1.0));
        let along = (x2 - x).atan2(-(y2 - y)).to_degrees();
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let s = size * (1.0 - 0.45 * t) * rng.range(0.8, 1.15);
        d.push_str(&draw::polygon(&leaf(x, y, s, s * 0.45, along + side * rng.range(35.0, 60.0))));
        d.push(' ');
    }
    let (w, h) = b.scene.node(parent).map_or((0.0, 0.0), |p| (p.size.width, p.size.height));
    b.add(parent, plaster(draw::path(name, 0.0, 0.0, w, h, d, draw::solid(WHITE)), depth));
}

/// A small five-petal rosette.
fn rosette(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, dark: bool) {
    for i in 0..5 {
        let a = f64::from(i) * 72.0;
        let (px, py) = draw::polar(cx, cy, r * 0.55, a);
        let n = shape("petal", &oval(px, py, r * 0.5, r * 0.36, a - 90.0), if dark { black_fill() } else { white_fill() });
        b.add(parent, if dark { lacquer(n, r * 0.15) } else { plaster(n, r * 0.15) });
    }
    let n = shape("heart", &oval(cx, cy, r * 0.3, r * 0.3, 0.0), if dark { black_fill() } else { white_fill() });
    b.add(parent, if dark { lacquer(n, r * 0.1) } else { plaster(n, r * 0.1) });
}

/// A cherub: head, curls, a raised arm, a drape, and (`devil`) horns and a
/// bat wing — or a halo and a feathered wing. `dir` ±1 faces it.
fn cherub(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, s: f64, dir: f64, devil: bool) {
    // The wing, behind.
    if devil {
        let (sx, sy) = (cx - dir * s * 0.1, cy - s * 0.1);
        let mut pts = vec![(sx, sy)];
        for i in 0..=6 {
            let t = f64::from(i) / 6.0;
            let a = (15.0 + 125.0 * t).to_radians();
            let r = s * if i % 2 == 0 { 1.0 } else { 0.72 };
            pts.push((sx - dir * r * a.sin(), sy - r * a.cos() * 0.8));
        }
        pts.push((cx, cy + s * 0.3));
        b.add(parent, plaster(shape("bat wing", &pts, white_fill()), 2.5));
    } else {
        for i in 0..5 {
            let a = -dir * (100.0 + 14.0 * f64::from(i));
            let len = s * (1.0 - 0.1 * f64::from(i));
            b.add(parent, plaster(shape("feather", &leaf(cx - dir * s * 0.15, cy - s * 0.05, len, s * 0.3, a), white_fill()), 2.0));
        }
    }
    // Body and drape.
    b.add(parent, plaster(shape("body", &oval(cx, cy + s * 0.35, s * 0.36, s * 0.5, dir * 15.0), white_fill()), 3.0));
    b.add(parent, plaster(shape("drape", &leaf(cx - dir * s * 0.1, cy + s * 0.9, s * 1.1, s * 0.5, -dir * 20.0), white_fill()), 2.5));
    b.add(parent, plaster(shape("leg", &oval(cx + dir * s * 0.15, cy + s * 0.85, s * 0.14, s * 0.3, -dir * 25.0), white_fill()), 2.5));
    // The raised arm, reaching toward the centre.
    b.add(parent, plaster(shape("arm", &leaf(cx + dir * s * 0.2, cy + s * 0.05, s * 0.8, s * 0.2, dir * 55.0), white_fill()), 2.5));
    // Head with curls.
    let (hx, hy) = (cx + dir * s * 0.05, cy - s * 0.32);
    b.add(parent, plaster(shape("head", &oval(hx, hy, s * 0.3, s * 0.3, 0.0), white_fill()), 3.0));
    for i in 0..6 {
        let (x, y) = draw::polar(hx, hy, s * 0.26, -80.0 + 32.0 * f64::from(i));
        b.add(parent, plaster(shape("curl", &oval(x, y, s * 0.08, s * 0.08, 0.0), white_fill()), 1.2));
    }
    if devil {
        for sx in [-1.0, 1.0] {
            b.add(parent, plaster(shape("horn", &leaf(hx + sx * s * 0.14, hy - s * 0.2, s * 0.26, s * 0.1, sx * 25.0), white_fill()), 1.5));
        }
    } else {
        let mut pts = oval(hx - dir * s * 0.05, hy - s * 0.4, s * 0.3, s * 0.08, -dir * 10.0);
        pts.push(pts[0]);
        let ring = ui::strokes("halo", &[pts], s * 0.05, draw::solid(WHITE));
        b.add(parent, plaster(ring, 1.5));
    }
}

/// A white plaster relief filling `w × h` of `parent`.
pub fn relief_white(b: &mut Builder<'_>, parent: NodeId, w: f64, h: f64) {
    b.add(parent, draw::rect("ground", 0.0, 0.0, w, h, draw::linear(180.0, &[(0.0, "#d9d9d7"), (1.0, "#e6e6e4")])));
    // The moulded frame: a stepped bevel, then the recessed field.
    let m = 14.0;
    let outer = draw::rect("moulding", m * 0.4, m * 0.4, w - m * 0.8, h - m * 0.8, draw::solid("#e9e9e7"));
    b.add(parent, draw::with_effects(outer, vec![draw::inner_shadow("#ffffff", 1.5, 1.5, 1.0), draw::inner_shadow("#0000004d", -1.5, -1.5, 2.0)]));
    let mut field = draw::frame("field", m, m, w - 2.0 * m, h - 2.0 * m);
    field.fills = vec![draw::linear(180.0, &[(0.0, "#dededc"), (1.0, "#e8e8e6")])];
    field.effects = vec![draw::inner_shadow("#00000059", 2.0, 4.0, 6.0)];
    field.clips_content = true;
    let field = b.add(parent, field);
    let (fw, fh) = (w - 2.0 * m, h - 2.0 * m);
    let (cx, cy) = (fw / 2.0, fh * 0.52);
    let mut rng = Rng::new(11);

    // Sprays of acanthus sweeping out from the shield, mirrored.
    for dir in [-1.0, 1.0] {
        for (k, (dx0, dy0, dx1, dy1, dx2, dy2)) in [
            (40.0, -30.0, 150.0, -110.0, 290.0, -55.0),
            (40.0, 30.0, 160.0, 110.0, 300.0, 45.0),
            (60.0, -5.0, 170.0, -40.0, 250.0, 5.0),
            (290.0, -55.0, 370.0, -100.0, 460.0, -25.0),
            (300.0, 45.0, 390.0, 95.0, 480.0, 20.0),
            (240.0, 0.0, 330.0, -10.0, 420.0, 15.0),
        ]
        .iter()
        .enumerate()
        {
            spray(b, field, ((cx + dir * dx0, cy + dy0), (cx + dir * dx1, cy + dy1), (cx + dir * dx2, cy + dy2)), 16 + 2 * k, 30.0, &mut rng, 2.4, "acanthus");
        }
        // The C-scrolls either side of the shield.
        for (x, y, r, turns, start) in [(150.0, 30.0, 40.0, 1.4, 0.0), (290.0, -30.0, 28.0, 1.2, PI), (95.0, -50.0, 20.0, 1.1, PI / 2.0), (400.0, 10.0, 24.0, 1.3, 0.0)] {
            let pts = scroll(cx + dir * x, cy + y, 3.0, r, turns, start, dir);
            b.add(field, plaster(ui::line_node("scroll", &pts, 10.0, WHITE), 3.0));
        }
        for (x, y, r) in [(220.0, -60.0, 11.0), (360.0, 55.0, 10.0), (120.0, 70.0, 9.0), (250.0, 62.0, 9.5), (420.0, -55.0, 9.0), (200.0, 20.0, 8.0), (330.0, -75.0, 8.0), (440.0, 70.0, 8.0)] {
            rosette(b, field, cx + dir * x, cy + y, r, false);
        }
    }
    // The shield, its bead strand and the rosette crowning it.
    let (top, bot) = (cy - 72.0, cy + 82.0);
    let mut shield = vec![(cx, top + 10.0)];
    for i in 0..=30 {
        let u = f64::from(i) / 30.0;
        let half = 46.0 * (1.0 - u.powf(2.5)).powf(0.6) + 4.0;
        shield.push((cx - half, top + (bot - top) * u));
    }
    for i in (0..=30).rev() {
        let u = f64::from(i) / 30.0;
        let half = 46.0 * (1.0 - u.powf(2.5)).powf(0.6) + 4.0;
        shield.push((cx + half, top + (bot - top) * u));
    }
    b.add(field, plaster(shape("shield", &shield, white_fill()), 4.0));
    b.add(field, plaster(shape("shield boss", &oval(cx, cy + 5.0, 26.0, 40.0, 0.0), white_fill()), 2.0));
    for i in 0..13 {
        let t = f64::from(i) / 12.0;
        let a = PI * (0.1 + 0.8 * t);
        let (x, y) = (cx - 34.0 * a.cos(), cy - 30.0 + 38.0 * a.sin());
        b.add(field, plaster(shape("bead", &oval(x, y, 3.2, 3.2, 0.0), white_fill()), 1.0));
    }
    rosette(b, field, cx, cy - 78.0, 12.0, false);
    // The cherubs at either end, reaching in.
    cherub(b, field, fw * 0.08, fh * 0.44, 72.0, 1.0, false);
    cherub(b, field, fw * 0.92, fh * 0.44, 72.0, -1.0, true);
}

// ── the acoustic head's black floral relief ──────────────────────────────

fn black_fill() -> document_proto::Paint {
    draw::radial_at(0.32, 0.25, 0.95, &[(0.0, "#46464a"), (0.55, "#242426"), (1.0, "#101011")])
}

fn lacquer(n: Node, depth: f64) -> Node {
    draw::with_effects(n, vec![draw::drop_shadow("#000000d9", depth * 0.5, depth, depth * 1.6), draw::inner_shadow("#ffffff26", depth * 0.25, depth * 0.35, depth * 0.3), draw::inner_shadow("#00000080", -depth * 0.3, -depth * 0.5, depth * 0.8)])
}

/// A rose: rings of cupped petals spiralling in to a tight bud.
fn rose(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, rng: &mut Rng) {
    for ring in 0..4 {
        let rr = r * (1.0 - 0.23 * f64::from(ring));
        let n = 5 + (3 - ring);
        let off = rng.range(0.0, 72.0);
        for i in 0..n {
            let a = off + 360.0 * f64::from(i) / f64::from(n);
            let (px, py) = draw::polar(cx, cy, rr * 0.45, a);
            let petal = oval(px, py, rr * 0.55, rr * 0.4, a + 90.0);
            b.add(parent, lacquer(shape("petal", &petal, black_fill()), (rr * 0.06).max(1.5)));
        }
    }
    let bud = scroll(cx, cy, 1.0, r * 0.28, 1.6, rng.range(0.0, TAU), 1.0);
    b.add(parent, lacquer(ui::line_node("bud", &bud, (r * 0.05).max(1.2), "#3a3a3d"), 1.2));
}

/// A five-petal flower.
fn blossom(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, rot: f64) {
    for i in 0..5 {
        let a = rot + f64::from(i) * 72.0;
        b.add(parent, lacquer(shape("petal", &leaf(cx, cy, r, r * 0.7, a), black_fill()), r * 0.06));
    }
    rosette(b, parent, cx, cy, r * 0.22, true);
}

/// A stalk of wheat from (`x`, `y`) along `deg`.
fn wheat(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, len: f64, deg: f64) {
    let a = deg.to_radians();
    let (ux, uy) = (a.sin(), -a.cos());
    b.add(parent, lacquer(ui::line_node("straw", &[(x, y), (x + ux * len, y + uy * len)], 2.5, "#2a2a2c"), 1.5));
    for k in 0..12 {
        let t = 0.35 + 0.65 * f64::from(k) / 12.0;
        let (gx, gy) = (x + ux * len * t, y + uy * len * t);
        for side in [-1.0, 1.0] {
            b.add(parent, lacquer(shape("grain", &leaf(gx, gy, len * 0.1, len * 0.05, deg + side * 35.0), black_fill()), 1.0));
        }
    }
}

/// A black floral relief filling `w × h` of `parent`.
pub fn floral_black(b: &mut Builder<'_>, parent: NodeId, w: f64, h: f64) {
    b.add(parent, draw::rect("ground", 0.0, 0.0, w, h, draw::linear(180.0, &[(0.0, "#1b1b1c"), (1.0, "#121213")])));
    let mut rng = Rng::new(23);
    // Leaves everywhere behind.
    for _ in 0..190 {
        let (x, y) = (rng.range(-30.0, w + 30.0), rng.range(-20.0, h + 20.0));
        let len = rng.range(34.0, 70.0);
        let deg = rng.range(0.0, 360.0);
        let pts = leaf(x, y, len, len * 0.45, deg);
        b.add(parent, lacquer(shape("leaf", &pts, black_fill()), 2.0));
        let a = deg.to_radians();
        let rib = [(x, y), (x + a.sin() * len * 0.85, y - a.cos() * len * 0.85)];
        b.add(parent, ui::line_node("rib", &rib, 0.8, "#0a0a0a"));
    }
    wheat(b, parent, w * 0.16, h * 0.5, 150.0, 60.0);
    wheat(b, parent, w * 0.4, h * 0.95, 170.0, 70.0);
    wheat(b, parent, w * 0.76, h * 0.2, 130.0, -70.0);
    wheat(b, parent, w * 0.62, h * 1.05, 160.0, 50.0);
    wheat(b, parent, w * 0.02, h * 0.95, 120.0, 40.0);
    for (x, y, r, rot) in [
        (0.13, 0.72, 50.0, 10.0),
        (0.47, 0.52, 46.0, 30.0),
        (0.61, 0.24, 44.0, -10.0),
        (0.955, 0.48, 40.0, 20.0),
        (0.33, 0.12, 26.0, 5.0),
        (0.86, 0.9, 28.0, 40.0),
        (0.71, 0.9, 22.0, 12.0),
        (0.53, 0.05, 24.0, 50.0),
        (0.24, 0.95, 26.0, 8.0),
    ] {
        blossom(b, parent, w * x, h * y, r, rot);
    }
    rose(b, parent, w * 0.07, h * 0.22, 54.0, &mut rng);
    rose(b, parent, w * 0.56, h * 0.82, 40.0, &mut rng);
    rose(b, parent, w * 0.97, h * 0.1, 40.0, &mut rng);
    rose(b, parent, w * 0.33, h * 0.42, 82.0, &mut rng);
    rose(b, parent, w * 0.8, h * 0.62, 86.0, &mut rng);
    let mut rim = draw::frame("recess", 0.0, 0.0, w, h);
    rim.effects = vec![draw::inner_shadow("#000000", 0.0, 3.0, 8.0)];
    b.add(parent, rim);
}

/// The maker's mark: a quick looped scribble `w` wide from (`x`, `cy`).
pub fn flourish(b: &mut Builder<'_>, parent: NodeId, x: f64, cy: f64, w: f64, ink: &str) {
    let mut pts = Vec::new();
    for i in 0..=60 {
        let t = f64::from(i) / 60.0;
        let a = t * TAU * 1.5;
        pts.push((x + w * 0.05 + w * 0.3 * t + w * 0.06 * a.cos(), cy - w * 0.12 * a.sin() + w * 0.05 * t));
    }
    for i in 0..=30 {
        let t = f64::from(i) / 30.0;
        pts.push((x + w * 0.35 + w * 0.2 * t, cy + w * 0.03 - w * 0.25 * (PI * t).sin()));
    }
    for i in 0..=20 {
        let t = f64::from(i) / 20.0;
        pts.push((x + w * 0.55 + w * 0.45 * t, cy - w * 0.02 * t));
    }
    b.add(parent, ui::line_node("mark", &pts, 1.1, ink));
    b.add(parent, ui::line_node("mark stroke", &[(x + w * 0.25, cy + w * 0.12), (x + w * 0.55, cy - w * 0.22)], 1.0, ink));
}
