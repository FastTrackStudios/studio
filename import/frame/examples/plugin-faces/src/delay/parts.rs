//! Panel-building helpers for the delay and saturation faces: placing kit
//! components by their shaft, taking their printed parts apart, silkscreen,
//! lamps and lit windows, LED meters, and the catalog entries every face
//! declares.
//!
//! Gathered from the compressor and EQ faces (`comp_vca::ui`,
//! `comp_tube::ui`) — placing by shaft, `tighten`, `unfold_needle`, `reink`,
//! the LED column built from a `Gauge` — plus what these units need that
//! those didn't: lit windows that *emit* (a display's phosphor, a lamp's
//! lens), the note-division table, and time params.

use document_proto::{Color, Emission, Node, NodeId, NodeKind, Paint, TextAlign, TextCase, TextData, Transform};
use surface_kit::{Place, draw};
use surface_proto::{
    Binding, ButtonMode, Control, ControlKind, EntryId, ParamInfo, Range, SlotRole, StreamInfo, StreamKind, Unit,
};
use surface_widgets::Builder;

// ── placing ──────────────────────────────────────────────────────────────

/// Move `id` so its top-left corner is at (`x`, `y`) in its parent.
pub fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

#[must_use]
pub fn size_of(b: &Builder<'_>, id: NodeId) -> (f64, f64) {
    b.scene.node(id).map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
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

/// Lay a straight top-to-bottom paint under everything on a kit panel.
///
/// The painted panels' own paint runs at a slight angle, and at the top
/// edge the renderer shows a dark band where the gradient starts; a square
/// coat underneath the ears/cheeks and the rest hides it.
pub fn underpaint(b: &mut Builder<'_>, panel: NodeId, stops: &[(f64, &str)]) {
    let (w, h) = size_of(b, panel);
    let coat = draw::rect("paint", 0.0, 0.0, w, h, draw::linear(180.0, stops));
    let _ = b.scene.apply(document_proto::Op::Insert { node: coat, parent: panel, index: 0 });
}

/// Repaint a node with `stops` top to bottom.
pub fn repaint(b: &mut Builder<'_>, id: NodeId, stops: &[(f64, &str)]) {
    if let Some(n) = b.scene.node_mut(id) {
        n.fills = vec![draw::linear(180.0, stops)];
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

/// Take a placed knob's own printed scale off — the face prints its own.
pub fn strip_scale(b: &mut Builder<'_>, knob: NodeId) {
    let ids = children_named(b, knob, &["scale dots", "scale arc", "scale majors", "scale minors", "numeral"]);
    remove(b, ids);
}

/// Take a placed control's legend (and its `Label` slot) off: the face
/// prints the legend itself.
pub fn strip_legend(b: &mut Builder<'_>, control: NodeId) {
    let ids: Vec<NodeId> = b.controls.get(&control).map(|c| c.slots_of(SlotRole::Label).map(NodeId).collect()).unwrap_or_default();
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
    }
    remove(b, ids);
    let dial = b.scene.node(control).and_then(|n| n.children.iter().find_map(|c| b.scene.node(*c).filter(|k| k.name == "knob").map(|k| k.size.height)));
    if let (Some(h), Some(n)) = (dial, b.scene.node_mut(control)) {
        n.size.height = h;
    }
}

/// Shrink a placed knob's box to its body (`d` across), keeping the shaft
/// where it is: the kit's box includes the scale margin and legend, too big
/// a touch target where controls sit close.
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
    // Anything else in the box (a legend) follows the knob.
    let others: Vec<NodeId> = b.scene.node(control).map(|n| n.children.iter().copied().filter(|c| *c != kid).collect()).unwrap_or_default();
    for o in others {
        if let Some(n) = b.scene.node_mut(o) {
            n.transform.tx -= inset;
            n.transform.ty -= inset;
        }
    }
}

/// Make a placed control's turning *frame* turn properly: its parts become
/// the `Needle`s instead (the chicken-head selector's pointer frame; the
/// renderer moves a turned frame's children without turning them).
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

/// Change a knob control's sweep (a stepped selector turns through exactly
/// its detents).
pub fn set_sweep(b: &mut Builder<'_>, control: NodeId, sweep: f64) {
    if let Some(c) = b.controls.get_mut(&control) {
        c.kind = ControlKind::Knob { sweep_degrees: sweep };
    }
}

/// Set every legend a placed button (or bank) prints at `size`: the kit
/// scales a cap's legend with the cap, so a long word ("Shimmer",
/// "Spectral") runs off a cap sized to sit in a row.
pub fn fit_legends(b: &mut Builder<'_>, control: NodeId, size: f64) {
    for id in b.scene.walk(control) {
        if let Some(n) = b.scene.node_mut(id)
            && let NodeKind::Text(t) = &mut n.kind
        {
            let dy = (t.font_size - size) * 0.68;
            t.font_size = size;
            t.letter_spacing = size * 0.06;
            n.transform.ty += dy;
        }
    }
}

/// Make a placed button momentary (a TAP key: on while held).
pub fn momentary(b: &mut Builder<'_>, control: NodeId) {
    if let Some(c) = b.controls.get_mut(&control) {
        c.kind = ControlKind::Button { mode: ButtonMode::Momentary };
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

/// Recolour every printed part of a placed component — text, scale ticks,
/// dots, detents — for the panel it sits on (the kit prints light ink).
pub fn reink(b: &mut Builder<'_>, root: NodeId, color: &str) {
    for id in b.scene.walk(root) {
        let Some(n) = b.scene.node_mut(id) else { continue };
        let printed = n.text().is_some() || n.name.starts_with("scale ") || n.name == "detent" || n.name == "numeral";
        if printed {
            n.fills = vec![draw::solid(color)];
        }
    }
}

/// Restyle a component's caption (its `Label` text).
pub fn caption_style(b: &mut Builder<'_>, root: NodeId, ink: Ink<'_>) {
    let label = b.controls.get(&root).and_then(|c| c.slots_of(SlotRole::Label).next());
    let Some(label) = label else { return };
    if let Some(n) = b.scene.node_mut(NodeId(label))
        && let NodeKind::Text(t) = &mut n.kind
    {
        let grow = ink.size / t.font_size;
        t.font_size = ink.size;
        t.font_weight = Some(ink.weight);
        t.letter_spacing = ink.size * ink.track;
        n.size.height = (n.size.height * grow).ceil();
        n.fills = vec![draw::solid(ink.color)];
    }
}

/// Add a live value readout (`Value` slot) under a placed knob's box,
/// growing the control's box to hold it.
pub fn add_value(b: &mut Builder<'_>, control: NodeId, size: f64, color: &str) -> NodeId {
    let (w, h) = size_of(b, control);
    let mut t = text_node("", 0.0, h + size * 0.72, w, &Ink::new(size, color).weight(600.0).track(0.02).plain(), TextAlign::Center);
    t.name = "value".into();
    let id = b.add(control, t);
    if let Some(n) = b.scene.node_mut(control) {
        n.size.height = h + (size * 1.5).ceil();
    }
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.push(surface_proto::Slot { role: SlotRole::Value, node: id.0 });
    }
    id
}

/// A knob as these faces use it: kit style `id` of diameter `d` with its
/// shaft at (`cx`, `cy`), its own legend and printed scale off, the box
/// tightened to the body, and the legend set by the face under it at
/// `legend_dy` below the shaft — so a row of knobs of different sizes still
/// has one baseline.
#[allow(clippy::too_many_arguments)]
pub fn knob(b: &mut Builder<'_>, parent: NodeId, id: &str, variant: &str, addr: &str, legend: &str, (cx, cy): (f64, f64), d: f64) -> NodeId {
    let mut p = Place::new(legend).size(d).bind(bind(addr)).legend(legend);
    if !variant.is_empty() {
        p = p.variant(variant);
    }
    let k = put(b, parent, id, &p, cx, cy);
    strip_legend(b, k);
    strip_scale(b, k);
    tighten(b, k, d);
    k
}

/// A placed knob's sweep in degrees.
#[must_use]
pub fn sweep_of(b: &Builder<'_>, control: NodeId) -> f64 {
    match b.controls.get(&control).map(|c| &c.kind) {
        Some(ControlKind::Knob { sweep_degrees }) => *sweep_degrees,
        _ => 300.0,
    }
}

/// A live value line (`Value` slot) for a placed control, centred under it
/// at panel `cy` — outside the control's box, so the box stays the body.
pub fn value_at(b: &mut Builder<'_>, control: NodeId, cy: f64, ink: Ink<'_>) -> NodeId {
    let ty = b.scene.node(control).map_or(0.0, |n| n.transform.ty);
    let (w, _) = size_of(b, control);
    let bw = 90.0;
    let mut t = text_node("", w / 2.0 - bw / 2.0, cy - ty, bw, &ink, TextAlign::Center);
    t.name = "value".into();
    let id = b.add(control, t);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.push(surface_proto::Slot { role: SlotRole::Value, node: id.0 });
    }
    id
}

/// The printed legend of a placed control, centred under it at panel
/// `cy`, as the control's `Label` slot — so the control is labelled (for
/// lint, for a screen reader) by the word printed on the panel.
pub fn legend_at(b: &mut Builder<'_>, control: NodeId, s: &str, cy: f64, ink: Ink<'_>) -> NodeId {
    let (tx, (w, _)) = (b.scene.node(control).map_or(0.0, |n| n.transform.tx), size_of(b, control));
    label_at(b, control, s, (tx + w / 2.0, cy), ink)
}

/// A control's printed legend centred on panel (`cx`, `cy`) — wherever the
/// panel prints it (beside a footswitch's LED, under a selector) — as its
/// `Label` slot.
pub fn label_at(b: &mut Builder<'_>, control: NodeId, s: &str, (cx, cy): (f64, f64), ink: Ink<'_>) -> NodeId {
    let (tx, ty) = b.scene.node(control).map_or((0.0, 0.0), |n| (n.transform.tx, n.transform.ty));
    let bw = width_for(s, &ink);
    let t = text_node(s, cx - tx - bw / 2.0, cy - ty, bw, &ink, TextAlign::Center);
    let id = b.add(control, t);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
        c.slots.push(surface_proto::Slot { role: SlotRole::Label, node: id.0 });
    }
    id
}

