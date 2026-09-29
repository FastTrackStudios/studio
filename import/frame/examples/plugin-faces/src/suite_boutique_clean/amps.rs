//! The three amp heads (Head form, 1200 wide, the height from each
//! faceplate's aspect, handle included), drawn from the plugin's own amp
//! pages:
//!
//! - `05-blush-head-1200x700`: *Blush* — a glossy pink head with rounded
//!   ends, a recessed control shelf (input, VOLUME, BASS, CUT, MASTER,
//!   LEVEL, power toggle and teal pilot) over a woven grille embossed with
//!   a maze of key shapes, and a braided cord handle.
//! - `06-ranch-head-1200x680`: *Ranch* — tan tolex with leather corners, a
//!   cream faceplate with a swooping lower edge (input, a three-way MODE,
//!   GAIN, BASS, MID, TREBLE, PRESENCE, MASTER, OUTPUT, power, pilot) over
//!   a brass circle-lattice grille with a centre plaque.
//! - `07-kintsugi-head-1200x650`: *Kintsugi* — a cream tolex head whose
//!   front is an art panel of broken porcelain mended in gold, over a
//!   brushed-gold control strip (input, BRIGHT, GAIN, BASS, MIDDLE, TREBLE,
//!   SHIFT, MASTER, PRESENCE, OUTPUT, POWER).

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::ParamInfo;
use surface_widgets::Builder;

use super::look::{self, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, HEAD_W, Type};

// ── shared ───────────────────────────────────────────────────────────────

/// A carry handle: a strap (`strap` paint, `thick` px) arching between two
/// brass fittings at `x0`/`x1`, its feet on `y`, rising `rise`.
#[allow(clippy::too_many_arguments)]
fn handle(b: &mut Builder<'_>, root: NodeId, x0: f64, x1: f64, y: f64, rise: f64, thick: f64, strap: &str) {
    let pts: Vec<(f64, f64)> = (0..=40)
        .map(|i| {
            let u = f64::from(i) / 40.0;
            let lift = (u * std::f64::consts::PI).sin().powf(0.45);
            (x0 + (x1 - x0) * u, y - rise * lift)
        })
        .collect();
    b.add(root, draw::with_effects(ui::strokes("strap", std::slice::from_ref(&pts), thick, draw::solid(strap)), vec![draw::drop_shadow("#00000059", 0.0, 3.0, 5.0)]));
    let hi: Vec<(f64, f64)> = pts.iter().map(|(x, y)| (*x, y - thick * 0.22)).collect();
    b.add(root, ui::strokes("strap light", &[hi], thick * 0.22, draw::solid("#ffffff26")));
    for x in [x0, x1] {
        let loop_ = parts::ring(x, y - 6.0, 9.0);
        b.add(root, ui::strokes("fitting", &[loop_], 4.0, draw::linear(160.0, &[(0.0, "#fff1c4"), (0.5, "#b8904a"), (1.0, "#7a5a2a")])));
        b.add(root, draw::rounded(draw::rect("foot", x - 16.0, y - 2.0, 32.0, 6.0, draw::linear(180.0, &[(0.0, "#e6c886"), (1.0, "#8a6630")])), 3.0));
    }
}

/// A knob, the suite's way: tight box, tick ring in `ink`, no legend.
#[allow(clippy::too_many_arguments)]
fn knob(b: &mut Builder<'_>, parent: NodeId, variant: &str, addr: &str, d: f64, at: (f64, f64), opts: &[&str], ink: &str) -> NodeId {
    parts::knob(b, parent, "knob.fluted_line", variant, addr, "", d, at, opts, ink)
}

/// A round push cap (bright, shift, power), its lamp over it in teal.
fn push(b: &mut Builder<'_>, parent: NodeId, addr: &str, d: f64, at: (f64, f64), lamp: bool) -> NodeId {
    let mut p = Place::new(addr).size(d).variant("grey").bind(ui::bind(addr)).legend("").touch(TOUCH_MIN);
    let color = format!("color={TEAL}");
    p = p.options(&[if lamp { color.as_str() } else { "nolamp" }]);
    parts::part(b, parent, "button.round_cap", p, &["cap"], at)
}

fn amp_face(file: &'static str, scene: document_proto::Scene, params: Vec<ParamInfo>) -> Face {
    look::face(file, scene, params)
}

// ── Blush ────────────────────────────────────────────────────────────────

