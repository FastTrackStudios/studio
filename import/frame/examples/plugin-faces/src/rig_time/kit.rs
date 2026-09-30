//! What every time face shares: its size, its params (signal's Delay and
//! Reverb block params, in their own units), and the pieces most of them
//! draw — a live echo train, a live decay, the machine selector, bypass.
//!
//! The pictures move with the params through motion expressions: an echo
//! train's repeats sit at multiples of `dly/time` and fall by
//! `dly/feedback` each time; a decay stretches with `verb/decay`. Faces
//! are free to draw their own instead.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw, motion};
use surface_proto::{ParamInfo, Range, Unit};
use surface_widgets::Builder;

use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial};
use crate::rig_faces::PLUGIN;

/// A lane of the Time module: every time face's size.
pub const W: f64 = 960.0;
pub const H: f64 = 164.0;

/// The corner signal keeps for its machine and division pickers (top
/// right, `CORNER.0` wide × `CORNER.1` tall): faces draw nothing there that
/// matters.
pub const CORNER: (f64, f64) = (170.0, 26.0);

/// The delay faces' namespace (signal's Delay block).
pub const DLY: &str = "dly";
/// The reverb faces' namespace (signal's Reverb block).
pub const VERB: &str = "verb";

// The section's colours, as Signal draws the Time module: every delay is
// blue and every reverb purple — a face's own finish can be anything, but
// its accents (lamps, displays, pointers, the bypass) are these, so a
// glance tells which section you're reading.
/// Delay blue (Signal's `DELAY_COLORS[0]`), a lighter tint, a deep shade,
/// and a lamp's glow.
pub const BLUE: &str = "#3b82f6";
pub const BLUE_LIGHT: &str = "#bcd6ff";
pub const BLUE_DEEP: &str = "#1e3a8a";
pub const BLUE_LAMP: &str = "#4aa0f4";
/// Reverb purple (Signal's `VERB_COLORS[0]`), likewise.
pub const PURPLE: &str = "#a78bfa";
pub const PURPLE_LIGHT: &str = "#ddd0ff";
pub const PURPLE_DEEP: &str = "#3b1d6e";
pub const PURPLE_LAMP: &str = "#b58cff";

/// The delay's machines, in the block's `style` order.
pub const DELAY_ALGOS: [&str; 13] = [
    "Tape", "Digital", "Analog", "Lo-Fi", "Shimmer", "Reverse", "Ice", "Rhythm", "Drum", "Oil Can", "MultiTap", "Spectral", "Filter",
];
/// The reverb's algorithms, in the block's `algorithm` order.
pub const VERB_ALGOS: [&str; 15] = [
    "Room", "Hall", "Plate", "Spring", "Cloud", "Bloom", "Shimmer", "Chorale", "Magneto", "NonLinear", "Swell", "Reflections", "Velvet", "FreeVerb", "Convolution",
];
/// A delay's tempo divisions, in the block's `tap_div_l` order.
pub const DIVS: [&str; 11] = ["1/4", "1/8.", "1/8", "1/8T", "1/16", "Golden", "Silver", "Free", "1/4.", "1/2", "1/4T"];

fn a(ns: &str, id: &str) -> String {
    format!("{ns}/{id}")
}

