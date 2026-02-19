//! Rectangle renderer — CSS div for rectangular shapes.

use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use crate::style::css_builder::node_to_css;
use frame_proto::NodeId;

/// Renders a rectangle node as a CSS div.
#[component]
pub fn RectangleRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let style = node_to_css(node);

    rsx! {
        div {
            class: "frame-node frame-rectangle",
            style: "{style}",
        }
    }
}
