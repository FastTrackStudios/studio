//! The desktop VCA compressor in a host's FX column (see [`crate::lane`]).
//!
//! - **Lane**: gloss black between the walnut cheeks and the aluminium
//!   trim; THRESHOLD with its BELOW / ABOVE lamps, COMPRESSION (the
//!   biggest knob), OUTPUT GAIN, the logotype, and the big decibel meter at
//!   the right. The meter shows what the METER keys last picked (the keys,
//!   and the mains key, stay on the rack and tall faces).
//! - **Tall**: the unit's own arrangement, which already has the column's
//!   proportions — the three knobs over the POWER and METER keys at the
//!   left, the meter over the logotype at the right — with every control
//!   larger than the desktop face gives it.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Ink, Place, TOUCH_MIN, draw};
use surface_proto::{Binding, Unit};
use surface_widgets::Builder;

use super::super::logo;
use super::super::ui::{self, bind};
use super::{INK, LegendOpt, RATIO_VALUES, RATIOS, THRESHOLDS, power_key};
use crate::face::{Face, canvas};
use crate::lane::Shape;

pub fn faces() -> Vec<Face> {
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let scene = canvas(
                &format!("VCA Compressor ({})", shape.suffix()),
                |b, root| build(b, root, shape),
            );
            let mut params = vec![
                ui::db("vcacomp/threshold", "Threshold", -40.0, 0.0, -20.0).featured(),
                ui::param(
                    "vcacomp/ratio",
                    "Compression",
                    ui::printed(RATIO_VALUES, Unit::Ratio),
                    4.0,
                )
                .featured(),
                ui::db("vcacomp/output", "Output Gain", -20.0, 20.0, 0.0).featured(),
            ];
            if shape == Shape::Tall {
                params.push(ui::choice(
                    "vcacomp/meter",
                    "Meter",
                    &["Input", "Output", "Gain Change"],
                    2,
                ));
            }
            let streams = vec![
                ui::gr_stream("vcacomp/gr", "Gain Reduction"),
                ui::level_stream("vcacomp/meter_level", "Meter (METER select)", 1),
            ];
            Face {
                file: shape.file("09-vca-comp"),
                plugin: "comp",
                scene,
                params,
                streams,
            }
        })
        .collect()
}

/// Where a shape puts things, panel px.
struct Spots {
    /// The trim strips' heights, top and bottom.
    trim: (f64, f64),
    /// THRESHOLD, COMPRESSION, OUTPUT GAIN: shaft x, and their sizes.
    knobs: [(f64, f64); 3],
    /// The knob row's shaft line, and the headers' baseline.
    row: f64,
    header: f64,
    /// The threshold lamps' line.
    lamps: f64,
    /// The meter's centre and width.
    meter: (f64, f64, f64),
    /// The logotype's centre (its mark line).
    logo: (f64, f64, f64),
}

const fn spots(shape: Shape) -> Spots {
    match shape {
        Shape::Lane => Spots {
            trim: (8.0, 9.0),
            knobs: [(88.0, 52.0), (212.0, 58.0), (346.0, 50.0)],
            row: 95.0,
            header: 19.0,
            lamps: 34.0,
            meter: (632.0, 82.0, 182.0),
            logo: (470.0, 76.0, 0.6),
        },
        Shape::Tall => Spots {
            trim: (16.0, 24.0),
            knobs: [(108.0, 64.0), (250.0, 78.0), (404.0, 64.0)],
            row: 150.0,
            header: 46.0,
            lamps: 70.0,
            meter: (612.0, 122.0, 232.0),
            logo: (612.0, 252.0, 0.82),
        },
    }
}

