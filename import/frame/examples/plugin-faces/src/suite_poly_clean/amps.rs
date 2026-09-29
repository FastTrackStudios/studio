//! The three heads, in the Head form at the plugin's faceplate aspect:
//! 1200 × 443 design px (the head's front, tolex border included).
//!
//! - `04-amp-flame-lead-head` — pink tolex round violet hammered glass lit
//!   by its tubes; a pale marble faceplate with seven pink-capped knobs.
//! - `05-amp-cherub-rhythm-head` — all white: a plaster relief (shield,
//!   scrolls, a cherub at each end), a channel lever among six white knobs.
//! - `06-amp-bloom-acoustic-head` — all black: a floral relief (roses,
//!   blossoms, wheat), seven black knurled knobs with a blend control.
//!
//! Each faceplate reads left to right: the input jack with its cable, the
//! knobs under tracked capitals and inside printed tick rings, the power
//! button and its white pilot bar, the maker's scribbled mark.

use document_proto::{CornerRadii, NodeId};
use surface_kit::draw;
use surface_proto::{ButtonMode, Control, ControlKind, ParamInfo, SlotRole};
use surface_widgets::Builder;

use super::parts::{self, knob_with, switch};
use super::{PLUGIN, art};
use crate::face::{Face, canvas};
use crate::reverb::ui;

pub const W: f64 = 1200.0;
pub const H: f64 = 443.0;
/// The opening inside the tolex border.
const IX: f64 = 33.0;
const IY: f64 = 34.0;
const IW: f64 = W - 2.0 * IX;
/// The art's height above the faceplate, and the faceplate's.
const ART_H: f64 = 259.0;
const PLATE_H: f64 = 116.0;