/// A scale printed round a shaft at (`cx`, `cy`) with marks where the
/// value really lands: `marks` are (position 0..1, label); `minors` evenly
/// spaced minor ticks across the travel. Labels are set outward.
#[allow(clippy::too_many_arguments)]
pub fn marks(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, sweep: f64, marks: &[(f64, &str)], minors: usize, ink: Ink<'_>) {
    let side = 2.0 * (r + 10.0);
    let (ox, oy) = (cx - side / 2.0, cy - side / 2.0);
    let c = side / 2.0;
    let angle = |p: f64| (p - 0.5) * sweep;
    let tick = |a: f64, len: f64, w: f64| {
        let (x0, y0) = draw::polar(c, c, r, a);
        let (x1, y1) = draw::polar(c, c, r + len, a);
        let (nx, ny) = (a.to_radians().cos() * w / 2.0, a.to_radians().sin() * w / 2.0);
        draw::polygon(&[(x0 - nx, y0 - ny), (x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny)])
    };
    if minors >= 2 {
        let mut minor = String::new();
        for i in 0..minors {
            minor.push_str(&tick(angle(i as f64 / (minors - 1) as f64), 2.6, 0.8));
            minor.push(' ');
        }
        b.add(parent, draw::with_opacity(draw::path("scale minors", ox, oy, side, side, minor, draw::solid(ink.color)), 0.6));
    }
    let mut major = String::new();
    for (p, _) in marks {
        major.push_str(&tick(angle(*p), 4.2, 1.3));
        major.push(' ');
    }
    b.add(parent, draw::path("scale majors", ox, oy, side, side, major, draw::solid(ink.color)));
    for (p, l) in marks {
        if l.is_empty() {
            continue;
        }
        let a = angle(*p);
        let tw = l.chars().count() as f64 * ink.size * 0.62;
        let push = 0.5 * (tw * a.to_radians().sin().abs() + ink.size * a.to_radians().cos().abs());
        let (lx, ly) = draw::polar(cx, cy, r + 7.0 + push, a);
        silk(b, parent, l, lx, ly, ink);
    }
}

