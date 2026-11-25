//! The dockspace and screenset model: what a layout does when panels move,
//! close, attach and come back from disk.

use dock::dockspace::{
    AreaRef, Axis, Dockspace, Node, Panel, Place, Rect, Repair, Share, Shown, Side, Zone,
};
use dock::screenset::{FormClass, Screenset, Screensets};

fn arrange_with_mixer() -> (
    Dockspace,
    dock::dockspace::InstanceId,
    dock::dockspace::InstanceId,
) {
    let mut d = Dockspace::empty();
    let arr = d.open("arrangement", Place::Center).unwrap();
    let mix = d.open("mixer", Place::Edge(Side::Bottom)).unwrap();
    (d, arr, mix)
}

#[test]
fn two_instances_of_one_kind_keep_their_own_state() {
    let mut d = Dockspace::empty();
    let a = d.open("arrangement", Place::Center).unwrap();
    let (_, stack) = d.locate(a).unwrap();
    let b = d
        .add(
            Panel::new("arrangement").titled("Zoomed"),
            Place::Stack(stack, Zone::Split(Side::Bottom, Share::Ratio(0.5))),
        )
        .unwrap();
    d.panel_mut(a).unwrap().state = "zoom=1".into();
    d.panel_mut(b).unwrap().state = "zoom=8".into();

    assert_eq!(d.of_kind("arrangement").count(), 2);
    assert_ne!(d.locate(a).unwrap().1, d.locate(b).unwrap().1);
    assert_eq!(d.panel(a).unwrap().state, "zoom=1");
    assert_eq!(d.panel(b).unwrap().state, "zoom=8");
}

#[test]
fn moving_the_last_panel_out_of_a_split_collapses_it() {
    let (mut d, arr, mix) = arrange_with_mixer();
    let (_, stack) = d.locate(arr).unwrap();
    let fx = d
        .open(
            "fx-row",
            Place::Stack(stack, Zone::Split(Side::Right, Share::Ratio(0.7))),
        )
        .unwrap();
    assert!(matches!(
        d.center,
        Some(Node::Split {
            axis: Axis::Row,
            ..
        })
    ));

    d.move_panel(fx, Place::Edge(Side::Left)).unwrap();

    assert!(
        matches!(d.center, Some(Node::Stack(_))),
        "split gone, stack left"
    );
    assert_eq!(d.locate(fx).unwrap().0, AreaRef::Edge(Side::Left));
    assert_eq!(d.locate(mix).unwrap().0, AreaRef::Edge(Side::Bottom));
}

#[test]
fn a_split_off_panel_lands_on_the_side_it_was_dropped() {
    let (mut d, arr, _) = arrange_with_mixer();
    let (_, stack) = d.locate(arr).unwrap();
    let tcp = d
        .open(
            "tcp",
            Place::Stack(stack, Zone::Split(Side::Left, Share::First(240.0))),
        )
        .unwrap();
    let Some(Node::Split {
        axis, share, first, ..
    }) = &d.center
    else {
        panic!("centre should be split");
    };
    assert_eq!(*axis, Axis::Row);
    assert_eq!(*share, Share::First(240.0));
    assert_eq!(first.stacks()[0].tabs, [tcp]);
}

#[test]
fn dropping_a_panel_onto_its_own_lone_stack_changes_nothing() {
    let (mut d, arr, _) = arrange_with_mixer();
    let before = d.clone();
    let (_, stack) = d.locate(arr).unwrap();
    d.move_panel(
        arr,
        Place::Stack(stack, Zone::Split(Side::Left, Share::Ratio(0.5))),
    )
    .unwrap();
    d.move_panel(arr, Place::Stack(stack, Zone::Tab)).unwrap();
    assert_eq!(d, before);
}

#[test]
fn attaching_the_tcp_links_it_to_the_arrangement_at_a_fixed_width() {
    let (mut d, arr, _) = arrange_with_mixer();
    let tcp = d.open("tcp", Place::Edge(Side::Right)).unwrap();
    d.attach(tcp, arr, Side::Left, 220.0).unwrap();

    let link = d.panel(arr).unwrap().link;
    assert!(link.is_some());
    assert_eq!(d.panel(tcp).unwrap().link, link);
    assert!(
        d.edges.right.root.is_none(),
        "the empty right dock is pruned"
    );
    assert!(matches!(
        d.center,
        Some(Node::Split {
            share: Share::First(w),
            ..
        }) if w == 220.0
    ));
}

#[test]
fn closing_every_panel_leaves_an_empty_dockspace() {
    let (mut d, arr, mix) = arrange_with_mixer();
    d.close(arr).unwrap();
    d.close(mix).unwrap();
    assert!(d.center.is_none() && d.edges.bottom.root.is_none());
    assert!(d.panels.is_empty());
}

