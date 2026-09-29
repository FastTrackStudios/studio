use design_lint::{Config, Severity, contrast_ratio, counts, lint};
use document_proto::{Color, NodeId, Op, Prop, Size};
use surface_memory::demo_catalog;
use surface_proto::{Binding, MidiMapping, MidiMessage, Curve, ParamAddr, SlotRole};
use surface_widgets::{Options, surface_for_catalog};

fn demo() -> document_proto::Scene {
    surface_for_catalog(&demo_catalog(), &Options::default()).to_scene()
}

fn control_named<'a>(scene: &'a document_proto::Scene, name: &str) -> &'a document_proto::Node {
    scene.nodes.values().find(|n| n.name == name && n.control.is_some()).unwrap()
}

#[test]
fn generated_surface_has_no_errors() {
    let issues = lint(&demo(), &demo_catalog(), &Config::default());
    let (errors, warnings, _) = counts(&issues);
    assert_eq!(errors, 0, "{}", issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"));
    // Small pan knob is below touch size — the only expected warning class.
    assert!(issues.iter().filter(|i| i.severity == Severity::Warning).all(|i| i.rule == "touch-target" || i.rule == "size-inconsistent"), "{warnings}");
}

#[test]
fn unknown_param_gets_a_suggestion() {
    let mut s = demo();
    let gain = control_named(&s, "Gain").clone();
    let mut c = gain.control.clone().unwrap();
    c.binding = Some(Binding::param("amp/gian"));
    s.apply(Op::Set { id: gain.id, prop: Prop::Control(Some(c)) }).unwrap();
    let issues = lint(&s, &demo_catalog(), &Config::default());
    let i = issues.iter().find(|i| i.rule == "binding").unwrap();
    assert!(i.fix.contains("did you mean `amp/gain`"), "{i}");
}

#[test]
fn missing_slots_and_dangling_slots() {
    let mut s = demo();
    let gain = control_named(&s, "Gain").clone();
    let mut c = gain.control.clone().unwrap();
    c.slots.retain(|sl| !matches!(sl.role, SlotRole::Needle | SlotRole::ValueArc));
    c.slots.push(surface_proto::Slot { role: SlotRole::Value, node: NodeId::new().0 });
    s.apply(Op::Set { id: gain.id, prop: Prop::Control(Some(c)) }).unwrap();
    let rules: Vec<_> = lint(&s, &demo_catalog(), &Config::default()).into_iter().map(|i| i.rule).collect();
    assert!(rules.contains(&"slot-missing".to_string()));
    assert!(rules.contains(&"slot-dangling".to_string()));
}

#[test]
fn duplicate_midi_is_an_error() {
    let mut s = demo();
    for name in ["Gain", "Bass"] {
        let n = control_named(&s, name).clone();
        let mut c = n.control.clone().unwrap();
        c.midi = Some(MidiMapping { channel: Some(1), message: MidiMessage::Cc { number: 21 }, curve: Curve::Linear });
        s.apply(Op::Set { id: n.id, prop: Prop::Control(Some(c)) }).unwrap();
    }
    assert!(lint(&s, &demo_catalog(), &Config::default()).iter().any(|i| i.rule == "midi-duplicate" && i.severity == Severity::Error));
}

#[test]
fn overlap_and_inconsistent_size() {
    let mut s = demo();
    let bass = control_named(&s, "Bass").clone();
    // Pull Bass out of flow onto Gain, and make it bigger.
    let mut lc = bass.layout_child.clone();
    lc.absolute = true;
    s.transact([
        Op::Set { id: bass.id, prop: Prop::LayoutChild(lc) },
        Op::Set { id: bass.id, prop: Prop::Transform(document_proto::Transform::translate(0.0, 0.0)) },
        Op::Set { id: bass.id, prop: Prop::Size(Size { width: 80.0, height: 120.0 }) },
    ])
    .unwrap();
    let rules: Vec<_> = lint(&s, &demo_catalog(), &Config::default()).into_iter().map(|i| i.rule).collect();
    assert!(rules.contains(&"overlap".to_string()), "{rules:?}");
    assert!(rules.contains(&"size-inconsistent".to_string()));
}

#[test]
fn low_contrast_text() {
    let mut s = demo();
    let label = s.nodes.values().find(|n| n.name == "caption").unwrap().id;
    s.apply(Op::Set { id: label, prop: Prop::Fills(vec![document_proto::Paint::solid(Color::rgba(0.1, 0.1, 0.12, 1.0))]) }).unwrap();
    assert!(lint(&s, &demo_catalog(), &Config::default()).iter().any(|i| i.rule == "contrast" && i.node == label));
    assert!((contrast_ratio(&Color::rgba(0.0, 0.0, 0.0, 1.0), &Color::rgba(1.0, 1.0, 1.0, 1.0)) - 21.0).abs() < 1e-9);
}

#[test]
fn unbound_controls_are_flagged() {
    let mut s = demo();
    let gain = control_named(&s, "Gain").clone();
    let mut c = gain.control.clone().unwrap();
    c.binding = None;
    s.apply(Op::Set { id: gain.id, prop: Prop::Control(Some(c)) }).unwrap();
    let issues = lint(&s, &demo_catalog(), &Config::default());
    assert!(issues.iter().any(|i| i.rule == "binding" && i.message.contains("not bound")));
    let _ = ParamAddr::new("x");
}

/// A 100×100 knob with a 50×50 knob inside it: nested in its node (a
/// concentric pair) or a sibling laid over it.
fn pair(nested: bool, inner_at: f64) -> document_proto::Scene {
    use document_proto::{Node, NodeKind, Transform};
    use surface_proto::{Control, ControlKind};
    let add = |s: &mut document_proto::Scene, parent, name: &str, at: f64, w: f64, bind: &str| {
        let mut n = Node::new(NodeKind::Frame, name);
        n.size = Size { width: w, height: w };
        n.transform = Transform::translate(at, at);
        let pointer = Node::new(NodeKind::Rectangle, "pointer");
        n.control = Some(Control::new(ControlKind::knob()).bound(Binding::param(bind)).slot(SlotRole::Needle, pointer.id.0));
        let id = n.id;
        s.apply(Op::Insert { node: n, parent, index: u32::MAX }).unwrap();
        s.apply(Op::Insert { node: pointer, parent: id, index: 0 }).unwrap();
        id
    };
    let mut s = document_proto::Scene::new("s");
    let page = s.pages()[0];
    let outer = add(&mut s, page, "Collar", 0.0, 100.0, "amp/gain");
    add(&mut s, if nested { outer } else { page }, "Cap", inner_at, 50.0, "amp/bass");
    s
}

#[test]
fn a_concentric_pair_is_not_an_overlap() {
    let overlaps = |s: &document_proto::Scene| lint(s, &demo_catalog(), &Config::default()).iter().filter(|i| i.rule == "overlap").count();
    assert_eq!(overlaps(&pair(true, 25.0)), 0, "a cap nested in its collar");
    assert_eq!(overlaps(&pair(false, 25.0)), 1, "a sibling laid over another is still ambiguous");
    assert_eq!(overlaps(&pair(true, 70.0)), 1, "nested but hanging out of the collar's box");
}
