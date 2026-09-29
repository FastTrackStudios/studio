//! One running surface against an in-process engine, host-agnostic: the
//! window and the headless screenshot loop both drive this.

use std::collections::BTreeMap;

use document_proto::{NodeId, PaintKind, Scene};
use surface_memory::Memory;
use surface_proto::{Catalog, ControlKind, ParamAddr, ParamValue, Source as Binding, StreamFrame, StreamKind};
use surface_runtime::{Bounds, EngineCall, Input, Key, Modifiers, Runtime, Surface, sync};

use crate::input::{Clicks, View};

const FONT: &[u8] = include_bytes!("../../cli/assets/inter/Inter-Variable.ttf");
/// Background when the surface's root frame has no solid fill.
const DARK: peniko::Color = peniko::Color::from_rgb8(0x0e, 0x0f, 0x12);

pub struct Player {
    engine: Memory,
    rt: Runtime,
    page: NodeId,
    /// The page's top-level frames' union: what gets fit to the window.
    content: Bounds,
    background: peniko::Color,
    /// Any control bound to a stream (meters, readouts) → redraw continuously.
    live: bool,
    /// Last-clicked control: keyboard input goes here.
    focus: Option<NodeId>,
    clicks: Clicks,
    /// Last cursor position, surface coordinates.
    cursor: Option<(f64, f64)>,
}

impl Player {
    /// # Errors
    /// When the surface has no page, or the engine refuses.
    pub fn new(engine: Memory, surface: Surface) -> Result<Self, String> {
        let mut rt = pollster::block_on(sync::connect(&engine, surface)).map_err(|e| format!("engine: {e:?}"))?;
        let scene = &rt.surface().scene;
        let page = *scene.pages().first().ok_or("surface has no <Page>")?;
        let boxes = render::build_scene_layout_boxes(scene, page);
        let tops = scene.node(page).map(|p| p.children.clone()).unwrap_or_default();
        let content = union(tops.iter().filter_map(|id| boxes.get(id)).map(bounds));
        let background = tops.first().and_then(|id| solid_fill(scene, *id)).unwrap_or(DARK);
        let live = rt
            .surface()
            .controls
            .values()
            .any(|c| matches!(c.binding.as_ref().map(|b| &b.source), Some(Binding::Stream { .. })));
        rt.set_boxes(boxes.iter().map(|(id, b)| (*id, bounds(b))).collect::<Vec<_>>());
        Ok(Self { engine, rt, page, content, background, live, focus: None, clicks: Clicks::default(), cursor: None })
    }

    /// Swap in a new design, keeping the engine (and so every value).
    ///
    /// # Errors
    /// As [`Player::new`]; the old surface stays on error.
    pub fn swap(&mut self, surface: Surface) -> Result<(), String> {
        let next = Self::new(self.engine.clone(), surface)?;
        *self = Self { focus: None, clicks: Clicks::default(), cursor: self.cursor, ..next };
        Ok(())
    }

    #[must_use]
    pub fn content(&self) -> Bounds {
        self.content
    }

    #[must_use]
    pub fn background(&self) -> peniko::Color {
        self.background
    }

    #[must_use]
    pub fn engine(&self) -> &Memory {
        &self.engine
    }

    #[cfg(test)]
    #[must_use]
    pub fn runtime(&self) -> &Runtime {
        &self.rt
    }

    /// Whether something moves without input (meters, a drag in flight).
    #[must_use]
    pub fn animating(&self) -> bool {
        self.live || self.rt.animating()
    }

    /// Advance to `t` ms: synthetic telemetry in, coalesced edits out.
    pub fn tick(&mut self, t: f64) {
        for frame in telemetry(self.rt.catalog(), t / 1000.0) {
            self.rt.apply_frame(frame, t);
        }
        let calls = self.rt.poll(t);
        self.dispatch(calls);
    }

    // ── input (surface coordinates; `logical` is for click distance) ─────

    pub fn pointer_down(&mut self, at: (f64, f64), logical: (f64, f64), mods: Modifiers, t: f64) {
        self.cursor = Some(at);
        let double = self.clicks.press(logical.0, logical.1, t);
        if let Some(id) = self.rt.hit(at.0, at.1) {
            self.focus = Some(id);
        }
        let mut calls = self.rt.pointer(Input::Down { x: at.0, y: at.1, t, mods }, t);
        if double {
            calls.extend(self.rt.pointer(Input::DoubleClick, t));
        }
        self.dispatch(calls);
    }

    pub fn pointer_move(&mut self, at: (f64, f64), mods: Modifiers, t: f64) {
        self.cursor = Some(at);
        let calls = self.rt.pointer(Input::Move { x: at.0, y: at.1, t, mods }, t);
        self.dispatch(calls);
    }

