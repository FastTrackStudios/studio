//! The digital stage: a clipper and bit-crusher that doesn't pretend to be
//! anything else.
//!
//! Matte black, one colour. The LCD draws the transfer curve honestly — a
//! staircase, because a short word *is* a staircase, flattened at the
//! ceiling where the clipper stops it — with the word length, sample rate
//! and ceiling read out beside it. Two keys pick the family's profiles
//! (clip or crush). Colour-capped knobs: DRIVE, then the clipper's CEILING,
//! the crusher's BITS and RATE, TONE and DITHER, and MIX and OUTPUT last.
//! Output level on two LED columns.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_proto::{Range, Unit};
use surface_widgets::Builder;

use crate::delay::art;
use crate::delay::parts::{self, Ink, Screen, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 300.0;
const INK: &str = "#e9edf6";
const DIM: &str = "#8a92a4";
const LCD: &str = "#ff6f9a";
const LCD_DIM: &str = "#7a2f47";
const MODES: &[&str] = &["Clip", "Crush"];

fn a(s: &str) -> String {
    format!("digital_sat/{s}")
}

/// The transfer curve: a 4-bit staircase under a -3 dB ceiling.
fn curve(b: &mut Builder<'_>, win: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    art::graticule(b, win, (x, y, w, h), (8, 4), "#ff6f9a14");
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    // Unity, dashed: what the curve bends away from.
    let mut dash = String::new();
    for i in 0..24 {
        let t0 = f64::from(i) / 24.0;
        let t1 = t0 + 0.5 / 24.0;
        dash.push_str(&art::ribbon(&[(x + w * t0, y + h * (1.0 - t0)), (x + w * t1, y + h * (1.0 - t1))], 0.8));
    }
    art::path_in(b, win, "unity", dash, draw::solid(LCD_DIM));
    art::path_in(b, win, "axes", format!("{}{}", art::ribbon(&[(x, cy), (x + w, cy)], 0.7), art::ribbon(&[(cx, y), (cx, y + h)], 0.7)), draw::solid(LCD_DIM));
    // Drive 2×, a soft ceiling at 0.7, quantised to 4 bits (±7 steps).
    let ceiling = 0.7_f64;
    let levels = 7.0;
    let mut pts = Vec::new();
    let n = 240;
    let mut last: Option<f64> = None;
    for i in 0..=n {
        let u = -1.0 + 2.0 * f64::from(i) / f64::from(n);
        let driven = (u * 2.2).tanh() / 2.2_f64.tanh();
        let clipped = driven.clamp(-ceiling, ceiling);
        let q = (clipped * levels).round() / levels;
        let px = cx + u * w / 2.0;
        let py = cy - q * h / 2.0;
        if let Some(prev) = last
            && (prev - py).abs() > 0.01
        {
            pts.push((px, prev));
        }
        pts.push((px, py));
        last = Some(py);
    }
    art::trace(b, win, "transfer", &pts, 1.8, LCD);
    art::tag(b, win, "In →", (x + w - 26.0, cy + 8.0), 6.0, LCD_DIM);
    art::tag(b, win, "Out", (cx + 4.0, y + 6.0), 6.0, LCD_DIM);
}

pub fn face() -> Face {
    let scene = canvas("Digital Saturation", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "black", W, H, "ears");
        parts::underpaint(b, panel, &[(0.0, "#1e2129"), (0.5, "#171a20"), (1.0, "#0f1116")]);

        // ── Nameplate, mode ────────────────────────────────────────────
        parts::silk_left(b, panel, "Digital", 50.0, 34.0, Ink::new(15.0, INK).weight(800.0).track(0.1));
        b.add(panel, draw::rect("accent", 50.0, 46.0, 30.0, 3.0, draw::solid(LCD)));
        parts::silk_left(b, panel, "Clipper · bit-crusher", 50.0, 62.0, Ink::new(6.8, DIM).weight(700.0).track(0.18));
        let modes = Place::new("Mode").size(46.0).variant("red").bind(bind(&a("mode"))).options(MODES);
        let modes = parts::put(b, panel, "button.square_bank", &modes, 124.0, 108.0);
        parts::fit_legends(b, modes, 8.0);
        parts::silk(b, panel, "Stage", 124.0, 141.0, Ink::new(6.5, DIM).weight(800.0).track(0.22));

        // ── LCD ────────────────────────────────────────────────────────
        let win = parts::screen(b, panel, "LCD", (250.0, 24.0, 450.0, 118.0), Screen { glass: ("#12070b", "#1b0a10"), ink: LCD, mesh: false });
        curve(b, win, (12.0, 10.0, 250.0, 98.0));
        let rows = [("Bits", "bits", 24.0), ("Rate", "rate", 56.0), ("Ceiling", "ceiling", 88.0)];
        for (tag, addr, y) in rows {
            art::tag(b, win, tag, (282.0, y), 7.0, LCD_DIM);
            parts::readout(b, win, tag, bind(&a(addr)), (320.0, y, 118.0), 17.0, LCD, TextAlign::Right);
        }

        // ── Output meters ──────────────────────────────────────────────
        let (mx, my) = (744.0, 28.0);
        for (i, x) in [mx, mx + 20.0].into_iter().enumerate() {
            parts::led_column(b, panel, if i == 0 { "Out L" } else { "Out R" }, &a("out"), i as u8, (x, my, 13.0, 110.0), 18, (-36.0, 3.0));
        }
        for (db, frac) in [("0", 3.0 / 39.0), ("-12", 15.0 / 39.0), ("-24", 27.0 / 39.0), ("-36", 1.0)] {
            parts::silk_left(b, panel, db, mx + 38.0, my + 3.0 + frac * 104.0, Ink::new(6.0, DIM).weight(700.0).track(0.0));
        }
        parts::silk(b, panel, "Output", mx + 16.0, 154.0, Ink::new(6.5, DIM).weight(800.0).track(0.2));
        let clip = Place::new("Hard knee").size(38.0).variant("red").legend("Hard").bind(bind(&a("hard")));
        parts::put(b, panel, "button.square", &clip, 866.0, 74.0);
        parts::silk(b, panel, "Knee", 866.0, 102.0, Ink::new(6.5, DIM).weight(800.0).track(0.2));

        // ── Knob row ───────────────────────────────────────────────────
        parts::groove(b, panel, 40.0, W - 40.0, 170.0);
        let row = 222.0;
        let legend = Ink::new(8.0, INK).weight(800.0).track(0.14);
        let value = Ink::new(7.5, DIM).weight(600.0).track(0.02).plain();
        let scale = Ink::new(6.5, DIM).weight(700.0).track(0.0).plain();
        let rate_marks = [(0.0, "1k"), (parts::log_pos(8000.0, 1000.0, 48000.0), "8k"), (1.0, "48k")];
        let knobs: [(&str, &str, &str, f64, f64, &[(f64, &str)]); 8] = [
            ("drive", "Drive", "red", 96.0, 54.0, parts::PCT_MARKS),
            ("ceiling", "Ceiling", "magenta", 206.0, 50.0, &[(0.0, "-24"), (0.5, "-12"), (1.0, "0")]),
            ("bits", "Bits", "blue", 316.0, 50.0, &[(0.0, "1"), (7.0 / 23.0, "8"), (15.0 / 23.0, "16"), (1.0, "24")]),
            ("rate", "Rate", "blue", 426.0, 50.0, &rate_marks),
            ("tone", "Tone", "grey", 536.0, 44.0, super::TRIM_MARKS),
            ("dither", "Dither", "grey", 636.0, 44.0, parts::PCT_MARKS),
            ("mix", "Mix", "black", 746.0, 46.0, parts::PCT_MARKS),
            ("output", "Output", "red", 858.0, 50.0, super::OUT_MARKS),
        ];
        for (addr, name, variant, x, d, marks) in knobs {
            let k = parts::knob(b, panel, "knob.collet", variant, &a(addr), name, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 6.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, 274.0, legend);
            if !matches!(addr, "bits" | "rate" | "ceiling") {
                parts::value_at(b, k, 288.0, value);
            }
        }
    });

    let params = vec![
        parts::percent(&a("drive"), "Drive", 40.0).featured(),
        parts::param(&a("ceiling"), "Ceiling", Range { steps: Some(49), ..Range::linear(-24.0, 0.0, Unit::Db) }, -3.0).featured(),
        parts::param(&a("bits"), "Bits", Range { steps: Some(24), ..Range::linear(1.0, 24.0, Unit::Custom { suffix: "bit".into() }) }, 12.0).featured(),
        parts::param(&a("rate"), "Rate", Range::log(1000.0, 48000.0, Unit::Hz), 48000.0),
        parts::trim(&a("tone"), "Tone"),
        parts::percent(&a("dither"), "Dither", 0.0),
        parts::percent(&a("mix"), "Mix", 100.0),
        super::output(&a("output")),
        parts::choice(&a("mode"), "Mode", MODES, 0).featured(),
        parts::toggle(&a("hard"), "Hard Knee", false),
    ];
    let streams = vec![parts::level_stream(&a("out"), "Output Level", 2)];
    Face { file: "05-digital", plugin: "saturate", scene, params, streams }
}
