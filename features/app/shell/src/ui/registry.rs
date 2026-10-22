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

use super::layout::Region;

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

    /// The panels that live in a region, in tab order.
    pub fn in_region(&self, region: Region) -> impl Iterator<Item = &Panel> {
        self.panels.iter().filter(move |p| p.region == region)
    }

    /// Whether anything lives in a region (an empty region is not drawn).
    #[must_use]
    pub fn has(&self, region: Region) -> bool {
        self.in_region(region).next().is_some()
    }

    /// The panel a region shows: the remembered one if it still lives
    /// there, else the region's first. An id a newer or older build saved
    /// that this build doesn't have falls back quietly (spec §2.3).
    #[must_use]
    pub fn showing(&self, region: Region, remembered: Option<&str>) -> Option<&Panel> {
        remembered
            .and_then(|id| self.get(id))
            .filter(|p| p.region == region)
            .or_else(|| self.in_region(region).next())
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

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::*;

    fn a() -> Element {
        rsx! { "a" }
    }
    fn b() -> Element {
        rsx! { "b" }
    }

    fn registry() -> PanelRegistry {
        PanelRegistry::new()
            .with(Panel::new("gig.setlist", "Set", Region::Left, a))
            .with(Panel::new("gig.browser", "Browse", Region::Left, b))
            .with(Panel::new("gig.chart", "Chart", Region::Main, a))
    }

    #[test]
    fn lookup_by_id_and_region() {
        let r = registry();
        assert_eq!(r.len(), 3);
        assert_eq!(r.get("gig.chart").map(|p| p.title), Some("Chart"));
        assert!(r.get("nope").is_none());
        let left: Vec<_> = r.in_region(Region::Left).map(|p| p.id).collect();
        assert_eq!(
            left,
            ["gig.setlist", "gig.browser"],
            "registration order is tab order"
        );
        assert!(r.has(Region::Main));
        assert!(!r.has(Region::Right));
    }

    #[test]
    fn register_replaces_by_id_in_place() {
        let mut r = registry();
        assert!(r.register(Panel::new("gig.setlist", "Setlist", Region::Left, b)));
        assert_eq!(r.len(), 3);
        let left: Vec<_> = r.in_region(Region::Left).map(|p| p.title).collect();
        assert_eq!(left, ["Setlist", "Browse"]);
        assert_eq!(r.get("gig.setlist").map(|p| p.render), Some(Render(b)));
    }

    #[test]
    fn showing_prefers_the_remembered_panel_and_falls_back() {
        let r = registry();
        assert_eq!(
            r.showing(Region::Left, Some("gig.browser")).map(|p| p.id),
            Some("gig.browser")
        );
        assert_eq!(
            r.showing(Region::Left, None).map(|p| p.id),
            Some("gig.setlist")
        );
        // Unknown id (another build's layout): the first panel.
        assert_eq!(
            r.showing(Region::Left, Some("gone")).map(|p| p.id),
            Some("gig.setlist")
        );
        // A known id that lives elsewhere doesn't leak into this region.
        assert_eq!(
            r.showing(Region::Left, Some("gig.chart")).map(|p| p.id),
            Some("gig.setlist")
        );
        assert!(r.showing(Region::Right, None).is_none());
    }

    #[test]
    fn render_equality_is_by_function() {
        assert_eq!(Render(a), Render(a));
        assert_ne!(Render(a), Render(b));
    }

    #[test]
    fn constraints_come_from_dock() {
        let p = Panel::new("x", "X", Region::Right, a)
            .with_constraints(PanelConstraints::new().with_min_width(260.0));
        assert_eq!(p.constraints.min_width, Some(260.0));
    }
}