/// Where `v` lands on a log sweep `min..max` (0..1).
#[must_use]
pub fn log_pos(v: f64, min: f64, max: f64) -> f64 {
    (v / min).ln() / (max / min).ln()
}

/// A detented rotary selector over a choice param: the kit's chicken-head
/// re-swept to the switch's real detents (≤ 60° a step), a dot at each and
/// the legends set round it. `variant` is the moulding (`cream`, `black`,
/// `red`).
#[allow(clippy::too_many_arguments)]
pub fn selector(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, variant: &str, (cx, cy): (f64, f64), d: f64, labels: &[&str], ink: Ink<'_>) -> NodeId {
    let n = labels.len().max(2);
    let sweep = detent_sweep(n);
    let blanks = vec![""; n];
    let p = Place::new(name).size(d).variant(variant).legend(name).scale(&blanks).bind(bind(addr));
    let k = put(b, parent, "knob.chicken_head", &p, cx, cy);
    strip_scale(b, k);
    strip_legend(b, k);
    tighten(b, k, d * 1.1);
    set_sweep(b, k, sweep);
    detents(b, parent, (cx, cy), d * 0.62 + 4.0, d * 0.62 + 14.0, labels, sweep, ink);
    k
}

// ── silkscreen ───────────────────────────────────────────────────────────

/// How a line of silkscreen is set.
#[derive(Clone, Copy)]
pub struct Ink<'a> {
    pub size: f64,
    pub color: &'a str,
    pub weight: f64,
    /// Letter spacing, em.
    pub track: f64,
    pub upper: bool,
}

impl<'a> Ink<'a> {
    #[must_use]
    pub const fn new(size: f64, color: &'a str) -> Self {
        Self { size, color, weight: 700.0, track: 0.1, upper: true }
    }
    #[must_use]
    pub const fn weight(mut self, w: f64) -> Self {
        self.weight = w;
        self
    }
    #[must_use]
    pub const fn track(mut self, t: f64) -> Self {
        self.track = t;
        self
    }
    #[must_use]
    pub const fn plain(mut self) -> Self {
        self.upper = false;
        self
    }
    #[must_use]
    pub const fn color(mut self, c: &'a str) -> Self {
        self.color = c;
        self
    }
    #[must_use]
    pub const fn size(mut self, s: f64) -> Self {
        self.size = s;
        self
    }
}

