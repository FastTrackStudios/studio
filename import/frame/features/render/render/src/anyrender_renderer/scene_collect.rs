//! `document_proto::Scene` → paint primitives.
//!
//! The render semantics live here (opacity inheritance, compositing layers,
//! clip decisions, sibling masks, instance fallback); the scene only carries
//! authored values.

use std::collections::HashMap;

use base64::Engine as _;
use document_proto::{self as doc, NodeId, NodeKind, Scene};
use taffy::prelude::*;
use taffy::TaffyTree;

use super::transform::AffineTransform2D;
use super::types::*;

// ── value conversion ─────────────────────────────────────────────────────

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// A paint color with the paint's opacity folded into alpha.
fn rgba(c: &doc::Color, paint_opacity: f64) -> Rgba {
    Rgba { r: clamp01(c.r), g: clamp01(c.g), b: clamp01(c.b), a: clamp01(clamp01(c.a) * clamp01(paint_opacity)) }
}

fn affine(t: &doc::Transform) -> AffineTransform2D {
    AffineTransform2D::from([[t.a, t.b, t.tx], [t.c, t.d, t.ty]])
}

pub(super) fn blend_mix(b: doc::BlendMode) -> BlendMix {
    use doc::BlendMode as B;
    match b {
        B::Multiply => BlendMix::Multiply,
        B::Screen => BlendMix::Screen,
        B::Overlay => BlendMix::Overlay,
        B::Darken => BlendMix::Darken,
        B::Lighten => BlendMix::Lighten,
        B::ColorDodge => BlendMix::ColorDodge,
        B::ColorBurn => BlendMix::ColorBurn,
        B::HardLight => BlendMix::HardLight,
        B::SoftLight => BlendMix::SoftLight,
        B::Difference => BlendMix::Difference,
        B::Exclusion => BlendMix::Exclusion,
        B::Hue => BlendMix::Hue,
        B::Saturation => BlendMix::Saturation,
        B::Color => BlendMix::Color,
        B::Luminosity => BlendMix::Luminosity,
        B::Normal | B::PassThrough | B::LinearBurn | B::LinearDodge => BlendMix::Normal,
    }
}

fn stops(stops: &[doc::GradientStop], paint_opacity: f64) -> Vec<GradientStop> {
    stops.iter().map(|s| GradientStop { offset: clamp01(s.position), color: rgba(&s.color, paint_opacity) }).collect()
}

