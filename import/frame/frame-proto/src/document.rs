use crate::figma::{FigmaDocumentMeta, FigmaPayload};
use crate::id::{DocumentId, NodeId};
use crate::node::FrameNode;
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
            let page_id = flatten_canvas(canvas, root_id, &mut nodes, &mut figma_index)?;
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
            if let Ok(doc) = Self::from_fts_export_json(text) {
                return Ok(doc);
            }
            if let Ok(doc) = Self::from_grida_bridge_json(text) {
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
    fn from_fts_export_json(json: &str) -> Result<Self, FrameDocumentError> {
        let export: FtsExportDocument = serde_json::from_str(json)?;
        if export.schema != "fts.figma.export/v1" {
            return Err(FrameDocumentError::UnsupportedBinaryFig);
        }

        let mut nodes = std::collections::HashMap::new();
        let mut figma_index = std::collections::HashMap::new();
        let mut synthetic_counter = 0_u64;

        let page_name = export
            .source
            .page
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Canvas".to_string());

        let mut root = FrameNode::new_synthetic(serde_json::json!({
            "id": "fts-document:0",
            "name": export.source.page.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "FTS Export".to_string()),
            "type": "DOCUMENT",
            "visible": true,
            "locked": false
        }));
        let root_id = root.id;

        let mut page = FrameNode::new_synthetic(serde_json::json!({
            "id": export.source.page.as_ref().map(|p| p.id.clone()).unwrap_or_else(|| "fts-canvas:0".to_string()),
            "name": page_name,
            "type": "CANVAS",
            "visible": true,
            "locked": false
        }));
        let page_id = page.id;
        page.parent = Some(root_id);
        root.children.push(page_id);

        for child in export.nodes {
            let child_id = flatten_synthetic_subtree(
                child,
                page_id,
                &mut synthetic_counter,
                &mut nodes,
                &mut figma_index,
            );
            page.children.push(child_id);
        }

        if !page.figma_id.is_empty() {
            figma_index.insert(page.figma_id.clone(), page_id);
        }
        if !root.figma_id.is_empty() {
            figma_index.insert(root.figma_id.clone(), root_id);
        }
        nodes.insert(page_id, page);
        nodes.insert(root_id, root);

        Ok(Self {
            id: DocumentId::new(),
            name: export
                .source
                .page
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "FTS Export".to_string()),
            meta: FigmaDocumentMeta {
                file_key: export.source.file_key,
                role: None,
                editor_type: Some(
                    export
                        .source
                        .editor_type
                        .unwrap_or_else(|| "figma".to_string()),
                ),
                version: None,
                schema_version: None,
                last_modified: export.generated_at,
                thumbnail_url: None,
            },
            payload: FigmaPayload::FigBinary {
                bytes: Vec::new(),
                note: "Parsed via FTS Figma export JSON".to_string(),
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

    fn from_grida_bridge_json(json: &str) -> Result<Self, FrameDocumentError> {
        let bridge: GridaBridgeDocument = serde_json::from_str(json)?;
        let file_name = bridge.source.clone();

        let mut nodes = std::collections::HashMap::new();
        let mut figma_index = std::collections::HashMap::new();
        let mut pages = Vec::new();
        let mut synthetic_counter = 0_u64;

        let mut root = FrameNode::new_synthetic(serde_json::json!({
            "id": "grida-document:0",
            "name": file_name,
            "type": "DOCUMENT",
            "visible": true,
            "locked": false
        }));
        let root_id = root.id;

        for (page_idx, page) in bridge.pages.into_iter().enumerate() {
            let mut page_raw = page.canvas;
            ensure_object_field(
                &mut page_raw,
                "id",
                serde_json::Value::String(format!("grida-canvas:{page_idx}")),
            );
            ensure_object_field(
                &mut page_raw,
                "name",
                serde_json::Value::String(page.name.clone()),
            );
            ensure_object_field(
                &mut page_raw,
                "type",
                serde_json::Value::String("CANVAS".to_string()),
            );
            ensure_object_field(&mut page_raw, "visible", serde_json::Value::Bool(true));
            ensure_object_field(&mut page_raw, "locked", serde_json::Value::Bool(false));

            let mut page_node = FrameNode::new_synthetic(page_raw);
            let page_id = page_node.id;
            page_node.parent = Some(root_id);

            for child in page.root_nodes {
                let child_id = flatten_synthetic_subtree(
                    child,
                    page_id,
                    &mut synthetic_counter,
                    &mut nodes,
                    &mut figma_index,
                );
                page_node.children.push(child_id);
            }

            if !page_node.figma_id.is_empty() {
                figma_index.insert(page_node.figma_id.clone(), page_id);
            }
            nodes.insert(page_id, page_node);
            pages.push(page_id);
            root.children.push(page_id);
        }

        if !root.figma_id.is_empty() {
            figma_index.insert(root.figma_id.clone(), root_id);
        }
        nodes.insert(root_id, root);

        Ok(Self {
            id: DocumentId::new(),
            name: file_name,
            meta: FigmaDocumentMeta {
                file_key: None,
                role: None,
                editor_type: Some("fig".to_string()),
                version: bridge.metadata.version.map(|v| v.to_string()),
                schema_version: None,
                last_modified: None,
                thumbnail_url: None,
            },
            payload: FigmaPayload::FigBinary {
                bytes: Vec::new(),
                note: "Parsed via Grida .fig bridge JSON".to_string(),
            },
            root: root_id,
            pages,
            nodes,
            figma_index,
            components: std::collections::HashMap::new(),
            component_sets: std::collections::HashMap::new(),
            styles: std::collections::HashMap::new(),
        })
    }

    fn from_fig_zip_bytes(bytes: &[u8]) -> Result<Self, FrameDocumentError> {
        if !bytes.starts_with(b"PK\x03\x04") {
            return Err(FrameDocumentError::UnsupportedBinaryFig);
        }

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| FrameDocumentError::FigZip(e.to_string()))?;

        let meta_json = read_zip_entry(&mut archive, "meta.json").ok();
        let canvas_fig = read_zip_entry(&mut archive, "canvas.fig")
            .map_err(|_| FrameDocumentError::UnsupportedBinaryFig)?;

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

        // Intentionally no thumbnail fallback render node here. We expose only
        // a binary canvas placeholder so renderer development targets actual
        // canvas decoding paths instead of image fallback.
        let mut canvas_binary = FrameNode::new_synthetic(serde_json::json!({
            "id": "fig-canvas-binary:0",
            "name": "canvas.fig (binary)",
            "type": "FIG_CANVAS_BINARY",
            "visible": true,
            "locked": true,
            "canvasFigBytes": canvas_fig.len()
        }));
        let canvas_binary_id = canvas_binary.id;
        canvas_binary.parent = Some(page_id);
        page.children.push(canvas_binary_id);
        if !canvas_binary.figma_id.is_empty() {
            figma_index.insert(canvas_binary.figma_id.clone(), canvas_binary_id);
        }
        nodes.insert(canvas_binary_id, canvas_binary);

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

fn flatten_synthetic_subtree(
    mut raw: serde_json::Value,
    parent_id: NodeId,
    synthetic_counter: &mut u64,
    nodes: &mut std::collections::HashMap<NodeId, FrameNode>,
    figma_index: &mut std::collections::HashMap<String, NodeId>,
) -> NodeId {
    ensure_object_field(
        &mut raw,
        "id",
        serde_json::Value::String({
            let next = *synthetic_counter;
            *synthetic_counter += 1;
            format!("grida-node:{next}")
        }),
    );
    ensure_object_field(
        &mut raw,
        "name",
        serde_json::Value::String("Unnamed".to_string()),
    );
    ensure_object_field(
        &mut raw,
        "type",
        serde_json::Value::String("UNKNOWN".to_string()),
    );
    ensure_object_field(&mut raw, "visible", serde_json::Value::Bool(true));
    ensure_object_field(&mut raw, "locked", serde_json::Value::Bool(false));

    let child_values = raw
        .get("children")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut node = FrameNode::new_synthetic(raw);
    let id = node.id;
    node.parent = Some(parent_id);

    for child in child_values {
        let child_id = flatten_synthetic_subtree(child, id, synthetic_counter, nodes, figma_index);
        node.children.push(child_id);
    }

    if !node.figma_id.is_empty() {
        figma_index.insert(node.figma_id.clone(), id);
    }
    nodes.insert(id, node);
    id
}

fn ensure_object_field(target: &mut serde_json::Value, key: &str, default: serde_json::Value) {
    if let Some(obj) = target.as_object_mut() {
        if obj.get(key).is_none() {
            obj.insert(key.to_string(), default);
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GridaBridgeDocument {
    source: String,
    #[serde(default)]
    metadata: GridaBridgeMetadata,
    #[serde(default)]
    pages: Vec<GridaBridgePage>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GridaBridgeMetadata {
    version: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GridaBridgePage {
    name: String,
    canvas: serde_json::Value,
    #[serde(rename = "rootNodes")]
    #[serde(default)]
    root_nodes: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct FtsExportDocument {
    schema: String,
    #[serde(rename = "generatedAt")]
    generated_at: Option<String>,
    source: FtsExportSource,
    #[serde(default)]
    nodes: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct FtsExportSource {
    #[serde(rename = "editorType")]
    editor_type: Option<String>,
    #[serde(rename = "fileKey")]
    file_key: Option<String>,
    page: Option<FtsExportPage>,
}

#[derive(Debug, Clone, Deserialize)]
struct FtsExportPage {
    id: String,
    name: String,
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
