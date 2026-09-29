//! `08-cab-square-900x900`: the cab page (900 × 900) — the cream 4×12 with
//! brass speaker rings on the plugin's periwinkle ground, a mic panel
//! mirrored either side of it (mic, position, distance, level, pan, the
//! mic's power and phase) and the room send under it, with the cab
//! selector: three cabinets — the cream 4×12, the brown 2×12 and the
//! blush 2×10.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Range, Unit};
use surface_widgets::Builder;

use super::look::{self, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, CAB, Type};

const INK: &str = "#262a3a";
const DIM: &str = "#5a6078";
const MICS: &[&str] = &["Dynamic 1", "Dynamic 2", "Ribbon 1", "Ribbon 2", "Condenser 1", "Condenser 2"];

/// One speaker: basket, brass ring and screws, cone, dust cap.
fn speaker(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    b.add(parent, draw::circle("well", cx, cy + 3.0, r * 1.06, draw::solid("#00000030")));
    let ring = draw::circle("ring", cx, cy, r * 1.04, draw::conic(30.0, &[(0.0, "#fff1c4"), (0.25, "#a77a3a"), (0.5, "#f0d48e"), (0.75, "#9a7236"), (1.0, "#fff1c4")]));
    b.add(parent, draw::with_effects(ring, vec![draw::drop_shadow("#00000059", 0.0, 2.0, 4.0)]));
    for i in 0..8 {
        let (x, y) = draw::polar(cx, cy, r * 0.98, 22.5 + 45.0 * f64::from(i));
        b.add(parent, draw::circle("screw", x, y, r * 0.025, draw::solid("#5e421a")));
    }
    b.add(parent, draw::circle("gasket", cx, cy, r * 0.92, draw::solid("#e9e2d4")));
    b.add(parent, draw::circle("surround", cx, cy, r * 0.84, draw::radial(&[(0.0, "#2c2a28"), (0.85, "#3a3734"), (1.0, "#171615")])));
    b.add(parent, draw::circle("cone", cx, cy, r * 0.7, draw::radial_at(0.45, 0.4, 0.75, &[(0.0, "#f4eee2"), (0.7, "#d8cfbe"), (1.0, "#b3a893")])));
    b.add(parent, draw::with_effects(draw::circle("dust cap", cx, cy, r * 0.28, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fbf7ee"), (1.0, "#c9bfae")])), vec![draw::drop_shadow("#00000040", 0.0, 2.0, 4.0)]));
}

/// The cream 4×12.
fn cabinet(b: &mut Builder<'_>, root: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let shell = look::slab("Cabinet", (x, y, w, h), 14.0, vec![draw::linear(180.0, &[(0.0, "#f8f4ea"), (0.5, "#efe8da"), (1.0, "#ddd3c1")])]);
    let shell = b.add(root, draw::with_effects(shell, vec![draw::drop_shadow("#0000004d", 0.0, 12.0, 24.0), draw::inner_shadow("#ffffffaa", 0.0, 2.0, 1.0)]));
    let (bx, by, bw, bh) = (24.0, 24.0, w - 48.0, h - 80.0);
    let baffle = look::slab("Baffle", (bx, by, bw, bh), 8.0, vec![draw::radial_at(0.5, 0.45, 0.8, &[(0.0, "#f4efe6"), (1.0, "#e0d8c8")])]);
    b.add(shell, draw::with_effects(draw::stroked(baffle, "#2a2826", 5.0), vec![draw::inner_shadow("#00000040", 0.0, 3.0, 6.0)]));
    let r = bw.min(bh) * 0.215;
    for (u, v) in [(0.27, 0.26), (0.73, 0.26), (0.27, 0.74), (0.73, 0.74)] {
        speaker(b, shell, bx + bw * u, by + bh * v, r);
    }
    // The brass strip along the foot, and the corner caps.
    let strip = draw::rounded(draw::rect("strip", 30.0, h - 38.0, w - 60.0, 9.0, draw::linear(180.0, &[(0.0, "#fff1c4"), (0.5, "#c9a45c"), (1.0, "#8a6630")])), 4.0);
    b.add(shell, draw::with_effects(strip, vec![draw::drop_shadow("#00000066", 0.0, 1.5, 2.0)]));
    for i in 0..9 {
        b.add(shell, draw::circle("rivet", 44.0 + (w - 88.0) * f64::from(i) / 8.0, h - 33.5, 1.8, draw::solid("#6b4e22")));
    }
    for (cx, cy) in [(0.0, 0.0), (w - 22.0, 0.0), (0.0, h - 22.0), (w - 22.0, h - 22.0)] {
        b.add(shell, draw::rounded(draw::rect("corner", cx, cy, 22.0, 22.0, draw::linear(150.0, &[(0.0, "#fff1c4"), (0.5, "#c9a45c"), (1.0, "#7a5a26")])), 5.0));
    }
    for fx in [x + 60.0, x + w - 100.0] {
        b.add(root, draw::rounded(draw::rect("foot", fx, y + h, 40.0, 16.0, draw::solid("#1a1a1c")), 4.0));
    }
}

/// A flat chrome knob with its caption and live value.
fn flat(b: &mut Builder<'_>, parent: NodeId, addr: &str, legend: &str, at: (f64, f64)) {
    let k = parts::knob(b, parent, "knob.flat_line", "light", addr, legend, 48.0, at, &[], INK);
    ui::add_value(b, k, 11.0, INK);
    parts::centred(b, parent, legend, (at.0, at.1 + 58.0), Type::new(9.5, DIM, 700.0).track(0.08).upper());
}

/// One mic's panel, its column centred on `cx`.
fn mic_panel(b: &mut Builder<'_>, root: NodeId, id: &str, title: &str, cx: f64) {
    let ns = |p: &str| format!("cab/{id}_{p}");
    let panel = look::slab("Mic panel", (cx - 105.0, 60.0, 210.0, 470.0), 16.0, vec![draw::solid("#dfe5fa")]);
    let panel = b.add(root, draw::with_effects(panel, vec![draw::drop_shadow("#3a44701f", 0.0, 4.0, 12.0)]));
    parts::words(b, panel, title, 18.0, 30.0, 100.0, TextAlign::Left, Type::new(12.0, INK, 800.0).track(0.12).upper());
    // The mic's power: a push cap with its lamp.
    let p = Place::new(format!("{title} on")).size(22.0).variant("grey").bind(ui::bind(&ns("on"))).legend("").options(&[&format!("color={TEAL}")]).touch(TOUCH_MIN);
    parts::part(b, panel, "button.round_cap", p, &["cap"], (175.0, 32.0));
    // The mic in use, on a dark readout, chosen by the knob under it.
    let win = ui::readout(b, panel, &format!("{title} mic"), ui::bind(&ns("mic")), 105.0, 78.0, 182.0, 30.0, "#e8ecf8");
    let _ = win;
    flat(b, panel, &ns("mic"), "Mic", (55.0, 150.0));
    flat(b, panel, &ns("pos"), "Position", (155.0, 150.0));
    flat(b, panel, &ns("dist"), "Distance", (55.0, 260.0));
    flat(b, panel, &ns("level"), "Level", (155.0, 260.0));
    flat(b, panel, &ns("pan"), "Pan", (55.0, 370.0));
    let p = Place::new(format!("{title} phase")).size(28.0).variant("grey").bind(ui::bind(&ns("phase"))).legend("").options(&["nolamp"]).touch(TOUCH_MIN);
    parts::part(b, panel, "button.round_cap", p, &["cap"], (155.0, 370.0));
    parts::centred(b, panel, "Phase Ø", (155.0, 428.0), Type::new(9.5, DIM, 700.0).track(0.08).upper());
}

pub(super) fn face() -> Face {
    let scene = canvas("Cab", |b, root| {
        parts::page(b, root, CAB, CAB);
        b.add(root, draw::rect("ground", 0.0, 0.0, CAB, CAB, draw::linear(180.0, &[(0.0, "#cdd6f5"), (0.7, "#c2ccef"), (1.0, "#aab4dc")])));
        b.add(root, draw::rect("floor", 0.0, 600.0, CAB, 300.0, draw::linear(180.0, &[(0.0, "#b8b0a6"), (0.05, "#d8cfc2"), (1.0, "#a89c8c")])));
        cabinet(b, root, (250.0, 70.0, 400.0, 530.0));
        mic_panel(b, root, "a", "Mic A", 120.0);
        mic_panel(b, root, "b", "Mic B", 780.0);
        // The cab selector.
        let sel = Place::new("Cab").size(56.0).variant("white").bind(ui::bind("cab/model")).options(&["Cream 4×12", "Brown 2×12", "Blush 2×10"]).keep_case();
        let s = surface_kit::place(b, root, "button.square_bank", &sel).unwrap_or_else(|e| panic!("{e}"));
        ui::strip_legend(b, s);
        parts::centre_on(b, s, (CAB / 2.0, 670.0));
        // The room send.
        let room = look::slab("Room", (300.0, 720.0, 300.0, 150.0), 16.0, vec![draw::solid("#dfe5fa")]);
        let room = b.add(root, draw::with_effects(room, vec![draw::drop_shadow("#3a44701f", 0.0, 4.0, 12.0)]));
        parts::words(b, room, "Room Send", 20.0, 28.0, 160.0, TextAlign::Left, Type::new(12.0, INK, 800.0).track(0.12).upper());
        let p = Place::new("Room send").size(22.0).variant("grey").bind(ui::bind("cab/room_on")).legend("").options(&[&format!("color={TEAL}")]).touch(TOUCH_MIN);
        parts::part(b, room, "button.round_cap", p, &["cap"], (260.0, 30.0));
        flat(b, room, "cab/room_level", "Level", (150.0, 70.0));
    });
    let mut params = vec![parts::pick("cab/model", "Cab", &["Cream 4x12", "Brown 2x12", "Blush 2x10"], 0)];
    for (id, name, pos) in [("a", "Mic A", 45.0), ("b", "Mic B", 60.0)] {
        let a = |p: &str| format!("cab/{id}_{p}");
        params.extend([
            parts::switch(&a("on"), &format!("{name} On"), id == "a"),
            parts::pick(&a("mic"), &format!("{name} Mic"), MICS, if id == "a" { 0 } else { 2 }),
            parts::pct(&a("pos"), &format!("{name} Position"), pos),
            parts::pct(&a("dist"), &format!("{name} Distance"), 10.0),
            parts::db(&a("level"), &format!("{name} Level"), -24.0, 6.0, 0.0),
            ui::param(&a("pan"), &format!("{name} Pan"), Range::linear(-100.0, 100.0, Unit::Percent), 0.0).featured().bipolar(),
            parts::switch(&a("phase"), &format!("{name} Phase"), false),
        ]);
    }
    params.push(parts::switch("cab/room_on", "Room Send", false));
    params.push(parts::db("cab/room_level", "Room Level", -24.0, 6.0, 0.0));
    look::face("08-cab-square-900x900", scene, params)
}