/// Generous box width for `s` at `ink`, so a line never wraps.
fn width_for(s: &str, ink: &Ink<'_>) -> f64 {
    s.chars().count() as f64 * ink.size * (0.72 + ink.track) + ink.size * 2.0
}

/// A text node whose box is `w` wide from `x`, vertically centred on `cy`.
#[must_use]
pub fn text_node(s: &str, x: f64, cy: f64, w: f64, ink: &Ink<'_>, align: TextAlign) -> Node {
    let mut n = draw::text("silk", s, x, cy - ink.size * 0.68, w, ink.size, ink.color, align);
    if let NodeKind::Text(t) = &mut n.kind {
        *t = TextData {
            characters: s.to_string(),
            font_size: ink.size,
            font_weight: Some(ink.weight),
            letter_spacing: ink.size * ink.track,
            text_case: if ink.upper { TextCase::Upper } else { TextCase::Original },
            align,
            ..TextData::default()
        };
    }
    n.name = format!("silk {s}");
    n
}

/// Silkscreen centred on (`cx`, `cy`).
pub fn silk(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, ink: Ink<'_>) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, cx - w / 2.0, cy, w, &ink, TextAlign::Center))
}

/// Silkscreen starting at `x`, vertically centred on `cy`.
pub fn silk_left(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, ink: Ink<'_>) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, x, cy, w, &ink, TextAlign::Left))
}

/// Silkscreen ending at `x`, vertically centred on `cy`.
pub fn silk_right(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, ink: Ink<'_>) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, x - w, cy, w, &ink, TextAlign::Right))
}

/// A horizontal hairline from `x0` to `x1` at `y`.
pub fn rule(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64, weight: f64, color: &str) -> NodeId {
    b.add(parent, draw::rect("rule", x0, y - weight / 2.0, x1 - x0, weight, draw::solid(color)))
}

/// A vertical hairline from `y0` to `y1` at `x`.
pub fn vrule(b: &mut Builder<'_>, parent: NodeId, x: f64, y0: f64, y1: f64, weight: f64, color: &str) -> NodeId {
    b.add(parent, draw::rect("rule", x - weight / 2.0, y0, weight, y1 - y0, draw::solid(color)))
}

/// An engraved groove: a dark line with a light one under it.
pub fn groove(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64) {
    rule(b, parent, x0, x1, y, 1.0, "#00000080");
    rule(b, parent, x0, x1, y + 1.0, 1.0, "#ffffff1c");
}

/// A vertical engraved groove.
pub fn vgroove(b: &mut Builder<'_>, parent: NodeId, x: f64, y0: f64, y1: f64) {
    vrule(b, parent, x, y0, y1, 1.0, "#00000080");
    vrule(b, parent, x + 1.0, y0, y1, 1.0, "#ffffff1c");
}

/// A rounded plate in `fill` with a fine bevel.
#[allow(clippy::too_many_arguments)]
pub fn plate(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), fill: Paint, radius: f64) -> NodeId {
    let n = draw::stroked(draw::rounded(draw::rect(name, x, y, w, h, fill), radius), "#00000066", 1.0);
    b.add(parent, draw::with_effects(n, vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0), draw::drop_shadow("#00000040", 0.0, 1.0, 1.5)]))
}

/// A recessed well: a darker rounded pocket sunk into the panel.
pub fn well(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), fill: Paint, radius: f64) -> NodeId {
    let mut n = draw::frame(name, x, y, w, h);
    n.fills = vec![fill];
    let n = draw::stroked(draw::rounded(n, radius), "#00000099", 1.0);
    b.add(parent, draw::with_effects(n, vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 5.0), draw::drop_shadow("#ffffff1f", 0.0, 1.0, 0.0)]))
}

/// A section outline.
pub fn section(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), color: &str, radius: f64) -> NodeId {
    b.add(parent, draw::stroked(draw::rounded(draw::frame("section", x, y, w, h), radius), color, 1.0))
}

/// Detent dots and legends printed around a shaft at (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn detents(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r_dot: f64, r_lbl: f64, labels: &[&str], sweep: f64, ink: Ink<'_>) {
    let n = labels.len();
    for (i, l) in labels.iter().enumerate() {
        let a = detent_angle(i, n, sweep);
        let (dx, dy) = draw::polar(cx, cy, r_dot, a);
        b.add(parent, draw::circle("detent", dx, dy, (ink.size * 0.17).max(1.1), draw::solid(ink.color)));
        if !l.is_empty() {
            let (lx, ly) = draw::polar(cx, cy, r_lbl, a);
            silk(b, parent, l, lx, ly, ink);
        }
    }
}

