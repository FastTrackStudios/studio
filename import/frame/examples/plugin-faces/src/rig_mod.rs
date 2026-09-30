//! The rig's modulation: a face for the Modulation module's chorus and the
//! Motion module's tremolo and vibrato, each a clean picture of what the
//! effect does to the sound, with its knobs beside it. Sleek, like the
//! Time module's Analog: a dark panel, one picture, a light name.
//!
//! Each fills the body of its panel (under the panel's own header, which
//! keeps the power, the ‹ › rotation and the engine picker): [`W`] × [`H`].
//! Colour, as Signal draws the groups: Modulation light blue, Motion pink.
//!
//! Binding: the chorus's params are `mod/<block param>`, the tremolo's and
//! the vibrato's `motion/<block param>`; `<ns>/on` is the block's bypass.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::NodeId;
use serde_json::json;
use surface_kit::{draw, motion};
use surface_proto::{ParamInfo, Range, Unit};
use surface_widgets::Builder;

use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;

pub const W: f64 = 480.0;
pub const H: f64 = 150.0;
pub const MOD: &str = "mod";
pub const MOTION: &str = "motion";
/// Modulation light blue, Motion pink (Signal's group colours).
pub const SKY: &str = "#7dd3fc";
pub const PINK: &str = "#ec4899";
use crate::rig_faces::PLUGIN;
const INK: &str = "#e8eef6";
const DIM: &str = "#7f8a9c";

/// `chorus::EngineType` order.
pub const MOD_ENGINES: [&str; 11] = ["Cubic", "BBD", "Tape", "Orbit", "Juno", "CE-2", "Dimension", "Clone", "Tri-Chorus", "SCF", "Julia"];
/// `TremMode` order.
pub const TREM_MODES: [&str; 3] = ["Mono", "Stereo", "Harmonic"];

fn a(ns: &str, id: &str) -> String {
    format!("{ns}/{id}")
}

/// A choice the block holds as an index (0, 1, 2…): ranged 0..n−1 so the
/// block's value lands on its option (a bare choice is ranged 0..1).
fn indexed(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    let last = options.len().saturating_sub(1).max(1) as f64;
    let mut p = ParamInfo::new(addr, name, Range::linear(0.0, last, Unit::None)).choice(options);
    p.default = default as f64 / last;
    p
}

