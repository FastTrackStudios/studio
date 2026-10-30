//! The arrangement's recording: a rebuild draws what a fresh build draws,
//! and what it costs, over a song shaped like Unbreakable
//! ([`crate::test_song`]).

use super::*;

fn palette() -> Palette {
    Palette::from_theme(&daw_ui::theming::Theme::dark())
}

/// One face for every recording, as a window has: the panel is kept
/// across recordings only in the face it was cut in.
fn font() -> &'static crate::text::Font {
    thread_local! {
        static FONT: &'static crate::text::Font =
            Box::leak(Box::new(crate::text::Font::embedded().expect("the embedded font")));
    }
    FONT.with(|font| *font)
}

/// Milliseconds since `at`.
fn ms(at: std::time::Instant) -> f64 {
    at.elapsed().as_secs_f64() * 1e3
}

/// The song, opened as a person working on its kit has it, small enough
/// to build in a test (`slices` slices a mic).
fn song(slices: usize) -> crate::test_song::Song {
    let song = crate::test_song::unbreakable(slices);
    song.open_as_edited();
    song
}

/// What the window draws `project`'s `rows` with, recorded.
struct Draw<'a> {
    palette: Palette,
    font: &'static crate::text::Font,
    previews: &'a crate::midi::Previews,
    hidden: std::collections::HashSet<String>,
}

impl<'a> Draw<'a> {
    fn new(previews: &'a crate::midi::Previews) -> Self {
        Self {
            palette: palette(),
            font: font(),
            previews,
            hidden: std::collections::HashSet::new(),
        }
    }

    fn build(&self, (project, rows): (&Project, &[(daw_proto::Track, u32)])) -> Arrangement {
        Arrangement::build_hiding(
            &self.palette,
            self.font,
            (project, rows),
            crate::layout::Layout::default(),
            self.previews,
            crate::tcp::Tcp::FULL,
            &self.hidden,
        )
    }

    fn rebuild(
        &self,
        scene: &mut Arrangement,
        (project, rows): (&Project, &[(daw_proto::Track, u32)]),
    ) {
        scene.rebuild_hiding(
            &self.palette,
            self.font,
            (project, rows),
            crate::layout::Layout::default(),
            self.previews,
            crate::tcp::Tcp::FULL,
            &self.hidden,
        );
    }
}

/// `slices` EDIT slices of mic `mic`, from the `first`th, moved later by
/// `by` seconds: a hit move, as the window reads it back
/// ([`crate::song_model::patch_with`]). The slices moved.
fn move_hits(
    song: &mut crate::test_song::Song,
    mic: usize,
    (first, slices): (usize, usize),
    by: f64,
) -> Vec<String> {
    let guid = song.mics[mic].clone();
    let moved: Vec<session_engine::view::ItemNow> = crate::test_song::on_lane(&song.raw, &guid, 0)
        .into_iter()
        .skip(first)
        .take(slices)
        .map(|item| session_engine::view::ItemNow {
            item: daw_proto::Item {
                position: daw_proto::PositionInSeconds::from_seconds(
                    item.position.as_seconds() + by,
                ),
                ..item.clone()
            },
            take_name: song.raw.names.get(&item.guid).cloned(),
            midi: false,
        })
        .collect();
    crate::song_model::patch_with(&mut song.raw, moved, &[])
}

/// `items`' waveforms read again, as the window does after an edit.
fn read_again(previews: &crate::midi::Previews, items: &[String]) {
    previews.put_waves(
        items
            .iter()
            .map(|guid| (guid.clone(), crate::test_song::wave_of(0.2, 7))),
    );
}

/// The waveforms drawn at replay over the same bodies, alike: the same
/// peaks, detail sources, place, colour, gain and drum.
fn assert_same_waves(rebuilt: &Arrangement, fresh: &Arrangement) {
    let mut over: Vec<&u32> = fresh.index.waves.keys().collect();
    over.sort_unstable();
    let mut rebuilt_over: Vec<&u32> = rebuilt.index.waves.keys().collect();
    rebuilt_over.sort_unstable();
    assert_eq!(rebuilt_over, over, "waveforms over the same bodies");
    for at in over {
        let (a, b) = (&rebuilt.index.waves[at], &fresh.index.waves[at]);
        assert_eq!(a.len(), b.len(), "as many layers over body {at}");
        for (a, b) in a.iter().zip(b) {
            let place = |l: &LaneWave| {
                (
                    [l.x0, l.x1, l.top, l.bottom, l.gain].map(f64::to_bits),
                    l.color,
                    l.role,
                )
            };
            assert!(*a.wave == *b.wave, "the same peaks over body {at}");
            assert_eq!(a.sources, b.sources, "read the same over body {at}");
            assert_eq!(place(a), place(b), "drawn alike over body {at}");
        }
    }
}

