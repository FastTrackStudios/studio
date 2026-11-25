//! What reading Frame data out of `plugin_data` JSON costs on a 500-control surface (frame#8).
//!
//! The budget is the designer plan's: re-recording after an edit within 8 ms, so a cold read of
//! the whole surface's Frame data (every node parsed, the slot index built) must fit in it, and a
//! warm read (the cache) in the 1 ms per-frame encode budget. Over budget would call for a typed
//! slot on figcraft's `Node` (the `fts` branch). Run with
//! `cargo test --workspace --test parse_cost -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use frame::document::proto::{Emission, Node, NodeId, NodeKind, Scene, ShaderFx, TextData};
use frame::surface::proto::{Binding, Control, ControlKind, Slot, SlotRole};
use frame_doc::doc::{Document, NodeId as FcId};
use frame_doc::{FrameCache, FrameData, control_of, controls, keys, scene_to_doc, slot_index};

const CONTROLS: usize = 500;
const RUNS: usize = 7;

fn add(scene: &mut Scene, parent: NodeId, mut n: Node) -> NodeId {
    n.parent = Some(parent);
    let id = n.id;
    scene.nodes.get_mut(&parent).unwrap().children.push(id);
    scene.nodes.insert(id, n);
    id
}

/// 500 knobs, each with a needle, a value readout and a lamp (an emission and a shader on it).
fn surface() -> Scene {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    for i in 0..CONTROLS {
        let knob = add(
            &mut scene,
            page,
            Node::new(NodeKind::Frame, format!("Knob {i}")),
        );
        let needle = add(&mut scene, knob, Node::new(NodeKind::Rectangle, "Needle"));
        let text = TextData {
            characters: "0.0 dB".into(),
            ..TextData::default()
        };
        let value = add(&mut scene, knob, Node::new(NodeKind::Text(text), "Value"));
        let mut lamp = Node::new(NodeKind::Ellipse, "Lamp");
        lamp.emission = Some(Emission {
            color: frame::document::proto::Color::rgba(1.0, 0.2, 0.1, 1.0),
            intensity: 1.5,
            radius: 6.0,
        });
        lamp.shader = Some(ShaderFx {
            shader: "glass".into(),
            params: [("strength".to_string(), 0.4)].into(),
            inputs: Vec::new(),
        });
        let lamp = add(&mut scene, knob, lamp);
        let mut c = Control::new(ControlKind::Knob {
            sweep_degrees: 270.0,
        });
        c.binding = Some(Binding::param(format!("p{i}")));
        c.default = Some(0.5);
        c.slots = vec![
            Slot {
                role: SlotRole::Needle,
                node: needle.0,
            },
            Slot {
                role: SlotRole::Value,
                node: value.0,
            },
            Slot {
                role: SlotRole::Led,
                node: lamp.0,
            },
        ];
        scene.nodes.get_mut(&knob).unwrap().control = Some(c);
    }
    scene
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn time<T>(mut f: impl FnMut() -> T) -> Duration {
    let mut runs = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let t = Instant::now();
        std::hint::black_box(f());
        runs.push(t.elapsed());
    }
    median(runs)
}

fn ids(doc: &Document) -> Vec<FcId> {
    doc.pages.iter().flat_map(|p| doc.subtree(*p)).collect()
}

#[test]
#[ignore = "benchmark: run with --ignored --nocapture"]
fn parse_cost_500_controls() {
    let scene = surface();
    let convert = time(|| scene_to_doc(&scene).unwrap());
    let mut doc = scene_to_doc(&scene).unwrap();
    let all = ids(&doc);
    let knobs: Vec<FcId> = all
        .iter()
        .copied()
        .filter(|i| {
            doc.node(*i)
                .unwrap()
                .plugin_data
                .contains_key(keys::CONTROL)
        })
        .collect();
    assert_eq!(knobs.len(), CONTROLS);
    let bytes: usize = all
        .iter()
        .flat_map(|i| doc.node(*i).unwrap().plugin_data.iter())
        .filter(|(k, _)| keys::DATA.contains(&k.as_str()))
        .map(|(_, v)| v.len())
        .sum();

    let cold = time(|| {
        all.iter()
            .map(|i| FrameData::of(doc.node(*i).unwrap()))
            .filter(|d| d.control.is_some())
            .count()
    });
    let index = time(|| slot_index(&doc));
    let mut cache = FrameCache::new();
    for i in &all {
        cache.get(&doc, *i);
    }
    let warm = time(|| {
        all.iter()
            .filter_map(|i| cache.get(&doc, *i))
            .filter(|d| d.control.is_some())
            .count()
    });
    let one = time(|| control_of(&doc, knobs[250]));
    let whole = time(|| controls(&doc));
    assert_eq!(controls(&doc).len(), CONTROLS);

    // An edit: one control's binding changes; the next read parses that node again, the rest hit.
    let parses = cache.parses();
    doc.node_mut(knobs[7]).unwrap().plugin_data.insert(
        keys::BINDING.into(),
        serde_json::to_string(&Binding::param("edited")).unwrap(),
    );
    let t = Instant::now();
    let hits = all
        .iter()
        .filter_map(|i| cache.get(&doc, *i))
        .filter(|d| d.control.is_some())
        .count();
    let edit = t.elapsed();
    assert_eq!((hits, cache.parses() - parses), (CONTROLS, 1));

    let cold_total = cold + index;
    let verdict = if cold_total <= Duration::from_millis(8) && warm <= Duration::from_millis(1) {
        "within budget"
    } else {
        "OVER budget"
    };
    eprintln!(
        "parse_cost ({} build): {CONTROLS} controls, {} nodes, {bytes} bytes of frame:* JSON\n  scene_to_doc {convert:?}\n  cold parse of every node {cold:?} ({:?}/control)\n  slot index {index:?}\n  cold total {cold_total:?} (budget 8 ms)\n  warm read through the cache {warm:?} (budget 1 ms)\n  read after a one-control edit {edit:?}\n  control_of one control {one:?}\n  every control whole (controls) {whole:?}\n  => {verdict}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        all.len(),
        cold / u32::try_from(CONTROLS).unwrap(),
    );
}
