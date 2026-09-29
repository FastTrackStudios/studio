//! Modern Rhythm EQ: a nine-band graphic equaliser as a tabletop unit —
//! Tabletop form at the manual render's aspect (1000 × 254, ≈ 3.9 : 1), the
//! black scratched colourway: a spun HPF knob and the POWER bat over it at
//! the left, nine faders with violet-lit caps between printed tick ladders,
//! each band's gain printed over it (−12.0 dB … +12.0 dB) and its frequency
//! under it, the spun LPF knob and a violet status lamp at the right.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_proto::{Control, ControlKind, Orientation, SlotRole, ValueFormat};
use surface_widgets::Builder;

use crate::delay::parts;
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Letter};

use super::PLUGIN;
use super::common::legend;

const W: f64 = 1000.0;
const H: f64 = 254.0;
const INK: &str = "#e9e9ea";
const VIOLET: &str = "#c38bff";

pub fn faces() -> Vec<Face> {
    vec![nine_band()]
}

/// A graphic-EQ fader: a black slot, a lit cap that rides it (`Thumb`),
/// the band's gain printed live over it (`Value`).
#[allow(clippy::too_many_arguments)]
fn fader(b: &mut Builder<'_>, p: NodeId, name: &str, addr: &str, (cx, y0, len): (f64, f64, f64), value_y: f64, freq: &str, freq_y: f64) -> NodeId {
    let w = 44.0;
    let root = b.add(p, draw::frame(name, cx - w / 2.0, y0, w, len));
    let slot = draw::rounded(draw::rect("slot", w / 2.0 - 6.0, 0.0, 12.0, len, draw::linear(90.0, &[(0.0, "#000000"), (0.5, "#151517"), (1.0, "#000000")])), 5.0);
    b.add(root, draw::with_effects(draw::stroked(slot, "#3a3b3e", 1.0), vec![draw::inner_shadow("#000000", 0.0, 2.0, 3.0)]));
    let cap_h = 26.0;
    let lane = b.add(root, draw::frame("lane", 0.0, 0.0, w, len));
    let cap = b.add(lane, draw::frame("cap", w / 2.0 - 8.0, 0.0, 16.0, cap_h));
    let body = draw::rounded(draw::rect("cap body", 0.0, 0.0, 16.0, cap_h, draw::linear(180.0, &[(0.0, "#1a1a1c"), (0.14, "#3a3a3e"), (0.2, "#8e6cc0"), (0.5, "#f1e4ff"), (0.8, "#8e6cc0"), (0.86, "#3a3a3e"), (1.0, "#1a1a1c")])), 2.0);
    b.add(cap, draw::emissive(body, draw::hex(VIOLET), 0.8, 18.0));
    // The live gain over the fader.
    let size = 9.0;
    let mut t = parts::text_node("", -30.0, value_y - y0, w + 60.0, &parts::Ink::new(size, INK).weight(700.0).track(0.0).plain(), TextAlign::Center);
    t.name = "value".into();
    let value = b.add(root, t);
    let mut c = Control::new(ControlKind::Fader { orientation: Orientation::Vertical }).slot(SlotRole::Thumb, cap.0).slot(SlotRole::Value, value.0);
    c.binding = Some(rk::bind(addr));
    c.format = Some(ValueFormat { decimals: Some(1), ..ValueFormat::default() });
    b.controls.insert(root, c);
    legend(b, root, freq, (cx, freq_y), Letter::new(9.0, 700.0, INK));
    root
}

/// The printed tick ladder between two faders.
fn ladder(b: &mut Builder<'_>, p: NodeId, cx: f64, (y0, y1): (f64, f64)) {
    let n = 13;
    let lines: Vec<Vec<(f64, f64)>> = (0..n).map(|i| {
        let y = y0 + (y1 - y0) * f64::from(i) / f64::from(n - 1);
        let hw = if i % 6 == 0 { 9.0 } else { 6.0 };
        vec![(cx - hw, y), (cx + hw, y)]
    }).collect();
    b.add(p, draw::with_opacity(parts::stroke_path("ladder", (0.0, 0.0, W, H), &lines, 1.0, INK, false), 0.45));
}

/// A spun filter knob with its readout over it ("HPF : OFF").
#[allow(clippy::too_many_arguments)]
fn filter(b: &mut Builder<'_>, p: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64), label: &str, glyph: bool) {
    let place = Place::new(name).size(62.0).variant("silver").bind(rk::bind(addr)).touch(surface_kit::TOUCH_MIN).options(&["ticks=0"]);
    let k = parts::put(b, p, "knob.spun_cap", &place, cx, cy);
    parts::strip_legend(b, k);
    // The readout: the name, then the frequency (or OFF at the stop).
    rk::letter_aligned(b, p, &format!("{label} :"), (cx - 2.0, 22.0), Letter::new(9.0, 700.0, INK), TextAlign::Right);
    let mut t = parts::text_node("", cx + 3.0, 22.0, 70.0, &parts::Ink::new(9.0, INK).weight(700.0).track(0.02), TextAlign::Left);
    t.name = "value".into();
    let v = b.add(k, {
        let (tx, ty) = b.scene.node(k).map_or((0.0, 0.0), |n| (n.transform.tx, n.transform.ty));
        let mut t = t;
        t.transform.tx -= tx;
        t.transform.ty -= ty;
        t
    });
    if let Some(c) = b.controls.get_mut(&k) {
        c.slots.push(surface_proto::Slot { role: SlotRole::Value, node: v.0 });
        c.format = Some(ValueFormat { decimals: Some(0), ..ValueFormat::default() });
    }
    // The slope glyph under the readout: a low cut rises, a high cut falls.
    let y = 50.0;
    let pts = if glyph { vec![(cx - 7.0, y + 5.0), (cx - 2.0, y), (cx + 7.0, y)] } else { vec![(cx - 7.0, y), (cx + 2.0, y), (cx + 7.0, y + 5.0)] };
    rk::line(b, p, &pts, 1.6, INK);
}

