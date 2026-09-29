//! `12-post-voices-harmonizer` — the four-voice harmonizer: a mint floor
//! unit (a custom outline at the pedals' height: 480 × 286, wider than the
//! BigBox class to hold its screen) whose dark screen carries the tuning,
//! key, scale, voicing and quantize bar; four voice columns (on, interval,
//! level, pan); a level/pan view with a stem per voice and the dry signal;
//! and the unison, width, tone and output knobs. MIDI and engage stomps at
//! the corners, the name between them.

use document_proto::{CornerRadii, NodeId, TextAlign};
use surface_kit::draw;
use surface_proto::{ButtonMode, Control, ControlKind, Orientation, Range, SlotRole, Unit};
use surface_widgets::Builder;

use super::parts::{self, PEDAL_H, knob, pedal, switch, word};
use super::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

const NS: &str = "voices";
const W: f64 = 480.0;
const PURPLE: &str = "#a066e8";
const TEXT: &str = "#e6e6e8";
const DIM: &str = "#8a8a90";
const INTERVALS: [&str; 15] = ["-8TH", "-7TH", "-6TH", "-5TH", "-4TH", "-3RD", "-2ND", "UNISON", "+2ND", "+3RD", "+4TH", "+5TH", "+6TH", "+7TH", "+8TH"];
const KEYS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const SCALES: [&str; 5] = ["MAJOR", "NATURAL MINOR", "HARMONIC MINOR", "DORIAN", "MIXOLYDIAN"];
const VOICINGS: [&str; 5] = ["<SELECT CHORD>", "TRIAD", "SUS2", "SUS4", "SEVENTH"];

fn label(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, size: f64, color: &str) {
    let w = s.chars().count() as f64 * size * 0.75 + 4.0;
    b.add(parent, ui::text("label", s, x, cy, w, size, color, TextAlign::Left, 700.0, false));
}

