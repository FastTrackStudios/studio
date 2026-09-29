//! Panel-building helpers for the reverb and modulation faces. The first
//! half is the VCA faces' helpers (placing by shaft, silkscreen, lamps,
//! readouts, LED meters, catalog entries), copied so these faces own them;
//! the second half is what these faces add: glass displays with glowing
//! readouts, stroked curves, lamp rows on a level stream, the LFO rate lamp,
//! a stompbox footswitch and enclosure, and a knob helper that prints its
//! own scale in any ink (the kit's is light-only).

use document_proto::{Node, NodeId, NodeKind, TextAlign, TextCase, Transform};
use surface_kit::{Place, draw};
use surface_proto::{
    Binding, Control, ControlKind, Curve, CurvePoint, EntryId, ParamInfo, Range, SlotRole, StreamInfo, StreamKind, Unit,
};
use surface_widgets::Builder;

// ── placing ──────────────────────────────────────────────────────────────

/// Move `id` so its top-left corner is at (`x`, `y`) in its parent.
pub fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

/// The point a component is placed by, in its own box: the centre of its
/// shaft for knobs and rotary switches (`knob` / `dial` child), otherwise
/// the centre of the box.
fn anchor(b: &Builder<'_>, root: NodeId) -> (f64, f64) {
    let Some(n) = b.scene.node(root) else { return (0.0, 0.0) };
    for c in &n.children {
        if let Some(k) = b.scene.node(*c)
            && matches!(k.name.as_str(), "knob" | "dial")
        {
            return (k.transform.tx + k.size.width / 2.0, k.transform.ty + k.size.height / 2.0);
        }
    }
    (n.size.width / 2.0, n.size.height / 2.0)
}

/// Place kit component `id` under `parent` with its shaft (or centre) at
/// (`cx`, `cy`).
///
/// # Panics
/// On an unknown component id or variant — faces are literals.
pub fn put(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, cx: f64, cy: f64) -> NodeId {
    let root = surface_kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"));
    let (ax, ay) = anchor(b, root);
    move_to(b, root, cx - ax, cy - ay);
    root
}

/// Place kit component `id` with its box's top-left at (`x`, `y`).
///
/// # Panics
/// On an unknown component id or variant.
pub fn put_at(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, x: f64, y: f64) -> NodeId {
    let root = surface_kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"));
    move_to(b, root, x, y);
    root
}

/// A kit panel of `w × h` in `variant`, at the origin of `parent`.
///
/// # Panics
/// On an unknown panel id or variant.
pub fn panel(b: &mut Builder<'_>, parent: NodeId, id: &str, variant: &str, w: f64, h: f64, ends: &str) -> NodeId {
    let opts = [format!("w={w}"), format!("h={h}"), format!("ends={ends}")];
    let p = Place { options: Some(opts.to_vec()), ..Place::new("Faceplate").variant(variant) };
    put_at(b, parent, id, &p, 0.0, 0.0)
}

/// Size the canvas's root frame to its panel, so the face renders at the
/// panel's size.
pub fn fit_root(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64) {
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size { width: w, height: h };
    }
}

fn children_named(b: &Builder<'_>, id: NodeId, names: &[&str]) -> Vec<NodeId> {
    b.scene.walk(id).into_iter().filter(|c| *c != id && b.scene.node(*c).is_some_and(|n| names.contains(&n.name.as_str()))).collect()
}

fn remove(b: &mut Builder<'_>, ids: Vec<NodeId>) {
    for id in ids {
        if b.scene.node(id).is_some() {
            let _ = b.scene.apply(document_proto::Op::Remove { id });
        }
    }
}

/// Take a placed knob's own printed scale off — the face prints its own
/// (detent legends, a scale that isn't the kit's even spread).
pub fn strip_scale(b: &mut Builder<'_>, knob: NodeId) {
    let ids = children_named(b, knob, &["scale dots", "scale arc", "scale majors", "scale minors", "numeral"]);
    remove(b, ids);
}

/// Take a placed control's legend (and its `Label` slot) off: the face
/// prints the legend itself, somewhere the kit doesn't (above a Distressor
/// dial, over a band's column).
pub fn strip_legend(b: &mut Builder<'_>, control: NodeId) {
    let ids: Vec<NodeId> = b.controls.get(&control).map(|c| c.slots_of(SlotRole::Label).map(NodeId).collect()).unwrap_or_default();
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
    }
    remove(b, ids);
    // A knob's box was grown to hold its legend: shrink it back to the dial.
    let dial = b.scene.node(control).and_then(|n| n.children.iter().find_map(|c| b.scene.node(*c).filter(|k| k.name == "knob").map(|k| k.size.height)));
    if let (Some(h), Some(n)) = (dial, b.scene.node_mut(control)) {
        n.size.height = h;
    }
}

/// Shrink a placed knob's box to its body (`d` across), keeping the shaft
/// where it is: the kit's box includes the scale margin and legend, which
/// is too big a touch target when another control sits close (the cap of
/// a concentric pair).
pub fn tighten(b: &mut Builder<'_>, control: NodeId, d: f64) {
    let Some(n) = b.scene.node(control) else { return };
    let (tx, ty) = (n.transform.tx, n.transform.ty);
    let Some((kid, side)) = n.children.iter().find_map(|c| b.scene.node(*c).filter(|k| k.name == "knob").map(|k| (*c, k.size.width))) else { return };
    let inset = (side - d) / 2.0;
    move_to(b, kid, -inset, -inset);
    move_to(b, control, tx + inset, ty + inset);
    if let Some(n) = b.scene.node_mut(control) {
        n.size = document_proto::Size { width: d, height: d };
    }
}

/// Make a placed control's turning *frame* turn properly: its parts become
/// the `Needle`s instead.
///
/// `lever.pultec` (the paddle) and `switch.rotary` (the chicken-head) put
/// their `Needle` slot on a frame of parts. A needle turns about the centre
/// of its parent — the lever's taller control box, below its boss — and
/// the renderer moves a turned frame's children without turning them, so
/// off the centre of travel the pointer slides instead of swinging. The
/// frame is square and centred on the pivot, so leaving it still and
/// turning each of its parts about *its* centre is exactly the swing.
pub fn unfold_needle(b: &mut Builder<'_>, control: NodeId) {
    let Some(c) = b.controls.get(&control) else { return };
    let frames: Vec<(NodeId, Vec<NodeId>)> = c
        .slots_of(SlotRole::Needle)
        .filter_map(|id| b.scene.node(NodeId(id)).filter(|n| matches!(n.kind, NodeKind::Frame) && !n.children.is_empty()).map(|n| (NodeId(id), n.children.clone())))
        .collect();
    let Some(c) = b.controls.get_mut(&control) else { return };
    for (frame, parts) in frames {
        c.slots.retain(|s| !(s.role == SlotRole::Needle && s.node == frame.0));
        c.slots.extend(parts.into_iter().map(|p| surface_proto::Slot { role: SlotRole::Needle, node: p.0 }));
    }
}

