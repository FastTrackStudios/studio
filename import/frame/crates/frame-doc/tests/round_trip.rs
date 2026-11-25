//! Scene → figcraft document → Scene gives back the same scene, for every parity scenario (Figma
//! exports) and every plugin face (frame#8).

use std::path::Path;

use frame::document::proto::Scene;
use frame_doc::{doc_to_scene, scene_to_doc};

/// Where `a` and `b` differ, node by node and field by field (empty: equal).
fn diff(a: &Scene, b: &Scene) -> Vec<String> {
    let mut out = Vec::new();
    if a.root != b.root {
        out.push(format!("root {} vs {}", a.root, b.root));
    }
    if a.variables != b.variables {
        out.push("variables".into());
    }
    if a.assets != b.assets {
        out.push(format!(
            "assets {:?} vs {:?}",
            a.assets.keys().collect::<Vec<_>>(),
            b.assets.keys().collect::<Vec<_>>()
        ));
    }
    for (id, n) in &a.nodes {
        let Some(m) = b.nodes.get(id) else {
            out.push(format!("node {id} ({}) missing", n.name));
            continue;
        };
        if n == m {
            continue;
        }
        let (x, y) = (
            serde_json::to_value(n).unwrap(),
            serde_json::to_value(m).unwrap(),
        );
        for (k, v) in x.as_object().unwrap() {
            if y.get(k) != Some(v) {
                out.push(format!(
                    "node {id} ({}) .{k}: {v} vs {}",
                    n.name,
                    y.get(k).unwrap_or(&serde_json::Value::Null)
                ));
            }
        }
    }
    out.extend(
        b.nodes
            .keys()
            .filter(|id| !a.nodes.contains_key(id))
            .map(|id| format!("extra node {id}")),
    );
    out
}

fn assert_round_trip(name: &str, scene: &Scene) {
    let doc = scene_to_doc(scene).unwrap_or_else(|e| panic!("{name}: scene_to_doc: {e}"));
    let back = doc_to_scene(&doc).unwrap_or_else(|e| panic!("{name}: doc_to_scene: {e}"));
    let d = diff(scene, &back);
    assert!(
        d.is_empty(),
        "{name}: {} differences:\n{}",
        d.len(),
        d.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(scene, &back, "{name}");
}

#[test]
fn plugin_faces_round_trip() {
    let faces = plugin_faces::all();
    assert!(!faces.is_empty());
    for f in &faces {
        assert_round_trip(f.file, &f.scene);
    }
}

#[test]
fn parity_scenarios_round_trip() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../frame/render/tests/parity/scenarios");
    let mut n = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for p in entries {
        let export = p.join("latest-bridge-export.json");
        let bytes = std::fs::read(&export).unwrap_or_else(|e| panic!("{}: {e}", export.display()));
        let scene = frame::figma::import::import_bytes(&bytes)
            .unwrap_or_else(|e| panic!("{}: {e}", export.display()));
        assert_round_trip(&p.file_name().unwrap().to_string_lossy(), &scene);
        n += 1;
    }
    assert!(n >= 26, "only {n} scenarios");
}

/// A scene that isn't a document of pages (a bare frame as the root) round-trips too.
#[test]
fn bare_root_round_trips() {
    use frame::document::proto::{Node, NodeKind};
    let mut scene = Scene::new("Page");
    let mut root = Node::new(NodeKind::Frame, "Panel");
    let child = Node::new(NodeKind::Rectangle, "Lamp");
    root.children.push(child.id);
    let child = Node {
        parent: Some(root.id),
        ..child
    };
    scene.nodes.clear();
    scene.root = root.id;
    scene.nodes.insert(root.id, root);
    scene.nodes.insert(child.id, child);
    assert_round_trip("bare", &scene);
}
