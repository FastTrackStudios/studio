//! A song's file folds its history into a snapshot as it grows: past
//! `COMPACT_AFTER` edit steps, the next one's save compacts the branch, so
//! an open decodes the song as it is rather than replaying every step
//! (Unbreakable's 377 steps took 16 s to open; its snapshot 0.2 s). And
//! nothing is lost: the song loads back with every step in it.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use daw::service::{ProjectContext, Tracks};

const EMPTY: &str = "<REAPER_PROJECT 0.1 \"7.0/test\" 0\n  TEMPO 120 4 4\n>\n";

/// More steps than a branch keeps before it is compacted.
const STEPS: usize = 70;

#[test]
fn a_long_history_is_folded_into_a_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rpp = dir.path().join("Long Song.RPP");
    std::fs::write(&rpp, EMPTY).expect("write project");
    let opened = session_daw::open::open_silent(&rpp).expect("open");
    let _runtime = session_daw::open::runtime()
        .expect("engine runtime")
        .enter();
    let saved = rpp.with_extension("session");
    session_daw::session_file::save_session(&opened.daw, &opened.project_guid, &saved)
        .expect("save");

    let song = ProjectContext::Project(opened.project_guid.clone());
    let steps = || -> Vec<String> {
        session_format::store::Store::open(&saved)
            .and_then(|store| store.steps(session_format::store::MAIN))
            .map(|steps| steps.into_iter().map(|s| s.label).collect())
            .unwrap_or_default()
    };
    let mut compacted = false;
    for i in 0..STEPS {
        let label = format!("Add T{i}");
        let before = steps().len();
        daw::service::Projects::begin_undo_block(&opened.daw, song.clone(), &label);
        Tracks::add(&opened.daw, song.clone(), &format!("T{i}"), None).expect("a track");
        daw::service::Projects::end_undo_block(&opened.daw, song.clone(), &label, None);
        // Recorded before the next, so each is its own update: the count
        // grows by it — or falls, when its save compacted the branch.
        let began = std::time::Instant::now();
        loop {
            let now = steps().len();
            if now > before {
                break;
            }
            if now < before {
                compacted = true;
                break;
            }
            assert!(began.elapsed().as_secs() < 20, "{label} recorded");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    assert!(compacted, "the branch was compacted");
    // The last step recorded too (steps that finish together are recorded
    // as one commit, named for them all) before the file is read back.
    let last = format!("T{}", STEPS - 1);
    let began = std::time::Instant::now();
    while !steps().iter().any(|l| l.contains(&last)) {
        assert!(
            began.elapsed().as_secs() < 20,
            "{last} recorded: {:?}",
            steps()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Compacted on the way: the updates kept are the ones since.
    let kept = steps();
    assert!(
        kept.len() < STEPS,
        "{} updates kept after {STEPS} steps: {kept:?}",
        kept.len()
    );

    // Loaded back as a new song: every step in it.
    let again = session_daw::project_file::load(
        &opened.daw,
        "Long Song",
        &saved,
        session_format::store::MAIN,
    )
    .expect("it loads");
    let names: Vec<String> = Tracks::all(&opened.daw, ProjectContext::Project(again))
        .into_iter()
        .map(|t| t.name)
        .collect();
    for i in 0..STEPS {
        assert!(
            names.contains(&format!("T{i}")),
            "T{i} survived the compaction: {names:?}"
        );
    }
}
