//! A design session: one document, one catalog, one undo history, and one
//! tool registry.
//!
//! Every AI surface — MCP, in-app chat, the CLI — lists [`tools`] and
//! dispatches [`Session::call`]. Each call is a single atomic transaction:
//! it applies fully or not at all, and one `undo` reverts it. Tools speak
//! frame markup (see `document-text/REFERENCE.md`) and return text meant
//! for a model to read: what changed, ids to refer to, and lint results.

use std::collections::BTreeMap;

use design_lint::{Config, Issue, Severity};
use document_proto::{History, Node, NodeId, NodeKind, Op, OpError, Prop, Scene};
use serde_json::{Value, json};
use surface_proto::{Binding, ButtonMode, Catalog, Control, ControlKind, EntryId, Orientation, ParamKind, Slot, SlotRole, Source};
use surface_widgets::{Builder, ControlSize, Options, Theme, surface_for_catalog};

/// A tool as adapters advertise it.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the arguments object.
    pub input_schema: Value,
    /// Whether the tool edits the document.
    pub mutates: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ToolError {}

impl From<OpError> for ToolError {
    fn from(e: OpError) -> Self {
        Self(e.to_string())
    }
}

type Result<T> = std::result::Result<T, ToolError>;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(ToolError(msg.into()))
}

pub struct Session {
    scene: Scene,
    catalog: Catalog,
    theme: Theme,
    history: History,
    lint: Config,
}

impl Session {
    #[must_use]
    pub fn new(scene: Scene, catalog: Catalog) -> Self {
        Self { scene, catalog, theme: Theme::default(), history: History::default(), lint: Config::default() }
    }

    /// An empty page, ready to design against `catalog`.
    #[must_use]
    pub fn blank(catalog: Catalog) -> Self {
        Self::new(Scene::new("Surface"), catalog)
    }

    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    #[must_use]
    pub fn issues(&self) -> Vec<Issue> {
        design_lint::lint(&self.scene, &self.catalog, &self.lint)
    }

    /// Run a tool by name with a JSON arguments object.
    ///
    /// # Errors
    /// A message for the model: unknown tool, bad arguments, parse errors
    /// (with line:col), or ops that don't fit the document.
    pub fn call(&mut self, name: &str, args: &Value) -> Result<String> {
        let a = Args(args);
        match name {
            "describe" => self.describe(a.opt_node(&self.scene, "node")?),
            "read" => self.read(a.opt_node(&self.scene, "node")?),
            "catalog" => Ok(self.catalog_text()),
            "lint" => Ok(self.lint_text()),
            "write" => {
                let parent = a.node(&self.scene, "parent")?;
                self.write(parent, a.str("markup")?, a.opt_u32("index")?)
            }
            "replace" => {
                let node = a.node(&self.scene, "node")?;
                self.replace(node, a.str("markup")?)
            }
            "remove" => {
                let node = a.node(&self.scene, "node")?;
                self.commit(vec![Op::Remove { id: node }]).map(|()| format!("removed {node}"))
            }
            "move" => {
                let node = a.node(&self.scene, "node")?;
                let parent = a.node(&self.scene, "parent")?;
                let index = a.opt_u32("index")?.unwrap_or(u32::MAX);
                self.commit(vec![Op::Move { id: node, parent, index }]).map(|()| format!("moved {node}"))
            }
            "add_control" => {
                let parent = a.node(&self.scene, "parent")?;
                self.add_control(parent, a.str("kind")?, a.opt_str("bind")?, a.opt_str("size")?, a.opt_str("name")?)
            }
            "bind" => {
                let node = a.node(&self.scene, "node")?;
                self.bind(node, a.opt_str("source")?, a.opt_str("kind")?, a.0.get("slots"))
            }
            "generate" => self.generate(a.opt_str("theme")?),
            "undo" => Ok(if self.history.undo(&mut self.scene)? { "undone".into() } else { "nothing to undo".into() }),
            "redo" => Ok(if self.history.redo(&mut self.scene)? { "redone".into() } else { "nothing to redo".into() }),
            other => err(format!("unknown tool `{other}`; tools: {}", tools().iter().map(|t| t.name).collect::<Vec<_>>().join(", "))),
        }
    }

