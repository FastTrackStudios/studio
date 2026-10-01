//! The Amp row's utilities as faces: the noise gate and the patch's level
//! and pan.
//!
//! - Gate (`51-gate`): the input's level, live, rising in a meter; the
//!   threshold a handle across it you drag; below the threshold the
//!   meter is shaded shut, above it the gate is open — and the OPEN / SHUT
//!   lamps say which, as you play. Attack and release under it. The live
//!   level is `gate/in` (dBFS), a value the host feeds.
//! - Level (`53-trim`): the patch's pan knob over a long-throw console
//!   fader for its level, on a dB scale.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::{draw, motion};
use surface_proto::{Control, ControlKind, Orientation, Range, SlotRole, Unit};
use surface_widgets::Builder;

use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_faces::PLUGIN;
use crate::rig_time::kit;

pub const GATE: (f64, f64) = (250.0, 392.0);
pub const TRIM: (f64, f64) = (92.0, 340.0);

const INK: &str = "#e6e9ee";
const DIM: &str = "#7c828c";
const OPEN: &str = "#4ade80";
const SHUT: &str = "#f87171";

/// The panel both sit on: dark, square, flush.
fn panel(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64)) -> NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, h);
    p.fills = vec![draw::linear(180.0, &[(0.0, "#131518"), (1.0, "#0a0b0d")])];
    b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff12", 0.0, 1.0, 0.0)]))
}

/// A vertical fader on `addr`: a slot `len` long at (`cx`, `y`), its cap
/// `cap` tall and `cap_w` wide. Returns the control's root.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fader(b: &mut Builder<'_>, parent: NodeId, addr: &str, (cx, y): (f64, f64), len: f64, (cap_w, cap): (f64, f64), draw_cap: impl FnOnce(&mut Builder<'_>, NodeId, f64, f64)) -> NodeId {
    let bw = cap_w + 8.0;
    let root = b.add(parent, draw::frame(&format!("{addr} fader"), cx - bw / 2.0, y, bw, len));
    let lane = b.add(root, draw::frame("lane", (bw - cap_w) / 2.0, 0.0, cap_w, len));
    let c = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap));
    draw_cap(b, c, cap_w, cap);
    let ctl = Control::new(ControlKind::Fader { orientation: Orientation::Vertical }).slot(SlotRole::Thumb, c.0).bound(ui::bind(addr));
    b.controls.insert(root, ctl);
    root
}

/// A lit word, its brightness following `input` (0…1).
fn lamp(b: &mut Builder<'_>, parent: NodeId, word: &str, (cx, cy): (f64, f64), color: &str, input: String) {
    let w = 46.0;
    let g = b.add(parent, draw::frame(word, cx - w / 2.0, cy - 9.0, w, 18.0));
    b.add(g, draw::stroked(draw::rounded(draw::rect("bezel", 0.0, 0.0, w, 18.0, draw::solid("#0c0d0f")), 4.0), "#2a2d33", 1.0));
    let lit = b.add(g, draw::frame("lit", 0.0, 0.0, w, 18.0));
    b.add(lit, draw::emissive(draw::rounded(draw::rect("glow", 1.0, 1.0, w - 2.0, 16.0, draw::solid(&ui::alpha(color, 0.25))), 3.0), draw::hex(color), 0.8, 8.0));
    b.add(lit, ui::text("word", word, 0.0, 9.0, w, 8.0, color, TextAlign::Center, 800.0, true));
    motion::add(b, lit, Motion::new(Target::Opacity).input(input).range(0.08, 1.0).dynamics(Dynamics::spring(400.0, 36.0)));
}

