use frame_proto::{FrameDocument, NodeId, RenderNodeProjection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiNode {
    pub projection: RenderNodeProjection,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiPage {
    pub page_id: NodeId,
    pub nodes: Vec<UiNode>,
}

pub fn build_ui_nodes(doc: &FrameDocument, root: NodeId) -> Vec<UiNode> {
    let mut out = Vec::new();
    push_nodes(doc, root, 0, &mut out);
    out
}

pub fn build_ui_pages(doc: &FrameDocument) -> Vec<UiPage> {
    doc.pages
        .iter()
        .copied()
        .map(|page_id| UiPage {
            page_id,
            nodes: build_ui_nodes(doc, page_id),
        })
        .collect()
}

fn push_nodes(doc: &FrameDocument, node_id: NodeId, depth: usize, out: &mut Vec<UiNode>) {
    let Some(projection) = doc.project_node(node_id) else {
        return;
    };

    let children = projection.children.clone();
    out.push(UiNode { projection, depth });

    for child in children {
        push_nodes(doc, child, depth + 1, out);
    }
}