    /// Apply `ops` as one undoable step, re-hugging containers in the
    /// same transaction.
    fn commit(&mut self, mut ops: Vec<Op>) -> Result<()> {
        let mut probe = self.scene.clone();
        probe.transact(ops.clone())?;
        ops.extend(probe.hug_ops(probe.root));
        Ok(self.history.commit(&mut self.scene, ops)?)
    }

    fn page(&self) -> NodeId {
        self.scene.pages().first().copied().unwrap_or(self.scene.root)
    }

    // ── reading ──────────────────────────────────────────────────────────

    fn describe(&self, node: Option<NodeId>) -> Result<String> {
        let root = node.unwrap_or(self.scene.root);
        let mut out = String::new();
        let mut stack = vec![(root, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            let Some(n) = self.scene.node(id) else { continue };
            out.push_str(&"  ".repeat(depth));
            out.push_str(&outline_line(n));
            out.push('\n');
            for c in n.children.iter().rev() {
                stack.push((*c, depth + 1));
            }
        }
        let issues = self.issues();
        let (e, w, i) = design_lint::counts(&issues);
        out.push_str(&format!("\nlint: {e} errors, {w} warnings, {i} notes (run `lint` for details)\n"));
        Ok(out)
    }

    fn read(&self, node: Option<NodeId>) -> Result<String> {
        match node {
            None => Ok(document_text::print(&self.scene)),
            Some(id) => document_text::print_subtree(&self.scene, id).ok_or_else(|| ToolError(format!("no node {id}"))),
        }
    }

    fn catalog_text(&self) -> String {
        let c = &self.catalog;
        let mut out = format!("engine `{}` v{}\n\nparams (bind as param:<addr>):\n", c.id, c.version);
        for (group, params) in c.groups() {
            out.push_str(&format!("  [{}]\n", group.unwrap_or("Other")));
            for p in params {
                let kind = match &p.kind {
                    ParamKind::Continuous => format!(
                        "{}..{} {}{}",
                        p.range.format(p.range.min),
                        p.range.format(p.range.max),
                        if p.bipolar { "bipolar " } else { "" },
                        match p.range.scale {
                            surface_proto::Scale::Logarithmic => "log",
                            _ => "",
                        }
                    ),
                    ParamKind::Toggle => "toggle".into(),
                    ParamKind::Choice { options } => format!("choice: {}", options.join(" | ")),
                };
                out.push_str(&format!("    {:<18} {:<12} {}  default {}\n", p.addr.0, p.name, kind.trim(), p.display(p.default)));
            }
        }
        out.push_str("\nactions (bind as action:<id>):\n");
        for a in &c.actions {
            out.push_str(&format!("    {:<18} {}\n", a.id.0, a.name));
        }
        out.push_str("\nstreams (bind as stream:<id>[:channel]):\n");
        for s in &c.streams {
            out.push_str(&format!("    {:<18} {} ({:?}, {} Hz)\n", s.id.0, s.name, s.kind, s.rate_hz));
        }
        out
    }

    fn lint_text(&self) -> String {
        let issues = self.issues();
        if issues.is_empty() {
            return "clean: no issues".into();
        }
        let (e, w, i) = design_lint::counts(&issues);
        let mut out = format!("{e} errors, {w} warnings, {i} notes\n");
        for issue in issues.iter().filter(|i| i.severity != Severity::Info).chain(issues.iter().filter(|i| i.severity == Severity::Info).take(10)) {
            out.push_str(&format!("{issue}\n"));
        }
        out
    }

    // ── writing ──────────────────────────────────────────────────────────

    fn write(&mut self, parent: NodeId, markup: &str, index: Option<u32>) -> Result<String> {
        let nodes = document_text::parse_fragment(markup).map_err(|e| ToolError(format!("markup {e}")))?;
        let ops = insert_ops(&self.scene, parent, index, nodes)?;
        let created: Vec<String> = ops
            .iter()
            .filter_map(|o| match o {
                Op::Insert { node, parent: p, .. } if *p == parent => Some(format!("{} \"{}\"", node.id, node.name)),
                _ => None,
            })
            .collect();
        let n = ops.len();
        self.commit(ops)?;
        Ok(format!("inserted {n} nodes; top-level: {}\n{}", created.join(", "), self.lint_summary()))
    }

    fn replace(&mut self, node: NodeId, markup: &str) -> Result<String> {
        let n = self.scene.node(node).ok_or_else(|| ToolError(format!("no node {node}")))?;
        let parent = n.parent.ok_or_else(|| ToolError("can't replace the root".into()))?;
        let index = self.scene.node(parent).and_then(|p| p.children.iter().position(|c| *c == node)).unwrap_or(0);
        let mut nodes = document_text::parse_fragment(markup).map_err(|e| ToolError(format!("markup {e}")))?;
        // Replacing in place keeps the node's identity (and anything bound to it).
        if let Some(top) = nodes.iter_mut().find(|n| n.parent.is_none()) {
            let new_id = top.id;
            top.id = node;
            for n in &mut nodes {
                if n.parent == Some(new_id) {
                    n.parent = Some(node);
                }
            }
        }
        let mut ops = vec![Op::Remove { id: node }];
        // The remove happens first inside the transaction; build inserts
        // against a scene without the old subtree.
        let mut probe = self.scene.clone();
        probe.apply(Op::Remove { id: node })?;
        ops.extend(insert_ops(&probe, parent, Some(u32::try_from(index).unwrap_or(u32::MAX)), nodes)?);
        self.commit(ops)?;
        Ok(format!("replaced {node}\n{}", self.lint_summary()))
    }

    fn add_control(&mut self, parent: NodeId, kind: &str, bind: Option<&str>, size: Option<&str>, name: Option<&str>) -> Result<String> {
        let binding = bind.map(parse_source).transpose()?;
        let size = match size.unwrap_or("medium") {
            "small" => ControlSize::Small,
            "medium" => ControlSize::Medium,
            "large" => ControlSize::Large,
            "stage" => ControlSize::Stage,
            other => return err(format!("unknown size `{other}`; use small | medium | large | stage")),
        };
        let default_name = binding
            .as_ref()
            .and_then(|b| match &b.source {
                Source::Param { addr } => self.catalog.param(addr).map(|p| p.name.clone()),
                Source::Action { id } => self.catalog.action(id).map(|a| a.name.clone()),
                Source::Stream { id, .. } => self.catalog.stream(id).map(|s| s.name.clone()),
                Source::Collection { id } => self.catalog.collection(id).map(|c| c.name.clone()),
            })
            .unwrap_or_else(|| kind.to_string());
        let name = name.unwrap_or(&default_name).to_string();
        let theme = self.theme.clone();
        let (ops, root) = self.build(parent, |b| match kind {
            "knob" => Ok(b.knob(parent, &name, size, binding.clone())),
            "fader" => Ok(b.fader(parent, &name, size.px() * 3.0, binding.clone())),
            "button" => Ok(b.button(parent, &name, size, ButtonMode::Toggle, binding.clone())),
            "trigger" => Ok(b.button(parent, &name, size, ButtonMode::Trigger, binding.clone())),
            "footswitch" => Ok(b.footswitch(parent, &name, binding.clone())),
            "meter" => Ok(b.meter(parent, &name, size.px() * 3.5, binding.clone())),
            "readout" => Ok(b.readout(parent, &name, size.px() * 1.5, binding.clone())),
            other => err(format!("unknown control kind `{other}`; use knob | fader | button | trigger | footswitch | meter | readout")),
        }, &theme)?;
        self.commit(ops)?;
        Ok(format!("added {kind} {root} \"{name}\"\n{}", self.lint_summary()))
    }

    /// Run a widget builder against a scratch copy and turn what it added
    /// into `Insert` ops (so it's one undoable transaction).
    fn build(&self, parent: NodeId, f: impl FnOnce(&mut Builder<'_>) -> Result<NodeId>, theme: &Theme) -> Result<(Vec<Op>, NodeId)> {
        if self.scene.node(parent).is_none() {
            return err(format!("no node {parent}"));
        }
        let mut scratch = self.scene.clone();
        let mut controls = BTreeMap::new();
        let root = {
            let mut b = Builder { scene: &mut scratch, controls: &mut controls, theme };
            f(&mut b)?
        };
        for (id, c) in controls {
            if let Some(n) = scratch.node_mut(id) {
                n.control = Some(c);
            }
        }
        let mut ops = Vec::new();
        for id in scratch.walk(root) {
            let mut n = scratch.node(id).cloned().ok_or_else(|| ToolError("builder lost a node".into()))?;
            let p = n.parent.unwrap_or(parent);
            let index = if id == root { u32::MAX } else { position(&scratch, p, id) };
            n.children.clear();
            ops.push(Op::Insert { node: n, parent: p, index });
        }
        Ok((ops, root))
    }

    fn bind(&mut self, node: NodeId, source: Option<&str>, kind: Option<&str>, slots: Option<&Value>) -> Result<String> {
        let n = self.scene.node(node).ok_or_else(|| ToolError(format!("no node {node}")))?;
        let binding = source.map(parse_source).transpose()?;
        let mut control = match (n.control.clone(), kind) {
            (_, Some(k)) => {
                let mut c = Control::new(parse_kind(k)?);
                if let Some(old) = &n.control {
                    c.binding = old.binding.clone();
                    c.slots = old.slots.clone();
                    c.midi = old.midi.clone();
                }
                c
            }
            (Some(c), None) => c,
            (None, None) => return err("this node isn't a control yet; pass `kind` (knob | fader | button | trigger | footswitch | meter | readout | xy | selector)"),
        };
        if binding.is_some() {
            control.binding = binding;
        }
        if let Some(slots) = slots {
            let map = slots.as_object().ok_or_else(|| ToolError("`slots` must be an object like {\"needle\": \"<node>\"}".into()))?;
            control.slots.clear();
            for (role, v) in map {
                let target = v.as_str().ok_or_else(|| ToolError(format!("slot `{role}` must name a node")))?;
                control.slots.push(Slot { role: parse_role(role)?, node: resolve(&self.scene, target)?.0 });
            }
        }
        self.commit(vec![Op::Set { id: node, prop: Prop::Control(Some(control)) }])?;
        Ok(format!("bound {node}\n{}", self.lint_summary()))
    }

    fn generate(&mut self, theme: Option<&str>) -> Result<String> {
        let theme = match theme.unwrap_or("stage-dark") {
            "stage-dark" => Theme::stage_dark(),
            "daylight" => Theme::daylight(),
            other => return err(format!("unknown theme `{other}`; use stage-dark | daylight")),
        };
        self.theme = theme.clone();
        let generated = surface_for_catalog(&self.catalog, &Options { theme, ..Options::default() }).to_scene();
        let gen_page = generated.pages()[0];
        let page = self.page();
        let mut ops: Vec<Op> = self.scene.node(page).map(|p| p.children.iter().map(|c| Op::Remove { id: *c }).collect()).unwrap_or_default();
        for id in generated.walk(gen_page).into_iter().skip(1) {
            let mut n = generated.node(id).cloned().ok_or_else(|| ToolError("generator lost a node".into()))?;
            let p = if n.parent == Some(gen_page) { page } else { n.parent.unwrap_or(page) };
            n.children.clear();
            ops.push(Op::Insert { node: n, parent: p, index: u32::MAX });
        }
        self.commit(ops)?;
        Ok(format!("generated a surface for `{}` ({} controls)\n{}", self.catalog.id, self.scene.nodes.values().filter(|n| n.control.is_some()).count(), self.lint_summary()))
    }

    fn lint_summary(&self) -> String {
        let issues = self.issues();
        let (e, w, _) = design_lint::counts(&issues);
        let mut s = format!("lint: {e} errors, {w} warnings");
        for i in issues.iter().filter(|i| i.severity == Severity::Error).take(5) {
            s.push_str(&format!("\n  {i}"));
        }
        s
    }
}

fn position(scene: &Scene, parent: NodeId, id: NodeId) -> u32 {
    scene.node(parent).and_then(|p| p.children.iter().position(|c| *c == id)).map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX))
}

