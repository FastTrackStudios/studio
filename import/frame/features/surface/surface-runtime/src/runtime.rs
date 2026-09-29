//! One running surface: design + controls + live values.
//!
//! The runtime is transport-agnostic and synchronous. The host (desktop
//! window, plugin editor, iPad app, browser) feeds it input, engine updates
//! and time; it returns [`EngineCall`]s to send and a scene to draw. The
//! async glue to a [`SurfaceEngine`](surface_proto::SurfaceEngine) is
//! [`crate::sync`].

use std::collections::{BTreeMap, HashMap};

use document_proto::{NodeId, NodeKind, Scene, Transform};
use surface_proto::{
    Binding, Catalog, Control, ControlKind, EditPhase, EntryId, MeterTap, ParamAddr, ParamKind, ParamValue, Source,
    StreamFrame, StreamKind,
};

use crate::coalesce::Coalescer;
use crate::gesture::{Bounds, Gesture, Input, Output};
use crate::meter::{Ballistics, MeterScale, Sense};
use crate::visual::{ControlState, control_ops};

// Live reload: swap the design, keep the running state.
mod swap;

/// A design plus the controls attached to its nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    pub scene: Scene,
    pub controls: BTreeMap<NodeId, Control>,
}

impl Surface {
    /// Read the controls stored on a scene's nodes.
    #[must_use]
    pub fn from_scene(scene: Scene) -> Self {
        let controls = scene.nodes.values().filter_map(|n| n.control.clone().map(|c| (n.id, c))).collect();
        Self { scene, controls }
    }

    /// The scene with every control stored on its node (what gets saved,
    /// synced, printed as text, and edited by tools).
    #[must_use]
    pub fn to_scene(&self) -> Scene {
        let mut scene = self.scene.clone();
        for node in scene.nodes.values_mut() {
            node.control = self.controls.get(&node.id).cloned();
        }
        scene
    }
}

/// Something to send to the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineCall {
    SetParam { addr: ParamAddr, value: f64, phase: EditPhase },
    Invoke { action: EntryId },
    Select { collection: EntryId, index: u32 },
}

pub struct Runtime {
    surface: Surface,
    catalog: Catalog,
    values: BTreeMap<ParamAddr, f64>,
    streams: HashMap<EntryId, Vec<f64>>,
    /// Ballistics per (control, stream, channel): a switched meter keeps
    /// every tap moving, so switching shows a settled reading.
    meters: HashMap<(NodeId, EntryId, u8), Ballistics>,
    gestures: HashMap<NodeId, Gesture>,
    /// XY pads' second axis, and footswitch/button positions not tied to params.
    local_positions: HashMap<NodeId, f64>,
    captured: Option<NodeId>,
    boxes: HashMap<NodeId, Bounds>,
    /// Screen → box space, for controls drawn through a transformed
    /// container (absent = identity).
    to_box: HashMap<NodeId, Transform>,
    coalescer: Coalescer,
    meter_scale: MeterScale,
}

impl Runtime {
    #[must_use]
    pub fn new(surface: Surface, catalog: Catalog) -> Self {
        let values = catalog.params.iter().map(|p| (p.addr.clone(), p.default)).collect();
        let gestures = surface.controls.iter().map(|(id, c)| (*id, Gesture::new(c.kind.clone()))).collect();
        Self {
            surface,
            catalog,
            values,
            streams: HashMap::new(),
            meters: HashMap::new(),
            gestures,
            local_positions: HashMap::new(),
            captured: None,
            boxes: HashMap::new(),
            to_box: HashMap::new(),
            coalescer: Coalescer::default(),
            meter_scale: MeterScale::default(),
        }
    }