fn build(b: &mut Builder<'_>, root: NodeId, shape: Shape) {
    let (w, h) = shape.size();
    let s = spots(shape);
    let x0 = 34.0 * w / 900.0;
    ui::fit_root(b, root, w, h);
    let panel = ui::panel(b, root, "panel.desktop", "black", w, h, "wood");
    b.add(
        panel,
        draw::rect(
            "gloss",
            x0,
            0.0,
            w - 2.0 * x0,
            h,
            draw::linear(
                170.0,
                &[
                    (0.0, "#ffffff14"),
                    (0.35, "#ffffff05"),
                    (0.5, "#ffffff00"),
                    (1.0, "#00000026"),
                ],
            ),
        ),
    );
    trim(b, panel, (x0, w - 2.0 * x0), 0.0, s.trim.0);
    trim(b, panel, (x0, w - 2.0 * x0), h - s.trim.1, s.trim.1);
    let tall = shape == Shape::Tall;
    let head = if tall { 14.0 } else { 12.0 };

    // ── Threshold, with its below/above lamps ──────────────────────────
    let (tx, td) = s.knobs[0];
    ui::silk(b, panel, "Threshold", tx, s.header, head, INK, 800.0);
    let lamp_w = if tall { 60.0 } else { 46.0 };
    let lamps = Place::new("Threshold lamps")
        .size(lamp_w)
        .scale(&["0.5", "1"])
        .options(&["gr", "colors=#f2c230,#e8483a"])
        .bind(Binding::stream("vcacomp/gr"));
    let lamps = ui::put(b, panel, "meter.led_ladder", &lamps, tx, s.lamps + 5.0);
    for id in b.scene.walk(lamps) {
        if let Some(n) = b.scene.node_mut(id)
            && n.name == "threshold"
        {
            n.visible = false;
        }
    }
    let side = if tall { 9.0 } else { 8.0 };
    ui::silk_right(
        b,
        panel,
        "Below",
        tx - lamp_w * 0.4,
        s.lamps,
        side,
        INK,
        700.0,
    );
    ui::silk_left(
        b,
        panel,
        "Above",
        tx + lamp_w * 0.4,
        s.lamps,
        side,
        INK,
        700.0,
    );
    knob(
        b,
        panel,
        ("vcacomp/threshold", "Threshold"),
        THRESHOLDS,
        (tx, s.row, td),
    );

    // ── Compression: the biggest knob ──────────────────────────────────
    let (cx, cd) = s.knobs[1];
    ui::silk(b, panel, "Compression", cx, s.header, head, INK, 800.0);
    knob(
        b,
        panel,
        ("vcacomp/ratio", "Compression"),
        RATIOS,
        (cx, s.row, cd),
    );

    // ── Output gain ────────────────────────────────────────────────────
    let (ox, od) = s.knobs[2];
    if tall {
        ui::silk(b, panel, "Output", ox, s.header - 17.0, head, INK, 800.0);
        ui::silk(b, panel, "Gain", ox, s.header, head, INK, 800.0);
    } else {
        ui::silk(b, panel, "Output Gain", ox, s.header, head, INK, 800.0);
    }
    knob(
        b,
        panel,
        ("vcacomp/output", "Output Gain"),
        &["-20", "-10", "0", "+10", "+20"],
        (ox, s.row, od),
    );

    if tall {
        keys(b, panel, s.knobs);
    }

    // ── The meter, and the name set as the unit's logotype ─────────────
    let (mx, my, mw) = s.meter;
    let meter = Place::new("Decibels")
        .size(mw)
        .legend("DECIBELS")
        .options(&["glass"])
        .bind(Binding::stream("vcacomp/meter_level"));
    ui::put(b, panel, "meter.vu_decibel_white", &meter, mx, my);
    logotype(b, panel, s.logo);
}

/// A knurled black knob with a tick ring and the unit's numerals round it,
/// its shaft at (`cx`, `cy`), `size` across.
fn knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    (addr, name): (&str, &str),
    scale: &[&str],
    (cx, cy, size): (f64, f64, f64),
) -> NodeId {
    let ticks = vec![""; scale.len()];
    let p = Place::new(name)
        .size(size)
        .scale(&ticks)
        .ink(Ink::Color(INK.into()))
        .touch(TOUCH_MIN)
        .bind(bind(addr));
    let k = ui::put(b, panel, "knob.knurled_insert", &p, cx, cy);
    ui::strip_legend(b, k);
    let label = if size > 60.0 { 11.0 } else { 10.0 };
    ui::scale_labels(
        b,
        panel,
        cx,
        cy,
        size * 0.72 + 9.0,
        scale,
        300.0,
        label,
        INK,
    );
    k
}

