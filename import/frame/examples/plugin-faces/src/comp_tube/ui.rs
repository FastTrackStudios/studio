//! Panel-building helpers shared by the tube faces: placing kit components
//! by their centre, silkscreen that survives both 1× and 2×, restyling a
//! component's printed ink for a light panel, readout windows, and the
//! catalog entries every face declares.

use document_proto::{Node, NodeId, NodeKind, Paint, TextAlign, TextCase, TextData, Transform};
use surface_kit::{Place, draw};
use surface_proto::{Binding, Control, ControlKind, ParamInfo, Range, SlotRole, StreamInfo, StreamKind, Unit};
use surface_widgets::Builder;

// ── placing ─────────────────────────────────────────────────────────────

/// Place kit component `id`; panics on a bad id/variant (faces are literals).
pub fn put(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place) -> NodeId {
    surface_kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"))
}

pub fn size_of(b: &Builder<'_>, id: NodeId) -> (f64, f64) {
    b.scene.node(id).map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
}

/// Size the face's root frame to its panel (the root has no layout to hug
/// with, and a renderer sizes the page from it).
pub fn fit_root(b: &mut Builder<'_>, root: NodeId, panel: NodeId) {
    let (w, h) = size_of(b, panel);
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size { width: w, height: h };
    }
}

pub fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

/// A rotary (knob, rotary switch) with its shaft at (`cx`, `cy`): every kit
/// rotary is drawn in a square the width of its root, centred on the shaft.
pub fn rotary_at(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, cx: f64, cy: f64) -> NodeId {
    let n = put(b, parent, id, p);
    let (w, _) = size_of(b, n);
    move_to(b, n, cx - w / 2.0, cy - w / 2.0);
    n
}

/// Anything with its box centred on (`cx`, `cy`).
pub fn centred_at(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, cx: f64, cy: f64) -> NodeId {
    let n = put(b, parent, id, p);
    let (w, h) = size_of(b, n);
    move_to(b, n, cx - w / 2.0, cy - h / 2.0);
    n
}

/// Recolour every printed part of a placed component — its text, scale
/// ticks, dots and detents — for the panel it sits on. The kit prints light
/// ink for dark panels; the LA-2A's grey and the brushed faces want dark.
pub fn reink(b: &mut Builder<'_>, root: NodeId, color: &str) {
    for id in b.scene.walk(root) {
        let Some(n) = b.scene.node_mut(id) else { continue };
        let printed = n.text().is_some() || n.name.starts_with("scale ") || n.name == "detent" || n.name == "numeral";
        if printed {
            n.fills = vec![draw::solid(color)];
        }
    }
}

/// Restyle a component's caption (its `Label` text) — size, weight, colour.
pub fn caption_style(b: &mut Builder<'_>, root: NodeId, size: f64, weight: f64, color: &str) {
    let label = b.controls.get(&root).and_then(|c| c.slots_of(SlotRole::Label).next());
    let Some(label) = label else { return };
    if let Some(n) = b.scene.node_mut(NodeId(label))
        && let NodeKind::Text(t) = &mut n.kind
    {
        let grow = size / t.font_size;
        t.font_size = size;
        t.font_weight = Some(weight);
        t.letter_spacing = size * 0.12;
        n.size.height = (n.size.height * grow).ceil();
        n.fills = vec![draw::solid(color)];
    }
}

/// Move a component's caption so its line is centred on panel `y` (to line
/// a switch's caption up with the knobs' beside it). The component's root
/// must already be in place.
pub fn caption_y(b: &mut Builder<'_>, root: NodeId, y: f64) {
    let label = b.controls.get(&root).and_then(|c| c.slots_of(SlotRole::Label).next());
    let top = b.scene.node(root).map_or(0.0, |n| n.transform.ty);
    if let Some(label) = label
        && let Some(n) = b.scene.node_mut(NodeId(label))
    {
        let h = n.size.height;
        n.transform.ty = y - top - h / 2.0;
    }
}

/// Hide the numerals (knob) or option legends (rotary switch) a component
/// prints round itself, for a face that engraves its own — the ticks and
/// detent dots stay.
pub fn drop_numerals(b: &mut Builder<'_>, root: NodeId) {
    for id in b.scene.walk(root) {
        if let Some(n) = b.scene.node_mut(id)
            && (n.name == "numeral" || n.name == "option")
        {
            n.visible = false;
        }
    }
}

