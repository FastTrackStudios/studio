//! Rig pages: the blues rig, laid out as sections a player swipes between.
//!
//! The rig follows the blues amp-sim suite's chain: Pre (an envelope
//! filter) → Amp-Drive → Verb & Trem → Cab → EQ & Comp → Time (delays and
//! reverbs). Each section is one page; on a phone you swipe between them,
//! on a wide screen several sit side by side or in rows, and any module on
//! a page zooms to fill the screen.
//!
//! This file builds the **Amp-Drive** section: the amp head with its drive
//! pedals seated in the grille, in chain order (boost, mild drive, teal
//! drive), plus an open slot for a fourth. Pages are built from the suite's
//! own faces (`face::embed`), so every knob on them is the pedal's or the
//! amp's own, bound to the same params.
//!
//! - `01-amp-drive` — wide (desktop, tablet, phone landscape): the head at
//!   its own size, pedals in the grille.
//! - `02-amp-drive-phone` — phone portrait (390 wide): the amp's controls
//!   as a touch-sized strip, then the pedals two by two (tap one to zoom).

use document_proto::{NodeId, TextAlign};
use surface_kit::{Place, draw};
use surface_widgets::Builder;

use crate::face::{Face, canvas, embed, face_size};
use crate::comp_tube::ui as place;
use crate::reverb::ui;
use crate::suite_studio_blues::{amps, pedals};

/// The plugin id every rig page's params are merged under.
pub const PLUGIN: &str = "rig-blues";

/// The drive pedals, in chain order.
fn drives() -> Vec<Face> {
    vec![pedals::plain_boost(), pedals::mild_drive(), pedals::teal_drive()]
}

/// The params of every face on a page.
fn params_of(faces: &[&Face]) -> Vec<surface_proto::ParamInfo> {
    let mut out: Vec<surface_proto::ParamInfo> = Vec::new();
    for f in faces {
        for p in &f.params {
            if !out.iter().any(|q| q.addr == p.addr) {
                out.push(p.clone());
            }
        }
    }
    out
}

/// An open pedal slot: a dashed outline the height of a pedal, "+ DRIVE".
fn open_slot(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    let mut slot = draw::frame("open slot", x, y, w, h);
    slot.stroke = document_proto::Stroke {
        paints: vec![draw::solid("#ffffff3d")],
        weight: 2.0,
        align: document_proto::StrokeAlign::Inside,
        dash_pattern: vec![8.0, 6.0],
        ..document_proto::Stroke::default()
    };
    slot.fills = vec![draw::solid("#00000040")];
    let slot = b.add(parent, draw::rounded(slot, 10.0));
    b.add(slot, draw::text("plus", "+", 0.0, h / 2.0 - 34.0, w, 40.0, "#ffffff80", TextAlign::Center));
    b.add(slot, draw::text("label", "DRIVE", 0.0, h / 2.0 + 14.0, w, 12.0, "#ffffff80", TextAlign::Center));
    slot
}

/// Pedals `faces` (plus `open` empty slots) across `(x, y, w, h)` at
/// `scale`, evenly spaced, bottoms aligned.
fn pedal_row(b: &mut Builder<'_>, parent: NodeId, faces: &[Face], open: usize, (x, y, w, h): (f64, f64, f64, f64), scale: f64) {
    let sizes: Vec<(f64, f64)> = faces.iter().map(face_size).chain(std::iter::repeat_n((156.0, 286.0), open)).collect();
    let total: f64 = sizes.iter().map(|s| s.0 * scale).sum();
    let gap = ((w - total) / (sizes.len() as f64 + 1.0)).max(0.0);
    let mut cx = x + gap;
    for (i, (pw, ph)) in sizes.iter().enumerate() {
        let (sw, sh) = (pw * scale, ph * scale);
        let rect = (cx, y + h - sh, sw, sh);
        match faces.get(i) {
            Some(f) => {
                embed(b, parent, f, rect);
            }
            None => {
                open_slot(b, parent, rect);
            }
        }
        cx += sw + gap;
    }
}

