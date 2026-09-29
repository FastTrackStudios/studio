use super::*;
use document_proto::{self as doc, Node, NodeKind, Scene};

/// A scene whose first page holds `nodes`; each entry is `(parent index or
/// None for the page, node)`. Returns the scene and its page id.
fn scene_of(nodes: Vec<(Option<usize>, Node)>) -> (Scene, doc::NodeId) {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    let ids: Vec<doc::NodeId> = nodes.iter().map(|(_, n)| n.id).collect();
    for (parent, mut node) in nodes {
        let parent_id = parent.map_or(page, |i| ids[i]);
        node.parent = Some(parent_id);
        scene.node_mut(parent_id).unwrap().children.push(node.id);
        scene.nodes.insert(node.id, node);
    }
    (scene, page)
}

fn node(kind: NodeKind, x: f64, y: f64, w: f64, h: f64) -> Node {
    let mut n = Node::new(kind, "n");
    n.transform = doc::Transform::translate(x, y);
    n.size = doc::Size { width: w, height: h };
    n
}

fn solid(r: f64, g: f64, b: f64, opacity: f64) -> doc::Paint {
    doc::Paint { opacity, ..doc::Paint::solid(doc::Color::rgba(r, g, b, 1.0)) }
}

#[test]
fn stroke_style_carries_dash_caps_and_opacity() {
    use super::scene_collect::stroke_style;

    let stroke = doc::Stroke {
        paints: vec![solid(1.0, 0.5, 0.0, 1.0)],
        weight: 2.0,
        cap: doc::StrokeCap::Round,
        join: doc::StrokeJoin::Bevel,
        align: doc::StrokeAlign::Inside,
        miter_limit: 8.0,
        dash_pattern: vec![4.0, 2.0],
        dash_offset: 0.0,
    };
    let stroke = stroke_style(&stroke, 0.75).expect("expected stroke");
    assert_eq!(stroke.width, 2.0);
    assert!(matches!(stroke.cap, StrokeCap::Round));
    assert!(matches!(stroke.join, StrokeJoin::Bevel));
    assert!(matches!(stroke.align, StrokeAlign::Inside));
    assert_eq!(stroke.miter_limit, 8.0);
    assert_eq!(stroke.dash_pattern, vec![4.0, 2.0]);
    assert!((stroke.color.a - 0.75).abs() < 1e-9);
}

#[test]
fn image_fill_reads_asset_bytes() {
    use super::scene_collect::fill_paint;

    let mut scene = Scene::new("Page");
    let asset = scene.add_asset("image/png", b"foobar".to_vec());
    let paint = doc::Paint {
        kind: doc::PaintKind::Image {
            asset,
            source_ref: Some("abc123".into()),
            scale_mode: doc::ImageScaleMode::Fill,
            transform: None,
        },
        opacity: 0.5,
        blend: doc::BlendMode::Normal,
        visible: true,
    };
    let Some(FillPaint::Image { image_hash, data_base64, alpha, scale_mode, .. }) = fill_paint(&paint, &scene) else {
        panic!("expected image fill");
    };
    assert_eq!(image_hash.as_deref(), Some("abc123"));
    assert_eq!(data_base64, "Zm9vYmFy");
    assert!((alpha - 0.5).abs() < 1e-9);
    assert_eq!(scale_mode, ImageScaleMode::Fill);
}

#[test]
fn gradient_paint_opacity_applied_to_stops() {
    use super::scene_collect::fill_paint;

    let scene = Scene::new("Page");
    let stop = |position, r, b| doc::GradientStop { position, color: doc::Color::rgba(r, 0.0, b, 1.0) };
    let paint = doc::Paint {
        kind: doc::PaintKind::LinearGradient { stops: vec![stop(0.0, 1.0, 0.0), stop(1.0, 0.0, 1.0)], transform: None },
        opacity: 0.5,
        blend: doc::BlendMode::Normal,
        visible: true,
    };
    let Some(FillPaint::GradientLinear { stops, .. }) = fill_paint(&paint, &scene) else {
        panic!("expected linear gradient");
    };
    assert_eq!(stops.len(), 2);
    assert!((stops[0].color.a - 0.5).abs() < 1e-9, "first stop alpha: {}", stops[0].color.a);
    assert!((stops[1].color.a - 0.5).abs() < 1e-9, "second stop alpha: {}", stops[1].color.a);
}

#[test]
fn inherited_opacity_applies_to_child_fills() {
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    frame.opacity = 0.5;
    let mut rect = node(NodeKind::Rectangle, 10.0, 10.0, 20.0, 20.0);
    rect.fills = vec![solid(1.0, 0.0, 0.0, 1.0)];
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), rect)]);

    let found = build_scene_primitives(&scene, root).iter().any(|p| match p {
        PaintPrimitive::Rect { fills, .. } => {
            fills.iter().any(|(f, _)| matches!(f, FillPaint::Solid(c) if (c.a - 0.5).abs() < 1e-9))
        }
        _ => false,
    });
    assert!(found, "expected child fill alpha to include parent opacity");
}

#[test]
fn emits_layer_primitives_for_non_normal_blend_nodes() {
    let mut frame = node(NodeKind::Frame, 0.0, 0.0, 50.0, 50.0);
    frame.blend = doc::BlendMode::Multiply;
    let (scene, root) = scene_of(vec![(None, frame)]);

    let primitives = build_scene_primitives(&scene, root);
    let start = primitives.iter().filter(|p| matches!(p, PaintPrimitive::LayerStart { .. })).count();
    let end = primitives.iter().filter(|p| matches!(p, PaintPrimitive::LayerEnd { .. })).count();
    assert!(start >= 1, "expected at least one layer start");
    assert_eq!(start, end, "layer boundaries should be balanced");
}

