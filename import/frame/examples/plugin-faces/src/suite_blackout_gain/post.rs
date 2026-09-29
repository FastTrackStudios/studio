//! Blackout Gain post-FX: the delay and the reverb, in the pre-FX boxes'
//! black and white line-art.

use document_proto::{NodeId, TextAlign};
use surface_kit::draw;
use surface_widgets::Builder;

use super::PLUGIN;
use super::art::{self, BLACK, GREEN, INK, Put};
use crate::delay::parts::{self, Ink, Screen};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Enclosure, KnobStyle};

pub fn faces() -> Vec<Face> {
    vec![delay(), reverb()]
}

const KNOB: KnobStyle<'static> = KnobStyle::new("knob.pedal_lobed", "black", 44.0, INK);
const LCD_BG: (&str, &str) = ("#2d3a33", "#23302a");
const LCD_INK: &str = "#d6e4d8";

fn black(class: &'static str) -> Enclosure<'static> {
    Enclosure::new(class, BLACK, "satin").base("#0b0b0b").jacks("top")
}

/// A hexagram (two interlaced triangles) printed at (`cx`, `cy`).
fn hexagram(b: &mut Builder<'_>, p: NodeId, size: (f64, f64), (cx, cy): (f64, f64), r: f64) {
    for rot in [0.0, 180.0] {
        let pts: Vec<(f64, f64)> = (0..3).map(|i| draw::polar(cx, cy, r, rot + 120.0 * f64::from(i))).collect();
        let mut n = draw::path("hexagram", 0.0, 0.0, size.0, size.1, draw::polygon(&pts), draw::solid(INK));
        n.fills = Vec::new();
        n.stroke = document_proto::Stroke { paints: vec![draw::solid(INK)], weight: 1.6, join: document_proto::StrokeJoin::Miter, ..document_proto::Stroke::default() };
        b.add(p, n);
    }
}

// ── 07 · Crypt Delay ─────────────────────────────────────────────────────

/// Crypt Delay — Pedal, BigBox (404 × 286). MIX and FEEDBACK in the top
/// corners, SYNC and PING-PONG bat toggles between them over a grey-green
/// LCD (the sync source and ping-pong state, the note value and the tempo),
/// TIME and TONE either side of a hexagram, two skulls looking out, the
/// name in tall condensed capitals, ENGAGE and TAP TEMPO stomps with their
/// green LEDs in the bottom band, the maker's badge between them.
fn delay() -> Face {
    let ns = "bgdelay";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Crypt Delay", |b, root| {
        let e = black("bigbox");
        let p = rk::pedal_body(b, root, &e);
        let size = e.size();
        let (w, _) = size;
        art::border(b, p, size, 7.0, (15.0, 272.0), "9V DC");
        art::skull(b, p, size, &Put::new(8.0, 104.0, 84.0, true));
        art::skull(b, p, size, &Put::new(w - 92.0, 104.0, 84.0, false));
        art::locks(b, p, size, (96.0, 110.0), 70.0, 3, 3.6, 1.2);
        art::locks(b, p, size, (w - 96.0, 110.0), 70.0, 3, 3.6, 1.2);
        art::flourish(b, p, size, (30.0, 30.0), 30.0, false);
        art::flourish(b, p, size, (w - 30.0, 30.0), 30.0, true);
        hexagram(b, p, size, (w / 2.0, 128.0), 16.0);

        let legend = art::knob_letter(11.0);
        let none = Ink::new(6.0, INK);
        for (addr, name, at, ly) in [("mix", "MIX", (58.0, 54.0), 88.0), ("feedback", "FEEDBACK", (w - 58.0, 54.0), 88.0), ("time", "TIME", (124.0, 146.0), 114.0), ("tone", "TONE", (w - 124.0, 146.0), 114.0)] {
            let k = rk::knob(b, p, &KNOB, &a(addr), "", at, 0.0, none);
            art::label_letter(b, k, name, (at.0, ly), legend);
        }
        for (addr, name, x) in [("sync", "SYNC", 124.0), ("pingpong", "PING PONG", w - 124.0)] {
            let t = rk::mini_toggle(b, p, name, &a(addr), (x, 42.0), 16.0, "chrome");
            art::label_letter(b, t, name, (x, 25.0), art::knob_letter(7.5));
            for dx in [-7.0, 7.0] {
                b.add(p, draw::circle("position dot", x + dx, 55.0, 1.3, draw::solid(INK)));
            }
        }
        // The LCD.
        let win = parts::screen(b, p, "LCD", (148.0, 62.0, 108.0, 38.0), Screen { glass: LCD_BG, ink: LCD_INK, mesh: false });
        let small = Ink::new(5.2, "#9fb0a3").weight(600.0).track(0.04);
        parts::silk_left(b, win, "FREE / DAW / TAP", 4.0, 7.5, small);
        parts::silk_right(b, win, "P-P: OFF / ON", 104.0, 7.5, small);
        parts::note_picker(b, win, "Note", &a("div"), (2.0, 25.0, 44.0), 10.0, LCD_INK, "#50605600");
        let bpm = parts::readout(b, win, "Tempo", rk::bind(&a("tempo")), (44.0, 25.0, 62.0), 10.0, LCD_INK, TextAlign::Right);
        parts::format(b, bpm, surface_proto::ValueFormat::decimals(1));

        art::title(b, p, "DELAY", (w / 2.0, 184.0), 118.0);
        rk::silk(b, p, "HANDMADE IN THE DARK", w / 2.0, 202.0, Ink::new(5.0, INK).weight(700.0).track(0.12));
        // The bottom band: ENGAGE and TAP TEMPO, LEDs inboard of them.
        art::switch_ring(b, p, (44.0, 236.0), 21.0);
        art::switch_ring(b, p, (w - 44.0, 236.0), 21.0);
        let on = art::stomp(b, p, "Engage", &a("on"), (44.0, 236.0), 34.0, GREEN, (48.0, -4.0), 15.0, false);
        art::label_letter(b, on, "ENGAGE", (92.0, 256.0), art::knob_letter(8.0));
        let tap = art::stomp(b, p, "Tap Tempo", &a("tap"), (w - 44.0, 236.0), 34.0, GREEN, (-48.0, -4.0), 15.0, true);
        art::label_letter(b, tap, "TAP TEMPO", (w - 92.0, 256.0), art::knob_letter(8.0));
        art::badge(b, p, size, (w / 2.0, 250.0), 120.0);
    });
    let params = vec![
        rk::ms(&a("time"), "Time", 20.0, 2000.0, 500.0).featured(),
        rk::choice(&a("div"), "Note", rk::DIVISIONS, 4),
        rk::toggle(&a("sync"), "Sync", true),
        rk::toggle(&a("pingpong"), "Ping Pong", false),
        rk::percent(&a("mix"), "Mix", 30.0).featured(),
        rk::percent(&a("feedback"), "Feedback", 40.0).featured(),
        rk::percent(&a("tone"), "Tone", 60.0),
        crate::delay::parts::param(&a("tempo"), "Tempo", surface_proto::Range::linear(40.0, 240.0, surface_proto::Unit::Bpm), 120.0),
        rk::toggle(&a("on"), "Engage", true).featured(),
        rk::toggle(&a("tap"), "Tap Tempo", false),
    ];
    Face { file: "07-post-bigbox-crypt-delay", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 08 · Tomb Verb ───────────────────────────────────────────────────────

/// Tomb Verb — Pedal, Standard (156 × 286). MIX and DECAY high either side
/// of a hexagram, TONE between and below, skulls at the sides, the name in
/// tall condensed capitals, the green LED between two scrolls in the
/// bottom band over the stomp switch.
fn reverb() -> Face {
    let ns = "bgverb";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Tomb Verb", |b, root| {
        let e = black("standard");
        let p = rk::pedal_body(b, root, &e);
        let size = e.size();
        let (w, _) = size;
        art::border(b, p, size, 7.0, (15.0, 272.0), "9V DC");
        hexagram(b, p, size, (w / 2.0, 36.0), 12.0);
        art::skull(b, p, size, &Put::new(-4.0, 112.0, 58.0, true));
        art::skull(b, p, size, &Put::new(w - 54.0, 112.0, 58.0, false));
        art::locks(b, p, size, (16.0, 150.0), 36.0, 2, 3.4, 1.0);
        art::locks(b, p, size, (w - 16.0, 150.0), 36.0, 2, 3.4, 1.0);
        art::knot(b, p, size, (22.0, 190.0), 16.0);
        art::knot(b, p, size, (w - 22.0, 190.0), 16.0);
        let small = KNOB.size(38.0);
        let legend = art::knob_letter(10.0);
        let none = Ink::new(6.0, INK);
        for (addr, name, at, ly) in [("mix", "MIX", (35.0, 42.0), 70.0), ("decay", "DECAY", (w - 35.0, 42.0), 70.0), ("tone", "TONE", (w / 2.0, 88.0), 117.0)] {
            let k = rk::knob(b, p, &small, &a(addr), "", at, 0.0, none);
            art::label_letter(b, k, name, (at.0, ly), legend);
        }
        art::title(b, p, "REVERB", (w / 2.0, 172.0), 96.0);
        rk::silk(b, p, "HANDMADE IN THE DARK", w / 2.0, 187.0, Ink::new(4.2, INK).weight(700.0).track(0.1));
        art::flourish(b, p, size, (w / 2.0 - 26.0, 204.0), 22.0, true);
        art::flourish(b, p, size, (w / 2.0 + 26.0, 204.0), 22.0, false);
        art::switch_ring(b, p, (w / 2.0, 235.0), 21.0);
        art::stomp(b, p, "Bypass", &a("on"), (w / 2.0, 235.0), 34.0, GREEN, (0.0, -31.0), 17.0, false);
        art::badge(b, p, size, (w / 2.0, 265.0), 100.0);
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 25.0).featured(),
        rk::percent(&a("decay"), "Decay", 50.0).featured(),
        rk::percent(&a("tone"), "Tone", 50.0),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "08-post-standard-tomb-verb", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
