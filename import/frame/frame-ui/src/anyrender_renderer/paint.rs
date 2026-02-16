use super::cache::{cached_bez_path, cached_svg_decode, cached_svg_fit_transform};
use super::collect::build_paint_primitives;
use super::extract::first_color_from_fill;
use super::geometry::{compute_path_fit_transform, extract_scale, to_peniko};
use super::types::*;
use frame_proto::{FrameDocument, NodeId};

pub fn paint_into_scene(scene: &mut impl anyrender::PaintScene, doc: &FrameDocument, root: NodeId) {
    paint_into_scene_with(scene, doc, root, kurbo::Affine::IDENTITY, None);
}

pub fn paint_into_scene_with_font(
    scene: &mut impl anyrender::PaintScene,
    doc: &FrameDocument,
    root: NodeId,
    font: TextFontRef<'_>,
) {
    paint_into_scene_with(scene, doc, root, kurbo::Affine::IDENTITY, Some(font));
}

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

pub fn paint_primitives_into_scene_with(
    scene: &mut impl anyrender::PaintScene,
    primitives: &[PaintPrimitive],
    scene_transform: kurbo::Affine,
    text_font: Option<TextFontRef<'_>>,
) {
    use anyrender::Glyph;
    use base64::Engine;
    use kurbo::{BezPath, Rect, RoundedRect, Stroke};
    use peniko::{Blob, Fill, FontData, ImageBrush, ImageData, ImageSampler};
    use skrifa::MetadataProvider;
    use skrifa::instance::Size;
    use skrifa::prelude::LocationRef;
    use skrifa::raw::{FileRef, FontRef};
    use std::sync::Arc;

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

    for primitive in primitives.iter().cloned() {
        match primitive {
            PaintPrimitive::LayerStart {
                x,
                y,
                width,
                height,
                blend,
                opacity,
                ..
            } => {
                let clip_rect = Rect::new(x, y, x + width, y + height);
                scene.push_layer(
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
                    scene.push_clip_layer(scene_transform, &rr);
                } else {
                    scene.push_clip_layer(scene_transform, &rect);
                }
            }
            PaintPrimitive::ClipEnd { .. } => {
                scene.pop_layer();
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
                ..
            } => {
                let rect = Rect::new(x, y, x + width, y + height);
                let uniform_corner = corner_radii.top_left;
                let blur_color_hint = fills
                    .first()
                    .and_then(first_color_from_fill)
                    .or_else(|| stroke.as_ref().map(|s| s.color.clone()));
                for effect in &effects {
                    if matches!(effect.kind, EffectKind::LayerBlur) {
                        paint_blurred_rect_content(
                            scene,
                            scene_transform,
                            x,
                            y,
                            width,
                            height,
                            uniform_corner,
                            fills.first(),
                            stroke.as_ref(),
                            effect,
                        );
                    }
                }
                for effect in &effects {
                    if matches!(effect.kind, EffectKind::DropShadow) {
                        paint_effect(
                            scene,
                            scene_transform,
                            &rect,
                            uniform_corner,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
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
                    for fill in &fills {
                        fill_shape(scene, scene_transform, &rr, x, y, width, height, fill);
                    }
                    if let Some(stroke) = stroke {
                        if stroke.width > 0.0 {
                            let stroke_style = to_kurbo_stroke(&stroke, 1.0);
                            let stroke_rr = RoundedRect::from_rect(stroke_rect, stroke_radius);
                            scene.stroke(
                                &stroke_style,
                                scene_transform,
                                to_peniko(stroke.color),
                                None,
                                &stroke_rr,
                            );
                        }
                    }
                } else {
                    for fill in &fills {
                        fill_shape(scene, scene_transform, &rect, x, y, width, height, fill);
                    }
                    if let Some(stroke) = stroke {
                        if stroke.width > 0.0 {
                            let stroke_style = to_kurbo_stroke(&stroke, 1.0);
                            let stroke_rect = stroke_aligned_rect(rect, Some(&stroke));
                            scene.stroke(
                                &stroke_style,
                                scene_transform,
                                to_peniko(stroke.color),
                                None,
                                &stroke_rect,
                            );
                        }
                    }
                }
                for effect in &effects {
                    if matches!(
                        effect.kind,
                        EffectKind::InnerShadow | EffectKind::BackgroundBlur
                    ) {
                        paint_effect(
                            scene,
                            scene_transform,
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
                fill_paths,
                stroke_paths,
                svg_base64,
                fills,
                stroke,
                effects,
                blend: _,
                ..
            } => {
                let mut had_any_path = false;
                let bounds_rect = Rect::new(path_x, path_y, path_x + width, path_y + height);
                let blur_color_hint = fills
                    .first()
                    .and_then(first_color_from_fill)
                    .or_else(|| stroke.as_ref().map(|s| s.color.clone()));
                let path_origin_transform =
                    scene_transform * kurbo::Affine::translate((path_x, path_y));
                let path_fit = compute_path_fit_transform(
                    &fill_paths,
                    &stroke_paths,
                    path_x,
                    path_y,
                    width,
                    height,
                );
                let path_transform = path_origin_transform * path_fit;
                let (sx, sy) = extract_scale(path_fit);
                let stroke_scale = ((sx.abs() + sy.abs()) * 0.5).max(0.01);
                let decoded_svg_text: Option<String> = svg_base64
                    .as_ref()
                    .and_then(|b64| cached_svg_decode(b64));

                if let (Some(svg_text), Some(layer_blur)) = (
                    decoded_svg_text.as_deref(),
                    effects
                        .iter()
                        .find(|e| matches!(e.kind, EffectKind::LayerBlur)),
                ) {
                    paint_blurred_svg_content(
                        scene,
                        scene_transform,
                        svg_text,
                        path_x,
                        path_y,
                        width,
                        height,
                        layer_blur,
                    );
                }
                for effect in &effects {
                    if matches!(effect.kind, EffectKind::LayerBlur) {
                        paint_blurred_path_content(
                            scene,
                            path_transform,
                            &fill_paths,
                            &stroke_paths,
                            fills.first(),
                            stroke.as_ref(),
                            stroke_scale,
                            path_x,
                            path_y,
                            width,
                            height,
                            effect,
                        );
                    }
                }
                for effect in &effects {
                    if matches!(effect.kind, EffectKind::DropShadow) {
                        paint_effect(
                            scene,
                            scene_transform,
                            &bounds_rect,
                            0.0,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
                if let Some(svg_text) = decoded_svg_text.as_deref() {
                    let svg_fit = cached_svg_fit_transform(svg_text, width, height);
                    let svg_transform = path_origin_transform * svg_fit;
                    if anyrender_svg::render_svg_str(scene, svg_text, svg_transform).is_ok() {
                        had_any_path = true;
                    }
                }

                if !had_any_path {
                    for fill_paint in &fills {
                        for path_data in &fill_paths {
                            if let Some(path) = cached_bez_path(path_data) {
                                fill_path_shape(
                                    scene,
                                    path_transform,
                                    &path,
                                    path_x,
                                    path_y,
                                    width,
                                    height,
                                    fill_paint,
                                );
                                had_any_path = true;
                            }
                        }
                    }

                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            for path_data in &stroke_paths {
                                if let Some(path) = cached_bez_path(path_data) {
                                    let stroke_style = to_kurbo_stroke(stroke, stroke_scale);
                                    scene.stroke(
                                        &stroke_style,
                                        path_transform,
                                        to_peniko(stroke.color.clone()),
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
                    for fill in &fills {
                        fill_shape(
                            scene,
                            scene_transform,
                            &rect,
                            path_x,
                            path_y,
                            width,
                            height,
                            fill,
                        );
                    }
                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            let stroke_style = to_kurbo_stroke(stroke, 1.0);
                            scene.stroke(
                                &stroke_style,
                                scene_transform,
                                to_peniko(stroke.color.clone()),
                                None,
                                &rect,
                            );
                        }
                    }
                }
                for effect in &effects {
                    if matches!(
                        effect.kind,
                        EffectKind::InnerShadow | EffectKind::BackgroundBlur
                    ) {
                        paint_effect(
                            scene,
                            scene_transform,
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
                blend: _,
                ..
            } => {
                let text = apply_text_case(&text, &text_case);
                let line_height = line_height.unwrap_or(font_size * 1.2).max(1.0);
                let max_line_chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                let approx_width =
                    (max_line_chars as f64 * (font_size * 0.6 + letter_spacing)).max(font_size);
                let line_count = text.lines().count().max(1) as f64;
                let _bounds_rect = Rect::new(x, y, x + approx_width, y + line_count * line_height);
                if let (Some(font_data), Some(font_ref)) = (&font_data, font_ref.as_ref()) {
                    let size = Size::new(font_size as f32);
                    let charmap = font_ref.charmap();
                    let glyph_metrics = font_ref.glyph_metrics(size, LocationRef::default());
                    let letter_spacing_f32 = letter_spacing as f32;

                    for (line_idx, line) in text.lines().enumerate() {
                        let mut glyphs = Vec::new();
                        let mut pen_x = 0.0_f32;
                        for ch in line.chars() {
                            let gid = charmap.map(ch).unwrap_or_default();
                            let advance = glyph_metrics.advance_width(gid).unwrap_or_default();
                            glyphs.push(Glyph {
                                id: gid.to_u32(),
                                x: pen_x,
                                y: 0.0,
                            });
                            pen_x += advance + letter_spacing_f32;
                        }

                        scene.draw_glyphs(
                            font_data,
                            font_size as f32,
                            false,
                            &[],
                            Fill::NonZero,
                            to_peniko(color.clone()),
                            1.0,
                            scene_transform
                                * kurbo::Affine::translate((
                                    x,
                                    y + font_size + (line_idx as f64 * line_height) as f64,
                                )),
                            None,
                            glyphs.into_iter(),
                        );
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
        scene: &mut impl anyrender::PaintScene,
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
                    scene.fill(Fill::NonZero, scene_transform, &gradient, brush_xform, shape);
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
                if let Some(image_brush) = decode_image_brush(data_base64, *alpha) {
                    let iw = image_brush.image.width as f64;
                    let ih = image_brush.image.height as f64;
                    if iw > 0.0 && ih > 0.0 {
                        let brush_transform =
                            image_brush_transform(*scale_mode, image_transform, x, y, width, height, iw, ih);
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
        scene: &mut impl anyrender::PaintScene,
        scene_transform: kurbo::Affine,
        base_rect: &Rect,
        corner_radius: f64,
        effect: &NodeEffect,
        blur_color_hint: Option<&Rgba>,
    ) {
        match effect.kind {
            EffectKind::DropShadow => {
                // Spread expands the shadow rect outward in all directions
                let shadow_rect = Rect::new(
                    base_rect.x0 + effect.offset_x - effect.spread,
                    base_rect.y0 + effect.offset_y - effect.spread,
                    base_rect.x1 + effect.offset_x + effect.spread,
                    base_rect.y1 + effect.offset_y + effect.spread,
                );
                // Corner radius grows additively with spread
                let shadow_corner = (corner_radius + effect.spread).max(0.0);
                scene.draw_box_shadow(
                    scene_transform,
                    shadow_rect,
                    to_peniko(effect.color.clone()),
                    shadow_corner,
                    effect.radius.max(0.0),
                );
            }
            EffectKind::InnerShadow => {
                // Approximate inner shadow by clipping to the node bounds and
                // drawing a shifted box shadow pass.
                if corner_radius > 0.0 {
                    let rr = RoundedRect::from_rect(*base_rect, corner_radius);
                    scene.push_clip_layer(scene_transform, &rr);
                } else {
                    scene.push_clip_layer(scene_transform, base_rect);
                }
                let shadow_rect = Rect::new(
                    base_rect.x0 - effect.offset_x,
                    base_rect.y0 - effect.offset_y,
                    base_rect.x1 - effect.offset_x,
                    base_rect.y1 - effect.offset_y,
                );
                // Corner radius grows additively with spread for inner shadow too
                let inner_corner = (corner_radius + effect.spread).max(0.0);
                scene.draw_box_shadow(
                    scene_transform,
                    shadow_rect,
                    to_peniko(effect.color.clone()),
                    inner_corner,
                    effect.radius.max(0.0),
                );
                scene.pop_layer();
            }
            EffectKind::LayerBlur => {
                // Multi-pass blur emulation around bounds, using node color hint when available.
                let radius = effect.radius.max(0.5);
                let hint = blur_color_hint.cloned().unwrap_or(Rgba {
                    r: 0.75,
                    g: 0.78,
                    b: 0.82,
                    a: 0.35,
                });
                for (dx, dy, weight) in layer_blur_kernel(radius) {
                    let shifted = Rect::new(
                        base_rect.x0 + dx,
                        base_rect.y0 + dy,
                        base_rect.x1 + dx,
                        base_rect.y1 + dy,
                    );
                    let mut c = hint.clone();
                    c.a = (hint.a * weight).clamp(0.0, 1.0);
                    scene.draw_box_shadow(
                        scene_transform,
                        shifted,
                        to_peniko(c),
                        corner_radius.max(effect.spread),
                        radius * 0.5,
                    );
                }
            }
            EffectKind::BackgroundBlur => {
                // Approximation: subtle backdrop haze in bounds.
                // True background blur requires backdrop sampling + filtering.
                let radius = effect.radius.max(0.5);
                for (dx, dy, weight) in layer_blur_kernel(radius * 0.8) {
                    let shifted = Rect::new(
                        base_rect.x0 + dx,
                        base_rect.y0 + dy,
                        base_rect.x1 + dx,
                        base_rect.y1 + dy,
                    );
                    scene.draw_box_shadow(
                        scene_transform,
                        shifted,
                        to_peniko(Rgba {
                            r: 0.90,
                            g: 0.92,
                            b: 0.96,
                            a: (weight * 0.65).clamp(0.0, 0.24),
                        }),
                        corner_radius.max(effect.spread),
                        radius * 0.45,
                    );
                }
            }
        }
    }

    fn paint_blurred_rect_content(
        scene: &mut impl anyrender::PaintScene,
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
        let radius = effect.radius.max(0.5);
        for (dx, dy, weight) in layer_blur_kernel(radius) {
            let shifted_transform = scene_transform * kurbo::Affine::translate((dx, dy));
            let rect = Rect::new(x, y, x + width, y + height);
            if corner_radius > 0.0 {
                let rr = RoundedRect::from_rect(rect, corner_radius);
                if let Some(fill) = fill.cloned().map(|f| f.with_opacity(weight)) {
                    fill_shape(scene, shifted_transform, &rr, x, y, width, height, &fill);
                }
                if let Some(stroke) = stroke {
                    if stroke.width > 0.0 {
                        let mut stroke_blur = stroke.clone();
                        stroke_blur.color.a = (stroke_blur.color.a * weight).clamp(0.0, 1.0);
                        let stroke_style = to_kurbo_stroke(&stroke_blur, 1.0);
                        let stroke_rect = stroke_aligned_rect(rect, Some(&stroke_blur));
                        let stroke_radius =
                            stroke_aligned_corner_radius(corner_radius, Some(&stroke_blur));
                        let stroke_rr = RoundedRect::from_rect(stroke_rect, stroke_radius);
                        scene.stroke(
                            &stroke_style,
                            shifted_transform,
                            to_peniko(stroke_blur.color),
                            None,
                            &stroke_rr,
                        );
                    }
                }
            } else {
                if let Some(fill) = fill.cloned().map(|f| f.with_opacity(weight)) {
                    fill_shape(scene, shifted_transform, &rect, x, y, width, height, &fill);
                }
                if let Some(stroke) = stroke {
                    if stroke.width > 0.0 {
                        let mut stroke_blur = stroke.clone();
                        stroke_blur.color.a = (stroke_blur.color.a * weight).clamp(0.0, 1.0);
                        let stroke_style = to_kurbo_stroke(&stroke_blur, 1.0);
                        let stroke_rect = stroke_aligned_rect(rect, Some(&stroke_blur));
                        scene.stroke(
                            &stroke_style,
                            shifted_transform,
                            to_peniko(stroke_blur.color),
                            None,
                            &stroke_rect,
                        );
                    }
                }
            }
        }
    }

    fn paint_blurred_path_content(
        scene: &mut impl anyrender::PaintScene,
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
                        fill_path_shape(scene, shifted, &path, x, y, width, height, &fill_paint);
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
                                &path,
                            );
                        }
                    }
                }
            }
        }
    }

    fn paint_blurred_svg_content(
        scene: &mut impl anyrender::PaintScene,
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
        for (dx, dy, weight) in layer_blur_kernel(radius) {
            let svg_transform =
                scene_transform * kurbo::Affine::translate((x + dx, y + dy)) * svg_fit;
            scene.push_layer(
                peniko::BlendMode {
                    mix: peniko::Mix::Normal,
                    compose: peniko::Compose::SrcOver,
                },
                weight as f32,
                scene_transform,
                &clip_rect,
            );
            let _ = anyrender_svg::render_svg_str(scene, svg_text, svg_transform);
            scene.pop_layer();
        }
    }

    fn fill_path_shape(
        scene: &mut impl anyrender::PaintScene,
        path_transform: kurbo::Affine,
        path: &BezPath,
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
                    path_transform,
                    to_peniko(color.clone()),
                    None,
                    path,
                );
            }
            FillPaint::GradientLinear { transform, stops } => {
                if let Some(gradient) =
                    build_linear_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, None, path);
                }
            }
            FillPaint::GradientRadial { transform, stops } => {
                if let Some((gradient, brush_xform)) =
                    build_radial_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, brush_xform, path);
                }
            }
            FillPaint::GradientAngular { transform, stops } => {
                if let Some(gradient) =
                    build_angular_gradient(x, y, width, height, transform.as_ref(), stops)
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
                if let Some(image_brush) = decode_image_brush(data_base64, *alpha) {
                    let iw = image_brush.image.width as f64;
                    let ih = image_brush.image.height as f64;
                    if iw > 0.0 && ih > 0.0 {
                        let brush_transform =
                            image_brush_transform(*scale_mode, image_transform, x, y, width, height, iw, ih);
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
        image_transform: &Option<[[f64; 3]; 2]>,
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
                // Figma imageTransform maps image-normalized [0,1]×[0,1] to
                // node-normalized [0,1]×[0,1]. The brush transform maps image
                // pixels → scene pixels, so the chain is:
                //   translate(node_origin) * scale(node_size) * T * scale(1/img_size)
                if let Some(t) = image_transform {
                    kurbo::Affine::translate((x, y))
                        * kurbo::Affine::scale_non_uniform(width, height)
                        * kurbo::Affine::new([
                            t[0][0], t[1][0], t[0][1], t[1][1], t[0][2], t[1][2],
                        ])
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
        let taps = ((radius / 6.0).round() as i32).clamp(6, 20) as usize;
        let spread = (radius * 0.35).max(0.35);
        let mut kernel = Vec::with_capacity(taps + 1);

        kernel.push((0.0, 0.0, 0.32));
        let edge_weight = 0.68 / taps as f64;
        for i in 0..taps {
            let t = i as f64 / taps as f64;
            let angle = std::f64::consts::TAU * t;
            let ring = if i % 2 == 0 { spread } else { spread * 0.72 };
            kernel.push((angle.cos() * ring, angle.sin() * ring, edge_weight));
        }
        kernel
    }

    fn build_linear_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&[[f64; 3]; 2]>,
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
        transform: Option<&[[f64; 3]; 2]>,
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
            let ax = (t[0][0] * width, t[1][0] * height);
            let ay = (t[0][1] * width, t[1][1] * height);
            let rx = (ax.0 * ax.0 + ax.1 * ax.1).sqrt() * 0.5;
            let ry = (ay.0 * ay.0 + ay.1 * ay.1).sqrt() * 0.5;
            let angle = ax.1.atan2(ax.0);

            // Unit circle gradient at origin; brush_transform maps to ellipse
            let gradient = peniko::Gradient::new_two_point_radial(
                Point::ZERO,
                0.0,
                Point::ZERO,
                1.0,
            )
            .with_stops(peniko_stops.as_slice());

            let brush_xform = kurbo::Affine::translate(center.to_vec2())
                * kurbo::Affine::rotate(angle)
                * kurbo::Affine::scale_non_uniform(rx.max(0.5), ry.max(0.5));

            Some((gradient, Some(brush_xform)))
        } else {
            let radius = (width.min(height) * 0.5).max(0.5) as f32;
            let gradient =
                peniko::Gradient::new_two_point_radial(center, 0.0, center, radius)
                    .with_stops(peniko_stops.as_slice());
            Some((gradient, None))
        }
    }

    fn build_angular_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&[[f64; 3]; 2]>,
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
            (t[1][0] * height).atan2(t[0][0] * width) as f32
        } else {
            0.0
        };
        let end_angle = start_angle + std::f32::consts::PI * 2.0;
        Some(
            peniko::Gradient::new_sweep(center, start_angle, end_angle)
                .with_stops(to_peniko_stops(stops).as_slice()),
        )
    }

    fn gradient_transform_point(transform: Option<&[[f64; 3]; 2]>, u: f64, v: f64) -> (f64, f64) {
        if let Some(t) = transform {
            let nx = t[0][0] * u + t[0][1] * v + t[0][2];
            let ny = t[1][0] * u + t[1][1] * v + t[1][2];
            (nx, ny)
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
            StrokeAlign::Center => base,
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

    fn decode_image_brush(data_base64: &str, alpha: f64) -> Option<ImageBrush> {
        use std::collections::HashMap;
        use std::hash::{Hash, Hasher};
        use std::sync::{LazyLock, Mutex};

        static CACHE: LazyLock<Mutex<HashMap<u64, ImageBrush>>> =
            LazyLock::new(|| Mutex::new(HashMap::new()));

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        data_base64.hash(&mut hasher);
        alpha.to_bits().hash(&mut hasher);
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
        let width = decoded.width();
        let height = decoded.height();

        let brush = ImageBrush {
            image: ImageData {
                data: Blob::new(Arc::new(decoded.into_vec())),
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
