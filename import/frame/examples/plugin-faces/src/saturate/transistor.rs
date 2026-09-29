//! The transistor stage, as the box it's best known in: a fuzz pedal.
//!
//! Hammered silver enclosure, black print. The three knobs every fuzz has
//! across the top — GAIN, TONE, LEVEL (output) — then the circuit's own
//! trims: EDGE (how hard the corner is), GATE (the starved, spitting end),
//! SAG and MIX. The mode toggle picks the family's two profiles (a clean
//! transistor drive, or the fuzz). One stomp switch with its LED, and the
//! part itself printed beside it — a PNP transistor's symbol.

use document_proto::NodeId;
use surface_kit::{Ink as KitInk, Place, draw};
use surface_widgets::Builder;

use crate::delay::art;
use crate::delay::parts::{self, Ink, bind};
use crate::face::{Face, canvas};

const W: f64 = 760.0;
const H: f64 = 520.0;
const INK: &str = "#17181a";
const DIM: &str = "#3d4045";

fn a(s: &str) -> String {
    format!("transistor/{s}")
}

/// A PNP transistor's schematic symbol, printed at (`cx`, `cy`), radius `r`.
fn symbol(b: &mut Builder<'_>, lid: NodeId, (cx, cy): (f64, f64), r: f64, ink: &str) {
    let wt = 2.2;
    // The envelope: a circle, stroked.
    let ring: Vec<(f64, f64)> = (0..=72).map(|i| draw::polar(cx, cy, r, f64::from(i) * 5.0)).collect();
    b.add(lid, parts::stroke_path("envelope", (cx - r - wt, cy - r - wt, 2.0 * (r + wt), 2.0 * (r + wt)), &[ring], wt, ink, true));
    // Base: a lead in from the left to the bar.
    let bar_x = cx - r * 0.28;
    let (ex, ey) = (cx + r * 0.42, cy - r * 0.62);
    let (kx, ky) = (cx + r * 0.42, cy + r * 0.62);
    let leads = [
        vec![(cx - r - 16.0, cy), (bar_x, cy)],
        // Collector and emitter: diagonals out to the right, then up and down.
        vec![(bar_x, cy - r * 0.22), (ex, ey), (ex, cy - r - 16.0)],
        vec![(bar_x, cy + r * 0.22), (kx, ky), (kx, cy + r + 16.0)],
    ];
    let mut leads = parts::lines_node("leads", &leads, wt, ink);
    leads.stroke.cap = document_proto::StrokeCap::None;
    b.add(lid, leads);
    let mut bar = parts::lines_node("base bar", &[vec![(bar_x, cy - r * 0.5), (bar_x, cy + r * 0.5)]], wt * 2.0, ink);
    bar.stroke.cap = document_proto::StrokeCap::None;
    b.add(lid, bar);
    // PNP: the emitter's arrow points in, toward the bar.
    let (mx, my) = ((bar_x + ex) / 2.0 - 1.0, (cy - r * 0.22 + ey) / 2.0 + 0.5);
    let (ux, uy) = ((bar_x - ex), (cy - r * 0.22 - ey));
    let len = ux.hypot(uy);
    let (ux, uy) = (ux / len, uy / len);
    let (px, py) = (-uy, ux);
    let s = 7.0;
    let arrow = [(mx + ux * s, my + uy * s), (mx - ux * s * 0.3 + px * s * 0.6, my - uy * s * 0.3 + py * s * 0.6), (mx - ux * s * 0.3 - px * s * 0.6, my - uy * s * 0.3 - py * s * 0.6)];
    let (ax, ay) = (mx - s, my - s);
    let local: Vec<(f64, f64)> = arrow.iter().map(|(x, y)| (x - ax, y - ay)).collect();
    b.add(lid, draw::path("emitter arrow", ax, ay, 2.0 * s, 2.0 * s, draw::polygon(&local), draw::solid(ink)));
    let t = Ink::new(8.0, ink).weight(800.0).track(0.0);
    parts::silk(b, lid, "E", ex + 10.0, cy - r - 10.0, t);
    parts::silk(b, lid, "C", kx + 10.0, cy + r + 10.0, t);
    parts::silk(b, lid, "B", cx - r - 22.0, cy - 10.0, t);
}

