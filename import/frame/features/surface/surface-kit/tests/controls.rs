//! The switching and panel families: every component places, wires the
//! slots its kind needs, and lints clean against the demo catalog.

use std::collections::BTreeMap;

use document_proto::{NodeId, Scene, TextCase};
use surface_kit::{ComponentDef, Family, Ink, Place};
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
                ControlKind::Button { .. } | ControlKind::Footswitch { .. } => {
                    assert!(has(SlotRole::Led) || has(SlotRole::Thumb), "{}: nothing flips", def.id);
                }
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
                let latching = matches!(c.kind, ControlKind::Button { mode: ButtonMode::Toggle } | ControlKind::Footswitch { .. });
                assert!(latching, "{}", def.id);
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

#[test]
fn rotary_switches_and_levers_turn_as_bodies_about_their_pivot() {
    for id in ["switch.rotary", "lever.pultec"] {
        let def = surface_kit::find(id).unwrap();
        let built = build(id, &placement(&def));
        let c = &built.controls[&built.root];
        let needle = built.scene.node(NodeId(c.slots_of(SlotRole::Needle).next().unwrap())).unwrap();
        assert!(matches!(needle.kind, document_proto::NodeKind::Frame) && needle.children.len() >= 3, "{id}: one turning body");
        let pivot = built.scene.node(needle.parent.unwrap()).unwrap();
        assert!((pivot.size.width - pivot.size.height).abs() < 1e-9, "{id}: it turns about a square's centre");
        assert_eq!(needle.size, pivot.size, "{id}");
        // The square is centred on the boss / the pointer's hub.
        let centre = (pivot.transform.tx + pivot.size.width / 2.0, pivot.transform.ty + pivot.size.height / 2.0);
        let hub = built
            .scene
            .walk(built.root)
            .into_iter()
            .filter_map(|n| built.scene.node(n))
            .find(|n| n.name == "boss" && n.parent == Some(built.root))
            .map(|n| (n.transform.tx + n.size.width / 2.0, n.transform.ty + n.size.height / 2.0));
        if let Some(hub) = hub {
            assert!((hub.0 - centre.0).abs() < 1e-6 && (hub.1 - centre.1).abs() < 1e-6, "{id}: pivot {centre:?}, boss {hub:?}");
        }
    }
}

#[test]
fn stepped_rotaries_take_a_sweep() {
    let sweep = |id: &str, p: &Place| match build(id, p).controls.values().next().unwrap().kind {
        ControlKind::Knob { sweep_degrees } => sweep_degrees,
        ref k => panic!("{}", k.name()),
    };
    let p = Place::new("Mode").options(&["A", "B", "C"]);
    assert_eq!(sweep("switch.rotary", &p), 120.0, "≤ 60° a detent");
    assert_eq!(sweep("switch.rotary", &p.clone().sweep(90.0)), 90.0);
    assert_eq!(sweep("lever.pultec", &p), 124.0);
    assert_eq!(sweep("lever.pultec", &p.clone().sweep(100.0)), 100.0);
}

/// The fills of every text and printed mark under a component's root.
fn printed(built: &Built) -> Vec<String> {
    built
        .scene
        .walk(built.root)
        .into_iter()
        .filter_map(|n| built.scene.node(n))
        .filter(|n| n.text().is_some() || n.name == "detent" || n.name == "tick" || n.name.starts_with("scale"))
        .filter_map(|n| match n.fills.first().map(|f| &f.kind) {
            Some(document_proto::PaintKind::Solid { color }) => Some(format!("{:.2},{:.2},{:.2}", color.r, color.g, color.b)),
            _ => None,
        })
        .collect()
}

#[test]
fn panel_print_takes_the_place_s_ink() {
    let dark = {
        let c = surface_kit::draw::hex(surface_kit::DARK_INK);
        format!("{:.2},{:.2},{:.2}", c.r, c.g, c.b)
    };
    for id in ["switch.toggle_bat", "switch.rotary", "lever.pultec", "lever.fader", "lever.slide", "button.push_in_bank", "button.led_bank", "switch.footswitch"] {
        let def = surface_kit::find(id).unwrap();
        let p = placement(&def).legend("Mode");
        let light = printed(&build(id, &p));
        assert!(!light.is_empty() && light.iter().all(|c| *c != dark), "{id}: {light:?}");
        let inked = printed(&build(id, &p.ink(Ink::Dark)));
        assert!(inked.iter().all(|c| *c == dark), "{id}: {inked:?}");
    }
}

