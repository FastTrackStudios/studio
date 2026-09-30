//! Analog: the bucket-brigade delay, kept simple — a matte midnight panel
//! with one clean line across it. The note is a bright spike at the left;
//! each repeat a soft swell further along (one delay time apart), lower by
//! the feedback and wider than the last, the way an analog repeat smears
//! and darkens. Beside it, the name and five knobs.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const INK: &str = "#e6eeff";
const DIM: &str = "#7f93bd";

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

/// A soft swell — a bell `w` wide and `h` tall standing on its baseline
/// at (`cx`, `base`) — filled with a glow and edged in `color`.
fn swell(b: &mut Builder<'_>, parent: NodeId, name: &str, (cx, base): (f64, f64), w: f64, h: f64, color: &str) -> NodeId {
    let g = b.add(parent, draw::frame(name, cx - w / 2.0, base - h, w, h));
    let pts: Vec<(f64, f64)> = (0..=40)
        .map(|k| {
            let u = f64::from(k) / 40.0;
            let z = (u - 0.5) * 5.0;
            (w * u, h - h * (-z * z / 2.0).exp())
        })
        .collect();
    let mut d = format!("M 0 {h:.1} ");
    for (x, y) in &pts {
        d += &format!("L {x:.1} {y:.1} ");
    }
    d += &format!("L {w:.1} {h:.1} Z");
    b.add(g, draw::path("glow", 0.0, 0.0, w, h, d, draw::linear(180.0, &[(0.0, &ui::alpha(color, 0.45)), (1.0, &ui::alpha(color, 0.0))])));
    b.add(g, ui::glowing(ui::strokes("edge", &[pts], 1.6, draw::solid(color)), color, 0.6, 4.0));
    g
}

/// The line in `view` (`w × h`): the note, then its repeats.
fn repeats(b: &mut Builder<'_>, view: NodeId, w: f64, h: f64) {
    let base = h - 18.0;
    // The baseline, and a faint time grid under it (a tick a quarter-second).
    b.add(view, ui::strokes("baseline", &[vec![(0.0, base), (w, base)]], 1.0, draw::solid(&ui::alpha(kit::BLUE_LIGHT, 0.35))));
    let window = 2500.0;
    let lead = 20.0;
    let span = w - lead - 16.0;
    let ticks: Vec<Vec<(f64, f64)>> = (0..=10).map(|i| {
        let x = lead + span * f64::from(i) * 250.0 / window;
        vec![(x, base + 3.0), (x, base + 7.0)]
    }).collect();
    b.add(view, ui::strokes("ticks", &ticks, 1.0, draw::solid(&ui::alpha(kit::BLUE_LIGHT, 0.3))));
    ui::silk_left(b, view, "0", lead - 2.0, base + 12.0, 5.5, DIM, 700.0);
    ui::silk(b, view, "2.5 s", lead + span - 6.0, base + 12.0, 5.5, DIM, 700.0);
    // The note.
    let note = draw::rect("note", lead - 1.0, 10.0, 2.0, base - 10.0, draw::solid(INK));
    b.add(view, ui::glowing(note, kit::BLUE_LIGHT, 0.9, 5.0));
    // The repeats: each drawn at the left, slid to its time, shrunk by the
    // feedback, wider each time round.
    let spring = Dynamics::spring(260.0, 28.0);
    for i in 1..=8_u32 {
        let fi = f64::from(i);
        let wd = 16.0 + 7.0 * fi;
        let s = swell(b, view, &format!("repeat {i}"), (lead, base), wd, base - 12.0, kit::BLUE);
        let at = format!("clamp({i} * param({DLY}/time) / {window}, 0, 1.2)");
        motion::add(b, s, Motion::new(Target::X).input(at).range(lead - wd / 2.0, lead - wd / 2.0 + span).dynamics(spring.clone()));
        let fall = format!("pow(param({DLY}/feedback), {}) * (0.3 + 0.7 * param({DLY}/on))", i - 1);
        motion::add(b, s, Motion::new(Target::StretchY).input(fall).range(0.01, 1.0).pivot(0.5, 1.0).dynamics(spring.clone()));
        // A slow wobble: the analog clock's modulation.
        let wob = format!("sin(time * 6.2832 * param({DLY}/mod_rate) + {fi}) * norm({DLY}/mod_depth)");
        motion::add(b, s, Motion::new(Target::Y).input(format!("0.5 + 0.5 * {wob}")).range(-2.0 * fi.sqrt(), 2.0 * fi.sqrt()));
    }
}

#[must_use]
pub fn face() -> Face {
    let scene = kit::lane("Analog", |b, root| {
        let mut panel = draw::frame("Panel", 0.0, 0.0, kit::W, kit::H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, "#16223d"), (1.0, "#0c1426")])];
        panel.corner_radii = document_proto::CornerRadii::uniform(10.0);
        let panel = b.add(root, draw::with_effects(draw::stroked(panel, "#2a3d66", 1.0), vec![draw::inner_shadow("#ffffff1c", 0.0, 1.0, 0.0)]));

        // ── The line ───────────────────────────────────────────────────
        let (vx, vy, vw, vh) = (14.0, 14.0, 540.0, kit::H - 28.0);
        let mut view = draw::frame("Repeats", vx, vy, vw, vh);
        view.fills = vec![draw::linear(180.0, &[(0.0, "#0a1122"), (1.0, "#070c19")])];
        view.clips_content = true;
        view.corner_radii = document_proto::CornerRadii::uniform(7.0);
        view.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 6.0)];
        let view = b.add(panel, view);
        repeats(b, view, vw, vh);

        // ── Name and knobs ─────────────────────────────────────────────
        let x0 = 578.0;
        let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
        crate::suite_studio_blues::parts::set(b, panel, "Analog", x0 + 44.0, 30.0, 26.0, INK, &sans, 0.02);
        ui::silk_left(b, panel, "Bucket-brigade delay", x0 + 96.0, 31.0, 6.5, DIM, 700.0);
        ui::rule(b, panel, x0, 48.0, 214.0, 1.0, &ui::alpha(kit::BLUE, 0.5));
        let knob = ["color=#1b2a4a", "pointer=#bcd6ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0b1224"];
        let knobs = [("time", "Time"), ("feedback", "Repeats"), ("high_cut", "Tone"), ("mod_depth", "Mod"), ("level", "Level")];
        for (i, (id, legend)) in knobs.iter().enumerate() {
            kit::knob3d(b, panel, &a(id), legend, 34.0, (x0 + 20.0 + 62.0 * i as f64, 92.0), &knob, INK, DIM);
        }
        kit::bypass(b, panel, DLY, kit::W - 36.0, 92.0, 26.0, "blue", "On");
    });
    kit::delay_face("62-dly-analog", scene)
}
