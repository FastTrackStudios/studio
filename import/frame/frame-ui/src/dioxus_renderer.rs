use dioxus::prelude::*;
use frame_proto::{FrameDocument, NodeId, RenderNodeProjection};

#[component]
pub fn ProjectionTree(document: FrameDocument, root: NodeId) -> Element {
    let bounds = compute_bounds(&document, root);
    let width = bounds.width.max(1.0);
    let height = bounds.height.max(1.0);

    rsx! {
        div {
            class: "frame-projection-root",
            style: "position: relative; width: {width}px; height: {height}px; overflow: auto; background: #0a0a0a;",
            NodeLayer {
                document: document.clone(),
                node_id: root,
                offset_x: bounds.min_x,
                offset_y: bounds.min_y,
            }
        }
    }
}

#[component]
fn NodeLayer(document: FrameDocument, node_id: NodeId, offset_x: f64, offset_y: f64) -> Element {
    let Some(projection) = document.project_node(node_id) else {
        return rsx! {};
    };
    if !projection.visible {
        return rsx! {};
    }
    let Some(node) = document.get_node(node_id) else {
        return rsx! {};
    };

    let style = node_style(&projection, &node.raw, offset_x, offset_y);
    let text_label = projection
        .text
        .as_ref()
        .map(|t| t.characters.clone())
        .unwrap_or_default();
    let text_css = text_style(&node.raw);

    let is_document = projection.figma_type == "DOCUMENT";
    let is_canvas = projection.figma_type == "CANVAS";
    let is_text = projection.figma_type == "TEXT";

    let children = projection.children.clone();

    rsx! {
        div {
            class: "frame-node-layer",
            style: "{style}",
            if is_text && !text_label.is_empty() {
                div {
                    style: "{text_css}",
                    "{text_label}"
                }
            }
            if !(is_document || is_canvas) && !is_text && children.is_empty() && projection.name != "Unnamed" {
                div {
                    style: "position: absolute; left: 4px; top: 2px; color: rgba(255,255,255,0.45); font-size: 10px; white-space: nowrap;",
                    "{projection.name}"
                }
            }
            for child_id in children {
                NodeLayer {
                    document: document.clone(),
                    node_id: child_id,
                    offset_x,
                    offset_y,
                }
            }
        }
    }
}

fn node_style(
    projection: &RenderNodeProjection,
    raw: &serde_json::Value,
    offset_x: f64,
    offset_y: f64,
) -> String {
    if projection.figma_type == "DOCUMENT" {
        return "position: relative; width: 100%; height: 100%;".to_string();
    }

    let mut css = String::from("position: absolute; box-sizing: border-box;");
    let (x, y) = extract_xy(projection);
    let x = x - offset_x;
    let y = y - offset_y;
    css.push_str(&format!(" left: {x}px; top: {y}px;"));

    if let Some(size) = &projection.size {
        if size.x > 0.0 {
            css.push_str(&format!(" width: {}px;", size.x));
        }
        if size.y > 0.0 {
            css.push_str(&format!(" height: {}px;", size.y));
        }
    }

    if let Some(opacity) = projection.opacity {
        css.push_str(&format!(" opacity: {};", opacity.clamp(0.0, 1.0)));
    }

    if let Some(fill) = first_solid_rgba(raw, "fills") {
        css.push_str(&format!(" background: {fill};"));
    }

    if let Some(stroke) = first_solid_rgba(raw, "strokes") {
        let weight = raw
            .get("strokeWeight")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .max(0.5);
        css.push_str(&format!(" border: {weight}px solid {stroke};"));
    }

    if let Some(radius) = raw.get("cornerRadius").and_then(|v| v.as_f64()) {
        css.push_str(&format!(" border-radius: {radius}px;"));
    }

    if raw
        .get("clipsContent")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        css.push_str(" overflow: hidden;");
    }

    if projection.figma_type == "CANVAS" {
        css.push_str(" position: relative;");
        css.push_str(" left: 0; top: 0;");
        if let Some(bg) = raw.get("backgroundColor").and_then(rgba_from_color_value) {
            css.push_str(&format!(" background: {bg};"));
        }
    }

    css
}

