//! The `frame:*` data on figcraft nodes: slots as marks, copies, figcraft edits, the parse cache
//! and instance overrides.

use frame::document::proto::{Node, NodeId, NodeKind, Scene, TextData};
use frame::surface::proto::{Binding, Control, ControlKind, Slot, SlotRole};
use frame_doc::doc::{Document, NodeId as FcId, NodeKind as FcKind};
use frame_doc::{
    FrameCache, FrameData, SlotMark, SlotTarget, control_of, controls, doc_to_scene, effective,
    frame_id, keys, scene_to_doc, set_data, slots_of,
};
use uuid::Uuid;

fn add(scene: &mut Scene, parent: NodeId, mut n: Node) -> NodeId {
    n.parent = Some(parent);
    let id = n.id;
    scene.nodes.get_mut(&parent).unwrap().children.push(id);
    scene.nodes.insert(id, n);
    id
}

struct Surface {
    scene: Scene,
    knob: NodeId,
    needle: NodeId,
    value: NodeId,
    meter: NodeId,
    lamp: NodeId,
    missing: Uuid,
}

/// A knob (needle and value inside it, a lamp inside the meter next to it, one slot naming a node
/// that isn't there) and a meter (its fill is that lamp).
fn surface() -> Surface {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    let knob = add(&mut scene, page, Node::new(NodeKind::Frame, "Knob"));
    let needle = add(&mut scene, knob, Node::new(NodeKind::Rectangle, "Needle"));
    let value = add(
        &mut scene,
        knob,
        Node::new(
            NodeKind::Text(TextData {
                characters: "0.0 dB".into(),
                ..TextData::default()
            }),
            "Value",
        ),
    );
    let meter = add(&mut scene, page, Node::new(NodeKind::Frame, "Meter"));
    let lamp = add(&mut scene, meter, Node::new(NodeKind::Ellipse, "Lamp"));
    let missing = Uuid::from_u128(7);
    let mut c = Control::new(ControlKind::Knob {
        sweep_degrees: 270.0,
    });
    c.binding = Some(Binding::param("gain"));
    c.slots = vec![
        Slot {
            role: SlotRole::Needle,
            node: needle.0,
        },
        Slot {
            role: SlotRole::Led,
            node: lamp.0,
        },
        Slot {
            role: SlotRole::Label,
            node: missing,
        },
        Slot {
            role: SlotRole::Value,
            node: value.0,
        },
    ];
    scene.nodes.get_mut(&knob).unwrap().control = Some(c);
    let mut m = Control::new(ControlKind::Knob {
        sweep_degrees: 90.0,
    });
    m.slots = vec![Slot {
        role: SlotRole::Fill,
        node: lamp.0,
    }];
    scene.nodes.get_mut(&meter).unwrap().control = Some(m);
    Surface {
        scene,
        knob,
        needle,
        value,
        meter,
        lamp,
        missing,
    }
}

fn all(doc: &Document) -> Vec<FcId> {
    doc.pages.iter().flat_map(|p| doc.subtree(*p)).collect()
}

fn fc(doc: &Document, id: NodeId) -> FcId {
    all(doc)
        .into_iter()
        .find(|i| frame_id(doc.node(*i).unwrap()) == Some(id.0))
        .unwrap()
}

#[test]
fn slots_are_marks_on_the_slot_nodes() {
    let s = surface();
    let doc = scene_to_doc(&s.scene).unwrap();
    let (knob, lamp) = (fc(&doc, s.knob), fc(&doc, s.lamp));
    // The lamp is the meter's fill (its nearest control) and the knob's LED (named).
    let marks: Vec<SlotMark> =
        serde_json::from_str(&doc.node(lamp).unwrap().plugin_data[keys::SLOT]).unwrap();
    assert_eq!(
        marks,
        [
            SlotMark {
                role: SlotRole::Led,
                at: 1,
                of: Some(s.knob.0)
            },
            SlotMark {
                role: SlotRole::Fill,
                at: 0,
                of: None
            }
        ]
    );
    // The control itself carries neither its binding nor its slots.
    let own = FrameData::of(doc.node(knob).unwrap());
    assert!(
        own.control
            .as_ref()
            .is_some_and(|c| c.binding.is_none() && c.slots.is_empty())
    );
    assert_eq!(own.binding, Some(Binding::param("gain")));
    assert_eq!(
        slots_of(&doc, knob),
        [
            (SlotRole::Needle, SlotTarget::Node(fc(&doc, s.needle))),
            (SlotRole::Led, SlotTarget::Node(lamp)),
            (SlotRole::Label, SlotTarget::Extern(s.missing)),
            (SlotRole::Value, SlotTarget::Node(fc(&doc, s.value))),
        ]
    );
    assert_eq!(control_of(&doc, knob), s.scene.nodes[&s.knob].control);
    assert_eq!(
        control_of(&doc, fc(&doc, s.meter)),
        s.scene.nodes[&s.meter].control
    );
    let all = controls(&doc);
    assert_eq!(
        (all.len(), all.get(&knob)),
        (2, s.scene.nodes[&s.knob].control.as_ref())
    );
    assert_eq!(doc_to_scene(&doc).unwrap(), s.scene);
}

