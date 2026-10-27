//! The panel registry: what panels exist, where each one lives by default,
//! and how to draw it.
//!
//! Each domain registers its own panels explicitly (no `inventory` magic),
//! behind its own cargo feature, so an iOS or wasm build lists only the
//! panels that compile there. A panel is a props-less component: everything
//! it needs comes in through context, so any panel can mount in any region
//! it is allowed in (docs/app-on-blitz.md, "panels are components").
//!
//! The size constraints reuse `dock`'s [`PanelConstraints`], so the docking
//! work (spec §10 step 6, `features/ui/dock`) can adopt this registry later
//! without a migration.

use dioxus::prelude::Element;
pub use dock::registry::PanelConstraints;

use super::layout::{Region, ShellLayout};

/// A panel's body: a props-less component function.
///
/// Wrapped so it can sit in props. Equality compares the function's
/// address, which is all a re-render needs to know.
#[derive(Clone, Copy)]
pub struct Render(pub fn() -> Element);

impl PartialEq for Render {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.0, other.0)
    }
}

impl std::fmt::Debug for Render {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Render(..)")
    }
}

/// One registered panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    /// Unique and stable: it is what a saved layout remembers. Namespace it
    /// by domain (`gig.setlist`, `session.mixer`).
    pub id: &'static str,
    /// Shown on the panel's tab and its rail button.
    pub title: &'static str,
    /// Where the panel goes unless the layout says otherwise.
    pub region: Region,
    /// Size hints for the region holding it.
    pub constraints: PanelConstraints,
    /// The body.
    pub render: Render,
}

impl Panel {
    /// A panel with no size constraints.
    #[must_use]
    pub fn new(
        id: &'static str,
        title: &'static str,
        region: Region,
        render: fn() -> Element,
    ) -> Self {
        Self {
            id,
            title,
            region,
            constraints: PanelConstraints::default(),
            render: Render(render),
        }
    }

    /// The same panel with size hints.
    #[must_use]
    pub fn with_constraints(mut self, constraints: PanelConstraints) -> Self {
        self.constraints = constraints;
        self
    }
}

/// Every panel a scope (an app, or a workspace in it) can show, in
/// registration order — which is also tab order within a region.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelRegistry {
    panels: Vec<Panel>,
}

impl PanelRegistry {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self { panels: Vec::new() }
    }

    /// Add a panel. A panel with the same id is replaced in place (keeping
    /// its tab position); returns whether one was.
    pub fn register(&mut self, panel: Panel) -> bool {
        if let Some(slot) = self.panels.iter_mut().find(|p| p.id == panel.id) {
            *slot = panel;
            true
        } else {
            self.panels.push(panel);
            false
        }
    }

    /// Builder form of [`PanelRegistry::register`].
    #[must_use]
    pub fn with(mut self, panel: Panel) -> Self {
        self.register(panel);
        self
    }

    /// A panel by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Panel> {
        self.panels.iter().find(|p| p.id == id)
    }

    /// The panels in a region under `layout` (a panel moved there by the
    /// person is in it; one moved away is not), in tab order.
    pub fn in_region<'a>(
        &'a self,
        region: Region,
        layout: &'a ShellLayout,
    ) -> impl Iterator<Item = &'a Panel> {
        self.panels
            .iter()
            .filter(move |p| layout.placed(p.id, p.region) == region)
    }

    /// Whether anything is in a region under `layout` (an empty region is
    /// not drawn).
    #[must_use]
    pub fn has(&self, region: Region, layout: &ShellLayout) -> bool {
        self.in_region(region, layout).next().is_some()
    }

    /// The panel a region shows: the remembered one if it still lives
    /// there, else the region's first. An id a newer or older build saved
    /// that this build doesn't have falls back quietly (spec §2.3).
    #[must_use]
    pub fn showing<'a>(
        &'a self,
        region: Region,
        remembered: Option<&str>,
        layout: &'a ShellLayout,
    ) -> Option<&'a Panel> {
        remembered
            .and_then(|id| self.get(id))
            .filter(|p| layout.placed(p.id, p.region) == region)
            .or_else(|| self.in_region(region, layout).next())
    }

    /// How many panels are registered.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.panels.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }
}
