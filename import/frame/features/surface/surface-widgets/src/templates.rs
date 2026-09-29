//! Catalog → surface. Deterministic layouts for the common shapes of a
//! live rig (a panel per parameter group, a footswitch row, an output strip)
//! — consistent by construction, and the starting point an AI designer
//! refines rather than a blank page.

use std::collections::BTreeMap;

use document_proto::{
    AutoLayout, CornerRadii, CrossAxisAlign, LayoutDirection, Node, NodeId, NodeKind, Paint, Scene,
};
use surface_proto::{Binding, ButtonMode, Catalog, EntryId, ParamInfo, ParamKind, Source, StreamKind};
use surface_runtime::Surface;

use crate::theme::{ControlSize, Theme};
use crate::widgets::Builder;

impl Builder<'_> {
    /// An auto-layout container. Call [`hug`] once the tree is built to
    /// size it to its content.
    pub fn stack(&mut self, parent: NodeId, name: &str, direction: LayoutDirection, gap: f64, padding: f64) -> NodeId {
        let mut n = Node::new(NodeKind::Frame, name);
        n.layout = Some(AutoLayout {
            direction,
            gap,
            padding_left: padding,
            padding_right: padding,
            padding_top: padding,
            padding_bottom: padding,
            wrap: false,
            main_align: None,
            cross_align: Some(CrossAxisAlign::Start),
            hug_main: true,
            hug_cross: true,
        });
        self.add(parent, n)
    }

    /// A titled panel: a column with a caption and a row for controls.
    /// Returns `(panel, row)`.
    pub fn panel(&mut self, parent: NodeId, title: &str) -> (NodeId, NodeId) {
        let t = self.theme.clone();
        let panel = self.stack(parent, title, LayoutDirection::Vertical, t.gap * 0.75, t.padding);
        if let Some(n) = self.scene.node_mut(panel) {
            n.fills = vec![Paint::solid(t.panel)];
            n.corner_radii = CornerRadii::uniform(t.radius);
        }
        self.caption(panel, &title.to_uppercase(), t.label_size + 1.0, 160.0);
        let row = self.stack(panel, "controls", LayoutDirection::Horizontal, t.gap, 0.0);
        (panel, row)
    }

    /// The natural control for a parameter.
    pub fn control_for(&mut self, parent: NodeId, p: &ParamInfo, size: ControlSize) -> NodeId {
        let binding = Some(Binding::param(p.addr.0.clone()));
        match &p.kind {
            ParamKind::Toggle => self.button(parent, &p.name, size, ButtonMode::Toggle, binding),
            ParamKind::Continuous | ParamKind::Choice { .. } => self.knob(parent, &p.name, size, binding),
        }
    }
}

/// Size every hugging container under `id` to its content.
pub fn hug(scene: &mut Scene, id: NodeId) {
    let ops = scene.hug_ops(id);
    scene.transact(ops).expect("hug only resizes existing nodes");
}

/// Options for [`surface_for_catalog`].
#[derive(Debug, Clone)]
pub struct Options {
    pub theme: Theme,
    pub knob_size: ControlSize,
    /// Footswitches for every toggle param and action.
    pub footswitch_row: bool,
    /// Master fader + meters, when the catalog has a level stream.
    pub output_strip: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { theme: Theme::default(), knob_size: ControlSize::Medium, footswitch_row: true, output_strip: true }
    }
}

/// A complete, fully bound surface for `catalog`:
///
/// ```text
/// ┌ Surface ──────────────────────────────────────────────┐
/// │ ┌ Amp ────────────┐ ┌ Drive ───┐ … │ ┌ Output ┐        │
/// │ │ ◯ ◯ ◯ ◯ ◯ ◯     │ │ ◯ ◯ [on] │   │ │ ▮ ▯▯   │        │
/// │ └─────────────────┘ └──────────┘   │ └────────┘        │
/// │ [Drive] [Delay] [Reverb] [Tap] [Next] …                 │
/// └───────────────────────────────────────────────────────┘
/// ```
#[must_use]
pub fn surface_for_catalog(catalog: &Catalog, opts: &Options) -> Surface {
    let mut scene = Scene::new("Surface");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let t = &opts.theme;
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: t };

    let root = b.stack(page, &format!("{} surface", catalog.id), LayoutDirection::Vertical, t.gap * 1.5, t.padding * 1.5);
    if let Some(n) = b.scene.node_mut(root) {
        n.fills = vec![Paint::solid(t.background)];
    }
    let top = b.stack(root, "panels", LayoutDirection::Horizontal, t.gap, 0.0);

    let output_params: Vec<&ParamInfo> = catalog.params.iter().filter(|p| p.group.as_deref() == Some("Output")).collect();
    for (group, params) in catalog.groups() {
        let name = group.unwrap_or("Other");
        if opts.output_strip && name == "Output" {
            continue;
        }
        let continuous: Vec<_> = params.iter().filter(|p| !matches!(p.kind, ParamKind::Toggle)).collect();
        if continuous.is_empty() {
            continue;
        }
        let (_, row) = b.panel(top, name);
        for p in continuous {
            b.control_for(row, p, opts.knob_size);
        }
    }

    if opts.output_strip {
        let level = catalog.streams.iter().find(|s| matches!(s.kind, StreamKind::Level { channels } if channels >= 1));
        let (_, row) = b.panel(top, "Output");
        let fader_param = output_params.iter().find(|p| matches!(p.kind, ParamKind::Continuous) && !p.bipolar);
        if let Some(p) = fader_param {
            b.fader(row, &p.name, 160.0, Some(Binding::param(p.addr.0.clone())));
        }
        if let Some(s) = level
            && let StreamKind::Level { channels } = s.kind
        {
            for ch in 0..channels.min(2) {
                let binding = Binding { source: Source::Stream { id: s.id.clone(), channel: Some(ch) }, ..Binding::stream(s.id.0.clone()) };
                b.meter(row, &format!("{} {}", s.name, ["L", "R"][usize::from(ch)]), 196.0, Some(binding));
            }
        }
        for p in output_params.iter().filter(|p| p.bipolar) {
            b.knob(row, &p.name, ControlSize::Small, Some(Binding::param(p.addr.0.clone())));
        }
    }

    if opts.footswitch_row {
        let row = b.stack(root, "footswitches", LayoutDirection::Horizontal, t.gap, 0.0);
        for p in catalog.params.iter().filter(|p| matches!(p.kind, ParamKind::Toggle)) {
            let name = p.name.trim_end_matches(" On").to_string();
            b.footswitch(row, &name, Some(Binding::param(p.addr.0.clone())));
        }
        for a in &catalog.actions {
            b.footswitch(row, &a.name, Some(Binding { source: Source::Action { id: EntryId::new(a.id.0.clone()) }, ..Binding::action(a.id.0.clone()) }));
        }
    }

    hug(&mut scene, root);
    Surface { scene, controls }
}
