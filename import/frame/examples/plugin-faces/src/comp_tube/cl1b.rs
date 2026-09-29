//! The other optical tube compressor (CL 1B), laid out from the unit.
//!
//! Same physics as the LA-2A, opposite philosophy: everything unlocked. A
//! blue 2U panel with the gain-reduction meter on the left and one row of
//! large black knobs — THRESHOLD, RATIO, ATTACK, RELEASE — then the
//! attack/release mode selector (FIXED / FIX-MAN / MANUAL) and GAIN. The
//! time scales are logarithmic, so their panel numerals are the true values
//! at evenly spaced marks, and every control has a readout under it: an
//! unlabelled 300 ms dial is exactly where a number helps.

use document_proto::NodeId;
use surface_kit::Place;
use surface_proto::{Range, Unit, ValueFormat};
use surface_widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

const W: f64 = 1070.0;
const H: f64 = 198.0;
const INK: &str = "#f1f6fb";
const DIM: &str = "#bcd0e0";
const GLOW: &str = "#ffd28a";
/// The knob row's centre line, and the lines under it.
const ROW: f64 = 94.0;
const CAPTION: f64 = ROW + 52.0;
const READOUT: f64 = ROW + 74.0;
/// Knob pitch along the row.
const PITCH: f64 = 112.0;

pub fn face() -> Face {
    let scene = canvas("CL 1B", build);
    let params = vec![
        ui::param("cl1b/threshold", "Threshold", Range::linear(-40.0, 0.0, Unit::Db), -20.0).short("Thresh"),
        ui::param("cl1b/ratio", "Ratio", Range::linear(2.0, 10.0, Unit::Ratio), 4.0),
        ui::param("cl1b/attack", "Attack", Range::log(0.5, 300.0, Unit::Milliseconds), 10.0),
        ui::param("cl1b/release", "Release", Range::log(0.05, 3.0, Unit::Seconds), 0.5),
        ui::choice("cl1b/timing", "Attack / Release Mode", &["Fixed", "Fix-Man", "Manual"], 2).short("Timing"),
        ui::param("cl1b/gain", "Gain", Range::linear(0.0, 24.0, Unit::Db), 4.0),
        ui::toggle("cl1b/power", "Power", true),
    ];
    Face { file: "05-cl1b", plugin: "comp", scene, params, streams: ui::streams("cl1b", "CL 1B") }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let panel = ui::put(b, root, "panel.painted", &Place::new("CL 1B panel").variant("blue").options(&[&format!("w={W}"), &format!("h={H}")]));
    ui::repaint(b, panel, &[(0.0, "#3b6f98"), (0.5, "#2b5a80"), (1.0, "#1f4464")]);

    // ── Identity + power, left ───────────────────────────────────────
    let x0 = 48.0;
    ui::silk_left(b, panel, "CL 1B", x0, 42.0, Ink::new(24.0, INK).weight(800.0).track(0.02));
    ui::silk_left(b, panel, "Opto Compressor", x0, 66.0, Ink::new(7.5, DIM).weight(700.0).track(0.22));
    ui::rule(b, panel, x0, x0 + 112.0, 76.0, 1.0, DIM);
    let power = ui::put(b, panel, "switch.toggle_bat", &Place::new("Power").size(40.0).legend("Power").options(&["On", "Off"]).bind(ui::bind("cl1b/power")));
    ui::caption_style(b, power, 8.0, 700.0, DIM);
    ui::move_to(b, power, x0 - 2.0, 100.0);
    ui::centred_at(b, panel, "button.jewel", &Place::new("Pilot").size(20.0).variant("red").bind(ui::bind("cl1b/power")), x0 + 92.0, 138.0);

    // ── The meter ────────────────────────────────────────────────────
    let mx = 270.0;
    ui::centred_at(b, panel, "meter.vu_ivory", &Place::new("Gain Reduction meter").size(190.0).variant("black_rack").options(&["glass"]).bind(ui::stream("cl1b/gr")), mx, ROW + 2.0);
    ui::silk(b, panel, "Gain Reduction", mx, CAPTION + 22.0, Ink::new(7.5, DIM).weight(700.0).track(0.24));

    // ── The knob row ─────────────────────────────────────────────────
    let numerals = Ink::new(7.5, INK).weight(600.0).track(0.0);
    let knob = |b: &mut Builder<'_>, name: &str, addr: &str, scale: &[&str], x: f64, value: &str, format: Option<ValueFormat>| {
        let k = ui::rotary_at(b, panel, "knob.bakelite", &Place::new(name).size(54.0).legend(name).scale(scale).touch(surface_kit::TOUCH_MIN).bind(ui::bind(addr)), x, ROW);
        ui::knob_scale(b, panel, k, (x, ROW), 43.0, 300.0, scale, numerals);
        ui::caption_style(b, k, 9.5, 800.0, INK);
        ui::caption_y(b, k, CAPTION);
        ui::readout(b, panel, &format!("{name} readout"), ui::bind(addr), (x, READOUT), (62.0, 16.0), value, GLOW, format);
    };
    let x1 = 424.0;
    knob(b, "Threshold", "cl1b/threshold", &["-40", "-30", "-20", "-10", "0"], x1, "-20.0 dB", Some(ValueFormat::decimals(1)));
    knob(b, "Ratio", "cl1b/ratio", &["2", "4", "6", "8", "10"], x1 + PITCH, "4.0 :1", Some(ValueFormat::decimals(1)));
    knob(b, "Attack", "cl1b/attack", &["0.5", "1.8", "6.5", "23", "84", "300"], x1 + 2.0 * PITCH, "10.0 ms", Some(ValueFormat::decimals(1)));
    knob(b, "Release", "cl1b/release", &[".05", ".11", ".26", ".58", "1.3", "3"], x1 + 3.0 * PITCH, "500 ms", None);

    // FIXED / FIX-MAN / MANUAL: the program-dependent fixed timing, the
    // knobs, or the knobs riding on the fixed curve.
    let tx = x1 + 4.0 * PITCH;
    let timing = ui::selector(b, panel, &Place::new("Timing").size(46.0).variant("black").legend("Timing").bind(ui::bind("cl1b/timing")), (tx, ROW), &["Fixed", "Fix-Man", "Manual"], numerals);
    ui::caption_style(b, timing, 9.5, 800.0, INK);
    ui::caption_y(b, timing, CAPTION);
    ui::readout(b, panel, "Timing readout", ui::bind("cl1b/timing"), (tx, READOUT), (62.0, 16.0), "Manual", GLOW, None);

    knob(b, "Gain", "cl1b/gain", &["0", "6", "12", "18", "24"], x1 + 5.0 * PITCH, "4.0 dB", Some(ValueFormat::decimals(1)));

    ui::fit_root(b, root, panel);
}
