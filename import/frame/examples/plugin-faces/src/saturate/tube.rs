//! The valve preamp: a black steel front with cream lettering, and the
//! valves themselves lit behind a window in it.
//!
//! The window is the unit — two small-signal valves standing in their
//! sockets, heaters glowing, their plates catching the light. Left of it the
//! stage selector (triode / pentode: the two profiles of the family), right
//! the output VU and the power toggle with its pilot. The knob row: DRIVE
//! and BIAS first (bias leans the curve — the even harmonics are there),
//! then SAG, HEAT (how hot the heater runs), TILT, and MIX and OUTPUT last,
//! where they are on every saturation face. The heaters glow with DRIVE
//! and HEAT and flicker as they're pushed (motion), with or without a GPU.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Num, Target};
use surface_kit::{Place, draw, motion};
use surface_widgets::Builder;

use crate::delay::art;
use crate::delay::parts::{self, Ink, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#eadcb8";
const DIM: &str = "#a8987a";
const STAGES: &[&str] = &["Triode", "Pentode"];

fn a(s: &str) -> String {
    format!("tube/{s}")
}

/// A valve's heater glow, alive: it warms (slowly, as a cathode does)
/// with DRIVE and HEAT and goes dark with the power off, and it wavers —
/// more the harder it's driven. Its light (emission) flickers with it.
fn heat(b: &mut Builder<'_>, heater: NodeId) {
    let warm = "(0.6 + 0.4*norm(tube/drive)) * (0.8 + 0.2*norm(tube/heat)) * norm(tube/power)";
    motion::add(b, heater, Motion::new(Target::Opacity).input(warm).dynamics(Dynamics::Ballistic { attack: Num::Value(0.35), release: Num::Value(0.9) }));
    let waver = Motion::new(Target::Opacity).dynamics(Dynamics::Noise { rate: Num::Value(7.0), amount: Num::Expr("0.03 + 0.12*norm(tube/drive)".into()) });
    motion::add(b, heater, Motion { input: "0".into(), lag: true, ..waver });
    let glows: Vec<NodeId> = b.scene.walk(heater).into_iter().skip(1).filter(|id| b.scene.node(*id).is_some_and(|n| n.emission.is_some())).collect();
    for g in glows {
        motion::add(b, g, motion::flicker("0.05 + 0.25*norm(tube/drive)"));
    }
}

/// The window onto the valves.
fn window(b: &mut Builder<'_>, panel: NodeId) {
    let (x, y, w, h) = (250.0, 22.0, 360.0, 150.0);
    // A brass bezel round the glass.
    let bezel = draw::rounded(draw::rect("bezel", x - 7.0, y - 7.0, w + 14.0, h + 14.0, draw::linear(160.0, &[(0.0, "#e9d49a"), (0.35, "#a8894a"), (0.7, "#d8bf82"), (1.0, "#6f5a2c")])), 10.0);
    b.add(panel, draw::with_effects(bezel, vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 5.0)]));
    let mut win = draw::frame("Valve window", x, y, w, h);
    win.fills = vec![draw::linear(180.0, &[(0.0, "#0d0907"), (1.0, "#1c120b")])];
    win.clips_content = true;
    let win = b.add(panel, draw::with_effects(draw::rounded(win, 5.0), vec![draw::inner_shadow("#000000", 0.0, 3.0, 8.0)]));
    // Warm light pooled behind the glass, and the chassis the valves stand on.
    b.add(win, draw::rect("warmth", 0.0, 0.0, w, h, draw::radial_at(0.5, 0.78, 0.7, &[(0.0, "#7a3a1255"), (1.0, "#7a3a1200")])));
    b.add(win, draw::rect("chassis", 0.0, h - 22.0, w, 22.0, draw::linear(180.0, &[(0.0, "#6d6a64"), (0.2, "#3b3934"), (1.0, "#1b1a18")])));
    // Each valve throws its light on the chassis at its foot.
    for cx in [110.0, 250.0] {
        let pool = draw::rect("pool", cx - 60.0, h - 30.0, 120.0, 26.0, draw::radial(&[(0.0, "#ff8a2a50"), (1.0, "#ff8a2a00")]));
        b.add(win, pool);
    }
    // Two valves, and a transformer can between them for scale.
    for heater in [art::valve(b, win, (110.0, 22.0), (46.0, 118.0), 0.85), art::valve(b, win, (250.0, 22.0), (46.0, 118.0), 0.7)] {
        heat(b, heater);
    }
    let can = draw::rounded(draw::rect("output can", 158.0, 62.0, 44.0, 70.0, draw::linear(90.0, &[(0.0, "#2a2b2d"), (0.4, "#6a6c70"), (0.6, "#4a4c50"), (1.0, "#1c1d1f")])), 4.0);
    b.add(win, draw::with_effects(can, vec![draw::drop_shadow("#000000", 0.0, 2.0, 4.0)]));
    parts::silk(b, win, "V1", 110.0, h - 10.0, Ink::new(6.5, "#c9c2b2").weight(800.0).track(0.2));
    parts::silk(b, win, "V2", 250.0, h - 10.0, Ink::new(6.5, "#c9c2b2").weight(800.0).track(0.2));
    // The glass over it all: a faint diagonal glint.
    let glint = draw::polygon(&[(w * 0.62, 0.0), (w * 0.74, 0.0), (w * 0.52, h), (w * 0.4, h)]);
    b.add(win, draw::path("glint", 0.0, 0.0, w, h, glint, draw::solid("#ffffff0c")));
}

