//! Tape: a multi-head tape echo with its lid off. The loop of tape lies
//! folded in its tank, runs out past the erase and record heads and the
//! three playback heads to the capstan, and back. It runs at the speed the
//! delay time makes it — the time is how long the tape takes from the
//! record head to the playback head, so a short time runs it fast — and
//! each playback head's lamp flashes on its repeat, dimmer as the repeats
//! die away.
//!
//! Beside the deck, the echo's own black panel in its olive vinyl case.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use crate::delay::art;
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const INK: &str = "#ece6d6";
const DIM: &str = "#9c978a";
const OXIDE: &str = "#5a3a22";
const LAMP: &str = "#ff9a3c";

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

/// The deck in `deck` (`w × h`): the tank, the heads, the capstan, and the
/// tape running through them.
#[allow(clippy::too_many_lines)]
fn deck(b: &mut Builder<'_>, deck: NodeId, w: f64, h: f64) {
    // The tank the loop lies folded in.
    let (kx, ky, kw, kh) = (10.0, 10.0, 132.0, h - 20.0);
    let tank = draw::rounded(draw::rect("tank", kx, ky, kw, kh, draw::linear(180.0, &[(0.0, "#0a0a0a"), (1.0, "#171717")])), 4.0);
    b.add(deck, draw::with_effects(draw::stroked(tank, "#2c2c2c", 1.0), vec![draw::inner_shadow("#000000", 0.0, 2.0, 6.0)]));
    // The folds: the loop, lying in loose S-bends.
    let folds: Vec<Vec<(f64, f64)>> = (0..15)
        .map(|i| {
            let x = kx + 12.0 + f64::from(i) * 7.6 + crate::reverb::jitter(f64::from(i)) * 1.5;
            (0..=16)
                .map(|k| {
                    let t = f64::from(k) / 16.0;
                    (x + (t * std::f64::consts::TAU * 1.3 + f64::from(i)).sin() * 3.2, ky + 8.0 + (kh - 16.0) * t)
                })
                .collect()
        })
        .collect();
    b.add(deck, ui::strokes("folds", &folds, 2.6, draw::solid(OXIDE)));
    b.add(deck, ui::strokes("fold sheen", &folds, 0.6, draw::solid("#b0805066")));
    ui::silk(b, deck, "Tape loop", kx + kw / 2.0, ky + kh - 6.0, 5.5, "#77705f", 800.0);

    // The run: out of the tank along the top past the heads, round the
    // capstan and the tension arm, back along the bottom.
    let top = 62.0;
    let bottom = h - 26.0;
    let (x_in, x_cap) = (kx + kw - 4.0, w - 58.0);
    let run = [(x_in, top), (x_cap, top), (x_cap + 20.0, top + 10.0), (w - 26.0, (top + bottom) / 2.0), (x_cap + 20.0, bottom - 8.0), (x_cap, bottom), (x_in, bottom)];
    art::tape(b, deck, &run, 5.0, OXIDE);

    // The heads, sitting on the top run.
    let heads = [("Erase", 186.0), ("Rec", 222.0), ("1", 272.0), ("2", 316.0), ("3", 360.0)];
    for (label, x) in heads {
        art::head(b, deck, (x, top - 13.0), 18.0, 24.0);
        ui::silk(b, deck, label, x, top + 12.0, 6.0, DIM, 800.0);
    }
    // Each playback head's lamp flashes on its repeat — the n-th head
    // heard n times the time after the note — dimmer as they die away.
    for (i, (_, x)) in heads[2..].iter().enumerate() {
        let n = i + 1;
        let lamp = b.add(deck, draw::emissive(draw::circle("head lamp", *x, top - 32.0, 2.6, draw::solid(LAMP)), draw::hex(LAMP), 1.0, 7.0));
        let flash = format!("pow(1 - fract(time * 1000 / ({n} * param({DLY}/time))), 4) * pow(param({DLY}/feedback), {}) * param({DLY}/on)", n - 1);
        motion::add(b, lamp, Motion::new(Target::Opacity).input(flash).range(0.08, 1.0));
    }
    // Guides, capstan and pinch roller, the tension arm.
    art::post(b, deck, (x_in + 8.0, top), 3.5);
    art::post(b, deck, (x_in + 8.0, bottom), 3.5);
    art::roller(b, deck, (x_cap, top + 13.0), 10.0);
    art::post(b, deck, (x_cap, top - 3.5), 4.0);
    ui::line(b, deck, "tension arm", &[(w - 26.0, (top + bottom) / 2.0), (w - 16.0, bottom + 6.0)], 3.0, "#8e9296");
    art::post(b, deck, (w - 26.0, (top + bottom) / 2.0), 4.5);
    ui::silk(b, deck, "Capstan", x_cap, top + 32.0, 5.5, DIM, 800.0);

    // The tape moving: splices and print-through marks carried along the
    // top run, left to right, at the speed the time sets (record head to
    // first playback head in one delay time).
    let (mx, mw) = (x_in + 14.0, x_cap - x_in - 26.0);
    let mut lane = draw::frame("tape moving", mx, top - 2.5, mw, 5.0);
    lane.clips_content = true;
    let lane = b.add(deck, lane);
    let pitch = 22.0;
    let marks = b.add(lane, draw::frame("marks", -pitch, 0.0, mw + 2.0 * pitch, 5.0));
    let ticks: Vec<Vec<(f64, f64)>> = (0..((mw + 2.0 * pitch) / pitch) as i32 + 1)
        .map(|i| {
            let x = f64::from(i) * pitch;
            vec![(x, 0.5), (x, 4.5)]
        })
        .collect();
    b.add(marks, ui::strokes("print marks", &ticks, 1.2, draw::solid("#c79a6a88")));
    let gap = heads[2].1 - heads[1].1;
    let speed = format!("fract(time * {:.1} / max(param({DLY}/time), 20) * param({DLY}/on))", gap * 1000.0 / pitch);
    motion::add(b, marks, Motion::new(Target::X).input(speed).range(-pitch, 0.0));
}

