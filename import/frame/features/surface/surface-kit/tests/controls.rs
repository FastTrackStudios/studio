//! The switching and panel families: every component places, wires the
//! slots its kind needs, and lints clean against the demo catalog.

use std::collections::BTreeMap;

use document_proto::{NodeId, Scene};
use surface_kit::{ComponentDef, Family, Place};
use surface_proto::{Binding, ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::{Builder, Theme};

struct Built {
    scene: Scene,
    controls: BTreeMap<NodeId, Control>,
    root: NodeId,
}

fn build(id: &str, p: &Place) -> Built {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let root = {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        surface_kit::place(&mut b, page, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
    };
    for (n, c) in &controls {
        scene.node_mut(*n).expect("control node").control = Some(c.clone());
    }
    Built { scene, controls, root }
}

fn families() -> Vec<ComponentDef> {
    surface_kit::components()
        .into_iter()
        .filter(|c| matches!(c.family, Family::Button | Family::Switch | Family::Lever | Family::Panel))
        .collect()
}

/// The demo param each kind of component is bound to.
fn placement(def: &ComponentDef) -> Place {
    let p = Place::new(def.title);
    match def.binds {
        "toggle param" => p.bind(Binding::param("drive/on")),
        "choice param" => p.bind(Binding::param("amp/channel")).options(&["Clean", "Crunch", "Lead"]),
        "none" => p,
        _ => p.bind(Binding::param("delay/mix")),
    }
}

#[test]
fn every_family_is_populated() {
    let all = families();
    for f in [Family::Button, Family::Switch, Family::Lever, Family::Panel] {
        assert!(all.iter().any(|c| c.family == f), "no {} components", f.name());
    }
    for c in &all {
        assert!(c.id.starts_with(&format!("{}.", c.family.name())), "{} is not in its family's namespace", c.id);
    }
}

#[test]
fn every_id_places_with_defaults_and_every_variant() {
    for def in families() {
        build(def.id, &Place::new("x"));
        for v in def.variants {
            build(def.id, &Place::new("x").variant(*v).size(def.default_size * 1.5));
        }
    }
}

#[test]
fn bound_controls_lint_clean() {
    let catalog = surface_memory::demo_catalog();
    for def in families().into_iter().filter(|d| d.binds != "none") {
        let built = build(def.id, &placement(&def));
        assert!(!built.controls.is_empty(), "{} registered no control", def.id);
        let issues = design_lint::lint(&built.scene, &catalog, &design_lint::Config::default());
        let errors: Vec<_> =
            issues.iter().filter(|i| i.severity == design_lint::Severity::Error).map(ToString::to_string).collect();
        assert!(errors.is_empty(), "{}:\n{}", def.id, errors.join("\n"));
        for c in built.controls.values() {
            assert_eq!(c.binding, placement(&def).binding, "{}: binding not passed through", def.id);
            let has = |r: SlotRole| c.slots.iter().any(|s| s.role == r);
            match c.kind {
                // A switch that doesn't visibly move reads as broken.
                ControlKind::Button { .. } => assert!(has(SlotRole::Led) || has(SlotRole::Thumb), "{}: nothing flips", def.id),
                ControlKind::Knob { .. } => assert!(has(SlotRole::Needle), "{}: no needle", def.id),
                ControlKind::Fader { .. } => assert!(has(SlotRole::Thumb), "{}: no thumb", def.id),
                ref k => panic!("{}: unexpected kind {}", def.id, k.name()),
            }
        }
    }
}

#[test]
fn toggles_are_toggles_and_choices_select() {
    for def in families() {
        let built = build(def.id, &placement(&def));
        match def.binds {
            "toggle param" => {
                assert_eq!(built.controls.len(), 1, "{}", def.id);
                let c = &built.controls[&built.root];
                assert!(matches!(c.kind, ControlKind::Button { mode: ButtonMode::Toggle }), "{}", def.id);
            }
            "choice param" if def.id.ends_with("_bank") => {
                let mut idx: Vec<(u32, u32)> = built
                    .controls
                    .values()
                    .map(|c| match c.kind {
                        ControlKind::Button { mode: ButtonMode::Select { index, count } } => (index, count),
                        ref k => panic!("{}: bank member is a {}", def.id, k.name()),
                    })
                    .collect();
                idx.sort_unstable();
                assert_eq!(idx, vec![(0, 3), (1, 3), (2, 3)], "{}", def.id);
                assert!(!built.controls.contains_key(&built.root), "{}: the bank frame itself is not a control", def.id);
            }
            "choice param" => {
                let c = &built.controls[&built.root];
                assert!(
                    matches!(c.kind, ControlKind::Knob { sweep_degrees } if sweep_degrees > 0.0 && sweep_degrees <= 300.0),
                    "{}",
                    def.id
                );
            }
            _ => {}
        }
    }
}

#[test]
fn lit_buttons_give_off_light_from_their_led_slot() {
    for def in families().into_iter().filter(|d| d.family == Family::Button && !d.id.starts_with("button.push_in")) {
        let built = build(def.id, &placement(&def));
        for c in built.controls.values() {
            let lights: Vec<_> = c
                .slots_of(SlotRole::Led)
                .filter_map(|id| built.scene.node(NodeId(id))?.emission.clone())
                .collect();
            assert_eq!(lights.len(), 1, "{}: one Led node carries the light", def.id);
            assert!(lights[0].intensity > 0.0 && lights[0].radius > 0.0, "{}", def.id);
        }
        // Nothing else in it glows: the light dims with the lamp.
        let glowing = built.scene.walk(built.root).into_iter().filter(|id| built.scene.node(*id).is_some_and(|n| n.emission.is_some())).count();
        assert_eq!(glowing, built.controls.len(), "{}", def.id);
    }
}

#[test]
fn banks_follow_their_options() {
    for def in families().into_iter().filter(|d| d.id.ends_with("_bank")) {
        let p = Place::new("Ratio").bind(Binding::param("amp/channel")).options(&["4", "8", "12", "20", "All"]);
        assert_eq!(build(def.id, &p).controls.len(), 5, "{}", def.id);
        // No options: numbered positions rather than nothing.
        assert_eq!(build(def.id, &Place::new("x")).controls.len(), 3, "{}", def.id);
    }
}

#[test]
fn panels_have_no_control_and_take_children() {
    for def in families().into_iter().filter(|d| d.family == Family::Panel) {
        assert_eq!(def.binds, "none");
        let mut scene = Scene::new("Kit");
        let page = scene.pages()[0];
        let mut controls = BTreeMap::new();
        let theme = Theme::stage_dark();
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        let panel = surface_kit::place(&mut b, page, def.id, &Place::new("Face").options(&["w=900", "h=300"])).unwrap();
        assert!(b.controls.is_empty(), "{} registered a control", def.id);
        let size = b.scene.node(panel).unwrap().size;
        assert_eq!((size.width, size.height), (900.0, 300.0), "{}", def.id);
        let child = surface_kit::place(&mut b, panel, "button.console", &Place::new("In").bind(Binding::param("drive/on"))).unwrap();
        assert_eq!(b.scene.node(child).unwrap().parent, Some(panel), "{}", def.id);
        let sized = surface_kit::place(&mut b, page, def.id, &Place::new("Face").size(600.0)).unwrap();
        assert_eq!(b.scene.node(sized).unwrap().size.width, 600.0, "{}", def.id);
    }
}
