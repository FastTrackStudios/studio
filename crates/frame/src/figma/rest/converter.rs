use crate::figma::model::FrameDocument;
use crate::figma::rest::error::ImportError;
use figma_api::models;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportDiagnostics {
    pub source_schema: Option<String>,
    pub normalized_aliases: usize,
    pub unsupported_node_keys: BTreeMap<String, usize>,
}

impl ImportDiagnostics {
    pub fn top_unsupported_keys(&self, limit: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> = self
            .unsupported_node_keys
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        entries.into_iter().take(limit).collect()
    }
}

/// Parse Figma REST API JSON and build a flattened FrameDocument.
pub fn import_figma_file(json: &str) -> Result<FrameDocument, ImportError> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    let mut _rewrites = 0;
    normalize_layout_compat_value(&mut value, &mut _rewrites);
    Ok(FrameDocument::from_source_value(value)?)
}

/// Convert a pre-deserialized Figma API `GetFile` into FrameDocument.
pub fn import_get_file(file: &models::GetFile) -> Result<FrameDocument, ImportError> {
    Ok(FrameDocument::from_figma_api(file.clone())?)
}

/// Parse source bytes into a FrameDocument.
///
/// JSON Figma API responses are supported. Binary `.fig` payloads currently
/// return `UnsupportedBinaryFig` from frame-proto.
pub fn import_figma_bytes(bytes: &[u8]) -> Result<FrameDocument, ImportError> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) {
            let mut _rewrites = 0;
            normalize_layout_compat_value(&mut value, &mut _rewrites);
            return Ok(FrameDocument::from_source_value(value)?);
        }
    }
    Ok(FrameDocument::from_source_bytes(bytes)?)
}

/// Parse source bytes and return both imported document and diagnostics
/// for normalization/dropped-field tracking.
pub fn import_figma_bytes_with_diagnostics(
    bytes: &[u8],
) -> Result<(FrameDocument, ImportDiagnostics), ImportError> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) {
            let mut normalized_aliases = 0;
            normalize_layout_compat_value(&mut value, &mut normalized_aliases);
            let diagnostics = ImportDiagnostics {
                source_schema: value
                    .get("schema")
                    .and_then(|s| s.as_str())
                    .map(ToString::to_string),
                normalized_aliases,
                unsupported_node_keys: collect_unsupported_node_keys(&value),
            };
            let doc = FrameDocument::from_source_value(value)?;
            return Ok((doc, diagnostics));
        }
    }
    let doc = FrameDocument::from_source_bytes(bytes)?;
    Ok((doc, ImportDiagnostics::default()))
}

fn normalize_layout_compat_value(value: &mut serde_json::Value, rewrites: &mut usize) {
    let Some(obj) = value.as_object_mut() else {
        if let Some(arr) = value.as_array_mut() {
            for entry in arr {
                normalize_layout_compat_value(entry, rewrites);
            }
        }
        return;
    };

    // Normalize alternate exporter keys into Figma REST-style keys.
    copy_key(obj, "layout_mode", "layoutMode", rewrites);
    copy_key(
        obj,
        "primary_axis_sizing_mode",
        "primaryAxisSizingMode",
        rewrites,
    );
    copy_key(
        obj,
        "counter_axis_sizing_mode",
        "counterAxisSizingMode",
        rewrites,
    );
    copy_key(
        obj,
        "primary_axis_align_items",
        "primaryAxisAlignItems",
        rewrites,
    );
    copy_key(
        obj,
        "counter_axis_align_items",
        "counterAxisAlignItems",
        rewrites,
    );
    copy_key(obj, "item_spacing", "itemSpacing", rewrites);
    copy_key(obj, "counter_axis_spacing", "counterAxisSpacing", rewrites);
    copy_key(obj, "padding_left", "paddingLeft", rewrites);
    copy_key(obj, "padding_right", "paddingRight", rewrites);
    copy_key(obj, "padding_top", "paddingTop", rewrites);
    copy_key(obj, "padding_bottom", "paddingBottom", rewrites);
    copy_key(obj, "layout_wrap", "layoutWrap", rewrites);
    copy_key(obj, "layout_positioning", "layoutPositioning", rewrites);
    copy_key(obj, "layout_align", "layoutAlign", rewrites);
    copy_key(obj, "layout_grow", "layoutGrow", rewrites);
    copy_key(obj, "min_width", "minWidth", rewrites);
    copy_key(obj, "max_width", "maxWidth", rewrites);
    copy_key(obj, "min_height", "minHeight", rewrites);
    copy_key(obj, "max_height", "maxHeight", rewrites);

    // Common geometry aliases from non-REST exporters.
    copy_key(obj, "relative_transform", "relativeTransform", rewrites);
    copy_key(
        obj,
        "absolute_bounding_box",
        "absoluteBoundingBox",
        rewrites,
    );
    copy_key(obj, "fill_geometry", "fillGeometry", rewrites);
    copy_key(obj, "stroke_geometry", "strokeGeometry", rewrites);
    copy_key(obj, "image_data_url", "imageDataUrl", rewrites);

    for (_, child) in obj.iter_mut() {
        normalize_layout_compat_value(child, rewrites);
    }
}

