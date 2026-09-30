//! Hall: the room itself, drawn as its architect did — a shoebox concert
//! hall in plan, on a blueprint: the stage and its apron at one end, the
//! stalls in rows either side of the centre aisle, the balconies along the
//! long walls, the doors at the back. The sound leaves the stage and fills
//! it: wavefronts spread from the source and ring through the hall, slower
//! to die away the longer the decay; the hall itself grows with the size.
//!
//! Beside the drawing, the house: velvet and gilt, the name in a theatre's
//! serif, and the controls.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

/// Blueprint ink and paper.
const LINE: &str = "#cdb8ff";
const PAPER: (&str, &str) = ("#1d1340", "#110b28");
/// The house: velvet and gilt.
const GILT: &str = "#d9b56a";
const GILT_DIM: &str = "#a8874a";

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// Open strokes in `color`.
fn lines(b: &mut Builder<'_>, parent: NodeId, name: &str, segs: &[Vec<(f64, f64)>], weight: f64, color: &str) -> NodeId {
    b.add(parent, ui::strokes(name, segs, weight, draw::solid(color)))
}

/// The hall in plan, in `plan` (`w × h`): drawn at full size, squeezed
/// toward the stage by the size motion on its group.
#[allow(clippy::too_many_lines)]
fn hall(b: &mut Builder<'_>, plan: NodeId, w: f64, h: f64) {
    // Faint drafting grid over the whole sheet.
    let mut grid: Vec<Vec<(f64, f64)>> = (1..(w / 14.0) as i32).map(|i| vec![(f64::from(i) * 14.0, 0.0), (f64::from(i) * 14.0, h)]).collect();
    grid.extend((1..(h / 14.0) as i32).map(|i| vec![(0.0, f64::from(i) * 14.0), (w, f64::from(i) * 14.0)]));
    lines(b, plan, "grid", &grid, 0.5, &ui::alpha(LINE, 0.08));

    // The building: grows with the size, the stage end fixed.
    let (x0, y0, hh) = (18.0, 18.0, h - 52.0);
    let len = w - 36.0;
    let g = b.add(plan, draw::frame("Hall", x0, y0, len, hh));
    motion::add(b, g, Motion::new(Target::StretchX).input(format!("norm({VERB}/size)")).range(0.62, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(260.0, 28.0)));
    motion::add(b, g, Motion::new(Target::StretchY).input(format!("norm({VERB}/size)")).range(0.82, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(260.0, 28.0)));
    // Walls (double line: masonry), the stage end squared off.
    let wall = |b: &mut Builder<'_>, inset: f64, weight: f64, alpha: f64| {
        let r = draw::stroked(draw::rect("wall", inset, inset, len - 2.0 * inset, hh - 2.0 * inset, draw::solid("#00000000")), &ui::alpha(LINE, alpha), weight);
        let mut r = r;
        r.fills.clear();
        b.add(g, r);
    };
    wall(b, 0.0, 1.8, 0.95);
    wall(b, 4.0, 0.8, 0.55);
    // The stage and its curved apron.
    let stage_w = len * 0.16;
    let mut stage = draw::rect("stage", 4.0, 4.0, stage_w, hh - 8.0, draw::solid(&ui::alpha(LINE, 0.1)));
    stage.stroke = document_proto::Stroke { paints: vec![draw::solid(&ui::alpha(LINE, 0.7))], weight: 1.0, ..document_proto::Stroke::default() };
    b.add(g, stage);
    let apron: Vec<(f64, f64)> = (0..=24)
        .map(|i| {
            let t = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(i) / 24.0;
            (4.0 + stage_w + t.cos() * hh * 0.12, hh / 2.0 + t.sin() * (hh / 2.0 - 12.0))
        })
        .collect();
    ui::line(b, g, "apron", &apron, 1.0, &ui::alpha(LINE, 0.7));
    ui::silk(b, g, "Stage", 4.0 + stage_w / 2.0, 16.0, 6.0, &ui::alpha(LINE, 0.8), 800.0);
    // The stalls: rows either side of the centre aisle, gently curved to
    // face the stage.
    let (sx0, sx1) = (stage_w + hh * 0.16 + 14.0, len - 34.0);
    let aisle = 7.0;
    let mut seats = Vec::new();
    let rows = ((sx1 - sx0) / 9.0) as i32;
    for r in 0..rows {
        let x = sx0 + f64::from(r) * 9.0;
        // Each row a shallow arc about the stage: its ends lean back.
        let bow = 4.0;
        let half = |y0: f64, y1: f64| -> Vec<(f64, f64)> {
            (0..=6)
                .map(|k| {
                    let y = y0 + (y1 - y0) * f64::from(k) / 6.0;
                    let off = ((y - hh / 2.0) / (hh / 2.0)).powi(2) * bow;
                    (x + off, y)
                })
                .collect()
        };
        seats.push(half(16.0, hh / 2.0 - aisle));
        seats.push(half(hh / 2.0 + aisle, hh - 16.0));
    }
    lines(b, g, "stalls", &seats, 1.1, &ui::alpha(LINE, 0.45));
    // Balconies along the long walls, and the side aisles under them.
    for y in [10.0, hh - 10.0] {
        lines(b, g, "balcony", &[vec![(sx0 - 6.0, y), (len - 10.0, y)]], 2.2, &ui::alpha(LINE, 0.35));
    }
    // Doors at the back: gaps with their swing arcs.
    for y in [hh * 0.3, hh * 0.7] {
        let arc: Vec<(f64, f64)> = (0..=8).map(|i| {
            let t = std::f64::consts::FRAC_PI_2 * f64::from(i) / 8.0;
            (len - t.sin() * 9.0, y - 9.0 + t.cos() * 9.0)
        }).collect();
        ui::line(b, g, "door", &arc, 0.8, &ui::alpha(LINE, 0.6));
    }
    // Dimension line along the bottom.
    let dy = -9.0;
    lines(b, g, "dimension", &[vec![(0.0, dy), (len, dy)], vec![(0.0, dy - 3.0), (0.0, dy + 3.0)], vec![(len, dy - 3.0), (len, dy + 3.0)]], 0.7, &ui::alpha(LINE, 0.55));

    // ── The sound: a source on the stage, and its wavefronts ──────────
    let (cx, cy) = (x0 + 4.0 + stage_w * 0.55, y0 + hh / 2.0);
    let mut room = draw::frame("Air", x0 + 5.0, y0 + 5.0, len - 10.0, hh - 10.0);
    room.clips_content = true;
    let room = b.add(plan, room);
    // The air shares the hall's size.
    motion::add(b, room, Motion::new(Target::StretchX).input(format!("norm({VERB}/size)")).range(0.62, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(260.0, 28.0)));
    motion::add(b, room, Motion::new(Target::StretchY).input(format!("norm({VERB}/size)")).range(0.82, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(260.0, 28.0)));
    let r_max = len * 1.05;
    let (lx, ly) = (cx - (x0 + 5.0), cy - (y0 + 5.0));
    for i in 0..5 {
        let ring = b.add(room, draw::frame(&format!("wavefront {i}"), lx - r_max, ly - r_max, 2.0 * r_max, 2.0 * r_max));
        let mut c = draw::stroked(draw::circle("front", r_max, r_max, r_max - 3.0, draw::solid("#00000000")), LINE, 5.0);
        c.fills.clear();
        b.add(ring, ui::glowing(c, LINE, 0.6, 5.0));
        // Each front spreads from the source and fades; they come slower
        // and ring longer as the decay grows.
        let phase = format!("fract(time * lerp(0.55, 0.16, norm({VERB}/decay)) + {})", f64::from(i) / 5.0);
        motion::add(b, ring, Motion::new(Target::StretchX).input(phase.clone()).range(0.02, 1.0).pivot(0.5, 0.5));
        motion::add(b, ring, Motion::new(Target::StretchY).input(phase.clone()).range(0.02, 1.0).pivot(0.5, 0.5));
        let fade = format!("(1 - {phase}) * param({VERB}/on)");
        motion::add(b, ring, Motion::new(Target::Opacity).input(fade).range(0.0, 1.0));
    }
    // The source.
    let src = b.add(plan, draw::emissive(draw::circle("source", cx, cy, 4.5, draw::solid(GILT)), draw::hex(GILT), 1.0, 12.0));
    motion::add(b, src, Motion::new(Target::Opacity).input(format!("0.35 + 0.65 * param({VERB}/on)")).range(0.0, 1.0));

    // Title block, as a drawing has.
    let (tx, ty) = (w - 212.0, h - 28.0);
    let mut tb = draw::rect("title block", tx, ty, 194.0, 20.0, draw::solid("#00000000"));
    tb.stroke = document_proto::Stroke { paints: vec![draw::solid(&ui::alpha(LINE, 0.6))], weight: 0.8, ..document_proto::Stroke::default() };
    tb.fills.clear();
    b.add(plan, tb);
    ui::silk_left(b, plan, "Concert Hall · Plan", tx + 6.0, ty + 10.0, 6.5, LINE, 800.0);
    ui::silk_left(b, plan, "1 : 200 · A-2", tx + 128.0, ty + 10.0, 5.5, &ui::alpha(LINE, 0.7), 700.0);
}

