use super::types::*;

/// Returns (x, y, width, height, rotation_radians).
///
/// Rotation is extracted from the relativeTransform matrix columns.
/// For a pure rotation: `[[cos θ, -sin θ, tx], [sin θ, cos θ, ty]]`.
pub(super) fn extract_local_rect(raw: &serde_json::Value) -> (f64, f64, f64, f64, f64) {
    // Prefer local node coordinates so rendering is stable regardless of how
    // upstream exporters populate absoluteBoundingBox (which may be global,
    // page-relative, or component-library-relative).
    let mut x = 0.0;
    let mut y = 0.0;
    let mut rotation = 0.0;
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
            // Extract rotation from matrix columns: atan2(c, a)
            let a = t[0]
                .as_array()
                .and_then(|row| row.get(0))
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);
            let c = t[1]
                .as_array()
                .and_then(|row| row.get(0))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            rotation = c.atan2(a);
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

    (x, y, w, h, rotation)
}

pub(super) fn first_solid_rgba(raw: &serde_json::Value, key: &str) -> Option<Rgba> {
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

pub(super) fn all_fill_paints(raw: &serde_json::Value, key: &str) -> Vec<(FillPaint, BlendMix)> {
    let Some(arr) = raw.get(key).and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|paint| {
            let visible = paint
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !visible {
                return None;
            }

            let blend = parse_blend_mode(paint);
            let paint_type = paint
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let fill = match paint_type {
                "SOLID" => solid_rgba_from_paint(paint).map(FillPaint::Solid),
                "GRADIENT_LINEAR" => gradient_stops_from_paint(paint).map(|stops| {
                    FillPaint::GradientLinear {
                        transform: gradient_transform_from_paint(paint),
                        stops,
                    }
                }),
                "GRADIENT_RADIAL" => gradient_stops_from_paint(paint).map(|stops| {
                    FillPaint::GradientRadial {
                        transform: gradient_transform_from_paint(paint),
                        stops,
                    }
                }),
                "GRADIENT_ANGULAR" => gradient_stops_from_paint(paint).map(|stops| {
                    FillPaint::GradientAngular {
                        transform: gradient_transform_from_paint(paint),
                        stops,
                    }
                }),
                "IMAGE" => image_fill_from_paint(raw, paint).map(|img| FillPaint::Image {
                    image_hash: img.image_hash,
                    data_base64: img.data_base64,
                    alpha: img.alpha,
                    scale_mode: img.scale_mode,
                    image_transform: img.image_transform,
                }),
                _ => None,
            };
            fill.map(|f| (f, blend))
        })
        .collect()
}