/// The Delay block's params, as `dly/…`.
#[must_use]
pub fn delay_params() -> Vec<ParamInfo> {
    let n = DLY;
    vec![
        ui::param(&a(n, "time"), "Time", Range::log(20.0, 2500.0, Unit::Milliseconds), 350.0),
        ui::param(&a(n, "feedback"), "Feedback", Range::linear(0.0, 0.95, Unit::None), 0.3),
        ui::param(&a(n, "level"), "Level", Range::linear(-60.0, 12.0, Unit::Db), -16.0),
        ui::choice(&a(n, "style"), "Machine", &DELAY_ALGOS, 1),
        ui::choice(&a(n, "tap_div_l"), "Division", &DIVS, 0),
        ui::param(&a(n, "high_pass"), "Low Cut", Range::linear(0.0, 900.0, Unit::Hz), 0.0),
        ui::param(&a(n, "high_cut"), "High Cut", Range::log(500.0, 20_000.0, Unit::Hz), 8000.0),
        ui::param(&a(n, "duck_sens"), "Duck", Range::linear(0.0, 18.0, Unit::Db), 0.0),
        ui::param(&a(n, "mod_rate"), "Mod Rate", Range::log(0.05, 8.0, Unit::Hz), 0.6),
        ui::param(&a(n, "mod_depth"), "Mod Depth", Range::linear(0.0, 1.0, Unit::None), 0.0),
        ui::param(&a(n, "pan"), "Pan", Range::linear(-1.0, 1.0, Unit::None), 0.0),
        ui::param(&a(n, "dry"), "Dry", Range::linear(0.0, 1.0, Unit::None), 1.0),
        ui::param(&a(n, "repeat_dyn"), "Repeat Dynamics", Range::linear(0.0, 1.0, Unit::None), 0.0),
        ui::param(&a(n, "interval"), "Interval", Range::linear(0.0, 30.0, Unit::None), 30.0),
        ui::param(&a(n, "blend"), "Blend", Range::linear(0.0, 1.0, Unit::None), 1.0),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

/// The Reverb block's params, as `verb/…`.
#[must_use]
pub fn reverb_params() -> Vec<ParamInfo> {
    let n = VERB;
    vec![
        ui::param(&a(n, "decay"), "Decay", Range::linear(0.0, 1.0, Unit::None), 0.4),
        ui::param(&a(n, "size"), "Size", Range::linear(0.0, 1.0, Unit::None), 0.5),
        ui::param(&a(n, "level"), "Level", Range::linear(-60.0, 12.0, Unit::Db), -16.0),
        ui::choice(&a(n, "algorithm"), "Algorithm", &VERB_ALGOS, 1),
        ui::param(&a(n, "predelay"), "Pre-Delay", Range::linear(0.0, 200.0, Unit::Milliseconds), 0.0),
        ui::param(&a(n, "damping"), "Damping", Range::linear(0.0, 1.0, Unit::None), 0.3),
        ui::param(&a(n, "tone"), "Tone", Range::linear(-1.0, 1.0, Unit::None), 0.0),
        ui::param(&a(n, "modulation"), "Modulation", Range::linear(0.0, 1.0, Unit::None), 0.2),
        ui::param(&a(n, "low_cut"), "Low Cut", Range::log(20.0, 2000.0, Unit::Hz), 20.0),
        ui::param(&a(n, "high_cut"), "High Cut", Range::log(1000.0, 20_000.0, Unit::Hz), 20_000.0),
        ui::param(&a(n, "duck"), "Duck", Range::linear(0.0, 1.0, Unit::None), 0.0),
        ui::param(&a(n, "pan_a"), "Pan", Range::linear(-1.0, 1.0, Unit::None), 0.0),
        ui::param(&a(n, "dry"), "Dry", Range::linear(0.0, 1.0, Unit::None), 1.0),
        ui::param(&a(n, "shim_shift1"), "Shift", Range::linear(-12.0, 12.0, Unit::None), 12.0),
        ui::param(&a(n, "shim_amount"), "Shimmer", Range::linear(0.0, 1.0, Unit::None), 0.35),
        ui::toggle(&a(n, "on"), "Bypass", true),
    ]
}

/// A delay face: its scene under `file`, with the delay's params.
#[must_use]
pub fn delay_face(file: &'static str, scene: document_proto::Scene) -> Face {
    Face { file, plugin: PLUGIN, scene, params: delay_params(), streams: Vec::new() }
}

/// A reverb face: its scene under `file`, with the reverb's params.
#[must_use]
pub fn reverb_face(file: &'static str, scene: document_proto::Scene) -> Face {
    Face { file, plugin: PLUGIN, scene, params: reverb_params(), streams: Vec::new() }
}

/// A lane's canvas: `build` draws into the root, already `W` × `H`.
pub fn lane(name: &str, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> document_proto::Scene {
    canvas(name, |b, root| {
        ui::fit_root(b, root, W, H);
        build(b, root);
        ui::flush(b, root, W, H);
    })
}

/// The machine selector: a rotary switch over the block's `style` (delay)
/// or `algorithm` (reverb) — turning it swaps the face — in `ink`.
pub fn selector(b: &mut Builder<'_>, parent: NodeId, ns: &str, cx: f64, cy: f64, d: f64, variant: &str, ink: &str, dim: &str) -> NodeId {
    let (addr, names): (String, &[&str]) = if ns == DLY { (a(ns, "style"), &DELAY_ALGOS) } else { (a(ns, "algorithm"), &VERB_ALGOS) };
    let s = Dial::new("switch.rotary", variant, d, ink, dim).options(names).sweep(300.0).legend(0.0);
    ui::dial(b, parent, &addr, "Machine", cx, cy, &s)
}

/// An illuminated bypass key over `ns/on`, lit while the block plays.
pub fn bypass(b: &mut Builder<'_>, parent: NodeId, ns: &str, cx: f64, cy: f64, d: f64, variant: &str, legend: &str) -> NodeId {
    let p = Place::new("Bypass").size(d).variant(variant).legend(legend).bind(ui::bind(&a(ns, "on")));
    ui::put(b, parent, "button.illuminated", &p, cx, cy)
}

/// A knob on `addr`, printed in the face's ink.
#[allow(clippy::too_many_arguments)]
pub fn knob(b: &mut Builder<'_>, parent: NodeId, style: &str, variant: &str, addr: &str, legend: &str, d: f64, cx: f64, cy: f64, ink: &str, dim: &str) -> NodeId {
    let s = Dial::new(style, variant, d, ink, dim).legend((d * 0.2).clamp(7.0, 10.0));
    ui::dial(b, parent, addr, legend, cx, cy, &s)
}

/// A live echo train in (`x`, `y`, `w`, `h`): `taps` repeats, each at a
/// multiple of `dly/time` across a window of `window_ms`, each lower than
/// the last by `dly/feedback`, drawn as glowing bars in `color`.
pub fn echo_train(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), taps: u32, window_ms: f64, color: &str) -> NodeId {
    let mut f = draw::frame("Echoes", x, y, w, h);
    f.clips_content = true;
    let f = b.add(parent, f);
    // The dry hit at zero, then the repeats.
    ui::line(b, f, "dry", &[(2.0, h), (2.0, 0.0)], 2.0, &ui::alpha(color, 0.5));
    for i in 1..=taps {
        let bar = draw::rect(&format!("tap {i}"), 0.0, 0.0, 3.0, h, draw::solid(color));
        let bar = b.add(f, ui::glowing(bar, color, 0.8, 4.0));
        let at = format!("clamp({i} * param({DLY}/time) / {window_ms}, 0, 1)");
        motion::add(b, bar, Motion::new(Target::X).input(at).range(2.0, w - 3.0).dynamics(Dynamics::spring(300.0, 30.0)));
        let fall = format!("pow(param({DLY}/feedback), {})", i - 1);
        motion::add(b, bar, Motion::new(Target::StretchY).input(fall).range(0.02, 1.0).pivot(0.5, 1.0).dynamics(Dynamics::spring(300.0, 30.0)));
    }
    f
}

/// A live decay envelope in (`x`, `y`, `w`, `h`): an exponential tail in
/// `color`, stretched across the box as `verb/decay` rises.
pub fn decay(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), color: &str) -> NodeId {
    let mut f = draw::frame("Decay", x, y, w, h);
    f.clips_content = true;
    let f = b.add(parent, f);
    let g = b.add(f, draw::frame("tail", 0.0, 0.0, w, h));
    let pts: Vec<(f64, f64)> = (0..=64)
        .map(|i| {
            let t = f64::from(i) / 64.0;
            (t * w, h - h * (-5.0 * t).exp())
        })
        .collect();
    let mut d = format!("M 0 {h:.1} ");
    for (px, py) in &pts {
        d += &format!("L {px:.1} {py:.1} ");
    }
    d += &format!("L {w:.1} {h:.1} Z");
    let fill = draw::path("envelope", 0.0, 0.0, w, h, d, draw::linear(180.0, &[(0.0, &ui::alpha(color, 0.55)), (1.0, &ui::alpha(color, 0.05))]));
    b.add(g, fill);
    ui::line(b, g, "edge", &pts, 1.6, color);
    motion::add(b, g, Motion::new(Target::StretchX).input(format!("norm({VERB}/decay)")).range(0.15, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(300.0, 30.0)));
    f
}

/// Printed legend, centred.
pub fn legend(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64, color: &str) -> NodeId {
    ui::silk(b, parent, s, cx, cy, size, color, 800.0)
}

/// A placeholder unit until the machine's own face is drawn: its name, the
/// selector, the main knobs and the picture.
#[must_use]
pub fn stub(ns: &str, name: &str, color: &str) -> document_proto::Scene {
    let ns = ns.to_string();
    let name = name.to_string();
    // The section's colour, whatever the placeholder asked for.
    let _ = color;
    let color = if ns == DLY { BLUE } else { PURPLE }.to_string();
    lane(&name.clone(), move |b, root| {
        const INK: &str = "#e8e6e1";
        const DIM: &str = "#8a8d94";
        let mut panel = draw::frame("Panel", 0.0, 0.0, W, H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, "#1d1f24"), (1.0, "#121317")])];
        panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
        let panel = b.add(root, panel);
        b.add(panel, ui::text("name", &name, 22.0, 30.0, 200.0, 20.0, &color, TextAlign::Left, 800.0, true));
        bypass(b, panel, &ns, 60.0, 100.0, 34.0, "green", "On");
        if ns == DLY {
            echo_train(b, panel, (220.0, 24.0, 380.0, 116.0), 8, 2000.0, &color);
            for (i, (id, l)) in [("time", "Time"), ("feedback", "Feedback"), ("level", "Level")].iter().enumerate() {
                knob(b, panel, "knob.3d", "flat", &a(&ns, id), l, 46.0, 670.0 + 100.0 * i as f64, 70.0, INK, DIM);
            }
        } else {
            decay(b, panel, (220.0, 24.0, 380.0, 116.0), &color);
            for (i, (id, l)) in [("decay", "Decay"), ("size", "Size"), ("level", "Level")].iter().enumerate() {
                knob(b, panel, "knob.3d", "flat", &a(&ns, id), l, 46.0, 670.0 + 100.0 * i as f64, 70.0, INK, DIM);
            }
        }
    })
}

