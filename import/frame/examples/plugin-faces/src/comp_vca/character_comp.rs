//! The digitally-controlled analogue character compressor — laid out from
//! photographs of the Empirical Labs Distressor (EL8): a black 1U front with
//! white hairline section frames.
//!
//! Left: the name, the IN key with its lamp, the 16-lamp gain-reduction
//! ladder (numbers printed *above* the lamps, lit right to left, green →
//! yellow → orange → red as reduction deepens), and the ratio row, whose
//! coloured lamps are the switch (click one to pick it; NUKE at the end).
//! Then the unlit REDLINE / 1% THD pair, and the DETECTOR and AUDIO lamp
//! columns over their grey keys. Right: four big cream dials — INPUT,
//! ATTACK, RELEASE, OUTPUT — whose 0–10 numerals are engraved on the
//! skirt and turn past a fixed index on the header line, and the POWER
//! rocker.

use document_proto::{Node, NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Binding, ButtonMode, Control, ControlKind, SlotRole, ValueFormat};
use surface_widgets::Builder;

use super::logo;
use super::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 1500.0;
const H: f64 = 136.0;
const INK: &str = "#eceef0";
const LINE: &str = "#e8eaecd9";
/// The panel's black, for knocking lines out behind print.
const PAINT: &str = "#1b1c1e";

/// Gain-reduction lamps in the order they light: 1 dB first (at the right).
const LADDER: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "12", "14", "17", "20", "23", "26"];
/// The ladder's lamp colours in the same order (photographed).
const LADDER_COLORS: &str = "colors=#46e05a,#46e05a,#46e05a,#46e05a,#46e05a,#46e05a,#f6d43c,#f6d43c,#f6d43c,#f6d43c,#ff8a24,#ff8a24,#ff8a24,#ff3b2f,#ff3b2f,#ff3b2f";
const RATIOS: &[&str] = &["1:1", "2:1", "3:1", "4:1", "6:1", "10:1", "20:1", "Nuke"];
const RATIO_COLORS: &[&str] = &["#ff3b2f", "#46e05a", "#46e05a", "#46e05a", "#f6d43c", "#ff8a24", "#ff3b2f", "#dfe6ee"];
const DIAL_ROW: f64 = 73.0;
const DIAL: f64 = 110.0;
const HEADER: f64 = 12.0;

/// A position across the front, as a fraction of its width (measured off
/// the photograph).
fn fx(f: f64) -> f64 {
    W * f
}

/// A patch of panel paint, knocking a frame line out behind print.
fn knock(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, w: f64, h: f64) {
    b.add(panel, draw::rect("knockout", cx - w / 2.0, cy - h / 2.0, w, h, draw::solid(PAINT)));
}

/// A header set on the frame line.
fn header(b: &mut Builder<'_>, panel: NodeId, s: &str, cx: f64, cy: f64, size: f64) {
    knock(b, panel, cx, cy, ui::text_w(s, size) + size * 1.2, size * 1.5);
    ui::silk(b, panel, s, cx, cy, size, INK, 800.0);
}

/// A lamp cell: lamp centre, legend centre, cell box (x, y, w, h).
type Cell = ((f64, f64), (f64, f64), (f64, f64, f64, f64));

/// Radio lamps over a choice param: each option a lamp in its own colour
/// with its legend, a `Select` button whose `Led` is the lit lens.
/// `cells` = (lamp centre, legend centre, cell box).
fn lamp_select(b: &mut Builder<'_>, panel: NodeId, addr: &str, opts: &[&str], colors: &[&str], cells: &[Cell], size: f64) {
    let n = opts.len() as u32;
    for (i, (label, ((lx, ly), (tx, ty), (x, y, w, h)))) in opts.iter().zip(cells).enumerate() {
        let cell = b.add(panel, draw::frame(&format!("{addr} {label}"), *x, *y, *w, *h));
        let d = 8.0;
        b.add(cell, draw::circle("bezel", lx - x, ly - y, d / 2.0 + 1.4, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])));
        b.add(cell, ui::dark_lens(lx - x, ly - y, d / 2.0, colors[i]));
        let lit = b.add(cell, ui::lens(lx - x, ly - y, d / 2.0, colors[i]));
        let tw = ui::text_w(label, size) + size;
        let t = b.add(cell, ui::text("legend", label, tx - x - tw / 2.0, ty - y, tw, size, INK, TextAlign::Center, 800.0, false));
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: n } }).slot(SlotRole::Led, lit.0).slot(SlotRole::Label, t.0);
        c.binding = Some(bind(addr));
        b.controls.insert(cell, c);
    }
}

