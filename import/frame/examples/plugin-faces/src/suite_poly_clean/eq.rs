//! `08-eq-nine-band-tabletop` — a pink nine-band graphic EQ in a
//! tabletop case, 1100 × 316 design px (a 3.9:1 case on two feet): a big
//! knurled high-pass knob and its power lever at the left, nine faders
//! with glowing caps (65 Hz … 16 kHz, ±12 dB, each with its readout
//! above), a big low-pass knob and the white pilot at the right.

use document_proto::{CornerRadii, NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_proto::{ButtonMode, Control, ControlKind, ParamInfo, Range, SlotRole, Unit};
use surface_widgets::Builder;

use super::parts::{self, knob_with};
use super::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 1100.0;
const H: f64 = 316.0;
const BODY_H: f64 = 284.0;
const NS: &str = "eq";
const BANDS: [(&str, &str); 9] = [("65", "65Hz"), ("125", "125Hz"), ("250", "250Hz"), ("500", "500Hz"), ("1k", "1kHz"), ("2k", "2kHz"), ("4k", "4kHz"), ("8k", "8kHz"), ("16k", "16kHz")];
const XS: [f64; 9] = [256.0, 330.0, 403.0, 477.0, 550.0, 624.0, 697.0, 771.0, 845.0];
const INK: &str = "#4a2a35";

/// A plain live value (a `Readout`'s `Value` text) centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
fn value(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, cx: f64, cy: f64, w: f64, size: f64) -> NodeId {
    let root = b.add(parent, draw::frame(name, cx - w / 2.0, cy - size, w, size * 2.0));
    let t = b.add(root, ui::text("value", "", 0.0, size, w, size, INK, TextAlign::Center, 500.0, false));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, t.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
    root
}

/// The power lever: a black capsule, its chrome ball at the right when on.
fn lever(b: &mut Builder<'_>, parent: NodeId, addr: &str, cx: f64, cy: f64) {
    let (w, h) = (48.0, 44.0);
    let root = b.add(parent, draw::frame("Power", cx - w / 2.0, cy - h / 2.0, w, h));
    let cap = draw::rounded(draw::rect("capsule", 6.0, h / 2.0 - 8.0, w - 12.0, 16.0, draw::linear(180.0, &[(0.0, "#2a2a2c"), (0.5, "#0b0b0c"), (1.0, "#1d1d1f")])), 8.0);
    b.add(root, draw::with_effects(cap, vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0), draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.5)]));
    b.add(root, draw::stroked(draw::circle("ring", 16.0, h / 2.0, 4.5, draw::solid("#00000000")), "#6a6a6c", 1.2));
    let mut poses = Vec::new();
    for (name, x) in [("off", 14.0), ("on", w - 12.0)] {
        let pose = b.add(root, draw::frame(name, 0.0, 0.0, w, h));
        b.add(pose, draw::circle("ball", x, h / 2.0, 5.5, draw::radial_at(0.35, 0.3, 0.85, &[(0.0, "#ffffff"), (0.6, "#b4b5b7"), (1.0, "#4a4b4d")])));
        poses.push(pose);
    }
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Off, poses[0].0).slot(SlotRole::On, poses[1].0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// The white pilot that glows while the EQ is on.
fn pilot(b: &mut Builder<'_>, parent: NodeId, addr: &str, cx: f64, cy: f64) {
    let root = b.add(parent, draw::frame("Pilot", cx - 20.0, cy - 8.0, 40.0, 16.0));
    b.add(root, draw::rounded(draw::rect("well", 2.0, 3.0, 36.0, 10.0, draw::solid("#00000040")), 5.0));
    let lens = draw::rounded(draw::rect("lens", 3.0, 3.5, 34.0, 9.0, draw::linear(180.0, &[(0.0, "#ffffff"), (0.5, "#dfe3ea"), (1.0, "#ffffff")])), 4.5);
    let lens = draw::with_effects(lens, vec![draw::drop_shadow("#ffffffb0", 0.0, 0.0, 5.0)]);
    let lens = b.add(root, draw::emissive(lens, draw::hex("#f2f4ff"), 0.5, 8.0));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Glow, lens.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

pub fn face() -> Face {
    let scene = canvas("Nine Band EQ (tabletop, 1100×316)", |b, root| {
        ui::fit_root(b, root, W, H);
        // Feet, then the case: a pink box, its top face catching the light.
        for fx in [0.1, 0.9] {
            let foot = draw::rounded(draw::rect("foot", W * fx - 36.0, BODY_H - 10.0, 72.0, H - BODY_H + 10.0, draw::linear(90.0, &[(0.0, "#050505"), (0.4, "#2b2b2d"), (1.0, "#070707")])), 6.0);
            b.add(root, foot);
        }
        let mut body = draw::frame("Case", 0.0, 0.0, W, BODY_H);
        body.fills = vec![
            draw::linear(180.0, &[(0.0, "#dea3b6"), (0.08, "#d596ab"), (0.6, "#c98ba0"), (1.0, "#b87a8f")]),
            draw::radial_at(0.5, 0.35, 0.7, &[(0.0, "#ffffff14"), (1.0, "#ffffff00")]),
        ];
        body.corner_radii = CornerRadii { top_left: 10.0, top_right: 10.0, bottom_left: 4.0, bottom_right: 4.0 };
        body.effects = vec![draw::inner_shadow("#ffffff66", 0.0, 2.0, 1.5), draw::inner_shadow("#6a334855", 0.0, -3.0, 4.0), draw::drop_shadow("#000000a0", 0.0, 10.0, 16.0)];
        let body = b.add(root, body);
        b.add(body, draw::rect("bottom lip", 0.0, BODY_H - 6.0, W, 6.0, draw::linear(180.0, &[(0.0, "#a86a80"), (1.0, "#8d5569")])));

        // High-pass: its readout, the knee, the big knob, the power lever.
        value(b, body, "HPF readout", "eq/hpf", 146.0, 32.0, 90.0, 10.0);
        parts::cut_glyph(b, body, 146.0, 60.0, 26.0, true, INK);
        knob_with(b, body, "knob.amp_cap", "eq_pink", "eq/hpf", "High Pass", 98.0, 146.0, 139.0, &[], INK);
        lever(b, body, "eq/on", 146.0, 230.0);
        // Low-pass and the pilot.
        value(b, body, "LPF readout", "eq/lpf", 951.0, 32.0, 90.0, 10.0);
        parts::cut_glyph(b, body, 951.0, 60.0, 26.0, false, INK);
        knob_with(b, body, "knob.amp_cap", "eq_pink", "eq/lpf", "Low Pass", 98.0, 951.0, 139.0, &[], INK);
        pilot(b, body, "eq/on", 951.0, 230.0);

        for ((id, label), x) in BANDS.iter().zip(XS) {
            let addr = format!("eq/g{id}");
            value(b, body, &format!("{label} readout"), &addr, x, 32.0, 64.0, 10.0);
            let p = Place::new(format!("{label} gain")).size(145.0).bind(ui::bind(&addr));
            let f = surface_kit::place(b, body, "lever.glow_cap", &p).unwrap_or_else(|e| panic!("{e}"));
            let fw = b.scene.node(f).map_or(0.0, |n| n.size.width);
            ui::move_to(b, f, x - fw / 2.0, 44.0);
            parts::word(b, body, label, x, 247.0, 10.0, INK, 500.0, 0.08);
        }
    });
    let mut params: Vec<ParamInfo> = BANDS.iter().map(|(id, label)| ui::param(&format!("{NS}/g{id}"), &format!("{label} Gain"), Range::linear(-12.0, 12.0, Unit::Db), 0.0).bipolar()).collect();
    params[0] = params[0].clone().featured();
    params.push(parts::hz(NS, "hpf", "High Pass", 20.0, 1000.0, 20.0));
    params.push(parts::hz(NS, "lpf", "Low Pass", 1000.0, 20000.0, 20000.0));
    params.push(parts::on(NS, "on", "Power", true).featured());
    Face { file: "08-eq-nine-band-tabletop", plugin: PLUGIN, scene, params, streams: vec![] }
}