    #[must_use]
    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The boxes of control nodes from the host's layout pass, for hit
    /// testing and position-based gestures.
    ///
    /// Boxes are axis-aligned in the renderer's *flattened* space (each node
    /// at its parent's box origin plus its own offset, ancestors' rotation
    /// and scale not applied — `render::build_scene_layout_boxes`). A control
    /// inside a rotated/scaled frame is drawn through its ancestors'
    /// transforms, so the runtime derives each control's box-to-screen
    /// transform from the scene and hit-tests through its inverse. Pass every
    /// node's box (not only the controls') so transformed ancestors are
    /// pivoted where the layout put them; missing ones fall back to the sum
    /// of their authored offsets.
    pub fn set_boxes(&mut self, boxes: impl IntoIterator<Item = (NodeId, Bounds)>) {
        let all: HashMap<NodeId, Bounds> = boxes.into_iter().collect();
        let transformed: Vec<(NodeId, Bounds, Option<Transform>)> = all
            .iter()
            .filter(|(id, _)| self.surface.controls.contains_key(id))
            .map(|(id, b)| (*id, *b, box_to_screen(&self.surface.scene, *id, &all)))
            .collect();
        self.set_transformed_boxes(transformed);
    }

    /// [`Self::set_boxes`] with each control's box-to-screen transform given
    /// by the host (`None` = the box is on screen as is): for hosts that
    /// already track world transforms.
    pub fn set_transformed_boxes(&mut self, boxes: impl IntoIterator<Item = (NodeId, Bounds, Option<Transform>)>) {
        self.boxes.clear();
        self.to_box.clear();
        for (id, b, t) in boxes {
            if !self.surface.controls.contains_key(&id) {
                continue;
            }
            self.boxes.insert(id, b);
            if let Some(inv) = t.and_then(|t| invert(&t)) {
                self.to_box.insert(id, inv);
            }
        }
    }

    /// A screen point in `id`'s box space.
    fn local(&self, id: NodeId, x: f64, y: f64) -> (f64, f64) {
        self.to_box.get(&id).map_or((x, y), |t| apply(t, x, y))
    }

    // ── engine → surface ──────────────────────────────────────────────────

    /// Every parameter value the runtime holds (engine state plus
    /// optimistic local edits) — to carry over into a rebuilt runtime.
    #[must_use]
    pub fn values(&self) -> Vec<ParamValue> {
        self.values.iter().map(|(addr, value)| ParamValue { addr: addr.clone(), value: *value }).collect()
    }

    pub fn apply_values(&mut self, values: impl IntoIterator<Item = ParamValue>) {
        for v in values {
            self.values.insert(v.addr, v.value);
        }
    }

    pub fn apply_frame(&mut self, frame: StreamFrame, t: f64) {
        let sense = match self.catalog.stream(&frame.id).map(|s| &s.kind) {
            Some(StreamKind::Level { .. }) => Some(Sense::Level),
            Some(StreamKind::GainReduction) => Some(Sense::Reduction),
            _ => None,
        };
        if let Some(sense) = sense {
            for (id, control) in &self.surface.controls {
                // Every stream the control can show: its binding's, and
                // each tap its meter switch can select.
                let bound = match &control.binding {
                    Some(Binding { source: Source::Stream { id: sid, channel }, .. }) => Some((sid, *channel)),
                    _ => None,
                };
                let taps = control.select.iter().flat_map(|s| s.taps.iter().flatten()).map(|tap| (&tap.stream, tap.channel));
                for (sid, channel) in bound.into_iter().chain(taps) {
                    if *sid != frame.id {
                        continue;
                    }
                    let ch = channel.unwrap_or(0);
                    if let Some(db) = frame.values.get(usize::from(ch)) {
                        self.meters.entry((*id, sid.clone(), ch)).or_insert_with(|| Ballistics::for_sense(sense)).push(*db, t);
                    }
                }
            }
        }
        self.streams.insert(frame.id.clone(), frame.values);
    }