    pub fn pointer_up(&mut self, t: f64) {
        let (x, y) = self.cursor.unwrap_or_default();
        let calls = self.rt.pointer(Input::Up { x, y, t }, t);
        self.dispatch(calls);
    }

    /// Capture lost (window blurred).
    pub fn cancel(&mut self, t: f64) {
        let calls = self.rt.pointer(Input::Cancel { t }, t);
        self.dispatch(calls);
    }

    /// Wheel over the hovered control.
    pub fn wheel(&mut self, notches: f64, mods: Modifiers, t: f64) {
        let Some(id) = self.cursor.and_then(|(x, y)| self.rt.hit(x, y)) else {
            return;
        };
        let calls = self.rt.feed(id, Input::Wheel { notches, mods }, t);
        self.dispatch(calls);
    }

    /// Keyboard to the last-clicked control.
    pub fn key(&mut self, key: Key, mods: Modifiers, t: f64) {
        let Some(id) = self.focus else { return };
        let calls = self.rt.feed(id, Input::Key { key, mods }, t);
        self.dispatch(calls);
    }

    fn dispatch(&mut self, calls: Vec<EngineCall>) {
        if calls.is_empty() {
            return;
        }
        let sent: Vec<ParamAddr> = calls
            .iter()
            .filter_map(|c| match c {
                EngineCall::SetParam { addr, .. } => Some(addr.clone()),
                _ => None,
            })
            .collect();
        if let Err(e) = pollster::block_on(sync::send(&self.engine, calls)) {
            eprintln!("frame: engine refused: {e:?}");
        }
        // Reflect what the engine actually holds (stepped params snap).
        // Only the sent params: others may have edits still coalescing.
        let echoed: Vec<ParamValue> = sent
            .into_iter()
            .filter_map(|addr| self.engine.value(&addr).map(|value| ParamValue { addr, value }))
            .collect();
        self.rt.apply_values(echoed);
    }

    // ── drawing ───────────────────────────────────────────────────────────

    /// Paint the frame at `t` ms through `view`.
    pub fn paint(&self, into: &mut vello::Scene, view: &View, t: f64) {
        let frame = self.rt.frame(t);
        render::paint_scene_into(into, &frame, self.page, view.affine(), Some(font()));
    }

    /// The first knob in paint order and its box (for scripted input).
    #[must_use]
    pub fn first_knob(&self) -> Option<(NodeId, Bounds)> {
        let s = self.rt.surface();
        let boxes = render::build_scene_layout_boxes(&s.scene, self.page);
        s.scene
            .walk(s.scene.root)
            .into_iter()
            .find(|id| s.controls.get(id).is_some_and(|c| matches!(c.kind, ControlKind::Knob { .. })))
            .and_then(|id| boxes.get(&id).map(|b| (id, bounds(b))))
    }

    /// The param a control is bound to.
    #[must_use]
    pub fn param_of(&self, id: NodeId) -> Option<ParamAddr> {
        match self.rt.surface().controls.get(&id)?.binding.as_ref()?.source.clone() {
            Binding::Param { addr } => Some(addr),
            _ => None,
        }
    }
}

#[must_use]
pub fn font() -> render::TextFontRef<'static> {
    render::TextFontRef { bytes: FONT, index: 0 }
}

/// A one-line red banner, `width` logical px wide, as a scene to paint
/// over the surface (so it uses the same text path as everything else).
#[must_use]
pub fn banner(message: &str, width: f64) -> Option<(Scene, NodeId)> {
    let text = serde_json::to_string(message).ok()?;
    let src = format!(
        r##"<Document><Page><Frame w={width} h=28 clip fill="#c62828e6"><Text x=10 y=6 w={w} h=16 size=12 fill="#ffffff" text={text} /></Frame></Page></Document>"##,
        w = (width - 20.0).max(0.0),
    );
    let scene = document_text::parse(&src, BTreeMap::new()).ok()?;
    let page = *scene.pages().first()?;
    Some((scene, page))
}

/// Synthetic telemetry at `t` seconds — the same signals as
/// [`Memory::tick`], computed here so the host feeds the runtime directly.
#[must_use]
pub fn telemetry(catalog: &Catalog, t: f64) -> Vec<StreamFrame> {
    catalog
        .streams
        .iter()
        .map(|s| {
            let values = match &s.kind {
                StreamKind::Level { channels } => (0..*channels)
                    .map(|c| -18.0 + 12.0 * (t * 2.3 + f64::from(c) * 0.7).sin() - 6.0 * (t * 0.37).cos().abs())
                    .collect(),
                StreamKind::GainReduction => vec![-3.0 - 3.0 * (t * 1.7).sin().abs()],
                StreamKind::Spectrum { bins } => (0..*bins)
                    .map(|b| -30.0 - 0.3 * f64::from(b) + 6.0 * (t * 3.0 + f64::from(b) * 0.2).sin())
                    .collect(),
                StreamKind::Tuner => vec![64.0, 8.0 * (t * 0.9).sin()],
                StreamKind::Scalar { range } => vec![range.to_real(0.5 + 0.5 * (t * 0.5).sin())],
            };
            StreamFrame { id: s.id.clone(), values }
        })
        .collect()
}

