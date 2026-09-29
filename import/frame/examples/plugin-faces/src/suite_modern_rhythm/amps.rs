//! Modern Rhythm amps: four heads, each a page of its own in the Head form
//! at the plugin faceplate's aspect (1200 × 545, 2.2 : 1 — the manual's
//! straight-on renders), the control strip across the bottom fifth.
//!
//! - **Hex Rhythm** (the plugin's own screenshot): black tolex, a lattice of
//!   interlocking hexagons over four lit power valves, a cream strip of
//!   black pointer knobs with 0–10 round them.
//! - **Croc Clean**: grey crocodile covering the whole front, a scratched
//!   steel strip, cream pointer knobs in tick rings, a BRIGHT toggle.
//! - **Cube Crunch**: cream tolex, an ivory lattice of tumbling cubes over
//!   the valves, a slate strip, black pointer knobs, BRIGHT.
//! - **Cage Lead**: a perforated black steel cage over glowing valves,
//!   spun-cap knobs on a black strip, a wire handle.
//!
//! All four: a red illuminated rocker for power, an input jack with its
//! cable, and a player's scribble (invented) at the right end.

use document_proto::NodeId;
use surface_kit::{Place, draw, motion};
use surface_widgets::Builder;

use crate::delay::parts;
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Letter};

use super::PLUGIN;
use super::common::legend;

pub fn faces() -> Vec<Face> {
    vec![hex_rhythm(), croc_clean(), cube_crunch(), cage_lead()]
}

pub const W: f64 = 1200.0;
pub const H: f64 = 545.0;

// ── the cabinet ──────────────────────────────────────────────────────────

/// A deterministic generator for textures.
struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn circle_d(x: f64, y: f64, r: f64, ccw: bool) -> String {
    let k = 0.5523 * r;
    if ccw {
        format!("M {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} Z ", x + r, x + r, y - k, x + k, y - r, y - r, x - k, y - r, x - r, y - k, x - r, x - r, y + k, x - k, y + r, y + r, x + k, y + r, x + r, y + k, x + r)
    } else {
        format!("M {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} Z ", x + r, x + r, y + k, x + k, y + r, y + r, x - k, y + r, x - r, y + k, x - r, x - r, y - k, x - k, y - r, y - r, x + k, y - r, x + r, y - k, x + r)
    }
}

/// Leather grain on a covering: fine light and dark flecks.
fn grain(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), density: f64, (light, dark): (f64, f64), seed: u64) {
    let mut r = Rng(seed | 1);
    for (c, a) in [("#ffffff", light), ("#000000", dark)] {
        let mut d = String::new();
        let n = (w * h * density) as usize;
        for _ in 0..n {
            let (px, py) = (x + r.f() * w, y + r.f() * h);
            let s = 0.6 + r.f() * 1.4;
            d.push_str(&format!("M {px:.1} {py:.1} l {s:.1} {:.1} l {:.1} {:.1} Z ", s * 0.4, -s * 0.7, s * 0.6));
        }
        b.add(parent, draw::with_opacity(draw::path("grain", 0.0, 0.0, W, H, d, draw::solid(c)), a));
    }
}

/// The feet under the cabinet.
fn feet(b: &mut Builder<'_>, root: NodeId, (x0, x1): (f64, f64), y: f64) {
    for x in [x0, x1] {
        let foot = draw::rounded(draw::rect("foot", x - 45.0, y - 4.0, 90.0, H - y + 2.0, draw::linear(180.0, &[(0.0, "#2c2d30"), (1.0, "#0a0a0b")])), 6.0);
        b.add(root, draw::with_effects(foot, vec![draw::drop_shadow("#00000099", 0.0, 3.0, 6.0)]));
    }
}

/// A black moulded carry handle (strap) on top of the cabinet.
fn strap_handle(b: &mut Builder<'_>, root: NodeId, (cx, top, w): (f64, f64, f64), color: &str) {
    for x in [cx - w / 2.0, cx + w / 2.0 - 34.0] {
        let cap = draw::rounded(draw::rect("handle cap", x, top + 12.0, 34.0, 22.0, draw::linear(180.0, &[(0.0, "#9a9b9d"), (1.0, "#3b3c3e")])), 3.0);
        b.add(root, cap);
    }
    let strap = draw::rounded(draw::rect("handle", cx - w / 2.0 + 14.0, top, w - 28.0, 16.0, draw::linear(180.0, &[(0.0, &parts::mixed(color, "#ffffff", 0.25)), (0.5, color), (1.0, &parts::mixed(color, "#000000", 0.5))])), 8.0);
    b.add(root, draw::with_effects(strap, vec![draw::drop_shadow("#000000a0", 0.0, 3.0, 5.0)]));
}