pub fn face() -> Face {
    let scene = canvas("Transistor", |b, root| {
        parts::fit_root(b, root, W, H);
        let lid = art::enclosure(b, root, (W, H), &[(0.0, "#d6dbe0"), (0.5, "#b4bbc2"), (1.0, "#8c949c")], (0.03, 0.05));

        // ── Title ──────────────────────────────────────────────────────
        parts::silk(b, lid, "Transistor", W / 2.0 - 4.0, 48.0, Ink::new(24.0, INK).weight(900.0).track(0.12));
        parts::silk(b, lid, "Fuzz · Germanium PNP", W / 2.0 - 4.0, 74.0, Ink::new(7.5, DIM).weight(800.0).track(0.3));

        let scale = Ink::new(7.0, DIM).weight(800.0).track(0.0).plain();
        let legend = Ink::new(10.5, INK).weight(900.0).track(0.18);
        let value = Ink::new(7.5, DIM).weight(700.0).track(0.02).plain();

        // ── The three every fuzz has ───────────────────────────────────
        let r1 = 156.0;
        let big: [(&str, &str, f64, &[(f64, &str)]); 3] = [("drive", "Gain", 176.0, parts::PCT_MARKS), ("tone", "Tone", 376.0, super::TRIM_MARKS), ("output", "Level", 576.0, super::OUT_MARKS)];
        for (addr, name, x, marks) in big {
            let k = parts::knob(b, lid, "knob.bakelite", "black", &a(addr), name, (x, r1), 76.0);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, lid, (x, r1), 45.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, r1 + 66.0, legend);
            parts::value_at(b, k, r1 + 82.0, value);
        }

        // ── The circuit's trims ────────────────────────────────────────
        let r2 = 310.0;
        let small: [(&str, &str, f64, &[(f64, &str)]); 4] = [("edge", "Edge", 150.0, super::TRIM_MARKS), ("gate", "Gate", 290.0, super::TRIM_MARKS), ("sag", "Sag", 430.0, super::TRIM_MARKS), ("mix", "Mix", 590.0, parts::PCT_MARKS)];
        for (addr, name, x, marks) in small {
            let k = parts::knob(b, lid, "knob.bakelite", "black", &a(addr), name, (x, r2), 48.0);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, lid, (x, r2), 31.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, r2 + 48.0, legend.size(9.0));
        }
        parts::vrule(b, lid, 512.0, r2 - 34.0, r2 + 54.0, 1.0, "#17181a40");

        // ── Foot ───────────────────────────────────────────────────────
        let fy = 452.0;
        let mode = Place::new("Mode").size(38.0).variant("chrome").legend("Mode").options(&["Fuzz", "Drive"]).bind(bind(&a("fuzz"))).ink(KitInk::Color(INK.into()));
        let sw = parts::put(b, lid, "switch.toggle_bat", &mode, 170.0, fy - 16.0);
        parts::caption_style(b, sw, Ink::new(7.5, INK).weight(900.0).track(0.18));
        let bypass = Place::new("Bypass").size(38.0).variant("red").bind(bind(&a("on")));
        let on = parts::footswitch(b, lid, &bypass, (376.0, fy));
        parts::label_at(b, on, "On", (408.0, fy - 40.0), Ink::new(7.5, DIM).weight(900.0).track(0.2));
        symbol(b, lid, (586.0, fy - 18.0), 26.0, INK);
    });

    let params = vec![
        parts::percent(&a("drive"), "Gain", 60.0).featured(),
        parts::trim(&a("tone"), "Tone").featured(),
        super::output(&a("output")),
        parts::trim(&a("edge"), "Edge"),
        parts::trim(&a("gate"), "Gate"),
        parts::trim(&a("sag"), "Sag"),
        parts::percent(&a("mix"), "Mix", 100.0),
        parts::toggle(&a("fuzz"), "Fuzz", true),
        parts::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "04-transistor", plugin: "saturate", scene, params, streams: Vec::new() }
}
