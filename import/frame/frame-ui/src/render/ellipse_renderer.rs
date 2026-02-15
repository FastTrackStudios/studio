//! Ellipse renderer — CSS div with 50% border-radius for full ellipses,
//! falls back to Anyrender for arcs/partial ellipses.

use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use crate::style::css_builder::node_to_css;
use frame_proto::NodeId;

/// Renders an ellipse node. Full ellipses use CSS border-radius: 50%.
/// Partial arcs will need Anyrender (TODO: wire up canvas).
#[component]
pub fn EllipseRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let mut style = node_to_css(node);
    style.push_str("border-radius: 50%; ");

    rsx! {
        div {
            class: "frame-node frame-ellipse",
            style: "{style}",
        }
    }
}
