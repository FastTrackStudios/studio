//! Surface lint.
//!
//! Checks a scene (design + controls) against an engine catalog and returns
//! [`Issue`]s phrased so a model can fix them without further context:
//! which node, what's wrong, and what to do. The AI design loop is
//! *write → lint → fix* until clean.
//!
//! | rule | severity | checks |
//! |---|---|---|
//! | `binding` | error | control unbound, bound to an unknown address, or to the wrong kind of source |
//! | `slot-dangling` | error | a slot names a node that isn't in the control's subtree |
//! | `slot-missing` | error | a control lacks the slots its kind needs to show its value |
//! | `midi-duplicate` | error | two controls listen to the same MIDI message |
//! | `overlap` | error | two input controls overlap (ambiguous touches) |
//! | `touch-target` | warning | an input control smaller than the minimum touch size |
//! | `size-inconsistent` | warning | same-kind controls in one group with different sizes |
//! | `contrast` | warning | text too close in luminance to what's behind it |
//! | `param-duplicate` | info | two controls bound to the same parameter |
//! | `unlabeled` | info | an input control with no label |

use std::collections::{BTreeMap, HashMap};

use document_proto::{Color, Node, NodeId, PaintKind, Scene};
use facet::Facet;
use render::LayoutBox;
use serde::{Deserialize, Serialize};
use surface_proto::{Catalog, Control, ControlKind, MidiMessage, SlotRole, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Issue {
    pub severity: Severity,
    pub rule: String,
    pub node: NodeId,
    /// The node's name, so messages read without a lookup.
    pub node_name: String,
    pub message: String,
    /// What to do about it.
    pub fix: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        write!(f, "{sev}[{}] \"{}\" ({}): {} — fix: {}", self.rule, self.node_name, self.node, self.message, self.fix)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Smallest input control edge, px.
    pub min_touch: f64,
    /// WCAG contrast ratio text must reach against its background.
    pub min_contrast: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self { min_touch: 44.0, min_contrast: 4.5 }
    }
}

/// Lint every control and text node under the scene's root.
#[must_use]
pub fn lint(scene: &Scene, catalog: &Catalog, config: &Config) -> Vec<Issue> {
    let boxes = render::build_scene_layout_boxes(scene, scene.root);
    let mut cx = Cx { scene, catalog, config, boxes: &boxes, out: Vec::new() };
    let controls: Vec<(&Node, &Control)> =
        scene.walk(scene.root).into_iter().filter_map(|id| scene.node(id)).filter_map(|n| n.control.as_ref().map(|c| (n, c))).collect();
    for (node, control) in &controls {
        cx.binding(node, control);
        cx.slots(node, control);
        cx.touch(node, control);
        cx.labeled(node, control);
    }
    cx.duplicates(&controls);
    cx.overlaps(&controls);
    cx.sizes(&controls);
    cx.contrast();
    cx.out.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.rule.cmp(&b.rule)));
    cx.out
}

/// `errors, warnings, infos`.
#[must_use]
pub fn counts(issues: &[Issue]) -> (usize, usize, usize) {
    let n = |s| issues.iter().filter(|i| i.severity == s).count();
    (n(Severity::Error), n(Severity::Warning), n(Severity::Info))
}

struct Cx<'a> {
    scene: &'a Scene,
    catalog: &'a Catalog,
    config: &'a Config,
    boxes: &'a HashMap<NodeId, LayoutBox>,
    out: Vec<Issue>,
}

