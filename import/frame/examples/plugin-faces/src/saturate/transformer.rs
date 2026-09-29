//! The transformer: iron and copper, in a console module's blue-grey.
//!
//! A window into the chassis shows the part doing the work — a laminated
//! core with its winding in the window of the E, clamped in a steel frame,
//! its copper warming as it is driven — wired to the solder tags below it.
//! The wing knobs are colour-coded the way a console codes them: red for
//! gain (DRIVE, OUTPUT), grey for the core's own trims (CORE, SAG, MIX),
//! blue for the frequency-dependent ones (LOW BLOOM, HYSTERESIS). Output
//! level on two LED columns.

use document_proto::NodeId;
use surface_kit::draw;
use surface_widgets::Builder;

use crate::delay::art;
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#e8edf3";
const DIM: &str = "#94a0ad";

fn a(s: &str) -> String {
    format!("transformer/{s}")
}

/// The chassis window: the transformer, its tags and leads.
fn chassis(b: &mut Builder<'_>, panel: NodeId) {
    let (x, y, w, h) = (236.0, 20.0, 392.0, 150.0);
    let win = parts::well(b, panel, "Chassis", (x, y, w, h), draw::linear(180.0, &[(0.0, "#2a2e33"), (1.0, "#1a1d21")]), 6.0);
    // Pressed-steel chassis ribs.
    for i in 0..5 {
        let yy = 16.0 + f64::from(i) * 28.0;
        parts::rule(b, win, 8.0, w - 8.0, yy, 1.0, "#00000040");
        parts::rule(b, win, 8.0, w - 8.0, yy + 1.0, 1.0, "#ffffff0d");
    }
    // The terminal strip at the right: a phenolic board with four tags.
    let (sx, sy, sw, sh) = (300.0, 18.0, 58.0, 114.0);
    b.add(win, draw::with_effects(draw::rounded(draw::rect("terminal strip", sx, sy, sw, sh, draw::linear(90.0, &[(0.0, "#4a2c18"), (0.5, "#6b4125"), (1.0, "#3e2413")])), 3.0), vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 4.0)]));
    let tags = [("#c8392b", "In +"), ("#1b1b1d", "In −"), ("#2f6fd0", "Out +"), ("#e9e6de", "Out −")];
    let (tx0, tw) = (54.0, 214.0);
    for (i, (col, label)) in tags.iter().enumerate() {
        let ty = sy + 16.0 + i as f64 * 27.0;
        // A lead from the winding's edge to its tag: a smooth S.
        let (x0, y0) = (tx0 + tw - 20.0 + i as f64 * 4.0, 40.0 + i as f64 * 14.0);
        let (x1, y1) = (sx + 10.0, ty);
        let pts: Vec<(f64, f64)> = (0..=16)
            .map(|k| {
                let t = f64::from(k) / 16.0;
                let e = t * t * (3.0 - 2.0 * t);
                (x0 + (x1 - x0) * t, y0 + (y1 - y0) * e)
            })
            .collect();
        art::path_in(b, win, "lead", art::ribbon(&pts, 2.6), draw::solid(col));
        b.add(win, draw::rounded(draw::rect("tag", sx + 6.0, ty - 4.0, 16.0, 8.0, draw::linear(180.0, &[(0.0, "#f0d27a"), (0.5, "#c9a24a"), (1.0, "#8a6a2a")])), 1.5));
        b.add(win, draw::circle("solder", sx + 12.0, ty, 3.4, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff"), (0.5, "#b9bcc0"), (1.0, "#6a6d72")])));
        parts::silk_left(b, win, label, sx + 26.0, ty, Ink::new(5.5, "#e9d6b8").weight(800.0).track(0.06).plain());
    }
    art::transformer(b, win, (tx0, 16.0, tw - 60.0, 104.0), 0.55);
    parts::silk(b, win, "Iron core · 1:2 · 600 Ω", 150.0, 140.0, Ink::new(6.0, "#7f8a96").weight(800.0).track(0.2));
    // Two mounting screws in the chassis.
    art::screw(b, win, (22.0, 22.0), 4.0, 30.0);
    art::screw(b, win, (w - 22.0, h - 22.0), 4.0, 70.0);
}

pub fn face() -> Face {
    let scene = canvas("Transformer", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.brushed", "black", W, H, "ears");
        parts::underpaint(b, panel, &[(0.0, "#434a53"), (0.5, "#323840"), (1.0, "#22272e")]);

        // ── Nameplate ──────────────────────────────────────────────────
        parts::silk_left(b, panel, "Transformer", 50.0, 34.0, Ink::new(15.0, INK).weight(800.0).track(0.06));
        parts::rule(b, panel, 50.0, 200.0, 47.0, 1.0, "#e8edf340");
        parts::silk_left(b, panel, "Line output stage", 50.0, 60.0, Ink::new(6.8, DIM).weight(700.0).track(0.18));
        parts::silk_left(b, panel, "Nickel · steel lamination", 50.0, 74.0, Ink::new(6.8, DIM).weight(700.0).track(0.18));
        parts::lamp(b, panel, (58.0, 118.0), 8.0, "#ff4a36", true);
        parts::silk_left(b, panel, "Power", 72.0, 118.0, Ink::new(6.5, DIM).weight(800.0).track(0.2));

        chassis(b, panel);

        // ── Output meters ──────────────────────────────────────────────
        let (mx, my) = (672.0, 26.0);
        for (i, x) in [mx, mx + 22.0].into_iter().enumerate() {
            parts::led_column(b, panel, if i == 0 { "Out L" } else { "Out R" }, &a("out"), i as u8, (x, my, 14.0, 120.0), 16, (-30.0, 6.0));
        }
        for (db, frac) in [("+6", 0.0), ("0", 6.0 / 36.0), ("-6", 12.0 / 36.0), ("-18", 24.0 / 36.0), ("-30", 1.0)] {
            parts::silk_left(b, panel, db, mx + 42.0, my + 4.0 + frac * 112.0, Ink::new(6.0, DIM).weight(700.0).track(0.0));
        }
        parts::silk(b, panel, "Output", mx + 18.0, 160.0, Ink::new(6.5, DIM).weight(800.0).track(0.2));

        // ── Knob row ───────────────────────────────────────────────────
        parts::groove(b, panel, 40.0, W - 40.0, 182.0);
        let row = 240.0;
        let legend = Ink::new(8.0, INK).weight(800.0).track(0.14);
        let value = Ink::new(7.5, DIM).weight(600.0).track(0.02).plain();
        let scale = Ink::new(6.5, DIM).weight(700.0).track(0.0).plain();
        let knobs: [(&str, &str, &str, f64, f64, &[(f64, &str)]); 7] = [
            ("drive", "Drive", "red", 104.0, 64.0, parts::PCT_MARKS),
            ("core", "Core", "grey", 228.0, 60.0, super::TRIM_MARKS),
            ("bloom", "Low Bloom", "blue", 350.0, 48.0, super::TRIM_MARKS),
            ("hysteresis", "Hysteresis", "blue", 456.0, 48.0, super::TRIM_MARKS),
            ("sag", "Sag", "grey", 562.0, 48.0, super::TRIM_MARKS),
            ("mix", "Mix", "grey", 710.0, 50.0, parts::PCT_MARKS),
            ("output", "Output", "red", 842.0, 58.0, super::OUT_MARKS),
        ];
        for (addr, name, variant, x, d, marks) in knobs {
            let k = parts::knob(b, panel, "knob.marconi", variant, &a(addr), name, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 7.0, sweep, marks, 21, scale);
            parts::legend_at(b, k, name, 294.0, legend);
            parts::value_at(b, k, 309.0, value);
        }
        parts::vgroove(b, panel, 636.0, 198.0, 312.0);
    });

    let params = vec![
        parts::percent(&a("drive"), "Drive", 30.0).featured(),
        parts::trim(&a("core"), "Core").featured(),
        parts::trim(&a("bloom"), "Low Bloom"),
        parts::trim(&a("hysteresis"), "Hysteresis"),
        parts::trim(&a("sag"), "Sag"),
        parts::percent(&a("mix"), "Mix", 100.0).featured(),
        super::output(&a("output")),
    ];
    let streams = vec![parts::level_stream(&a("out"), "Output Level", 2)];
    Face { file: "03-transformer", plugin: "saturate", scene, params, streams }
}