/// Insert ops for a parsed fragment (pre-order, parents wired; top-level
/// nodes go under `parent` starting at `index`).
fn insert_ops(scene: &Scene, parent: NodeId, index: Option<u32>, nodes: Vec<Node>) -> Result<Vec<Op>> {
    if scene.node(parent).is_none() {
        return err(format!("no node {parent}"));
    }
    let mut next = index.unwrap_or(u32::MAX);
    let mut ops = Vec::new();
    for mut n in nodes {
        if scene.node(n.id).is_some() {
            return err(format!("id {} is already in the document; omit `id` to get a fresh one", n.id));
        }
        n.children.clear();
        match n.parent {
            Some(p) => ops.push(Op::Insert { node: n, parent: p, index: u32::MAX }),
            None => {
                ops.push(Op::Insert { node: n, parent, index: next });
                if next != u32::MAX {
                    next += 1;
                }
            }
        }
    }
    Ok(ops)
}

fn outline_line(n: &Node) -> String {
    let kind = match &n.kind {
        NodeKind::Text(t) => format!("Text \"{}\"", t.characters.chars().take(40).collect::<String>()),
        NodeKind::Instance { .. } => "Instance".into(),
        NodeKind::BooleanOperation { .. } => "BooleanOperation".into(),
        NodeKind::Other { tag } => tag.clone(),
        k => format!("{k:?}"),
    };
    let mut s = format!("{kind} \"{}\" {} {}×{}", n.name, n.id, n.size.width.round(), n.size.height.round());
    if let Some(l) = &n.layout {
        s.push_str(&format!(" [{}]", if matches!(l.direction, document_proto::LayoutDirection::Horizontal) { "row" } else { "column" }));
    }
    if let Some(c) = &n.control {
        s.push_str(&format!(" ⟨{}", c.kind.name()));
        if let Some(b) = &c.binding {
            s.push_str(&format!(" → {}", source_text(&b.source)));
        }
        s.push('⟩');
    }
    s
}

