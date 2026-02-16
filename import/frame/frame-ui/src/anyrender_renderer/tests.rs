use super::*;
use frame_proto::FrameDocument;

#[test]
fn parses_stroke_style_with_dash_and_caps() {
    use super::extract::parse_stroke_style;

    let raw = serde_json::json!({
        "strokes": [{
            "type": "SOLID",
            "color": { "r": 1.0, "g": 0.5, "b": 0.0, "a": 1.0 },
            "opacity": 1.0
        }],
        "strokeWeight": 2.0,
        "strokeCap": "ROUND",
        "strokeJoin": "BEVEL",
        "strokeAlign": "INSIDE",
        "strokeMiterLimit": 8.0,
        "dashPattern": [4, 2]
    });

    let stroke = parse_stroke_style(&raw, 0.75).expect("expected stroke");
    assert_eq!(stroke.width, 2.0);
    assert!(matches!(stroke.cap, StrokeCap::Round));
    assert!(matches!(stroke.join, StrokeJoin::Bevel));
    assert!(matches!(stroke.align, StrokeAlign::Inside));
    assert_eq!(stroke.miter_limit, 8.0);
    assert_eq!(stroke.dash_pattern, vec![4.0, 2.0]);
    assert!((stroke.color.a - 0.75).abs() < 1e-9);
}

#[test]
fn parses_image_fill_from_hydrated_assets() {
    use super::extract::image_fill_from_paint;

    let raw = serde_json::json!({
        "imageFillAssets": {
            "abc123": "Zm9vYmFy"
        }
    });
    let paint = serde_json::json!({
        "type": "IMAGE",
        "imageHash": "abc123",
        "opacity": 0.5
    });

    let parsed = image_fill_from_paint(&raw, &paint).expect("expected image fill");
    assert_eq!(parsed.0.as_deref(), Some("abc123"));
    assert_eq!(parsed.1, "Zm9vYmFy");
    assert!((parsed.2 - 0.5).abs() < 1e-9);
}

#[test]
fn inherited_opacity_applies_to_child_fills() {
    let json = serde_json::json!({
        "schema": "fts.figma.export/v1",
        "generatedAt": "2026-02-16T00:00:00.000Z",
        "source": { "editorType": "figma", "page": { "id": "0:1", "name": "Canvas" } },
        "selection": { "ids": [], "names": [], "totalRoots": 0, "totalNodes": 0 },
        "options": { "includeSvg": false, "includePng": false, "maxDepth": 4 },
        "nodes": [{
            "id": "1:1",
            "name": "RootFrame",
            "type": "FRAME",
            "visible": true,
            "opacity": 0.5,
            "relativeTransform": [[1.0,0.0,0.0],[0.0,1.0,0.0]],
            "size": { "x": 100.0, "y": 100.0 },
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
            "children": [{
                "id": "1:2",
                "name": "ChildRect",
                "type": "RECTANGLE",
                "visible": true,
                "relativeTransform": [[1.0,0.0,10.0],[0.0,1.0,10.0]],
                "size": { "x": 20.0, "y": 20.0 },
                "absoluteBoundingBox": { "x": 10.0, "y": 10.0, "width": 20.0, "height": 20.0 },
                "fills": [{
                    "type": "SOLID",
                    "visible": true,
                    "opacity": 1.0,
                    "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 }
                }],
                "children": []
            }]
        }]
    });
    let bytes = serde_json::to_vec(&json).expect("json bytes");
    let doc = FrameDocument::from_source_bytes(&bytes).expect("import");
    let root = doc.pages[0];
    let primitives = build_paint_primitives(&doc, root);

    let mut found = false;
    for p in primitives {
        if let PaintPrimitive::Rect { fills, .. } = &p {
            // One rect is the frame background (may be missing fill); we only care
            // that at least one solid fill carries inherited alpha 0.5.
            for fill in fills {
                if let FillPaint::Solid(c) = fill {
                    if (c.a - 0.5).abs() < 1e-9 {
                        found = true;
                        break;
                    }
                }
            }
            if found {
                break;
            }
        }
    }
    assert!(found, "expected child fill alpha to include parent opacity");
}

#[test]
fn emits_layer_primitives_for_non_normal_blend_nodes() {
    let json = serde_json::json!({
        "schema": "fts.figma.export/v1",
        "generatedAt": "2026-02-16T00:00:00.000Z",
        "source": { "editorType": "figma", "page": { "id": "0:1", "name": "Canvas" } },
        "selection": { "ids": [], "names": [], "totalRoots": 0, "totalNodes": 0 },
        "options": { "includeSvg": false, "includePng": false, "maxDepth": 4 },
        "nodes": [{
            "id": "2:1",
            "name": "BlendFrame",
            "type": "FRAME",
            "visible": true,
            "blendMode": "MULTIPLY",
            "relativeTransform": [[1.0,0.0,0.0],[0.0,1.0,0.0]],
            "size": { "x": 50.0, "y": 50.0 },
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 50.0, "height": 50.0 },
            "children": []
        }]
    });
    let bytes = serde_json::to_vec(&json).expect("json bytes");
    let doc = FrameDocument::from_source_bytes(&bytes).expect("import");
    let root = doc.pages[0];
    let primitives = build_paint_primitives(&doc, root);

    let mut start = 0usize;
    let mut end = 0usize;
    for p in primitives {
        match p {
            PaintPrimitive::LayerStart { .. } => start += 1,
            PaintPrimitive::LayerEnd { .. } => end += 1,
            _ => {}
        }
    }
    assert!(start >= 1, "expected at least one layer start");
    assert_eq!(start, end, "layer boundaries should be balanced");
}

#[test]
#[cfg(feature = "anyrender")]
fn path_fit_preserves_local_geometry_without_forced_scaling() {
    use super::geometry::compute_path_fit_transform;

    let fill_paths = vec!["M 12 12 L 20 20 L 12 20 Z".to_string()];
    let transform = compute_path_fit_transform(&fill_paths, &[], 100.0, 200.0, 100.0, 100.0);
    assert_eq!(transform, kurbo::Affine::IDENTITY);
}

#[test]
#[cfg(feature = "anyrender")]
fn path_fit_cancels_node_origin_for_absolute_geometry() {
    use super::geometry::compute_path_fit_transform;

    let fill_paths = vec!["M 100 200 L 200 200 L 200 300 L 100 300 Z".to_string()];
    let transform = compute_path_fit_transform(&fill_paths, &[], 100.0, 200.0, 100.0, 100.0);
    assert_eq!(transform, kurbo::Affine::translate((-100.0, -200.0)));
}
