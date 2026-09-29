//! The tape echo: a machine with reels on it, in cream enamel and oxide,
//! walnut cheeks.
//!
//! The deck is the face: the supply reel full, the take-up filling, the
//! tape pulled past a record head and three playback heads, then capstan
//! and pinch roller. The playback heads are the "heads" control — each has
//! a lamp that is its switch, so which repeats you hear is where the lit
//! heads are (1 at the repeat rate, 2 and 3 at twice and three times it).
//! Right of the deck, the record-level VU and the time window — the repeat
//! rate in ms and the note it's locked to. Below, the knobs a tape echo
//! has: repeat rate and intensity (feedback) first as on every delay face,
//! then record level (drive into the tape), wow, flutter, tone, the tape's
//! age, and the echo level (mix) last.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Ink as KitInk, Place, draw};
use surface_widgets::Builder;

use super::art::{self, Reel};
use super::parts::{self, Ink, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 350.0;
const INK: &str = "#2f2417";
const DIM: &str = "#7b6b53";
const OXIDE: &str = "#5a3620";
const LAMP: &str = "#ffb040";
const T_MIN: f64 = 20.0;
const T_MAX: f64 = 2000.0;

fn a(s: &str) -> String {
    format!("tape_echo/{s}")
}

/// The deck: reels, heads, tape path.
fn deck(b: &mut Builder<'_>, panel: NodeId) {
    let (x0, y0, w, h) = (56.0, 22.0, 560.0, 190.0);
    let deck = parts::well(b, panel, "Deck", (x0, y0, w, h), draw::linear(180.0, &[(0.0, "#2c2a26"), (1.0, "#1a1916")]), 8.0);
    let steel = Reel { flange: ("#eef0f2", "#9ea2a8", "#6b6f75"), oxide: OXIDE };
    let (sy, r) = (92.0, 74.0);
    let (sx, tx) = (100.0, 460.0);
    // Tape path: off the supply pack, round a guide, across the heads,
    // through capstan and roller, round a guide, onto the take-up pack.
    let ty = 138.0;
    let (sp, tp) = (r * 0.92, r * 0.56);
    let from = draw::polar(sx, sy, sp, 104.0);
    let to = draw::polar(tx, sy, tp, 238.0);
    let g1 = (188.0, ty + 7.0);
    let g2 = (406.0, ty + 7.0);
    art::tape(b, deck, &[from, (g1.0 - 3.0, ty + 1.0), (g1.0 + 6.0, ty), (g2.0 - 6.0, ty), (g2.0 + 3.0, ty + 1.0), to], 3.6, "#6e4428");
    art::reel(b, deck, (sx, sy), r, 0.92, 8.0, steel);
    art::reel(b, deck, (tx, sy), r, 0.56, 47.0, steel);
    art::post(b, deck, g1, 7.0);
    art::post(b, deck, g2, 7.0);
    // Head block: a machined plate the heads stand on.
    parts::plate(b, deck, "Head block", (206.0, ty + 1.0, 188.0, 46.0), draw::linear(180.0, &[(0.0, "#c9ccd0"), (1.0, "#7d8187")]), 4.0);
    let heads = [("Rec", 228.0), ("1", 272.0), ("2", 310.0), ("3", 348.0)];
    let engraved = Ink::new(6.5, "#1d1e20").weight(800.0).track(0.12);
    for (_, x) in heads {
        art::head(b, deck, (x, ty + 12.0), 22.0, 22.0);
    }
    parts::silk(b, deck, "Rec", 228.0, ty + 34.0, engraved);
    // Capstan and pinch roller.
    art::roller(b, deck, (380.0, ty - 11.0), 10.0);
    art::post(b, deck, (380.0, ty + 3.0), 4.0);
    // The playback heads' switches: a lamp each, lit when that head plays.
    for (i, (_, x)) in heads.iter().skip(1).enumerate() {
        let lamp = parts::lamp_toggle(b, deck, &format!("Head {}", i + 1), &a(&format!("head_{}", i + 1)), (*x, ty - 22.0), 9.0, 30.0, LAMP);
        // The number engraved under the head is the switch's legend.
        parts::label_at(b, lamp, &format!("{}", i + 1), (*x, ty + 34.0), engraved);
    }
    parts::silk(b, deck, "Playback heads", 310.0, ty - 42.0, Ink::new(6.5, "#b9b2a4").weight(700.0).track(0.2));
    parts::silk(b, deck, "Record", 228.0, ty - 22.0, Ink::new(6.0, "#8d877c").weight(700.0).track(0.14));
    parts::silk_left(b, deck, "Supply", 14.0, h - 12.0, Ink::new(6.0, "#8d877c").weight(700.0).track(0.2));
    parts::silk_right(b, deck, "Take-up", w - 14.0, h - 12.0, Ink::new(6.0, "#8d877c").weight(700.0).track(0.2));
    let _ = x0 + y0;
}

pub fn face() -> Face {
    let scene = canvas("Tape Echo", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "cream", W, H, "wood");
        parts::repaint(b, panel, &[(0.0, "#e9dfc9"), (0.5, "#dacfb5"), (1.0, "#c5b89b")]);

        deck(b, panel);

        // ── Nameplate, VU, time window ─────────────────────────────────
        let cx = 780.0;
        parts::silk(b, panel, "Tape Echo", cx, 30.0, Ink::new(17.0, INK).weight(800.0).track(0.06).plain());
        parts::silk(b, panel, "EC-3 · Three-head repeat", cx, 47.0, Ink::new(6.5, DIM).weight(700.0).track(0.2));
        let vu = Place::new("Record level").size(158.0).variant("black_rack").legend("VU").bind(parts::stream(&a("in")));
        let vu = parts::put(b, panel, "meter.vu_amber", &vu, cx, 112.0);
        parts::glass(b, vu);
        parts::silk(b, panel, "Record level", cx, 178.0, Ink::new(6.5, DIM).weight(700.0).track(0.22));

        let win = parts::well(b, panel, "Time window", (cx - 110.0, 190.0, 220.0, 30.0), draw::linear(180.0, &[(0.0, "#150d06"), (1.0, "#22160b")]), 4.0);
        parts::readout(b, win, "Repeat time", bind(&a("time")), (6.0, 15.0, 100.0), 16.0, LAMP, TextAlign::Right);
        parts::note_picker(b, win, "Note", &a("div"), (122.0, 15.0, 70.0), 12.0, LAMP, "#7a5320");

        // ── Knob row ───────────────────────────────────────────────────
        parts::rule(b, panel, 56.0, W - 56.0, 228.0, 1.0, "#2f241740");
        let row = 276.0;
        let legend_y = 326.0;
        let value_y = 340.0;
        let tm = parts::time_marks(T_MIN, T_MAX);
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)]); 8] = [
            ("time", "Repeat Rate", 104.0, 60.0, &tm),
            ("feedback", "Intensity", 214.0, 60.0, parts::FB_MARKS),
            ("drive", "Rec Level", 320.0, 46.0, parts::PCT_MARKS),
            ("wow", "Wow", 402.0, 44.0, parts::PCT_MARKS),
            ("flutter", "Flutter", 478.0, 44.0, parts::PCT_MARKS),
            ("tone", "Tone", 554.0, 44.0, parts::PCT_MARKS),
            ("age", "Tape Age", 630.0, 44.0, parts::PCT_MARKS),
            ("mix", "Echo", 850.0, 54.0, parts::PCT_MARKS),
        ];
        let scale = Ink::new(6.5, DIM).weight(700.0).track(0.0).plain();
        for (addr, legend, x, d, marks) in knobs {
            let k = parts::knob(b, panel, "knob.bakelite", "black", &a(addr), legend, (x, row), d);
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, panel, (x, row), d / 2.0 + 5.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, legend, legend_y, Ink::new(8.0, INK).weight(800.0).track(0.14));
            if addr != "time" {
                parts::value_at(b, k, value_y, Ink::new(7.0, DIM).weight(600.0).track(0.02).plain());
            }
        }
        // Free-running or locked to the tempo: a bat toggle by itself.
        let sync = Place::new("Sync").size(40.0).variant("chrome").legend("Time").options(&["Sync", "Free"]).bind(bind(&a("sync"))).ink(KitInk::Color(INK.into()));
        let sw = parts::put(b, panel, "switch.toggle_bat", &sync, 734.0, row - 2.0);
        parts::caption_style(b, sw, Ink::new(7.5, INK).weight(800.0).track(0.14));
    });

    let params = vec![
        parts::time_ms(&a("time"), "Repeat Rate", T_MIN, T_MAX, 375.0).featured(),
        parts::toggle(&a("sync"), "Sync", false),
        parts::choice(&a("div"), "Note", parts::DIVISIONS, parts::DEFAULT_DIV),
        parts::feedback(&a("feedback"), "Intensity", 35.0).featured(),
        parts::percent(&a("drive"), "Rec Level", 35.0).featured(),
        parts::percent(&a("wow"), "Wow", 20.0),
        parts::percent(&a("flutter"), "Flutter", 15.0),
        parts::percent(&a("tone"), "Tone", 50.0),
        parts::percent(&a("age"), "Tape Age", 50.0),
        parts::percent(&a("mix"), "Echo", 35.0).featured(),
        parts::toggle(&a("head_1"), "Head 1", true),
        parts::toggle(&a("head_2"), "Head 2", false),
        parts::toggle(&a("head_3"), "Head 3", false),
    ];
    let streams = vec![parts::level_stream(&a("in"), "Record Level", 1)];
    Face { file: "02-tape", plugin: "delay", scene, params, streams }
}
