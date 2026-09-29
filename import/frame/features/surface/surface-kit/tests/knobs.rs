//! The knob family: every style places, wires one needle, scales with
//! `Place::size`, rejects unknown variants, and lints clean.

use std::collections::BTreeMap;

use document_proto::{NodeId, Scene};
use surface_kit::{Family, Place, components, place};
use surface_proto::{Binding, Control, ControlKind, SlotRole};
use surface_widgets::{Builder, Theme};

fn knob_ids() -> Vec<&'static str> {
    components().into_iter().filter(|c| c.family == Family::Knob).map(|c| c.id).collect()
}

/// Place `id` into a fresh scene; returns the scene (controls attached to
/// their nodes) and the root.
fn placed(id: &str, p: &Place) -> Result<(Scene, NodeId, Control), String> {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let root = {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        place(&mut b, page, id, p)?
    };
    for (n, c) in &controls {
        scene.node_mut(*n).expect("control node").control = Some(c.clone());
    }
    let control = controls.get(&root).cloned().ok_or("no control on the root")?;
    Ok((scene, root, control))
}

#[test]
fn the_kit_has_every_source_style() {
    let ids = knob_ids();
    for id in [
        "knob.bakelite",
        "knob.metal",
        "knob.skirted",
        "knob.daka",
        "knob.marconi",
        "knob.collet",
        "knob.silver_top",
        "knob.metal_fluted",
        "knob.pointer",
        "knob.neve",
        "knob.soft_pointer",
        "knob.chicken_head",
        "knob.dial",
    ] {
        assert!(ids.contains(&id), "missing {id}");
    }
}

#[test]
fn every_knob_places_with_one_needle_inside_it() {
    for id in knob_ids() {
        let (scene, root, control) = placed(id, &Place::new("Gain")).unwrap();
        assert!(matches!(control.kind, ControlKind::Knob { sweep_degrees } if sweep_degrees > 0.0), "{id}");
        let needles: Vec<_> = control.slots_of(SlotRole::Needle).collect();
        assert_eq!(needles.len(), 1, "{id} has {} needles", needles.len());
        let needle = NodeId(needles[0]);
        let subtree = scene.walk(root);
        assert!(subtree.contains(&needle) && needle != root, "{id}'s needle is not a descendant");
        // It turns about its parent's centre, which must be the knob's
        // shaft: a square frame the needle fills.
        let n = scene.node(needle).unwrap();
        let parent = scene.node(n.parent.unwrap()).unwrap();
        assert!((parent.size.width - parent.size.height).abs() < 1e-9, "{id}: pivot frame not square");
        assert!(subtree.len() < 150, "{id} is {} nodes", subtree.len());
        assert_eq!(control.slots_of(SlotRole::Label).count(), 1, "{id}");
    }
}

#[test]
fn sizes_scale_with_place_size() {
    for id in knob_ids() {
        let (a, ra, _) = placed(id, &Place::new("A").size(50.0)).unwrap();
        let (b, rb, _) = placed(id, &Place::new("B").size(100.0)).unwrap();
        let (wa, wb) = (a.node(ra).unwrap().size.width, b.node(rb).unwrap().size.width);
        assert!((wb / wa - 2.0).abs() < 0.02, "{id}: {wa} → {wb}");
    }
}

#[test]
fn unknown_variants_are_rejected_and_known_ones_place() {
    for def in components().into_iter().filter(|c| c.family == Family::Knob) {
        let err = placed(def.id, &Place::new("x").variant("plaid")).unwrap_err();
        assert!(err.contains("plaid"), "{err}");
        for v in def.variants {
            placed(def.id, &Place::new("x").variant(*v)).unwrap_or_else(|e| panic!("{} {v}: {e}", def.id));
        }
    }
}

#[test]
fn a_legend_and_a_scale_are_honoured() {
    let (scene, root, _) = placed("knob.bakelite", &Place::new("x").legend("Input").scale(&["-48", "0", "+12"])).unwrap();
    let texts: Vec<String> =
        scene.walk(root).into_iter().filter_map(|id| scene.node(id)?.text().map(|t| t.characters.clone())).collect();
    for s in ["Input", "-48", "0", "+12"] {
        assert!(texts.iter().any(|t| t == s), "missing {s} in {texts:?}");
    }
}

#[test]
fn every_knob_lints_clean_bound_to_a_param() {
    let catalog = surface_memory::demo_catalog();
    for id in knob_ids() {
        let (scene, _, _) = placed(id, &Place::new("Gain").bind(Binding::param("amp/gain"))).unwrap();
        let issues = design_lint::lint(&scene, &catalog, &design_lint::Config::default());
        assert!(issues.is_empty(), "{id}: {}", issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"));
    }
}
