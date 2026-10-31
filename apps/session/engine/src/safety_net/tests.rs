//! The checkpoint before a drum rewrite: one per action as the user ran
//! it, none for a dry run or what a key runs over a selection, uniquely
//! named, refused when it cannot be made — and only the automatic forks of
//! older songs pruned. (A mark holding the song as it was, and going back
//! to it: `history`'s tests.)

use std::cell::Cell;

use daw::standalone::Standalone;

use super::{BIG_STRETCH, Candidate, Reach, guarded, is_automatic, name_for, on_a_key, prune_plan};

#[test]
fn a_name_already_taken_gets_the_next_number() {
    let stamp = "2026-10-31 14:05";
    assert_eq!(
        name_for("DRUMS_REQUANTIZE", stamp, &[]),
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05"
    );
    let taken = vec![
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05".to_owned(),
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05 2".to_owned(),
    ];
    assert_eq!(
        name_for("DRUMS_REQUANTIZE", stamp, &taken),
        "Before DRUMS_REQUANTIZE 2026-10-31 14:05 3"
    );
}

#[test]
fn every_rewrite_is_far_enough_but_a_dry_run() {
    let cases = [
        (Reach::Song, true),
        (Reach::Nothing, false),
        (Reach::song_unless(true), false),
        (Reach::song_unless(false), true),
        (Reach::of_spans([(10.0, 10.0 + BIG_STRETCH)]), true),
        (Reach::of_spans([(0.0, 0.5)]), true),
        (Reach::of_spans([(0.0, 20.0), (50.0, 70.0)]), true),
    ];
    for (reach, far) in cases {
        assert_eq!(reach.is_big(), far, "{reach:?}");
    }
}

#[test]
fn an_action_run_by_another_runs_under_its_checkpoint() {
    let forks = Cell::new(0_u32);
    let fork = || {
        forks.set(forks.get() + 1);
        Ok(format!("Before X {}", forks.get()))
    };
    {
        let outer = guarded(Reach::Song, fork).expect("forked");
        let inner = guarded(Reach::Song, fork).expect("under the outer one");
        assert_eq!(inner.name(), outer.name());
        assert_eq!(outer.name().as_deref(), Some("Before X 1"));
    }
    // The outer one done: the next action forks its own.
    let next = guarded(Reach::Song, fork).expect("forked");
    assert_eq!(next.name().as_deref(), Some("Before X 2"));
    assert_eq!(forks.get(), 2);
}

#[test]
fn a_dry_run_marks_nothing_and_a_failed_mark_refuses() {
    let small =
        guarded(Reach::Nothing, || Err("must not fork".to_owned())).expect("nothing to fork");
    assert_eq!(small.name(), None);
    let refused = guarded(Reach::Song, || Err("the file refused".to_owned()));
    assert_eq!(
        refused.map(|n| n.name()),
        Err("the file refused".to_owned())
    );
    // A refusal holds nothing: the next action forks its own.
    let next = guarded(Reach::Song, || Ok("Before Y".to_owned())).expect("forked");
    assert_eq!(next.name().as_deref(), Some("Before Y"));
}

/// What a key runs over a selection forks nothing, nor does any action it
/// runs (Q runs QUANTIZE): a keypress never waits on a save. After it,
/// the next action forks its own.
#[test]
fn an_action_a_key_runs_marks_nothing_nor_does_what_it_runs() {
    {
        let _key = on_a_key();
        let inner = guarded(Reach::Song, || Err("must not fork".to_owned()))
            .expect("under the key: nothing to fork");
        assert_eq!(inner.name(), None);
    }
    let next = guarded(Reach::Song, || Ok("Before Z".to_owned())).expect("forked");
    assert_eq!(next.name().as_deref(), Some("Before Z"));
}

