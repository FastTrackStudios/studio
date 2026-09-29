//! The document path (Figma → `Scene` → primitives) must draw exactly what
//! the Figma-JSON path draws, on every parity scenario. This is the gate for
//! moving the renderer off Figma JSON.

use std::path::{Path, PathBuf};

use figma_model::FrameDocument;

fn scenarios() -> Vec<(String, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/parity/scenarios");
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .expect("parity scenarios dir")
        .filter_map(|e| {
            let d = e.ok()?.path();
            let json = [d.join("latest-bridge-export.json"), d.join("export/latest-bridge-export.json")]
                .into_iter()
                .find(|p| p.exists())?;
            Some((d.file_name()?.to_string_lossy().to_string(), json))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn scene_path_draws_what_figma_path_draws() {
    let all = scenarios();
    assert!(all.len() >= 20, "expected the parity corpus, found {} scenarios", all.len());
    let mut failures = Vec::new();
    let mut total = 0usize;
    let mut images = 0usize;
    let mut snapshots = 0usize;
    for (name, json) in &all {
        let bytes = std::fs::read(json).unwrap();
        let fig = FrameDocument::from_source_bytes(&bytes).unwrap();
        let root = fig.pages.first().copied().unwrap_or(fig.root);
        let scene = figma_import::import_document(&fig);
        let scene_root = document_proto::NodeId(root.0);

        let old_boxes = render::build_layout_boxes(&fig, root);
        let new_boxes = render::build_scene_layout_boxes(&scene, scene_root);
        for (id, b) in &old_boxes {
            if new_boxes.get(&document_proto::NodeId(id.0)) != Some(b) {
                failures.push(format!("{name}: layout box differs for {id}"));
                break;
            }
        }

        let old = render::build_paint_primitives(&fig, root);
        let new = render::build_scene_primitives(&scene, scene_root);
        total += new.len();
        images += scene.assets.values().filter(|a| a.mime.starts_with("image/") && a.mime != "image/svg+xml").count();
        snapshots += scene.nodes.values().filter(|n| n.svg_snapshot.is_some()).count();
        if old != new {
            let at = old.iter().zip(&new).position(|(a, b)| a != b).unwrap_or(old.len().min(new.len()));
            failures.push(format!(
                "{name}: primitives differ (old {} / new {}) at #{at}\n  old: {:?}\n  new: {:?}",
                old.len(),
                new.len(),
                old.get(at),
                new.get(at)
            ));
        }
    }
    eprintln!("compared {total} primitives across {} scenarios ({images} image assets, {snapshots} svg snapshots)", all.len());
    assert!(total > 500, "suspiciously few primitives: {total}");
    assert!(failures.is_empty(), "{} of {} scenarios differ:\n{}", failures.len(), all.len(), failures.join("\n"));
}