/// `rebuilt` draws, hit-tests and lays out exactly as `fresh` does.
fn assert_same(rebuilt: &Arrangement, fresh: &Arrangement) {
    assert_eq!(
        rebuilt.lanes.commands.len(),
        fresh.lanes.commands.len(),
        "as many commands"
    );
    let differs = rebuilt
        .lanes
        .commands
        .iter()
        .zip(&fresh.lanes.commands)
        .position(|(a, b)| a != b);
    assert_eq!(differs, None, "the same commands, in order");
    assert_eq!(
        rebuilt.index.lanes, fresh.index.lanes,
        "each row's commands"
    );
    assert_eq!(rebuilt.index.x, fresh.index.x, "each command's extent");
    assert_eq!(rebuilt.index.panel, fresh.index.panel, "the panel's rows");
    assert_eq!(
        rebuilt.index.panel_bar, fresh.index.panel_bar,
        "the bands' rows"
    );
    let at = rebuilt
        .panel
        .commands
        .iter()
        .zip(&fresh.panel.commands)
        .position(|(a, b)| a != b);
    assert!(
        rebuilt.panel == fresh.panel,
        "the same panel: {} vs {} commands, first differing {at:?}: {}",
        rebuilt.panel.commands.len(),
        fresh.panel.commands.len(),
        at.map(|i| format!(
            "{:?}\n{:?}",
            rebuilt.panel.commands[i], fresh.panel.commands[i]
        ))
        .unwrap_or_default()
        .chars()
        .take(1500)
        .collect::<String>()
    );
    assert!(rebuilt.panel_bar == fresh.panel_bar, "the same bands");
    assert_same_waves(rebuilt, fresh);
    assert_eq!(rebuilt.items, fresh.items, "the same item boxes");
    assert_eq!(rebuilt.titles, fresh.titles, "the same titles");
    assert_eq!(rebuilt.track_h, fresh.track_h);
    assert_eq!(rebuilt.head_h, fresh.head_h);
    assert_eq!(rebuilt.rows_at, fresh.rows_at);
    assert_eq!(rebuilt.lane_tracks, fresh.lane_tracks);
    assert_eq!(rebuilt.laned, fresh.laned);
    assert_eq!(rebuilt.stacks, fresh.stacks);
    assert_eq!(rebuilt.takes, fresh.takes);
    assert_eq!(rebuilt.lane_takes, fresh.lane_takes);
    assert_eq!(rebuilt.item_count, fresh.item_count);
}

/// The song planned as the window plans it.
fn planned(song: &crate::test_song::Song) -> (daw_ui::studio::ProjectRef, daw_ui::studio::RowsRef) {
    song.planner().plan(&song.raw)
}

#[test]
fn a_rebuild_after_a_hit_move_draws_what_a_fresh_build_does() {
    for cuts in [false, true] {
        crate::folds::show_cuts(cuts);
        let mut song = song(120);
        let draw = Draw::new(&song.previews);
        let (project, rows) = planned(&song);
        let mut scene = draw.build((&project, &rows));
        drop(draw);
        // Edit after edit, each recording from the last: what one keeps
        // must hold for the next.
        for (mic, first) in [(2, 40), (7, 80), (2, 41)] {
            let moved = move_hits(&mut song, mic, (first, 3), 0.005);
            let draw = Draw::new(&song.previews);
            let (project, rows) = planned(&song);
            draw.rebuild(&mut scene, (&project, &rows));
            assert_same(&scene, &draw.build((&project, &rows)));
            // And once their waveforms are read again.
            read_again(&song.previews, &moved);
            draw.rebuild(&mut scene, (&project, &rows));
            assert_same(&scene, &draw.build((&project, &rows)));
        }
    }
}

#[test]
fn a_rebuild_takes_every_row_an_edit_did_not_touch() {
    crate::folds::show_cuts(true);
    let mut song = song(120);
    let draw = Draw::new(&song.previews);
    let (project, rows) = planned(&song);
    let mut scene = draw.build((&project, &rows));
    drop(draw);
    // The waveforms as they were: a move the window has not read again.
    let _ = move_hits(&mut song, 0, (10, 2), 0.01);
    let draw = Draw::new(&song.previews);
    let (project, rows) = planned(&song);
    let drums = rows
        .iter()
        .position(|(t, _)| t.guid == song.drums)
        .expect("the Drums row");
    let before: Vec<u64> = scene.kept.rows_whole();
    let pieces: Vec<u64> = scene.kept.chunk_keys(drums);
    draw.rebuild(&mut scene, (&project, &rows));
    let after: Vec<u64> = scene.kept.rows_whole();
    let changed: Vec<usize> = before
        .iter()
        .zip(&after)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(row, _)| row)
        .collect();
    assert_eq!(changed, vec![drums], "only the kit's row changed");
    // Of the kit's row, only the pieces the two slices are in.
    let recorded = scene
        .kept
        .chunk_keys(drums)
        .into_iter()
        .filter(|key| !pieces.contains(key))
        .count();
    assert!(
        (1..=2).contains(&recorded),
        "{recorded} of the kit's {} pieces recorded again",
        pieces.len()
    );
    assert_same(&scene, &draw.build((&project, &rows)));
}