#[test]
fn extracts_rotation_from_transform() {
    let frame = node(NodeKind::Frame, 0.0, 0.0, 100.0, 100.0);
    let mut rect = node(NodeKind::Rectangle, 0.0, 0.0, 15.0, 1.0);
    rect.transform = doc::Transform { a: 0.934, b: -0.358, tx: 4.584, c: 0.358, d: 0.934, ty: 22.253 };
    rect.fills = vec![solid(1.0, 1.0, 1.0, 1.0)];
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), rect)]);

    // atan2(0.358, 0.934) ≈ 0.3649 radians ≈ 20.9°
    let found = build_scene_primitives(&scene, root)
        .iter()
        .any(|p| matches!(p, PaintPrimitive::Rect { rotation, .. } if rotation.abs() > 0.3 && rotation.abs() < 0.4));
    assert!(found, "expected rotated rect with ~0.365 radian rotation");
}

#[test]
fn per_fill_blend_mode_is_kept() {
    let mut rect = node(NodeKind::Rectangle, 0.0, 0.0, 100.0, 100.0);
    rect.fills = vec![
        solid(1.0, 0.0, 0.0, 1.0),
        doc::Paint { blend: doc::BlendMode::SoftLight, ..solid(0.0, 0.0, 1.0, 0.5) },
    ];
    let (scene, root) = scene_of(vec![(None, rect)]);

    let found = build_scene_primitives(&scene, root).iter().any(|p| match p {
        PaintPrimitive::Rect { fills, .. } => fills.iter().any(|(_, b)| *b == BlendMix::SoftLight),
        _ => false,
    });
    assert!(found, "expected at least one fill with SoftLight blend mode");
}

#[test]
fn ellipse_without_geometry_emits_round_rect() {
    // An ellipse with no exported geometry draws as a rect with circular radii.
    let frame = node(NodeKind::Frame, 0.0, 0.0, 200.0, 200.0);
    let mut ellipse = node(NodeKind::Ellipse, 10.0, 10.0, 40.0, 40.0);
    ellipse.fills = vec![solid(0.5, 0.5, 0.5, 1.0)];
    let (scene, root) = scene_of(vec![(None, frame), (Some(0), ellipse)]);

    let rect = build_scene_primitives(&scene, root).into_iter().find_map(|p| match p {
        PaintPrimitive::Rect { width, height, corner_radii, .. }
            if (width - 40.0).abs() < 1e-6 && (height - 40.0).abs() < 1e-6 =>
        {
            Some(corner_radii)
        }
        _ => None,
    });
    let radii = rect.expect("ELLIPSE without geometry should emit a Rect, not a Path");
    assert!((radii.top_left - 20.0).abs() < 1e-6, "expected circular corner radius 20.0, got {}", radii.top_left);
}

#[test]
fn diagnostics_count_typed_features() {
    let mut blended = node(NodeKind::Frame, 0.0, 0.0, 10.0, 10.0);
    blended.blend = doc::BlendMode::Multiply;
    let mut text = node(NodeKind::Text(doc::TextData::default()), 0.0, 0.0, 10.0, 10.0);
    text.effects = vec![doc::Effect {
        kind: doc::EffectKind::LayerBlur,
        visible: true,
        color: doc::Color::rgba(0.0, 0.0, 0.0, 0.25),
        offset_x: 0.0,
        offset_y: 0.0,
        radius: 4.0,
        spread: 0.0,
    }];
    let mut masked = node(NodeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    masked.mask = Some(doc::MaskKind::Luminance);
    let mut hidden_fill = node(NodeKind::Rectangle, 0.0, 0.0, 10.0, 10.0);
    hidden_fill.fills = vec![doc::Paint { visible: false, ..solid(0.0, 0.0, 0.0, 1.0) }];
    let (scene, root) = scene_of(vec![(None, blended), (None, text), (None, masked), (None, hidden_fill)]);

    let d = collect_render_diagnostics(&scene, root);
    assert_eq!(d.total_nodes, 5);
    assert_eq!(d.non_normal_blend_nodes, 1);
    assert_eq!((d.layer_blur_nodes, d.layer_blur_approx_nodes), (1, 1));
    assert_eq!((d.alpha_mask_nodes, d.luminance_mask_nodes), (0, 1));
    assert_eq!(d.gradient_fill_nodes + d.image_fill_nodes, 0);
}

#[test]
#[cfg(feature = "anyrender")]
fn path_fit_preserves_local_geometry_without_forced_scaling() {
    use super::geometry::compute_path_fit_transform;

    let fill_paths = vec!["M 12 12 L 20 20 L 12 20 Z".to_string()];
    let transform = compute_path_fit_transform(&fill_paths, &[], 100.0, 200.0, 100.0, 100.0, false);
    assert_eq!(transform, kurbo::Affine::IDENTITY);
}

#[test]
#[cfg(feature = "anyrender")]
fn path_fit_cancels_node_origin_for_absolute_geometry() {
    use super::geometry::compute_path_fit_transform;

    let fill_paths = vec!["M 100 200 L 200 200 L 200 300 L 100 300 Z".to_string()];
    let transform = compute_path_fit_transform(&fill_paths, &[], 100.0, 200.0, 100.0, 100.0, false);
    assert_eq!(transform, kurbo::Affine::translate((-100.0, -200.0)));
}
