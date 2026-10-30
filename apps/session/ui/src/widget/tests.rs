//! The arrangement widget over a made-up song ([`crate::test_song`]).

use super::*;

/// The song with every folder but Drums open, its lanes shut, and the
/// window over it as a panel mounts it, with its planner.
fn window(slices: usize) -> (crate::test_song::Song, ArrangementWidget) {
    let song = crate::test_song::unbreakable(slices);
    for folder in song.raw.tracks.iter().filter(|t| t.is_folder) {
        if folder.guid != song.drums {
            crate::folds::toggle(&folder.guid, false);
        }
    }
    let planner = song.planner();
    let (project, rows) = planner.plan(&song.raw);
    let view: Shared = Rc::new(RefCell::new(View::OPENING));
    let widget =
        ArrangementWidget::for_session(&project, &rows, &song.previews, false, view, false)
            .with_planner(planner, Rc::new(Cell::new(0.0)));
    (song, widget)
}

fn row_of(widget: &ArrangementWidget, guid: &str) -> usize {
    widget
        .rows
        .iter()
        .position(|(track, _)| track.guid == guid)
        .expect("the row")
}

#[test]
fn the_lanes_button_on_a_shut_kit_shows_every_lane() {
    let (song, mut widget) = window(30);
    let kit = row_of(&widget, &song.drums);
    assert_eq!(
        widget.scene.lane_stack(kit),
        None,
        "shut, the kit shows what plays"
    );
    widget.act(crate::pointer::RowSpot {
        row: kit,
        control: crate::row::Control::LaneShow,
    });
    let _ = widget.paint_scene(1600, 900, 1.0);
    let stack = widget.scene.lane_stack(kit).expect("the kit's lanes shown");
    let shown = widget.scene.lanes_shown(kit).expect("lane by lane");
    let names: Vec<&str> = stack
        .lanes()
        .iter()
        .filter_map(|lane| crate::lanes::name(shown, *lane))
        .collect();
    assert_eq!(
        names.len(),
        crate::test_song::DRUM_LANES as usize,
        "every lane: {names:?}"
    );
    assert_eq!(
        names[..3],
        ["EDIT", "COMP", "C1"],
        "what plays, the comp, then the takes"
    );
}

#[test]
fn the_lanes_button_on_a_track_shows_every_lane() {
    let (song, mut widget) = window(30);
    crate::folds::toggle(&song.drums, false);
    let _ = widget.paint_scene(1600, 900, 1.0);
    let mic = row_of(&widget, &song.mics[0]);
    assert_eq!(widget.scene.lane_stack(mic), None, "one lane, what plays");
    widget.act(crate::pointer::RowSpot {
        row: mic,
        control: crate::row::Control::LaneShow,
    });
    let _ = widget.paint_scene(1600, 900, 1.0);
    let stack = widget.scene.lane_stack(mic).expect("the mic's lanes shown");
    assert_eq!(stack.lanes().len(), crate::test_song::DRUM_LANES as usize);
}

/// The guid of the folder called `name`.
fn folder(song: &crate::test_song::Song, name: &str) -> String {
    song.raw
        .tracks
        .iter()
        .find(|t| t.is_folder && t.name == name)
        .map(|t| t.guid.clone())
        .expect("the folder")
}

#[test]
fn the_lanes_button_on_an_open_kit_shows_every_lane_of_its_shut_pieces() {
    let (song, mut widget) = window(30);
    // The kit open, its pieces shut: a row a drum, each a fold.
    crate::folds::toggle(&song.drums, false);
    let pieces: Vec<String> = ["Kick", "Snare", "Toms", "Cymbals", "Rooms"]
        .into_iter()
        .map(|name| folder(&song, name))
        .collect();
    for piece in &pieces {
        crate::folds::toggle(piece, true);
    }
    let _ = widget.paint_scene(1600, 900, 1.0);
    let kit = row_of(&widget, &song.drums);
    let kick = row_of(&widget, &pieces[0]);
    assert_eq!(
        widget.scene.lane_stack(kick),
        None,
        "shut, the kick shows what plays"
    );
    widget.act(crate::pointer::RowSpot {
        row: kit,
        control: crate::row::Control::LaneShow,
    });
    let _ = widget.paint_scene(1600, 900, 1.0);
    for piece in &pieces {
        let row = row_of(&widget, piece);
        let stack = widget
            .scene
            .lane_stack(row)
            .expect("each piece's lanes shown");
        assert_eq!(stack.lanes().len(), crate::test_song::DRUM_LANES as usize);
    }
    // And shut again, the same way.
    widget.act(crate::pointer::RowSpot {
        row: kit,
        control: crate::row::Control::LaneShow,
    });
    let _ = widget.paint_scene(1600, 900, 1.0);
    assert_eq!(widget.scene.lane_stack(row_of(&widget, &pieces[0])), None);
}

#[test]
fn a_lane_stepped_on_a_shut_kit_folds_the_kit_from_that_lane() {
    crate::folds::show_cuts(true);
    let (song, mut widget) = window(30);
    let kit = row_of(&widget, &song.drums);
    let drawn = |widget: &ArrangementWidget| {
        widget
            .scene
            .item_boxes()
            .iter()
            .filter(|b| b.row == kit)
            .count()
    };
    assert_eq!(drawn(&widget), 30, "what plays: each mic's EDIT slices");
    widget.act(crate::pointer::RowSpot {
        row: kit,
        control: crate::row::Control::LaneNext,
    });
    let _ = widget.paint_scene(1600, 900, 1.0);
    assert_eq!(
        drawn(&widget),
        40,
        "the COMP lane's pieces, every mic stepped to it"
    );
}

#[test]
fn an_edit_of_items_draws_them_without_planning_the_rows_again() {
    crate::folds::show_cuts(true);
    let (song, mut widget) = window(30);
    let _ = widget.paint_scene(1600, 900, 1.0);
    let Some((planner, raw)) = widget.replan.clone() else {
        panic!("a planner");
    };
    // Three of a mic's slices moved later, as the engine says.
    let moved: Vec<session_engine::view::ItemNow> =
        crate::test_song::on_lane(&raw.borrow(), &song.mics[2], 0)
            .into_iter()
            .skip(5)
            .take(3)
            .map(|item| session_engine::view::ItemNow {
                item: daw_proto::Item {
                    position: daw_proto::PositionInSeconds::from_seconds(
                        item.position.as_seconds() + 0.01,
                    ),
                    ..item.clone()
                },
                take_name: raw.borrow().names.get(&item.guid).cloned(),
                midi: false,
            })
            .collect();
    let _ = crate::song_model::patch_with(&mut raw.borrow_mut(), moved, &[]);
    let rows = widget.rows.clone();
    assert!(
        widget.repatched(),
        "only items changed: the rows stand, the items drawn again"
    );
    let (project, planned_rows) = planner.plan(&raw.borrow());
    assert_eq!(widget.rows, rows, "the rows stand");
    assert_eq!(
        planned_rows.as_slice(),
        rows.as_slice(),
        "as planning them would"
    );
    assert!(widget.project == *project.0, "the rows' project as planned");
    let fresh = Arrangement::build(
        &widget.palette,
        &widget.font,
        &project,
        &planned_rows,
        widget.layout,
        &song.previews,
        widget.scene.tcp,
    );
    assert_eq!(
        widget.scene.item_boxes(),
        fresh.item_boxes(),
        "drawn as a fresh recording"
    );
}