    /// The stream a meter reads right now: its switch's selected tap (or
    /// `None` when the switch is off), else its binding's stream.
    fn tap(&self, control: &Control) -> Option<MeterTap> {
        if let Some(sel) = &control.select {
            let info = self.catalog.param(&sel.param);
            let v = self.values.get(&sel.param).copied().unwrap_or_else(|| info.map_or(0.0, |p| p.default));
            let n = match info.map(|p| &p.kind) {
                Some(ParamKind::Choice { options }) => options.len(),
                Some(ParamKind::Toggle) => 2,
                _ => sel.taps.len(),
            };
            let i = if n < 2 { 0 } else { (v.clamp(0.0, 1.0) * (n - 1) as f64).round() as usize };
            return sel.tap(i).cloned();
        }
        match &control.binding {
            Some(Binding { source: Source::Stream { id, channel }, .. }) => {
                Some(MeterTap { stream: id.clone(), channel: *channel, offset_db: 0.0 })
            }
            _ => None,
        }
    }

    /// A meter's state over `scale` for its current tap: the ballistic
    /// level, or its rest (silence; 0 dB of gain reduction) before data.
    fn meter_state(&self, id: NodeId, control: &Control, scale: &MeterScale, t: f64) -> ControlState {
        let Some(tap) = self.tap(control) else {
            // Switched off: the movement rests at its left stop.
            return ControlState::default();
        };
        let sense = match self.catalog.stream(&tap.stream).map(|s| &s.kind) {
            Some(StreamKind::GainReduction) => Sense::Reduction,
            _ => Sense::Level,
        };
        let m = self.meters.get(&(id, tap.stream.clone(), tap.channel.unwrap_or(0)));
        // A level in dBFS reads against the meter's reference (0 VU =
        // −18 dBFS); gain reduction is already relative to unity.
        let offset = tap.offset_db - if sense == Sense::Level { control.reference_dbfs.unwrap_or(0.0) } else { 0.0 };
        let level = m.map_or(sense.rest_db(), |m| m.level_db(t)) + offset;
        let peak = m.map(|m| m.peak_hold_db(t) + offset);
        let fmt = control.format.clone().unwrap_or_default();
        // A bar reads its held peak; a needle its level. Before data the
        // text stays as authored (a placeholder).
        let shown = if matches!(control.kind, ControlKind::Meter { .. }) { peak } else { m.map(|_| level) };
        ControlState {
            position: scale.position(level),
            peak: peak.map(|p| scale.position(p)),
            value_text: shown.map(|db| fmt.db(db)),
            ..ControlState::default()
        }
    }

    // ── input → engine ────────────────────────────────────────────────────

    /// The topmost control under a screen point (last in paint order wins),
    /// through any rotation/scale of the control's containers.
    #[must_use]
    pub fn hit(&self, x: f64, y: f64) -> Option<NodeId> {
        let order = self.surface.scene.walk(self.surface.scene.root);
        order.into_iter().rev().find(|id| {
            let (x, y) = self.local(*id, x, y);
            self.surface.controls.get(id).is_some_and(|c| c.kind.is_input())
                && self.boxes.get(id).is_some_and(|b| x >= b.x && y >= b.y && x <= b.x + b.width && y <= b.y + b.height)
        })
    }

    /// Feed pointer input at time `t` (ms). A control captures the pointer
    /// from `Down` until `Up`/`Cancel`.
    pub fn pointer(&mut self, input: Input, t: f64) -> Vec<EngineCall> {
        let target = match input {
            Input::Down { x, y, .. } => {
                self.captured = self.hit(x, y);
                self.captured
            }
            Input::Wheel { .. } | Input::DoubleClick | Input::Key { .. } => self.captured,
            _ => self.captured,
        };
        let Some(id) = target else { return Vec::new() };
        let out = self.feed(id, input, t);
        if matches!(input, Input::Up { .. } | Input::Cancel { .. }) {
            self.captured = None;
        }
        out
    }

