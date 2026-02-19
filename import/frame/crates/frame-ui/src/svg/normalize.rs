pub(crate) fn normalize_svg_for_figma_parity(svg: &str, width: u32, height: u32) -> String {
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
        if let Some(norm) = normalize_translated_path_line(t) {
            out.push(norm);
            continue;
        }
        // Figma often emits keyword black for solid text/shape paths.
        out.push(
            t.replace("fill=\"#000000\"", "fill=\"black\"")
                .replace("stroke=\"#000000\"", "stroke=\"black\""),
        );
    }
    if !saw_root {
        // fallback: maintain well-formed output even if parser changes
        out.push(format!(r##"<rect width="{width}" height="{height}" fill="#FFFFFF"/>"##));
    }
    out.push("</svg>".to_string());
    out.join("\n") + "\n"
}

#[cfg(feature = "svg_export")]
const SVG_NUM_PRECISION_DP: u32 = 4;

#[cfg(feature = "svg_export")]
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
        fill = normalize_svg_color_keyword(fill);
        attrs.push(format!(r#"fill="{fill}""#));
    }
    if let Some(opacity) = fill_opacity {
        let val = trim_decimal(&opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"fill-opacity="{val}""#));
        }
    }
    if let Some(stroke) = stroke {
        let stroke = normalize_svg_color_keyword(stroke);
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(opacity) = stroke_opacity {
        let val = trim_decimal(&opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"stroke-opacity="{val}""#));
        }
    }
    if let Some(stroke_width) = stroke_width {
        let val = trim_decimal(&stroke_width);
        if val != "1" {
            attrs.push(format!(r#"stroke-width="{val}""#));
        }
    }
    if let Some(linecap) = linecap {
        if linecap != "butt" {
            attrs.push(format!(r#"stroke-linecap="{linecap}""#));
        }
    }
    if let Some(linejoin) = linejoin {
        if linejoin != "miter" {
            attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
        }
    }
    Some(format!("<rect {}/>", attrs.join(" ")))
}

#[cfg(feature = "svg_export")]
fn parse_translate_decimal(transform: &str) -> Option<(rust_decimal::Decimal, rust_decimal::Decimal)> {
    let raw = transform.strip_prefix("translate(")?.strip_suffix(')')?;
    let mut parts = raw.split(',');
    let x = parse_decimal(parts.next()?.trim())?;
    let y = parse_decimal(parts.next()?.trim())?;
    Some((x, y))
}

#[cfg(feature = "svg_export")]
fn parse_decimal(v: &str) -> Option<rust_decimal::Decimal> {
    rust_decimal::Decimal::from_str_exact(v)
        .or_else(|_| v.parse::<rust_decimal::Decimal>())
        .ok()
}

#[cfg(feature = "svg_export")]
fn extract_xml_attr(line: &str, key: &str) -> Option<String> {
    let needle = format!(r#"{key}=""#);
    let mut search_from = 0usize;
    while let Some(found) = line[search_from..].find(&needle) {
        let idx = search_from + found;
        let prev = line[..idx].chars().next_back();
        let is_exact_name = !matches!(
            prev,
            Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '_'
        );
        if is_exact_name {
            let start = idx + needle.len();
            let rest = &line[start..];
            let end = rest.find('"')?;
            return Some(rest[..end].to_string());
        }
        search_from = idx + needle.len();
    }
    None
}

#[cfg(feature = "svg_export")]
fn trim_decimal(v: &str) -> String {
    parse_decimal(v)
        .map(format_decimal)
        .unwrap_or_else(|| v.to_string())
}

#[cfg(feature = "svg_export")]
fn normalize_svg_color_keyword(value: String) -> String {
    if value.eq_ignore_ascii_case("#000000") {
        "black".to_string()
    } else if value.eq_ignore_ascii_case("#FFFFFF") {
        "white".to_string()
    } else {
        value
    }
}

#[cfg(feature = "svg_export")]
fn should_emit_opacity(v: &str) -> bool {
    parse_decimal(v)
        .map(|x| !format_decimal(x).eq("1"))
        .unwrap_or(true)
}

#[cfg(feature = "svg_export")]
fn normalize_translated_path_line(line: &str) -> Option<String> {
    if !line.starts_with("<path ") {
        return None;
    }
    let d = extract_xml_attr(line, "d")?;
    let transform = extract_xml_attr(line, "transform")?;
    let (tx, ty) = parse_translate_decimal(&transform)?;
    let translated = if tx.is_zero() && ty.is_zero() {
        d
    } else {
        translate_and_compact_path_data(&d, tx, ty)?
    };

    let mut attrs = vec![format!(r#"d="{translated}""#)];

    if let Some(fill) = extract_xml_attr(line, "fill") {
        let fill = normalize_svg_color_keyword(fill);
        attrs.push(format!(r#"fill="{fill}""#));
    }
    if let Some(fill_opacity) = extract_xml_attr(line, "fill-opacity") {
        let val = trim_decimal(&fill_opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"fill-opacity="{val}""#));
        }
    }
    if let Some(stroke) = extract_xml_attr(line, "stroke") {
        let stroke = normalize_svg_color_keyword(stroke);
        attrs.push(format!(r#"stroke="{stroke}""#));
    }
    if let Some(stroke_opacity) = extract_xml_attr(line, "stroke-opacity") {
        let val = trim_decimal(&stroke_opacity);
        if should_emit_opacity(&val) {
            attrs.push(format!(r#"stroke-opacity="{val}""#));
        }
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

#[cfg(feature = "svg_export")]
fn translate_and_compact_path_data(
    d: &str,
    tx: rust_decimal::Decimal,
    ty: rust_decimal::Decimal,
) -> Option<String> {
    use rust_decimal::Decimal;

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
    let mut first = true;
    let mut curr_x = Decimal::ZERO;
    let mut curr_y = Decimal::ZERO;
    let mut subpath_x = Decimal::ZERO;
    let mut subpath_y = Decimal::ZERO;

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
        let mut nums = Vec::<Decimal>::new();
        while idx < tokens.len() && !next_is_cmd(idx) {
            nums.push(parse_decimal(&tokens[idx])?);
            idx += 1;
        }

        let emit = |ch: char, values: &[Decimal], out: &mut String, first: &mut bool| {
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
                let mut vals = Vec::with_capacity(nums.len());
                for (i, pair) in nums.chunks_exact(2).enumerate() {
                    let x = pair[0] + tx;
                    let y = pair[1] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                    if i == 0 {
                        subpath_x = x;
                        subpath_y = y;
                    }
                }
                emit('M', &vals, &mut out, &mut first);
            }
            'L' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                for pair in nums.chunks_exact(2) {
                    let x = pair[0] + tx;
                    let y = pair[1] + ty;
                    if y == curr_y {
                        emit('H', &[x], &mut out, &mut first);
                    } else if x == curr_x {
                        emit('V', &[y], &mut out, &mut first);
                    } else {
                        emit('L', &[x, y], &mut out, &mut first);
                    }
                    curr_x = x;
                    curr_y = y;
                }
            }
            'C' => {
                if nums.len() % 6 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(6) {
                    vals.push(seg[0] + tx);
                    vals.push(seg[1] + ty);
                    vals.push(seg[2] + tx);
                    vals.push(seg[3] + ty);
                    let x = seg[4] + tx;
                    let y = seg[5] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit('C', &vals, &mut out, &mut first);
            }
            'Q' | 'S' => {
                if nums.len() % 4 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(4) {
                    vals.push(seg[0] + tx);
                    vals.push(seg[1] + ty);
                    let x = seg[2] + tx;
                    let y = seg[3] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit(cmd, &vals, &mut out, &mut first);
            }
            'T' => {
                if nums.len() % 2 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(2) {
                    let x = seg[0] + tx;
                    let y = seg[1] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit('T', &vals, &mut out, &mut first);
            }
            'A' => {
                if nums.len() % 7 != 0 {
                    return None;
                }
                let mut vals = Vec::with_capacity(nums.len());
                for seg in nums.chunks_exact(7) {
                    vals.push(seg[0]);
                    vals.push(seg[1]);
                    vals.push(seg[2]);
                    vals.push(seg[3]);
                    vals.push(seg[4]);
                    let x = seg[5] + tx;
                    let y = seg[6] + ty;
                    vals.push(x);
                    vals.push(y);
                    curr_x = x;
                    curr_y = y;
                }
                emit('A', &vals, &mut out, &mut first);
            }
            'H' => {
                let mut vals = Vec::with_capacity(nums.len());
                for x in nums {
                    let x = x + tx;
                    vals.push(x);
                    curr_x = x;
                }
                emit('H', &vals, &mut out, &mut first);
            }
            'V' => {
                let mut vals = Vec::with_capacity(nums.len());
                for y in nums {
                    let y = y + ty;
                    vals.push(y);
                    curr_y = y;
                }
                emit('V', &vals, &mut out, &mut first);
            }
            'Z' | 'z' => {
                out.push('Z');
                curr_x = subpath_x;
                curr_y = subpath_y;
            }
            _ => return None,
        }
    }

    Some(out)
}

#[cfg(feature = "svg_export")]
fn fmt_path_num(v: rust_decimal::Decimal) -> String {
    format_decimal(v)
}

#[cfg(feature = "svg_export")]
fn format_decimal(v: rust_decimal::Decimal) -> String {
    use rust_decimal::RoundingStrategy;

    if v.is_zero() {
        return "0".to_string();
    }
    let rounded =
        v.round_dp_with_strategy(SVG_NUM_PRECISION_DP, RoundingStrategy::MidpointAwayFromZero);
    if rounded.is_zero() {
        return "0".to_string();
    }
    rounded.normalize().to_string()
}

#[cfg(all(feature = "svg_export", test))]
mod svg_normalize_tests {
    use super::*;

    #[test]
    fn zero_translate_preserves_path_data() {
        let line = r##"<path d="M1 2L3 4Z" transform="translate(0,0)" fill="#000000"/>"##;
        let out = normalize_translated_path_line(line).expect("normalized");
        assert!(out.contains(r#"d="M1 2L3 4Z""#));
        assert!(out.contains(r#"fill="black""#));
    }

    #[test]
    fn translate_uses_decimal_math() {
        let line = r##"<path d="M29.7955 31.9489" transform="translate(0.1,0.2)" fill="#111111"/>"##;
        let out = normalize_translated_path_line(line).expect("normalized");
        assert!(out.contains(r#"d="M29.8955 32.1489""#), "{out}");
    }

    #[test]
    fn translate_preserves_command_shape() {
        let line = r##"<path d="M0 0 10 0 10 10" transform="translate(5,6)" fill="#111111"/>"##;
        let out = normalize_translated_path_line(line).expect("normalized");
        assert!(out.contains(r#"d="M5 6 15 6 15 16""#), "{out}");
        assert!(!out.contains("L"), "{out}");
    }
}

#[cfg(feature = "svg_export")]
pub(crate) fn extract_export_bbox(
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

#[cfg(feature = "svg_export")]
pub(crate) fn extract_export_base_color(
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
