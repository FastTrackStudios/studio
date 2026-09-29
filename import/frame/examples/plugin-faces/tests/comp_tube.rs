//! The tube compressor faces: each lints clean against the merged `comp`
//! catalog, binds only what exists, and meters gain reduction so that no
//! reduction rests on 0 VU.

use document_proto::Scene;
use plugin_faces::{Face, catalogs, comp_tube};
use surface_kit::meters::scale::Card;
use surface_proto::{Catalog, ControlKind, Source, StreamKind};

fn comp_catalog() -> Catalog {
    catalogs(&plugin_faces::all()).remove("comp").expect("comp catalog")
}

fn controls(scene: &Scene) -> Vec<(&document_proto::Node, &surface_proto::Control)> {
    scene.walk(scene.root).into_iter().filter_map(|id| scene.node(id)).filter_map(|n| n.control.as_ref().map(|c| (n, c))).collect()
}

fn lint_errors(face: &Face, scene: &Scene, catalog: &Catalog) {
    let issues = design_lint::lint(scene, catalog, &design_lint::Config::default());
    let errors: Vec<String> = issues.iter().filter(|i| i.severity == design_lint::Severity::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{}: {} lint errors:\n{}", face.file, errors.len(), errors.join("\n"));
}

#[test]
fn there_are_four_tube_faces() {
    let files: Vec<&str> = comp_tube::faces().iter().map(|f| f.file).collect();
    assert_eq!(files, ["04-la2a", "05-cl1b", "06-fairchild-670", "07-vari-mu"]);
    assert!(comp_tube::faces().iter().all(|f| f.plugin == "comp"));
}

#[test]
fn every_face_lints_without_errors() {
    let catalog = comp_catalog();
    for face in comp_tube::faces() {
        lint_errors(&face, &face.scene, &catalog);
        // …and so does the markup it is written out as.
        let text = document_text::print(&face.scene);
        let parsed = document_text::parse(&text, Default::default()).unwrap_or_else(|e| panic!("{}: {e}", face.file));
        lint_errors(&face, &parsed, &catalog);
    }
}

#[test]
fn every_binding_exists_and_every_param_is_bound() {
    let catalog = comp_catalog();
    for face in comp_tube::faces() {
        let prefix = face.file;
        let mut bound = std::collections::BTreeSet::new();
        for (node, c) in controls(&face.scene) {
            match c.binding.as_ref().map(|b| &b.source) {
                Some(Source::Param { addr }) => {
                    assert!(catalog.param(addr).is_some(), "{prefix}: \"{}\" binds unknown param {addr}", node.name);
                    assert!(face.params.iter().any(|p| &p.addr == addr), "{prefix}: {addr} is not this face's");
                    bound.insert(addr.0.clone());
                }
                Some(Source::Stream { id, .. }) => {
                    assert!(catalog.stream(id).is_some(), "{prefix}: \"{}\" binds unknown stream {}", node.name, id.0);
                    assert!(face.streams.iter().any(|s| &s.id == id), "{prefix}: {} is not this face's", id.0);
                }
                other => panic!("{prefix}: \"{}\" has binding {other:?}", node.name),
            }
        }
        for p in &face.params {
            assert!(p.featured, "{prefix}: {} not featured", p.addr);
            assert!(bound.contains(&p.addr.0), "{prefix}: {} has no control", p.addr);
        }
    }
}

#[test]
fn params_and_streams_are_namespaced_per_face() {
    for (face, ns) in comp_tube::faces().iter().zip(["la2a/", "cl1b/", "fairchild/", "varimu/"]) {
        assert!(face.params.iter().all(|p| p.addr.0.starts_with(ns)), "{}", face.file);
        assert!(face.streams.iter().all(|s| s.id.0.starts_with(ns)), "{}", face.file);
    }
}

#[test]
fn no_gain_reduction_rests_on_zero_vu() {
    for face in comp_tube::faces() {
        let gr = face.streams.iter().find(|s| matches!(s.kind, StreamKind::GainReduction)).expect("a GR stream");
        let meters: Vec<_> = controls(&face.scene)
            .into_iter()
            .filter(|(_, c)| matches!(&c.binding, Some(b) if matches!(&b.source, Source::Stream { id, .. } if *id == gr.id)))
            .collect();
        assert_eq!(meters.len(), 1, "{}: one GR meter", face.file);
        let (_, c) = meters[0];
        let ControlKind::Gauge { floor_db, ceiling_db, .. } = c.kind else { panic!("{}: GR meter is not a gauge", face.file) };
        let curve = &c.binding.as_ref().expect("binding").curve;
        let rest = curve.apply((0.0 - floor_db) / (ceiling_db - floor_db));
        assert!((rest - Card::Vu.fraction(0.0)).abs() < 1e-9, "{}: 0 dB GR rests at {rest}", face.file);
        // …and 10 dB of reduction lands on the printed -10.
        let ten = curve.apply((-10.0 - floor_db) / (ceiling_db - floor_db));
        assert!((ten - Card::Vu.fraction(-10.0)).abs() < 1e-9, "{}", face.file);
    }
}
