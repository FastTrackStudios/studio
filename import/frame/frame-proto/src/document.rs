use crate::figma::{FigmaDocumentMeta, FigmaPayload};
use crate::id::{DocumentId, NodeId};
use crate::node::FrameNode;
use base64::Engine;
use figma_api::models;
use serde::{Deserialize, Serialize};
use std::io::Read;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrameDocumentError {
    #[error("failed to parse Figma API JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("input bytes are not a supported .fig ZIP container")]
    UnsupportedBinaryFig,
    #[error("failed to read .fig ZIP container: {0}")]
    FigZip(String),
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
        Self::from_fig_zip_bytes(bytes)
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

impl FrameDocument {
    fn from_fig_zip_bytes(bytes: &[u8]) -> Result<Self, FrameDocumentError> {
        if !bytes.starts_with(b"PK\x03\x04") {
            return Err(FrameDocumentError::UnsupportedBinaryFig);
        }

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| FrameDocumentError::FigZip(e.to_string()))?;

        let meta_json = read_zip_entry(&mut archive, "meta.json").ok();
        let thumbnail_png = read_zip_entry(&mut archive, "thumbnail.png").ok();

        let meta_value = meta_json
            .as_deref()
            .and_then(|raw| serde_json::from_slice::<serde_json::Value>(raw).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        let file_name = meta_value
            .get("file_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Imported .fig")
            .to_string();

        let mut root = FrameNode::new_synthetic(serde_json::json!({
            "id": "fig-document:0",
            "name": file_name,
            "type": "DOCUMENT",
            "visible": true,
            "locked": false
        }));
        let root_id = root.id;

        let mut page = FrameNode::new_synthetic(serde_json::json!({
            "id": "fig-canvas:0",
            "name": "Canvas",
            "type": "CANVAS",
            "visible": true,
            "locked": false
        }));
        let page_id = page.id;
        page.parent = Some(root_id);
        root.children.push(page_id);

        let mut nodes = std::collections::HashMap::new();
        let mut figma_index = std::collections::HashMap::new();

        if !root.figma_id.is_empty() {
            figma_index.insert(root.figma_id.clone(), root_id);
        }
        if !page.figma_id.is_empty() {
            figma_index.insert(page.figma_id.clone(), page_id);
        }

        if let Some(png) = thumbnail_png {
            let thumb_size = meta_value
                .get("client_meta")
                .and_then(|v| v.get("thumbnail_size"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let data_url = format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            );
            let mut thumb = FrameNode::new_synthetic(serde_json::json!({
                "id": "fig-thumb:0",
                "name": "Thumbnail",
                "type": "FIG_THUMBNAIL",
                "visible": true,
                "locked": true,
                "size": {
                    "x": thumb_size.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    "y": thumb_size.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0)
                },
                "imageDataUrl": data_url
            }));
            let thumb_id = thumb.id;
            thumb.parent = Some(page_id);
            page.children.push(thumb_id);
            if !thumb.figma_id.is_empty() {
                figma_index.insert(thumb.figma_id.clone(), thumb_id);
            }
            nodes.insert(thumb_id, thumb);
        }

        nodes.insert(page_id, page);
        nodes.insert(root_id, root);

        Ok(Self {
            id: DocumentId::new(),
            name: file_name,
            meta: FigmaDocumentMeta {
                file_key: None,
                role: None,
                editor_type: Some("fig".to_string()),
                version: None,
                schema_version: None,
                last_modified: None,
                thumbnail_url: None,
            },
            payload: FigmaPayload::FigBinary {
                bytes: bytes.to_vec(),
                note: "Parsed .fig ZIP container (thumbnail fallback renderer)".to_string(),
            },
            root: root_id,
            pages: vec![page_id],
            nodes,
            figma_index,
            components: std::collections::HashMap::new(),
            component_sets: std::collections::HashMap::new(),
            styles: std::collections::HashMap::new(),
        })
    }
}

fn read_zip_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, FrameDocumentError> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| FrameDocumentError::FigZip(e.to_string()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| FrameDocumentError::FigZip(e.to_string()))?;
    Ok(bytes)
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
