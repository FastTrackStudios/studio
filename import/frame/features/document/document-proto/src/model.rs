//! The frame document model.
//!
//! A `Scene` is a tree of typed `Node`s plus a content-addressed asset table.
//! This is frame's own model: importers (Figma REST, `.fig`) translate into
//! it once, and everything downstream (render, surface runtime, AI tools,
//! the `.frame` file) reads only this.
//!
//! Values are stored *as authored* (paint opacity, stroke weight, node
//! opacity…); resolution into draw calls (opacity inheritance, layers,
//! clip decisions) is the renderer's job.

use std::collections::BTreeMap;

use facet::Facet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── ids ──────────────────────────────────────────────────────────────────

/// A node's identity. Stable across saves, edits and replicas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[facet(transparent)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

impl NodeId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An asset's identity: the lowercase hex blake3 of its bytes, so the same
/// image imported twice is stored once.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[facet(transparent)]
#[serde(transparent)]
pub struct AssetId(pub String);

impl AssetId {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── geometry ─────────────────────────────────────────────────────────────

/// Row-major 2×3 affine: `x' = a·x + b·y + tx`, `y' = c·x + d·y + ty`
/// (the same layout Figma's `relativeTransform` uses).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub tx: f64,
    pub c: f64,
    pub d: f64,
    pub ty: f64,
}