pub(super) fn image_fill_from_paint(
    raw: &serde_json::Value,
    paint: &serde_json::Value,
) -> Option<ImageFillData> {
    let image_hash = paint
        .get("imageHash")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    let image_fill_assets = raw.get("imageFillAssets").and_then(|v| v.as_object())?;
    let asset_value = image_hash
        .as_ref()
        .and_then(|hash| image_fill_assets.get(hash))?;
    // Support both v1 flat string and v2 nested object with "base64" field
    let data_base64 = asset_value
        .as_str()
        .or_else(|| asset_value.get("base64").and_then(|v| v.as_str()))?
        .to_string();

    let alpha = paint
        .get("opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);

    let scale_mode = match paint.get("scaleMode").and_then(|v| v.as_str()) {
        Some("FILL") => ImageScaleMode::Fill,
        Some("FIT") => ImageScaleMode::Fit,
        Some("CROP") => ImageScaleMode::Crop,
        Some("TILE") => ImageScaleMode::Tile,
        Some("STRETCH") => ImageScaleMode::Stretch,
        _ => ImageScaleMode::Fill, // Figma default
    };

    let image_transform = parse_image_transform(paint);

    Some(ImageFillData {
        image_hash,
        data_base64,
        alpha,
        scale_mode,
        image_transform,
    })
}

pub(super) struct ImageFillData {
    pub image_hash: Option<String>,
    pub data_base64: String,
    pub alpha: f64,
    pub scale_mode: ImageScaleMode,
    pub image_transform: Option<[[f64; 3]; 2]>,
}

fn parse_image_transform(paint: &serde_json::Value) -> Option<[[f64; 3]; 2]> {
    let t = paint.get("imageTransform")?.as_array()?;
    if t.len() < 2 {
        return None;
    }
    let r0 = t[0].as_array()?;
    let r1 = t[1].as_array()?;
    if r0.len() < 3 || r1.len() < 3 {
        return None;
    }
    Some([
        [
            r0[0].as_f64().unwrap_or(1.0),
            r0[1].as_f64().unwrap_or(0.0),
            r0[2].as_f64().unwrap_or(0.0),
        ],
        [
            r1[0].as_f64().unwrap_or(0.0),
            r1[1].as_f64().unwrap_or(1.0),
            r1[2].as_f64().unwrap_or(0.0),
        ],
    ])
}

pub(super) fn parse_effects(raw: &serde_json::Value, opacity: f64) -> Vec<NodeEffect> {
    let Some(effects) = raw.get("effects").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    effects
        .iter()
        .filter_map(|effect| {
            let visible = effect
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !visible {
                return None;
            }

            let kind = match effect.get("type").and_then(|v| v.as_str()) {
                Some("DROP_SHADOW") => EffectKind::DropShadow,
                Some("INNER_SHADOW") => EffectKind::InnerShadow,
                Some("LAYER_BLUR") => EffectKind::LayerBlur,
                Some("BACKGROUND_BLUR") => EffectKind::BackgroundBlur,
                _ => return None,
            };

            let color = solid_rgba_from_paint(effect).unwrap_or(Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.25,
            });

            let offset_x = effect
                .get("offset")
                .and_then(|v| v.get("x"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let offset_y = effect
                .get("offset")
                .and_then(|v| v.get("y"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let radius = effect
                .get("radius")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .max(0.0);
            let spread = effect
                .get("spread")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .max(0.0);

            Some(NodeEffect {
                kind,
                color: color.with_opacity(opacity),
                offset_x,
                offset_y,
                radius,
                spread,
            })
        })
        .collect()
}

pub(super) fn parse_stroke_style(raw: &serde_json::Value, opacity: f64) -> Option<StrokeStyle> {
    let color = first_solid_rgba(raw, "strokes")?.with_opacity(opacity);
    let width = raw
        .get("strokeWeight")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .max(0.0);
    if width <= 0.0 {
        return None;
    }

    let cap = get_typed::<StrokeCap>(raw, "strokeCap").unwrap_or(StrokeCap::None);
    let join = get_typed::<StrokeJoin>(raw, "strokeJoin").unwrap_or(StrokeJoin::Miter);
    let align = get_typed::<StrokeAlign>(raw, "strokeAlign").unwrap_or(StrokeAlign::Center);
    let miter_limit = raw
        .get("strokeMiterLimit")
        .and_then(|v| v.as_f64())
        .unwrap_or(4.0)
        .max(1.0);
    let dash_pattern = raw
        .get("dashPattern")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_f64())
                .filter(|v| *v > 0.0)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let dash_offset = raw
        .get("strokeDashes")
        .and_then(|v| v.get("offset"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    Some(StrokeStyle {
        color,
        width,
        cap,
        join,
        align,
        miter_limit,
        dash_pattern,
        dash_offset,
    })
}

pub(super) fn parse_blend_mode(raw: &serde_json::Value) -> BlendMix {
    match raw.get("blendMode").and_then(|v| v.as_str()) {
        Some("MULTIPLY") => BlendMix::Multiply,
        Some("SCREEN") => BlendMix::Screen,
        Some("OVERLAY") => BlendMix::Overlay,
        Some("DARKEN") => BlendMix::Darken,
        Some("LIGHTEN") => BlendMix::Lighten,
        Some("COLOR_DODGE") => BlendMix::ColorDodge,
        Some("COLOR_BURN") => BlendMix::ColorBurn,
        Some("HARD_LIGHT") => BlendMix::HardLight,
        Some("SOFT_LIGHT") => BlendMix::SoftLight,
        Some("DIFFERENCE") => BlendMix::Difference,
        Some("EXCLUSION") => BlendMix::Exclusion,
        Some("HUE") => BlendMix::Hue,
        Some("SATURATION") => BlendMix::Saturation,
        Some("COLOR") => BlendMix::Color,
        Some("LUMINOSITY") => BlendMix::Luminosity,
        _ => BlendMix::Normal,
    }
}

fn get_typed<T>(raw: &serde_json::Value, key: &str) -> Option<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let value = raw.get(key)?;
    serde_json::from_value(value.clone()).ok()
}

pub(super) fn solid_rgba_from_paint(paint: &serde_json::Value) -> Option<Rgba> {
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
    Some(Rgba {
        r,
        g,
        b,
        a: (a * opacity).clamp(0.0, 1.0),
    })
}

fn gradient_stops_from_paint(paint: &serde_json::Value) -> Option<Vec<GradientStop>> {
    let stops = paint.get("gradientStops")?.as_array()?;
    let mut out = Vec::new();
    for stop in stops {
        let color_value = stop.get("color")?;
        let position = stop
            .get("position")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let color = solid_rgba_from_paint(&serde_json::json!({ "color": color_value }))?;
        out.push(GradientStop {
            offset: position,
            color,
        });
    }
    if out.is_empty() { None } else { Some(out) }
}

fn gradient_transform_from_paint(paint: &serde_json::Value) -> Option<[[f64; 3]; 2]> {
    let t = paint.get("gradientTransform")?.as_array()?;
    if t.len() < 2 {
        return None;
    }
    let r0 = t[0].as_array()?;
    let r1 = t[1].as_array()?;
    if r0.len() < 3 || r1.len() < 3 {
        return None;
    }
    Some([
        [
            r0[0].as_f64().unwrap_or(1.0),
            r0[1].as_f64().unwrap_or(0.0),
            r0[2].as_f64().unwrap_or(0.0),
        ],
        [
            r1[0].as_f64().unwrap_or(0.0),
            r1[1].as_f64().unwrap_or(1.0),
            r1[2].as_f64().unwrap_or(0.0),
        ],
    ])
}

pub(super) fn first_color_from_fill(fill: &FillPaint) -> Option<Rgba> {
    match fill {
        FillPaint::Solid(c) => Some(c.clone()),
        FillPaint::GradientLinear { stops, .. }
        | FillPaint::GradientRadial { stops, .. }
        | FillPaint::GradientAngular { stops, .. } => stops.first().map(|s| s.color.clone()),
        FillPaint::Image { .. } => None,
    }
}

pub(super) fn path_strings(raw: &serde_json::Value, key: &str) -> Vec<String> {
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

pub(super) fn exported_svg_base64(raw: &serde_json::Value) -> Option<String> {
    raw.get("exports")
        .and_then(|v| v.get("svgBase64"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}
