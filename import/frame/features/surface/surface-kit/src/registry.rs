//! The component registry: what exists ([`components`]) and how to put one
//! in a document ([`place`]).

use document_proto::NodeId;
use surface_proto::Binding;
use surface_widgets::Builder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Knob,
    Meter,
    Button,
    Switch,
    Lever,
    Panel,
}

impl Family {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Knob => "knob",
            Self::Meter => "meter",
            Self::Button => "button",
            Self::Switch => "switch",
            Self::Lever => "lever",
            Self::Panel => "panel",
        }
    }
}

/// How to place a component. Every field but `name` is optional; a
/// component picks sensible defaults for its style.
#[derive(Debug, Clone, Default)]
pub struct Place {
    pub name: String,
    /// Primary dimension in px (knob diameter, meter width, button edge).
    pub size: Option<f64>,
    pub binding: Option<Binding>,
    /// A style-specific variant (a colour, a finish: `"red"`, `"black"`).
    pub variant: Option<String>,
    /// Legend printed with it (`"INPUT"`); `None` = the bound param's name.
    pub legend: Option<String>,
    /// Printed scale labels, first at the start of travel (`["0", …, "10"]`).
    pub scale: Option<Vec<String>>,
    /// Options for multi-position components (rotary switches, button banks).
    pub options: Option<Vec<String>>,
}

impl Place {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    #[must_use]
    pub fn size(mut self, px: f64) -> Self {
        self.size = Some(px);
        self
    }

    #[must_use]
    pub fn bind(mut self, b: Binding) -> Self {
        self.binding = Some(b);
        self
    }

    #[must_use]
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.variant = Some(v.into());
        self
    }

    #[must_use]
    pub fn legend(mut self, l: impl Into<String>) -> Self {
        self.legend = Some(l.into());
        self
    }

    #[must_use]
    pub fn scale(mut self, labels: &[&str]) -> Self {
        self.scale = Some(labels.iter().map(ToString::to_string).collect());
        self
    }

    #[must_use]
    pub fn options(mut self, labels: &[&str]) -> Self {
        self.options = Some(labels.iter().map(ToString::to_string).collect());
        self
    }
}

/// Builds a component under `parent`; returns the control's root node.
pub type BuildFn = fn(&mut Builder<'_>, NodeId, &Place) -> NodeId;

/// One component the kit offers.
#[derive(Clone)]
pub struct ComponentDef {
    /// Stable id: `family.style` (`"knob.daka"`, `"meter.vu_ivory"`).
    pub id: &'static str,
    pub family: Family,
    pub title: &'static str,
    /// One or two sentences for a model choosing between components: what
    /// it looks like, what real unit it's from, what it's good for.
    pub description: &'static str,
    /// Accepted `Place::variant` values (empty = none).
    pub variants: &'static [&'static str],
    pub default_size: f64,
    /// What it binds to: `"param"`, `"choice param"`, `"toggle param"`,
    /// `"level stream"`, `"action"`, `"none"`.
    pub binds: &'static str,
    pub build: BuildFn,
}

impl std::fmt::Debug for ComponentDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentDef").field("id", &self.id).field("family", &self.family).finish_non_exhaustive()
    }
}

/// Every component, grouped by family, in a stable order.
#[must_use]
pub fn components() -> Vec<ComponentDef> {
    let mut all: Vec<ComponentDef> = Vec::new();
    all.extend(crate::knobs::defs());
    all.extend(crate::meters::defs());
    all.extend(crate::controls::defs());
    all.extend(crate::panels::defs());
    all.sort_by_key(|c| (c.family, c.id));
    all
}

#[must_use]
pub fn find(id: &str) -> Option<ComponentDef> {
    components().into_iter().find(|c| c.id == id)
}

/// Place component `id` under `parent`.
///
/// # Errors
/// Unknown id (with the closest ids), or an unknown variant.
pub fn place(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place) -> Result<NodeId, String> {
    let all = components();
    let Some(def) = all.iter().find(|c| c.id == id) else {
        let mut near: Vec<&str> = all.iter().map(|c| c.id).filter(|c| c.split('.').next() == id.split('.').next()).collect();
        if near.is_empty() {
            near = all.iter().map(|c| c.id).collect();
        }
        return Err(format!("no component `{id}`; try: {}", near.join(", ")));
    };
    if let Some(v) = &p.variant
        && !def.variants.is_empty()
        && !def.variants.contains(&v.as_str())
    {
        return Err(format!("`{id}` has no variant `{v}`; variants: {}", def.variants.join(", ")));
    }
    Ok((def.build)(b, parent, p))
}
