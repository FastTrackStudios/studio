//! `09-eq-rack-1140x286`: the nine-band graphic EQ in its desk case (1140
//! wide, the case's own 4:1 aspect), in the three colourways the rig ships
//! (`eq/color`, each a layer shown for its option: blush-pink plastic,
//! brushed gold, or cream) — HPF and LPF on dark scalloped knobs with
//! brass lines (both open, as the reference shows them), a brass bypass
//! toggle, nine wide faders (deep rounded slots, dark domed caps with a
//! cream index) over tick scales with the gain printed above each and the
//! band below, and the teal pilot.

use document_proto::{NodeId, TextAlign};
use surface_kit::materials::{Dir, Material, fill};
use surface_kit::draw;
use surface_proto::{Control, ControlKind, Orientation, SlotRole};
use surface_widgets::Builder;

use super::look::{self, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, KnobLook, RACK_W, Type};

const INK: &str = "#1e1719";
const BANDS: [(&str, &str); 9] = [("65", "65 Hz"), ("125", "125 Hz"), ("250", "250 Hz"), ("500", "500 Hz"), ("1k", "1 kHz"), ("2k", "2 kHz"), ("4k", "4 kHz"), ("8k", "8 kHz"), ("16k", "16 kHz")];
/// The fader travel: top of the lane and its length.
const LANE: (f64, f64) = (58.0, 150.0);

/// A plain printed readout of `addr` (`Value` slot), centred at `at`.
pub(super) fn readout(b: &mut Builder<'_>, parent: NodeId, addr: &str, at: (f64, f64), w: f64, t: Type<'_>, align: TextAlign) -> NodeId {
    let h = (t.size * 1.6).ceil();
    let x = match align {
        TextAlign::Left => at.0,
        TextAlign::Right => at.0 - w,
        _ => at.0 - w / 2.0,
    };
    let root = b.add(parent, draw::frame(&format!("{addr} readout"), x, at.1 - h / 2.0, w, h));
    let v = parts::words(b, root, "", 0.0, h / 2.0, w, align, t);
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, v.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A wide EQ fader centred on `x`: a deep rounded slot between tick
/// scales and a dark domed cap with a cream index, the cap riding a lane
/// `LANE.1` long (a vertical `Fader` whose `Thumb` is the cap).
fn fader(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, x: f64) -> NodeId {
    let (top, travel) = LANE;
    let (cap_w, cap_h) = (34.0, 20.0);
    let w = 62.0;
    let root = b.add(parent, draw::frame(name, x - w / 2.0, top - cap_h / 2.0, w, travel + cap_h));
    let c = w / 2.0;
    // Tick scales either side.
    let ticks: Vec<Vec<(f64, f64)>> = (0..=12)
        .flat_map(|i| {
            let y = cap_h / 2.0 + travel * f64::from(i) / 12.0;
            let len = if i % 6 == 0 { 9.0 } else { 6.0 };
            [vec![(c - 13.0 - len, y), (c - 13.0, y)], vec![(c + 13.0, y), (c + 13.0 + len, y)]]
        })
        .collect();
    b.add(root, ui::strokes("ticks", &ticks, 0.9, draw::solid("#1e171966")));
    let slot = draw::rounded(draw::rect("slot", c - 7.0, cap_h / 2.0 - 6.0, 14.0, travel + 12.0, draw::linear(90.0, &[(0.0, "#050405"), (0.5, "#221b1d"), (1.0, "#0a0809")])), 7.0);
    b.add(root, draw::with_effects(slot, vec![draw::drop_shadow("#ffffff80", 0.0, 1.0, 0.0)]));
    let lane = b.add(root, draw::frame("lane", c - cap_w / 2.0, 0.0, cap_w, travel + cap_h));
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap_h));
    let body = draw::rounded(draw::rect("cap body", 0.0, 0.0, cap_w, cap_h, draw::linear(180.0, &[(0.0, "#5a5456"), (0.35, "#2a2426"), (1.0, "#0d0a0b")])), cap_h / 2.0);
    b.add(cap, draw::with_effects(body, vec![draw::drop_shadow("#000000a0", 1.0, 4.0, 5.0)]));
    b.add(cap, draw::rounded(draw::rect("cap sheen", 4.0, 2.0, cap_w - 8.0, cap_h * 0.35, draw::solid("#ffffff24")), cap_h * 0.2));
    b.add(cap, draw::rounded(draw::rect("index", 5.0, cap_h / 2.0 - 1.2, cap_w - 10.0, 2.4, draw::solid("#efe0c4")), 1.2));
    let mut ctl = Control::new(ControlKind::Fader { orientation: Orientation::Vertical }).slot(SlotRole::Thumb, cap.0);
    ctl.binding = Some(ui::bind(addr));
    b.controls.insert(root, ctl);
    root
}

