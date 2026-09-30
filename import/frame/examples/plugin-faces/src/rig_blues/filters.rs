//! The input's filters as single units, each with what it is doing drawn
//! inside it: the wah (the rocker beside a screen of its resonant peak,
//! riding up the spectrum as the toe goes down) and the envelope filter in
//! two finishes — the black-box Q-Tron for the John Mayer rig and a
//! postal-service box for Cory Wong's — the filter's response on a screen
//! in the box, sweeping with the envelope.
//!
//! Both envelope filters take one block's params, `env/*` (mode, response,
//! range, peak, gain, boost, sweep, on), so either finish draws the same
//! filter.

use document_proto::motion::{Dynamics, Motion, Num, Target, Wave};
use document_proto::{CornerRadii, NodeId, ResizeX, TextAlign};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_proto::{Binding, Control, ControlKind, Range, SlotRole, Unit};
use surface_widgets::Builder;

use super::PLUGIN;
use super::drives::{H, enclosure, knob, led, on, stomp};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_studio_blues::parts::{self, set};

/// A screen of a resonant peak on a log-frequency grid, (`x`, `y`, `w`,
/// `h`) in `parent`: the peak rides across with `at` (an expression, 0 =
/// the left of its range, 1 = the right) and stands taller with `height`
/// (0…1). In `color` over `glass`. Returns the screen (it stretches with
/// its face).
#[allow(clippy::too_many_arguments)]
fn sweep_screen(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), at: &str, height: &str, color: &str, glass: (&str, &str), labels: &[&str]) -> NodeId {
    let mut s = draw::frame("Screen", x, y, w, h);
    s.fills = vec![draw::linear(180.0, &[(0.0, glass.0), (1.0, glass.1)])];
    s.clips_content = true;
    s.corner_radii = CornerRadii::uniform(6.0);
    s.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
    s.layout_child.resize_x = Some(ResizeX::Stretch);
    let screen = b.add(parent, s);
    // What slides in it is clipped by a plain frame of its own (the
    // screen's effects would lift the clip off it).
    let mut clip = draw::frame("glass", 0.0, 0.0, w, h);
    clip.clips_content = true;
    let s = b.add(screen, clip);
    // The grid: octaves across, levels up — stretching with the screen.
    let mut grid = draw::frame("grid", 0.0, 0.0, w, h);
    grid.layout_child.resize_x = Some(ResizeX::Scale);
    let grid = b.add(s, grid);
    let mut lines = Vec::new();
    for i in 1..8 {
        let gx = w * f64::from(i) / 8.0;
        lines.push(vec![(gx, 6.0), (gx, h - 16.0)]);
    }
    for i in 1..4 {
        let gy = 6.0 + (h - 22.0) * f64::from(i) / 4.0;
        lines.push(vec![(0.0, gy), (w, gy)]);
    }
    b.add(grid, ui::strokes("grid", &lines, 1.0, draw::solid("#ffffff12")));
    for (i, l) in labels.iter().enumerate() {
        let lx = w * (0.08 + 0.84 * i as f64 / (labels.len().max(2) - 1) as f64);
        b.add(grid, draw::text("freq", l, lx - 14.0, h - 10.0, 28.0, 7.0, &ui::alpha(color, 0.55), TextAlign::Center));
    }
    // The response: a band-pass bump drawn once, twice the screen wide,
    // slid under it; it scales with the screen.
    let mut ride = draw::frame("ride", 0.0, 0.0, w, h);
    ride.layout_child.resize_x = Some(ResizeX::Scale);
    let ride = b.add(s, ride);
    let (floor, top) = (h - 18.0, 14.0);
    let pts: Vec<(f64, f64)> = (0..=140)
        .map(|i| {
            let px = -w * 0.5 + 2.0 * w * f64::from(i) / 140.0;
            let d = (px - w * 0.5) / (w * 0.07);
            (px, floor - (floor - top) * (0.12 + 0.88 / (1.0 + d * d)))
        })
        .collect();
    let curve = b.add(ride, draw::frame("Response", 0.0, 0.0, w, h));
    let mut fill = pts.clone();
    fill.push((1.5 * w, floor));
    fill.push((-0.5 * w, floor));
    b.add(curve, draw::path("area", 0.0, 0.0, w, h, draw::polygon(&fill), draw::linear(180.0, &[(0.0, &ui::alpha(color, 0.4)), (1.0, &ui::alpha(color, 0.02))])));
    // A glow that stays in the screen (an emissive's bloom would not).
    b.add(curve, ui::glowing(ui::strokes("line", &[pts], 2.2, draw::solid(color)), color, 0.7, 5.0));
    motion::add(b, curve, Motion::new(Target::X).input(format!("({}) * ({at}) - {}", w * 0.84, w * 0.42)).dynamics(Dynamics::spring(400.0, 32.0)));
    motion::add(b, curve, Motion::new(Target::StretchY).input(format!("0.45 + 0.75 * ({height})")).pivot(0.5, floor / h).dynamics(Dynamics::spring(400.0, 32.0)));
    screen
}

