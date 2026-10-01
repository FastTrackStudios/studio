//! The Pre FX module as lanes, in the Time module's form: two half-height
//! lanes stacked — the delay on top, the reverb under it — beside the
//! tremolo.
//!
//! - Aqua-Puss (`17-pedal-aqua-puss`): the teal delay, its knobs and name
//!   at the left, its repeats on a screen across the rest.
//! - Spring (`19-spring-reverb`): a purple spring tank — three coils hung
//!   between their transducers, shivering as the reverb rings, longer with
//!   the dwell, the drip glinting — and its knobs.

use document_proto::motion::{Motion, Target};
use document_proto::{CornerRadii, ResizeX};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;

use super::PLUGIN;
use super::filters::repeats_screen;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;
use crate::suite_studio_blues::parts::set;

/// A lane: the Time module's proportions.
pub const LANE: (f64, f64) = (720.0, 164.0);

/// A coil spring along `y` from `x0` to `x1`: `turns` loops, `r` high.
fn coil(x0: f64, x1: f64, y: f64, turns: usize, r: f64) -> Vec<(f64, f64)> {
    let n = turns * 12;
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let ang = t * turns as f64 * std::f64::consts::TAU;
            (x0 + (x1 - x0) * t + ang.cos() * r * 0.35, y + ang.sin() * r)
        })
        .collect()
}

/// Where a Pre FX lane's parts go at `tier`, on a lane `w` wide: the
/// picture's right edge, the Time knob (centre, size), the knob row
/// (centres, line, size), the readout's size — the Time faces' places on a
/// phone, the same order in a desktop lane's 720.
struct Spots {
    w: f64,
    picture_r: f64,
    time: (f64, f64, f64),
    knobs: ([f64; 3], f64, f64),
    text: f64,
    small: bool,
}

impl Spots {
    fn at(tier: kit::Tier) -> Self {
        if matches!(tier, kit::Tier::Phone | kit::Tier::Strip) {
            Self { w: tier.lane_w(), picture_r: kit::PHONE_PICTURE_R, time: kit::PHONE_TIME, knobs: kit::PHONE_KNOBS, text: 11.0, small: true }
        } else {
            Self { w: LANE.0, picture_r: 300.0, time: (350.0, 70.0, 56.0), knobs: ([440.0, 516.0, 592.0], 82.0, 40.0), text: 8.5, small: false }
        }
    }
}

/// A Pre FX lane's panel `w` wide in its fill.
fn lane_panel(b: &mut surface_widgets::Builder<'_>, root: document_proto::NodeId, w: f64, fill: document_proto::Paint, sheen: &str) -> document_proto::NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, LANE.1);
    p.fills = vec![fill];
    b.add(root, draw::with_effects(p, vec![draw::inner_shadow(sheen, 0.0, 1.5, 0.0)]))
}

