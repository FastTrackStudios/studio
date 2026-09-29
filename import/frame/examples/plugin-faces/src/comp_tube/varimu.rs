//! The modern variable-mu bus compressor (a Manley-style Vari-Mu), laid out
//! from the unit — one channel of it.
//!
//! A brushed faceplate with the gain-reduction meter in a chrome bezel at
//! its centre; left of it the gain staging — detented INPUT, THRESHOLD and
//! continuous ATTACK — right of it the five-position RECOVERY switch and
//! OUTPUT, and a column of bat toggles at the end: COMP/LIMIT (1.5:1 soft
//! vs 4:1), the 100 Hz sidechain high-pass that stops the kick working the
//! whole mix, and POWER.

use document_proto::NodeId;
use surface_kit::Place;
use surface_proto::{Range, Unit};
use surface_widgets::Builder;

use super::ui::{self, Ink};
use crate::face::{Face, canvas};

const W: f64 = 900.0;
const H: f64 = 262.0;
const INK: &str = "#1b1c1e";
const DIM: &str = "#45474b";
const GLOW: &str = "#ffc978";
const ROW: f64 = 134.0;
const CAPTION: f64 = ROW + 50.0;
const READOUT: f64 = ROW + 72.0;
const PITCH: f64 = 116.0;

pub fn face() -> Face {
    let scene = canvas("Vari-Mu", build);
    let params = vec![
        ui::param("varimu/input", "Input", Range { steps: Some(11), ..Range::linear(-8.0, 8.0, Unit::Db) }, 0.0),
        ui::param("varimu/threshold", "Threshold", Range::linear(-40.0, 0.0, Unit::Db), -20.0).short("Thresh"),
        ui::param("varimu/attack", "Attack", Range::linear(25.0, 70.0, Unit::Milliseconds), 45.0),
        ui::choice("varimu/recovery", "Recovery", &["0.2 s", "0.4 s", "0.6 s", "1.2 s", "2.5 s"], 2),
        ui::param("varimu/output", "Output", Range::linear(-6.0, 18.0, Unit::Db), 0.0),
        ui::toggle("varimu/limit", "Comp / Limit", false).short("Limit"),
        ui::toggle("varimu/hp_sc", "HP Sidechain 100 Hz", true).short("HP SC"),
        ui::toggle("varimu/power", "Power", true),
    ];
    Face { file: "07-vari-mu", plugin: "comp", scene, params, streams: ui::streams("varimu", "Vari-Mu") }
}

fn build(b: &mut Builder<'_>, root: NodeId) {
    let panel = ui::put(b, root, "panel.brushed", &Place::new("Vari-Mu panel").variant("natural").options(&[&format!("w={W}"), &format!("h={H}")]));

    // ── Identity, over the gain staging ──────────────────────────────
    let x0 = 50.0;
    ui::silk_left(b, panel, "Variable Mu", x0, 36.0, Ink::new(19.0, INK).weight(800.0).track(0.04));
    ui::silk_left(b, panel, "Limiter · Compressor", x0 + 176.0, 38.0, Ink::new(7.5, DIM).weight(700.0).track(0.2));
    ui::rule(b, panel, x0, x0 + 300.0, 52.0, 1.0, INK);

    // ── Gain staging, left ───────────────────────────────────────────
    let numerals = Ink::new(8.0, INK).weight(600.0).track(0.0);
    let knob = |b: &mut Builder<'_>, name: &str, addr: &str, scale: &[&str], x: f64, value: &str| {
        let k = ui::rotary_at(b, panel, "knob.metal", &Place::new(name).size(56.0).variant("black").legend(name).scale(scale).bind(ui::bind(addr)), x, ROW);
        ui::reink(b, k, INK);
        ui::knob_scale(b, panel, k, (x, ROW), 45.0, scale, numerals);
        ui::caption_style(b, k, 9.5, 800.0, INK);
        ui::caption_y(b, k, CAPTION);
        ui::readout(b, panel, &format!("{name} readout"), ui::bind(addr), (x, READOUT), (62.0, 16.0), value, GLOW);
    };
    let x1 = 92.0;
    knob(b, "Input", "varimu/input", &["-8", "-4", "0", "+4", "+8"], x1, "0.00 dB");
    knob(b, "Threshold", "varimu/threshold", &["-40", "-30", "-20", "-10", "0"], x1 + PITCH, "-20.0 dB");
    knob(b, "Attack", "varimu/attack", &["25", "36", "48", "59", "70"], x1 + 2.0 * PITCH, "45.0 ms");

    // ── The meter ────────────────────────────────────────────────────
    let mx = 492.0;
    ui::centred_at(b, panel, "meter.vu_amber", &Place::new("Gain Reduction meter").size(196.0).variant("chrome").bind(ui::stream("varimu/gr")), mx, ROW - 14.0);
    ui::silk(b, panel, "Gain Reduction", mx, CAPTION + 12.0, Ink::new(8.0, DIM).weight(700.0).track(0.24));

    // ── Recovery + output, right ─────────────────────────────────────
    let rx = 652.0;
    let rec = ui::selector(b, panel, &Place::new("Recovery").size(42.0).variant("black").legend("Recovery").bind(ui::bind("varimu/recovery")), (rx, ROW), &[".2", ".4", ".6", "1.2", "2.5"], numerals, INK);
    ui::reink(b, rec, INK);
    ui::caption_style(b, rec, 9.5, 800.0, INK);
    ui::caption_y(b, rec, CAPTION);
    ui::readout(b, panel, "Recovery readout", ui::bind("varimu/recovery"), (rx, READOUT), (62.0, 16.0), "0.6 s", GLOW);
    knob(b, "Output", "varimu/output", &["-6", "0", "+6", "+12", "+18"], rx + 108.0, "0.00 dB");

    // ── The switch column ────────────────────────────────────────────
    let sx = 836.0;
    let bat = |b: &mut Builder<'_>, name: &str, addr: &str, legends: &[&str], top: f64| {
        let s = ui::put(b, panel, "switch.toggle_bat", &Place::new(name).size(34.0).legend(name).options(legends).bind(ui::bind(addr)));
        ui::reink(b, s, INK);
        ui::caption_style(b, s, 7.5, 800.0, INK);
        let (w, _) = ui::size_of(b, s);
        ui::move_to(b, s, sx - w / 2.0, top);
    };
    bat(b, "Mode", "varimu/limit", &["Limit", "Comp"], 22.0);
    bat(b, "HP SC", "varimu/hp_sc", &["100", "Out"], 94.0);
    bat(b, "Power", "varimu/power", &["On", "Off"], 166.0);
    ui::centred_at(b, panel, "button.jewel", &Place::new("Pilot").size(18.0).variant("amber").bind(ui::bind("varimu/power")), sx, 238.0);

    ui::fit_root(b, root, panel);
}
