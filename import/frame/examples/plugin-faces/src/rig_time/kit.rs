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
use surface_proto::{ButtonMode, Control, ControlKind, ParamInfo, Range, SlotRole, Unit};
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

/// A face's file: `base` for its light finish, `base-dark` for its dark.
#[must_use]
pub fn file(base: &'static str, dark: bool) -> &'static str {
    if dark { Box::leak(format!("{base}-dark").into_boxed_str()) } else { base }
}

/// The room a face is drawn in, smallest first: a face is one design
/// (one builder, its look written once) laid out for each tier it has — the
/// app draws the version for the room it is given, whatever the device.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// A channel strip's width.
    Strip,
    /// A phone held sideways: a lane about 760 × 150 points.
    Phone,
    Ipad,
    /// The base layout.
    Desktop,
    Fullscreen,
}

impl Tier {
    /// Its name in the manifest's `tiers`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Strip => "strip",
            Self::Phone => "phone",
            Self::Ipad => "ipad",
            Self::Desktop => "desktop",
            Self::Fullscreen => "fullscreen",
        }
    }

    /// A lane's width at this tier (its height is always `H`).
    #[must_use]
    pub const fn lane_w(self) -> f64 {
        match self {
            Self::Strip | Self::Phone => 760.0,
            Self::Ipad | Self::Desktop | Self::Fullscreen => W,
        }
    }
}

/// Where every Time face puts its controls on a phone, in lane
/// coordinates, so paging from one to the next nothing moves: the picture
/// keeps to the left of `PHONE_PICTURE_R`, the Time knob (centre, size),
/// and up to three knobs in a row (centres, their line, size).
pub const PHONE_PICTURE_R: f64 = 190.0;
pub const PHONE_TIME: (f64, f64, f64) = (252.0, 64.0, 80.0);
pub const PHONE_KNOBS: ([f64; 3], f64, f64) = ([392.0, 500.0, 608.0], 82.0, 58.0);

