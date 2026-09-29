//! The four pre-FX stompboxes (156 × 286, the Standard class): soft satin
//! enclosures in cream, peach and khaki, teal fluted knobs inside fine tick
//! rings with `min` / `max`, the brush mark at the top, the block-caps name
//! between chevrons, and a carved walnut plate round a brass stomp with a
//! teal cap and a glowing teal LED.
//!
//! - `01-comp-standard-156x286`: COMP (Comp, EQ, Volume, Mix) — grooves.
//! - `02-od1-standard-156x286`: OD1 (Volume, Gain, Tone, Mid Boost) — a
//!   hexagon lattice.
//! - `03-od2-standard-156x286`: OD2 (Volume, Gain, Cut) — tumbling blocks
//!   with droplets.
//! - `04-mod-standard-156x286`: MOD (Speed, Intensity; tremolo / chorus /
//!   vibrato on a three-way toggle) — concentric rings; its LED breathes
//!   at the speed.

use document_proto::NodeId;
use surface_proto::{Range, Unit};
use surface_widgets::Builder;

use super::look::{self, INK, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, PEDAL_H, STANDARD_W, Type};

const DIM: &str = "#4a4744";
const KNOB_D: f64 = 38.0;

/// Plate geometry (in the lid's space).
const PLATE: (f64, f64, f64, f64) = (10.0, 177.0, 136.0, 92.0);

