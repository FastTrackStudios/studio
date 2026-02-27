#[cfg(feature = "svg_export")]
pub(super) type StrokeSourceMap =
    std::collections::HashMap<String, (Vec<crate::anyrender_renderer::FillPaint>, Option<f64>)>;

#[cfg(feature = "svg_export")]
pub(super) fn collect_source_absolute_bboxes(
    bytes: &[u8],
) -> Result<std::collections::HashMap<String, (f64, f64, f64, f64)>, String> {
    fn walk(
        node: &serde_json::Value,
        out: &mut std::collections::HashMap<String, (f64, f64, f64, f64)>,
    ) {
        let id = node.get("id").and_then(|v| v.as_str());
        let bb = node.get("absoluteBoundingBox");
        if let (Some(id), Some(bb)) = (id, bb) {
            let x = bb.get("x").and_then(|v| v.as_f64());
            let y = bb.get("y").and_then(|v| v.as_f64());
            let w = bb.get("width").and_then(|v| v.as_f64());
            let h = bb.get("height").and_then(|v| v.as_f64());
            if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                out.insert(id.to_string(), (x, y, w.max(0.0), h.max(0.0)));
            }
        }
        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            for child in children {
                walk(child, out);
            }
        }
    }

    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid export JSON: {e}"))?;
    let mut out = std::collections::HashMap::new();
    if let Some(nodes) = json.get("nodes").and_then(|v| v.as_array()) {
        for node in nodes {
            walk(node, &mut out);
        }
    }
    Ok(out)
}

#[cfg(feature = "svg_export")]
pub(super) fn collect_source_stroke_data(bytes: &[u8]) -> Result<StrokeSourceMap, String> {
    fn walk(node: &serde_json::Value, out: &mut StrokeSourceMap) {
        if let Some(id) = node.get("id").and_then(|v| v.as_str()) {
            let paints = raw_stroke_paints(node);
            let stroke_weight = node.get("strokeWeight").and_then(|v| v.as_f64());
            if !paints.is_empty() || stroke_weight.is_some() {
                out.insert(id.to_string(), (paints, stroke_weight));
            }
        }
        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            for child in children {
                walk(child, out);
            }
        }
    }

    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid export JSON: {e}"))?;
    let mut out = std::collections::HashMap::new();
    if let Some(nodes) = json.get("nodes").and_then(|v| v.as_array()) {
        for node in nodes {
            walk(node, &mut out);
        }
    }
    Ok(out)
}

#[cfg(feature = "svg_export")]
pub(super) fn extract_export_root_name(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<String>, String> {
    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid export JSON: {e}"))?;
    let Some(nodes) = json.get("nodes").and_then(|n| n.as_array()) else {
        return Ok(None);
    };
    if nodes.is_empty() {
        return Ok(None);
    }
    let target = if let Some(root_id) = root_figma_id {
        nodes.iter()
            .find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
            .unwrap_or(&nodes[0])
    } else {
        &nodes[0]
    };
    Ok(target
        .get("name")
        .and_then(|v| v.as_str())
        .map(ToString::to_string))
}

#[cfg(feature = "svg_export")]
pub(super) fn extract_export_root_id(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<String>, String> {
    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid export JSON: {e}"))?;
    let Some(nodes) = json.get("nodes").and_then(|n| n.as_array()) else {
        return Ok(None);
    };
    if nodes.is_empty() {
        return Ok(None);
    }
    let target = if let Some(root_id) = root_figma_id {
        nodes.iter()
            .find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
            .unwrap_or(&nodes[0])
    } else {
        &nodes[0]
    };
    Ok(target
        .get("id")
        .and_then(|v| v.as_str())
        .map(ToString::to_string))
}

#[cfg(feature = "svg_export")]
fn raw_gradient_stops_from_paint(
    paint: &serde_json::Value,
) -> Option<Vec<crate::anyrender_renderer::GradientStop>> {
    let stops = paint.get("gradientStops")?.as_array()?;
    let paint_opacity = paint
        .get("opacity")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let mut out = Vec::new();
    for stop in stops {
        let color = stop.get("color")?;
        let r = color.get("r")?.as_f64()?.clamp(0.0, 1.0);
        let g = color.get("g")?.as_f64()?.clamp(0.0, 1.0);
        let b = color.get("b")?.as_f64()?.clamp(0.0, 1.0);
        let a = color
            .get("a")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
            * paint_opacity;
        let position = stop
            .get("position")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        out.push(crate::anyrender_renderer::GradientStop {
            offset: position,
            color: crate::anyrender_renderer::Rgba { r, g, b, a },
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(feature = "svg_export")]
fn raw_gradient_transform_from_paint(
    paint: &serde_json::Value,
) -> Option<crate::anyrender_renderer::AffineTransform2D> {
    let t = paint.get("gradientTransform")?.as_array()?;
    if t.len() < 2 {
        return None;
    }
    let r0 = t[0].as_array()?;
    let r1 = t[1].as_array()?;
    if r0.len() < 3 || r1.len() < 3 {
        return None;
    }
    Some(crate::anyrender_renderer::AffineTransform2D::from([
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
    ]))
}

#[cfg(feature = "svg_export")]
pub(super) fn raw_stroke_paints(
    node_raw: &serde_json::Value,
) -> Vec<crate::anyrender_renderer::FillPaint> {
    let Some(strokes) = node_raw.get("strokes").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for paint in strokes {
        let visible = paint
            .get("visible")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !visible {
            continue;
        }
        let paint_type = paint
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let parsed = match paint_type {
            "SOLID" => {
                let color = paint.get("color");
                color.and_then(|c| {
                    Some(crate::anyrender_renderer::FillPaint::Solid(
                        crate::anyrender_renderer::Rgba {
                            r: c.get("r")?.as_f64()?.clamp(0.0, 1.0),
                            g: c.get("g")?.as_f64()?.clamp(0.0, 1.0),
                            b: c.get("b")?.as_f64()?.clamp(0.0, 1.0),
                            a: c.get("a")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(1.0)
                                .clamp(0.0, 1.0)
                                * paint
                                    .get("opacity")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(1.0)
                                    .clamp(0.0, 1.0),
                        },
                    ))
                })
            }
            "GRADIENT_LINEAR" => raw_gradient_stops_from_paint(paint).map(|stops| {
                crate::anyrender_renderer::FillPaint::GradientLinear {
                    transform: raw_gradient_transform_from_paint(paint),
                    stops,
                }
            }),
            "GRADIENT_RADIAL" => raw_gradient_stops_from_paint(paint).map(|stops| {
                crate::anyrender_renderer::FillPaint::GradientRadial {
                    transform: raw_gradient_transform_from_paint(paint),
                    stops,
                }
            }),
            _ => None,
        };
        if let Some(fill) = parsed {
            out.push(fill);
        }
    }
    out
}