#[test]
fn legends_keep_their_case_when_asked() {
    for id in ["switch.toggle_bat", "switch.rotary", "button.console", "button.push_in_bank", "switch.footswitch"] {
        let def = surface_kit::find(id).unwrap();
        let cases = |p: &Place| -> Vec<TextCase> {
            let b = build(id, p);
            b.scene.walk(b.root).into_iter().filter_map(|n| b.scene.node(n)?.text().map(|t| t.text_case)).collect()
        };
        let p = placement(&def).legend("kHz");
        assert!(cases(&p).contains(&TextCase::Upper), "{id}");
        assert!(cases(&p.keep_case()).iter().all(|c| *c == TextCase::Original), "{id}");
    }
}

/// Each bank member's select index, by its position along the bank.
fn order(built: &Built, vertical: bool) -> Vec<u32> {
    let mut v: Vec<(f64, u32)> = built
        .controls
        .iter()
        .map(|(id, c)| {
            let t = built.scene.node(*id).unwrap().transform;
            let at = if vertical { t.ty } else { t.tx };
            match c.kind {
                ControlKind::Button { mode: ButtonMode::Select { index, .. } } => (at, index),
                ref k => panic!("{}", k.name()),
            }
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.into_iter().map(|(_, i)| i).collect()
}

#[test]
fn banks_can_run_backwards() {
    for id in ["button.console_bank", "button.square_bank", "button.led_bank", "button.push_in_bank"] {
        let vertical = id == "button.push_in_bank";
        let p = Place::new("Ratio").bind(Binding::param("amp/channel")).options(&["Clean", "Crunch", "Lead"]);
        assert_eq!(order(&build(id, &p), vertical), [0, 1, 2], "{id}");
        assert_eq!(order(&build(id, &p.reversed()), vertical), [2, 1, 0], "{id}");
    }
}

#[test]
fn a_long_cap_legend_is_set_small_enough_to_fit() {
    let p = Place::new("Algo").bind(Binding::param("amp/channel")).size(28.0).options(&["Hall", "Shimmer", "Spectral"]);
    let built = build("button.square_bank", &p);
    for id in built.scene.walk(built.root) {
        let n = built.scene.node(id).unwrap();
        if let Some(t) = n.text() {
            let width = t.characters.chars().count() as f64 * t.font_size * 0.68;
            assert!(width <= 28.0, "{}: {width} px on a 28 px cap at {}", t.characters, t.font_size);
        }
    }
    let size_of = |s: &str| {
        built.scene.walk(built.root).into_iter().filter_map(|n| built.scene.node(n)?.text().filter(|t| t.characters == s).map(|t| t.font_size)).next().unwrap()
    };
    assert!(size_of("Hall") > size_of("Spectral"), "a short word keeps its size");
}

#[test]
fn a_footswitch_latches_or_is_momentary_and_lights_its_led() {
    let p = Place::new("Bypass").bind(Binding::param("drive/on"));
    let latching = build("switch.footswitch", &p);
    let c = &latching.controls[&latching.root];
    assert!(matches!(c.kind, ControlKind::Footswitch { .. }));
    let led = NodeId(c.slots_of(SlotRole::Led).next().expect("its LED"));
    assert!(latching.scene.node(led).unwrap().emission.is_some());
    let momentary = build("switch.footswitch", &p.clone().options(&["momentary"]));
    assert!(matches!(momentary.controls[&momentary.root].kind, ControlKind::Button { mode: ButtonMode::Momentary }));
    let dark = build("switch.footswitch", &p.clone().variant("none"));
    assert_eq!(dark.controls[&dark.root].slots_of(SlotRole::Led).count(), 0, "no LED");
    // The LED follows the param through the runtime.
    let mut scene = latching.scene.clone();
    for (n, c) in &latching.controls {
        scene.node_mut(*n).unwrap().control = Some(c.clone());
    }
    let mut rt = surface_runtime::Runtime::new(surface_runtime::Surface::from_scene(scene), surface_memory::demo_catalog());
    for (on, lit) in [(0.0, false), (1.0, true)] {
        rt.apply_values([surface_proto::ParamValue { addr: surface_proto::ParamAddr::new("drive/on"), value: on }]);
        let n = rt.frame(0.0).node(led).unwrap().clone();
        assert_eq!(n.emission.is_some(), lit, "LED at {on}");
    }
}