fn copy_key(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    from: &str,
    to: &str,
    rewrites: &mut usize,
) {
    if obj.contains_key(to) {
        return;
    }
    if let Some(value) = obj.get(from).cloned() {
        obj.insert(to.to_string(), value);
        *rewrites += 1;
    }
}

fn collect_unsupported_node_keys(value: &serde_json::Value) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    collect_unsupported_node_keys_recursive(value, &mut out);
    out
}

fn collect_unsupported_node_keys_recursive(
    value: &serde_json::Value,
    out: &mut BTreeMap<String, usize>,
) {
    if let Some(arr) = value.as_array() {
        for entry in arr {
            collect_unsupported_node_keys_recursive(entry, out);
        }
        return;
    }
    let Some(obj) = value.as_object() else {
        return;
    };

    let is_node = obj.contains_key("id")
        && obj.contains_key("type")
        && (obj.contains_key("children")
            || obj.get("type").and_then(|v| v.as_str()) == Some("DOCUMENT")
            || obj.get("type").and_then(|v| v.as_str()) == Some("CANVAS"));
    if is_node {
        for key in obj.keys() {
            if !is_supported_projection_key(key) {
                *out.entry(key.clone()).or_insert(0) += 1;
            }
        }
    }

    for (_, child) in obj {
        collect_unsupported_node_keys_recursive(child, out);
    }
}

fn is_supported_projection_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "name"
            | "type"
            | "visible"
            | "locked"
            | "opacity"
            | "blendMode"
            | "size"
            | "relativeTransform"
            | "fills"
            | "strokes"
            | "effects"
            | "fillGeometry"
            | "strokeGeometry"
            | "characters"
            | "style"
            | "textCase"
            | "textDecoration"
            | "textAutoResize"
            | "textAlignHorizontal"
            | "textAlignVertical"
            | "imageDataUrl"
            | "layoutMode"
            | "primaryAxisSizingMode"
            | "counterAxisSizingMode"
            | "primaryAxisAlignItems"
            | "counterAxisAlignItems"
            | "itemSpacing"
            | "counterAxisSpacing"
            | "paddingLeft"
            | "paddingRight"
            | "paddingTop"
            | "paddingBottom"
            | "layoutWrap"
            | "layoutPositioning"
            | "layoutAlign"
            | "layoutGrow"
            | "layoutSizingHorizontal"
            | "layoutSizingVertical"
            | "minWidth"
            | "maxWidth"
            | "minHeight"
            | "maxHeight"
            | "strokeWeight"
            | "strokeAlign"
            | "strokeJoin"
            | "strokeCap"
            | "strokeMiterLimit"
            | "dashPattern"
            | "clipsContent"
            | "isMask"
            | "maskType"
            | "componentId"
            | "componentSetId"
            | "variantProperties"
            | "componentProperties"
            | "componentPropertyReferences"
            | "assetRefs"
            | "imageFillAssetRefs"
            | "imageFillAssets"
            | "explicitVariableModes"
            | "boundVariables"
            | "pluginData"
            | "sharedPluginData"
            | "children"
            | "exports"
            | "absoluteBoundingBox"
            | "rotation"
            | "constraints"
            | "cornerRadius"
            | "cornerSmoothing"
            | "topLeftRadius"
            | "topRightRadius"
            | "bottomLeftRadius"
            | "bottomRightRadius"
            | "rectangleCornerRadii"
    )
}
