//! Djent Forge cab and EQ.
//!
//! - **2×12 Stack Cab** — Cab, 900 × 900: the dark cast 2×2 cabinet the
//!   plugin's cab page shows, two mics on the cones, and the page's two mic
//!   columns (mic choice, on, position, distance, level, pan, phase) and the
//!   room send, in the plugin's own light chrome.
//! - **Ivory Nine-Band** — Tabletop, 1000 × 290: a white machined graphic
//!   EQ on turned feet — HPF and LPF cup knobs reading OFF, nine pearl
//!   sliders 65 Hz – 16 kHz reading dB, an orange power toggle and pilot.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole, ValueFormat};
use surface_widgets::Builder;

use super::PLUGIN;
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk};

pub fn faces() -> Vec<Face> {
    vec![cab(), eq()]
}

/// The mics the cab page offers (generic names).
const MICS: &[&str] = &["Dynamic A", "Dyn A Off-Axis", "Dynamic B", "Condenser A", "Condenser B", "Ribbon A", "Ribbon B"];

/// The chrome's ink on its light stage.
const CHROME_INK: &str = "#23262c";

/// A drop-down that picks one of a choice param's options: a rounded box
/// printing the choice, a chevron; a `Selector` (wheel / arrows step it).
fn dropdown(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (x, y, w, h): (f64, f64, f64, f64), options: &[&str], dark: bool) -> NodeId {
    let (fill, ink) = if dark { ("#111214", "#f2f2f4") } else { ("#e3e6f0", CHROME_INK) };
    let mut f = draw::frame(name, x, y, w, h);
    f.fills = vec![draw::solid(fill)];
    f.clips_content = true;
    let root = b.add(parent, draw::stroked(draw::rounded(f, 6.0), if dark { "#000000" } else { "#b8bdd0" }, 1.0));
    let t = parts::text_node("", 12.0, h / 2.0, w - 34.0, &Ink::new(11.5, ink).weight(500.0).track(0.0).plain(), TextAlign::Left);
    let mut t = t;
    t.name = "value".into();
    let value = b.add(root, t);
    let (cx, cy) = (w - 16.0, h / 2.0);
    rk::line(b, root, &[(cx - 5.0, cy - 2.5), (cx, cy + 2.5), (cx + 5.0, cy - 2.5)], 1.5, ink);
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, value.0);
    c.binding = Some(rk::bind(addr));
    c.format = Some(ValueFormat::labels(options));
    b.controls.insert(root, c);
    root
}