fn fill_paint(p: &doc::Paint, scene: &Scene) -> Option<FillPaint> {
    Some(match &p.kind {
        doc::PaintKind::Solid { color } => FillPaint::Solid(rgba(color, p.opacity)),
        doc::PaintKind::LinearGradient { stops: s, transform } => {
            FillPaint::GradientLinear { transform: transform.as_ref().map(affine), stops: stops(s, p.opacity) }
        }
        doc::PaintKind::RadialGradient { stops: s, transform } => {
            FillPaint::GradientRadial { transform: transform.as_ref().map(affine), stops: stops(s, p.opacity) }
        }
        doc::PaintKind::AngularGradient { stops: s, transform } => {
            FillPaint::GradientAngular { transform: transform.as_ref().map(affine), stops: stops(s, p.opacity) }
        }
        doc::PaintKind::Image { asset, source_ref, scale_mode, transform } => FillPaint::Image {
            image_hash: source_ref.clone(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(scene.asset(asset).map(|a| a.bytes.as_slice()).unwrap_or_default()),
            alpha: clamp01(p.opacity),
            scale_mode: match scale_mode {
                doc::ImageScaleMode::Fill => ImageScaleMode::Fill,
                doc::ImageScaleMode::Fit => ImageScaleMode::Fit,
                doc::ImageScaleMode::Crop => ImageScaleMode::Crop,
                doc::ImageScaleMode::Tile => ImageScaleMode::Tile,
                doc::ImageScaleMode::Stretch => ImageScaleMode::Stretch,
            },
            image_transform: transform.as_ref().map(affine),
        },
    })
}

/// Visible paints, in stack order.
pub(super) fn fill_paints(paints: &[doc::Paint], scene: &Scene) -> Vec<(FillPaint, BlendMix)> {
    paints.iter().filter(|p| p.visible).filter_map(|p| fill_paint(p, scene).map(|f| (f, blend_mix(p.blend)))).collect()
}

fn first_color(fill: &FillPaint) -> Option<Rgba> {
    match fill {
        FillPaint::Solid(c) => Some(c.clone()),
        FillPaint::GradientLinear { stops, .. }
        | FillPaint::GradientRadial { stops, .. }
        | FillPaint::GradientAngular { stops, .. } => stops.first().map(|s| s.color.clone()),
        FillPaint::Image { .. } => None,
    }
}

fn stroke_cap(c: doc::StrokeCap) -> StrokeCap {
    use doc::StrokeCap as D;
    match c {
        D::None => StrokeCap::None,
        D::Round => StrokeCap::Round,
        D::Square => StrokeCap::Square,
        D::LineArrow => StrokeCap::LineArrow,
        D::TriangleArrow => StrokeCap::TriangleArrow,
        D::DiamondFilled => StrokeCap::DiamondFilled,
        D::CircleFilled => StrokeCap::CircleFilled,
        D::TriangleFilled => StrokeCap::TriangleFilled,
        D::WashiTape1 => StrokeCap::WashiTape1,
        D::WashiTape2 => StrokeCap::WashiTape2,
        D::WashiTape3 => StrokeCap::WashiTape3,
        D::WashiTape4 => StrokeCap::WashiTape4,
        D::WashiTape5 => StrokeCap::WashiTape5,
        D::WashiTape6 => StrokeCap::WashiTape6,
    }
}

/// The drawn stroke: the first visible solid paint, when the weight is positive.
pub(super) fn stroke_style(s: &doc::Stroke, opacity: f64) -> Option<StrokeStyle> {
    let color = s.paints.iter().filter(|p| p.visible).find_map(|p| match &p.kind {
        doc::PaintKind::Solid { color } => Some(rgba(color, p.opacity)),
        _ => None,
    })?;
    let width = s.weight.max(0.0);
    if width <= 0.0 {
        return None;
    }
    Some(StrokeStyle {
        color: color.with_opacity(opacity),
        width,
        cap: stroke_cap(s.cap),
        join: match s.join {
            doc::StrokeJoin::Miter => StrokeJoin::Miter,
            doc::StrokeJoin::Bevel => StrokeJoin::Bevel,
            doc::StrokeJoin::Round => StrokeJoin::Round,
        },
        align: match s.align {
            doc::StrokeAlign::Inside => StrokeAlign::Inside,
            doc::StrokeAlign::Outside => StrokeAlign::Outside,
            doc::StrokeAlign::Center => StrokeAlign::Center,
        },
        miter_limit: s.miter_limit.max(1.0),
        dash_pattern: s.dash_pattern.iter().copied().filter(|v| *v > 0.0).collect(),
        dash_offset: s.dash_offset,
    })
}

pub(super) fn node_effects(effects: &[doc::Effect], opacity: f64) -> Vec<NodeEffect> {
    effects
        .iter()
        .filter(|e| e.visible)
        .map(|e| NodeEffect {
            kind: match e.kind {
                doc::EffectKind::DropShadow => EffectKind::DropShadow,
                doc::EffectKind::InnerShadow => EffectKind::InnerShadow,
                doc::EffectKind::LayerBlur => EffectKind::LayerBlur,
                doc::EffectKind::BackgroundBlur => EffectKind::BackgroundBlur,
            },
            color: Rgba { r: e.color.r, g: e.color.g, b: e.color.b, a: e.color.a }.with_opacity(opacity),
            offset_x: e.offset_x,
            offset_y: e.offset_y,
            radius: e.radius.max(0.0),
            spread: e.spread,
        })
        .collect()
}

pub(super) fn corner_radii(r: &doc::CornerRadii) -> figma_model::CornerRadii {
    figma_model::CornerRadii {
        top_left: r.top_left,
        top_right: r.top_right,
        bottom_right: r.bottom_right,
        bottom_left: r.bottom_left,
    }
}

fn text_case(c: doc::TextCase) -> TextCase {
    match c {
        doc::TextCase::Original => TextCase::Original,
        doc::TextCase::Upper => TextCase::Upper,
        doc::TextCase::Lower => TextCase::Lower,
        doc::TextCase::Title => TextCase::Title,
        doc::TextCase::SmallCaps => TextCase::SmallCaps,
        doc::TextCase::SmallCapsForced => TextCase::SmallCapsForced,
    }
}

fn text_style_runs(text: &doc::TextData, opacity: f64) -> Vec<TextStyleRun> {
    text.runs
        .iter()
        .filter(|r| !r.text.is_empty())
        .map(|r| TextStyleRun {
            text: r.text.clone(),
            // The first solid paint of the run, visible or not (matches the
            // exporter's own run color).
            color: r.fills.iter().find_map(|p| match &p.kind {
                doc::PaintKind::Solid { color } => Some(rgba(color, p.opacity).with_opacity(opacity)),
                _ => None,
            }),
            font_size: r.font_size,
            letter_spacing: r.letter_spacing,
            font_style: r.font_style.clone(),
            font_weight: r.font_weight,
        })
        .collect()
}

/// One fill per visibly-distinct run, for painting text outline glyph groups.
fn text_outline_fills(text: &doc::TextData, scene: &Scene, opacity: f64) -> Vec<(FillPaint, BlendMix)> {
    let mut runs: Vec<(FillPaint, BlendMix)> = Vec::new();
    for run in &text.runs {
        if run.text.trim().is_empty() {
            continue;
        }
        let Some((fill, blend)) = fill_paints(&run.fills, scene).into_iter().map(|(f, b)| (f.with_opacity(opacity), b)).next()
        else {
            continue;
        };
        if runs.last().is_none_or(|(lf, lb)| *lb != blend || *lf != fill) {
            runs.push((fill, blend));
        }
    }
    runs
}

fn snapshot_base64(node: &doc::Node, scene: &Scene) -> Option<String> {
    let asset = scene.asset(node.svg_snapshot.as_ref()?)?;
    Some(base64::engine::general_purpose::STANDARD.encode(&asset.bytes))
}

enum Class {
    Text,
    Shape,
    Other,
}

fn class(kind: &NodeKind) -> Class {
    match kind {
        NodeKind::Text(_) => Class::Text,
        NodeKind::Rectangle
        | NodeKind::Ellipse
        | NodeKind::Line
        | NodeKind::Polygon
        | NodeKind::Star
        | NodeKind::Vector
        | NodeKind::BooleanOperation { .. } => Class::Shape,
        _ => Class::Other,
    }
}

fn local_rect(node: &doc::Node) -> (f64, f64, f64, f64) {
    (node.transform.tx, node.transform.ty, node.size.width, node.size.height)
}

// ── layout ───────────────────────────────────────────────────────────────

/// Absolute boxes for `root` and its subtree, running auto layout.
#[must_use]
pub fn build_scene_layout_boxes(scene: &Scene, root: NodeId) -> HashMap<NodeId, LayoutBox> {
    let mut out = HashMap::new();
    let (x, y, w, h) = scene.node(root).map_or((0.0, 0.0, 0.0, 0.0), local_rect);
    layout_recursive(scene, root, x, y, w, h, &mut out);
    out
}

fn layout_recursive(scene: &Scene, id: NodeId, ax: f64, ay: f64, w: f64, h: f64, out: &mut HashMap<NodeId, LayoutBox>) {
    out.insert(id, LayoutBox { x: ax, y: ay, width: w, height: h });
    let Some(node) = scene.node(id) else { return };
    if node.children.is_empty() {
        return;
    }
    let free = |out: &mut HashMap<NodeId, LayoutBox>| {
        for c in &node.children {
            let (cx, cy, cw, ch) = scene.node(*c).map_or((0.0, 0.0, 0.0, 0.0), local_rect);
            layout_recursive(scene, *c, ax + cx, ay + cy, cw, ch, out);
        }
    };
    let Some(al) = &node.layout else {
        free(out);
        return;
    };

    let mut taffy = TaffyTree::<()>::new();
    let mut kids = Vec::new();
    for c in &node.children {
        if let Ok(t) = taffy.new_leaf(child_style(scene, *c)) {
            kids.push((*c, t));
        }
    }
    let ids: Vec<_> = kids.iter().map(|(_, t)| *t).collect();
    let Ok(container) = taffy.new_with_children(container_style(al, w, h), &ids) else {
        free(out);
        return;
    };
    let _ = taffy.compute_layout(
        container,
        taffy::Size { width: AvailableSpace::Definite(w as f32), height: AvailableSpace::Definite(h as f32) },
    );
    for (c, t) in kids {
        if let Ok(l) = taffy.layout(t) {
            layout_recursive(
                scene,
                c,
                ax + f64::from(l.location.x),
                ay + f64::from(l.location.y),
                f64::from(l.size.width),
                f64::from(l.size.height),
                out,
            );
        } else {
            let (cx, cy, cw, ch) = scene.node(c).map_or((0.0, 0.0, 0.0, 0.0), local_rect);
            layout_recursive(scene, c, ax + cx, ay + cy, cw, ch, out);
        }
    }
}

fn container_style(al: &doc::AutoLayout, w: f64, h: f64) -> Style {
    let row = matches!(al.direction, doc::LayoutDirection::Horizontal);
    Style {
        display: Display::Flex,
        size: taffy::Size { width: Dimension::Length(w as f32), height: Dimension::Length(h as f32) },
        flex_direction: if row { FlexDirection::Row } else { FlexDirection::Column },
        gap: if row {
            taffy::Size { width: LengthPercentage::Length(al.gap as f32), height: LengthPercentage::Length(0.0) }
        } else {
            taffy::Size { width: LengthPercentage::Length(0.0), height: LengthPercentage::Length(al.gap as f32) }
        },
        padding: taffy::Rect {
            left: LengthPercentage::Length(al.padding_left as f32),
            right: LengthPercentage::Length(al.padding_right as f32),
            top: LengthPercentage::Length(al.padding_top as f32),
            bottom: LengthPercentage::Length(al.padding_bottom as f32),
        },
        flex_wrap: if al.wrap { FlexWrap::Wrap } else { FlexWrap::NoWrap },
        justify_content: al.main_align.map(|a| match a {
            doc::MainAxisAlign::Start => JustifyContent::Start,
            doc::MainAxisAlign::Center => JustifyContent::Center,
            doc::MainAxisAlign::End => JustifyContent::End,
            doc::MainAxisAlign::SpaceBetween => JustifyContent::SpaceBetween,
        }),
        align_items: al.cross_align.map(|a| match a {
            doc::CrossAxisAlign::Start => AlignItems::Start,
            doc::CrossAxisAlign::Center => AlignItems::Center,
            doc::CrossAxisAlign::End => AlignItems::End,
            doc::CrossAxisAlign::Baseline => AlignItems::Baseline,
        }),
        ..Default::default()
    }
}

fn child_style(scene: &Scene, id: NodeId) -> Style {
    let Some(node) = scene.node(id) else { return Style::default() };
    let (x, y, w, h) = local_rect(node);
    let lc = &node.layout_child;
    let mut style = Style {
        size: taffy::Size { width: Dimension::Length(w as f32), height: Dimension::Length(h as f32) },
        // Children keep their authored size; auto layout never shrinks them.
        flex_shrink: 0.0,
        min_size: taffy::Size {
            width: Dimension::Length(lc.min_width.unwrap_or(0.0) as f32),
            height: Dimension::Length(lc.min_height.unwrap_or(0.0) as f32),
        },
        max_size: taffy::Size {
            width: lc.max_width.map_or(Dimension::Auto, |v| Dimension::Length(v as f32)),
            height: lc.max_height.map_or(Dimension::Auto, |v| Dimension::Length(v as f32)),
        },
        align_self: lc.align_self.map(|a| match a {
            doc::AlignSelf::Start => AlignSelf::Start,
            doc::AlignSelf::Center => AlignSelf::Center,
            doc::AlignSelf::End => AlignSelf::End,
            doc::AlignSelf::Stretch => AlignSelf::Stretch,
        }),
        ..Default::default()
    };
    if let Some(g) = lc.grow {
        style.flex_grow = g as f32;
    }
    if lc.absolute {
        style.position = Position::Absolute;
        style.inset = taffy::Rect {
            left: LengthPercentageAuto::Length(x as f32),
            right: LengthPercentageAuto::Auto,
            top: LengthPercentageAuto::Length(y as f32),
            bottom: LengthPercentageAuto::Auto,
        };
    }
    style
}

// ── primitives ───────────────────────────────────────────────────────────

/// The paint primitives for `root`'s subtree, in paint order.
#[must_use]
pub fn build_scene_primitives(scene: &Scene, root: NodeId) -> Vec<PaintPrimitive> {
    let boxes = build_scene_layout_boxes(scene, root);
    let mut out = Vec::new();
    let mut cx = Cx { scene, boxes: &boxes, out: &mut out };
    cx.node(root, 0.0, 0.0, false, 1.0, &[]);
    out
}

struct Cx<'a> {
    scene: &'a Scene,
    boxes: &'a HashMap<NodeId, LayoutBox>,
    out: &'a mut Vec<PaintPrimitive>,
}