fn text_style(raw: &serde_json::Value) -> String {
    let mut css =
        String::from("position: absolute; inset: 0; overflow: hidden; white-space: pre-wrap;");
    if let Some(color) = first_solid_rgba(raw, "fills") {
        css.push_str(&format!(" color: {color};"));
    } else {
        css.push_str(" color: rgba(255,255,255,0.92);");
    }

    if let Some(style) = raw.get("style") {
        if let Some(font_size) = style.get("fontSize").and_then(|v| v.as_f64()) {
            css.push_str(&format!(" font-size: {font_size}px;"));
        }
        if let Some(font_family) = style.get("fontFamily").and_then(|v| v.as_str()) {
            css.push_str(&format!(" font-family: '{font_family}', sans-serif;"));
        }
        if let Some(font_weight) = style.get("fontWeight").and_then(|v| v.as_u64()) {
            css.push_str(&format!(" font-weight: {font_weight};"));
        }
        if let Some(line_height) = style.get("lineHeightPx").and_then(|v| v.as_f64()) {
            css.push_str(&format!(" line-height: {line_height}px;"));
        }
        if let Some(letter_spacing) = style.get("letterSpacing").and_then(|v| v.as_f64()) {
            css.push_str(&format!(" letter-spacing: {letter_spacing}px;"));
        }
    }

    css
}

fn first_solid_rgba(raw: &serde_json::Value, key: &str) -> Option<String> {
    let fills = raw.get(key)?.as_array()?;
    for fill in fills {
        let fill_type = fill.get("type").and_then(|v| v.as_str());
        if fill_type != Some("SOLID") {
            continue;
        }
        let visible = fill
            .get("visible")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !visible {
            continue;
        }
        let opacity = fill.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0);
        if let Some(rgba) = fill.get("color").and_then(rgba_from_color_value) {
            return Some(replace_alpha(rgba, opacity));
        }
    }
    None
}

fn rgba_from_color_value(v: &serde_json::Value) -> Option<String> {
    let r = (v.get("r")?.as_f64()?.clamp(0.0, 1.0) * 255.0).round();
    let g = (v.get("g")?.as_f64()?.clamp(0.0, 1.0) * 255.0).round();
    let b = (v.get("b")?.as_f64()?.clamp(0.0, 1.0) * 255.0).round();
    let a = v
        .get("a")
        .and_then(|x| x.as_f64())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    Some(format!("rgba({r},{g},{b},{a})"))
}

fn replace_alpha(rgba: String, alpha: f64) -> String {
    let Some(prefix) = rgba.strip_suffix(")") else {
        return rgba;
    };
    let Some((rgb_prefix, _old_alpha)) = prefix.rsplit_once(',') else {
        return rgba;
    };
    format!("{rgb_prefix},{})", alpha.clamp(0.0, 1.0))
}

fn extract_xy(projection: &RenderNodeProjection) -> (f64, f64) {
    if let Some(t) = &projection.relative_transform {
        if t.len() >= 2 && t[0].len() >= 3 && t[1].len() >= 3 {
            return (t[0][2], t[1][2]);
        }
    }
    (0.0, 0.0)
}

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    width: f64,
    height: f64,
}

fn compute_bounds(document: &FrameDocument, root: NodeId) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for node_id in document.walk_subtree(root) {
        let Some(node) = document.get_node(node_id) else {
            continue;
        };
        let Some(bounds) = node.raw.get("absoluteBoundingBox") else {
            continue;
        };
        let x = bounds.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = bounds.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let w = bounds.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let h = bounds.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);

        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }

    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        if let Some(root_projection) = document.project_node(root) {
            if let Some(size) = root_projection.size {
                return Bounds {
                    min_x: 0.0,
                    min_y: 0.0,
                    width: size.x.max(1.0),
                    height: size.y.max(1.0),
                };
            }
        }
        return Bounds {
            min_x: 0.0,
            min_y: 0.0,
            width: 1024.0,
            height: 768.0,
        };
    }

    Bounds {
        min_x,
        min_y,
        width: (max_x - min_x).max(1.0),
        height: (max_y - min_y).max(1.0),
    }
}
