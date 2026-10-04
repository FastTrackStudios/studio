//! The desktop VCA compressor — laid out from photographs of the original
//! dbx 160 (1976): a gloss-black half-rack front between walnut cheeks,
//! with brushed-aluminium trim along its top and bottom edges.
//!
//! Left to right as the unit has them: THRESHOLD (printed in the unit's
//! input volts, .01 → 3 V) with its BELOW / ABOVE lamps — a two-lamp ladder
//! on the gain-reduction stream: dark with no reduction, BELOW lit as the
//! signal reaches threshold (half a dB), ABOVE once it is over (1 dB) —
//! COMPRESSION from 1:1 to ∞:1 on the unit's own uneven scale (the biggest
//! knob), and OUTPUT GAIN ±20 dB. The knobs are black and knurled with a
//! polished aluminium insert. Under them, the white POWER key and the
//! interlocked METER keys (input, output, gain change). The big meter at the
//! right is a pale card printed in blue with a plain decibel scale, in a
//! deep bevelled frame, fed by whichever signal the METER keys pick.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Ink, Place, TOUCH_MIN, draw};
use surface_proto::{Binding, Unit};
use surface_widgets::Builder;

use super::logo;
use super::ui::{self, bind};
use crate::face::{Face, canvas};

pub mod lane;

const W: f64 = 860.0;
const H: f64 = 360.0;
/// The inside edge of the walnut cheek, and the width between the cheeks.
const X0: f64 = 33.0;
const IW: f64 = 794.0;
const INK: &str = "#f0f1f2";
const ROW: f64 = 160.0;

/// A position across the front, as a fraction of the width between the
/// cheeks (measured off the photograph).
fn fx(f: f64) -> f64 {
    X0 + IW * f
}

/// Threshold as printed: the input level (volts) where compression starts,
/// evenly spaced in dB — .01 V at the stop is -38 dBu, 3 V is +12 dBu.
const THRESHOLDS: &[&str] = &[".01", ".03", ".1", ".3", "1", "3"];
/// The compression dial's own uneven printing, 1:1 to limiting.
const RATIOS: &[&str] = &["1", "1.5", "2", "3", "4", "6", "10", "20", "∞"];
/// The real ratio at each printed mark; ∞ stands in as 60:1.
const RATIO_VALUES: &[f64] = &[1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 10.0, 20.0, 60.0];

/// A header printed over a control, bold white capitals.
fn header(b: &mut Builder<'_>, panel: NodeId, lines: &[&str], cx: f64, bottom: f64) {
    let n = lines.len() as f64;
    for (i, l) in lines.iter().enumerate() {
        ui::silk(b, panel, l, cx, bottom - (n - 1.0 - i as f64) * 15.0, 12.0, INK, 800.0);
    }
}

/// A knurled black knob with a tick ring and the unit's numerals round it.
fn knob(b: &mut Builder<'_>, panel: NodeId, addr: &str, name: &str, size: f64, scale: &[&str], cx: f64) -> NodeId {
    let ticks = vec![""; scale.len()];
    let p = Place::new(name).size(size).scale(&ticks).ink(Ink::Color(INK.into())).touch(TOUCH_MIN).bind(bind(addr));
    let k = ui::put(b, panel, "knob.knurled_insert", &p, cx, ROW);
    ui::strip_legend(b, k);
    ui::scale_labels(b, panel, cx, ROW, size * 0.72 + 9.0, scale, 300.0, 10.0, INK);
    k
}

/// A brushed-aluminium trim strip across the front.
fn trim(b: &mut Builder<'_>, panel: NodeId, y: f64, h: f64) {
    let strip = draw::rect("trim", X0, y, IW, h, draw::linear(180.0, &[(0.0, "#f4f5f6"), (0.35, "#c9ccd0"), (0.7, "#a4a8ad"), (1.0, "#dcdfe2")]));
    b.add(panel, draw::with_effects(strip, vec![draw::inner_shadow("#00000066", 0.0, -1.0, 0.0), draw::inner_shadow("#ffffffaa", 0.0, 1.0, 0.0)]));
    // The groove the strip is rolled with.
    b.add(panel, draw::rect("trim groove", X0, y + h * 0.62, IW, 1.0, draw::solid("#00000033")));
}

/// The POWER key: a white piano key in a black housing (the unit's mains
/// switch — decoration here, as a plugin has no mains).
fn power_key(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, w: f64) {
    let h = w * 0.42;
    let pad = w * 0.07;
    let house = draw::rect("power housing", cx - w / 2.0 - pad, cy - h / 2.0 - pad, w + 2.0 * pad, h + 2.0 * pad, draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#16171a")]));
    b.add(panel, draw::with_effects(draw::rounded(house, 2.0), vec![draw::inner_shadow("#000000", 0.0, 1.0, 2.0), draw::drop_shadow("#ffffff1c", 0.0, 1.0, 0.0)]));
    let cap = draw::rect("power key", cx - w / 2.0, cy - h / 2.0, w, h, draw::linear(180.0, &[(0.0, "#fbfbfa"), (0.18, "#f1f1ee"), (0.8, "#e2e2de"), (1.0, "#bdbdb9")]));
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(draw::rounded(cap, 1.5), "#000000b0", 1.0),
            vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 2.5), draw::inner_shadow("#ffffff80", 0.0, 1.0, 0.0)],
        ),
    );
}