const AQUA: &str = "aquapuss";
const AQUA_INK: &str = "#0b2a33";
const AQUA_KNOB: [&str; 6] = ["color=#f2f1ec", "pointer=#0b2a33", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#d6d4cc"];

fn aqua_fill() -> document_proto::Paint {
    draw::linear(160.0, &[(0.0, "#5fd0e2"), (0.5, "#3fb4c8"), (1.0, "#2c95a8")])
}

/// The Aqua-Puss's head where every Time face has it: the nameplate (its
/// presets, its name in its bubbly hand), the bypass and the arrow.
fn aqua_head(b: &mut surface_widgets::Builder<'_>, p: document_proto::NodeId, tier: kit::Tier, sp: &Spots, back: bool) {
    let (h, size, fs) = if sp.small { (30.0, 12.0, 22.0) } else { (26.0, 10.0, 19.0) };
    let tw = if sp.small { 140.0 } else { 120.0 };
    kit::nameplate_right(b, p, AQUA, sp.w, (8.0, h), tw, if sp.small { 210.0 } else { 180.0 }, size, ("#06323d", "#0b4a58"), |b, t| {
        set(b, t, "Aqua-Puss", (tw - 14.0) / 2.0, h / 2.0, fs, "#f4fbfd", &Lettering::family(family::KNEWAVE), 0.0);
    });
    kit::face_bypass_w(b, p, AQUA, tier, sp.w, "blue", "On");
    kit::flip_arrow_w(b, p, AQUA, tier, sp.w, back, (AQUA_INK, "#ffffff40"));
}

fn aqua_params() -> Vec<surface_proto::ParamInfo> {
    let mut v = kit::delay_params_in(AQUA);
    v.extend(kit::nameplate_params(AQUA));
    v.extend(kit::flip_params(AQUA));
    v
}

/// The Aqua-Puss as a lane.
#[must_use]
pub fn aqua_lane() -> Face {
    aqua_at(kit::Tier::Desktop)
}

/// The Aqua-Puss at `tier`, in the Time faces' form: its repeats on the
/// screen at the left, the Delay as the Time knob (TIME | BEAT), Feedback,
/// Blend and Tone in the knob row, the head at the right.
#[must_use]
pub fn aqua_at(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Aqua-Puss", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, aqua_fill(), "#ffffff40");
        let screen = repeats_screen(b, p, (10.0, 10.0, sp.picture_r - 10.0, LANE.1 - 20.0), AQUA);
        let (tx, ty, td) = sp.time;
        kit::time_knob(b, p, AQUA, (tx, ty, td), sp.text, (AQUA_INK, "#0b4a58", "#ffffffa0"), |b, pp, addr| {
            kit::knob3d_bare(b, pp, addr, td, (tx, ty), &AQUA_KNOB);
        });
        let (xs, ky, kd) = sp.knobs;
        for (i, (id, legend)) in [("feedback", "Feedback"), ("level", "Blend"), ("high_cut", "Tone")].iter().enumerate() {
            kit::knob3d(b, p, &format!("{AQUA}/{id}"), legend, kd, (xs[i], ky), &AQUA_KNOB, AQUA_INK, AQUA_INK);
        }
        aqua_head(b, p, tier, &sp, false);
        kit::responsive(b, p, &[screen]);
    });
    let mut params = aqua_params();
    params.extend(kit::time_params(AQUA));
    Face { file: kit::tier_file("17-pedal-aqua-puss", tier, false), plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Aqua-Puss's back: its teal, no screen — the advanced controls.
#[must_use]
pub fn aqua_back(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Aqua-Puss", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, aqua_fill(), "#ffffff40");
        let knobs = [("mod_depth", "Mod"), ("mod_rate", "Mod Rate"), ("high_pass", "Low Cut"), ("duck_sens", "Duck"), ("duck_release", "Duck Rel"), ("pan", "Pan"), ("dry", "Dry")];
        kit::back_knobs_w(b, p, tier, sp.w, &knobs, AQUA_INK, |b, pp, id, legend, d, at| {
            kit::knob3d(b, pp, &format!("{AQUA}/{id}"), legend, d, at, &AQUA_KNOB, AQUA_INK, AQUA_INK);
        });
        aqua_head(b, p, tier, &sp, true);
    });
    Face { file: kit::tier_file("17-pedal-aqua-puss-back", tier, false), plugin: PLUGIN, scene, params: aqua_params(), streams: Vec::new() }
}