/// Black metal corner protectors at the cabinet's four corners.
fn corners(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    for (cx, cy, sx, sy) in [(x, y, 1.0, 1.0), (x + w, y, -1.0, 1.0), (x, y + h, 1.0, -1.0), (x + w, y + h, -1.0, -1.0)] {
        let s = 30.0;
        let (x0, y0) = (if sx > 0.0 { cx - 5.0 } else { cx - s + 5.0 }, if sy > 0.0 { cy - 5.0 } else { cy - s + 5.0 });
        let n = draw::rounded(draw::rect("corner", x0, y0, s, s, draw::linear(160.0, &[(0.0, "#4a4c50"), (0.45, "#1c1d20"), (1.0, "#08080a")])), 9.0);
        b.add(root, draw::with_effects(n, vec![draw::drop_shadow("#000000b0", 0.0, 2.0, 3.0), draw::inner_shadow("#ffffff30", 0.0, 1.0, 1.0)]));
        b.add(root, draw::circle("rivet", cx + 7.0 * sx, cy + 7.0 * sy, 2.4, draw::radial(&[(0.0, "#b9babd"), (1.0, "#1b1c1e")])));
    }
}

/// The cabinet box in `covering` paint, rounded; returns nothing — the face
/// lays the front out over it.
fn cabinet(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64), covering: (&str, &str), radius: f64) {
    let n = draw::rounded(draw::rect("cabinet", x, y, w, h, draw::linear(180.0, &[(0.0, covering.0), (1.0, covering.1)])), radius);
    b.add(root, draw::with_effects(draw::stroked(n, "#000000", 1.5), vec![draw::drop_shadow("#000000c0", 0.0, 8.0, 18.0), draw::inner_shadow("#ffffff1c", 0.0, 2.0, 2.0), draw::inner_shadow("#00000080", 0.0, -3.0, 6.0)]));
}

// ── valves ───────────────────────────────────────────────────────────────

/// A power valve seen through the front: glass, dark plates, the getter's
/// silvering at the top, and the heater's orange glow (lit and flickering
/// a little, as a filament does).
fn valve(b: &mut Builder<'_>, parent: NodeId, (cx, top, w, h): (f64, f64, f64, f64), glow: f64) {
    let x = cx - w / 2.0;
    b.add(parent, draw::rounded(draw::rect("valve base", x + w * 0.1, top + h - 18.0, w * 0.8, 18.0, draw::linear(180.0, &[(0.0, "#2b2420"), (1.0, "#0c0a09")])), 3.0));
    let glass = draw::rounded(draw::rect("glass", x, top, w, h - 14.0, draw::linear(90.0, &[(0.0, "#ffffff10"), (0.2, "#ffffff2a"), (0.5, "#ffffff08"), (0.85, "#ffffff22"), (1.0, "#ffffff0c")])), w * 0.42);
    b.add(parent, draw::stroked(glass, "#ffffff26", 1.0));
    b.add(parent, draw::rounded(draw::rect("getter", x + 3.0, top + 3.0, w - 6.0, h * 0.14, draw::linear(180.0, &[(0.0, "#c9c3bd55"), (1.0, "#2a262200")])), w * 0.35));
    let (px, pw) = (cx - w * 0.3, w * 0.6);
    b.add(parent, draw::rect("plates", px, top + h * 0.26, pw, h * 0.5, draw::linear(90.0, &[(0.0, "#141210"), (0.5, "#3a342e"), (1.0, "#141210")])));
    for i in 0..3 {
        let gx = px + pw * (0.2 + 0.3 * f64::from(i));
        let g = draw::rect("heater", gx - 1.5, top + h * 0.3, 3.0, h * 0.42, draw::linear(180.0, &[(0.0, "#ffb86a"), (0.5, "#ff7a1a"), (1.0, "#ffb86a")]));
        let id = b.add(parent, draw::emissive(g, draw::hex("#ff7a1a"), 0.9 * glow, 14.0));
        motion::add(b, id, motion::flicker(0.08));
    }
    let halo = draw::circle("glow", cx, top + h * 0.5, w * 0.7, draw::radial(&[(0.0, "#ff8a3a55"), (1.0, "#ff8a3a00")]));
    b.add(parent, draw::with_opacity(halo, glow));
}