/// The nine-band graphic EQ — Tabletop, 1000 × 254.
fn nine_band() -> Face {
    let ns = "mreq";
    let a = |s: &str| format!("{ns}/{s}");
    let bands = ["75 Hz", "150 Hz", "250 Hz", "400 Hz", "800 Hz", "1.5 kHz", "4.5 kHz", "8 kHz", "12 kHz"];
    let scene = canvas("Nine-Band Graphic", |b, root| {
        parts::fit_root(b, root, W, H);
        // Rubber feet under the case.
        for x in [110.0, 890.0] {
            b.add(root, draw::rounded(draw::rect("foot", x - 50.0, H - 16.0, 100.0, 16.0, draw::linear(180.0, &[(0.0, "#1c1c1e"), (1.0, "#050506")])), 5.0));
        }
        let plate = draw::rounded(draw::rect("case", 0.0, 0.0, W, H - 12.0, draw::linear(180.0, &[(0.0, "#46474a"), (0.1, "#2d2e31"), (0.9, "#1f2023"), (1.0, "#0f1012")])), 8.0);
        b.add(root, draw::with_effects(draw::stroked(plate, "#000000", 1.5), vec![draw::drop_shadow("#000000c0", 0.0, 6.0, 14.0), draw::inner_shadow("#ffffff26", 0.0, 1.5, 1.0)]));
        // Scratched metal: pale scuffs and fine grain.
        let mut s: u64 = 0x9e11;
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut d = String::new();
        for _ in 0..160 {
            let (px, py) = (8.0 + rnd() * (W - 16.0), 8.0 + rnd() * (H - 30.0));
            let (lx, ly) = (5.0 + rnd() * 30.0, (rnd() - 0.5) * 16.0);
            d.push_str(&format!("M {px:.1} {py:.1} l {lx:.1} {ly:.1} l 0.4 0.5 l {:.1} {:.1} Z ", -lx, -ly));
        }
        b.add(root, draw::with_opacity(draw::path("scratches", 0.0, 0.0, W, H, d, draw::solid("#ffffff")), 0.09));
        let mut d = String::new();
        for _ in 0..40 {
            let (px, py) = (20.0 + rnd() * (W - 40.0), 15.0 + rnd() * (H - 45.0));
            let r = 6.0 + rnd() * 18.0;
            d.push_str(&format!("M {px:.1} {py:.1} q {:.1} {:.1} {:.1} {:.1} q {:.1} {:.1} {:.1} {:.1} Z ", r, -r * 0.3, r * 1.6, r * 0.2, -r * 0.6, r * 0.5, -r * 1.6, -r * 0.2));
        }
        b.add(root, draw::with_opacity(draw::path("scuffs", 0.0, 0.0, W, H, d, draw::solid("#ffffff")), 0.05));

        let xs: Vec<f64> = (0..9).map(|i| 237.0 + 65.5 * f64::from(i)).collect();
        for i in 0..8 {
            ladder(b, root, (xs[i] + xs[i + 1]) / 2.0, (58.0, 200.0));
        }
        for (i, (x, f)) in xs.iter().zip(bands).enumerate() {
            fader(b, root, f, &a(&format!("b{}", i + 1)), (*x, 50.0, 158.0), 22.0, f, 222.0);
        }
        filter(b, root, "HPF", &a("hpf"), (128.0, 108.0), "HPF", true);
        filter(b, root, "LPF", &a("lpf"), (872.0, 108.0), "LPF", false);
        let t = rk::mini_toggle(b, root, "Power", &a("power"), (128.0, 182.0 - 6.0), 12.0, "chrome");
        legend(b, t, "POWER", (128.0, 216.0), Letter::new(8.0, 760.0, INK).track(0.04));
        rk::led(b, root, (872.0, 182.0), 9.0, VIOLET, true);
    });
    let mut params: Vec<surface_proto::ParamInfo> = (1..=9).map(|i| rk::db(&a(&format!("b{i}")), &format!("Band {} Gain", bands[i - 1]), -12.0, 12.0, 0.0)).collect();
    params[0] = params[0].clone().featured();
    params.push(rk::hz(&a("hpf"), "High-Pass", 20.0, 1000.0, 20.0).featured());
    params.push(rk::hz(&a("lpf"), "Low-Pass", 2000.0, 20000.0, 20000.0).featured());
    params.push(rk::toggle(&a("power"), "Power", true).featured());
    Face { file: "10-eq-tabletop-nine-band", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