/// A light-grey key with the pill-shaped dimple the unit's keys have —
/// decoration: the lamps above it are the switch.
fn grey_key(b: &mut Builder<'_>, panel: NodeId, cx: f64, cy: f64, w: f64, h: f64, dark: bool) -> Node {
    let (c0, c1, c2) = if dark { ("#56585c", "#3a3c40", "#26282b") } else { ("#f0f0ee", "#cfd0cf", "#a9aaa9") };
    let cap = draw::rect("key", cx - w / 2.0, cy - h / 2.0, w, h, draw::linear(180.0, &[(0.0, c0), (0.5, c1), (1.0, c2)]));
    let cap = draw::with_effects(
        draw::stroked(draw::rounded(cap, 2.0), "#000000c0", 1.0),
        vec![draw::drop_shadow("#000000c0", 0.0, 2.0, 3.0), draw::inner_shadow("#ffffff70", 0.0, 1.0, 0.0)],
    );
    b.add(panel, cap);
    let dimple = draw::rect(
        "dimple",
        cx - w * 0.36,
        cy - h * 0.28,
        w * 0.72,
        h * 0.56,
        draw::linear(180.0, &[(0.0, if dark { "#2c2e31" } else { "#b8b9b8" }), (1.0, if dark { "#4a4c50" } else { "#e6e6e4" })]),
    );
    draw::rounded(dimple, h * 0.28)
}

/// A big cream dial under its header, with a fixed index on the line.
fn dial(b: &mut Builder<'_>, panel: NodeId, addr: &str, legend: &str, format: Option<ValueFormat>, cx: f64) {
    let mut p = Place::new(legend).size(DIAL).variant("cream").touch(TOUCH_MIN).bind(bind(addr));
    p.format = format;
    let k = ui::put(b, panel, "knob.numbered_cream", &p, cx, DIAL_ROW);
    ui::strip_legend(b, k);
    // The kit's index triangle stands above a 2U panel's dial; on 1U the
    // index is a short white tick down from the header line.
    for id in b.scene.walk(k) {
        if let Some(n) = b.scene.node_mut(id)
            && n.name == "panel index"
        {
            n.visible = false;
        }
    }
    ui::rule(b, panel, cx - 1.0, HEADER, 2.0, DIAL_ROW - DIAL / 2.0 - HEADER - 1.0, INK);
}

pub fn face() -> Face {
    let scene = unit("charcomp", "Character", "charcomp/input", "charcomp/output", "charcomp/in");
    let params = vec![
        ui::param("charcomp/input", "Input", surface_proto::Range::linear(0.0, 10.0, surface_proto::Unit::None), 5.0).featured(),
        ui::ms("charcomp/attack", "Attack", 0.05, 50.0, 5.0).featured(),
        ui::ms("charcomp/release", "Release", 50.0, 3500.0, 400.0).featured(),
        ui::db("charcomp/output", "Output", -12.0, 24.0, 0.0).featured(),
        ui::choice("charcomp/ratio", "Ratio", RATIOS, 3).featured(),
        ui::choice("charcomp/detector", "Detector", &["Off", "HP", "HP+"], 0),
        ui::choice("charcomp/audio", "Audio", &["Clean", "Dist 2", "Dist 3"], 0),
        ui::toggle("charcomp/in", "In", true),
    ];
    let streams = vec![ui::gr_stream("charcomp/gr", "Gain Reduction")];
    Face { file: "10-character-comp", plugin: "comp", scene, params, streams }
}

/// The unit's size (a 1U front).
pub const SIZE: (f64, f64) = (W, H);

/// The ratio lamps' order, for a host mapping them to its own ratios.
pub const RATIO_NAMES: &[&str] = RATIOS;

