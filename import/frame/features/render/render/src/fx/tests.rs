//! GPU pixel tests of the effects stage (skipped without a GPU adapter).

use std::collections::BTreeMap;

use document_proto::{self as doc, Node, NodeKind, Scene};

use super::stage::slice_with_context;
use super::{Frame, Mode};
use crate::offscreen::{Offscreen, Pixels};
use crate::PaintPrimitive;

const SIZE: u32 = 100;

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

fn shape(kind: NodeKind, x: f64, y: f64, w: f64, h: f64, rgb: (f64, f64, f64)) -> Node {
    let mut n = Node::new(kind, "n");
    n.transform = doc::Transform::translate(x, y);
    n.size = doc::Size { width: w, height: h };
    n.fills = vec![doc::Paint::solid(doc::Color::rgba(rgb.0, rgb.1, rgb.2, 1.0))];
    n
}

fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("skipping GPU effects test: {e}");
            None
        }
    }
}

fn render(o: &mut Offscreen, scene: &Scene, root: doc::NodeId, mode: Mode) -> Pixels {
    let frame = Frame { mode, ..Frame::new(scene, root, SIZE, SIZE) };
    o.render_with_effects(&frame).expect("render")
}

fn px(p: &Pixels, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * p.width + x) * 4) as usize;
    [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
}

/// A red dot in the middle of a black canvas that emits red light.
fn emissive_dot() -> (Scene, doc::NodeId) {
    let mut dot = shape(NodeKind::Ellipse, 44.0, 44.0, 12.0, 12.0, (1.0, 0.1, 0.1));
    dot.emission = Some(doc::Emission { color: doc::Color::rgba(1.0, 0.2, 0.1, 1.0), intensity: 1.5, radius: 16.0 });
    scene_of(vec![(None, dot)])
}

#[test]
fn emission_blooms_outside_its_geometry_on_the_gpu_only_softly_in_fallback() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = emissive_dot();
    let off = render(&mut o, &scene, root, Mode::Off);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    let fallback = render(&mut o, &scene, root, Mode::Fallback);
    // 8 px outside the dot's edge.
    let (x, y) = (50, 36);
    assert_eq!(px(&off, x, y)[0], 0, "no light without effects");
    assert!(px(&gpu, x, y)[0] > 12, "GPU bloom lights the surroundings: {:?}", px(&gpu, x, y));
    assert!(px(&fallback, x, y)[0] > 4, "fallback glow lights the surroundings: {:?}", px(&fallback, x, y));
    // The dot itself is still drawn; far corners stay dark.
    assert!(px(&off, 50, 50)[0] > 200);
    assert!(px(&gpu, 2, 2)[0] < 3, "{:?}", px(&gpu, 2, 2));
}

#[test]
fn emission_scales_with_opacity() {
    let Some(mut o) = offscreen() else { return };
    let (mut scene, root) = emissive_dot();
    let bright = render(&mut o, &scene, root, Mode::Gpu);
    let dot = scene.node(root).unwrap().children[0];
    scene.node_mut(dot).unwrap().opacity = 0.0;
    let dark = render(&mut o, &scene, root, Mode::Gpu);
    assert!(px(&bright, 50, 36)[0] > 12);
    assert_eq!(px(&dark, 50, 36)[0], 0, "a dimmed lamp gives no light");
}

/// Vertical stripes 20..80 × 20..80 under a shader node covering 30..70.
fn shaded(shader: &str) -> (Scene, doc::NodeId) {
    let mut panel = shape(NodeKind::Frame, 30.0, 30.0, 40.0, 40.0, (0.2, 0.5, 0.9));
    panel.shader = Some(doc::ShaderFx { shader: shader.to_string(), params: BTreeMap::new() });
    let mut nodes = vec![(None, shape(NodeKind::Rectangle, 10.0, 10.0, 80.0, 80.0, (0.5, 0.5, 0.5))), (None, panel)];
    for i in 0..8 {
        nodes.push((Some(1), shape(NodeKind::Rectangle, f64::from(i) * 5.0, 0.0, 2.5, 40.0, (1.0, 1.0, 1.0))));
    }
    scene_of(nodes)
}

fn changed_inside_only(off: &Pixels, fx: &Pixels, what: &str) {
    let mut inside = 0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let differs = px(off, x, y) != px(fx, x, y);
            let in_box = (30..70).contains(&x) && (30..70).contains(&y);
            if differs {
                assert!(in_box, "{what} changed ({x},{y}) outside its box: {:?} → {:?}", px(off, x, y), px(fx, x, y));
                inside += 1;
            }
        }
    }
    assert!(inside > 100, "{what} changed only {inside} px inside its box");
}

#[test]
fn glass_and_crt_change_pixels_only_inside_their_box() {
    let Some(mut o) = offscreen() else { return };
    for shader in ["glass", "crt", "lighting"] {
        let (scene, root) = shaded(shader);
        let off = render(&mut o, &scene, root, Mode::Off);
        let fallback = render(&mut o, &scene, root, Mode::Fallback);
        assert_eq!(off, fallback, "{shader}: the fallback skips shaders");
        let gpu = render(&mut o, &scene, root, Mode::Gpu);
        changed_inside_only(&off, &gpu, shader);
    }
}

