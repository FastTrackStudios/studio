use crate::error::ImportError;
use figma_api::models;
use frame_proto::FrameDocument;

/// Parse Figma REST API JSON and build a flattened FrameDocument.
pub fn import_figma_file(json: &str) -> Result<FrameDocument, ImportError> {
    let normalized = normalize_layout_compat_json(json);
    Ok(FrameDocument::from_figma_api_json(&normalized)?)
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
        let normalized = normalize_layout_compat_json(text);
        if let Ok(doc) = FrameDocument::from_source_bytes(normalized.as_bytes()) {
            return Ok(doc);
        }
    }
    Ok(FrameDocument::from_source_bytes(bytes)?)
}

fn normalize_layout_compat_json(input: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(input) else {
        return input.to_string();
    };
    normalize_layout_compat_value(&mut value);
    serde_json::to_string(&value).unwrap_or_else(|_| input.to_string())
}

fn normalize_layout_compat_value(value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        if let Some(arr) = value.as_array_mut() {
            for entry in arr {
                normalize_layout_compat_value(entry);
            }
        }
        return;
    };

    // Normalize alternate exporter keys into Figma REST-style keys.
    copy_key(obj, "layout_mode", "layoutMode");
    copy_key(obj, "primary_axis_sizing_mode", "primaryAxisSizingMode");
    copy_key(obj, "counter_axis_sizing_mode", "counterAxisSizingMode");
    copy_key(obj, "primary_axis_align_items", "primaryAxisAlignItems");
    copy_key(obj, "counter_axis_align_items", "counterAxisAlignItems");
    copy_key(obj, "item_spacing", "itemSpacing");
    copy_key(obj, "counter_axis_spacing", "counterAxisSpacing");
    copy_key(obj, "padding_left", "paddingLeft");
    copy_key(obj, "padding_right", "paddingRight");
    copy_key(obj, "padding_top", "paddingTop");
    copy_key(obj, "padding_bottom", "paddingBottom");
    copy_key(obj, "layout_wrap", "layoutWrap");
    copy_key(obj, "layout_positioning", "layoutPositioning");
    copy_key(obj, "layout_align", "layoutAlign");
    copy_key(obj, "layout_grow", "layoutGrow");
    copy_key(obj, "min_width", "minWidth");
    copy_key(obj, "max_width", "maxWidth");
    copy_key(obj, "min_height", "minHeight");
    copy_key(obj, "max_height", "maxHeight");

    // Common geometry aliases from non-REST exporters.
    copy_key(obj, "relative_transform", "relativeTransform");
    copy_key(obj, "absolute_bounding_box", "absoluteBoundingBox");
    copy_key(obj, "fill_geometry", "fillGeometry");
    copy_key(obj, "stroke_geometry", "strokeGeometry");
    copy_key(obj, "image_data_url", "imageDataUrl");

    for (_, child) in obj.iter_mut() {
        normalize_layout_compat_value(child);
    }
}

fn copy_key(obj: &mut serde_json::Map<String, serde_json::Value>, from: &str, to: &str) {
    if obj.contains_key(to) {
        return;
    }
    if let Some(value) = obj.get(from).cloned() {
        obj.insert(to.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_minimal_get_file_json() {
        let json = r#"{
            "name": "Test File",
            "role": "viewer",
            "lastModified": "2024-01-01T00:00:00Z",
            "editorType": "figma",
            "version": "1",
            "document": {
                "id": "0:0",
                "name": "Document",
                "type": "DOCUMENT",
                "scrollBehavior": "SCROLLS",
                "children": [
                    {
                        "id": "0:1",
                        "name": "Page 1",
                        "type": "CANVAS",
                        "scrollBehavior": "SCROLLS",
                        "children": [],
                        "backgroundColor": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 },
                        "prototypeStartNodeID": null,
                        "flowStartingPoints": [],
                        "prototypeDevice": { "type": "NONE", "rotation": "NONE" }
                    }
                ]
            },
            "components": {},
            "componentSets": {},
            "schemaVersion": 0,
            "styles": {}
        }"#;

        let doc = import_figma_file(json).expect("import should succeed");
        assert_eq!(doc.name, "Test File");
        assert_eq!(doc.pages.len(), 1);
        assert!(doc.node_count() >= 2);
    }

    #[test]
    fn imports_from_json_bytes() {
        let json = r#"{
            "name": "Bytes Test",
            "role": "viewer",
            "lastModified": "2024-01-01T00:00:00Z",
            "editorType": "figma",
            "version": "1",
            "document": {
                "id": "0:0",
                "name": "Document",
                "type": "DOCUMENT",
                "scrollBehavior": "SCROLLS",
                "children": [
                    {
                        "id": "0:1",
                        "name": "Page 1",
                        "type": "CANVAS",
                        "scrollBehavior": "SCROLLS",
                        "children": [],
                        "backgroundColor": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 },
                        "prototypeStartNodeID": null,
                        "flowStartingPoints": [],
                        "prototypeDevice": { "type": "NONE", "rotation": "NONE" }
                    }
                ]
            },
            "components": {},
            "componentSets": {},
            "schemaVersion": 0,
            "styles": {}
        }"#;

        let doc = import_figma_bytes(json.as_bytes()).expect("import from bytes should succeed");
        assert_eq!(doc.name, "Bytes Test");
        assert_eq!(doc.pages.len(), 1);
    }

    #[test]
    fn rejects_non_json_binary_fig_bytes_for_now() {
        let err = import_figma_bytes(&[0x89, b'F', b'I', b'G']).expect_err("should fail");
        let message = err.to_string();
        assert!(message.contains("not a supported .fig ZIP container"));
    }
}
