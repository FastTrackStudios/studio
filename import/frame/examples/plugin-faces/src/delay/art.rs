//! Hardware the kit doesn't have, drawn from plain nodes: tape reels and a
//! head block, a stompbox enclosure and its footswitch, a valve behind a
//! window, a transformer's core and windings, a bucket-brigade chip, and
//! the stroked traces a display draws (echo trains, transfer curves).
//!
//! Geometry only — paths are filled (the renderer fills, it doesn't
//! stroke), so a line is a ribbon of quads and a ring is two circles wound
//! against each other (non-zero fill leaves the hole).

use document_proto::{NodeId, TextAlign};
use surface_kit::draw;
use surface_proto::{Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{self, Ink, bind, emit, hexa, mixed};

// ── geometry ─────────────────────────────────────────────────────────────

/// A full circle as a path, wound clockwise or (for a hole) counter-.
#[must_use]
pub fn circle_d(cx: f64, cy: f64, r: f64, ccw: bool) -> String {
    let s = i32::from(!ccw);
    format!("M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 {s} {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 {s} {:.3} {cy:.3} Z ", cx - r, cx + r, cx - r)
}

/// A ring between `r_in` and `r_out`.
#[must_use]
pub fn ring_d(cx: f64, cy: f64, r_in: f64, r_out: f64) -> String {
    format!("{}{}", circle_d(cx, cy, r_out, false), circle_d(cx, cy, r_in, true))
}

/// A closed polygon, optionally wound backwards (a hole).
#[must_use]
pub fn poly_d(points: &[(f64, f64)], hole: bool) -> String {
    let mut pts = points.to_vec();
    if hole {
        pts.reverse();
    }
    draw::polygon(&pts) + " "
}

/// A stroked polyline: a ribbon `w` wide through `pts`, each quad run a
/// little into the next so the joints stay closed.
#[must_use]
pub fn ribbon(pts: &[(f64, f64)], w: f64) -> String {
    let hw = w / 2.0;
    let mut d = String::new();
    for s in pts.windows(2) {
        let ((ax, ay), (bx, by)) = (s[0], s[1]);
        let len = (bx - ax).hypot(by - ay).max(1e-6);
        let (nx, ny) = (-(by - ay) / len * hw, (bx - ax) / len * hw);
        let (ex, ey) = ((bx - ax) / len * hw * 0.8, (by - ay) / len * hw * 0.8);
        d.push_str(&draw::polygon(&[(ax + nx, ay + ny), (bx + ex + nx, by + ey + ny), (bx + ex - nx, by + ey - ny), (ax - nx, ay - ny)]));
        d.push(' ');
    }
    d
}

/// A ring sector as a path, `a0..a1` degrees (0 = up, clockwise).
#[must_use]
pub fn sector_d(cx: f64, cy: f64, r_in: f64, r_out: f64, a0: f64, a1: f64, hole: bool) -> String {
    let n = 24;
    let mut pts = Vec::with_capacity(2 * n + 2);
    for i in 0..=n {
        pts.push(draw::polar(cx, cy, r_out, a0 + (a1 - a0) * i as f64 / n as f64));
    }
    for i in (0..=n).rev() {
        pts.push(draw::polar(cx, cy, r_in, a0 + (a1 - a0) * i as f64 / n as f64));
    }
    poly_d(&pts, hole)
}

/// A path drawn in `parent`'s own coordinates, its box the parent's. Its
/// geometry must stay inside that box (the renderer fits geometry that
/// strays outside its node's box back into it).
#[must_use]
fn over(b: &Builder<'_>, parent: NodeId, name: &str, d: String, fill: document_proto::Paint) -> document_proto::Node {
    let (w, h) = parts::size_of(b, parent);
    draw::path(name, 0.0, 0.0, w, h, d, fill)
}

// ── fasteners ────────────────────────────────────────────────────────────

/// A panel screw: a domed head with a slot at `angle`.
pub fn screw(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, angle: f64) {
    let head = draw::circle("screw", cx, cy, r, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f2f2ef"), (0.45, "#a9aaa6"), (1.0, "#4e4f4c")]));
    b.add(parent, draw::with_effects(head, vec![draw::drop_shadow("#00000099", 0.0, 0.8, 1.2)]));
    // The slot in its own box round the head: a path's geometry has to
    // stay inside its node's box, and a screw can sit on its parent's edge.
    let (x0, y0) = draw::polar(r, r, r * 0.8, angle);
    let (x1, y1) = draw::polar(r, r, r * 0.8, angle + 180.0);
    let d = ribbon(&[(x0, y0), (x1, y1)], r * 0.32);
    b.add(parent, draw::path("screw slot", cx - r, cy - r, 2.0 * r, 2.0 * r, d, draw::solid("#2a2a28cc")));
}

// ── tape ─────────────────────────────────────────────────────────────────

/// Metals and plastics a reel can be made of.
#[derive(Clone, Copy)]
pub struct Reel<'a> {
    /// Flange stops, light to dark.
    pub flange: (&'a str, &'a str, &'a str),
    /// The tape pack's oxide.
    pub oxide: &'a str,
}

/// A tape reel seen face-on: the oxide pack wound to `fill` of the radius,
/// under a flange with three windows cut through it, and the hub and
/// spindle. Decoration.
pub fn reel(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, fill: f64, rot: f64, s: Reel<'_>) {
    let side = 2.0 * r + 8.0;
    let (ox, oy) = (cx - side / 2.0, cy - side / 2.0);
    let c = side / 2.0;
    let root = b.add(parent, draw::frame("reel", ox, oy, side, side));
    // Its shadow on the deck.
    b.add(root, draw::with_effects(draw::circle("reel shadow", c + 1.5, c + 3.0, r, draw::solid("#00000066")), vec![draw::drop_shadow("#00000080", 0.0, 3.0, 8.0)]));
    // The pack: oxide, faintly banded where the winding tension changed.
    let rp = r * fill;
    let pack = draw::circle("tape pack", c, c, rp, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, &mixed(s.oxide, "#000000", 0.4)), (0.3, s.oxide), (0.92, &mixed(s.oxide, "#ffffff", 0.08)), (1.0, &mixed(s.oxide, "#000000", 0.35))]));
    b.add(root, pack);
    let mut bands = String::new();
    for k in 1..6 {
        let rr = r * 0.26 + (rp - r * 0.26) * f64::from(k) / 6.0;
        bands.push_str(&ring_d(c, c, rr - 0.35, rr + 0.35));
    }
    b.add(root, draw::with_opacity(draw::path("pack bands", 0.0, 0.0, side, side, bands, draw::solid("#000000")), 0.25));
    // The flange: a disc with three windows. Windows are holes (wound
    // backwards), so the pack and the deck show through.
    let mut d = circle_d(c, c, r, false);
    for i in 0..3 {
        let a = rot + 120.0 * f64::from(i);
        d.push_str(&sector_d(c, c, r * 0.34, r * 0.86, a + 12.0, a + 108.0, true));
    }
    d.push_str(&circle_d(c, c, r * 0.2, true));
    let flange = draw::path("flange", 0.0, 0.0, side, side, d, draw::conic(rot + 20.0, &[(0.0, s.flange.0), (0.18, s.flange.1), (0.4, s.flange.0), (0.62, s.flange.2), (0.82, s.flange.0), (1.0, s.flange.0)]));
    b.add(root, draw::with_opacity(flange, 0.92));
    // The flange's rim and the windows' edges catch the light.
    let mut rim = ring_d(c, c, r - 1.2, r);
    for i in 0..3 {
        let a = rot + 120.0 * f64::from(i);
        rim.push_str(&sector_d(c, c, r * 0.34 - 0.8, r * 0.34, a + 12.0, a + 108.0, false));
    }
    b.add(root, draw::with_opacity(draw::path("flange rim", 0.0, 0.0, side, side, rim, draw::solid("#ffffff")), 0.35));
    // Hub and spindle: a dark boss with three drive dogs.
    b.add(root, draw::circle("hub", c, c, r * 0.2, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#56585c"), (1.0, "#101113")])));
    let mut dogs = String::new();
    for i in 0..3 {
        let a = rot + 60.0 + 120.0 * f64::from(i);
        let (x0, y0) = draw::polar(c, c, r * 0.07, a);
        let (x1, y1) = draw::polar(c, c, r * 0.18, a);
        dogs.push_str(&ribbon(&[(x0, y0), (x1, y1)], r * 0.05));
    }
    b.add(root, draw::path("drive dogs", 0.0, 0.0, side, side, dogs, draw::solid("#c9cbcf")));
    b.add(root, draw::circle("spindle", c, c, r * 0.065, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#ffffff"), (0.5, "#b8babd"), (1.0, "#5c5e62")])));
}

/// A chrome tape guide / capstan post.
pub fn post(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64) {
    let n = draw::circle("post", cx, cy, r, draw::radial_at(0.38, 0.32, 0.85, &[(0.0, "#ffffff"), (0.35, "#c4c6c9"), (1.0, "#46484c")]));
    b.add(parent, draw::with_effects(n, vec![draw::drop_shadow("#000000a0", 0.5, 1.5, 2.5)]));
}

/// A rubber pinch roller.
pub fn roller(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64) {
    let n = draw::circle("pinch roller", cx, cy, r, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#4a4b4e"), (0.7, "#1a1b1d"), (1.0, "#060607")]));
    b.add(parent, draw::with_effects(n, vec![draw::drop_shadow("#000000a0", 0.5, 1.5, 3.0)]));
    b.add(parent, draw::circle("roller hub", cx, cy, r * 0.3, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#e8e9eb"), (1.0, "#6a6c70")])));
}

/// The tape itself: a ribbon of oxide through `pts`.
pub fn tape(b: &mut Builder<'_>, parent: NodeId, pts: &[(f64, f64)], w: f64, oxide: &str) {
    let d = ribbon(pts, w);
    let n = over(b, parent, "tape", d, draw::solid(oxide));
    b.add(parent, n);
}

/// A tape head face-on: a mu-metal can with its gap, `w × h` centred on
/// (`cx`, `cy`).
pub fn head(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), w: f64, h: f64) {
    let can = draw::rounded(draw::rect("head", cx - w / 2.0, cy - h / 2.0, w, h, draw::linear(90.0, &[(0.0, "#6d7075"), (0.3, "#e3e5e8"), (0.55, "#aeb1b6"), (1.0, "#55585d")])), w * 0.22);
    b.add(parent, draw::with_effects(draw::stroked(can, "#1c1d1f", 0.8), vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 2.5)]));
    b.add(parent, draw::rect("head gap", cx - 0.45, cy - h / 2.0 + 2.0, 0.9, h - 4.0, draw::solid("#1a1b1d")));
}

// ── stompbox ─────────────────────────────────────────────────────────────

/// A die-cast stompbox enclosure, `w × h`, in `paint` (top to bottom):
/// rounded, bevelled, with the four lid screws. Returns the lid (a parent
/// for the rest).
///
/// `hammer` is the hammered finish's (dimple, highlight) opacity — (0, 0)
/// for a smooth paint; a light paint wants faint dimples.
pub fn enclosure(b: &mut Builder<'_>, parent: NodeId, (w, h): (f64, f64), paint: &[(f64, &str)], hammer: (f64, f64)) -> NodeId {
    // Its shadow, then the casting's side wall showing below the lid.
    b.add(parent, draw::with_effects(draw::rounded(draw::rect("case shadow", 6.0, 10.0, w - 12.0, h - 12.0, draw::solid("#00000080")), 22.0), vec![draw::drop_shadow("#000000a0", 0.0, 6.0, 12.0)]));
    let wall = draw::rounded(draw::rect("case wall", 4.0, 4.0, w - 8.0, h - 6.0, draw::linear(180.0, &[(0.0, &mixed(paint[paint.len() - 1].1, "#000000", 0.35)), (1.0, &mixed(paint[paint.len() - 1].1, "#000000", 0.6))])), 22.0);
    b.add(parent, wall);
    let mut lid = draw::frame("lid", 4.0, 2.0, w - 8.0, h - 12.0);
    lid.fills = vec![draw::linear(180.0, paint)];
    let lid = draw::with_effects(draw::stroked(draw::rounded(lid, 20.0), "#00000080", 1.0), vec![draw::inner_shadow("#ffffff40", 0.0, 2.0, 2.0), draw::inner_shadow("#00000059", 0.0, -3.0, 6.0)]);
    let lid = b.add(parent, lid);
    let (lw, lh) = (w - 8.0, h - 12.0);
    if hammer.0 > 0.0 || hammer.1 > 0.0 {
        // A hammered finish: a scatter of soft dimples, lighter up top.
        let mut light = String::new();
        let mut dark = String::new();
        let mut seed: u32 = 0x5eed_1e55;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f64::from(seed >> 8) / f64::from(1_u32 << 24)
        };
        for _ in 0..900 {
            let (x, y) = (14.0 + rnd() * (lw - 28.0), 12.0 + rnd() * (lh - 24.0));
            let r = 2.0 + rnd() * 4.5;
            dark.push_str(&circle_d(x, y, r, false));
            light.push_str(&circle_d(x + r * 0.12, y + r * 0.15, r * 0.8, false));
        }
        b.add(lid, draw::with_opacity(draw::path("hammer dark", 0.0, 0.0, lw, lh, dark, draw::solid("#000000")), hammer.0));
        b.add(lid, draw::with_opacity(draw::path("hammer light", 0.0, 0.0, lw, lh, light, draw::solid("#ffffff")), hammer.1));
    }
    for (x, y) in [(18.0, 16.0), (lw - 18.0, 16.0), (18.0, lh - 16.0), (lw - 18.0, lh - 16.0)] {
        screw(b, lid, (x, y), 5.0, 35.0);
    }
    lid
}

/// A stomp footswitch: a hex nut round a chrome plunger cap, `d` across,
/// with its status LED `led_dy` above. A `Footswitch` over a toggle param;
/// the LED is the `Led` slot. The control's box spans switch and LED.
#[allow(clippy::too_many_arguments)]
pub fn footswitch(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64), d: f64, led_dy: f64, led: &str) -> NodeId {
    let top = cy - led_dy - 8.0;
    let (bw, bh) = (d * 1.25, cy + d * 0.62 - top);
    let root = b.add(parent, draw::frame(name, cx - bw / 2.0, top, bw, bh));
    let (x, y) = (bw / 2.0, cy - top);
    // LED in its chrome bezel.
    let ly = y - led_dy;
    b.add(root, draw::circle("led bezel", x, ly, 6.2, draw::radial_at(0.4, 0.3, 0.8, &[(0.0, "#f4f4f2"), (1.0, "#6c6d6a")])));
    b.add(root, parts::dark_lens(x, ly, 4.2, led));
    let lit = b.add(root, parts::lens(x, ly, 4.2, led));
    // Nut, washer, plunger.
    let nut = draw::path("nut", 0.0, 0.0, bw, bh, draw::regular(x, y, d * 0.5, 6, 30.0), draw::linear(160.0, &[(0.0, "#f0f1f2"), (0.45, "#9a9da2"), (1.0, "#3c3e42")]));
    b.add(root, draw::with_effects(draw::circle("nut shadow", x, y + 3.0, d * 0.46, draw::solid("#00000099")), vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 6.0)]));
    b.add(root, nut);
    b.add(root, draw::circle("washer", x, y, d * 0.4, draw::conic(10.0, &[(0.0, "#f2f2f0"), (0.25, "#8f9194"), (0.5, "#e4e5e6"), (0.75, "#7b7d80"), (1.0, "#f2f2f0")])));
    let cap = draw::circle("plunger", x, y, d * 0.3, draw::radial_at(0.38, 0.32, 0.85, &[(0.0, "#ffffff"), (0.3, "#d9dbde"), (0.75, "#8a8d92"), (1.0, "#4a4c50")]));
    b.add(root, draw::with_effects(cap, vec![draw::drop_shadow("#00000080", 0.0, 1.5, 2.0)]));
    let mut c = Control::new(ControlKind::Footswitch { hold_ms: 500 }).slot(SlotRole::Led, lit.0);
    c.binding = Some(bind(addr));
    b.controls.insert(root, c);
    root
}

