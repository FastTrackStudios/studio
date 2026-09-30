//! The blues rig's drives: recreations of the four real pedals the suite's
//! drives are modelled on (docs/rig-sections.md) — their enclosures,
//! colours, knobs and lettering, under names of our own.
//!
//! - `blade` — the dual-FET clean boost: gloss black, a white hairline
//!   round the lid, its volume knob standing out of the right side.
//!   172 × 286.
//! - `pegasus` — the gold overdrive: three oxblood domes, serif capitals,
//!   landscape. 400 × 286.
//! - `british-blues` — the 1991 British overdrive: black crinkle, a ledge
//!   carrying three knobs with blue lines, the blue logo between rules, a
//!   script embossed in the lid. 222 × 286.
//! - `tube-drive` — the green overdrive whose switch is a ribbed rubber
//!   treadle. 172 × 286.

use document_proto::{CornerRadii, NodeId};
use surface_kit::Place;
use surface_kit::draw::{self, Lettering, family};
use surface_proto::{Binding, ButtonMode, Control, ControlKind, Range, SlotRole, Unit};
use surface_widgets::Builder;

use crate::comp_tube::ui as place;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_studio_blues::parts::{self, set};

use super::PLUGIN;

/// Every pedal's height (docs/form-factors.md).
pub const H: f64 = 286.0;

/// The rendered enclosure, `w` wide, in `color` and `finish` (plus
/// `extra` options), sized into `root`. Returns its lid (the parent for
/// controls and print) and the lid's size.
fn enclosure(b: &mut Builder<'_>, root: NodeId, w: f64, color: &str, finish: &str, extra: &[&str]) -> (NodeId, f64, f64) {
    ui::fit_root(b, root, w, H);
    let mut opts = vec![format!("w={w}"), format!("h={H}"), format!("color={color}"), format!("finish={finish}")];
    opts.extend(extra.iter().map(ToString::to_string));
    let p = Place { options: Some(opts), ..Place::new("Enclosure").variant("standard") };
    let enc = ui::put_at(b, root, "panel.pedal_3d", &p, 0.0, 0.0);
    let lid = b.scene.node(enc).and_then(|n| n.children.iter().copied().find(|c| b.scene.node(*c).is_some_and(|c| c.name == "lid"))).expect("pedal_3d has a lid");
    let (lw, lh) = b.scene.node(lid).map_or((w, H), |n| (n.size.width, n.size.height));
    (lid, lw, lh)
}

/// A rendered knob: preset `variant` with `options` over it, `d` across,
/// its shaft at (`cx`, `cy`), bound to `addr`. The face letters its own.
#[allow(clippy::too_many_arguments)]
fn knob(b: &mut Builder<'_>, parent: NodeId, variant: &str, options: &[&str], addr: &str, name: &str, d: f64, cx: f64, cy: f64) -> NodeId {
    let mut opts = vec!["touch=44", "ticks=none"];
    opts.extend_from_slice(options);
    let p = Place::new(name).legend(" ").size(d).variant(variant).options(&opts).bind(ui::bind(addr));
    place::centred_at(b, parent, "knob.3d", &p, cx, cy)
}

fn inter(weight: f64) -> Lettering {
    Lettering::family(family::INTER).weight(weight)
}

fn on(ns: &str) -> surface_proto::ParamInfo {
    ui::toggle(&format!("{ns}/on"), "Bypass", true).featured()
}

/// The footswitch alone (its LED is where the pedal puts it: [`led`]),
/// `d` across the nut, at (`cx`, `cy`).
fn stomp(b: &mut Builder<'_>, parent: NodeId, ns: &str, d: f64, cx: f64, cy: f64) -> NodeId {
    let p = Place::new("Bypass").size(d).variant("none").legend("").bind(Binding::param(format!("{ns}/on"))).options(&["touch=44"]);
    place::centred_at(b, parent, "switch.stomp", &p, cx, cy)
}

