//! One running surface: design + controls + live values.
//!
//! The runtime is transport-agnostic and synchronous. The host (desktop
//! window, plugin editor, iPad app, browser) feeds it input, engine updates
//! and time; it returns [`EngineCall`]s to send and a scene to draw. The
//! async glue to a [`SurfaceEngine`](surface_proto::SurfaceEngine) is
//! [`crate::sync`].

use std::collections::{BTreeMap, HashMap};

use document_proto::{NodeId, Scene};
use surface_proto::{
    Binding, Catalog, Control, ControlKind, EditPhase, EntryId, ParamAddr, ParamKind, ParamValue, Source, StreamFrame,
    StreamKind,
};

use crate::coalesce::Coalescer;
use crate::gesture::{Bounds, Gesture, Input, Output};
use crate::meter::{Ballistics, MeterScale};
use crate::visual::{ControlState, control_ops};

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
    meters: HashMap<(NodeId, u8), Ballistics>,
    gestures: HashMap<NodeId, Gesture>,
    /// XY pads' second axis, and footswitch/button positions not tied to params.
    local_positions: HashMap<NodeId, f64>,
    captured: Option<NodeId>,
    boxes: HashMap<NodeId, Bounds>,
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

    /// The on-screen boxes of control nodes (from the host's layout pass),
    /// for hit testing and position-based gestures.
    pub fn set_boxes(&mut self, boxes: impl IntoIterator<Item = (NodeId, Bounds)>) {
        self.boxes = boxes.into_iter().filter(|(id, _)| self.surface.controls.contains_key(id)).collect();
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
        let is_level = matches!(
            self.catalog.stream(&frame.id).map(|s| &s.kind),
            Some(StreamKind::Level { .. } | StreamKind::GainReduction)
        );
        if is_level {
            for (id, control) in &self.surface.controls {
                if let Some(Binding { source: Source::Stream { id: sid, channel }, .. }) = &control.binding
                    && *sid == frame.id
                {
                    let ch = channel.unwrap_or(0);
                    if let Some(db) = frame.values.get(usize::from(ch)) {
                        self.meters.entry((*id, ch)).or_default().push(*db, t);
                    }
                }
            }
        }
        self.streams.insert(frame.id.clone(), frame.values);
    }

    // ── input → engine ────────────────────────────────────────────────────

    /// The topmost control under a point (last in paint order wins).
    #[must_use]
    pub fn hit(&self, x: f64, y: f64) -> Option<NodeId> {
        let order = self.surface.scene.walk(self.surface.scene.root);
        order.into_iter().rev().find(|id| {
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
        match (&control.kind, control.binding.as_ref().map(|b| &b.source)) {
            (ControlKind::Meter { .. }, Some(Source::Stream { channel, .. })) => {
                let ch = channel.unwrap_or(0);
                let m = self.meters.get(&(id, ch));
                ControlState {
                    position: m.map_or(0.0, |m| self.meter_scale.position(m.level_db(t))),
                    peak: m.map(|m| self.meter_scale.position(m.peak_hold_db(t))),
                    value_text: m.map(|m| format!("{:.1} dB", m.peak_hold_db(t))),
                    ..ControlState::default()
                }
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
                    value_text: v.map(|v| range.map_or_else(|| format!("{v:.1}"), |r| r.format(v))),
                    ..ControlState::default()
                }
            }
            (_, Some(Source::Param { addr })) => {
                let info = self.catalog.param(addr);
                let value = self.values.get(addr).copied().unwrap_or(0.0);
                ControlState {
                    position: self.position(id, control),
                    bipolar: info.is_some_and(|p| p.bipolar),
                    value_text: info.map(|p| p.display(value)),
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