    /// Feed input to one control directly (focused keyboard input, a mapped
    /// hardware footswitch, wheel over a hovered control).
    pub fn feed(&mut self, id: NodeId, input: Input, t: f64) -> Vec<EngineCall> {
        let Some(control) = self.surface.controls.get(&id).cloned() else { return Vec::new() };
        let position = self.position(id, &control);
        let bounds = self.boxes.get(&id).copied().unwrap_or_default();
        // Gestures work in the control's own (box) space: a fader in a
        // rotated frame slides along its rotated track.
        let input = match input {
            Input::Down { x, y, t, mods } => {
                let (x, y) = self.local(id, x, y);
                Input::Down { x, y, t, mods }
            }
            Input::Move { x, y, t, mods } => {
                let (x, y) = self.local(id, x, y);
                Input::Move { x, y, t, mods }
            }
            Input::Up { x, y, t } => {
                let (x, y) = self.local(id, x, y);
                Input::Up { x, y, t }
            }
            other => other,
        };
        let outputs = self.gestures.entry(id).or_insert_with(|| Gesture::new(control.kind.clone())).handle(input, position, bounds);
        let mut calls = Vec::new();
        for o in outputs {
            self.output(id, &control, o, t, &mut calls);
        }
        calls
    }

    /// Advance time: flush coalesced edits, fire footswitch holds.
    pub fn poll(&mut self, t: f64) -> Vec<EngineCall> {
        let mut calls: Vec<EngineCall> = self
            .coalescer
            .poll(t)
            .into_iter()
            .map(|e| EngineCall::SetParam { addr: e.addr, value: e.value, phase: e.phase })
            .collect();
        if let Some(id) = self.captured {
            calls.extend(self.feed(id, Input::Tick { t }, t));
        }
        calls
    }

    fn output(&mut self, id: NodeId, control: &Control, o: Output, t: f64, calls: &mut Vec<EngineCall>) {
        match o {
            Output::Set { position, phase } => self.set_position(id, control.binding.as_ref(), position, phase, t, calls),
            Output::SetXy { x, y, phase } => {
                self.set_position(id, control.binding.as_ref(), x, phase, t, calls);
                self.local_positions.insert(id, y);
                if let Some(yb) = &control.y_binding {
                    self.send(yb, y, phase, t, calls);
                }
            }
            Output::Reset => {
                let position = control.default.unwrap_or_else(|| self.default_position(control));
                self.set_position(id, control.binding.as_ref(), position, EditPhase::End, t, calls);
            }
            Output::Fire | Output::Tap => match control.binding.as_ref().map(|b| &b.source) {
                Some(Source::Action { id: action }) => calls.push(EngineCall::Invoke { action: action.clone() }),
                // A tapped footswitch bound to a toggle param flips it.
                Some(Source::Param { .. }) => {
                    let position = if self.position(id, control) >= 0.5 { 0.0 } else { 1.0 };
                    self.set_position(id, control.binding.as_ref(), position, EditPhase::End, t, calls);
                }
                _ => {}
            },
            Output::Hold | Output::HoldRelease => {
                // Hold semantics are the binding's business (a hold action);
                // record it locally so the design can show it (LED).
                self.local_positions.insert(id, if matches!(o, Output::Hold) { 1.0 } else { 0.0 });
            }
        }
    }

    fn set_position(
        &mut self,
        id: NodeId,
        binding: Option<&Binding>,
        position: f64,
        phase: EditPhase,
        t: f64,
        calls: &mut Vec<EngineCall>,
    ) {
        match binding {
            Some(b) => self.send(b, position, phase, t, calls),
            None => {
                self.local_positions.insert(id, position);
            }
        }
    }

    fn send(&mut self, b: &Binding, position: f64, phase: EditPhase, t: f64, calls: &mut Vec<EngineCall>) {
        let Source::Param { addr } = &b.source else { return };
        let value = b.to_target(position);
        // Optimistic: show the edit immediately; the engine echo confirms.
        self.values.insert(addr.clone(), value);
        calls.extend(
            self.coalescer
                .push(addr.clone(), value, phase, t)
                .into_iter()
                .map(|e| EngineCall::SetParam { addr: e.addr, value: e.value, phase: e.phase }),
        );
    }

    fn default_position(&self, control: &Control) -> f64 {
        match control.binding.as_ref() {
            Some(b @ Binding { source: Source::Param { addr }, .. }) => {
                self.catalog.param(addr).map_or(0.0, |p| b.to_position(p.default))
            }
            _ => 0.0,
        }
    }

