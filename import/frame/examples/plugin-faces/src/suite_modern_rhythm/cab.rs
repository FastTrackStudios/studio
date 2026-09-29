//! Modern Rhythm cab: a closed-back 4×12 behind a black honeycomb mesh, on
//! casters — Cab form, 900 × 900 — with the suite's mic strip under it, as
//! the plugin's cab page lays its controls out round the cabinet: the mic
//! (six generic models, and a custom IR slot) with its on switch, POSITION,
//! DISTANCE, MIC LEVEL and PAN on flat dark dials with value arcs, ROOM
//! SEND and its LEVEL, and PHASE. A mic stands at the upper-left cone.

use document_proto::{NodeId, TextAlign};
use surface_kit::draw;
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole, ValueFormat};
use surface_widgets::Builder;

use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Letter};

use super::PLUGIN;

const W: f64 = 900.0;
const H: f64 = 900.0;
const INK: &str = "#e4e4e6";
const DIM: &str = "#9a9ba0";
const MICS: &[&str] = &["Dynamic A", "Dynamic A Off-Axis", "Dynamic B", "Condenser A", "Condenser B", "Ribbon A", "Ribbon B", "Custom IR"];

pub fn faces() -> Vec<Face> {
    vec![four_by_twelve()]
}

/// The plugin's own flat dial: a dark disc in a track ring, a light arc
/// from the start of travel to the value, a pointer line; the reading
/// printed under it and the name under that.
#[allow(clippy::too_many_arguments)]
pub fn ui_knob(b: &mut Builder<'_>, p: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64), d: f64, format: Option<ValueFormat>) -> NodeId {
    let w = d.max(44.0) + 30.0;
    let h = d + 38.0;
    let root = b.add(p, draw::frame(name, cx - w / 2.0, cy - d / 2.0, w, h));
    let dial = b.add(root, draw::frame("dial", (w - d) / 2.0, 0.0, d, d));
    let ring = (d * 0.11).max(3.0);
    let mut track = draw::path("track", 0.0, 0.0, d, d, surface_widgets::sector_path(d * 0.5, d * 0.5, d * 0.5 - ring, d * 0.5, -150f64.to_radians(), 150f64.to_radians()), draw::solid("#3a3b3f"));
    track.name = "track".into();
    b.add(dial, track);
    let mut arc = document_proto::Node::new(document_proto::NodeKind::Ellipse, "value-arc");
    arc.size = document_proto::Size { width: d, height: d };
    arc.fills = vec![draw::solid("#f2f2f2")];
    let arc = b.add(dial, arc);
    let inset = ring + d * 0.06;
    let cap = draw::circle("cap", d / 2.0, d / 2.0, d / 2.0 - inset, draw::linear(180.0, &[(0.0, "#3a3b3f"), (1.0, "#232427")]));
    b.add(dial, draw::with_effects(draw::stroked(cap, "#48494e", 1.0), vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)]));
    let pw = (d * 0.05).max(2.0);
    let pointer = b.add(dial, draw::rounded(draw::rect("pointer", d / 2.0 - pw / 2.0, inset + d * 0.05, pw, (d / 2.0 - inset) * 0.8, draw::solid("#f2f2f2")), pw / 2.0));
    let mut v = parts::text_node("", 0.0, d + 12.0, w, &Ink::new(11.0, INK).weight(600.0).track(0.0).plain(), TextAlign::Center);
    v.name = "value".into();
    let value = b.add(root, v);
    let label = rk::letter(b, root, &name.to_uppercase(), (w / 2.0, d + 30.0), Letter::new(9.5, 640.0, DIM).track(0.04));
    let mut c = Control::new(ControlKind::Knob { sweep_degrees: 300.0 }).slot(SlotRole::ValueArc, arc.0).slot(SlotRole::Needle, pointer.0).slot(SlotRole::Value, value.0).slot(SlotRole::Label, label.0);
    c.binding = Some(rk::bind(addr));
    c.format = format;
    b.controls.insert(root, c);
    root
}