#[test]
fn a_copied_control_drives_its_own_copies() {
    let s = surface();
    let mut doc = scene_to_doc(&s.scene).unwrap();
    let knob = fc(&doc, s.knob);
    let page = doc.pages[0];
    let copy = doc.duplicate_into(knob, page, None).unwrap();
    let kids = doc.node(copy).unwrap().children.clone();
    // The copy's slots are its own needle and value; the lamp outside it stays the original's.
    assert_eq!(
        slots_of(&doc, copy),
        [
            (SlotRole::Needle, SlotTarget::Node(kids[0])),
            (SlotRole::Label, SlotTarget::Extern(s.missing)),
            (SlotRole::Value, SlotTarget::Node(kids[1]))
        ]
    );
    assert_eq!(slots_of(&doc, knob).len(), 4);

    // Back in v1 the original keeps its ids; the copy gets fresh ones and its slots follow.
    let back = doc_to_scene(&doc).unwrap();
    assert_eq!(back.nodes[&s.knob], s.scene.nodes[&s.knob]);
    let copy_id = back
        .pages()
        .iter()
        .flat_map(|p| back.nodes[p].children.clone())
        .find(|c| *c != s.knob && *c != s.meter)
        .unwrap();
    let copy = &back.nodes[&copy_id];
    assert_eq!(copy.name, "Knob");
    let slots: Vec<Uuid> = copy
        .control
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .map(|s| s.node)
        .collect();
    assert_eq!(slots, [copy.children[0].0, s.missing, copy.children[1].0]);
    assert!(
        copy.children
            .iter()
            .all(|c| back.nodes[c].parent == Some(copy_id) && *c != s.needle && *c != s.value)
    );
}

#[test]
fn a_figcraft_edit_wins_over_the_kept_v1_values() {
    let mut s = surface();
    // A value figcraft holds only approximately (f32 opacity), kept beside it.
    s.scene.nodes.get_mut(&s.needle).unwrap().opacity = 0.3;
    let mut doc = scene_to_doc(&s.scene).unwrap();
    assert!(
        doc.node(fc(&doc, s.needle))
            .unwrap()
            .plugin_data
            .contains_key(keys::V1)
    );
    assert_eq!(doc_to_scene(&doc).unwrap().nodes[&s.needle].opacity, 0.3);

    let needle = fc(&doc, s.needle);
    let n = doc.node_mut(needle).unwrap();
    n.opacity = 0.5;
    n.name = "Pointer".into();
    let back = doc_to_scene(&doc).unwrap();
    let b = &back.nodes[&s.needle];
    assert_eq!((b.opacity, b.name.as_str()), (0.5, "Pointer"));
    let rest = |n: &Node| Node {
        opacity: 0.0,
        name: String::new(),
        ..n.clone()
    };
    assert_eq!(rest(b), rest(&s.scene.nodes[&s.needle]));
    // The text's kept runs and fonts stay while its characters are unedited, and give way to an edit.
    let value = fc(&doc, s.value);
    if let FcKind::Text(t) = &mut doc.node_mut(value).unwrap().kind {
        t.characters = "-3.0 dB".into();
    }
    match &doc_to_scene(&doc).unwrap().nodes[&s.value].kind {
        NodeKind::Text(t) => assert_eq!(t.characters, "-3.0 dB"),
        k => panic!("{k:?}"),
    }
}

#[test]
fn the_cache_parses_once_per_revision() {
    let s = surface();
    let mut doc = scene_to_doc(&s.scene).unwrap();
    let knob = fc(&doc, s.knob);
    let mut cache = FrameCache::new();
    let a = cache.get(&doc, knob).unwrap();
    let b = cache.get(&doc, knob).unwrap();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    assert_eq!(cache.parses(), 1);
    doc.node_mut(knob).unwrap().plugin_data.insert(
        keys::BINDING.into(),
        serde_json::to_string(&Binding::param("ratio")).unwrap(),
    );
    assert_eq!(
        cache.get(&doc, knob).unwrap().binding,
        Some(Binding::param("ratio"))
    );
    assert_eq!(cache.parses(), 2);
    assert!(cache.get(&doc, FcId(9999)).is_none());
}