#[test]
fn a_rebuild_after_waveforms_arrive_draws_what_a_fresh_build_does() {
    crate::folds::show_cuts(false);
    let song = song(80);
    let draw = Draw::new(&song.previews);
    let (project, rows) = planned(&song);
    let mut scene = draw.build((&project, &rows));
    // A waveform read again on a plain track and under the kit.
    let plain = rows
        .iter()
        .find(|(t, _)| !t.is_folder && t.lane_count == 0)
        .and_then(|(t, _)| song.raw.items.get(&t.guid)?.first())
        .map(|i| i.guid.clone())
        .expect("a plain track's item");
    let under = crate::test_song::on_lane(&song.raw, &song.mics[5], 0)[7]
        .guid
        .clone();
    song.previews.put_waves([
        (plain, crate::test_song::wave_of(18.0, 99)),
        (under, crate::test_song::wave_of(3.0, 98)),
    ]);
    draw.rebuild(&mut scene, (&project, &rows));
    assert_same(&scene, &draw.build((&project, &rows)));
}

#[test]
fn a_rebuild_after_a_folder_opens_draws_what_a_fresh_build_does() {
    crate::folds::show_cuts(true);
    let song = song(60);
    let draw = Draw::new(&song.previews);
    let (project, rows) = planned(&song);
    let mut scene = draw.build((&project, &rows));
    crate::folds::toggle(&song.drums, false);
    let (project, rows) = planned(&song);
    draw.rebuild(&mut scene, (&project, &rows));
    assert_same(&scene, &draw.build((&project, &rows)));
    crate::folds::toggle(&song.drums, true);
    let (project, rows) = planned(&song);
    draw.rebuild(&mut scene, (&project, &rows));
    assert_same(&scene, &draw.build((&project, &rows)));
}

#[test]
fn chunks_never_cut_a_run_of_slices() {
    let items: Vec<daw_proto::Item> = (0..40)
        .map(|k| daw_proto::Item {
            guid: format!("i{k}"),
            position: daw_proto::PositionInSeconds::from_seconds(f64::from(k) * 0.5),
            length: daw_proto::Duration::from_seconds(0.5),
            ..daw_proto::Item::default()
        })
        .collect();
    let refs: Vec<&daw_proto::Item> = items.iter().collect();
    let joined = |at: usize| (8..30).contains(&at);
    let runs = seamless_runs(&refs, |_, _| true, joined);
    let chunks = record::chunks_of(&refs, &runs);
    assert_eq!(
        chunks,
        vec![0..8, 8..32, 32..40],
        "cut every 4 s, the run 8..30 whole"
    );
}

#[test]
#[ignore = "timing, asserts nothing: cargo nextest run -p session-ui --run-ignored only -E 'test(=arrangement::tests::rebuild_after_a_hit_move_timings)' --no-capture"]
fn rebuild_after_a_hit_move_timings() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let mut song = song(1300);
    for cuts in [false, true] {
        crate::folds::show_cuts(cuts);
        let planner = song.planner();
        let (project, rows) = planner.plan(&song.raw);
        let mut planned = (*project.0).clone();
        let mut scene = Draw::new(&song.previews).build((&project, &rows));
        // As the window did: the rows planned again, the song recorded whole
        // (its folders' mixes kept from the recording before).
        for edit in 0..5 {
            let _ = move_hits(&mut song, edit, (300 + edit * 80, 3), 0.005);
            let draw = Draw::new(&song.previews);
            let at = std::time::Instant::now();
            let (project, rows) = planner.plan(&song.raw);
            let plan_ms = ms(at);
            let at = std::time::Instant::now();
            let fresh = draw.build((&project, &rows));
            let build_ms = ms(at);
            let at = std::time::Instant::now();
            let _ = planner.plan_strips(&song.raw);
            let strips_ms = ms(at);
            drop(fresh);
            tracing::info!(
                cuts,
                plan_ms,
                build_ms,
                strips_ms,
                "before: after a hit move"
            );
        }
        // As it does: the rows' project patched, the recording cut again
        // from the last — and again once the moved slices' peaks arrive.
        let (project, rows) = planner.plan(&song.raw);
        planned.clone_from(&project.0);
        Draw::new(&song.previews).rebuild(&mut scene, (&planned, &rows));
        for edit in 5..10 {
            let moved = move_hits(&mut song, edit, (300 + edit * 80, 3), 0.005);
            let draw = Draw::new(&song.previews);
            let at = std::time::Instant::now();
            let patched = crate::replan::patch_planned(
                &mut planned,
                &song.raw,
                &rows,
                crate::replan::Folding::current(),
            );
            let patch_ms = ms(at);
            assert_eq!(patched, crate::replan::Patched::Items);
            let at = std::time::Instant::now();
            draw.rebuild(&mut scene, (&planned, &rows));
            let edited_ms = ms(at);
            read_again(&song.previews, &moved);
            let at = std::time::Instant::now();
            draw.rebuild(&mut scene, (&planned, &rows));
            let arrived_ms = ms(at);
            tracing::info!(
                cuts,
                rows = rows.len(),
                items = project.item_count,
                patch_ms,
                edited_ms,
                arrived_ms,
                "after: after a hit move"
            );
        }
    }
}
