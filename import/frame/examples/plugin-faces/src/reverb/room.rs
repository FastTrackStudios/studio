//! The room: a plain studio box whose character is its early reflections.
//!
//! The recessed plate at the top draws the room in perspective — back wall,
//! floor lines, the source against the back wall and the listener near the
//! front — with the first reflections folding off the walls, and beside it
//! those same reflections as a tap diagram over the first 80 ms. SIZE and
//! DECAY, the two a room is chosen by, are the big brushed knobs on the
//! right with the room keys under them; the rest run along the bottom.

use document_proto::NodeId;
use surface_kit::{Place, draw};
use surface_proto::Unit;
use surface_widgets::Builder;

use super::ui::{self, Dial, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 320.0;
const INK: &str = "#e3e8ee";
const DIM: &str = "#98a2ad";
const RAY: &str = "#86d69a";

/// Early reflections: (ms, level 0..1).
const TAPS: &[(f64, f64)] = &[(0.0, 1.0), (7.5, 0.72), (11.0, 0.6), (14.5, 0.66), (19.0, 0.44), (23.0, 0.5), (29.0, 0.33), (34.0, 0.38), (41.0, 0.24), (47.0, 0.27), (55.0, 0.17), (63.0, 0.14), (72.0, 0.1)];

fn room_art(b: &mut Builder<'_>, plate: NodeId, w: f64, h: f64) {
    let line = ui::alpha(INK, 0.55);
    let faint = ui::alpha(INK, 0.16);
    // The box: front opening, back wall, the four edges between.
    let (fx0, fy0, fx1, fy1) = (22.0, 14.0, 318.0, h - 14.0);
    let (bx0, by0, bx1, by1) = (84.0, 40.0, 256.0, h - 44.0);
    b.add(plate, draw::rect("back wall", bx0, by0, bx1 - bx0, by1 - by0, draw::linear(180.0, &[(0.0, "#ffffff08"), (1.0, "#ffffff12")])));
    let floor = draw::polygon(&[(bx0, by1), (bx1, by1), (fx1, fy1), (fx0, fy1)]);
    b.add(plate, ui::area("floor", floor, w, h, draw::linear(180.0, &[(0.0, "#86d69a0a"), (1.0, "#86d69a1c")])));
    for (a, c) in [((fx0, fy0), (bx0, by0)), ((fx1, fy0), (bx1, by0)), ((fx0, fy1), (bx0, by1)), ((fx1, fy1), (bx1, by1))] {
        ui::line(b, plate, "edge", &[a, c], 1.0, &line);
    }
    ui::line(b, plate, "back wall", &[(bx0, by0), (bx1, by0), (bx1, by1), (bx0, by1), (bx0, by0)], 1.2, &line);
    ui::line(b, plate, "front", &[(fx0, fy0), (fx1, fy0), (fx1, fy1), (fx0, fy1), (fx0, fy0)], 1.0, &faint);
    // Floor boards, receding.
    for i in 1..8 {
        let t = f64::from(i) / 8.0;
        ui::line(b, plate, "board", &[(bx0 + (bx1 - bx0) * t, by1), (fx0 + (fx1 - fx0) * t, fy1)], 0.6, &faint);
    }
    // Source at the back, listener near the front.
    let src = (bx0 + 34.0, by1 - 10.0);
    let ear = (fx0 + (fx1 - fx0) * 0.6, fy1 - 16.0);
    b.add(plate, draw::rounded(draw::rect("speaker", src.0 - 7.0, src.1 - 12.0, 14.0, 20.0, draw::linear(180.0, &[(0.0, "#4a5057"), (1.0, "#22262a")])), 2.0));
    b.add(plate, draw::circle("cone", src.0, src.1 + 1.0, 4.5, draw::solid("#0e1012")));
    b.add(plate, ui::glowing(draw::circle("listener", ear.0, ear.1, 4.0, draw::solid(RAY)), RAY, 0.8, 5.0));
    ui::line(b, plate, "direct", &[src, ear], 0.8, &ui::alpha(INK, 0.35));
    // First reflections: off the side walls, the ceiling and the back.
    let bounces = [(fx0 + 16.0, fy0 + 60.0), (bx1 + 30.0, by0 + 10.0), ((bx0 + bx1) / 2.0 + 20.0, by0 - 14.0), (fx1 - 20.0, fy1 - 36.0)];
    for (i, p) in bounces.iter().enumerate() {
        let o = 0.9 - 0.16 * i as f64;
        ui::line(b, plate, "ray", &[src, *p, ear], 1.1, &ui::alpha(RAY, o));
        b.add(plate, ui::glowing(draw::circle("bounce", p.0, p.1, 2.2, draw::solid(RAY)), RAY, 0.8, 4.0));
    }

    // The tap diagram: the same reflections over time.
    let (x0, x1, base, top) = (352.0, w - 20.0, h - 30.0, 26.0);
    ui::silk_left(b, plate, "Early reflections", x0, 16.0, 6.5, DIM, 800.0);
    ui::line(b, plate, "axis", &[(x0, base), (x1, base)], 1.0, &line);
    for ms in [0.0, 20.0, 40.0, 60.0, 80.0] {
        let x = x0 + (x1 - x0) * ms / 80.0;
        ui::line(b, plate, "tick", &[(x, base), (x, base + 3.5)], 1.0, &line);
        let l = if ms >= 80.0 { "80 ms".to_string() } else { format!("{ms:.0}") };
        ui::plain(b, plate, &l, x, base + 11.0, 6.5, DIM, 600.0);
    }
    let mut spikes = Vec::new();
    for (ms, a) in TAPS {
        let x = x0 + (x1 - x0) * ms / 80.0;
        let y = base - (base - top) * a;
        spikes.push(vec![(x, base), (x, y)]);
    }
    b.add(plate, ui::glowing(ui::bars("taps", &spikes, 2.2, RAY), RAY, 0.7, 3.0));
    for (ms, a) in TAPS {
        let x = x0 + (x1 - x0) * ms / 80.0;
        b.add(plate, draw::circle("tap head", x, base - (base - top) * a, 1.8, draw::solid(RAY)));
    }
}

pub fn face() -> Face {
    let scene = canvas("Room Reverb", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "charcoal", W, H, "ears");

        ui::plain(b, panel, "Room", 80.0, 32.0, 17.0, INK, 800.0);
        ui::silk_left(b, panel, "Ambience processor", 118.0, 28.0, 7.5, INK, 800.0);
        ui::silk_left(b, panel, "Early reflections · short decay", 118.0, 40.0, 6.0, DIM, 700.0);

        // ── The plate: the room and its reflections ───────────────────
        let (px, py, pw, ph) = (50.0, 54.0, 560.0, 140.0);
        let mut plate = draw::frame("Room", px, py, pw, ph);
        plate.fills = vec![draw::linear(180.0, &[(0.0, "#16191c"), (1.0, "#202428")])];
        plate.corner_radii = document_proto::CornerRadii::uniform(6.0);
        plate.clips_content = true;
        plate.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0), draw::drop_shadow("#ffffff1a", 0.0, 1.0, 0.0)];
        let plate = b.add(panel, plate);
        room_art(b, plate, pw, ph);

        // ── SIZE and DECAY, with the room keys ─────────────────────────
        let big = Dial::new("knob.metal", "silver", 58.0, INK, DIM).legend(10.0);
        ui::dial(b, panel, "room/size", "Size", 704.0, 96.0, &big.scale(&["0", "", "", "", "", "100"]).legend_at(160.0));
        ui::dial(b, panel, "room/decay", "Decay", 842.0, 96.0, &big.scale(&["0.2", "", "", "", "", "4 s"]).legend_at(160.0));
        let keys = Place::new("Room").size(62.0).variant("green").bind(bind("room/type")).options(&["Room", "Chamber", "Studio"]);
        let k = ui::put(b, panel, "button.led_bank", &keys, 773.0, 240.0);
        ui::strip_legend(b, k);
        ui::silk(b, panel, "Space", 773.0, 272.0, 7.5, INK, 800.0);
        ui::rule(b, panel, 640.0, 40.0, 1.0, 250.0, "#00000066");
        ui::rule(b, panel, 641.0, 40.0, 1.0, 250.0, "#ffffff12");

        // ── The row ────────────────────────────────────────────────────
        let small = Dial::new("knob.metal", "black", 38.0, INK, DIM).legend_at(292.0);
        let row: &[(&str, &str)] = &[
            ("room/predelay", "Pre-Delay"),
            ("room/damping", "Damping"),
            ("room/tone", "Tone"),
            ("room/diffusion", "Diffusion"),
            ("room/walls", "Walls"),
            ("room/width", "Width"),
            ("room/mix", "Mix"),
        ];
        let pitch = (pw - 40.0) / (row.len() - 1) as f64;
        for (i, (addr, legend)) in row.iter().enumerate() {
            ui::dial(b, panel, addr, legend, px + 20.0 + pitch * i as f64, 238.0, &small);
        }
    });

    let params = vec![
        super::pct("room/size", "Size", 45.0).featured(),
        ui::param("room/decay", "Decay", surface_proto::Range::log(0.2, 4.0, Unit::Seconds), 0.8).featured(),
        ui::choice("room/type", "Room", &["Room", "Chamber", "Studio"], 0).featured(),
        super::predelay("room/predelay", "Pre-Delay", 5.0),
        super::pct("room/damping", "Damping", 35.0),
        super::pct("room/tone", "Tone", 50.0),
        super::pct("room/diffusion", "Diffusion", 60.0),
        super::pct("room/walls", "Walls", 50.0),
        super::width("room/width"),
        super::mix("room/mix", 25.0),
    ];
    Face { file: "04-room", plugin: "reverb", scene, params, streams: super::levels("room") }
}