#[test]
fn a_float_has_its_own_tree_and_goes_when_emptied() {
    let (mut d, arr, _) = arrange_with_mixer();
    let ed = d
        .open("editor", Place::Float(Rect::new(10.0, 10.0, 600.0, 400.0)))
        .unwrap();
    let AreaRef::Float(_) = d.locate(ed).unwrap().0 else {
        panic!("editor should float");
    };
    let (_, stack) = d.locate(ed).unwrap();
    d.open(
        "editor",
        Place::Stack(stack, Zone::Split(Side::Right, Share::Ratio(0.5))),
    )
    .unwrap();
    assert_eq!(d.floats[0].root.stacks().len(), 2);

    d.move_panel(ed, Place::Center).unwrap();
    assert_eq!(d.floats.len(), 1);
    assert_eq!(
        d.locate(ed).unwrap().1,
        d.locate(arr).unwrap().1,
        "a tab beside it"
    );
}

#[test]
fn any_change_ends_a_maximise() {
    let (mut d, arr, _) = arrange_with_mixer();
    d.toggle_maximized(arr).unwrap();
    assert_eq!(d.maximized, Some(arr));
    d.open("lyrics", Place::Edge(Side::Right)).unwrap();
    assert_eq!(d.maximized, None);
}

#[test]
fn opening_into_a_hidden_dock_shows_it() {
    let mut d = Dockspace::empty();
    d.set_shown(Side::Left, Shown::Hidden);
    d.open("browser", Place::Edge(Side::Left)).unwrap();
    assert!(d.edges.left.takes_room());
}

#[test]
fn repair_mends_a_damaged_layout() {
    let (mut d, arr, mix) = arrange_with_mixer();
    let ghost = Panel::new("ghost").id;
    let orphan = Panel::new("chart");
    let orphan_id = orphan.id;
    d.panels.push(orphan);
    d.edges.left.root = Some(Node::row(
        Node::one(ghost),
        Node::one(arr),
        Share::Ratio(f32::NAN),
    ));

    let fixed = d.repair();

    assert!(fixed.contains(&Repair::DanglingTab(ghost)));
    assert!(fixed.contains(&Repair::Duplicate(arr)));
    assert!(fixed.contains(&Repair::Orphan(orphan_id)));
    assert!(
        d.edges.left.root.is_none(),
        "left held only the ghost and a duplicate"
    );
    assert!(d.locate(arr).is_some() && d.locate(mix).is_some());
    assert!(d.repair().is_empty(), "repair is idempotent");
}

#[test]
fn a_share_keeps_both_halves_visible() {
    assert_eq!(Share::First(900.0).first_of(1000.0, 200.0), 800.0);
    assert_eq!(Share::Second(50.0).first_of(1000.0, 100.0), 900.0);
    assert_eq!(
        Share::Ratio(0.5).first_of(100.0, 80.0),
        50.0,
        "too short for both minimums"
    );
    assert_eq!(
        Share::Ratio(2.0).sanitized(),
        Share::Ratio(1.0 - Share::MIN_RATIO)
    );
}

fn signal_and_session() -> Screensets {
    let mut desk = Dockspace::empty();
    desk.open("arrangement", Place::Center).unwrap();
    let mut phone = Dockspace::empty();
    phone.open("rig", Place::Center).unwrap();
    Screensets::new(vec![
        Screenset::new("Session")
            .with(FormClass::Desk, desk)
            .in_slot(1),
        Screenset::new("Signal")
            .with(FormClass::PhoneLandscape, phone)
            .in_slot(2),
    ])
}

#[test]
fn a_set_without_a_class_shows_its_nearest_stand_in() {
    let mut s = signal_and_session();
    assert_eq!(
        s.current(FormClass::TabletLandscape).panels[0]
            .kind
            .as_str(),
        "arrangement"
    );
    s.recall(s.in_slot(2).unwrap().id);
    assert_eq!(
        s.current(FormClass::PhonePortrait).panels[0].kind.as_str(),
        "rig"
    );
    assert_eq!(s.current(FormClass::Desk).panels[0].kind.as_str(), "rig");
}

#[test]
fn turning_the_device_keeps_each_orientations_edits() {
    let mut s = signal_and_session();
    s.current(FormClass::TabletLandscape)
        .open("mixer", Place::Edge(Side::Bottom))
        .unwrap();
    s.current(FormClass::TabletPortrait);
    assert_eq!(s.current(FormClass::TabletLandscape).panels.len(), 2);
    assert_eq!(s.current(FormClass::TabletPortrait).panels.len(), 1);
    assert!(s.is_dirty(FormClass::TabletLandscape));
    assert!(!s.is_dirty(FormClass::TabletPortrait));
}

