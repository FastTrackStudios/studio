//! The tabletop harmonic-tremolo and spring-reverb unit that sits after the
//! amp: a navy case on copper feet, a copper mesh window across the top,
//! its name in wide pale capitals, and two sections bracketed under their
//! names — tremolo (on/off, a rate lamp pulsing at the speed, INTENSITY,
//! SPEED, the subdivision on a small display, SYNC/FREE) and reverb (MIX,
//! TONE, DECAY, its lamp, on/off). Tabletop, 1200 × 490.

use document_proto::motion::{Dynamics, Motion, Num, Target, Wave};
use document_proto::{CornerRadii, NodeId};
use surface_kit::{Place, draw, motion};
use surface_proto::{Range, SlotRole, Unit, ValueFormat};
use surface_widgets::Builder;

use super::parts::{PLUGIN, knob, print, print_left, print_right, toggle};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 1200.0;
const H: f64 = 490.0;
const BODY_H: f64 = 448.0;
const INK: &str = "#e2d2c6";
const DIM: &str = "#c9b8ac";

fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64) {
    b.add(parent, draw::with_effects(draw::circle("screw", cx, cy, 9.0, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#5a5f70"), (1.0, "#15171f")])), vec![draw::inner_shadow("#00000090", 0.0, 1.0, 2.0)]));
    b.add(parent, draw::circle("screw head", cx, cy, 5.5, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#8a8f9c"), (1.0, "#2a2d36")])));
}

/// A bracket under a section name: a rule with ticks down at `ticks`.
fn bracket(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64, ticks: &[f64]) {
    let mut lines = vec![vec![(x0, y), (x1, y)]];
    lines.extend(ticks.iter().map(|x| vec![(*x, y), (*x, y + 9.0)]));
    b.add(parent, ui::strokes("bracket", &lines, 1.6, draw::solid(DIM)));
}

pub fn spring_tank() -> Face {
    let scene = canvas("Spring Tank", |b, root| {
        ui::fit_root(b, root, W, H);
        // Copper feet under the case.
        for x in [92.0, 1016.0] {
            let foot = draw::rect("foot", x, BODY_H - 20.0, 92.0, H - BODY_H + 20.0, draw::linear(90.0, &[(0.0, "#8a5234"), (0.3, "#e8a57a"), (0.55, "#f6c9a4"), (1.0, "#7a4228")]));
            b.add(root, draw::rounded(foot, 6.0));
        }
        let mut case = draw::frame("Case", 0.0, 0.0, W, BODY_H);
        case.fills = vec![draw::linear(180.0, &[(0.0, "#363949"), (0.5, "#2e3140"), (1.0, "#262834")]), draw::radial_at(0.5, 0.2, 1.0, &[(0.0, "#ffffff10"), (1.0, "#ffffff00")])];
        case.corner_radii = CornerRadii::uniform(32.0);
        case.stroke = document_proto::Stroke { paints: vec![draw::linear(90.0, &[(0.0, "#a8664a"), (0.5, "#5a3a34"), (1.0, "#a8664a")])], weight: 3.0, ..document_proto::Stroke::default() };
        case.effects = vec![draw::drop_shadow("#000000c0", 0.0, 8.0, 20.0), draw::inner_shadow("#ffffff30", 0.0, 2.0, 2.0), draw::inner_shadow("#00000070", 0.0, -4.0, 10.0)];
        let case = b.add(root, case);
        for (x, y) in [(28.0, 31.0), (W - 28.0, 31.0), (28.0, BODY_H - 31.0), (W - 28.0, BODY_H - 31.0)] {
            screw(b, case, x, y);
        }
        // The mesh window in its copper surround.
        let (mx, my, mw, mh) = (51.0, 50.0, W - 102.0, 110.0);
        b.add(case, draw::with_effects(draw::rounded(draw::rect("surround", mx - 5.0, my - 5.0, mw + 10.0, mh + 10.0, draw::linear(180.0, &[(0.0, "#f2c7a6"), (0.5, "#c98a66"), (1.0, "#f0c09c")])), 24.0), vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)]));
        let mesh = Place { options: Some(vec![format!("w={mw}"), format!("h={mh}"), "r=20".into()]), ..Place::new("Mesh").variant("mesh") };
        ui::put_at(b, case, "panel.amp_grille", &mesh, mx, my);

        // The name and the two sections.
        print_left(b, case, "SPRING TANK", 40.0, 212.0, 40.0, INK, 600.0, 0.08);
        print_left(b, case, "Harmonic Tremolo", 44.0, 262.0, 16.0, INK, 500.0, 0.03);
        print_right(b, case, "Spring Reverb", 1160.0, 262.0, 16.0, INK, 500.0, 0.03);
        bracket(b, case, 45.0, 576.0, 290.0, &[45.0, 266.0, 381.0, 576.0]);
        bracket(b, case, 715.0, 1160.0, 290.0, &[715.0, 830.0, 942.0, 1160.0]);

        let row = 356.0;
        let label_y = 420.0;
        // Tremolo.
        print(b, case, "ON", 102.0, 322.0, 12.0, INK, 500.0, 0.08);
        toggle(b, case, "tank/trem_on", "Tremolo", 24.0, 102.0, row);
        print(b, case, "OFF", 102.0, 394.0, 12.0, INK, 500.0, 0.08);
        let lamp = ui::stream_lamp(b, case, "Tremolo lamp", "tank/lfo", "", 171.0, row, 30.0, "#3cf0a0", INK);
        let glow = b.controls.get(&lamp).and_then(|c| c.slots_of(SlotRole::Glow).next()).map(NodeId);
        if let Some(glow) = glow {
            motion::add(b, glow, Motion::new(Target::None).named("lfo").dynamics(Dynamics::Lfo { rate: Num::Expr("param(tank/speed)".into()), shape: Wave::Sine, phase: 0.0 }));
            motion::add(b, glow, Motion::new(Target::Opacity).input("or(scalar(tank/lfo), motion(lfo))").dynamics(Dynamics::spring(900.0, 50.0)));
        }
        for (addr, name, cx) in [("tank/intensity", "INTENSITY", 266.0), ("tank/speed", "SPEED", 381.0)] {
            knob(b, case, "knob.cream_cap", "cream", addr, name, 84.0, cx, row);
            print(b, case, name, cx, label_y, 12.0, INK, 500.0, 0.12);
        }
        let disp = Place::new("Division").size(72.0).bind(ui::bind("tank/division")).options(&["color=#b8c8ff", "digits=3", "h=38"]).format(ValueFormat::default());
        ui::put(b, case, "meter.segment_display", &disp, 493.0, row);
        b.add(case, draw::rect("dash", 535.0, row - 0.8, 14.0, 1.6, draw::solid(DIM)));
        print(b, case, "SYNC", 577.0, 318.0, 12.0, INK, 500.0, 0.12);
        toggle(b, case, "tank/sync", "Sync", 24.0, 577.0, row);
        print(b, case, "FREE", 577.0, 394.0, 12.0, INK, 500.0, 0.12);
        // Reverb.
        for (addr, name, cx) in [("tank/mix", "MIX", 714.0), ("tank/tone", "TONE", 830.0), ("tank/decay", "DECAY", 942.0)] {
            knob(b, case, "knob.cream_cap", "cream", addr, name, 84.0, cx, row);
            print(b, case, name, cx, label_y, 12.0, INK, 500.0, 0.12);
        }
        let jewel = Place::new("Reverb lamp").size(30.0).variant("green").bind(ui::bind("tank/verb_on")).legend("").touch(surface_kit::TOUCH_MIN);
        let j = ui::put(b, case, "button.jewel", &jewel, 1033.0, row);
        ui::strip_legend(b, j);
        print(b, case, "ON", 1101.0, 322.0, 12.0, INK, 500.0, 0.08);
        toggle(b, case, "tank/verb_on", "Reverb", 24.0, 1101.0, row);
        print(b, case, "OFF", 1101.0, 394.0, 12.0, INK, 500.0, 0.08);
    });
    let divisions = ["1/1", "1/2", "1/4", "1/4D", "1/4T", "1/8", "1/8D", "1/8T", "1/16", "1/16D", "1/16T"];
    let params = vec![
        ui::toggle("tank/trem_on", "Tremolo", true).featured(),
        ui::percent("tank/intensity", "Intensity", 50.0).featured(),
        ui::param("tank/speed", "Speed", Range::log(0.5, 12.0, Unit::Hz), 4.0).featured(),
        ui::choice("tank/division", "Division", &divisions, 6),
        ui::choice("tank/sync", "Sync", &["Free", "Sync"], 1),
        ui::percent("tank/mix", "Mix", 50.0).featured(),
        ui::percent("tank/tone", "Tone", 50.0),
        ui::percent("tank/decay", "Decay", 50.0),
        ui::toggle("tank/verb_on", "Reverb", true).featured(),
    ];
    Face { file: "10-tabletop-spring-tank", plugin: PLUGIN, scene, params, streams: vec![ui::lfo_stream("tank/lfo", "Tremolo LFO")] }
}
