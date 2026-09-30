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
pub const H: f64 = 168.0;
pub const MOD: &str = "mod";
pub const MOTION: &str = "motion";
/// Modulation light blue, Motion pink (Signal's group colours).
pub const SKY: &str = "#7dd3fc";
pub const PINK: &str = "#ec4899";
use crate::rig_faces::PLUGIN;

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

/// How a face is finished: its panel, its picture's window, its inks and
/// its knobs. Each effect has its own, so the chorus and the tremolo can't
/// be mistaken for each other.
struct Theme {
    panel: [(f64, &'static str); 2],
    edge: &'static str,
    window: [(f64, &'static str); 2],
    ink: &'static str,
    dim: &'static str,
    accent: &'static str,
    knob: &'static str,
    skirt: &'static str,
    bypass: &'static str,
}

/// A face's canvas: `build` draws into the panel, already `W` × `H`. The
/// top-right corner ([`CORNER`]) is left for Signal's switcher pill.
fn panel(name: &str, t: &Theme, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> document_proto::Scene {
    let (fill, edge) = (t.panel, t.edge);
    canvas(name, move |b, root| {
        ui::fit_root(b, root, W, H);
        let mut p = draw::frame("Panel", 0.0, 0.0, W, H);
        p.fills = vec![draw::linear(180.0, &fill)];
        p.corner_radii = document_proto::CornerRadii::uniform(8.0);
        let p = b.add(root, draw::with_effects(draw::stroked(p, edge, 1.0), vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)]));
        build(b, p);
    })
}

/// The picture's window on the right, clipped.
fn view(b: &mut Builder<'_>, parent: NodeId, t: &Theme) -> (NodeId, f64, f64) {
    let (x, y, w, h) = (VIEW_X, 8.0, W - VIEW_X - 8.0, H - 16.0);
    let mut v = draw::frame("View", x, y, w, h);
    v.fills = vec![draw::linear(180.0, &t.window)];
    v.clips_content = true;
    v.corner_radii = document_proto::CornerRadii::uniform(6.0);
    v.effects = vec![draw::inner_shadow("#00000080", 0.0, 2.0, 6.0)];
    (b.add(parent, v), w, h)
}

/// The picture's window starts here; left of it, the name and the knobs.
const VIEW_X: f64 = 200.0;

/// The left column: the name, light, its line of description, the On key
/// beside the name, and the knobs in a row under them.
fn column(b: &mut Builder<'_>, p: NodeId, t: &Theme, ns: &str, name: &str, sub: &str, list: &[(&str, &str)]) {
    let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
    let w = name.chars().count() as f64 * 11.5;
    crate::suite_studio_blues::parts::set(b, p, name, 14.0 + w / 2.0, 30.0, 21.0, t.ink, &sans, 0.02);
    ui::silk_left(b, p, sub, 14.0, 49.0, 6.0, t.dim, 700.0);
    ui::rule(b, p, 14.0, 57.0, VIEW_X - 30.0, 1.0, &ui::alpha(t.accent, 0.6));
    kit::bypass(b, p, ns, VIEW_X - 30.0, 30.0, 22.0, t.bypass, "On");
    let pointer = format!("pointer={}", t.accent);
    let color = format!("color={}", t.knob);
    let skirt = format!("skirt_color={}", t.skirt);
    let opts = [color.as_str(), pointer.as_str(), "index=line", "index_w=0.1", "skirt=plain", skirt.as_str()];
    let step = ((VIEW_X - 30.0) / list.len() as f64).min(46.0);
    for (i, (id, legend)) in list.iter().enumerate() {
        kit::knob3d(b, p, &a(ns, id), legend, 28.0, (14.0 + step * (i as f64 + 0.5), 100.0), &opts, t.ink, t.dim);
    }
}

// ── Chorus ───────────────────────────────────────────────────────────────

/// Light and airy: an ice-blue panel, a bright window.
const CHORUS: Theme = Theme {
    panel: [(0.0, "#eaf6fd"), (1.0, "#cfe6f4")],
    edge: "#a9cde4",
    window: [(0.0, "#fbfeff"), (1.0, "#e2f2fb")],
    ink: "#12314a",
    dim: "#5d7d96",
    accent: "#0ea5e9",
    knob: "#f8fbfd",
    skirt: "#9cc3dc",
    bypass: "blue",
};

/// The chorus: the note's wave, dark, and its voices — copies of it in
/// blues, each drifting ahead and behind on the LFO (the depth how far,
/// the rate how fast), spread apart with the width and as strong as the
/// mix. The thickening you hear, as you see it: light through water.
fn chorus() -> Face {
    let t = &CHORUS;
    let scene = panel("Chorus", t, |b, p| {
        let (v, vw, vh) = view(b, p, t);
        let mid = vh / 2.0 + 6.0;
        let period = 70.0;
        let wave = |amp: f64| -> Vec<(f64, f64)> {
            (0..=160).map(|k| {
                let x = -period + (vw + 2.0 * period) * f64::from(k) / 160.0;
                (x, mid - amp * (x / period * std::f64::consts::TAU).sin())
            }).collect()
        };
        // Soft caustics behind: light through water.
        for i in 0..5 {
            let f = f64::from(i);
            let c = b.add(v, draw::circle("caustic", 40.0 + f * 58.0, mid + 18.0 * (f * 1.7).sin(), 38.0, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#7dd3fc40"), (1.0, "#7dd3fc00")])));
            motion::add(b, c, Motion::new(Target::X).input(format!("0.5 + 0.5 * sin(time * 0.4 + {f})")).range(-10.0, 10.0));
        }
        // The voices: each its own blue.
        let voices = [(-1.0, 0.0, "#0ea5e9"), (1.0, 2.1, "#38bdf8"), (-0.5, 4.2, "#6366f1"), (0.5, 1.3, "#22d3ee")];
        for (i, (spread, phase, color)) in voices.iter().enumerate() {
            let g = b.add(v, draw::frame(&format!("voice {i}"), 0.0, 0.0, vw, vh));
            b.add(g, ui::strokes("voice", &[wave(30.0)], 2.2, draw::solid(color)));
            let drift = format!("0.5 + 0.5 * sin(6.2832 * time * param({MOD}/rate) + {phase})");
            motion::add(b, g, Motion::new(Target::X).input(format!("{drift} * norm({MOD}/depth) + 0.5 * (1 - norm({MOD}/depth))")).range(-16.0, 16.0));
            motion::add(b, g, Motion::new(Target::Y).input(format!("0.5 + 0.5 * {spread} * norm({MOD}/width)")).range(-16.0, 16.0).dynamics(Dynamics::spring(200.0, 26.0)));
            motion::add(b, g, Motion::new(Target::Opacity).input(format!("(0.25 + 0.75 * param({MOD}/mix)) * (0.2 + 0.8 * param({MOD}/on))")).range(0.0, 0.75));
        }
        // The note, in front.
        b.add(v, ui::strokes("dry", &[wave(30.0)], 2.0, draw::solid(t.ink)));
        ui::silk_left(b, v, "Dry", 10.0, 12.0, 5.5, t.ink, 800.0);
        ui::silk_left(b, v, "Voices", 30.0, 12.0, 5.5, t.accent, 800.0);
        column(b, p, t, MOD, "Chorus", "Voices · detune · width", &[("rate", "Rate"), ("depth", "Depth"), ("width", "Width"), ("mix", "Mix")]);
    });
    Face { file: "100-mod-chorus", plugin: PLUGIN, scene, params: chorus_params(), streams: Vec::new() }
}

// ── Tremolo ──────────────────────────────────────────────────────────────

/// A neon sign on black brick.
const TREMOLO: Theme = Theme {
    panel: [(0.0, "#1a0b14"), (1.0, "#0c0509")],
    edge: "#3a1328",
    window: [(0.0, "#12060d"), (1.0, "#070205")],
    ink: "#ffe4f1",
    dim: "#b07896",
    accent: PINK,
    knob: "#24121c",
    skirt: "#0c0509",
    bypass: "red",
};

/// The tremolo, as neon: a ring that brightens and fades with the volume
/// now, and the volume's curve as a neon tube rolling out of it, as deep
/// as the depth and as fast as the rate. Stereo and Harmonic light a
/// second ring and tube against the first.
fn tremolo() -> Face {
    let t = &TREMOLO;
    let scene = panel("Tremolo", t, |b, p| {
        let (v, vw, vh) = view(b, p, t);
        // Brick behind the sign, barely lit.
        let mut rows: Vec<Vec<(f64, f64)>> = (1..8).map(|i| vec![(0.0, f64::from(i) * 19.0), (vw, f64::from(i) * 19.0)]).collect();
        for r in 0..8 {
            for k in 0..8 {
                let x = f64::from(k) * 40.0 + if r % 2 == 0 { 0.0 } else { 20.0 };
                rows.push(vec![(x, f64::from(r) * 19.0), (x, f64::from(r + 1) * 19.0)]);
            }
        }
        b.add(v, ui::strokes("brick", &rows, 1.0, draw::solid("#ffffff0a")));
        // Full volume sits under the corner pill's height, clear of it.
        let (lamp_x, top, bottom) = (46.0, 44.0, vh - 18.0);
        let period = 64.0;
        let span = vw - lamp_x;
        let curve = |phase: f64| -> Vec<(f64, f64)> {
            (0..=180).map(|k| {
                let x = (span + period) * f64::from(k) / 180.0;
                let g = 0.5 + 0.5 * ((x / period + phase) * std::f64::consts::TAU).cos();
                (x, top + (bottom - top) * (1.0 - g))
            }).collect()
        };
        for (side, phase, color) in [(0_u8, 0.0, PINK), (1, 0.5, "#c084fc")] {
            let mut clip = draw::frame("tube window", lamp_x, 0.0, span, vh);
            clip.clips_content = true;
            let clip = b.add(v, clip);
            let roll = b.add(clip, draw::frame("tube", 0.0, 0.0, span + period, vh));
            // The tube: a hot core in a wide glow.
            b.add(roll, draw::emissive(ui::strokes("glow", &[curve(phase)], 5.0, draw::solid(&ui::alpha(color, 0.35))), draw::hex(color), 0.9, 10.0));
            b.add(roll, ui::strokes("core", &[curve(phase)], 1.6, draw::solid("#fff0f7")));
            motion::add(b, roll, Motion::new(Target::X).input(format!("fract(time * param({MOTION}/rate) * param({MOTION}/on))")).range(0.0, -period));
            motion::add(b, roll, Motion::new(Target::StretchY).input(format!("norm({MOTION}/depth)")).range(0.02, 1.0).pivot(0.5, top / vh).dynamics(Dynamics::spring(220.0, 26.0)));
            if side == 1 {
                motion::add(b, clip, Motion::new(Target::Opacity).input(format!("clamp(param({MOTION}/mode), 0, 1)")).range(0.0, 0.9));
            }
            // The ring: this side's volume now.
            let (ry, ph) = (vh / 2.0 + if side == 0 { -18.0 } else { 18.0 }, if side == 0 { 0.0 } else { 3.1416 });
            let ring = b.add(v, draw::frame("ring", lamp_x - 22.0, ry - 22.0, 44.0, 44.0));
            let mut c = draw::stroked(draw::circle("neon ring", 22.0, 22.0, 15.0, draw::solid("#00000000")), "#fff0f7", 2.2);
            c.fills.clear();
            b.add(ring, draw::emissive(c, draw::hex(color), 1.0, 14.0));
            let now = format!("(1 - norm({MOTION}/depth) * (0.5 - 0.5 * cos(6.2832 * time * param({MOTION}/rate) + {ph}))) * param({MOTION}/on)");
            motion::add(b, ring, Motion::new(Target::Opacity).input(now).range(0.08, 1.0));
            if side == 1 {
                motion::add(b, ring, Motion::new(Target::Opacity).input(format!("clamp(param({MOTION}/mode), 0, 1)")).range(0.0, 1.0));
            }
        }
        ui::silk_left(b, v, "Volume", lamp_x + 6.0, vh - 8.0, 5.5, t.dim, 800.0);
        column(b, p, t, MOTION, "Tremolo", "Volume · rate · depth", &[("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")]);
    });
    Face { file: "101-motion-tremolo", plugin: PLUGIN, scene, params: trem_params(), streams: Vec::new() }
}

// ── Vibrato ──────────────────────────────────────────────────────────────

/// A tuner's dark plum.
const VIBRATO: Theme = Theme {
    panel: [(0.0, "#1d1424"), (1.0, "#110b16")],
    edge: "#33223f",
    window: [(0.0, "#0d0911"), (1.0, "#07050a")],
    ink: "#f5e8ff",
    dim: "#9d86ad",
    accent: PINK,
    knob: "#241a2c",
    skirt: "#0c0810",
    bypass: "red",
};

/// The vibrato: pitch, as a tuner reads it — the needle swinging sharp and
/// flat about the note, as far as the depth and as fast as the rate — over
/// the wave it bends, its cycles crowding and spreading.
fn vibrato() -> Face {
    let t = &VIBRATO;
    let scene = panel("Vibrato", t, |b, p| {
        let (v, vw, vh) = view(b, p, t);
        let (cx, cy, r) = (vw / 2.0, vh + 34.0, 130.0);
        let arc: Vec<(f64, f64)> = (0..=48).map(|i| draw::polar(cx, cy, r, -40.0 + 80.0 * f64::from(i) / 48.0)).collect();
        b.add(v, ui::strokes("scale", &[arc], 1.0, draw::solid("#ffffff33")));
        for i in -4..=4 {
            let ang = 10.0 * f64::from(i);
            let (len, col) = if i == 0 { (14.0, PINK) } else { (7.0, "#ffffff4d") };
            b.add(v, ui::strokes("tick", &[vec![draw::polar(cx, cy, r, ang), draw::polar(cx, cy, r - len, ang)]], 1.2, draw::solid(col)));
        }
        ui::silk(b, v, "Flat", cx - r * 0.72, cy - r * 0.6, 6.0, t.dim, 800.0);
        ui::silk(b, v, "Sharp", cx + r * 0.72, cy - r * 0.6, 6.0, t.dim, 800.0);
        let side = 2.0 * (r + 6.0);
        let needle = b.add(v, draw::frame("needle", cx - side / 2.0, cy - side / 2.0, side, side));
        b.add(needle, ui::glowing(draw::rect("blade", side / 2.0 - 1.0, side / 2.0 - r + 2.0, 2.0, r - 48.0, draw::solid(PINK)), PINK, 0.8, 5.0));
        let swing = format!("0.5 + 0.5 * sin(6.2832 * time * param({MOTION}/rate)) * norm({MOTION}/depth) * param({MOTION}/on)");
        motion::add(b, needle, Motion::new(Target::Rotate).input(swing.clone()).range(-38.0, 38.0).pivot(0.5, 0.5));
        let wave: Vec<(f64, f64)> = (0..=160).map(|k| {
            let x = vw * f64::from(k) / 160.0;
            (x, vh - 22.0 - 9.0 * (x / 18.0 * std::f64::consts::TAU).sin())
        }).collect();
        let w = b.add(v, draw::frame("wave", 0.0, 0.0, vw, vh));
        b.add(w, ui::strokes("wave", &[wave], 1.4, draw::solid(&ui::alpha(t.ink, 0.7))));
        motion::add(b, w, Motion::new(Target::StretchX).input(swing).range(1.12, 0.88).pivot(0.0, 0.5));
        column(b, p, t, MOTION, "Vibrato", "Pitch · rate · depth", &[("rate", "Rate"), ("depth", "Depth"), ("mix", "Mix")]);
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