/// Four big power valves and four small preamp valves across a chassis
/// window at `(x, y, w, h)`.
fn valve_row(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), glow: f64) {
    b.add(parent, draw::rect("chassis", x, y, w, h, draw::linear(180.0, &[(0.0, "#0a0908"), (0.7, "#171411"), (1.0, "#0c0a09")])));
    let cx = x + w / 2.0;
    let pitch = w * 0.12;
    for i in 0..4 {
        let vx = cx + (f64::from(i) - 1.5) * pitch;
        valve(b, parent, (vx - pitch * 0.45, y + h * 0.44, pitch * 0.34, h * 0.52), glow * 0.7);
    }
    for i in 0..4 {
        let vx = cx + (f64::from(i) - 1.5) * pitch;
        valve(b, parent, (vx + pitch * 0.05, y + h * 0.1, pitch * 0.5, h * 0.86), glow);
    }
}

// ── grilles ──────────────────────────────────────────────────────────────

/// A lattice of interlocking hexagons: each cell a hexagon with a smaller
/// one inside, joined by three spokes — cut from steel, the gaps showing
/// what is behind.
fn hex_lattice(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), r: f64, color: &str, weight: f64) {
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    let dx = r * 3f64.sqrt();
    let dy = r * 1.5;
    let rows = (h / dy).ceil() as i32 + 1;
    let cols = (w / dx).ceil() as i32 + 1;
    for j in 0..=rows {
        for i in -1..=cols {
            let cx = x + f64::from(i) * dx + if j % 2 == 1 { dx / 2.0 } else { 0.0 };
            let cy = y + f64::from(j) * dy;
            let hexp = |rr: f64, rot: f64| -> Vec<(f64, f64)> { (0..=6).map(|k| draw::polar(cx, cy, rr, rot + 60.0 * f64::from(k))).collect() };
            lines.push(hexp(r, 0.0));
            lines.push(hexp(r * 0.48, 30.0));
            for k in 0..3 {
                let a = 60.0 + 120.0 * f64::from(k);
                lines.push(vec![draw::polar(cx, cy, r * 0.48 * 0.866, a), draw::polar(cx, cy, r, a - 30.0)]);
            }
        }
    }
    let mut frame = draw::frame("lattice", x, y, w, h);
    frame.clips_content = true;
    let f = b.add(parent, frame);
    let shifted: Vec<Vec<(f64, f64)>> = lines.iter().map(|l| l.iter().map(|(px, py)| (px - x, py - y)).collect()).collect();
    let n = parts::stroke_path("lattice", (0.0, 0.0, w, h), &shifted, weight, color, false);
    b.add(f, draw::with_effects(n, vec![draw::drop_shadow("#000000c0", 0.0, 2.0, 3.0)]));
}

/// A lattice of tumbling cubes (isometric): hexagon outlines with three
/// edges in from alternate corners.
fn cube_lattice(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), r: f64, color: &str, weight: f64) {
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    let dx = r * 3f64.sqrt();
    let dy = r * 1.5;
    let rows = (h / dy).ceil() as i32 + 1;
    let cols = (w / dx).ceil() as i32 + 1;
    for j in 0..=rows {
        for i in -1..=cols {
            let cx = x + f64::from(i) * dx + if j % 2 == 1 { dx / 2.0 } else { 0.0 };
            let cy = y + f64::from(j) * dy;
            lines.push((0..=6).map(|k| draw::polar(cx, cy, r, 60.0 * f64::from(k))).collect());
            for k in 0..3 {
                let a = 120.0 * f64::from(k) + if (i + j) % 2 == 0 { 0.0 } else { 60.0 };
                lines.push(vec![(cx, cy), draw::polar(cx, cy, r, a)]);
                // A second, inset line per face: the lattice's double bars.
                lines.push(vec![draw::polar(cx, cy, r * 0.5, a + 60.0), draw::polar(cx, cy, r * 0.5, a + 60.0 + 60.0)]);
            }
        }
    }
    let mut frame = draw::frame("lattice", x, y, w, h);
    frame.clips_content = true;
    let f = b.add(parent, frame);
    let shifted: Vec<Vec<(f64, f64)>> = lines.iter().map(|l| l.iter().map(|(px, py)| (px - x, py - y)).collect()).collect();
    let n = parts::stroke_path("lattice", (0.0, 0.0, w, h), &shifted, weight, color, false);
    b.add(f, draw::with_effects(n, vec![draw::drop_shadow("#00000099", 0.0, 1.5, 2.0)]));
}

