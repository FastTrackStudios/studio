//! The three amps, drawn straight on with their faceplates as the control
//! surface (the plugin shows the same controls on a floating strip; here
//! they live where the amp has them).
//!
//! - `clean-head` — a boutique clean head: brushed steel faceplate, chrome
//!   skirted knobs with dotted scales, red slide switches, a diamond-piped
//!   grille. Head, 1200 × 540.
//! - `lead-head` — a lead head: black faceplate, black chicken-heads under
//!   mini toggles, a script logo, basketweave grille, brown sides. Head,
//!   1200 × 540.
//! - `silk-combo` — a two-channel '60s combo: black faceplate, numbered
//!   skirted knobs, wheat grille. Combo, 1000 × 770.

use document_proto::{CornerRadii, NodeId};
use surface_kit::boutique::letters::Hand;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Binding, Range, Unit};
use surface_widgets::Builder;

use super::parts::{self, PLUGIN, knob, letter_in, print};
use crate::face::{Face, canvas};
use crate::reverb::ui;

/// A cabinet `w × h` of `tolex` filling `root`.
fn cabinet(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64, tolex: &str) -> NodeId {
    ui::fit_root(b, root, w, h);
    let p = Place { options: Some(vec![format!("w={w}"), format!("h={h}"), "r=16".into()]), ..Place::new("Cabinet").variant(tolex) };
    ui::put_at(b, root, "panel.amp_cabinet", &p, 0.0, 0.0)
}

/// A grille cloth `variant` at (`x`, `y`), `w × h`, framed by `trim`.
#[allow(clippy::too_many_arguments)]
fn grille(b: &mut Builder<'_>, parent: NodeId, variant: &str, x: f64, y: f64, w: f64, h: f64, trim: Option<&str>, extra: &[&str]) -> NodeId {
    let mut opts = vec![format!("w={w}"), format!("h={h}")];
    opts.extend(extra.iter().map(ToString::to_string));
    let p = Place { options: Some(opts), ..Place::new("Grille").variant(variant) };
    let g = ui::put_at(b, parent, "panel.amp_grille", &p, x, y);
    if let Some(t) = trim {
        b.add(parent, draw::stroked(draw::rounded(draw::rect("piping", x - 2.0, y - 2.0, w + 4.0, h + 4.0, draw::solid("#00000000")), 3.0), t, 2.5));
    }
    g
}