/// A scale printed round a shaft: ticks at each label over `sweep`, minor
/// ticks between, labels set outward so long ones clear the ticks.
#[allow(clippy::too_many_arguments)]
pub fn print_scale(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, labels: &[&str], sweep: f64, ink: Ink<'_>) {
    let n = labels.len();
    let side = 2.0 * (r + 8.0);
    let (ox, oy) = (cx - side / 2.0, cy - side / 2.0);
    let c = side / 2.0;
    let tick = |a: f64, len: f64, w: f64| {
        let (x0, y0) = draw::polar(c, c, r, a);
        let (x1, y1) = draw::polar(c, c, r + len, a);
        let (nx, ny) = (a.to_radians().cos() * w / 2.0, a.to_radians().sin() * w / 2.0);
        draw::polygon(&[(x0 - nx, y0 - ny), (x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny)])
    };
    let mut majors = String::new();
    let mut minors = String::new();
    for i in 0..n {
        majors.push_str(&tick(detent_angle(i, n, sweep), 4.0, 1.2));
        if i + 1 < n {
            minors.push_str(&tick((detent_angle(i, n, sweep) + detent_angle(i + 1, n, sweep)) / 2.0, 2.5, 0.8));
        }
    }
    b.add(parent, draw::path("scale majors", ox, oy, side, side, majors, draw::solid(ink.color)));
    b.add(parent, draw::with_opacity(draw::path("scale minors", ox, oy, side, side, minors, draw::solid(ink.color)), 0.6));
    for (i, l) in labels.iter().enumerate() {
        if l.is_empty() {
            continue;
        }
        let a = detent_angle(i, n, sweep);
        let tw = l.chars().count() as f64 * ink.size * 0.6;
        let push = 0.5 * (tw * a.to_radians().sin().abs() + ink.size * a.to_radians().cos().abs());
        let (lx, ly) = draw::polar(cx, cy, r + 6.0 + push, a);
        silk(b, parent, l, lx, ly, ink);
    }
}

// ── light ────────────────────────────────────────────────────────────────

/// `#rrggbbaa` of `c` at alpha `a`.
#[must_use]
pub fn hexa(c: &str, a: f64) -> String {
    let col = draw::with_alpha(draw::hex(c), a);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", q(col.r), q(col.g), q(col.b), q(col.a))
}

/// `a` mixed toward `b` by `t`, as `#rrggbb`.
#[must_use]
pub fn mixed(a: &str, b: &str, t: f64) -> String {
    let c = draw::mix(draw::hex(a), draw::hex(b), t);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b))
}

/// Make `n` emit light: a bloom on the GPU renderer, nothing elsewhere
/// (pair it with a drawn glow for the still render).
#[must_use]
pub fn emit(mut n: Node, color: &str, intensity: f64, radius: f64) -> Node {
    let c: Color = draw::hex(color);
    n.emission = Some(Emission { color: c, intensity, radius });
    n
}

/// A lit lens: hot core, the colour, a dark rim, and its glow.
#[must_use]
pub fn lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let hot = [(0.0, "#ffffffee".to_string()), (0.2, mixed(color, "#ffffff", 0.55)), (0.55, color.to_string()), (1.0, mixed(color, "#000000", 0.5))];
    let stops: Vec<(f64, &str)> = hot.iter().map(|(p, c)| (*p, c.as_str())).collect();
    let n = draw::with_effects(draw::circle("lamp", cx, cy, r, draw::radial_at(0.38, 0.32, 0.9, &stops)), vec![draw::drop_shadow(&hexa(color, 0.8), 0.0, 0.0, r * 2.2)]);
    emit(n, color, 1.2, r * 3.0)
}

/// An unlit lens.
#[must_use]
pub fn dark_lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let dark = [(0.0, "#ffffff22".to_string()), (0.5, mixed(color, "#000000", 0.62)), (1.0, "#000000d0".to_string())];
    let stops: Vec<(f64, &str)> = dark.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::with_effects(draw::circle("lens", cx, cy, r, draw::radial_at(0.38, 0.3, 0.95, &stops)), vec![draw::inner_shadow("#000000b0", 0.0, 0.0, r * 0.7)])
}

/// A panel lamp in a black bezel, lit or not. Decoration, not a control.
pub fn lamp(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), d: f64, color: &str, lit: bool) {
    let r = d / 2.0;
    b.add(parent, draw::with_effects(draw::circle("bezel", cx, cy, r + (d * 0.18).max(1.2), draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#060607")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    b.add(parent, dark_lens(cx, cy, r, color));
    if lit {
        b.add(parent, lens(cx, cy, r, color));
    }
}

/// A lamp that *is* a toggle: its lens lights while `addr` is on (a head
/// select, a pattern step). The control's box is `hit` square around it.
#[allow(clippy::too_many_arguments)]
pub fn lamp_toggle(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64), d: f64, hit: f64, color: &str) -> NodeId {
    let root = b.add(parent, draw::frame(name, cx - hit / 2.0, cy - hit / 2.0, hit, hit));
    let c = hit / 2.0;
    lamp(b, root, (c, c), d, color, false);
    let lit = b.add(root, lens(c, c, d / 2.0, color));
    let mut ctl = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, lit.0);
    ctl.binding = Some(bind(addr));
    b.controls.insert(root, ctl);
    root
}

// ── windows ──────────────────────────────────────────────────────────────

/// How a lit display reads: its glass, its phosphor, and how it glows.
#[derive(Clone, Copy)]
pub struct Screen<'a> {
    /// Glass top and bottom.
    pub glass: (&'a str, &'a str),
    /// The lit characters.
    pub ink: &'a str,
    /// Draw a VFD mesh over the glass.
    pub mesh: bool,
}

