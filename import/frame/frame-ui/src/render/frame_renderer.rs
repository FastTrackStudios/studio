//! Frame/container renderer — CSS div with auto-layout (flexbox).
//!
//! Handles NodeKind::Frame, Component, Section, and Page.
//! Children are recursively rendered via FrameNodeRenderer.

use crate::context::use_frame_context;
use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use crate::style::css_builder::node_to_css;
use frame_proto::NodeId;

use super::node_renderer::FrameNodeRenderer;

/// Renders a frame/container node as a CSS div with flex layout.
#[component]
pub fn FrameRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let style = node_to_css(node);
    let children_ids: Vec<NodeId> = node.children.clone();
    let ctx = use_frame_context();

    // Render plugin data overlays if a PluginRenderer is provided.
    let plugin_elements: Vec<Element> = if let Some(ref renderer) = ctx.plugin_renderer {
        node.plugin_data
            .iter()
            .filter_map(|(key, data)| renderer.render(key, data))
            .collect()
    } else {
        Vec::new()
    };

    rsx! {
        div {
            class: "frame-node frame-container",
            style: "{style}",
            // Recursive child rendering
            for child_id in children_ids {
                FrameNodeRenderer { key: "{child_id}", node_id: child_id }
            }
            // Plugin overlays
            for element in plugin_elements {
                {element}
            }
        }
    }
}