/// A filter-shape glyph (what a Neve prints over a band instead of its
/// name): a stroked curve `w` wide centred on (`cx`, `cy`).
pub fn glyph(b: &mut Builder<'_>, parent: NodeId, shape: Glyph, cx: f64, cy: f64, w: f64, color: &str) {
    let h = w * 0.42;
    let (x0, y0) = (cx - w / 2.0, cy - h / 2.0);
    let ease = |t: f64| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // The response, y (0 = top) against x, both 0..1.
    let curve = |u: f64| match shape {
        Glyph::HighShelf => 0.8 - 0.6 * ease((u - 0.3) / 0.4),
        Glyph::LowShelf => 0.2 + 0.6 * ease((u - 0.3) / 0.4),
        Glyph::Bell => 0.82 - 0.7 * (-((u - 0.5) / 0.13).powi(2)).exp(),
        Glyph::HighPass => 0.2 + 0.8 * (1.0 - (u / 0.42).min(1.0)).powi(2),
    };
    // The renderer fills geometry; a stroked curve is a ribbon of quads.
    let n = 48;
    let pts: Vec<(f64, f64)> = (0..=n).map(|i| {
        let u = f64::from(i) / f64::from(n);
        (u * w, curve(u) * h)
    }).collect();
    let hw = (w * 0.03).max(0.7);
    let mut d = String::new();
    for s in pts.windows(2) {
        let ((ax, ay), (bx, by)) = (s[0], s[1]);
        let len = (bx - ax).hypot(by - ay).max(1e-6);
        let (nx, ny) = (-(by - ay) / len * hw, (bx - ax) / len * hw);
        // Overlap each quad a little into the next, so joints stay closed.
        let (ex, ey) = ((bx - ax) / len * hw * 0.8, (by - ay) / len * hw * 0.8);
        d.push_str(&draw::polygon(&[(ax + nx, ay + ny), (bx + ex + nx, by + ey + ny), (bx + ex - nx, by + ey - ny), (ax - nx, ay - ny)]));
        d.push(' ');
    }
    b.add(parent, draw::path("glyph", x0, y0, w, h, d, draw::solid(color)));
}

/// Which filter shape [`glyph`] draws.
#[derive(Clone, Copy)]
pub enum Glyph {
    HighShelf,
    LowShelf,
    Bell,
    HighPass,
}

/// A range whose evenly spaced printed marks are exactly `values` (a dial
/// printed unevenly — a console's frequency pots, a ratio scale): the
/// control position follows the print, piecewise between marks.
///
/// # Panics
/// With fewer than two values.
#[must_use]
pub fn printed(values: &[f64], unit: Unit) -> Range {
    assert!(values.len() >= 2, "a printed scale needs two marks");
    let last = (values.len() - 1) as f64;
    let (lo, hi) = (values[0], values[values.len() - 1]);
    let points = values.iter().enumerate().map(|(i, v)| CurvePoint { x: i as f64 / last, y: (v - lo) / (hi - lo) }).collect();
    Range { min: lo, max: hi, scale: surface_proto::Scale::Curve { curve: Curve::Points { points } }, unit, steps: None }
}

/// A scale printed on the panel around a shaft at (`cx`, `cy`): a hairline
/// arc at `r`, a major tick at each label, a minor tick between, and the
/// labels set just outside — pushed out by their own width, so long ones
/// (`100mV`) clear the ticks at the sides.
#[allow(clippy::too_many_arguments)]
pub fn print_scale(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, labels: &[&str], sweep: f64, size: f64, color: &str) {
    let n = labels.len();
    let side = 2.0 * (r + 4.0);
    let (ox, oy) = (cx - side / 2.0, cy - side / 2.0);
    let c = side / 2.0;
    let arc = draw::sector(c, c, r - 0.35, r + 0.35, -sweep / 2.0, sweep / 2.0);
    b.add(parent, draw::with_opacity(draw::path("scale arc", ox, oy, side, side, arc, draw::solid(color)), 0.45));
    let tick = |a: f64, len: f64, w: f64| {
        let (x0, y0) = draw::polar(c, c, r, a);
        let (x1, y1) = draw::polar(c, c, r + len, a);
        let (nx, ny) = ((a.to_radians()).cos() * w / 2.0, (a.to_radians()).sin() * w / 2.0);
        draw::polygon(&[(x0 - nx, y0 - ny), (x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny)])
    };
    let mut majors = String::new();
    let mut minors = String::new();
    for i in 0..n {
        majors.push_str(&tick(detent_angle(i, n, sweep), 4.0, 1.3));
        if i + 1 < n {
            minors.push_str(&tick((detent_angle(i, n, sweep) + detent_angle(i + 1, n, sweep)) / 2.0, 2.5, 0.8));
        }
    }
    b.add(parent, draw::path("scale majors", ox, oy, side, side, majors, draw::solid(color)));
    b.add(parent, draw::with_opacity(draw::path("scale minors", ox, oy, side, side, minors, draw::solid(color)), 0.6));
    for (i, l) in labels.iter().enumerate() {
        if l.is_empty() {
            continue;
        }
        let a = detent_angle(i, n, sweep);
        let w = l.chars().count() as f64 * size * 0.6;
        let push = 0.5 * (w * a.to_radians().sin().abs() + size * a.to_radians().cos().abs());
        let (lx, ly) = draw::polar(cx, cy, r + 6.0 + push, a);
        b.add(parent, text("scale label", l, lx - w, ly, 2.0 * w, size, color, TextAlign::Center, 600.0, false));
    }
}

/// Change a knob control's sweep (a stepped selector turns through exactly
/// its detents, not the kit's 300°).
pub fn set_sweep(b: &mut Builder<'_>, control: NodeId, sweep: f64) {
    if let Some(c) = b.controls.get_mut(&control) {
        c.kind = ControlKind::Knob { sweep_degrees: sweep };
    }
}

/// Detent spacing for an `n`-position switch: ≤ 60° a click, ≤ 300° in all.
#[must_use]
pub fn detent_sweep(n: usize) -> f64 {
    if n < 2 {
        return 0.0;
    }
    let step = (300.0 / (n - 1) as f64).min(60.0);
    step * (n - 1) as f64
}

/// Angle of position `i` of `n` over `sweep` (0 = up, clockwise).
#[must_use]
pub fn detent_angle(i: usize, n: usize, sweep: f64) -> f64 {
    if n < 2 {
        return 0.0;
    }
    -sweep / 2.0 + sweep * i as f64 / (n - 1) as f64
}

/// Add a live value readout (`Value` slot) under a placed knob's legend,
/// growing the control's box to hold it.
pub fn add_value(b: &mut Builder<'_>, control: NodeId, size: f64, color: &str) -> NodeId {
    let (w, h) = b.scene.node(control).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let mut t = text("value", "", 0.0, h + size * 0.72, w, size, color, TextAlign::Center, 500.0, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.02;
    }
    let id = b.add(control, t);
    if let Some(n) = b.scene.node_mut(control) {
        n.size.height = h + (size * 1.5).ceil();
    }
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.push(surface_proto::Slot { role: SlotRole::Value, node: id.0 });
    }
    id
}

