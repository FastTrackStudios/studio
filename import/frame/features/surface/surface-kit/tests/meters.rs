//! The meter family: every VU face places, wires one needle about its
//! pivot, scales with its size, lints clean, and reads on its printed marks.

use std::collections::BTreeMap;

use document_proto::{Node, NodeId, NodeKind, Scene};
use surface_kit::meters::scale::{self, Card, PIVOT_X, PIVOT_Y};
use surface_kit::{Family, Place, components, place};
use surface_proto::{Binding, ControlKind, EntryId, SlotRole, Source, StreamFrame};
use surface_runtime::{Runtime, Surface};
use surface_widgets::{Builder, Theme};

fn level() -> Binding {
    Binding { source: Source::Stream { id: EntryId::new("out_level"), channel: Some(0) }, ..Binding::stream("out_level") }
}

/// Place `id` in a fresh scene; returns the scene (controls attached) and
/// the component's root.
fn placed(id: &str, p: &Place) -> (Scene, NodeId) {
    let mut scene = Scene::new("Meters");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
    let holder = b.add(page, Node::new(NodeKind::Frame, "holder"));
    let root = place(&mut b, holder, id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
    for (nid, c) in &controls {
        scene.node_mut(*nid).expect("control node").control = Some(c.clone());
    }
    (scene, root)
}

fn meters() -> Vec<surface_kit::ComponentDef> {
    components().into_iter().filter(|c| c.family == Family::Meter).collect()
}

#[test]
fn every_meter_and_variant_places_with_one_needle_in_a_pivot_frame() {
    assert!(meters().len() >= 5);
    for def in meters() {
        assert_eq!(def.binds, "level stream");
        let variants: Vec<Option<&str>> = std::iter::once(None).chain(def.variants.iter().map(|v| Some(*v))).collect();
        for v in variants {
            let p = Place { variant: v.map(str::to_string), ..Place::new("Out").bind(level()) };
            let (scene, root) = placed(def.id, &p);
            let control = scene.node(root).and_then(|n| n.control.clone()).expect("root carries the control");
            assert!(matches!(control.kind, ControlKind::Gauge { .. }), "{} {v:?}", def.id);
            let needles: Vec<_> = control.slots_of(SlotRole::Needle).collect();
            assert_eq!(needles.len(), 1, "{} {v:?}: one Needle slot", def.id);
            let needle = scene.node(NodeId(needles[0])).expect("needle node");
            let pivot = scene.node(needle.parent.expect("needle has a parent")).expect("pivot");
            assert!(matches!(pivot.kind, NodeKind::Frame), "{}: the needle sits in a frame", def.id);
            assert!((pivot.size.width - pivot.size.height).abs() < 1e-6, "{}: the pivot frame is square", def.id);
            // The pivot frame is centred on the movement's pivot.
            let card = scene.node(pivot.parent.expect("card")).expect("card");
            let k = card.size.width / scale::VU_W;
            let c = (pivot.transform.tx + pivot.size.width / 2.0, pivot.transform.ty + pivot.size.height / 2.0);
            assert!((c.0 - PIVOT_X * k).abs() < 1e-6 && (c.1 - PIVOT_Y * k).abs() < 1e-6, "{}: pivot at {c:?}", def.id);
            // Drawn at the centre of travel: straight up, centred on the pivot.
            assert!(needle.transform.rotation().abs() < 1e-9);
            assert!((needle.transform.tx + needle.size.width / 2.0 - pivot.size.width / 2.0).abs() < 1e-6);
        }
    }
}

#[test]
fn unknown_variants_are_rejected() {
    let mut scene = Scene::new("x");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
    assert!(place(&mut b, page, "meter.vu_ivory", &Place::new("x").variant("red")).is_err(), "only the backlit face takes a tint");
}

#[test]
fn meters_scale_with_place_size() {
    for def in meters() {
        for size in [140.0, 220.0, 400.0] {
            for v in [None, Some("open")] {
                let p = Place { variant: v.map(str::to_string), ..Place::new("Out").size(size).bind(level()) };
                let (scene, root) = placed(def.id, &p);
                let n = scene.node(root).expect("root");
                assert!((n.size.width - size).abs() < 1e-6, "{} {v:?} at {size}: {}", def.id, n.size.width);
                assert!(n.size.height > size * 0.5 && n.size.height < size * 0.8, "{}: {}", def.id, n.size.height);
            }
        }
    }
}

#[test]
fn meters_lint_clean_bound_to_the_output_level() {
    let catalog = surface_memory::demo_catalog();
    for def in meters() {
        let (scene, _) = placed(def.id, &Place::new("Out").bind(level()));
        let issues = design_lint::lint(&scene, &catalog, &design_lint::Config::default());
        let (errors, warnings, _) = design_lint::counts(&issues);
        assert_eq!(
            (errors, warnings),
            (0, 0),
            "{}: {}",
            def.id,
            issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
        );
    }
}

/// The needle's angle (degrees) with `db` on the bound stream.
fn needle_angle(id: &str, variant: Option<&str>, db: f64) -> f64 {
    let p = Place { variant: variant.map(str::to_string), ..Place::new("Out").bind(level()) };
    let (scene, root) = placed(id, &p);
    let needle = NodeId(scene.node(root).and_then(|n| n.control.as_ref()).and_then(|c| c.slots_of(SlotRole::Needle).next()).expect("needle"));
    let mut rt = Runtime::new(Surface::from_scene(scene), surface_memory::demo_catalog());
    rt.apply_frame(StreamFrame { id: EntryId::new("out_level"), values: vec![db, db] }, 0.0);
    rt.frame(0.0).node(needle).expect("needle").transform.rotation().to_degrees()
}

/// The angle at which a tick for `v` is printed, measured off its drawn
/// position about the pivot.
fn printed_angle(card: Card, v: f64) -> f64 {
    let (x, y) = scale::point(card, v, 6.0);
    (x - PIVOT_X).atan2(PIVOT_Y - y).to_degrees()
}

#[test]
fn the_needle_lands_on_the_printed_vu_marks() {
    for db in [-20.0, -10.0, -7.0, -5.0, 0.0, 3.0] {
        let got = needle_angle("meter.vu_ivory", None, db);
        let want = printed_angle(Card::Vu, db);
        assert!((got - want).abs() < 0.5, "{db} dB: needle at {got:.2}°, printed at {want:.2}°");
    }
    // 0 VU sits about three-quarters of the way across, not at the middle.
    assert!(needle_angle("meter.vu_amber", None, 0.0) > 10.0);
}

#[test]
fn the_decibel_card_reads_linearly() {
    for db in [-40.0, -20.0, 0.0, 20.0] {
        let got = needle_angle("meter.vu_amber_blue", None, db);
        let want = printed_angle(Card::Decibels, db);
        assert!((got - want).abs() < 0.5, "{db} dB: needle at {got:.2}°, printed at {want:.2}°");
    }
    // A card variant rewires the law with the print.
    let got = needle_angle("meter.vu_ivory", Some("db"), -10.0);
    assert!((got - printed_angle(Card::Decibels, -10.0)).abs() < 0.5);
}
