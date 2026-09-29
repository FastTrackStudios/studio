//! The boutique suite's own look: its brush-stroke maker's mark, the carved
//! walnut plates, the chevron name row, brass hardware placed by centre,
//! and the soft teal lamp.

use document_proto::{Node, NodeId};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, Type};

pub const PLUGIN: &str = "suite-boutique-clean";
/// The suite's lamp colour.
pub const TEAL: &str = "#3ff0e6";
/// Printed ink on the cream enclosures.
pub const INK: &str = "#161514";

/// A face in this suite, its readouts formatted.
#[must_use]
pub fn face(file: &'static str, scene: document_proto::Scene, params: Vec<surface_proto::ParamInfo>) -> Face {
    ui::formatted(Face { file, plugin: PLUGIN, scene, params, streams: Vec::new() })
}

/// A face with streams.
#[must_use]
pub fn face_with(file: &'static str, scene: document_proto::Scene, params: Vec<surface_proto::ParamInfo>, streams: Vec<surface_proto::StreamInfo>) -> Face {
    ui::formatted(Face { file, plugin: PLUGIN, scene, params, streams })
}

/// The maker's mark: an invented brush mark — an open ensō, a falling
/// stroke and a wave (no letters in any script) — set where the rig prints
/// its lettering, `s` px tall, centred at (`cx`, `cy`).
pub fn brush_mark(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), s: f64, color: &str) {
    let u = s / 10.0;
    let at = |x: f64, y: f64| (cx + x * u, cy + y * u);
    let curve = |pts: &[(f64, f64)]| pts.iter().map(|(x, y)| at(*x, *y)).collect::<Vec<_>>();
    // An open ensō, left.
    let enso: Vec<(f64, f64)> = (0..=28)
        .map(|i| {
            let a = std::f64::consts::TAU * (0.08 + 0.84 * f64::from(i) / 28.0);
            at(-7.5 + 4.6 * a.sin(), 0.2 - 4.6 * a.cos())
        })
        .collect();
    // A wave, right.
    let wave: Vec<(f64, f64)> = (0..=16)
        .map(|i| {
            let t = f64::from(i) / 16.0;
            at(3.0 + 10.0 * t, 1.0 - 2.2 * (t * std::f64::consts::TAU).sin())
        })
        .collect();
    let lines = vec![enso, curve(&[(-0.8, -5.2), (0.6, -1.0), (0.2, 4.6)]), wave];

    parts::art(b, parent, &lines, (1.1 * u).max(0.9), color);
}

/// The rig's emblem, invented: a small lotus in a circle.
pub fn emblem(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, color: &str) {
    let petal = |a: f64| {
        let tip = draw::polar(cx, cy + r * 0.25, r * 0.85, a);
        let l = draw::polar(cx, cy + r * 0.25, r * 0.3, a - 40.0);
        let rr = draw::polar(cx, cy + r * 0.25, r * 0.3, a + 40.0);
        vec![(cx, cy + r * 0.25), l, tip, rr, (cx, cy + r * 0.25)]
    };
    let lines = vec![petal(0.0), petal(-45.0), petal(45.0), vec![(cx - r * 0.7, cy + r * 0.45), (cx + r * 0.7, cy + r * 0.45)]];
    parts::art(b, parent, &lines, (r * 0.09).max(0.9), color);
}

/// A carved walnut plate `(x, y, w, h)`: returns the clipped plate frame to
/// carve into.
pub fn walnut(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    let mut plate = draw::frame("Walnut", x, y, w, h);
    plate.fills = vec![
        draw::linear(95.0, &[(0.0, "#6c3320"), (0.3, "#8a4630"), (0.55, "#743924"), (0.8, "#8c4a32"), (1.0, "#663021")]),
        draw::linear(180.0, &[(0.0, "#ffffff14"), (1.0, "#00000026")]),
    ];
    plate.corner_radii = document_proto::CornerRadii::uniform(3.5);
    plate.clips_content = true;
    plate.stroke = document_proto::Stroke { paints: vec![draw::solid("#2c150c")], weight: 1.2, ..document_proto::Stroke::default() };
    plate.effects = vec![draw::inner_shadow("#000000a0", 0.0, 1.5, 3.0), draw::drop_shadow("#ffffff66", 0.0, 0.8, 0.0)];
    b.add(parent, plate)
}

/// Carve `lines` into a plate: a dark groove with a lit lower lip.
pub fn carve(b: &mut Builder<'_>, plate: NodeId, lines: &[Vec<(f64, f64)>], weight: f64) {
    let lit: Vec<Vec<(f64, f64)>> = lines.iter().map(|l| l.iter().map(|(x, y)| (*x + weight * 0.35, *y + weight * 0.45)).collect()).collect();
    b.add(plate, ui::strokes("carve light", &lit, weight * 0.8, draw::solid("#c07a5a66")));
    b.add(plate, ui::strokes("carve", lines, weight, draw::solid("#3a1a0fd0")));
}

/// The `> >>>> NAME >>>> >` row across a pedal at `cy`.
pub fn name_row(b: &mut Builder<'_>, parent: NodeId, name: &str, w: f64, cy: f64, ink: &str) {
    let chevron = |x: f64, h: f64| vec![(x - h * 0.45, cy - h), (x + h * 0.45, cy), (x - h * 0.45, cy + h)];
    let mut big = vec![chevron(16.0, 5.0), chevron(w - 16.0, 5.0)];
    let mut small = Vec::new();
    for i in 0..4 {
        small.push(chevron(33.0 + 4.3 * f64::from(i), 1.8));
        small.push(chevron(w - 46.0 + 4.3 * f64::from(i), 1.8));
    }
    big.truncate(2);
    parts::art(b, parent, &big, 1.9, ink);
    parts::art(b, parent, &small, 0.7, ink);
    parts::centred(b, parent, name, (w / 2.0, cy), Type::new(12.5, ink, 800.0).track(0.08));
}

/// A brass stomp on `addr`, its cap `d` across, centred at `at`.
pub fn stomp(b: &mut Builder<'_>, parent: NodeId, addr: &str, d: f64, at: (f64, f64), variant: &str) -> NodeId {
    let p = Place::new("Bypass").size(d).variant(variant).bind(ui::bind(addr)).touch(TOUCH_MIN);
    parts::part(b, parent, "switch.brass_stomp", p, &["cap"], at)
}

/// A brass bat toggle on toggle `addr`, nut `d`, centred at `at`.
pub fn brass_toggle(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, d: f64, at: (f64, f64)) -> NodeId {
    let p = Place::new(name).size(d).bind(ui::bind(addr)).touch(TOUCH_MIN);
    parts::part(b, parent, "switch.brass_toggle", p, &["bushing"], at)
}

/// A three-way brass toggle on choice `addr` (options right, down, left).
pub fn brass_toggle3(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, d: f64, at: (f64, f64)) -> NodeId {
    let p = Place::new(name).size(d).bind(ui::bind(addr)).touch(TOUCH_MIN);
    parts::part(b, parent, "switch.brass_toggle3", p, &["bushing"], at)
}

/// A rounded-rect frame with fills — the building block of cabinets.
#[must_use]
pub fn slab(name: &str, (x, y, w, h): (f64, f64, f64, f64), r: f64, fills: Vec<document_proto::Paint>) -> Node {
    let mut n = draw::frame(name, x, y, w, h);
    n.fills = fills;
    n.corner_radii = document_proto::CornerRadii::uniform(r);
    n
}
