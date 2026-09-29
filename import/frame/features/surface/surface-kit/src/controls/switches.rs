//! Panel switches — the bat toggle, the rocker and the rotary selector.
//! Ported from `fts-audio-ui::hardware::switches` (`ToggleSwitch`) and the
//! rack kit's detented rotary switch (`Ring::Detents`).
//!
//! A toggle is drawn so that the only thing anyone reads about it — which
//! way the bat leans — is the moving part: the bat is a `Thumb` in a lane
//! the height of the slot, so it sits up when the toggle param is on and
//! down when it is off. A rotary selector is a `Knob` over a choice param
//! whose sweep is exactly its detents.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, fill, ink, lin, rad, shade, text_w, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// Two legends, top then bottom, from `Place::options`.
fn two(p: &Place, top: &str, bottom: &str) -> [String; 2] {
    match p.options.as_deref() {
        Some([a, b, ..]) => [a.clone(), b.clone()],
        _ => [top.to_string(), bottom.to_string()],
    }
}

/// A two-position bat toggle (LA-2A COMPRESS/LIMIT, a POWER switch): a
/// chrome bat through a nut in a black slot, legends beside it, caption
/// (`Label`) above.
fn toggle_bat(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let bh = p.size.unwrap_or(44.0);
    let k = bh / 44.0;
    let bw = 22.0 * k;
    let bat = match p.variant.as_deref() {
        Some("black") => ["#2a2a2e", "#5a5a60", "#3a3a3e", "#141416"],
        _ => ["#7c7a74", "#f6f4ee", "#c9c7c0", "#6c6a64"],
    };
    let [on, off] = two(p, "On", "Off");
    let size = 8.5 * k;
    let legend_w = text_w(&on, size).max(text_w(&off, size)).max(size * 2.0);
    let gap = 6.0 * k;
    let cap_h = (9.0 * k * 1.4).ceil();
    let w = legend_w + gap + bw;
    let h = cap_h + 4.0 * k + bh;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let label = b.add(root, ink("label", p.legend.as_deref().unwrap_or(""), 0.0, cap_h / 2.0, w, 9.0 * k, PANEL_INK, TextAlign::Center, 700.0));
    let top = cap_h + 4.0 * k;
    b.add(root, ink("on", &on, 0.0, top + size * 0.9, legend_w, size, PANEL_INK, TextAlign::Right, 700.0));
    b.add(root, ink("off", &off, 0.0, top + bh - size * 0.9, legend_w, size, PANEL_INK, TextAlign::Right, 700.0));
    let x = legend_w + gap;
    let slot = draw::rect("slot", x, top, bw, bh, lin(180.0, &[(0.0, "#17171a".into()), (1.0, "#0a0a0c".into())]));
    let slot = draw::stroked(draw::rounded(slot, 5.0 * k), "#00000099", k.max(1.0));
    b.add(root, draw::with_effects(slot, vec![draw::inner_shadow("#000000b3", 0.0, 0.0, 5.0 * k)]));
    // The nut the bat comes out of.
    let (cx, cy) = (x + bw / 2.0, top + bh / 2.0);
    b.add(root, draw::path("nut", 0.0, 0.0, w, h, draw::regular(cx, cy, bw * 0.40, 6, 30.0), rad(0.35, 0.3, 0.9, &[(0.0, "#e8e8e4".into()), (1.0, "#5c5c5a".into())])));
    b.add(root, draw::circle("bushing", cx, cy, bw * 0.26, rad(0.4, 0.35, 0.8, &[(0.0, "#bdbdb8".into()), (1.0, "#3a3a38".into())])));
    // The lane the bat travels in: up = on, down = off.
    let lane_y = top + bh * 0.06;
    let lane = b.add(root, draw::frame("lane", x, lane_y, bw, bh * 0.88));
    let bat_node = draw::rect(
        "bat",
        bw * 0.28,
        0.0,
        bw * 0.44,
        bh * 0.46,
        lin(90.0, &[(0.0, bat[0].into()), (0.38, bat[1].into()), (0.62, bat[2].into()), (1.0, bat[3].into())]),
    );
    let bat_node = draw::with_effects(draw::rounded(bat_node, bw * 0.22), vec![draw::drop_shadow("#00000099", 0.0, 1.5 * k, 3.0 * k)]);
    let bat_id = b.add(lane, bat_node);
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, bat_id.0).slot(SlotRole::Label, label.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

/// A rocker: the proud end catches the light, the pressed end is in shade.
/// The light is the `Thumb` (top half when on), `I`/`O` printed over it.
fn rocker(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let h = p.size.unwrap_or(44.0);
    let k = h / 44.0;
    let w = 26.0 * k;
    let (cap_c, mark) = match p.variant.as_deref() {
        Some("red") => ("#b8261c", "#f6e6e2"),
        Some("white") => ("#dcd8cc", "#23252a"),
        _ => ("#1f2023", "#e8e8e6"),
    };
    let [on, off] = two(p, "I", "O");
    let cap_h = (9.0 * k * 1.4).ceil();
    let top = cap_h + 4.0 * k;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, top + h));
    let label = b.add(root, ink("label", p.legend.as_deref().unwrap_or(""), -12.0 * k, cap_h / 2.0, w + 24.0 * k, 9.0 * k, PANEL_INK, TextAlign::Center, 700.0));
    let bezel = draw::rect("bezel", 0.0, top, w, h, lin(180.0, &[(0.0, "#2a2a2e".into()), (1.0, "#0e0e10".into())]));
    let bezel = draw::stroked(draw::rounded(bezel, 3.5 * k), "#000000b3", k.max(1.0));
    b.add(root, draw::with_effects(bezel, vec![draw::drop_shadow("#00000080", 0.0, k, 3.0 * k)]));
    let inset = 3.0 * k;
    let (cx, cy, cw, ch) = (inset, top + inset, w - 2.0 * inset, h - 2.0 * inset);
    let body = draw::rect("rocker", cx, cy, cw, ch, fill(&shade(cap_c, 0.25)));
    b.add(root, draw::with_effects(draw::rounded(body, 2.0 * k), vec![draw::inner_shadow("#000000a0", 0.0, 0.0, 3.0 * k)]));
    let lane = b.add(root, draw::frame("lane", cx, cy, cw, ch));
    let proud = draw::rect("proud", 0.0, 0.0, cw, ch * 0.5, lin(180.0, &[(0.0, tint(cap_c, 0.22)), (0.85, cap_c.to_string()), (1.0, shade(cap_c, 0.3))]));
    let proud = draw::with_effects(draw::rounded(proud, 2.0 * k), vec![draw::inner_shadow("#ffffff40", 0.0, k, 0.0), draw::drop_shadow("#000000a0", 0.0, 0.0, 2.0 * k)]);
    let proud = b.add(lane, proud);
    let size = 10.0 * k;
    b.add(root, ink("on", &on, cx, cy + ch * 0.25, cw, size, mark, TextAlign::Center, 700.0));
    b.add(root, ink("off", &off, cx, cy + ch * 0.75, cw, size, mark, TextAlign::Center, 700.0));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, proud.0).slot(SlotRole::Label, label.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

/// Detent geometry for `n` positions: `(step, sweep)` in degrees. A rotary
/// switch clicks through ~30–60° per detent and never more than 300° in all.
fn detents(n: usize) -> (f64, f64) {
    if n < 2 {
        return (0.0, 0.0);
    }
    let step = (300.0 / (n - 1) as f64).min(60.0);
    (step, step * (n - 1) as f64)
}

/// A detented rotary selector: a black Bakelite chicken-head pointer, a dot
/// at each detent and the option legends printed around it.
fn rotary(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(48.0);
    let k = d / 48.0;
    let body_c = match p.variant.as_deref() {
        Some("cream") => "#e2d8c0",
        Some("red") => "#a8261e",
        _ => "#1d1b1a",
    };
    let line_c = if p.variant.as_deref() == Some("cream") { "#1d1b1a" } else { "#f2efe6" };
    let opts: Vec<String> = match &p.options {
        Some(o) if !o.is_empty() => o.clone(),
        _ => (1..=3).map(|i| i.to_string()).collect(),
    };
    let n = opts.len();
    let (step, sweep) = detents(n);
    let size = 9.0 * k;
    let widest = opts.iter().map(|o| text_w(o, size)).fold(0.0, f64::max);
    let r_dot = d * 0.5 + 4.0 * k;
    let r_lbl = r_dot + 5.0 * k + size * 0.6;
    let half = r_lbl + widest / 2.0 + 2.0;
    let side = 2.0 * half;
    let cap_h = (9.0 * k * 1.4).ceil();
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, side, side * 0.5 + d * 0.5 + 6.0 * k + cap_h));
    let (cx, cy) = (half, half);
    let ink_c = PANEL_INK;
    for (i, o) in opts.iter().enumerate() {
        let a = -sweep / 2.0 + i as f64 * step;
        let (dx, dy) = draw::polar(cx, cy, r_dot, a);
        b.add(root, draw::circle("detent", dx, dy, 1.4 * k, draw::solid(ink_c)));
        let (lx, ly) = draw::polar(cx, cy, r_lbl, a);
        let w = text_w(o, size) + 6.0;
        b.add(root, ink("option", o, lx - w / 2.0, ly, w, size, ink_c, TextAlign::Center, 700.0));
    }
    let label = b.add(root, ink("label", p.legend.as_deref().unwrap_or(""), 0.0, cy + d * 0.5 + 6.0 * k + cap_h / 2.0, side, 9.0 * k, ink_c, TextAlign::Center, 700.0));
    // The dial: a square centred on the pivot, so the pointer inside it
    // rotates about the shaft.
    let dial = b.add(root, draw::frame("dial", cx - d / 2.0, cy - d / 2.0, d, d));
    let c0 = d / 2.0;
    let pointer = b.add(dial, draw::frame("pointer", 0.0, 0.0, d, d));
    // Chicken head, pointing up (the centre of travel): a tapered bar from
    // a narrow tip to a wide, rounded tail, over a round boss.
    let pt = |x: f64, y: f64| format!("{:.3} {:.3}", c0 + x * d, c0 + y * d);
    let head = format!(
        "M {} Q {} {} L {} Q {} {} Q {} {} Z",
        pt(-0.035, -0.5),
        pt(0.0, -0.53),
        pt(0.035, -0.5),
        pt(0.2, 0.18),
        pt(0.24, 0.36),
        pt(0.0, 0.40),
        pt(-0.24, 0.36),
        pt(-0.2, 0.18),
    );
    let body = |c: &str| rad(0.42, 0.3, 0.8, &[(0.0, tint(c, 0.28)), (0.5, c.to_string()), (1.0, shade(c, 0.5))]);
    b.add(pointer, draw::path("head", 0.0, 0.0, d, d, head, body(body_c)));
    b.add(pointer, draw::with_effects(draw::circle("boss", c0, c0 + d * 0.06, d * 0.2, body(body_c)), vec![draw::drop_shadow("#000000a0", 0.0, d * 0.05, d * 0.1), draw::inner_shadow("#ffffff26", 0.0, k, k)]));
    let lw = (d * 0.05).max(1.5);
    b.add(pointer, draw::rounded(draw::rect("index", c0 - lw / 2.0, c0 - d * 0.47, lw, d * 0.34, draw::solid(line_c)), lw / 2.0));
    let mut c = Control::new(ControlKind::Knob { sweep_degrees: sweep }).slot(SlotRole::Needle, pointer.0).slot(SlotRole::Label, label.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "switch.toggle_bat",
            family: Family::Switch,
            title: "Bat toggle",
            description: "A two-position panel toggle — chrome bat through a hex nut in a black slot, legends beside it (LA-2A COMPRESS/LIMIT, POWER). The bat leans up when the toggle param is on. `Place::options` = [top, bottom] legends (default On/Off); `legend` = caption above. Size = slot height.",
            variants: &["chrome", "black"],
            default_size: 44.0,
            binds: "toggle param",
            build: toggle_bat,
        },
        ComponentDef {
            id: "switch.rocker",
            family: Family::Switch,
            title: "Rocker switch",
            description: "A rocker in a black bezel: the raised end catches the light (top = on). Printed I/O, or `Place::options` = [top, bottom]. Power and mode switches. Size = bezel height.",
            variants: &["black", "red", "white"],
            default_size: 44.0,
            binds: "toggle param",
            build: rocker,
        },
        ComponentDef {
            id: "switch.rotary",
            family: Family::Switch,
            title: "Rotary selector",
            description: "A detented rotary switch: black Bakelite chicken-head pointer with a dot and printed legend at each position (LA-2A/Manley meter and mode selectors). Put the choice param's option names in `Place::options` — the sweep is built from their count (≤60° per detent); without them it prints three numbered positions. Size = knob diameter.",
            variants: &["black", "cream", "red"],
            default_size: 48.0,
            binds: "choice param",
            build: rotary,
        },
    ]
}