fn bounds(b: &render::LayoutBox) -> Bounds {
    Bounds { x: b.x, y: b.y, width: b.width, height: b.height }
}

fn union(boxes: impl Iterator<Item = Bounds>) -> Bounds {
    boxes
        .reduce(|a, b| {
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            Bounds {
                x,
                y,
                width: (a.x + a.width).max(b.x + b.width) - x,
                height: (a.y + a.height).max(b.y + b.height) - y,
            }
        })
        .unwrap_or_default()
}

fn solid_fill(scene: &Scene, id: NodeId) -> Option<peniko::Color> {
    scene.node(id)?.fills.iter().rev().find(|p| p.visible).and_then(|p| match p.kind {
        PaintKind::Solid { color: c } => {
            let u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(peniko::Color::from_rgb8(u8(c.r), u8(c.g), u8(c.b)))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use surface_widgets::{Options, surface_for_catalog};

    fn demo() -> Player {
        let catalog = surface_memory::demo_catalog();
        Player::new(Memory::demo(), surface_for_catalog(&catalog, &Options::default())).unwrap()
    }

    #[test]
    fn content_and_background_come_from_the_root_frame() {
        let p = demo();
        let c = p.content();
        assert!(c.width > 100.0 && c.height > 100.0, "{c:?}");
        assert_eq!(p.background(), DARK, "stage-dark root fill is #0e0f12");
        assert!(p.animating(), "demo has meters");
    }

    #[test]
    fn drag_changes_the_engine_value() {
        let mut p = demo();
        let (id, b) = p.first_knob().unwrap();
        let addr = p.param_of(id).unwrap();
        let before = p.engine().value(&addr).unwrap();
        let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        p.pointer_down((cx, cy), (cx, cy), Modifiers::default(), 0.0);
        p.pointer_move((cx, cy - 60.0), Modifiers::default(), 16.0);
        p.pointer_up(32.0);
        let after = p.engine().value(&addr).unwrap();
        assert!((after - before - 0.3).abs() < 1e-9, "{before} → {after}");
    }

    #[test]
    fn double_click_resets_and_keys_go_to_focus() {
        let mut p = demo();
        let (id, b) = p.first_knob().unwrap();
        let addr = p.param_of(id).unwrap();
        let default = p.engine().value(&addr).unwrap();
        let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        p.pointer_down((cx, cy), (cx, cy), Modifiers::default(), 0.0);
        p.pointer_up(10.0);
        p.key(Key::PageUp, Modifiers::default(), 20.0);
        assert!(p.engine().value(&addr).unwrap() > default);
        p.pointer_down((cx, cy), (cx, cy), Modifiers::default(), 1000.0);
        p.pointer_up(1010.0);
        p.pointer_down((cx + 1.0, cy), (cx + 1.0, cy), Modifiers::default(), 1200.0);
        p.pointer_up(1210.0);
        assert!((p.engine().value(&addr).unwrap() - default).abs() < 1e-9, "double-click resets");
    }

    #[test]
    fn swap_keeps_engine_values() {
        let mut p = demo();
        let (id, _) = p.first_knob().unwrap();
        let addr = p.param_of(id).unwrap();
        pollster::block_on(sync::send(
            p.engine(),
            vec![EngineCall::SetParam { addr: addr.clone(), value: 0.9, phase: surface_proto::EditPhase::End }],
        ))
        .unwrap();
        let catalog = surface_memory::demo_catalog();
        p.swap(surface_for_catalog(&catalog, &Options::default())).unwrap();
        let (id, _) = p.first_knob().unwrap();
        let control = p.runtime().surface().controls[&id].clone();
        assert!((p.runtime().position(id, &control) - 0.9).abs() < 1e-9);
    }

    #[test]
    fn banner_parses_any_message() {
        let (scene, page) = banner("3:7: expected '>' \"quoted\" <tag> {brace}", 600.0).unwrap();
        assert_eq!(scene.node(page).map(|p| p.children.len()), Some(1));
    }

    #[test]
    fn telemetry_matches_the_catalog_streams() {
        let c = surface_memory::demo_catalog();
        let frames = telemetry(&c, 1.25);
        assert_eq!(frames.len(), c.streams.len());
        assert_eq!(frames[0].values.len(), 2, "stereo out_level");
    }
}
