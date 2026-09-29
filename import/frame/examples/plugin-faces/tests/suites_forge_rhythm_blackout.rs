//! The Djent Forge, Modern Rhythm and Blackout Gain suites: every unit
//! lints clean against its suite's merged catalog, every binding resolves
//! to the unit's own params and streams, every param has a control, each
//! unit keeps to its own namespace, pedals are one height, and no unit
//! prints a maker's, artist's or product's real name.

use design_lint::{Config, Severity, lint};
use surface_proto::Source;

fn suites() -> Vec<(&'static str, Vec<plugin_faces::Face>)> {
    vec![
        (plugin_faces::suite_djent_forge::PLUGIN, plugin_faces::suite_djent_forge::faces()),
        (plugin_faces::suite_modern_rhythm::PLUGIN, plugin_faces::suite_modern_rhythm::faces()),
        (plugin_faces::suite_blackout_gain::PLUGIN, plugin_faces::suite_blackout_gain::faces()),
    ]
}

fn root_size(f: &plugin_faces::Face) -> (f64, f64) {
    let page = f.scene.pages()[0];
    let root = f.scene.node(page).and_then(|p| p.children.first().copied()).expect("root");
    let n = f.scene.node(root).expect("root node");
    (n.size.width, n.size.height)
}

#[test]
fn every_unit_belongs_to_its_suite() {
    for (plugin, faces) in suites() {
        assert!(!faces.is_empty(), "{plugin}: no units");
        for f in &faces {
            assert_eq!(f.plugin, plugin, "{}", f.file);
        }
    }
}

#[test]
fn every_unit_lints_without_errors() {
    let all = plugin_faces::all();
    let catalogs = plugin_faces::catalogs(&all);
    for (plugin, faces) in suites() {
        for f in faces {
            let issues = lint(&f.scene, &catalogs[plugin], &Config::default());
            let errors: Vec<String> = issues.iter().filter(|i| i.severity == Severity::Error).map(ToString::to_string).collect();
            assert!(errors.is_empty(), "{plugin}/{}: {} lint errors:\n{}", f.file, errors.len(), errors.join("\n"));
        }
    }
}

#[test]
fn every_binding_resolves_and_every_param_is_bound() {
    for (_, faces) in suites() {
        for f in faces {
            let mut addrs = Vec::new();
            for id in f.scene.walk(f.scene.root) {
                let Some(c) = f.scene.node(id).and_then(|n| n.control.as_ref()) else { continue };
                // A handle binds its axes, not the control.
                if let surface_proto::ControlKind::Handle { x, y, wheel, .. } = &c.kind
                    && c.binding.is_none()
                {
                    let axes: Vec<&surface_proto::Binding> = [x, y, wheel].into_iter().flatten().collect();
                    assert!(!axes.is_empty(), "{}: handle {id} is unbound", f.file);
                    for b in axes {
                        let Source::Param { addr } = &b.source else { panic!("{}: handle {id} binds a non-param", f.file) };
                        assert!(f.params.iter().any(|p| &p.addr == addr), "{}: unknown param {}", f.file, addr.0);
                        addrs.push(addr.0.clone());
                    }
                    continue;
                }
                let Some(b) = &c.binding else { panic!("{}: control {id} is unbound", f.file) };
                match &b.source {
                    Source::Param { addr } => {
                        assert!(f.params.iter().any(|p| &p.addr == addr), "{}: unknown param {}", f.file, addr.0);
                        addrs.push(addr.0.clone());
                    }
                    Source::Stream { id, .. } => {
                        assert!(f.streams.iter().any(|s| &s.id == id), "{}: unknown stream {}", f.file, id.0);
                        addrs.push(id.0.clone());
                    }
                    other => panic!("{}: unexpected binding {other:?}", f.file),
                }
            }
            for p in &f.params {
                assert!(addrs.contains(&p.addr.0), "{}: param {} has no control", f.file, p.addr.0);
            }
            for s in &f.streams {
                assert!(addrs.contains(&s.id.0), "{}: stream {} is not shown", f.file, s.id.0);
            }
            assert!(f.params.iter().any(|p| p.featured), "{}: no featured params", f.file);
        }
    }
}

#[test]
fn each_unit_keeps_to_its_own_namespace() {
    for (plugin, faces) in suites() {
        let mut seen: Vec<String> = Vec::new();
        for f in faces {
            let ns = f.params[0].addr.0.split('/').next().unwrap_or_default().to_string();
            for p in &f.params {
                assert!(p.addr.0.starts_with(&format!("{ns}/")), "{}: {} is outside {ns}/", f.file, p.addr.0);
            }
            for s in &f.streams {
                assert!(s.id.0.starts_with(&format!("{ns}/")), "{}: stream {} is outside {ns}/", f.file, s.id.0);
            }
            assert!(!seen.contains(&ns), "{plugin}/{}: namespace {ns} is shared", f.file);
            seen.push(ns);
        }
    }
}

#[test]
fn pedals_share_one_height_and_a_class_width() {
    const WIDTHS: &[f64] = &[92.0, 144.0, 156.0, 172.0, 222.0, 241.0, 286.0, 404.0];
    for (_, faces) in suites() {
        for f in faces {
            let (w, h) = root_size(&f);
            if f.file.contains("-treadle-") && !f.file.contains("-custom") {
                assert!((w - 241.0).abs() < 0.5 && (h - 600.0).abs() < 0.5 || (h - 286.0).abs() < 0.5, "{}: treadle is {w}×{h}", f.file);
            } else if f.file.contains("-pre-") || f.file.contains("-post-") && !f.file.contains("-tabletop-") {
                assert!((h - 286.0).abs() < 0.5, "{}: a pedal {h} tall", f.file);
                assert!(WIDTHS.iter().any(|c| (c - w).abs() < 0.5), "{}: {w} is no pedal class", f.file);
            }
        }
    }
}

#[test]
fn no_real_names_are_printed() {
    const BANNED: &[&str] = &[
        "neural", "archetype", "misha", "mansoor", "nolly", "getgood", "fortin", "nameless", "horizon", "zuul", "hexdrive", "grind", "periphery",
        "bogner", "mesa", "marshall", "fender", "vox", "peavey", "5150", "celestion", "shure", "sm57", "royer", "neumann", "sennheiser", "md421",
    ];
    for (_, faces) in suites() {
        for f in faces {
            let file = f.file.to_ascii_lowercase();
            for w in BANNED {
                assert!(!file.contains(w), "{}: file name carries `{w}`", f.file);
            }
            for id in f.scene.walk(f.scene.root) {
                let Some(n) = f.scene.node(id) else { continue };
                let mut words = vec![n.name.to_ascii_lowercase()];
                if let document_proto::NodeKind::Text(t) = &n.kind {
                    words.push(t.characters.to_ascii_lowercase());
                }
                for s in words {
                    for w in BANNED {
                        assert!(!s.contains(w), "{}: prints `{w}` ({s})", f.file);
                    }
                }
            }
            for p in &f.params {
                let s = format!("{} {}", p.addr.0, p.name).to_ascii_lowercase();
                for w in BANNED {
                    assert!(!s.contains(w), "{}: param {s} carries `{w}`", f.file);
                }
            }
        }
    }
}
