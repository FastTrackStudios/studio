//! The passive program equalizer (EQP-1A), laid out from its panel.
//!
//! Across the top, the two bands' BOOST and ATTEN — separate controls on
//! the *same* low band, which is the only reason the famous boost-and-cut
//! trick exists — each Daka-Ware knob printed 0–10, with a window above it
//! reading the dB that position means. Below, the two frequency *levers*
//! (four and seven positions, read at a glance), BANDWIDTH between them from
//! SHARP to BROAD, the high-cut frequency selector, the IN/OUT toggle with
//! its pilot jewel, and the output.

use surface_kit::{Place, TOUCH_MIN};
use surface_proto::{Binding, ValueFormat};

use crate::comp_vca::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 900.0;
const H: f64 = 331.0;
const INK: &str = "#eef3f6";
const DIM: &str = "#b6c8d3";
const TOP: f64 = 106.0;
const BOTTOM: f64 = 244.0;
const KNOB: f64 = 76.0;
const READ: &str = "#ffcf7a";

const ZERO_TEN: &[&str] = &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];
const LOW_FREQS: &[&str] = &["20", "30", "60", "100"];
const HIGH_FREQS: &[&str] = &["3", "4", "5", "8", "10", "12", "16"];

/// A frequency lever with its band title above and its unit below.
#[allow(clippy::too_many_arguments)]
fn lever(b: &mut surface_widgets::Builder<'_>, panel: document_proto::NodeId, addr: &str, title: &str, unit: &str, options: &[&str], size: f64, cx: f64) {
    ui::silk(b, panel, title, cx, 198.0, 8.5, INK, 800.0);
    let p = Place::new(title).size(size).variant("light").touch(TOUCH_MIN).bind(bind(addr)).options(options);
    let l = ui::put(b, panel, "lever.pultec", &p, cx, BOTTOM + 14.0);
    ui::strip_legend(b, l);
    ui::silk(b, panel, unit, cx, 314.0, 7.5, DIM, 700.0);
}

#[allow(clippy::too_many_arguments)]
fn daka(b: &mut surface_widgets::Builder<'_>, panel: document_proto::NodeId, addr: &str, legend: &str, cx: f64, cy: f64, d: f64, window: bool) {
    let p = Place::new(legend).size(d).legend(legend).bind(bind(addr)).scale(ZERO_TEN);
    ui::put(b, panel, "knob.daka", &p, cx, cy);
    if window {
        ui::readout(b, panel, &format!("{legend} dB"), Binding::param(addr), cx, cy - d / 2.0 - 36.0, 58.0, 18.0, READ, ValueFormat::decimals(1));
    }
}

pub fn face() -> Face {
    let scene = canvas("Pultec EQP-1A", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "pultec", W, H, "ears");

        // ── Top row: boost and atten, low band then high ───────────────
        daka(b, panel, "pultec/low_boost", "Boost", 196.0, TOP, KNOB, true);
        daka(b, panel, "pultec/low_atten", "Atten", 318.0, TOP, KNOB, true);
        daka(b, panel, "pultec/high_boost", "Boost", 520.0, TOP, KNOB, true);
        daka(b, panel, "pultec/high_atten", "Atten", 642.0, TOP, KNOB, true);
        ui::rule(b, panel, 419.0, 34.0, 1.0, 132.0, "#00000040");
        ui::rule(b, panel, 420.0, 34.0, 1.0, 132.0, "#ffffff1c");

        // The nameplate, top left, where the top row leaves room.
        ui::silk_left(b, panel, "Program", 44.0, 40.0, 9.5, INK, 800.0);
        ui::silk_left(b, panel, "Equalizer", 44.0, 55.0, 9.5, INK, 800.0);
        ui::silk_left(b, panel, "EQP-1A", 44.0, 74.0, 7.5, DIM, 700.0);

        // The high-cut frequency: a small selector at the top right.
        ui::silk(b, panel, "Atten Sel", 792.0, 40.0, 7.5, INK, 800.0);
        let sel = Place::new("Atten Sel").size(40.0).legend("KCS").touch(TOUCH_MIN).bind(bind("pultec/high_atten_freq")).options(&["5", "10", "20"]);
        ui::put(b, panel, "switch.rotary", &sel, 792.0, TOP - 14.0);

        // ── Bottom row: the levers and bandwidth ──────────────────────
        lever(b, panel, "pultec/low_freq", "Low Frequency", "CPS", LOW_FREQS, 104.0, 226.0);
        daka(b, panel, "pultec/bandwidth", "Bandwidth", 419.0, BOTTOM, 64.0, false);
        ui::silk(b, panel, "Sharp", 368.0, BOTTOM + 36.0, 7.0, DIM, 700.0);
        ui::silk(b, panel, "Broad", 470.0, BOTTOM + 36.0, 7.0, DIM, 700.0);
        lever(b, panel, "pultec/high_boost_freq", "High Frequency", "KCS", HIGH_FREQS, 118.0, 606.0);

        // ── In/out with its pilot jewel, and the output ────────────────
        let jewel = Place::new("Pilot").size(22.0).variant("red").bind(bind("pultec/eq_in"));
        ui::put(b, panel, "button.jewel", &jewel, 718.0, 196.0);
        let sw = Place::new("EQ").size(40.0).legend("EQ").bind(bind("pultec/eq_in")).options(&["In", "Out"]);
        ui::put(b, panel, "switch.toggle_bat", &sw, 712.0, BOTTOM + 24.0);

        ui::readout(b, panel, "Output dB", Binding::param("pultec/trim"), 800.0, 190.0, 58.0, 18.0, READ, ValueFormat::decimals(1));
        // The knob prints the ticks; the dB legends are set clear of them.
        let out = Place::new("Output").size(44.0).legend("Output").touch(TOUCH_MIN).bind(bind("pultec/trim")).scale(&[""; 5]);
        ui::put(b, panel, "knob.pointer", &out, 800.0, BOTTOM + 10.0);
        ui::scale_labels(b, panel, 800.0, BOTTOM + 10.0, 42.0, &["-24", "-12", "0", "+12", "+24"], 300.0, 7.0, INK);
    });

    let params = vec![
        ui::db("pultec/low_boost", "Low Boost", 0.0, 13.0, 0.0).featured(),
        ui::db("pultec/low_atten", "Low Atten", 0.0, 13.0, 0.0).featured(),
        ui::choice("pultec/low_freq", "Low Frequency", &["20 Hz", "30 Hz", "60 Hz", "100 Hz"], 1).featured(),
        ui::db("pultec/high_boost", "High Boost", 0.0, 16.0, 0.0).featured(),
        ui::choice("pultec/high_boost_freq", "High Frequency", &["3 kHz", "4 kHz", "5 kHz", "8 kHz", "10 kHz", "12 kHz", "16 kHz"], 3),
        ui::param("pultec/bandwidth", "Bandwidth", surface_proto::Range::linear(0.0, 10.0, surface_proto::Unit::None), 5.0),
        ui::db("pultec/high_atten", "High Atten", 0.0, 16.0, 0.0),
        ui::choice("pultec/high_atten_freq", "Atten Sel", &["5 kHz", "10 kHz", "20 kHz"], 1),
        ui::db("pultec/trim", "Output", -24.0, 24.0, 0.0),
        ui::toggle("pultec/eq_in", "EQ In", true).featured(),
    ];
    Face { file: "01-pultec-eqp1a", plugin: "eq", scene, params, streams: Vec::new() }
}