/// A jack socket on the enclosure's top edge, with its legend.
pub fn jack(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), legend: &str, ink: Ink<'_>) {
    b.add(parent, draw::circle("jack nut", cx, cy, 9.0, draw::linear(160.0, &[(0.0, "#eeeeec"), (1.0, "#55565a")])));
    b.add(parent, draw::circle("jack", cx, cy, 5.0, draw::solid("#0b0b0c")));
    parts::silk(b, parent, legend, cx, cy + 18.0, ink);
}

// ── valves ───────────────────────────────────────────────────────────────

/// A preamp valve standing in its socket, `w` wide and `h` tall from the
/// top of the glass to the socket, its heater glowing `glow` (0..1).
pub fn valve(b: &mut Builder<'_>, parent: NodeId, (cx, top): (f64, f64), (w, h): (f64, f64), glow: f64) {
    let root = b.add(parent, draw::frame("valve", cx - w / 2.0 - 12.0, top - 12.0, w + 24.0, h + 24.0));
    let (x0, y0) = (12.0, 12.0);
    let glass_h = h - 16.0;
    // Warm light on whatever is behind it.
    let halo = draw::circle("halo", x0 + w / 2.0, y0 + glass_h * 0.66, w * 1.1, draw::radial(&[(0.0, &hexa("#ff8a2a", 0.5 * glow)), (0.45, &hexa("#ff6a1a", 0.18 * glow)), (1.0, "#ff6a1a00")]));
    b.add(root, halo);
    // Internals: mica spacers, the plate structure, the heater.
    let (ix, iw) = (x0 + w * 0.2, w * 0.6);
    let plate_y = y0 + glass_h * 0.3;
    let plate_h = glass_h * 0.46;
    b.add(root, draw::rect("mica top", ix - 2.0, plate_y - 4.0, iw + 4.0, 3.0, draw::solid("#d9d4c4c0")));
    b.add(root, draw::rect("mica bottom", ix - 2.0, plate_y + plate_h + 1.0, iw + 4.0, 3.0, draw::solid("#d9d4c4c0")));
    let plates = draw::rect("plates", ix, plate_y, iw, plate_h, draw::linear(90.0, &[(0.0, "#2c2d2f"), (0.2, "#5c5e62"), (0.5, "#3b3c3f"), (0.8, "#5c5e62"), (1.0, "#2c2d2f")]));
    b.add(root, plates);
    // The slot in the plates the cathode glows through.
    let slot = draw::rounded(draw::rect("cathode", x0 + w / 2.0 - w * 0.07, plate_y + plate_h * 0.14, w * 0.14, plate_h * 0.72, draw::linear(90.0, &[(0.0, "#ff6a1a"), (0.5, "#ffd08a"), (1.0, "#ff6a1a")])), 2.0);
    let slot = draw::with_effects(slot, vec![draw::drop_shadow(&hexa("#ff7a22", 0.9 * glow), 0.0, 0.0, 7.0)]);
    b.add(root, draw::with_opacity(emit(slot, "#ff8a2a", 1.5 * glow, 16.0), 0.4 + 0.6 * glow));
    // Heater filament glow below the plates.
    let fil = draw::circle("filament", x0 + w / 2.0, plate_y + plate_h + 7.0, w * 0.1, draw::radial(&[(0.0, "#ffe2b0"), (0.5, "#ff8a2a"), (1.0, "#ff8a2a00")]));
    b.add(root, emit(fil, "#ff8a2a", 1.2 * glow, 12.0));
    // Glass: a rounded envelope with a nipple on top and a getter flash.
    let glass = draw::rounded(draw::rect("glass", x0, y0, w, glass_h, draw::linear(90.0, &[(0.0, "#ffffff30"), (0.12, "#ffffff10"), (0.5, "#ffffff05"), (0.85, "#ffffff16"), (1.0, "#ffffff2e")])), w * 0.48);
    b.add(root, draw::stroked(glass, "#ffffff40", 1.0));
    let getter = draw::rounded(draw::rect("getter", x0 + 3.0, y0 + 4.0, w - 6.0, glass_h * 0.16, draw::linear(180.0, &[(0.0, "#c9c6c0d0"), (0.6, "#6b6863b0"), (1.0, "#2e2c2a20")])), w * 0.44);
    b.add(root, getter);
    b.add(root, draw::rounded(draw::rect("nipple", x0 + w / 2.0 - 3.0, y0 - 4.0, 6.0, 6.0, draw::solid("#ffffff40")), 3.0));
    // A glint down the glass.
    b.add(root, draw::rounded(draw::rect("glint", x0 + w * 0.16, y0 + glass_h * 0.12, w * 0.07, glass_h * 0.7, draw::linear(180.0, &[(0.0, "#ffffff00"), (0.3, "#ffffff80"), (1.0, "#ffffff00")])), 2.0));
    // Base and socket.
    let base = draw::rounded(draw::rect("base", x0 - 3.0, y0 + glass_h - 2.0, w + 6.0, 14.0, draw::linear(180.0, &[(0.0, "#3a3633"), (1.0, "#0f0e0d")])), 3.0);
    b.add(root, draw::with_effects(base, vec![draw::drop_shadow("#00000099", 0.0, 2.0, 3.0)]));
}