fn source_text(s: &Source) -> String {
    match s {
        Source::Param { addr } => format!("param:{}", addr.0),
        Source::Action { id } => format!("action:{}", id.0),
        Source::Stream { id, channel: Some(c) } => format!("stream:{}:{c}", id.0),
        Source::Stream { id, channel: None } => format!("stream:{}", id.0),
        Source::Collection { id } => format!("collection:{}", id.0),
    }
}

/// `param:amp/gain` · `action:tap_tempo` · `stream:out_level[:ch]` · `collection:songs`.
fn parse_source(s: &str) -> Result<Binding> {
    let (kind, rest) = s.split_once(':').ok_or_else(|| ToolError(format!("source `{s}` needs a prefix: param: | action: | stream: | collection:")))?;
    let source = match kind {
        "param" => return Ok(Binding::param(rest)),
        "action" => Source::Action { id: EntryId::new(rest) },
        "stream" => match rest.rsplit_once(':') {
            Some((id, ch)) if ch.parse::<u8>().is_ok() => Source::Stream { id: EntryId::new(id), channel: ch.parse().ok() },
            _ => Source::Stream { id: EntryId::new(rest), channel: None },
        },
        "collection" => Source::Collection { id: EntryId::new(rest) },
        other => return err(format!("unknown source kind `{other}`; use param | action | stream | collection")),
    };
    Ok(Binding { source, ..Binding::param("") })
}