// ── silkscreen ───────────────────────────────────────────────────────────

/// A text node whose box is `w` wide from `x`, vertically centred on `cy`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn text(name: &str, s: &str, x: f64, cy: f64, w: f64, size: f64, color: &str, align: TextAlign, weight: f64, upper: bool) -> Node {
    let mut n = draw::text(name, s, x, cy - size * 0.68, w, size, color, align);
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(weight);
        if upper {
            t.text_case = TextCase::Upper;
            t.letter_spacing = size * 0.08;
        }
    }
    n
}

/// Rough width of `s` at `size`, uppercase with the silkscreen's tracking.
#[must_use]
pub fn text_w(s: &str, size: f64) -> f64 {
    s.chars().count() as f64 * size * 0.72
}

/// Silkscreen: uppercase, tracked, centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn silk(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("silk", s, cx - w / 2.0, cy, w, size, color, TextAlign::Center, weight, true))
}

/// Silkscreen set from its left edge.
#[allow(clippy::too_many_arguments)]
pub fn silk_left(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("silk", s, x, cy, w, size, color, TextAlign::Left, weight, true))
}

/// Silkscreen set to end at `x`.
#[allow(clippy::too_many_arguments)]
pub fn silk_right(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("silk", s, x - w, cy, w, size, color, TextAlign::Right, weight, true))
}

/// Mixed-case text (a model line in script-less caps is too loud).
#[allow(clippy::too_many_arguments)]
pub fn plain(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("print", s, cx - w / 2.0, cy, w, size, color, TextAlign::Center, weight, false))
}

/// A hairline rule.
pub fn rule(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, w: f64, h: f64, color: &str) -> NodeId {
    b.add(parent, draw::rect("rule", x, y, w, h, draw::solid(color)))
}

/// A rounded outline box (section frame) with an optional title set into
/// its top edge on a patch of `panel` colour.
#[allow(clippy::too_many_arguments)]
pub fn section(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, w: f64, h: f64, color: &str, weight: f64) -> NodeId {
    let n = draw::stroked(draw::rounded(draw::frame("section", x, y, w, h), 6.0), color, weight);
    b.add(parent, n)
}

/// Detent dots and legends printed around a shaft at (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn detents(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r_dot: f64, r_lbl: f64, labels: &[&str], sweep: f64, size: f64, color: &str) {
    let n = labels.len();
    for (i, l) in labels.iter().enumerate() {
        let a = detent_angle(i, n, sweep);
        let (dx, dy) = draw::polar(cx, cy, r_dot, a);
        b.add(parent, draw::circle("detent", dx, dy, (size * 0.16).max(1.1), draw::solid(color)));
        if !l.is_empty() {
            let (lx, ly) = draw::polar(cx, cy, r_lbl, a);
            let w = text_w(l, size) + size;
            b.add(parent, text("detent label", l, lx - w / 2.0, ly, w, size, color, TextAlign::Center, 700.0, false));
        }
    }
}

// ── lamps & windows ──────────────────────────────────────────────────────

fn hexa(c: &str, a: f64) -> String {
    let col = draw::with_alpha(draw::hex(c), a);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", q(col.r), q(col.g), q(col.b), q(col.a))
}

fn mixed(a: &str, b: &str, t: f64) -> String {
    let c = draw::mix(draw::hex(a), draw::hex(b), t);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b))
}

/// A lit lens: hot core, the colour, a dark rim, and its glow.
#[must_use]
pub fn lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let hot = [
        (0.0, "#ffffffee".to_string()),
        (0.2, mixed(color, "#ffffff", 0.55)),
        (0.55, color.to_string()),
        (1.0, mixed(color, "#000000", 0.5)),
    ];
    let stops: Vec<(f64, &str)> = hot.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::with_effects(draw::circle("lamp", cx, cy, r, draw::radial_at(0.38, 0.32, 0.9, &stops)), vec![draw::drop_shadow(&hexa(color, 0.8), 0.0, 0.0, r * 2.2)])
}

/// An unlit lens in its bezel.
#[must_use]
pub fn dark_lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let dark = [(0.0, "#ffffff22".to_string()), (0.5, mixed(color, "#000000", 0.62)), (1.0, "#000000d0".to_string())];
    let stops: Vec<(f64, &str)> = dark.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::with_effects(draw::circle("lens", cx, cy, r, draw::radial_at(0.38, 0.3, 0.95, &stops)), vec![draw::inner_shadow("#000000b0", 0.0, 0.0, r * 0.7)])
}

