//! Dynamic panel registry — runtime-extensible panel registration.
//!
//! Any crate can contribute panels by registering a [`PanelDescriptor`] at
//! runtime. The [`PanelRegistry`] replaces the need for a hardcoded enum as
//! the single source of truth for what panels exist.
//!
//! The existing [`PanelId`](crate::panel::PanelId) enum continues to work
//! via its `Into<String>` impl for backward compatibility.

use facet::Facet;
use std::collections::HashMap;

use crate::context::{ActionContext, WhenExpr};

// region: --- Types

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum DockPosition {
    Left,
    Right,
    Bottom,
    Center,
    Float,
}

/// Size constraints for a panel, used by the layout engine during resize
/// negotiation and constraint solving.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct PanelConstraints {
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
    pub preferred_width: Option<f64>,
    pub preferred_height: Option<f64>,
    /// Higher priority panels get space first during layout negotiation (0–255).
    pub resize_priority: u8,
}

impl Default for PanelConstraints {
    fn default() -> Self {
        Self {
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            preferred_width: None,
            preferred_height: None,
            resize_priority: 128,
        }
    }
}

impl PanelConstraints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_min_width(mut self, width: f64) -> Self {
        self.min_width = Some(width);
        self
    }

    pub fn with_max_width(mut self, width: f64) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn with_min_height(mut self, height: f64) -> Self {
        self.min_height = Some(height);
        self
    }

    pub fn with_max_height(mut self, height: f64) -> Self {
        self.max_height = Some(height);
        self
    }

    pub fn with_preferred_width(mut self, width: f64) -> Self {
        self.preferred_width = Some(width);
        self
    }

    pub fn with_preferred_height(mut self, height: f64) -> Self {
        self.preferred_height = Some(height);
        self
    }

    pub fn with_resize_priority(mut self, priority: u8) -> Self {
        self.resize_priority = priority;
        self
    }
}

/// Describes a panel that can be registered with the dock system.
///
/// This is the runtime metadata for a panel. The dock system uses it for
/// display names, icons, default placement, and constraint negotiation.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct PanelDescriptor {
    /// Unique string identifier for this panel.
    pub id: String,
    /// Human-readable display name shown in tabs and headers.
    pub display_name: String,
    /// Icon identifier (e.g. lucide icon name).
    pub icon: String,
    /// Category for grouping in panel pickers (e.g. "Session", "Signal", "Utility").
    pub category: String,
    /// Preferred default position when auto-placing this panel.
    pub default_position: DockPosition,
    /// Size constraints for layout negotiation.
    pub constraints: PanelConstraints,
    /// Optional when-clause controlling contextual visibility.
    pub visibility_when: Option<String>,
}

impl PanelDescriptor {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            icon: String::new(),
            category: String::new(),
            default_position: DockPosition::Center,
            constraints: PanelConstraints::default(),
            visibility_when: None,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    pub fn with_default_position(mut self, position: DockPosition) -> Self {
        self.default_position = position;
        self
    }

    pub fn with_constraints(mut self, constraints: PanelConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn with_visibility_when(mut self, when: impl Into<String>) -> Self {
        self.visibility_when = Some(when.into());
        self
    }
}

// endregion: --- Types

// region: --- PanelRegistry

/// Runtime registry of available panels.
///
/// This is the single source of truth for what panels exist in the application.
/// Any crate can register panels at startup via [`register`](Self::register).
#[derive(Debug, Clone, Default)]
pub struct PanelRegistry {
    panels: HashMap<String, PanelDescriptor>,
}

impl PanelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a panel descriptor. Overwrites any existing panel with the same ID.
    pub fn register(&mut self, descriptor: PanelDescriptor) {
        self.panels.insert(descriptor.id.clone(), descriptor);
    }

    /// Remove a panel by ID. Returns the removed descriptor if it existed.
    pub fn unregister(&mut self, id: &str) -> Option<PanelDescriptor> {
        self.panels.remove(id)
    }

    /// Look up a panel descriptor by ID.
    pub fn get(&self, id: &str) -> Option<&PanelDescriptor> {
        self.panels.get(id)
    }

    /// Get all registered panel descriptors.
    pub fn all(&self) -> Vec<&PanelDescriptor> {
        self.panels.values().collect()
    }

    /// Get all registered panel IDs.
    pub fn all_ids(&self) -> Vec<&str> {
        self.panels.keys().map(|s| s.as_str()).collect()
    }

    /// Get panels grouped by category.
    ///
    /// Returns a map from category name to the descriptors in that category.
    /// Panels with an empty category are grouped under `""`.
    pub fn by_category(&self) -> HashMap<&str, Vec<&PanelDescriptor>> {
        let mut groups: HashMap<&str, Vec<&PanelDescriptor>> = HashMap::new();
        for desc in self.panels.values() {
            groups.entry(desc.category.as_str()).or_default().push(desc);
        }
        groups
    }

    /// Number of registered panels.
    pub fn len(&self) -> usize {
        self.panels.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }

    /// Check if a panel ID is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.panels.contains_key(id)
    }

    /// Return IDs of panels visible in the given action context.
    pub fn visible_panels(&self, ctx: &ActionContext) -> Vec<String> {
        self.panels
            .values()
            .filter(|desc| {
                desc.visibility_when
                    .as_deref()
                    .map(|expr| WhenExpr::parse(expr).evaluate(ctx))
                    .unwrap_or(true)
            })
            .map(|desc| desc.id.clone())
            .collect()
    }

    /// Return IDs of panels that have context visibility rules.
    pub fn context_managed_panels(&self) -> Vec<String> {
        self.panels
            .values()
            .filter(|desc| desc.visibility_when.is_some())
            .map(|desc| desc.id.clone())
            .collect()
    }
}

// endregion: --- PanelRegistry
