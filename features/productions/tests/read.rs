//! What each adapter reads, over the adapters' own fixtures in place
//! (read only): one production per DAW.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

use productions::{Daw, find, read};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../adapters")
        .join(rel)
}

#[test]
fn an_ableton_set_gives_everything_but_a_save_time() {
    let found = find(&fixture("ableton/tests/fixtures/ableton/LucidDreaming.als"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].daw, Daw::Ableton);
    let facts = read(&found[0]);
    assert_eq!(facts.error, None);
    assert!(facts.tempo.is_some_and(|t| t > 0.0));
    assert!(facts.length.is_some_and(|l| l > 0.0));
    assert!(facts.tracks.is_some_and(|t| t > 0));
    assert!(facts.plugins.is_some_and(|p| !p.is_empty()));
    // Its samples are not beside the fixture.
    assert!(facts.missing_media.is_some_and(|m| !m.is_empty()));
    assert!(found[0].saved.is_some());
}

#[test]
fn a_pro_tools_session_gives_no_key() {
    let found = find(&fixture("protools/tests/fixtures/protools/RegionTest.ptx"));
    let facts = read(&found[0]);
    assert_eq!(facts.error, None);
    assert_eq!(found[0].daw, Daw::ProTools);
    assert!(facts.tempo.is_some_and(|t| t > 0.0));
    assert!(facts.tracks.is_some_and(|t| t > 0));
    assert!(facts.length.is_some_and(|l| l > 0.0));
    assert!(facts.plugins.is_some() && facts.missing_media.is_some());
    assert_eq!(facts.key, None);
    assert_eq!(facts.songs, []);
}

#[test]
fn a_reaper_project_gives_tempo_and_tracks() {
    let found = find(&fixture(
        "reaper/dawfile-reaper/resources/Template-with-takes-lanes.RPP",
    ));
    assert_eq!(found.len(), 1, "an .RPP in capitals is a REAPER project");
    assert_eq!(found[0].daw, Daw::Reaper);
    let facts = read(&found[0]);
    assert_eq!(facts.error, None);
    assert!(facts.tempo.is_some_and(|t| t > 0.0), "{facts:?}");
    assert!(facts.tracks.is_some_and(|t| t > 0), "{facts:?}");
    assert!(facts.plugins.is_some() && facts.missing_media.is_some());
    assert_eq!(facts.key, None);
}

#[test]
fn a_logic_project_gives_tempo_key_and_tracks_only() {
    let found = find(&fixture("logic/tests/fixtures/logic/FileDecrypt.logicx"));
    assert_eq!(found.len(), 1, "the bundle is one project, not walked into");
    assert_eq!(found[0].daw, Daw::Logic);
    assert_eq!(found[0].name, "FileDecrypt");
    let facts = read(&found[0]);
    assert_eq!(facts.error, None);
    assert!(facts.tempo.is_some_and(|t| t > 0.0), "{facts:?}");
    assert!(facts.key.is_some(), "{facts:?}");
    assert!(facts.tracks.is_some(), "{facts:?}");
    assert_eq!(facts.length, None);
    assert_eq!(facts.plugins, None);
    assert_eq!(facts.missing_media, None);
}

#[test]
fn a_file_that_does_not_read_is_still_a_production() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("Broken.als");
    std::fs::write(&file, b"not a live set").unwrap();
    let found = find(tmp.path());
    assert_eq!(found.len(), 1);
    let facts = read(&found[0]);
    assert!(facts.error.is_some());
    assert_eq!(facts.tempo, None);
}
