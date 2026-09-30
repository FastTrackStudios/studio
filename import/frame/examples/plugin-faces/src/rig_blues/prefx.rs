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
use surface_proto::{Range, Unit};

use super::PLUGIN;
use super::filters::repeats_screen;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;
use crate::suite_studio_blues::parts::set;

/// A lane: the Time module's proportions.
pub const LANE: (f64, f64) = (720.0, 164.0);

/// The Aqua-Puss as a lane.
#[must_use]
pub fn aqua_lane() -> Face {
    const NS: &str = "aquapuss";
    const INK: &str = "#0b2a33";
    let (w, h) = LANE;
    let scene = canvas("Aqua-Puss", |b, root| {
        ui::fit_root(b, root, w, h);
        let mut p = draw::frame("Panel", 0.0, 0.0, w, h);
        p.fills = vec![draw::linear(160.0, &[(0.0, "#5fd0e2"), (0.5, "#3fb4c8"), (1.0, "#2c95a8")])];
        let p = b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff40", 0.0, 1.5, 0.0)]));
        set(b, p, "Aqua-Puss", 92.0, 30.0, 26.0, "#f4fbfd", &Lettering::family(family::KNEWAVE), 0.0);
        ui::silk_left(b, p, "Analog delay", 30.0, 52.0, 6.5, INK, 800.0);
        kit::bypass(b, p, NS, 196.0, 30.0, 22.0, "blue", "On");
        let knob = ["color=#f2f1ec", "pointer=#0b2a33", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#d6d4cc"];
        for (i, (id, legend)) in [("time", "Delay"), ("feedback", "Feedback"), ("level", "Blend")].iter().enumerate() {
            kit::knob3d(b, p, &format!("{NS}/{id}"), legend, 34.0, (46.0 + 72.0 * i as f64, 96.0), &knob, INK, INK);
        }
        let screen = repeats_screen(b, p, (236.0, 10.0, w - 246.0, h - 20.0), NS);
        kit::responsive(b, p, &[screen]);
    });
    let params = vec![
        ui::param(&format!("{NS}/time"), "Delay", Range::log(20.0, 600.0, Unit::Milliseconds), 120.0).featured(),
        ui::param(&format!("{NS}/feedback"), "Feedback", Range::linear(0.0, 0.95, Unit::None), 0.15).featured(),
        ui::param(&format!("{NS}/level"), "Blend", Range::linear(-60.0, 12.0, Unit::Db), -16.0).featured(),
        ui::toggle(&format!("{NS}/on"), "Bypass", true).featured(),
    ];
    Face { file: "17-pedal-aqua-puss", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

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

/// The spring reverb as a lane, in purple.
#[must_use]
pub fn spring_lane() -> Face {
    const NS: &str = "spring";
    const INK: &str = "#efe7ff";
    const DIM: &str = "#a898cc";
    const COIL: &str = "#c9b8ff";
    let (w, h) = LANE;
    let scene = canvas("Spring", |b, root| {
        ui::fit_root(b, root, w, h);
        let mut p = draw::frame("Panel", 0.0, 0.0, w, h);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#2c1850"), (1.0, "#180c30")])];
        let p = b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff1c", 0.0, 1.0, 0.0)]));

        // ── The tank ────────────────────────────────────────────────────
        let (tx, ty, tw, th) = (10.0, 10.0, 440.0, h - 20.0);
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
        // The transducers at either end.
        for x in [14.0, tw - 34.0] {
            b.add(art, draw::with_effects(draw::rounded(draw::rect("transducer", x, 18.0, 20.0, th - 36.0, draw::linear(90.0, &[(0.0, "#3a2a5c"), (0.5, "#6b58a0"), (1.0, "#2a1d45")])), 3.0), vec![draw::drop_shadow("#00000080", 0.0, 1.5, 3.0)]));
        }
        // The three springs: each shivers as the reverb rings — a burst
        // on each note, dying away over the dwell.
        for (i, y) in [th * 0.3, th * 0.5, th * 0.7].iter().enumerate() {
            let g = b.add(art, draw::frame("spring", 0.0, 0.0, tw, th));
            b.add(g, ui::glowing(ui::strokes("coil", &[coil(34.0, tw - 34.0, *y, 38, 5.0)], 1.3, draw::solid(COIL)), "#a78bfa", 0.5, 4.0));
            let ring = format!("sin(time * {} + {i}) * pow(1 - fract(time * lerp(0.9, 0.25, norm({NS}/decay)) + {}), 2) * param({NS}/on)", 38.0 + 7.0 * i as f64, i as f64 * 0.21);
            motion::add(b, g, Motion::new(Target::Y).input(format!("0.5 + 0.5 * {ring}")).range(-3.0, 3.0));
        }
        // The drip: glints running down the springs.
        for k in 0..8 {
            let f = f64::from(k);
            let gl = b.add(art, draw::emissive(draw::circle("drip", 40.0 + (f * 53.0) % (tw - 80.0), th * (0.3 + 0.2 * f64::from(k % 3)), 1.6, draw::solid("#f3eaff")), draw::hex("#c4b5fd"), 1.0, 5.0));
            motion::add(b, gl, Motion::new(Target::Opacity).input(format!("pow(0.5 + 0.5 * sin(time * {:.2} + {:.2}), 10) * norm({NS}/modulation) * param({NS}/on)", 1.3 + 0.4 * f, f * 1.9)).range(0.0, 1.0));
        }

        // ── Its panel ───────────────────────────────────────────────────
        let x0 = tx + tw + 18.0;
        let serif = Lettering::family(family::PLAYFAIR_DISPLAY).weight(700.0);
        set(b, p, "Spring", x0 + 38.0, 28.0, 22.0, INK, &serif, 0.02);
        ui::silk_left(b, p, "Reverb tank", x0 + 84.0, 29.0, 6.5, DIM, 700.0);
        // Below the host's top-right corner.
        kit::bypass(b, p, NS, w - 30.0, 52.0, 20.0, "purple", "On");
        let knob = ["color=#241640", "pointer=#c4b5fd", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#100820"];
        for (i, (id, legend)) in [("decay", "Dwell"), ("tone", "Tone"), ("modulation", "Drip"), ("level", "Mix")].iter().enumerate() {
            kit::knob3d(b, p, &format!("{NS}/{id}"), legend, 34.0, (x0 + 24.0 + 64.0 * i as f64, 96.0), &knob, INK, DIM);
        }
        kit::responsive(b, p, &[t]);
    });
    let params = vec![
        ui::param("spring/decay", "Dwell", Range::linear(0.0, 1.0, Unit::None), 0.35).featured(),
        ui::param("spring/tone", "Tone", Range::linear(-1.0, 1.0, Unit::None), 0.0).featured(),
        ui::param("spring/damping", "Damping", Range::linear(0.0, 1.0, Unit::None), 0.3),
        ui::param("spring/level", "Mix", Range::linear(-60.0, 12.0, Unit::Db), -16.5).featured(),
        ui::param("spring/tension", "Tension", Range::linear(0.0, 1.0, Unit::None), 0.5),
        ui::param("spring/modulation", "Drip", Range::linear(0.0, 1.0, Unit::None), 0.2),
        ui::toggle("spring/on", "Reverb", true),
    ];
    Face { file: "19-spring-reverb", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
