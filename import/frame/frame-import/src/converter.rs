use crate::error::ImportError;
use figma_api::models;
use frame_proto::FrameDocument;

/// Parse Figma REST API JSON and build a flattened FrameDocument.
pub fn import_figma_file(json: &str) -> Result<FrameDocument, ImportError> {
    Ok(FrameDocument::from_figma_api_json(json)?)
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
    Ok(FrameDocument::from_source_bytes(bytes)?)
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
}