/// A square outlined button that latches (mic on/off, phase): its fill
/// shows while latched.
fn latch(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (x, y, w, h): (f64, f64, f64, f64), text: &str) -> NodeId {
    let root = b.add(parent, draw::stroked(draw::rounded(draw::frame(name, x, y, w, h), 6.0), "#7d86a8", 1.2));
    let on = b.add(root, draw::rounded(draw::rect("latched", 1.0, 1.0, w - 2.0, h - 2.0, draw::solid("#b9c0dc")), 5.0));
    if !text.is_empty() {
        rk::print(b, root, text, (w / 2.0, h / 2.0), Ink::new(12.0, CHROME_INK).weight(600.0).track(0.06));
    }
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, on.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// The chrome's pill switch: a dark track, the thumb at the right when on.
fn pill(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64)) -> NodeId {
    let (w, h) = (TOUCH_MIN, TOUCH_MIN);
    let root = b.add(parent, draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h));
    b.add(root, draw::stroked(draw::rounded(draw::rect("track", 6.0, h / 2.0 - 9.0, 32.0, 18.0, draw::solid("#dfe3f0")), 9.0), CHROME_INK, 2.0));
    b.add(root, draw::stroked(draw::circle("thumb off", 15.0, h / 2.0, 5.0, draw::solid("#00000000")), CHROME_INK, 1.6));
    let on = b.add(root, draw::rounded(draw::rect("on", 6.0, h / 2.0 - 9.0, 32.0, 18.0, draw::solid(CHROME_INK)), 9.0));
    let _ = on;
    let thumb = b.add(root, draw::circle("thumb", 29.0, h / 2.0, 6.0, draw::solid("#ffffff")));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, on.0).slot(SlotRole::Led, thumb.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

fn ring(b: &mut Builder<'_>, parent: NodeId, addr: &str, legend: &str, (cx, cy): (f64, f64), d: f64) -> NodeId {
    let p = Place::new(legend).size(d).variant("light").legend(legend).bind(rk::bind(addr)).touch(TOUCH_MIN);
    parts::put(b, parent, "knob.plugin_ring", &p, cx, cy)
}

/// A 12-inch speaker seen through the baffle: the cut-out's rounded lip,
/// the surround, the cone, the dust cap.
fn speaker(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64) {
    b.add(parent, draw::with_effects(draw::circle("cut-out", cx, cy, r * 1.12, draw::radial(&[(0.0, "#0e0f10"), (0.85, "#1a1b1d"), (1.0, "#2e3033")])), vec![draw::inner_shadow("#000000", 0.0, 4.0, 10.0)]));
    b.add(parent, draw::circle("basket", cx, cy, r, draw::conic(30.0, &[(0.0, "#2a2b2e"), (0.25, "#4a4c50"), (0.5, "#1c1d1f"), (0.75, "#3e4044"), (1.0, "#2a2b2e")])));
    b.add(parent, draw::circle("surround", cx, cy, r * 0.93, draw::radial(&[(0.0, "#101112"), (0.86, "#141517"), (0.93, "#3a3c40"), (1.0, "#0c0c0d")])));
    b.add(parent, draw::circle("cone", cx, cy, r * 0.82, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#26272a"), (0.4, "#3a3c40"), (0.75, "#4c4e53"), (1.0, "#1e1f22")])));
    b.add(parent, draw::circle("cone light", cx - r * 0.2, cy - r * 0.25, r * 0.6, draw::radial(&[(0.0, "#ffffff18"), (1.0, "#ffffff00")])));
    b.add(parent, draw::with_effects(draw::circle("dust cap", cx, cy, r * 0.32, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#5e6166"), (0.6, "#2c2d31"), (1.0, "#141517")])), vec![draw::drop_shadow("#00000099", 0.0, 3.0, 6.0)]));
}

/// A mic seen from the front, aimed at a cone: its body, grille and clip.
fn mic(b: &mut Builder<'_>, parent: NodeId, (x, y): (f64, f64), len: f64, deg: f64) {
    let body = draw::rotated(draw::rounded(draw::rect("mic body", x - len / 2.0, y - 7.0, len, 14.0, draw::linear(180.0, &[(0.0, "#55585e"), (0.5, "#1a1b1d"), (1.0, "#3a3c40")])), 6.0), deg);
    b.add(parent, draw::with_effects(body, vec![draw::drop_shadow("#000000aa", 2.0, 6.0, 8.0)]));
    let (gx, gy) = (x + (len / 2.0 - 10.0) * deg.to_radians().cos(), y + (len / 2.0 - 10.0) * deg.to_radians().sin());
    b.add(parent, draw::circle("mic grille", gx, gy, 9.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#8a8d93"), (1.0, "#2a2b2e")])));
}

/// One mic's column of controls, its left edge at `x`.
#[allow(clippy::too_many_lines)]
fn mic_column(b: &mut Builder<'_>, root: NodeId, ns: &str, which: &str, x: f64, dark_menu: bool) {
    let a = |s: &str| format!("{ns}/{which}_{s}");
    let label = if which == "a" { "Mic A" } else { "Mic B" };
    rk::print(b, root, label, (x + 80.0, 150.0), Ink::new(11.0, CHROME_INK).weight(700.0).track(0.12));
    dropdown(b, root, &format!("{label} choice"), &a("mic"), (x, 170.0, 116.0, 36.0), MICS, dark_menu);
    let on = latch(b, root, &format!("{label} on"), &a("on"), (x + 122.0, 170.0, 40.0, 36.0), "");
    // The power glyph: a broken ring and a bar.
    let (gx, gy, gr) = (20.0, 18.0, 8.0);
    let arc: Vec<(f64, f64)> = (0..=24).map(|i| draw::polar(gx, gy, gr, 35.0 + 290.0 * f64::from(i) / 24.0)).collect();
    rk::line(b, on, &arc, 1.6, CHROME_INK);
    rk::line(b, on, &[(gx, gy - gr - 1.5), (gx, gy - 1.0)], 1.6, CHROME_INK);
    ring(b, root, &a("position"), "Position", (x + 38.0, 262.0), 58.0);
    ring(b, root, &a("distance"), "Distance", (x + 124.0, 262.0), 58.0);
    ring(b, root, &a("level"), "Mic Level", (x + 38.0, 380.0), 58.0);
    ring(b, root, &a("pan"), "Pan", (x + 124.0, 380.0), 58.0);
    rk::line(b, root, &[(x, 470.0), (x + 162.0, 470.0)], 1.0, "#aab1c8");
    latch(b, root, &format!("{label} phase"), &a("phase"), (x + 36.0, 800.0, 90.0, 32.0), "PHASE Ø");
}

fn mic_params(ns: &str, which: &str, default_mic: usize) -> Vec<surface_proto::ParamInfo> {
    let a = |s: &str| format!("{ns}/{which}_{s}");
    let label = if which == "a" { "Mic A" } else { "Mic B" };
    vec![
        rk::choice(&a("mic"), &format!("{label} Model"), MICS, default_mic).featured(),
        rk::toggle(&a("on"), &format!("{label} On"), true),
        rk::percent(&a("position"), &format!("{label} Position"), 50.0),
        rk::percent(&a("distance"), &format!("{label} Distance"), 0.0),
        rk::db(&a("level"), &format!("{label} Level"), -24.0, 12.0, 0.0),
        {
            let r = surface_proto::Range { steps: Some(201), ..surface_proto::Range::linear(-100.0, 100.0, surface_proto::Unit::Percent) };
            parts::param(&a("pan"), &format!("{label} Pan"), r, 0.0).bipolar()
        },
        rk::toggle(&a("phase"), &format!("{label} Phase"), false),
    ]
}

// ── 09 · 2×12 Stack Cab ──────────────────────────────────────────────────

/// 2×12 Stack Cab — Cab, 900 × 900. See the module docs.
fn cab() -> Face {
    let ns = "dfcab";
    let a = |s: &str| format!("{ns}/{s}");
    let (w, h) = (900.0, 900.0);
    let scene = canvas("Stack Cab", |b, root| {
        parts::fit_root(b, root, w, h);
        let stage = draw::rounded(draw::rect("stage", 0.0, 0.0, w, h, draw::linear(180.0, &[(0.0, "#d3d9ea"), (1.0, "#c3cadf")])), 18.0);
        b.add(root, stage);
        // The cabinet: dark cast, a raised baffle, four speakers.
        let cab = draw::rounded(draw::rect("cabinet", 205.0, 200.0, 490.0, 580.0, draw::linear(180.0, &[(0.0, "#3d3e41"), (1.0, "#2b2c2f")])), 10.0);
        b.add(root, draw::stroked(draw::with_effects(cab, vec![draw::drop_shadow("#00000066", 0.0, 10.0, 20.0), draw::inner_shadow("#ffffff1f", 0.0, 2.0, 1.0)]), "#141516", 1.5));
        let baffle = parts::well(b, root, "baffle", (222.0, 217.0, 456.0, 546.0), draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#2f3033")]), 4.0);
        for (x, y) in [(12.0, 12.0), (444.0, 12.0), (12.0, 534.0), (444.0, 534.0), (12.0, 273.0), (444.0, 273.0)] {
            b.add(baffle, draw::circle("screw", x, y, 3.2, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#8a8c90"), (1.0, "#1a1b1d")])));
        }
        for (cx, cy) in [(116.0, 146.0), (340.0, 146.0), (116.0, 400.0), (340.0, 400.0)] {
            speaker(b, baffle, (cx, cy), 98.0);
        }
        mic(b, baffle, (84.0, 138.0), 96.0, 12.0);
        mic(b, baffle, (372.0, 138.0), 96.0, 168.0);
        // The page's controls: the two mic columns and the room send.
        mic_column(b, root, ns, "a", 22.0, true);
        mic_column(b, root, ns, "b", 716.0, false);
        rk::print(b, root, "Room Send", (w / 2.0 - 18.0, 70.0), Ink::new(13.0, CHROME_INK).weight(700.0).track(0.06));
        pill(b, root, "Room send", &a("room"), (w / 2.0 + 62.0, 70.0));
        ring(b, root, &a("room_level"), "Level", (w / 2.0, 118.0), 40.0);
        // A room column echoes the level under mic B, as the page does.
        rk::print(b, root, "Cab", (w / 2.0, 885.0), Ink::new(9.0, "#6b7392").weight(600.0).track(0.3));
    });
    let mut params = mic_params(ns, "a", 1);
    params.extend(mic_params(ns, "b", 6));
    params.push(rk::toggle(&a("room"), "Room Send", false));
    params.push(rk::db(&a("room_level"), "Room Level", -24.0, 12.0, 0.0));
    Face { file: "09-cab-2x12", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 10 · Ivory Nine-Band ─────────────────────────────────────────────────

/// The bands, as the unit prints them.
const BANDS: [(&str, &str, f64); 9] = [
    ("b65", "65 Hz", 65.0),
    ("b125", "125 Hz", 125.0),
    ("b250", "250 Hz", 250.0),
    ("b500", "500 Hz", 500.0),
    ("b1k", "1 kHz", 1000.0),
    ("b2k", "2 kHz", 2000.0),
    ("b4k", "4 kHz", 4000.0),
    ("b8k", "8 kHz", 8000.0),
    ("b16k", "16 kHz", 16000.0),
];

/// Ivory Nine-Band — Tabletop, 1000 × 290. See the module docs.
#[allow(clippy::too_many_lines)]
fn eq() -> Face {
    let ns = "dfeq";
    let a = |s: &str| format!("{ns}/{s}");
    let (w, h) = (1000.0, 290.0);
    let scene = canvas("Ivory Nine-Band", |b, root| {
        parts::fit_root(b, root, w, h);
        for x in [74.0, 840.0] {
            b.add(root, draw::rounded(draw::rect("foot", x, 246.0, 86.0, 40.0, draw::linear(90.0, &[(0.0, "#1a1b1d"), (0.35, "#8a8b8e"), (0.55, "#3e3f42"), (1.0, "#141516")])), 3.0));
        }
        let body = draw::rounded(draw::rect("case", 0.0, 0.0, w, 254.0, draw::linear(180.0, &[(0.0, "#f4f5f6"), (0.5, "#e6e7e9"), (1.0, "#cfd1d5")])), 36.0);
        b.add(root, draw::stroked(draw::with_effects(body, vec![draw::drop_shadow("#00000040", 0.0, 10.0, 18.0), draw::inner_shadow("#ffffffc0", 0.0, 2.0, 1.0)]), "#b3b6bc", 1.0));
        let face = draw::rounded(draw::rect("face", 29.0, 31.0, 942.0, 193.0, draw::linear(180.0, &[(0.0, "#eeeff1"), (1.0, "#e3e4e7")])), 8.0);
        b.add(root, draw::with_effects(face, vec![draw::inner_shadow("#00000026", 0.0, 1.5, 3.0), draw::drop_shadow("#ffffffa0", 0.0, 1.0, 0.0)]));
        for (x, y) in [(48.0, 47.0), (955.0, 47.0), (48.0, 207.0), (955.0, 207.0)] {
            b.add(root, draw::circle("screw", x, y, 5.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff"), (1.0, "#9a9da3")])));
            rk::line(b, root, &[(x - 3.0, y), (x + 3.0, y)], 1.0, "#6d7076");
            rk::line(b, root, &[(x, y - 3.0), (x, y + 3.0)], 1.0, "#6d7076");
        }
        for x in [72.0, 928.0] {
            for y in [0.0, 247.0] {
                rk::line(b, root, &[(x, y + 3.0), (x, y + 10.0)], 1.0, "#8d9096");
                rk::line(b, root, &[(x + 4.0, y + 3.0), (x + 4.0, y + 10.0)], 1.0, "#8d9096");
            }
        }
        let ink = Ink::new(11.5, "#6b6e75").weight(500.0).track(0.0).plain();
        // HPF and LPF: cup knobs with a value readout over them.
        let filt = [("hpf", "HPF", 160.0, "M -26 10 L -14 -2 L 26 -2"), ("lpf", "LPF", 840.0, "M -26 -2 L 14 -2 L 26 10")];
        for (addr, name, x, glyph) in filt {
            let p = Place::new(name).size(80.0).variant("white").legend("").bind(rk::bind(&a(addr))).touch(TOUCH_MIN);
            let k = parts::put(b, root, "knob.cup", &p, x, 128.0);
            parts::strip_legend(b, k);
            let v = parts::value_at(b, k, 45.0, ink);
            let _ = v;
            parts::format(b, k, ValueFormat::decimals(0));
            // The response glyph: a dotted slope into a flat line.
            let pts: Vec<(f64, f64)> = glyph
                .split(" L ")
                .map(|s| s.trim_start_matches("M ").split(' ').map(|n| n.parse::<f64>().unwrap_or(0.0)).collect::<Vec<_>>())
                .map(|v| (x + v[0], 70.0 + v[1]))
                .collect();
            rk::line(b, root, &pts, 1.2, "#3b3e44");
        }
        // Nine sliders, a dotted rule between each at noon.
        for (i, (addr, label, _)) in BANDS.iter().enumerate() {
            let x = 250.0 + 62.0 * i as f64;
            let p = Place { options: Some(vec!["w=15".into()]), ..Place::new(*label).size(134.0).variant("pearl").bind(rk::bind(&a(addr))).touch(TOUCH_MIN) };
            let s = parts::put_at(b, root, "lever.eq_slider", &p, x - TOUCH_MIN / 2.0, 59.0);
            parts::value_at(b, s, 45.0, ink);
            parts::format(b, s, ValueFormat::decimals(1).unit("dB"));
            rk::label(b, s, label, (x, 210.0), ink);
            if i < 8 {
                let dots: Vec<Vec<(f64, f64)>> = (0..7).map(|j| {
                    let dx = x + 18.0 + f64::from(j) * 4.4;
                    vec![(dx, 126.0), (dx + 1.2, 126.0)]
                }).collect();
                b.add(root, parts::stroke_path("dots", (x + 16.0, 124.0, 34.0, 4.0), &dots, 1.0, "#a7aab0", false));
            }
        }
        // Power: an orange-tipped toggle, and the pilot ring under LPF.
        let pw = rk::mini_toggle(b, root, "Power", &a("power"), (89.0, 196.0), 14.0, "chrome");
        rk::label(b, pw, "Power", (89.0, 167.0), Ink::new(9.5, "#55585e").weight(500.0).track(0.02));
        let pilot = draw::circle("pilot", 836.0, 196.0, 7.0, draw::radial(&[(0.0, "#ffffff"), (0.45, "#ffd0c0"), (0.7, "#ff5a3a"), (1.0, "#ff5a3a00")]));
        b.add(root, parts::emit(pilot, "#ff6a4a", 0.8, 14.0));
        b.add(root, draw::stroked(draw::circle("pilot ring", 836.0, 196.0, 9.0, draw::solid("#00000000")), "#ff7a5a", 2.0));
    });
    let mut params = vec![
        rk::hz(&a("hpf"), "High Pass", 20.0, 400.0, 20.0),
        rk::hz(&a("lpf"), "Low Pass", 2000.0, 20000.0, 20000.0),
    ];
    for (addr, label, _) in BANDS {
        params.push(rk::db(&a(addr), &format!("{label} Gain"), -12.0, 12.0, 0.0).featured());
    }
    params.push(rk::toggle(&a("power"), "Power", true).featured());
    Face { file: "10-eq-tabletop-nine-band", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