    /// A control's current position (0..1).
    #[must_use]
    pub fn position(&self, id: NodeId, control: &Control) -> f64 {
        match control.binding.as_ref() {
            Some(b @ Binding { source: Source::Param { addr }, .. }) => {
                self.values.get(addr).map_or(0.0, |v| b.to_position(*v))
            }
            _ => self.local_positions.get(&id).copied().unwrap_or(0.0),
        }
    }

    // ── surface → screen ──────────────────────────────────────────────────

    /// What each control shows right now.
    #[must_use]
    pub fn state(&self, id: NodeId, control: &Control, t: f64) -> ControlState {
        // The binding's curve shapes a meter's dB span, so a needle follows
        // a printed scale law (a VU card crowds its low end) and a ladder's
        // lamps land on their printed thresholds.
        let curve = || control.binding.as_ref().map_or(surface_proto::Curve::Linear, |b| b.curve.clone());
        let level_stream = |sid: &EntryId| {
            matches!(self.catalog.stream(sid).map(|s| &s.kind), Some(StreamKind::Level { .. } | StreamKind::GainReduction))
        };
        match (&control.kind, control.binding.as_ref().map(|b| &b.source)) {
            (ControlKind::Gauge { floor_db, ceiling_db, .. }, Some(Source::Stream { id: sid, .. })) if level_stream(sid) || control.select.is_some() => {
                let scale = MeterScale { floor_db: *floor_db, ceiling_db: *ceiling_db, curve: curve() };
                self.meter_state(id, control, &scale, t)
            }
            (ControlKind::Meter { .. }, Some(Source::Stream { id: sid, .. })) if level_stream(sid) || control.select.is_some() => {
                let scale = MeterScale { curve: curve(), ..self.meter_scale.clone() };
                self.meter_state(id, control, &scale, t)
            }
            (_, Some(Source::Stream { id: sid, channel })) => {
                let v = self.streams.get(sid).and_then(|v| v.get(usize::from(channel.unwrap_or(0)))).copied();
                let range = self.catalog.stream(sid).and_then(|s| match &s.kind {
                    StreamKind::Scalar { range } => Some(range.clone()),
                    _ => None,
                });
                ControlState {
                    position: match (&range, v) {
                        (Some(r), Some(v)) => r.to_normalized(v),
                        _ => 0.0,
                    },
                    value_text: v.map(|v| match (range, &control.format) {
                        (Some(r), Some(f)) => r.format_as(v, f),
                        (Some(r), None) => r.format(v),
                        (None, Some(f)) => surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None).format_as(v, f),
                        (None, None) => format!("{v:.1}"),
                    }),
                    ..ControlState::default()
                }
            }
            (_, Some(Source::Param { addr })) => {
                let info = self.catalog.param(addr);
                let value = self.values.get(addr).copied().unwrap_or(0.0);
                let lit = match control.kind {
                    ControlKind::Button { mode: surface_proto::ButtonMode::Select { index, count } } => {
                        let at = surface_proto::ButtonMode::select_position(index, count);
                        Some((self.position(id, control) - at).abs() < 0.5 / f64::from(count.max(2) - 1))
                    }
                    _ => None,
                };
                ControlState {
                    lit,
                    position: self.position(id, control),
                    bipolar: info.is_some_and(|p| p.bipolar),
                    value_text: info.map(|p| p.display_as(value, control.format.as_ref())),
                    label_text: info.map(|p| p.short_name.clone().unwrap_or_else(|| p.name.clone())),
                    ..ControlState::default()
                }
            }
            (_, Some(Source::Action { id: aid })) => ControlState {
                position: self.local_positions.get(&id).copied().unwrap_or(0.0),
                label_text: self.catalog.action(aid).map(|a| a.name.clone()),
                ..ControlState::default()
            },
            _ => ControlState { position: self.local_positions.get(&id).copied().unwrap_or(0.0), ..ControlState::default() },
        }
    }

    /// The scene to draw at time `t` (ms): the design with every control's
    /// slots driven by its current state.
    #[must_use]
    pub fn frame(&self, t: f64) -> Scene {
        let mut scene = self.surface.scene.clone();
        for (id, control) in &self.surface.controls {
            let state = self.state(*id, control, t);
            let ops = control_ops(&self.surface.scene, *id, control, &state);
            // Slot ops only touch existing nodes; a failure means a dangling
            // slot, which lint reports — skip that control rather than abort.
            let _ = scene.transact(ops);
        }
        scene
    }

    /// Whether any control needs redrawing over time (meters falling,
    /// a gesture in flight) — lets hosts sleep when nothing moves.
    #[must_use]
    pub fn animating(&self) -> bool {
        self.captured.is_some() || !self.meters.is_empty()
    }

    /// Controls whose parameter is a choice/toggle (for selector hit-mapping).
    #[must_use]
    pub fn choice_count(&self, control: &Control) -> Option<usize> {
        match control.binding.as_ref().map(|b| &b.source) {
            Some(Source::Param { addr }) => match self.catalog.param(addr).map(|p| &p.kind) {
                Some(ParamKind::Choice { options }) => Some(options.len()),
                Some(ParamKind::Toggle) => Some(2),
                _ => None,
            },
            _ => None,
        }
    }
}