/// A dark display window (`x, y, w, h`), recessed, with an optional VFD
/// mesh. Returns the window frame, a parent for readouts.
pub fn screen(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), s: Screen<'_>) -> NodeId {
    // The bezel the glass sits in.
    let bezel = draw::rounded(draw::rect("bezel", x - 5.0, y - 5.0, w + 10.0, h + 10.0, draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1c1d20")])), 6.0);
    b.add(parent, draw::with_effects(bezel, vec![draw::drop_shadow("#ffffff1a", 0.0, 1.0, 0.0), draw::inner_shadow("#000000", 0.0, 1.0, 2.0)]));
    let mut win = draw::frame(name, x, y, w, h);
    win.fills = vec![draw::linear(180.0, &[(0.0, s.glass.0), (1.0, s.glass.1)])];
    win.clips_content = true;
    let win = draw::with_effects(draw::rounded(win, 3.0), vec![draw::inner_shadow("#000000d0", 0.0, 2.0, 6.0)]);
    let root = b.add(parent, win);
    if s.mesh {
        let mut d = String::new();
        let pitch = 3.0;
        let mut yy = 1.5;
        while yy < h {
            d.push_str(&format!("M 0 {yy:.2} L {w:.2} {yy:.2} L {w:.2} {:.2} L 0 {:.2} Z ", yy + 0.45, yy + 0.45));
            yy += pitch;
        }
        b.add(root, draw::with_opacity(draw::path("mesh", 0.0, 0.0, w, h, d, draw::solid("#000000")), 0.35));
    }
    root
}

/// The glass sheen over a window: a soft diagonal highlight, added last.
pub fn sheen(b: &mut Builder<'_>, win: NodeId) {
    let (w, h) = size_of(b, win);
    let d = draw::polygon(&[(w * 0.08, 0.0), (w * 0.42, 0.0), (w * 0.26, h), (0.0, h), (0.0, h * 0.35)]);
    b.add(win, draw::path("sheen", 0.0, 0.0, w, h, d, draw::linear(90.0, &[(0.0, "#ffffff00"), (0.5, "#ffffff0f"), (1.0, "#ffffff00")])));
}

/// A live readout in a window: the bound value, lit, left-, centre- or
/// right-aligned in the box (`x, cy, w`) at `size`. `placeholder` is what a
/// still render prints before the engine speaks.
#[allow(clippy::too_many_arguments)]
pub fn readout(b: &mut Builder<'_>, parent: NodeId, name: &str, binding: Binding, (x, cy, w): (f64, f64, f64), size: f64, ink: &str, align: TextAlign) -> NodeId {
    let h = (size * 1.5).ceil();
    let root = b.add(parent, draw::frame(name, x, cy - h / 2.0, w, h));
    let lit = Ink::new(size, ink).weight(600.0).track(0.04).plain();
    let mut t = text_node("", 0.0, h / 2.0, w, &lit, align);
    t.name = "value".into();
    t.effects = vec![draw::drop_shadow(&hexa(ink, 0.7), 0.0, 0.0, size * 0.45)];
    let t = emit(t, ink, 0.9, size * 0.8);
    let value = b.add(root, t);
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    b.controls.insert(root, c);
    root
}

/// A note-value picker on a display: the division the time locks to, lit,
/// with the step arrows beside it. A `Selector` over the division param —
/// wheel or arrow keys step it — whose `Value` slot is the lit text.
#[allow(clippy::too_many_arguments)]
pub fn note_picker(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (x, cy, w): (f64, f64, f64), size: f64, ink: &str, frame: &str) -> NodeId {
    let h = (size * 1.6).ceil();
    let root = b.add(parent, draw::stroked(draw::rounded(draw::frame(name, x, cy - h / 2.0, w, h), 3.0), frame, 1.0));
    let lit_ink = Ink::new(size, ink).weight(600.0).track(0.04).plain();
    let mut t = text_node("", 0.0, h / 2.0, w - 10.0, &lit_ink, TextAlign::Center);
    t.name = "value".into();
    t.effects = vec![draw::drop_shadow(&hexa(ink, 0.7), 0.0, 0.0, size * 0.45)];
    let value = b.add(root, emit(t, ink, 0.9, size * 0.8));
    let (ax, s) = (w - 8.0, size * 0.22);
    let up = draw::polygon(&[(ax - s, h / 2.0 - 1.5), (ax + s, h / 2.0 - 1.5), (ax, h / 2.0 - 1.5 - s * 1.2)]);
    let down = draw::polygon(&[(ax - s, h / 2.0 + 1.5), (ax + s, h / 2.0 + 1.5), (ax, h / 2.0 + 1.5 + s * 1.2)]);
    b.add(root, draw::path("step arrows", 0.0, 0.0, w, h, format!("{up} {down}"), draw::solid(frame)));
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, value.0);
    c.binding = Some(bind(addr));
    b.controls.insert(root, c);
    root
}