/// A pill toggle (the plugin's switch): a track that lights, a knob that
/// slides — `Thumb` travels right when on.
fn pill(b: &mut Builder<'_>, p: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64)) -> NodeId {
    let root = b.add(p, draw::frame(name, cx - 22.0, cy - 22.0, 44.0, 44.0));
    b.add(root, draw::rounded(draw::rect("track", 6.0, 14.0, 32.0, 16.0, draw::solid("#e8e8ea")), 8.0));
    let lane = b.add(root, draw::frame("lane", 8.0, 16.0, 28.0, 12.0));
    let thumb = b.add(lane, draw::circle("thumb", 6.0, 6.0, 5.5, draw::solid("#141416")));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, thumb.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// An outlined text button (PHASE Ø), latching.
fn outline_button(b: &mut Builder<'_>, p: NodeId, name: &str, addr: &str, (cx, cy, w): (f64, f64, f64)) -> NodeId {
    let root = b.add(p, draw::frame(name, cx - w / 2.0, cy - 22.0, w, 44.0));
    b.add(root, draw::stroked(draw::rounded(draw::rect("outline", 0.0, 6.0, w, 32.0, draw::solid("#00000000")), 5.0), "#8e8f94", 1.4));
    let lit = b.add(root, draw::rounded(draw::rect("latched", 1.0, 7.0, w - 2.0, 30.0, draw::solid("#ffffff22")), 4.0));
    let label = rk::letter(b, root, &format!("{} Ø", name.to_uppercase()), (w / 2.0, 22.0), Letter::new(11.0, 650.0, INK).track(0.04));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, lit.0).slot(SlotRole::Label, label.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// The mic picker: a black box printing the chosen mic, a chevron.
fn mic_picker(b: &mut Builder<'_>, p: NodeId, addr: &str, (x, cy, w): (f64, f64, f64)) -> NodeId {
    let root = b.add(p, draw::rounded(draw::frame("Mic", x, cy - 22.0, w, 44.0), 5.0));
    if let Some(n) = b.scene.node_mut(root) {
        n.fills = vec![draw::solid("#050506")];
    }
    let mut t = parts::text_node("", 14.0, 22.0, w - 40.0, &Ink::new(14.0, INK).weight(500.0).track(0.0).plain(), TextAlign::Left);
    t.name = "value".into();
    let value = b.add(root, t);
    rk::line(b, root, &[(w - 26.0, 19.0), (w - 20.0, 25.0), (w - 14.0, 19.0)], 1.8, INK);
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, value.0);
    c.binding = Some(rk::bind(addr));
    c.format = Some(ValueFormat::labels(MICS));
    b.controls.insert(root, c);
    root
}

/// The mic's power: a round outlined button with the power glyph.
fn mic_power(b: &mut Builder<'_>, p: NodeId, addr: &str, (cx, cy): (f64, f64)) -> NodeId {
    let root = b.add(p, draw::frame("Mic On", cx - 22.0, cy - 22.0, 44.0, 44.0));
    b.add(root, draw::rounded(draw::rect("bg", 0.0, 0.0, 44.0, 44.0, draw::solid("#2a2b2f")), 5.0));
    let arc = draw::path("glyph", 0.0, 0.0, 44.0, 44.0, surface_widgets::sector_path(22.0, 23.0, 8.0, 9.6, 35f64.to_radians(), 325f64.to_radians()), draw::solid(INK));
    let on = b.add(root, draw::emissive(arc, draw::hex(INK), 0.3, 4.0));
    b.add(root, draw::rect("stem", 21.2, 12.0, 1.6, 10.0, draw::solid(INK)));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, on.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A honeycomb mesh over `(x, y, w, h)`: zigzag rows and the short uprights
/// between them, as one stroked path.
fn honeycomb(b: &mut Builder<'_>, p: NodeId, (x, y, w, h): (f64, f64, f64, f64), r: f64, color: &str, weight: f64) {
    let dx = r * 3f64.sqrt();
    let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut row = 0;
    let mut yy = 0.0;
    while yy < h + r {
        let off = if row % 2 == 0 { 0.0 } else { dx / 2.0 };
        let mut zig = Vec::new();
        let mut xx = -dx + off;
        while xx < w + dx {
            zig.push((xx, yy + r * 0.5));
            zig.push((xx + dx / 2.0, yy));
            xx += dx;
        }
        lines.push(zig);
        let mut xx = -dx + off;
        while xx < w + dx {
            lines.push(vec![(xx, yy + r * 0.5), (xx, yy + r * 1.5)]);
            xx += dx;
        }
        yy += r * 1.5;
        row += 1;
    }
    let mut f = draw::frame("mesh", x, y, w, h);
    f.clips_content = true;
    let f = b.add(p, f);
    b.add(f, parts::stroke_path("honeycomb", (0.0, 0.0, w, h), &lines, weight, color, false));
}

/// A 12-inch cone seen through the mesh.
fn speaker(b: &mut Builder<'_>, p: NodeId, (cx, cy): (f64, f64), r: f64) {
    b.add(p, draw::circle("basket", cx, cy, r, draw::radial(&[(0.0, "#1b1b1d"), (0.85, "#101012"), (0.93, "#2a2a2d"), (1.0, "#0a0a0b")])));
    b.add(p, draw::stroked(draw::circle("surround", cx, cy, r * 0.86, draw::radial_at(0.45, 0.4, 0.8, &[(0.0, "#2f3033"), (0.7, "#141416"), (1.0, "#0d0d0e")])), "#3a3b3e", 1.5));
    for k in [0.66, 0.5] {
        b.add(p, draw::stroked(draw::circle("ridge", cx, cy, r * k, draw::solid("#00000000")), "#2a2b2e", 1.0));
    }
    b.add(p, draw::circle("dust cap", cx, cy, r * 0.28, draw::radial_at(0.42, 0.36, 0.8, &[(0.0, "#4a4b4f"), (0.7, "#1e1f22"), (1.0, "#0e0e10")])));
}

/// A dynamic mic aimed in at a cone's edge, from the left.
fn mic(b: &mut Builder<'_>, p: NodeId, (tip_x, tip_y): (f64, f64)) {
    let len = 130.0;
    let body = draw::rounded(draw::rect("mic body", tip_x - len, tip_y - 10.0, len - 28.0, 20.0, draw::linear(180.0, &[(0.0, "#5a5b5f"), (0.35, "#2a2b2e"), (1.0, "#0e0e10")])), 6.0);
    b.add(p, draw::with_effects(draw::rotated(body, 16.0), vec![draw::drop_shadow("#000000b0", 3.0, 6.0, 8.0)]));
    let grille = draw::rounded(draw::rect("mic grille", tip_x - 34.0, tip_y - 12.0, 34.0, 24.0, draw::linear(180.0, &[(0.0, "#b9babc"), (0.5, "#6c6d70"), (1.0, "#3a3b3d")])), 10.0);
    b.add(p, draw::rotated(grille, 16.0));
}

fn four_by_twelve() -> Face {
    let ns = "mrcab";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("4x12 Cabinet", |b, root| {
        parts::fit_root(b, root, W, H);
        // Casters.
        for x in [130.0, 390.0, 510.0, 770.0] {
            b.add(root, draw::rect("caster mount", x - 22.0, 690.0, 44.0, 10.0, draw::solid("#3a3b3e")));
            b.add(root, draw::rect("caster fork", x - 10.0, 698.0, 20.0, 18.0, draw::linear(90.0, &[(0.0, "#8a8b8e"), (1.0, "#3a3b3e")])));
            b.add(root, draw::rounded(draw::rect("caster wheel", x - 16.0, 712.0, 32.0, 30.0, draw::radial(&[(0.0, "#9a9b9e"), (0.4, "#2a2b2e"), (1.0, "#0c0c0d")])), 12.0));
        }
        // The cabinet: black tolex, a rim, corner caps.
        let cab = draw::rounded(draw::rect("cabinet", 70.0, 20.0, 760.0, 680.0, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#141416")])), 8.0);
        b.add(root, draw::with_effects(draw::stroked(cab, "#000000", 1.5), vec![draw::drop_shadow("#000000c0", 0.0, 10.0, 20.0), draw::inner_shadow("#ffffff1c", 0.0, 2.0, 2.0)]));
        for (x, y) in [(70.0, 20.0), (830.0, 20.0), (70.0, 700.0), (830.0, 700.0)] {
            let c = draw::rounded(draw::rect("corner", x - 12.0, y - 12.0, 24.0, 24.0, draw::linear(150.0, &[(0.0, "#8a8b8e"), (0.5, "#3a3b3e"), (1.0, "#111113")])), 5.0);
            b.add(root, draw::with_effects(c, vec![draw::drop_shadow("#00000099", 0.0, 1.0, 2.0)]));
        }
        let g = (100.0, 50.0, 700.0, 620.0);
        b.add(root, draw::with_effects(draw::rect("baffle", g.0, g.1, g.2, g.3, draw::solid("#0c0c0d")), vec![draw::inner_shadow("#000000", 0.0, 3.0, 6.0)]));
        for (sx, sy) in [(270.0, 205.0), (630.0, 205.0), (270.0, 515.0), (630.0, 515.0)] {
            speaker(b, root, (sx, sy), 142.0);
        }
        mic(b, root, (190.0, 200.0));
        honeycomb(b, root, g, 5.0, "#1c1d20", 1.3);
        b.add(root, draw::with_opacity(draw::rect("mesh sheen", g.0, g.1, g.2, g.3, draw::linear(160.0, &[(0.0, "#ffffff14"), (0.5, "#ffffff00"), (1.0, "#00000030")])), 1.0));

        // The mic strip.
        let strip = draw::rounded(draw::rect("mic strip", 10.0, 758.0, 880.0, 134.0, draw::linear(180.0, &[(0.0, "#1e1f22"), (1.0, "#141517")])), 10.0);
        b.add(root, draw::with_effects(draw::stroked(strip, "#2c2d31", 1.0), vec![draw::drop_shadow("#00000099", 0.0, 3.0, 8.0)]));
        mic_picker(b, root, &a("mic"), (26.0, 800.0, 196.0));
        mic_power(b, root, &a("mic_on"), (248.0, 800.0));
        rk::letter_aligned(b, root, "MIC", (28.0, 846.0), Letter::new(9.5, 640.0, DIM).track(0.04), TextAlign::Left);
        let x0 = 330.0;
        let pitch = 88.0;
        let fmt3 = Some(ValueFormat { decimals: Some(3), unit: Some(String::new()), ..ValueFormat::default() });
        ui_knob(b, root, "Position", &a("position"), (x0, 800.0), 42.0, fmt3.clone());
        ui_knob(b, root, "Distance", &a("distance"), (x0 + pitch, 800.0), 42.0, fmt3);
        ui_knob(b, root, "Mic Level", &a("level"), (x0 + 2.0 * pitch, 800.0), 42.0, Some(ValueFormat { decimals: Some(1), ..ValueFormat::default() }));
        ui_knob(b, root, "Pan", &a("pan"), (x0 + 3.0 * pitch, 800.0), 42.0, Some(ValueFormat { decimals: Some(0), unit: Some(String::new()), ..ValueFormat::default() }));
        parts::vrule(b, root, 700.0, 776.0, 874.0, 1.0, "#3a3b3f");
        rk::letter(b, root, "ROOM SEND", (755.0, 780.0), Letter::new(9.5, 700.0, INK).track(0.04));
        pill(b, root, "Room Send", &a("room"), (822.0, 780.0));
        ui_knob(b, root, "Room Level", &a("room_level"), (760.0, 822.0), 34.0, Some(ValueFormat { decimals: Some(1), ..ValueFormat::default() }));
        outline_button(b, root, "Phase", &a("phase"), (852.0, 848.0, 64.0));
    });
    let params = vec![
        rk::choice(&a("mic"), "Mic", MICS, 1).featured(),
        rk::toggle(&a("mic_on"), "Mic On", true),
        crate::delay::parts::param(&a("position"), "Position", surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None), 0.5).featured(),
        crate::delay::parts::param(&a("distance"), "Distance", surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None), 0.0).featured(),
        rk::db(&a("level"), "Mic Level", -24.0, 12.0, 0.0),
        crate::delay::parts::param(&a("pan"), "Pan", surface_proto::Range::linear(-100.0, 100.0, surface_proto::Unit::Percent), 0.0).bipolar(),
        rk::toggle(&a("room"), "Room Send", false),
        rk::db(&a("room_level"), "Room Level", -24.0, 12.0, 0.0),
        rk::toggle(&a("phase"), "Phase Invert", false),
    ];
    Face { file: "09-cab-4x12", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