/// A panel lamp in a black bezel, lit or not (a pilot light, an
/// above/below-threshold pair). Decoration, not a control.
pub fn lamp(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, d: f64, color: &str, lit: bool) {
    let r = d / 2.0;
    b.add(parent, draw::with_effects(draw::circle("bezel", cx, cy, r + (d * 0.16).max(1.2), draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    b.add(parent, dark_lens(cx, cy, r, color));
    if lit {
        b.add(parent, lens(cx, cy, r, color));
    }
}

/// A recessed readout window with a live `Value` slot — the one concession
/// to the screen: the number behind a printed scale.
#[allow(clippy::too_many_arguments)]
pub fn readout(b: &mut Builder<'_>, parent: NodeId, name: &str, binding: Binding, cx: f64, cy: f64, w: f64, h: f64, ink: &str) -> NodeId {
    let mut win = draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h);
    win.fills = vec![draw::linear(180.0, &[(0.0, "#07080a"), (1.0, "#121418")])];
    let win = draw::with_effects(draw::stroked(draw::rounded(win, 3.0), "#00000099", 1.0), vec![draw::inner_shadow("#000000c0", 0.0, 1.5, 3.0), draw::drop_shadow("#ffffff1c", 0.0, 1.0, 0.0)]);
    let root = b.add(parent, win);
    let size = (h * 0.52).round();
    let value = b.add(root, text("value", "", 0.0, h / 2.0, w, size, ink, TextAlign::Center, 600.0, false));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    b.controls.insert(root, c);
    root
}

// ── LED meters ───────────────────────────────────────────────────────────

/// Colour of an LED at `db` on a level meter: green, amber from -6, red
/// from 0 — or all one colour.
fn level_color(db: f64) -> &'static str {
    if db >= 0.0 {
        "#ff3b2f"
    } else if db >= -6.0 {
        "#ffb21e"
    } else {
        "#46e05a"
    }
}

/// A vertical LED bar meter: `n` segments over `floor..ceiling` dB, lit
/// from the bottom by a stream. It is a `Gauge` (whose range and curve are
/// the binding's) whose moving part is a dark overlay sliding off the lit
/// column as the level rises — a `Thumb` in a lane twice the column's
/// height, clipped to the column. The `Needle` slot points at the static
/// column with no sweep, so nothing turns.
#[allow(clippy::too_many_arguments)]
pub fn led_column(b: &mut Builder<'_>, parent: NodeId, name: &str, stream: &str, channel: u8, x: f64, y: f64, w: f64, h: f64, n: usize, floor: f64, ceiling: f64) -> NodeId {
    let mut housing = draw::frame(name, x, y, w, h);
    housing.fills = vec![draw::solid("#070809")];
    let housing = draw::with_effects(draw::stroked(draw::rounded(housing, 2.0), "#000000", 1.0), vec![draw::inner_shadow("#000000", 0.0, 1.0, 3.0), draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0)]);
    let root = b.add(parent, housing);
    let pad = 2.0;
    let (cw, ch) = (w - 2.0 * pad, h - 2.0 * pad);
    let mut col = draw::frame("column", pad, pad, cw, ch);
    col.clips_content = true;
    let col = b.add(root, col);
    let pitch = ch / n as f64;
    for i in 0..n {
        // Segment i from the bottom, its colour the dB at its top edge.
        let db = floor + (ceiling - floor) * (i as f64 + 1.0) / n as f64;
        let c = level_color(db - 0.01);
        let sy = ch - (i as f64 + 1.0) * pitch + pitch * 0.18;
        let seg = draw::rounded(draw::rect("segment", 0.0, sy, cw, pitch * 0.64, draw::linear(90.0, &[(0.0, &mixed(c, "#000000", 0.25)), (0.5, &mixed(c, "#ffffff", 0.18)), (1.0, &mixed(c, "#000000", 0.25))])), 1.0);
        b.add(col, seg);
    }
    let lane = b.add(col, draw::frame("lane", 0.0, -ch, cw, 2.0 * ch));
    let mask = b.add(lane, draw::rect("unlit", 0.0, 0.0, cw, ch, draw::solid("#060708d9")));
    let mut c = Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: floor, ceiling_db: ceiling })
        .slot(SlotRole::Needle, col.0)
        .slot(SlotRole::Thumb, mask.0);
    c.binding = Some(Binding { source: surface_proto::Source::Stream { id: EntryId::new(stream), channel: Some(channel) }, ..Binding::stream(stream) });
    b.controls.insert(root, c);
    root
}

/// A horizontal ladder of lamps that light one by one as gain reduction
/// deepens — the Distressor's. `steps` are the dB each lamp stands for, left
/// to right (deepest first); the rightmost lights first.
///
/// A `Gauge` over `-deepest..0` whose curve maps each printed step to its
/// lamp, so lamps are evenly pitched however uneven their dB. Each lamp has
/// its own dark cover — a `Thumb` in a tall lane, clipped to the lamp — that
/// slides up and away once the reduction reaches that lamp's step.
#[allow(clippy::too_many_arguments)]
pub fn gr_ladder(b: &mut Builder<'_>, parent: NodeId, name: &str, stream: &str, x: f64, y: f64, pitch: f64, d: f64, steps: &[f64], color: &str) -> NodeId {
    let n = steps.len();
    let deepest = steps.iter().copied().fold(0.0, f64::max);
    let w = pitch * n as f64;
    let root = b.add(parent, draw::frame(name, x, y, w, d + 4.0));
    let lamps = b.add(root, draw::frame("lamps", 0.0, 0.0, w, d + 4.0));
    let mut c = Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: -deepest, ceiling_db: 0.0 }).slot(SlotRole::Needle, lamps.0);
    // Lamp k (counting from the right, 1-based) is fully lit at position
    // k/n; the covers ramp over the step before it.
    let span = 1.0 / n as f64;
    let bx = d + 1.0;
    let lane_h = 2.0 * bx / span;
    for (i, _) in steps.iter().enumerate() {
        let k = (n - i) as f64;
        let cx = pitch * (i as f64 + 0.5);
        let cy = d / 2.0 + 2.0;
        b.add(lamps, draw::circle("bezel", cx, cy, d / 2.0 + 1.5, draw::solid("#050506")));
        let mut clip = draw::frame("lamp clip", cx - bx / 2.0, cy - bx / 2.0, bx, bx);
        clip.clips_content = true;
        clip.corner_radii = document_proto::CornerRadii::uniform(bx / 2.0);
        let clip = b.add(lamps, clip);
        b.add(clip, lens(bx / 2.0, bx / 2.0, d / 2.0, color));
        // Cover: height S (the slide), lane 2S. Its bottom sits at
        // S(lit_at - p): over the whole lamp while p <= (k-1)/n, clear of it
        // (above) once p >= k/n.
        let s = lane_h / 2.0;
        let lit_at = k * span;
        let y_off = -s - s * (1.0 - lit_at);
        let lane = b.add(clip, draw::frame("lane", 0.0, y_off, bx, lane_h));
        let cover = b.add(lane, draw::rect("cover", 0.0, 0.0, bx, s, draw::solid("#070809d6")));
        c = c.slot(SlotRole::Thumb, cover.0);
    }
    // Position = lamp index: each printed step lands on its lamp.
    let mut pts: Vec<CurvePoint> = vec![CurvePoint { x: 1.0, y: 0.0 }];
    for (i, db) in steps.iter().enumerate() {
        let k = (n - i) as f64;
        pts.push(CurvePoint { x: 1.0 - db / deepest, y: k / n as f64 });
    }
    pts.sort_by(|a, b| a.x.total_cmp(&b.x));
    pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9);
    c.binding = Some(Binding { curve: Curve::Points { points: pts }, ..Binding::stream(stream) });
    b.controls.insert(root, c);
    root
}

