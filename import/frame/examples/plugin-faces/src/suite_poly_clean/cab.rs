//! `07-cab-berry-four-twelve` — the cab (Cab form, 900 × 900): a berry-pink
//! 4×12 with a lilac-grey grille, its four speakers just showing through
//! the cloth, and a mic on each of the top two — drag a mic across its
//! speaker to move it from the cone's centre to its edge. Under the cab,
//! the mic rail: each mic's model, distance, level and pan, and the room
//! send between them.

use std::fmt::Write as _;

use document_proto::{CornerRadii, NodeId};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Axis, Binding, Control, ControlKind, HandleAxis, Range, Unit};
use surface_widgets::Builder;

use super::parts::{self, knob};
use super::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 900.0;
const H: f64 = 900.0;
const CAB_H: f64 = 760.0;
const MICS: [&str; 6] = ["Dynamic A", "Dynamic B", "Condenser A", "Condenser B", "Ribbon A", "Ribbon B"];

/// A speaker behind the cloth: the basket ring, the cone's rolls, the cap.
fn speaker(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    b.add(parent, draw::circle("cone", cx, cy, r, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#0000000d"), (0.7, "#00000012"), (0.92, "#0000001f"), (1.0, "#ffffff0d")])));
    for f in [1.0, 0.93, 0.62, 0.3] {
        b.add(parent, draw::stroked(draw::circle("roll", cx, cy, r * f, draw::solid("#00000000")), "#0000001c", 1.6));
        b.add(parent, draw::stroked(draw::circle("roll light", cx + 1.0, cy + 1.5, r * f, draw::solid("#00000000")), "#ffffff0f", 1.0));
    }
    b.add(parent, draw::circle("dust cap", cx, cy, r * 0.22, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff1a"), (1.0, "#0000002e")])));
}

/// A dynamic mic on a `Handle` in `lane` (the speaker's width): x is the
/// mic's position across the cone.
fn mic(b: &mut Builder<'_>, lane: NodeId, addr: &str, lean: f64) {
    let touch = 56.0;
    let handle = b.add(lane, draw::frame("Mic", 0.0, 0.0, touch, touch));
    let (cx, cy) = (touch / 2.0, touch / 2.0);
    let a = lean.to_radians();
    let (ux, uy) = (a.sin(), a.cos());
    let (nx, ny) = (-uy, ux);
    // The body runs from the head (at the handle's centre) back and down.
    let len = 84.0;
    let (w0, w1) = (13.0, 10.0);
    let tail = (cx + ux * len, cy + uy * len);
    let body = [(cx + nx * w0 / 2.0, cy + ny * w0 / 2.0), (tail.0 + nx * w1 / 2.0, tail.1 + ny * w1 / 2.0), (tail.0 - nx * w1 / 2.0, tail.1 - ny * w1 / 2.0), (cx - nx * w0 / 2.0, cy - ny * w0 / 2.0)];
    let shadow: Vec<(f64, f64)> = body.iter().map(|(x, y)| (x + 8.0, y + 12.0)).collect();
    b.add(handle, draw::with_effects(draw::path("mic shadow", 0.0, 0.0, touch, touch, draw::polygon(&shadow), draw::solid("#00000059")), vec![draw::drop_shadow("#00000040", 0.0, 0.0, 6.0)]));
    b.add(handle, draw::path("mic body", 0.0, 0.0, touch, touch, draw::polygon(&body), draw::linear(90.0 + lean, &[(0.0, "#050505"), (0.45, "#3b3b3e"), (0.6, "#1a1a1b"), (1.0, "#050505")])));
    // The grille head: rings of mesh round the capsule.
    b.add(handle, draw::circle("head", cx, cy, 11.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#4a4a4d"), (1.0, "#0b0b0c")])));
    let mut mesh = String::new();
    for i in 0..6 {
        let (x, y) = (cx + ux * f64::from(i) * 3.0, cy + uy * f64::from(i) * 3.0);
        let _ = write!(mesh, "M {:.2} {:.2} L {:.2} {:.2} ", x + nx * 8.0, y + ny * 8.0, x - nx * 8.0, y - ny * 8.0);
    }
    b.add(handle, draw::stroked(draw::path("mesh", 0.0, 0.0, touch, touch, mesh, draw::solid("#00000000")), "#6a6a6e", 0.8));
    let kind = ControlKind::Handle {
        x: Some(Binding::param(addr)),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls.insert(handle, Control::new(kind));
}

/// One mic's strip on the rail: model, distance, level, pan.
fn mic_strip(b: &mut Builder<'_>, rail: NodeId, ns: &str, label: &str, x0: f64) {
    let ink = "#e9d9e1";
    parts::word(b, rail, label, x0 + 150.0, 14.0, 9.0, "#b99cab", 700.0, 0.2);
    let p = Place::new(format!("{label} model")).size(34.0).variant("black").bind(ui::bind(&format!("{ns}/model"))).options(&["Dyn A", "Dyn B", "Cond A", "Cond B", "Rib A", "Rib B"]).ink(surface_kit::Ink::Color(ink.into())).keep_case().touch(TOUCH_MIN);
    let k = ui::put(b, rail, "switch.rotary", &p, x0 + 38.0, 64.0);
    ui::strip_legend(b, k);
    for (i, (id, name)) in [("distance", "distance"), ("level", "level"), ("pan", "pan")].iter().enumerate() {
        let x = x0 + 128.0 + 76.0 * i as f64;
        knob(b, rail, "knob.amp_cap", "pink", &format!("{ns}/{id}"), name, 34.0, x, 58.0);
        parts::word(b, rail, name, x, 96.0, 8.5, ink, 500.0, 0.08);
    }
}

pub fn face() -> Face {
    let scene = canvas("Berry 4x12 Cab (Cab, 900×900)", |b, root| {
        ui::fit_root(b, root, W, H);
        // Feet under the cab.
        for fx in [0.12, 0.88] {
            b.add(root, draw::rounded(draw::rect("foot", W * fx - 30.0, CAB_H - 10.0, 60.0, 24.0, draw::linear(180.0, &[(0.0, "#1b1b1c"), (1.0, "#050505")])), 4.0));
        }
        let mut cab = draw::frame("Cabinet", 0.0, 0.0, W, CAB_H);
        cab.fills = vec![draw::linear(180.0, &[(0.0, "#d9578a"), (0.5, "#c9477a"), (1.0, "#b23b6a")])];
        cab.corner_radii = CornerRadii::uniform(8.0);
        cab.effects = vec![draw::inner_shadow("#ffffff4d", 0.0, 2.0, 2.0), draw::inner_shadow("#00000040", 0.0, -3.0, 4.0), draw::drop_shadow("#000000a0", 0.0, 10.0, 20.0)];
        let cab = b.add(root, cab);
        let m = 26.0;
        let mut grille = draw::frame("Grille", m, m, W - 2.0 * m, CAB_H - 2.0 * m);
        grille.fills = vec![draw::linear(180.0, &[(0.0, "#8c7a8f"), (0.5, "#83708a"), (1.0, "#766379")])];
        grille.clips_content = true;
        grille.effects = vec![draw::inner_shadow("#000000a0", 0.0, 3.0, 6.0)];
        let (gw, gh) = (W - 2.0 * m, CAB_H - 2.0 * m);
        let grille = b.add(cab, grille);
        // The cloth's weave.
        let mut weave = String::new();
        let mut x = 0.0;
        while x < gw {
            let _ = write!(weave, "M {x:.1} 0 L {x:.1} {gh:.1} ");
            x += 3.0;
        }
        let mut y = 0.0;
        while y < gh {
            let _ = write!(weave, "M 0 {y:.1} L {gw:.1} {y:.1} ");
            y += 3.0;
        }
        b.add(grille, draw::stroked(draw::path("weave", 0.0, 0.0, gw, gh, weave, draw::solid("#00000000")), "#ffffff0d", 0.8));
        let r = 150.0;
        let centres = [(gw * 0.27, gh * 0.27), (gw * 0.73, gh * 0.27), (gw * 0.27, gh * 0.73), (gw * 0.73, gh * 0.73)];
        for (cx, cy) in centres {
            speaker(b, grille, cx, cy, r);
        }
        b.add(grille, draw::rect("cloth sheen", 0.0, 0.0, gw, gh, draw::linear(120.0, &[(0.0, "#ffffff10"), (0.5, "#ffffff00"), (1.0, "#0000001a")])));
        // Mics on the top pair: lanes across each speaker.
        for (i, lean) in [(0, 225.0), (1, 135.0)] {
            let (cx, cy) = centres[i];
            let lane = b.add(grille, draw::frame(&format!("Mic {} lane", i + 1), cx - r, cy - 28.0, 2.0 * r, 56.0));
            mic(b, lane, &format!("mic{}/position", i + 1), lean);
        }

        // The rail.
        let mut rail = draw::frame("Mic rail", 0.0, CAB_H + 20.0, W, H - CAB_H - 20.0);
        rail.fills = vec![draw::linear(180.0, &[(0.0, "#241b20"), (1.0, "#161013")])];
        rail.corner_radii = CornerRadii::uniform(10.0);
        rail.effects = vec![draw::inner_shadow("#ffffff1a", 0.0, 1.0, 1.0)];
        let rail = b.add(root, rail);
        mic_strip(b, rail, "mic1", "MIC 1", 20.0);
        mic_strip(b, rail, "mic2", "MIC 2", W - 20.0 - 330.0);
        knob(b, rail, "knob.amp_cap", "pink", "room/send", "Room send", 38.0, W / 2.0, 58.0);
        parts::word(b, rail, "room send", W / 2.0, 96.0, 8.5, "#e9d9e1", 500.0, 0.08);
    });
    let mut params = Vec::new();
    for (i, pos) in [(1, 0.45), (2, 0.6)] {
        let ns = format!("mic{i}");
        params.push(ui::param(&format!("{ns}/position"), &format!("Mic {i} Position"), Range::linear(0.0, 1.0, Unit::None), pos).featured());
        params.push(parts::pick(&ns, "model", &format!("Mic {i} Model"), &MICS, if i == 1 { 0 } else { 5 }));
        params.push(ui::param(&format!("{ns}/distance"), &format!("Mic {i} Distance"), Range::linear(0.0, 1.0, Unit::None), 0.0));
        params.push(parts::db(&ns, "level", &format!("Mic {i} Level"), -24.0, 12.0, 0.0).bipolar());
        params.push(ui::param(&format!("{ns}/pan"), &format!("Mic {i} Pan"), Range::linear(-100.0, 100.0, Unit::None), 0.0).bipolar());
    }
    params.push(parts::db("room", "send", "Room Send", -60.0, 0.0, -60.0));
    Face { file: "07-cab-berry-four-twelve", plugin: PLUGIN, scene, params, streams: vec![] }
}
