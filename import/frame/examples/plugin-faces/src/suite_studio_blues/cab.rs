//! The cab page: a 1×12 open-back cab on the studio floor with its two
//! mics in front of the speaker — a ribbon on the left, a dynamic on the
//! right — each mic's panel beside it (model, on, position, distance,
//! level, pan), the room size over the cab and the room send and phase
//! under it. Cab, 900 × 900.

use document_proto::{CornerRadii, NodeId, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Binding, Range, Unit};
use surface_widgets::Builder;

use super::parts::{PLUGIN, print};
use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial};

const W: f64 = 900.0;
const H: f64 = 900.0;
const INK: &str = "#eceae6";
const DIM: &str = "#a9a5a0";

const MICS: &[&str] = &[
    "Dynamic 57",
    "Dynamic 57 Off-Axis",
    "Dynamic 409",
    "Dynamic 421",
    "Condenser 184",
    "Condenser 414",
    "Condenser 4006",
    "Condenser U67",
    "Ribbon 121",
    "Ribbon 121 Horizontal",
    "Ribbon 160",
];

/// A mic's panel in a column centred on `cx`.
fn mic_panel(b: &mut Builder<'_>, parent: NodeId, n: u8, cx: f64) {
    let ns = format!("cab/m{n}");
    let mut card = draw::frame(&format!("Mic {n}"), cx - 84.0, 170.0, 168.0, 430.0);
    card.fills = vec![draw::linear(180.0, &[(0.0, "#141416e6"), (1.0, "#0c0c0ee6")])];
    card.corner_radii = CornerRadii::uniform(12.0);
    card.stroke = document_proto::Stroke { paints: vec![draw::solid("#ffffff14")], weight: 1.0, ..document_proto::Stroke::default() };
    let card = b.add(parent, card);
    let c = 84.0;
    print(b, card, &format!("MIC {n}"), c, 24.0, 12.0, INK, 700.0, 0.1);
    // The model: a readout of its name, a stepped selector under it.
    ui::readout(b, card, "Model", Binding::param(format!("{ns}_model")), c, 58.0, 150.0, 30.0, INK);
    let p = Place::new("Model select").size(38.0).variant("grey").bind(Binding::param(format!("{ns}_model"))).legend("").scale(&[]).touch(TOUCH_MIN);
    let k = ui::put(b, card, "knob.soft_pointer", &p, 46.0, 120.0);
    ui::strip_legend(b, k);
    print(b, card, "MODEL", 46.0, 156.0, 9.0, DIM, 700.0, 0.1);
    let on = Place::new("Mic on").size(30.0).variant("white").bind(Binding::param(format!("{ns}_on"))).legend("").touch(TOUCH_MIN);
    let o = ui::put(b, card, "button.square", &on, 122.0, 120.0);
    ui::strip_legend(b, o);
    print(b, card, "ON", 122.0, 156.0, 9.0, DIM, 700.0, 0.1);
    let dial = Dial::new("knob.soft_pointer", "grey", 52.0, INK, DIM).scale(&["", "", "", "", "", "", "", "", "", "", ""]).legend(9.0);
    ui::dial(b, card, &format!("{ns}_position"), "POSITION", 46.0, 222.0, &dial);
    ui::dial(b, card, &format!("{ns}_distance"), "DISTANCE", 122.0, 222.0, &dial);
    ui::dial(b, card, &format!("{ns}_level"), "MIC LEVEL", 46.0, 342.0, &dial);
    ui::dial(b, card, &format!("{ns}_pan"), "PAN", 122.0, 342.0, &dial);
}

