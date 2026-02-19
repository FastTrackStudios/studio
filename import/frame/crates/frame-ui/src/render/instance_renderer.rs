//! Instance renderer — renders a component instance with overrides applied.

use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use crate::style::css_builder::node_to_css;
use frame_proto::NodeId;

use super::node_renderer::FrameNodeRenderer;

/// Renders a component instance. For now this works like a frame —
/// it renders its children with CSS styling. Override application
/// will be expanded later (requires deep-cloning the component tree
/// and patching properties).
#[component]
pub fn InstanceRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let style = node_to_css(node);
    let children_ids: Vec<NodeId> = node.children.clone();

    rsx! {
        div {
            class: "frame-node frame-instance",
            style: "{style}",
            for child_id in children_ids {
                FrameNodeRenderer { key: "{child_id}", node_id: child_id }
            }
        }
    }
}