fn parse_kind(k: &str) -> Result<ControlKind> {
    Ok(match k {
        "knob" => ControlKind::knob(),
        "fader" => ControlKind::fader(),
        "hfader" => ControlKind::Fader { orientation: Orientation::Horizontal },
        "button" => ControlKind::Button { mode: ButtonMode::Toggle },
        "momentary" => ControlKind::Button { mode: ButtonMode::Momentary },
        "trigger" => ControlKind::Button { mode: ButtonMode::Trigger },
        "footswitch" => ControlKind::footswitch(),
        "meter" => ControlKind::Meter { orientation: Orientation::Vertical },
        "xy" => ControlKind::XyPad,
        "selector" => ControlKind::Selector,
        "readout" => ControlKind::Readout,
        other => return err(format!("unknown control kind `{other}`")),
    })
}

fn parse_role(r: &str) -> Result<SlotRole> {
    Ok(match r.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
        "needle" | "pointer" => SlotRole::Needle,
        "valuearc" | "arc" => SlotRole::ValueArc,
        "modarc" => SlotRole::ModArc,
        "fill" => SlotRole::Fill,
        "thumb" | "cap" => SlotRole::Thumb,
        "value" | "readout" => SlotRole::Value,
        "label" => SlotRole::Label,
        "led" => SlotRole::Led,
        "peakhold" | "peak" => SlotRole::PeakHold,
        _ => return err(format!("unknown slot role `{r}`; use needle | value-arc | mod-arc | fill | thumb | value | label | led | peak-hold")),
    })
}

