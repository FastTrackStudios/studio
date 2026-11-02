//! The engine keeps `frame:*` data through its own edits (frame#8): copy/paste and duplicate carry
//! it to the copies, components and their instances share it, and an instance's override outlives
//! an edit of its main.

use frame::document::proto::{Color, Emission, Node, NodeId, NodeKind, Scene, Size};
use frame::surface::proto::{Binding, Control, ControlKind, Slot, SlotRole};
use frame_editor::doc::doc::{Document, NodeId as FcId};
use frame_editor::doc::{
    SlotTarget, control_of, effective, frame_id, keys, scene_to_doc, set_data, slots_of,
};
use frame_editor::engine::Session;
use serde_json::{Value, json};

fn id(v: &Value) -> FcId {
    FcId(v.as_u64().or_else(|| v.as_str()?.parse().ok()).unwrap())
}

fn ex(s: &mut Session, cmd: &str, params: Value) -> Value {
    s.execute(cmd, &params)
        .unwrap_or_else(|e| panic!("{cmd}: {e:?}"))
}

fn doc(s: &Session) -> &Document {
    s.doc().unwrap()
}

fn kids(s: &Session, id: FcId) -> Vec<FcId> {
    doc(s).node(id).unwrap().children.clone()
}

fn add(scene: &mut Scene, parent: NodeId, mut n: Node) -> NodeId {
    n.parent = Some(parent);
    let id = n.id;
    scene.nodes.get_mut(&parent).unwrap().children.push(id);
    scene.nodes.insert(id, n);
    id
}

/// A session on a knob (a frame with a needle in it, bound to `gain`); returns it and the knob.
fn knob_session() -> (Session, FcId, Control) {
    let mut scene = Scene::new("Page");
    let page = scene.pages()[0];
    let mut frame = Node::new(NodeKind::Frame, "Knob");
    frame.size = Size {
        width: 60.0,
        height: 60.0,
    };
    let knob = add(&mut scene, page, frame);
    let needle = add(&mut scene, knob, Node::new(NodeKind::Rectangle, "Needle"));
    let mut c = Control::new(ControlKind::Knob {
        sweep_degrees: 270.0,
    });
    c.binding = Some(Binding::param("gain"));
    c.slots = vec![Slot {
        role: SlotRole::Needle,
        node: needle.0,
    }];
    scene.nodes.get_mut(&knob).unwrap().control = Some(c.clone());
    let d = scene_to_doc(&scene).unwrap();
    let fc = d
        .pages
        .iter()
        .flat_map(|p| d.subtree(*p))
        .find(|i| frame_id(d.node(*i).unwrap()) == Some(knob.0))
        .unwrap();
    (Session::with_document(d), fc, c)
}

/// The copy is a control of its own whose needle slot is the copy's needle.
fn assert_own_control(s: &Session, copy: FcId, original: FcId, c: &Control) {
    assert_ne!(copy, original);
    let needle = kids(s, copy)[0];
    assert_eq!(
        slots_of(doc(s), copy),
        [(SlotRole::Needle, SlotTarget::Node(needle))]
    );
    let got = control_of(doc(s), copy).unwrap();
    assert_eq!((&got.kind, &got.binding), (&c.kind, &c.binding));
    // The original still drives its own needle.
    assert_eq!(
        slots_of(doc(s), original),
        [(SlotRole::Needle, SlotTarget::Node(kids(s, original)[0]))]
    );
}