#[test]
fn saving_writes_this_class_and_recalling_drops_unsaved_edits() {
    let mut s = signal_and_session();
    let session = s.active.unwrap();
    s.current(FormClass::TabletLandscape)
        .open("mixer", Place::Edge(Side::Bottom))
        .unwrap();
    assert!(s.save(FormClass::TabletLandscape));
    assert!(!s.is_dirty(FormClass::TabletLandscape));
    assert!(s
        .get(session)
        .unwrap()
        .variant(FormClass::TabletLandscape)
        .is_some());

    s.current(FormClass::Desk)
        .open("chart", Place::Edge(Side::Right))
        .unwrap();
    s.recall(session);
    assert_eq!(s.current(FormClass::Desk).panels.len(), 1);
}

#[test]
fn screensets_survive_json() {
    let mut s = signal_and_session();
    let d = s.current(FormClass::Desk);
    let arr = d.panels[0].id;
    let tcp = d.open("tcp", Place::Edge(Side::Left)).unwrap();
    d.attach(tcp, arr, Side::Left, 200.0).unwrap();
    d.panel_mut(arr).unwrap().state = r#"{"zoom":4}"#.into();
    d.open("editor", Place::Float(Rect::new(1.0, 2.0, 3.0, 4.0)))
        .unwrap();
    s.save(FormClass::Desk);

    let back = Screensets::from_json(&s.to_json().unwrap()).unwrap();
    assert_eq!(back, s);
    assert!(Screensets::from_json("{not json").is_none());
}

#[test]
fn every_built_in_set_has_a_whole_layout_for_every_class() {
    use dock::builtin::{kind, screensets};
    let sets = screensets();
    assert_eq!(sets.active, Some(dock::builtin::id::SESSION));
    for set in &sets.sets {
        for class in FormClass::ALL {
            let mut space = set
                .variant(class)
                .unwrap_or_else(|| panic!("{} has no {class:?} layout", set.name))
                .clone();
            assert!(
                space.repair().is_empty(),
                "{} {class:?} is not whole",
                set.name
            );
            // A phone's transport is in its centre (its docks are sheets, and
            // play/stop must stay one tap away); a desk's and a tablet's is
            // the window's bar, so their layouts hold none.
            let transport = space.of_kind(kind::TRANSPORT).next().map(|p| p.id);
            let at = transport
                .and_then(|t| space.locate(t))
                .map(|(area, _)| area);
            if class.is_phone() {
                assert_eq!(
                    at,
                    Some(AreaRef::Center),
                    "{} {class:?}: transport at {at:?}",
                    set.name
                );
            } else {
                assert_eq!(
                    at, None,
                    "{} {class:?}: the window bar carries the transport",
                    set.name
                );
            }
        }
    }
}

#[test]
fn a_drop_near_the_window_edge_docks_and_over_a_stack_splits_or_tabs() {
    use dock::hit::target;
    let host = Rect::new(0.0, 0.0, 1000.0, 800.0);
    let mut d = Dockspace::empty();
    let a = d.open("arrangement", Place::Center).unwrap();
    let (_, stack) = d.locate(a).unwrap();
    let stacks = [(stack, Rect::new(100.0, 100.0, 800.0, 600.0))];

    assert_eq!(
        target(5.0, 400.0, host, &stacks).unwrap().place,
        Place::Edge(Side::Left)
    );
    assert_eq!(
        target(500.0, 400.0, host, &stacks).unwrap().place,
        Place::Stack(stack, Zone::Tab)
    );
    let right = target(880.0, 400.0, host, &stacks).unwrap();
    assert_eq!(
        right.place,
        Place::Stack(stack, Zone::Split(Side::Right, Share::Ratio(0.5)))
    );
    assert_eq!(right.highlight, Rect::new(500.0, 100.0, 400.0, 600.0));
    assert!(
        target(50.0, 50.0, host, &stacks).is_none(),
        "between the edge band and the stack"
    );
    assert!(target(2000.0, 10.0, host, &stacks).is_none());
}

#[test]
fn panels_have_no_tab_strip_unless_a_stack_asks_for_one() {
    use dock::dockspace::Chrome;
    let (mut d, arr, _) = arrange_with_mixer();
    let (_, stack) = d.locate(arr).unwrap();
    assert_eq!(d.stack(stack).unwrap().chrome, Chrome::Bare);
    d.set_chrome(stack, Chrome::Tabs).unwrap();
    assert_eq!(d.stack(stack).unwrap().chrome, Chrome::Tabs);
}

#[test]
fn built_in_stacks_show_tabs_only_where_panels_share_a_place() {
    use dock::dockspace::Chrome;
    for set in &dock::builtin::screensets().sets {
        for v in &set.variants {
            for (_, root) in v.space.areas() {
                for s in root.stacks() {
                    let want = if s.tabs.len() > 1 {
                        Chrome::Tabs
                    } else {
                        Chrome::Bare
                    };
                    assert_eq!(s.chrome, want, "{} {:?}", set.name, v.class);
                }
            }
        }
    }
}
