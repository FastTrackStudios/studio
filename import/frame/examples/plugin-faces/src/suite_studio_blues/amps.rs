//! The three amps, drawn straight on as their product renders show them —
//! a tolex box with lit edges, metal corners, a handle and feet, a
//! faceplate standing proud and a grille sat back (`materials::cabinet`) —
//! with their faceplates as the control surface.
//!
//! - `clean-head` — a boutique clean head: an aged brushed-steel
//!   faceplate, chrome caps on ribbed skirts, red slide switches, a heavy
//!   slab-serif logo, red-and-green piped diamond grille cloth, black
//!   corners. Head, 1200 × 548.
//! - `lead-head` — a lead head: black faceplate, black bakelite
//!   chicken-heads under mini toggles, a script logo with its model in gold
//!   marker, black basketweave behind gold piping, a black handle. Head,
//!   1200 × 548.
//! - `silk-combo` — a two-channel '60s combo in brown tolex: a black
//!   faceplate in a silver frame, black skirted knobs numbered round the
//!   dial, a script logo, wheat-and-oxblood cloth. Combo, 1000 × 753.

use document_proto::{Node, NodeId};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::materials::cabinet::{self, CabinetSpec, Handle, Kind};
use surface_kit::materials::light;
use surface_kit::materials::paint::{Cloth, Dir, Grain, Material};
use surface_kit::{Place, TOUCH_MIN};
use surface_proto::{Binding, Range, Unit};
use surface_widgets::Builder;

use super::parts::{self, Cap, PLUGIN, knob, knob_scaled, print, set};
use crate::face::{Face, canvas};
use crate::reverb::ui;

/// Build `spec` into `root` (sized to it); returns the faceplate and the
/// grille frames and the faceplate's size.
fn build(b: &mut Builder<'_>, root: NodeId, spec: &CabinetSpec) -> (NodeId, NodeId, (f64, f64)) {
    ui::fit_root(b, root, spec.w, spec.h);
    let c = cabinet::cabinet(b, root, "Cabinet", spec);
    // These heads' handle brackets are blackened steel, like their corners.
    if spec.handle == Handle::Leather {
        let hh = spec.h * 0.09;
        let bs = hh * 0.55;
        for cx in [spec.w * 0.38 + hh * 0.15, spec.w * 0.62 - hh * 0.15] {
            let br = draw::rounded(draw::rect("bracket", cx - bs * 0.6, hh - bs * 0.55, bs * 1.2, bs * 0.9, draw::linear(light::sheen_angle(), &[(0.0, "#6a6c70"), (0.45, "#2a2b2e"), (1.0, "#111214")])), bs * 0.25);
            b.add(c.root, br);
        }
    }
    let size = b.scene.node(c.faceplate).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    (c.faceplate, c.grille, size)
}

/// A cloth (the kit's woven grille) over the whole of `grille`, recessed.
fn cloth(b: &mut Builder<'_>, grille: NodeId, variant: &str, extra: &[&str]) {
    let (w, h) = b.scene.node(grille).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let mut opts = vec![format!("w={w}"), format!("h={h}"), "r=0".into()];
    opts.extend(extra.iter().map(ToString::to_string));
    let p = Place { options: Some(opts), ..Place::new("Cloth").variant(variant) };
    let g = ui::put_at(b, grille, "panel.amp_grille", &p, 0.0, 0.0);
    // The recess is lit as an edge, not an inner shadow (a big effect layer).
    if let Some(n) = b.scene.node_mut(g) {
        n.effects.clear();
    }
    for e in light::edge("cloth recess", (0.0, 0.0, w, h), 2.0, 10.0, 1.2, false) {
        b.add(grille, e);
    }
}

/// An input jack: a chrome nut round a black bore.
fn jack(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    let (hx, hy) = light::hotspot();
    b.add(parent, draw::with_effects(draw::circle("jack nut", cx, cy, r, draw::radial_at(hx, hy, 0.9, &[(0.0, "#fafaf8"), (0.5, "#9a9b9e"), (1.0, "#3e3f42")])), light::stand(1.5, 1.0)));
    b.add(parent, draw::circle("jack bore", cx, cy, r * 0.55, draw::radial_at(0.5, 0.4, 0.8, &[(0.0, "#2a2a2c"), (1.0, "#000000")])));
}