/// A perforated steel sheet: a staggered grid of round holes (cut out, so
/// what is behind shows), with the sheet's own sheen.
fn perforated(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), pitch: f64, hole: f64) {
    let mut d = format!("M {x:.2} {y:.2} L {:.2} {y:.2} L {:.2} {:.2} L {x:.2} {:.2} Z ", x + w, x + w, y + h, y + h);
    let rows = (h / (pitch * 0.866)) as i32;
    let cols = (w / pitch) as i32;
    for j in 0..rows {
        for i in 0..cols {
            let cx = x + pitch * 0.6 + f64::from(i) * pitch + if j % 2 == 1 { pitch / 2.0 } else { 0.0 };
            let cy = y + pitch * 0.55 + f64::from(j) * pitch * 0.866;
            if cx + hole > x + w - 2.0 {
                continue;
            }
            d.push_str(&circle_d(cx, cy, hole, true));
        }
    }
    let n = draw::path("cage", 0.0, 0.0, W, H, d, draw::linear(170.0, &[(0.0, "#3a3b3e"), (0.5, "#222326"), (1.0, "#141517")]));
    b.add(parent, draw::with_effects(n, vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 4.0)]));
}

// ── the strip ────────────────────────────────────────────────────────────

/// The control strip plate at `(x, y, w, h)` in `paint`, a bevelled edge.
fn strip(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64), paint: (&str, &str), scratches: bool, seed: u64) {
    let n = draw::rect("strip", x, y, w, h, draw::linear(180.0, &[(0.0, paint.0), (1.0, paint.1)]));
    b.add(root, draw::with_effects(draw::stroked(n, "#00000080", 1.0), vec![draw::inner_shadow("#ffffff30", 0.0, 1.5, 1.0), draw::inner_shadow("#00000066", 0.0, -2.0, 3.0), draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)]));
    if scratches {
        let mut r = Rng(seed | 1);
        let mut d = String::new();
        for _ in 0..90 {
            let (px, py) = (x + r.f() * w, y + 4.0 + r.f() * (h - 8.0));
            let (lx, ly) = (6.0 + r.f() * 28.0, (r.f() - 0.5) * 10.0);
            d.push_str(&format!("M {px:.1} {py:.1} l {lx:.1} {ly:.1} l 0.5 0.4 l {:.1} {:.1} Z ", -lx, -ly));
        }
        b.add(root, draw::with_opacity(draw::path("scratches", 0.0, 0.0, W, H, d, draw::solid("#ffffff")), 0.07));
        grain(b, root, (x, y, w, h), 0.02, (0.05, 0.08), seed ^ 0x55);
    }
}

/// An amp knob: `id` style at (`cx`, `cy`), its legend cut above.
#[allow(clippy::too_many_arguments)]
fn knob(b: &mut Builder<'_>, p: NodeId, id: &str, variant: &str, opts: &[&str], addr: &str, name: &str, (cx, cy): (f64, f64), d: f64, ink: &str, legend_y: f64) {
    let mut place = Place::new(name).size(d).variant(variant).bind(rk::bind(addr)).touch(surface_kit::TOUCH_MIN).ink(surface_kit::Ink::Color(ink.into()));
    place.options = Some(opts.iter().map(ToString::to_string).collect());
    let k = parts::put(b, p, id, &place, cx, cy);
    parts::strip_legend(b, k);
    legend(b, k, &name.to_uppercase(), (cx, legend_y), Letter::new(10.5, 760.0, ink).track(0.06));
}

/// The input jack with its cable running off to the left and down.
fn jack(b: &mut Builder<'_>, root: NodeId, (cx, cy): (f64, f64)) {
    let cable = [(cx, cy), (cx - 8.0, cy + 30.0), (cx - 40.0, cy + 80.0), (cx - 90.0, cy + 110.0), (cx - 160.0, cy + 122.0)];
    rk::curve(b, root, "cable shadow", (W, H), &cable.iter().map(|(x, y)| (x + 2.0, y + 5.0)).collect::<Vec<_>>(), 12.0, "#00000080");
    rk::curve(b, root, "cable", (W, H), &cable, 10.0, "#161618");
    rk::curve(b, root, "cable sheen", (W, H), &cable.iter().map(|(x, y)| (x - 1.5, y - 1.5)).collect::<Vec<_>>(), 2.0, "#ffffff22");
    b.add(root, draw::circle("jack nut", cx, cy, 15.0, draw::conic(20.0, &[(0.0, "#e9e9e7"), (0.25, "#6f7072"), (0.5, "#d9d9d7"), (0.75, "#5a5b5d"), (1.0, "#e9e9e7")])));
    b.add(root, draw::circle("plug", cx, cy, 9.0, draw::radial(&[(0.0, "#3a3b3e"), (1.0, "#0b0b0c")])));
}

