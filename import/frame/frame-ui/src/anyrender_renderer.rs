use frame_proto::{FrameDocument, NodeId, RenderNodeClass};
#[cfg(feature = "anyrender")]
use kurbo::Shape;
use taffy::prelude::*;
use taffy::TaffyTree;

#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    fn with_opacity(&self, opacity: f64) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: (self.a * opacity).clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaintPrimitive {
    Rect {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill: Option<Rgba>,
        stroke: Option<(Rgba, f64)>,
        corner_radius: f64,
    },
    Text {
        node_id: NodeId,
        x: f64,
        y: f64,
        text: String,
        color: Rgba,
        font_size: f64,
    },
    Path {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill_paths: Vec<String>,
        stroke_paths: Vec<String>,
        svg_base64: Option<String>,
        fill: Option<Rgba>,
        stroke: Option<(Rgba, f64)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn build_paint_primitives(doc: &FrameDocument, root: NodeId) -> Vec<PaintPrimitive> {
    let layout_boxes = build_layout_boxes(doc, root);
    let mut out = Vec::new();
    collect_primitives_recursive(doc, root, 0.0, 0.0, false, &layout_boxes, &mut out);

    out
}

pub fn build_layout_boxes(
    doc: &FrameDocument,
    root: NodeId,
) -> std::collections::HashMap<NodeId, LayoutBox> {
    compute_layout_boxes(doc, root)
}

fn collect_primitives_recursive(
    doc: &FrameDocument,
    node_id: NodeId,
    parent_x: f64,
    parent_y: f64,
    force_local_positioning: bool,
    layout_boxes: &std::collections::HashMap<NodeId, LayoutBox>,
    out: &mut Vec<PaintPrimitive>,
) {
    let Some(node) = doc.get_node(node_id) else {
        return;
    };
    if !node.visible {
        return;
    }

    let opacity = node
        .raw
        .get("opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);

    let (x, y, width, height) = if !force_local_positioning {
        layout_boxes
            .get(&node_id)
            .map(|b| (b.x, b.y, b.width, b.height))
            .unwrap_or_else(|| {
                let (local_x, local_y, width, height) = extract_local_rect(&node.raw);
                (parent_x + local_x, parent_y + local_y, width, height)
            })
    } else {
        let (local_x, local_y, width, height) = extract_local_rect(&node.raw);
        (parent_x + local_x, parent_y + local_y, width, height)
    };

    let fill = first_solid_rgba(&node.raw, "fills").map(|c| c.with_opacity(opacity));
    let stroke = first_solid_rgba(&node.raw, "strokes")
        .map(|c| {
            let w = node
                .raw
                .get("strokeWeight")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0)
                .max(0.0);
            (c.with_opacity(opacity), w)
        })
        .filter(|(_, w)| *w > 0.0);
    let corner_radius = node
        .raw
        .get("cornerRadius")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .max(0.0);

    let fill_paths = path_strings(&node.raw, "fillGeometry");
    let stroke_paths = path_strings(&node.raw, "strokeGeometry");
    let svg_base64 = exported_svg_base64(&node.raw);
    let has_vector_geometry =
        !fill_paths.is_empty() || !stroke_paths.is_empty() || svg_base64.is_some();
    let is_instance = node.figma_type == "INSTANCE";

    let mut rendered_as_atomic_svg = false;

    match doc.project_node(node_id).map(|p| p.class) {
        Some(RenderNodeClass::Text) => {
            let text = node
                .raw
                .get("characters")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if !text.is_empty() {
                let color = fill.clone().unwrap_or(Rgba {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: opacity,
                });
                let font_size = node
                    .raw
                    .get("style")
                    .and_then(|v| v.get("fontSize"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(14.0)
                    .max(1.0);

                out.push(PaintPrimitive::Text {
                    node_id,
                    x,
                    y,
                    text,
                    color,
                    font_size,
                });
            }
        }
        _ if is_instance && svg_base64.is_some() => {
            // Prefer exported instance SVG over component-master fallback. This
            // preserves variant/override visuals from Figma and avoids drawing
            // stale master internals at incorrect offsets.
            rendered_as_atomic_svg = true;
            out.push(PaintPrimitive::Path {
                node_id,
                x,
                y,
                width,
                height,
                fill_paths,
                stroke_paths,
                svg_base64,
                fill,
                stroke,
            });
        }
        Some(RenderNodeClass::Vector) | Some(RenderNodeClass::Shape) if has_vector_geometry => {
            out.push(PaintPrimitive::Path {
                node_id,
                x,
                y,
                width,
                height,
                fill_paths,
                stroke_paths,
                svg_base64,
                fill,
                stroke,
            });
        }
        _ => {
            if width > 0.0 && height > 0.0 {
                out.push(PaintPrimitive::Rect {
                    node_id,
                    x,
                    y,
                    width,
                    height,
                    fill,
                    stroke,
                    corner_radius,
                });
            }
        }
    }

    if rendered_as_atomic_svg {
        return;
    }

    for child_id in &node.children {
        collect_primitives_recursive(doc, *child_id, x, y, false, layout_boxes, out);
    }

    // Bridge JSON often encodes INSTANCEs without child trees. Resolve from
    // component master definitions so we still render the actual visuals.
    let has_children = !node.children.is_empty();
    let has_svg_export = exported_svg_base64(&node.raw).is_some();
    if is_instance && !has_children && !has_svg_export {
        if let Some(component_id) = node.raw.get("componentId").and_then(|v| v.as_str()) {
            if let Some(component_node) = doc.get_by_figma_id(component_id) {
                for child_id in &component_node.children {
                    collect_primitives_recursive(doc, *child_id, x, y, true, layout_boxes, out);
                }
            }
        }
    }
}

fn compute_layout_boxes(
    doc: &FrameDocument,
    root: NodeId,
) -> std::collections::HashMap<NodeId, LayoutBox> {
    let mut out = std::collections::HashMap::new();
    let (x, y, width, height) = extract_node_rect(doc, root);
    compute_layout_boxes_recursive(doc, root, x, y, width, height, &mut out);
    out
}

fn compute_layout_boxes_recursive(
    doc: &FrameDocument,
    node_id: NodeId,
    abs_x: f64,
    abs_y: f64,
    width: f64,
    height: f64,
    out: &mut std::collections::HashMap<NodeId, LayoutBox>,
) {
    out.insert(
        node_id,
        LayoutBox {
            x: abs_x,
            y: abs_y,
            width,
            height,
        },
    );

    let Some(node) = doc.get_node(node_id) else {
        return;
    };
    let Some(projection) = doc.project_node(node_id) else {
        return;
    };
    let children = node.children.clone();
    if children.is_empty() {
        return;
    }

    let is_auto_layout = projection
        .auto_layout
        .as_ref()
        .and_then(|l| l.mode)
        .filter(|m| {
            matches!(
                m,
                frame_proto::AutoLayoutMode::Horizontal | frame_proto::AutoLayoutMode::Vertical
            )
        })
        .is_some();

    if !is_auto_layout {
        for child_id in children {
            let (cx, cy, cw, ch) = extract_node_rect(doc, child_id);
            compute_layout_boxes_recursive(doc, child_id, abs_x + cx, abs_y + cy, cw, ch, out);
        }
        return;
    }

    let mut taffy = TaffyTree::<()>::new();
    let mut child_nodes: Vec<(NodeId, taffy::NodeId)> = Vec::new();

    for child_id in &children {
        let style = child_taffy_style(doc, *child_id);
        if let Ok(id) = taffy.new_leaf(style) {
            child_nodes.push((*child_id, id));
        }
    }

    let mut taffy_children = Vec::with_capacity(child_nodes.len());
    for (_, id) in &child_nodes {
        taffy_children.push(*id);
    }

    let container_style = container_taffy_style(&projection, width, height);
    let Ok(container) = taffy.new_with_children(container_style, &taffy_children) else {
        for child_id in children {
            let (cx, cy, cw, ch) = extract_node_rect(doc, child_id);
            compute_layout_boxes_recursive(doc, child_id, abs_x + cx, abs_y + cy, cw, ch, out);
        }
        return;
    };

    let _ = taffy.compute_layout(
        container,
        Size {
            width: AvailableSpace::Definite(width as f32),
            height: AvailableSpace::Definite(height as f32),
        },
    );

    for (child_id, taffy_id) in child_nodes {
        if let Ok(layout) = taffy.layout(taffy_id) {
            let cx = abs_x + layout.location.x as f64;
            let cy = abs_y + layout.location.y as f64;
            let cw = layout.size.width as f64;
            let ch = layout.size.height as f64;
            compute_layout_boxes_recursive(doc, child_id, cx, cy, cw, ch, out);
        } else {
            let (cx, cy, cw, ch) = extract_node_rect(doc, child_id);
            compute_layout_boxes_recursive(doc, child_id, abs_x + cx, abs_y + cy, cw, ch, out);
        }
    }
}

fn extract_node_rect(doc: &FrameDocument, node_id: NodeId) -> (f64, f64, f64, f64) {
    doc.get_node(node_id)
        .map(|n| extract_local_rect(&n.raw))
        .unwrap_or((0.0, 0.0, 0.0, 0.0))
}

fn container_taffy_style(
    projection: &frame_proto::RenderNodeProjection,
    width: f64,
    height: f64,
) -> Style {
    let mut style = Style {
        display: Display::Flex,
        size: Size {
            width: Dimension::Length(width as f32),
            height: Dimension::Length(height as f32),
        },
        ..Default::default()
    };

    if let Some(layout) = &projection.auto_layout {
        if let Some(mode) = layout.mode {
            style.flex_direction = match mode {
                frame_proto::AutoLayoutMode::Horizontal => FlexDirection::Row,
                frame_proto::AutoLayoutMode::Vertical => FlexDirection::Column,
                frame_proto::AutoLayoutMode::None => FlexDirection::Row,
            };
        }
        if let Some(v) = layout.item_spacing {
            style.gap = if style.flex_direction == FlexDirection::Column {
                Size {
                    width: LengthPercentage::Length(0.0),
                    height: LengthPercentage::Length(v as f32),
                }
            } else {
                Size {
                    width: LengthPercentage::Length(v as f32),
                    height: LengthPercentage::Length(0.0),
                }
            };
        }
        style.padding = Rect {
            left: LengthPercentage::Length(layout.padding_left.unwrap_or(0.0) as f32),
            right: LengthPercentage::Length(layout.padding_right.unwrap_or(0.0) as f32),
            top: LengthPercentage::Length(layout.padding_top.unwrap_or(0.0) as f32),
            bottom: LengthPercentage::Length(layout.padding_bottom.unwrap_or(0.0) as f32),
        };
        if let Some(wrap) = layout.wrap {
            style.flex_wrap = match wrap {
                frame_proto::AutoLayoutWrap::Wrap => FlexWrap::Wrap,
                frame_proto::AutoLayoutWrap::NoWrap => FlexWrap::NoWrap,
            };
        }
        style.justify_content = layout.primary_axis_align_items.map(|a| match a {
            frame_proto::AutoLayoutPrimaryAlign::Min => JustifyContent::Start,
            frame_proto::AutoLayoutPrimaryAlign::Center => JustifyContent::Center,
            frame_proto::AutoLayoutPrimaryAlign::Max => JustifyContent::End,
            frame_proto::AutoLayoutPrimaryAlign::SpaceBetween => JustifyContent::SpaceBetween,
        });
        style.align_items = layout.counter_axis_align_items.map(|a| match a {
            frame_proto::AutoLayoutCounterAlign::Min => AlignItems::Start,
            frame_proto::AutoLayoutCounterAlign::Center => AlignItems::Center,
            frame_proto::AutoLayoutCounterAlign::Max => AlignItems::End,
            frame_proto::AutoLayoutCounterAlign::Baseline => AlignItems::Baseline,
        });
    }

    style
}

fn child_taffy_style(doc: &FrameDocument, node_id: NodeId) -> Style {
    let Some(node) = doc.get_node(node_id) else {
        return Style::default();
    };
    let projection = doc.project_node(node_id);
    let (x, y, width, height) = extract_local_rect(&node.raw);
    let mut style = Style {
        size: Size {
            width: Dimension::Length(width as f32),
            height: Dimension::Length(height as f32),
        },
        // Figma auto-layout children should not collapse when parent is constrained.
        // Keep explicit node size from Figma and disable shrink by default.
        flex_shrink: 0.0,
        min_size: Size {
            width: Dimension::Length(
                projection
                    .as_ref()
                    .and_then(|p| p.auto_layout.as_ref())
                    .and_then(|l| l.min_width)
                    .unwrap_or(0.0) as f32,
            ),
            height: Dimension::Length(
                projection
                    .as_ref()
                    .and_then(|p| p.auto_layout.as_ref())
                    .and_then(|l| l.min_height)
                    .unwrap_or(0.0) as f32,
            ),
        },
        max_size: Size {
            width: projection
                .as_ref()
                .and_then(|p| p.auto_layout.as_ref())
                .and_then(|l| l.max_width)
                .map(|v| Dimension::Length(v as f32))
                .unwrap_or(Dimension::Auto),
            height: projection
                .as_ref()
                .and_then(|p| p.auto_layout.as_ref())
                .and_then(|l| l.max_height)
                .map(|v| Dimension::Length(v as f32))
                .unwrap_or(Dimension::Auto),
        },
        ..Default::default()
    };

    if let Some(layout) = projection.as_ref().and_then(|p| p.auto_layout.as_ref()) {
        if let Some(grow) = layout.grow {
            style.flex_grow = grow as f32;
        }
        style.align_self = layout.align_self.and_then(|a| match a {
            frame_proto::AutoLayoutAlignSelf::Auto | frame_proto::AutoLayoutAlignSelf::Inherit => {
                None
            }
            frame_proto::AutoLayoutAlignSelf::Min => Some(AlignSelf::Start),
            frame_proto::AutoLayoutAlignSelf::Center => Some(AlignSelf::Center),
            frame_proto::AutoLayoutAlignSelf::Max => Some(AlignSelf::End),
            frame_proto::AutoLayoutAlignSelf::Stretch => Some(AlignSelf::Stretch),
        });
        if matches!(
            layout.positioning,
            Some(frame_proto::AutoLayoutPositioning::Absolute)
        ) {
            style.position = Position::Absolute;
            style.inset = Rect {
                left: LengthPercentageAuto::Length(x as f32),
                right: LengthPercentageAuto::Auto,
                top: LengthPercentageAuto::Length(y as f32),
                bottom: LengthPercentageAuto::Auto,
            };
        }
        // NOTE: Do not map Hug -> Dimension::Auto here without measurement callbacks.
        // Taffy cannot infer intrinsic size for vector/text groups in this pipeline and
        // collapses nodes, which causes overlap. We keep explicit Figma sizes for now.
    }

    style
}

fn extract_local_rect(raw: &serde_json::Value) -> (f64, f64, f64, f64) {
    // Prefer local node coordinates so rendering is stable regardless of how
    // upstream exporters populate absoluteBoundingBox (which may be global,
    // page-relative, or component-library-relative).
    let mut x = 0.0;
    let mut y = 0.0;
    if let Some(t) = raw.get("relativeTransform").and_then(|v| v.as_array()) {
        if t.len() >= 2 {
            x = t[0]
                .as_array()
                .and_then(|row| row.get(2))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            y = t[1]
                .as_array()
                .and_then(|row| row.get(2))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
        }
    }

    let w = raw
        .get("size")
        .and_then(|v| v.get("x"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let h = raw
        .get("size")
        .and_then(|v| v.get("y"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    (x, y, w, h)
}

fn first_solid_rgba(raw: &serde_json::Value, key: &str) -> Option<Rgba> {
    let arr = raw.get(key)?.as_array()?;
    for paint in arr {
        let visible = paint
            .get("visible")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !visible {
            continue;
        }
        if paint.get("type").and_then(|v| v.as_str()) != Some("SOLID") {
            continue;
        }

        let color = paint.get("color")?;
        let opacity = paint
            .get("opacity")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        let r = color.get("r")?.as_f64()?.clamp(0.0, 1.0);
        let g = color.get("g")?.as_f64()?.clamp(0.0, 1.0);
        let b = color.get("b")?.as_f64()?.clamp(0.0, 1.0);
        let a = color
            .get("a")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        return Some(Rgba {
            r,
            g,
            b,
            a: (a * opacity).clamp(0.0, 1.0),
        });
    }

    None
}

fn path_strings(raw: &serde_json::Value, key: &str) -> Vec<String> {
    raw.get(key)
        .and_then(|v| v.as_array())
        .into_iter()
        .flat_map(|arr| arr.iter())
        .filter_map(|entry| {
            entry
                .get("path")
                .and_then(|v| v.as_str())
                .or_else(|| entry.get("data").and_then(|v| v.as_str()))
        })
        .map(ToString::to_string)
        .collect()
}

fn exported_svg_base64(raw: &serde_json::Value) -> Option<String> {
    raw.get("exports")
        .and_then(|v| v.get("svgBase64"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

#[cfg(feature = "anyrender")]
#[derive(Debug, Clone, Copy)]
pub struct TextFontRef<'a> {
    pub bytes: &'a [u8],
    pub index: u32,
}

#[cfg(feature = "anyrender")]
pub fn paint_into_scene(scene: &mut impl anyrender::PaintScene, doc: &FrameDocument, root: NodeId) {
    paint_into_scene_with(scene, doc, root, kurbo::Affine::IDENTITY, None);
}

#[cfg(feature = "anyrender")]
pub fn paint_into_scene_with_font(
    scene: &mut impl anyrender::PaintScene,
    doc: &FrameDocument,
    root: NodeId,
    font: TextFontRef<'_>,
) {
    paint_into_scene_with(scene, doc, root, kurbo::Affine::IDENTITY, Some(font));
}

#[cfg(feature = "anyrender")]
pub fn paint_into_scene_with(
    scene: &mut impl anyrender::PaintScene,
    doc: &FrameDocument,
    root: NodeId,
    scene_transform: kurbo::Affine,
    text_font: Option<TextFontRef<'_>>,
) {
    let primitives = build_paint_primitives(doc, root);
    paint_primitives_into_scene_with(scene, &primitives, scene_transform, text_font);
}

#[cfg(feature = "anyrender")]
pub fn paint_primitives_into_scene_with(
    scene: &mut impl anyrender::PaintScene,
    primitives: &[PaintPrimitive],
    scene_transform: kurbo::Affine,
    text_font: Option<TextFontRef<'_>>,
) {
    use anyrender::Glyph;
    use base64::Engine;
    use kurbo::{BezPath, Rect, RoundedRect, Stroke};
    use peniko::{Blob, Fill, FontData};
    use skrifa::instance::Size;
    use skrifa::prelude::LocationRef;
    use skrifa::raw::{FileRef, FontRef};
    use skrifa::MetadataProvider;
    use std::sync::Arc;

    let font_data: Option<FontData> =
        text_font.map(|font| FontData::new(Blob::new(Arc::new(font.bytes.to_vec())), font.index));

    let font_ref: Option<FontRef<'_>> = font_data.as_ref().and_then(|f| {
        let file_ref = FileRef::new(f.data.as_ref()).ok()?;
        match file_ref {
            FileRef::Font(font) => Some(font),
            FileRef::Collection(collection) => collection.get(f.index).ok(),
        }
    });

    for primitive in primitives.iter().cloned() {
        match primitive {
            PaintPrimitive::Rect {
                x,
                y,
                width,
                height,
                fill,
                stroke,
                corner_radius,
                ..
            } => {
                let rect = Rect::new(x, y, x + width, y + height);
                if corner_radius > 0.0 {
                    let rr = RoundedRect::from_rect(rect, corner_radius);
                    if let Some(fill) = fill {
                        scene.fill(Fill::NonZero, scene_transform, to_peniko(fill), None, &rr);
                    }
                    if let Some((stroke_color, stroke_width)) = stroke {
                        if stroke_width > 0.0 {
                            scene.stroke(
                                &Stroke::new(stroke_width),
                                scene_transform,
                                to_peniko(stroke_color),
                                None,
                                &rr,
                            );
                        }
                    }
                } else {
                    if let Some(fill) = fill {
                        scene.fill(Fill::NonZero, scene_transform, to_peniko(fill), None, &rect);
                    }
                    if let Some((stroke_color, stroke_width)) = stroke {
                        if stroke_width > 0.0 {
                            scene.stroke(
                                &Stroke::new(stroke_width),
                                scene_transform,
                                to_peniko(stroke_color),
                                None,
                                &rect,
                            );
                        }
                    }
                }
            }
            PaintPrimitive::Path {
                x: path_x,
                y: path_y,
                width,
                height,
                fill_paths,
                stroke_paths,
                svg_base64,
                fill,
                stroke,
                ..
            } => {
                let mut had_any_path = false;
                let path_origin_transform =
                    scene_transform * kurbo::Affine::translate((path_x, path_y));
                let path_fit =
                    compute_path_fit_transform(&fill_paths, &stroke_paths, width, height);
                let path_transform = path_origin_transform * path_fit;
                let (sx, sy) = extract_scale(path_fit);
                let stroke_scale = ((sx.abs() + sy.abs()) * 0.5).max(0.01);

                if let Some(svg_base64) = svg_base64.as_ref() {
                    if let Ok(svg_bytes) =
                        base64::engine::general_purpose::STANDARD.decode(svg_base64)
                    {
                        if let Ok(svg_text) = String::from_utf8(svg_bytes) {
                            let svg_fit = compute_svg_fit_transform(&svg_text, width, height);
                            let svg_transform = path_origin_transform * svg_fit;
                            if anyrender_svg::render_svg_str(scene, &svg_text, svg_transform)
                                .is_ok()
                            {
                                had_any_path = true;
                            }
                        }
                    }
                }

                if !had_any_path {
                    if let Some(fill_color) = fill.as_ref() {
                        for path_data in &fill_paths {
                            if let Ok(path) = BezPath::from_svg(path_data) {
                                scene.fill(
                                    Fill::NonZero,
                                    path_transform,
                                    to_peniko(fill_color.clone()),
                                    None,
                                    &path,
                                );
                                had_any_path = true;
                            }
                        }
                    }

                    if let Some((stroke_color, stroke_width)) = stroke.as_ref() {
                        if *stroke_width > 0.0 {
                            for path_data in &stroke_paths {
                                if let Ok(path) = BezPath::from_svg(path_data) {
                                    scene.stroke(
                                        &Stroke::new(*stroke_width * stroke_scale),
                                        path_transform,
                                        to_peniko(stroke_color.clone()),
                                        None,
                                        &path,
                                    );
                                    had_any_path = true;
                                }
                            }
                        }
                    }
                }

                if !had_any_path {
                    // fallback bounds draw when path payload is missing/unparseable
                    let rect = Rect::new(path_x, path_y, path_x + width, path_y + height);
                    if let Some(fill) = fill.as_ref() {
                        scene.fill(
                            Fill::NonZero,
                            scene_transform,
                            to_peniko(fill.clone()),
                            None,
                            &rect,
                        );
                    }
                    if let Some((stroke_color, stroke_width)) = stroke.as_ref() {
                        if *stroke_width > 0.0 {
                            scene.stroke(
                                &Stroke::new(*stroke_width),
                                scene_transform,
                                to_peniko(stroke_color.clone()),
                                None,
                                &rect,
                            );
                        }
                    }
                }
            }
            PaintPrimitive::Text {
                x,
                y,
                text,
                color,
                font_size,
                ..
            } => {
                if let (Some(font_data), Some(font_ref)) = (&font_data, font_ref.as_ref()) {
                    let size = Size::new(font_size as f32);
                    let charmap = font_ref.charmap();
                    let glyph_metrics = font_ref.glyph_metrics(size, LocationRef::default());

                    let mut glyphs = Vec::new();
                    let mut pen_x = 0.0_f32;

                    for ch in text.chars() {
                        if ch == '\n' {
                            continue;
                        }
                        let gid = charmap.map(ch).unwrap_or_default();
                        let advance = glyph_metrics.advance_width(gid).unwrap_or_default();
                        glyphs.push(Glyph {
                            id: gid.to_u32(),
                            x: pen_x,
                            y: 0.0,
                        });
                        pen_x += advance;
                    }

                    scene.draw_glyphs(
                        font_data,
                        font_size as f32,
                        false,
                        &[],
                        Fill::NonZero,
                        to_peniko(color),
                        1.0,
                        scene_transform * kurbo::Affine::translate((x, y + font_size)),
                        None,
                        glyphs.into_iter(),
                    );
                } else {
                    // fallback to a baseline block when no text font is configured
                    let width = (text.chars().count() as f64 * font_size * 0.5).max(font_size);
                    let line = Rect::new(x, y + font_size * 0.85, x + width, y + font_size * 0.95);
                    scene.fill(
                        Fill::NonZero,
                        scene_transform,
                        to_peniko(color),
                        None,
                        &line,
                    );
                }
            }
        }
    }
}

#[cfg(feature = "anyrender")]
fn to_peniko(color: Rgba) -> peniko::Color {
    peniko::Color::from_rgba8(
        (color.r * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.g * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.b * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.a * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

#[cfg(feature = "anyrender")]
fn compute_path_fit_transform(
    fill_paths: &[String],
    stroke_paths: &[String],
    target_width: f64,
    target_height: f64,
) -> kurbo::Affine {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = false;

    for path_data in fill_paths.iter().chain(stroke_paths.iter()) {
        let Ok(path) = kurbo::BezPath::from_svg(path_data) else {
            continue;
        };
        let bb = path.bounding_box();
        if !bb.x0.is_finite() || !bb.y0.is_finite() || !bb.x1.is_finite() || !bb.y1.is_finite() {
            continue;
        }
        min_x = min_x.min(bb.x0);
        min_y = min_y.min(bb.y0);
        max_x = max_x.max(bb.x1);
        max_y = max_y.max(bb.y1);
        found = true;
    }

    if !found {
        return kurbo::Affine::IDENTITY;
    }

    let src_w = (max_x - min_x).max(0.0001);
    let src_h = (max_y - min_y).max(0.0001);
    let effective_w = if target_width > 0.0 {
        target_width
    } else {
        src_w
    };
    let effective_h = if target_height > 0.0 {
        target_height
    } else {
        src_h
    };
    let sx = effective_w / src_w;
    let sy = effective_h / src_h;

    kurbo::Affine::scale_non_uniform(sx, sy) * kurbo::Affine::translate((-min_x, -min_y))
}

#[cfg(feature = "anyrender")]
fn compute_svg_fit_transform(
    svg_text: &str,
    target_width: f64,
    target_height: f64,
) -> kurbo::Affine {
    let opt = anyrender_svg::usvg::Options::default();
    let Ok(tree) = anyrender_svg::usvg::Tree::from_str(svg_text, &opt) else {
        return kurbo::Affine::IDENTITY;
    };
    let root_bounds = tree.root().abs_layer_bounding_box();
    let src_x = f64::from(root_bounds.x());
    let src_y = f64::from(root_bounds.y());
    let src_w = f64::from(root_bounds.width()).max(0.0001);
    let src_h = f64::from(root_bounds.height()).max(0.0001);
    let effective_w = if target_width > 0.0 {
        target_width
    } else {
        src_w
    };
    let effective_h = if target_height > 0.0 {
        target_height
    } else {
        src_h
    };
    let sx = effective_w / src_w;
    let sy = effective_h / src_h;
    // Preserve aspect ratio to avoid SVG skew when target box differs slightly
    // from intrinsic export bounds. Center within the destination box.
    let s = sx.min(sy);
    let fitted_w = src_w * s;
    let fitted_h = src_h * s;
    let offset_x = (effective_w - fitted_w) * 0.5;
    let offset_y = (effective_h - fitted_h) * 0.5;
    kurbo::Affine::translate((offset_x, offset_y))
        * kurbo::Affine::scale(s)
        * kurbo::Affine::translate((-src_x, -src_y))
}

#[cfg(feature = "anyrender")]
fn extract_scale(transform: kurbo::Affine) -> (f64, f64) {
    let c = transform.as_coeffs();
    // Approximate axis scales from affine matrix columns.
    let sx = (c[0] * c[0] + c[1] * c[1]).sqrt();
    let sy = (c[2] * c[2] + c[3] * c[3]).sqrt();
    (sx, sy)
}