impl Cx<'_> {
    fn child_box(&self, child: NodeId, px: f64, py: f64) -> (f64, f64, f64, f64) {
        self.boxes.get(&child).map_or_else(
            || {
                let (lx, ly, w, h) = self.scene.node(child).map_or((0.0, 0.0, 0.0, 0.0), local_rect);
                (px + lx, py + ly, w, h)
            },
            |b| (b.x, b.y, b.width, b.height),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn node(
        &mut self,
        node_id: NodeId,
        parent_x: f64,
        parent_y: f64,
        force_local: bool,
        inherited_opacity: f64,
        inherited_blurs: &[NodeEffect],
    ) {
        let scene = self.scene;
        let Some(node) = scene.node(node_id) else { return };
        if !node.visible {
            return;
        }
        let effective_opacity = clamp01(inherited_opacity * clamp01(node.opacity));
        let rotation = node.transform.rotation();
        let (lx, ly, lw, lh) = local_rect(node);
        let (chain_x, chain_y) = (parent_x + lx, parent_y + ly);
        let (x, y, width, height) = if force_local {
            (chain_x, chain_y, lw, lh)
        } else {
            self.boxes.get(&node_id).map_or((chain_x, chain_y, lw, lh), |b| (b.x, b.y, b.width, b.height))
        };

        let mut radii = corner_radii(&node.corner_radii);
        if matches!(node.kind, NodeKind::Ellipse) && radii.is_zero() {
            radii = figma_model::CornerRadii::uniform(width.min(height) * 0.5);
        }
        let blend = blend_mix(node.blend);
        let has_filter_effects = node.effects.iter().any(|e| {
            e.visible
                && matches!(e.kind, doc::EffectKind::DropShadow | doc::EffectKind::InnerShadow | doc::EffectKind::LayerBlur)
        });
        // Non-normal blends, and filtered nodes with partial opacity, composite
        // through an explicit layer that carries the opacity.
        let needs_layer =
            (blend != BlendMix::Normal || (has_filter_effects && effective_opacity < 1.0)) && width > 0.0 && height > 0.0;
        let content_opacity = if needs_layer { 1.0 } else { effective_opacity };
        let primitive_blend = if needs_layer { BlendMix::Normal } else { blend.clone() };

        let fills: Vec<(FillPaint, BlendMix)> =
            fill_paints(&node.fills, scene).into_iter().map(|(f, b)| (f.with_opacity(content_opacity), b)).collect();
        let stroke = stroke_style(&node.stroke, content_opacity);
        let effects = node_effects(&node.effects, content_opacity);
        let mut merged_effects = inherited_blurs.to_vec();
        merged_effects.extend(effects.iter().cloned());
        let mut child_blurs = inherited_blurs.to_vec();
        child_blurs.extend(effects.iter().filter(|e| matches!(e.kind, EffectKind::LayerBlur)).cloned());

        if needs_layer {
            self.out.push(PaintPrimitive::LayerStart { node_id, x, y, width, height, blend: blend.clone(), opacity: effective_opacity });
        }

        let fill_paths = node.fill_geometry.clone();
        let stroke_paths = node.stroke_geometry.clone();
        let svg_base64 = snapshot_base64(node, scene);
        let has_native_geometry = !fill_paths.is_empty() || !stroke_paths.is_empty();
        let has_vector_geometry = has_native_geometry || svg_base64.is_some();
        let instance_component = match &node.kind {
            NodeKind::Instance { component } => Some(*component),
            _ => None,
        };
        let is_boolean = matches!(node.kind, NodeKind::BooleanOperation { .. });

        let child_overflows = node.clips_content
            && width > 0.0
            && height > 0.0
            && node.children.iter().any(|c| {
                const EPS: f64 = 1e-3;
                let (cx, cy, cw, ch) = self.child_box(*c, x, y);
                cx < x - EPS || cy < y - EPS || (cx + cw) > x + width + EPS || (cy + ch) > y + height + EPS
            });
        let clip_subtree = node.clips_content && !node.children.is_empty() && (fills.is_empty() || child_overflows);
        if clip_subtree && width > 0.0 && height > 0.0 {
            self.out.push(PaintPrimitive::ClipStart { node_id, x, y, width, height, corner_radii: radii.clone() });
        }

        let mut atomic_svg = false;
        let path = |x: f64, y: f64, normalize: bool, outline: bool, fp: Vec<String>, sp: Vec<String>, svg: Option<String>, fills: Vec<(FillPaint, BlendMix)>, stroke: Option<StrokeStyle>| PaintPrimitive::Path {
            node_id,
            x,
            y,
            width,
            height,
            normalize_to_origin: normalize,
            text_outline: outline,
            fill_paths: fp,
            stroke_paths: sp,
            svg_base64: svg,
            fills,
            stroke,
            effects: merged_effects.clone(),
            blend: primitive_blend.clone(),
            rotation,
        };

        match (class(&node.kind), &node.kind) {
            // Text with exported glyph outlines: paint the outlines (the
            // exporter's own shaping), at raw transform-chain coordinates.
            (Class::Text, NodeKind::Text(text)) if has_native_geometry => {
                let mut outline_fills = fills;
                if outline_fills.is_empty() {
                    outline_fills = text_outline_fills(text, scene, content_opacity);
                }
                if stroke_paths.is_empty() && !fill_paths.is_empty() && outline_fills.len() == fill_paths.len() {
                    for (d, fill) in fill_paths.into_iter().zip(outline_fills) {
                        self.out.push(path(chain_x, chain_y, true, true, vec![d], Vec::new(), None, vec![fill], stroke.clone()));
                    }
                } else {
                    self.out.push(path(chain_x, chain_y, true, true, fill_paths, stroke_paths, svg_base64, outline_fills, stroke));
                }
            }
            (Class::Text, NodeKind::Text(text)) => {
                if !text.characters.is_empty() {
                    let color = fills
                        .first()
                        .and_then(|(f, _)| first_color(f))
                        .unwrap_or(Rgba { r: 1.0, g: 1.0, b: 1.0, a: content_opacity });
                    self.out.push(PaintPrimitive::Text {
                        node_id,
                        x,
                        y,
                        text: text.characters.clone(),
                        color,
                        font_size: text.font_size.max(1.0),
                        line_height: text.line_height,
                        letter_spacing: text.letter_spacing,
                        text_case: text_case(text.text_case),
                        style_runs: text_style_runs(text, content_opacity),
                        blend: primitive_blend.clone(),
                    });
                }
            }
            // A detached instance (main component not in this scene) or a
            // boolean with only an exporter snapshot: draw the snapshot whole.
            _ if svg_base64.is_some()
                && !has_native_geometry
                && node.children.is_empty()
                && (instance_component == Some(None) || is_boolean) =>
            {
                atomic_svg = true;
                self.out.push(path(x, y, false, false, fill_paths, stroke_paths, svg_base64, fills, stroke));
            }
            _ if instance_component.is_some() => {}
            (Class::Shape, _) if has_vector_geometry => {
                self.out.push(path(x, y, false, false, fill_paths, stroke_paths, svg_base64, fills, stroke));
            }
            _ => {
                if width > 0.0 && height > 0.0 {
                    self.out.push(PaintPrimitive::Rect {
                        node_id,
                        x,
                        y,
                        width,
                        height,
                        fills,
                        stroke,
                        corner_radii: radii,
                        effects: merged_effects.clone(),
                        blend: primitive_blend,
                        rotation,
                    });
                }
            }
        }

        if !atomic_svg {
            self.children(node, x, y, content_opacity, &child_blurs);
            // An instance exported without its subtree: draw the main
            // component's children in its place.
            if node.children.is_empty()
                && let Some(Some(component)) = instance_component
                && let Some(main) = scene.node(component)
            {
                for c in &main.children {
                    self.node(*c, x, y, true, content_opacity, &child_blurs);
                }
            }
        }

        if clip_subtree && width > 0.0 && height > 0.0 {
            self.out.push(PaintPrimitive::ClipEnd { node_id });
        }
        if needs_layer {
            self.out.push(PaintPrimitive::LayerEnd { node_id });
        }
    }

    /// Children in order; a mask child masks the siblings after it.
    fn children(&mut self, node: &doc::Node, x: f64, y: f64, opacity: f64, blurs: &[NodeEffect]) {
        let mut active_mask: Option<NodeId> = None;
        for &child_id in &node.children {
            let Some(child) = self.scene.node(child_id) else { continue };
            let supported_mask = matches!(child.mask, Some(doc::MaskKind::Alpha | doc::MaskKind::Vector));
            if supported_mask {
                if let Some(m) = active_mask.take() {
                    self.out.push(PaintPrimitive::MaskApplyEnd { node_id: m });
                }
                let (mx, my, mw, mh) = self.child_box(child_id, x, y);
                if mw > 0.0 && mh > 0.0 {
                    self.out.push(PaintPrimitive::MaskDefStart { node_id: child_id, x: mx, y: my, width: mw, height: mh, luminance: false });
                    self.node(child_id, x, y, false, opacity, blurs);
                    self.out.push(PaintPrimitive::MaskDefEnd { node_id: child_id });
                    self.out.push(PaintPrimitive::MaskApplyStart { node_id: child_id, x: mx, y: my, width: mw, height: mh });
                    active_mask = Some(child_id);
                }
                continue;
            }
            self.node(child_id, x, y, false, opacity, blurs);
        }
        if let Some(m) = active_mask {
            self.out.push(PaintPrimitive::MaskApplyEnd { node_id: m });
        }
    }
}
