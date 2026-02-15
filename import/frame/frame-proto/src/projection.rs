use crate::document::FrameDocument;
use crate::id::NodeId;
use crate::node::FrameNode;
use figma_api::models;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderNodeClass {
    Container,
    Text,
    Shape,
    Vector,
    Utility,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderNodeProjection {
    pub id: NodeId,
    pub figma_id: String,
    pub figma_type: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub class: RenderNodeClass,
    pub opacity: Option<f64>,
    pub blend_mode: Option<models::BlendMode>,
    pub size: Option<models::Vector>,
    pub relative_transform: Option<Vec<Vec<f64>>>,
    pub fills: Vec<models::Paint>,
    pub strokes: Vec<models::Paint>,
    pub effects: Vec<models::Effect>,
    pub fill_geometry: Vec<models::Path>,
    pub stroke_geometry: Vec<models::Path>,
    pub text: Option<RenderTextProjection>,
    pub image_data_url: Option<String>,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderTextProjection {
    pub characters: String,
    pub style: Option<models::TypeStyle>,
}

impl FrameDocument {
    pub fn project_node(&self, node_id: NodeId) -> Option<RenderNodeProjection> {
        let node = self.get_node(node_id)?;
        Some(project_node(node))
    }

    pub fn project_subtree(&self, root: NodeId) -> Vec<RenderNodeProjection> {
        self.walk_subtree(root)
            .into_iter()
            .filter_map(|id| self.project_node(id))
            .collect()
    }

    pub fn project_pages(&self) -> Vec<RenderNodeProjection> {
        self.pages
            .iter()
            .copied()
            .filter_map(|id| self.project_node(id))
            .collect()
    }
}

pub fn project_node(node: &FrameNode) -> RenderNodeProjection {
    let raw = &node.raw;

    RenderNodeProjection {
        id: node.id,
        figma_id: node.figma_id.clone(),
        figma_type: node.figma_type.clone(),
        name: node.name.clone(),
        visible: node.visible,
        locked: node.locked,
        class: classify_node_type(&node.figma_type),
        opacity: raw.get("opacity").and_then(|v| v.as_f64()),
        blend_mode: get_typed(raw, "blendMode"),
        size: get_typed(raw, "size"),
        relative_transform: get_typed(raw, "relativeTransform"),
        fills: get_typed_array(raw, "fills"),
        strokes: get_typed_array(raw, "strokes"),
        effects: get_typed_array(raw, "effects"),
        fill_geometry: get_typed_array(raw, "fillGeometry"),
        stroke_geometry: get_typed_array(raw, "strokeGeometry"),
        text: get_text_projection(raw),
        image_data_url: raw
            .get("imageDataUrl")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        children: node.children.clone(),
    }
}

fn classify_node_type(figma_type: &str) -> RenderNodeClass {
    match figma_type {
        "DOCUMENT" | "CANVAS" | "FRAME" | "GROUP" | "COMPONENT" | "COMPONENT_SET"
        | "INSTANCE" | "SECTION" | "TRANSFORM_GROUP" | "TABLE" | "TABLE_CELL" => {
            RenderNodeClass::Container
        }
        "TEXT" | "TEXT_PATH" | "SHAPE_WITH_TEXT" | "STICKY" => RenderNodeClass::Text,
        "RECTANGLE" | "ELLIPSE" | "LINE" | "REGULAR_POLYGON" | "STAR" | "BOOLEAN_OPERATION"
        | "CONNECTOR" | "WASHI_TAPE" => RenderNodeClass::Shape,
        "VECTOR" => RenderNodeClass::Vector,
        "SLICE" | "EMBED" | "LINK_UNFURL" | "WIDGET" => RenderNodeClass::Utility,
        _ => RenderNodeClass::Unknown,
    }
}

fn get_text_projection(raw: &serde_json::Value) -> Option<RenderTextProjection> {
    let characters = raw.get("characters")?.as_str()?.to_string();
    let style = get_typed(raw, "style");
    Some(RenderTextProjection { characters, style })
}

fn get_typed<T>(raw: &serde_json::Value, key: &str) -> Option<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let value = raw.get(key)?;
    serde_json::from_value(value.clone()).ok()
}

fn get_typed_array<T>(raw: &serde_json::Value, key: &str) -> Vec<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let Some(value) = raw.get(key) else {
        return Vec::new();
    };
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
        .collect()
}
