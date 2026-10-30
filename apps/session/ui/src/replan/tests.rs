use super::*;

use session_engine::view::ItemNow;

/// The song, opened as a person working on its kit has it.
fn song(slices: usize) -> crate::test_song::Song {
    let song = crate::test_song::unbreakable(slices);
    song.open_as_edited();
    song
}

/// `item` as the engine would hand it back, its take named `name`.
fn now(song: &crate::test_song::Song, item: daw_proto::Item) -> ItemNow {
    ItemNow {
        take_name: song.raw.names.get(&item.guid).cloned(),
        item,
        midi: false,
    }
}

/// Mic `mic`'s EDIT slices `range`, each `f` of itself.
fn edit(
    song: &crate::test_song::Song,
    mic: usize,
    range: std::ops::Range<usize>,
    f: impl Fn(daw_proto::Item) -> daw_proto::Item,
) -> Vec<ItemNow> {
    crate::test_song::on_lane(&song.raw, &song.mics[mic], 0)
        .into_iter()
        .skip(range.start)
        .take(range.len())
        .map(|item| now(song, f(item.clone())))
        .collect()
}

fn later(by: f64) -> impl Fn(daw_proto::Item) -> daw_proto::Item {
    move |item| daw_proto::Item {
        position: daw_proto::PositionInSeconds::from_seconds(item.position.as_seconds() + by),
        ..item
    }
}

/// `planned` patched up to `song`'s raw project, and what planning it
/// whole makes.
fn patched_and_planned(
    song: &crate::test_song::Song,
    planned: &mut Project,
    rows: &[(Track, u32)],
) -> (Patched, Project) {
    let patched = patch_planned(planned, &song.raw, rows, Folding::current());
    let (fresh, fresh_rows) = song.planner().plan(&song.raw);
    assert_eq!(fresh_rows.as_slice(), rows, "the rows stand");
    (patched, (*fresh.0).clone())
}

#[test]
fn a_patched_plan_after_a_hit_move_is_the_plan_made_again() {
    for cuts in [false, true] {
        crate::folds::show_cuts(cuts);
        let mut song = song(80);
        let (planned, rows) = song.planner().plan(&song.raw);
        let mut planned = (*planned.0).clone();
        let kit = planned.folds[&song.drums].clone();
        let moved = edit(&song, 3, 20..23, later(0.005));
        let _ = crate::song_model::patch_with(&mut song.raw, moved, &[]);
        let (patched, fresh) = patched_and_planned(&song, &mut planned, &rows);
        assert_eq!(patched, Patched::Items);
        assert!(
            planned == fresh,
            "cuts {cuts}: the same project, folds and all"
        );
        assert!(planned.folds[&song.drums] != kit, "the kit folded again");
    }
}

#[test]
fn a_patched_plan_after_slices_are_cut_and_removed_is_the_plan_made_again() {
    crate::folds::show_cuts(true);
    let mut song = song(60);
    let (planned, rows) = song.planner().plan(&song.raw);
    let mut planned = (*planned.0).clone();
    // A slice cut in two, another gone, a third renamed.
    let first = crate::test_song::on_lane(&song.raw, &song.mics[1], 0)[10].clone();
    let half = first.length.as_seconds() / 2.0;
    let left = daw_proto::Item {
        length: daw_proto::Duration::from_seconds(half),
        ..first.clone()
    };
    let right = daw_proto::Item {
        guid: "{ITEM-CUT}".to_owned(),
        position: daw_proto::PositionInSeconds::from_seconds(first.position.as_seconds() + half),
        length: daw_proto::Duration::from_seconds(half),
        ..first.clone()
    };
    let gone = crate::test_song::on_lane(&song.raw, &song.mics[2], 0)[30]
        .guid
        .clone();
    let renamed = crate::test_song::on_lane(&song.raw, &song.mics[4], 0)[5].clone();
    let changed = vec![
        now(&song, left),
        ItemNow {
            take_name: Some("65-Out-260907_1321-02.wav".to_owned()),
            ..now(&song, right)
        },
        ItemNow {
            take_name: Some("65-In-260907_1321-09.wav".to_owned()),
            ..now(&song, renamed)
        },
    ];
    let _ = crate::song_model::patch_with(&mut song.raw, changed, &[gone]);
    let (patched, fresh) = patched_and_planned(&song, &mut planned, &rows);
    assert_eq!(patched, Patched::Items);
    assert!(planned == fresh, "the same project, folds and all");
}

#[test]
fn a_patched_plan_after_a_plain_track_s_edit_is_the_plan_made_again() {
    crate::folds::show_cuts(false);
    let mut song = song(40);
    let (planned, rows) = song.planner().plan(&song.raw);
    let mut planned = (*planned.0).clone();
    let plain = rows
        .iter()
        .find(|(t, _)| !t.is_folder && t.lane_count == 0)
        .map(|(t, _)| t.guid.clone())
        .expect("a plain track");
    let item = song.raw.items[&plain][3].clone();
    let moved = vec![now(&song, later(1.5)(item))];
    let _ = crate::song_model::patch_with(&mut song.raw, moved, &[]);
    let (patched, fresh) = patched_and_planned(&song, &mut planned, &rows);
    assert_eq!(patched, Patched::Items);
    assert!(planned == fresh, "the same project");
}

#[test]
fn nothing_changed_is_the_same() {
    let song = song(20);
    let (planned, rows) = song.planner().plan(&song.raw);
    let mut planned = (*planned.0).clone();
    assert_eq!(
        patch_planned(&mut planned, &song.raw, &rows, Folding::current()),
        Patched::Same
    );
}

#[test]
fn a_track_changed_plans_the_rows_again() {
    let mut song = song(20);
    let (planned, rows) = song.planner().plan(&song.raw);
    let mut planned = (*planned.0).clone();
    song.raw.tracks[3].name = "Renamed".to_owned();
    assert_eq!(
        patch_planned(&mut planned, &song.raw, &rows, Folding::current()),
        Patched::Replan
    );
}

#[test]
fn a_patched_plan_after_a_slice_changes_lane_is_the_plan_made_again() {
    for cuts in [false, true] {
        crate::folds::show_cuts(cuts);
        let mut song = song(40);
        let (planned, rows) = song.planner().plan(&song.raw);
        let mut planned = (*planned.0).clone();
        let to_take = edit(&song, 6, 12..13, |item| daw_proto::Item {
            fixed_lane: Some(5),
            ..item
        });
        let _ = crate::song_model::patch_with(&mut song.raw, to_take, &[]);
        let (patched, fresh) = patched_and_planned(&song, &mut planned, &rows);
        assert_eq!(patched, Patched::Items);
        assert!(
            planned == fresh,
            "cuts {cuts}: the same project, both lanes folded again"
        );
    }
}
