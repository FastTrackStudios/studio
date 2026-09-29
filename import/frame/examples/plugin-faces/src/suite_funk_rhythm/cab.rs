//! `09-cab-900x900`: the cab page (900 × 900) — a wood-framed 4×12 with a
//! white baffle and four black speakers, the three cabs as glyph buttons
//! under it (a select bank on `cab/model`), and the rig's shared mic
//! controls either side: for each of two mics a dropdown of six mics and
//! an on switch, position, distance, level and pan knobs with their
//! readouts, and a phase button; between them the room send and its level.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{ButtonMode, Control, ControlKind, Range, SlotRole, Unit, ValueFormat};
use surface_widgets::Builder;

use super::parts::{self, CAB, Type};
use super::pedals::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

pub(crate) const MICS: &[&str] = &["Dynamic 1", "Dynamic 2", "Condenser", "Ribbon 1", "Ribbon 2", "Small Condenser"];
const CABS: &[&str] = &["White 4x12", "Blue 4x12", "Green 4x12"];
const INK: &str = "#2b2624";
const DIM: &str = "#7a6e69";

/// A wood-framed 4×12 at (`x`, `y`), `s` square, baffle `baffle`.
pub(crate) fn cabinet(b: &mut Builder<'_>, parent: NodeId, (x, y, s): (f64, f64, f64), baffle: &str, wood: [&str; 3]) {
    let frame = draw::rect("cab", x, y, s, s, draw::linear(90.0, &[(0.0, wood[0]), (0.5, wood[1]), (1.0, wood[2])]));
    b.add(parent, draw::with_effects(draw::rounded(frame, 10.0), vec![draw::drop_shadow("#00000080", 0.0, 8.0, 16.0), draw::inner_shadow("#ffffff40", 0.0, 2.0, 2.0)]));
    let m = s * 0.07;
    let panel = draw::rect("baffle", x + m, y + m, s - 2.0 * m, s - 2.0 * m, draw::radial_at(0.45, 0.4, 0.8, &[(0.0, &ui::blend(baffle, "#ffffff", 0.25)), (1.0, &ui::blend(baffle, "#000000", 0.12))]));
    b.add(parent, draw::with_effects(panel, vec![draw::inner_shadow("#00000073", 0.0, 3.0, 8.0), draw::drop_shadow("#ffffff33", 0.0, 1.0, 0.0)]));
    let q = (s - 2.0 * m) / 4.0;
    let r = q * 0.9;
    for (i, j) in [(1.0, 1.0), (3.0, 1.0), (1.0, 3.0), (3.0, 3.0)] {
        let (cx, cy) = (x + m + q * i, y + m + q * j);
        b.add(parent, draw::circle("recess", cx, cy, r * 1.06, draw::radial(&[(0.85, "#00000000"), (0.95, "#00000040"), (1.0, "#00000000")])));
        b.add(parent, draw::with_effects(draw::circle("basket", cx, cy, r, draw::solid("#141415")), vec![draw::drop_shadow("#00000066", 0.0, 2.0, 5.0)]));
        let cone = draw::circle("cone", cx, cy, r * 0.84, draw::conic(0.0, &[(0.0, "#2c2c2e"), (0.25, "#141415"), (0.5, "#303033"), (0.75, "#121213"), (1.0, "#2c2c2e")]));
        b.add(parent, cone);
        let rings: Vec<Vec<(f64, f64)>> = (1..9).map(|k| parts::ring(cx, cy, r * (0.3 + 0.065 * f64::from(k)))).collect();
        b.add(parent, ui::strokes("ridges", &rings, 1.0, draw::solid("#4a4a4e66")));
        b.add(parent, draw::circle("dust cap", cx, cy, r * 0.22, draw::radial_at(0.38, 0.32, 0.8, &[(0.0, "#9a9a9e"), (0.4, "#303033"), (1.0, "#0a0a0b")])));
        for a in 0..8 {
            let (sx, sy) = draw::polar(cx, cy, r * 0.93, 22.5 + 45.0 * f64::from(a));
            b.add(parent, draw::circle("bolt", sx, sy, 2.2, draw::solid("#3a3a3c")));
        }
    }
    // Feet.
    for fx in [x + s * 0.08, x + s * 0.8] {
        b.add(parent, draw::path("foot", 0.0, 0.0, CAB, CAB, draw::polygon(&[(fx, y + s), (fx + s * 0.12, y + s), (fx + s * 0.1, y + s + 14.0), (fx + s * 0.02, y + s + 14.0)]), draw::solid(wood[2])));
    }
}

