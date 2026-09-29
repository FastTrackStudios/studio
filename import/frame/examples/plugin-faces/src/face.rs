//! What a face is, and the scaffolding every face uses.

use std::collections::BTreeMap;

use document_proto::{Node, NodeId, NodeKind, Scene};
use surface_proto::{Catalog, Control, ParamInfo};
use surface_widgets::{Builder, Theme};

/// One plugin face: a surface plus the params it binds.
pub struct Face {
    /// File stem, e.g. `"01-1176-black"`.
    pub file: &'static str,
    /// Which plugin's catalog it belongs to (`comp`, `eq`, `delay`, …).
    pub plugin: &'static str,
    pub scene: Scene,
    /// Params this face binds (merged into `<plugin>.catalog.json`).
    pub params: Vec<ParamInfo>,
    pub streams: Vec<surface_proto::StreamInfo>,
}

/// A fresh one-page scene with a builder over it; returns the root frame
/// (unsized, no layout — place panels/components with explicit positions,
/// or give it a layout).
pub fn canvas(name: &str, build: impl FnOnce(&mut Builder<'_>, NodeId)) -> Scene {
    let mut scene = Scene::new("Surface");
    let page = scene.pages()[0];
    let mut controls: BTreeMap<NodeId, Control> = BTreeMap::new();
    let theme = Theme::stage_dark();
    {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        let root = b.add(page, Node::new(NodeKind::Frame, name));
        build(&mut b, root);
    }
    for (id, c) in controls {
        if let Some(n) = scene.node_mut(id) {
            n.control = Some(c);
        }
    }
    let root = scene.node(page).and_then(|p| p.children.first().copied()).expect("root frame");
    let hug = scene.hug_ops(root);
    scene.transact(hug).expect("hug");
    scene
}

/// Merge faces' params/streams into one catalog per plugin.
#[must_use]
pub fn catalogs(faces: &[Face]) -> BTreeMap<&'static str, Catalog> {
    let mut out: BTreeMap<&'static str, Catalog> = BTreeMap::new();
    for f in faces {
        let c = out.entry(f.plugin).or_insert_with(|| Catalog { id: format!("fts.{}", f.plugin), version: 1, ..Catalog::default() });
        for p in &f.params {
            if c.param(&p.addr).is_none() {
                c.params.push(p.clone());
            }
        }
        for s in &f.streams {
            if c.stream(&s.id).is_none() {
                c.streams.push(s.clone());
            }
        }
    }
    out
}