#[test]
fn only_a_checkpoint_a_rewrite_made_is_automatic() {
    for (name, automatic) in [
        ("Before DRUMS_REQUANTIZE 2026-10-31 14:05", true),
        ("Before QUANTIZE 2026-10-31 14:05 3", true),
        ("Before HITS_DETECT 2026-01-02 09:00", true),
        // Named by a user, or kept aside by a live set.
        ("Before drum quantize", false),
        ("Drum edit (chart, leveled)", false),
        ("Before joining Sunday Night (2026-10-29)", false),
        ("Before QUANTIZE today", false),
        ("Before QUANTIZE 2026-10-31 14:05 mine", false),
        ("main", false),
    ] {
        assert_eq!(is_automatic(name), automatic, "{name}");
    }
}

/// Pruning keeps the newest `keep` automatic checkpoints and lets go of
/// those older than `older_than_days`, never touching a named one, the
/// one open, or one another version opens through.
#[test]
fn pruning_lets_go_of_old_automatic_checkpoints_only() {
    const DAY: i64 = 86_400;
    let now = 100 * DAY;
    let auto = |n: u32, days_ago: i64| Candidate {
        name: format!("Before QUANTIZE 2026-10-{:02} 10:00", n),
        at: now - days_ago * DAY,
        parent: false,
        open: false,
    };
    let mut branches = vec![
        Candidate {
            name: "main".to_owned(),
            at: 0,
            parent: true,
            open: false,
        },
        Candidate {
            name: "Mix A".to_owned(),
            at: 0,
            parent: false,
            open: false,
        },
        auto(1, 30),
        auto(2, 20),
        auto(3, 10),
        auto(4, 1),
    ];
    let keep_two = prune_plan(&branches, (Some(2), None), now);
    assert_eq!(
        keep_two.removed,
        [
            "Before QUANTIZE 2026-10-02 10:00",
            "Before QUANTIZE 2026-10-01 10:00"
        ]
    );
    assert_eq!(keep_two.kept, 2);

    let older = prune_plan(&branches, (None, Some(15.0)), now);
    assert_eq!(
        older.removed,
        [
            "Before QUANTIZE 2026-10-02 10:00",
            "Before QUANTIZE 2026-10-01 10:00"
        ]
    );

    // The oldest is open; the next was forked from: both left, and said.
    branches[2].open = true;
    branches[3].parent = true;
    let guarded = prune_plan(&branches, (Some(0), None), now);
    assert_eq!(
        guarded.removed,
        [
            "Before QUANTIZE 2026-10-04 10:00",
            "Before QUANTIZE 2026-10-03 10:00"
        ]
    );
    assert_eq!(guarded.left.len(), 2, "{:?}", guarded.left);
    assert!(
        !guarded.removed.iter().any(|n| n == "Mix A" || n == "main"),
        "a named version is never let go"
    );
}

/// Song `guid` seeded into `daw` with one track, `Kick`, at `volume`.
fn seed_song(daw: &Standalone, guid: &str, volume: f64) {
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    });
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let facade = daw::standalone::bootstrap::build_in_process_daw(daw.clone())
                .await
                .expect("a facade");
            let project = facade.daw.project(guid.to_owned()).await.expect("the song");
            project
                .tracks()
                .add_with_guid("{KICK}", "Kick", None)
                .await
                .expect("a track");
        });
    set_volume(daw, guid, volume);
}

fn set_volume(daw: &Standalone, guid: &str, volume: f64) {
    daw.write_project(guid, |p| {
        if let Some(t) = p.tracks.iter_mut().find(|t| t.name == "Kick") {
            t.volume = volume;
        }
    });
}

#[test]
fn a_song_not_open_from_its_file_is_refused() {
    let daw = Standalone::new();
    seed_song(&daw, "fileless-song", 0.5);
    let refused = super::mark_before("fileless-song", "QUANTIZE", "2026-10-31 14:05");
    assert!(
        refused
            .as_ref()
            .is_err_and(|why: &String| why.contains("project file")),
        "{refused:?}"
    );
}
