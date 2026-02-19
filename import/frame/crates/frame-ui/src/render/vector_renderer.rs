//! Vector renderer — placeholder for Anyrender canvas fallback.
//!
//! This renders nodes that can't be expressed in CSS (vectors, polygons,
//! stars, lines, boolean ops). Currently a placeholder div; will be wired
//! to Anyrender's PaintScene when the `anyrender` feature is added.

use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use frame_proto::NodeId;

/// Renders a vector/shape node. Currently a sized placeholder div.
/// When Anyrender integration is wired up, this will use a canvas
/// element with the PaintScene trait to draw paths.
#[component]
pub fn VectorRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let width = node.size.width;
    let height = node.size.height;
    let opacity = node.opacity;

    rsx! {
        div {
            class: "frame-node frame-vector",
            style: "width: {width}px; height: {height}px; opacity: {opacity};",
            // TODO: Anyrender canvas element for actual path rendering
        }
    }
}