/// A rendered knob (`knob.3d`) on `addr` with the kit's `options` (colour,
/// pointer, skirt…), `d` across at (`cx`, `cy`), its legend under it and
/// its live value between, in the face's inks.
#[allow(clippy::too_many_arguments)]
pub fn knob3d(b: &mut Builder<'_>, parent: NodeId, addr: &str, legend_text: &str, d: f64, (cx, cy): (f64, f64), options: &[&str], ink: &str, dim: &str) -> NodeId {
    let mut opts = vec!["touch=44", "ticks=none"];
    opts.extend_from_slice(options);
    let p = Place::new(legend_text).legend(" ").size(d).variant("flat").options(&opts).bind(ui::bind(addr));
    let k = ui::put(b, parent, "knob.3d", &p, cx, cy);
    ui::strip_legend(b, k);
    ui::add_value(b, k, (d * 0.16).clamp(7.0, 9.0), dim);
    let size = (d * 0.19).clamp(7.0, 10.0);
    ui::silk(b, parent, legend_text, cx, cy + d / 2.0 + 20.0, size, ink, 800.0);
    k
}

/// A seven-segment readout of `addr` (to `decimals`, no unit), lit in
/// `color` over its unlit segments ("888…", `digits` wide), right-aligned
/// in the box (`x`, `cy`, `w`) at `size`.
#[allow(clippy::too_many_arguments)]
pub fn seg_readout(b: &mut Builder<'_>, parent: NodeId, addr: &str, (x, cy, w): (f64, f64, f64), size: f64, digits: usize, decimals: u8, color: &str) -> NodeId {
    use document_proto::{NodeKind, TextData};
    let h = (size * 1.3).ceil();
    let root = b.add(parent, draw::frame(&format!("{addr} readout"), x, cy - h / 2.0, w, h));
    let text = |s: &str, fill: &str| {
        let mut n = draw::text("segments", s, 0.0, 0.0, w, size, fill, TextAlign::Right);
        if let NodeKind::Text(t) = &mut n.kind {
            *t = TextData { characters: s.to_string(), font_family: Some(draw::family::DSEG7.into()), font_size: size, align: TextAlign::Right, ..TextData::default() };
        }
        n
    };
    let unlit = if decimals == 0 { "8".repeat(digits) } else { format!("{}.{}", "8".repeat(digits - usize::from(decimals)), "8".repeat(usize::from(decimals))) };
    b.add(root, text(&unlit, &ui::alpha(color, 0.12)));
    let lit = b.add(root, draw::emissive(text("", color), draw::hex(color), 0.9, size * 0.5));
    let c = surface_proto::Control::new(surface_proto::ControlKind::Readout)
        .slot(surface_proto::SlotRole::Value, lit.0)
        .bound(ui::bind(addr))
        .formatted(surface_proto::ValueFormat::decimals(decimals).unit(""));
    b.controls.insert(root, c);
    root
}
