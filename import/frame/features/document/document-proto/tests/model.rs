use document_proto::*;

fn rect(name: &str) -> Node {
    let mut n = Node::new(NodeKind::Rectangle, name);
    n.size = Size { width: 10.0, height: 20.0 };
    n.fills = vec![Paint::solid(Color::rgba(1.0, 0.0, 0.0, 1.0))];
    n
}

fn sample() -> (Scene, NodeId, NodeId) {
    let mut scene = Scene::new("Page 1");
    let page = scene.pages()[0];
    let mut frame = Node::new(NodeKind::Frame, "Panel");
    frame.layout = Some(AutoLayout {
        direction: LayoutDirection::Horizontal,
        gap: 8.0,
        padding_left: 4.0,
        padding_right: 4.0,
        padding_top: 4.0,
        padding_bottom: 4.0,
        wrap: false,
        main_align: Some(MainAxisAlign::Center),
        cross_align: None,
    });
    let frame_id = frame.id;
    scene.apply(Op::Insert { node: frame, parent: page, index: u32::MAX }).unwrap();
    let r = rect("Knob");
    let r_id = r.id;
    scene.apply(Op::Insert { node: r, parent: frame_id, index: u32::MAX }).unwrap();
    let mut t = Node::new(NodeKind::Text(TextData { characters: "Gain".into(), ..TextData::default() }), "Label");
    t.fills = vec![Paint::solid(Color::rgba(1.0, 1.0, 1.0, 1.0))];
    scene.apply(Op::Insert { node: t, parent: frame_id, index: u32::MAX }).unwrap();
    scene.add_asset("image/png", vec![1, 2, 3]);
    (scene, frame_id, r_id)
}

#[test]
fn remove_is_undone_by_its_inverse() {
    let (mut scene, frame, _) = sample();
    let before = scene.clone();
    let inverse = scene.apply(Op::Remove { id: frame }).unwrap();
    assert!(scene.node(frame).is_none());
    assert_eq!(scene.nodes.len(), before.nodes.len() - 3);
    scene.transact(inverse).unwrap();
    assert_eq!(scene, before);
}

#[test]
fn move_and_set_round_trip() {
    let (mut scene, frame, knob) = sample();
    let before = scene.clone();
    let page = scene.pages()[0];
    let inv = scene
        .transact([
            Op::Move { id: knob, parent: page, index: 0 },
            Op::Set { id: knob, prop: Prop::Opacity(0.5) },
            Op::Set { id: frame, prop: Prop::Name("Renamed".into()) },
        ])
        .unwrap();
    assert_eq!(scene.node(knob).unwrap().parent, Some(page));
    assert_eq!(scene.node(page).unwrap().children[0], knob);
    scene.transact(inv).unwrap();
    assert_eq!(scene, before);
}

#[test]
fn failed_transaction_rolls_back() {
    let (mut scene, frame, knob) = sample();
    let before = scene.clone();
    let err = scene
        .transact([
            Op::Set { id: knob, prop: Prop::Visible(false) },
            // A frame can't move under its own child.
            Op::Move { id: frame, parent: knob, index: 0 },
        ])
        .unwrap_err();
    assert_eq!(err, OpError::Cycle(frame));
    assert_eq!(scene, before);
}

#[test]
fn history_undo_redo() {
    let (mut scene, _, knob) = sample();
    let original = scene.clone();
    let mut history = History::default();
    history.commit(&mut scene, [Op::Set { id: knob, prop: Prop::Size(Size { width: 64.0, height: 64.0 }) }]).unwrap();
    let edited = scene.clone();
    assert!(history.undo(&mut scene).unwrap());
    assert_eq!(scene, original);
    assert!(history.redo(&mut scene).unwrap());
    assert_eq!(scene, edited);
    assert!(history.redo(&mut scene).is_ok_and(|did| !did));
}

#[test]
fn root_is_protected() {
    let (mut scene, _, _) = sample();
    assert_eq!(scene.apply(Op::Remove { id: scene.root }).unwrap_err(), OpError::Root);
}

#[test]
fn assets_are_content_addressed() {
    let mut scene = Scene::new("p");
    let a = scene.add_asset("image/png", vec![9, 9]);
    let b = scene.add_asset("image/png", vec![9, 9]);
    assert_eq!(a, b);
    assert_eq!(scene.assets.len(), 1);
    assert_eq!(a.0.len(), 64);
}

#[test]
fn scene_round_trips_through_facet_json() {
    let (scene, _, _) = sample();
    let json = facet_json::to_string(&scene).unwrap();
    let back: Scene = facet_json::from_str(&json).unwrap();
    assert_eq!(back, scene);
}

#[test]
fn scene_round_trips_through_serde_json() {
    let (scene, _, _) = sample();
    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();
    assert_eq!(back, scene);
}