/// The over/under-threshold lamp pair of a compressor: BELOW lit while the
/// unit is not reducing gain, ABOVE lit once it reduces by `on_db` or more.
/// One `Gauge` on the gain-reduction stream (inverted, so no signal reads as
/// no reduction) with a sliding cover per lamp, as [`gr_ladder`].
#[allow(clippy::too_many_arguments)]
pub fn threshold_lamps(b: &mut Builder<'_>, parent: NodeId, stream: &str, below: (f64, f64), above: (f64, f64), d: f64, colors: (&str, &str), on_db: f64) -> NodeId {
    let span_db = 20.0;
    let eps = on_db / span_db;
    let bx = d + 1.0;
    let (x0, y0) = (below.0.min(above.0) - bx / 2.0, below.1.min(above.1) - bx / 2.0);
    let (x1, y1) = (below.0.max(above.0) + bx / 2.0, below.1.max(above.1) + bx / 2.0);
    let root = b.add(parent, draw::frame("Threshold lamps", x0, y0, x1 - x0, y1 - y0));
    let lamps = b.add(root, draw::frame("lamps", 0.0, 0.0, x1 - x0, y1 - y0));
    let mut c = Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: -span_db, ceiling_db: 0.0 }).slot(SlotRole::Needle, lamps.0);
    for ((cx, cy), color, lit_at_rest) in [(below, colors.0, true), (above, colors.1, false)] {
        let (lx, ly) = (cx - x0, cy - y0);
        let bezel = draw::circle("bezel", lx, ly, d / 2.0 + (d * 0.16).max(1.2), draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")]));
        b.add(lamps, draw::with_effects(bezel, vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
        let mut clip = draw::frame("lamp clip", lx - bx / 2.0, ly - bx / 2.0, bx, bx);
        clip.clips_content = true;
        clip.corner_radii = document_proto::CornerRadii::uniform(bx / 2.0);
        let clip = b.add(lamps, clip);
        b.add(clip, dark_lens(bx / 2.0, bx / 2.0, d / 2.0, color));
        b.add(clip, lens(bx / 2.0, bx / 2.0, d / 2.0, color));
        // Cover height S, lane 2S; its top sits at y_off + S(1 - p).
        let s = bx / eps;
        let y_off = if lit_at_rest { bx - s } else { -s - s * (1.0 - eps) };
        let lane = b.add(clip, draw::frame("lane", 0.0, y_off, bx, 2.0 * s));
        let cover = b.add(lane, draw::rect("cover", 0.0, 0.0, bx, s, draw::solid("#0a0a0bd0")));
        c = c.slot(SlotRole::Thumb, cover.0);
    }
    c.binding = Some(Binding { curve: Curve::Inverted { inner: Box::new(Curve::Linear) }, ..Binding::stream(stream) });
    b.controls.insert(root, c);
    root
}

// ── catalog entries ──────────────────────────────────────────────────────

/// A continuous param with a real range; `default` in real units.
#[must_use]
pub fn param(addr: &str, name: &str, range: Range, default: f64) -> ParamInfo {
    ParamInfo::new(addr, name, range).with_default_real(default)
}

/// A dB param over `min..max`.
#[must_use]
pub fn db(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    let p = param(addr, name, Range::linear(min, max, Unit::Db), default);
    if min < 0.0 && max > 0.0 { p.bipolar() } else { p }
}

/// A dB param in whole `step`-dB clicks (the API's 2 dB gain).
#[must_use]
pub fn db_stepped(addr: &str, name: &str, min: f64, max: f64, step: f64, default: f64) -> ParamInfo {
    let mut r = Range::linear(min, max, Unit::Db);
    r.steps = Some(((max - min) / step).round() as u32 + 1);
    let p = param(addr, name, r, default);
    if min < 0.0 && max > 0.0 { p.bipolar() } else { p }
}

/// A time in ms swept logarithmically.
#[must_use]
pub fn ms(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    param(addr, name, Range::log(min, max, Unit::Milliseconds), default)
}

/// 0–100 %.
#[must_use]
pub fn percent(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(addr, name, Range::linear(0.0, 100.0, Unit::Percent), default)
}

/// A stepped selector over named positions; `default` is an index.
#[must_use]
pub fn choice(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).choice(options);
    let last = options.len().saturating_sub(1).max(1) as f64;
    p.default = default as f64 / last;
    p
}

/// An on/off param.
#[must_use]
pub fn toggle(addr: &str, name: &str, on: bool) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).toggle();
    p.default = if on { 1.0 } else { 0.0 };
    p
}

/// A gain-reduction stream.
#[must_use]
pub fn gr_stream(id: &str, name: &str) -> StreamInfo {
    StreamInfo { id: EntryId::new(id), name: name.into(), kind: StreamKind::GainReduction, rate_hz: 30.0 }
}

/// A level stream of `channels`.
#[must_use]
pub fn level_stream(id: &str, name: &str, channels: u8) -> StreamInfo {
    StreamInfo { id: EntryId::new(id), name: name.into(), kind: StreamKind::Level { channels }, rate_hz: 30.0 }
}

/// A param binding.
#[must_use]
pub fn bind(addr: &str) -> Binding {
    Binding::param(addr)
}

// ══ what the reverb and modulation faces add ═════════════════════════════

/// `#rrggbbaa` for `color` at alpha `a`.
#[must_use]
pub fn alpha(color: &str, a: f64) -> String {
    hexa(color, a)
}

/// `a` mixed toward `b` by `t`, as `#rrggbb`.
#[must_use]
pub fn blend(a: &str, b: &str, t: f64) -> String {
    mixed(a, b, t)
}

/// Make `n` give off light: an `emission` (the GPU renderer blooms it) plus
/// a soft drop shadow of the same colour, so it still glows where the
/// renderer ignores emission.
#[must_use]
pub fn glowing(mut n: Node, color: &str, intensity: f64, radius: f64) -> Node {
    n.emission = Some(document_proto::Emission { color: draw::hex(color), intensity, radius });
    // The software renderer shadows a vector's whole box, not its geometry:
    // vectors get their glow from emission alone (or a halo drawn under).
    if !matches!(n.kind, NodeKind::Vector) {
        n.effects.push(draw::drop_shadow(&hexa(color, (0.55 * intensity).min(0.9)), 0.0, 0.0, radius));
    }
    n
}

/// A ribbon `2 * hw` wide along the polyline `pts` (the renderer fills
/// geometry; a stroked curve is a run of quads overlapping at the joints).
#[must_use]
pub fn ribbon(pts: &[(f64, f64)], hw: f64) -> String {
    let mut d = String::new();
    for s in pts.windows(2) {
        let ((ax, ay), (bx, by)) = (s[0], s[1]);
        let len = (bx - ax).hypot(by - ay).max(1e-6);
        let (nx, ny) = (-(by - ay) / len * hw, (bx - ax) / len * hw);
        let (ex, ey) = ((bx - ax) / len * hw * 0.8, (by - ay) / len * hw * 0.8);
        d.push_str(&draw::polygon(&[(ax + nx, ay + ny), (bx + ex + nx, by + ey + ny), (bx + ex - nx, by + ey - ny), (ax - nx, ay - ny)]));
        d.push(' ');
    }
    d
}

/// A stroked polyline node, points in its parent's coordinates.
#[must_use]
pub fn line_node(name: &str, pts: &[(f64, f64)], weight: f64, color: &str) -> Node {
    shape(name, ribbon(pts, weight / 2.0), pts, weight, draw::solid(color))
}