/// A select-bank button: a glyph of an amp over its cab, lit when chosen.
fn cab_glyph(b: &mut Builder<'_>, parent: NodeId, i: usize, (x, y): (f64, f64)) {
    let (w, h) = (54.0, 50.0);
    let root = b.add(parent, draw::frame(&format!("Cab {}", i + 1), x, y, w, h));
    let on = b.add(root, draw::rounded(draw::rect("chosen", 0.0, 0.0, w, h, draw::solid("#fbf7f4")), 3.0));
    let lines = vec![
        parts::round_rect(6.0, 5.0, w - 12.0, 13.0, 1.0),
        parts::round_rect(4.0, 22.0, w - 8.0, 24.0, 1.0),
        parts::ring(17.0, 34.0, 7.0),
        parts::ring(w - 17.0, 34.0, 7.0),
    ];
    b.add(root, ui::strokes("glyph", &lines, 2.0, draw::solid(INK)));
    let c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: 3 } }).slot(SlotRole::On, on.0);
    let mut c = c;
    c.binding = Some(ui::bind("cab/model"));
    b.controls.insert(root, c);
}

/// A dropdown of `options` on `addr`: a dark pill whose text is the
/// choice, stepped by wheel or arrows.
fn dropdown(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, (x, y, w, h): (f64, f64, f64, f64), options: &[&str]) {
    let root = b.add(parent, draw::rounded(draw::frame(name, x, y, w, h), 6.0));
    if let Some(n) = b.scene.node_mut(root) {
        n.fills = vec![draw::solid("#141414")];
    }
    let value = b.add(root, ui::text("value", "", 14.0, h / 2.0, w - 44.0, 15.0, "#f4f4f2", TextAlign::Left, 500.0, false));
    b.add(root, ui::strokes("chevron", &[vec![(w - 26.0, h / 2.0 + 3.0), (w - 20.0, h / 2.0 - 3.0), (w - 14.0, h / 2.0 + 3.0)]], 1.8, draw::solid("#f4f4f2")));
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, value.0);
    c.binding = Some(ui::bind(addr));
    c.format = Some(ValueFormat::labels(options));
    b.controls.insert(root, c);
}

/// A latching square toggle with a legend: dark when on.
fn toggle_button(b: &mut Builder<'_>, parent: NodeId, addr: &str, legend: &str, (x, y, w, h): (f64, f64, f64, f64)) {
    let root = b.add(parent, draw::frame(legend, x, y, w, h));
    let off = b.add(root, draw::stroked(draw::rounded(draw::rect("off", 0.0, 0.0, w, h, draw::solid("#fbf8f6")), 6.0), "#b9aeaa", 1.2));
    let on = b.add(root, draw::rounded(draw::rect("on", 0.0, 0.0, w, h, draw::solid("#2b2624")), 6.0));
    let _ = (off, on);
    let t_off = b.add(root, ui::text("legend", legend, 0.0, h / 2.0, w, 13.0, INK, TextAlign::Center, 600.0, true));
    let t_on = b.add(root, ui::text("legend", legend, 0.0, h / 2.0, w, 13.0, "#fbf8f6", TextAlign::Center, 600.0, true));
    let c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::On, on.0).slot(SlotRole::On, t_on.0).slot(SlotRole::Off, t_off.0).slot(SlotRole::Off, off.0);
    let mut c = c;
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// A flat knob with its value under it and its caption under that.
fn flat(b: &mut Builder<'_>, parent: NodeId, addr: &str, caption: &str, (cx, cy): (f64, f64)) {
    let p = Place::new(caption).size(46.0).variant("light").bind(ui::bind(addr)).touch(TOUCH_MIN).options(&["track=#00000024"]);
    let k = ui::put(b, parent, "knob.flat_line", &p, cx, cy);
    ui::strip_legend(b, k);
    ui::add_value(b, k, 13.0, INK);
    parts::centred(b, parent, caption, (cx, cy + 58.0), Type::new(10.5, DIM, 600.0).upper().track(0.04));
}