/// The wah's rocker, (`x`, `y`, `w`, `h`) in `root`: the chrome halves and
/// their ribbed tread tipping about the axle with `ns/position`, the tread
/// itself the control, the toe switch engaging it.
fn rocker(b: &mut Builder<'_>, root: NodeId, (rx, ry, rw, rh): (f64, f64, f64, f64), ns: &str) {
    let pos = format!("norm({ns}/position)");
    let axle = ry + rh * 0.62;
    let half = |b: &mut Builder<'_>, name: &str, y0: f64, y1: f64, toe: bool| {
        let g = b.add(root, draw::frame(name, rx, y0, rw, y1 - y0));
        let (gh, gw) = (y1 - y0, rw);
        let mut chrome = draw::rect("chrome", 0.0, 0.0, gw, gh, draw::linear(90.0, &[(0.0, "#5e6166"), (0.18, "#e9ebed"), (0.5, "#a8abaf"), (0.82, "#f0f2f4"), (1.0, "#5e6166")]));
        chrome.corner_radii = if toe { CornerRadii { top_left: 14.0, top_right: 14.0, bottom_right: 0.0, bottom_left: 0.0 } } else { CornerRadii { top_left: 0.0, top_right: 0.0, bottom_right: 14.0, bottom_left: 14.0 } };
        b.add(g, chrome);
        let (tx, tw) = (9.0, gw - 18.0);
        let (ty, th) = if toe { (9.0, gh - 9.0) } else { (0.0, gh - 38.0) };
        b.add(g, draw::rounded(draw::rect("tread", tx, ty, tw, th, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#121315")])), 5.0));
        let ribs: Vec<Vec<(f64, f64)>> = (0..((th / 11.0) as usize)).map(|i| {
            let y = ty + 6.0 + i as f64 * 11.0;
            vec![(tx + 5.0, y), (tx + tw - 5.0, y)]
        }).collect();
        b.add(g, ui::strokes("ribs", &ribs, 2.2, draw::solid("#3a3b3f")));
        if !toe {
            set(b, g, "cry baby", gw / 2.0, gh - 20.0, 17.0, "#101012", &Lettering::family(family::PACIFICO), 0.0);
        }
        let shade = b.add(g, draw::with_opacity(draw::rect("shade", 0.0, 0.0, gw, gh, draw::solid("#000000")), 0.0));
        let tip = if toe { pos.clone() } else { format!("1 - {pos}") };
        motion::add(b, shade, Motion::new(Target::Opacity).input(tip.clone()).range(0.0, 0.45).dynamics(Dynamics::spring(500.0, 40.0)));
        let py = if toe { 1.0 } else { 0.0 };
        motion::add(b, g, Motion::new(Target::StretchY).input(tip.clone()).range(1.06, 0.86).pivot(0.5, py).dynamics(Dynamics::spring(500.0, 40.0)));
        motion::add(b, g, Motion::new(Target::StretchX).input(tip).range(1.04, 0.96).pivot(0.5, py).dynamics(Dynamics::spring(500.0, 40.0)));
    };
    half(b, "Toe", ry, axle, true);
    half(b, "Heel", axle, ry + rh, false);
    b.add(root, draw::rect("axle", rx, axle - 1.0, rw, 2.0, draw::solid("#00000080")));
    let lane = b.add(root, draw::frame("lane", rx, ry + 10.0, rw, rh - 20.0));
    let band = b.add(lane, draw::frame("tilt", 0.0, 0.0, rw, 70.0));
    b.add(band, draw::rect("sheen", 0.0, 0.0, rw, 70.0, draw::linear(180.0, &[(0.0, "#ffffff00"), (0.5, "#ffffff22"), (1.0, "#ffffff00")])));
    let mut c = Control::new(ControlKind::Fader { orientation: surface_proto::Orientation::Vertical }).slot(SlotRole::Thumb, band.0);
    c.binding = Some(ui::bind(&format!("{ns}/position")));
    b.controls.insert(lane, c);
    let toe = b.add(root, draw::frame("Toe switch", rx + 20.0, ry, rw - 40.0, 16.0));
    b.controls.insert(toe, Control::new(ControlKind::footswitch()).bound(Binding::param(format!("{ns}/on"))));
}

/// The wah as one unit: its casting holds the rocker and, beside it, a
/// screen of its resonant peak — low with the heel down, high with the
/// toe — so you see where the pedal is voicing as you rock it.
#[must_use]
pub fn wah_unit() -> Face {
    const W: f64 = 330.0;
    const NS: &str = "wah";
    let scene = canvas("Wah", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut cast = draw::frame("Casting", 0.0, 0.0, W, H);
        cast.fills = vec![draw::linear(90.0, &[(0.0, "#0c0c0d"), (0.3, "#202124"), (0.7, "#1a1a1d"), (1.0, "#0a0a0b")])];
        cast.corner_radii = CornerRadii::uniform(16.0);
        let cast = b.add(root, draw::with_effects(cast, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]));
        let hammer: Vec<(f64, f64)> = (0..120).map(|i| {
            let f = f64::from(i);
            ((f * 37.0) % W, (f * 59.0) % H)
        }).collect();
        for (x, y) in hammer {
            b.add(cast, draw::circle("crinkle", x, y, 1.6, draw::solid("#ffffff06")));
        }
        led(b, cast, NS, 76.0, 12.0, 5.0, "#e0281c");
        rocker(b, cast, (14.0, 24.0, 124.0, H - 36.0), NS);
        // The screen: where the wah is voicing.
        let (sx, sy, sw, sh) = (152.0, 34.0, W - 166.0, H - 92.0);
        parts::print(b, cast, "SWEEP", sx + 22.0, 20.0, 7.5, "#9a9ca3", 800.0, 0.14);
        sweep_screen(b, cast, (sx, sy, sw, sh), &format!("norm({NS}/position)"), &format!("0.65 * param({NS}/on) + 0.2"), "#ff5a3c", ("#140806", "#0a0403"), &["350", "700", "1.4k", "2.2k"]);
        parts::print(b, cast, "HEEL", sx + 16.0, sy + sh + 14.0, 7.0, "#7a7c83", 800.0, 0.12);
        parts::print(b, cast, "TOE", sx + sw - 14.0, sy + sh + 14.0, 7.0, "#7a7c83", 800.0, 0.12);
        // Where it sits now, as a number of cycles.
        let read = crate::delay::parts::readout(b, cast, "Position", ui::bind(&format!("{NS}/position")), (sx, sy + sh + 34.0, sw), 9.0, "#ffb199", TextAlign::Center);
        if let Some(n) = b.scene.node_mut(read) {
            n.layout_child.resize_x = Some(ResizeX::Stretch);
        }
    });
    let params = vec![
        ui::param(&format!("{NS}/position"), "Position", Range::linear(0.0, 1.0, Unit::None), 0.5).featured(),
        on(NS),
    ];
    Face { file: "30-wah-unit", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The envelope filter's params, shared by its finishes.
fn env_params() -> Vec<surface_proto::ParamInfo> {
    const NS: &str = "env";
    vec![
        ui::choice(&format!("{NS}/mode"), "Mode", &["LP", "BP", "HP", "Mix"], 1),
        ui::choice(&format!("{NS}/response"), "Response", &["Fast", "Slow"], 0),
        ui::choice(&format!("{NS}/range"), "Range", &["Hi", "Lo"], 1),
        ui::param(&format!("{NS}/peak"), "Peak", Range::linear(0.0, 1.0, Unit::None), 0.6).featured(),
        ui::param(&format!("{NS}/gain"), "Gain", Range::linear(0.0, 1.0, Unit::None), 0.5).featured(),
        ui::choice(&format!("{NS}/boost"), "Boost", &["Off", "On"], 0),
        ui::choice(&format!("{NS}/sweep"), "Sweep", &["Down", "Up"], 1),
        on(NS),
    ]
}

/// Where the envelope puts the peak: the input's level (gained), or with
/// no input a slow idle sweep while it is on; higher with RANGE up.
fn envelope() -> String {
    "clamp(0.15 * (1 - norm(env/range)) + or(level(env/in) * (0.3 + norm(env/gain)), 0.5 + 0.35 * motion(idle)), 0, 1)".to_string()
}

/// The idle sweep's LFO, on the screen's curve (runs only while on).
fn idle(b: &mut Builder<'_>, screen: NodeId) {
    motion::add(b, screen, Motion::new(Target::None).named("idle").input("param(env/on)").dynamics(Dynamics::Lfo { rate: Num::Value(0.35), shape: Wave::Sine, phase: 0.0 }));
}

/// The John Mayer rig's envelope filter: the Q-Tron's black box, six knobs
/// across the top, the magenta Q and orange TRON+, and in the box a screen
/// of the filter sweeping with the envelope.
#[must_use]
pub fn q_tron_unit() -> Face {
    const W: f64 = 600.0;
    const NS: &str = "env";
    const INK: &str = "#f4f4f2";
    let scene = canvas("Q-Tron+", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#111113", "satin", &["jack_inset=10"]);
        parts::print(b, lid, "24V", lw * 0.5, 9.0, 6.5, INK, 700.0, 0.06);
        let row = lh * 0.18;
        for (i, (id, name)) in [("mode", "MODE"), ("response", "RESPONSE"), ("range", "RANGE"), ("peak", "PEAK"), ("gain", "GAIN"), ("boost", "BOOST")].iter().enumerate() {
            let cx = lw * (0.07 + 0.172 * i as f64);
            knob(b, lid, "flat", &["color=#1a1a1c", "pointer=#f4f4f2", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#0a0a0b"], &format!("{NS}/{id}"), name, 40.0, cx, row);
            parts::print(b, lid, name, cx, row + 29.0, 7.5, INK, 800.0, 0.04);
        }
        // The identity, left: the name, the maker's script, the switches.
        let bubble = Lettering::family(family::KNEWAVE);
        set(b, lid, "Q", lw * 0.1, lh * 0.64, 96.0, "#d6247f", &bubble, 0.0);
        set(b, lid, "TRON", lw * 0.29, lh * 0.6, 50.0, "#f7811e", &bubble, 0.04);
        set(b, lid, "+", lw * 0.42, lh * 0.72, 32.0, INK, &Lettering::family(family::INTER).weight(900.0), 0.0);
        set(b, lid, "electro-harmonix", lw * 0.25, lh * 0.44, 13.0, INK, &Lettering::family(family::PLAYFAIR_DISPLAY).weight(500.0), 0.0);
        parts::print(b, lid, "SWEEP", lw * 0.2, lh * 0.83, 7.0, INK, 800.0, 0.06);
        parts::toggle(b, lid, &format!("{NS}/sweep"), "Sweep", 10.0, lw * 0.27, lh * 0.83);
        led(b, lid, NS, lw * 0.36, lh * 0.83, 5.0, "#e0281c");
        stomp(b, lid, NS, 28.0, lw * 0.43, lh * 0.87);
        // The screen, right: the filter at work.
        let screen = sweep_screen(b, lid, (lw * 0.49, lh * 0.36, lw * 0.47, lh * 0.56), &envelope(), "norm(env/peak)", "#ff4fb0", ("#12060d", "#070205"), &["100", "300", "1k", "3k"]);
        idle(b, screen);
        parts::print(b, lid, "FILTER", lw * 0.52, lh * 0.33, 7.0, "#ff8fcf", 800.0, 0.14);
        crate::rig_time::kit::responsive(b, lid, &[screen]);
    });
    Face { file: "31-filter-q-tron", plugin: PLUGIN, scene, params: env_params(), streams: Vec::new() }
}

/// Cory Wong's envelope filter, in a postal-service finish: a navy box
/// edged in airmail stripes, POSTAL SERVICE stencilled on it, its knobs on
/// a cream label — and the filter's screen framed as a stamp, perforated,
/// a cancellation mark rolling over its corner.
#[must_use]
pub fn postal_unit() -> Face {
    const W: f64 = 600.0;
    const NS: &str = "env";
    const CREAM: &str = "#f3ead3";
    const NAVY: &str = "#1d3563";
    const RED: &str = "#c8302b";
    let scene = canvas("Postal", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, NAVY, "satin", &["jack_inset=10"]);
        // Airmail stripes round the lid's edge.
        let stripe = |b: &mut Builder<'_>, name: &str, (x, y, w, h): (f64, f64, f64, f64)| {
            let mut f = draw::frame(name, x, y, w, h);
            f.clips_content = true;
            let f = b.add(lid, f);
            let n = ((w + h) / 14.0) as i32;
            for k in 0..n {
                let c = if k % 2 == 0 { RED } else { CREAM };
                let x0 = f64::from(k) * 14.0 - h;
                b.add(f, draw::path("stripe", 0.0, 0.0, w, h, draw::polygon(&[(x0, h), (x0 + 7.0, h), (x0 + 7.0 + h, 0.0), (x0 + h, 0.0)]), draw::solid(c)));
            }
            f
        };
        stripe(b, "stripes top", (0.0, 0.0, lw, 7.0));
        stripe(b, "stripes bottom", (0.0, lh - 7.0, lw, 7.0));
        // The label: cream, the knobs on it.
        let label = draw::rounded(draw::rect("label", lw * 0.03, lh * 0.08, lw * 0.44, lh * 0.5, draw::linear(180.0, &[(0.0, CREAM), (1.0, "#e6dbbf")])), 4.0);
        b.add(lid, draw::with_effects(label, vec![draw::drop_shadow("#00000060", 0.0, 1.5, 3.0)]));
        for (i, (id, name)) in [("gain", "SENS"), ("peak", "Q"), ("range", "RANGE"), ("mode", "MODE"), ("response", "DECAY"), ("boost", "LEVEL")].iter().enumerate() {
            let (col, r) = (i % 3, i / 3);
            let cx = lw * (0.09 + 0.15 * col as f64);
            let cy = lh * (0.19 + 0.21 * r as f64);
            knob(b, lid, "flat", &["color=#1d3563", "pointer=#f3ead3", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#0f1d38"], &format!("{NS}/{id}"), name, 34.0, cx, cy);
            parts::print(b, lid, name, cx, cy + 25.0, 7.0, NAVY, 800.0, 0.08);
        }
        // The name, stencilled.
        let stencil = Lettering::family(family::OSWALD).weight(700.0);
        set(b, lid, "POSTAL SERVICE", lw * 0.25, lh * 0.7, 25.0, CREAM, &stencil, 0.12);
        parts::print(b, lid, "ENVELOPE FILTER · FIRST CLASS", lw * 0.25, lh * 0.79, 7.0, "#9fb3d9", 800.0, 0.12);
        led(b, lid, NS, lw * 0.09, lh * 0.88, 5.0, "#ffcc33");
        stomp(b, lid, NS, 28.0, lw * 0.25, lh * 0.88);
        // The stamp: a perforated white border round the screen.
        let (sx, sy, sw, sh) = (lw * 0.51, lh * 0.12, lw * 0.45, lh * 0.74);
        let mut stamp = draw::frame("Stamp", sx - 8.0, sy - 8.0, sw + 16.0, sh + 16.0);
        stamp.fills = vec![draw::solid(CREAM)];
        stamp.layout_child.resize_x = Some(ResizeX::Stretch);
        let stamp_id = b.add(lid, draw::with_effects(stamp, vec![draw::drop_shadow("#00000070", 0.0, 2.0, 4.0)]));
        // Perforations along its top and bottom (the scene's edge colour).
        let mut perf = draw::frame("perforations", 0.0, 0.0, sw + 16.0, sh + 16.0);
        perf.layout_child.resize_x = Some(ResizeX::Scale);
        let perf = b.add(stamp_id, perf);
        let n = ((sw + 16.0) / 9.0) as i32;
        for k in 0..=n {
            let x = f64::from(k) * 9.0;
            for y in [0.0, sh + 16.0] {
                b.add(perf, draw::circle("hole", x, y, 3.0, draw::solid(NAVY)));
            }
        }
        let screen = sweep_screen(b, lid, (sx, sy, sw, sh), &envelope(), "norm(env/peak)", "#ffcc33", ("#0f1d38", "#070e1c"), &["100", "300", "1k", "3k"]);
        idle(b, screen);
        // The cancellation mark: wavy lines over the stamp's corner.
        let waves: Vec<Vec<(f64, f64)>> = (0..4).map(|k| {
            (0..=30).map(|i| {
                let x = f64::from(i) * 3.0;
                (x, 8.0 + f64::from(k) * 6.0 + 2.2 * (x / 8.0).sin())
            }).collect()
        }).collect();
        let mut cancel = draw::frame("cancel", sx + sw - 70.0, sy - 14.0, 92.0, 34.0);
        cancel.layout_child.resize_x = Some(ResizeX::Right);
        let cancel = b.add(lid, cancel);
        b.add(cancel, ui::strokes("waves", &waves, 1.4, draw::solid("#1d3563aa")));
        crate::rig_time::kit::responsive(b, lid, &[screen, stamp_id]);
    });
    Face { file: "34-filter-postal", plugin: PLUGIN, scene, params: env_params(), streams: Vec::new() }
}

/// The Aqua-Puss as a unit: the teal box's knobs, name and stomp on the
/// left, and set into the box a screen of its repeats — the note, then
/// each echo a delay time on, lower by the feedback, softer and wider as
/// the analog line dulls them.
#[must_use]
pub fn aqua_puss_unit() -> Face {
    const W: f64 = 560.0;
    const NS: &str = "aquapuss";
    const INK: &str = "#0b2a33";
    let scene = canvas("Aqua-Puss", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#3fb4c8", "gloss", &["jack_inset=8"]);
        let left = lw * 0.36;
        led(b, lid, NS, left * 0.5, 10.0, 5.0, "#e0281c");
        for (i, (id, name)) in [("time", "DELAY"), ("feedback", "FEEDBACK"), ("level", "BLEND")].iter().enumerate() {
            let cx = left * (0.2 + 0.3 * i as f64);
            let cy = lh * 0.22;
            knob(b, lid, "flat", &["color=#f2f1ec", "pointer=#0b2a33", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#d6d4cc"], &format!("{NS}/{id}"), name, 44.0, cx, cy);
            parts::print(b, lid, name, cx, cy + 32.0, 7.5, INK, 800.0, 0.06);
        }
        set(b, lid, "Aqua-Puss", left * 0.5, lh * 0.5, 30.0, "#f4fbfd", &Lettering::family(family::KNEWAVE), 0.0);
        parts::print(b, lid, "ANALOG DELAY", left * 0.5, lh * 0.62, 7.5, INK, 800.0, 0.2);
        stomp(b, lid, NS, 32.0, left * 0.5, lh * 0.77);
        parts::print(b, lid, "WAY HUGE", left * 0.5, lh * 0.92, 9.0, INK, 900.0, 0.2);

        // The screen: the repeats.
        let screen = repeats_screen(b, lid, (left + 6.0, lh * 0.08, lw - left - 18.0, lh * 0.84), NS);
        crate::rig_time::kit::responsive(b, lid, &[screen]);
    });
    let params = vec![
        ui::param(&format!("{NS}/time"), "Delay", Range::log(20.0, 600.0, Unit::Milliseconds), 120.0).featured(),
        ui::param(&format!("{NS}/feedback"), "Feedback", Range::linear(0.0, 0.95, Unit::None), 0.15).featured(),
        ui::param(&format!("{NS}/level"), "Blend", Range::linear(-60.0, 12.0, Unit::Db), -16.0).featured(),
        on(NS),
    ];
    Face { file: "17-pedal-aqua-puss", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// A screen of a delay's repeats, (`sx`, `sy`, `sw`, `sh`) in `lid`: the
/// note, then each echo a delay time on (`ns/time`, over one second),
/// lower by the feedback, softer and wider as the analog line dulls them.
/// The screen stretches with its face; what is in it scales.
pub(super) fn repeats_screen(b: &mut Builder<'_>, lid: NodeId, (sx, sy, sw, sh): (f64, f64, f64, f64), ns: &str) -> NodeId {
    const GLOW: &str = "#8ff3ff";
    let mut s = draw::frame("Screen", sx, sy, sw, sh);
    s.fills = vec![draw::linear(180.0, &[(0.0, "#062a33"), (1.0, "#03161b")])];
    s.corner_radii = CornerRadii::uniform(6.0);
    s.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
    s.layout_child.resize_x = Some(ResizeX::Stretch);
    let screen = b.add(lid, s);
    let mut glass = draw::frame("glass", 0.0, 0.0, sw, sh);
    glass.clips_content = true;
    let glass = b.add(screen, glass);
    let mut ride = draw::frame("ride", 0.0, 0.0, sw, sh);
    ride.layout_child.resize_x = Some(ResizeX::Scale);
    let ride = b.add(glass, ride);
    let base = sh - 22.0;
    let window = 1000.0;
    let (lead, span) = (18.0, sw - 34.0);
    let grid: Vec<Vec<(f64, f64)>> = (0..=10).map(|i| {
        let x = lead + span * f64::from(i) / 10.0;
        vec![(x, 10.0), (x, base)]
    }).collect();
    b.add(ride, ui::strokes("grid", &grid, 1.0, draw::solid("#8ff3ff14")));
    b.add(ride, ui::strokes("baseline", &[vec![(0.0, base), (sw, base)]], 1.0, draw::solid("#8ff3ff55")));
    for (i, l) in ["0", "250", "500", "750", "1 s"].iter().enumerate() {
        b.add(ride, draw::text("t", l, lead + span * i as f64 / 4.0 - 14.0, base + 11.0, 28.0, 7.0, "#8ff3ff88", TextAlign::Center));
    }
    // The note.
    b.add(ride, ui::glowing(draw::rect("note", lead - 1.5, 12.0, 3.0, base - 12.0, draw::solid("#f4fbfd")), GLOW, 0.8, 5.0));
    // The repeats.
    let spring = Dynamics::spring(260.0, 28.0);
    for i in 1..=10_u32 {
        let fi = f64::from(i);
        let wd = 6.0 + 2.5 * fi;
        let g = b.add(ride, draw::frame(&format!("repeat {i}"), lead - wd / 2.0, 12.0, wd, base - 12.0));
        b.add(g, ui::glowing(draw::rounded(draw::rect("echo", 0.0, 0.0, wd, base - 12.0, draw::linear(180.0, &[(0.0, GLOW), (1.0, "#2bb6cc")])), wd / 2.0), GLOW, 0.6, 4.0));
        let at = format!("clamp({i} * param({ns}/time) / {window}, 0, 1.2)");
        motion::add(b, g, Motion::new(Target::X).input(at).range(0.0, span).dynamics(spring.clone()));
        let fall = format!("pow(param({ns}/feedback), {}) * (0.3 + 0.7 * param({ns}/on))", i - 1);
        motion::add(b, g, Motion::new(Target::StretchY).input(fall).range(0.01, 1.0).pivot(0.5, 1.0).dynamics(spring.clone()));
    }
    parts::print(b, screen, "REPEATS", 52.0, 12.0, 6.5, "#8ff3ffaa", 800.0, 0.14);
    screen
}