/// A filled path whose geometry `d` is in its parent's coordinates, boxed
/// to the extent of `pts` grown by `pad` (the renderer culls by the box).
#[must_use]
pub fn shape(name: &str, d: String, pts: &[(f64, f64)], pad: f64, paint: document_proto::Paint) -> Node {
    let (mut x1, mut y1) = (0.0_f64, 0.0_f64);
    for (x, y) in pts {
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    draw::path(name, 0.0, 0.0, x1 + pad + 1.0, y1 + pad + 1.0, d, paint)
}

/// A filled path in parent coordinates spanning the parent's `w × h`.
#[must_use]
pub fn area(name: &str, d: String, w: f64, h: f64, paint: document_proto::Paint) -> Node {
    draw::path(name, 0.0, 0.0, w, h, d, paint)
}

/// A stroked polyline in `parent`.
pub fn line(b: &mut Builder<'_>, parent: NodeId, name: &str, pts: &[(f64, f64)], weight: f64, color: &str) -> NodeId {
    b.add(parent, line_node(name, pts, weight, color))
}

/// The outline of an ellipse as a closed polyline.
#[must_use]
pub fn ellipse_pts(cx: f64, cy: f64, rx: f64, ry: f64, n: usize) -> Vec<(f64, f64)> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

/// A rounded rectangle in `parent`.
#[allow(clippy::too_many_arguments)]
pub fn fill_rect(b: &mut Builder<'_>, parent: NodeId, name: &str, x: f64, y: f64, w: f64, h: f64, paint: document_proto::Paint, r: f64) -> NodeId {
    b.add(parent, draw::rounded(draw::rect(name, x, y, w, h, paint), r))
}

// ── displays ─────────────────────────────────────────────────────────────

/// A recessed glass display window (`x, y, w, h`) lit in `tint`: a bezel, a
/// near-black face with the faintest bloom of its phosphor. Returns the face
/// frame (clips its content) to draw into — its origin is the window's
/// top-left; finish with [`glass`].
///
/// `crt` adds the optional `crt` shader effect — the display reads the same
/// without it.
#[allow(clippy::too_many_arguments)]
pub fn display(b: &mut Builder<'_>, parent: NodeId, name: &str, x: f64, y: f64, w: f64, h: f64, tint: &str, crt: bool) -> NodeId {
    let bez = 5.0;
    let bezel = draw::rounded(draw::rect("bezel", x - bez, y - bez, w + 2.0 * bez, h + 2.0 * bez, draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1c1d20")])), 6.0);
    b.add(parent, draw::with_effects(bezel, vec![draw::drop_shadow("#ffffff1f", 0.0, 1.0, 0.0), draw::inner_shadow("#000000", 0.0, 2.0, 3.0)]));
    let mut face = draw::frame(name, x, y, w, h);
    face.fills = vec![draw::radial_at(0.5, 0.45, 0.75, &[(0.0, &mixed(tint, "#050607", 0.88)), (1.0, "#040505")])];
    face.clips_content = true;
    face.corner_radii = document_proto::CornerRadii::uniform(3.0);
    face.effects = vec![draw::inner_shadow("#000000e0", 0.0, 2.0, 8.0)];
    if crt {
        face.shader = Some(document_proto::ShaderFx { shader: "crt".into(), params: std::collections::BTreeMap::from([("strength".to_string(), 0.35)]) });
    }
    b.add(parent, face)
}

/// The glass over a [`display`], added last so it lies over what's drawn.
pub fn glass(b: &mut Builder<'_>, face: NodeId) {
    let (w, h) = b.scene.node(face).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let sheen = draw::linear(160.0, &[(0.0, "#ffffff1a"), (0.34, "#ffffff08"), (0.35, "#ffffff00"), (1.0, "#ffffff00")]);
    b.add(face, draw::rect("glass", 0.0, 0.0, w, h, sheen));
}

/// Text set in a display: glowing phosphor, mixed case.
#[allow(clippy::too_many_arguments)]
pub fn lit_text(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, w: f64, size: f64, color: &str, align: TextAlign, weight: f64) -> NodeId {
    let mut t = text("lit", s, x, cy, w, size, color, align, weight, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.06;
    }
    b.add(parent, glowing(t, color, 0.7, size * 0.3))
}

/// A glowing live value in a display: a `Readout` whose `Value` text is lit
/// in `color`, in a box `w` wide from `x`, centred on `cy`.
#[allow(clippy::too_many_arguments)]
pub fn lit_readout(b: &mut Builder<'_>, parent: NodeId, name: &str, binding: Binding, x: f64, cy: f64, w: f64, size: f64, color: &str, align: TextAlign) -> NodeId {
    let h = (size * 1.5).ceil();
    let root = b.add(parent, draw::frame(name, x, cy - h / 2.0, w, h));
    let mut t = text("value", "", 0.0, h / 2.0, w, size, color, align, 600.0, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.04;
    }
    let value = b.add(root, glowing(t, color, 0.7, size * 0.3));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    b.controls.insert(root, c);
    root
}

// ── lamps on streams ─────────────────────────────────────────────────────

/// A row of `n` lamps (`w × h` each, `pitch` apart, from `x`) that light
/// left to right as a level stream rises over `floor..ceiling` dB — a
/// `Gauge` whose lamps each have a sliding cover (as [`gr_ladder`]).
/// `colors(i)` paints lamp `i`.
#[allow(clippy::too_many_arguments)]
pub fn lamp_row(b: &mut Builder<'_>, parent: NodeId, name: &str, stream: &str, x: f64, y: f64, n: usize, pitch: f64, w: f64, h: f64, floor: f64, ceiling: f64, colors: &dyn Fn(usize) -> String) -> NodeId {
    let total = pitch * (n - 1) as f64 + w;
    let root = b.add(parent, draw::frame(name, x, y, total, h));
    let lamps = b.add(root, draw::frame("lamps", 0.0, 0.0, total, h));
    let mut c = Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: floor, ceiling_db: ceiling }).slot(SlotRole::Needle, lamps.0);
    // Each cover is S tall in a 2S lane, so its top sits at y_off + S(1 - p):
    // over the lamp while p <= (k-1)/n, above and clear of it once p >= k/n.
    let span = 1.0 / n as f64;
    let s = h / span;
    for i in 0..n {
        let k = (i + 1) as f64;
        let lx = pitch * i as f64;
        let col = colors(i);
        b.add(lamps, draw::rounded(draw::rect("seat", lx - 1.0, -1.0, w + 2.0, h + 2.0, draw::solid("#030304")), 2.5));
        let mut clip = draw::frame("lamp clip", lx, 0.0, w, h);
        clip.clips_content = true;
        clip.corner_radii = document_proto::CornerRadii::uniform(1.5);
        let clip = b.add(lamps, clip);
        let lit = draw::rect("lit", 0.0, 0.0, w, h, draw::linear(180.0, &[(0.0, &mixed(&col, "#ffffff", 0.35)), (0.5, &col), (1.0, &mixed(&col, "#000000", 0.2))]));
        b.add(clip, glowing(lit, &col, 0.9, h * 0.6));
        let lit_at = k * span;
        let y_off = -s * (2.0 - lit_at);
        let lane = b.add(clip, draw::frame("lane", 0.0, y_off, w, 2.0 * s));
        let cover = b.add(lane, draw::rect("cover", 0.0, 0.0, w, s, draw::solid(&mixed(&col, "#050506", 0.84))));
        c = c.slot(SlotRole::Thumb, cover.0);
    }
    c.binding = Some(Binding::stream(stream));
    b.controls.insert(root, c);
    root
}

/// The LFO rate lamp: a lens that brightens and dims with the LFO. A
/// `Readout` on a scalar `0..1` stream: a graded cover slides off the lens
/// as the LFO rises (`Thumb`), and the halo flashes on the upper half of
/// the cycle (`Led`). The readout's `Value` text is kept but never shown —
/// the lamp *is* the readout (the kit has no lamp control for a scalar).
#[allow(clippy::too_many_arguments)]
pub fn lfo_lamp(b: &mut Builder<'_>, parent: NodeId, name: &str, stream: &str, cx: f64, cy: f64, d: f64, color: &str) -> NodeId {
    let r = d / 2.0;
    let rim = (d * 0.22).max(2.0);
    let side = d + 2.0 * rim;
    let root = b.add(parent, draw::frame(name, cx - side / 2.0, cy - side / 2.0, side, side));
    let c0 = side / 2.0;
    let halo = draw::circle("halo", c0, c0, r * 1.9, draw::radial(&[(0.0, &hexa(color, 0.5)), (0.5, &hexa(color, 0.16)), (1.0, &hexa(color, 0.0))]));
    let halo = b.add(root, glowing(halo, color, 0.5, d));
    let bezel = draw::circle("bezel", c0, c0, r + rim, draw::conic(20.0, &[(0.0, "#f0f0ec"), (0.25, "#7d7e7b"), (0.5, "#e2e2de"), (0.75, "#6c6d6a"), (1.0, "#f0f0ec")]));
    b.add(root, draw::with_effects(bezel, vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 3.0)]));
    b.add(root, draw::circle("seat", c0, c0, r + rim * 0.35, draw::solid("#0b0b0c")));
    let mut clip = draw::frame("lens clip", c0 - r, c0 - r, d, d);
    clip.clips_content = true;
    clip.corner_radii = document_proto::CornerRadii::uniform(r);
    let clip = b.add(root, clip);
    b.add(clip, glowing(lens(r, r, r, color), color, 1.0, d * 0.8));
    // A graded cover C tall: opaque at its top, clear at its bottom. It
    // slides (C - d), so the lens sees its top at p = 0, its bottom at p = 1.
    let cover_h = d * 4.0;
    let travel = cover_h - d;
    let lane = b.add(clip, draw::frame("lane", 0.0, -travel, d, cover_h + travel));
    let dark = mixed(color, "#000000", 0.82);
    let cover = b.add(lane, draw::rect("cover", 0.0, 0.0, d, cover_h, draw::linear(180.0, &[(0.0, &hexa(&dark, 0.96)), (0.3, &hexa(&dark, 0.9)), (0.85, &hexa(&dark, 0.0)), (1.0, &hexa(&dark, 0.0))])));
    b.add(clip, draw::circle("glint", r * 0.7, r * 0.6, r * 0.28, draw::solid("#ffffff55")));
    let mut value = text("value", "", 0.0, side / 2.0, side, 1.0, color, TextAlign::Center, 400.0, false);
    value.opacity = 0.0;
    let value = b.add(root, value);
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0).slot(SlotRole::Thumb, cover.0).slot(SlotRole::Led, halo.0);
    c.binding = Some(Binding::stream(stream));
    b.controls.insert(root, c);
    root
}