/// Engrave `labels` evenly over `sweep` degrees (centred on straight up)
/// round a shaft at (`cx`, `cy`), each set *outward* from radius `r` — a
/// label on the right starts at the radius, one on the left ends at it — so
/// no legend ever runs back into the knob, whatever its length.
pub fn dial_numerals(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, sweep: f64, labels: &[&str], ink: Ink<'_>) {
    let n = labels.len();
    for (i, s) in labels.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let t = if n < 2 { 0.5 } else { i as f64 / (n - 1) as f64 };
        let a = (t - 0.5) * sweep;
        let (x, y) = draw::polar(cx, cy, r, a);
        let (sin, cos) = a.to_radians().sin_cos();
        let tw = s.chars().count() as f64 * ink.size * (0.6 + ink.track);
        silk(b, parent, s, x + sin * tw / 2.0, y - cos * ink.size * 0.35, ink);
    }
}

/// A knob's scale, re-engraved: the kit's numerals hidden, `labels` set
/// outward from radius `r` over the knob's 300° travel.
pub fn knob_scale(b: &mut Builder<'_>, parent: NodeId, knob: NodeId, at: (f64, f64), r: f64, labels: &[&str], ink: Ink<'_>) {
    drop_numerals(b, knob);
    dial_numerals(b, parent, at, r, 300.0, labels, ink);
}

/// A detented rotary selector over a choice param: the kit's chicken-head
/// (its pointer is one vector, so it turns cleanly) re-swept to the
/// switch's real detents — at most 60° a step — with a dot at each detent
/// and the legends engraved outward from them. `variant` is the moulding
/// (`cream`, `black`, `red`). Returns the control's root.
#[allow(clippy::too_many_arguments)]
pub fn selector(
    b: &mut Builder<'_>,
    parent: NodeId,
    p: &Place,
    at: (f64, f64),
    labels: &[&str],
    ink: Ink<'_>,
    dots: &str,
) -> NodeId {
    let n = labels.len().max(2);
    let step = (300.0 / (n - 1) as f64).min(60.0);
    let sweep = step * (n - 1) as f64;
    let blanks = vec![""; n];
    let place = Place { scale: Some(blanks.iter().map(ToString::to_string).collect()), ..p.clone() };
    let root = rotary_at(b, parent, "knob.chicken_head", &place, at.0, at.1);
    for id in b.scene.walk(root) {
        if let Some(node) = b.scene.node_mut(id)
            && node.name.starts_with("scale ")
        {
            node.visible = false;
        }
    }
    if let Some(c) = b.controls.get_mut(&root)
        && let ControlKind::Knob { sweep_degrees } = &mut c.kind
    {
        *sweep_degrees = sweep;
    }
    let d = p.size.unwrap_or(64.0);
    let r_dot = d * 0.5 * 1.25 + 4.0;
    for i in 0..n {
        let a = (i as f64 / (n - 1) as f64 - 0.5) * sweep;
        let (x, y) = draw::polar(at.0, at.1, r_dot, a);
        b.add(parent, draw::circle("detent", x, y, 1.6, draw::solid(dots)));
    }
    dial_numerals(b, parent, at, r_dot + 5.0, sweep, labels, ink);
    root
}

/// Repaint a panel with a straight top-to-bottom sheen (`stops` top first).
pub fn repaint(b: &mut Builder<'_>, panel: NodeId, stops: &[(f64, &str)]) {
    if let Some(n) = b.scene.node_mut(panel) {
        n.fills = vec![draw::linear(180.0, stops)];
        n.stroke.weight = 1.0;
    }
}

// ── silkscreen ──────────────────────────────────────────────────────────

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
    pub const fn new(size: f64, color: &'a str) -> Self {
        Self { size, color, weight: 600.0, track: 0.08, upper: true }
    }
    pub const fn weight(mut self, w: f64) -> Self {
        self.weight = w;
        self
    }
    pub const fn track(mut self, t: f64) -> Self {
        self.track = t;
        self
    }
    pub const fn plain(mut self) -> Self {
        self.upper = false;
        self
    }
    pub const fn color(mut self, c: &'a str) -> Self {
        self.color = c;
        self
    }
    pub const fn size(mut self, s: f64) -> Self {
        self.size = s;
        self
    }
}

/// Generous box width for `s` at `ink`, so a line never wraps.
fn width_for(s: &str, ink: &Ink<'_>) -> f64 {
    s.chars().count() as f64 * ink.size * (0.72 + ink.track) + ink.size * 2.0
}