/// The player's scribble: an invented signature-like flourish.
fn scribble(b: &mut Builder<'_>, root: NodeId, (cx, cy): (f64, f64), color: &str) {
    let pts = [
        (cx - 26.0, cy + 12.0), (cx - 24.0, cy - 14.0), (cx - 22.0, cy + 8.0), (cx - 12.0, cy - 20.0), (cx - 8.0, cy + 10.0), (cx - 2.0, cy - 2.0), (cx + 4.0, cy + 8.0),
        (cx + 10.0, cy - 8.0), (cx + 12.0, cy + 9.0), (cx + 20.0, cy - 6.0), (cx + 24.0, cy + 14.0), (cx + 30.0, cy + 2.0),
    ];
    rk::curve(b, root, "scribble", (W, H), &pts, 1.6, color);
    rk::curve(b, root, "scribble", (W, H), &[(cx - 30.0, cy + 18.0), (cx, cy + 20.0), (cx + 34.0, cy + 15.0)], 1.3, color);
}

/// The red power rocker centred on (`cx`, `cy`).
fn power(b: &mut Builder<'_>, root: NodeId, addr: &str, (cx, cy): (f64, f64), h: f64) {
    let p = Place::new("Power").size(h).variant("red").bind(rk::bind(addr)).touch(surface_kit::TOUCH_MIN);
    parts::put(b, root, "switch.lamp_rocker", &p, cx, cy);
}

/// A thin vertical separator printed on the strip.
fn separator(b: &mut Builder<'_>, root: NodeId, x: f64, (y0, y1): (f64, f64), color: &str) {
    parts::vrule(b, root, x, y0, y1, 1.0, color);
}

fn params(ns: &str, knobs: &[(&str, &str, f64)], toggles: &[(&str, &str, bool)]) -> Vec<surface_proto::ParamInfo> {
    let mut v: Vec<surface_proto::ParamInfo> = knobs
        .iter()
        .map(|(a, n, d)| {
            let p = rk::dial10(&format!("{ns}/{a}"), n, *d);
            if matches!(*a, "gain" | "master" | "output") { p.featured() } else { p }
        })
        .collect();
    for (a, n, on) in toggles {
        v.push(rk::toggle(&format!("{ns}/{a}"), n, *on));
    }
    v
}

// ── 05 · Hex Rhythm ──────────────────────────────────────────────────────

