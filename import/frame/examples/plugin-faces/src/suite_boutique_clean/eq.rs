//! `09-eq-rack-1140x286`: the nine-band graphic EQ in its blush-pink desk
//! case (1140 wide, the case's own 4:1 aspect) — HPF and LPF on dark
//! scalloped knobs with brass lines, a brass bypass toggle, nine black
//! faders over tick scales with the gain printed above each and the band
//! below, and the teal pilot.

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_proto::{Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::look::{self, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, RACK_W, Type};

const INK: &str = "#1e1719";
const BANDS: [(&str, &str); 9] = [("65", "65 Hz"), ("125", "125 Hz"), ("250", "250 Hz"), ("500", "500 Hz"), ("1k", "1 kHz"), ("2k", "2 kHz"), ("4k", "4 kHz"), ("8k", "8 kHz"), ("16k", "16 kHz")];

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

pub(super) fn face() -> Face {
    let (w, h) = (RACK_W, 286.0);
    let scene = canvas("Graphic EQ", |b, root| {
        parts::page(b, root, w, h);
        let case = look::slab("Case", (0.0, 0.0, w, h - 12.0), 16.0, vec![draw::linear(180.0, &[(0.0, "#f6dcdd"), (0.12, "#ecc6c8"), (0.85, "#e3b6b9"), (1.0, "#cf9ea2")])]);
        let case = b.add(root, draw::with_effects(case, vec![draw::drop_shadow("#00000066", 0.0, 8.0, 16.0), draw::inner_shadow("#ffffffb0", 0.0, 2.0, 1.0), draw::inner_shadow("#9a5a6255", 0.0, -3.0, 5.0)]));
        for x in [120.0, w - 170.0] {
            b.add(root, draw::rounded(draw::rect("foot", x, h - 14.0, 50.0, 14.0, draw::solid("#1b1b1d")), 4.0));
        }
        let lab = Type::new(10.5, INK, 800.0);
        // HPF.
        parts::words(b, case, "HPF :", 100.0, 35.0, 44.0, TextAlign::Right, lab);
        readout(b, case, "eq/hpf", (148.0, 35.0), 60.0, lab, TextAlign::Left);
        parts::knob(b, case, "knob.fluted_line", "brown", "eq/hpf", "HPF", 64.0, (153.0, 116.0), &["ticks=0", "lobes=9", "line=#e0b060"], INK);
        parts::art(b, case, &[vec![(142.0, 176.0), (146.0, 169.0), (158.0, 169.0)]], 1.8, INK);
        look::brass_toggle(b, case, "eq/on", "Bypass", 20.0, (150.0, 218.0));
        // LPF.
        parts::words(b, case, "LPF :", 944.0, 35.0, 44.0, TextAlign::Right, lab);
        readout(b, case, "eq/lpf", (992.0, 35.0), 70.0, lab, TextAlign::Left);
        parts::knob(b, case, "knob.fluted_line", "brown", "eq/lpf", "LPF", 64.0, (990.0, 116.0), &["ticks=0", "lobes=9", "line=#e0b060"], INK);
        parts::art(b, case, &[vec![(980.0, 169.0), (992.0, 169.0), (996.0, 176.0)]], 1.8, INK);
        parts::status_led(b, case, "eq/on", 12.0, (990.0, 218.0), TEAL, "light", None);
        // The nine faders.
        for (i, (id, band)) in BANDS.iter().enumerate() {
            let x = 262.0 + 76.4 * i as f64;
            let addr = format!("eq/g{id}");
            let p = Place::new(*band).size(150.0).variant("black").bind(ui::bind(&addr)).legend("").ink(surface_kit::Ink::Color("#6a5256".into()));
            let f = surface_kit::place(b, case, "lever.fader", &p).unwrap_or_else(|e| panic!("{e}"));
            ui::strip_legend(b, f);
            // The fader's lane runs from its top: put its travel 58..208.
            let fw = b.scene.node(f).map_or(0.0, |n| n.size.width);
            ui::move_to(b, f, x - fw / 2.0, 58.0 - 20.0);
            readout(b, case, &addr, (x, 35.0), 70.0, lab, TextAlign::Center);
            parts::centred(b, case, band, (x, 232.0), lab);
        }
    });
    let mut params = vec![parts::hz("eq/hpf", "HPF", 20.0, 500.0, 20.0), parts::hz("eq/lpf", "LPF", 2000.0, 20000.0, 20000.0), parts::switch("eq/on", "Bypass", true)];
    for (id, band) in BANDS {
        params.push(parts::db(&format!("eq/g{id}"), band, -12.0, 12.0, 0.0));
    }
    look::face("09-eq-rack-1140x286", scene, params)
}