/// A node by uuid, or by name when the name is unique.
fn resolve(scene: &Scene, r: &str) -> Result<NodeId> {
    if let Ok(u) = uuid::Uuid::parse_str(r) {
        return scene.node(NodeId(u)).map(|n| n.id).ok_or_else(|| ToolError(format!("no node {r}")));
    }
    let hits: Vec<&Node> = scene.nodes.values().filter(|n| n.name == r).collect();
    match hits.as_slice() {
        [one] => Ok(one.id),
        [] => err(format!("no node named \"{r}\"; use `describe` to list nodes")),
        many => err(format!("{} nodes are named \"{r}\"; pass an id instead ({})", many.len(), many.iter().take(4).map(|n| n.id.to_string()).collect::<Vec<_>>().join(", "))),
    }
}

struct Args<'a>(&'a Value);

impl Args<'_> {
    fn opt_str(&self, k: &str) -> Result<Option<&str>> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => err(format!("`{k}` must be a string")),
        }
    }
    fn str(&self, k: &str) -> Result<&str> {
        self.opt_str(k)?.ok_or_else(|| ToolError(format!("missing `{k}`")))
    }
    fn opt_u32(&self, k: &str) -> Result<Option<u32>> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_u64().and_then(|n| u32::try_from(n).ok()).map(Some).ok_or_else(|| ToolError(format!("`{k}` must be a non-negative integer"))),
        }
    }
    fn opt_node(&self, scene: &Scene, k: &str) -> Result<Option<NodeId>> {
        self.opt_str(k)?.map(|r| resolve(scene, r)).transpose()
    }
    fn node(&self, scene: &Scene, k: &str) -> Result<NodeId> {
        resolve(scene, self.str(k)?)
    }
}

