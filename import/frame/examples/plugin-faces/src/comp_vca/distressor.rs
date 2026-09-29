//! The digitally-controlled analogue hybrid, laid out from the unit.
//!
//! Two bordered sections, as its panel has. Up top, everything that
//! *reports*: the gain-reduction ladder (lamps that light right to left,
//! 1 dB first, driven by the gain-reduction stream), the ratio row (its
//! lamps are the switch — click one to pick it; NUKE at the end), and the
//! detector and audio-mode rows. Below, the four big dials with their
//! legends printed *above* them: the numerals are on each dial's brushed
//! skirt and turn past a fixed index, so the panel around them is bare.
//! Readouts under the dials give the time constants and gains the 0–10
//! skirts stand for.

use document_proto::NodeId;
use surface_kit::Place;
use surface_widgets::Builder;

use super::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 940.0;
const H: f64 = 360.0;
const INK: &str = "#e8eaec";
const DIM: &str = "#9aa1a8";
const LINE: &str = "#e8eaec73";

/// Gain-reduction lamps, left to right: deepest first, 1 dB at the right.
const LADDER: &[f64] = &[26.0, 23.0, 20.0, 17.0, 14.0, 12.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
const RATIOS: &[&str] = &["1:1", "2:1", "3:1", "4:1", "6:1", "10:1", "20:1", "Nuke"];
const DIAL_ROW: f64 = 268.0;
const DIAL: f64 = 80.0;

/// A big dial with its legend printed above it and a readout below.
fn dial(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, cx: f64) {
    ui::silk(b, panel, legend, cx, 196.0, 9.5, INK, 800.0);
    let p = Place::new(legend).size(DIAL).variant("black").bind(bind(addr));
    let k = ui::put(b, panel, "knob.dial", &p, cx, DIAL_ROW);
    ui::strip_legend(b, k);
    ui::add_value(b, k, 8.0, DIM);
}

pub fn face() -> Face {
    let scene = canvas("Distressor", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "black", W, H, "ears");

        // ── Upper section: what the unit reports ───────────────────────
        ui::section(b, panel, 40.0, 16.0, 860.0, 156.0, LINE, 1.2);
        ui::silk_left(b, panel, "Hybrid Compressor", 60.0, 46.0, 13.0, INK, 800.0);
        ui::silk_left(b, panel, "Model 8 · Digital control, analog path", 60.0, 64.0, 6.5, DIM, 600.0);

        let inb = Place::new("In").size(30.0).variant("grey").legend("In").bind(bind("distressor/in"));
        ui::put(b, panel, "button.console", &inb, 96.0, 124.0);

        // The ladder: 16 lamps, printed with the dB each stands for.
        let pitch = 25.0;
        let x0 = 522.0 - pitch * LADDER.len() as f64 / 2.0;
        ui::gr_ladder(b, panel, "Gain Reduction", "distressor/gr", x0, 30.0, pitch, 9.0, LADDER, "#ff4a36");
        for (i, db) in LADDER.iter().enumerate() {
            ui::plain(b, panel, &format!("{db}"), x0 + pitch * (i as f64 + 0.5), 55.0, 7.0, DIM, 600.0);
        }
        ui::silk(b, panel, "Gain Reduction  dB", 522.0, 72.0, 7.0, INK, 700.0);

        // Ratio: the lamps are the switch.
        let ratio = Place::new("Ratio").size(48.0).variant("amber").bind(bind("distressor/ratio")).options(RATIOS);
        ui::put(b, panel, "button.led_bank", &ratio, 522.0, 118.0);
        ui::silk(b, panel, "Ratio", 522.0, 150.0, 8.0, INK, 800.0);

        // Detector and audio modes, one lamp per mode.
        ui::rule(b, panel, 736.0, 30.0, 1.0, 128.0, LINE);
        let det = Place::new("Detector").size(42.0).variant("green").bind(bind("distressor/detector")).options(&["Off", "HP", "HP+"]);
        ui::put(b, panel, "button.led_bank", &det, 818.0, 44.0);
        ui::silk(b, panel, "Detector", 818.0, 76.0, 7.5, INK, 800.0);
        let audio = Place::new("Audio").size(42.0).variant("red").bind(bind("distressor/audio")).options(&["Clean", "Dist 2", "Dist 3"]);
        ui::put(b, panel, "button.led_bank", &audio, 818.0, 118.0);
        ui::silk(b, panel, "Audio", 818.0, 150.0, 7.5, INK, 800.0);

        // ── Lower section: the dials ───────────────────────────────────
        ui::section(b, panel, 40.0, 180.0, 860.0, 164.0, LINE, 1.2);
        dial(b, panel, "distressor/input", "Input", 146.0);
        dial(b, panel, "distressor/attack", "Attack", 306.0);
        dial(b, panel, "distressor/release", "Release", 466.0);
        dial(b, panel, "distressor/output", "Output", 626.0);

        ui::rule(b, panel, 716.0, 194.0, 1.0, 136.0, LINE);
        ui::silk(b, panel, "Mix", 808.0, 196.0, 9.5, INK, 800.0);
        let mix = Place::new("Mix").size(48.0).variant("black").bind(bind("distressor/mix")).scale(&["Dry", "", "", "", "Comp"]);
        let mix = ui::put(b, panel, "knob.pointer", &mix, 808.0, DIAL_ROW - 4.0);
        ui::strip_legend(b, mix);
        ui::add_value(b, mix, 8.0, DIM);
    });

    let params = vec![
        ui::param("distressor/input", "Input", surface_proto::Range::linear(0.0, 10.0, surface_proto::Unit::None), 5.0).featured(),
        ui::ms("distressor/attack", "Attack", 0.05, 50.0, 5.0).featured(),
        ui::ms("distressor/release", "Release", 50.0, 3500.0, 400.0).featured(),
        ui::db("distressor/output", "Output", -12.0, 24.0, 0.0).featured(),
        ui::choice("distressor/ratio", "Ratio", RATIOS, 3).featured(),
        ui::choice("distressor/detector", "Detector", &["Off", "HP", "HP+"], 0),
        ui::choice("distressor/audio", "Audio", &["Clean", "Dist 2", "Dist 3"], 0),
        ui::percent("distressor/mix", "Mix", 100.0),
        ui::toggle("distressor/in", "In", true),
    ];
    let streams = vec![ui::gr_stream("distressor/gr", "Gain Reduction")];
    Face { file: "10-distressor", plugin: "comp", scene, params, streams }
}