fn mic_panel(b: &mut Builder<'_>, parent: NodeId, m: &str, x0: f64) {
    let pre = format!("cab/{m}_");
    let a = |s: &str| format!("{pre}{s}");
    dropdown(b, parent, &a("mic"), &format!("Mic {}", m.to_uppercase()), (x0, 560.0, 250.0, 44.0), MICS);
    toggle_button(b, parent, &a("on"), "On", (x0 + 262.0, 560.0, 60.0, 44.0));
    for (i, (id, cap)) in [("pos", "Position"), ("dist", "Distance"), ("level", "Mic Level"), ("pan", "Pan")].iter().enumerate() {
        flat(b, parent, &a(id), cap, (x0 + 40.0 + 82.0 * i as f64, 676.0));
    }
    toggle_button(b, parent, &a("phase"), "Phase Ø", (x0 + 100.0, 790.0, 120.0, 44.0));
}

pub(super) fn face() -> Face {
    let scene = canvas("Funk Cab", |b, root| {
        parts::page(b, root, CAB, CAB);
        let mut bg = draw::frame("room", 0.0, 0.0, CAB, CAB);
        bg.fills = vec![draw::linear(180.0, &[(0.0, "#f6ddd4"), (0.56, "#f3d6cc"), (0.561, "#e8c9bd"), (1.0, "#f3d9cf")])];
        let bg = b.add(root, bg);
        cabinet(b, bg, (250.0, 36.0, 400.0), "#e9e9e7", ["#b98553", "#d4a370", "#a8744a"]);
        for i in 0..3 {
            cab_glyph(b, bg, i, (365.0 + 58.0 * i as f64, 470.0));
        }
        parts::centred(b, bg, "MIC A", (192.0, 540.0), Type::new(11.0, DIM, 700.0).track(0.08));
        parts::centred(b, bg, "MIC B", (707.0, 540.0), Type::new(11.0, DIM, 700.0).track(0.08));
        mic_panel(b, bg, "a", 30.0);
        mic_panel(b, bg, "b", 548.0);
        // The room send, between the mics.
        b.add(bg, draw::rect("rule", 400.0, 870.0 - 40.0, 100.0, 1.0, draw::solid("#0000001f")));
        parts::centred(b, bg, "ROOM SEND", (450.0, 842.0), Type::new(10.5, DIM, 700.0).track(0.06));
        toggle_button(b, bg, "cab/room_on", "Room", (412.0, 850.0, 76.0, 44.0));
        let p = Place::new("Room level").size(40.0).variant("light").bind(ui::bind("cab/room_level")).touch(TOUCH_MIN);
        let k = ui::put(b, bg, "knob.flat_line", &p, 450.0, 780.0);
        ui::strip_legend(b, k);
        ui::add_value(b, k, 12.0, INK);
    });
    let mut params = vec![parts::pick("cab/model", "Cab", CABS, 0)];
    for m in ["a", "b"] {
        let a = |s: &str| format!("cab/{m}_{s}");
        let up = m.to_uppercase();
        params.extend([
            parts::pick(&a("mic"), &format!("Mic {up}"), MICS, if m == "a" { 0 } else { 3 }),
            parts::switch(&a("on"), &format!("Mic {up} On"), true),
            ui::param(&a("pos"), &format!("Mic {up} Position"), Range::linear(0.0, 1.0, Unit::None), 0.5).featured(),
            ui::param(&a("dist"), &format!("Mic {up} Distance"), Range::linear(0.0, 1.0, Unit::None), 0.0).featured(),
            parts::db(&a("level"), &format!("Mic {up} Level"), -24.0, 12.0, 0.0),
            ui::param(&a("pan"), &format!("Mic {up} Pan"), Range::linear(-100.0, 100.0, Unit::Percent), 0.0).featured().bipolar(),
            parts::switch(&a("phase"), &format!("Mic {up} Phase"), false),
        ]);
    }
    params.push(parts::switch("cab/room_on", "Room Send", false));
    params.push(parts::db("cab/room_level", "Room Level", -24.0, 12.0, 0.0));
    ui::formatted(Face { file: "09-cab-900x900", plugin: PLUGIN, scene, params, streams: Vec::new() })
}
