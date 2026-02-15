use crate::document::FrameDocument;
use crate::id::NodeId;
use crate::layout::{
    AutoLayout, AutoLayoutAlignSelf, AutoLayoutCounterAlign, AutoLayoutMode, AutoLayoutPositioning,
    AutoLayoutPrimaryAlign, AutoLayoutSizingMode, AutoLayoutWrap,
};
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
    pub auto_layout: Option<AutoLayout>,
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
        auto_layout: get_auto_layout_projection(raw),
        children: node.children.clone(),
    }
}

fn classify_node_type(figma_type: &str) -> RenderNodeClass {
    match figma_type {
        "DOCUMENT" | "CANVAS" | "FRAME" | "GROUP" | "COMPONENT" | "COMPONENT_SET" | "INSTANCE"
        | "SECTION" | "TRANSFORM_GROUP" | "TABLE" | "TABLE_CELL" => RenderNodeClass::Container,
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

fn get_auto_layout_projection(raw: &serde_json::Value) -> Option<AutoLayout> {
    let mode = get_string(raw, "layoutMode")
        .and_then(|v| parse_layout_mode(v.as_str()))
        .filter(|m| *m != AutoLayoutMode::None);
    let primary_axis_sizing_mode =
        get_string(raw, "primaryAxisSizingMode").and_then(|v| parse_sizing_mode(v.as_str()));
    let counter_axis_sizing_mode =
        get_string(raw, "counterAxisSizingMode").and_then(|v| parse_sizing_mode(v.as_str()));
    let primary_axis_align_items =
        get_string(raw, "primaryAxisAlignItems").and_then(|v| parse_primary_align(v.as_str()));
    let counter_axis_align_items =
        get_string(raw, "counterAxisAlignItems").and_then(|v| parse_counter_align(v.as_str()));
    let item_spacing = get_f64(raw, "itemSpacing");
    let counter_axis_spacing = get_f64(raw, "counterAxisSpacing");
    let padding_left = get_f64(raw, "paddingLeft");
    let padding_right = get_f64(raw, "paddingRight");
    let padding_top = get_f64(raw, "paddingTop");
    let padding_bottom = get_f64(raw, "paddingBottom");
    let wrap = get_string(raw, "layoutWrap").and_then(|v| parse_layout_wrap(v.as_str()));
    let positioning =
        get_string(raw, "layoutPositioning").and_then(|v| parse_layout_positioning(v.as_str()));
    let align_self = get_string(raw, "layoutAlign").and_then(|v| parse_align_self(v.as_str()));
    let grow = get_f64(raw, "layoutGrow");
    let min_width = get_f64(raw, "minWidth");
    let max_width = get_f64(raw, "maxWidth");
    let min_height = get_f64(raw, "minHeight");
    let max_height = get_f64(raw, "maxHeight");

    let out = AutoLayout {
        mode,
        primary_axis_sizing_mode,
        counter_axis_sizing_mode,
        primary_axis_align_items,
        counter_axis_align_items,
        item_spacing,
        counter_axis_spacing,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
        wrap,
        positioning,
        align_self,
        grow,
        min_width,
        max_width,
        min_height,
        max_height,
    };

    if out == AutoLayout::default() {
        None
    } else {
        Some(out)
    }
}

fn get_string(raw: &serde_json::Value, key: &str) -> Option<String> {
    raw.get(key)?.as_str().map(ToString::to_string)
}

fn get_f64(raw: &serde_json::Value, key: &str) -> Option<f64> {
    raw.get(key).and_then(|v| v.as_f64())
}

fn parse_layout_mode(v: &str) -> Option<AutoLayoutMode> {
    match v {
        "NONE" => Some(AutoLayoutMode::None),
        "HORIZONTAL" => Some(AutoLayoutMode::Horizontal),
        "VERTICAL" => Some(AutoLayoutMode::Vertical),
        _ => None,
    }
}

fn parse_sizing_mode(v: &str) -> Option<AutoLayoutSizingMode> {
    match v {
        "FIXED" => Some(AutoLayoutSizingMode::Fixed),
        "AUTO" => Some(AutoLayoutSizingMode::Hug),
        _ => None,
    }
}

fn parse_primary_align(v: &str) -> Option<AutoLayoutPrimaryAlign> {
    match v {
        "MIN" => Some(AutoLayoutPrimaryAlign::Min),
        "CENTER" => Some(AutoLayoutPrimaryAlign::Center),
        "MAX" => Some(AutoLayoutPrimaryAlign::Max),
        "SPACE_BETWEEN" => Some(AutoLayoutPrimaryAlign::SpaceBetween),
        _ => None,
    }
}

fn parse_counter_align(v: &str) -> Option<AutoLayoutCounterAlign> {
    match v {
        "MIN" => Some(AutoLayoutCounterAlign::Min),
        "CENTER" => Some(AutoLayoutCounterAlign::Center),
        "MAX" => Some(AutoLayoutCounterAlign::Max),
        "BASELINE" => Some(AutoLayoutCounterAlign::Baseline),
        _ => None,
    }
}

fn parse_layout_positioning(v: &str) -> Option<AutoLayoutPositioning> {
    match v {
        "AUTO" => Some(AutoLayoutPositioning::Auto),
        "ABSOLUTE" => Some(AutoLayoutPositioning::Absolute),
        _ => None,
    }
}

fn parse_layout_wrap(v: &str) -> Option<AutoLayoutWrap> {
    match v {
        "NO_WRAP" => Some(AutoLayoutWrap::NoWrap),
        "WRAP" => Some(AutoLayoutWrap::Wrap),
        _ => None,
    }
}

fn parse_align_self(v: &str) -> Option<AutoLayoutAlignSelf> {
    match v {
        "INHERIT" => Some(AutoLayoutAlignSelf::Inherit),
        "STRETCH" => Some(AutoLayoutAlignSelf::Stretch),
        "MIN" => Some(AutoLayoutAlignSelf::Min),
        "CENTER" => Some(AutoLayoutAlignSelf::Center),
        "MAX" => Some(AutoLayoutAlignSelf::Max),
        _ => Some(AutoLayoutAlignSelf::Auto),
    }
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