/// The cab: tolex, the grille, the speaker's shadow behind it.
fn cabinet(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, w: f64, h: f64) {
    b.add(parent, draw::circle("floor shadow", x + w / 2.0, y + h + 6.0, w * 0.55, draw::radial(&[(0.0, "#000000a0"), (1.0, "#00000000")])));
    let p = Place { options: Some(vec![format!("w={w}"), format!("h={h}"), "r=14".into()]), ..Place::new("Cabinet").variant("black") };
    let cab = ui::put_at(b, parent, "panel.amp_cabinet", &p, x, y);
    // Side posts, as the cab's front frame shows them.
    for px in [18.0, w - 58.0] {
        b.add(cab, draw::rounded(draw::rect("post", px, 14.0, 40.0, h - 28.0, draw::linear(90.0, &[(0.0, "#1a191b"), (0.5, "#2c2a2e"), (1.0, "#141315")])), 4.0));
    }
    let g = Place { options: Some(vec![format!("w={}", w - 150.0), format!("h={}", h - 60.0)]), ..Place::new("Grille").variant("basket") };
    let grille = ui::put_at(b, cab, "panel.amp_grille", &g, 75.0, 30.0);
    let (gw, gh) = (w - 150.0, h - 60.0);
    b.add(grille, draw::circle("speaker", gw / 2.0, gh / 2.0, gh * 0.42, draw::radial(&[(0.0, "#ffffff10"), (0.45, "#ffffff06"), (0.7, "#00000030"), (0.72, "#ffffff0c"), (1.0, "#00000000")])));
    b.add(cab, draw::stroked(draw::rounded(draw::rect("frame", 74.0, 29.0, gw + 2.0, gh + 2.0, draw::solid("#00000000")), 3.0), "#4a484c", 2.0));
}

/// A ribbon mic seen side on, pointing right at the speaker.
fn ribbon(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64) {
    b.add(parent, draw::rounded(draw::rect("body", x, y - 9.0, 120.0, 18.0, draw::linear(180.0, &[(0.0, "#e6e8ea"), (0.45, "#9aa0a6"), (1.0, "#4a4e54")])), 8.0));
    b.add(parent, draw::rounded(draw::rect("band", x + 56.0, y - 4.0, 30.0, 8.0, draw::solid("#1f7c78")), 3.0));
    let lines: Vec<Vec<(f64, f64)>> = (0..12).map(|i| {
        let lx = x + 90.0 + f64::from(i) * 5.0;
        vec![(lx, y - 13.0), (lx, y + 13.0)]
    }).collect();
    b.add(parent, draw::rounded(draw::rect("head", x + 86.0, y - 14.0, 66.0, 28.0, draw::linear(180.0, &[(0.0, "#d8dadc"), (0.5, "#8a9096"), (1.0, "#4a4e54")])), 6.0));
    b.add(parent, ui::strokes("grill", &lines, 1.4, draw::solid("#3a3e44")));
}