struct Look {
    tolex: [&'static str; 3],
    plate: [&'static str; 2],
    ink: &'static str,
    knob: &'static str,
    knob_d: f64,
    cable: &'static str,
}

enum Art {
    Glass,
    Relief,
    Floral,
}

/// A faceplate control: a knob, or the channel lever.
enum Ctl {
    Knob(&'static str, &'static str, f64),
    Channel(&'static str, f64),
}

fn head(b: &mut Builder<'_>, root: NodeId, ns: &str, look: &Look, art_kind: &Art, ctls: &[Ctl]) {
    ui::fit_root(b, root, W, H);
    // Tolex: a padded box, lighter along its top.
    let mut tolex = draw::frame("Tolex", 0.0, 0.0, W, H);
    tolex.fills = vec![draw::linear(180.0, &[(0.0, look.tolex[0]), (0.5, look.tolex[1]), (1.0, look.tolex[2])])];
    tolex.corner_radii = CornerRadii::uniform(26.0);
    tolex.effects = vec![draw::inner_shadow("#ffffff3a", 0.0, 4.0, 6.0), draw::inner_shadow("#00000059", 0.0, -6.0, 10.0), draw::drop_shadow("#000000a0", 0.0, 8.0, 18.0)];
    let tolex = b.add(root, tolex);
    // A grain of the covering: faint diagonal hatching.
    let mut grain = String::new();
    let mut x = -H;
    while x < W {
        grain.push_str(&format!("M {x:.1} {H:.1} L {:.1} 0 ", x + H));
        x += 5.0;
    }
    let mut g = draw::path("grain", 0.0, 0.0, W, H, grain, draw::solid("#00000000"));
    g = draw::stroked(g, "#0000000d", 0.6);
    b.add(tolex, g);
    // The opening's lip.
    let lip = draw::rounded(draw::rect("opening lip", IX - 3.0, IY - 3.0, IW + 6.0, ART_H + PLATE_H + 6.0, draw::solid("#00000040")), 7.0);
    b.add(tolex, draw::with_effects(lip, vec![draw::inner_shadow("#00000080", 0.0, 2.0, 3.0)]));

    let mut art_frame = draw::frame("Front art", IX, IY, IW, ART_H);
    art_frame.clips_content = true;
    art_frame.corner_radii = CornerRadii { top_left: 5.0, top_right: 5.0, bottom_left: 0.0, bottom_right: 0.0 };
    let art_id = b.add(tolex, art_frame);
    match art_kind {
        Art::Glass => art::hammered_glass(b, art_id, IW, ART_H),
        Art::Relief => art::relief_white(b, art_id, IW, ART_H),
        Art::Floral => art::floral_black(b, art_id, IW, ART_H),
    }

    let mut plate = draw::frame("Faceplate", IX, IY + ART_H, IW, PLATE_H);
    plate.fills = vec![
        draw::linear(180.0, &[(0.0, look.plate[0]), (1.0, look.plate[1])]),
        draw::linear(100.0, &[(0.0, "#ffffff00"), (0.35, "#ffffff14"), (0.5, "#ffffff00"), (0.8, "#0000000a"), (1.0, "#ffffff00")]),
    ];
    plate.corner_radii = CornerRadii { top_left: 0.0, top_right: 0.0, bottom_left: 5.0, bottom_right: 5.0 };
    plate.effects = vec![draw::inner_shadow("#00000066", 0.0, 3.0, 4.0), draw::inner_shadow("#ffffff40", 0.0, -1.0, 1.0)];
    let plate = b.add(tolex, plate);

    // The input jack and its cable, curling out under the head.
    let (jx, jy) = (152.0, 71.0);
    let cable = [(jx, jy), (jx - 4.0, jy + 30.0), (jx - 22.0, PLATE_H + 20.0), (jx - 60.0, PLATE_H + 34.0)];
    b.add(plate, draw::with_effects(ui::line_node("cable", &cable, 8.0, look.cable), vec![draw::drop_shadow("#00000080", 1.0, 3.0, 3.0)]));
    b.add(plate, draw::path("jack nut", 0.0, 0.0, IW, PLATE_H, draw::regular(jx, jy, 15.0, 6, 0.0), draw::linear(150.0, &[(0.0, "#f4f4f2"), (0.5, "#9a9b99"), (1.0, "#e0e0de")])));
    b.add(plate, draw::circle("plug", jx, jy, 9.5, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff"), (1.0, "#6e6f71")])));
    b.add(plate, draw::circle("plug tip", jx, jy, 4.0, draw::solid(look.cable)));

    for c in ctls {
        match c {
            Ctl::Knob(id, legend, x) => {
                parts::word(b, plate, legend, *x, 27.0, 7.5, look.ink, 700.0, 0.32);
                knob_with(b, plate, "knob.amp_cap", look.knob, &format!("{ns}/{id}"), legend, look.knob_d, *x, 71.0, &["ticks=11"], look.ink);
            }
            Ctl::Channel(id, x) => {
                parts::word(b, plate, "CH1", *x, 50.0, 6.0, look.ink, 700.0, 0.12);
                switch(b, plate, "switch.mini_lever", "chrome", &format!("{ns}/{id}"), "Channel", 14.0, *x, 71.0, &["vertical"], &["bushing"]);
                parts::word(b, plate, "CH2", *x, 93.0, 6.0, look.ink, 700.0, 0.12);
            }
        }
    }
    power(b, plate, &format!("{ns}/power"), 887.0, 71.0, look.cable);
    art::flourish(b, plate, 995.0, 71.0, 100.0, look.ink);
}

/// The power button and its pilot: a small dark push-button, and a white
/// bar that lights while the amp is on.
fn power(b: &mut Builder<'_>, parent: NodeId, addr: &str, cx: f64, cy: f64, dark: &str) {
    let root = b.add(parent, draw::frame("Power", cx - 22.0, cy - 22.0, 100.0, 44.0));
    b.add(root, draw::with_effects(draw::circle("button ring", 22.0, 22.0, 9.5, draw::solid("#00000055")), vec![draw::drop_shadow("#ffffff30", 0.0, 1.0, 0.5)]));
    let btn = draw::circle("button", 22.0, 22.0, 8.0, draw::radial_at(0.38, 0.3, 0.9, &[(0.0, "#6a6a6e"), (0.6, dark), (1.0, "#000000")]));
    b.add(root, draw::with_effects(btn, vec![draw::drop_shadow("#00000099", 0.0, 1.5, 2.0)]));
    b.add(root, draw::rounded(draw::rect("pilot slot", 70.0, 19.0, 24.0, 6.0, draw::solid("#00000040")), 3.0));
    let bar = draw::rounded(draw::rect("pilot", 70.0, 19.5, 24.0, 5.0, draw::linear(180.0, &[(0.0, "#ffffff"), (1.0, "#e4e8ff")])), 2.5);
    let bar = draw::with_effects(bar, vec![draw::drop_shadow("#ffffffcc", 0.0, 0.0, 5.0)]);
    let bar = b.add(root, draw::emissive(bar, draw::hex("#f4f6ff"), 0.5, 6.0));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, bar.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

fn dials(ns: &str, ids: &[(&str, &str, f64)]) -> Vec<ParamInfo> {
    let mut v: Vec<ParamInfo> = ids.iter().map(|(id, name, d)| parts::dial10(ns, id, name, *d)).collect();
    if let Some(first) = v.first_mut() {
        *first = first.clone().featured();
    }
    v
}

const XS: [f64; 7] = [266.0, 354.0, 442.0, 530.0, 617.0, 706.0, 793.0];

pub fn flame_lead() -> Face {
    const NS: &str = "lead";
    let look = Look { tolex: ["#f2a0b8", "#e98aa6", "#d8718f"], plate: ["#ebe7e9", "#d9d3d6"], ink: "#3a2a31", knob: "pink", knob_d: 38.0, cable: "#1e1e20" };
    let scene = canvas("Flame Lead (Head, 1200×443)", |b, root| {
        let names = [("gain", "GAIN"), ("bass", "BASS"), ("middle", "MIDDLE"), ("treble", "TREBLE"), ("presence", "PRESENCE"), ("master", "MASTER"), ("output", "OUTPUT")];
        let ctls: Vec<Ctl> = names.iter().zip(XS).map(|((id, l), x)| Ctl::Knob(id, l, x)).collect();
        head(b, root, NS, &look, &Art::Glass, &ctls);
    });
    let mut params = dials(NS, &[("gain", "Gain", 6.5), ("bass", "Bass", 5.0), ("middle", "Middle", 5.0), ("treble", "Treble", 5.5), ("presence", "Presence", 5.0), ("master", "Master", 5.0), ("output", "Output", 5.0)]);
    params.push(parts::on(NS, "power", "Power", true).featured());
    Face { file: "04-amp-flame-lead-head", plugin: PLUGIN, scene, params, streams: vec![] }
}

pub fn cherub_rhythm() -> Face {
    const NS: &str = "rhythm";
    let look = Look { tolex: ["#f7f7f5", "#ebebe9", "#d9d9d7"], plate: ["#eeeeec", "#e0e0de"], ink: "#454545", knob: "white", knob_d: 42.0, cable: "#e8e8e6" };
    let scene = canvas("Cherub Rhythm (Head, 1200×443)", |b, root| {
        let ctls = [
            Ctl::Knob("gain", "GAIN", 267.0),
            Ctl::Channel("channel", 338.0),
            Ctl::Knob("bass", "BASS", 411.0),
            Ctl::Knob("middle", "MIDDLE", 507.0),
            Ctl::Knob("treble", "TREBLE", 602.0),
            Ctl::Knob("presence", "PRESENCE", 699.0),
            Ctl::Knob("output", "OUTPUT", 794.0),
        ];
        head(b, root, NS, &look, &Art::Relief, &ctls);
    });
    let mut params = dials(NS, &[("gain", "Gain", 5.0), ("bass", "Bass", 5.0), ("middle", "Middle", 5.0), ("treble", "Treble", 5.0), ("presence", "Presence", 5.0), ("output", "Output", 5.0)]);
    params.push(parts::on(NS, "channel", "Channel 1", true));
    params.push(parts::on(NS, "power", "Power", true).featured());
    Face { file: "05-amp-cherub-rhythm-head", plugin: PLUGIN, scene, params, streams: vec![] }
}

pub fn bloom_acoustic() -> Face {
    const NS: &str = "acoustic";
    let look = Look { tolex: ["#2a2a2c", "#1d1d1f", "#121213"], plate: ["#232325", "#19191a"], ink: "#e9e9e9", knob: "black", knob_d: 38.0, cable: "#0c0c0d" };
    let scene = canvas("Bloom Acoustic (Head, 1200×443)", |b, root| {
        let names = [("gain", "GAIN"), ("bass", "BASS"), ("middle", "MIDDLE"), ("treble", "TREBLE"), ("presence", "PRESENCE"), ("blend", "BLEND"), ("output", "OUTPUT")];
        let ctls: Vec<Ctl> = names.iter().zip(XS).map(|((id, l), x)| Ctl::Knob(id, l, x)).collect();
        head(b, root, NS, &look, &Art::Floral, &ctls);
    });
    let mut params = dials(NS, &[("gain", "Gain", 4.0), ("bass", "Bass", 5.0), ("middle", "Middle", 5.0), ("treble", "Treble", 5.5), ("presence", "Presence", 5.0), ("blend", "Blend", 5.0), ("output", "Output", 5.0)]);
    params.push(parts::on(NS, "power", "Power", true).featured());
    Face { file: "06-amp-bloom-acoustic-head", plugin: PLUGIN, scene, params, streams: vec![] }
}
