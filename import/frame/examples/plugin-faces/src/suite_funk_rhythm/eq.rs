//! `10-eq-desktop-1140x282`: the *Nine-Band Desk EQ*, a desktop graphic
//! EQ as wide as a rack face (1140 × 282, the reference's 4:1) in a
//! speckled mauve-grey case — big taupe dome knobs for the high-pass and
//! low-pass filters (their settings printed live above), nine lit white
//! faders from 65 Hz to 16 kHz with their gain printed above, a cream
//! POWER button and its pilot LED.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Range, Unit};
use surface_widgets::Builder;

use super::parts::{self, RACK_W, Type};
use super::pedals::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

const BANDS: &[(&str, &str, f64)] = &[
    ("b65", "65 Hz", 272.0),
    ("b125", "125 Hz", 347.0),
    ("b250", "250 Hz", 420.0),
    ("b500", "500 Hz", 495.0),
    ("b1k", "1 kHz", 569.0),
    ("b2k", "2 kHz", 642.0),
    ("b4k", "4 kHz", 718.0),
    ("b8k", "8 kHz", 792.0),
    ("b16k", "16 kHz", 865.0),
];
const INK: &str = "#f4efec";

/// A live value line: `prefix` printed, then the param's value.
fn value_line(b: &mut Builder<'_>, parent: NodeId, addr: &str, prefix: &str, (cx, cy): (f64, f64), w: f64) {
    let t = Type::new(10.5, INK, 800.0).track(0.06);
    if prefix.is_empty() {
        ui::lit_readout(b, parent, &format!("{addr} readout"), ui::bind(addr), cx - w / 2.0, cy, w, 10.5, INK, document_proto::TextAlign::Center);
    } else {
        parts::words(b, parent, prefix, cx - w / 2.0, cy, w / 2.0 - 2.0, document_proto::TextAlign::Right, t);
        ui::lit_readout(b, parent, &format!("{addr} readout"), ui::bind(addr), cx + 2.0, cy, w / 2.0, 10.5, INK, document_proto::TextAlign::Left);
    }
}

/// The filter's slope drawn over its knob.
fn slope(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), high: bool) {
    let pts = if high { vec![(cx - 9.0, cy + 4.0), (cx - 4.0, cy - 2.0), (cx + 9.0, cy - 2.0)] } else { vec![(cx - 9.0, cy - 2.0), (cx + 4.0, cy - 2.0), (cx + 9.0, cy + 4.0)] };
    parts::art(b, parent, &[pts], 2.0, INK);
}

pub(super) fn face() -> Face {
    let (w, h) = (RACK_W, 282.0);
    let scene = canvas("Nine-Band Desk EQ", |b, root| {
        parts::page(b, root, w, h);
        let case = draw::rect("case", 0.0, 0.0, w, h, draw::linear(180.0, &[(0.0, "#b3a2a4"), (0.12, "#9f8e90"), (0.85, "#8e7e80"), (1.0, "#6f6163")]));
        let case = b.add(root, draw::with_effects(draw::rounded(case, 12.0), vec![draw::drop_shadow("#00000080", 0.0, 6.0, 12.0), draw::inner_shadow("#ffffff4d", 0.0, 2.0, 2.0), draw::inner_shadow("#00000059", 0.0, -4.0, 6.0)]));
        // Speckle.
        let mut sp = String::new();
        let mut seed: u32 = 0x1234_5678;
        for _ in 0..2400 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let (x, y) = (f64::from(seed % 11_400) / 10.0, f64::from((seed / 11_400) % 2_820) / 10.0);
            sp.push_str(&format!("M {x:.1} {y:.1} h 0.7 v 0.7 h -0.7 Z "));
        }
        b.add(case, draw::path("speckle", 0.0, 0.0, w, h, sp, draw::solid("#ffffff1c")));
        // Filters.
        for (addr, prefix, x, high) in [("eq/hpf", "HPF :", 174.0, true), ("eq/lpf", "LPF :", 967.0, false)] {
            value_line(b, case, addr, prefix, (x, 36.0), 150.0);
            slope(b, case, (x, 72.0), high);
            let p = Place::new(prefix).size(92.0).variant("taupe").bind(ui::bind(addr)).options(&["dots=0", "smooth"]).touch(TOUCH_MIN);
            let k = ui::put(b, case, "knob.dot_knurl", &p, x, 139.0);
            ui::strip_legend(b, k);
        }
        // Bands.
        for (id, hz, x) in BANDS {
            let addr = format!("eq/{id}");
            value_line(b, case, &addr, "", (*x, 36.0), 70.0);
            let p = Place::new(*hz).size(150.0).bind(ui::bind(&addr)).touch(TOUCH_MIN);
            let f = surface_kit::place(b, case, "lever.lit_fader", &p).unwrap_or_else(|e| panic!("{e}"));
            parts::centre_on(b, f, (*x, 141.0));
            parts::centred(b, case, hz, (*x, 251.0), Type::new(10.5, INK, 800.0).track(0.06));
        }
        // Power.
        let p = Place::new("Power").size(30.0).variant("cream").bind(ui::bind("eq/power")).options(&["nolamp"]).legend("").touch(TOUCH_MIN);
        let k = surface_kit::place(b, case, "button.round_cap", &p).unwrap_or_else(|e| panic!("{e}"));
        ui::strip_legend(b, k);
        parts::centre_on(b, k, (174.0, 229.0));
        parts::centred(b, case, "POWER", (174.0, 259.0), Type::new(8.5, INK, 800.0).track(0.2));
        parts::status_led(b, case, "eq/power", 7.0, (964.0, 232.0), "#eef6ff", "chrome", None);
    });
    let mut params = vec![
        parts::hz("eq/hpf", "HPF", 20.0, 1000.0, 20.0),
        parts::hz("eq/lpf", "LPF", 1000.0, 20000.0, 20000.0),
        parts::switch("eq/power", "Power", true),
    ];
    for (id, hz, _) in BANDS {
        params.push(ui::param(&format!("eq/{id}"), hz, Range::linear(-12.0, 12.0, Unit::Db), 0.0).featured().bipolar());
    }
    ui::formatted(Face { file: "10-eq-desktop-1140x282", plugin: PLUGIN, scene, params, streams: Vec::new() })
}
