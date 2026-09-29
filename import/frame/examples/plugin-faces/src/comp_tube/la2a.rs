//! The optical leveling amplifier (LA-2A), laid out from the unit.
//!
//! Grey 3U panel: the model plate top left over the COMPRESS/LIMIT toggle,
//! GAIN left of the meter and PEAK REDUCTION right of it — both plain black
//! pointer knobs read against a 0–100 scale printed on the panel — the amber
//! VU in its black frame dead centre, and the meter selector (+10 / GAIN
//! REDUCTION / +4) over POWER and its red jewel at the right. Nothing else:
//! the unit's whole character is that there is no threshold, ratio, attack
//! or release to touch.

use document_proto::NodeId;
use surface_kit::{Ink as KitInk, Place};
use surface_proto::{MeterTap, SourceSelect};
use surface_widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

const W: f64 = 900.0;
const H: f64 = 250.0;
/// Silkscreen: black-brown ink on the grey, and the red of the model line.
const INK: &str = "#26231f";
const DIM: &str = "#4a4640";
const RED: &str = "#9c2a20";
/// The control row's centre line.
const ROW: f64 = 128.0;

pub fn face() -> Face {
    let scene = canvas("LA-2A", build);
    let params = vec![
        ui::dial_0_100("la2a/gain", "Gain", 40.0).short("Gain"),
        ui::dial_0_100("la2a/peak_reduction", "Peak Reduction", 45.0).short("Peak Red"),
        ui::toggle("la2a/limit", "Compress / Limit", false).short("Limit"),
        ui::choice("la2a/meter", "Meter", &["Output +10", "Gain Reduction", "Output +4"], 1),
        ui::toggle("la2a/power", "Power", true),
    ];
    Face { file: "04-la2a", plugin: "comp", scene, params, streams: ui::streams("la2a", "LA-2A") }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let panel = ui::put(b, root, "panel.painted", &Place::new("LA-2A panel").variant("grey").options(&[&format!("w={W}"), &format!("h={H}")]));

    // ── Identity plate, top left ─────────────────────────────────────
    let x0 = 52.0;
    ui::silk_left(b, panel, "Leveling Amplifier", x0, 34.0, Ink::new(9.0, INK).weight(700.0).track(0.22));
    ui::rule(b, panel, x0, x0 + 150.0, 45.0, 1.0, INK);
    ui::silk_left(b, panel, "Model LA-2A", x0, 60.0, Ink::new(17.0, RED).weight(800.0).track(0.04).plain());
    ui::silk_left(b, panel, "Optical · Tube · T4 Cell", x0, 80.0, Ink::new(7.5, DIM).weight(600.0).track(0.16));

    // COMPRESS / LIMIT — the switch that decides the knee.
    let sw = ui::put(b, panel, "switch.toggle_bat", &Place::new("Compress / Limit").size(46.0).legend("Mode").options(&["Limit", "Compress"]).ink(dark()).bind(ui::bind("la2a/limit")));
    let (sw_w, _) = ui::size_of(b, sw);
    ui::move_to(b, sw, x0 + 58.0 - sw_w / 2.0 + 18.0, 118.0);
    ui::caption_style(b, sw, 8.0, 700.0, DIM);

    // ── Gain | meter | Peak reduction ────────────────────────────────
    let scale = ["0", "10", "20", "30", "40", "50", "60", "70", "80", "90", "100"];
    let numerals = Ink::new(8.0, INK).weight(600.0).track(0.0);
    let gain = ui::rotary_at(b, panel, "knob.pointer", &Place::new("Gain").size(60.0).legend("Gain").scale(&scale).touch(surface_kit::TOUCH_MIN).ink(dark()).bind(ui::bind("la2a/gain")), 262.0, ROW);
    ui::knob_scale(b, panel, gain, (262.0, ROW), 62.0, 300.0, &scale, numerals);
    ui::caption_style(b, gain, 11.0, 800.0, INK);

    // The meter switch really switches it: the output on its +10 range,
    // gain reduction, or the output on +4.
    let vu = Place::new("Gain Reduction meter").size(206.0).variant("black_rack").legend("VU").options(&["glass"]).bind(ui::stream("la2a/gr")).select(SourceSelect::new(
        "la2a/meter",
        vec![Some(MeterTap::new("la2a/out").offset(-10.0)), Some(MeterTap::new("la2a/gr")), Some(MeterTap::new("la2a/out").offset(-4.0))],
    ));
    ui::centred_at(b, panel, "meter.vu_amber", &vu, W / 2.0, ROW - 6.0);
    ui::silk(b, panel, "Gain Reduction", W / 2.0, 224.0, Ink::new(7.5, DIM).weight(700.0).track(0.24));

    let peak = ui::rotary_at(b, panel, "knob.pointer", &Place::new("Peak Reduction").size(60.0).legend("Peak Reduction").scale(&scale).touch(surface_kit::TOUCH_MIN).ink(dark()).bind(ui::bind("la2a/peak_reduction")), W - 262.0, ROW);
    ui::knob_scale(b, panel, peak, (W - 262.0, ROW), 62.0, 300.0, &scale, numerals);
    ui::caption_style(b, peak, 11.0, 800.0, INK);

    // ── Meter selector over power, right ─────────────────────────────
    let mx = 790.0;
    let meter = ui::selector(b, panel, &Place::new("Meter").size(40.0).variant("black").legend("Meter").bind(ui::bind("la2a/meter")), (mx, 70.0), &["+10", "GR", "+4"], numerals);
    ui::caption_style(b, meter, 8.5, 800.0, INK);
    ui::caption_y(b, meter, 102.0);

    let power = ui::put(b, panel, "switch.toggle_bat", &Place::new("Power").size(40.0).legend("Power").options(&["On", "Off"]).ink(dark()).bind(ui::bind("la2a/power")));
    let (pw, _) = ui::size_of(b, power);
    ui::move_to(b, power, mx - pw / 2.0 - 4.0, 150.0);
    ui::caption_style(b, power, 8.0, 700.0, DIM);
    ui::centred_at(b, panel, "button.jewel", &Place::new("Pilot").size(22.0).variant("red").bind(ui::bind("la2a/power")), mx + 34.0, 172.0);

    // A fine rule under the whole row, the serial plate at the foot.
    ui::silk_left(b, panel, "Input 600 Ω · Output +10 dBm max", x0, 224.0, Ink::new(6.5, DIM).weight(600.0).track(0.14));
    ui::silk_right(b, panel, "Serial No. 2A-1965", W - 52.0, 224.0, Ink::new(6.5, DIM).weight(600.0).track(0.14));
    ui::fit_root(b, root, panel);
}

/// The kit's print in the panel's black-brown ink.
fn dark() -> KitInk {
    KitInk::Color(INK.into())
}