const SPRING: &str = "spring";
const SPRING_INK: &str = "#efe7ff";
const SPRING_DIM: &str = "#a898cc";
const SPRING_KNOB: [&str; 6] = ["color=#241640", "pointer=#c4b5fd", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#100820"];

fn spring_fill() -> document_proto::Paint {
    draw::linear(180.0, &[(0.0, "#2c1850"), (1.0, "#180c30")])
}

/// The spring tank in `p` at (`tx`, `ty`), `tw × th`: three coils hung
/// between their transducers, shivering as the reverb rings, the drip
/// glinting. Returns the tank (it takes a lane's width).
fn tank(b: &mut surface_widgets::Builder<'_>, p: document_proto::NodeId, (tx, ty, tw, th): (f64, f64, f64, f64)) -> document_proto::NodeId {
    const NS: &str = SPRING;
    const COIL: &str = "#c9b8ff";
    let mut t = draw::frame("Tank", tx, ty, tw, th);
    t.fills = vec![draw::linear(180.0, &[(0.0, "#140a26"), (1.0, "#0b0616")])];
    t.clips_content = true;
    t.corner_radii = CornerRadii::uniform(6.0);
    t.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
    t.layout_child.resize_x = Some(ResizeX::Stretch);
    let t = b.add(p, t);
    let mut art = draw::frame("springs", 0.0, 0.0, tw, th);
    art.layout_child.resize_x = Some(ResizeX::Scale);
    let art = b.add(t, art);
    for x in [10.0, tw - 26.0] {
        b.add(art, draw::with_effects(draw::rounded(draw::rect("transducer", x, 18.0, 16.0, th - 36.0, draw::linear(90.0, &[(0.0, "#3a2a5c"), (0.5, "#6b58a0"), (1.0, "#2a1d45")])), 3.0), vec![draw::drop_shadow("#00000080", 0.0, 1.5, 3.0)]));
    }
    let turns = ((tw - 52.0) / 11.0).max(8.0) as usize;
    for (i, y) in [th * 0.3, th * 0.5, th * 0.7].iter().enumerate() {
        let g = b.add(art, draw::frame("spring", 0.0, 0.0, tw, th));
        b.add(g, ui::glowing(ui::strokes("coil", &[coil(26.0, tw - 26.0, *y, turns, 5.0)], 1.3, draw::solid(COIL)), "#a78bfa", 0.5, 4.0));
        let ring = format!("sin(time * {} + {i}) * pow(1 - fract(time * lerp(0.9, 0.25, norm({NS}/decay)) + {}), 2) * param({NS}/on)", 38.0 + 7.0 * i as f64, i as f64 * 0.21);
        motion::add(b, g, Motion::new(Target::Y).input(format!("0.5 + 0.5 * {ring}")).range(-3.0, 3.0));
    }
    for k in 0..8 {
        let f = f64::from(k);
        let gl = b.add(art, draw::emissive(draw::circle("drip", 30.0 + (f * 53.0) % (tw - 60.0).max(1.0), th * (0.3 + 0.2 * f64::from(k % 3)), 1.6, draw::solid("#f3eaff")), draw::hex("#c4b5fd"), 1.0, 5.0));
        motion::add(b, gl, Motion::new(Target::Opacity).input(format!("pow(0.5 + 0.5 * sin(time * {:.2} + {:.2}), 10) * norm({NS}/modulation) * param({NS}/on)", 1.3 + 0.4 * f, f * 1.9)).range(0.0, 1.0));
    }
    t
}

fn spring_head(b: &mut surface_widgets::Builder<'_>, p: document_proto::NodeId, tier: kit::Tier, sp: &Spots, back: bool) {
    let (h, size, fs) = if sp.small { (30.0, 12.0, 22.0) } else { (26.0, 10.0, 19.0) };
    let tw = if sp.small { 104.0 } else { 92.0 };
    let serif = Lettering::family(family::PLAYFAIR_DISPLAY).weight(700.0);
    kit::nameplate_right(b, p, SPRING, sp.w, (8.0, h), tw, if sp.small { 210.0 } else { 180.0 }, size, (kit::PURPLE_LAMP, SPRING_DIM), |b, t| {
        set(b, t, "Spring", (tw - 14.0) / 2.0, h / 2.0, fs, SPRING_INK, &serif, 0.02);
    });
    kit::face_bypass_w(b, p, SPRING, tier, sp.w, "purple", "On");
    kit::flip_arrow_w(b, p, SPRING, tier, sp.w, back, (SPRING_INK, "#ffffff1c"));
}

fn spring_params() -> Vec<surface_proto::ParamInfo> {
    let mut v = kit::reverb_params_in(SPRING);
    v.extend(kit::nameplate_params(SPRING));
    v.extend(kit::flip_params(SPRING));
    v
}

/// The spring reverb as a lane, in purple.
#[must_use]
pub fn spring_lane() -> Face {
    spring_at(kit::Tier::Desktop)
}

/// The spring at `tier`, in the Time faces' form: the tank at the left, the
/// Dwell as the Time knob (TIME | BEAT), Tone, Drip and Mix in the knob
/// row, the head at the right.
#[must_use]
pub fn spring_at(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Spring", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, spring_fill(), "#ffffff1c");
        let t = tank(b, p, (10.0, 10.0, sp.picture_r - 10.0, LANE.1 - 20.0));
        let (tx, ty, td) = sp.time;
        kit::time_knob(b, p, SPRING, (tx, ty, td), sp.text, (SPRING_INK, SPRING_DIM, &ui::alpha(kit::PURPLE_LAMP, 0.55)), |b, pp, addr| {
            kit::knob3d_bare(b, pp, addr, td, (tx, ty), &SPRING_KNOB);
        });
        let (xs, ky, kd) = sp.knobs;
        for (i, (id, legend)) in [("tone", "Tone"), ("modulation", "Drip"), ("level", "Mix")].iter().enumerate() {
            kit::knob3d(b, p, &format!("{SPRING}/{id}"), legend, kd, (xs[i], ky), &SPRING_KNOB, SPRING_INK, SPRING_DIM);
        }
        spring_head(b, p, tier, &sp, false);
        kit::responsive(b, p, &[t]);
    });
    let mut params = spring_params();
    params.extend(kit::time_params(SPRING));
    Face { file: kit::tier_file("19-spring-reverb", tier, false), plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The spring's back: its purple, no tank — the advanced controls.
#[must_use]
pub fn spring_back(tier: kit::Tier) -> Face {
    let sp = Spots::at(tier);
    let scene = canvas("Spring", |b, root| {
        ui::fit_root(b, root, sp.w, LANE.1);
        let p = lane_panel(b, root, sp.w, spring_fill(), "#ffffff1c");
        let knobs = [("damping", "Damping"), ("low_cut", "Low Cut"), ("high_cut", "High Cut"), ("predelay", "Pre-Delay"), ("duck", "Duck"), ("duck_threshold", "Duck Thr"), ("duck_release", "Duck Rel"), ("pan_a", "Pan")];
        kit::back_knobs_w(b, p, tier, sp.w, &knobs, SPRING_INK, |b, pp, id, legend, d, at| {
            kit::knob3d(b, pp, &format!("{SPRING}/{id}"), legend, d, at, &SPRING_KNOB, SPRING_INK, SPRING_DIM);
        });
        spring_head(b, p, tier, &sp, true);
    });
    Face { file: kit::tier_file("19-spring-reverb-back", tier, false), plugin: PLUGIN, scene, params: spring_params(), streams: Vec::new() }
}
