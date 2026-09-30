//! The materials family and the rendered parts: every material places as
//! a swatch within a node and byte budget, its texture assets are in the
//! document, and the parts give callers the frames they fill.

use std::collections::BTreeMap;

use document_proto::{NodeId, PaintKind, Scene};
use surface_kit::materials::cabinet::{CabinetSpec, Kind, cabinet};
use surface_kit::{Family, Place, components};
use surface_widgets::{Builder, Theme};

fn build(id: &str, p: &Place) -> (Scene, NodeId) {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let root = {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        surface_kit::place(&mut b, page, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
    };
    (scene, root)
}

/// Nodes under `root`, image-filled nodes, and the bytes of assets used.
fn cost(scene: &Scene, root: NodeId) -> (usize, usize, usize) {
    let nodes = scene.walk(root);
    let mut images = 0;
    let mut assets = std::collections::BTreeSet::new();
    for id in &nodes {
        for p in &scene.node(*id).unwrap().fills {
            if let PaintKind::Image { asset, .. } = &p.kind {
                images += 1;
                assert!(scene.asset(asset).is_some(), "an image fill's asset is in the document");
                assets.insert(asset.clone());
            }
        }
    }
    let bytes = assets.iter().map(|a| scene.asset(a).unwrap().bytes.len()).sum();
    (nodes.len(), images, bytes)
}

#[test]
fn every_material_places_in_every_finish_within_budget() {
    let mats: Vec<_> = components().into_iter().filter(|c| c.family == Family::Material).collect();
    assert!(mats.len() >= 16, "{} materials", mats.len());
    for def in mats {
        assert!(def.id.starts_with("material."), "{}", def.id);
        let variants: Vec<Option<&str>> = if def.variants.is_empty() { vec![None] } else { def.variants.iter().map(|v| Some(*v)).collect() };
        for v in variants {
            let mut p = Place::new("Swatch").options(&["w=480", "h=320"]);
            if let Some(v) = v {
                p = p.variant(v);
            }
            let (scene, root) = build(def.id, &p);
            let (nodes, images, bytes) = cost(&scene, root);
            println!("{:<26} {:<16} nodes {nodes:>3}  image rects {images:>2}  asset bytes {bytes:>6}", def.id, v.unwrap_or("-"));
            assert!(nodes <= 60, "{} {v:?}: {nodes} nodes", def.id);
            assert!(images <= surface_kit::materials::texture::MAX_TILES, "{} {v:?}: {images} tiles", def.id);
            assert!(bytes <= 160_000, "{} {v:?}: {bytes} bytes of texture", def.id);
            let size = scene.node(root).unwrap().size;
            assert_eq!((size.width, size.height), (480.0, 320.0));
        }
    }
}

#[test]
fn a_texture_is_stored_once_however_many_surfaces_use_it() {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
    for _ in 0..3 {
        surface_kit::place(&mut b, page, "material.tolex", &Place::new("T").variant("bronco")).unwrap();
    }
    assert_eq!(b.scene.assets.len(), 1);
}

#[test]
fn parts_report_their_cost() {
    for (id, p) in [
        ("knob.render", Place::new("Drive").variant("fluted")),
        ("knob.render", Place::new("Drive").variant("knurled").options(&["material=metal"])),
        ("knob.render", Place::new("Drive").variant("pointer")),
        ("panel.pedal_3d", Place::new("Pedal").options(&["screws"])),
        ("panel.amp_head", Place::new("Amp")),
        ("panel.amp_combo", Place::new("Amp").options(&["grille=diamond"])),
    ] {
        let (scene, root) = build(id, &p);
        let (nodes, images, bytes) = cost(&scene, root);
        println!("{id:<18} {:<10} nodes {nodes:>3}  image rects {images:>2}  asset bytes {bytes:>6}", p.variant.as_deref().unwrap_or("-"));
        assert!(nodes < 200, "{id}: {nodes}");
    }
}

#[test]
fn a_cabinet_gives_its_faceplate_and_grille_to_fill() {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
    let cab = cabinet(&mut b, page, "Head", &CabinetSpec::new(Kind::Head, 900.0, 400.0));
    for (id, name) in [(cab.faceplate, "faceplate"), (cab.grille, "grille")] {
        let n = b.scene.node(id).unwrap();
        assert_eq!(n.name, name);
        assert_eq!(n.parent, Some(cab.root));
    }
    let face = b.scene.node(cab.faceplate).unwrap().size;
    let knob = surface_kit::place(&mut b, cab.faceplate, "knob.render", &Place::new("Gain")).unwrap();
    assert_eq!(b.scene.node(knob).unwrap().parent, Some(cab.faceplate));
    assert!(face.width > 700.0 && face.height > 80.0, "{face:?}");
}

#[test]
fn a_rendered_knob_keeps_its_light_out_of_the_turning_body() {
    use surface_proto::SlotRole;
    for cap in ["flat", "domed", "fluted", "knurled", "chicken_head", "pointer"] {
        for m in ["plastic", "metal", "chrome", "rubber"] {
            let mut scene = Scene::new("Kit");
            let page = scene.pages()[0];
            let mut controls = BTreeMap::new();
            let theme = Theme::stage_dark();
            let root = {
                let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
                let mopt = format!("material={m}");
                surface_kit::place(&mut b, page, "knob.render", &Place::new("x").variant(cap).options(&[mopt.as_str()])).unwrap()
            };
            let needle = NodeId(controls[&root].slots_of(SlotRole::Needle).next().unwrap());
            for id in scene.walk(needle) {
                let n = scene.node(id).unwrap();
                assert!(n.effects.iter().all(|e| e.offset_x == 0.0 && e.offset_y == 0.0), "{cap}/{m}: {} has an offset shadow and turns", n.name);
                for p in &n.fills {
                    let ok = matches!(p.kind, PaintKind::Solid { .. }) || matches!(&p.kind, PaintKind::RadialGradient { transform: None, .. });
                    assert!(ok, "{cap}/{m}: {} turns with a lit (off-centre) paint", n.name);
                }
            }
        }
    }
}
