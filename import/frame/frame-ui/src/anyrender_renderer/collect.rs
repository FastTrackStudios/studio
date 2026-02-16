use super::extract::*;
use super::layout::build_layout_boxes;
use super::types::*;
use frame_proto::{FrameDocument, NodeId, RenderNodeClass};

pub fn collect_render_diagnostics(doc: &FrameDocument, root: NodeId) -> RenderDiagnostics {
    let mut out = RenderDiagnostics::default();
    for node_id in doc.walk_subtree(root) {
        let Some(p) = doc.project_node(node_id) else {
            continue;
        };
        out.total_nodes += 1;
        if p.feature_flags.has_non_normal_blend {
            out.non_normal_blend_nodes += 1;
        }
        if p.feature_flags.has_gradient_fill {
            out.gradient_fill_nodes += 1;
        }
        if p.feature_flags.has_image_fill {
            out.image_fill_nodes += 1;
        }
        if p.feature_flags.has_drop_shadow {
            out.drop_shadow_nodes += 1;
        }
        if p.feature_flags.has_inner_shadow {
            out.inner_shadow_nodes += 1;
        }
        if p.feature_flags.has_layer_blur {
            out.layer_blur_nodes += 1;
            if is_layer_blur_approximated(&p) {
                out.layer_blur_approx_nodes += 1;
            }
        }
        if p.feature_flags.has_background_blur {
            out.background_blur_nodes += 1;
            out.background_blur_approx_nodes += 1;
        }
        if p.feature_flags.has_alpha_mask {
            out.alpha_mask_nodes += 1;
        }
        if p.feature_flags.has_luminance_mask {
            out.luminance_mask_nodes += 1;
        }
    }
    out
}

fn is_layer_blur_approximated(p: &frame_proto::RenderNodeProjection) -> bool {
    matches!(
        p.class,
        frame_proto::RenderNodeClass::Text
            | frame_proto::RenderNodeClass::Utility
            | frame_proto::RenderNodeClass::Unknown
    )
}

pub fn build_paint_primitives(doc: &FrameDocument, root: NodeId) -> Vec<PaintPrimitive> {
    let layout_boxes = build_layout_boxes(doc, root);
    let mut out = Vec::new();
    collect_primitives_recursive(
        doc,
        root,
        0.0,
        0.0,
        false,
        1.0,
        &[],
        &layout_boxes,
        &mut out,
    );

    out
}

