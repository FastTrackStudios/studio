//! The variable-mu valve limiter (the 670), laid out from the unit — one
//! channel of it.
//!
//! Black panel, white silkscreen, two amber meters across the top in vented
//! bezels (gain reduction, and the output the unit's meter switch could put
//! on it), and the channel's controls in one row below: INPUT GAIN,
//! THRESHOLD, the six-position TIME CONSTANT switch, the AGC mode switch
//! (LEFT/RIGHT or LAT/VERT — stereo or mid/side) and OUTPUT.
//!
//! The time constant is the unit's personality: each position pairs an
//! attack with a release and 5–6 are program-dependent. The unit never
//! printed what the positions meant; this panel does, on an engraved plate
//! beside the switch.

use document_proto::NodeId;
use surface_kit::{Place, draw};
use surface_proto::{Range, Unit, ValueFormat};
use surface_widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 392.0;
const INK: &str = "#eceef0";
const DIM: &str = "#9aa0a6";
const RED: &str = "#e0483a";
const GLOW: &str = "#ffc978";
/// The meter row and the control row.
const METERS: f64 = 96.0;
const ROW: f64 = 284.0;
const CAPTION: f64 = ROW + 60.0;
const READOUT: f64 = ROW + 82.0;

/// The switch's table: attack, release (second stage), from the unit.
const TIME_CONSTANTS: [(&str, &str, &str); 6] = [
    ("1", "0.2 ms", "0.3 s"),
    ("2", "0.2 ms", "0.8 s"),
    ("3", "0.4 ms", "2 s"),
    ("4", "0.8 ms", "5 s"),
    ("5", "0.4 ms", "2 s / 10 s"),
    ("6", "0.2 ms", "2 s / 25 s"),
];