/// An LFO stream: the modulator's output, 0..1, at a lamp's frame rate.
#[must_use]
pub fn lfo_stream(id: &str, name: &str) -> StreamInfo {
    StreamInfo { id: EntryId::new(id), name: name.into(), kind: StreamKind::Scalar { range: Range::linear(0.0, 1.0, Unit::None) }, rate_hz: 60.0 }
}

// ── stompbox parts ───────────────────────────────────────────────────────

/// A stompbox footswitch: a chrome, domed actuator in its hex nut, with the
/// status LED above it — one `Footswitch` control on a toggle param whose
/// `Led` is that lamp. `d` is the cap's diameter; (`cx`, `cy`) its centre.
#[allow(clippy::too_many_arguments)]
pub fn footswitch(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, cx: f64, cy: f64, d: f64, led: &str) -> NodeId {
    let nut = d * 0.78;
    let led_r = d * 0.1;
    let led_gap = d * 1.05;
    let w = nut * 2.0 + 8.0;
    let top = led_gap + led_r * 2.0 + 4.0;
    let h = top + nut + 6.0;
    let root = b.add(parent, draw::frame(name, cx - w / 2.0, cy - top, w, h));
    let (x0, y0) = (w / 2.0, top);
    // Status LED in a black bezel.
    let ly = y0 - led_gap;
    b.add(root, draw::circle("led bezel", x0, ly, led_r + 2.4, draw::linear(180.0, &[(0.0, "#1a1a1c"), (1.0, "#3a3a3e")])));
    b.add(root, dark_lens(x0, ly, led_r, led));
    let lit = b.add(root, glowing(lens(x0, ly, led_r, led), led, 1.0, led_r * 3.0));
    // Nut, washer, barrel, domed actuator.
    b.add(root, draw::circle("shadow", x0, y0 + 3.0, nut * 0.98, draw::radial(&[(0.0, "#00000080"), (0.8, "#00000050"), (1.0, "#00000000")])));
    let hexnut = draw::path("nut", 0.0, 0.0, w, h, draw::regular(x0, y0, nut, 6, 30.0), draw::linear(160.0, &[(0.0, "#f4f4f1"), (0.45, "#9a9b98"), (0.55, "#6f706d"), (1.0, "#d6d6d2")]));
    b.add(root, hexnut);
    b.add(root, draw::circle("washer", x0, y0, nut * 0.8, draw::conic(0.0, &[(0.0, "#e9e9e5"), (0.2, "#8c8d8a"), (0.45, "#f2f2ee"), (0.7, "#7a7b78"), (1.0, "#e9e9e5")])));
    b.add(root, draw::circle("barrel", x0, y0, d / 2.0 + 3.0, draw::solid("#2b2b2d")));
    let cap = draw::circle("cap", x0, y0, d / 2.0, draw::radial_at(0.38, 0.32, 0.85, &[(0.0, "#ffffff"), (0.25, "#d9dad7"), (0.6, "#8e8f8c"), (1.0, "#4a4b49")]));
    b.add(root, draw::with_effects(cap, vec![draw::inner_shadow("#00000066", 0.0, -2.0, 4.0)]));
    b.add(root, draw::circle("spec", x0 - d * 0.16, y0 - d * 0.18, d * 0.12, draw::solid("#ffffffbb")));
    let mut c = Control::new(ControlKind::footswitch()).slot(SlotRole::Led, lit.0);
    c.binding = Some(Binding::param(addr));
    b.controls.insert(root, c);
    root
}

