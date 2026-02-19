#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("render_svg requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(err) = run() {
        eprintln!("render_svg error: {err}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use frame_proto::FrameDocument;
    use frame_ui::{build_layout_boxes, build_paint_primitives, PaintPrimitive};
    use svg::node::element::{ClipPath, Definitions, Group, Path as SvgPath, Rectangle};
    use svg::Document;

    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| "usage: render_svg <input_json> --out <output_svg> [--root FIGMA_ID] [--width PX] [--height PX]".to_string())?;

    let mut out_path: Option<String> = None;
    let mut root_figma_id: Option<String> = None;
    let mut width_override: Option<u32> = None;
    let mut height_override: Option<u32> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out_path = args.next(),
            "--root" => root_figma_id = args.next(),
            "--width" => {
                width_override = Some(
                    args.next()
                        .ok_or_else(|| "--width requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --width value: {e}"))?,
                )
            }
            "--height" => {
                height_override = Some(
                    args.next()
                        .ok_or_else(|| "--height requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --height value: {e}"))?,
                )
            }
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    let out_path = out_path.ok_or_else(|| "--out <output_svg> is required".to_string())?;
    let bytes = std::fs::read(&input).map_err(|e| format!("failed reading {input}: {e}"))?;
    let export_bbox = extract_export_bbox(&bytes, root_figma_id.as_deref())?;
    let base_color =
        extract_export_base_color(&bytes, root_figma_id.as_deref())?.unwrap_or((255, 255, 255, 255));
    let doc = FrameDocument::from_source_bytes(&bytes)
        .map_err(|e| format!("failed importing source document from {input}: {e}"))?;
    let root = match root_figma_id {
        Some(figma_id) => doc
            .get_by_figma_id(&figma_id)
            .map(|n| n.id)
            .ok_or_else(|| format!("root figma id not found: {figma_id}"))?,
        None => doc.pages.first().copied().unwrap_or(doc.root),
    };

    let layout_boxes = build_layout_boxes(&doc, root);
    let subtree = doc.walk_subtree(root);

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for node_id in subtree {
        if let Some(b) = layout_boxes.get(&node_id).copied() {
            if b.width <= 0.0 || b.height <= 0.0 {
                continue;
            }
            min_x = min_x.min(b.x);
            min_y = min_y.min(b.y);
            max_x = max_x.max(b.x + b.width);
            max_y = max_y.max(b.y + b.height);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        if let Some((x, y, w, h)) = export_bbox {
            min_x = x;
            min_y = y;
            max_x = x + w;
            max_y = y + h;
        } else {
            min_x = 0.0;
            min_y = 0.0;
            max_x = 1.0;
            max_y = 1.0;
        }
    }

    let (bbox_width, bbox_height) = export_bbox
        .map(|(_, _, w, h)| (w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32))
        .unwrap_or((1, 1));
    let natural_width = (max_x - min_x).ceil().max(1.0) as u32;
    let natural_height = (max_y - min_y).ceil().max(1.0) as u32;
    let width = width_override.unwrap_or(natural_width).clamp(1, 8192);
    let height = height_override.unwrap_or(natural_height).clamp(1, 8192);
    let width = if width_override.is_some() {
        width
    } else {
        width.max(bbox_width)
    };
    let height = if height_override.is_some() {
        height
    } else {
        height.max(bbox_height)
    };

    let primitives = build_paint_primitives(&doc, root);

    let mut svg_doc = Document::new()
        .set("width", width)
        .set("height", height)
        .set("viewBox", format!("0 0 {width} {height}"))
        .set("fill", "none");

    let mut bg = Rectangle::new().set("width", width).set("height", height);
    let (bg_hex, bg_opacity) = svg_color_paint_values(base_color.0, base_color.1, base_color.2, base_color.3);
    bg = bg.set("fill", bg_hex);
    if let Some(opacity) = bg_opacity {
        bg = bg.set("fill-opacity", opacity);
    }
    push_node(&mut svg_doc, &mut Vec::new(), bg);

    let tx = -min_x;
    let ty = -min_y;
    let mut group_stack: Vec<Group> = Vec::new();
    let mut clip_count: u64 = 0;

    for primitive in primitives {
        match primitive {
            PaintPrimitive::LayerStart { opacity, .. } => {
                group_stack.push(Group::new().set("opacity", fmt_f64(opacity.clamp(0.0, 1.0))));
            }
            PaintPrimitive::LayerEnd { .. } => {
                if let Some(group) = group_stack.pop() {
                    push_node(&mut svg_doc, &mut group_stack, group);
                }
            }
            PaintPrimitive::ClipStart {
                x,
                y,
                width,
                height,
                ..
            } => {
                clip_count += 1;
                let id = format!("clip{clip_count}");
                let clip_rect = Rectangle::new()
                    .set("x", fmt_f64(x + tx))
                    .set("y", fmt_f64(y + ty))
                    .set("width", fmt_f64(width.max(0.0)))
                    .set("height", fmt_f64(height.max(0.0)));
                let defs = Definitions::new().add(ClipPath::new().set("id", id.clone()).add(clip_rect));
                push_node(&mut svg_doc, &mut group_stack, defs);
                group_stack.push(Group::new().set("clip-path", format!("url(#{id})")));
            }
            PaintPrimitive::ClipEnd { .. } => {
                if let Some(group) = group_stack.pop() {
                    push_node(&mut svg_doc, &mut group_stack, group);
                }
            }
            PaintPrimitive::Rect {
                x,
                y,
                width,
                height,
                fills,
                stroke,
                ..
            } => {
                for (fill, _) in fills {
                    if let Some(style) = svg_fill_style(&fill) {
                        let mut rect = Rectangle::new()
                            .set("x", fmt_f64(x + tx))
                            .set("y", fmt_f64(y + ty))
                            .set("width", fmt_f64(width.max(0.0)))
                            .set("height", fmt_f64(height.max(0.0)));
                        rect = apply_fill_style_rect(rect, &style);
                        push_node(&mut svg_doc, &mut group_stack, rect);
                    }
                }
                if let Some(stroke) = stroke {
                    let mut rect = Rectangle::new()
                        .set("x", fmt_f64(x + tx))
                        .set("y", fmt_f64(y + ty))
                        .set("width", fmt_f64(width.max(0.0)))
                        .set("height", fmt_f64(height.max(0.0)))
                        .set("fill", "none")
                        .set("stroke-width", fmt_f64(stroke.width.max(0.0)))
                        .set("stroke-linecap", svg_linecap(stroke.cap))
                        .set("stroke-linejoin", svg_linejoin(stroke.join));
                    let (hex, opacity) = svg_color_paint_values(
                        to_u8(stroke.color.r),
                        to_u8(stroke.color.g),
                        to_u8(stroke.color.b),
                        (stroke.color.a * 255.0).round().clamp(0.0, 255.0) as u8,
                    );
                    rect = rect.set("stroke", hex);
                    if let Some(opacity) = opacity {
                        rect = rect.set("stroke-opacity", opacity);
                    }
                    push_node(&mut svg_doc, &mut group_stack, rect);
                }
            }
            PaintPrimitive::Path {
                x,
                y,
                fill_paths,
                stroke_paths,
                fills,
                stroke,
                text_outline: _text_outline,
                ..
            } => {
                let px = x + tx;
                let py = y + ty;
                let transform = format!("translate({},{})", fmt_f64(px), fmt_f64(py));

                for (fill, _) in fills {
                    if let Some(style) = svg_fill_style(&fill) {
                        for d in &fill_paths {
                            let mut path = SvgPath::new()
                                .set("d", d.as_str())
                                .set("transform", transform.clone());
                            path = apply_fill_style_path(path, &style);
                            push_node(&mut svg_doc, &mut group_stack, path);
                        }
                    }
                }

                if let Some(stroke) = stroke {
                    let (hex, opacity) = svg_color_paint_values(
                        to_u8(stroke.color.r),
                        to_u8(stroke.color.g),
                        to_u8(stroke.color.b),
                        (stroke.color.a * 255.0).round().clamp(0.0, 255.0) as u8,
                    );
                    for d in &stroke_paths {
                        let mut path = SvgPath::new()
                            .set("d", d.as_str())
                            .set("transform", transform.clone())
                            .set("fill", "none")
                            .set("stroke", hex.clone())
                            .set("stroke-width", fmt_f64(stroke.width.max(0.0)))
                            .set("stroke-linecap", svg_linecap(stroke.cap))
                            .set("stroke-linejoin", svg_linejoin(stroke.join));
                        if let Some(opacity) = &opacity {
                            path = path.set("stroke-opacity", opacity.clone());
                        }
                        push_node(&mut svg_doc, &mut group_stack, path);
                    }
                }
            }
            // Figma parity SVG exports are path-based for text.
            PaintPrimitive::Text { .. } => {}
        }
    }

    while let Some(group) = group_stack.pop() {
        push_node(&mut svg_doc, &mut group_stack, group);
    }

    let raw_svg = svg_doc.to_string();
    let out_svg = normalize_svg_for_figma_parity(&raw_svg, width, height);

    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed creating output dir {}: {e}", parent.display()))?;
    }
    std::fs::write(&out_path, out_svg).map_err(|e| format!("failed writing SVG {out_path}: {e}"))?;

    println!(
        "{{\"ok\":true,\"input\":\"{}\",\"out\":\"{}\",\"width\":{},\"height\":{}}}",
        input, out_path, width, height
    );
    Ok(())
}

