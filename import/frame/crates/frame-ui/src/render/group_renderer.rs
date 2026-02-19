//! Group renderer — transparent wrapper that just renders children.

use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use crate::style::css_builder::node_to_css;
use frame_proto::NodeId;

use super::node_renderer::FrameNodeRenderer;

/// Renders a group node — a transparent wrapper that passes through
/// opacity/blend-mode but otherwise just renders its children.
#[component]
pub fn GroupRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let style = node_to_css(node);
    let children_ids: Vec<NodeId> = node.children.clone();

    rsx! {
        div {
            class: "frame-node frame-group",
            style: "{style}",
            for child_id in children_ids {
                FrameNodeRenderer { key: "{child_id}", node_id: child_id }
            }
        }
    }
}