/// Lit legend text on a display (not bound): the fixed annunciators.
pub fn lit(b: &mut Builder<'_>, parent: NodeId, s: &str, (x, cy): (f64, f64), ink: Ink<'_>, align: TextAlign) -> NodeId {
    let w = width_for(s, &ink);
    let x0 = match align {
        TextAlign::Center => x - w / 2.0,
        TextAlign::Right => x - w,
        _ => x,
    };
    let mut t = text_node(s, x0, cy, w, &ink, align);
    t.effects = vec![draw::drop_shadow(&hexa(ink.color, 0.6), 0.0, 0.0, ink.size * 0.5)];
    b.add(parent, emit(t, ink.color, 0.6, ink.size))
}

// ── LED meters ───────────────────────────────────────────────────────────

/// Colour of an LED at `db`: green, amber from -6, red from 0.
fn level_color(db: f64) -> &'static str {
    if db >= 0.0 {
        "#ff3b2f"
    } else if db >= -6.0 {
        "#ffb21e"
    } else {
        "#46e05a"
    }
}

/// A vertical LED bar meter: `n` segments over `floor..ceiling` dB, lit from
/// the bottom by `stream` channel `channel`. A `Gauge` whose moving part is
/// a dark overlay sliding off the lit column — a `Thumb` in a lane twice
/// the column's height, clipped to the column (bar meters ignore curves).
#[allow(clippy::too_many_arguments)]
pub fn led_column(b: &mut Builder<'_>, parent: NodeId, name: &str, stream: &str, channel: u8, (x, y, w, h): (f64, f64, f64, f64), n: usize, (floor, ceiling): (f64, f64)) -> NodeId {
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
        let db = floor + (ceiling - floor) * (i as f64 + 1.0) / n as f64;
        let c = level_color(db - 0.01);
        let sy = ch - (i as f64 + 1.0) * pitch + pitch * 0.18;
        let seg = draw::rounded(draw::rect("segment", 0.0, sy, cw, pitch * 0.64, draw::linear(90.0, &[(0.0, &mixed(c, "#000000", 0.25)), (0.5, &mixed(c, "#ffffff", 0.18)), (1.0, &mixed(c, "#000000", 0.25))])), 1.0);
        b.add(col, emit(seg, c, 0.8, pitch));
    }
    let lane = b.add(col, draw::frame("lane", 0.0, -ch, cw, 2.0 * ch));
    let mask = b.add(lane, draw::rect("unlit", 0.0, 0.0, cw, ch, draw::solid("#060708db")));
    let mut c = Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: floor, ceiling_db: ceiling }).slot(SlotRole::Needle, col.0).slot(SlotRole::Thumb, mask.0);
    c.binding = Some(Binding { source: surface_proto::Source::Stream { id: EntryId::new(stream), channel: Some(channel) }, ..Binding::stream(stream) });
    b.controls.insert(root, c);
    root
}

/// A horizontal LED bar meter, lit from the left (a digital unit's input
/// or output ladder): `n` segments over `floor..ceiling` dB.
#[allow(clippy::too_many_arguments)]
pub fn led_row(b: &mut Builder<'_>, parent: NodeId, name: &str, stream: &str, channel: u8, (x, y, w, h): (f64, f64, f64, f64), n: usize, (floor, ceiling): (f64, f64)) -> NodeId {
    let mut housing = draw::frame(name, x, y, w, h);
    housing.fills = vec![draw::solid("#070809")];
    let housing = draw::with_effects(draw::stroked(draw::rounded(housing, 2.0), "#000000", 1.0), vec![draw::inner_shadow("#000000", 0.0, 1.0, 3.0), draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0)]);
    let root = b.add(parent, housing);
    let pad = 2.0;
    let (cw, ch) = (w - 2.0 * pad, h - 2.0 * pad);
    let mut col = draw::frame("row", pad, pad, cw, ch);
    col.clips_content = true;
    let col = b.add(root, col);
    let pitch = cw / n as f64;
    for i in 0..n {
        let db = floor + (ceiling - floor) * (i as f64 + 1.0) / n as f64;
        let c = level_color(db - 0.01);
        let sx = i as f64 * pitch + pitch * 0.14;
        let seg = draw::rounded(draw::rect("segment", sx, 0.0, pitch * 0.72, ch, draw::linear(180.0, &[(0.0, &mixed(c, "#ffffff", 0.2)), (1.0, &mixed(c, "#000000", 0.3))])), 1.0);
        b.add(col, emit(seg, c, 0.8, pitch));
    }
    // The cover slides right as the level rises: a Thumb in a lane twice
    // the row's width. A Thumb travels along a lane's long axis.
    let lane = b.add(col, draw::frame("lane", 0.0, 0.0, 2.0 * cw, ch));
    let mask = b.add(lane, draw::rect("unlit", 0.0, 0.0, cw, ch, draw::solid("#060708db")));
    let mut c = Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: floor, ceiling_db: ceiling }).slot(SlotRole::Needle, col.0).slot(SlotRole::Thumb, mask.0);
    c.binding = Some(Binding { source: surface_proto::Source::Stream { id: EntryId::new(stream), channel: Some(channel) }, ..Binding::stream(stream) });
    b.controls.insert(root, c);
    root
}