/// A brushed-aluminium trim strip across the front, `(x, w)`.
fn trim(b: &mut Builder<'_>, panel: NodeId, (x, w): (f64, f64), y: f64, h: f64) {
    let strip = draw::rect(
        "trim",
        x,
        y,
        w,
        h,
        draw::linear(
            180.0,
            &[
                (0.0, "#f4f5f6"),
                (0.35, "#c9ccd0"),
                (0.7, "#a4a8ad"),
                (1.0, "#dcdfe2"),
            ],
        ),
    );
    b.add(
        panel,
        draw::with_effects(
            strip,
            vec![
                draw::inner_shadow("#00000066", 0.0, -1.0, 0.0),
                draw::inner_shadow("#ffffffaa", 0.0, 1.0, 0.0),
            ],
        ),
    );
    b.add(
        panel,
        draw::rect(
            "trim groove",
            x,
            y + h * 0.62,
            w,
            1.0,
            draw::solid("#00000033"),
        ),
    );
}

/// The tall face's key row under the knobs: POWER under THRESHOLD, the
/// interlocked METER keys under COMPRESSION and OUTPUT GAIN, bracketed.
fn keys(b: &mut Builder<'_>, panel: NodeId, knobs: [(f64, f64); 3]) {
    let py = 262.0;
    let px = knobs[0].0;
    ui::silk(b, panel, "Power", px, py - 30.0, 11.0, INK, 700.0);
    power_key(b, panel, px, py, 74.0);
    let mid = (knobs[1].0 + knobs[2].0) / 2.0;
    let sel = Place::new("Meter")
        .size(62.0)
        .variant("white")
        .bind(bind("vcacomp/meter"))
        .options(&["Input", "Output", "Gain Change"])
        .ink(Ink::Color(INK.into()))
        .opt_legend(10.0);
    ui::put(b, panel, "button.piano_bank", &sel, mid, py - 10.0);
    let half = 102.0;
    let (bl, br, by) = (mid - half, mid + half, py + 30.0);
    ui::rule(b, panel, bl, by - 8.0, 1.5, 8.0, INK);
    ui::rule(b, panel, br - 1.5, by - 8.0, 1.5, 8.0, INK);
    ui::rule(b, panel, bl, by, half - 34.0, 1.5, INK);
    ui::rule(b, panel, br - (half - 34.0), by, half - 34.0, 1.5, INK);
    ui::silk(b, panel, "Meter", mid, by + 1.0, 11.0, INK, 800.0);
}

/// The logotype: heavy, tightly set lowercase, a dark jewel-like dot, a
/// bold word, and the line under it — centred on (`cx`, `cy`) at scale `k`
/// of the desktop face's.
fn logotype(b: &mut Builder<'_>, panel: NodeId, (cx, cy, k): (f64, f64, f64)) {
    let heavy = logo::Style {
        size: 40.0 * k,
        weight: 900.0,
        italic: false,
        tracking: -0.04,
    };
    let bold = logo::Style {
        size: 30.0 * k,
        weight: 800.0,
        italic: false,
        tracking: -0.01,
    };
    logo::mark(
        b,
        panel,
        "vca",
        cx - 150.0 * k,
        cy - 2.0 * k,
        138.0 * k,
        heavy,
        "#ecebe6",
        TextAlign::Right,
    );
    b.add(
        panel,
        draw::with_effects(
            draw::circle(
                "mark dot",
                cx - 1.0 * k,
                cy,
                5.5 * k,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#2a2b2e"), (1.0, "#020203")]),
            ),
            vec![draw::drop_shadow("#ffffff26", 0.0, 1.0, 0.0)],
        ),
    );
    logo::mark(
        b,
        panel,
        "comp",
        cx + 12.0 * k,
        cy + 2.0 * k,
        140.0 * k,
        bold,
        "#ecebe6",
        TextAlign::Left,
    );
    ui::silk(
        b,
        panel,
        "Compressor/Limiter",
        cx,
        cy + 34.0 * k,
        13.0 * k,
        "#ecebe6",
        800.0,
    );
}