fn collect_primitives_recursive(
    doc: &FrameDocument,
    node_id: NodeId,
    parent_x: f64,
    parent_y: f64,
    force_local_positioning: bool,
    inherited_opacity: f64,
    inherited_layer_blurs: &[NodeEffect],
    layout_boxes: &std::collections::HashMap<NodeId, LayoutBox>,
    out: &mut Vec<PaintPrimitive>,
) {
    let Some(node) = doc.get_node(node_id) else {
        return;
    };
    if !node.visible {
        return;
    }

    let local_opacity = node
        .raw
        .get("opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let effective_opacity = (inherited_opacity * local_opacity).clamp(0.0, 1.0);

    // Always extract rotation from relativeTransform, even when position/size
    // comes from layout boxes. Layout boxes flatten position but don't encode rotation.
    let (_lx, _ly, _lw, _lh, rotation) = extract_local_rect(&node.raw);
    let (x, y, width, height) = if !force_local_positioning {
        layout_boxes
            .get(&node_id)
            .map(|b| (b.x, b.y, b.width, b.height))
            .unwrap_or_else(|| {
                let (local_x, local_y, width, height, _rot) = extract_local_rect(&node.raw);
                (parent_x + local_x, parent_y + local_y, width, height)
            })
    } else {
        let (local_x, local_y, width, height, _rot) = extract_local_rect(&node.raw);
        (parent_x + local_x, parent_y + local_y, width, height)
    };

    let fills: Vec<(FillPaint, BlendMix)> = all_fill_paints(&node.raw, "fills")
        .into_iter()
        .map(|(f, b)| (f.with_opacity(effective_opacity), b))
        .collect();
    let stroke = parse_stroke_style(&node.raw, effective_opacity);
    let corner_radii = {
        if let Some(arr) = node
            .raw
            .get("rectangleCornerRadii")
            .and_then(|v| v.as_array())
        {
            if arr.len() >= 4 {
                frame_proto::CornerRadii {
                    top_left: arr[0].as_f64().unwrap_or(0.0).max(0.0),
                    top_right: arr[1].as_f64().unwrap_or(0.0).max(0.0),
                    bottom_right: arr[2].as_f64().unwrap_or(0.0).max(0.0),
                    bottom_left: arr[3].as_f64().unwrap_or(0.0).max(0.0),
                }
            } else {
                frame_proto::CornerRadii::uniform(
                    node.raw
                        .get("cornerRadius")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                        .max(0.0),
                )
            }
        } else {
            frame_proto::CornerRadii::uniform(
                node.raw
                    .get("cornerRadius")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0),
            )
        }
    };
    let effects = parse_effects(&node.raw, effective_opacity);
    let mut merged_effects = inherited_layer_blurs.to_vec();
    merged_effects.extend(effects.clone());
    let mut child_layer_blurs = inherited_layer_blurs.to_vec();
    child_layer_blurs.extend(
        effects
            .iter()
            .filter(|e| matches!(e.kind, EffectKind::LayerBlur))
            .cloned(),
    );
    let blend = parse_blend_mode(&node.raw);
    let needs_layer =
        (blend != BlendMix::Normal || effective_opacity < 0.9999) && width > 0.0 && height > 0.0;
    if needs_layer {
        out.push(PaintPrimitive::LayerStart {
            node_id,
            x,
            y,
            width,
            height,
            blend: blend.clone(),
            opacity: effective_opacity,
        });
    }

    let fill_paths = path_strings(&node.raw, "fillGeometry");
    let stroke_paths = path_strings(&node.raw, "strokeGeometry");
    let svg_base64 = exported_svg_base64(&node.raw);
    let has_vector_geometry =
        !fill_paths.is_empty() || !stroke_paths.is_empty() || svg_base64.is_some();
    let is_instance = node.figma_type == "INSTANCE";
    let is_boolean_operation = node.figma_type == "BOOLEAN_OPERATION";
    let is_mask = node
        .raw
        .get("isMask")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mask_type = node
        .raw
        .get("maskType")
        .and_then(|v| v.as_str())
        .unwrap_or("ALPHA");

    let mut rendered_as_atomic_svg = false;

    let node_class = frame_proto::classify_node_type(&node.figma_type);
    match node_class {
        RenderNodeClass::Text if has_vector_geometry => {
            // Text with pre-rendered glyph outlines in fillGeometry — render as vector paths.
            // Bridge exports don't include font metadata so glyph-by-glyph layout fails;
            // the fillGeometry paths are the authoritative visual representation.
            out.push(PaintPrimitive::Path {
                node_id,
                x,
                y,
                width,
                height,
                fill_paths,
                stroke_paths,
                svg_base64,
                fills,
                stroke,
                effects: merged_effects.clone(),
                blend,
                rotation,
            });
        }
        RenderNodeClass::Text => {
            let text = node
                .raw
                .get("characters")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if !text.is_empty() {
                let color = fills
                    .first()
                    .and_then(|(f, _)| first_color_from_fill(f))
                    .unwrap_or(Rgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: effective_opacity,
                    });
                // v2 plugin format: fontSize at top level
                // v1 REST API format: style.fontSize
                let font_size = node
                    .raw
                    .get("fontSize")
                    .and_then(|v| v.as_f64())
                    .or_else(|| {
                        node.raw
                            .get("style")
                            .and_then(|v| v.get("fontSize"))
                            .and_then(|v| v.as_f64())
                    })
                    .unwrap_or(14.0)
                    .max(1.0);
                let line_height = node
                    .raw
                    .get("lineHeightPx")
                    .and_then(|v| v.as_f64())
                    .or_else(|| {
                        node.raw
                            .get("style")
                            .and_then(|v| v.get("lineHeightPx"))
                            .and_then(|v| v.as_f64())
                    })
                    .filter(|v| *v > 0.0);
                let letter_spacing = node
                    .raw
                    .get("letterSpacingPx")
                    .and_then(|v| v.as_f64())
                    .or_else(|| {
                        node.raw
                            .get("style")
                            .and_then(|v| v.get("letterSpacing"))
                            .and_then(|v| v.as_f64())
                    })
                    .unwrap_or(0.0);
                // v2: textCase at top level; v1: style.textCase
                let text_case = match node
                    .raw
                    .get("textCase")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        node.raw
                            .get("style")
                            .and_then(|v| v.get("textCase"))
                            .and_then(|v| v.as_str())
                    })
                {
                    Some("UPPER") => TextCase::Upper,
                    Some("LOWER") => TextCase::Lower,
                    Some("TITLE") => TextCase::Title,
                    Some("SMALL_CAPS") => TextCase::SmallCaps,
                    Some("SMALL_CAPS_FORCED") => TextCase::SmallCapsForced,
                    _ => TextCase::Original,
                };

                out.push(PaintPrimitive::Text {
                    node_id,
                    x,
                    y,
                    text,
                    color,
                    font_size,
                    line_height,
                    letter_spacing,
                    text_case,
                    blend,
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
                fills,
                stroke,
                effects: merged_effects.clone(),
                blend,
                rotation,
            });
        }
        _ if is_boolean_operation && svg_base64.is_some() => {
            // Boolean operation fidelity is most accurate from Figma-exported SVG.
            // Rendering children directly can duplicate paths and lose operation semantics.
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
                fills,
                stroke,
                effects: merged_effects.clone(),
                blend,
                rotation,
            });
        }
        RenderNodeClass::Vector | RenderNodeClass::Shape if has_vector_geometry => {
            out.push(PaintPrimitive::Path {
                node_id,
                x,
                y,
                width,
                height,
                fill_paths,
                stroke_paths,
                svg_base64,
                fills,
                stroke,
                effects: merged_effects.clone(),
                blend,
                rotation,
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
                    fills,
                    stroke,
                    corner_radii,
                    effects: merged_effects.clone(),
                    blend,
                    rotation,
                });
            }
        }
    }

    if rendered_as_atomic_svg {
        if needs_layer {
            out.push(PaintPrimitive::LayerEnd { node_id });
        }
        return;
    }

    let clips_content = node
        .raw
        .get("clipsContent")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let clip_subtree = clips_content || (is_mask && !mask_type.eq_ignore_ascii_case("LUMINANCE"));
    if clip_subtree && width > 0.0 && height > 0.0 {
        out.push(PaintPrimitive::ClipStart {
            node_id,
            x,
            y,
            width,
            height,
            corner_radii,
        });
    }

    // Figma masks affect subsequent siblings within the same parent scope.
    // We model this with clip layers driven by `isMask` children.
    let mut active_sibling_mask: Option<NodeId> = None;
    for child_id in &node.children {
        let Some(child) = doc.get_node(*child_id) else {
            continue;
        };
        let child_is_mask = child
            .raw
            .get("isMask")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let child_mask_type = child
            .raw
            .get("maskType")
            .and_then(|v| v.as_str())
            .unwrap_or("ALPHA");
        let child_mask_supported = !child_mask_type.eq_ignore_ascii_case("LUMINANCE");

        if child_is_mask && child_mask_supported {
            if let Some(mask_node_id) = active_sibling_mask.take() {
                out.push(PaintPrimitive::ClipEnd {
                    node_id: mask_node_id,
                });
            }

            let (mx, my, mw, mh) = layout_boxes
                .get(child_id)
                .map(|b| (b.x, b.y, b.width, b.height))
                .unwrap_or_else(|| {
                    let (local_x, local_y, width, height, _rot) = extract_local_rect(&child.raw);
                    (x + local_x, y + local_y, width, height)
                });
            let mcorner_radii = {
                if let Some(arr) = child
                    .raw
                    .get("rectangleCornerRadii")
                    .and_then(|v| v.as_array())
                {
                    if arr.len() >= 4 {
                        frame_proto::CornerRadii {
                            top_left: arr[0].as_f64().unwrap_or(0.0).max(0.0),
                            top_right: arr[1].as_f64().unwrap_or(0.0).max(0.0),
                            bottom_right: arr[2].as_f64().unwrap_or(0.0).max(0.0),
                            bottom_left: arr[3].as_f64().unwrap_or(0.0).max(0.0),
                        }
                    } else {
                        frame_proto::CornerRadii::uniform(
                            child.raw.get("cornerRadius").and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0),
                        )
                    }
                } else {
                    frame_proto::CornerRadii::uniform(
                        child.raw.get("cornerRadius").and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0),
                    )
                }
            };
            if mw > 0.0 && mh > 0.0 {
                out.push(PaintPrimitive::ClipStart {
                    node_id: *child_id,
                    x: mx,
                    y: my,
                    width: mw,
                    height: mh,
                    corner_radii: mcorner_radii,
                });
                active_sibling_mask = Some(*child_id);
            }

            // Do not render mask geometry itself by default.
            continue;
        }

        collect_primitives_recursive(
            doc,
            *child_id,
            x,
            y,
            false,
            effective_opacity,
            &child_layer_blurs,
            layout_boxes,
            out,
        );
    }

    if let Some(mask_node_id) = active_sibling_mask.take() {
        out.push(PaintPrimitive::ClipEnd {
            node_id: mask_node_id,
        });
    }

    // Bridge JSON often encodes INSTANCEs without child trees. Resolve from
    // component master definitions so we still render the actual visuals.
    let has_children = !node.children.is_empty();
    let has_svg_export = exported_svg_base64(&node.raw).is_some();
    if is_instance && !has_children && !has_svg_export {
        if let Some(component_id) = node.raw.get("componentId").and_then(|v| v.as_str()) {
            if let Some(component_node) = doc.get_by_figma_id(component_id) {
                for child_id in &component_node.children {
                    collect_primitives_recursive(
                        doc,
                        *child_id,
                        x,
                        y,
                        true,
                        effective_opacity,
                        &child_layer_blurs,
                        layout_boxes,
                        out,
                    );
                }
            }
        }
    }

    if clip_subtree && width > 0.0 && height > 0.0 {
        out.push(PaintPrimitive::ClipEnd { node_id });
    }
    if needs_layer {
        out.push(PaintPrimitive::LayerEnd { node_id });
    }
}