/// The unit's front, its controls under `ns/` (`attack`, `release`,
/// `ratio`, `detector`, `audio`, `in`, the `gr` stream), its logotype
/// `name`, its INPUT and OUTPUT dials bound to `input` and `output` (a
/// host may drive a threshold and a level with them) and its IN key to
/// `on`.
pub fn unit(ns: &str, name: &str, input: &str, output: &str, on: &str) -> document_proto::Scene {
    let a = |id: &str| format!("{ns}/{id}");
    canvas("Character Compressor", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "black", W, H, "ears");
        if let Some(n) = b.scene.node_mut(panel) {
            n.fills = vec![draw::linear(178.0, &[(0.0, "#232427"), (0.5, "#1b1c1e"), (1.0, "#141517")])];
        }

        // ── The frames ─────────────────────────────────────────────────
        ui::section(b, panel, fx(0.055), HEADER, fx(0.363) - fx(0.055), 112.0, LINE, 1.6);
        ui::section(b, panel, fx(0.363), HEADER, fx(0.937) - fx(0.363), 112.0, LINE, 1.6);
        // The ladder stands in the left frame's top line.
        knock(b, panel, (fx(0.105) + fx(0.360)) / 2.0, HEADER, fx(0.360) - fx(0.105), 8.0);

        // ── The name, set as the unit's logotype (big, heavy, slanted
        // white), and the IN key ─────────────────────────────────────
        let script = logo::Style { size: 34.0, weight: 800.0, italic: true, tracking: -0.02 };
        logo::mark(b, panel, name, fx(0.057), 62.0, 200.0, script, "#f2f2f0", TextAlign::Left);
        let dimple = grey_key(b, panel, fx(0.2), 67.0, 44.0, 20.0, true);
        b.add(panel, dimple);
        lamp_toggle(b, panel, on, fx(0.2) - 24.0, fx(0.245), 67.0);

        // ── Gain reduction: 16 lamps, numbers above ────────────────────
        let ladder = Place::new("Gain Reduction").size(23.7 * LADDER.len() as f64).scale(LADDER).options(&["gr", LADDER_COLORS]).reversed().bind(Binding::stream(a("gr")));
        let lad = ui::put(b, panel, "meter.led_ladder", &ladder, fx(0.2325), 30.0);
        for id in b.scene.walk(lad) {
            if let Some(n) = b.scene.node_mut(id)
                && n.name == "threshold"
            {
                n.visible = false;
            }
        }
        let (x0, pitch) = (fx(0.2325) - 23.7 * 8.0 + 23.7 / 2.0, 23.7);
        for (i, l) in LADDER.iter().rev().enumerate() {
            ui::silk(b, panel, l, x0 + i as f64 * pitch, HEADER, 8.5, INK, 800.0);
        }
        ui::silk(b, panel, "Gain Reduction", fx(0.231), 44.0, 10.0, INK, 800.0);

        // ── Ratio: the lamps are the switch ────────────────────────────
        let cells: Vec<_> = (0..RATIOS.len())
            .map(|i| {
                let x = fx(0.1385) + i as f64 * 35.5;
                ((x, 110.0), (x, 97.0), (x - 17.0, 89.0, 34.0, 28.0))
            })
            .collect();
        lamp_select(b, panel, &a("ratio"), RATIOS, RATIO_COLORS, &cells, 8.0);
        header(b, panel, "Ratio", fx(0.216), 124.0, 11.0);
        // The unlit REDLINE and 1% THD lamps, by their sine.
        let sx = fx(0.297);
        let sine: String = (0..=24)
            .map(|i| {
                let t = f64::from(i) / 24.0;
                format!("{} {:.2} {:.2} ", if i == 0 { "M" } else { "L" }, t * 30.0, 8.0 - 7.0 * (t * std::f64::consts::TAU).sin())
            })
            .collect();
        let mut s = draw::stroked(draw::path("sine", sx - 15.0, 48.0, 30.0, 16.0, sine.trim_end().to_string(), draw::solid("#00000000")), INK, 1.6);
        s.fills.clear();
        b.add(panel, s);
        ui::rule(b, panel, sx - 17.0, 56.0, 34.0, 1.0, INK);
        ui::lamp(b, panel, fx(0.320), 46.0, 7.0, "#ff3b2f", false);
        ui::silk_left(b, panel, "Redline", fx(0.320) + 7.0, 46.0, 8.5, INK, 800.0);
        ui::lamp(b, panel, fx(0.320), 66.0, 7.0, "#f6d43c", false);
        ui::silk_left(b, panel, "1% THD", fx(0.320) + 7.0, 66.0, 8.5, INK, 800.0);
        let d = grey_key(b, panel, fx(0.3375), 94.0, 36.0, 20.0, false);
        b.add(panel, d);
        ui::silk(b, panel, "Ratio", fx(0.3375), 114.0, 10.5, INK, 800.0);

        // ── Detector and audio: lamp columns over grey keys ────────────
        for (addr, opts, colors, keyx, legend) in [
            (a("detector"), &["Off", "HP", "HP+"], &["#46e05a", "#f6d43c", "#ff3b2f"], fx(0.391), "Detector"),
            (a("audio"), &["Clean", "Dist 2", "Dist 3"], &["#46e05a", "#f6d43c", "#ff3b2f"], fx(0.443), "Audio"),
        ] {
            let lx = keyx + 18.0;
            let cells: Vec<_> = (0..3)
                .map(|i| {
                    let y = 26.0 + i as f64 * 20.0;
                    ((lx, y), (lx - 26.0, y), (lx - 48.0, y - 10.0, 56.0, 20.0))
                })
                .collect();
            lamp_select(b, panel, &addr, opts, colors, &cells, 8.5);
            let d = grey_key(b, panel, keyx, 94.0, 36.0, 20.0, false);
            b.add(panel, d);
            header(b, panel, legend, keyx, 114.0, 10.5);
        }

        // ── The dials ──────────────────────────────────────────────────
        for (addr, legend, format, f, hx) in [
            (input.to_string(), "Input", Some(ValueFormat::decimals(1)), 0.517, 0.4875),
            (a("attack"), "Attack", Some(ValueFormat::decimals(1)), 0.622, 0.592),
            (a("release"), "Release", None, 0.727, 0.696),
            (output.to_string(), "Output", Some(ValueFormat::decimals(1)), 0.833, 0.803),
        ] {
            header(b, panel, legend, fx(hx), HEADER, 11.5);
            dial(b, panel, &addr, legend, format, fx(f));
        }

        // ── Power ──────────────────────────────────────────────────────
        header(b, panel, "Power", fx(0.91), HEADER, 11.5);
        let (px, py) = (fx(0.9075), 70.0);
        let bezel = draw::rect("rocker bezel", px - 17.0, py - 30.0, 34.0, 60.0, draw::linear(180.0, &[(0.0, "#0b0b0c"), (1.0, "#1c1d1f")]));
        b.add(
            panel,
            draw::with_effects(
                draw::stroked(draw::rounded(bezel, 2.0), "#000000", 1.0),
                vec![draw::drop_shadow("#ffffff22", 0.0, 1.0, 0.0), draw::inner_shadow("#000000", 0.0, 2.0, 3.0)],
            ),
        );
        let rocker = draw::rect("rocker", px - 12.0, py - 25.0, 24.0, 50.0, draw::linear(180.0, &[(0.0, "#3a3b3e"), (0.48, "#1d1e20"), (0.52, "#0d0d0e"), (1.0, "#1a1b1d")]));
        b.add(panel, draw::with_effects(draw::rounded(rocker, 1.5), vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)]));
    })
}

/// The IN key's lamp and legend: a toggle whose `Led` is the lamp, over the
/// dark key to its left.
fn lamp_toggle(b: &mut Builder<'_>, panel: NodeId, addr: &str, x0: f64, cx: f64, cy: f64) {
    let h = TOUCH_MIN;
    let cell = b.add(panel, draw::frame("In", x0, cy - h / 2.0, cx - x0 + 36.0, h));
    let (lx, ly) = (cx - x0, h / 2.0);
    b.add(cell, draw::circle("bezel", lx, ly, 5.4, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])));
    b.add(cell, ui::dark_lens(lx, ly, 4.0, "#ff3b2f"));
    let lit = b.add(cell, ui::lens(lx, ly, 4.0, "#ff3b2f"));
    let t = b.add(cell, ui::text("legend", "In", lx + 8.0, ly, 40.0, 10.5, INK, TextAlign::Left, 800.0, true));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, lit.0).slot(SlotRole::Label, t.0);
    c.binding = Some(bind(addr));
    b.controls.insert(cell, c);
}