// ── iron ─────────────────────────────────────────────────────────────────

/// A transformer face-on: a laminated E-I core with its copper winding
/// showing in the window, clamped by a steel frame, `w × h` from (`x`, `y`).
pub fn transformer(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), heat: f64) {
    let root = b.add(parent, draw::frame("transformer", x, y, w, h));
    b.add(root, draw::with_effects(draw::rounded(draw::rect("shadow", 3.0, 5.0, w - 2.0, h - 2.0, draw::solid("#00000070")), 3.0), vec![draw::drop_shadow("#00000090", 0.0, 4.0, 8.0)]));
    // Core: laminations as fine horizontal stripes.
    let core = draw::rect("core", 0.0, 0.0, w, h, draw::linear(90.0, &[(0.0, "#3e4247"), (0.5, "#5d6268"), (1.0, "#34373b")]));
    b.add(root, draw::stroked(core, "#16181a", 1.0));
    let mut lam = String::new();
    let mut yy = 2.0;
    while yy < h {
        lam.push_str(&format!("M 0 {yy:.2} L {w:.2} {yy:.2} L {w:.2} {:.2} L 0 {:.2} Z ", yy + 0.6, yy + 0.6));
        yy += 2.4;
    }
    b.add(root, draw::with_opacity(draw::path("laminations", 0.0, 0.0, w, h, lam, draw::solid("#0c0d0e")), 0.45));
    // The two windows of the E, with the winding filling them.
    let (wx, ww) = (w * 0.16, w * 0.68);
    let (wy, wh) = (h * 0.2, h * 0.6);
    let copper = draw::rect("winding", wx, wy, ww, wh, draw::linear(180.0, &[(0.0, "#e0a064"), (0.35, "#b8692f"), (0.7, "#8c4a1e"), (1.0, "#5e2f12")]));
    b.add(root, draw::with_effects(copper, vec![draw::inner_shadow("#000000a0", 0.0, 0.0, 4.0)]));
    let mut turns = String::new();
    let mut xx = wx + 1.5;
    while xx < wx + ww {
        turns.push_str(&format!("M {xx:.2} {wy:.2} L {:.2} {wy:.2} L {:.2} {:.2} L {xx:.2} {:.2} Z ", xx + 0.7, xx + 0.7, wy + wh, wy + wh));
        xx += 2.2;
    }
    b.add(root, draw::with_opacity(draw::path("turns", 0.0, 0.0, w, h, turns, draw::solid("#3a1a08")), 0.5));
    // The centre leg of the E through the middle of the winding.
    b.add(root, draw::stroked(draw::rect("centre leg", w * 0.43, wy, w * 0.14, wh, draw::linear(90.0, &[(0.0, "#3e4247"), (0.5, "#666b71"), (1.0, "#34373b")])), "#16181a", 0.8));
    // Heat in the copper as it's driven.
    let hot = draw::rect("heat", wx, wy, ww, wh, draw::radial(&[(0.0, &hexa("#ff7a2a", 0.45 * heat)), (1.0, "#ff7a2a00")]));
    b.add(root, emit(hot, "#ff7a2a", 0.6 * heat, 18.0));
    // Clamp frame: two steel straps with bolts.
    for sy in [-3.0, h - 5.0] {
        let strap = draw::rect("strap", -6.0, sy, w + 12.0, 8.0, draw::linear(180.0, &[(0.0, "#d8dadd"), (0.5, "#8d9095"), (1.0, "#4a4d51")]));
        b.add(root, draw::with_effects(draw::stroked(strap, "#1f2123", 0.6), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
        screw(b, root, (-1.0, sy + 4.0), 3.2, 20.0);
        screw(b, root, (w + 1.0, sy + 4.0), 3.2, 70.0);
    }
}

// ── chips ────────────────────────────────────────────────────────────────

/// A DIP chip, `pins` a side, `w × h` body centred on (`cx`, `cy`), with
/// its part number printed on it.
#[allow(clippy::too_many_arguments)]
pub fn chip(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), (w, h): (f64, f64), pins: usize, part: &str, maker: &str) {
    let (x0, y0) = (cx - w / 2.0, cy - h / 2.0);
    let pitch = w / pins as f64;
    for i in 0..pins {
        let px = x0 + pitch * (i as f64 + 0.5) - 2.5;
        for (py, ph) in [(y0 - 7.0, 8.0), (y0 + h - 1.0, 8.0)] {
            b.add(parent, draw::rect("pin", px, py, 5.0, ph, draw::linear(90.0, &[(0.0, "#8d9095"), (0.5, "#eef0f2"), (1.0, "#8d9095")])));
        }
    }
    let body = draw::rounded(draw::rect("package", x0, y0, w, h, draw::linear(180.0, &[(0.0, "#2a2b2d"), (1.0, "#121314")])), 2.5);
    b.add(parent, draw::with_effects(body, vec![draw::drop_shadow("#000000b0", 0.0, 3.0, 5.0), draw::inner_shadow("#ffffff1a", 0.0, 1.0, 0.0)]));
    b.add(parent, draw::circle("notch", x0 + 1.0, cy, 4.0, draw::solid("#0b0b0c")));
    b.add(parent, draw::circle("pin 1", x0 + 9.0, y0 + h - 8.0, 2.0, draw::solid("#060607")));
    parts::silk(b, parent, part, cx, cy - 5.0, Ink::new(10.0, "#c9c6bf").weight(700.0).track(0.12));
    parts::silk(b, parent, maker, cx, cy + 9.0, Ink::new(6.0, "#8d8a84").weight(600.0).track(0.2));
}

// ── traces ───────────────────────────────────────────────────────────────

/// A lit trace on a display: the ribbon through `pts` in `color`, `w` wide,
/// with a glow round it.
pub fn trace(b: &mut Builder<'_>, parent: NodeId, name: &str, pts: &[(f64, f64)], w: f64, color: &str) -> NodeId {
    for (k, a) in [(5.0, 0.08), (3.0, 0.16), (1.8, 0.3)] {
        let halo = over(b, parent, "glow", ribbon(pts, w * k), draw::solid(&hexa(color, a)));
        b.add(parent, halo);
    }
    let n = over(b, parent, name, ribbon(pts, w), draw::solid(color));
    b.add(parent, emit(n, color, 1.0, w * 4.0))
}

/// A lit bar on a display (an echo's level, a pattern step).
#[allow(clippy::too_many_arguments)]
pub fn bar(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), color: &str, alpha: f64) -> NodeId {
    let n = draw::rounded(draw::rect("bar", x, y, w, h, draw::linear(180.0, &[(0.0, &mixed(color, "#ffffff", 0.35)), (1.0, color)])), (w * 0.25).min(2.0));
    let n = draw::with_opacity(draw::with_effects(n, vec![draw::drop_shadow(&hexa(color, 0.6), 0.0, 0.0, 5.0)]), alpha);
    b.add(parent, emit(n, color, 0.8 * alpha, 8.0))
}

