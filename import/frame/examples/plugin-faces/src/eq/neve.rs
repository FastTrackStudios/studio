//! The class-A console channel (1073), laid out from the module's front as
//! one row.
//!
//! A red gain wing at the left; a fixed 12 kHz shelf; the mid and low bands
//! as **concentric** pairs — the grey collar clicks through the stepped
//! frequencies (OFF · 0·36 … 7·2 kHz, OFF · 35 … 220 Hz), the cap in the
//! middle sets the gain — which is how the module fits a whole EQ in one
//! row; the blue high-pass wing; the output trim; and the latching PHASE and
//! EQL buttons on the pale strip at the right. Each band carries the shape
//! the module prints over it instead of a name. The colour is the index:
//! red gain, blue filter, grey bands.

use document_proto::NodeId;
use surface_kit::{Ink, Place, TOUCH_MIN, detent_sweep, draw};
use surface_widgets::Builder;

use crate::comp_vca::ui::{self, Glyph, bind};
use crate::face::{Face, canvas};

const W: f64 = 1240.0;
const H: f64 = 300.0;
const INK: &str = "#eef1f4";
const DIM: &str = "#9aa1aa";
const ROW: f64 = 164.0;
const GLYPH_Y: f64 = 50.0;

const MID_FREQS: &[&str] = &["Off", "·36", "·7", "1·6", "3·2", "4·8", "7·2"];
const LOW_FREQS: &[&str] = &["Off", "35", "60", "110", "220"];
const HPF_FREQS: &[&str] = &["Off", "50", "80", "160", "300"];

/// Collar radius of a concentric pair (the row's scale radius).
const COLLAR: f64 = 46.0;

/// The panel's light ink, for the kit's print.
fn ink() -> Ink {
    Ink::Color(INK.into())
}

/// A concentric pair: a stepped frequency collar with the gain cap in it —
/// the cap's control nested in the collar's, so each has its own disc.
fn concentric(b: &mut Builder<'_>, panel: NodeId, freq: &str, gain: &str, labels: &[&str], legend: &str, cx: f64) {
    let cap = Place::new(format!("{legend} gain")).bind(bind(gain));
    let p = Place::new(format!("{legend} frequency"))
        .size(2.0 * COLLAR)
        .variant("grey_black")
        .legend(legend)
        .keep_case()
        .scale(labels)
        .sweep(detent_sweep(labels.len()))
        .ink(ink())
        .bind(bind(freq))
        .inner(cap);
    let collar = ui::put(b, panel, "knob.concentric", &p, cx, ROW);
    // The cap is read by its index, as on the module: no second legend.
    let caps: Vec<NodeId> = b.scene.walk(collar).into_iter().filter(|id| *id != collar && b.controls.contains_key(id)).collect();
    for c in caps {
        ui::strip_legend(b, c);
    }
}

/// A wing knob (the two coloured controls on the module), its scale printed
/// round it; `stepped` makes it click through exactly those positions.
#[allow(clippy::too_many_arguments)]
fn wing(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, color: &str, scale: &[&str], stepped: bool, cx: f64) {
    let p = Place::new(if legend.is_empty() { "Drive" } else { legend }).size(80.0).variant(color).legend(legend).keep_case().ink(ink()).touch(TOUCH_MIN).bind(bind(addr));
    let p = if stepped { p.options(scale) } else { p.scale(scale) };
    let k = ui::put(b, panel, "knob.marconi", &p, cx, ROW);
    if legend.is_empty() {
        ui::strip_legend(b, k);
    }
}

/// A grey knob with its scale printed round it.
#[allow(clippy::too_many_arguments)]
fn grey(b: &mut Builder<'_>, panel: NodeId, addr: &str, name: &str, legend: &str, d: f64, scale: &[&str], cx: f64) -> NodeId {
    let p = Place::new(name).size(d).variant("grey").format(surface_proto::ValueFormat::decimals(1)).legend(legend).keep_case().scale(scale).ink(ink()).touch(TOUCH_MIN).bind(bind(addr));
    ui::put(b, panel, "knob.neve", &p, cx, ROW)
}

