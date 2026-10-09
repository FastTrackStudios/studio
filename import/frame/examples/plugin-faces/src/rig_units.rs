//! The rig's own units composed for a block of Signal's routing grid,
//! minimal: each only what makes it that unit — the Katana's one big knob,
//! the Klon's gold and its Centaur, the Bluesbreaker's plate, the 808's
//! green and its switch, the chorus's voices in their window, the spring
//! tank's springs ringing, the Distressor's gain-reduction lamps.
//!
//! The block's top and foot carry its preset and its name on bands of
//! their own: each unit sits in the middle, between them.

use document_proto::motion::{Motion, Target};
use document_proto::{NodeId, TextAlign};
use serde_json::json;
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_cells::{BLEED, S, block, face};

/// Each unit's params: on (its bypass).
const NS: &str = "unit";

fn on() -> Vec<surface_proto::ParamInfo> {
    vec![ui::toggle(&format!("{NS}/on"), "Bypass", true)]
}

/// A knob seen from above: its cap, a pointer at `deg` (0 up).
fn knob(b: &mut Builder<'_>, p: NodeId, (cx, cy): (f64, f64), r: f64, cap: (&str, &str), pointer: &str, deg: f64) {
    b.add(p, draw::with_effects(draw::circle("knob", cx, cy, r, draw::radial_at(0.38, 0.32, 0.9, &[(0.0, cap.0), (1.0, cap.1)])), vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)]));
    let tip = draw::polar(cx, cy, r * 0.82, deg);
    let root = draw::polar(cx, cy, r * 0.25, deg);
    b.add(p, ui::strokes("pointer", &[vec![root, tip]], (r * 0.14).max(1.6), draw::solid(pointer)));
}

/// A lamp, lit while the unit is on.
fn lamp(b: &mut Builder<'_>, p: NodeId, (cx, cy): (f64, f64), colour: &str) {
    b.add(p, draw::circle("lamp ring", cx, cy, 5.5, draw::solid("#0b0b0c")));
    let l = b.add(p, draw::emissive(draw::circle("lamp", cx, cy, 3.6, draw::solid(colour)), draw::hex(colour), 1.0, 9.0));
    motion::add(b, l, Motion::new(Target::Opacity).input(format!("param({NS}/on)")).range(0.12, 1.0));
}

fn words(b: &mut Builder<'_>, p: NodeId, s: &str, cy: f64, size: f64, colour: &str, l: &Lettering) {
    b.add(p, draw::styled_text("legend", s, 0.0, cy - size * 0.7, S, size, colour, TextAlign::Center, l));
}

/// The Keeley Katana: a black box and its one big chrome knob.
fn katana() -> Face {
    let scene = block("Katana", &[(0.0, "#2c2c30"), (1.0, "#111113")], |b, v, _| {
        knob(b, v, (S / 2.0, 92.0), 30.0, ("#f4f5f7", "#7d8188"), "#141416", 225.0);
        lamp(b, v, (150.0, 60.0), "#3b82f6");
        words(b, v, "KATANA", 140.0, 13.0, "#d4d6da", &Lettering::family(family::MICHROMA).weight(400.0));
    });
    face("40-unit-katana-block", scene, on())
}

/// The Klon Centaur: gold, three small knobs, the Centaur.
fn klon() -> Face {
    let scene = block("Klon", &[(0.0, "#ecd08a"), (0.55, "#d3ac55"), (1.0, "#a77f2c")], |b, v, _| {
        for (i, x) in [52.0, 92.0, 132.0].into_iter().enumerate() {
            knob(b, v, (x, 74.0), 13.0, ("#3a3a3c", "#0d0d0e"), "#f3e6c4", [250.0, 190.0, 230.0][i]);
        }
        words(b, v, "Centaur", 116.0, 26.0, "#3b2610", &Lettering::family(family::DANCING_SCRIPT).weight(700.0));
        lamp(b, v, (S / 2.0, 140.0), "#ef4444");
    });
    face("41-unit-klon-block", scene, on())
}