fn text_node(s: &str, x: f64, cy: f64, w: f64, ink: &Ink<'_>, align: TextAlign) -> Node {
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

/// A hairline rule from (`x0`, `y`) to (`x1`, `y`).
pub fn rule(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64, weight: f64, color: &str) -> NodeId {
    b.add(parent, draw::rect("rule", x0, y - weight / 2.0, x1 - x0, weight, draw::solid(color)))
}

/// An engraved plate: a rounded rectangle in `fill` with a fine bevel.
#[allow(clippy::too_many_arguments)]
pub fn plate(b: &mut Builder<'_>, parent: NodeId, name: &str, x: f64, y: f64, w: f64, h: f64, fill: Paint, radius: f64) -> NodeId {
    let n = draw::rounded(draw::rect(name, x, y, w, h, fill), radius);
    let n = draw::stroked(n, "#00000066", 1.0);
    b.add(parent, draw::with_effects(n, vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0), draw::drop_shadow("#00000040", 0.0, 1.0, 1.5)]))
}

// ── readouts ────────────────────────────────────────────────────────────

/// A small readout window: a dark recessed glass with the bound value
/// printed in it (the runtime writes the `Value` slot). `placeholder` is
/// what a still render shows before the engine speaks.
#[allow(clippy::too_many_arguments)]
pub fn readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    binding: Binding,
    (cx, cy): (f64, f64),
    (w, h): (f64, f64),
    placeholder: &str,
    glow: &str,
) -> NodeId {
    let mut n = draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h);
    n.fills = vec![draw::solid("#0b0c0d")];
    let n = draw::stroked(draw::rounded(n, h * 0.22), "#000000cc", 1.0);
    let n = draw::with_effects(
        n,
        vec![draw::inner_shadow("#000000e6", 0.0, 1.5, 3.0), draw::drop_shadow("#ffffff2e", 0.0, 1.0, 0.0)],
    );
    let root = b.add(parent, n);
    // The glass: a faint sheen over the top half.
    b.add(root, draw::rounded(draw::rect("glass", 1.0, 1.0, w - 2.0, h * 0.45, draw::linear(180.0, &[(0.0, "#ffffff14"), (1.0, "#ffffff00")])), h * 0.2));
    let size = h * 0.56;
    let ink = Ink::new(size, glow).weight(600.0).track(0.06).plain();
    let mut t = text_node(placeholder, 0.0, h / 2.0, w, &ink, TextAlign::Center);
    t.name = "value".into();
    t.effects = vec![draw::drop_shadow(&format!("{glow}8c"), 0.0, 0.0, size * 0.5)];
    let value = b.add(root, t);
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    b.controls.insert(root, c);
    root
}

// ── gauges ──────────────────────────────────────────────────────────────

/// Re-range a placed needle meter: its card still prints `-20 … +3`, but
/// the needle reads `floor..ceiling` of the bound stream — an output meter
/// calibrated so 0 VU is `zero_dbfs`.
pub fn calibrate_vu(b: &mut Builder<'_>, meter: NodeId, zero_dbfs: f64) {
    if let Some(c) = b.controls.get_mut(&meter)
        && let ControlKind::Gauge { floor_db, ceiling_db, .. } = &mut c.kind
    {
        *floor_db = -20.0 + zero_dbfs;
        *ceiling_db = 3.0 + zero_dbfs;
    }
}

// ── catalog ─────────────────────────────────────────────────────────────

/// A continuous, featured param.
pub fn param(addr: &str, name: &str, range: Range, default_real: f64) -> ParamInfo {
    ParamInfo::new(addr, name, range).featured().with_default_real(default_real)
}

/// A dial printed 0–100 in whole numbers.
pub fn dial_0_100(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(addr, name, Range { steps: Some(101), ..Range::linear(0.0, 100.0, Unit::None) }, default)
}

/// A switch over named positions (`default` = index).
pub fn choice(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).featured().choice(options);
    let last = options.len().saturating_sub(1).max(1);
    p.default = default as f64 / last as f64;
    p
}

/// A two-position switch.
pub fn toggle(addr: &str, name: &str, on: bool) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).featured().toggle();
    p.default = if on { 1.0 } else { 0.0 };
    p
}

/// The gain-reduction stream every face meters, and an output level.
pub fn streams(face: &str, name: &str) -> Vec<StreamInfo> {
    vec![
        StreamInfo { id: surface_proto::EntryId::new(format!("{face}/gr")), name: format!("{name} Gain Reduction"), kind: StreamKind::GainReduction, rate_hz: 30.0 },
        StreamInfo { id: surface_proto::EntryId::new(format!("{face}/out")), name: format!("{name} Output"), kind: StreamKind::Level { channels: 2 }, rate_hz: 30.0 },
    ]
}

pub fn bind(addr: &str) -> Binding {
    Binding::param(addr)
}

pub fn stream(id: &str) -> Binding {
    Binding::stream(id)
}
