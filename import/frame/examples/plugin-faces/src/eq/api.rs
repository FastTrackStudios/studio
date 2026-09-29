//! The three-band proportional-Q module (550A style), on a grey rack panel.
//!
//! Three bands of **stepped** frequencies — four low, five mid, five high —
//! each a detented skirted selector turning exactly through its positions,
//! over a gain knob that clicks in 2 dB steps from -12 to +12. The coarseness
//! is the design: a console EQ you set rather than dial. The panel is light,
//! so every scale and legend is printed in the panel's dark ink.

use document_proto::NodeId;
use surface_kit::{Ink, Place, TOUCH_MIN, detent_sweep};
use surface_widgets::Builder;

use crate::comp_vca::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 900.0;
const H: f64 = 331.0;
const INK: &str = "#1f2226";
const DIM: &str = "#4a5057";
const FREQ_ROW: f64 = 132.0;
const GAIN_ROW: f64 = 256.0;
const BANDS: [f64; 3] = [180.0, 380.0, 580.0];

const LOW: &[&str] = &["50", "100", "200", "400"];
const MID: &[&str] = &[".4", ".8", "1.5", "3", "5"];
const HIGH: &[&str] = &["5", "7", "10", "12.5", "15"];
const GAIN: &[&str] = &["-12", "-8", "-4", "0", "+4", "+8", "+12"];

/// A detented frequency selector: skirted knob stepped through exactly its
/// positions. The knob prints the ticks; the legends are set here, clear
/// of them and big enough to read.
fn selector(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, labels: &[&str], cx: f64) {
    let ticks = vec![""; labels.len()];
    let p = Place::new(legend).size(52.0).variant("black").legend(legend).keep_case().options(labels).scale(&ticks).ink(Ink::Dark).touch(TOUCH_MIN).bind(bind(addr));
    ui::put(b, panel, "knob.skirted", &p, cx, FREQ_ROW);
    ui::scale_labels(b, panel, cx, FREQ_ROW, 43.0, labels, detent_sweep(labels.len()), 9.0, INK);
}

/// A knob with its scale and legend printed in dark ink.
#[allow(clippy::too_many_arguments)]
fn knob(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, d: f64, labels: &[&str], cx: f64, cy: f64) {
    let ticks = vec![""; labels.len()];
    let p = Place::new(legend).size(d).variant("black").scale(&ticks).ink(Ink::Dark).touch(TOUCH_MIN).bind(bind(addr));
    let k = ui::put(b, panel, "knob.bakelite", &p, cx, cy);
    // The legend goes under the printed scale, not inside it.
    ui::strip_legend(b, k);
    ui::silk(b, panel, legend, cx, cy + d / 2.0 + 26.0, 8.5, INK, 800.0);
    ui::scale_labels(b, panel, cx, cy, d * 0.82 + 2.0, labels, 300.0, 8.0, INK);
}

pub fn face() -> Face {
    let scene = canvas("API 550A", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "api", W, H, "ears");

        ui::silk_left(b, panel, "Discrete Equalizer", 46.0, 34.0, 12.0, INK, 800.0);
        ui::silk_left(b, panel, "Model 550A  ·  Proportional Q", 46.0, 52.0, 7.5, DIM, 700.0);

        // ── The three bands: stepped frequency over stepped gain ───────
        let units = ["Hz", "kHz", "kHz"];
        for (i, (addr, labels)) in [("api550/low_freq", LOW), ("api550/mid_freq", MID), ("api550/high_freq", HIGH)].into_iter().enumerate() {
            selector(b, panel, addr, units[i], labels, BANDS[i]);
        }
        for (i, (addr, legend)) in [("api550/low_gain", "Low"), ("api550/mid_gain", "Mid"), ("api550/high_gain", "High")].into_iter().enumerate() {
            knob(b, panel, addr, legend, 50.0, GAIN, BANDS[i], GAIN_ROW);
        }
        for x in [280.0, 480.0, 680.0] {
            ui::rule(b, panel, x, 76.0, 1.0, 232.0, "#00000030");
            ui::rule(b, panel, x + 1.0, 76.0, 1.0, 232.0, "#ffffff40");
        }

        // ── In, drive and trim ─────────────────────────────────────────
        let inb = Place::new("EQ In").size(36.0).variant("black").legend("In").bind(bind("api550/eq_in"));
        ui::put(b, panel, "button.console", &inb, 780.0, FREQ_ROW - 8.0);
        ui::silk(b, panel, "EQ", 780.0, FREQ_ROW - 44.0, 8.5, INK, 800.0);
        knob(b, panel, "api550/drive", "Drive", 36.0, &["0", "", "", "", "10"], 742.0, GAIN_ROW);
        knob(b, panel, "api550/trim", "Trim", 36.0, &["-24", "", "0", "", "+24"], 826.0, GAIN_ROW);
    });

    let params = vec![
        ui::choice("api550/low_freq", "Low Freq", &["50 Hz", "100 Hz", "200 Hz", "400 Hz"], 1),
        ui::db_stepped("api550/low_gain", "Low Gain", -12.0, 12.0, 2.0, 0.0).featured(),
        ui::choice("api550/mid_freq", "Mid Freq", &["400 Hz", "800 Hz", "1.5 kHz", "3 kHz", "5 kHz"], 2),
        ui::db_stepped("api550/mid_gain", "Mid Gain", -12.0, 12.0, 2.0, 0.0).featured(),
        ui::choice("api550/high_freq", "High Freq", &["5 kHz", "7 kHz", "10 kHz", "12.5 kHz", "15 kHz"], 2),
        ui::db_stepped("api550/high_gain", "High Gain", -12.0, 12.0, 2.0, 0.0).featured(),
        ui::toggle("api550/eq_in", "EQ In", true).featured(),
        ui::percent("api550/drive", "Drive", 0.0),
        ui::db("api550/trim", "Trim", -24.0, 24.0, 0.0),
    ];
    Face { file: "03-api-550a", plugin: "eq", scene, params, streams: Vec::new() }
}