#[test]
fn copy_paste_keeps_frame_data() {
    let (mut s, knob, c) = knob_session();
    ex(&mut s, "edit.copy", json!({"ids": [knob]}));
    let pasted = ex(&mut s, "edit.paste", json!({"x": 300, "y": 0}));
    let copy = id(&pasted["ids"][0]);
    let (a, b) = (
        &doc(&s).node(knob).unwrap().plugin_data,
        &doc(&s).node(copy).unwrap().plugin_data,
    );
    let frame_keys = |m: &std::collections::BTreeMap<String, String>| {
        m.keys()
            .filter(|k| keys::is_frame(k))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert!(frame_keys(b).contains(&keys::CONTROL.to_string()));
    assert_eq!(frame_keys(a), frame_keys(b));
    assert_own_control(&s, copy, knob, &c);
}

#[test]
fn duplicate_keeps_frame_data() {
    let (mut s, knob, c) = knob_session();
    let dup = ex(&mut s, "edit.duplicate", json!({"ids": [knob]}));
    assert_own_control(&s, id(&dup["ids"][0]), knob, &c);
}

#[test]
fn instances_inherit_and_keep_overrides_through_main_edits() {
    let (mut s, knob, c) = knob_session();
    let main = id(&ex(&mut s, "component.create", json!({"ids": [knob]}))["id"]);
    let inst = id(&ex(
        &mut s,
        "instance.create",
        json!({"component": main, "x": 300, "y": 0}),
    )["id"]);
    let main_needle = kids(&s, main)[0];
    let needle = kids(&s, inst)[0];
    // The instance is the same control, its needle playing the same part.
    assert_eq!(effective(doc(&s), inst).binding, c.binding);
    assert_eq!(
        effective(doc(&s), needle).slots,
        effective(doc(&s), main_needle).slots
    );
    assert_eq!(control_of(doc(&s), inst).unwrap().kind, c.kind);

    // Override the instance's binding, then edit the main (its binding, a layer's geometry).
    let ratio = serde_json::to_string(&Binding::param("ratio")).unwrap();
    set_data(s.doc_mut().unwrap(), inst, keys::BINDING, Some(ratio)).unwrap();
    let mix = serde_json::to_string(&Binding::param("mix")).unwrap();
    ex(
        &mut s,
        "node.set",
        json!({"id": main, "props": {"pluginData": {keys::BINDING: mix}}}),
    );
    ex(
        &mut s,
        "node.set",
        json!({"id": main_needle, "props": {"width": 4}}),
    );
    assert_eq!(
        effective(doc(&s), main).binding,
        Some(Binding::param("mix"))
    );
    assert_eq!(
        effective(doc(&s), inst).binding,
        Some(Binding::param("ratio")),
        "the override survives main edits"
    );

    // A second instance, untouched, follows the main.
    let other = id(&ex(
        &mut s,
        "instance.create",
        json!({"component": main, "x": 600, "y": 0}),
    )["id"]);
    assert_eq!(
        effective(doc(&s), other).binding,
        Some(Binding::param("mix"))
    );
    // Dropping the override inherits again.
    set_data(s.doc_mut().unwrap(), inst, keys::BINDING, None).unwrap();
    assert_eq!(
        effective(doc(&s), inst).binding,
        Some(Binding::param("mix"))
    );
}

#[test]
fn an_override_on_an_instance_layer_survives_a_sync() {
    let (mut s, knob, _) = knob_session();
    let main = id(&ex(&mut s, "component.create", json!({"ids": [knob]}))["id"]);
    let inst = id(&ex(
        &mut s,
        "instance.create",
        json!({"component": main, "x": 300, "y": 0}),
    )["id"]);
    let main_needle = kids(&s, main)[0];
    let needle = kids(&s, inst)[0];
    let glow = Emission {
        color: Color::rgba(1.0, 0.5, 0.0, 1.0),
        intensity: 2.0,
        radius: 4.0,
    };
    set_data(
        s.doc_mut().unwrap(),
        needle,
        keys::EMISSION,
        Some(serde_json::to_string(&glow).unwrap()),
    )
    .unwrap();
    assert_eq!(effective(doc(&s), needle).emission.as_ref(), Some(&glow));
    // The main's layer changes; the engine rebuilds the instance's layers from it.
    ex(
        &mut s,
        "node.set",
        json!({"id": main_needle, "props": {"width": 6, "height": 30}}),
    );
    let needle = kids(&s, inst)[0];
    assert_eq!(
        effective(doc(&s), needle).emission,
        Some(glow),
        "the layer override outlives the rebuild"
    );
    assert!(effective(doc(&s), main_needle).emission.is_none());
}