/// A face's file at `tier`: `base` (`-dark`) at the base tier, else
/// `base-<tier>` (`-dark`).
#[must_use]
pub fn tier_file(base: &'static str, tier: Tier, dark: bool) -> &'static str {
    if tier == Tier::Desktop {
        return file(base, dark);
    }
    let t = tier.name();
    Box::leak(if dark { format!("{base}-{t}-dark") } else { format!("{base}-{t}") }.into_boxed_str())
}

/// A lane's canvas at `tier`: `build` draws into the root, already
/// `tier.lane_w()` × `H`.
pub fn tier_lane(name: &str, tier: Tier, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> document_proto::Scene {
    let w = tier.lane_w();
    canvas(name, |b, root| {
        ui::fit_root(b, root, w, H);
        build(b, root);
        ui::flush(b, root, w, H);
    })
}

/// A lane's canvas: `build` draws into the root, already `W` × `H`.
pub fn lane(name: &str, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> document_proto::Scene {
    canvas(name, |b, root| {
        ui::fit_root(b, root, W, H);
        build(b, root);
        ui::flush(b, root, W, H);
    })
}

/// The nameplate every Time face carries in the same place — the top
/// right, one row `h` tall, its right end 12 in from the face's edge, `y`
/// down (see [`nameplate_top_right`]): at the right the algorithm's name,
/// lettered as the face likes (`title` draws it into a `title_w` × `h`
/// box), a press on it opening the algorithm menu; left of it the preset
/// playing (`preset_w` wide: its name, then ‹ › to step it), a press on
/// its name opening the presets. The host fills `<ns>/preset` and answers the presses
/// ([`nameplate_params`]).
#[allow(clippy::too_many_arguments)]
pub fn nameplate(b: &mut Builder<'_>, parent: NodeId, ns: &str, (x, y, h): (f64, f64, f64), title_w: f64, preset_w: f64, size: f64, (accent, dim): (&str, &str), title: impl FnOnce(&mut Builder<'_>, NodeId)) {
    let press = |b: &mut Builder<'_>, node: NodeId, id: &str| {
        b.controls.insert(node, Control::new(ControlKind::Button { mode: ButtonMode::Momentary }).bound(ui::bind(&a(ns, id))));
    };
    // The preset first: its name, set against the arrows (names differ in
    // length; the row stays together at the right), then ‹ ›.
    let arrow = size + 6.0;
    let nw = preset_w - 2.0 * arrow;
    let field = b.add(parent, draw::frame("preset", x, y, nw, h));
    let mut name = draw::text("value", "", 0.0, (h - size * 1.3) / 2.0, nw, size, accent, TextAlign::Right);
    if let document_proto::NodeKind::Text(d) = &mut name.kind {
        d.font_family = Some(draw::family::INTER.into());
        d.font_weight = Some(650.0);
    }
    let name = b.add(field, name);
    b.controls.insert(field, Control::new(ControlKind::Readout).slot(SlotRole::Value, name.0).bound(ui::bind(&a(ns, "preset"))));
    // Over the name, a press opens the presets.
    let open = b.add(parent, draw::frame("preset browse", x, y, nw, h));
    press(b, open, "preset_browse");
    let prev = b.add(parent, draw::frame("preset prev", x + nw, y, arrow, h));
    ui::line(b, prev, "chevron", &[(arrow / 2.0 + 2.0, h / 2.0 - 4.0), (arrow / 2.0 - 2.0, h / 2.0), (arrow / 2.0 + 2.0, h / 2.0 + 4.0)], 1.4, dim);
    press(b, prev, "preset_prev");
    let next = b.add(parent, draw::frame("preset next", x + nw + arrow, y, arrow, h));
    ui::line(b, next, "chevron", &[(arrow / 2.0 - 2.0, h / 2.0 - 4.0), (arrow / 2.0 + 2.0, h / 2.0), (arrow / 2.0 - 2.0, h / 2.0 + 4.0)], 1.4, dim);
    press(b, next, "preset_next");
    // Then the algorithm, at the right: its name, and a caret saying it
    // opens.
    let t = b.add(parent, draw::frame("algorithm", x + preset_w + 6.0, y, title_w, h));
    title(b, t);
    let (cx, cy) = (title_w - 6.0, h / 2.0);
    ui::line(b, t, "caret", &[(cx - 4.0, cy - 2.0), (cx, cy + 2.0), (cx + 4.0, cy - 2.0)], 1.4, dim);
    press(b, t, "algo_menu");
}

/// [`nameplate`] in its standard place: the top right of a face at
/// `tier`, `y` down.
#[allow(clippy::too_many_arguments)]
pub fn nameplate_top_right(b: &mut Builder<'_>, parent: NodeId, ns: &str, tier: Tier, (y, h): (f64, f64), title_w: f64, preset_w: f64, size: f64, inks: (&str, &str), title: impl FnOnce(&mut Builder<'_>, NodeId)) {
    let x = tier.lane_w() - 12.0 - preset_w - 6.0 - title_w;
    nameplate(b, parent, ns, (x, y, h), title_w, preset_w, size, inks, title);
}

/// The Time knob every Time face has, the same at every face: the face's
/// own knob on `<ns>/time_knob` (`knob` draws it, bare, at (`cx`, `cy`), `d`
/// across), the time read out under it ("2.2 s", "1 bar · 2.0 s",
/// "540 ms", "1/8. · 375 ms" — the host's `<ns>/time_text`), and for its
/// legend the TIME | BEAT switch on `<ns>/sync`: free, or locked to the
/// tempo. The host maps it onto the block either way (not the block's own `time`,
/// which the pictures animate from).
#[allow(clippy::too_many_arguments)]
pub fn time_knob(b: &mut Builder<'_>, parent: NodeId, ns: &str, (cx, cy, d): (f64, f64, f64), size: f64, (ink, dim, lit): (&str, &str, &str), knob: impl FnOnce(&mut Builder<'_>, NodeId, &str)) {
    knob(b, parent, &a(ns, "time_knob"));
    // The time, under the knob.
    let (rw, rh) = (d + 70.0, (size * 1.35).ceil());
    let ry = cy + d / 2.0 + 3.0;
    let field = b.add(parent, draw::frame("time readout", cx - rw / 2.0, ry, rw, rh));
    let mut t = draw::text("value", "", 0.0, 0.0, rw, size, ink, TextAlign::Center);
    if let document_proto::NodeKind::Text(td) = &mut t.kind {
        td.font_family = Some(draw::family::INTER.into());
        td.font_weight = Some(650.0);
    }
    let t = b.add(field, t);
    b.controls.insert(field, Control::new(ControlKind::Readout).slot(SlotRole::Value, t.0).bound(ui::bind(&a(ns, "time_text"))));
    // TIME | BEAT, the side it is on lit.
    let (sw, sh) = ((size * 5.4).max(70.0), (size * 0.8 + 8.0).ceil());
    let sy = ry + rh + 2.0;
    let pill = b.add(parent, draw::frame("time switch", cx - sw / 2.0, sy, sw, sh));
    b.add(pill, draw::rounded(draw::rect("well", 0.0, 0.0, sw, sh, draw::solid(&ui::alpha(dim, 0.22))), sh / 2.0));
    for (i, (word, on)) in [("TIME", format!("1 - param({ns}/sync)")), ("BEAT", format!("param({ns}/sync)"))].iter().enumerate() {
        let x = i as f64 * sw / 2.0;
        let half = b.add(pill, draw::frame(word, x, 0.0, sw / 2.0, sh));
        let glow = b.add(half, draw::rounded(draw::rect("lit", 1.5, 1.5, sw / 2.0 - 3.0, sh - 3.0, draw::solid(lit)), (sh - 3.0) / 2.0));
        motion::add(b, glow, Motion::new(Target::Opacity).input(on.clone()).range(0.0, 1.0).dynamics(Dynamics::spring(400.0, 36.0)));
        ui::silk(b, half, word, sw / 4.0, sh / 2.0, size * 0.8, ink, 800.0);
    }
    b.controls.insert(pill, Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).bound(ui::bind(&a(ns, "sync"))));
}