/// Faint graticule over a display: `nx × ny` cells in `(w, h)`.
pub fn graticule(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), (nx, ny): (u32, u32), color: &str) {
    let mut d = String::new();
    for i in 0..=nx {
        let xx = x + w * f64::from(i) / f64::from(nx);
        d.push_str(&format!("M {:.2} {y:.2} L {:.2} {y:.2} L {:.2} {:.2} L {:.2} {:.2} Z ", xx - 0.35, xx + 0.35, xx + 0.35, y + h, xx - 0.35, y + h));
    }
    for j in 0..=ny {
        let yy = y + h * f64::from(j) / f64::from(ny);
        d.push_str(&format!("M {x:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2} L {x:.2} {:.2} Z ", yy - 0.35, x + w, yy - 0.35, x + w, yy + 0.35, yy + 0.35));
    }
    let n = over(b, parent, "graticule", d, draw::solid(color));
    b.add(parent, n);
}

/// Tiny lit label on a display.
pub fn tag(b: &mut Builder<'_>, parent: NodeId, s: &str, (x, cy): (f64, f64), size: f64, color: &str) {
    parts::lit(b, parent, s, (x, cy), Ink::new(size, color).weight(700.0).track(0.14), TextAlign::Left);
}

/// A shape node in `parent`'s coordinates (public for faces that draw
/// their own).
pub fn path_in(b: &mut Builder<'_>, parent: NodeId, name: &str, d: String, fill: document_proto::Paint) -> NodeId {
    let n = over(b, parent, name, d, fill);
    b.add(parent, n)
}
