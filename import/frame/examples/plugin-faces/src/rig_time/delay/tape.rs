//! Tape: the boutique suite's pale grey tape echo (`suite_boutique_clean`'s
//! *ECHO* rack) laid out as a lane of the Time module, in the same four
//! bays: the tall E C H O over the printed VU and the time display; the
//! active-head lamps over the big white dial (here the delay time); the
//! black and orange knurled knobs; the brush mark, the emblem and the
//! orange POWER.
//!
//! The VU's needle kicks with every repeat and falls back, harder the more
//! feedback there is; the head lamps flash on their repeats — the n-th
//! head n times the time after the note, dimmer as the repeats die away.

use document_proto::motion::{Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::meters::scale::Card;
use surface_kit::{Place, TOUCH_MIN, draw, motion};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};
use crate::suite_boutique_clean::look::{self, KnobLook};
use crate::suite_funk_rhythm::parts::{self, Type};

/// The accent: delay blue where the original is orange.
const ORANGE: &str = kit::BLUE;
const INK: &str = "#1f2023";
const LAMP: &str = "#e2eeff";

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

/// How loud the repeats are now, 0…1: a kick on every repeat falling
/// away, bigger with more feedback, still when it's off.
fn repeats() -> String {
    format!("pow(1 - fract(time * 1000 / max(param({DLY}/time), 20)), 3) * (0.35 + 0.65 * param({DLY}/feedback) / 0.95) * param({DLY}/on)")
}

fn bay(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), fill: &[(f64, &str)]) -> NodeId {
    let p = look::slab(name, (x, y, w, h), 3.0, vec![draw::linear(180.0, fill), draw::linear(90.0, &[(0.0, "#ffffff00"), (0.5, "#ffffff10"), (1.0, "#ffffff00")])]);
    b.add(parent, draw::with_effects(p, vec![draw::inner_shadow("#ffffff80", 0.0, 1.0, 0.5), draw::drop_shadow("#00000040", 0.0, 1.0, 2.0)]))
}