// ── transforms (the renderer's group semantics) ──────────────────────────

fn is_translation(t: &Transform) -> bool {
    const EPS: f64 = 1e-9;
    (t.a - 1.0).abs() <= EPS && t.b.abs() <= EPS && t.c.abs() <= EPS && (t.d - 1.0).abs() <= EPS
}

/// `p ∘ q`: apply `q`, then `p`.
fn compose(p: &Transform, q: &Transform) -> Transform {
    Transform {
        a: p.a * q.a + p.b * q.c,
        b: p.a * q.b + p.b * q.d,
        tx: p.a * q.tx + p.b * q.ty + p.tx,
        c: p.c * q.a + p.d * q.c,
        d: p.c * q.b + p.d * q.d,
        ty: p.c * q.tx + p.d * q.ty + p.ty,
    }
}

fn invert(t: &Transform) -> Option<Transform> {
    let det = t.a * t.d - t.b * t.c;
    if det.abs() < 1e-12 {
        return None;
    }
    let (a, b, c, d) = (t.d / det, -t.b / det, -t.c / det, t.a / det);
    Some(Transform { a, b, tx: -(a * t.tx + b * t.ty), c, d, ty: -(c * t.tx + d * t.ty) })
}

fn apply(t: &Transform, x: f64, y: f64) -> (f64, f64) {
    (t.a * x + t.b * y + t.tx, t.c * x + t.d * y + t.ty)
}

/// How the renderer draws `id`'s flattened box on screen: every container
/// on the way down whose transform has a rotation/scale/skew wraps its
/// subtree in that linear part pivoted about its own box origin (groups and
/// boolean ops are transparent, as in Figma), and the node itself is turned
/// about its origin. `None` when nothing on the way is transformed.
fn box_to_screen(scene: &Scene, id: NodeId, boxes: &HashMap<NodeId, Bounds>) -> Option<Transform> {
    let mut chain = vec![id];
    let mut up = scene.node(id)?.parent;
    while let Some(p) = up {
        chain.push(p);
        up = scene.node(p).and_then(|n| n.parent);
    }
    chain.reverse();
    let mut at = (0.0, 0.0);
    let mut world: Option<Transform> = None;
    for nid in chain {
        let n = scene.node(nid)?;
        at = (at.0 + n.transform.tx, at.1 + n.transform.ty);
        let (ox, oy) = boxes.get(&nid).map_or(at, |b| (b.x, b.y));
        at = (ox, oy);
        let t = &n.transform;
        if is_translation(t) || matches!(n.kind, NodeKind::Group | NodeKind::BooleanOperation { .. }) {
            continue;
        }
        let g = Transform { a: t.a, b: t.b, tx: ox - (t.a * ox + t.b * oy), c: t.c, d: t.d, ty: oy - (t.c * ox + t.d * oy) };
        world = Some(world.map_or(g, |w| compose(&w, &g)));
    }
    world
}