/// A dynamic mic, angled into the cone from the right.
fn dynamic(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64) {
    let body = draw::rounded(draw::rect("body", x, y - 13.0, 90.0, 26.0, draw::linear(180.0, &[(0.0, "#4a4a4e"), (0.5, "#1c1c1e"), (1.0, "#08080a")])), 10.0);
    b.add(parent, draw::rotated(body, -28.0));
    b.add(parent, draw::with_effects(draw::circle("grille", x + 18.0, y + 16.0, 18.0, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#3a3a3e"), (1.0, "#0a0a0c")])), vec![draw::drop_shadow("#00000090", 0.0, 3.0, 6.0)]));
}

pub fn cab() -> Face {
    let scene = canvas("Cab Room", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut room = draw::frame("Room", 0.0, 0.0, W, H);
        room.fills = vec![draw::linear(180.0, &[(0.0, "#1a1210"), (0.45, "#2a1a16"), (0.62, "#3a2420"), (0.63, "#5a3624"), (1.0, "#1c100a")])];
        room.clips_content = true;
        let room = b.add(root, room);
        // The wall's slats and the floorboards.
        let slats: Vec<Vec<(f64, f64)>> = (0..30).map(|i| {
            let x = f64::from(i) * 30.0 + 8.0;
            vec![(x, 0.0), (x, H * 0.62)]
        }).collect();
        b.add(room, ui::strokes("slats", &slats, 10.0, draw::linear(180.0, &[(0.0, "#3a2418"), (0.6, "#6a3e28"), (1.0, "#2a1810")])));
        let boards: Vec<Vec<(f64, f64)>> = (0..10).map(|i| {
            let y = H * 0.63 + f64::from(i).powf(1.4) * 12.0;
            vec![(0.0, y), (W, y)]
        }).collect();
        b.add(room, ui::strokes("boards", &boards, 1.0, draw::solid("#00000050")));
        b.add(room, draw::rect("light", 0.0, 0.0, W, H, draw::radial_at(0.5, 0.62, 0.6, &[(0.0, "#ff9a6030"), (1.0, "#00000000")])));

        // Room size over the cab.
        print(b, room, "ROOM SIZE", 380.0, 92.0, 12.0, INK, 700.0, 0.08);
        let size = Place::new("Room size").size(34.0).variant("white").bind(Binding::param("cab/room_size")).options(&["S", "M", "L"]).legend("");
        ui::put(b, room, "button.square_bank", &size, 510.0, 92.0);

        cabinet(b, room, 200.0, 170.0, 500.0, 370.0);
        ribbon(b, room, 285.0, 355.0);
        dynamic(b, room, 540.0, 335.0);

        mic_panel(b, room, 1, 105.0);
        mic_panel(b, room, 2, 795.0);

        // The room send and phase under the cab.
        let room_on = Place::new("Room send").size(30.0).variant("white").bind(Binding::param("cab/room_on")).legend("").touch(TOUCH_MIN);
        let r = ui::put(b, room, "button.square", &room_on, 520.0, 620.0);
        ui::strip_legend(b, r);
        print(b, room, "ROOM SEND", 420.0, 620.0, 12.0, INK, 700.0, 0.08);
        let dial = Dial::new("knob.soft_pointer", "grey", 56.0, INK, DIM).scale(&["", "", "", "", "", "", "", "", "", "", ""]).legend(10.0);
        ui::dial(b, room, "cab/room_level", "LEVEL", 450.0, 710.0, &dial);
        let phase = Place::new("Phase").size(34.0).variant("white").bind(Binding::param("cab/phase")).legend("").touch(TOUCH_MIN);
        let ph = ui::put(b, room, "button.square", &phase, 520.0, 830.0);
        ui::strip_legend(b, ph);
        let mut t = ui::text("phase", "PHASE Ø", 360.0, 830.0, 130.0, 13.0, INK, TextAlign::Right, 700.0, false);
        if let document_proto::NodeKind::Text(d) = &mut t.kind {
            d.letter_spacing = 1.0;
        }
        b.add(room, t);
    });
    let mut params = vec![ui::choice("cab/room_size", "Room size", &["S", "M", "L"], 1)];
    for (n, model) in [(1, 9), (2, 0)] {
        let ns = format!("cab/m{n}");
        params.push(ui::choice(&format!("{ns}_model"), &format!("Mic {n} model"), MICS, model).featured());
        params.push(ui::toggle(&format!("{ns}_on"), &format!("Mic {n}"), true));
        params.push(ui::param(&format!("{ns}_position"), &format!("Mic {n} position"), Range::linear(0.0, 1.0, Unit::None), 0.3));
        params.push(ui::param(&format!("{ns}_distance"), &format!("Mic {n} distance"), Range::linear(0.0, 1.0, Unit::None), 0.0));
        params.push(ui::param(&format!("{ns}_level"), &format!("Mic {n} level"), Range::linear(-24.0, 12.0, Unit::Db), 0.0).featured());
        params.push(ui::param(&format!("{ns}_pan"), &format!("Mic {n} pan"), Range::linear(-100.0, 100.0, Unit::Percent), 0.0).bipolar());
    }
    params.push(ui::toggle("cab/room_on", "Room send", false));
    params.push(ui::param("cab/room_level", "Room level", Range::linear(-60.0, 0.0, Unit::Db), -40.0));
    params.push(ui::toggle("cab/phase", "Phase", false));
    Face { file: "14-cab-room", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