#[must_use]
pub fn face() -> Face {
    let scene = kit::lane("Hall", |b, root| {
        // The house: velvet in a gilt frame.
        let mut panel = draw::frame("House", 0.0, 0.0, kit::W, kit::H);
        panel.fills = vec![draw::linear(180.0, &[(0.0, "#3d1b63"), (0.55, "#281043"), (1.0, "#180929")])];
        panel.corner_radii = document_proto::CornerRadii::uniform(6.0);
        panel.stroke = document_proto::Stroke { paints: vec![draw::solid(GILT_DIM)], weight: 2.0, ..document_proto::Stroke::default() };
        let panel = b.add(root, panel);
        // The velvet's pile.
        let pile: Vec<Vec<(f64, f64)>> = (0..80).map(|i| {
            let x = 612.0 + f64::from(i) * 4.4;
            vec![(x, 4.0), (x, kit::H - 4.0)]
        }).collect();
        b.add(panel, ui::strokes("pile", &pile, 1.6, draw::solid("#ffffff07")));

        // ── The drawing ────────────────────────────────────────────────
        let (px, py, pw, ph) = (10.0, 10.0, 592.0, kit::H - 20.0);
        let mut sheet = draw::frame("Blueprint", px, py, pw, ph);
        sheet.fills = vec![draw::linear(160.0, &[(0.0, PAPER.0), (1.0, PAPER.1)])];
        sheet.clips_content = true;
        sheet.corner_radii = document_proto::CornerRadii::uniform(3.0);
        sheet.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 8.0)];
        let sheet = b.add(panel, sheet);
        hall(b, sheet, pw, ph);
        // A gilt frame round the drawing.
        let mut rim = draw::rect("gilt", px - 2.0, py - 2.0, pw + 4.0, ph + 4.0, draw::solid("#00000000"));
        rim.fills.clear();
        rim.stroke = document_proto::Stroke { paints: vec![draw::linear(90.0, &[(0.0, GILT_DIM), (0.5, GILT), (1.0, GILT_DIM)])], weight: 2.0, ..document_proto::Stroke::default() };
        b.add(panel, rim);

        // ── The house's name, and its controls ─────────────────────────
        let x0 = 622.0;
        let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(700.0);
        crate::suite_studio_blues::parts::set(b, panel, "Hall", x0 + 38.0, 30.0, 24.0, GILT, &serif, 0.06);
        b.add(panel, ui::text("sub", "Concert hall reverberation", x0 + 82.0, 26.0, 150.0, 6.5, GILT_DIM, TextAlign::Left, 700.0, true));
        b.add(panel, ui::text("sub2", "Stage · stalls · balcony", x0 + 82.0, 36.0, 150.0, 5.5, &ui::alpha(GILT_DIM, 0.8), TextAlign::Left, 600.0, true));
        let ink = "#ecdcbc";
        let dim = "#b39c77";
        kit::bypass(b, panel, VERB, x0 + 14.0, 104.0, 28.0, "purple", "In");
        let brass = ["color=#c9a45c", "pointer=#2a1a0c", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#6e5428"];
        kit::knob3d(b, panel, &a("decay"), "Decay", 54.0, (x0 + 78.0, 80.0), &brass, ink, dim);
        let small = ["color=#1a0a0e", "pointer=#d9b56a", "index=line", "index_w=0.12", "skirt=plain", "skirt_color=#a8874a"];
        for (i, (id, legend)) in [("size", "Size"), ("predelay", "Pre-Delay"), ("damping", "Damp"), ("level", "Level")].iter().enumerate() {
            kit::knob3d(b, panel, &a(id), legend, 34.0, (x0 + 140.0 + 54.0 * i as f64, 96.0), &small, ink, dim);
        }
    });
    kit::reverb_face("81-verb-hall", scene)
}