struct Pedal<'a> {
    title: &'a str,
    ns: &'a str,
    name: &'a str,
    color: &'a str,
    /// Knob: param id, legend, centre, legend y.
    knobs: &'a [(&'a str, &'a str, (f64, f64), f64)],
    carve: fn(&mut Builder<'_>, NodeId),
    pulse: Option<&'a str>,
    extra: Option<fn(&mut Builder<'_>, NodeId)>,
}

fn build(p: &Pedal<'_>) -> document_proto::Scene {
    canvas(p.title, |b, root| {
        let lid = parts::stompbox(b, root, STANDARD_W, PEDAL_H, p.color, "satin", 12.0);
        look::brush_mark(b, lid, (STANDARD_W / 2.0, 11.0), 8.0, INK);
        for (id, legend, at, ly) in p.knobs {
            parts::knob(b, lid, "knob.fluted_line", "teal", &format!("{}/{id}", p.ns), legend, KNOB_D, *at, &["ticks=29", "minmax"], DIM);
            parts::centred(b, lid, legend, (at.0, *ly), Type::new(7.0, INK, 800.0).track(0.03));
        }
        if let Some(extra) = p.extra {
            extra(b, lid);
        }
        look::name_row(b, lid, p.name, STANDARD_W, 153.0, INK);
        let plate = look::walnut(b, lid, PLATE);
        (p.carve)(b, plate);
        let on = format!("{}/on", p.ns);
        parts::status_led(b, lid, &on, 5.5, (STANDARD_W / 2.0, 193.0), TEAL, "dark", p.pulse);
        look::stomp(b, lid, &on, 20.0, (STANDARD_W / 2.0, 231.0), "teal");
    })
}

// ── carvings, in the plate's own space (136 × 92) ─────────────────────────

fn grooves(b: &mut Builder<'_>, plate: NodeId) {
    let lines: Vec<Vec<(f64, f64)>> = (0..34).map(|i| {
        let x = 2.0 + 4.0 * f64::from(i);
        vec![(x, -2.0), (x, 94.0)]
    }).collect();
    look::carve(b, plate, &lines, 1.5);
}

fn hexes(b: &mut Builder<'_>, plate: NodeId) {
    let r = 7.5;
    let (dx, dy) = (r * 3.0_f64.sqrt(), r * 1.5);
    let mut lines = Vec::new();
    for row in -1..9 {
        for col in -1..11 {
            let cx = f64::from(col) * dx + if row % 2 == 0 { 0.0 } else { dx / 2.0 };
            let cy = f64::from(row) * dy;
            let mut hex: Vec<(f64, f64)> = (0..=6).map(|i| surface_kit::draw::polar(cx, cy, r, 60.0 * f64::from(i))).collect();
            let inner: Vec<(f64, f64)> = (0..=6).map(|i| surface_kit::draw::polar(cx, cy, r * 0.5, 60.0 * f64::from(i))).collect();
            hex.truncate(7);
            lines.push(hex);
            lines.push(inner[..4].to_vec());
        }
    }
    look::carve(b, plate, &lines, 0.9);
}

fn blocks(b: &mut Builder<'_>, plate: NodeId) {
    let r = 11.0;
    let (dx, dy) = (r * 3.0_f64.sqrt(), r * 1.5);
    let mut lines = Vec::new();
    let mut drops = String::new();
    for row in -1..8 {
        for col in -1..9 {
            let cx = f64::from(col) * dx + if row % 2 == 0 { 0.0 } else { dx / 2.0 };
            let cy = f64::from(row) * dy;
            let hex: Vec<(f64, f64)> = (0..=6).map(|i| surface_kit::draw::polar(cx, cy, r, 60.0 * f64::from(i))).collect();
            lines.push(hex);
            // The cube's three edges from the centre.
            for a in [0.0, 120.0, 240.0] {
                lines.push(vec![(cx, cy), surface_kit::draw::polar(cx, cy, r, a)]);
            }
            // A droplet on each face.
            for a in [60.0, 180.0, 300.0] {
                let (x, y) = surface_kit::draw::polar(cx, cy, r * 0.5, a);
                let (tx, ty) = surface_kit::draw::polar(x, y, 3.2, a + 180.0);
                drops.push_str(&format!("M {tx:.2} {ty:.2} Q {:.2} {:.2} {x:.2} {y:.2} Q {:.2} {:.2} {tx:.2} {ty:.2} Z ", x + 2.4, y - 0.5, x - 2.4, y + 0.5));
            }
        }
    }
    look::carve(b, plate, &lines, 0.9);
    b.add(plate, surface_kit::draw::path("droplets", 0.0, 0.0, 136.0, 92.0, drops, surface_kit::draw::solid("#4a2213b0")));
}

fn rings(b: &mut Builder<'_>, plate: NodeId) {
    let (cx, cy) = (STANDARD_W / 2.0 - PLATE.0, 231.0 - PLATE.1);
    let lines: Vec<Vec<(f64, f64)>> = (1..22).map(|i| parts::ring(cx, cy, 6.0 + 5.0 * f64::from(i))).collect();
    look::carve(b, plate, &lines, 1.6);
}

// ── the pedals ───────────────────────────────────────────────────────────

const TWO_BY_TWO: [((f64, f64), f64); 4] = [((38.0, 37.0), 68.0), ((118.0, 37.0), 68.0), ((38.0, 98.0), 130.0), ((118.0, 98.0), 130.0)];

pub(super) fn comp() -> Face {
    let k = [("comp", "Comp"), ("eq", "EQ"), ("volume", "Volume"), ("mix", "Mix")];
    let knobs: Vec<(&str, &str, (f64, f64), f64)> = k.iter().zip(TWO_BY_TWO).map(|((id, l), (at, y))| (*id, *l, at, y)).collect();
    let scene = build(&Pedal { title: "Comp", ns: "comp", name: "COMP", color: "#ebe5da", knobs: &knobs, carve: grooves, pulse: None, extra: None });
    let params = vec![parts::pct("comp/comp", "Comp", 50.0), parts::pct("comp/eq", "EQ", 50.0), parts::db("comp/volume", "Volume", -12.0, 12.0, 0.0), parts::pct("comp/mix", "Mix", 60.0), parts::switch("comp/on", "Bypass", true)];
    look::face("01-comp-standard-156x286", scene, params)
}

pub(super) fn od1() -> Face {
    let k = [("volume", "Volume"), ("gain", "Gain"), ("tone", "Tone"), ("mid", "Mid Boost")];
    let knobs: Vec<(&str, &str, (f64, f64), f64)> = k.iter().zip(TWO_BY_TWO).map(|((id, l), (at, y))| (*id, *l, at, y)).collect();
    let scene = build(&Pedal { title: "OD1", ns: "od1", name: "OD1", color: "#e6e0d6", knobs: &knobs, carve: hexes, pulse: None, extra: None });
    let params = vec![parts::db("od1/volume", "Volume", -12.0, 12.0, 0.0), parts::pct("od1/gain", "Gain", 35.0), parts::pct("od1/tone", "Tone", 50.0), parts::pct("od1/mid", "Mid Boost", 40.0), parts::switch("od1/on", "Bypass", true)];
    look::face("02-od1-standard-156x286", scene, params)
}

pub(super) fn od2() -> Face {
    let knobs = [("volume", "Volume", (40.0, 38.0), 67.0), ("gain", "Gain", (116.0, 38.0), 67.0), ("cut", "Cut", (78.0, 99.0), 127.0)];
    let scene = build(&Pedal { title: "OD2", ns: "od2", name: "OD2", color: "#ecc7ae", knobs: &knobs, carve: blocks, pulse: None, extra: None });
    let params = vec![parts::db("od2/volume", "Volume", -12.0, 12.0, 0.0), parts::pct("od2/gain", "Gain", 55.0), parts::pct("od2/cut", "Cut", 40.0), parts::switch("od2/on", "Bypass", true)];
    look::face("03-od2-standard-156x286", scene, params)
}

fn mode_switch(b: &mut Builder<'_>, lid: NodeId) {
    look::brass_toggle3(b, lid, "mod/mode", "Mode", 18.0, (78.0, 100.0));
    let t = Type::new(7.0, INK, 800.0).track(0.03);
    parts::centred(b, lid, "Tremolo", (40.0, 96.0), t);
    parts::centred(b, lid, "Vibrato", (116.0, 96.0), t);
    parts::centred(b, lid, "Chorus", (78.0, 124.0), t);
}

pub(super) fn modulation() -> Face {
    let knobs = [("speed", "Speed", (40.0, 38.0), 67.0), ("intensity", "Intensity", (116.0, 38.0), 67.0)];
    let scene = build(&Pedal { title: "MOD", ns: "mod", name: "MOD", color: "#dcd2b3", knobs: &knobs, carve: rings, pulse: Some("param(mod/speed)"), extra: Some(mode_switch) });
    let params = vec![
        ui::param("mod/speed", "Speed", Range::log(0.1, 12.0, Unit::Hz), 3.0).featured(),
        parts::pct("mod/intensity", "Intensity", 50.0),
        // In the toggle's order: right, down, left.
        parts::pick("mod/mode", "Mode", &["Vibrato", "Chorus", "Tremolo"], 1),
        parts::switch("mod/on", "Bypass", true),
    ];
    look::face("04-mod-standard-156x286", scene, params)
}