/// The status LED at (`cx`, `cy`), `d` across, lit while `ns/on` is.
fn led(b: &mut Builder<'_>, parent: NodeId, ns: &str, cx: f64, cy: f64, d: f64, color: &str) -> NodeId {
    let r = d / 2.0;
    let f = b.add(parent, draw::frame("LED", cx - d, cy - d, 2.0 * d, 2.0 * d));
    b.add(f, draw::with_effects(draw::circle("bezel", d, d, r + (d * 0.16).max(1.2), draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    let off = b.add(f, draw::circle("lens", d, d, r, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#ffffff30"), (1.0, "#000000d0")])));
    let lit = draw::circle("lit", d, d, r, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#ffffff"), (0.35, color), (1.0, color)]));
    let lit = b.add(f, draw::emissive(lit, draw::hex(color), 0.9, d * 2.5));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Off, off.0).slot(SlotRole::On, lit.0);
    c.binding = Some(Binding::param(format!("{ns}/on")));
    b.controls.insert(f, c);
    f
}

/// The dual-FET clean boost. Gloss black, a white hairline round the lid,
/// the blue LED at its head, the volume knob out of the right side with a
/// scale printed beside it, the maker's square mark at the foot.
#[must_use]
pub fn blade() -> Face {
    const W: f64 = 172.0;
    const NS: &str = "blade";
    const INK: &str = "#e9e9ec";
    let scene = canvas("Blade", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#111113", "gloss", &["jack_inset=10", "radius=14"]);
        b.add(lid, draw::stroked(draw::rounded(draw::rect("border", 6.0, 16.0, lw - 12.0, lh - 26.0, draw::solid("#00000000")), 9.0), INK, 1.4));
        parts::print_left(b, lid, "9VDC ⊖·⊕", 12.0, 9.0, 5.5, INK, 600.0, 0.04);
        led(b, lid, NS, lw * 0.52, 10.0, 5.0, "#3d6bff");
        // The scale beside the side knob.
        let ky = lh * 0.26;
        let ticks: Vec<Vec<(f64, f64)>> = (0..10)
            .map(|i| {
                let y = ky - 24.0 + f64::from(i) * 5.4;
                let len = if i % 3 == 0 { 11.0 } else { 7.0 };
                vec![(lw - 40.0 - len, y), (lw - 40.0, y)]
            })
            .collect();
        b.add(lid, ui::strokes("scale", &ticks, 0.9, draw::solid(INK)));
        parts::print(b, lid, "PULL: BOOST", lw - 46.0, ky + 34.0, 4.6, INK, 600.0, 0.04);
        set(b, lid, "BLADE", lw * 0.5, lh * 0.5, 25.0, "#f4f4f6", &inter(500.0).width(92.0), 0.04);
        parts::print(b, lid, "DUAL FET CLASS A CLEAN BOOST", lw * 0.5, lh * 0.5 + 18.0, 5.4, "#bdbdc2", 500.0, 0.06);
        stomp(b, lid, NS, 34.0, lw * 0.5, lh * 0.71);
        let (mx, my, ms) = (lw * 0.5 - 13.0, lh * 0.8, 26.0);
        b.add(lid, draw::stroked(draw::rounded(draw::rect("mark", mx, my, ms, ms, draw::solid("#00000000")), 3.0), INK, 1.6));
        set(b, lid, "B", lw * 0.5, my + ms * 0.52, 17.0, INK, &inter(800.0).italic(), 0.0);
        parts::print(b, lid, "HAND BUILT · CLEAN FROM THE FIRST FET", lw * 0.5, lh - 14.0, 4.2, "#9d9da3", 500.0, 0.05);
        // The volume knob stands out of the right side, over the jack.
        knob(b, root, "knurled", &["color=#161618", "ribs=40", "index=none", "skirt=none"], &format!("{NS}/volume"), "Volume", 46.0, W - 14.0, 10.0 + ky);
    });
    let params = vec![
        ui::param(&format!("{NS}/volume"), "Volume", Range::linear(0.0, 23.0, Unit::Db), 11.0).featured(),
        on(NS),
    ];
    Face { file: "11-drive-blade", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The gold overdrive. Landscape, gold with the paint worn soft, three
/// oxblood domes (the gain's with a pointer), spaced serif capitals, the
/// LED low by the gain, the name split round a small chrome stomp.
#[must_use]
pub fn pegasus() -> Face {
    const W: f64 = 400.0;
    const NS: &str = "pegasus";
    const INK: &str = "#2b1c08";
    let serif = Lettering::family(family::PLAYFAIR_DISPLAY).weight(700.0);
    let scene = canvas("Pegasus", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#c99038", "satin", &["jack_inset=8"]);
        b.add(lid, draw::rect("sheen", 0.0, 0.0, lw, lh, draw::linear(115.0, &[(0.0, "#ffe2a02a"), (0.4, "#ffffff00"), (0.75, "#4a2c0620"), (1.0, "#ffe2a014")])));
        for (i, (id, name)) in [("gain", "GAIN"), ("treble", "TREBLE"), ("output", "OUTPUT")].iter().enumerate() {
            let cx = lw * (0.2 + 0.3 * i as f64);
            let pointer = if i == 0 { "index=full" } else { "index=none" };
            knob(b, lid, "domed", &["color=#3a0f0c", "pointer=#3a0f0c", "skirt=plain", "skirt_color=#2a0a08", "dome=0.35", "coat=0.8", pointer], &format!("{NS}/{id}"), name, 66.0, cx, lh * 0.2);
            set(b, lid, name, cx, lh * 0.4, 11.0, INK, &serif, 0.12);
        }
        led(b, lid, NS, lw * 0.07, lh * 0.38, 6.0, "#d0261a");
        stomp(b, lid, NS, 30.0, lw * 0.66, lh * 0.72);
        set(b, lid, "ORO", lw * 0.2, lh * 0.73, 12.0, INK, &serif, 0.14);
        set(b, lid, "PEGASUS", lw * 0.84, lh * 0.73, 12.0, INK, &serif, 0.14);
    });
    let params = vec![
        ui::percent(&format!("{NS}/gain"), "Gain", 30.0).featured(),
        ui::percent(&format!("{NS}/treble"), "Treble", 50.0).featured(),
        ui::percent(&format!("{NS}/output"), "Output", 60.0).featured(),
        on(NS),
    ];
    Face { file: "12-drive-pegasus", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The 1991 British overdrive. Black crinkle; the knobs stand on a raised
/// ledge across the top, black with blue lines; the logo in blue italic
/// between rules; a red LED over the stomp; the maker's script embossed in
/// the lid below.
#[must_use]
pub fn british_blues() -> Face {
    const W: f64 = 222.0;
    const NS: &str = "british";
    const BLUE: &str = "#5cc4f0";
    let scene = canvas("British Blues", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, "#1c1c1e", "hammertone", &["jack_inset=10"]);
        let ledge_h = lh * 0.2;
        let ledge = draw::rect("ledge", 0.0, 0.0, lw, ledge_h, draw::linear(180.0, &[(0.0, "#2c2c2f"), (0.8, "#1a1a1c"), (1.0, "#0a0a0b")]));
        b.add(lid, draw::with_effects(ledge, vec![draw::drop_shadow("#000000d0", 0.0, 3.0, 5.0)]));
        b.add(lid, draw::rect("ledge lip", 0.0, ledge_h - 1.0, lw, 1.0, draw::solid("#48484c")));
        for (i, (id, name)) in [("gain", "Gain"), ("tone", "Tone"), ("volume", "Volume")].iter().enumerate() {
            let cx = lw * (0.22 + 0.28 * i as f64);
            knob(b, lid, "knurled", &["color=#121214", &format!("pointer={BLUE}"), "index=line", "index_w=0.14", "ribs=36"], &format!("{NS}/{id}"), name, 40.0, cx, ledge_h * 0.5);
            parts::print(b, lid, name, cx, ledge_h + 10.0, 7.5, "#e2e2e6", 500.0, 0.02);
        }
        let logo = Lettering::family(family::OSWALD).weight(700.0).italic();
        let ly = lh * 0.4;
        b.add(lid, draw::rect("rule", 12.0, ly - 16.0, lw - 24.0, 1.6, draw::solid(BLUE)));
        set(b, lid, "BRITISH BLUES", lw * 0.5, ly, 20.0, BLUE, &logo, 0.0);
        b.add(lid, draw::rect("rule", 12.0, ly + 12.0, lw - 24.0, 1.6, draw::solid(BLUE)));
        led(b, lid, NS, lw * 0.5, lh * 0.5, 5.0, "#e0281c");
        stomp(b, lid, NS, 34.0, lw * 0.5, lh * 0.6);
        // The script, embossed: a lit edge above, a shadow below.
        let script = Lettering::family(family::DANCING_SCRIPT).weight(700.0);
        let sy = lh * 0.82;
        set(b, lid, "Kingsway", lw * 0.5 - 0.8, sy - 0.9, 38.0, "#46464b", &script, 0.0);
        set(b, lid, "Kingsway", lw * 0.5 + 0.8, sy + 0.9, 38.0, "#050506", &script, 0.0);
        set(b, lid, "Kingsway", lw * 0.5, sy, 38.0, "#1e1e21", &script, 0.0);
    });
    let params = vec![
        ui::percent(&format!("{NS}/gain"), "Gain", 40.0).featured(),
        ui::percent(&format!("{NS}/tone"), "Tone", 50.0).featured(),
        ui::percent(&format!("{NS}/volume"), "Volume", 55.0).featured(),
        on(NS),
    ];
    Face { file: "13-drive-british-blues", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The green overdrive. Three black knobs with broad mint pointers and
/// scales, the grey name band with its model box, and a black rubber
/// treadle ribbed in chevrons — the whole treadle is the switch.
#[must_use]
pub fn tube_drive() -> Face {
    const W: f64 = 172.0;
    const NS: &str = "tubedrive";
    const GREEN: &str = "#57c23f";
    const DARK: &str = "#0d2a08";
    let scene = canvas("Tube Drive", |b, root| {
        let (lid, lw, lh) = enclosure(b, root, W, GREEN, "gloss", &["jack_inset=6", "front=0", "radius=10"]);
        parts::print(b, lid, "▲OUT", lw * 0.14, 6.0, 5.0, DARK, 700.0, 0.02);
        parts::print(b, lid, "▼DC", lw * 0.66, 6.0, 5.0, DARK, 700.0, 0.02);
        parts::print(b, lid, "▼IN", lw * 0.86, 6.0, 5.0, DARK, 700.0, 0.02);
        led(b, lid, NS, lw * 0.5, 7.0, 5.0, "#e01818");
        for (i, (id, name)) in [("drive", "DRIVE"), ("tone", "TONE"), ("level", "LEVEL")].iter().enumerate() {
            let cx = lw * (0.19 + 0.31 * i as f64);
            let cy = lh * 0.2;
            knob(b, lid, "flat", &["color=#101012", "pointer=#7ee8b4", "index=full", "index_w=0.42", "index_in=0", "index_out=0.95", "skirt=plain"], &format!("{NS}/{id}"), name, 42.0, cx, cy);
            parts::print(b, lid, name, cx, cy - 34.0, 5.5, DARK, 700.0, 0.04);
            let (lo, hi) = if *id == "tone" { ("−", "+") } else { ("0", "10") };
            parts::print(b, lid, lo, cx - 16.0, cy + 27.0, 5.5, DARK, 700.0, 0.0);
            parts::print(b, lid, hi, cx + 16.0, cy + 27.0, 5.5, DARK, 700.0, 0.0);
        }
        // The name band.
        let (by, bh) = (lh * 0.33, lh * 0.12);
        b.add(lid, draw::rect("band", 0.0, by, lw, bh, draw::linear(180.0, &[(0.0, "#46494c"), (1.0, "#2a2c2f")])));
        set(b, lid, "TUBE DRIVE", lw * 0.56, by + bh * 0.32, 15.0, "#f1f1ef", &inter(700.0).width(88.0), 0.0);
        set(b, lid, "CLASSIC", lw * 0.74, by + bh * 0.76, 9.0, "#8fe07a", &inter(800.0), 0.04);
        set(b, lid, "TD10", lw * 0.1, by + bh * 0.76, 7.5, "#f1f1ef", &inter(800.0), 0.0);
        for i in 0..5 {
            b.add(lid, draw::rect("stripe", 2.0, by + bh + 6.0 + f64::from(i) * 5.0, lw * 0.07, 1.4, draw::solid("#e6ebe4")));
        }
        // The treadle: the whole of it is the switch.
        let (tx, ty) = (lw * 0.12, by + bh + 4.0);
        let (tw, th) = (lw * 0.84, lh - ty - 6.0);
        let mut tread = draw::frame("treadle", tx, ty, tw, th);
        tread.fills = vec![draw::linear(180.0, &[(0.0, "#2c2c2e"), (0.08, "#18181a"), (1.0, "#0c0c0d")])];
        tread.corner_radii = CornerRadii::uniform(6.0);
        tread.clips_content = true;
        tread.effects = vec![draw::drop_shadow("#000000c0", 0.0, 3.0, 6.0)];
        let tread = b.add(lid, tread);
        let mut lit = Vec::new();
        let mut dark = Vec::new();
        for r in 0..8 {
            let y = th * 0.08 + f64::from(r) * th * 0.1;
            for k in 0..3 {
                let (x0, x1) = (tw * (0.06 + 0.3 * f64::from(k)), tw * (0.3 + 0.3 * f64::from(k)));
                let xm = (x0 + x1) / 2.0;
                dark.push(vec![(x0, y + 5.0), (xm, y), (x1, y + 5.0)]);
                lit.push(vec![(x0, y + 6.4), (xm, y + 1.4), (x1, y + 6.4)]);
            }
        }
        b.add(tread, ui::strokes("rib shadow", &dark, 2.4, draw::solid("#050506")));
        b.add(tread, ui::strokes("rib light", &lit, 1.0, draw::solid("#3a3a3e")));
        let brand = Lettering::family(family::DANCING_SCRIPT).weight(700.0).italic();
        set(b, tread, "Kenzai", tw * 0.5, th * 0.9, 17.0, "#303034", &brand, 0.0);
        b.controls.insert(tread, Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).bound(Binding::param(format!("{NS}/on"))));
    });
    let params = vec![
        ui::percent(&format!("{NS}/drive"), "Drive", 45.0).featured(),
        ui::percent(&format!("{NS}/tone"), "Tone", 50.0).featured(),
        ui::percent(&format!("{NS}/level"), "Level", 55.0).featured(),
        on(NS),
    ];
    Face { file: "14-drive-tube-drive", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The four, in chain order.
#[must_use]
pub fn all() -> Vec<Face> {
    vec![blade(), pegasus(), british_blues(), tube_drive()]
}