/// Hex Rhythm — Head, 1200 × 545 (the plugin's faceplate, 2.2 : 1). Black
/// leather-grain tolex, corner protectors, a black steel hex lattice over
/// the lit valves, a cream strip: GAIN · BASS MID TREBLE · RESONANCE
/// PRESENCE MASTER OUTPUT, black pointer knobs with 0–10.
fn hex_rhythm() -> Face {
    let ns = "mrhex";
    let knobs: [(&str, &str, f64); 8] = [("gain", "Gain", 6.0), ("bass", "Bass", 5.0), ("mid", "Mid", 5.0), ("treble", "Treble", 5.5), ("resonance", "Resonance", 5.0), ("presence", "Presence", 5.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Hex Rhythm", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (210.0, 1010.0), 510.0);
        strap_handle(b, root, (W / 2.0 + 10.0, 14.0, 300.0), "#1c1d1f");
        let cab = (84.0, 66.0, 1090.0, 446.0);
        cabinet(b, root, cab, ("#26272a", "#141416"), 10.0);
        grain(b, root, cab, 0.06, (0.05, 0.25), 0x4e11);
        corners(b, root, cab);
        // The grille window: valves behind a hex lattice, a black frame round it.
        let g = (118.0, 100.0, 1022.0, 240.0);
        b.add(root, draw::with_effects(draw::rect("grille frame", g.0 - 6.0, g.1 - 6.0, g.2 + 12.0, g.3 + 12.0, draw::solid("#0b0b0c")), vec![draw::inner_shadow("#000000", 0.0, 2.0, 4.0)]));
        valve_row(b, root, g, 1.0);
        hex_lattice(b, root, g, 38.0, "#2a2b2f", 7.5);
        for (x, y) in [(g.0 + 3.0, g.1 + 3.0), (g.0 + g.2 - 3.0, g.1 + 3.0), (g.0 + 3.0, g.1 + g.3 - 3.0), (g.0 + g.2 - 3.0, g.1 + g.3 - 3.0)] {
            b.add(root, draw::circle("screw", x, y, 3.0, draw::radial(&[(0.0, "#8a8b8d"), (1.0, "#1a1a1c")])));
        }
        // The strip.
        let s = (118.0, 358.0, 1022.0, 124.0);
        strip(b, root, s, ("#ebe6da", "#dcd6c8"), false, 1);
        const INK: &str = "#1a1a1a";
        let xs = [245.0, 330.0, 429.0, 527.0, 623.0, 714.0, 809.0, 905.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "knob.amp_pointer", "black", &["numerals"], &format!("{ns}/{a}"), n, (x, 430.0), 34.0, INK, 390.0);
        }
        separator(b, root, 287.0, (386.0, 470.0), "#1a1a1a90");
        separator(b, root, 576.0, (386.0, 470.0), "#1a1a1a90");
        jack(b, root, (156.0, 422.0));
        power(b, root, &format!("{ns}/power"), (988.0, 424.0), 58.0);
        scribble(b, root, (1072.0, 424.0), "#1a1a1a");
    });
    let params = params(ns, &knobs, &[("power", "Power", true)]);
    Face { file: "05-amp-head-hex-rhythm", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 06 · Croc Clean ──────────────────────────────────────────────────────

/// Crocodile covering: rows of irregular rounded scales with dark seams.
fn croc(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64), seed: u64) {
    let mut r = Rng(seed | 1);
    let mut light = String::new();
    let mut pale = String::new();
    let mut dark = String::new();
    // Columns of scales, each column its own run of heights (the hide's
    // scales run in vertical bands, as the head's covering shows them).
    let mut xx = x;
    while xx < x + w {
        let cw = 9.0 + r.f() * 8.0;
        let mut yy = y - r.f() * 20.0;
        while yy < y + h {
            let rh = 11.0 + r.f() * 11.0;
            let (x0, x1) = ((xx + 0.9).max(x), (xx + cw - 0.9).min(x + w));
            let (y0, y1) = ((yy + 0.9).max(y), (yy + rh - 0.9).min(y + h));
            if x1 > x0 + 2.0 && y1 > y0 + 2.0 {
                let rr = 4.0f64.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
                let light = if r.f() < 0.35 { &mut pale } else { &mut light };
                light.push_str(&format!(
                    "M {:.1} {y0:.1} L {:.1} {y0:.1} Q {x1:.1} {y0:.1} {x1:.1} {:.1} L {x1:.1} {:.1} Q {x1:.1} {y1:.1} {:.1} {y1:.1} L {:.1} {y1:.1} Q {x0:.1} {y1:.1} {x0:.1} {:.1} L {x0:.1} {:.1} Q {x0:.1} {y0:.1} {:.1} {y0:.1} Z ",
                    x0 + rr, x1 - rr, y0 + rr, y1 - rr, x1 - rr, x0 + rr, y1 - rr, y0 + rr, x0 + rr
                ));
                dark.push_str(&format!("M {:.1} {:.1} L {x1:.1} {:.1} L {x1:.1} {y1:.1} L {:.1} {y1:.1} Z ", x0 + 1.5, y1 - 3.0, y1 - 3.0, x0 + 1.5));
            }
            yy += rh;
        }
        xx += cw;
    }
    b.add(root, draw::rect("croc seams", x, y, w, h, draw::solid("#77736c")));
    b.add(root, draw::path("croc scales", 0.0, 0.0, W, H, light, draw::linear(170.0, &[(0.0, "#46433f"), (0.5, "#34322e"), (1.0, "#282624")])));
    b.add(root, draw::path("croc scales pale", 0.0, 0.0, W, H, pale, draw::linear(170.0, &[(0.0, "#524e49"), (0.5, "#3d3a36"), (1.0, "#2e2c29")])));
    b.add(root, draw::with_opacity(draw::path("croc shade", 0.0, 0.0, W, H, dark, draw::solid("#000000")), 0.35));
}