impl Cx<'_> {
    fn push(&mut self, severity: Severity, rule: &str, node: &Node, message: String, fix: String) {
        self.out.push(Issue { severity, rule: rule.into(), node: node.id, node_name: node.name.clone(), message, fix });
    }

    fn binding(&mut self, node: &Node, control: &Control) {
        for issue in control.validate(self.catalog) {
            let fix = match &issue {
                surface_proto::BindingIssue::Unbound => format!(
                    "bind it: set `binding.source` to a {} from the catalog",
                    if control.kind.is_input() { "param or action" } else { "stream or param" }
                ),
                surface_proto::BindingIssue::UnknownParam { addr } => {
                    format!("use an existing address{}", nearest(&addr.0, self.catalog.params.iter().map(|p| p.addr.0.as_str())))
                }
                surface_proto::BindingIssue::UnknownAction { id } => {
                    format!("use an existing action{}", nearest(&id.0, self.catalog.actions.iter().map(|a| a.id.0.as_str())))
                }
                surface_proto::BindingIssue::UnknownStream { id } => {
                    format!("use an existing stream{}", nearest(&id.0, self.catalog.streams.iter().map(|s| s.id.0.as_str())))
                }
                surface_proto::BindingIssue::UnknownCollection { id } => format!(
                    "use an existing collection{}",
                    nearest(&id.0, self.catalog.collections.iter().map(|c| c.id.0.as_str()))
                ),
                surface_proto::BindingIssue::Mismatch { .. } => "change the control kind or bind a matching source".into(),
            };
            self.push(Severity::Error, "binding", node, issue.to_string(), fix);
        }
    }

    fn slots(&mut self, node: &Node, control: &Control) {
        let subtree = self.scene.walk(node.id);
        for slot in &control.slots {
            let id = NodeId(slot.node);
            if !subtree.contains(&id) || id == node.id && slot.role != SlotRole::Led {
                self.push(
                    Severity::Error,
                    "slot-dangling",
                    node,
                    format!("{:?} slot points at {} which is not inside this control", slot.role, id),
                    "point the slot at a descendant of the control, or remove it".into(),
                );
            }
        }
        let has = |r: SlotRole| control.slots.iter().any(|s| s.role == r);
        let needs: &[(&[SlotRole], &str)] = match control.kind {
            ControlKind::Knob { .. } => &[(&[SlotRole::Needle, SlotRole::ValueArc], "a Needle or a ValueArc")],
            ControlKind::Fader { .. } => &[(&[SlotRole::Fill, SlotRole::Thumb], "a Fill or a Thumb")],
            ControlKind::Meter { .. } => &[(&[SlotRole::Fill], "a Fill")],
            ControlKind::Readout => &[(&[SlotRole::Value], "a Value text")],
            ControlKind::Button { .. } | ControlKind::Footswitch { .. } | ControlKind::XyPad | ControlKind::Selector => &[],
        };
        for (any_of, what) in needs {
            if !any_of.iter().any(|r| has(*r)) {
                self.push(
                    Severity::Error,
                    "slot-missing",
                    node,
                    format!("{} has no slot to show its value", control.kind.name()),
                    format!("add {what} slot pointing at a child node"),
                );
            }
        }
    }

    fn touch(&mut self, node: &Node, control: &Control) {
        if !control.kind.is_input() {
            return;
        }
        let Some(b) = self.boxes.get(&node.id) else { return };
        let min = self.config.min_touch;
        if b.width < min || b.height < min {
            self.push(
                Severity::Warning,
                "touch-target",
                node,
                format!("{}×{} px is below the {min} px touch minimum", b.width.round(), b.height.round()),
                format!("make it at least {min}×{min} (stage use: larger)"),
            );
        }
    }

    fn labeled(&mut self, node: &Node, control: &Control) {
        if control.kind.is_input() && !control.slots.iter().any(|s| s.role == SlotRole::Label) {
            let has_text_child = self.scene.walk(node.id).iter().any(|id| self.scene.node(*id).is_some_and(|n| n.text().is_some()));
            if !has_text_child {
                self.push(Severity::Info, "unlabeled", node, "no label".into(), "add a Text child with a Label slot".into());
            }
        }
    }

    fn duplicates(&mut self, controls: &[(&Node, &Control)]) {
        let mut midi: BTreeMap<(Option<u8>, u8, u8), &Node> = BTreeMap::new();
        let mut params: BTreeMap<&str, &Node> = BTreeMap::new();
        let mut found = Vec::new();
        for (node, c) in controls {
            if let Some(m) = &c.midi {
                let key = match m.message {
                    MidiMessage::Cc { number } => (m.channel, 0, number),
                    MidiMessage::Note { number } => (m.channel, 1, number),
                    MidiMessage::PitchBend => (m.channel, 2, 0),
                    MidiMessage::Aftertouch => (m.channel, 3, 0),
                };
                if let Some(first) = midi.insert(key, node) {
                    found.push((Severity::Error, "midi-duplicate", *node, format!("same MIDI message as \"{}\"", first.name), "give one of them a different CC/note or channel".to_string()));
                }
            }
            if let Some(b) = &c.binding
                && let Source::Param { addr } = &b.source
                && let Some(first) = params.insert(addr.0.as_str(), node)
            {
                found.push((
                    Severity::Info,
                    "param-duplicate",
                    *node,
                    format!("`{}` is also controlled by \"{}\"", addr.0, first.name),
                    "fine for mirrored views; otherwise bind a different parameter".to_string(),
                ));
            }
        }
        for (s, r, n, m, f) in found {
            self.push(s, r, n, m, f);
        }
    }

    fn overlaps(&mut self, controls: &[(&Node, &Control)]) {
        let inputs: Vec<(&Node, LayoutBox)> =
            controls.iter().filter(|(_, c)| c.kind.is_input()).filter_map(|(n, _)| self.boxes.get(&n.id).map(|b| (*n, *b))).collect();
        let mut found = Vec::new();
        for (i, (a, ab)) in inputs.iter().enumerate() {
            for (b, bb) in &inputs[i + 1..] {
                let ix = (ab.x + ab.width).min(bb.x + bb.width) - ab.x.max(bb.x);
                let iy = (ab.y + ab.height).min(bb.y + bb.height) - ab.y.max(bb.y);
                if ix > 0.5 && iy > 0.5 {
                    found.push((*b, format!("overlaps \"{}\" by {}×{} px", a.name, ix.round(), iy.round())));
                }
            }
        }
        for (n, m) in found {
            self.push(Severity::Error, "overlap", n, m, "move it apart, or put both in an auto-layout container".into());
        }
    }

    fn sizes(&mut self, controls: &[(&Node, &Control)]) {
        let mut groups: BTreeMap<(Option<NodeId>, &str), Vec<&Node>> = BTreeMap::new();
        for (n, c) in controls {
            groups.entry((n.parent, c.kind.name())).or_default().push(n);
        }
        let mut found = Vec::new();
        for ((_, kind), nodes) in groups {
            let Some(first) = nodes.first() else { continue };
            for n in &nodes[1..] {
                if (n.size.width - first.size.width).abs() > 0.5 || (n.size.height - first.size.height).abs() > 0.5 {
                    found.push((*n, format!(
                        "{kind} is {}×{} but its sibling \"{}\" is {}×{}",
                        n.size.width, n.size.height, first.name, first.size.width, first.size.height
                    )));
                }
            }
        }
        for (n, m) in found {
            self.push(Severity::Warning, "size-inconsistent", n, m, "use the same size for same-kind controls in a group".into());
        }
    }

    fn contrast(&mut self) {
        let mut found = Vec::new();
        for id in self.scene.walk(self.scene.root) {
            let Some(node) = self.scene.node(id) else { continue };
            if node.text().is_none_or(|t| t.characters.trim().is_empty()) || !node.visible {
                continue;
            }
            let Some(fg) = solid(node) else { continue };
            let Some(bg) = self.background_of(node) else { continue };
            let ratio = contrast_ratio(&fg, &bg);
            if ratio < self.config.min_contrast {
                found.push((node, ratio));
            }
        }
        for (node, ratio) in found {
            let min = self.config.min_contrast;
            self.push(
                Severity::Warning,
                "contrast",
                node,
                format!("text contrast {ratio:.1}:1 against its background (needs {min}:1)"),
                "lighten the text or darken the background (or vice versa)".into(),
            );
        }
    }

    /// The nearest ancestor with an opaque solid fill.
    fn background_of(&self, node: &Node) -> Option<Color> {
        let mut cur = node.parent;
        while let Some(id) = cur {
            let n = self.scene.node(id)?;
            if let Some(c) = solid(n).filter(|c| c.a > 0.99) {
                return Some(c);
            }
            cur = n.parent;
        }
        None
    }
}

fn solid(n: &Node) -> Option<Color> {
    n.fills.iter().rev().filter(|p| p.visible).find_map(|p| match &p.kind {
        PaintKind::Solid { color } => Some(Color { a: color.a * p.opacity, ..*color }),
        _ => None,
    })
}

fn luminance(c: &Color) -> f64 {
    let ch = |v: f64| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c.r) + 0.7152 * ch(c.g) + 0.0722 * ch(c.b)
}

/// WCAG 2 contrast ratio (1..=21).
#[must_use]
pub fn contrast_ratio(a: &Color, b: &Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `" — did you mean `x`?"` for the closest candidate, if any is close.
fn nearest<'a>(wanted: &str, candidates: impl Iterator<Item = &'a str>) -> String {
    let best = candidates.map(|c| (edit_distance(wanted, c), c)).min_by_key(|(d, _)| *d);
    match best {
        Some((d, c)) if d <= wanted.len().max(3) / 2 + 1 => format!(" — did you mean `{c}`?"),
        _ => String::new(),
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