/// A maze tile's raised key shape inside (`x, y, w, h`), variant `v`.
fn maze(x: f64, y: f64, w: f64, h: f64, v: usize) -> Vec<Vec<(f64, f64)>> {
    let (l, r, t, bt) = (x + 10.0, x + w - 10.0, y + 12.0, y + h - 12.0);
    let (m, cx) = ((t + bt) / 2.0, (l + r) / 2.0);
    match v % 4 {
        0 => vec![vec![(r, t), (l, t), (l, bt), (r, bt)], vec![(r - 18.0, m), (l + 18.0, m)], vec![(r, m - 14.0), (r, m + 14.0)]],
        1 => vec![vec![(l, t), (r, t), (r, m), (l, m), (l, bt), (r, bt)], vec![(cx, t + 10.0), (cx, m - 8.0)]],
        2 => vec![vec![(l, m), (l, t), (r, t), (r, bt), (cx, bt), (cx, m + 10.0)], vec![(l + 16.0, m), (cx + 8.0, m)], vec![(l, bt - 14.0), (l, bt)]],
        _ => vec![vec![(cx, t), (l, t), (l, bt), (r, bt), (r, m)], vec![(cx, m), (r - 14.0, m)], vec![(r, t), (r - 20.0, t)]],
    }
}

pub(super) fn blush() -> Face {
    const PINK: &str = "#efc9cf";
    const INK: &str = "#2a1d20";
    let (w, h) = (HEAD_W, 700.0);
    let top = 96.0; // the head's top edge (the handle stands above)
    let scene = canvas("Blush Head", |b, root| {
        parts::page(b, root, w, h);
        // Feet.
        for x in [300.0, 900.0] {
            b.add(root, draw::path("foot", 0.0, 0.0, w, h, draw::polygon(&[(x - 60.0, top + 560.0), (x + 60.0, top + 560.0), (x + 30.0, h), (x - 30.0, h)]), draw::linear(180.0, &[(0.0, "#2d2c3a"), (1.0, "#12121a")])));
        }
        // The shell: glossy, fully rounded ends.
        let shell = look::slab("Shell", (0.0, top, w, 580.0), 130.0, vec![draw::linear(180.0, &["#fbe3e6", PINK, "#dcaab2"].iter().enumerate().map(|(i, c)| (i as f64 / 2.0, *c)).collect::<Vec<_>>()), draw::radial_at(0.3, 0.08, 0.6, &[(0.0, "#ffffff66"), (1.0, "#ffffff00")])]);
        let shell = b.add(root, draw::with_effects(shell, vec![draw::drop_shadow("#00000059", 0.0, 10.0, 24.0), draw::inner_shadow("#ffffffaa", 0.0, 3.0, 2.0), draw::inner_shadow("#a0606a55", 0.0, -8.0, 14.0)]));
        // The recessed front: the control shelf over the grille.
        let front = look::slab("Front", (34.0, 36.0, w - 68.0, 510.0), 96.0, vec![draw::solid("#e7b9c0")]);
        let front = b.add(shell, draw::with_effects(front, vec![draw::inner_shadow("#8a4a5577", 0.0, 6.0, 12.0)]));
        let shelf = look::slab("Shelf", (8.0, 8.0, w - 84.0, 196.0), 88.0, vec![draw::linear(180.0, &[(0.0, "#f0c6cc"), (1.0, "#e6b3ba")])]);
        let shelf = b.add(front, draw::with_effects(shelf, vec![draw::drop_shadow("#00000033", 0.0, 3.0, 5.0), draw::inner_shadow("#ffffff80", 0.0, 1.5, 1.0)]));
        // Grille panel: woven cloth with a maze of raised keys.
        let grille = look::slab("Grille", (56.0, 226.0, w - 180.0, 250.0), 22.0, vec![draw::solid("#dcb0b7")]);
        let grille = b.add(front, draw::with_effects(grille, vec![draw::inner_shadow("#7a3a4566", 0.0, 3.0, 6.0)]));
        let (gw, gh) = (w - 180.0, 250.0);
        let mut weave = String::new();
        for i in 0..(gh as usize / 4) {
            weave.push_str(&format!("M 0 {y:.1} L {gw:.1} {y:.1} ", y = 4.0 * i as f64));
        }
        let mut wn = draw::path("weave", 0.0, 0.0, gw, gh, weave, draw::solid("#00000000"));
        wn.stroke = document_proto::Stroke { paints: vec![draw::solid("#ffffff1c")], weight: 1.0, ..document_proto::Stroke::default() };
        b.add(grille, wn);
        let cols = 7;
        let (tw, th) = (gw / cols as f64, gh / 2.0);
        let mut keys = Vec::new();
        for row in 0..2 {
            for col in 0..cols {
                if row == 1 && col == cols - 1 {
                    continue;
                }
                keys.extend(maze(tw * col as f64, th * f64::from(row), tw, th, col * 3 + row as usize * 5 + 1));
                keys.push(parts::round_rect(tw * col as f64 + 3.0, th * f64::from(row) + 3.0, tw - 6.0, th - 6.0, 10.0));
            }
        }
        // Raised keys: their shadow down-right, the light up-left, the top.
        let off = |dx: f64, dy: f64| keys.iter().map(|l| l.iter().map(|(x, y)| (x + dx, y + dy)).collect()).collect::<Vec<Vec<(f64, f64)>>>();
        b.add(grille, ui::strokes("key shade", &off(2.5, 3.0), 13.0, draw::solid("#a0606e70")));
        b.add(grille, ui::strokes("key light", &off(-1.5, -1.5), 12.0, draw::solid("#fff1f3")));
        b.add(grille, ui::strokes("keys", &keys, 11.0, draw::solid("#eac3c9")));
        look::brush_mark(b, front, (w - 150.0, 450.0), 22.0, INK);
        // The control shelf. Coordinates in the shelf's space.
        let plate = look::slab("Emblem", (58.0, 38.0, 104.0, 104.0), 12.0, vec![draw::linear(160.0, &[(0.0, "#f8dde1"), (1.0, "#e0b0b8")])]);
        let plate = b.add(shelf, draw::with_effects(plate, vec![draw::drop_shadow("#00000040", 0.0, 3.0, 6.0), draw::inner_shadow("#ffffffa0", 0.0, 1.5, 1.0)]));
        for (x, y) in [(12.0, 12.0), (92.0, 12.0), (12.0, 92.0), (92.0, 92.0)] {
            b.add(plate, draw::circle("rivet", x, y, 3.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fff1c4"), (1.0, "#8a6630")])));
        }
        look::emblem(b, plate, (52.0, 48.0), 30.0, INK);
        let t = Type::new(11.0, INK, 700.0).track(0.28).upper();
        parts::centred(b, shelf, "Input", (245.0, 58.0), t);
        let knobs = [("blush/volume", "Volume"), ("blush/bass", "Bass"), ("blush/cut", "Cut"), ("blush/master", "Master"), ("blush/level", "Level")];
        for (i, (addr, legend)) in knobs.iter().enumerate() {
            let x = 358.0 + 133.0 * i as f64;
            knob(b, shelf, "black", addr, 74.0, (x, 104.0), &["ticks=41", "lobes=9"], "#5a4046");
            parts::centred(b, shelf, legend, (x, 164.0), t);
        }
        parts::centred(b, shelf, "Power", (1003.0, 50.0), t);
        look::brass_toggle(b, shelf, "blush/power", "Power", 22.0, (1003.0, 100.0));
        parts::status_led(b, shelf, "blush/power", 13.0, (1003.0, 148.0), TEAL, "light", None);
        // Input jack, and the cable over the grille and off the unit.
        let (jx, jy) = (34.0 + 8.0 + 245.0, top + 36.0 + 8.0 + 104.0);
        parts::cable(b, root, (jx, jy), 14.0, &[(jx + 4.0, jy + 200.0), (jx - 6.0, jy + 400.0), (jx - 60.0, h - 30.0), (jx - 200.0, h - 8.0)]);
        handle(b, root, 340.0, 860.0, top + 6.0, 84.0, 12.0, "#2b2340");
    });
    let params = vec![
        parts::dial10("blush/volume", "Volume", 5.0),
        parts::dial10("blush/bass", "Bass", 5.0),
        parts::dial10("blush/cut", "Cut", 3.0),
        parts::dial10("blush/master", "Master", 6.0),
        parts::dial10("blush/level", "Level", 6.0),
        parts::switch("blush/power", "Power", true),
    ];
    amp_face("05-blush-head-1200x700", scene, params)
}

// ── Ranch ────────────────────────────────────────────────────────────────

fn tolex(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), r: f64, base: [&str; 3]) -> NodeId {
    let mut n = look::slab("Tolex", (x, y, w, h), r, vec![draw::linear(180.0, &[(0.0, base[0]), (0.5, base[1]), (1.0, base[2])])]);
    n.clips_content = true;
    let n = b.add(parent, draw::with_effects(n, vec![draw::drop_shadow("#00000066", 0.0, 10.0, 22.0), draw::inner_shadow("#ffffff40", 0.0, 2.0, 2.0)]));
    // Leather grain: a sparse speckle of pits and lights.
    let mut pits = String::new();
    let mut lights = String::new();
    let mut seed: u32 = 0x2545_f491;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f64::from(seed) / f64::from(u32::MAX)
    };
    for i in 0..((w * h / 90.0) as usize) {
        let (px, py, s) = (next() * w, next() * h, 0.6 + next() * 1.4);
        let d = format!("M {px:.1} {py:.1} l {s:.1} {:.1} l {:.1} {s:.1} Z ", s * 0.3, -s * 0.8);
        if i % 3 == 0 { lights.push_str(&d) } else { pits.push_str(&d) }
    }
    b.add(n, draw::path("grain", 0.0, 0.0, w, h, pits, draw::solid("#0000002a")));
    b.add(n, draw::path("grain", 0.0, 0.0, w, h, lights, draw::solid("#ffffff1c")));
    n
}

