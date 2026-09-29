use super::cache::{cached_bez_path, cached_svg_decode, cached_svg_fit_transform, cached_svg_tree};
use super::scene_collect::first_color;
use super::geometry::{compute_path_fit_transform, extract_scale, to_peniko};
use super::types::*;

const TEXT_ANCHOR_SHIFT_X: f64 = 1.0;
const TEXT_ANCHOR_SHIFT_Y: f64 = 1.0;

/// Paint a frame document (`root`'s subtree) into a vello scene.
pub fn paint_scene_into(
    scene: &mut vello::Scene,
    doc: &document_proto::Scene,
    root: document_proto::NodeId,
    scene_transform: kurbo::Affine,
    text_font: Option<TextFontRef<'_>>,
) {
    let primitives = super::scene_collect::build_scene_primitives(doc, root);
    paint_primitives_into_scene_with(scene, &primitives, scene_transform, text_font);
}

pub fn paint_primitives_into_scene_with(
    scene: &mut vello::Scene,
    primitives: &[PaintPrimitive],
    root_transform: kurbo::Affine,
    text_font: Option<TextFontRef<'_>>,
) {
    use base64::Engine;
    use kurbo::{BezPath, Rect, RoundedRect, Stroke};
    use peniko::{Blob, Fill, FontData, ImageBrush, ImageData, ImageSampler};
    use skrifa::instance::Size;
    use skrifa::raw::{FileRef, FontRef};
    use skrifa::MetadataProvider;
    use std::sync::Arc;
    use vello::Glyph;

    let font_data: Option<FontData> = text_font.map(|font| {
        // Cache the Arc<[u8]> allocation to avoid copying font bytes every frame.
        // Key on (pointer, length, index) since the same slice is typically passed repeatedly.
        use std::sync::{LazyLock, Mutex};
        static FONT_CACHE: LazyLock<Mutex<Option<(usize, usize, u32, FontData)>>> =
            LazyLock::new(|| Mutex::new(None));

        let ptr = font.bytes.as_ptr() as usize;
        let len = font.bytes.len();
        if let Ok(cache) = FONT_CACHE.lock() {
            if let Some((cp, cl, ci, ref data)) = *cache {
                if cp == ptr && cl == len && ci == font.index {
                    return data.clone();
                }
            }
        }

        let data = FontData::new(Blob::new(Arc::new(font.bytes.to_vec())), font.index);
        if let Ok(mut cache) = FONT_CACHE.lock() {
            *cache = Some((ptr, len, font.index, data.clone()));
        }
        data
    });

    let font_ref: Option<FontRef<'_>> = font_data.as_ref().and_then(|f| {
        let file_ref = FileRef::new(f.data.as_ref()).ok()?;
        match file_ref {
            FileRef::Font(font) => Some(font),
            FileRef::Collection(collection) => collection.get(f.index).ok(),
        }
    });

    // Mask buffering state: collect primitives between MaskDefStart..MaskDefEnd
    // so they can be replayed as a luminance mask when MaskApplyEnd is reached.
    let mut capturing_mask = false;
    let mut mask_def_primitives: Vec<&PaintPrimitive> = Vec::new();
    let mut mask_bounds: Option<Rect> = None;
    // Inherited group transforms (`GroupStart`), outermost first; each entry
    // is already composed with the ones before it.
    let mut group_stack: Vec<kurbo::Affine> = Vec::new();

    for primitive in primitives {
        // If we're capturing mask definition primitives, buffer them instead of rendering
        if capturing_mask {
            match primitive {
                PaintPrimitive::MaskDefEnd { .. } => {
                    capturing_mask = false;
                }
                _ => {
                    mask_def_primitives.push(primitive);
                }
            }
            continue;
        }

        let scene_transform = group_stack.last().copied().unwrap_or(root_transform);
        match primitive {
            PaintPrimitive::GroupStart { transform, .. } => {
                group_stack.push(scene_transform * to_kurbo_affine(transform));
            }
            PaintPrimitive::GroupEnd { .. } => {
                group_stack.pop();
            }
            PaintPrimitive::LayerStart {
                x,
                y,
                width,
                height,
                blend,
                opacity,
                ..
            } => {
                let (x, y, width, height, opacity) = (*x, *y, *width, *height, *opacity);
                let clip_rect = Rect::new(x, y, x + width, y + height);
                scene.push_layer(
                    Fill::NonZero,
                    peniko::BlendMode {
                        mix: to_peniko_mix(&blend),
                        compose: peniko::Compose::SrcOver,
                    },
                    opacity as f32,
                    scene_transform,
                    &clip_rect,
                );
            }
            PaintPrimitive::LayerEnd { .. } => {
                scene.pop_layer();
            }
            PaintPrimitive::ClipStart {
                x,
                y,
                width,
                height,
                corner_radii,
                ..
            } => {
                let (x, y, width, height) = (*x, *y, *width, *height);
                let rect = Rect::new(x, y, x + width, y + height);
                if !corner_radii.is_zero() {
                    let rr = RoundedRect::from_rect(
                        rect,
                        kurbo::RoundedRectRadii::new(
                            corner_radii.top_left,
                            corner_radii.top_right,
                            corner_radii.bottom_right,
                            corner_radii.bottom_left,
                        ),
                    );
                    scene.push_clip_layer(Fill::NonZero, scene_transform, &rr);
                } else {
                    scene.push_clip_layer(Fill::NonZero, scene_transform, &rect);
                }
            }
            PaintPrimitive::ClipEnd { .. } => {
                scene.pop_layer();
            }
            PaintPrimitive::MaskDefStart {
                x, y, width, height, ..
            } => {
                capturing_mask = true;
                mask_def_primitives.clear();
                mask_bounds = Some(Rect::new(*x, *y, *x + *width, *y + *height));
            }
            PaintPrimitive::MaskDefEnd { .. } => {
                // Handled above in the capturing_mask branch
            }
            PaintPrimitive::MaskApplyStart {
                x,
                y,
                width,
                height,
                ..
            } => {
                let rect = Rect::new(*x, *y, *x + *width, *y + *height);
                // Isolation layer — everything inside gets composited then masked
                scene.push_layer(Fill::NonZero, peniko::BlendMode::default(), 1.0, scene_transform, &rect);
            }
            PaintPrimitive::MaskApplyEnd { .. } => {
                // Apply the buffered mask primitives as a luminance mask
                let clip = mask_bounds.unwrap_or(Rect::new(0.0, 0.0, 1e6, 1e6));
                scene.push_luminance_mask_layer(Fill::NonZero, 1.0, scene_transform, &clip);
                // Replay mask definition primitives
                if !mask_def_primitives.is_empty() {
                    render_mask_primitives(
                        scene,
                        &mask_def_primitives,
                        scene_transform,
                        text_font,
                        &font_data,
                        font_ref.as_ref(),
                    );
                }
                scene.pop_layer(); // luminance mask
                scene.pop_layer(); // isolation
                mask_def_primitives.clear();
                mask_bounds = None;
            }
            PaintPrimitive::Rect {
                x,
                y,
                width,
                height,
                fills,
                stroke,
                corner_radii,
                effects,
                blend: _,
                rotation,
                ..
            } => {
                let (x, y, width, height, rotation) = (*x, *y, *width, *height, *rotation);
                let corner_radii = *corner_radii;
                let (node_transform, lx, ly) = if rotation.abs() > 1e-6 {
                    let t = scene_transform
                        * kurbo::Affine::translate((x, y))
                        * kurbo::Affine::rotate(rotation);
                    (t, 0.0, 0.0)
                } else {
                    (scene_transform, x, y)
                };
                let rect = Rect::new(lx, ly, lx + width, ly + height);
                let uniform_corner = corner_radii.top_left;
                let blur_color_hint = fills
                    .first()
                    .and_then(|(f, _)| first_color(f))
                    .or_else(|| stroke.as_ref().map(|s| s.color.clone()));
                let has_layer_blur = effects.iter().any(|e| matches!(e.kind, EffectKind::LayerBlur));
                for effect in effects {
                    if matches!(effect.kind, EffectKind::LayerBlur) {
                        paint_blurred_rect_content(
                            scene,
                            node_transform,
                            lx,
                            ly,
                            width,
                            height,
                            uniform_corner,
                            fills.first().map(|(f, _)| f),
                            stroke.as_ref(),
                            effect,
                        );
                    }
                }
                for effect in effects {
                    if matches!(effect.kind, EffectKind::DropShadow | EffectKind::Glow) {
                        paint_effect(
                            scene,
                            node_transform,
                            &rect,
                            uniform_corner,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
                // When LayerBlur is active, the blurred rect IS the content —
                // don't draw sharp fills/strokes on top.
                if !has_layer_blur {
                if !corner_radii.is_zero() {
                    let radii = kurbo::RoundedRectRadii::new(
                        corner_radii.top_left,
                        corner_radii.top_right,
                        corner_radii.bottom_right,
                        corner_radii.bottom_left,
                    );
                    let stroke_rect = stroke_aligned_rect(rect, stroke.as_ref());
                    let stroke_radius =
                        stroke_aligned_corner_radius(uniform_corner, stroke.as_ref());
                    let rr = RoundedRect::from_rect(rect, radii);
                    for (fill, fill_blend) in fills {
                        let need_blend_layer = *fill_blend != BlendMix::Normal;
                        if need_blend_layer {
                            scene.push_layer(
                                Fill::NonZero,
                                peniko::BlendMode {
                                    mix: to_peniko_mix(fill_blend),
                                    compose: peniko::Compose::SrcOver,
                                },
                                1.0,
                                node_transform,
                                &rect,
                            );
                        }
                        fill_shape(scene, node_transform, &rr, lx, ly, width, height, fill);
                        if need_blend_layer {
                            scene.pop_layer();
                        }
                    }
                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            let stroke_style = to_kurbo_stroke(stroke, 1.0);
                            let stroke_rr = RoundedRect::from_rect(stroke_rect, stroke_radius);
                            scene.stroke(
                                &stroke_style,
                                node_transform,
                                to_peniko(stroke.color.clone()),
                                None,
                                &stroke_rr,
                            );
                        }
                    }
                } else {
                    for (fill, fill_blend) in fills {
                        let need_blend_layer = *fill_blend != BlendMix::Normal;
                        if need_blend_layer {
                            scene.push_layer(
                                Fill::NonZero,
                                peniko::BlendMode {
                                    mix: to_peniko_mix(fill_blend),
                                    compose: peniko::Compose::SrcOver,
                                },
                                1.0,
                                node_transform,
                                &rect,
                            );
                        }
                        fill_shape(scene, node_transform, &rect, lx, ly, width, height, fill);
                        if need_blend_layer {
                            scene.pop_layer();
                        }
                    }
                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            let stroke_style = to_kurbo_stroke(stroke, 1.0);
                            let stroke_rect = stroke_aligned_rect(rect, Some(stroke));
                            scene.stroke(
                                &stroke_style,
                                node_transform,
                                to_peniko(stroke.color.clone()),
                                None,
                                &stroke_rect,
                            );
                        }
                    }
                }
                } // !has_layer_blur
                for effect in effects {
                    if matches!(
                        effect.kind,
                        EffectKind::InnerShadow | EffectKind::BackgroundBlur
                    ) {
                        paint_effect(
                            scene,
                            node_transform,
                            &rect,
                            uniform_corner,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
            }
            PaintPrimitive::Path {
                x: path_x,
                y: path_y,
                width,
                height,
                normalize_to_origin,
                text_outline,
                fill_paths,
                stroke_paths,
                svg_base64,
                fills,
                stroke,
                effects,
                blend: _,
                rotation,
                ..
            } => {
                let mut path_x = *path_x;
                let mut path_y = *path_y;
                let (width, height, rotation) = (*width, *height, *rotation);
                if *text_outline {
                    path_x += TEXT_ANCHOR_SHIFT_X;
                    path_y += TEXT_ANCHOR_SHIFT_Y;
                }
                let (base_transform, lx, ly) = if rotation.abs() > 1e-6 {
                    let t = scene_transform
                        * kurbo::Affine::translate((path_x, path_y))
                        * kurbo::Affine::rotate(rotation);
                    (t, 0.0, 0.0)
                } else {
                    (scene_transform, path_x, path_y)
                };
                let mut had_any_path = false;
                let has_native_geometry = !fill_paths.is_empty() || !stroke_paths.is_empty();
                let bounds_rect = Rect::new(lx, ly, lx + width, ly + height);
                let blur_color_hint = fills
                    .first()
                    .and_then(|(f, _)| first_color(f))
                    .or_else(|| stroke.as_ref().map(|s| s.color.clone()));
                let path_origin_transform = base_transform * kurbo::Affine::translate((lx, ly));
                let path_fit =
                    compute_path_fit_transform(
                        fill_paths,
                        stroke_paths,
                        lx,
                        ly,
                        width,
                        height,
                        *normalize_to_origin,
                    );
                let path_transform = path_origin_transform * path_fit;
                let (sx, sy) = extract_scale(path_fit);
                let stroke_scale = ((sx.abs() + sy.abs()) * 0.5).max(0.01);
                let decoded_svg_text: Option<std::sync::Arc<String>> = if has_native_geometry {
                    None
                } else {
                    svg_base64.as_ref().and_then(|b64| cached_svg_decode(b64))
                };
                let has_layer_blur = effects.iter().any(|e| matches!(e.kind, EffectKind::LayerBlur));

                if let (Some(svg_text), Some(layer_blur)) = (
                    decoded_svg_text.as_deref(),
                    effects
                        .iter()
                        .find(|e| matches!(e.kind, EffectKind::LayerBlur)),
                ) {
                    paint_blurred_svg_content(
                        scene,
                        base_transform,
                        svg_text,
                        lx,
                        ly,
                        width,
                        height,
                        layer_blur,
                    );
                }
                for effect in effects {
                    if matches!(effect.kind, EffectKind::LayerBlur) {
                        paint_blurred_path_content(
                            scene,
                            path_transform,
                            fill_paths,
                            stroke_paths,
                            fills.first().map(|(f, _)| f),
                            stroke.as_ref(),
                            stroke_scale,
                            lx,
                            ly,
                            width,
                            height,
                            effect,
                        );
                    }
                }
                for effect in effects {
                    // A glow can't follow an arbitrary outline here; emitters
                    // are mostly round (LEDs, jewels), so it's a rounded box.
                    let corner = match effect.kind {
                        EffectKind::DropShadow => 0.0,
                        EffectKind::Glow => width.min(height) * 0.5,
                        _ => continue,
                    };
                    paint_effect(scene, base_transform, &bounds_rect, corner, effect, blur_color_hint.as_ref());
                }
                if !has_layer_blur {
                if let Some(svg_text) = decoded_svg_text.as_deref() {
                    let svg_fit = cached_svg_fit_transform(svg_text, width, height);
                    let svg_transform = path_origin_transform * svg_fit;
                    if let Some(tree) = cached_svg_tree(svg_text) {
                        {
                        let mut painter = anyrender_vello::VelloScenePainter::new(scene);
                        anyrender_svg::render_svg_tree(&mut painter, &tree, svg_transform);
                    }
                        had_any_path = true;
                    }
                }
                } // !has_layer_blur for SVG

                if !had_any_path && !has_layer_blur {
                    for (fill_paint, fill_blend) in fills {
                        let need_blend_layer = *fill_blend != BlendMix::Normal;
                        if need_blend_layer {
                            scene.push_layer(
                                Fill::NonZero,
                                peniko::BlendMode {
                                    mix: to_peniko_mix(fill_blend),
                                    compose: peniko::Compose::SrcOver,
                                },
                                1.0,
                                base_transform,
                                &bounds_rect,
                            );
                        }
                        for path_data in fill_paths {
                            if let Some(path) = cached_bez_path(path_data) {
                                fill_path_shape(
                                    scene,
                                    path_transform,
                                    &*path,
                                    lx,
                                    ly,
                                    width,
                                    height,
                                    fill_paint,
                                );
                                had_any_path = true;
                            }
                        }
                        if need_blend_layer {
                            scene.pop_layer();
                        }
                    }

                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            for path_data in stroke_paths {
                                if let Some(path) = cached_bez_path(path_data) {
                                    // Figma strokeGeometry is already the outlined stroke shape.
                                    // Fill it directly to avoid double-stroking artifacts.
                                    scene.fill(
                                        peniko::Fill::NonZero,
                                        path_transform,
                                        to_peniko(stroke.color.clone()),
                                        None,
                                        &*path,
                                    );
                                    had_any_path = true;
                                }
                            }
                        }
                    }
                }

                if !had_any_path {
                    let rect = Rect::new(lx, ly, lx + width, ly + height);
                    for (fill, fill_blend) in fills {
                        let need_blend_layer = *fill_blend != BlendMix::Normal;
                        if need_blend_layer {
                            scene.push_layer(
                                Fill::NonZero,
                                peniko::BlendMode {
                                    mix: to_peniko_mix(fill_blend),
                                    compose: peniko::Compose::SrcOver,
                                },
                                1.0,
                                base_transform,
                                &rect,
                            );
                        }
                        fill_shape(scene, base_transform, &rect, lx, ly, width, height, fill);
                        if need_blend_layer {
                            scene.pop_layer();
                        }
                    }
                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            let stroke_style = to_kurbo_stroke(stroke, 1.0);
                            scene.stroke(
                                &stroke_style,
                                base_transform,
                                to_peniko(stroke.color.clone()),
                                None,
                                &rect,
                            );
                        }
                    }
                }
                for effect in effects {
                    if matches!(
                        effect.kind,
                        EffectKind::InnerShadow | EffectKind::BackgroundBlur
                    ) {
                        paint_effect(
                            scene,
                            base_transform,
                            &bounds_rect,
                            0.0,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
            }
            PaintPrimitive::Text {
                x,
                y,
                text,
                color,
                font_size,
                line_height,
                letter_spacing,
                text_case,
                style_runs,
                blend: _,
                width: box_width,
                align,
                ..
            } => {
                let (x, y, font_size, letter_spacing) = (
                    *x + TEXT_ANCHOR_SHIFT_X,
                    *y + TEXT_ANCHOR_SHIFT_Y,
                    *font_size,
                    *letter_spacing,
                );
                let text = apply_text_case(text, text_case);
                let line_height = line_height.unwrap_or(font_size * 1.2).max(1.0);
                let max_line_chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                let approx_width =
                    (max_line_chars as f64 * (font_size * 0.6 + letter_spacing)).max(font_size);
                let line_count = text.lines().count().max(1) as f64;
                let _bounds_rect = Rect::new(x, y, x + approx_width, y + line_count * line_height);
                if let (Some(font_data), Some(font_ref)) = (&font_data, font_ref.as_ref()) {
                    let runs = if style_runs.is_empty() {
                        vec![TextStyleRun {
                            text: text.clone(),
                            color: Some(color.clone()),
                            font_size: Some(font_size),
                            letter_spacing: Some(letter_spacing),
                            font_style: None,
                            font_weight: None,
                        }]
                    } else {
                        style_runs
                            .iter()
                            .cloned()
                            .map(|mut run| {
                                run.text = apply_text_case(&run.text, text_case);
                                run
                            })
                            .collect::<Vec<_>>()
                    };

                    let charmap = font_ref.charmap();
                    // Measure each line first so it can be aligned in its box.
                    let mut line_widths: Vec<f64> = vec![0.0];
                    for run in &runs {
                        let size = Size::new(run.font_size.unwrap_or(font_size).max(1.0) as f32);
                        let var_loc = font_ref.axes().location(std::iter::empty::<(&str, f32)>());
                        let gm = font_ref.glyph_metrics(size, &var_loc);
                        let spacing = run.letter_spacing.unwrap_or(letter_spacing);
                        for ch in run.text.chars() {
                            if ch == '\n' {
                                line_widths.push(0.0);
                                continue;
                            }
                            let gid = charmap.map(ch).unwrap_or_default();
                            if let Some(w) = line_widths.last_mut() {
                                *w += f64::from(gm.advance_width(gid).unwrap_or_default()) + spacing;
                            }
                        }
                    }
                    let line_start = |line: usize| -> f64 {
                        let w = line_widths.get(line).copied().unwrap_or(0.0);
                        match align {
                            TextAlignment::Left => 0.0,
                            TextAlignment::Center => ((*box_width - w) * 0.5).max(0.0),
                            TextAlignment::Right => (*box_width - w).max(0.0),
                        }
                    };
                    let mut line_idx: usize = 0;
                    let mut pen_x: f64 = line_start(0);
                    for run in runs {
                        let run_size = run.font_size.unwrap_or(font_size).max(1.0);
                        let run_letter_spacing = run.letter_spacing.unwrap_or(letter_spacing) as f32;
                        let run_color = run.color.clone().unwrap_or_else(|| color.clone());
                        let style_str = run
                            .font_style
                            .as_deref()
                            .map(|s| s.to_ascii_lowercase())
                            .unwrap_or_default();
                        let is_bold = run.font_weight.unwrap_or(400.0) >= 600.0
                            || style_str.contains("bold");
                        let is_italic = style_str.contains("italic");

                        let size = Size::new(run_size as f32);
                        let axes = font_ref.axes();
                        let var_loc = axes.location(std::iter::empty::<(&str, f32)>());
                        let metrics = font_ref.metrics(size, &var_loc);
                        let glyph_metrics = font_ref.glyph_metrics(size, &var_loc);
                        let ascent = f64::from(metrics.ascent);

                        for ch in run.text.chars() {
                            if ch == '\n' {
                                line_idx += 1;
                                pen_x = line_start(line_idx);
                                continue;
                            }
                            let gid = charmap.map(ch).unwrap_or_default();
                            let advance = f64::from(glyph_metrics.advance_width(gid).unwrap_or_default());
                            let base_transform = scene_transform
                                * kurbo::Affine::translate((
                                    x + pen_x,
                                    y + ascent + (line_idx as f64 * line_height),
                                ));
                            let text_transform = if is_italic {
                                base_transform * kurbo::Affine::new([1.0, 0.0, 0.22, 1.0, 0.0, 0.0])
                            } else {
                                base_transform
                            };

                            scene
                                .draw_glyphs(font_data)
                                .font_size(run_size as f32)
                                .hint(true)
                                .brush(to_peniko(run_color.clone()))
                                .brush_alpha(1.0)
                                .transform(text_transform)
                                .draw(
                                    Fill::NonZero,
                                    std::iter::once(Glyph {
                                        id: gid.to_u32(),
                                        x: 0.0,
                                        y: 0.0,
                                    }),
                                );

                            if is_bold {
                                scene
                                    .draw_glyphs(font_data)
                                    .font_size(run_size as f32)
                                    .hint(true)
                                    .brush(to_peniko(run_color.clone()))
                                    .brush_alpha(1.0)
                                    .transform(text_transform * kurbo::Affine::translate((0.45, 0.0)))
                                    .draw(
                                        Fill::NonZero,
                                        std::iter::once(Glyph {
                                            id: gid.to_u32(),
                                            x: 0.0,
                                            y: 0.0,
                                        }),
                                    );
                            }

                            pen_x += advance + f64::from(run_letter_spacing);
                        }
                    }
                } else {
                    // fallback to a baseline block when no text font is configured
                    for (line_idx, line_text) in text.lines().enumerate() {
                        let width = (line_text.chars().count() as f64
                            * (font_size * 0.5 + letter_spacing))
                            .max(font_size);
                        let y_line = y + (line_idx as f64 * line_height);
                        let line = Rect::new(
                            x,
                            y_line + font_size * 0.85,
                            x + width,
                            y_line + font_size * 0.95,
                        );
                        scene.fill(
                            Fill::NonZero,
                            scene_transform,
                            to_peniko(color.clone()),
                            None,
                            &line,
                        );
                    }
                }
            }
        }
    }

    fn fill_shape(
        scene: &mut vello::Scene,
        scene_transform: kurbo::Affine,
        shape: &impl kurbo::Shape,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill: &FillPaint,
    ) {
        use peniko::Fill;

        match fill {
            FillPaint::Solid(color) => {
                scene.fill(
                    Fill::NonZero,
                    scene_transform,
                    to_peniko(color.clone()),
                    None,
                    shape,
                );
            }
            FillPaint::GradientLinear { transform, stops } => {
                if let Some(gradient) =
                    build_linear_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, scene_transform, &gradient, None, shape);
                }
            }
            FillPaint::GradientRadial { transform, stops } => {
                if let Some((gradient, brush_xform)) =
                    build_radial_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(
                        Fill::NonZero,
                        scene_transform,
                        &gradient,
                        brush_xform,
                        shape,
                    );
                }
            }
            FillPaint::GradientAngular { transform, stops } => {
                if let Some(gradient) =
                    build_angular_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, scene_transform, &gradient, None, shape);
                }
            }
            FillPaint::Image {
                image_hash: _,
                data_base64,
                alpha,
                scale_mode,
                image_transform,
            } => {
                if let Some(image_brush) = decode_image_brush(data_base64, *alpha, width, height) {
                    let iw = image_brush.image.width as f64;
                    let ih = image_brush.image.height as f64;
                    if iw > 0.0 && ih > 0.0 {
                        let brush_transform = image_brush_transform(
                            *scale_mode,
                            image_transform,
                            x,
                            y,
                            width,
                            height,
                            iw,
                            ih,
                        );
                        scene.fill(
                            Fill::NonZero,
                            scene_transform,
                            image_brush.as_ref(),
                            Some(brush_transform),
                            shape,
                        );
                    }
                }
            }
        }
    }

    fn paint_effect(
        scene: &mut vello::Scene,
        scene_transform: kurbo::Affine,
        base_rect: &Rect,
        corner_radius: f64,
        effect: &NodeEffect,
        blur_color_hint: Option<&Rgba>,
    ) {
        match effect.kind {
            EffectKind::DropShadow | EffectKind::Glow => {
                // Spread expands the shadow rect outward in all directions
                let shadow_rect = Rect::new(
                    base_rect.x0 + effect.offset_x - effect.spread,
                    base_rect.y0 + effect.offset_y - effect.spread,
                    base_rect.x1 + effect.offset_x + effect.spread,
                    base_rect.y1 + effect.offset_y + effect.spread,
                );
                // Corner radius grows additively with spread
                let shadow_corner = (corner_radius + effect.spread).max(0.0);
                // Figma radius is CSS blur radius (≈ 2×sigma); draw_box_shadow expects sigma
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    shadow_rect,
                    to_peniko(effect.color.clone()),
                    shadow_corner,
                    (effect.radius * 0.5).max(0.0),
                );
            }
            EffectKind::InnerShadow => {
                // Inner shadow: fill element with shadow color, then punch a
                // blurred hole at the offset position using DestOut compositing.
                // (Pattern from Blitz reference implementation.)
                let rr = RoundedRect::from_rect(*base_rect, corner_radius);

                // Layer 1: isolate inner shadow compositing group
                scene.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, scene_transform, &rr);

                // Fill entire clipped area with shadow color
                scene.fill(
                    peniko::Fill::NonZero,
                    scene_transform,
                    to_peniko(effect.color.clone()),
                    None,
                    &rr,
                );

                // Layer 2: DestOut — punch a blurred hole at offset position
                scene.push_layer(Fill::NonZero, peniko::Compose::DestOut, 1.0, scene_transform, &rr);

                // Spread shrinks the hole (opposite of drop shadow)
                let hole_rect = Rect::new(
                    base_rect.x0 + effect.offset_x + effect.spread,
                    base_rect.y0 + effect.offset_y + effect.spread,
                    base_rect.x1 + effect.offset_x - effect.spread,
                    base_rect.y1 + effect.offset_y - effect.spread,
                );
                let hole_corner = (corner_radius - effect.spread).max(0.0);
                // Figma radius is CSS blur radius (≈ 2×sigma)
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    hole_rect,
                    to_peniko(Rgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    hole_corner,
                    (effect.radius * 0.5).max(0.0),
                );

                scene.pop_layer(); // DestOut
                scene.pop_layer(); // isolation
            }
            EffectKind::LayerBlur => {
                // Single GPU-native Gaussian blur of the bounding rect.
                let sigma = (effect.radius * 0.5).max(0.5);
                let hint = blur_color_hint.cloned().unwrap_or(Rgba {
                    r: 0.75,
                    g: 0.78,
                    b: 0.82,
                    a: 1.0,
                });
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    *base_rect,
                    to_peniko(hint),
                    corner_radius.max(effect.spread),
                    sigma,
                );
            }
            EffectKind::BackgroundBlur => {
                // Single blurred rect approximation for backdrop blur.
                // True background blur requires backdrop sampling which vello doesn't support.
                let sigma = (effect.radius * 0.5).max(0.5);
                let hint_color = blur_color_hint.cloned().unwrap_or(Rgba {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.3,
                });
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    *base_rect,
                    to_peniko(hint_color),
                    corner_radius.max(effect.spread),
                    sigma,
                );
            }
        }
    }

    fn paint_blurred_rect_content(
        scene: &mut vello::Scene,
        scene_transform: kurbo::Affine,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        corner_radius: f64,
        fill: Option<&FillPaint>,
        stroke: Option<&StrokeStyle>,
        effect: &NodeEffect,
    ) {
        let sigma = (effect.radius * 0.5).max(0.5);
        let rect = Rect::new(x, y, x + width, y + height);

        // For solid fills, use vello's GPU-native Gaussian blur on the rectangle.
        // This produces a proper blur rather than shifted semi-transparent copies.
        if let Some(fill) = fill {
            match fill {
                FillPaint::Solid(color) => {
                    scene.draw_blurred_rounded_rect(
                        scene_transform,
                        rect,
                        to_peniko(color.clone()),
                        corner_radius,
                        sigma,
                    );
                }
                // For gradient/image fills, fall back to shifted copies
                _ => {
                    for (dx, dy, weight) in layer_blur_kernel(effect.radius.max(0.5)) {
                        let shifted_transform =
                            scene_transform * kurbo::Affine::translate((dx, dy));
                        if corner_radius > 0.0 {
                            let rr = RoundedRect::from_rect(rect, corner_radius);
                            let f = fill.clone().with_opacity(weight);
                            fill_shape(scene, shifted_transform, &rr, x, y, width, height, &f);
                        } else {
                            let f = fill.clone().with_opacity(weight);
                            fill_shape(scene, shifted_transform, &rect, x, y, width, height, &f);
                        }
                    }
                }
            }
        }
        // Stroke blur: single blurred rect with stroke color
        if let Some(stroke) = stroke {
            if stroke.width > 0.0 {
                let stroke_rect = stroke_aligned_rect(rect, Some(stroke));
                let stroke_radius = stroke_aligned_corner_radius(corner_radius, Some(stroke));
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    stroke_rect,
                    to_peniko(stroke.color.clone()),
                    stroke_radius,
                    sigma,
                );
            }
        }
    }

    fn paint_blurred_path_content(
        scene: &mut vello::Scene,
        path_transform: kurbo::Affine,
        fill_paths: &[String],
        stroke_paths: &[String],
        fill: Option<&FillPaint>,
        stroke: Option<&StrokeStyle>,
        stroke_scale: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        effect: &NodeEffect,
    ) {
        let radius = effect.radius.max(0.5);
        for (dx, dy, weight) in layer_blur_kernel(radius) {
            let shifted = kurbo::Affine::translate((dx, dy)) * path_transform;
            if let Some(fill_paint) = fill.cloned().map(|f| f.with_opacity(weight)) {
                for path_data in fill_paths {
                    if let Some(path) = cached_bez_path(path_data) {
                        fill_path_shape(scene, shifted, &*path, x, y, width, height, &fill_paint);
                    }
                }
            }
            if let Some(stroke) = stroke {
                if stroke.width > 0.0 {
                    let mut stroke_blur = stroke.clone();
                    stroke_blur.color.a = (stroke_blur.color.a * weight).clamp(0.0, 1.0);
                    let stroke_style = to_kurbo_stroke(&stroke_blur, stroke_scale);
                    for path_data in stroke_paths {
                        if let Some(path) = cached_bez_path(path_data) {
                            scene.stroke(
                                &stroke_style,
                                shifted,
                                to_peniko(stroke_blur.color.clone()),
                                None,
                                &*path,
                            );
                        }
                    }
                }
            }
        }
    }

    fn paint_blurred_svg_content(
        scene: &mut vello::Scene,
        scene_transform: kurbo::Affine,
        svg_text: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        effect: &NodeEffect,
    ) {
        let radius = effect.radius.max(0.5);
        let clip_rect = Rect::new(x, y, x + width, y + height);
        let svg_fit = cached_svg_fit_transform(svg_text, width, height);
        // Cache the parsed SVG tree once — blur kernel loops would otherwise
        // re-parse the XML 6-20 times per blurred element per frame.
        let Some(tree) = cached_svg_tree(svg_text) else {
            return;
        };
        for (dx, dy, weight) in layer_blur_kernel(radius) {
            let svg_transform =
                scene_transform * kurbo::Affine::translate((x + dx, y + dy)) * svg_fit;
            scene.push_layer(
                Fill::NonZero,
                peniko::BlendMode {
                    mix: peniko::Mix::Normal,
                    compose: peniko::Compose::SrcOver,
                },
                weight as f32,
                scene_transform,
                &clip_rect,
            );
            {
                let mut painter = anyrender_vello::VelloScenePainter::new(scene);
                anyrender_svg::render_svg_tree(&mut painter, &tree, svg_transform);
            }
            scene.pop_layer();
        }
    }

    fn fill_path_shape(
        scene: &mut vello::Scene,
        path_transform: kurbo::Affine,
        path: &BezPath,
        _x: f64,
        _y: f64,
        width: f64,
        height: f64,
        fill: &FillPaint,
    ) {
        use peniko::Fill;

        // Gradient coordinates must be in the same space as the path shape.
        // fillGeometry paths are in local coords (0,0 origin); path_transform
        // maps them to screen space.  Build gradients at (0,0) so they align.
        match fill {
            FillPaint::Solid(color) => {
                scene.fill(
                    Fill::NonZero,
                    path_transform,
                    to_peniko(color.clone()),
                    None,
                    path,
                );
            }
            FillPaint::GradientLinear { transform, stops } => {
                if let Some(gradient) =
                    build_linear_gradient(0.0, 0.0, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, None, path);
                }
            }
            FillPaint::GradientRadial { transform, stops } => {
                if let Some((gradient, brush_xform)) =
                    build_radial_gradient(0.0, 0.0, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, brush_xform, path);
                }
            }
            FillPaint::GradientAngular { transform, stops } => {
                if let Some(gradient) =
                    build_angular_gradient(0.0, 0.0, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, None, path);
                }
            }
            FillPaint::Image {
                image_hash: _,
                data_base64,
                alpha,
                scale_mode,
                image_transform,
            } => {
                if let Some(image_brush) = decode_image_brush(data_base64, *alpha, width, height) {
                    let iw = image_brush.image.width as f64;
                    let ih = image_brush.image.height as f64;
                    if iw > 0.0 && ih > 0.0 {
                        // Use local-space origin (0, 0) — same as gradient fix above
                        let brush_transform = image_brush_transform(
                            *scale_mode,
                            image_transform,
                            0.0,
                            0.0,
                            width,
                            height,
                            iw,
                            ih,
                        );
                        scene.fill(
                            Fill::NonZero,
                            path_transform,
                            image_brush.as_ref(),
                            Some(brush_transform),
                            path,
                        );
                    }
                }
            }
        }
    }

    fn image_brush_transform(
        scale_mode: ImageScaleMode,
        image_transform: &Option<AffineTransform2D>,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        iw: f64,
        ih: f64,
    ) -> kurbo::Affine {
        match scale_mode {
            ImageScaleMode::Fill => {
                // Scale uniformly to cover the entire node, center the image
                let scale = if width > 0.0 && height > 0.0 {
                    (width / iw).max(height / ih)
                } else {
                    1.0
                };
                let offset_x = x + (width - iw * scale) * 0.5;
                let offset_y = y + (height - ih * scale) * 0.5;
                kurbo::Affine::translate((offset_x, offset_y)) * kurbo::Affine::scale(scale)
            }
            ImageScaleMode::Fit => {
                // Scale uniformly to fit inside the node, center the image
                let scale = if width > 0.0 && height > 0.0 {
                    (width / iw).min(height / ih)
                } else {
                    1.0
                };
                let offset_x = x + (width - iw * scale) * 0.5;
                let offset_y = y + (height - ih * scale) * 0.5;
                kurbo::Affine::translate((offset_x, offset_y)) * kurbo::Affine::scale(scale)
            }
            ImageScaleMode::Crop => {
                // Figma imageTransform maps node-normalized [0,1]×[0,1] to
                // image-normalized [0,1]×[0,1]. The brush transform maps image
                // pixels → scene pixels, so we need the INVERSE of imageTransform:
                //   translate(node_origin) * scale(node_size) * inv(T) * scale(1/img_size)
                if let Some(t) = image_transform {
                    let det = t.determinant();
                    if det.abs() < 1e-12 {
                        let scale = (width / iw).max(height / ih);
                        let ox = x + (width - iw * scale) * 0.5;
                        let oy = y + (height - ih * scale) * 0.5;
                        return kurbo::Affine::translate((ox, oy)) * kurbo::Affine::scale(scale);
                    }
                    // Invert the 2x3 affine: node-norm → image-norm becomes
                    // image-norm → node-norm
                    let inv = kurbo::Affine::new([
                        t.d / det,
                        -t.c / det,
                        -t.b / det,
                        t.a / det,
                        (t.b * t.ty - t.d * t.tx) / det,
                        (t.c * t.tx - t.a * t.ty) / det,
                    ]);
                    kurbo::Affine::translate((x, y))
                        * kurbo::Affine::scale_non_uniform(width, height)
                        * inv
                        * kurbo::Affine::scale_non_uniform(1.0 / iw, 1.0 / ih)
                } else {
                    // No transform — fall back to Fill
                    let scale = (width / iw).max(height / ih);
                    let ox = x + (width - iw * scale) * 0.5;
                    let oy = y + (height - ih * scale) * 0.5;
                    kurbo::Affine::translate((ox, oy)) * kurbo::Affine::scale(scale)
                }
            }
            ImageScaleMode::Tile => {
                // Simple 1:1 tiling from the node origin (Vello Pad extend handles the rest)
                kurbo::Affine::translate((x, y))
            }
            ImageScaleMode::Stretch => {
                // Non-uniform stretch to fill node exactly
                let sx = if width > 0.0 { width / iw } else { 1.0 };
                let sy = if height > 0.0 { height / ih } else { 1.0 };
                kurbo::Affine::translate((x, y)) * kurbo::Affine::scale_non_uniform(sx, sy)
            }
        }
    }

    fn layer_blur_kernel(radius: f64) -> Vec<(f64, f64, f64)> {
        let sigma = (radius * 0.5).max(0.5);
        // Cap the integer grid to at most 7x7 = 49 samples to avoid
        // overwhelming the GPU with draw calls per blurred element.
        let half_grid: i32 = 3; // grid goes -3..+3 → 7 taps per axis
        // Step size spreads the grid to cover 2.5σ spatially
        let step = (sigma * 2.5) / half_grid as f64;
        let mut kernel = Vec::new();
        let mut total_weight = 0.0;

        for iy in -half_grid..=half_grid {
            for ix in -half_grid..=half_grid {
                let dx = ix as f64 * step;
                let dy = iy as f64 * step;
                let w = (-0.5 * (dx * dx + dy * dy) / (sigma * sigma)).exp();
                kernel.push((dx, dy, w));
                total_weight += w;
            }
        }

        // Normalize weights to sum to 1.0
        for entry in &mut kernel {
            entry.2 /= total_weight;
        }
        kernel
    }

    fn build_linear_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&AffineTransform2D>,
        stops: &[GradientStop],
    ) -> Option<peniko::Gradient> {
        use kurbo::Point;

        if stops.is_empty() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        // Figma linear gradient runs from UV (0, 0.5) to (1, 0.5)
        let (snx, sny) = gradient_transform_point(transform, 0.0, 0.5);
        let (enx, eny) = gradient_transform_point(transform, 1.0, 0.5);
        let start = Point::new(x + snx * width, y + sny * height);
        let end = Point::new(x + enx * width, y + eny * height);
        Some(peniko::Gradient::new_linear(start, end).with_stops(to_peniko_stops(stops).as_slice()))
    }

    fn build_radial_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&AffineTransform2D>,
        stops: &[GradientStop],
    ) -> Option<(peniko::Gradient, Option<kurbo::Affine>)> {
        use kurbo::Point;

        if stops.is_empty() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        // Figma radial center is at UV (0.5, 0.5)
        let (cnx, cny) = gradient_transform_point(transform, 0.5, 0.5);
        let center = Point::new(x + cnx * width, y + cny * height);
        let peniko_stops = to_peniko_stops(stops);

        if let Some(t) = transform {
            // Ellipse axes from transform column vectors scaled to pixel space
            let ax = (t.a * width, t.c * height);
            let ay = (t.b * width, t.d * height);
            let rx = (ax.0 * ax.0 + ax.1 * ax.1).sqrt() * 0.5;
            let ry = (ay.0 * ay.0 + ay.1 * ay.1).sqrt() * 0.5;
            let angle = ax.1.atan2(ax.0);

            // Unit circle gradient at origin; brush_transform maps to ellipse
            let gradient =
                peniko::Gradient::new_two_point_radial(Point::ZERO, 0.0, Point::ZERO, 1.0)
                    .with_stops(peniko_stops.as_slice());

            let brush_xform = kurbo::Affine::translate(center.to_vec2())
                * kurbo::Affine::rotate(angle)
                * kurbo::Affine::scale_non_uniform(rx.max(0.5), ry.max(0.5));

            Some((gradient, Some(brush_xform)))
        } else {
            let radius = (width.min(height) * 0.5).max(0.5) as f32;
            let gradient = peniko::Gradient::new_two_point_radial(center, 0.0, center, radius)
                .with_stops(peniko_stops.as_slice());
            Some((gradient, None))
        }
    }

    fn build_angular_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&AffineTransform2D>,
        stops: &[GradientStop],
    ) -> Option<peniko::Gradient> {
        use kurbo::Point;

        if stops.is_empty() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        // Figma angular/sweep center is at UV (0.5, 0.5)
        let (cnx, cny) = gradient_transform_point(transform, 0.5, 0.5);
        let center = Point::new(x + cnx * width, y + cny * height);
        // Extract rotation angle from transform's first column
        let start_angle = if let Some(t) = transform {
            (t.c * height).atan2(t.a * width) as f32
        } else {
            0.0
        };
        let end_angle = start_angle + std::f32::consts::PI * 2.0;
        Some(
            peniko::Gradient::new_sweep(center, start_angle, end_angle)
                // A sweep is periodic: wrap past the seam instead of padding,
                // so a start angle other than +x doesn't leave a flat wedge.
                .with_extend(peniko::Extend::Repeat)
                .with_stops(to_peniko_stops(stops).as_slice()),
        )
    }

    fn gradient_transform_point(transform: Option<&AffineTransform2D>, u: f64, v: f64) -> (f64, f64) {
        if let Some(t) = transform {
            t.transform_point(u, v)
        } else {
            (u, v)
        }
    }

    fn to_peniko_stops(stops: &[GradientStop]) -> Vec<peniko::ColorStop> {
        use peniko::color::DynamicColor;

        stops
            .iter()
            .map(|stop| peniko::ColorStop {
                offset: stop.offset as f32,
                color: DynamicColor::from_alpha_color(to_peniko(stop.color.clone())),
            })
            .collect()
    }

    fn to_kurbo_stroke(style: &StrokeStyle, width_scale: f64) -> Stroke {
        let cap = match style.cap {
            StrokeCap::None => kurbo::Cap::Butt,
            StrokeCap::Round => kurbo::Cap::Round,
            StrokeCap::Square => kurbo::Cap::Square,
            StrokeCap::LineArrow
            | StrokeCap::TriangleArrow
            | StrokeCap::DiamondFilled
            | StrokeCap::CircleFilled
            | StrokeCap::TriangleFilled
            | StrokeCap::WashiTape1
            | StrokeCap::WashiTape2
            | StrokeCap::WashiTape3
            | StrokeCap::WashiTape4
            | StrokeCap::WashiTape5
            | StrokeCap::WashiTape6 => kurbo::Cap::Butt,
        };
        let join = match style.join {
            StrokeJoin::Miter => kurbo::Join::Miter,
            StrokeJoin::Round => kurbo::Join::Round,
            StrokeJoin::Bevel => kurbo::Join::Bevel,
        };
        let mut stroke = Stroke::new((style.width * width_scale).max(0.0))
            .with_caps(cap)
            .with_join(join)
            .with_miter_limit(style.miter_limit.max(1.0));
        if !style.dash_pattern.is_empty() {
            stroke = stroke.with_dashes(
                style.dash_offset * width_scale,
                style
                    .dash_pattern
                    .iter()
                    .copied()
                    .map(|v| (v * width_scale).max(0.0)),
            );
        }
        stroke
    }

    fn to_peniko_mix(blend: &BlendMix) -> peniko::Mix {
        match blend {
            BlendMix::Normal => peniko::Mix::Normal,
            BlendMix::Multiply => peniko::Mix::Multiply,
            BlendMix::Screen => peniko::Mix::Screen,
            BlendMix::Overlay => peniko::Mix::Overlay,
            BlendMix::Darken => peniko::Mix::Darken,
            BlendMix::Lighten => peniko::Mix::Lighten,
            BlendMix::ColorDodge => peniko::Mix::ColorDodge,
            BlendMix::ColorBurn => peniko::Mix::ColorBurn,
            BlendMix::HardLight => peniko::Mix::HardLight,
            BlendMix::SoftLight => peniko::Mix::SoftLight,
            BlendMix::Difference => peniko::Mix::Difference,
            BlendMix::Exclusion => peniko::Mix::Exclusion,
            BlendMix::Hue => peniko::Mix::Hue,
            BlendMix::Saturation => peniko::Mix::Saturation,
            BlendMix::Color => peniko::Mix::Color,
            BlendMix::Luminosity => peniko::Mix::Luminosity,
        }
    }

    fn stroke_aligned_rect(base: Rect, style: Option<&StrokeStyle>) -> Rect {
        let Some(style) = style else {
            return base;
        };
        let half = (style.width * 0.5).max(0.0);
        let snap_half = |v: f64| ((v * 2.0).round()) * 0.5;
        match style.align {
            StrokeAlign::Inside => Rect::new(
                base.x0 + half,
                base.y0 + half,
                base.x1 - half,
                base.y1 - half,
            ),
            StrokeAlign::Outside => Rect::new(
                base.x0 - half,
                base.y0 - half,
                base.x1 + half,
                base.y1 + half,
            ),
            StrokeAlign::Center => Rect::new(
                snap_half(base.x0),
                snap_half(base.y0),
                snap_half(base.x1),
                snap_half(base.y1),
            ),
        }
    }

    fn stroke_aligned_corner_radius(base: f64, style: Option<&StrokeStyle>) -> f64 {
        let Some(style) = style else {
            return base.max(0.0);
        };
        let half = (style.width * 0.5).max(0.0);
        match style.align {
            StrokeAlign::Inside => (base - half).max(0.0),
            StrokeAlign::Outside => (base + half).max(0.0),
            StrokeAlign::Center => base.max(0.0),
        }
    }

    fn decode_image_brush(
        data_base64: &str,
        alpha: f64,
        display_w: f64,
        display_h: f64,
    ) -> Option<ImageBrush> {
        use std::collections::HashMap;
        use std::hash::{Hash, Hasher};
        use std::sync::{LazyLock, Mutex};

        static CACHE: LazyLock<Mutex<HashMap<u64, ImageBrush>>> =
            LazyLock::new(|| Mutex::new(HashMap::new()));

        // Include display dimensions in cache key (rounded to nearest 64px to avoid
        // excessive cache entries from sub-pixel differences)
        let bucket_w = ((display_w / 64.0).ceil() as u32).max(1) * 64;
        let bucket_h = ((display_h / 64.0).ceil() as u32).max(1) * 64;

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        data_base64.hash(&mut hasher);
        alpha.to_bits().hash(&mut hasher);
        bucket_w.hash(&mut hasher);
        bucket_h.hash(&mut hasher);
        let cache_key = hasher.finish();

        if let Ok(cache) = CACHE.lock() {
            if let Some(cached) = cache.get(&cache_key) {
                return Some(cached.clone());
            }
        }

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_base64)
            .ok()?;
        let decoded = image::load_from_memory(&bytes).ok()?.into_rgba8();
        let (orig_w, orig_h) = (decoded.width(), decoded.height());

        // Downscale to 2x display size (for retina quality) if the source image
        // is significantly larger than the display area. This avoids uploading
        // multi-megabyte textures to the GPU when only a fraction of pixels are visible.
        let max_w = (display_w * 2.0).ceil() as u32;
        let max_h = (display_h * 2.0).ceil() as u32;
        let (final_img, width, height) = if orig_w > max_w.max(64) || orig_h > max_h.max(64) {
            let scale = (max_w as f64 / orig_w as f64).min(max_h as f64 / orig_h as f64);
            let new_w = ((orig_w as f64 * scale).round() as u32).max(1);
            let new_h = ((orig_h as f64 * scale).round() as u32).max(1);
            let resized = image::imageops::resize(
                &decoded,
                new_w,
                new_h,
                image::imageops::FilterType::Triangle,
            );
            let (w, h) = (resized.width(), resized.height());
            (resized.into_vec(), w, h)
        } else {
            (decoded.into_vec(), orig_w, orig_h)
        };

        let brush = ImageBrush {
            image: ImageData {
                data: Blob::new(Arc::new(final_img)),
                format: peniko::ImageFormat::Rgba8,
                alpha_type: peniko::ImageAlphaType::Alpha,
                width,
                height,
            },
            sampler: ImageSampler {
                x_extend: peniko::Extend::Pad,
                y_extend: peniko::Extend::Pad,
                quality: peniko::ImageQuality::Medium,
                alpha: alpha as f32,
            },
        };

        if let Ok(mut cache) = CACHE.lock() {
            cache.insert(cache_key, brush.clone());
        }

        Some(brush)
    }

    /// Render buffered mask-definition primitives into the scene.
    /// These are the shape primitives captured between MaskDefStart..MaskDefEnd.
    fn render_mask_primitives(
        scene: &mut vello::Scene,
        prims: &[&PaintPrimitive],
        scene_transform: kurbo::Affine,
        _text_font: Option<TextFontRef<'_>>,
        _font_data: &Option<FontData>,
        _font_ref: Option<&FontRef<'_>>,
    ) {
        let root_transform = scene_transform;
        let mut group_stack: Vec<kurbo::Affine> = Vec::new();
        for prim in prims {
            let scene_transform = group_stack.last().copied().unwrap_or(root_transform);
            match prim {
                PaintPrimitive::GroupStart { transform, .. } => {
                    group_stack.push(scene_transform * to_kurbo_affine(transform));
                }
                PaintPrimitive::GroupEnd { .. } => {
                    group_stack.pop();
                }
                PaintPrimitive::Rect {
                    x, y, width, height, fills, corner_radii, rotation, ..
                } => {
                    let (x, y, width, height, rotation) = (*x, *y, *width, *height, *rotation);
                    let (node_transform, lx, ly) = if rotation.abs() > 1e-6 {
                        let t = scene_transform
                            * kurbo::Affine::translate((x, y))
                            * kurbo::Affine::rotate(rotation);
                        (t, 0.0, 0.0)
                    } else {
                        (scene_transform, x, y)
                    };
                    let rect = Rect::new(lx, ly, lx + width, ly + height);
                    if !corner_radii.is_zero() {
                        let radii = kurbo::RoundedRectRadii::new(
                            corner_radii.top_left,
                            corner_radii.top_right,
                            corner_radii.bottom_right,
                            corner_radii.bottom_left,
                        );
                        let rr = RoundedRect::from_rect(rect, radii);
                        for (fill, _) in fills {
                            fill_shape(scene, node_transform, &rr, lx, ly, width, height, fill);
                        }
                    } else {
                        for (fill, _) in fills {
                            fill_shape(scene, node_transform, &rect, lx, ly, width, height, fill);
                        }
                    }
                }
                PaintPrimitive::Path {
                    x: path_x, y: path_y, width, height, fill_paths, fills, rotation,
                    normalize_to_origin, stroke_paths, ..
                } => {
                    let (width, height, rotation) = (*width, *height, *rotation);
                    let (base_transform, lx, ly) = if rotation.abs() > 1e-6 {
                        let t = scene_transform
                            * kurbo::Affine::translate((*path_x, *path_y))
                            * kurbo::Affine::rotate(rotation);
                        (t, 0.0, 0.0)
                    } else {
                        (scene_transform, *path_x, *path_y)
                    };
                    let path_origin_transform = base_transform * kurbo::Affine::translate((lx, ly));
                    let path_fit = compute_path_fit_transform(
                        fill_paths, stroke_paths, lx, ly, width, height, *normalize_to_origin,
                    );
                    let path_transform = path_origin_transform * path_fit;
                    for (fill_paint, _) in fills {
                        for path_data in fill_paths {
                            if let Some(path) = cached_bez_path(path_data) {
                                fill_path_shape(scene, path_transform, &*path, lx, ly, width, height, fill_paint);
                            }
                        }
                    }
                }
                _ => {
                    // Other primitive types in mask defs are uncommon; skip them.
                }
            }
        }
    }

    /// Row-major `[[a, b, tx], [c, d, ty]]` → kurbo's column-major coefficients.
    fn to_kurbo_affine(t: &AffineTransform2D) -> kurbo::Affine {
        kurbo::Affine::new([t.a, t.c, t.b, t.d, t.tx, t.ty])
    }

    fn apply_text_case(text: &str, text_case: &TextCase) -> String {
        match text_case {
            TextCase::Original => text.to_string(),
            TextCase::Upper | TextCase::SmallCaps | TextCase::SmallCapsForced => {
                text.to_uppercase()
            }
            TextCase::Lower => text.to_lowercase(),
            TextCase::Title => {
                let mut out = String::with_capacity(text.len());
                let mut new_word = true;
                for ch in text.chars() {
                    if ch.is_whitespace() {
                        new_word = true;
                        out.push(ch);
                    } else if new_word {
                        out.extend(ch.to_uppercase());
                        new_word = false;
                    } else {
                        out.push(ch);
                    }
                }
                out
            }
        }
    }
}