fn screws(b: &mut Builder<'_>, parent: NodeId, pts: &[(f64, f64)]) {
    for (x, y) in pts {
        b.add(parent, draw::circle("screw", *x, *y, 4.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f4f4f4"), (1.0, "#8a8b8d")])));
        b.add(parent, draw::rect("cross", x - 2.6, y - 0.5, 5.2, 1.0, draw::solid("#3a3a3c")));
        b.add(parent, draw::rect("cross", x - 0.5, y - 2.6, 1.0, 5.2, draw::solid("#3a3a3c")));
    }
}

/// The VU printed on the panel, needle pivoting at (`px`, `py`), scale
/// radius `r` — a real VU card's law (−20 crowded left, 0 VU at about
/// 70 %), orange past 0. The needle rides the repeats.
fn vu(b: &mut Builder<'_>, parent: NodeId, (px, py): (f64, f64), r: f64) {
    let card = Card::Vu;
    let sweep = 100.0;
    let (floor, ceil) = card.range();
    let side = 2.0 * (r + 20.0);
    let c = side / 2.0;
    let root = b.add(parent, draw::frame("VU", px - c, py - c, side, side));
    let at = |db: f64| (card.fraction(db) - 0.5) * sweep;
    let arc: Vec<(f64, f64)> = (0..=60).map(|i| draw::polar(c, c, r, at(floor + (ceil - floor) * f64::from(i) / 60.0))).collect();
    b.add(root, ui::strokes("arc", &[arc], 1.1, draw::solid(INK)));
    let hot: Vec<(f64, f64)> = (0..=16).map(|i| draw::polar(c, c, r + 2.5, at(3.0 * f64::from(i) / 16.0))).collect();
    b.add(root, ui::strokes("over", &[hot], 5.0, draw::solid(ORANGE)));
    let numeral = Type::new(7.5, INK, 400.0).family(draw::family::SAIRA);
    for (db, s) in [(-20.0, "-20"), (-10.0, "10"), (-5.0, "5"), (-3.0, "3"), (0.0, "0"), (1.0, "1"), (3.0, "3+")] {
        let a = at(db);
        let col = if db >= 0.0 { ORANGE } else { INK };
        b.add(root, ui::strokes("tick", &[vec![draw::polar(c, c, r, a), draw::polar(c, c, r + 5.5, a)]], 1.2, draw::solid(col)));
        let (x, y) = draw::polar(c, c, r + 12.0, a);
        parts::centred(b, root, s, (x, y), Type { color: col, ..numeral });
    }
    parts::centred(b, root, "VU", (c, c - r * 0.48), Type::new(14.0, INK, 400.0).family(draw::family::SAIRA).width(85.0));
    // The needle, drawn upright, turned by the repeats from the rest.
    let needle = b.add(root, draw::frame("needle", 0.0, 0.0, side, side));
    b.add(needle, draw::with_effects(draw::rect("blade", c - 0.8, c - r - 2.0, 1.6, r + 2.0, draw::solid("#1a1a1c")), vec![draw::drop_shadow("#00000040", 1.5, 2.0, 1.5)]));
    let (rest, top) = (at(floor), at(1.0));
    motion::add(b, needle, Motion::new(Target::Rotate).input(repeats()).range(rest, top).pivot(0.5, 0.5));
    let base = draw::rounded(draw::rect("pivot cover", c - 9.0, c - 10.0, 18.0, 26.0, draw::linear(90.0, &[(0.0, "#2a2a2c"), (0.5, "#3c3c40"), (1.0, "#141416")])), 7.0);
    b.add(root, draw::with_effects(base, vec![draw::drop_shadow("#00000060", 0.0, 2.0, 3.0)]));
}

fn glow_key(b: &mut Builder<'_>, parent: NodeId, addr: &str, d: f64, at: (f64, f64), color: &str) -> NodeId {
    let opts = vec![format!("color={color}")];
    let p = Place { options: Some(opts), ..Place::new(addr).size(d).bind(ui::bind(addr)).touch(TOUCH_MIN) };
    parts::part(b, parent, "button.glow_key", p, &["bezel"], at)
}

#[must_use]
pub fn face() -> Face {
    let scene = kit::lane("Tape", |b, root| {
        let (w, h) = (kit::W, kit::H);
        let grey = [(0.0, "#e6e7e8"), (1.0, "#cfd0d2")];
        let unit = bay(b, root, "Unit", (0.0, 0.0, w, h), &[(0.0, "#bfc0c2"), (1.0, "#a9aaac")]);
        let t = Type::new(8.0, INK, 500.0).track(0.08).upper();
        let (y, bh) = (6.0, h - 12.0);

        // Bay 1: the letters, the VU, the display.
        let bay1 = bay(b, unit, "Bay 1", (6.0, y, 330.0, bh), &grey);
        for (i, ch) in "ECHO".chars().enumerate() {
            parts::centred(b, bay1, &ch.to_string(), (30.0, 24.0 + 33.0 * i as f64), Type::new(30.0, INK, 300.0).family(draw::family::SAIRA).width(88.0));
        }
        vu(b, bay1, (194.0, 84.0), 54.0);
        let disp = ui::display(b, bay1, "Display", 76.0, 114.0, 236.0, 26.0, "#9aa0a8", "glass");
        ui::lit_text(b, disp, "DELAY TIME", 8.0, 13.0, 70.0, 7.5, "#c9ccd2", TextAlign::Left, 500.0);
        ui::lit_readout(b, disp, "Delay time", ui::bind(&a("time")), 74.0, 13.0, 56.0, 8.0, "#e8eaee", TextAlign::Left);
        ui::lit_text(b, disp, "FEEDBACK", 136.0, 13.0, 50.0, 7.5, "#c9ccd2", TextAlign::Left, 500.0);
        ui::lit_readout(b, disp, "Feedback", ui::bind(&a("feedback")), 186.0, 13.0, 44.0, 8.0, ORANGE, TextAlign::Right);
        ui::glass(b, disp);
        screws(b, bay1, &[(9.0, 9.0), (9.0, bh - 9.0)]);

        // Bay 2: the heads and the big white dial.
        let bw2 = 170.0;
        let bay2 = bay(b, unit, "Bay 2", (338.0, y, bw2, bh), &grey);
        let cx = bw2 / 2.0;
        parts::centred(b, bay2, "Active Heads", (cx, 13.0), Type::new(6.5, INK, 500.0).track(0.08).upper());
        parts::art(b, bay2, &[vec![(cx - 44.0, 22.0), (cx - 44.0, 13.0), (cx - 36.0, 13.0)], vec![(cx + 44.0, 22.0), (cx + 44.0, 13.0), (cx + 36.0, 13.0)]], 0.9, INK);
        for (n, x) in [(1, cx - 36.0), (2, cx), (3, cx + 36.0)] {
            // The lamp's socket, and the lamp flashing on its head's repeat.
            b.add(bay2, draw::with_effects(draw::circle("lamp socket", x, 27.0, 4.8, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#d8d9db"), (1.0, "#8e9094")])), vec![draw::inner_shadow("#00000060", 0.0, 1.0, 1.5)]));
            let lamp = b.add(bay2, draw::emissive(draw::circle("head lamp", x, 27.0, 3.4, draw::solid(LAMP)), draw::hex(LAMP), 1.0, 7.0));
            let flash = format!("pow(1 - fract(time * 1000 / ({n} * max(param({DLY}/time), 20))), 4) * pow(param({DLY}/feedback), {}) * param({DLY}/on)", n - 1);
            motion::add(b, lamp, Motion::new(Target::Opacity).input(flash).range(0.05, 1.0));
        }
        let arc: Vec<(f64, f64)> = (0..=30).map(|i| draw::polar(cx, 90.0, 48.0, -135.0 + 270.0 * f64::from(i) / 30.0)).collect();
        parts::art(b, bay2, &[arc], 1.0, INK);
        for i in 0..5 {
            let (x, y) = draw::polar(cx, 90.0, 54.0, -135.0 + 67.5 * f64::from(i));
            b.add(bay2, draw::circle("detent", x, y, 2.2, draw::solid(INK)));
        }
        let dial = KnobLook { preset: "white_knurl", opts: &["color=#eeede9", "index=none", "diamond=0", "ribs=90", "knurl_band=0.5", "rib_depth=0.02", "cap_r=0.97", "height=0.5", "dome=0.03", "edge=0.1", "ticks=none", "fallback=knurled"] };
        look::place_knob(b, bay2, &dial, &a("time"), "Delay Time", 80.0, (cx, 90.0), INK);
        parts::centred(b, bay2, "Delay", (cx, 85.0), Type::new(6.5, "#3a3b3e", 600.0).track(0.08).upper());
        parts::centred(b, bay2, "Time", (cx, 95.0), Type::new(6.5, "#3a3b3e", 600.0).track(0.08).upper());
        // Its pointer: an orange index at the rim, turning with the time.
        let turn = b.add(bay2, draw::frame("time index", cx - 40.0, 50.0, 80.0, 80.0));
        b.add(turn, draw::circle("index", 40.0, 8.0, 3.0, draw::solid(ORANGE)));
        motion::add(b, turn, Motion::new(Target::Rotate).input(format!("norm({DLY}/time)")).range(-135.0, 135.0).pivot(0.5, 0.5));

        // Bay 3: the knobs.
        let bw3 = 318.0;
        let bay3 = bay(b, unit, "Bay 3", (510.0, y, bw3, bh), &grey);
        let black = KnobLook { preset: "knurled", opts: &["color=#1c1c1e", "pointer=#3b82f6", "index=dot", "cap_r=0.9", "height=0.6", "dome=0.08", "edge=0.1", "ribs=48", "rib_depth=0.03", "knurl_band=0.7", "index_w=0.09", "index_in=0.66", "index_out=0.66", "coat=0.5", "rough=0.25", "ticks=none"] };
        let orange = KnobLook { preset: "knurled", opts: &["color=#3b82f6", "pointer=#dbe8ff", "index=dot", "cap_r=0.9", "height=0.6", "dome=0.05", "edge=0.08", "ribs=24", "rib_depth=0.06", "knurl_band=1", "index_w=0.08", "index_in=0.66", "index_out=0.66", "coat=0.4", "rough=0.3", "ticks=none"] };
        let knobs = [
            ("feedback", "Feedback", false, 0, 0),
            ("mod_depth", "Tape-Mod", false, 1, 0),
            ("level", "Level", false, 2, 0),
            ("mod_rate", "Wow Rate", false, 0, 1),
            ("high_pass", "Bass", true, 1, 1),
            ("high_cut", "Treble", true, 2, 1),
        ];
        for (id, legend, warm, col, row) in knobs {
            let (x, cy) = (58.0 + 101.0 * f64::from(col), 38.0 + 64.0 * f64::from(row));
            look::knob(b, bay3, if warm { &orange } else { &black }, &a(id), 32.0, (x, cy), 11, INK, false);
            parts::centred(b, bay3, legend, (x, cy + 28.0), Type::new(7.0, INK, 500.0).track(0.08).upper());
        }

        // Bay 4: the mark, the emblem, the power.
        let bay4 = bay(b, unit, "Bay 4", (830.0, y, w - 836.0, bh), &grey);
        let c4 = (w - 836.0) / 2.0;
        look::brush_mark(b, bay4, (c4, 42.0), 16.0, "#8a8c90");
        look::emblem(b, bay4, (c4, 74.0), 12.0, INK);
        glow_key(b, bay4, &a("on"), 26.0, (c4, 108.0), ORANGE);
        parts::centred(b, bay4, "Power", (c4, 136.0), t);
        screws(b, bay4, &[(w - 836.0 - 9.0, bh - 9.0)]);
    });
    kit::delay_face("60-dly-tape", scene)
}
