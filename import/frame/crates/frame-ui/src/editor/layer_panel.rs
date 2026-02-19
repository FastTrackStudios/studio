//! Layer panel — tree view of the document's node hierarchy.

use crate::prelude::*;
use crate::signals::*;
use frame_proto::NodeId;

/// Layer tree panel showing the document hierarchy.
#[component]
pub fn LayerPanel() -> Element {
    let active_page = FRAME_ACTIVE_PAGE.read();
    let doc = FRAME_DOCUMENT.read();

    let page_id: Option<NodeId> = *active_page;

    rsx! {
        div {
            class: "frame-layer-panel",
            style: "padding: 8px; font-size: 12px;",

            if let Some(page_id) = page_id {
                if doc.get_node(page_id).is_some() {
                    LayerTreeNode { node_id: page_id, depth: 0 }
                }
            }
        }
    }
}

/// A single entry in the layer tree, rendered recursively.
#[component]
fn LayerTreeNode(node_id: NodeId, depth: u32) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let name = node.name.clone();
    let children_ids: Vec<NodeId> = node.children.clone();
    let indent = depth * 16;

    rsx! {
        div {
            class: "frame-layer-item",

            div {
                style: "padding: 2px 4px; padding-left: {indent}px; cursor: pointer;",
                "{name}"
            }

            for child_id in children_ids {
                LayerTreeNode { key: "{child_id}", node_id: child_id, depth: depth + 1 }
            }
        }
    }
}
