//! Frame context — dependency injection for rendering.
//!
//! Follows dock-dioxus context.rs pattern: PluginRenderer wraps an Rc<dyn Fn>
//! with Rc::ptr_eq for PartialEq (required by Dioxus props).

use crate::prelude::*;
use std::rc::Rc;

/// Custom renderer callback for plugin data on nodes.
///
/// Downstream crates (Surface, Architect) use this to inject interactive
/// elements (knobs, equipment labels) into Frame design nodes.
#[derive(Clone)]
pub struct PluginRenderer(Rc<dyn Fn(&str, &serde_json::Value) -> Option<Element>>);

impl PluginRenderer {
    pub fn new(
        f: impl Fn(&str, &serde_json::Value) -> Option<Element> + 'static,
    ) -> Self {
        Self(Rc::new(f))
    }

    /// Attempt to render plugin data. Returns None if the plugin key is unknown.
    pub fn render(&self, plugin_key: &str, data: &serde_json::Value) -> Option<Element> {
        (self.0)(plugin_key, data)
    }
}

impl PartialEq for PluginRenderer {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Context for Frame UI rendering.
#[derive(Clone)]
pub struct FrameContext {
    pub plugin_renderer: Option<PluginRenderer>,
    pub editable: bool,
}

/// Hook to access the Frame context.
pub fn use_frame_context() -> FrameContext {
    use_context::<FrameContext>()
}

/// Provider component that injects Frame context into the component tree.
#[component]
pub fn FrameProvider(
    #[props(default)] plugin_renderer: Option<PluginRenderer>,
    #[props(default = true)] editable: bool,
    children: Element,
) -> Element {
    use_context_provider(move || FrameContext {
        plugin_renderer: plugin_renderer.clone(),
        editable,
    });
    children
}
