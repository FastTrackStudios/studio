//! Panel-building helpers shared by the VCA-compressor and EQ faces: placing
//! kit components by the centre of their shaft, silkscreen, lamps, readout
//! windows, LED meters, and the catalog entries the faces bind.
//!
//! Everything is plain frame nodes plus `surface_proto::Control`s, the same
//! as the kit; these only save each face from repeating the arithmetic.

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