#[cfg(feature = "anyrender")]
#[derive(Clone)]
enum FillStyle {
    Solid { hex: String, opacity: Option<String> },
    DebugStroke { hex: &'static str },
}

#[cfg(feature = "anyrender")]
fn apply_fill_style_rect(mut rect: svg::node::element::Rectangle, style: &FillStyle) -> svg::node::element::Rectangle {
    match style {
        FillStyle::Solid { hex, opacity } => {
            rect = rect.set("fill", hex.clone());
            if let Some(opacity) = opacity {
                rect = rect.set("fill-opacity", opacity.clone());
            }
            rect
        }
        FillStyle::DebugStroke { hex } => rect.set("fill", "none").set("stroke", *hex).set("stroke-width", 1),
    }
}

#[cfg(feature = "anyrender")]
fn apply_fill_style_path(mut path: svg::node::element::Path, style: &FillStyle) -> svg::node::element::Path {
    match style {
        FillStyle::Solid { hex, opacity } => {
            path = path.set("fill", hex.clone());
            if let Some(opacity) = opacity {
                path = path.set("fill-opacity", opacity.clone());
            }
            path
        }
        FillStyle::DebugStroke { hex } => path.set("fill", "none").set("stroke", *hex).set("stroke-width", 1),
    }
}

#[cfg(feature = "anyrender")]
fn svg_fill_style(fill: &frame_ui::anyrender_renderer::FillPaint) -> Option<FillStyle> {
    use frame_ui::anyrender_renderer::FillPaint;
    match fill {
        FillPaint::Solid(c) => {
            let (hex, opacity) = svg_color_paint_values(
                to_u8(c.r),
                to_u8(c.g),
                to_u8(c.b),
                (c.a * 255.0).round().clamp(0.0, 255.0) as u8,
            );
            Some(FillStyle::Solid { hex, opacity })
        }
        FillPaint::GradientLinear { .. }
        | FillPaint::GradientRadial { .. }
        | FillPaint::GradientAngular { .. } => Some(FillStyle::DebugStroke { hex: "#FF00FF" }),
        FillPaint::Image { .. } => Some(FillStyle::DebugStroke { hex: "#00A3FF" }),
    }
}

#[cfg(feature = "anyrender")]
fn push_node<N: svg::Node>(doc: &mut svg::Document, groups: &mut Vec<svg::node::element::Group>, node: N) {
    if let Some(group) = groups.pop() {
        groups.push(group.add(node));
    } else {
        let current = std::mem::replace(doc, svg::Document::new());
        *doc = current.add(node);
    }
}

#[cfg(feature = "anyrender")]
fn to_u8(v: f64) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(feature = "anyrender")]
fn svg_color_paint_values(r: u8, g: u8, b: u8, a: u8) -> (String, Option<String>) {
    let hex = format!("#{r:02X}{g:02X}{b:02X}");
    if a >= 255 {
        (hex, None)
    } else {
        (hex, Some(format!("{:.6}", f64::from(a) / 255.0)))
    }
}