pub fn face() -> Face {
    let scene = canvas("VCA Compressor", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.desktop", "black", W, H, "wood");
        // Gloss black, not satin: a broad reflection across the upper half.
        b.add(panel, draw::rect("gloss", X0, 0.0, IW, H, draw::linear(170.0, &[(0.0, "#ffffff14"), (0.35, "#ffffff05"), (0.5, "#ffffff00"), (1.0, "#00000026")])));
        trim(b, panel, 0.0, 20.0);
        trim(b, panel, H - 30.0, 30.0);

        // ── Threshold, with its below/above lamps ──────────────────────
        let tx = fx(0.10);
        header(b, panel, &["Threshold"], tx, 56.0);
        let lamps = Place::new("Threshold lamps").size(52.0).scale(&["0.5", "1"]).options(&["gr", "colors=#f2c230,#e8483a"]).bind(Binding::stream("vcacomp/gr"));
        let lamps = ui::put(b, panel, "meter.led_ladder", &lamps, tx, 82.0);
        // The unit prints what the lamps mean, not their thresholds.
        for id in b.scene.walk(lamps) {
            if let Some(n) = b.scene.node_mut(id)
                && n.name == "threshold"
            {
                n.visible = false;
            }
        }
        ui::silk_right(b, panel, "Below", tx - 20.0, 76.0, 7.5, INK, 700.0);
        ui::silk_left(b, panel, "Above", tx + 20.0, 76.0, 7.5, INK, 700.0);
        knob(b, panel, "vcacomp/threshold", "Threshold", 70.0, THRESHOLDS, tx);

        // ── Compression: the biggest knob ──────────────────────────────
        let cx = fx(0.28);
        header(b, panel, &["Compression"], cx, 56.0);
        knob(b, panel, "vcacomp/ratio", "Compression", 80.0, RATIOS, cx);

        // ── Output gain ────────────────────────────────────────────────
        let ox = fx(0.49);
        header(b, panel, &["Output", "Gain"], ox, 64.0);
        knob(b, panel, "vcacomp/output", "Output Gain", 70.0, &["-20", "-10", "0", "+10", "+20"], ox);

        // ── The key row: POWER and the interlocked METER keys ──────────
        let py = 280.0;
        ui::silk(b, panel, "Power", fx(0.13), py - 24.0, 9.5, INK, 700.0);
        power_key(b, panel, fx(0.13), py, 62.0);
        let meter_sel =
            Place::new("Meter").size(52.0).variant("white").bind(bind("vcacomp/meter")).options(&["Input", "Output", "Gain Change"]).ink(Ink::Color(INK.into())).opt_legend(8.5);
        ui::put(b, panel, "button.piano_bank", &meter_sel, fx(0.40), py - 8.0);
        // The bracket under the row, METER set into it.
        let (bl, br, by) = (fx(0.40) - 84.0, fx(0.40) + 84.0, py + 26.0);
        ui::rule(b, panel, bl, by - 8.0, 1.5, 8.0, INK);
        ui::rule(b, panel, br - 1.5, by - 8.0, 1.5, 8.0, INK);
        ui::rule(b, panel, bl, by, 58.0, 1.5, INK);
        ui::rule(b, panel, br - 58.0, by, 58.0, 1.5, INK);
        ui::silk(b, panel, "Meter", fx(0.40), by + 1.0, 9.5, INK, 800.0);

        // ── The meter, and the name set as the unit's logotype ─────────
        let mx = fx(0.80);
        let meter = Place::new("Decibels").size(300.0).legend("DECIBELS").options(&["glass"]).bind(Binding::stream("vcacomp/meter_level"));
        ui::put(b, panel, "meter.vu_decibel_white", &meter, mx, 140.0);
        // Heavy, tightly set lowercase, a dark jewel-like dot, then a bold
        // word — in the logotype's off-white.
        let heavy = logo::Style { size: 40.0, weight: 900.0, italic: false, tracking: -0.04 };
        let bold = logo::Style { size: 30.0, weight: 800.0, italic: false, tracking: -0.01 };
        logo::mark(b, panel, "vca", mx - 150.0, 272.0, 138.0, heavy, "#ecebe6", TextAlign::Right);
        b.add(
            panel,
            draw::with_effects(
                draw::circle("mark dot", mx - 1.0, 274.0, 5.5, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#2a2b2e"), (1.0, "#020203")])),
                vec![draw::drop_shadow("#ffffff26", 0.0, 1.0, 0.0)],
            ),
        );
        logo::mark(b, panel, "comp", mx + 12.0, 276.0, 140.0, bold, "#ecebe6", TextAlign::Left);
        ui::silk(b, panel, "Compressor/Limiter", mx, 306.0, 11.0, "#ecebe6", 800.0);
    });

    let params = vec![
        ui::db("vcacomp/threshold", "Threshold", -40.0, 0.0, -20.0).featured(),
        ui::param("vcacomp/ratio", "Compression", ui::printed(RATIO_VALUES, Unit::Ratio), 4.0).featured(),
        ui::db("vcacomp/output", "Output Gain", -20.0, 20.0, 0.0).featured(),
        ui::choice("vcacomp/meter", "Meter", &["Input", "Output", "Gain Change"], 2),
    ];
    let streams = vec![ui::gr_stream("vcacomp/gr", "Gain Reduction"), ui::level_stream("vcacomp/meter_level", "Meter (METER select)", 1)];
    Face { file: "09-vca-comp", plugin: "comp", scene, params, streams }
}

/// A legend size for a placed component configured by `legend=` option.
trait LegendOpt {
    fn opt_legend(self, size: f64) -> Self;
}

impl LegendOpt for Place {
    fn opt_legend(mut self, size: f64) -> Self {
        self.options.get_or_insert_with(Vec::new).push(format!("legend={size}"));
        self
    }
}
