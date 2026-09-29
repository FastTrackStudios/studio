//! The tape machine: the electronics of a two-track mastering deck —
//! what you drive tape saturation with.
//!
//! A meter bridge across the top: two ivory VUs, left and right, reading
//! the level going to tape. Either side of it the machine's two switches —
//! SPEED (7½, 15, 30 in/s; slower is more head bump and less top) and the
//! tape FORMULATION (standard or hot, the family's two profiles). Below, the
//! record electronics: RECORD LEVEL (drive into the tape), BIAS (under- or
//! over-biased from the calibrated point), EMPHASIS, COMPRESSION, and MIX and
//! OUTPUT last.

use document_proto::NodeId;
use surface_kit::{Place, draw};
use surface_proto::{Binding, EntryId, Source};
use surface_widgets::Builder;

use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 330.0;
const INK: &str = "#efe6d4";
const DIM: &str = "#b3a58f";
const SPEEDS: &[&str] = &["7½", "15", "30"];
const SPEED_NAMES: &[&str] = &["7.5 in/s", "15 in/s", "30 in/s"];
const FORMULATIONS: &[&str] = &["Standard", "Hot"];

fn a(s: &str) -> String {
    format!("tape_sat/{s}")
}

/// One VU on the bridge, reading channel `ch` of the output.
fn vu(b: &mut Builder<'_>, bridge: NodeId, ch: u8, cx: f64, legend: &str) {
    let binding = Binding { source: Source::Stream { id: EntryId::new(a("out")), channel: Some(ch) }, ..Binding::stream(a("out")) };
    let p = Place::new(format!("VU {legend}")).size(182.0).variant("black_rack").legend("VU").bind(binding);
    let vu = parts::put(b, bridge, "meter.vu_ivory", &p, cx, 66.0);
    parts::glass(b, vu);
    parts::silk(b, bridge, legend, cx, 136.0, Ink::new(7.0, DIM).weight(800.0).track(0.3));
}

pub fn face() -> Face {
    let scene = canvas("Tape Machine", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.brushed", "black", W, H, "ears");
        parts::repaint(b, panel, &[(0.0, "#4d453c"), (0.5, "#3b342d"), (1.0, "#2a2520")]);

        // ── Nameplate, speed ───────────────────────────────────────────
        parts::silk_left(b, panel, "Tape Machine", 50.0, 34.0, Ink::new(15.0, INK).weight(800.0).track(0.06));
        parts::silk_left(b, panel, "Two-track · ¼ inch · record electronics", 50.0, 54.0, Ink::new(6.5, DIM).weight(600.0).track(0.14));
        let speed = parts::selector(b, panel, "Speed", &a("speed"), "black", (132.0, 128.0), 54.0, SPEEDS, INK);
        parts::legend_at(b, speed, "Speed in/s", 168.0, Ink::new(6.8, DIM).weight(800.0).track(0.2));

        // ── Meter bridge ───────────────────────────────────────────────
        let bridge = parts::well(b, panel, "Meter bridge", (246.0, 18.0, 468.0, 146.0), draw::linear(180.0, &[(0.0, "#1c1a17"), (1.0, "#121110")]), 6.0);
        vu(b, bridge, 0, 118.0, "Left");
        vu(b, bridge, 1, 350.0, "Right");
        parts::silk(b, bridge, "Level to tape", 234.0, 136.0, Ink::new(6.0, "#7d7466").weight(700.0).track(0.2));

        // ── Formulation ────────────────────────────────────────────────
        let tape = parts::selector(b, panel, "Tape", &a("formulation"), "black", (822.0, 128.0), 54.0, FORMULATIONS, INK);
        parts::legend_at(b, tape, "Formulation", 168.0, Ink::new(6.8, DIM).weight(800.0).track(0.2));
        parts::lamp(b, panel, (822.0, 40.0), 8.0, "#ff4a36", true);
        parts::silk(b, panel, "Record", 822.0, 56.0, Ink::new(6.0, DIM).weight(800.0).track(0.24));

        // ── Record electronics ─────────────────────────────────────────
        parts::groove(b, panel, 40.0, W - 40.0, 180.0);
        let row = 240.0;
        let legend = Ink::new(8.0, INK).weight(800.0).track(0.14);
        let value = Ink::new(7.5, DIM).weight(600.0).track(0.02).plain();
        let scale = Ink::new(6.5, DIM).weight(700.0).track(0.0).plain();
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)]); 6] = [
            ("drive", "Record Level", 116.0, 64.0, parts::PCT_MARKS),
            ("bias", "Bias", 252.0, 54.0, super::TRIM_MARKS),
            ("emphasis", "Emphasis", 380.0, 46.0, super::TRIM_MARKS),
            ("compression", "Compression", 496.0, 46.0, super::TRIM_MARKS),
            ("mix", "Mix", 690.0, 50.0, parts::PCT_MARKS),
            ("output", "Output", 830.0, 56.0, super::OUT_MARKS),
        ];
        for (addr, name, x, d, marks) in knobs {
            let k = parts::knob(b, panel, "knob.skirted", "black", &a(addr), name, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 5.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, 294.0, legend);
            parts::value_at(b, k, 309.0, value);
        }
        parts::vgroove(b, panel, 600.0, 196.0, 312.0);
    });

    let params = vec![
        parts::percent(&a("drive"), "Record Level", 30.0).featured(),
        parts::trim(&a("bias"), "Bias").featured(),
        parts::trim(&a("emphasis"), "Emphasis"),
        parts::trim(&a("compression"), "Compression"),
        parts::percent(&a("mix"), "Mix", 100.0).featured(),
        super::output(&a("output")),
        parts::choice(&a("speed"), "Speed", SPEED_NAMES, 1).featured(),
        parts::choice(&a("formulation"), "Formulation", FORMULATIONS, 0),
    ];
    let streams = vec![parts::level_stream(&a("out"), "Output Level", 2)];
    Face { file: "02-tape", plugin: "saturate", scene, params, streams }
}
