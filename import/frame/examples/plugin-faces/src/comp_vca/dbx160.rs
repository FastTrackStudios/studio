//! The desktop VCA compressor of the 160's generation: three knobs and a
//! meter, in walnut cheeks.
//!
//! THRESHOLD (printed in the unit's input volts, 10 mV → 3 V) with its
//! BELOW / ABOVE lamps — a two-lamp ladder on the gain-reduction stream:
//! dark with no reduction, BELOW lit as the signal reaches threshold (half
//! a dB of reduction), ABOVE once it is over (1 dB) — COMPRESSION from 1:1 to ∞:1 on the unit's own
//! uneven scale, and OUTPUT GAIN ±20 dB. The meter is an amber card printed
//! in blue with a plain decibel scale, fed by whichever signal the METER
//! buttons pick (input, output or gain change), as the unit's is.

use document_proto::NodeId;
use surface_kit::{Ink, Place, TOUCH_MIN};
use surface_proto::{Binding, Unit, ValueFormat};
use surface_widgets::Builder;

use super::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 940.0;
const H: f64 = 300.0;
const INK: &str = "#eceef0";
const DIM: &str = "#9aa0a6";
const ROW: f64 = 150.0;

/// Threshold as printed: the input level (volts) where compression starts,
/// evenly spaced in dB — 10 mV at the stop is -38 dBu, 3 V is +12 dBu.
const THRESHOLDS: &[&str] = &["10mV", "30mV", "100mV", ".3V", "1V", "3V"];
/// The compression dial's own uneven printing, 1:1 to limiting.
const RATIOS: &[&str] = &["1", "1.5", "2", "3", "4", "6", "10", "20", "∞"];
/// The real ratio at each printed mark; ∞ stands in as 60:1.
const RATIO_VALUES: &[f64] = &[1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 10.0, 20.0, 60.0];

/// A section header printed over a control, the way the unit titles them.
fn header(b: &mut Builder<'_>, panel: NodeId, lines: &[&str], cx: f64) {
    let n = lines.len() as f64;
    for (i, l) in lines.iter().enumerate() {
        ui::silk(b, panel, l, cx, 46.0 - (n - 1.0 - i as f64) * 15.0, 9.5, INK, 800.0);
    }
}

/// A fluted aluminium knob with its scale and no legend: the header above
/// names it.
fn knob(b: &mut Builder<'_>, panel: NodeId, addr: &str, name: &str, size: f64, scale: &[&str], cx: f64) -> NodeId {
    let ticks = vec![""; scale.len()];
    let p = Place::new(name).size(size).variant("silver").scale(&ticks).ink(Ink::Color(INK.into())).touch(TOUCH_MIN).format(ValueFormat::decimals(1)).bind(bind(addr));
    let k = ui::put(b, panel, "knob.metal_fluted", &p, cx, ROW);
    ui::strip_legend(b, k);
    ui::scale_labels(b, panel, cx, ROW, size / 2.0 + 19.0, scale, 300.0, 8.0, INK);
    k
}

pub fn face() -> Face {
    let scene = canvas("dbx 160 Compressor", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.desktop", "black", W, H, "wood");

        // ── Threshold, with its below/above lamps ──────────────────────
        header(b, panel, &["Threshold"], 180.0);
        let lamps = Place::new("Threshold lamps").size(84.0).variant("red").scale(&["0.5", "1"]).options(&["gr"]).bind(Binding::stream("dbx160/gr"));
        let lamps = ui::put(b, panel, "meter.led_ladder", &lamps, 180.0, 80.0);
        // The unit prints what the lamps mean, not their thresholds.
        for id in b.scene.walk(lamps) {
            if let Some(n) = b.scene.node_mut(id)
                && n.name == "threshold"
            {
                n.visible = false;
            }
        }
        ui::silk_right(b, panel, "Below", 150.0, 74.0, 7.0, DIM, 700.0);
        ui::silk_left(b, panel, "Above", 210.0, 74.0, 7.0, DIM, 700.0);
        let thr = knob(b, panel, "dbx160/threshold", "Threshold", 60.0, THRESHOLDS, 180.0);
        ui::add_value(b, thr, 8.0, DIM);

        // ── Compression ────────────────────────────────────────────────
        header(b, panel, &["Compression"], 380.0);
        ui::silk(b, panel, "Ratio", 380.0, 74.0, 7.0, DIM, 700.0);
        knob(b, panel, "dbx160/ratio", "Compression", 68.0, RATIOS, 380.0);

        // ── Output gain ────────────────────────────────────────────────
        header(b, panel, &["Output", "Gain"], 580.0);
        let out = knob(b, panel, "dbx160/output", "Output Gain", 60.0, &["-20", "-10", "0", "+10", "+20"], 580.0);
        ui::add_value(b, out, 8.0, DIM);

        // ── The switching row: power pilot and the meter source ────────
        ui::lamp(b, panel, 180.0, 246.0, 10.0, "#e0463a", true);
        ui::silk(b, panel, "Power", 180.0, 268.0, 7.5, DIM, 700.0);
        let meter_sel = Place::new("Meter").size(30.0).variant("white").legend("Meter").bind(bind("dbx160/meter")).options(&["In", "Out", "Gain"]);
        ui::put(b, panel, "button.square_bank", &meter_sel, 480.0, 244.0);
        ui::silk(b, panel, "Meter", 480.0, 276.0, 7.5, DIM, 700.0);

        // A seam between the controls and the movement.
        ui::rule(b, panel, 668.0, 30.0, 1.0, 240.0, "#00000080");
        ui::rule(b, panel, 669.0, 30.0, 1.0, 240.0, "#ffffff12");

        // ── The movement and the panel's mark ──────────────────────────
        let meter = Place::new("Decibels").size(200.0).legend("dB").options(&["glass"]).bind(Binding::stream("dbx160/meter_level"));
        ui::put(b, panel, "meter.vu_amber_blue", &meter, 790.0, 118.0);
        ui::plain(b, panel, "160", 790.0, 226.0, 22.0, INK, 800.0);
        ui::silk(b, panel, "Compressor / Limiter", 790.0, 254.0, 7.5, DIM, 700.0);
    });

    let params = vec![
        ui::db("dbx160/threshold", "Threshold", -40.0, 0.0, -20.0).featured(),
        ui::param("dbx160/ratio", "Compression", ui::printed(RATIO_VALUES, Unit::Ratio), 4.0).featured(),
        ui::db("dbx160/output", "Output Gain", -20.0, 20.0, 0.0).featured(),
        ui::choice("dbx160/meter", "Meter", &["Input", "Output", "Gain Change"], 2),
    ];
    let streams = vec![ui::gr_stream("dbx160/gr", "Gain Reduction"), ui::level_stream("dbx160/meter_level", "Meter (METER select)", 1)];
    Face { file: "09-dbx-160", plugin: "comp", scene, params, streams }
}
