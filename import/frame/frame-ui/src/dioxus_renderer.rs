use dioxus::prelude::*;
use frame_proto::{FrameDocument, NodeId, RenderNodeProjection};

use crate::model::build_ui_nodes;

#[component]
pub fn ProjectionTree(document: FrameDocument, root: NodeId) -> Element {
    let nodes = build_ui_nodes(&document, root);

    rsx! {
        div {
            class: "frame-projection-tree",
            for entry in nodes {
                NodeBox { projection: entry.projection, depth: entry.depth }
            }
        }
    }
}

#[component]
fn NodeBox(projection: RenderNodeProjection, depth: usize) -> Element {
    let indent = depth * 12;
    let style = node_style(&projection, indent);

    let label = if let Some(text) = &projection.text {
        if text.characters.is_empty() {
            projection.name.clone()
        } else {
            format!("{}: {}", projection.name, text.characters)
        }
    } else {
        projection.name.clone()
    };

    rsx! {
        div {
            class: "frame-node",
            style: "{style}",
            "{label}"
        }
    }
}

fn node_style(p: &RenderNodeProjection, indent: usize) -> String {
    let mut css = String::new();
    css.push_str("position: relative; box-sizing: border-box; ");
    css.push_str(&format!("margin-left: {}px; ", indent));
    css.push_str("padding: 6px 8px; border: 1px solid rgba(0,0,0,0.15); ");

    if let Some(size) = &p.size {
        if size.x > 0.0 {
            css.push_str(&format!("width: {}px; ", size.x));
        }
        if size.y > 0.0 {
            css.push_str(&format!("min-height: {}px; ", size.y));
        }
    }

    if let Some(opacity) = p.opacity {
        css.push_str(&format!("opacity: {}; ", opacity.clamp(0.0, 1.0)));
    }

    css
}