/// The case in colourway `i`: blush paint, brushed gold, cream paint.
fn case(b: &mut Builder<'_>, layer: NodeId, (w, h): (f64, f64), i: usize) {
    let mut shell = draw::rounded(draw::frame("Case", 0.0, 0.0, w, h), 16.0);
    shell.effects = vec![draw::drop_shadow("#00000066", 0.0, 8.0, 16.0)];
    let shell = b.add(layer, shell);
    let m = match i {
        0 => Material::Powder { color: "#eec7ca".into(), coarse: false },
        1 => Material::Brushed { color: "#c79a4e".into(), dir: Dir::Horizontal },
        _ => Material::Powder { color: "#f0ece5".into(), coarse: false },
    };
    fill(b, shell, (0.0, 0.0, w, h), 16.0, &m);
    for n in surface_kit::materials::light::edge("case edge", (0.0, 0.0, w, h), 16.0, 10.0, 1.0, true) {
        b.add(shell, n);
    }
}

pub(super) fn face() -> Face {
    let (w, h) = (RACK_W, 286.0);
    let scene = canvas("Graphic EQ", |b, root| {
        parts::page(b, root, w, h);
        for x in [120.0, w - 170.0] {
            b.add(root, draw::rounded(draw::rect("foot", x, h - 14.0, 50.0, 14.0, draw::solid("#1b1b1d")), 4.0));
        }
        for i in 0..3 {
            let layer = parts::variant_layer(b, root, "eq/color", i, (0.0, 0.0, w, h - 12.0));
            case(b, layer, (w, h - 12.0), i);
        }
        let case = b.add(root, draw::frame("Controls", 0.0, 0.0, w, h - 12.0));
        let lab = Type::new(10.5, INK, 700.0);
        let knob = KnobLook { cap: "fluted", opts: &["color=#2a1d1c", "pointer=#e0b060", "ticks=none"] };
        // HPF.
        parts::words(b, case, "HPF :", 100.0, 35.0, 44.0, TextAlign::Right, lab);
        readout(b, case, "eq/hpf", (148.0, 35.0), 60.0, lab, TextAlign::Left);
        parts::knob(b, case, &knob, "eq/hpf", "HPF", 62.0, (153.0, 116.0), INK);
        parts::art(b, case, &[vec![(142.0, 176.0), (146.0, 169.0), (158.0, 169.0)]], 1.8, INK);
        look::brass_toggle(b, case, "eq/on", "Bypass", 20.0, (150.0, 218.0));
        // LPF.
        parts::words(b, case, "LPF :", 944.0, 35.0, 44.0, TextAlign::Right, lab);
        readout(b, case, "eq/lpf", (992.0, 35.0), 70.0, lab, TextAlign::Left);
        parts::knob(b, case, &knob, "eq/lpf", "LPF", 62.0, (990.0, 116.0), INK);
        parts::art(b, case, &[vec![(980.0, 169.0), (992.0, 169.0), (996.0, 176.0)]], 1.8, INK);
        parts::status_led(b, case, "eq/on", 12.0, (990.0, 218.0), TEAL, "light", None);
        // The nine faders.
        for (i, (id, band)) in BANDS.iter().enumerate() {
            let x = 262.0 + 76.4 * i as f64;
            let addr = format!("eq/g{id}");
            fader(b, case, &addr, band, x);
            readout(b, case, &addr, (x, 35.0), 70.0, lab, TextAlign::Center);
            parts::centred(b, case, band, (x, 238.0), lab);
        }
    });
    let mut params = vec![
        parts::pick("eq/color", "Colour", &["Blush", "Gold", "Cream"], 0),
        parts::hz("eq/hpf", "HPF", 20.0, 500.0, 20.0),
        parts::hz("eq/lpf", "LPF", 2000.0, 20000.0, 20000.0),
        parts::switch("eq/on", "Bypass", true),
    ];
    // The reference's faders: top, middle, bottom, repeated.
    for (i, (id, band)) in BANDS.iter().enumerate() {
        params.push(parts::db(&format!("eq/g{id}"), band, -12.0, 12.0, [12.0, 0.0, -12.0][i % 3]));
    }
    look::face("09-eq-rack-1140x286", scene, params)
}