#[cfg(test)]
mod tests {
    use document_proto::{Node, Op, Size};
    use surface_proto::{Catalog, ControlKind};

    use super::*;

    fn insert(scene: &mut Scene, parent: NodeId, node: Node) -> NodeId {
        let id = node.id;
        scene.apply(Op::Insert { node, parent, index: u32::MAX }).unwrap();
        id
    }

    /// A 100×100 panel at (100, 0) turned 90° clockwise about its corner
    /// (so it covers x ∈ [0, 100]), holding a 20×40 knob at its corner —
    /// drawn at x ∈ [60, 100], y ∈ [0, 20].
    fn turned_panel() -> (Runtime, NodeId, HashMap<NodeId, Bounds>) {
        let mut scene = Scene::new("Stage");
        let page = scene.pages()[0];
        let mut panel = Node::new(NodeKind::Frame, "panel");
        panel.size = Size { width: 100.0, height: 100.0 };
        panel.transform = Transform { a: 0.0, b: -1.0, tx: 100.0, c: 1.0, d: 0.0, ty: 0.0 };
        let panel = insert(&mut scene, page, panel);
        let mut knob = Node::new(NodeKind::Frame, "knob");
        knob.size = Size { width: 20.0, height: 40.0 };
        let knob = insert(&mut scene, panel, knob);
        let controls = [(knob, Control::new(ControlKind::knob()))].into_iter().collect();
        let boxes = render::build_scene_layout_boxes(&scene, page)
            .into_iter()
            .map(|(id, b)| (id, Bounds { x: b.x, y: b.y, width: b.width, height: b.height }))
            .collect();
        (Runtime::new(Surface { scene, controls }, Catalog::default()), knob, boxes)
    }

    #[test]
    fn a_control_in_a_rotated_frame_is_hit_where_it_is_drawn() {
        let (mut rt, knob, boxes) = turned_panel();
        // The layout box is the unturned one: x ∈ [100, 120], y ∈ [0, 40].
        assert_eq!(boxes[&knob], Bounds { x: 100.0, y: 0.0, width: 20.0, height: 40.0 });
        rt.set_boxes(boxes.clone());
        assert_eq!(rt.hit(80.0, 10.0), Some(knob), "where it is drawn");
        assert_eq!(rt.hit(62.0, 18.0), Some(knob));
        assert_eq!(rt.hit(110.0, 30.0), None, "not where its unturned box is");
        assert_eq!(rt.hit(50.0, 10.0), None);

        // Only the controls' boxes: ancestors pivot at their authored offsets.
        rt.set_boxes([(knob, boxes[&knob])]);
        assert_eq!(rt.hit(80.0, 10.0), Some(knob));
        assert_eq!(rt.hit(110.0, 30.0), None);

        // A host that tracks world transforms can hand them in directly.
        let turn = Transform { a: 0.0, b: -1.0, tx: 100.0, c: 1.0, d: 0.0, ty: -100.0 };
        rt.set_transformed_boxes([(knob, boxes[&knob], Some(turn))]);
        assert_eq!(rt.hit(80.0, 10.0), Some(knob));
        rt.set_transformed_boxes([(knob, boxes[&knob], None)]);
        assert_eq!(rt.hit(110.0, 30.0), Some(knob), "no transform: the box as given");
    }

    #[test]
    fn untransformed_controls_keep_their_boxes() {
        let (mut rt, knob, _) = turned_panel();
        let mut scene = rt.surface().scene.clone();
        let panel = scene.node(knob).and_then(|n| n.parent).unwrap();
        scene.node_mut(panel).unwrap().transform = Transform::translate(100.0, 0.0);
        rt.surface.scene = scene;
        rt.set_boxes([(knob, Bounds { x: 100.0, y: 0.0, width: 20.0, height: 40.0 })]);
        assert!(rt.to_box.is_empty());
        assert_eq!(rt.hit(110.0, 30.0), Some(knob));
    }
}
