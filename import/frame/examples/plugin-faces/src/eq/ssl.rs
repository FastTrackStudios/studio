//! The 4000-series console channel EQ, laid out from the strip.
//!
//! Sections left to right — FILTERS, LF, LMF, HMF, HF, then the switching
//! and the output meter — divided by hairlines, exactly as the strip is.
//! Each band reads as a column: gain on top, frequency (and Q) beneath, its
//! switch at the foot. The cap colours are the console's and are not
//! decoration: an engineer reaches for "the blue one" (LMF), "the green
//! one" (HMF), the magenta HF, without reading anything.
//!
//! Every frequency pot follows its printed scale — the marks are where the
//! console prints them, not an even log spread — and the switching the FTS
//! panel drew unwired (the mid-band Qs, ÷3 / ×3, BELL, FLTR IN, phase) is
//! all real here. The output meter is a stereo LED bar on the output level.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN};
use surface_proto::{Binding, Source, Unit, ValueFormat};
use surface_widgets::Builder;

use crate::comp_vca::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 1060.0;
const H: f64 = 320.0;
const INK: &str = "#eceef0";
const DIM: &str = "#9aa0a6";
const HEAD: f64 = 28.0;
const GAIN_ROW: f64 = 90.0;
const FREQ_ROW: f64 = 196.0;
const SWITCH_ROW: f64 = 284.0;
const GAIN: f64 = 46.0;
const FREQ: f64 = 42.0;
const Q: f64 = 36.0;

const GAIN_SCALE: &[&str] = &["-15", "", "", "", "", "0", "", "", "", "", "+15"];
const HPF: &[f64] = &[16.0, 20.0, 70.0, 120.0, 200.0, 300.0, 350.0];
const LPF: &[f64] = &[3000.0, 5000.0, 8000.0, 12000.0, 16000.0, 20000.0, 22000.0];
const LF: &[f64] = &[30.0, 50.0, 100.0, 200.0, 300.0, 450.0];
const LMF: &[f64] = &[200.0, 300.0, 800.0, 1000.0, 1500.0, 2000.0, 2500.0];
const HMF: &[f64] = &[600.0, 800.0, 1500.0, 3000.0, 4500.0, 6000.0, 7000.0];
const HF: &[f64] = &[1500.0, 2000.0, 5000.0, 8000.0, 10000.0, 14000.0, 16000.0];
const QS: &[f64] = &[0.5, 1.0, 1.5, 2.0, 3.0];

/// How the console prints a frequency: Hz under 1 k, else kHz with the
/// leading zero dropped (`.8`, `1.5`, `16`).
fn mark(v: f64, khz: bool) -> String {
    if !khz {
        return format!("{v}");
    }
    let k = v / 1000.0;
    let s = format!("{k}");
    s.strip_prefix('0').map_or(s.clone(), ToString::to_string)
}

fn marks(values: &[f64], khz: bool) -> Vec<String> {
    values.iter().map(|v| mark(*v, khz)).collect()
}

/// Every other mark printed (the rest left as ticks): the two filter caps
/// sit close enough that a full scale at a readable size runs together.
fn alternate(marks: &[String]) -> Vec<String> {
    marks.iter().enumerate().map(|(i, m)| if i % 2 == 0 { m.clone() } else { String::new() }).collect()
}

/// A collet cap with its printed scale and legend.
#[allow(clippy::too_many_arguments)]
fn cap(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, color: &str, d: f64, scale: &[String], cx: f64, cy: f64) -> NodeId {
    let scale: Vec<&str> = scale.iter().map(String::as_str).collect();
    let p = Place::new(legend).size(d).variant(color).legend(legend).touch(TOUCH_MIN).bind(bind(addr)).scale(&scale);
    ui::put(b, panel, "knob.collet", &p, cx, cy)
}

/// How a placed control's readout prints: 0.1 dB for the trim, whole
/// percent for the drive — not the range's default hundredths.
fn readout_format(b: &mut Builder<'_>, control: NodeId, f: ValueFormat) {
    if let Some(c) = b.controls.get_mut(&control) {
        c.format = Some(f);
    }
}

/// A small backlit switch at the foot of a column.
fn switch(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, lamp: &str, cx: f64, cy: f64) {
    let p = Place::new(legend).size(26.0).variant(lamp).legend(legend).bind(bind(addr));
    ui::put(b, panel, "button.square", &p, cx, cy);
}