fn gate() -> Face {
    let (w, h) = GATE;
    let scene = canvas("Gate", |b, root| {
        ui::fit_root(b, root, w, h);
        let p = panel(b, root, GATE);
        let sans = draw::Lettering::family(draw::family::INTER).weight(600.0);
        crate::suite_studio_blues::parts::set(b, p, "GATE", 42.0, 24.0, 16.0, INK, &sans, 0.16);
        kit::bypass(b, p, "gate", w - 30.0, 24.0, 24.0, "green", "On");

        // Open: the input above the threshold (a dB of hysteresis to it).
        let open = "clamp((param(gate/in) - param(gate/threshold)) * 0.7 + 0.3, 0, 1) * param(gate/on) + (1 - param(gate/on))".to_string();
        lamp(b, p, "OPEN", (w * 0.3, 54.0), OPEN, open.clone());
        lamp(b, p, "SHUT", (w * 0.7, 54.0), SHUT, format!("1 - ({open})"));

        // ── The meter ──────────────────────────────────────────────────
        let (mx, my, mw, mh) = (54.0, 78.0, 58.0, h - 108.0);
        // The dB scale beside it.
        for (db, label) in [(0.0, "0"), (-18.0, "-18"), (-36.0, "-36"), (-54.0, "-54"), (-72.0, "-72"), (-90.0, "-90")] {
            let y = my + mh * (db / -90.0);
            b.add(p, ui::strokes("tick", &[vec![(mx - 6.0, y), (mx - 1.0, y)]], 1.0, draw::solid("#ffffff40")));
            b.add(p, ui::text("db", label, mx - 44.0, y, 36.0, 7.0, DIM, TextAlign::Right, 700.0, false));
        }
        let mut track = draw::frame("Meter", mx, my, mw, mh);
        track.fills = vec![draw::linear(90.0, &[(0.0, "#07080a"), (0.5, "#111317"), (1.0, "#07080a")])];
        track.clips_content = true;
        track.corner_radii = document_proto::CornerRadii::uniform(3.0);
        track.effects = vec![draw::inner_shadow("#000000", 0.0, 2.0, 4.0)];
        let track = b.add(p, track);
        // The level, live: green where it clears the threshold.
        let bar = b.add(track, draw::frame("level", 8.0, 0.0, mw - 16.0, mh));
        b.add(bar, draw::emissive(draw::rect("bar", 0.0, 0.0, mw - 16.0, mh, draw::linear(180.0, &[(0.0, "#fde047"), (0.18, "#86efac"), (1.0, "#16a34a")])), draw::hex(OPEN), 0.5, 6.0));
        motion::add(b, bar, Motion::new(Target::StretchY).input("clamp((param(gate/in) + 90) / 90, 0, 1)".to_string()).range(0.0, 1.0).pivot(0.5, 1.0).dynamics(Dynamics::spring(500.0, 40.0)));
        // Below the threshold: shut, shaded over whatever plays there.
        let shade = b.add(track, draw::frame("shut", 0.0, 0.0, mw, mh));
        b.add(shade, draw::rect("shade", 0.0, 0.0, mw, mh, draw::linear(180.0, &[(0.0, "#1a0b0dcc"), (1.0, "#0b0506e6")])));
        let hatch: Vec<Vec<(f64, f64)>> = (0..40).map(|i| {
            let y = f64::from(i) * 9.0;
            vec![(0.0, y), (mw, y - mw * 0.6)]
        }).collect();
        b.add(shade, ui::strokes("hatch", &hatch, 1.0, draw::solid("#f8717114")));
        motion::add(b, shade, Motion::new(Target::StretchY).input("clamp((param(gate/threshold) + 90) / 90, 0, 1)".to_string()).range(0.0, 1.0).pivot(0.5, 1.0).dynamics(Dynamics::spring(400.0, 36.0)));
        // The threshold: a handle across the meter, dragged up and down.
        let cap_h = 20.0;
        fader(b, p, "gate/threshold", (mx + mw / 2.0, my - cap_h / 2.0), mh + cap_h, (mw + 30.0, cap_h), |b, c, cw, ch| {
            b.add(c, draw::rect("line", 0.0, ch / 2.0 - 0.75, cw, 1.5, draw::solid("#fde68a")));
            let tab = draw::rounded(draw::rect("tab", cw - 30.0, 2.0, 30.0, ch - 4.0, draw::linear(180.0, &[(0.0, "#fef3c7"), (1.0, "#fbbf24")])), 4.0);
            b.add(c, draw::with_effects(tab, vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 3.0)]));
            b.add(c, ui::text("thr", "THR", cw - 30.0, ch / 2.0, 30.0, 7.0, "#3a2a06", TextAlign::Center, 800.0, true));
        });

        // ── Threshold, attack, release ─────────────────────────────────
        let rx = 196.0;
        ui::silk(b, p, "Threshold", rx, 92.0, 6.5, DIM, 800.0);
        crate::delay::parts::readout(b, p, "Threshold value", ui::bind("gate/threshold"), (rx - 40.0, 108.0, 80.0), 11.0, "#fde68a", TextAlign::Center);
        let knob = ["color=#1b1e23", "pointer=#86efac", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0a0b0d"];
        kit::knob3d(b, p, "gate/attack", "Attack", 36.0, (rx, 170.0), &knob, INK, DIM);
        kit::knob3d(b, p, "gate/release", "Release", 36.0, (rx, 262.0), &knob, INK, DIM);
        ui::silk(b, p, "Shut below · open above", w / 2.0, h - 14.0, 6.0, DIM, 700.0);
        ui::flush(b, root, w, h);
    });
    let params = vec![
        ui::param("gate/threshold", "Threshold", Range::linear(-90.0, 0.0, Unit::Db), -50.0).featured(),
        ui::param("gate/attack", "Attack", Range::log(0.1, 50.0, Unit::Milliseconds), 1.0),
        ui::param("gate/release", "Release", Range::log(5.0, 500.0, Unit::Milliseconds), 120.0),
        ui::param("gate/in", "Input", Range::linear(-90.0, 0.0, Unit::Db), -90.0),
        ui::toggle("gate/on", "Gate", true).featured(),
    ];
    Face { file: "51-gate", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

fn trim() -> Face {
    let (w, h) = TRIM;
    let scene = canvas("Level", |b, root| {
        ui::fit_root(b, root, w, h);
        let p = panel(b, root, TRIM);
        let sans = draw::Lettering::family(draw::family::INTER).weight(600.0);
        crate::suite_studio_blues::parts::set(b, p, "LEVEL", w / 2.0, 20.0, 12.0, INK, &sans, 0.16);
        // Pan.
        let knob = ["color=#1b1e23", "pointer=#e6e9ee", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0a0b0d"];
        let k = kit::knob3d(b, p, "trim/pan", "Pan", 34.0, (w / 2.0, 58.0), &knob, INK, DIM);
        let _ = k;
        ui::silk(b, p, "L", w / 2.0 - 26.0, 72.0, 6.0, DIM, 800.0);
        ui::silk(b, p, "R", w / 2.0 + 26.0, 72.0, 6.0, DIM, 800.0);
        // The level: a long-throw fader on a dB scale, +24 at the top.
        let (fy, flen) = (122.0, h - 150.0);
        let cap_h = 30.0;
        let travel = flen - cap_h;
        for (db, label) in [(24.0, "+24"), (12.0, "+12"), (6.0, "+6"), (0.0, "0"), (-6.0, "-6"), (-12.0, "-12"), (-24.0, "-24")] {
            let y = fy + cap_h / 2.0 + travel * (24.0 - db) / 48.0;
            let col = if db == 0.0 { INK } else { "#ffffff38" };
            b.add(p, ui::strokes("tick", &[vec![(w / 2.0 + 12.0, y), (w / 2.0 + 18.0, y)]], 1.0, draw::solid(col)));
            b.add(p, ui::text("db", label, w / 2.0 + 20.0, y, 24.0, 6.0, if db == 0.0 { INK } else { DIM }, TextAlign::Left, 700.0, false));
        }
        // The slot.
        let slot = draw::rounded(draw::rect("slot", w / 2.0 - 2.5, fy + cap_h / 2.0, 5.0, travel, draw::solid("#050506")), 2.5);
        b.add(p, draw::with_effects(slot, vec![draw::inner_shadow("#000000", 0.0, 1.0, 2.0), draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0)]));
        fader(b, p, "trim/gain_db", (w / 2.0, fy), flen, (22.0, cap_h), |b, c, cw, ch| {
            let body = draw::rounded(draw::rect("cap", 0.0, 0.0, cw, ch, draw::linear(180.0, &[(0.0, "#5b5f66"), (0.45, "#2c2f34"), (0.55, "#1c1e22"), (1.0, "#3b3e44")])), 3.0);
            b.add(c, draw::with_effects(body, vec![draw::drop_shadow("#000000c0", 0.0, 3.0, 5.0)]));
            b.add(c, draw::rect("line", 1.0, ch / 2.0 - 0.75, cw - 2.0, 1.5, draw::solid("#f4f4f5")));
        });
        crate::delay::parts::readout(b, p, "Level value", ui::bind("trim/gain_db"), (4.0, h - 14.0, w - 8.0), 8.0, INK, TextAlign::Center);
        ui::flush(b, root, w, h);
    });
    let params = vec![
        ui::param("trim/gain_db", "Level", Range::linear(-24.0, 24.0, Unit::Db), 0.0).featured(),
        ui::param("trim/pan", "Pan", Range::linear(-1.0, 1.0, Unit::None), 0.0).featured(),
    ];
    Face { file: "53-trim", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// Both faces.
#[must_use]
pub fn faces() -> Vec<Face> {
    vec![gate(), trim()]
}