#[test]
fn bad_json_reads_as_absent_with_an_error() {
    let mut map = std::collections::BTreeMap::new();
    map.insert(keys::CONTROL.to_string(), "{not json".to_string());
    map.insert(keys::OF.to_string(), "\"Knob\"".to_string());
    let d = FrameData::parse(&map);
    assert!(d.control.is_none());
    assert_eq!(d.of.as_deref(), Some("Knob"));
    assert_eq!(d.errors.len(), 1);
    assert_eq!(d.errors[0].key, keys::CONTROL);
}

/// A main with a control layer and an instance of it whose layer mirrors that one, as the engine
/// builds them (`source` links each to the main's).
#[test]
fn instances_inherit_and_override() {
    let mut doc = Document::new("t");
    let page = doc.pages[0];
    let comp_kind = FcKind::Component {
        description: String::new(),
        props: Default::default(),
        prop_order: Vec::new(),
    };
    let main = doc
        .create(page, comp_kind, "Main", 0.0, 0.0, 100.0, 100.0)
        .unwrap();
    let layer = doc
        .create(main, FcKind::Frame, "Knob", 0.0, 0.0, 50.0, 50.0)
        .unwrap();
    let knob = Control::new(ControlKind::Knob {
        sweep_degrees: 270.0,
    });
    set_data(
        &mut doc,
        layer,
        keys::CONTROL,
        Some(serde_json::to_string(&knob).unwrap()),
    )
    .unwrap();
    set_data(
        &mut doc,
        layer,
        keys::BINDING,
        Some(serde_json::to_string(&Binding::param("gain")).unwrap()),
    )
    .unwrap();

    let inst = doc
        .create(
            page,
            FcKind::Instance {
                component: main,
                props: Default::default(),
            },
            "Inst",
            200.0,
            0.0,
            100.0,
            100.0,
        )
        .unwrap();
    doc.node_mut(inst).unwrap().source = Some(main);
    let mirror = doc
        .create(inst, FcKind::Frame, "Knob", 0.0, 0.0, 50.0, 50.0)
        .unwrap();
    doc.node_mut(mirror).unwrap().source = Some(layer);
    let slot = doc
        .create(layer, FcKind::Frame, "Needle", 0.0, 0.0, 10.0, 10.0)
        .unwrap();
    set_data(
        &mut doc,
        slot,
        keys::SLOT,
        Some(
            serde_json::to_string(&vec![SlotMark {
                role: SlotRole::Needle,
                at: 0,
                of: None,
            }])
            .unwrap(),
        ),
    )
    .unwrap();
    let mirror_slot = doc
        .create(mirror, FcKind::Frame, "Needle", 0.0, 0.0, 10.0, 10.0)
        .unwrap();
    doc.node_mut(mirror_slot).unwrap().source = Some(slot);
    assert_eq!(
        slots_of(&doc, mirror),
        vec![(SlotRole::Needle, SlotTarget::Node(mirror_slot))]
    );
    assert_eq!(
        effective(&doc, mirror).binding,
        Some(Binding::param("gain"))
    );
    assert_eq!(control_of(&doc, mirror).unwrap().kind, knob.kind);

    // An override on the layer lands on the instance root; the main keeps its own.
    set_data(
        &mut doc,
        mirror,
        keys::BINDING,
        Some(serde_json::to_string(&Binding::param("ratio")).unwrap()),
    )
    .unwrap();
    assert!(
        !doc.node(mirror)
            .unwrap()
            .plugin_data
            .contains_key(keys::BINDING)
    );
    assert!(
        doc.node(inst)
            .unwrap()
            .plugin_data
            .contains_key(keys::OVERRIDE)
    );
    assert_eq!(
        effective(&doc, mirror).binding,
        Some(Binding::param("ratio"))
    );
    assert_eq!(effective(&doc, layer).binding, Some(Binding::param("gain")));
    // Removing the override inherits again.
    set_data(&mut doc, mirror, keys::BINDING, None).unwrap();
    assert!(
        !doc.node(inst)
            .unwrap()
            .plugin_data
            .contains_key(keys::OVERRIDE)
    );
    assert_eq!(
        effective(&doc, mirror).binding,
        Some(Binding::param("gain"))
    );
}