impl Transform {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, tx: 0.0, c: 0.0, d: 1.0, ty: 0.0 };

    #[must_use]
    pub const fn translate(tx: f64, ty: f64) -> Self {
        Self { a: 1.0, b: 0.0, tx, c: 0.0, d: 1.0, ty }
    }

    /// Rotation in radians, from the matrix columns (`atan2(c, a)`).
    #[must_use]
    pub fn rotation(&self) -> f64 {
        self.c.atan2(self.a)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct CornerRadii {
    pub top_left: f64,
    pub top_right: f64,
    pub bottom_right: f64,
    pub bottom_left: f64,
}

impl CornerRadii {
    #[must_use]
    pub const fn uniform(r: f64) -> Self {
        Self { top_left: r, top_right: r, bottom_right: r, bottom_left: r }
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0 && self.top_right == 0.0 && self.bottom_right == 0.0 && self.bottom_left == 0.0
    }
}

// ── paint ────────────────────────────────────────────────────────────────

/// Straight (non-premultiplied) RGBA, each channel 0..=1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    #[must_use]
    pub const fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum BlendMode {
    #[default]
    Normal,
    PassThrough,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    LinearBurn,
    LinearDodge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct GradientStop {
    pub position: f64,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ImageScaleMode {
    #[default]
    Fill,
    Fit,
    Crop,
    Tile,
    Stretch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum PaintKind {
    Solid { color: Color },
    LinearGradient { stops: Vec<GradientStop>, transform: Option<Transform> },
    RadialGradient { stops: Vec<GradientStop>, transform: Option<Transform> },
    AngularGradient { stops: Vec<GradientStop>, transform: Option<Transform> },
    Image {
        /// The image bytes, in the scene's asset table.
        asset: AssetId,
        /// The source tool's own reference (Figma `imageHash`), for re-import.
        source_ref: Option<String>,
        scale_mode: ImageScaleMode,
        transform: Option<Transform>,
    },
}

/// One entry of a fill or stroke paint stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Paint {
    pub kind: PaintKind,
    /// Paint opacity, multiplied into the paint's colors when drawn.
    pub opacity: f64,
    pub blend: BlendMode,
    pub visible: bool,
}

impl Paint {
    #[must_use]
    pub const fn solid(color: Color) -> Self {
        Self { kind: PaintKind::Solid { color }, opacity: 1.0, blend: BlendMode::Normal, visible: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum StrokeCap {
    #[default]
    None,
    Round,
    Square,
    LineArrow,
    TriangleArrow,
    DiamondFilled,
    CircleFilled,
    TriangleFilled,
    WashiTape1,
    WashiTape2,
    WashiTape3,
    WashiTape4,
    WashiTape5,
    WashiTape6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Bevel,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum StrokeAlign {
    Inside,
    Outside,
    #[default]
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Stroke {
    pub paints: Vec<Paint>,
    pub weight: f64,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub align: StrokeAlign,
    pub miter_limit: f64,
    pub dash_pattern: Vec<f64>,
    pub dash_offset: f64,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            paints: Vec::new(),
            weight: 1.0,
            cap: StrokeCap::None,
            join: StrokeJoin::Miter,
            align: StrokeAlign::Center,
            miter_limit: 4.0,
            dash_pattern: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

// ── effects ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum EffectKind {
    DropShadow,
    InnerShadow,
    LayerBlur,
    BackgroundBlur,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Effect {
    pub kind: EffectKind,
    pub visible: bool,
    pub color: Color,
    pub offset_x: f64,
    pub offset_y: f64,
    pub radius: f64,
    pub spread: f64,
}

// ── text ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum TextCase {
    #[default]
    Original,
    Upper,
    Lower,
    Title,
    SmallCaps,
    SmallCapsForced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

/// A styled span of a text node. Unset fields inherit the node's style.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct TextRun {
    pub text: String,
    pub fills: Vec<Paint>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub font_weight: Option<f64>,
    pub font_style: Option<String>,
    pub letter_spacing: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct TextData {
    pub characters: String,
    pub font_family: Option<String>,
    pub font_size: f64,
    pub font_weight: Option<f64>,
    /// Line height in px; `None` = the font's natural line height.
    pub line_height: Option<f64>,
    /// Letter spacing in px.
    pub letter_spacing: f64,
    pub text_case: TextCase,
    pub align: TextAlign,
    pub runs: Vec<TextRun>,
}

impl Default for TextData {
    fn default() -> Self {
        Self {
            characters: String::new(),
            font_family: None,
            font_size: 14.0,
            font_weight: None,
            line_height: None,
            letter_spacing: 0.0,
            text_case: TextCase::Original,
            align: TextAlign::Left,
            runs: Vec::new(),
        }
    }
}

// ── layout ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum LayoutDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
    Baseline,
}

/// Flex-style auto layout on a container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct AutoLayout {
    pub direction: LayoutDirection,
    pub gap: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
    pub wrap: bool,
    pub main_align: Option<MainAxisAlign>,
    pub cross_align: Option<CrossAxisAlign>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum AlignSelf {
    Start,
    Center,
    End,
    Stretch,
}

/// How a node sits inside an auto-layout parent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct LayoutChild {
    pub grow: Option<f64>,
    pub align_self: Option<AlignSelf>,
    /// Taken out of flow and placed at its own transform.
    pub absolute: bool,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_width: Option<f64>,
    pub max_height: Option<f64>,
}

// ── nodes ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum NodeKind {
    /// The scene root. Its children are pages.
    Document,
    Page,
    Frame,
    Group,
    Component,
    ComponentSet,
    Instance {
        /// The main component, when it is part of this scene.
        component: Option<NodeId>,
    },
    Rectangle,
    Ellipse,
    Line,
    Polygon,
    Star,
    Vector,
    BooleanOperation { op: Option<BooleanOp> },
    Text(TextData),
    /// Non-rendering helpers (slices, embeds, widgets).
    Utility,
    /// A node type this model has no dedicated kind for yet; `tag` names it.
    Other { tag: String },
}

/// Where a node came from, so re-imports can update in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SourceRef {
    /// e.g. `"figma"`.
    pub tool: String,
    /// The source tool's node id (Figma `"12:34"`).
    pub id: String,
    /// The source tool's node type (Figma `"FRAME"`).
    pub node_type: String,
    /// The node's absolute, axis-aligned bounds as the source tool computed
    /// them at import (Figma `absoluteBoundingBox`). A snapshot for exporters
    /// that reproduce the source's own output; edits don't maintain it.
    pub bounds: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum MaskKind {
    Alpha,
    Vector,
    Luminance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub visible: bool,
    pub locked: bool,

    /// Placement relative to the parent.
    pub transform: Transform,
    pub size: Size,
    pub opacity: f64,
    pub blend: BlendMode,
    pub corner_radii: CornerRadii,
    pub clips_content: bool,
    /// When set, this node masks the siblings painted after it.
    pub mask: Option<MaskKind>,

    pub fills: Vec<Paint>,
    pub stroke: Stroke,
    pub effects: Vec<Effect>,

    /// Vector outlines as SVG path data, in node-local coordinates.
    pub fill_geometry: Vec<String>,
    pub stroke_geometry: Vec<String>,
    /// A pre-rendered SVG of the whole node (an exporter's snapshot), used
    /// where native geometry can't reproduce it (booleans, detached instances).
    pub svg_snapshot: Option<AssetId>,

    pub layout: Option<AutoLayout>,
    pub layout_child: LayoutChild,

    /// Makes this node a control (knob, fader, footswitch…): its binding to
    /// an engine and the descendant nodes it drives.
    pub control: Option<surface_proto::Control>,

    pub source: Option<SourceRef>,
}

impl Node {
    /// A fresh node of `kind` with neutral defaults.
    #[must_use]
    pub fn new(kind: NodeKind, name: impl Into<String>) -> Self {
        Self {
            id: NodeId::new(),
            name: name.into(),
            kind,
            parent: None,
            children: Vec::new(),
            visible: true,
            locked: false,
            transform: Transform::IDENTITY,
            size: Size::default(),
            opacity: 1.0,
            blend: BlendMode::Normal,
            corner_radii: CornerRadii::default(),
            clips_content: false,
            mask: None,
            fills: Vec::new(),
            stroke: Stroke::default(),
            effects: Vec::new(),
            fill_geometry: Vec::new(),
            stroke_geometry: Vec::new(),
            svg_snapshot: None,
            layout: None,
            layout_child: LayoutChild::default(),
            control: None,
            source: None,
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&TextData> {
        match &self.kind {
            NodeKind::Text(t) => Some(t),
            _ => None,
        }
    }
}

// ── assets + scene ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Asset {
    pub id: AssetId,
    /// e.g. `image/png`, `image/svg+xml`, `font/ttf`.
    pub mime: String,
    pub bytes: Vec<u8>,
}

impl Asset {
    #[must_use]
    pub fn new(mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self { id: AssetId::of(&bytes), mime: mime.into(), bytes }
    }
}

/// A whole document: the node tree and the assets it references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Scene {
    pub root: NodeId,
    pub nodes: BTreeMap<NodeId, Node>,
    pub assets: BTreeMap<AssetId, Asset>,
}

impl Scene {
    /// An empty document with a root and one page.
    #[must_use]
    pub fn new(page_name: &str) -> Self {
        let mut root = Node::new(NodeKind::Document, "Document");
        let mut page = Node::new(NodeKind::Page, page_name);
        page.parent = Some(root.id);
        root.children.push(page.id);
        let root_id = root.id;
        let mut nodes = BTreeMap::new();
        nodes.insert(root.id, root);
        nodes.insert(page.id, page);
        Self { root: root_id, nodes, assets: BTreeMap::new() }
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    #[must_use]
    pub fn pages(&self) -> &[NodeId] {
        self.node(self.root).map_or(&[], |r| r.children.as_slice())
    }

    #[must_use]
    pub fn asset(&self, id: &AssetId) -> Option<&Asset> {
        self.assets.get(id)
    }

    /// Store `bytes` (deduplicated by content) and return its id.
    pub fn add_asset(&mut self, mime: impl Into<String>, bytes: Vec<u8>) -> AssetId {
        let asset = Asset::new(mime, bytes);
        let id = asset.id.clone();
        self.assets.entry(id.clone()).or_insert(asset);
        id
    }

    /// Pre-order walk of `id` and its descendants.
    #[must_use]
    pub fn walk(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let Some(node) = self.node(n) else { continue };
            out.push(n);
            stack.extend(node.children.iter().rev().copied());
        }
        out
    }

    /// Find a node by its import source id (e.g. a Figma node id).
    #[must_use]
    pub fn by_source_id(&self, source_id: &str) -> Option<&Node> {
        self.nodes.values().find(|n| n.source.as_ref().is_some_and(|s| s.id == source_id))
    }
}