#[cfg(feature = "anyrender")]
fn svg_linecap(cap: frame_ui::anyrender_renderer::StrokeCap) -> &'static str {
    match cap {
        frame_ui::anyrender_renderer::StrokeCap::Round => "round",
        frame_ui::anyrender_renderer::StrokeCap::Square => "square",
        _ => "butt",
    }
}

#[cfg(feature = "anyrender")]
fn svg_linejoin(join: frame_ui::anyrender_renderer::StrokeJoin) -> &'static str {
    match join {
        frame_ui::anyrender_renderer::StrokeJoin::Round => "round",
        frame_ui::anyrender_renderer::StrokeJoin::Bevel => "bevel",
        _ => "miter",
    }
}

#[cfg(feature = "anyrender")]
fn fmt_f64(v: f64) -> String {
    format!("{:.3}", v)
}

#[cfg(feature = "anyrender")]
fn normalize_svg_for_figma_parity(svg: &str, width: u32, height: u32) -> String {
    let mut out = Vec::new();
    let root = format!(
        r#"<svg width="{width}" height="{height}" viewBox="0 0 {width} {height}" fill="none" xmlns="http://www.w3.org/2000/svg">"#
    );
    out.push(root);

    let mut saw_root = false;
    for line in svg.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("<svg ") {
            saw_root = true;
            continue;
        }
        if t == "</svg>" {
            continue;
        }
        if let Some(norm) = normalize_rect_line(t) {
            out.push(norm);
            continue;
        }
        if let Some(norm) = path_rect_to_figma_rect(t) {
            out.push(norm);
            continue;
        }
        if let Some(norm) = normalize_translated_path_line(t) {
            out.push(norm);
            continue;
        }
        // Figma often emits keyword black for solid text/shape paths.
        out.push(t.replace("fill=\"#000000\"", "fill=\"black\""));
    }
    if !saw_root {
        // fallback: maintain well-formed output even if parser changes
        out.push(format!(r##"<rect width="{width}" height="{height}" fill="#FFFFFF"/>"##));
    }
    out.push("</svg>".to_string());
    out.join("\n") + "\n"
}

#[cfg(feature = "anyrender")]
fn normalize_rect_line(line: &str) -> Option<String> {
    if !line.starts_with("<rect ") {
        return None;
    }
    let x = extract_xml_attr(line, "x");
    let y = extract_xml_attr(line, "y");
    let w = extract_xml_attr(line, "width")?;
    let h = extract_xml_attr(line, "height")?;
    let fill = extract_xml_attr(line, "fill");
    let fill_opacity = extract_xml_attr(line, "fill-opacity");
    let stroke = extract_xml_attr(line, "stroke");
    let stroke_opacity = extract_xml_attr(line, "stroke-opacity");
    let stroke_width = extract_xml_attr(line, "stroke-width");
    let linecap = extract_xml_attr(line, "stroke-linecap");
    let linejoin = extract_xml_attr(line, "stroke-linejoin");

    let mut attrs: Vec<String> = Vec::new();
    if let Some(x) = x {
        attrs.push(format!(r#"x="{}""#, trim_decimal(&x)));
    }
    if let Some(y) = y {
        attrs.push(format!(r#"y="{}""#, trim_decimal(&y)));
    }
    attrs.push(format!(r#"width="{}""#, trim_decimal(&w)));
    attrs.push(format!(r#"height="{}""#, trim_decimal(&h)));
    if let Some(mut fill) = fill {
        if fill == "#000000" {
            fill = "black".to_string();
        }
        attrs.push(format!(r#"fill="{fill}""#));
    }
    if let Some(opacity) = fill_opacity {
        attrs.push(format!(r#"fill-opacity="{}""#, trim_decimal(&opacity)));
    }
    if let Some(stroke) = stroke {
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(opacity) = stroke_opacity {
        attrs.push(format!(r#"stroke-opacity="{}""#, trim_decimal(&opacity)));
    }
    if let Some(stroke_width) = stroke_width {
        attrs.push(format!(r#"stroke-width="{}""#, trim_decimal(&stroke_width)));
    }
    if let Some(linecap) = linecap {
        attrs.push(format!(r#"stroke-linecap="{linecap}""#));
    }
    if let Some(linejoin) = linejoin {
        attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
    }
    Some(format!("<rect {}/>", attrs.join(" ")))
}

#[cfg(feature = "anyrender")]
fn path_rect_to_figma_rect(line: &str) -> Option<String> {
    if !line.starts_with("<path ") {
        return None;
    }
    let d = extract_xml_attr(line, "d")?;
    let transform = extract_xml_attr(line, "transform")?;
    let fill = extract_xml_attr(line, "fill")?;
    let fill_opacity = extract_xml_attr(line, "fill-opacity");
    if extract_xml_attr(line, "stroke").is_some() {
        return None;
    }
    let (w, h) = parse_axis_aligned_rect_d(&d)?;
    let (tx, ty) = parse_translate(&transform)?;
    let mut fill_out = fill;
    if fill_out == "#000000" {
        fill_out = "black".to_string();
    }
    let mut attrs = vec![
        format!(r#"x="{}""#, trim_decimal(&format!("{tx}"))),
        format!(r#"y="{}""#, trim_decimal(&format!("{ty}"))),
        format!(r#"width="{}""#, trim_decimal(&format!("{w}"))),
        format!(r#"height="{}""#, trim_decimal(&format!("{h}"))),
        format!(r#"fill="{fill_out}""#),
    ];
    if let Some(opacity) = fill_opacity {
        attrs.push(format!(r#"fill-opacity="{}""#, trim_decimal(&opacity)));
    }
    Some(format!("<rect {}/>", attrs.join(" ")))
}

#[cfg(feature = "anyrender")]
fn parse_axis_aligned_rect_d(d: &str) -> Option<(f64, f64)> {
    let norm = d
        .replace('M', " M ")
        .replace('L', " L ")
        .replace('Z', " Z ")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    if norm.len() != 16 {
        return None;
    }
    if norm[0] != "M" || norm[3] != "L" || norm[6] != "L" || norm[9] != "L" || norm[12] != "L" || norm[15] != "Z" {
        return None;
    }
    let x0 = norm[1].parse::<f64>().ok()?;
    let y0 = norm[2].parse::<f64>().ok()?;
    let x1 = norm[4].parse::<f64>().ok()?;
    let y1 = norm[5].parse::<f64>().ok()?;
    let x2 = norm[7].parse::<f64>().ok()?;
    let y2 = norm[8].parse::<f64>().ok()?;
    let x3 = norm[10].parse::<f64>().ok()?;
    let y3 = norm[11].parse::<f64>().ok()?;
    let x4 = norm[13].parse::<f64>().ok()?;
    let y4 = norm[14].parse::<f64>().ok()?;
    if x0 != 0.0 || y0 != 0.0 || y1 != 0.0 || x3 != 0.0 || x4 != 0.0 || y4 != 0.0 {
        return None;
    }
    if (x1 - x2).abs() > 1e-6 || (y2 - y3).abs() > 1e-6 {
        return None;
    }
    Some((x1, y2))
}

#[cfg(feature = "anyrender")]
fn parse_translate(transform: &str) -> Option<(f64, f64)> {
    let raw = transform.strip_prefix("translate(")?.strip_suffix(')')?;
    let mut parts = raw.split(',');
    let x = parts.next()?.trim().parse::<f64>().ok()?;
    let y = parts.next()?.trim().parse::<f64>().ok()?;
    Some((x, y))
}

#[cfg(feature = "anyrender")]
fn extract_xml_attr(line: &str, key: &str) -> Option<String> {
    let needle = format!(r#"{key}=""#);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(feature = "anyrender")]
fn trim_decimal(v: &str) -> String {
    if let Ok(num) = v.parse::<f64>() {
        if (num - num.round()).abs() < 1e-9 {
            return format!("{}", num.round() as i64);
        }
        let s = format!("{num:.6}");
        return s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    v.to_string()
}

#[cfg(feature = "anyrender")]
fn normalize_translated_path_line(line: &str) -> Option<String> {
    if !line.starts_with("<path ") {
        return None;
    }
    let d = extract_xml_attr(line, "d")?;
    let transform = extract_xml_attr(line, "transform")?;
    let (tx, ty) = parse_translate(&transform)?;
    let translated = translate_and_compact_path_data(&d, tx, ty)?;

    let mut attrs = vec![format!(r#"d="{translated}""#)];

    if let Some(fill) = extract_xml_attr(line, "fill") {
        let fill = if fill == "#000000" {
            "black".to_string()
        } else {
            fill
        };
        attrs.push(format!(r#"fill="{fill}""#));
    }
    if let Some(fill_opacity) = extract_xml_attr(line, "fill-opacity") {
        attrs.push(format!(r#"fill-opacity="{}""#, trim_decimal(&fill_opacity)));
    }
    if let Some(stroke) = extract_xml_attr(line, "stroke") {
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(stroke_opacity) = extract_xml_attr(line, "stroke-opacity") {
        attrs.push(format!(r#"stroke-opacity="{}""#, trim_decimal(&stroke_opacity)));
    }
    if let Some(stroke_width) = extract_xml_attr(line, "stroke-width") {
        attrs.push(format!(r#"stroke-width="{}""#, trim_decimal(&stroke_width)));
    }
    if let Some(linecap) = extract_xml_attr(line, "stroke-linecap") {
        attrs.push(format!(r#"stroke-linecap="{linecap}""#));
    }
    if let Some(linejoin) = extract_xml_attr(line, "stroke-linejoin") {
        attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
    }
    if let Some(clip_path) = extract_xml_attr(line, "clip-path") {
        attrs.push(format!(r#"clip-path="{clip_path}""#));
    }

    Some(format!("<path {}/>", attrs.join(" ")))
}

#[cfg(feature = "anyrender")]
fn translate_and_compact_path_data(d: &str, tx: f64, ty: f64) -> Option<String> {
    let mut tokens = Vec::<String>::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() {
            tokens.push(c.to_string());
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < chars.len() {
            let ch = chars[i];
            if ch.is_ascii_alphabetic() || ch.is_ascii_whitespace() || ch == ',' {
                break;
            }
            i += 1;
        }
        tokens.push(chars[start..i].iter().collect::<String>());
    }

    let mut out = String::new();
    let mut idx = 0usize;
    let mut cur = (0.0f64, 0.0f64);
    let mut sub_start = (0.0f64, 0.0f64);
    let mut first = true;

    while idx < tokens.len() {
        let cmd = tokens.get(idx)?.chars().next()?;
        if !cmd.is_ascii_alphabetic() {
            return None;
        }
        idx += 1;
        let next_is_cmd = |pos: usize| -> bool {
            tokens
                .get(pos)
                .and_then(|t| t.chars().next())
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
        };
        let mut nums = Vec::<f64>::new();
        while idx < tokens.len() && !next_is_cmd(idx) {
            nums.push(tokens[idx].parse::<f64>().ok()?);
            idx += 1;
        }

        let emit = |ch: char, values: &[f64], out: &mut String, first: &mut bool| {
            if *first {
                *first = false;
            }
            out.push(ch);
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&fmt_path_num(*value));
            }
        };

        match cmd {
            'M' => {
                if nums.len() < 2 || nums.len() % 2 != 0 {
                    return None;
                }
                let mut first_pair = true;
                for pair in nums.chunks_exact(2) {
                    let x = pair[0] + tx;
                    let y = pair[1] + ty;
                    if first_pair {
                        emit('M', &[x, y], &mut out, &mut first);
                        sub_start = (x, y);
                        first_pair = false;
                    } else if (x - cur.0).abs() < 1e-6 {
                        emit('V', &[y], &mut out, &mut first);
                    } else if (y - cur.1).abs() < 1e-6 {
                        emit('H', &[x], &mut out, &mut first);
                    } else {
                        emit('L', &[x, y], &mut out, &mut first);
                    }
                    cur = (x, y);
                }
            }
            'L' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                for pair in nums.chunks_exact(2) {
                    let x = pair[0] + tx;
                    let y = pair[1] + ty;
                    if (x - cur.0).abs() < 1e-6 {
                        emit('V', &[y], &mut out, &mut first);
                    } else if (y - cur.1).abs() < 1e-6 {
                        emit('H', &[x], &mut out, &mut first);
                    } else {
                        emit('L', &[x, y], &mut out, &mut first);
                    }
                    cur = (x, y);
                }
            }
            'C' => {
                if nums.len() % 6 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(6) {
                    let vals = [
                        seg[0] + tx,
                        seg[1] + ty,
                        seg[2] + tx,
                        seg[3] + ty,
                        seg[4] + tx,
                        seg[5] + ty,
                    ];
                    emit('C', &vals, &mut out, &mut first);
                    cur = (vals[4], vals[5]);
                }
            }
            'Q' | 'S' => {
                if nums.len() % 4 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(4) {
                    let vals = [seg[0] + tx, seg[1] + ty, seg[2] + tx, seg[3] + ty];
                    emit(cmd, &vals, &mut out, &mut first);
                    cur = (vals[2], vals[3]);
                }
            }
            'T' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(2) {
                    let vals = [seg[0] + tx, seg[1] + ty];
                    emit('T', &vals, &mut out, &mut first);
                    cur = (vals[0], vals[1]);
                }
            }
            'A' => {
                if nums.len() % 7 != 0 {
                    return None;
                }
                for seg in nums.chunks_exact(7) {
                    let vals = [
                        seg[0],
                        seg[1],
                        seg[2],
                        seg[3],
                        seg[4],
                        seg[5] + tx,
                        seg[6] + ty,
                    ];
                    emit('A', &vals, &mut out, &mut first);
                    cur = (vals[5], vals[6]);
                }
            }
            'H' => {
                for x in nums {
                    let x = x + tx;
                    emit('H', &[x], &mut out, &mut first);
                    cur.0 = x;
                }
            }
            'V' => {
                for y in nums {
                    let y = y + ty;
                    emit('V', &[y], &mut out, &mut first);
                    cur.1 = y;
                }
            }
            'Z' | 'z' => {
                out.push('Z');
                cur = sub_start;
            }
            _ => return None,
        }
    }

    Some(out)
}

#[cfg(feature = "anyrender")]
fn fmt_path_num(v: f64) -> String {
    let scaled_f = v * 10_000.0;
    let sign = if scaled_f < 0.0 { -1i64 } else { 1i64 };
    let abs = scaled_f.abs();
    let base = abs.floor();
    let frac = abs - base;
    let rounded_abs = if frac > 0.5 + 1e-10 {
        base + 1.0
    } else if frac < 0.5 - 1e-10 {
        base
    } else if (base as i64) % 2 == 0 {
        base
    } else {
        base + 1.0
    };
    let scaled = sign * (rounded_abs as i64);
    if scaled % 10_000 == 0 {
        return format!("{}", scaled / 10_000);
    }
    let abs = scaled.abs();
    let whole = abs / 10_000;
    let frac = abs % 10_000;
    let sign = if scaled < 0 { "-" } else { "" };
    format!("{sign}{whole}.{frac:04}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[cfg(feature = "anyrender")]
fn extract_export_bbox(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<(f64, f64, f64, f64)>, String> {
    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let nodes = match json.get("nodes").and_then(|n| n.as_array()) {
        Some(nodes) if !nodes.is_empty() => nodes,
        _ => return Ok(None),
    };
    let target = if let Some(root_id) = root_figma_id {
        nodes.iter().find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
    } else {
        nodes.first()
    }
    .unwrap_or(&nodes[0]);
    let bbox = match target.get("absoluteBoundingBox") {
        Some(v) => v,
        None => return Ok(None),
    };
    let x = bbox.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = bbox.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let w = bbox.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let h = bbox.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if w <= 0.0 || h <= 0.0 {
        return Ok(None);
    }
    Ok(Some((x, y, w, h)))
}

#[cfg(feature = "anyrender")]
fn extract_export_base_color(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<(u8, u8, u8, u8)>, String> {
    fn figma_f64_to_u8(v: f64) -> u8 {
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    }

    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    let nodes = match json.get("nodes").and_then(|n| n.as_array()) {
        Some(nodes) if !nodes.is_empty() => nodes,
        _ => return Ok(None),
    };

    let target = if let Some(root_id) = root_figma_id {
        nodes.iter().find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
    } else {
        nodes.first()
    }
    .unwrap_or(&nodes[0]);

    let fills = match target.get("fills").and_then(|f| f.as_array()) {
        Some(fills) if !fills.is_empty() => fills,
        _ => return Ok(None),
    };

    for fill in fills {
        if fill.get("visible").and_then(|v| v.as_bool()) == Some(false) {
            continue;
        }
        if fill.get("type").and_then(|v| v.as_str()) != Some("SOLID") {
            continue;
        }

        let color = match fill.get("color") {
            Some(c) => c,
            None => continue,
        };

        let r = figma_f64_to_u8(color.get("r").and_then(|v| v.as_f64()).unwrap_or(1.0));
        let g = figma_f64_to_u8(color.get("g").and_then(|v| v.as_f64()).unwrap_or(1.0));
        let b = figma_f64_to_u8(color.get("b").and_then(|v| v.as_f64()).unwrap_or(1.0));
        let a = figma_f64_to_u8(fill.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0));
        return Ok(Some((r, g, b, a)));
    }

    Ok(None)
}