/// The wide Amp-Drive page: the clean head, pedals in its grille.
#[must_use]
pub fn amp_drive() -> Face {
    let drives = drives();
    let amp = amps::clean_head_with(|b, grille| {
        let (gw, gh) = b.scene.node(grille).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
        // Pedals stand on the grille's floor, a little under full size.
        let scale = ((gh - 36.0) / 286.0).min(1.0);
        pedal_row(b, grille, &drives, 1, (0.0, 0.0, gw, gh - 16.0), scale);
    });
    let mut faces: Vec<&Face> = drives.iter().collect();
    faces.push(&amp);
    let params = params_of(&faces);
    let Face { scene, .. } = amp;
    Face { file: "01-amp-drive", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// Phone portrait: the amp's controls as a strip, the pedals two by two.
#[must_use]
pub fn amp_drive_phone() -> Face {
    const W: f64 = 390.0;
    const H: f64 = 844.0;
    const INK: &str = "#141414";
    let drives = drives();
    let amp = amps::clean_head();
    let scene = canvas("Amp-Drive (phone)", |b, root| {
        if let Some(n) = b.scene.node_mut(root) {
            n.size = document_proto::Size { width: W, height: H };
            n.fills = vec![draw::solid("#0e0f12")];
        }
        // Section header, as the rig shell shows it (dots = sections).
        b.add(root, draw::text("section", "AMP · DRIVE", 20.0, 58.0, 250.0, 15.0, "#e8e6e1", TextAlign::Left));
        for i in 0..6 {
            let on = i == 1;
            b.add(root, draw::circle("page dot", 300.0 + f64::from(i) * 13.0, 67.0, if on { 3.5 } else { 2.5 }, draw::solid(if on { "#e8e6e1" } else { "#ffffff40" })));
        }
        // The amp strip: the head's faceplate steel, its knobs touch-sized.
        let plate = draw::rounded(draw::frame("amp strip", 12.0, 88.0, W - 24.0, 232.0), 8.0);
        let mut plate = plate;
        plate.fills = vec![draw::linear(180.0, &[(0.0, "#d0d1d3"), (1.0, "#aeafb2")])];
        let plate = b.add(root, plate);
        let slab = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(900.0).italic();
        let legend = draw::Lettering::family(draw::family::PIRATA_ONE);
        b.add(plate, draw::styled_text("logo", "OPEN ROOM", 14.0, 10.0, 200.0, 18.0, "#0e0e0e", TextAlign::Left, &slab));
        let knob = |b: &mut Builder<'_>, addr: &str, name: &str, cx: f64, cy: f64, d: f64| {
            let p = Place::new(name).legend(" ").size(d).variant("spun_metal").options(&["skirt=knurled", "touch=44"]).bind(ui::bind(addr));
            place::centred_at(b, plate, "knob.3d", &p, cx, cy);
            b.add(plate, draw::styled_text("legend", &name.to_uppercase(), cx - 36.0, cy + d / 2.0 + 4.0, 72.0, 11.0, INK, TextAlign::Center, &legend));
        };
        let row1 = ["Volume", "Treble", "Middle", "Bass", "Level"];
        for (i, name) in row1.iter().enumerate() {
            knob(b, &format!("clean/{}", name.to_lowercase()), name, 36.0 + f64::from(i as u32) * 73.0, 68.0, 50.0);
        }
        for (i, (addr, name)) in [("clean/high", "High"), ("clean/low", "Low"), ("clean/send", "Send"), ("clean/return", "Return")].iter().enumerate() {
            knob(b, addr, name, 44.0 + f64::from(i as u32) * 64.0, 160.0, 40.0);
        }
        // The slide switches as pills.
        for (i, (addr, name)) in [("clean/bright", "BRIGHT"), ("clean/mid", "MID"), ("clean/reverb_on", "VERB")].iter().enumerate() {
            let p = Place::new(*name).size(26.0).variant("red").bind(ui::bind(addr));
            let cy = 132.0 + f64::from(i as u32) * 30.0;
            place::centred_at(b, plate, "switch.mini_slide", &p, 300.0, cy);
            b.add(plate, draw::text("pill", name, 314.0, cy - 6.0, 44.0, 9.0, INK, TextAlign::Left));
        }
        // The drives, two by two: each the pedal itself (tap to zoom).
        let scale = 0.82;
        let (pw, ph) = (156.0 * scale, 286.0 * scale);
        let cell = |i: usize| {
            let (col, row) = (i % 2, i / 2);
            let gap = (W - 2.0 * pw) / 3.0;
            (gap + col as f64 * (pw + gap), 340.0 + row as f64 * (ph + 18.0))
        };
        for (i, f) in drives.iter().enumerate() {
            let (x, y) = cell(i);
            let (fw, fh) = face_size(f);
            let s = (ph / fh).min(pw / fw);
            embed(b, root, f, (x + (pw - fw * s) / 2.0, y, fw * s, fh * s));
        }
        let (x, y) = cell(3);
        open_slot(b, root, (x, y, pw, ph));
    });
    let mut faces: Vec<&Face> = drives.iter().collect();
    faces.push(&amp);
    let params = params_of(&faces);
    Face { file: "02-amp-drive-phone", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// Every blues-rig page.
#[must_use]
pub fn faces() -> Vec<Face> {
    vec![amp_drive(), amp_drive_phone()]
}
