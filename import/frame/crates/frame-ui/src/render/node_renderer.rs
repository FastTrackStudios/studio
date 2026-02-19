//! Recursive node renderer — dispatches to type-specific Dioxus components.
//!
//! Follows the DockNodeRenderer pattern at dock-dioxus/src/components/dock_root.rs.

use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use frame_proto::node::NodeKind;
use frame_proto::NodeId;

use super::ellipse_renderer::EllipseRenderer;
use super::frame_renderer::FrameRenderer;
use super::group_renderer::GroupRenderer;
use super::instance_renderer::InstanceRenderer;
use super::rectangle_renderer::RectangleRenderer;
use super::text_renderer::TextRenderer;
use super::vector_renderer::VectorRenderer;

/// Recursive node renderer — reads the document signal and dispatches to
/// the correct Dioxus component based on NodeKind.
#[component]
pub fn FrameNodeRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => {
            return rsx! {
                div { class: "frame-missing-node",
                    "Missing node"
                }
            }
        }
    };

    // Invisible nodes render nothing.
    if !node.visible {
        return rsx! {};
    }

    // CSS-renderable nodes get lightweight div-based components.
    // Complex vectors/shapes fall through to the Anyrender canvas.
    if node.is_css_renderable() {
        match &node.kind {
            NodeKind::Frame { .. }
            | NodeKind::Component { .. }
            | NodeKind::Section => rsx! { FrameRenderer { node_id } },

            NodeKind::Rectangle { .. } => rsx! { RectangleRenderer { node_id } },
            NodeKind::Text { .. } => rsx! { TextRenderer { node_id } },
            NodeKind::Group => rsx! { GroupRenderer { node_id } },

            NodeKind::Instance { .. } => rsx! { InstanceRenderer { node_id } },

            NodeKind::Page { .. } => rsx! { FrameRenderer { node_id } },

            // Slice has no visual — just render children.
            NodeKind::Slice => rsx! { GroupRenderer { node_id } },

            // Fallback: CSS-renderable but unmatched → frame renderer.
            _ => rsx! { FrameRenderer { node_id } },
        }
    } else {
        // Ellipses, vectors, polygons, stars, lines, boolean ops → Anyrender.
        match &node.kind {
            NodeKind::Ellipse { .. } => rsx! { EllipseRenderer { node_id } },
            _ => rsx! { VectorRenderer { node_id } },
        }
    }
}