/// A slotted screw head.
fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64) {
    surface_kit::materials::enclosure::screw(b, parent, cx, cy, r);
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

fn pct(ns: &str, id: &str, name: &str, default: f64) -> surface_proto::ParamInfo {
    ui::param(&format!("{ns}/{id}"), name, Range::linear(0.0, 10.0, Unit::None), default)
}


fn with_shadow(n: Node, a: &str) -> Node {
    draw::with_effects(n, vec![draw::drop_shadow(a, 0.0, 1.0, 1.0)])
}

/// The boutique clean head.
pub fn clean_head() -> Face {
    const W: f64 = 1200.0;
    const H: f64 = 548.0;
    const INK: &str = "#141414";
    let scene = canvas("Clean Head", |b, root| {
        let mut spec = CabinetSpec::new(Kind::Head, W, H);
        spec.covering = Material::Tolex { grain: Grain::Bronco, color: "#1d1c1c".into(), scale: 1.0 };
        spec.grille = Material::DiamondGrille { cloth: "#161314".into(), stripes: vec!["#b8324a".into(), "#e6e2d6".into(), "#5f9e86".into()], pitch: 96.0 };
        spec.face = Material::Brushed { color: "#c3c4c6".into(), dir: Dir::Horizontal };
        spec.face_frac = 0.29;
        spec.piping = Some("#b9babc".into());
        spec.corners = Some("black".into());
        spec.handle = Handle::Leather;
        spec.handle_color = "#1e1c1b".into();
        let (plate, _grille, (pw, ph)) = build(b, root, &spec);
        // Age: grime specks and a few rust spots on the steel.
        let mut rng = 7_u64;
        let mut next = |m: f64| {
            rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((rng >> 20) as f64 % 10_000.0) / 10_000.0 * m
        };
        let mut specks = Vec::new();
        for _ in 0..320 {
            let (x, y) = (next(pw), next(ph));
            specks.push(vec![(x, y), (x + 0.8, y + 0.5)]);
        }
        let mut rust = Vec::new();
        for _ in 0..40 {
            let (x, y) = (next(pw), next(ph));
            rust.push(vec![(x, y), (x + 1.4, y + 0.9)]);
        }
        b.add(plate, draw::with_opacity(ui::strokes("age", &specks, 1.2, draw::solid("#3a3434")), 0.3));
        b.add(plate, draw::with_opacity(ui::strokes("rust", &rust, 2.2, draw::solid("#8a2a1c")), 0.45));
        let sy = ph / 132.0;
        let sx = pw / 1120.0;
        let (x, y) = (|v: f64| v * sx, |v: f64| v * sy);
        // The panel's legends: a quirky display face, hand-set.
        let legend = Lettering::family(family::PIRATA_ONE);
        let small = Lettering::family(family::INTER).weight(500.0);
        let ky = y(66.0);
        let ly = y(112.0);
        print(b, plate, "FET", x(48.0), y(24.0), 11.0, INK, 700.0, 0.02);
        jack(b, plate, x(78.0), y(26.0), 9.0);
        jack(b, plate, x(78.0), y(100.0), 9.0);
        let dots: &[&str] = &["", "", "", "", "", "", "", "", "", "", ""];
        let chrome = |b: &mut Builder<'_>, addr: &str, name: &str, cx: f64| {
            knob_scaled(b, plate, Cap::ChromeSkirt, addr, name, 52.0, x(cx), ky, dots, INK);
            set(b, plate, name, x(cx), ly, 15.0, INK, &legend, 0.04);
        };
        chrome(b, "clean/volume", "VOLUME", 129.0);
        for (addr, name, top, bottom, cx) in [("clean/bright", "Bright", "BRIGHT", "", 181.0), ("clean/deep", "Deep", "DEEP", "", 227.0), ("clean/voice", "Guitar", "GUITAR", "MIC", 273.0)] {
            set(b, plate, top, x(cx), y(18.0), 7.0, INK, &small, 0.02);
            screw(b, plate, x(cx), y(33.0), 3.2);
            slide(b, plate, addr, name, "red", 26.0, x(cx), ky);
            screw(b, plate, x(cx), y(100.0), 3.2);
            if !bottom.is_empty() {
                set(b, plate, bottom, x(cx), y(116.0), 7.0, INK, &small, 0.02);
            }
        }
        for (addr, name, cx) in [("clean/treble", "TREBLE", 327.0), ("clean/middle", "MIDDLE", 395.0), ("clean/bass", "BASS", 466.0), ("clean/high", "HIGH", 538.0), ("clean/low", "LOW", 609.0)] {
            chrome(b, addr, name, cx);
        }
        set(b, plate, "FILTER", x(573.0), y(14.0), 14.0, INK, &legend, 0.04);
        set(b, plate, "ON", x(663.0), y(20.0), 7.0, INK, &small, 0.02);
        screw(b, plate, x(663.0), y(34.0), 3.2);
        slide(b, plate, "clean/reverb_on", "Reverb on", "red", 26.0, x(663.0), ky);
        screw(b, plate, x(663.0), y(100.0), 3.2);
        set(b, plate, "OFF", x(663.0), y(116.0), 7.0, INK, &small, 0.02);
        set(b, plate, "REVERB", x(752.0), y(14.0), 14.0, INK, &legend, 0.04);
        chrome(b, "clean/send", "SEND", 716.0);
        chrome(b, "clean/return", "RETURN", 789.0);
        set(b, plate, "MID", x(839.0), y(20.0), 7.0, INK, &small, 0.02);
        screw(b, plate, x(839.0), y(34.0), 3.2);
        slide(b, plate, "clean/mid", "Mid", "red", 26.0, x(839.0), ky);
        screw(b, plate, x(839.0), y(100.0), 3.2);
        chrome(b, "clean/level", "LEVEL", 892.0);
        // The logo: heavy slab capitals, a little italic, inked on.
        let slab = Lettering::family(family::PLAYFAIR_DISPLAY).weight(900.0).italic();
        set(b, plate, "OPEN", x(1010.0), y(40.0), 25.0, "#0e0e0e", &slab, 0.0);
        set(b, plate, "ROOM", x(1010.0), y(72.0), 25.0, "#0e0e0e", &slab, 0.0);
        set(b, plate, "hand wired", x(1010.0), y(104.0), 17.0, "#1a1a1a", &legend, 0.08);
        jewel(b, plate, "clean/on", "red", 30.0, x(1088.0), y(70.0));
    });
    let ns = "clean";
    let params = vec![
        pct(ns, "volume", "Volume", 4.0).featured(),
        ui::toggle("clean/bright", "Bright", true),
        ui::toggle("clean/deep", "Deep", true),
        ui::choice("clean/voice", "Voicing", &["Mic", "Guitar"], 1),
        pct(ns, "treble", "Treble", 5.0).featured(),
        pct(ns, "middle", "Middle", 5.0).featured(),
        pct(ns, "bass", "Bass", 5.0).featured(),
        pct(ns, "high", "Filter high", 5.0),
        pct(ns, "low", "Filter low", 5.0),
        ui::toggle("clean/reverb_on", "Reverb", true),
        pct(ns, "send", "Reverb send", 5.0),
        pct(ns, "return", "Reverb return", 5.0),
        ui::toggle("clean/mid", "Mid boost", true),
        pct(ns, "level", "Level", 5.0).featured(),
        ui::toggle("clean/on", "Power", true),
    ];
    Face { file: "07-amp-head-clean-head", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The lead head.
pub fn lead_head() -> Face {
    const W: f64 = 1200.0;
    const H: f64 = 548.0;
    const INK: &str = "#ebe7df";
    let scene = canvas("Lead Head", |b, root| {
        let mut spec = CabinetSpec::new(Kind::Head, W, H);
        spec.covering = Material::Tolex { grain: Grain::Bronco, color: "#1b1a1a".into(), scale: 0.8 };
        spec.grille = Material::GrilleCloth { cloth: Cloth::Black };
        spec.face = Material::Plastic { color: "#161415".into(), gloss: 0.35 };
        spec.face_frac = 0.32;
        spec.piping = Some("#a8997c".into());
        spec.corners = Some("black".into());
        spec.handle = Handle::Leather;
        spec.handle_color = "#1c1b1c".into();
        let (plate, grille, (pw, ph)) = build(b, root, &spec);
        cloth(b, grille, "basket", &[]);
        let sy = ph / 152.0;
        let sx = pw / 1120.0;
        let (x, y) = (|v: f64| v * sx, |v: f64| v * sy);
        let sans = Lettering::family(family::INTER).weight(400.0);
        print(b, plate, "INPUT", x(71.0), y(40.0), 11.0, INK, 400.0, 0.0);
        jack(b, plate, x(71.0), y(80.0), 12.0);
        let toggles = [("lead/bright", "Bright", "BRIGHT", 138.0), ("lead/mid", "Mid", "MID", 218.0), ("lead/deep", "Deep", "DEEP", 299.0), ("lead/eq", "EQ", "EQ1", 379.0), ("lead/loop", "Loop bypass", "BYPASS", 540.0), ("lead/lead", "Lead", "LEAD", 622.0)];
        for (addr, name, legend, cx) in toggles {
            set(b, plate, legend, x(cx), y(12.0), 7.5, INK, &sans, 0.0);
            parts::toggle(b, plate, addr, name, 11.0, x(cx), y(32.0));
        }
        set(b, plate, "EQ2", x(379.0), y(50.0), 7.5, INK, &sans, 0.0);
        // Treble to presence: the reference's pointers sit near noon, a
        // little either side.
        let knobs = [("lead/treble", "TREBLE", 138.0), ("lead/middle", "MIDDLE", 218.0), ("lead/bass", "BASS", 299.0), ("lead/gain", "GAIN", 379.0), ("lead/master", "MASTER", 460.0), ("lead/lead_gain", "LEAD GAIN", 540.0), ("lead/lead_master", "LEAD MASTER", 622.0), ("lead/reverb", "REVERB", 701.0), ("lead/presence", "PRESENCE", 780.0)];
        for (addr, name, cx) in knobs {
            set(b, plate, name, x(cx), y(64.0), 12.0, INK, &sans, -0.01);
            knob(b, plate, Cap::ChickenHead, addr, name, 60.0, x(cx), y(110.0));
        }
        // The logo in a painted script, a model number under it in gold marker.
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        b.add(plate, with_shadow(draw::styled_text("logo", "Studio", x(840.0), y(60.0) - 24.0, 180.0, 34.0, "#f4f1ea", document_proto::TextAlign::Center, &script), "#000000a0"));
        parts::marker(b, plate, "Lead", x(955.0), y(100.0), 17.0, "#d9b76a");
        set(b, plate, "BY THE SHOP", x(880.0), y(104.0), 7.0, INK, &Lettering::family(family::INTER).weight(700.0), 0.04);
        print(b, plate, "ON", x(1028.0), y(38.0), 10.0, INK, 400.0, 0.0);
        parts::toggle(b, plate, "lead/on", "Standby", 15.0, x(1028.0), y(76.0));
        print(b, plate, "STANDBY", x(1028.0), y(128.0), 10.0, INK, 400.0, 0.0);
        // The pilot light, its filament wavering a little.
        b.add(plate, draw::circle("pilot bezel", x(1085.0), y(76.0), 10.0, draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#060607")])));
        let lens = draw::circle("pilot", x(1085.0), y(76.0), 7.0, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#ffe0e0"), (0.35, "#ff4a4a"), (1.0, "#8a0a0a")]));
        let lens = b.add(plate, draw::emissive(lens, draw::hex("#ff2a2a"), 0.9, 18.0));
        surface_kit::motion::add(b, lens, surface_kit::motion::flicker(0.06));
    });
    let ns = "lead";
    let params = vec![
        ui::toggle("lead/bright", "Bright", false),
        ui::toggle("lead/mid", "Mid", false),
        ui::toggle("lead/deep", "Deep", false),
        ui::choice("lead/eq", "EQ", &["EQ2", "EQ1"], 1),
        ui::toggle("lead/loop", "Loop bypass", false),
        ui::toggle("lead/lead", "Lead channel", false),
        pct(ns, "treble", "Treble", 5.5).featured(),
        pct(ns, "middle", "Middle", 5.0).featured(),
        pct(ns, "bass", "Bass", 5.0).featured(),
        pct(ns, "gain", "Gain", 5.0).featured(),
        pct(ns, "master", "Master", 5.0).featured(),
        pct(ns, "lead_gain", "Lead gain", 5.0),
        pct(ns, "lead_master", "Lead master", 4.5),
        pct(ns, "reverb", "Reverb", 5.5),
        pct(ns, "presence", "Presence", 4.5),
        ui::toggle("lead/on", "Standby", true),
    ];
    Face { file: "08-amp-head-lead-head", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The two-channel combo.
pub fn silk_combo() -> Face {
    const W: f64 = 1000.0;
    const H: f64 = 753.0;
    const INK: &str = "#f1ece4";
    let scene = canvas("Silk Combo", |b, root| {
        let mut spec = CabinetSpec::new(Kind::Combo, W, H);
        spec.covering = Material::Tolex { grain: Grain::Levant, color: "#4c2c20".into(), scale: 1.0 };
        spec.grille = Material::GrilleCloth { cloth: Cloth::Wheat };
        spec.face = Material::Plastic { color: "#141313".into(), gloss: 0.3 };
        spec.face_frac = 0.135;
        spec.piping = Some("#d8cbb2".into());
        spec.corners = None;
        spec.handle = Handle::Strap;
        spec.handle_color = "#2a1a14".into();
        let (plate, grille, (pw, ph)) = build(b, root, &spec);
        cloth(b, grille, "wheat", &[]);
        // The silver frame round the black plate.
        b.add(plate, draw::stroked(draw::rounded(draw::rect("frame", 3.0, 3.0, pw - 6.0, ph - 6.0, draw::solid("#00000000")), 2.0), "#cfc9be", 1.4));
        let sy = ph / 76.0;
        let sx = pw / 952.0;
        let (x, y) = (|v: f64| v * sx, |v: f64| v * sy);
        let sans = Lettering::family(family::INTER).weight(700.0);
        let ky = y(34.0);
        let ly = y(64.0);
        let numbered = |b: &mut Builder<'_>, addr: &str, name: &str, cx: f64| {
            knob_scaled(b, plate, Cap::BlackSkirt, addr, name, 42.0, x(cx), ky, &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"], INK);
            set(b, plate, name, x(cx), ly, 7.0, INK, &sans, 0.02);
        };
        // Normal channel.
        for (label, cx) in [("1", 36.0), ("2", 71.0)] {
            set(b, plate, label, x(cx), y(11.0), 8.0, INK, &sans, 0.0);
            jack(b, plate, x(cx), y(32.0), 9.0);
        }
        set(b, plate, "NORMAL", x(54.0), y(56.0), 11.0, INK, &sans, 0.02);
        set(b, plate, "BRIGHT", x(100.0), y(9.0), 5.5, INK, &sans, 0.0);
        slide(b, plate, "combo/normal_bright", "Normal bright", "black", 20.0, x(100.0), y(32.0));
        for (addr, name, cx) in [("combo/normal_volume", "VOLUME", 147.0), ("combo/normal_treble", "TREBLE", 195.0), ("combo/normal_bass", "BASS", 243.0)] {
            numbered(b, addr, name, cx);
        }
        b.add(plate, draw::rect("divider", x(273.0), 6.0, 1.2, ph - 12.0, draw::solid("#cfc9be")));
        // Vibrato channel.
        for (label, cx) in [("1", 304.0), ("2", 340.0)] {
            set(b, plate, label, x(cx), y(11.0), 8.0, INK, &sans, 0.0);
            jack(b, plate, x(cx), y(32.0), 9.0);
        }
        set(b, plate, "VIBRATO", x(322.0), y(56.0), 11.0, INK, &sans, 0.02);
        set(b, plate, "BRIGHT", x(366.0), y(9.0), 5.5, INK, &sans, 0.0);
        slide(b, plate, "combo/bright", "Bright", "black", 20.0, x(366.0), y(32.0));
        for (addr, name, cx) in [("combo/volume", "VOLUME", 412.0), ("combo/treble", "TREBLE", 458.0), ("combo/bass", "BASS", 504.0), ("combo/reverb", "REVERB", 550.0), ("combo/speed", "SPEED", 596.0), ("combo/intensity", "INTENSITY", 642.0)] {
            numbered(b, addr, name, cx);
        }
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        b.add(plate, with_shadow(draw::styled_text("logo", "Silk", x(690.0), y(4.0), 140.0, 28.0, "#f2dcd6", document_proto::TextAlign::Center, &script), "#00000080"));
        b.add(plate, with_shadow(draw::styled_text("logo", "Tone", x(735.0), y(30.0), 140.0, 28.0, "#f2dcd6", document_proto::TextAlign::Center, &script), "#00000080"));
        set(b, plate, "TWO CHANNEL", x(840.0), y(62.0), 5.5, INK, &sans, 0.06);
        jewel(b, plate, "combo/on", "red", 24.0, x(912.0), y(30.0));
    });
    let ns = "combo";
    let params = vec![
        ui::toggle("combo/normal_bright", "Normal bright", false),
        pct(ns, "normal_volume", "Normal volume", 5.0),
        pct(ns, "normal_treble", "Normal treble", 5.0),
        pct(ns, "normal_bass", "Normal bass", 5.0),
        ui::toggle("combo/bright", "Bright", true),
        pct(ns, "volume", "Volume", 5.0).featured(),
        pct(ns, "treble", "Treble", 5.0).featured(),
        pct(ns, "bass", "Bass", 5.0).featured(),
        pct(ns, "reverb", "Reverb", 5.0),
        pct(ns, "speed", "Vibrato speed", 5.0),
        pct(ns, "intensity", "Vibrato intensity", 5.0),
        ui::toggle("combo/on", "Power", true),
    ];
    Face { file: "09-amp-combo-silk-combo", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
