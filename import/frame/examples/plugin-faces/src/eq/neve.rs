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
use surface_kit::{Place, draw};
use surface_proto::{Control, ControlKind, SlotRole};
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

/// Collar radius, and the cap's diameter, of a concentric pair.
const COLLAR: f64 = 46.0;
const CAP: f64 = 52.0;

/// A concentric pair: a stepped frequency collar with a gain cap in it.
///
/// Two controls may not share a hit box, so the collar's box is the band
/// *above* the cap (grab the collar by its top edge, as you would the real
/// one) and the cap's box is the cap; the collar is drawn whole under both.
#[allow(clippy::too_many_arguments)]
fn concentric(b: &mut Builder<'_>, panel: NodeId, freq: &str, gain: &str, labels: &[&str], legend: &str, cx: f64) {
    let (r, pad) = (COLLAR, 32.0);
    let n = labels.len();
    let sweep = ui::detent_sweep(n);
    ui::detents(b, panel, cx, ROW, r + 6.0, r + 17.0, labels, sweep, 8.5, INK);

    // The collar control: its box the band above the cap.
    let top = ROW - r - pad;
    let root = b.add(panel, draw::frame(&format!("{legend} frequency"), cx - r - pad, top, 2.0 * (r + pad), ROW - CAP / 2.0 - top));
    let collar = b.add(root, draw::frame("collar", pad, pad, 2.0 * r, 2.0 * r));
    let ring = draw::circle("ring", r, r, r, draw::radial_at(0.38, 0.3, 0.9, &[(0.0, "#e3e5e8"), (0.55, "#aeb2b8"), (1.0, "#6c7076")]));
    let ring = draw::with_effects(draw::stroked(ring, "#00000080", 1.0), vec![draw::drop_shadow("#0000008c", 0.0, 2.5, 6.0), draw::inner_shadow("#ffffff66", 0.0, 1.0, 1.0)]);
    b.add(collar, ring);
    // Knurled grip around the rim.
    let knurl: String = (0..72)
        .map(|i| {
            let a = f64::from(i) * 5.0;
            let (x0, y0) = draw::polar(r, r, r - 5.0, a);
            let (x1, y1) = draw::polar(r, r, r - 1.2, a);
            let (nx, ny) = (a.to_radians().cos() * 0.5, a.to_radians().sin() * 0.5);
            draw::polygon(&[(x0 - nx, y0 - ny), (x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny)])
        })
        .collect();
    b.add(collar, draw::path("knurl", 0.0, 0.0, 2.0 * r, 2.0 * r, knurl, draw::solid("#00000038")));
    // The index that turns: a white tab at the rim, drawn at centre of travel.
    let tab = draw::polygon(&[(r - 3.2, 1.0), (r + 3.2, 1.0), (r + 2.0, 15.0), (r - 2.0, 15.0)]);
    let index = b.add(collar, draw::path("index", 0.0, 0.0, 2.0 * r, 2.0 * r, tab, draw::solid("#f4f4f0")));
    let mut c = Control::new(ControlKind::Knob { sweep_degrees: sweep }).slot(SlotRole::Needle, index.0);
    c.binding = Some(bind(freq));
    b.controls.insert(root, c);

    // The gain cap in the middle.
    let p = Place::new(format!("{legend} gain")).size(CAP).variant("grey").bind(bind(gain)).scale(&[""]);
    let k = ui::put(b, panel, "knob.neve", &p, cx, ROW);
    ui::strip_scale(b, k);
    ui::strip_legend(b, k);
    ui::tighten(b, k, CAP);

    ui::silk(b, panel, legend, cx, ROW + r + 30.0, 9.0, INK, 800.0);
}

/// A wing knob (the two coloured controls on the module).
fn wing(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, color: &str, cx: f64) -> NodeId {
    let p = Place::new(legend).size(80.0).variant(color).bind(bind(addr)).scale(&[""]);
    let k = ui::put(b, panel, "knob.marconi", &p, cx, ROW);
    ui::strip_scale(b, k);
    ui::strip_legend(b, k);
    if !legend.is_empty() {
        ui::silk(b, panel, legend, cx, ROW + COLLAR + 30.0, 9.0, INK, 800.0);
    }
    k
}

pub fn face() -> Face {
    let scene = canvas("Neve 1073", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "neve", W, H, "ears");

        // ── Gain: the red wing, the biggest thing on the panel ─────────
        ui::silk(b, panel, "Drive", 140.0, GLYPH_Y, 9.0, INK, 800.0);
        wing(b, panel, "neve1073/drive", "", "red", 140.0);
        let drive = ["0", "2", "4", "6", "8", "10"];
        ui::detents(b, panel, 140.0, ROW, COLLAR + 6.0, COLLAR + 17.0, &drive, 300.0, 8.5, INK);

        // ── High shelf, fixed at 12 kHz ────────────────────────────────
        ui::glyph(b, panel, Glyph::HighShelf, 322.0, GLYPH_Y, 34.0, INK);
        let p = Place::new("12 kHz").size(64.0).variant("grey").bind(bind("neve1073/high_gain")).scale(&[""]);
        let k = ui::put(b, panel, "knob.neve", &p, 322.0, ROW);
        ui::strip_scale(b, k);
        ui::strip_legend(b, k);
        ui::detents(b, panel, 322.0, ROW, COLLAR + 6.0, COLLAR + 17.0, &["-16", "-8", "0", "+8", "+16"], 300.0, 8.5, INK);
        ui::silk(b, panel, "12 kHz", 322.0, ROW + COLLAR + 30.0, 9.0, INK, 800.0);

        // ── Mid and low: concentric pairs ──────────────────────────────
        ui::glyph(b, panel, Glyph::Bell, 510.0, GLYPH_Y, 34.0, INK);
        concentric(b, panel, "neve1073/mid_freq", "neve1073/mid_gain", MID_FREQS, "kHz", 510.0);
        ui::glyph(b, panel, Glyph::LowShelf, 700.0, GLYPH_Y, 34.0, INK);
        concentric(b, panel, "neve1073/low_freq", "neve1073/low_gain", LOW_FREQS, "Hz", 700.0);

        // ── High pass: the blue wing, stepped ─────────────────────────
        ui::glyph(b, panel, Glyph::HighPass, 888.0, GLYPH_Y, 34.0, INK);
        let hpf = wing(b, panel, "neve1073/hpf", "Hz", "blue", 888.0);
        let sweep = ui::detent_sweep(HPF_FREQS.len());
        ui::set_sweep(b, hpf, sweep);
        ui::detents(b, panel, 888.0, ROW, COLLAR + 6.0, COLLAR + 17.0, HPF_FREQS, sweep, 8.5, INK);

        // ── Output trim ────────────────────────────────────────────────
        ui::silk(b, panel, "Trim", 1030.0, GLYPH_Y, 9.0, INK, 800.0);
        let p = Place::new("Trim").size(48.0).variant("grey").bind(bind("neve1073/trim")).scale(&[""]);
        let t = ui::put(b, panel, "knob.neve", &p, 1030.0, ROW);
        ui::strip_scale(b, t);
        ui::strip_legend(b, t);
        ui::detents(b, panel, 1030.0, ROW, 34.0, 45.0, &["-10", "", "0", "", "+10"], 300.0, 8.5, INK);
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