/// A die-cast stompbox enclosure `w × h` filling `root`, painted `paint`
/// (top, mid, bottom): a dark cast edge, the painted lid with a satin
/// sheen, four screws. Returns the lid (its origin is inset by the edge).
pub fn enclosure(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64, paint: [&str; 3], screws: &str) -> NodeId {
    fit_root(b, root, w, h);
    let mut body = draw::frame("Enclosure", 0.0, 0.0, w, h);
    body.fills = vec![draw::linear(180.0, &[(0.0, &mixed(paint[1], "#000000", 0.35)), (1.0, &mixed(paint[2], "#000000", 0.6))])];
    body.corner_radii = document_proto::CornerRadii::uniform(22.0);
    let body = b.add(root, body);
    let e = EDGE;
    let mut lid = draw::frame("Lid", e, e, w - 2.0 * e, h - 2.0 * e);
    lid.fills = vec![
        draw::linear(172.0, &[(0.0, paint[0]), (0.5, paint[1]), (1.0, paint[2])]),
        draw::linear(115.0, &[(0.0, "#ffffff00"), (0.42, "#ffffff00"), (0.5, "#ffffff1a"), (0.58, "#ffffff00"), (1.0, "#ffffff00")]),
    ];
    lid.corner_radii = document_proto::CornerRadii::uniform(15.0);
    lid.effects = vec![draw::inner_shadow("#ffffff40", 0.0, 1.5, 1.0), draw::inner_shadow("#00000055", 0.0, -3.0, 6.0), draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)];
    let lid = b.add(body, lid);
    let (lw, lh) = (w - 2.0 * e, h - 2.0 * e);
    for (sx, sy) in [(13.0, 13.0), (lw - 13.0, 13.0), (13.0, lh - 13.0), (lw - 13.0, lh - 13.0)] {
        let head = draw::circle("screw", sx, sy, 4.2, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#f4f4f0"), (1.0, screws)]));
        b.add(lid, draw::with_effects(head, vec![draw::inner_shadow("#00000080", 0.0, 0.0, 1.5)]));
        b.add(lid, draw::rotated(draw::rect("slot", sx - 2.6, sy - 0.5, 5.2, 1.0, draw::solid("#0000008c")), 40.0));
    }
    lid
}

/// Print `labels` under a placed bank's keys (evenly across its box), at
/// `cy`: the bank's own caps then carry numerals or nothing.
#[allow(clippy::too_many_arguments)]
pub fn bank_labels(b: &mut Builder<'_>, parent: NodeId, bank: NodeId, labels: &[&str], cy: f64, size: f64, color: &str) {
    let (bx, bw) = b.scene.node(bank).map_or((0.0, 0.0), |n| (n.transform.tx, n.size.width));
    let n = labels.len() as f64;
    for (i, l) in labels.iter().enumerate() {
        silk(b, parent, l, bx + bw * (i as f64 + 0.5) / n, cy, size, color, 800.0);
    }
}

/// Re-ink every text under `root` (a placed component's printed legends)
/// in `color` — the kit prints light ink only.
pub fn reink(b: &mut Builder<'_>, root: NodeId, color: &str) {
    for id in b.scene.walk(root) {
        if let Some(n) = b.scene.node_mut(id)
            && matches!(n.kind, NodeKind::Text(_))
        {
            n.fills = vec![draw::solid(color)];
        }
    }
}

/// Repaint a kit panel in paint the kit doesn't carry (an indigo, a
/// blue-black): its fill becomes `stops`, top to bottom; its finish stays.
pub fn repaint(b: &mut Builder<'_>, panel: NodeId, stops: &[(f64, &str)]) {
    if let Some(n) = b.scene.node_mut(panel) {
        n.fills = vec![draw::linear(178.0, stops)];
    }
}

/// The cast edge round an [`enclosure`]'s lid.
pub const EDGE: f64 = 9.0;

// ── knobs in any ink ─────────────────────────────────────────────────────

/// How a knob is printed.
#[derive(Clone, Copy)]
pub struct Dial<'a> {
    /// Kit id (`knob.metal`) and variant.
    pub style: &'a str,
    pub variant: &'a str,
    pub d: f64,
    /// Scale labels printed round it (empty = none).
    pub scale: &'a [&'a str],
    pub ink: &'a str,
    pub dim: &'a str,
    /// Show the live value under the dial.
    pub value: bool,
    /// Legend size (0 = the face prints its own).
    pub legend: f64,
    /// Legend centre line, absolute (0 = just under the dial).
    pub legend_y: f64,
    /// Travel in degrees (a stepped selector turns through its detents).
    pub sweep: f64,
}

impl<'a> Dial<'a> {
    #[must_use]
    pub const fn new(style: &'a str, variant: &'a str, d: f64, ink: &'a str, dim: &'a str) -> Self {
        Self { style, variant, d, scale: &[], ink, dim, value: true, legend: 9.0, legend_y: 0.0, sweep: 300.0 }
    }
    #[must_use]
    pub const fn legend_at(mut self, y: f64) -> Self {
        self.legend_y = y;
        self
    }
    #[must_use]
    pub const fn sweep(mut self, deg: f64) -> Self {
        self.sweep = deg;
        self
    }
    #[must_use]
    pub const fn d(mut self, d: f64) -> Self {
        self.d = d;
        self
    }
    #[must_use]
    pub const fn scale(mut self, s: &'a [&'a str]) -> Self {
        self.scale = s;
        self
    }
    #[must_use]
    pub const fn no_value(mut self) -> Self {
        self.value = false;
        self
    }
    #[must_use]
    pub const fn legend(mut self, size: f64) -> Self {
        self.legend = size;
        self
    }
}

/// Place a kit knob with its shaft at (`cx`, `cy`), take the kit's light
/// print off, and print its scale, legend (below) and live value (just
/// under the dial) in the face's own inks. The control's box is the dial
/// plus the value line.
pub fn dial(b: &mut Builder<'_>, parent: NodeId, addr: &str, legend: &str, cx: f64, cy: f64, s: &Dial<'_>) -> NodeId {
    let p = Place::new(legend).size(s.d).variant(s.variant).bind(bind(addr)).scale(&[""]);
    let k = put(b, parent, s.style, &p, cx, cy);
    strip_legend(b, k);
    strip_scale(b, k);
    // The box is the dial, but never under a fingertip (44 px).
    tighten(b, k, s.d.max(44.0));
    if !s.scale.is_empty() {
        let size = (s.d * 0.13).clamp(7.0, 9.0);
        print_scale(b, parent, cx, cy, s.d / 2.0 + (s.d * 0.1).max(5.0), s.scale, s.sweep, size, s.ink);
    }
    if (s.sweep - 300.0).abs() > f64::EPSILON {
        set_sweep(b, k, s.sweep);
    }
    if s.value {
        add_value(b, k, (s.d * 0.14).clamp(7.5, 9.0), s.dim);
    }
    if s.legend > 0.0 {
        let below = if s.value { s.d / 2.0 + 24.0 } else { s.d / 2.0 + 14.0 };
        let y = if s.legend_y > 0.0 { s.legend_y } else { cy + below + s.legend * 0.4 };
        silk(b, parent, legend, cx, y, s.legend, s.ink, 800.0);
    }
    k
}

