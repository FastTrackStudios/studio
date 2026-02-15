use crate::figma::{FigmaDocumentMeta, FigmaPayload};
use crate::id::{DocumentId, NodeId};
use crate::node::FrameNode;
use figma_api::models;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrameDocumentError {
    #[error("failed to parse Figma API JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("input bytes are not JSON Figma API payload; binary .fig parsing is not implemented yet")]
    UnsupportedBinaryFig,
    #[error("root document node was not created")]
    MissingRoot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameDocument {
    pub id: DocumentId,
    pub name: String,
    pub meta: FigmaDocumentMeta,
    pub payload: FigmaPayload,
    pub root: NodeId,
    pub pages: Vec<NodeId>,
    pub nodes: std::collections::HashMap<NodeId, FrameNode>,
    pub figma_index: std::collections::HashMap<String, NodeId>,
    pub components: std::collections::HashMap<String, models::Component>,
    pub component_sets: std::collections::HashMap<String, models::ComponentSet>,
    pub styles: std::collections::HashMap<String, models::Style>,
}

impl FrameDocument {
    pub fn from_figma_api(file: models::GetFile) -> Result<Self, FrameDocumentError> {
        let mut nodes = std::collections::HashMap::new();
        let mut figma_index = std::collections::HashMap::new();
        let mut pages = Vec::new();

        let mut root_node = FrameNode::new_document((*file.document).clone());
        let root_id = root_node.id;
        root_node.parent = None;

        for canvas in &file.document.children {
            let page_id = flatten_canvas(
                canvas,
                root_id,
                &mut nodes,
                &mut figma_index,
            )?;
            pages.push(page_id);
            root_node.children.push(page_id);
        }

        if !root_node.figma_id.is_empty() {
            figma_index.insert(root_node.figma_id.clone(), root_id);
        }
        nodes.insert(root_id, root_node);

        Ok(Self {
            id: DocumentId::new(),
            name: file.name.clone(),
            meta: FigmaDocumentMeta::from_get_file(&file),
            payload: FigmaPayload::ApiGetFile(Box::new(file.clone())),
            root: root_id,
            pages,
            nodes,
            figma_index,
            components: file.components,
            component_sets: file.component_sets,
            styles: file.styles,
        })
    }

    pub fn from_figma_api_json(json: &str) -> Result<Self, FrameDocumentError> {
        let file: models::GetFile = serde_json::from_str(json)?;
        Self::from_figma_api(file)
    }

    pub fn from_source_bytes(bytes: &[u8]) -> Result<Self, FrameDocumentError> {
        if let Ok(text) = std::str::from_utf8(bytes) {
            if let Ok(doc) = Self::from_figma_api_json(text) {
                return Ok(doc);
            }
        }
        Err(FrameDocumentError::UnsupportedBinaryFig)
    }

    pub fn get_node(&self, id: NodeId) -> Option<&FrameNode> {
        self.nodes.get(&id)
    }

    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut FrameNode> {
        self.nodes.get_mut(&id)
    }

    pub fn get_by_figma_id(&self, figma_id: &str) -> Option<&FrameNode> {
        let id = self.figma_index.get(figma_id)?;
        self.nodes.get(id)
    }

    pub fn walk_subtree(&self, node_id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.walk_recursive(node_id, &mut out);
        out
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn walk_recursive(&self, node_id: NodeId, out: &mut Vec<NodeId>) {
        out.push(node_id);
        if let Some(node) = self.nodes.get(&node_id) {
            for child_id in &node.children {
                self.walk_recursive(*child_id, out);
            }
        }
    }
}

fn flatten_canvas(
    canvas: &models::CanvasNode,
    parent_id: NodeId,
    nodes: &mut std::collections::HashMap<NodeId, FrameNode>,
    figma_index: &mut std::collections::HashMap<String, NodeId>,
) -> Result<NodeId, FrameDocumentError> {
    let mut node = FrameNode::new_canvas(canvas.clone());
    let id = node.id;
    node.parent = Some(parent_id);

    for child in &canvas.children {
        let child_id = flatten_subcanvas(child, id, nodes, figma_index)?;
        node.children.push(child_id);
    }

    if !node.figma_id.is_empty() {
        figma_index.insert(node.figma_id.clone(), id);
    }
    nodes.insert(id, node);
    Ok(id)
}

fn flatten_subcanvas(
    sub: &models::SubcanvasNode,
    parent_id: NodeId,
    nodes: &mut std::collections::HashMap<NodeId, FrameNode>,
    figma_index: &mut std::collections::HashMap<String, NodeId>,
) -> Result<NodeId, FrameDocumentError> {
    let mut node = FrameNode::new_subcanvas(sub.clone());
    let id = node.id;
    node.parent = Some(parent_id);

    let children = subcanvas_children_from_raw(&node.raw)?;
    for child in children {
        let child_id = flatten_subcanvas(&child, id, nodes, figma_index)?;
        node.children.push(child_id);
    }

    if !node.figma_id.is_empty() {
        figma_index.insert(node.figma_id.clone(), id);
    }
    nodes.insert(id, node);
    Ok(id)
}

fn subcanvas_children_from_raw(
    raw: &serde_json::Value,
) -> Result<Vec<models::SubcanvasNode>, FrameDocumentError> {
    let Some(children) = raw.get("children") else {
        return Ok(Vec::new());
    };
    let Some(arr) = children.as_array() else {
        return Ok(Vec::new());
    };

    let mut out = Vec::with_capacity(arr.len());
    for value in arr {
        let parsed: models::SubcanvasNode = serde_json::from_value(value.clone())?;
        out.push(parsed);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetData {
    pub name: String,
    pub mime: Option<String>,
    pub bytes: Option<Vec<u8>>,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Image,
    Font,
    Video,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StyleDefinition {
    Raw(models::Style),
}