pub(super) fn ranch() -> Face {
    const INK: &str = "#1d1a17";
    let (w, h) = (HEAD_W, 680.0);
    let top = 56.0;
    let bh = 606.0;
    let scene = canvas("Ranch Head", |b, root| {
        parts::page(b, root, w, h);
        for x in [210.0, 990.0] {
            b.add(root, draw::rounded(draw::rect("foot", x - 40.0, top + bh - 10.0, 80.0, h - top - bh + 10.0, draw::linear(180.0, &[(0.0, "#2a2a2c"), (1.0, "#0e0e10")])), 14.0));
        }
        let body = tolex(b, root, (0.0, top, w, bh), 30.0, ["#a4532d", "#8c4424", "#6e3319"]);
        // The grille: brass circle lattice over black cloth.
        let (gx, gy, gw, gh) = (54.0, 170.0, w - 108.0, bh - 170.0 - 54.0);
        let mut g = look::slab("Grille", (gx, gy, gw, gh), 4.0, vec![draw::radial_at(0.5, 0.5, 0.8, &[(0.0, "#1a1714"), (1.0, "#090807")])]);
        g.clips_content = true;
        let grille = b.add(body, draw::with_effects(g, vec![draw::inner_shadow("#000000d0", 0.0, 4.0, 10.0)]));
        let pitch = 32.0;
        let mut circles = Vec::new();
        for row in 0..((gh / pitch) as i32 + 2) {
            for col in 0..((gw / pitch) as i32 + 2) {
                circles.push(parts::ring(f64::from(col) * pitch, f64::from(row) * pitch, pitch * 0.5));
                circles.push(parts::ring(f64::from(col) * pitch + pitch / 2.0, f64::from(row) * pitch + pitch / 2.0, pitch * 0.36));
            }
        }
        b.add(grille, ui::strokes("lattice shade", &circles.iter().map(|l| l.iter().map(|(x, y)| (x + 1.5, y + 2.0)).collect()).collect::<Vec<_>>(), 5.0, draw::solid("#00000099")));
        b.add(grille, ui::strokes("lattice", &circles, 3.4, draw::solid("#b98f4a")));
        // The faceplate, its lower edge swooping up in the middle.
        let (fx, fy, fw) = (54.0, 54.0, w - 108.0);
        let mut edge = vec![(0.0, 0.0), (fw, 0.0), (fw, 248.0)];
        edge.extend((0..=40).map(|i| {
            let u = f64::from(i) / 40.0;
            (fw * (1.0 - u), 248.0 - 56.0 * (u * std::f64::consts::PI).sin().powf(0.8))
        }));
        let mut face = draw::path("Faceplate", fx, fy, fw, 250.0, draw::polygon(&edge), draw::linear(180.0, &[(0.0, "#f3ecdc"), (1.0, "#e2d6bd")]));
        face.effects = vec![draw::drop_shadow("#00000080", 0.0, 4.0, 8.0), draw::inner_shadow("#ffffff99", 0.0, 1.5, 1.0)];
        b.add(body, face);
        let plate = b.add(body, draw::frame("Controls", fx, fy, fw, 250.0));
        for (x, y) in [(20.0, 20.0), (fw - 20.0, 20.0)] {
            parts::screw(b, plate, x, y, 5.0, "#9a7236");
        }
        let t = Type::new(9.5, INK, 800.0).track(0.06).upper();
        parts::centred(b, plate, "Input", (78.0, 64.0), t);
        let (jx, jy) = (fx + 78.0, top + fy + 98.0);
        parts::centred(b, plate, "Mode", (138.0, 38.0), t);
        parts::centred(b, plate, "Thicker", (138.0, 64.0), Type::new(8.0, INK, 800.0).upper());
        parts::centred(b, plate, "Thick", (182.0, 98.0), Type::new(8.0, INK, 800.0).upper());
        parts::centred(b, plate, "Normal", (138.0, 132.0), Type::new(8.0, INK, 800.0).upper());
        look::brass_toggle3(b, plate, "ranch/mode", "Mode", 20.0, (138.0, 98.0));
        let knobs = [("ranch/gain", "Gain"), ("ranch/bass", "Bass"), ("ranch/mid", "Mid"), ("ranch/treble", "Treble"), ("ranch/presence", "Presence"), ("ranch/master", "Master"), ("ranch/output", "Output")];
        for (i, (addr, legend)) in knobs.iter().enumerate() {
            let x = 253.0 + 100.0 * i as f64;
            knob(b, plate, "black", addr, 56.0, (x, 98.0), &["ticks=41", "lobes=24", "line=#e0a878"], "#4a4238");
            parts::centred(b, plate, legend, (x, 150.0), t);
            if i + 1 < knobs.len() {
                ui::rule(b, plate, x + 30.0, 150.0, 40.0, 1.0, "#4a423880");
            }
        }
        ui::rule(b, plate, 897.0, 98.0, 40.0, 1.0, "#4a423880");
        parts::centred(b, plate, "Power", (955.0, 60.0), t);
        look::brass_toggle(b, plate, "ranch/power", "Power", 22.0, (955.0, 98.0));
        parts::status_led(b, plate, "ranch/power", 14.0, (955.0, 148.0), TEAL, "dark", None);
        look::emblem(b, plate, (44.0, 190.0), 22.0, INK);
        // A scrawled, invented signature in the corner.
        let sig: Vec<(f64, f64)> = (0..=60).map(|i| {
            let u = f64::from(i) / 60.0;
            (fw - 110.0 + 70.0 * u, 190.0 - 12.0 * (u * 19.0).sin() * (1.0 - u * 0.6) - 8.0 * u)
        }).collect();
        parts::art(b, plate, &[sig, vec![(fw - 100.0, 200.0), (fw - 36.0, 196.0)]], 1.2, "#4a4238");
        // The plaque on the grille.
        let (px, py, pw, ph) = (w / 2.0 - 100.0, gy + 150.0, 200.0, 100.0);
        let plq = look::slab("Plaque", (px, py, pw, ph), 12.0, vec![draw::linear(170.0, &[(0.0, "#f4efe4"), (1.0, "#d9d1c0")])]);
        let plq = b.add(body, draw::with_effects(draw::stroked(plq, "#a88a5a", 3.0), vec![draw::drop_shadow("#000000a0", 0.0, 4.0, 8.0)]));
        for (x, y) in [(12.0, 12.0), (pw - 12.0, 12.0), (12.0, ph - 12.0), (pw - 12.0, ph - 12.0)] {
            b.add(plq, draw::circle("rivet", x, y, 3.5, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fff1c4"), (1.0, "#8a6630")])));
        }
        look::brush_mark(b, plq, (pw / 2.0, ph / 2.0), 22.0, "#2b2622");
        for (x0, dir) in [(24.0, 1.0), (pw - 24.0, -1.0)] {
            let wave: Vec<(f64, f64)> = (0..=20).map(|i| {
                let u = f64::from(i) / 20.0;
                (x0 + dir * 44.0 * u, ph / 2.0 + 10.0 - 12.0 * (u * 3.2).sin())
            }).collect();
            parts::art(b, plq, &[wave], 2.4, "#4d5a52");
        }
        // Leather corners.
        for (cx, cy) in [(0.0, top), (w - 64.0, top), (0.0, top + bh - 64.0), (w - 64.0, top + bh - 64.0)] {
            let c = look::slab("Corner", (cx, cy, 64.0, 64.0), 22.0, vec![draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#a0522d"), (1.0, "#5a2a14")])]);
            let c = b.add(root, draw::with_effects(c, vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0), draw::inner_shadow("#ffffff33", 0.0, 1.5, 1.0)]));
            for (rx, ry) in [(16.0, 16.0), (48.0, 16.0), (16.0, 48.0), (48.0, 48.0)] {
                b.add(c, draw::circle("rivet", rx, ry, 3.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fff1c4"), (1.0, "#8a6630")])));
            }
        }
        parts::cable(b, root, (jx, jy), 13.0, &[(jx + 2.0, jy + 180.0), (jx - 8.0, jy + 360.0), (jx - 50.0, h - 20.0), (jx - 150.0, h - 4.0)]);
        handle(b, root, 430.0, 770.0, top + 4.0, 44.0, 16.0, "#8a3f1f");
    });
    let params = vec![
        parts::pick("ranch/mode", "Mode", &["Thick", "Normal", "Thicker"], 1),
        parts::dial10("ranch/gain", "Gain", 4.0),
        parts::dial10("ranch/bass", "Bass", 5.0),
        parts::dial10("ranch/mid", "Mid", 5.5),
        parts::dial10("ranch/treble", "Treble", 6.0),
        parts::dial10("ranch/presence", "Presence", 5.0),
        parts::dial10("ranch/master", "Master", 6.0),
        parts::dial10("ranch/output", "Output", 5.0),
        parts::switch("ranch/power", "Power", true),
    ];
    amp_face("06-ranch-head-1200x680", scene, params)
}

// ── Kintsugi ─────────────────────────────────────────────────────────────

/// A five-pointed maple leaf at (`cx`, `cy`), `r` across, turned `rot`.
fn leaf(cx: f64, cy: f64, r: f64, rot: f64) -> String {
    // Five broad lobes (the top three largest), each serrated, over a
    // notched base where the stem comes in.
    let pts: Vec<(f64, f64)> = (0..120)
        .map(|i| {
            let t = f64::from(i) / 120.0;
            let a = t * 360.0 - 180.0; // −180..180, 0 = up
            let lobe = (a.to_radians() * 2.5).cos().abs().powf(0.7);
            let size = if a.abs() > 150.0 { 0.25 } else if a.abs() > 100.0 { 0.72 } else { 1.0 };
            let serr = if i % 4 == 0 { 0.1 } else { 0.0 };
            draw::polar(cx, cy, r * size * (0.42 + 0.58 * lobe + serr), rot + a)
        })
        .collect();
    let mut d = draw::polygon(&pts);
    let stem = draw::polar(cx, cy, r * 0.7, rot + 180.0);
    d.push_str(&format!(" M {cx:.1} {cy:.1} L {:.1} {:.1} L {:.1} {:.1} Z", stem.0 + 1.0, stem.1, stem.0 - 1.0, stem.1));
    d
}

fn kintsugi_art(b: &mut Builder<'_>, art: NodeId, w: f64, h: f64) {
    // A porcelain field, darker to the sides where the hair falls.
    b.add(art, draw::rect("porcelain", 0.0, 0.0, w, h, draw::radial_at(0.5, 0.45, 0.55, &[(0.0, "#f1ece6"), (0.55, "#d9d3cf"), (0.78, "#6d6a6c"), (1.0, "#1f1d1f")])));
    let mut hair = Vec::new();
    for i in 0..60 {
        let t = f64::from(i) / 60.0;
        let x = if i % 2 == 0 { t * w * 0.24 } else { w - t * w * 0.24 };
        hair.push(vec![(x, -10.0), (x + if i % 2 == 0 { 40.0 } else { -40.0 }, h + 10.0)]);
    }
    b.add(art, ui::strokes("strands", &hair, 1.2, draw::solid("#ffffff14")));
    // Two closed, lined eyes of the mended mask, drawn in ink.
    for (cx, dir) in [(w * 0.32, 1.0), (w * 0.64, -1.0)] {
        let lid: Vec<(f64, f64)> = (0..=24).map(|i| {
            let u = f64::from(i) / 24.0;
            (cx - 90.0 + 180.0 * u, h * 0.62 - 16.0 * (u * std::f64::consts::PI).sin() + dir * 6.0 * u)
        }).collect();
        let brow: Vec<(f64, f64)> = (0..=24).map(|i| {
            let u = f64::from(i) / 24.0;
            (cx - 100.0 + 200.0 * u, h * 0.3 - 22.0 * (u * std::f64::consts::PI).sin() + dir * 10.0 * u)
        }).collect();
        b.add(art, ui::strokes("lid", std::slice::from_ref(&lid), 5.0, draw::solid("#2a2a38")));
        let lashes: Vec<Vec<(f64, f64)>> = lid.iter().step_by(2).map(|(x, y)| vec![(*x, *y), (x + 3.0, y + 12.0)]).collect();
        b.add(art, ui::strokes("lashes", &lashes, 1.6, draw::solid("#2a2a38")));
        b.add(art, ui::strokes("brow", &[brow], 9.0, draw::solid("#3a3a4acc")));
    }
    // The gold seams.
    let cracks: Vec<Vec<(f64, f64)>> = vec![
        vec![(w * 0.33, 0.0), (w * 0.36, h * 0.18), (w * 0.31, h * 0.35), (w * 0.37, h * 0.55), (w * 0.35, h * 0.8), (w * 0.42, h)],
        vec![(w * 0.36, h * 0.18), (w * 0.5, h * 0.2), (w * 0.56, 0.0)],
        vec![(w * 0.5, h * 0.2), (w * 0.52, h * 0.45), (w * 0.62, h * 0.52), (w * 0.66, h * 0.8), (w * 0.62, h)],
        vec![(w * 0.52, h * 0.45), (w * 0.4, h * 0.5)],
        vec![(w * 0.62, h * 0.52), (w * 0.75, h * 0.4), (w * 0.9, h * 0.47), (w, h * 0.55)],
        vec![(0.0, h * 0.35), (w * 0.12, h * 0.3), (w * 0.2, h * 0.15), (w * 0.26, 0.0)],
        vec![(w * 0.12, h * 0.3), (w * 0.2, h * 0.5), (w * 0.31, h * 0.35)],
        vec![(w * 0.66, h * 0.8), (w * 0.8, h * 0.9), (w * 0.88, h)],
        vec![(w * 0.6, 0.0), (w * 0.62, h * 0.12), (w * 0.72, h * 0.1), (w * 0.78, 0.0)],
    ];
    b.add(art, ui::strokes("seam shade", &cracks.iter().map(|l| l.iter().map(|(x, y)| (x + 1.5, y + 2.0)).collect()).collect::<Vec<_>>(), 4.0, draw::solid("#00000055")));
    let seams = ui::strokes("seams", &cracks, 3.4, draw::solid("#e2b852"));
    b.add(art, ui::glowing(seams, "#f0c860", 0.25, 4.0));
    // Gold leaves drifting in from the sides.
    let leaves = [(70.0, 70.0, 34.0, 20.0), (140.0, 220.0, 28.0, -30.0), (60.0, 300.0, 22.0, 50.0), (220.0, 330.0, 16.0, 10.0), (w - 150.0, 60.0, 36.0, -15.0), (w - 70.0, 170.0, 26.0, 35.0), (w - 190.0, 320.0, 30.0, -40.0), (w - 60.0, 330.0, 18.0, 5.0), (w * 0.58, h * 0.72, 18.0, 25.0), (w * 0.44, h * 0.9, 12.0, -20.0)];
    let mut d = String::new();
    for (x, y, r, rot) in leaves {
        d.push_str(&leaf(x, y, r, rot));
        d.push(' ');
    }
    let gold = draw::path("leaves", 0.0, 0.0, w, h, d, draw::linear(140.0, &[(0.0, "#fbe7a8"), (0.5, "#cfa24a"), (1.0, "#f2d88a")]));
    b.add(art, draw::with_effects(gold, vec![draw::drop_shadow("#00000066", 1.0, 2.0, 2.0)]));
}

pub(super) fn kintsugi() -> Face {
    const INK: &str = "#2a2418";
    let (w, h) = (HEAD_W, 650.0);
    let top = 58.0;
    let bh = 575.0;
    let scene = canvas("Kintsugi Head", |b, root| {
        parts::page(b, root, w, h);
        for x in [150.0, 1050.0] {
            b.add(root, draw::rounded(draw::rect("foot", x - 50.0, top + bh - 10.0, 100.0, h - top - bh + 10.0, draw::linear(180.0, &[(0.0, "#2a2a2c"), (1.0, "#0e0e10")])), 10.0));
        }
        let body = tolex(b, root, (0.0, top, w, bh), 34.0, ["#f7f3ea", "#ece6da", "#d9d1c2"]);
        let (ax, ay, aw, ah) = (41.0, 38.0, w - 82.0, 368.0);
        let mut art = look::slab("Art", (ax, ay, aw, ah), 10.0, vec![draw::solid("#1c1b1d")]);
        art.clips_content = true;
        art.effects = vec![draw::inner_shadow("#000000c0", 0.0, 3.0, 8.0), draw::drop_shadow("#ffffff80", 0.0, 1.5, 0.0)];
        let art = b.add(body, art);
        kintsugi_art(b, art, aw, ah);
        // The gold control strip.
        let (sx, sy, sw, sh) = (25.0, 439.0, w - 50.0, 108.0);
        let strip = look::slab("Strip", (sx, sy, sw, sh), 8.0, vec![
            draw::linear(180.0, &[(0.0, "#f3dc9a"), (0.45, "#c9a55a"), (0.55, "#b8944a"), (1.0, "#e6c77e")]),
            draw::linear(90.0, &[(0.0, "#ffffff00"), (0.3, "#ffffff22"), (0.5, "#ffffff00"), (0.8, "#ffffff1a"), (1.0, "#ffffff00")]),
        ]);
        let strip = b.add(body, draw::with_effects(draw::stroked(strip, "#7a5a26", 2.0), vec![draw::drop_shadow("#00000080", 0.0, 3.0, 6.0), draw::inner_shadow("#fff6d0aa", 0.0, 2.0, 1.0)]));
        for (x, y) in [(14.0, 14.0), (sw - 14.0, 14.0), (14.0, sh - 14.0), (sw - 14.0, sh - 14.0)] {
            parts::screw(b, strip, x, y, 5.0, "#7a5a26");
        }
        let cy = 53.0;
        let t = Type::new(9.5, INK, 700.0).track(0.12).upper();
        look::brush_mark(b, strip, (80.0, cy), 20.0, INK);
        look::emblem(b, strip, (140.0, cy), 18.0, INK);
        parts::centred(b, strip, "Input", (218.0, 86.0), t);
        push(b, strip, "kintsugi/bright", 26.0, (306.0, cy), true);
        parts::centred(b, strip, "Bright", (306.0, 86.0), t);
        let knobs: [(&str, &str, f64); 7] = [("kintsugi/gain", "Gain", 388.0), ("kintsugi/bass", "Bass", 475.0), ("kintsugi/middle", "Middle", 561.0), ("kintsugi/treble", "Treble", 648.0), ("kintsugi/master", "Master", 808.0), ("kintsugi/presence", "Presence", 894.0), ("kintsugi/output", "Output", 979.0)];
        for (addr, legend, x) in knobs {
            knob(b, strip, "maroon", addr, 52.0, (x, cy - 4.0), &["ticks=0", "lobes=18"], INK);
            parts::centred(b, strip, legend, (x, 90.0), t);
        }
        push(b, strip, "kintsugi/shift", 26.0, (729.0, cy), true);
        parts::centred(b, strip, "Shift", (729.0, 86.0), t);
        push(b, strip, "kintsugi/power", 26.0, (1057.0, cy), true);
        parts::centred(b, strip, "Power", (1057.0, 86.0), t);
        let (jx, jy) = (sx + 218.0, top + sy + cy - 4.0);
        parts::cable(b, root, (jx, jy), 12.0, &[(jx - 60.0, jy + 26.0), (jx - 140.0, jy + 70.0), (jx - 200.0, h - 6.0)]);
        handle(b, root, 390.0, 810.0, top + 4.0, 46.0, 14.0, "#2a2224");
    });
    let params = vec![
        parts::switch("kintsugi/bright", "Bright", false),
        parts::dial10("kintsugi/gain", "Gain", 6.0),
        parts::dial10("kintsugi/bass", "Bass", 5.0),
        parts::dial10("kintsugi/middle", "Middle", 5.0),
        parts::dial10("kintsugi/treble", "Treble", 6.0),
        parts::switch("kintsugi/shift", "Shift", false),
        parts::dial10("kintsugi/master", "Master", 5.0),
        parts::dial10("kintsugi/presence", "Presence", 5.0),
        parts::dial10("kintsugi/output", "Output", 5.0),
        parts::switch("kintsugi/power", "Power", true),
    ];
    amp_face("07-kintsugi-head-1200x650", scene, params)
}