/// The tool registry every adapter advertises.
#[must_use]
pub fn tools() -> Vec<ToolDef> {
    let node = json!({"type": "string", "description": "node id (uuid) or unique node name"});
    let obj = |props: Value, required: &[&str]| json!({"type": "object", "properties": props, "required": required});
    vec![
        ToolDef {
            name: "describe",
            description: "Outline the document (or a subtree): one line per node with kind, name, id, size, layout and control binding; plus a lint summary. Start here.",
            input_schema: obj(json!({"node": node}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "read",
            description: "Frame markup of the whole document or one subtree (lossless; see the markup reference).",
            input_schema: obj(json!({"node": node}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "catalog",
            description: "What the engine exposes: parameters (addr, name, range/unit, default), actions, telemetry streams. Bind controls only to these.",
            input_schema: obj(json!({}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "lint",
            description: "Check the surface: wiring, missing slots, overlaps, touch targets, consistency, contrast. Each issue says how to fix it. Iterate until there are no errors.",
            input_schema: obj(json!({}), &[]),
            mutates: false,
        },
        ToolDef {
            name: "write",
            description: "Insert a frame-markup fragment (one or more elements) under `parent` at `index` (default: end). Omit ids to get fresh ones. Returns the new top-level ids and lint.",
            input_schema: obj(json!({"parent": node, "markup": {"type": "string"}, "index": {"type": "integer", "minimum": 0}}), &["parent", "markup"]),
            mutates: true,
        },
        ToolDef {
            name: "replace",
            description: "Replace a node's subtree with a markup fragment; the node keeps its id.",
            input_schema: obj(json!({"node": node, "markup": {"type": "string"}}), &["node", "markup"]),
            mutates: true,
        },
        ToolDef { name: "remove", description: "Delete a node and its subtree.", input_schema: obj(json!({"node": node}), &["node"]), mutates: true },
        ToolDef {
            name: "move",
            description: "Reparent/reorder a node.",
            input_schema: obj(json!({"node": node, "parent": node, "index": {"type": "integer", "minimum": 0}}), &["node", "parent"]),
            mutates: true,
        },
        ToolDef {
            name: "add_control",
            description: "Add a canonical, themed control under `parent`, fully wired. kind: knob | fader | button | trigger | footswitch | meter | readout. bind: param:<addr> | action:<id> | stream:<id>[:channel]. size: small | medium | large | stage.",
            input_schema: obj(
                json!({"parent": node, "kind": {"type": "string", "enum": ["knob", "fader", "button", "trigger", "footswitch", "meter", "readout"]}, "bind": {"type": "string"}, "size": {"type": "string", "enum": ["small", "medium", "large", "stage"]}, "name": {"type": "string"}}),
                &["parent", "kind"],
            ),
            mutates: true,
        },
        ToolDef {
            name: "bind",
            description: "Make any drawn node a control, or rewire one. kind (to create/change), source (param:… | action:… | stream:… | collection:…), slots: {role: node} with roles needle, value-arc, mod-arc, fill, thumb, value, label, led, peak-hold.",
            input_schema: obj(json!({"node": node, "kind": {"type": "string"}, "source": {"type": "string"}, "slots": {"type": "object", "additionalProperties": {"type": "string"}}}), &["node"]),
            mutates: true,
        },
        ToolDef {
            name: "generate",
            description: "Replace the page with a complete, bound surface generated from the catalog (a panel per group, output strip, footswitch row). A good starting point to refine.",
            input_schema: obj(json!({"theme": {"type": "string", "enum": ["stage-dark", "daylight"]}}), &[]),
            mutates: true,
        },
        ToolDef { name: "undo", description: "Revert the last edit (one tool call).", input_schema: obj(json!({}), &[]), mutates: true },
        ToolDef { name: "redo", description: "Re-apply the last undone edit.", input_schema: obj(json!({}), &[]), mutates: true },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use surface_memory::demo_catalog;

    fn call(s: &mut Session, name: &str, args: Value) -> String {
        s.call(name, &args).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    #[test]
    fn registry_matches_dispatch() {
        let mut s = Session::blank(demo_catalog());
        for t in tools() {
            // Every advertised tool is dispatched (unknown-tool is the only
            // error that names the tool list).
            let r = s.call(t.name, &json!({}));
            assert!(!matches!(&r, Err(ToolError(m)) if m.starts_with("unknown tool")), "{}", t.name);
            assert_eq!(t.input_schema["type"], "object");
        }
    }

    #[test]
    fn build_a_panel_with_tools_then_undo() {
        let mut s = Session::blank(demo_catalog());
        let page = s.scene().pages()[0].to_string();
        let out = call(&mut s, "write", json!({"parent": page, "markup": r##"<Frame name="Amp" layout="row" gap=12 padding=16 fill="#17191e" radius=10 w=400 h=160 />"##}));
        assert!(out.contains("inserted 1 nodes"), "{out}");
        for (addr, name) in [("amp/gain", "Gain"), ("amp/bass", "Bass"), ("amp/treble", "Treble")] {
            let out = call(&mut s, "add_control", json!({"parent": "Amp", "kind": "knob", "bind": format!("param:{addr}")}));
            assert!(out.contains(&format!("\"{name}\"")), "{out}");
        }
        let out = call(&mut s, "lint", json!({}));
        assert!(!out.contains("error["), "{out}");
        let described = call(&mut s, "describe", json!({}));
        assert!(described.contains("⟨knob → param:amp/treble⟩"), "{described}");

        let before = s.scene().clone();
        call(&mut s, "add_control", json!({"parent": "Amp", "kind": "footswitch", "bind": "action:tap_tempo"}));
        assert_ne!(s.scene(), &before);
        call(&mut s, "undo", json!({}));
        assert_eq!(s.scene(), &before, "one tool call = one undo step");
    }

    #[test]
    fn errors_are_actionable() {
        let mut s = Session::blank(demo_catalog());
        let page = s.scene().pages()[0].to_string();
        let e = s.call("write", &json!({"parent": page, "markup": "<Rect w=10 />"})).unwrap_err();
        assert!(e.0.contains("Rectangle"), "{e}");
        let e = s.call("add_control", &json!({"parent": page, "kind": "wheel"})).unwrap_err();
        assert!(e.0.contains("knob | fader"), "{e}");
        let e = s.call("bind", &json!({"node": page, "source": "amp/gain"})).unwrap_err();
        assert!(e.0.contains("prefix"), "{e}");
        // A bad binding still commits, and lint explains it.
        call(&mut s, "add_control", json!({"parent": page, "kind": "knob", "bind": "param:amp/gian"}));
        assert!(call(&mut s, "lint", json!({})).contains("did you mean `amp/gain`"));
    }

    #[test]
    fn bind_a_hand_drawn_knob() {
        let mut s = Session::blank(demo_catalog());
        let page = s.scene().pages()[0].to_string();
        call(&mut s, "write", json!({"parent": page, "markup": r##"
            <Frame name="MyKnob" w=64 h=64>
              <Ellipse name="face" w=64 h=64 fill="#333333" />
              <Rectangle name="tick" x=30 y=4 w=4 h=20 fill="#ffffff" />
            </Frame>"##}));
        let out = call(&mut s, "bind", json!({"node": "MyKnob", "kind": "knob", "source": "param:delay/mix", "slots": {"needle": "tick"}}));
        assert!(out.contains("0 errors"), "{out}");
        let n = s.scene().nodes.values().find(|n| n.name == "MyKnob").unwrap();
        assert_eq!(n.control.as_ref().unwrap().slots.len(), 1);
    }

    #[test]
    fn generate_then_catalog_text() {
        let mut s = Session::blank(demo_catalog());
        let out = call(&mut s, "generate", json!({}));
        assert!(out.contains("26 controls") && out.contains("0 errors"), "{out}");
        let c = call(&mut s, "catalog", json!({}));
        assert!(c.contains("amp/channel") && c.contains("Clean | Crunch | Lead") && c.contains("tap_tempo"), "{c}");
        call(&mut s, "undo", json!({}));
        assert_eq!(s.scene().nodes.len(), 2, "generate undoes as one step");
    }
}