/// The Marshall Bluesbreaker: black, its cream script plate, gold knobs.
fn bluesbreaker() -> Face {
    let scene = block("Bluesbreaker", &[(0.0, "#232325"), (1.0, "#0e0e0f")], |b, v, _| {
        let plate = b.add(v, draw::with_effects(draw::rounded(draw::rect("plate", 22.0, 58.0, S - 44.0, 34.0, draw::linear(180.0, &[(0.0, "#f1e6c8"), (1.0, "#d8c99e")])), 5.0), vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)]));
        b.add(plate, draw::styled_text("logo", "Bluesbreaker", 0.0, 6.0, S - 44.0, 20.0, "#1b1712", TextAlign::Center, &Lettering::family(family::DANCING_SCRIPT).weight(700.0)));
        for x in [56.0, 92.0, 128.0] {
            knob(b, v, (x, 118.0), 11.0, ("#f0d98a", "#a8822c"), "#1b1712", 225.0);
        }
    });
    face("42-unit-bluesbreaker-block", scene, on())
}

/// The Ibanez TS808: green, its silver plate, its big switch.
fn ts808() -> Face {
    let scene = block("TS808", &[(0.0, "#5aa84f"), (1.0, "#2e6a2a")], |b, v, _| {
        knob(b, v, (54.0, 62.0), 13.0, ("#3a3a3c", "#0d0d0e"), "#e9eef0", 230.0);
        knob(b, v, (S / 2.0, 58.0), 8.5, ("#3a3a3c", "#0d0d0e"), "#e9eef0", 180.0);
        knob(b, v, (130.0, 62.0), 13.0, ("#3a3a3c", "#0d0d0e"), "#e9eef0", 210.0);
        let plate = b.add(v, draw::rect("plate", 14.0, 84.0, S - 28.0, 20.0, draw::linear(180.0, &[(0.0, "#eef0f2"), (1.0, "#b9bdc2")])));
        b.add(plate, draw::styled_text("model", "TS808 OVERDRIVE PRO", 0.0, 4.0, S - 28.0, 9.0, "#1a1c1e", TextAlign::Center, &Lettering::family(family::INTER).weight(800.0)));
        b.add(v, draw::with_effects(draw::rounded(draw::rect("switch", 40.0, 112.0, S - 80.0, 26.0, draw::linear(180.0, &[(0.0, "#e2e4e6"), (1.0, "#9a9ea3")])), 4.0), vec![draw::drop_shadow("#00000070", 0.0, 2.0, 3.0)]));
        lamp(b, v, (S / 2.0, 72.0), "#ef4444");
    });
    face("43-unit-ts808-block", scene, on())
}

/// A chorus pedal: aqua, its voices drifting past each other in a window.
fn chorus() -> Face {
    let scene = block("Chorus", &[(0.0, "#71d0e3"), (1.0, "#2b8aa3")], |b, v, _| {
        let mut w = draw::frame("Window", 22.0, 52.0, S - 44.0, 62.0);
        w.fills = vec![draw::linear(180.0, &[(0.0, "#082029"), (1.0, "#03111a")])];
        w.clips_content = true;
        w.corner_radii = document_proto::CornerRadii::uniform(6.0);
        let win = b.add(v, w);
        let (ww, wh) = (S - 44.0, 62.0);
        let wave = |amp: f64| -> Vec<(f64, f64)> {
            (0..=100).map(|k| {
                let x = -40.0 + (ww + 80.0) * f64::from(k) / 100.0;
                (x, wh / 2.0 - amp * (x / 40.0 * std::f64::consts::TAU).sin())
            }).collect()
        };
        for (i, (colour, phase)) in [("#38bdf8", 0.0), ("#a5f3fc", 2.1), ("#22d3ee", 4.2)].into_iter().enumerate() {
            let g = b.add(win, draw::frame(&format!("voice {i}"), 0.0, 0.0, ww, wh));
            b.add(g, draw::emissive(ui::strokes("voice", &[wave(14.0)], 2.0, draw::solid(colour)), draw::hex(colour), 0.6, 6.0));
            motion::add(b, g, Motion::new(Target::X).input(format!("0.5 + 0.5 * sin(time * 1.6 + {phase}) * param({NS}/on)")).range(-12.0, 12.0));
        }
        words(b, v, "CHORUS", 132.0, 13.0, "#062a36", &Lettering::family(family::MICHROMA).weight(400.0));
    });
    face("44-unit-chorus-block", scene, on())
}

