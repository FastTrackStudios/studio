use super::extract::extract_local_rect;
use super::types::LayoutBox;
use frame_proto::{FrameDocument, NodeId};
use taffy::prelude::*;
use taffy::TaffyTree;

pub fn build_layout_boxes(
    doc: &FrameDocument,
    root: NodeId,
) -> std::collections::HashMap<NodeId, LayoutBox> {
    compute_layout_boxes(doc, root)
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
    let children = node.children.clone();
    if children.is_empty() {
        return;
    }

    let auto_layout = frame_proto::get_auto_layout_projection(&node.raw);
    let is_auto_layout = auto_layout
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

    let container_style = container_taffy_style(auto_layout.as_ref(), width, height);
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
        .map(|n| {
            let (x, y, w, h, _rotation) = extract_local_rect(&n.raw);
            (x, y, w, h)
        })
        .unwrap_or((0.0, 0.0, 0.0, 0.0))
}

fn container_taffy_style(
    auto_layout: Option<&frame_proto::AutoLayout>,
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

    if let Some(layout) = auto_layout {
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
    let auto_layout = frame_proto::get_auto_layout_projection(&node.raw);
    let (x, y, width, height, _rotation) = extract_local_rect(&node.raw);
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
                auto_layout
                    .as_ref()
                    .and_then(|l| l.min_width)
                    .unwrap_or(0.0) as f32,
            ),
            height: Dimension::Length(
                auto_layout
                    .as_ref()
                    .and_then(|l| l.min_height)
                    .unwrap_or(0.0) as f32,
            ),
        },
        max_size: Size {
            width: auto_layout
                .as_ref()
                .and_then(|l| l.max_width)
                .map(|v| Dimension::Length(v as f32))
                .unwrap_or(Dimension::Auto),
            height: auto_layout
                .as_ref()
                .and_then(|l| l.max_height)
                .map(|v| Dimension::Length(v as f32))
                .unwrap_or(Dimension::Auto),
        },
        ..Default::default()
    };

    if let Some(layout) = auto_layout.as_ref() {
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