#[test]
fn shader_keeps_paint_order_with_later_siblings() {
    let Some(mut o) = offscreen() else { return };
    let (mut scene, root) = shaded("crt");
    // A green bar painted after the shaded panel, across it.
    let bar = shape(NodeKind::Rectangle, 0.0, 45.0, 100.0, 10.0, (0.0, 1.0, 0.0));
    let id = bar.id;
    scene.node_mut(root).unwrap().children.push(id);
    scene.nodes.insert(id, Node { parent: Some(root), ..bar });
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    let c = px(&gpu, 50, 50);
    assert!(c[1] > 240 && c[0] < 10, "the later sibling stays on top: {c:?}");
}

fn with_asset_shader(source: &[u8]) -> (Scene, doc::NodeId) {
    let (mut scene, root) = shaded("");
    let asset = scene.add_asset("text/wgsl", source.to_vec());
    let panel = scene.node(root).unwrap().children[1];
    scene.node_mut(panel).unwrap().shader =
        Some(doc::ShaderFx { shader: format!("asset:{}", asset.0), params: BTreeMap::new() });
    (scene, root)
}

#[test]
fn custom_asset_shader_runs() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = with_asset_shader(
        b"fn effect(uv: vec2<f32>) -> vec4<f32> { let c = sample_src(uv); return vec4<f32>(vec3<f32>(c.a) - c.rgb, c.a); }",
    );
    let off = render(&mut o, &scene, root, Mode::Off);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    changed_inside_only(&off, &gpu, "invert");
    // A white stripe inverts to black; the blue panel to orange.
    let (w, b) = (px(&off, 31, 50), px(&gpu, 31, 50));
    assert!(w[0] > 240 && b[0] < 15, "white → black: {w:?} → {b:?}");
    let (p, q) = (px(&off, 34, 50), px(&gpu, 34, 50));
    assert!((i32::from(p[0]) + i32::from(q[0]) - 255).abs() < 6, "blue → orange: {p:?} → {q:?}");
}

#[test]
fn broken_custom_shader_falls_back_to_plain() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = with_asset_shader(b"fn effect(uv: vec2<f32>) -> vec4<f32> { return nope; }");
    let off = render(&mut o, &scene, root, Mode::Off);
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    assert_eq!(off, gpu, "a broken shader draws its subtree plainly");
    // Missing asset / unknown built-in: the same.
    let (scene, root) = shaded("no-such-shader");
    assert_eq!(render(&mut o, &scene, root, Mode::Off), render(&mut o, &scene, root, Mode::Gpu));
}

#[test]
fn no_effects_renders_like_plain_vello() {
    let Some(mut o) = offscreen() else { return };
    let (scene, root) = shaded("");
    let gpu = render(&mut o, &scene, root, Mode::Gpu);
    let mut vs = vello::Scene::new();
    crate::paint_scene_into(&mut vs, &scene, root, kurbo::Affine::IDENTITY, None);
    let plain = o.render(&vs, SIZE, SIZE, peniko::Color::from_rgba8(0, 0, 0, 255)).unwrap();
    assert_eq!(gpu, plain);
}

#[test]
fn context_slices_reopen_and_close_enclosing_scopes() {
    let (mut scene, root) = shaded("crt");
    let panel = scene.node(root).unwrap().children[1];
    scene.node_mut(panel).unwrap().opacity = 0.5;
    scene.node_mut(panel).unwrap().blend = doc::BlendMode::Multiply;
    let (prims, plan) = crate::build_scene_primitives_and_effects(&scene, root);
    let r = &plan.shaders[0];
    let sub = slice_with_context(&prims, r.range.clone());
    assert!(matches!(sub.first(), Some(PaintPrimitive::LayerStart { .. })));
    assert!(matches!(sub.last(), Some(PaintPrimitive::LayerEnd { .. })));
    // A slice starting inside the layer reopens it and closes it.
    let inner = slice_with_context(&prims, r.range.start + 1..r.range.start + 2);
    assert!(matches!(inner.first(), Some(PaintPrimitive::LayerStart { .. })));
    assert!(matches!(inner.last(), Some(PaintPrimitive::LayerEnd { .. })));
    assert_eq!(inner.len(), 3);
}

#[test]
fn mode_parses_and_params_pack_into_named_slots() {
    assert_eq!("gpu".parse::<Mode>(), Ok(Mode::Gpu));
    assert_eq!("fallback".parse::<Mode>(), Ok(Mode::Fallback));
    assert_eq!("off".parse::<Mode>(), Ok(Mode::Off));
    assert!("nope".parse::<Mode>().is_err());
    let params = BTreeMap::from([("strength".to_string(), 0.25), ("angle".to_string(), 90.0), ("bogus".to_string(), 1.0)]);
    let (values, mask) = super::pack_params(&params);
    assert_eq!(values[0], 0.25);
    assert_eq!(values[4], 90.0);
    assert_eq!(mask, 0b1_0001);
}

#[test]
fn builtins_compile_through_naga() {
    for id in super::BUILTINS {
        let src = super::effect_module(super::builtin_source(id).unwrap());
        let module = vello::wgpu::naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{id}: {}", e.emit_to_string(&src)));
        vello::wgpu::naga::valid::Validator::new(
            vello::wgpu::naga::valid::ValidationFlags::all(),
            vello::wgpu::naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{id}: {}", e.emit_to_string(&src)));
    }
}