/// A spring tank: only its springs, ringing while it is on.
fn spring() -> Face {
    let scene = block("Spring", &[(0.0, "#17181c"), (1.0, "#0a0b0d")], |b, v, _| {
        let (x0, x1) = (22.0, S - 22.0);
        for (i, y) in [66.0, 92.0, 118.0].into_iter().enumerate() {
            // A coil: many turns, end to end.
            let turns = 26.0;
            let coil: Vec<(f64, f64)> = (0..=240).map(|k| {
                let t = f64::from(k) / 240.0;
                (x0 + (x1 - x0) * t, 7.0 * (t * turns * std::f64::consts::TAU).sin())
            }).collect();
            let g = b.add(v, draw::frame(&format!("spring {i}"), 0.0, y, S, 0.0));
            b.add(g, ui::strokes("coil", &[coil], 1.6, draw::linear(90.0, &[(0.0, "#8a8f96"), (0.5, "#e6e9ec"), (1.0, "#8a8f96")])));
            let ring = format!("0.5 + 0.5 * sin(time * {} + {i}) * param({NS}/on)", 9.0 + 2.5 * i as f64);
            motion::add(b, g, Motion::new(Target::StretchY).input(ring).range(0.55, 1.35).pivot(0.5, 0.5));
            for x in [x0 - 6.0, x1] {
                b.add(v, draw::rounded(draw::rect("anchor", x, y - 6.0, 6.0, 12.0, draw::solid("#5b6068")), 2.0));
            }
        }
    });
    face("45-unit-spring-block", scene, on())
}

/// The Distressor: its black face, its name, and the gain-reduction lamps
/// working across it.
pub fn distressor() -> Face {
    let scene = block("Distressor", &[(0.0, "#1b1b1d"), (1.0, "#08080a")], |b, v, _| {
        words(b, v, "DISTRESSOR", 62.0, 12.0, "#e9e9ec", &Lettering::family(family::MICHROMA).weight(400.0));
        // Ten lamps: green, amber, red — as much as it is taking off.
        let gr = "(0.4 + 0.3 * sin(time * 2.1) + 0.15 * sin(time * 5.3))";
        for i in 0..10 {
            let x = 30.0 + f64::from(i) * 14.0;
            let colour = if i < 5 { "#4ade80" } else if i < 8 { "#facc15" } else { "#ef4444" };
            b.add(v, draw::rounded(draw::rect("well", x - 5.0, 84.0, 10.0, 18.0, draw::solid("#050506")), 2.0));
            let l = b.add(v, draw::emissive(draw::rounded(draw::rect("lamp", x - 4.0, 85.0, 8.0, 16.0, draw::solid(colour)), 2.0), draw::hex(colour), 0.9, 7.0));
            motion::add(b, l, Motion::new(Target::Opacity).input(format!("clamp(({gr} * 10 - {i}) * param(dist/on), 0, 1)")).range(0.08, 1.0));
        }
        knob(b, v, (44.0, 124.0), 11.0, ("#f1f2f4", "#8b8f95"), "#111", 240.0);
        knob(b, v, (140.0, 124.0), 11.0, ("#f1f2f4", "#8b8f95"), "#111", 200.0);
    });
    face("55-post-comp-distressor-block", scene, vec![ui::toggle("dist/on", "In", true)])
}

#[must_use]
pub fn faces() -> Vec<Face> {
    vec![katana(), klon(), bluesbreaker(), ts808(), chorus(), spring(), distressor()]
}

/// `faces.json`'s `blocks` for them: a drive by its pedal, the pre effects'
/// chorus and spring, any spring reverb.
#[must_use]
pub fn manifest() -> Vec<serde_json::Value> {
    let bleed = BLEED / S;
    let e = |m: &[&str], face: &str| json!({ "match": m, "face": face, "ns": NS, "bleed": bleed });
    vec![
        e(&["drive:keeley katana", "drive:katana"], "40-unit-katana-block"),
        e(&["drive:klon centaur", "drive:klon"], "41-unit-klon-block"),
        e(&["drive:marshall bluesbreaker", "drive:bluesbreaker"], "42-unit-bluesbreaker-block"),
        e(&["drive:ibanez ts808", "drive:ts808", "drive:tube screamer"], "43-unit-ts808-block"),
        e(&["pre:chorus"], "44-unit-chorus-block"),
        e(&["pre:reverb", "reverb:spring"], "45-unit-spring-block"),
        json!({ "match": ["post comp"], "face": "55-post-comp-distressor-block", "ns": "dist", "bleed": bleed }),
    ]
}