/// Croc Clean — Head, 1200 × 545. Grey crocodile over the whole front, a
/// scratched steel strip: BRIGHT, then GAIN BASS MIDDLE TREBLE MASTER
/// OUTPUT on cream pointer knobs inside tick rings.
fn croc_clean() -> Face {
    let ns = "mrcroc";
    let knobs: [(&str, &str, f64); 6] = [("gain", "Gain", 4.0), ("bass", "Bass", 5.0), ("middle", "Middle", 5.0), ("treble", "Treble", 6.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Croc Clean", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (240.0, 1010.0), 505.0);
        // A straight bar handle with chrome end caps.
        let bar = draw::rounded(draw::rect("handle", 380.0, 22.0, 440.0, 10.0, draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#0c0c0d")])), 5.0);
        b.add(root, draw::with_effects(bar, vec![draw::drop_shadow("#00000099", 0.0, 3.0, 4.0)]));
        for x in [355.0, 815.0] {
            b.add(root, draw::rounded(draw::rect("handle cap", x, 18.0, 110.0 * 0.3, 20.0, draw::linear(180.0, &[(0.0, "#e2e3e5"), (1.0, "#7c7d80")])), 3.0));
        }
        let cab = (100.0, 40.0, 1070.0, 470.0);
        cabinet(b, root, cab, ("#34322f", "#1d1c1a"), 10.0);
        croc(b, root, (112.0, 52.0, 1046.0, 446.0), 0xc40c);
        corners(b, root, cab);
        // The front's upper edge: a dark piping line.
        b.add(root, draw::stroked(draw::rect("front frame", 140.0, 76.0, 990.0, 290.0, draw::solid("#00000000")), "#1a1917", 3.0));
        let s = (135.0, 364.0, 1000.0, 124.0);
        strip(b, root, s, ("#5a5c5f", "#3a3c3f"), true, 0x5c7a);
        const INK: &str = "#f2f2f0";
        let xs = [357.0, 463.0, 568.0, 673.0, 777.0, 884.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "knob.amp_pointer", "cream", &["ticks=21"], &format!("{ns}/{a}"), n, (x, 430.0), 40.0, INK, 384.0);
        }
        let t = rk::mini_toggle(b, root, "Bright", &format!("{ns}/bright"), (279.0, 430.0 - 6.0), 14.0, "chrome");
        legend(b, t, "BRIGHT", (279.0, 395.0), Letter::new(10.0, 760.0, INK).track(0.06));
        jack(b, root, (202.0, 426.0));
        power(b, root, &format!("{ns}/power"), (985.0, 424.0), 58.0);
        scribble(b, root, (1072.0, 424.0), "#f2f2f0");
    });
    let params = params(ns, &knobs, &[("bright", "Bright", false), ("power", "Power", true)]);
    Face { file: "06-amp-head-croc-clean", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 07 · Cube Crunch ─────────────────────────────────────────────────────

/// Cube Crunch — Head, 1200 × 545. Cream tolex with chrome corners and a
/// brown leather handle, an ivory lattice of tumbling cubes over pale
/// valves, a slate strip: BRIGHT, GAIN · BASS MID TREBLE · PRESENCE DEPTH
/// MASTER OUTPUT on black pointer knobs with 0–10.
fn cube_crunch() -> Face {
    let ns = "mrcube";
    let knobs: [(&str, &str, f64); 8] = [("gain", "Gain", 5.0), ("bass", "Bass", 5.0), ("mid", "Mid", 5.0), ("treble", "Treble", 5.0), ("presence", "Presence", 5.0), ("depth", "Depth", 5.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Cube Crunch", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (200.0, 1030.0), 508.0);
        strap_handle(b, root, (W / 2.0 - 25.0, 10.0, 300.0), "#9c5a3a");
        let cab = (84.0, 62.0, 1090.0, 448.0);
        cabinet(b, root, cab, ("#ece8dd", "#d8d2c3"), 10.0);
        grain(b, root, cab, 0.05, (0.2, 0.08), 0xc0be);
        // Chrome corners instead of black.
        for (cx, cy) in [(cab.0 + 6.0, cab.1 + 6.0), (cab.0 + cab.2 - 6.0, cab.1 + 6.0), (cab.0 + 6.0, cab.1 + cab.3 - 6.0), (cab.0 + cab.2 - 6.0, cab.1 + cab.3 - 6.0)] {
            b.add(root, draw::with_effects(draw::circle("corner", cx, cy, 11.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff"), (0.5, "#a5a6a8"), (1.0, "#4b4c4e")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
        }
        let g = (120.0, 96.0, 1018.0, 244.0);
        b.add(root, draw::rect("grille back", g.0, g.1, g.2, g.3, draw::solid("#0b0b0b")));
        valve_row(b, root, g, 0.35);
        // The lattice itself, ivory, with its own faint frosting over the valves.
        cube_lattice(b, root, g, 30.0, "#ece9e1", 5.0);
        b.add(root, draw::stroked(draw::rect("grille edge", g.0, g.1, g.2, g.3, draw::solid("#00000000")), "#c9c4b6", 3.0));
        let s = (120.0, 360.0, 1018.0, 124.0);
        strip(b, root, s, ("#44474b", "#2e3134"), true, 0xc7);
        const INK: &str = "#f0f0ee";
        let xs = [301.0, 391.0, 476.0, 561.0, 651.0, 741.0, 815.0, 914.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "knob.amp_pointer", "black", &["numerals"], &format!("{ns}/{a}"), n, (x, 428.0), 30.0, INK, 384.0);
        }
        separator(b, root, 346.0, (380.0, 465.0), "#f0f0ee70");
        separator(b, root, 608.0, (380.0, 465.0), "#f0f0ee70");
        let t = rk::mini_toggle(b, root, "Bright", &format!("{ns}/bright"), (227.0, 430.0 - 6.0), 14.0, "chrome");
        legend(b, t, "BRIGHT", (227.0, 392.0), Letter::new(9.5, 760.0, INK).track(0.06));
        jack(b, root, (170.0, 422.0));
        power(b, root, &format!("{ns}/power"), (1000.0, 424.0), 56.0);
        scribble(b, root, (1080.0, 424.0), "#f0f0ee");
    });
    let params = params(ns, &knobs, &[("bright", "Bright", false), ("power", "Power", true)]);
    Face { file: "07-amp-head-cube-crunch", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 08 · Cage Lead ───────────────────────────────────────────────────────

/// Cage Lead — Head, 1200 × 545. A black perforated steel cage over four
/// glowing valves, a bent-wire handle, a black scratched strip: GAIN BASS
/// MIDDLE TREBLE MASTER OUTPUT on black spun-cap knobs in tick rings.
fn cage_lead() -> Face {
    let ns = "mrcage";
    let knobs: [(&str, &str, f64); 6] = [("gain", "Gain", 7.0), ("bass", "Bass", 5.0), ("middle", "Middle", 5.0), ("treble", "Treble", 5.0), ("master", "Master", 5.0), ("output", "Output", 5.0)];
    let scene = canvas("Cage Lead", |b, root| {
        parts::fit_root(b, root, W, H);
        feet(b, root, (270.0, 1000.0), 505.0);
        // The bent-wire handle.
        let wire = [(450.0, 92.0), (450.0, 30.0), (455.0, 24.0), (770.0, 24.0), (775.0, 30.0), (775.0, 92.0)];
        parts::line(b, root, "handle shadow", &wire.iter().map(|(x, y)| (x + 2.0, y + 3.0)).collect::<Vec<_>>(), 6.0, "#00000080");
        parts::line(b, root, "handle", &wire, 5.0, "#2e2f31");
        parts::line(b, root, "handle sheen", &wire.iter().map(|(x, y)| (x - 1.0, y - 1.0)).collect::<Vec<_>>(), 1.2, "#8a8b8d");
        let cab = (118.0, 90.0, 1005.0, 418.0);
        cabinet(b, root, cab, ("#26272a", "#111214"), 6.0);
        let g = (126.0, 98.0, 989.0, 262.0);
        valve_row(b, root, g, 1.0);
        perforated(b, root, g, 17.0, 5.6);
        // A folded lip where the cage meets the strip.
        b.add(root, draw::rect("lip", 118.0, 358.0, 1005.0, 8.0, draw::linear(180.0, &[(0.0, "#3b3c3f"), (1.0, "#0e0f10")])));
        let s = (118.0, 366.0, 1005.0, 132.0);
        strip(b, root, s, ("#2a2b2e", "#17181a"), true, 0xca9e);
        const INK: &str = "#eeeeec";
        let xs = [277.0, 395.0, 513.0, 632.0, 752.0, 871.0];
        for ((a, n, _), x) in knobs.iter().zip(xs) {
            knob(b, root, "knob.spun_cap", "black", &["ticks=21"], &format!("{ns}/{a}"), n, (x, 444.0), 44.0, INK, 400.0);
        }
        jack(b, root, (178.0, 432.0));
        power(b, root, &format!("{ns}/power"), (962.0, 440.0), 56.0);
        scribble(b, root, (1050.0, 440.0), "#eeeeec");
    });
    let params = params(ns, &knobs, &[("power", "Power", true)]);
    Face { file: "08-amp-head-cage-lead", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