#[must_use]
pub fn face() -> Face {
    let scene = kit::lane("Tape", |b, root| {
        // The case: olive vinyl.
        let mut case = draw::frame("Case", 0.0, 0.0, kit::W, kit::H);
        case.fills = vec![draw::linear(180.0, &[(0.0, "#4a5640"), (1.0, "#2f3829")])];
        case.corner_radii = document_proto::CornerRadii::uniform(7.0);
        let case = b.add(root, draw::with_effects(case, vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)]));
        let grain: Vec<Vec<(f64, f64)>> = (0..60)
            .map(|i| {
                let y = 2.0 + f64::from(i) * 2.7;
                vec![(0.0, y), (kit::W, y + crate::reverb::jitter(f64::from(i)) * 2.0)]
            })
            .collect();
        b.add(case, ui::strokes("vinyl grain", &grain, 0.6, draw::solid("#0000001c")));

        // ── The deck, lid off, under smoked acrylic ────────────────────
        let (dx, dy, dw, dh) = (12.0, 12.0, 460.0, kit::H - 24.0);
        let mut dk = draw::frame("Deck", dx, dy, dw, dh);
        dk.fills = vec![draw::linear(180.0, &[(0.0, "#232323"), (1.0, "#141414")])];
        dk.clips_content = true;
        dk.corner_radii = document_proto::CornerRadii::uniform(5.0);
        dk.effects = vec![draw::inner_shadow("#000000", 0.0, 3.0, 8.0)];
        let dk = b.add(case, dk);
        deck(b, dk, dw, dh);
        b.add(dk, draw::rect("acrylic", 0.0, 0.0, dw, dh, draw::linear(165.0, &[(0.0, "#ffffff12"), (0.35, "#ffffff05"), (0.36, "#ffffff00"), (1.0, "#ffffff00")])));

        // ── The panel ──────────────────────────────────────────────────
        let (px, py, pw, ph) = (484.0, 8.0, kit::W - 492.0, kit::H - 16.0);
        let mut panel = draw::frame("Panel", px, py, pw, ph);
        panel.fills = vec![draw::linear(180.0, &[(0.0, "#1c1c1b"), (1.0, "#0e0e0d")])];
        panel.corner_radii = document_proto::CornerRadii::uniform(4.0);
        let panel = b.add(case, draw::with_effects(draw::stroked(panel, "#000000", 1.0), vec![draw::drop_shadow("#00000099", 0.0, 2.0, 4.0), draw::inner_shadow("#ffffff1a", 0.0, 1.0, 0.0)]));
        let script = draw::Lettering::family(draw::family::MICHROMA).weight(400.0);
        crate::suite_studio_blues::parts::set(b, panel, "Tape Echo", 70.0, 20.0, 14.0, INK, &script, 0.04);
        ui::silk_left(b, panel, "Multi-head · Loop tape", 132.0, 20.0, 6.0, DIM, 700.0);
        ui::rule(b, panel, 12.0, 33.0, pw - 200.0, 1.0, "#ece6d633");
        kit::bypass(b, panel, DLY, 28.0, 90.0, 28.0, "amber", "On");
        let knob = ["color=#141414", "pointer=#f1ede2", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#b9bcbf"];
        let knobs = [("time", "Repeat Rate"), ("feedback", "Intensity"), ("level", "Echo Vol"), ("high_pass", "Bass"), ("high_cut", "Treble"), ("mod_depth", "Wow")];
        for (i, (id, legend)) in knobs.iter().enumerate() {
            kit::knob3d(b, panel, &a(id), legend, 38.0, (86.0 + 64.0 * i as f64, 74.0), &knob, INK, DIM);
        }
    });
    kit::delay_face("60-dly-tape", scene)
}