pub fn face() -> Face {
    let scene = canvas("670", build);
    let params = vec![
        ui::param("fairchild/input_gain", "Input Gain", Range::linear(-20.0, 0.0, Unit::Db), 0.0).short("Input"),
        ui::param("fairchild/threshold", "Threshold", Range::linear(-40.0, 0.0, Unit::Db), -18.0).short("Thresh"),
        ui::choice("fairchild/time_constant", "Time Constant", &["1", "2", "3", "4", "5 (auto)", "6 (auto)"], 1).short("Time"),
        ui::choice("fairchild/agc", "AGC", &["Left / Right", "Lat / Vert"], 0),
        ui::param("fairchild/output", "Output", Range::linear(0.0, 20.0, Unit::Db), 6.0),
        ui::toggle("fairchild/power", "Power", true),
    ];
    Face { file: "06-fairchild-670", plugin: "comp", scene, params, streams: ui::streams("fairchild", "670") }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let panel = ui::put(b, root, "panel.painted", &Place::new("670 panel").variant("black").options(&[&format!("w={W}"), &format!("h={H}")]));
    ui::repaint(b, panel, &[(0.0, "#26272a"), (0.5, "#1a1b1d"), (1.0, "#0f1012")]);

    // ── The meters ───────────────────────────────────────────────────
    let (m1, m2) = (220.0, W - 220.0);
    ui::centred_at(b, panel, "meter.vu_amber", &Place::new("Gain Reduction meter").size(232.0).variant("black_rack").options(&["glass"]).bind(ui::stream("fairchild/gr")), m1, METERS);
    let out = ui::centred_at(b, panel, "meter.vu_amber", &Place::new("Output meter").size(232.0).variant("black_rack").options(&["glass"]).bind(ui::stream("fairchild/out")), m2, METERS);
    // 0 VU = -18 dBFS on the output meter.
    ui::calibrate_vu(b, out, -18.0);
    let label = Ink::new(8.5, RED).weight(800.0).track(0.24);
    ui::silk(b, panel, "Gain Reduction", m1, METERS + 92.0, label);
    ui::silk(b, panel, "Output Level", m2, METERS + 92.0, label.color(INK));

    // ── Identity + power, between them ───────────────────────────────
    let cx = W / 2.0;
    ui::silk(b, panel, "670", cx, 44.0, Ink::new(40.0, INK).weight(800.0).track(0.02));
    ui::silk(b, panel, "Limiting Amplifier", cx, 78.0, Ink::new(8.5, INK).weight(700.0).track(0.26));
    ui::rule(b, panel, cx - 70.0, cx + 70.0, 90.0, 1.0, RED);
    ui::silk(b, panel, "Variable-Mu · Valve", cx, 101.0, Ink::new(7.0, DIM).weight(600.0).track(0.22));
    let power = ui::put(b, panel, "switch.toggle_bat", &Place::new("Power").size(40.0).legend("Power").options(&["On", "Off"]).bind(ui::bind("fairchild/power")));
    ui::caption_style(b, power, 7.5, 700.0, DIM);
    let (pw, _) = ui::size_of(b, power);
    ui::move_to(b, power, cx - pw / 2.0 - 14.0, 118.0);
    ui::centred_at(b, panel, "button.jewel", &Place::new("Pilot").size(22.0).variant("red").bind(ui::bind("fairchild/power")), cx + 34.0, 154.0);

    // A hairline between the meter row and the channel.
    ui::rule(b, panel, 44.0, W - 44.0, 204.0, 1.0, "#ffffff1f");
    ui::rule(b, panel, 44.0, W - 44.0, 205.0, 1.0, "#00000080");
    ui::silk_left(b, panel, "Channel A", 48.0, 220.0, Ink::new(8.0, RED).weight(800.0).track(0.26));

    // ── The channel ──────────────────────────────────────────────────
    let numerals = Ink::new(8.0, INK).weight(600.0).track(0.0);
    let knob = |b: &mut Builder<'_>, name: &str, addr: &str, scale: &[&str], x: f64, value: &str| {
        let format = Some(ValueFormat::decimals(1));
        let k = ui::rotary_at(b, panel, "knob.bakelite", &Place::new(name).size(64.0).legend(name).scale(scale).touch(surface_kit::TOUCH_MIN).bind(ui::bind(addr)), x, ROW);
        ui::knob_scale(b, panel, k, (x, ROW), 52.0, 300.0, scale, numerals);
        ui::caption_style(b, k, 9.5, 800.0, INK);
        ui::caption_y(b, k, CAPTION);
        ui::readout(b, panel, &format!("{name} readout"), ui::bind(addr), (x, READOUT), (64.0, 16.0), value, GLOW, format);
    };
    knob(b, "Input Gain", "fairchild/input_gain", &["-20", "-16", "-12", "-8", "-4", "0"], 118.0, "0.0 dB");
    knob(b, "Threshold", "fairchild/threshold", &["-40", "-32", "-24", "-16", "-8", "0"], 252.0, "-18.0 dB");

    // TIME CONSTANT — six detents, and the plate that says what they are.
    let tc = 400.0;
    let sw = ui::selector(b, panel, &Place::new("Time Constant").size(56.0).variant("black").legend("Time Constant").bind(ui::bind("fairchild/time_constant")), (tc, ROW), &["1", "2", "3", "4", "5", "6"], numerals.size(9.0).weight(700.0));
    ui::caption_style(b, sw, 9.5, 800.0, INK);
    ui::caption_y(b, sw, CAPTION);
    time_constant_plate(b, panel, 488.0, ROW - 62.0);

    let agc_x = 726.0;
    let agc = ui::selector(b, panel, &Place::new("AGC").size(48.0).variant("black").legend("AGC").bind(ui::bind("fairchild/agc")), (agc_x, ROW), &["L-R", "Lat-Vert"], numerals);
    ui::caption_style(b, agc, 9.5, 800.0, INK);
    ui::caption_y(b, agc, CAPTION);
    ui::readout(b, panel, "AGC readout", ui::bind("fairchild/agc"), (agc_x, READOUT), (76.0, 16.0), "Left / Right", GLOW, None);

    knob(b, "Output", "fairchild/output", &["0", "4", "8", "12", "16", "20"], 852.0, "6.0 dB");

    ui::fit_root(b, root, panel);
}

/// The engraved plate listing each TIME CONSTANT position's attack and
/// release, top-left at (`x`, `y`).
fn time_constant_plate(b: &mut Builder<'_>, panel: NodeId, x: f64, y: f64) {
    let (w, h) = (156.0, 124.0);
    ui::plate(b, panel, "time constants", x, y, w, h, draw::linear(180.0, &[(0.0, "#2c2d30"), (1.0, "#202124")]), 4.0);
    let head = Ink::new(6.5, DIM).weight(700.0).track(0.16);
    let (c1, c2, c3) = (x + 16.0, x + 58.0, x + 116.0);
    ui::silk(b, panel, "Pos", c1, y + 14.0, head);
    ui::silk(b, panel, "Attack", c2, y + 14.0, head);
    ui::silk(b, panel, "Release", c3, y + 14.0, head);
    ui::rule(b, panel, x + 8.0, x + w - 8.0, y + 23.0, 0.8, "#ffffff40");
    let row = Ink::new(7.5, INK).weight(600.0).track(0.02).plain();
    for (i, (pos, attack, release)) in TIME_CONSTANTS.iter().enumerate() {
        let cy = y + 36.0 + i as f64 * 14.5;
        let auto = i >= 4;
        ui::silk(b, panel, pos, c1, cy, row.weight(800.0));
        ui::silk(b, panel, attack, c2, cy, row);
        ui::silk(b, panel, release, c3, cy, if auto { row.color(GLOW) } else { row });
    }
    ui::silk(b, panel, "5–6 program dependent", x + w / 2.0, y + h + 10.0, Ink::new(6.5, GLOW).weight(600.0).track(0.1));
}
