//! Cloud: the ambient reverb as the sky it sounds like — a dusk sky, soft
//! clouds drifting across it and grains of light glinting in them. The
//! clouds grow with the size and thicken with the decay; the modulation
//! sets how fast they drift. Beside the sky, a frosted panel.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::NodeId;
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const INK: &str = "#2e2a4a";
const DIM: &str = "#6d6790";

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}


/// A cloud: a cluster of soft puffs, `w × h` (drawn at its full size),
/// in `tint`.
fn cloud(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), tint: &str, seed: f64) -> NodeId {
    let g = b.add(parent, draw::frame(name, x, y, w, h));
    let puffs = 7;
    for i in 0..puffs {
        let t = f64::from(i) / f64::from(puffs - 1);
        let j = crate::reverb::jitter(seed + f64::from(i));
        let r = h * (0.28 + 0.2 * (std::f64::consts::PI * t).sin() + 0.06 * j);
        let (cx, cy) = (w * (0.12 + 0.76 * t), h * (0.62 - 0.22 * (std::f64::consts::PI * t).sin() + 0.05 * j));
        let puff = draw::circle("puff", cx, cy, r, draw::radial_at(0.45, 0.4, 0.75, &[(0.0, &ui::alpha(tint, 0.6)), (0.35, &ui::alpha(tint, 0.38)), (0.7, &ui::alpha(tint, 0.12)), (1.0, &ui::alpha(tint, 0.0))]));
        b.add(g, puff);
    }
    g
}

/// The sky in `sky` (`w × h`).
fn sky(b: &mut Builder<'_>, sky: NodeId, w: f64, h: f64) {
    // Stars, faint, fading in and out slowly.
    for i in 0..26 {
        let f = f64::from(i);
        let (x, y) = ((f * 83.0 + 17.0) % w, 6.0 + (f * 41.0) % (h * 0.55));
        let s = b.add(sky, draw::circle("star", x, y, 0.8 + 0.5 * crate::reverb::jitter(f).abs(), draw::solid("#fff6e8")));
        motion::add(b, s, Motion::new(Target::Opacity).input(format!("0.5 + 0.5 * sin(time * {:.2} + {f})", 0.4 + 0.3 * crate::reverb::jitter(f * 3.0).abs())).range(0.15, 0.9));
    }
    // The glow low on the horizon.
    b.add(sky, draw::circle("afterglow", w * 0.3, h * 1.25, h * 0.9, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#ffb38a99"), (1.0, "#ffb38a00")])));

    // The clouds, back to front: each drifts across and wraps round, the
    // near ones faster; all of them faster with the modulation.
    let layers = [
        ("far", 0.7, 0.012, 22.0, "#b9a8d8", 0.55),
        ("mid", 0.85, 0.02, 48.0, "#e2d2f0", 0.75),
        ("near", 1.0, 0.032, 70.0, "#fff1f4", 0.9),
    ];
    for (li, (name, scale, speed, cy, tint, alpha)) in layers.iter().enumerate() {
        for k in 0..3 {
            let cw = 220.0 * scale;
            let ch = 70.0 * scale;
            let c = cloud(b, sky, name, (0.0, cy - ch / 2.0 + 6.0 * f64::from(k % 2), cw, ch), tint, f64::from(li as u32 * 7 + k));
            let offset = f64::from(k) / 3.0 + 0.13 * li as f64;
            let drift = format!("fract(time * {speed} * (0.4 + 1.6 * norm({VERB}/modulation)) + {offset})");
            motion::add(b, c, Motion::new(Target::X).input(drift).range(-cw, w));
            // Bigger with the size, thicker with the decay.
            motion::add(b, c, Motion::new(Target::Scale).input(format!("norm({VERB}/size)")).range(0.7, 1.15).dynamics(Dynamics::spring(160.0, 22.0)));
            motion::add(b, c, Motion::new(Target::Opacity).input(format!("(0.35 + 0.65 * norm({VERB}/decay)) * (0.35 + 0.65 * param({VERB}/on))")).range(0.0, *alpha));
        }
    }
    // Grains of light in the cloud: the reverb's grains, glinting.
    for i in 0..30 {
        let f = f64::from(i);
        let (x, y) = ((f * 131.0 + 40.0) % (w - 20.0) + 10.0, 30.0 + (f * 59.0) % (h - 50.0));
        let g = b.add(sky, draw::emissive(draw::circle("grain", x, y, 1.3, draw::solid("#fffbe0")), draw::hex("#ffe9b0"), 1.0, 5.0));
        let glint = format!("pow(0.5 + 0.5 * sin(time * {:.2} + {:.2}), 12) * param({VERB}/on)", 1.1 + 0.9 * crate::reverb::jitter(f).abs(), f * 1.7);
        motion::add(b, g, Motion::new(Target::Opacity).input(glint).range(0.0, 1.0));
    }
}

#[must_use]
pub fn face() -> Face {
    let scene = kit::lane("Cloud", |b, root| {
        // The frosted panel.
        let mut panel = draw::frame("Panel", 0.0, 0.0, kit::W, kit::H);
        panel.fills = vec![draw::linear(160.0, &[(0.0, "#f4f0fb"), (0.6, "#e3dcf2"), (1.0, "#d2c8ea")])];
        panel.corner_radii = document_proto::CornerRadii::uniform(10.0);
        b.add(root, draw::with_effects(panel, vec![draw::inner_shadow("#ffffffcc", 0.0, 1.5, 1.0), draw::inner_shadow("#5a4a8a33", 0.0, -3.0, 6.0)]));
        let panel = root;

        // ── The sky ────────────────────────────────────────────────────
        let (sx, sy, sw, sh) = (10.0, 10.0, 600.0, kit::H - 20.0);
        let mut s = draw::frame("Sky", sx, sy, sw, sh);
        s.fills = vec![draw::linear(180.0, &[(0.0, "#1b1840"), (0.45, "#4a3a78"), (0.8, "#a8739a"), (1.0, "#f0a88a")])];
        s.clips_content = true;
        s.corner_radii = document_proto::CornerRadii::uniform(8.0);
        s.effects = vec![draw::inner_shadow("#00000066", 0.0, 2.0, 8.0)];
        let s = b.add(panel, s);
        sky(b, s, sw, sh);

        // ── The panel ──────────────────────────────────────────────────
        let x0 = 628.0;
        let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(400.0);
        crate::suite_studio_blues::parts::set(b, panel, "Cloud", x0 + 34.0, 24.0, 22.0, INK, &serif, 0.08);
        ui::silk_left(b, panel, "Ambient reverb", x0 + 76.0, 22.0, 6.5, DIM, 700.0);
        ui::silk_left(b, panel, "Diffuse · granular · endless", x0 + 76.0, 31.0, 5.5, DIM, 600.0);
        kit::bypass(b, panel, VERB, x0 + 16.0, 110.0, 26.0, "purple", "On");
        let pearl = ["color=#fbf9ff", "pointer=#7a68c8", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#cfc4ea"];
        kit::knob3d(b, panel, &a("decay"), "Decay", 50.0, (x0 + 76.0, 84.0), &pearl, INK, DIM);
        let small = [("size", "Size"), ("modulation", "Drift"), ("predelay", "Pre-Delay"), ("level", "Level")];
        for (i, (id, legend)) in small.iter().enumerate() {
            kit::knob3d(b, panel, &a(id), legend, 30.0, (x0 + 140.0 + 50.0 * i as f64, 92.0), &pearl, INK, DIM);
        }
    });
    kit::reverb_face("84-verb-cloud", scene)
}
