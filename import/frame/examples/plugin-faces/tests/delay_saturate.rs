//! The delay and saturation faces lint clean against their plugin's merged
//! catalog, every binding resolves to the face's own params and streams,
//! and each face keeps to its own namespace.

use design_lint::{Config, Severity, lint};
use surface_proto::{ParamKind, Source};

const DELAY: &[&str] = &["01-digital", "02-tape", "03-analog", "04-pitch", "05-rhythmic", "06-special"];
const SATURATE: &[&str] = &["01-tube", "02-tape", "03-transformer", "04-transistor", "05-digital"];

fn faces() -> Vec<plugin_faces::Face> {
    let mut v = plugin_faces::delay::faces();
    v.extend(plugin_faces::saturate::faces());
    v
}

#[test]
fn every_face_is_here() {
    let delay: Vec<&str> = plugin_faces::delay::faces().iter().map(|f| f.file).collect();
    assert_eq!(delay, DELAY);
    let sat: Vec<&str> = plugin_faces::saturate::faces().iter().map(|f| f.file).collect();
    assert_eq!(sat, SATURATE);
    assert!(plugin_faces::delay::faces().iter().all(|f| f.plugin == "delay"));
    assert!(plugin_faces::saturate::faces().iter().all(|f| f.plugin == "saturate"));
}

#[test]
fn every_face_lints_without_errors() {
    let all = plugin_faces::all();
    let catalogs = plugin_faces::catalogs(&all);
    for f in faces() {
        let catalog = &catalogs[f.plugin];
        let issues = lint(&f.scene, catalog, &Config::default());
        let errors: Vec<String> = issues.iter().filter(|i| i.severity == Severity::Error).map(ToString::to_string).collect();
        assert!(errors.is_empty(), "{}/{}: {} lint errors:\n{}", f.plugin, f.file, errors.len(), errors.join("\n"));
    }
}

#[test]
fn every_binding_resolves_to_the_faces_own_entries() {
    for f in faces() {
        let mut bound = 0;
        for id in f.scene.walk(f.scene.root) {
            let Some(c) = f.scene.node(id).and_then(|n| n.control.as_ref()) else { continue };
            let Some(b) = &c.binding else { panic!("{}: control {id} is unbound", f.file) };
            bound += 1;
            match &b.source {
                Source::Param { addr } => assert!(f.params.iter().any(|p| &p.addr == addr), "{}: unknown param {}", f.file, addr.0),
                Source::Stream { id, .. } => assert!(f.streams.iter().any(|s| &s.id == id), "{}: unknown stream {}", f.file, id.0),
                other => panic!("{}: unexpected binding {other:?}", f.file),
            }
        }
        assert!(bound > 0, "{}: nothing bound", f.file);
        assert!(f.params.iter().any(|p| p.featured), "{}: no featured params", f.file);
    }
}

#[test]
fn every_param_and_stream_is_bound_by_its_face() {
    for f in faces() {
        let mut addrs = Vec::new();
        for id in f.scene.walk(f.scene.root) {
            if let Some(b) = f.scene.node(id).and_then(|n| n.control.as_ref()).and_then(|c| c.binding.as_ref()) {
                match &b.source {
                    Source::Param { addr } => addrs.push(addr.0.clone()),
                    Source::Stream { id, .. } => addrs.push(id.0.clone()),
                    _ => {}
                }
            }
        }
        for p in &f.params {
            assert!(addrs.contains(&p.addr.0), "{}: param {} has no control", f.file, p.addr.0);
        }
        for s in &f.streams {
            assert!(addrs.contains(&s.id.0), "{}: stream {} is not metered", f.file, s.id.0);
        }
    }
}

#[test]
fn each_face_keeps_to_its_own_namespace() {
    let mut seen: Vec<(String, &str)> = Vec::new();
    for f in faces() {
        let ns = f.params[0].addr.0.split('/').next().unwrap_or_default().to_string();
        for p in &f.params {
            assert!(p.addr.0.starts_with(&format!("{ns}/")), "{}: {} is outside {ns}/", f.file, p.addr.0);
        }
        for s in &f.streams {
            assert!(s.id.0.starts_with(&format!("{ns}/")), "{}: stream {} is outside {ns}/", f.file, s.id.0);
        }
        assert!(!seen.iter().any(|(n, _)| *n == ns), "{}: namespace {ns} is shared", f.file);
        seen.push((ns, f.file));
    }
}

#[test]
fn delays_read_their_time_in_ms_and_as_a_note() {
    for f in plugin_faces::delay::faces() {
        let time = f.params.iter().find(|p| p.addr.0.ends_with("/time") || p.addr.0.ends_with("/time_l")).unwrap_or_else(|| panic!("{}: no time", f.file));
        assert_eq!(time.range.unit, surface_proto::Unit::Milliseconds, "{}", f.file);
        let div = f.params.iter().find(|p| p.addr.0.ends_with("/div") || p.addr.0.ends_with("/div_l")).unwrap_or_else(|| panic!("{}: no note division", f.file));
        let ParamKind::Choice { options } = &div.kind else { panic!("{}: division is not a choice", f.file) };
        assert_eq!(options.len(), 21, "{}: the 21 musical-time divisions", f.file);
        assert!(options.iter().any(|o| o == "1/8D"));
        for name in ["feedback", "mix"] {
            assert!(f.params.iter().any(|p| p.addr.0.ends_with(&format!("/{name}"))), "{}: no {name}", f.file);
        }
        // The time is printed live somewhere (a readout's Value slot).
        let reads_time = f.scene.walk(f.scene.root).into_iter().any(|id| {
            f.scene.node(id).and_then(|n| n.control.as_ref()).is_some_and(|c| {
                matches!(c.kind, surface_proto::ControlKind::Readout) && c.binding.as_ref().is_some_and(|b| matches!(&b.source, Source::Param { addr } if addr == &time.addr))
            })
        });
        assert!(reads_time, "{}: no ms readout", f.file);
    }
}

#[test]
fn saturators_have_drive_mix_and_output() {
    for f in plugin_faces::saturate::faces() {
        for name in ["drive", "mix", "output"] {
            assert!(f.params.iter().any(|p| p.addr.0.ends_with(&format!("/{name}"))), "{}: no {name}", f.file);
        }
        let out = f.params.iter().find(|p| p.addr.0.ends_with("/output")).expect("output");
        assert_eq!(out.range.unit, surface_proto::Unit::Db, "{}", f.file);
    }
}

#[test]
fn faces_are_sized_to_their_panels() {
    for f in faces() {
        let page = f.scene.pages()[0];
        let root = f.scene.node(page).and_then(|p| p.children.first().copied()).expect("root");
        let n = f.scene.node(root).expect("root node");
        assert!(n.size.width >= 700.0 && n.size.height >= 280.0, "{}: root is {}×{}", f.file, n.size.width, n.size.height);
    }
}