/// A `knob.3d` with no value or legend of its own (the [`time_knob`]
/// prints them), in the kit's options.
pub fn knob3d_bare(b: &mut Builder<'_>, parent: NodeId, addr: &str, d: f64, (cx, cy): (f64, f64), options: &[&str]) -> NodeId {
    let mut opts = vec!["touch=44", "ticks=none"];
    opts.extend_from_slice(options);
    let p = Place::new("Time").legend(" ").size(d).variant("flat").options(&opts).bind(ui::bind(addr));
    let k = ui::put(b, parent, "knob.3d", &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// The params a [`time_knob`] binds: its position, the switch, and the
/// time the host reads out.
#[must_use]
pub fn time_params(ns: &str) -> Vec<ParamInfo> {
    vec![
        ui::param(&a(ns, "time_knob"), "Time", Range::linear(0.0, 1.0, Unit::None), 0.5).featured(),
        ui::toggle(&a(ns, "sync"), "Beat", false),
        ui::param(&a(ns, "time_text"), "Time", Range::linear(0.0, 1.0, Unit::None), 0.0).text(),
    ]
}

/// Every Time face's bypass in the same place: on the right edge, at mid
/// height, the same size at a tier (a finger's 44 points on a phone) —
/// whatever the face, it is there to hit.
pub fn face_bypass(b: &mut Builder<'_>, parent: NodeId, ns: &str, tier: Tier, variant: &str, legend: &str) -> NodeId {
    let (cx, cy, d) = bypass_spot(tier);
    bypass(b, parent, ns, cx, cy, d, variant, legend)
}

/// Where a Time face's bypass goes at `tier` — its centre and size — for
/// a face whose own key sits there in its own style (the tape echo's
/// power key).
#[must_use]
pub fn bypass_spot(tier: Tier) -> (f64, f64, f64) {
    let (d, inset) = if matches!(tier, Tier::Phone | Tier::Strip) { (44.0, 36.0) } else { (28.0, 26.0) };
    (tier.lane_w() - inset, H * 0.56, d)
}

/// The params a [`nameplate`] binds: the preset's name (text the host
/// fills) and the four presses.
#[must_use]
pub fn nameplate_params(ns: &str) -> Vec<ParamInfo> {
    let mut v = vec![ui::param(&a(ns, "preset"), "Preset", Range::linear(0.0, 1.0, Unit::None), 0.0).text()];
    for (id, name) in [("algo_menu", "Algorithm"), ("preset_prev", "Previous preset"), ("preset_next", "Next preset"), ("preset_browse", "Presets")] {
        v.push(ui::toggle(&a(ns, id), name, false));
    }
    v
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
pub fn stub(ns: &str, name: &str, color: &str, dark: bool) -> document_proto::Scene {
    let ns = ns.to_string();
    let name = name.to_string();
    // The section's colour, whatever the placeholder asked for.
    let _ = color;
    let color = if ns == DLY { BLUE } else { PURPLE }.to_string();
    lane(&name.clone(), move |b, root| {
        let (ink, dim, top, bottom) = if dark { ("#e8e6e1", "#8a8d94", "#1d1f24", "#121317") } else { ("#1d1f24", "#686c74", "#eceef1", "#d7dae0") };
        let mut panel = draw::frame("Panel", 0.0, 0.0, W, H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, top), (1.0, bottom)])];
        panel.corner_radii = document_proto::CornerRadii::uniform(8.0);
        let panel = b.add(root, panel);
        b.add(panel, ui::text("name", &name, 22.0, 30.0, 200.0, 20.0, &color, TextAlign::Left, 800.0, true));
        bypass(b, panel, &ns, 60.0, 100.0, 34.0, if ns == DLY { "blue" } else { "purple" }, "On");
        if ns == DLY {
            echo_train(b, panel, (220.0, 24.0, 380.0, 116.0), 8, 2000.0, &color);
            for (i, (id, l)) in [("time", "Time"), ("feedback", "Feedback"), ("level", "Level")].iter().enumerate() {
                knob(b, panel, "knob.3d", "flat", &a(&ns, id), l, 46.0, 670.0 + 100.0 * i as f64, 70.0, ink, dim);
            }
        } else {
            decay(b, panel, (220.0, 24.0, 380.0, 116.0), &color);
            for (i, (id, l)) in [("decay", "Decay"), ("size", "Size"), ("level", "Level")].iter().enumerate() {
                knob(b, panel, "knob.3d", "flat", &a(&ns, id), l, 46.0, 670.0 + 100.0 * i as f64, 70.0, ink, dim);
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

/// Make a face responsive about its picture: `stretch` (the picture's
/// window, a bay of knobs) takes a change of width; the rest of `parent`'s
/// children pin to whichever side of it they sit on — left of it to the
/// left edge, right of it to the right — so controls keep their size and
/// spacing while the picture widens or narrows. Anything spanning the
/// whole face (its ground) stretches of itself.
pub fn responsive(b: &mut Builder<'_>, parent: NodeId, stretch: &[NodeId]) {
    use document_proto::ResizeX;
    let span = |b: &Builder<'_>, id: NodeId| b.scene.node(id).map(|n| (n.transform.tx, n.transform.tx + n.size.width));
    let Some((l, r)) = stretch.iter().filter_map(|id| span(b, *id)).reduce(|a, c| (a.0.min(c.0), a.1.max(c.1))) else { return };
    let kids = b.scene.node(parent).map(|n| n.children.clone()).unwrap_or_default();
    let pw = b.scene.node(parent).map_or(0.0, |n| n.size.width);
    for id in kids {
        let Some(n) = b.scene.node_mut(id) else { continue };
        if n.layout_child.resize_x.is_some() {
            continue;
        }
        let (x, w) = (n.transform.tx, n.size.width);
        n.layout_child.resize_x = if stretch.contains(&id) {
            Some(ResizeX::Stretch)
        } else if w >= pw * 0.95 {
            None
        } else if x + w / 2.0 <= l {
            Some(ResizeX::Left)
        } else if x + w / 2.0 >= r {
            Some(ResizeX::Right)
        } else {
            None
        };
    }
}

/// `id` with its sibling `offset` places along (−1: the one drawn just
/// before it, a window's bezel; +1: just after, its rim), for
/// [`responsive`].
#[must_use]
pub fn with_sibling(b: &Builder<'_>, parent: NodeId, id: NodeId, offset: isize) -> Vec<NodeId> {
    let kids = b.scene.node(parent).map(|n| n.children.clone()).unwrap_or_default();
    let mut v = vec![id];
    if let Some(i) = kids.iter().position(|k| *k == id)
        && let Some(s) = kids.get(i.wrapping_add_signed(offset))
    {
        v.push(*s);
    }
    v
}

/// A drawing that should scale with its window (a plate's sheet and
/// pickups, a hall's plan): `parent`'s children take position and width in
/// proportion when the window resizes (their own children keep their size);
/// what spans the window stretches with it instead.
pub fn scale_children(b: &mut Builder<'_>, parent: NodeId) {
    let kids = b.scene.node(parent).map(|n| n.children.clone()).unwrap_or_default();
    let pw = b.scene.node(parent).map_or(0.0, |n| n.size.width);
    for id in kids {
        if let Some(n) = b.scene.node_mut(id)
            && n.layout_child.resize_x.is_none()
            // What spans the window (its frame, its glass) stretches with it.
            && n.size.width < pw * 0.9
        {
            n.layout_child.resize_x = Some(document_proto::ResizeX::Scale);
        }
    }
}