// ── catalog entries ──────────────────────────────────────────────────────

/// A continuous param with a real range; `default` in real units.
#[must_use]
pub fn param(addr: &str, name: &str, range: Range, default: f64) -> ParamInfo {
    ParamInfo::new(addr, name, range).with_default_real(default)
}

/// 0–100 %.
#[must_use]
pub fn percent(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(addr, name, Range { steps: Some(101), ..Range::linear(0.0, 100.0, Unit::Percent) }, default)
}

/// A dB param over `min..max`.
#[must_use]
pub fn db(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    let p = param(addr, name, Range::linear(min, max, Unit::Db), default);
    if min < 0.0 && max > 0.0 { p.bipolar() } else { p }
}

/// A trim around the circuit as designed: −50 … +50 % of its travel, noon
/// = the voicing as it comes.
#[must_use]
pub fn trim(addr: &str, name: &str) -> ParamInfo {
    param(addr, name, Range { steps: Some(101), ..Range::linear(-50.0, 50.0, Unit::Percent) }, 0.0).bipolar()
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

/// A stream binding.
#[must_use]
pub fn stream(id: &str) -> Binding {
    Binding::stream(id)
}

// ── time ─────────────────────────────────────────────────────────────────

/// The note values a synced time locks to, longest first — the same 21 as
/// `musical-time` (`1/1 … 1/64T`, straight / dotted / triplet).
pub const DIVISIONS: &[&str] = &[
    "1/1", "1/1D", "1/1T", "1/2", "1/2D", "1/2T", "1/4", "1/4D", "1/4T", "1/8", "1/8D", "1/8T", "1/16", "1/16D", "1/16T", "1/32", "1/32D",
    "1/32T", "1/64", "1/64D", "1/64T",
];
/// 1/8D: at 120 BPM exactly the 375 ms free-running default, so switching a
/// 120 BPM session to Sync changes nothing.
pub const DEFAULT_DIV: usize = 10;
/// The time mode switch's positions.
pub const TIME_MODES: &[&str] = &["Free", "Sync"];

/// A delay time in ms, swept logarithmically over what the line holds.
#[must_use]
pub fn time_ms(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    param(addr, name, Range::log(min, max, Unit::Milliseconds), default)
}

/// Scale marks for a time knob over `min..max` ms (log).
#[must_use]
pub fn time_marks(min: f64, max: f64) -> Vec<(f64, &'static str)> {
    const NAMES: &[(f64, &str)] = &[
        (1.0, "1"), (2.0, "2"), (5.0, "5"), (10.0, "10"), (20.0, "20"), (25.0, "25"), (40.0, "40"), (50.0, "50"), (100.0, "100"),
        (200.0, "200"), (500.0, "500"), (800.0, "800"), (1000.0, "1k"), (1200.0, "1.2k"), (2000.0, "2k"), (4000.0, "4k"),
    ];
    // The two ends, and the decades between that don't crowd them.
    let mut v: Vec<(f64, &'static str)> = NAMES.iter().filter(|(ms, _)| (*ms - min).abs() < 1e-9 || (*ms - max).abs() < 1e-9).map(|(ms, l)| (log_pos(*ms, min, max), *l)).collect();
    for (ms, l) in [(10.0, "10"), (100.0, "100"), (1000.0, "1k")] {
        let p = log_pos(ms, min, max);
        if p > 0.14 && p < 0.86 {
            v.push((p, l));
        }
    }
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

/// 0 / 5 / 10 over a 0–100 % knob.
pub const PCT_MARKS: &[(f64, &str)] = &[(0.0, "0"), (0.5, "5"), (1.0, "10")];
/// Feedback's scale: 0, 50, 100 and the run-away past it.
pub const FB_MARKS: &[(f64, &str)] = &[(0.0, "0"), (50.0 / 110.0, "50"), (100.0 / 110.0, "100"), (1.0, "")];

/// Feedback, 0–110 %: past 100 the repeats run away, as the units do.
#[must_use]
pub fn feedback(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(addr, name, Range { steps: Some(111), ..Range::linear(0.0, 110.0, Unit::Percent) }, default)
}