/// An input jack: a chrome ring round a black bore.
fn jack(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    b.add(parent, draw::with_effects(draw::circle("jack nut", cx, cy, r, draw::conic(0.0, &[(0.0, "#f0f0ec"), (0.25, "#77787a"), (0.5, "#e2e2de"), (0.75, "#5c5d60"), (1.0, "#f0f0ec")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    b.add(parent, draw::circle("jack bore", cx, cy, r * 0.55, draw::radial_at(0.5, 0.4, 0.8, &[(0.0, "#2a2a2c"), (1.0, "#000000")])));
}

/// A slotted screw head.
fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    b.add(parent, draw::with_effects(draw::circle("screw", cx, cy, r, draw::radial_at(0.35, 0.3, 0.9, &[(0.0, "#f4f4f0"), (0.6, "#9a9a96"), (1.0, "#4a4a48")])), vec![draw::drop_shadow("#00000070", 0.0, 0.6, 1.0)]));
    b.add(parent, draw::rotated(draw::rect("slot", cx - r * 0.7, cy - r * 0.12, r * 1.4, r * 0.24, draw::solid("#2a2a28")), 35.0));
}

/// The pilot/bypass jewel: a kit jewel on `addr`, its lamp flickering a
/// little like the valve heater behind it.
fn jewel(b: &mut Builder<'_>, parent: NodeId, addr: &str, variant: &str, d: f64, cx: f64, cy: f64) -> NodeId {
    let p = Place::new("Power").size(d).variant(variant).bind(Binding::param(addr)).legend("").touch(TOUCH_MIN);
    let j = ui::put(b, parent, "button.jewel", &p, cx, cy);
    ui::strip_legend(b, j);
    let leds: Vec<NodeId> = b.controls.get(&j).map(|c| c.slots_of(surface_proto::SlotRole::Led).map(NodeId).collect()).unwrap_or_default();
    for l in leds {
        surface_kit::motion::add(b, l, surface_kit::motion::flicker(0.06));
    }
    j
}

/// A mini slide switch on `addr`.
#[allow(clippy::too_many_arguments)]
fn slide(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, variant: &str, h: f64, cx: f64, cy: f64) -> NodeId {
    let p = Place::new(name).size(h).variant(variant).bind(Binding::param(addr)).legend("").touch(TOUCH_MIN);
    ui::put(b, parent, "switch.mini_slide", &p, cx, cy)
}

fn pct(ns: &str, id: &str, name: &str) -> surface_proto::ParamInfo {
    ui::param(&format!("{ns}/{id}"), name, Range::linear(0.0, 10.0, Unit::None), 5.0)
}

/// The monoline capitals the boutique head's panel is printed in.
const fn panel_hand(size: f64) -> Hand {
    Hand { size, weight: 0.12, slant: 0.0, wobble: 0.004, tracking: 0.1 }
}

/// The boutique clean head.
pub fn clean_head() -> Face {
    const W: f64 = 1200.0;
    const H: f64 = 540.0;
    const INK: &str = "#161616";
    let scene = canvas("Clean Head", |b, root| {
        let cab = cabinet(b, root, W, H, "black");
        // The brushed steel faceplate, speckled with age.
        let (px, py, pw, ph) = (40.0, 36.0, W - 80.0, 132.0);
        let mut plate = draw::frame("Faceplate", px, py, pw, ph);
        plate.fills = vec![draw::linear(180.0, &[(0.0, "#d9dadb"), (0.5, "#c3c4c6"), (1.0, "#a9aaad")]), draw::linear(90.0, &[(0.0, "#ffffff00"), (0.3, "#ffffff22"), (0.55, "#ffffff00"), (0.8, "#ffffff18"), (1.0, "#ffffff00")])];
        plate.corner_radii = CornerRadii::uniform(3.0);
        plate.effects = vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 4.0), draw::inner_shadow("#ffffff80", 0.0, 1.0, 0.5)];
        let plate = b.add(cab, plate);
        let mut rng = 7_u64;
        let mut specks = Vec::new();
        for _ in 0..260 {
            rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let x = (rng >> 20) as f64 % pw;
            rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let y = (rng >> 20) as f64 % ph;
            specks.push(vec![(x, y), (x + 0.8, y + 0.5)]);
        }
        b.add(plate, draw::with_opacity(ui::strokes("age", &specks, 1.3, draw::solid("#5a2a2a")), 0.35));
        let hand = panel_hand(8.0);
        let ky = 70.0;
        let ly = 116.0;
        // FET input and the normal input under it.
        print(b, plate, "FET", 52.0, 20.0, 10.0, INK, 700.0, 0.05);
        jack(b, plate, 78.0, 22.0, 9.0);
        jack(b, plate, 78.0, 100.0, 9.0);
        let dots: &[&str] = &["", "", "", "", "", "", "", "", "", "", ""];
        let chrome = |b: &mut Builder<'_>, addr: &str, name: &str, cx: f64| {
            parts::scaled_knob(b, plate, "knob.chrome_skirt", "chrome", addr, name, 50.0, cx, ky, dots, INK);
            letter_in(b, plate, name, cx, ly, &hand, INK);
        };
        chrome(b, "clean/volume", "VOLUME", 128.0);
        for (addr, name, top, bottom, cx) in [("clean/bright", "Bright", "BRIGHT", "", 180.0), ("clean/deep", "Deep", "DEEP", "", 225.0), ("clean/voice", "Guitar", "GUITAR", "MIC", 270.0)] {
            print(b, plate, top, cx, 20.0, 6.5, INK, 500.0, 0.02);
            screw(b, plate, cx, 34.0, 3.2);
            slide(b, plate, addr, name, "red", 26.0, cx, ky);
            screw(b, plate, cx, 104.0, 3.2);
            if !bottom.is_empty() {
                print(b, plate, bottom, cx, 118.0, 6.5, INK, 500.0, 0.02);
            }
        }
        for (addr, name, cx) in [("clean/treble", "TREBLE", 324.0), ("clean/middle", "MIDDLE", 395.0), ("clean/bass", "BASS", 466.0), ("clean/high", "HIGH", 538.0), ("clean/low", "LOW", 609.0)] {
            chrome(b, addr, name, cx);
        }
        letter_in(b, plate, "FILTER", 573.0, 16.0, &hand, INK);
        print(b, plate, "ON", 663.0, 22.0, 6.5, INK, 500.0, 0.02);
        screw(b, plate, 663.0, 36.0, 3.2);
        slide(b, plate, "clean/reverb_on", "Reverb on", "red", 26.0, 663.0, ky);
        screw(b, plate, 663.0, 104.0, 3.2);
        print(b, plate, "OFF", 663.0, 118.0, 6.5, INK, 500.0, 0.02);
        letter_in(b, plate, "REVERB", 752.0, 16.0, &hand, INK);
        chrome(b, "clean/send", "SEND", 716.0);
        chrome(b, "clean/return", "RETURN", 789.0);
        print(b, plate, "MID", 839.0, 22.0, 6.5, INK, 500.0, 0.02);
        screw(b, plate, 839.0, 36.0, 3.2);
        slide(b, plate, "clean/mid", "Mid", "red", 26.0, 839.0, ky);
        screw(b, plate, 839.0, 104.0, 3.2);
        chrome(b, "clean/level", "LEVEL", 888.0);
        // The logo: heavy slab capitals, hand-inked.
        let logo = Hand { size: 20.0, weight: 0.2, slant: 0.12, wobble: 0.01, tracking: 0.06 };
        letter_in(b, plate, "OPEN", 1015.0, 42.0, &logo, "#0e0e0e");
        letter_in(b, plate, "ROOM", 1015.0, 72.0, &logo, "#0e0e0e");
        letter_in(b, plate, "HAND WIRED", 1015.0, 104.0, &Hand { size: 9.0, weight: 0.12, slant: 0.0, wobble: 0.01, tracking: 0.14 }, "#1a1a1a");
        jewel(b, cab, "clean/on", "red", 30.0, 1128.0, 104.0);
        grille(b, cab, "diamond", 48.0, 182.0, W - 96.0, H - 222.0, None, &["rows=4"]);
    });
    let ns = "clean";
    let params = vec![
        pct(ns, "volume", "Volume").featured(),
        ui::toggle("clean/bright", "Bright", false),
        ui::toggle("clean/deep", "Deep", false),
        ui::choice("clean/voice", "Voicing", &["Mic", "Guitar"], 1),
        pct(ns, "treble", "Treble").featured(),
        pct(ns, "middle", "Middle").featured(),
        pct(ns, "bass", "Bass").featured(),
        pct(ns, "high", "Filter high"),
        pct(ns, "low", "Filter low"),
        ui::toggle("clean/reverb_on", "Reverb", false),
        pct(ns, "send", "Reverb send"),
        pct(ns, "return", "Reverb return"),
        ui::toggle("clean/mid", "Mid boost", false),
        pct(ns, "level", "Level").featured(),
        ui::toggle("clean/on", "Power", true),
    ];
    Face { file: "07-amp-head-clean-head", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The lead head.
pub fn lead_head() -> Face {
    const W: f64 = 1200.0;
    const H: f64 = 540.0;
    const INK: &str = "#e9e6df";
    let scene = canvas("Lead Head", |b, root| {
        let cab = cabinet(b, root, W, H, "black");
        // Brown sides show at the ends.
        for x in [0.0, W - 22.0] {
            b.add(cab, draw::rect("side", x, 0.0, 22.0, H, draw::linear(90.0, &[(0.0, "#6a4630"), (1.0, "#3e281a")])));
        }
        let (px, py, pw, ph) = (40.0, 30.0, W - 80.0, 152.0);
        let mut plate = draw::frame("Faceplate", px, py, pw, ph);
        plate.fills = vec![draw::linear(180.0, &[(0.0, "#1e1c1d"), (1.0, "#121112")])];
        plate.corner_radii = CornerRadii { top_left: 4.0, top_right: 30.0, bottom_right: 4.0, bottom_left: 4.0 };
        plate.effects = vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.5), draw::drop_shadow("#00000090", 0.0, 2.0, 3.0)];
        let plate = b.add(cab, plate);
        print(b, plate, "INPUT", 71.0, 42.0, 9.5, INK, 400.0, 0.0);
        jack(b, plate, 71.0, 74.0, 11.0);
        let toggles = [("lead/bright", "Bright", "BRIGHT", 138.0), ("lead/mid", "Mid", "MID", 218.0), ("lead/deep", "Deep", "DEEP", 299.0), ("lead/eq", "EQ", "EQ1", 379.0), ("lead/loop", "Loop bypass", "BYPASS", 540.0), ("lead/lead", "Lead", "LEAD", 622.0)];
        for (addr, name, legend, cx) in toggles {
            print(b, plate, legend, cx, 12.0, 6.5, INK, 500.0, 0.0);
            parts::toggle(b, plate, addr, name, 11.0, cx, 32.0);
        }
        print(b, plate, "EQ2", 379.0, 50.0, 6.5, INK, 500.0, 0.0);
        let knobs = [("lead/treble", "TREBLE", 138.0), ("lead/middle", "MIDDLE", 218.0), ("lead/bass", "BASS", 299.0), ("lead/gain", "GAIN", 379.0), ("lead/master", "MASTER", 460.0), ("lead/lead_gain", "LEAD GAIN", 540.0), ("lead/lead_master", "LEAD MASTER", 622.0), ("lead/reverb", "REVERB", 701.0), ("lead/presence", "PRESENCE", 780.0)];
        for (addr, name, cx) in knobs {
            print(b, plate, name, cx, 62.0, 9.5, INK, 400.0, -0.02);
            knob(b, plate, "knob.chicken_head", "black", addr, name, 46.0, cx, 106.0);
        }
        // The logo in a painted script, a model number under it in gold.
        letter_in(b, plate, "STUDIO", 915.0, 70.0, &Hand::script(22.0), "#f4f1ea");
        letter_in(b, plate, "LEAD", 950.0, 104.0, &Hand::script(18.0), "#d8b56a");
        print(b, plate, "ON", 1012.0, 44.0, 9.0, INK, 400.0, 0.0);
        parts::toggle(b, plate, "lead/on", "Standby", 13.0, 1012.0, 76.0);
        print(b, plate, "STANDBY", 1012.0, 124.0, 9.0, INK, 400.0, 0.0);
        // The pilot light, its filament wavering a little.
        b.add(plate, draw::circle("pilot bezel", 1070.0, 76.0, 9.5, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])));
        let lens = draw::circle("pilot", 1070.0, 76.0, 6.5, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#ffe0e0"), (0.35, "#ff4a4a"), (1.0, "#8a0a0a")]));
        let lens = b.add(plate, draw::emissive(lens, draw::hex("#ff2a2a"), 0.9, 18.0));
        surface_kit::motion::add(b, lens, surface_kit::motion::flicker(0.06));
        // A tolex band, then the grille in its frame.
        grille(b, cab, "basket", 52.0, 214.0, W - 104.0, H - 250.0, Some("#9a8a70"), &[]);
    });
    let ns = "lead";
    let params = vec![
        ui::toggle("lead/bright", "Bright", false),
        ui::toggle("lead/mid", "Mid", false),
        ui::toggle("lead/deep", "Deep", false),
        ui::choice("lead/eq", "EQ", &["EQ2", "EQ1"], 1),
        ui::toggle("lead/loop", "Loop bypass", false),
        ui::toggle("lead/lead", "Lead channel", false),
        pct(ns, "treble", "Treble").featured(),
        pct(ns, "middle", "Middle").featured(),
        pct(ns, "bass", "Bass").featured(),
        pct(ns, "gain", "Gain").featured(),
        pct(ns, "master", "Master").featured(),
        pct(ns, "lead_gain", "Lead gain"),
        pct(ns, "lead_master", "Lead master"),
        pct(ns, "reverb", "Reverb"),
        pct(ns, "presence", "Presence"),
        ui::toggle("lead/on", "Standby", true),
    ];
    Face { file: "08-amp-head-lead-head", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The two-channel combo.
pub fn silk_combo() -> Face {
    const W: f64 = 1000.0;
    const H: f64 = 770.0;
    const INK: &str = "#f1ece4";
    let scene = canvas("Silk Combo", |b, root| {
        let cab = cabinet(b, root, W, H, "black");
        let (px, py, pw, ph) = (24.0, 20.0, W - 48.0, 76.0);
        let mut plate = draw::frame("Faceplate", px, py, pw, ph);
        plate.fills = vec![draw::linear(180.0, &[(0.0, "#1a1918"), (1.0, "#0e0d0d")])];
        plate.stroke = document_proto::Stroke { paints: vec![draw::solid("#c9c3b8")], weight: 1.2, ..document_proto::Stroke::default() };
        plate.effects = vec![draw::inner_shadow("#ffffff1c", 0.0, 1.0, 0.5)];
        let plate = b.add(cab, plate);
        let ky = 36.0;
        let ly = 64.0;
        // Normal channel.
        print(b, plate, "1", 36.0, 10.0, 7.5, INK, 600.0, 0.0);
        print(b, plate, "2", 71.0, 10.0, 7.5, INK, 600.0, 0.0);
        jack(b, plate, 36.0, 30.0, 10.0);
        jack(b, plate, 71.0, 30.0, 10.0);
        print(b, plate, "NORMAL", 54.0, 56.0, 10.0, INK, 700.0, 0.02);
        print(b, plate, "BRIGHT", 98.0, 10.0, 5.5, INK, 700.0, 0.0);
        slide(b, plate, "combo/normal_bright", "Normal bright", "black", 22.0, 98.0, 30.0);
        let numbered = |b: &mut Builder<'_>, addr: &str, name: &str, cx: f64| {
            knob(b, plate, "knob.numbered_skirt", "black", addr, name, 42.0, cx, ky);
            print(b, plate, name, cx, ly, 7.0, INK, 700.0, 0.02);
        };
        for (addr, name, cx) in [("combo/normal_volume", "VOLUME", 145.0), ("combo/normal_treble", "TREBLE", 191.0), ("combo/normal_bass", "BASS", 240.0)] {
            numbered(b, addr, name, cx);
        }
        b.add(plate, draw::rect("divider", 272.0, 6.0, 1.2, ph - 12.0, draw::solid("#c9c3b8")));
        // Vibrato channel.
        print(b, plate, "1", 306.0, 10.0, 7.5, INK, 600.0, 0.0);
        print(b, plate, "2", 342.0, 10.0, 7.5, INK, 600.0, 0.0);
        jack(b, plate, 306.0, 30.0, 10.0);
        jack(b, plate, 342.0, 30.0, 10.0);
        print(b, plate, "VIBRATO", 324.0, 56.0, 10.0, INK, 700.0, 0.02);
        print(b, plate, "BRIGHT", 364.0, 10.0, 5.5, INK, 700.0, 0.0);
        slide(b, plate, "combo/bright", "Bright", "black", 22.0, 364.0, 30.0);
        for (addr, name, cx) in [("combo/volume", "VOLUME", 410.0), ("combo/treble", "TREBLE", 455.0), ("combo/bass", "BASS", 505.0), ("combo/reverb", "REVERB", 553.0), ("combo/speed", "SPEED", 601.0), ("combo/intensity", "INTENSITY", 648.0)] {
            numbered(b, addr, name, cx);
        }
        letter_in(b, plate, "SILK", 745.0, 26.0, &Hand::script(17.0), "#f2d9d4");
        letter_in(b, plate, "TONE", 772.0, 50.0, &Hand::script(17.0), "#f2d9d4");
        print(b, plate, "TWO CHANNEL", 795.0, 66.0, 6.0, INK, 700.0, 0.06);
        jewel(b, plate, "combo/on", "red", 26.0, 910.0, 30.0);
        grille(b, cab, "wheat", 34.0, 108.0, W - 68.0, H - 140.0, Some("#d8cbb2"), &[]);
    });
    let ns = "combo";
    let params = vec![
        ui::toggle("combo/normal_bright", "Normal bright", false),
        pct(ns, "normal_volume", "Normal volume"),
        pct(ns, "normal_treble", "Normal treble"),
        pct(ns, "normal_bass", "Normal bass"),
        ui::toggle("combo/bright", "Bright", false),
        pct(ns, "volume", "Volume").featured(),
        pct(ns, "treble", "Treble").featured(),
        pct(ns, "bass", "Bass").featured(),
        pct(ns, "reverb", "Reverb"),
        pct(ns, "speed", "Vibrato speed"),
        pct(ns, "intensity", "Vibrato intensity"),
        ui::toggle("combo/on", "Power", true),
    ];
    Face { file: "09-amp-combo-silk-combo", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