fn chorus_params() -> Vec<ParamInfo> {
    let n = MOD;
    vec![
        ui::param(&a(n, "rate"), "Rate", Range::log(0.05, 10.0, Unit::Hz), 1.0),
        ui::param(&a(n, "depth"), "Depth", Range::linear(0.0, 1.0, Unit::None), 0.4),
        ui::param(&a(n, "mix"), "Mix", Range::linear(0.0, 1.0, Unit::None), 0.4),
        indexed(&a(n, "engine"), "Engine", &MOD_ENGINES, 0),
        ui::param(&a(n, "color"), "Color", Range::linear(0.0, 1.0, Unit::None), 0.5),
        ui::param(&a(n, "feedback"), "Feedback", Range::linear(0.0, 1.0, Unit::None), 0.0),
        ui::param(&a(n, "width"), "Width", Range::linear(0.0, 1.0, Unit::None), 1.0),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

fn trem_params() -> Vec<ParamInfo> {
    let n = MOTION;
    vec![
        ui::param(&a(n, "rate"), "Rate", Range::log(0.05, 12.0, Unit::Hz), 4.0),
        ui::param(&a(n, "depth"), "Depth", Range::linear(0.0, 1.0, Unit::None), 0.5),
        ui::param(&a(n, "mix"), "Mix", Range::linear(0.0, 1.0, Unit::None), 1.0),
        indexed(&a(n, "mode"), "Mode", &TREM_MODES, 1),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

fn vibrato_params() -> Vec<ParamInfo> {
    let n = MOTION;
    vec![
        ui::param(&a(n, "rate"), "Rate", Range::log(0.05, 10.0, Unit::Hz), 5.0),
        ui::param(&a(n, "depth"), "Depth", Range::linear(0.0, 1.0, Unit::None), 0.4),
        ui::param(&a(n, "mix"), "Mix", Range::linear(0.0, 1.0, Unit::None), 1.0),
        indexed(&a(n, "engine"), "Engine", &MOD_ENGINES, 0),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

/// A panel's canvas: `build` draws into the root, already `W` × `H`.
fn panel(name: &str, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> document_proto::Scene {
    canvas(name, |b, root| {
        ui::fit_root(b, root, W, H);
        let mut p = draw::frame("Panel", 0.0, 0.0, W, H);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#121821"), (1.0, "#0a0e14")])];
        p.corner_radii = document_proto::CornerRadii::uniform(8.0);
        let p = b.add(root, draw::with_effects(draw::stroked(p, "#232c3a", 1.0), vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]));
        build(b, p);
    })
}

/// The picture's window, (`x`, `y`, `w`, `h`), clipped.
fn view(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    let mut v = draw::frame("View", x, y, w, h);
    v.fills = vec![draw::linear(180.0, &[(0.0, "#080c12"), (1.0, "#05080c")])];
    v.clips_content = true;
    v.corner_radii = document_proto::CornerRadii::uniform(6.0);
    v.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 6.0)];
    b.add(parent, v)
}

/// The name, light, over its line of description, at (`x`, `y`).
fn title(b: &mut Builder<'_>, parent: NodeId, name: &str, sub: &str, (x, y): (f64, f64), color: &str) {
    let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
    let w = name.chars().count() as f64 * 11.0;
    crate::suite_studio_blues::parts::set(b, parent, name, x + w / 2.0, y, 20.0, INK, &sans, 0.02);
    ui::silk_left(b, parent, sub, x, y + 17.0, 6.0, DIM, 700.0);
    ui::rule(b, parent, x, y + 25.0, 150.0, 1.0, &ui::alpha(color, 0.55));
}

/// Knobs in a row from `x`, `step` apart, centred on `cy`.
fn knobs(b: &mut Builder<'_>, parent: NodeId, ns: &str, list: &[(&str, &str)], (x, cy): (f64, f64), step: f64, color: &str) {
    let pointer = format!("pointer={color}");
    let opts = ["color=#1a2230", pointer.as_str(), "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0a0e14"];
    for (i, (id, legend)) in list.iter().enumerate() {
        kit::knob3d(b, parent, &a(ns, id), legend, 28.0, (x + step * i as f64, cy), &opts, INK, DIM);
    }
}

/// The engine's name, lit, at (`x`, `cy`) — `ns/engine` or `ns/mode`.
fn lit_name(b: &mut Builder<'_>, parent: NodeId, addr: &str, (x, cy, w): (f64, f64, f64), color: &str) -> NodeId {
    crate::delay::parts::readout(b, parent, &format!("{addr} name"), ui::bind(addr), (x, cy, w), 8.0, color, document_proto::TextAlign::Left)
}

// ── Chorus ───────────────────────────────────────────────────────────────

/// The chorus: the note's wave, bright, and its voices — copies of it,
/// each drifting ahead and behind on the LFO (the depth how far, the rate
/// how fast), spread apart with the width and as strong as the mix. The
/// thickening you hear, as you see it.
fn chorus() -> Face {
    let scene = panel("Chorus", |b, p| {
        let (vx, vy, vw, vh) = (10.0, 10.0, 270.0, H - 20.0);
        let v = view(b, p, (vx, vy, vw, vh));
        let mid = vh / 2.0;
        let period = 64.0;
        let wave = |amp: f64| -> Vec<(f64, f64)> {
            (0..=160).map(|k| {
                let x = -period + (vw + 2.0 * period) * f64::from(k) / 160.0;
                (x, mid - amp * (x / period * std::f64::consts::TAU).sin())
            }).collect()
        };
        // The voices, behind.
        let voices = [(-1.0, 0.0), (1.0, 2.1), (-0.5, 4.2), (0.5, 1.3)];
        for (i, (spread, phase)) in voices.iter().enumerate() {
            let g = b.add(v, draw::frame(&format!("voice {i}"), 0.0, 0.0, vw, vh));
            b.add(g, ui::glowing(ui::strokes("voice", &[wave(26.0)], 1.4, draw::solid(SKY)), SKY, 0.5, 4.0));
            let drift = format!("0.5 + 0.5 * sin(6.2832 * time * param({MOD}/rate) + {phase}) ");
            let reach = 4.0 + 12.0 * f64::from(u8::from(i < 4));
            motion::add(b, g, Motion::new(Target::X).input(format!("{drift} * norm({MOD}/depth) + 0.5 * (1 - norm({MOD}/depth))")).range(-reach, reach));
            motion::add(b, g, Motion::new(Target::Y).input(format!("0.5 + 0.5 * {spread} * norm({MOD}/width)")).range(-14.0, 14.0).dynamics(Dynamics::spring(200.0, 26.0)));
            motion::add(b, g, Motion::new(Target::Opacity).input(format!("(0.25 + 0.75 * param({MOD}/mix)) * (0.2 + 0.8 * param({MOD}/on))")).range(0.0, 0.7));
        }
        // The note, in front.
        b.add(v, ui::strokes("dry", &[wave(26.0)], 1.8, draw::solid(INK)));
        ui::silk_left(b, v, "Dry", 8.0, 10.0, 5.5, DIM, 800.0);
        ui::silk_left(b, v, "Voices", 30.0, 10.0, 5.5, SKY, 800.0);

        let x0 = 296.0;
        title(b, p, "Chorus", "Voices · detune · width", (x0, 26.0), SKY);
        lit_name(b, p, &a(MOD, "engine"), (x0 + 110.0, 26.0, 70.0), SKY);
        knobs(b, p, MOD, &[("rate", "Rate"), ("depth", "Depth"), ("width", "Width"), ("mix", "Mix")], (x0 + 18.0, 88.0), 44.0, SKY);
    });
    Face { file: "100-mod-chorus", plugin: PLUGIN, scene, params: chorus_params(), streams: Vec::new() }
}

// ── Tremolo ──────────────────────────────────────────────────────────────

/// The tremolo: its lamp — the volume now, pulsing — and the volume's
/// curve rolling past it, as deep as the depth and as fast as the rate.
/// Stereo and Harmonic show the second side (or band) against it.
fn tremolo() -> Face {
    let scene = panel("Tremolo", |b, p| {
        let (vx, vy, vw, vh) = (10.0, 10.0, 290.0, H - 20.0);
        let v = view(b, p, (vx, vy, vw, vh));
        let (lamp_x, top, bottom) = (40.0, 22.0, vh - 20.0);
        let period = 60.0;
        // The curve: the gain between 1 (top) and 1 − depth, one LFO cycle
        // `period` wide, rolling left as the rate runs.
        let span = vw - lamp_x;
        let curve = |phase: f64| -> Vec<(f64, f64)> {
            (0..=180).map(|k| {
                let x = (span + period) * f64::from(k) / 180.0;
                let g = 0.5 + 0.5 * ((x / period + phase) * std::f64::consts::TAU).cos();
                (x, top + (bottom - top) * (1.0 - g))
            }).collect()
        };
        for (side, phase, color, label) in [(0_u8, 0.0, PINK, "L"), (1, 0.5, "#f9a8d4", "R")] {
            let mut clip = draw::frame(&format!("curve {label} window"), lamp_x, 0.0, span, vh);
            clip.clips_content = true;
            let clip = b.add(v, clip);
            let roll = b.add(clip, draw::frame(&format!("curve {label}"), 0.0, 0.0, span + period, vh));
            b.add(roll, ui::glowing(ui::strokes("gain", &[curve(phase)], 1.8, draw::solid(color)), color, 0.6, 4.0));
            motion::add(b, roll, Motion::new(Target::X).input(format!("fract(time * param({MOTION}/rate) * param({MOTION}/on))")).range(0.0, -period));
            motion::add(b, roll, Motion::new(Target::StretchY).input(format!("norm({MOTION}/depth)")).range(0.02, 1.0).pivot(0.5, top / vh).dynamics(Dynamics::spring(220.0, 26.0)));
            if side == 1 {
                // The second side shows in Stereo and Harmonic.
                motion::add(b, roll, Motion::new(Target::Opacity).input(format!("clamp(param({MOTION}/mode), 0, 1)")).range(0.0, 0.8));
            }
        }
        // Full volume and the floor, faint.
        b.add(v, ui::strokes("full", &[vec![(lamp_x, top), (vw, top)]], 0.8, draw::solid("#ffffff1a")));
        ui::silk_left(b, v, "Volume", lamp_x + 4.0, top - 8.0, 5.5, DIM, 800.0);
        // The lamp: the volume now.
        let lamp = b.add(v, draw::frame("lamp", lamp_x - 20.0, vh / 2.0 - 20.0, 40.0, 40.0));
        b.add(lamp, draw::emissive(draw::circle("glow", 20.0, 20.0, 11.0, draw::radial_at(0.45, 0.4, 0.7, &[(0.0, "#ffe4f2"), (0.6, PINK), (1.0, "#9d174d")])), draw::hex(PINK), 1.0, 14.0));
        let now = format!("(1 - norm({MOTION}/depth) * (0.5 - 0.5 * cos(6.2832 * time * param({MOTION}/rate)))) * param({MOTION}/on)");
        motion::add(b, lamp, Motion::new(Target::Opacity).input(now.clone()).range(0.12, 1.0));
        motion::add(b, lamp, Motion::new(Target::Scale).input(now).range(0.75, 1.0));
        b.add(v, ui::strokes("now", &[vec![(lamp_x, 8.0), (lamp_x, vh - 8.0)]], 0.8, draw::solid(&ui::alpha(PINK, 0.35))));

        let x0 = 316.0;
        title(b, p, "Tremolo", "Volume · rate · depth", (x0, 26.0), PINK);
        lit_name(b, p, &a(MOTION, "mode"), (x0 + 110.0, 26.0, 50.0), PINK);
        knobs(b, p, MOTION, &[("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")], (x0 + 22.0, 88.0), 52.0, PINK);
    });
    Face { file: "101-motion-tremolo", plugin: PLUGIN, scene, params: trem_params(), streams: Vec::new() }
}

// ── Vibrato ──────────────────────────────────────────────────────────────

/// The vibrato: pitch, as a tuner reads it — the needle swinging sharp and
/// flat about the note, as far as the depth and as fast as the rate — over
/// the wave it bends, its cycles crowding and spreading.
fn vibrato() -> Face {
    let scene = panel("Vibrato", |b, p| {
        let (vx, vy, vw, vh) = (10.0, 10.0, 290.0, H - 20.0);
        let v = view(b, p, (vx, vy, vw, vh));
        // The tuner's scale: flat to sharp, the note at the top.
        let (cx, cy, r) = (vw / 2.0, vh + 30.0, 120.0);
        let arc: Vec<(f64, f64)> = (0..=48).map(|i| draw::polar(cx, cy, r, -40.0 + 80.0 * f64::from(i) / 48.0)).collect();
        b.add(v, ui::strokes("scale", &[arc], 1.0, draw::solid("#ffffff33")));
        for i in -4..=4 {
            let ang = 10.0 * f64::from(i);
            let (len, col) = if i == 0 { (14.0, PINK) } else { (7.0, "#ffffff4d") };
            b.add(v, ui::strokes("tick", &[vec![draw::polar(cx, cy, r, ang), draw::polar(cx, cy, r - len, ang)]], 1.2, draw::solid(col)));
        }
        ui::silk(b, v, "Flat", cx - r * 0.78, cy - r * 0.6, 6.0, DIM, 800.0);
        ui::silk(b, v, "Sharp", cx + r * 0.78, cy - r * 0.6, 6.0, DIM, 800.0);
        // The needle: swinging as the pitch does.
        let side = 2.0 * (r + 6.0);
        let needle = b.add(v, draw::frame("needle", cx - side / 2.0, cy - side / 2.0, side, side));
        b.add(needle, ui::glowing(draw::rect("blade", side / 2.0 - 1.0, side / 2.0 - r + 2.0, 2.0, r - 44.0, draw::solid(PINK)), PINK, 0.8, 5.0));
        let swing = format!("0.5 + 0.5 * sin(6.2832 * time * param({MOTION}/rate)) * norm({MOTION}/depth) * param({MOTION}/on)");
        motion::add(b, needle, Motion::new(Target::Rotate).input(swing.clone()).range(-38.0, 38.0).pivot(0.5, 0.5));
        // The wave it bends: squeezed and stretched with the pitch.
        let wave: Vec<(f64, f64)> = (0..=160).map(|k| {
            let x = vw * f64::from(k) / 160.0;
            (x, vh - 22.0 - 9.0 * (x / 18.0 * std::f64::consts::TAU).sin())
        }).collect();
        let w = b.add(v, draw::frame("wave", 0.0, 0.0, vw, vh));
        b.add(w, ui::strokes("wave", &[wave], 1.4, draw::solid(&ui::alpha(INK, 0.7))));
        motion::add(b, w, Motion::new(Target::StretchX).input(swing).range(1.12, 0.88).pivot(0.0, 0.5));

        let x0 = 316.0;
        title(b, p, "Vibrato", "Pitch · rate · depth", (x0, 26.0), PINK);
        lit_name(b, p, &a(MOTION, "engine"), (x0 + 110.0, 26.0, 50.0), PINK);
        knobs(b, p, MOTION, &[("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")], (x0 + 22.0, 88.0), 52.0, PINK);
    });
    Face { file: "102-motion-vibrato", plugin: PLUGIN, scene, params: vibrato_params(), streams: Vec::new() }
}

/// Every modulation face.
#[must_use]
pub fn faces() -> Vec<Face> {
    vec![chorus(), tremolo(), vibrato()]
}

/// `faces.json`'s `modulation` entry: each block type's face.
#[must_use]
pub fn manifest() -> serde_json::Value {
    let size = [W, H];
    json!([
        { "match": ["chorus"], "face": "100-mod-chorus", "ns": MOD, "size": size },
        { "match": ["trem", "tremolo"], "face": "101-motion-tremolo", "ns": MOTION, "size": size },
        { "match": ["vibrato"], "face": "102-motion-vibrato", "ns": MOTION, "size": size },
    ])
}