/// A section divider: a dark groove with a lit lip.
fn divider(b: &mut Builder<'_>, panel: NodeId, x: f64) {
    ui::rule(b, panel, x, 14.0, 1.0, H - 28.0, "#00000073");
    ui::rule(b, panel, x + 1.0, 14.0, 1.0, H - 28.0, "#ffffff12");
}

pub fn face() -> Face {
    let scene = canvas("SSL Channel EQ", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "charcoal", W, H, "ears");

        // ── Filters ────────────────────────────────────────────────────
        ui::silk(b, panel, "Filters", 110.0, HEAD, 10.0, INK, 800.0);
        ui::plain(b, panel, "E · G Series", 110.0, HEAD + 16.0, 7.0, DIM, 600.0);
        cap(b, panel, "sslch/hpf", "HP Hz", "cream", FREQ, &alternate(&marks(HPF, false)), 72.0, FREQ_ROW);
        cap(b, panel, "sslch/lpf", "LP kHz", "cream", FREQ, &alternate(&marks(LPF, true)), 148.0, FREQ_ROW);
        switch(b, panel, "sslch/filters_in", "In", "white", 110.0, SWITCH_ROW);
        divider(b, panel, 190.0);

        // ── LF ─────────────────────────────────────────────────────────
        ui::silk(b, panel, "LF", 255.0, HEAD, 11.0, INK, 800.0);
        cap(b, panel, "sslch/lf_gain", "Gain", "black", GAIN, &GAIN_SCALE.iter().map(ToString::to_string).collect::<Vec<_>>(), 255.0, GAIN_ROW);
        cap(b, panel, "sslch/lf_freq", "Hz", "black", FREQ, &marks(LF, false), 255.0, FREQ_ROW);
        switch(b, panel, "sslch/lf_bell", "Bell", "white", 255.0, SWITCH_ROW);
        divider(b, panel, 320.0);

        // ── LMF ────────────────────────────────────────────────────────
        ui::silk(b, panel, "LMF", 400.0, HEAD, 11.0, INK, 800.0);
        let gains: Vec<String> = GAIN_SCALE.iter().map(ToString::to_string).collect();
        cap(b, panel, "sslch/lmf_gain", "Gain", "blue", GAIN, &gains, 400.0, GAIN_ROW);
        cap(b, panel, "sslch/lmf_q", "Q", "blue", Q, &marks(QS, false), 360.0, FREQ_ROW);
        cap(b, panel, "sslch/lmf_freq", "kHz", "blue", FREQ, &marks(LMF, true), 440.0, FREQ_ROW);
        switch(b, panel, "sslch/lmf_div3", "÷3", "blue", 400.0, SWITCH_ROW);
        divider(b, panel, 480.0);

        // ── HMF ────────────────────────────────────────────────────────
        ui::silk(b, panel, "HMF", 560.0, HEAD, 11.0, INK, 800.0);
        cap(b, panel, "sslch/hmf_gain", "Gain", "green", GAIN, &gains, 560.0, GAIN_ROW);
        cap(b, panel, "sslch/hmf_q", "Q", "green", Q, &marks(QS, false), 520.0, FREQ_ROW);
        cap(b, panel, "sslch/hmf_freq", "kHz", "green", FREQ, &marks(HMF, true), 600.0, FREQ_ROW);
        switch(b, panel, "sslch/hmf_x3", "×3", "green", 560.0, SWITCH_ROW);
        divider(b, panel, 640.0);

        // ── HF ─────────────────────────────────────────────────────────
        ui::silk(b, panel, "HF", 705.0, HEAD, 11.0, INK, 800.0);
        cap(b, panel, "sslch/hf_gain", "Gain", "magenta", GAIN, &gains, 705.0, GAIN_ROW);
        cap(b, panel, "sslch/hf_freq", "kHz", "magenta", FREQ, &marks(HF, true), 705.0, FREQ_ROW);
        switch(b, panel, "sslch/hf_bell", "Bell", "white", 705.0, SWITCH_ROW);
        divider(b, panel, 770.0);

        // ── Switching and output ───────────────────────────────────────
        ui::silk(b, panel, "Master", 836.0, HEAD, 10.0, INK, 800.0);
        switch(b, panel, "sslch/eq_in", "EQ", "green", 806.0, GAIN_ROW - 6.0);
        switch(b, panel, "sslch/phase", "Ø", "red", 866.0, GAIN_ROW - 6.0);
        ui::silk(b, panel, "In", 806.0, GAIN_ROW + 20.0, 7.0, DIM, 700.0);
        ui::silk(b, panel, "Phase", 866.0, GAIN_ROW + 20.0, 7.0, DIM, 700.0);
        let trim = cap(b, panel, "sslch/trim", "Trim", "cream", 38.0, &["-24", "", "", "0", "", "", "+24"].map(ToString::to_string), 804.0, FREQ_ROW);
        ui::add_value(b, trim, 7.0, DIM);
        readout_format(b, trim, ValueFormat::decimals(1));
        let drive = cap(b, panel, "sslch/drive", "Drive", "cream", 34.0, &["0", "", "", "", "10"].map(ToString::to_string), 872.0, FREQ_ROW);
        ui::add_value(b, drive, 7.0, DIM);
        readout_format(b, drive, ValueFormat::decimals(0));
        divider(b, panel, 904.0);

        // ── The output meter: two LED bars on a printed dB scale ───────
        ui::silk(b, panel, "Out", 968.0, HEAD, 10.0, INK, 800.0);
        let (top, bottom, floor, ceiling) = (52.0, 280.0, -40.0, 6.0);
        for (name, ch, x) in [("Output L", 0, 950.0), ("Output R", 1, 972.0)] {
            let bind = Binding { source: Source::Stream { id: surface_proto::EntryId::new("sslch/out"), channel: Some(ch) }, ..Binding::stream("sslch/out") };
            let col = Place::new(name).size(bottom - top).options(&["floor=-40", "ceiling=6", "segments=23", "thick=14"]).bind(bind);
            ui::put_at(b, panel, "meter.led_column", &col, x, top);
        }
        for db in [6.0, 0.0, -10.0, -20.0, -30.0, -40.0] {
            let y = top + 2.0 + (bottom - top - 4.0) * (ceiling - db) / (ceiling - floor);
            let label = if db > 0.0 { format!("+{db}") } else { format!("{db}") };
            ui::silk_right(b, panel, &label, 944.0, y, 7.0, INK, 700.0);
            ui::rule(b, panel, 946.0, y - 0.5, 3.0, 1.0, "#eceef080");
        }
        ui::silk(b, panel, "L", 957.0, bottom + 12.0, 7.0, DIM, 700.0);
        ui::silk(b, panel, "R", 979.0, bottom + 12.0, 7.0, DIM, 700.0);
        ui::plain(b, panel, "dB", 1008.0, top + 4.0, 7.5, DIM, 700.0);
    });

    let params = vec![
        ui::param("sslch/hpf", "HPF", ui::printed(HPF, Unit::Hz), 20.0),
        ui::param("sslch/lpf", "LPF", ui::printed(LPF, Unit::Hz), 22000.0),
        ui::toggle("sslch/filters_in", "Filters In", true),
        ui::db("sslch/lf_gain", "LF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param("sslch/lf_freq", "LF Freq", ui::printed(LF, Unit::Hz), 100.0),
        ui::toggle("sslch/lf_bell", "LF Bell", false),
        ui::db("sslch/lmf_gain", "LMF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param("sslch/lmf_freq", "LMF Freq", ui::printed(LMF, Unit::Hz), 800.0),
        ui::param("sslch/lmf_q", "LMF Q", ui::printed(QS, Unit::None), 1.0),
        ui::toggle("sslch/lmf_div3", "LMF ÷3", false),
        ui::db("sslch/hmf_gain", "HMF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param("sslch/hmf_freq", "HMF Freq", ui::printed(HMF, Unit::Hz), 3000.0),
        ui::param("sslch/hmf_q", "HMF Q", ui::printed(QS, Unit::None), 1.0),
        ui::toggle("sslch/hmf_x3", "HMF ×3", false),
        ui::db("sslch/hf_gain", "HF Gain", -15.0, 15.0, 0.0).featured(),
        ui::param("sslch/hf_freq", "HF Freq", ui::printed(HF, Unit::Hz), 8000.0),
        ui::toggle("sslch/hf_bell", "HF Bell", false),
        ui::toggle("sslch/eq_in", "EQ In", true).featured(),
        ui::toggle("sslch/phase", "Phase", false),
        ui::db("sslch/trim", "Trim", -24.0, 24.0, 0.0),
        ui::percent("sslch/drive", "Drive", 0.0),
    ];
    let streams = vec![ui::level_stream("sslch/out", "Output Level", 2)];
    Face { file: "02-ssl-channel", plugin: "eq", scene, params, streams }
}