pub fn face() -> Face {
    let scene = canvas("Valve Preamp", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "black", W, H, "ears");
        parts::repaint(b, panel, &[(0.0, "#2b231e"), (0.5, "#1e1814"), (1.0, "#14100d")]);

        // ── Nameplate and stage ───────────────────────────────────────
        parts::silk_left(b, panel, "Valve Preamp", 50.0, 36.0, Ink::new(16.0, INK).weight(800.0).track(0.06).plain());
        parts::rule(b, panel, 50.0, 214.0, 49.0, 1.0, "#eadcb850");
        parts::silk_left(b, panel, "Class A · single-ended", 50.0, 62.0, Ink::new(6.8, DIM).weight(700.0).track(0.18));
        let stage = parts::selector(b, panel, "Stage", &a("stage"), "cream", (130.0, 134.0), 54.0, STAGES, INK);
        parts::legend_at(b, stage, "Stage", 168.0, Ink::new(7.0, DIM).weight(800.0).track(0.22));

        window(b, panel);

        // ── Output VU, power ───────────────────────────────────────────
        let vu = Place::new("Output").size(176.0).variant("black_rack").legend("VU").bind(parts::stream(&a("out")));
        let vu = parts::put(b, panel, "meter.vu_amber", &vu, 752.0, 92.0);
        parts::glass(b, vu);
        parts::silk(b, panel, "Output", 752.0, 160.0, Ink::new(6.8, DIM).weight(800.0).track(0.22));
        let power = Place::new("Power").size(38.0).variant("chrome").legend("Power").options(&["On", "Off"]).bind(bind(&a("power")));
        let pw = parts::put(b, panel, "switch.toggle_bat", &power, 882.0, 76.0);
        parts::caption_style(b, pw, Ink::new(6.5, INK).weight(800.0).track(0.14));
        let pilot = parts::put(b, panel, "button.jewel", &Place::new("Pilot").size(24.0).variant("amber").bind(bind(&a("power"))), 882.0, 132.0);
        parts::legend_at(b, pilot, "Pilot", 154.0, Ink::new(6.5, DIM).weight(800.0).track(0.2));

        // ── Knob row ───────────────────────────────────────────────────
        parts::rule(b, panel, 44.0, W - 44.0, 184.0, 1.0, "#eadcb826");
        let row = 238.0;
        let legend = Ink::new(8.5, INK).weight(800.0).track(0.14);
        let value = Ink::new(7.5, DIM).weight(600.0).track(0.02).plain();
        let scale = Ink::new(6.5, DIM).weight(700.0).track(0.0).plain();
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)]); 7] = [
            ("drive", "Drive", 110.0, 62.0, parts::PCT_MARKS),
            ("bias", "Bias", 236.0, 62.0, super::TRIM_MARKS),
            ("sag", "Sag", 360.0, 46.0, super::TRIM_MARKS),
            ("heat", "Heat", 466.0, 46.0, super::TRIM_MARKS),
            ("tilt", "Tilt", 572.0, 46.0, super::TRIM_MARKS),
            ("mix", "Mix", 710.0, 50.0, parts::PCT_MARKS),
            ("output", "Output", 842.0, 56.0, super::OUT_MARKS),
        ];
        for (addr, name, x, d, marks) in knobs {
            let k = parts::knob(b, panel, "knob.bakelite", "brown", &a(addr), name, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 5.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, 290.0, legend);
            parts::value_at(b, k, 305.0, value);
        }
        parts::vrule(b, panel, 640.0, 200.0, 304.0, 1.0, "#eadcb826");
    });

    let params = vec![
        parts::percent(&a("drive"), "Drive", 25.0).featured(),
        parts::trim(&a("bias"), "Bias").featured(),
        parts::trim(&a("sag"), "Sag"),
        parts::trim(&a("heat"), "Heat"),
        parts::trim(&a("tilt"), "Tilt"),
        parts::percent(&a("mix"), "Mix", 100.0).featured(),
        super::output(&a("output")),
        parts::choice(&a("stage"), "Stage", STAGES, 0),
        parts::toggle(&a("power"), "Power", true),
    ];
    let streams = vec![parts::level_stream(&a("out"), "Output Level", 2)];
    Face { file: "01-tube", plugin: "saturate", scene, params, streams }
}