pub fn face() -> Face {
    let scene = canvas("Neve 1073", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "neve", W, H, "ears");

        // ── Gain: the red wing, the biggest thing on the panel ─────────
        ui::silk(b, panel, "Drive", 140.0, GLYPH_Y, 9.0, INK, 800.0);
        wing(b, panel, "neve1073/drive", "", "red", &["0", "2", "4", "6", "8", "10"], false, 140.0);

        // ── High shelf, fixed at 12 kHz ────────────────────────────────
        ui::glyph(b, panel, Glyph::HighShelf, 322.0, GLYPH_Y, 34.0, INK);
        grey(b, panel, "neve1073/high_gain", "12 kHz", "12 kHz", 64.0, &["-16", "-8", "0", "+8", "+16"], 322.0);

        // ── Mid and low: concentric pairs ──────────────────────────────
        ui::glyph(b, panel, Glyph::Bell, 510.0, GLYPH_Y, 34.0, INK);
        concentric(b, panel, "neve1073/mid_freq", "neve1073/mid_gain", MID_FREQS, "kHz", 510.0);
        ui::glyph(b, panel, Glyph::LowShelf, 700.0, GLYPH_Y, 34.0, INK);
        concentric(b, panel, "neve1073/low_freq", "neve1073/low_gain", LOW_FREQS, "Hz", 700.0);

        // ── High pass: the blue wing, stepped ─────────────────────────
        ui::glyph(b, panel, Glyph::HighPass, 888.0, GLYPH_Y, 34.0, INK);
        wing(b, panel, "neve1073/hpf", "Hz", "blue", HPF_FREQS, true, 888.0);

        // ── Output trim ────────────────────────────────────────────────
        ui::silk(b, panel, "Trim", 1030.0, GLYPH_Y, 9.0, INK, 800.0);
        let t = grey(b, panel, "neve1073/trim", "Trim", "", 48.0, &["-10", "", "0", "", "+10"], 1030.0);
        ui::strip_legend(b, t);
        ui::add_value(b, t, 8.0, DIM);

        // ── The pale strip: phase and EQL ──────────────────────────────
        let strip = draw::rect("strip", 1112.0, 22.0, 96.0, 256.0, draw::linear(180.0, &[(0.0, "#e9e7df"), (1.0, "#cfccc2")]));
        b.add(panel, draw::with_effects(draw::rounded(strip, 3.0), vec![draw::drop_shadow("#00000066", 0.0, 1.0, 3.0)]));
        let dark = "#20242b";
        let ph = Place::new("Phase").size(34.0).variant("silver").legend("Ø").bind(bind("neve1073/phase"));
        ui::put(b, panel, "button.metal", &ph, 1160.0, 92.0);
        ui::silk(b, panel, "Phase", 1160.0, 50.0, 8.0, dark, 800.0);
        let eq = Place::new("EQL").size(34.0).variant("silver").legend("In").bind(bind("neve1073/eq_in"));
        ui::put(b, panel, "button.metal", &eq, 1160.0, 192.0);
        ui::silk(b, panel, "EQL", 1160.0, 150.0, 8.0, dark, 800.0);
        ui::plain(b, panel, "1073", 1160.0, 256.0, 13.0, dark, 800.0);
    });

    let params = vec![
        ui::percent("neve1073/drive", "Drive", 0.0).featured(),
        ui::db("neve1073/high_gain", "High Gain", -16.0, 16.0, 0.0).featured(),
        ui::choice("neve1073/mid_freq", "Mid Freq", &["Off", "360 Hz", "700 Hz", "1.6 kHz", "3.2 kHz", "4.8 kHz", "7.2 kHz"], 0).featured(),
        ui::db("neve1073/mid_gain", "Mid Gain", -18.0, 18.0, 0.0).featured(),
        ui::choice("neve1073/low_freq", "Low Freq", &["Off", "35 Hz", "60 Hz", "110 Hz", "220 Hz"], 0),
        ui::db("neve1073/low_gain", "Low Gain", -16.0, 16.0, 0.0),
        ui::choice("neve1073/hpf", "High Pass", &["Off", "50 Hz", "80 Hz", "160 Hz", "300 Hz"], 0),
        ui::db("neve1073/trim", "Trim", -10.0, 10.0, 0.0),
        ui::toggle("neve1073/phase", "Phase", false),
        ui::toggle("neve1073/eq_in", "EQL", true).featured(),
    ];
    Face { file: "04-neve-1073", plugin: "eq", scene, params, streams: Vec::new() }
}