/// A value box: a dark bar whose fill (or a marker, for a stepped
/// interval) shows the value, the value printed over it.
#[allow(clippy::too_many_arguments)]
fn value_bar(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (x, y, w, h): (f64, f64, f64, f64), marker: bool) {
    let root = b.add(parent, draw::frame(name, x, y, w, h));
    b.add(root, draw::rounded(draw::rect("well", 0.0, 0.0, w, h, draw::linear(180.0, &[(0.0, "#46464a"), (1.0, "#39393c")])), 2.0));
    let lane = b.add(root, draw::frame("lane", 0.0, 0.0, w, h));
    let slot = if marker {
        let m = b.add(lane, draw::rect("marker", 0.0, 0.0, 7.0, h, draw::solid(&ui::alpha(PURPLE, 0.8))));
        (SlotRole::Thumb, m)
    } else {
        let f = b.add(lane, draw::rect("fill", 0.0, 0.0, w, h, draw::linear(90.0, &[(0.0, &ui::alpha(PURPLE, 0.75)), (1.0, &ui::alpha(PURPLE, 0.55))])));
        (SlotRole::Fill, f)
    };
    b.add(root, draw::rect("underline", 0.0, h - 1.0, w, 1.0, draw::solid(&ui::alpha(PURPLE, 0.9))));
    let t = b.add(root, ui::text("value", "", 0.0, h / 2.0, w, h * 0.62, TEXT, TextAlign::Center, 700.0, false));
    let mut c = Control::new(ControlKind::Fader { orientation: Orientation::Horizontal }).slot(slot.0, slot.1.0).slot(SlotRole::Value, t.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// A dropdown: a dark box printing the chosen option, a chevron.
fn dropdown(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (x, y, w, h): (f64, f64, f64, f64)) {
    let root = b.add(parent, draw::frame(name, x, y, w, h));
    b.add(root, draw::rounded(draw::rect("box", 0.0, 0.0, w, h, draw::solid("#3a3a3d")), 2.5));
    let t = b.add(root, ui::text("value", "", 3.5, h / 2.0, w - 12.0, h * 0.55, TEXT, TextAlign::Left, 700.0, false));
    ui::line(b, root, "chevron", &[(w - 8.0, h / 2.0 - 1.2), (w - 5.5, h / 2.0 + 1.2), (w - 3.0, h / 2.0 - 1.2)], 0.8, TEXT);
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, t.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// A toggle pill (a voice's on switch, drawn in its header box).
fn header(b: &mut Builder<'_>, parent: NodeId, n: usize, addr: &str, (x, y, w, h): (f64, f64, f64, f64)) {
    let root = b.add(parent, draw::frame(&format!("Voice {n}"), x, y, w, h));
    b.add(root, draw::rounded(draw::rect("box", 0.0, 0.0, w, h, draw::linear(180.0, &[(0.0, "#6a6a6e"), (1.0, "#57575b")])), 1.5));
    b.add(root, ui::text("title", &format!("VOICE {n}"), 4.0, h / 2.0, w - 18.0, h * 0.55, TEXT, TextAlign::Center, 700.0, false));
    let (px, py, pw, ph) = (w - 15.0, h / 2.0 - 2.5, 10.0, 5.0);
    let on = b.add(root, draw::frame("on", 0.0, 0.0, w, h));
    b.add(on, draw::rounded(draw::rect("pill", px, py, pw, ph, draw::solid("#f4f4f4")), 2.5));
    b.add(on, draw::circle("dot", px + pw - 2.5, py + 2.5, 1.6, draw::solid("#3a3a3d")));
    let off = b.add(root, draw::frame("off", 0.0, 0.0, w, h));
    b.add(off, draw::stroked(draw::rounded(draw::rect("pill", px, py, pw, ph, draw::solid("#00000000")), 2.5), "#f4f4f4", 0.7));
    b.add(off, draw::circle("dot", px + 2.5, py + 2.5, 1.4, draw::solid("#f4f4f4")));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::On, on.0).slot(SlotRole::Off, off.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// A voice's stem in the level/pan view: the level as a stem, its number
/// on a ring at the top.
fn stem(b: &mut Builder<'_>, parent: NodeId, tag: &str, addr: &str, cx: f64, (top, h): (f64, f64), bright: bool) {
    let w = 16.0;
    let root = b.add(parent, draw::frame(&format!("Stem {tag}"), cx - w / 2.0, top, w, h));
    let color = if bright { "#e58cf0" } else { "#9b62d6" };
    let lane = b.add(root, draw::frame("lane", 0.0, 0.0, w, h));
    let fill = b.add(lane, draw::rect("stem", w / 2.0 - 1.2, 0.0, 2.4, h, draw::solid(color)));
    let ring = b.add(lane, draw::frame("ring", 0.0, 0.0, w, w));
    b.add(ring, draw::circle("halo", w / 2.0, w / 2.0, 7.5, draw::solid(&ui::alpha(color, 0.45))));
    b.add(ring, draw::circle("cap", w / 2.0, w / 2.0, 5.5, draw::solid(if bright { "#fafafa" } else { "#c9c9cc" })));
    b.add(ring, ui::text("n", tag, 0.0, w / 2.0, w, 5.5, "#3a3a3d", TextAlign::Center, 700.0, false));
    let mut c = Control::new(ControlKind::Fader { orientation: Orientation::Vertical }).slot(SlotRole::Fill, fill.0).slot(SlotRole::Thumb, ring.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// A bottom-row knob and its caption with the live value.
fn screen_knob(b: &mut Builder<'_>, parent: NodeId, id: &str, caption: &str, cx: f64) {
    let addr = format!("{NS}/{id}");
    knob(b, parent, "knob.arc_disc", "dark", &addr, caption, 20.0, cx, 170.0);
    let cw = caption.chars().count() as f64 * 5.4;
    label(b, parent, caption, cx - (cw + 26.0) / 2.0, 192.0, 7.0, TEXT);
    let r = b.add(parent, draw::frame(&format!("{caption} value"), cx - (cw + 26.0) / 2.0 + cw + 1.0, 187.0, 30.0, 10.0));
    let t = b.add(r, ui::text("value", "", 0.0, 5.0, 30.0, 7.0, TEXT, TextAlign::Left, 700.0, false));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, t.0);
    c.binding = Some(ui::bind(&addr));
    b.controls.insert(r, c);
}

pub fn face() -> Face {
    let scene = canvas("Voices Harmonizer (floor unit, 480×286)", |b, root| {
        let lid = pedal(b, root, W, PEDAL_H, ["#d0dad7", "#c3cecb", "#b3bfbc"], "#f2f8f6");
        // The screen.
        let (sx, sy, sw, sh) = (19.0, 20.0, 442.0, 200.0);
        let mut screen = draw::frame("Screen", sx, sy, sw, sh);
        screen.fills = vec![draw::linear(160.0, &[(0.0, "#252527"), (0.5, "#1d1d1f"), (1.0, "#232325")]), draw::radial_at(0.3, 0.6, 0.6, &[(0.0, "#ffffff0a"), (1.0, "#ffffff00")])];
        screen.corner_radii = CornerRadii::uniform(6.0);
        screen.effects = vec![draw::inner_shadow("#000000", 0.0, 1.5, 3.0), draw::drop_shadow("#ffffff66", 0.0, 1.0, 0.5)];
        let scr = b.add(lid, screen);

        // Tuning, key, scale, voicing, quantize.
        label(b, scr, "TUNING", 6.0, 10.0, 6.5, TEXT);
        let r = b.add(scr, draw::frame("Tuning", 36.0, 5.0, 36.0, 10.0));
        let t = b.add(r, ui::text("value", "", 0.0, 5.0, 36.0, 6.5, TEXT, TextAlign::Left, 700.0, false));
        let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, t.0);
        c.binding = Some(ui::bind("voices/tuning"));
        b.controls.insert(r, c);
        label(b, scr, "KEY", 80.0, 10.0, 6.5, TEXT);
        dropdown(b, scr, "Key", "voices/key", (96.0, 4.0, 26.0, 12.0));
        label(b, scr, "SCALE", 130.0, 10.0, 6.5, TEXT);
        dropdown(b, scr, "Scale", "voices/scale", (152.0, 4.0, 68.0, 12.0));
        label(b, scr, "VOICING", 224.0, 10.0, 6.5, TEXT);
        dropdown(b, scr, "Voicing", "voices/voicing", (254.0, 4.0, 110.0, 12.0));
        label(b, scr, "QUANTIZE", 372.0, 10.0, 6.0, TEXT);
        let q = b.add(scr, draw::frame("Quantize", 406.0, 4.0, 32.0, 12.0));
        b.add(q, draw::rounded(draw::rect("well", 0.0, 0.0, 32.0, 12.0, draw::solid("#3a3a3d")), 2.5));
        let off = b.add(q, draw::frame("off", 0.0, 0.0, 32.0, 12.0));
        b.add(off, draw::rounded(draw::rect("seg", 1.0, 1.0, 15.0, 10.0, draw::solid("#5e5e62")), 2.0));
        let on = b.add(q, draw::frame("on", 0.0, 0.0, 32.0, 12.0));
        b.add(on, draw::rounded(draw::rect("seg", 16.0, 1.0, 15.0, 10.0, draw::solid("#5e5e62")), 2.0));
        b.add(q, ui::text("OFF", "OFF", 1.0, 6.0, 15.0, 5.5, TEXT, TextAlign::Center, 700.0, false));
        b.add(q, ui::text("ON", "ON", 16.0, 6.0, 15.0, 5.5, DIM, TextAlign::Center, 700.0, false));
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Off, off.0).slot(SlotRole::On, on.0);
        c.binding = Some(ui::bind("voices/quantize"));
        b.controls.insert(q, c);

        // The four voices.
        for (n, x) in [(1, 5.0), (2, 62.0), (3, 324.0), (4, 382.0)] {
            let v = format!("{NS}/v{n}");
            header(b, scr, n, &format!("{v}_on"), (x, 34.0, 55.0, 12.0));
            let on = n == 1 || n == 4;
            let ink = if on { TEXT } else { DIM };
            label(b, scr, "INTERVAL", x, 64.0, 6.5, ink);
            value_bar(b, scr, &format!("Voice {n} interval"), &format!("{v}_interval"), (x, 70.0, 55.0, 12.0), true);
            label(b, scr, "LEVEL", x, 94.0, 6.5, ink);
            value_bar(b, scr, &format!("Voice {n} level"), &format!("{v}_level"), (x, 100.0, 55.0, 12.0), false);
            label(b, scr, "PAN", x, 124.0, 6.5, ink);
            value_bar(b, scr, &format!("Voice {n} pan"), &format!("{v}_pan"), (x, 130.0, 55.0, 12.0), false);
        }
        ui::rule(b, scr, 60.5, 52.0, 0.6, 92.0, "#ffffff26");
        ui::rule(b, scr, 380.5, 52.0, 0.6, 92.0, "#ffffff26");

        // The level/pan view.
        let mut view = draw::frame("Level pan view", 124.0, 34.0, 194.0, 114.0);
        view.fills = vec![draw::linear(180.0, &[(0.0, "#39393c"), (1.0, "#303033")])];
        view.corner_radii = CornerRadii::uniform(4.0);
        let view = b.add(scr, view);
        b.add(view, ui::text("tab", "LEVEL/PAN", 38.0, 8.0, 56.0, 6.5, TEXT, TextAlign::Center, 700.0, false));
        b.add(view, draw::rect("tab rule", 40.0, 13.5, 52.0, 1.0, draw::solid(TEXT)));
        b.add(view, ui::text("tab", "DETUNE/DELAY", 96.0, 8.0, 64.0, 6.5, DIM, TextAlign::Center, 700.0, false));
        b.add(view, draw::rect("rule", 0.0, 15.0, 194.0, 0.6, draw::solid("#ffffff1f")));
        for (tag, addr, x, bright) in [("1", "voices/v1_level", 0.24, true), ("2", "voices/v2_level", 0.37, false), ("DI", "voices/dry", 0.5, true), ("3", "voices/v3_level", 0.63, false), ("4", "voices/v4_level", 0.76, true)] {
            stem(b, view, tag, addr, 194.0 * x, (20.0, 76.0), bright);
        }

        // The bottom row.
        screen_knob(b, scr, "unisons", "UNISONS", 48.0);
        ui::line(b, scr, "unison rail", &[(96.0, 191.0), (134.0, 191.0)], 0.6, DIM);
        screen_knob(b, scr, "width", "WIDTH", 164.0);
        screen_knob(b, scr, "tone", "TONE", 282.0);
        screen_knob(b, scr, "output", "OUTPUT", 398.0);

        // The footer: stomps, lamps, the name.
        switch(b, lid, "switch.hex_stomp", "chrome", "voices/midi", "MIDI", 24.0, 40.0, 246.0, &["led=right"], &["cap"]);
        word(b, lid, "midi", 98.0, 246.0, 10.0, "#6c7270", 500.0, 0.06);
        switch(b, lid, "switch.hex_stomp", "chrome", "voices/on", "Engage", 24.0, W - 40.0, 246.0, &["led=left"], &["cap"]);
        word(b, lid, "engage", W - 108.0, 246.0, 10.0, "#6c7270", 500.0, 0.06);
        word(b, lid, "harmonizer", W / 2.0, 246.0, 21.0, "#55595a", 700.0, 0.07);
    });
    let mut params = vec![
        ui::param("voices/tuning", "Tuning", Range::linear(430.0, 450.0, Unit::Hz), 440.0),
        parts::pick(NS, "key", "Key", &KEYS, 9),
        parts::pick(NS, "scale", "Scale", &SCALES, 1),
        parts::pick(NS, "voicing", "Voicing", &VOICINGS, 0),
        parts::on(NS, "quantize", "Quantize", false),
    ];
    for (n, on, iv, lvl, pan) in [(1, true, 5, -6.0, -50.0), (2, false, 9, -12.0, -25.0), (3, false, 11, -12.0, 25.0), (4, true, 14, -24.0, 50.0)] {
        params.push(parts::on(NS, &format!("v{n}_on"), &format!("Voice {n}"), on));
        params.push(parts::pick(NS, &format!("v{n}_interval"), &format!("Voice {n} Interval"), &INTERVALS, iv));
        params.push(parts::db(NS, &format!("v{n}_level"), &format!("Voice {n} Level"), -36.0, 0.0, lvl));
        params.push(ui::param(&format!("voices/v{n}_pan"), &format!("Voice {n} Pan"), Range::linear(-100.0, 100.0, Unit::None), pan).bipolar());
    }
    params.push(parts::db(NS, "dry", "Dry Level", -36.0, 0.0, 0.0));
    params.push(parts::pct(NS, "unisons", "Unisons", 50.0));
    params.push(parts::pct(NS, "width", "Width", 50.0));
    params.push(parts::pct(NS, "tone", "Tone", 50.0));
    params.push(parts::db(NS, "output", "Output", -24.0, 12.0, 0.0).bipolar());
    params.push(parts::on(NS, "midi", "MIDI", false));
    params.push(parts::on(NS, "on", "Engage", true).featured());
    params[0] = params[0].clone().featured();
    Face { file: "12-post-voices-harmonizer", plugin: PLUGIN, scene, params, streams: vec![] }
}
