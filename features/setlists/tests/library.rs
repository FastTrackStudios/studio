//! A song library on disk: a song brought in from its files with
//! everything of its own (an arrangement, its session, media, its
//! categories), engraved, and engraved again after an edit.
#![cfg(feature = "engrave")]

use std::path::Path;

use setlists::Library;
use setlists::chart::{SongFiles, import_song, reengrave};
use setlists::vault::Vault;

/// A traditional tune and text (public domain), with a timed lyric lane.
const OLD_HUNDREDTH: &str = "Old Hundredth - Traditional\n#G 72bpm 3/4\n\nCount 2\n\nVS 8\nG G C G\nG G D D\n[lyrics lrc]\n@1 Praise @1:3 God @2 from @2:3 whom @3 all @3:3 bless- @4 ings @4:3 flow\n@5 praise @5:2 him @6 all @6:3 crea- @7 tures @8 here @8:3 be- @8:3.5 low\n\nVS 8\nG G C G\nG G D G\n[lyrics lrc]\n@1 Praise @1:3 him @2 a- @2:3 bove @5 ye @6 heaven- @6:3 ly @7 host\n";

#[test]
fn a_song_keeps_everything_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let kf = src.path().join("hundredth.kf");
    std::fs::write(&kf, OLD_HUNDREDTH).unwrap();
    let acoustic = src.path().join("acoustic.kf");
    std::fs::write(&acoustic, OLD_HUNDREDTH.replace("#G 72bpm", "#D 66bpm")).unwrap();
    let session = src.path().join("hundredth.RPP");
    std::fs::write(&session, "<REAPER_PROJECT 0.1 \"7.0\"\n>\n").unwrap();
    let click = src.path().join("Click.wav");
    std::fs::write(&click, b"RIFF").unwrap();

    let vault = Vault::at(dir.path());
    let lib = Library::named("Hymns");
    let hymns = ["Doxology".to_owned()];
    let song = import_song(
        &vault,
        &lib,
        &SongFiles {
            chart: Some(&kf),
            session: Some(&session),
            media: &[&click],
            categories: &hymns,
            ..SongFiles::default()
        },
        None,
    )
    .unwrap();
    // The title, key, tempo and meter come from the chart's header.
    assert_eq!(song.title, "Old Hundredth");
    assert_eq!(song.artist.as_deref(), Some("Traditional"));
    assert_eq!(song.chart.key.as_deref(), Some("G"));
    assert_eq!(song.chart.tempo, Some(72.0));
    assert_eq!(song.chart.meter.as_deref(), Some("3/4"));
    assert_eq!(song.categories, ["Doxology"]);
    assert_eq!(song.media, ["Media/Click.wav"]);
    assert_ne!(song.chart.pages.len(), 0);

    // An arrangement: a chart of its own, in its own folder.
    let song = import_song(
        &vault,
        &lib,
        &SongFiles {
            chart: Some(&acoustic),
            arrangement: Some("Acoustic"),
            ..SongFiles::default()
        },
        Some("Old Hundredth"),
    )
    .unwrap();
    let arr = &song.arrangements[0];
    assert_eq!(arr.name, "Acoustic");
    assert_eq!(arr.chart.key.as_deref(), Some("D"));
    assert_eq!(
        arr.chart.source.as_deref(),
        Some("Arrangements/Acoustic/Old Hundredth (Acoustic).kf")
    );
    // The song's own chart and session are kept.
    assert_eq!(song.chart.key.as_deref(), Some("G"));
    assert_eq!(song.session.as_deref(), Some("Old Hundredth.RPP"));

    let folder = vault.path(&lib.song_dir("Old Hundredth"));
    let page = std::fs::read(folder.join(&song.chart.pages[0])).unwrap();
    assert!(page.starts_with(b"\x89PNG"));
    assert!(
        std::fs::read(folder.join("Old Hundredth - Master Rhythm.pdf"))
            .unwrap()
            .starts_with(b"%PDF")
    );
    assert!(Path::new(&folder.join(&arr.chart.pages[0])).exists());
    assert!(folder.join("Media/Click.wav").exists());

    let note = vault.read(&lib.song_note("Old Hundredth")).unwrap();
    assert!(note.contains("### Verse 1\n\nPraise God from whom all blessings flow\\\npraise him all creatures here below"), "{note}");
    assert!(
        note.contains("## Arrangements\n\n### Acoustic\n\n**D · 66 bpm · 3/4**"),
        "{note}"
    );
    assert!(
        vault
            .read(&lib.note())
            .unwrap()
            .contains("groupBy: categories")
    );

    // An edit to the chart, engraved again (the arrangement's too).
    let kf_in = lib.song_file("Old Hundredth", "Old Hundredth.kf");
    vault
        .write(&kf_in, OLD_HUNDREDTH.replace("72bpm", "80bpm").as_bytes())
        .unwrap();
    let again = reengrave(&vault, &lib, "Old Hundredth").unwrap();
    assert_eq!(again.chart.tempo, Some(80.0));
    assert_eq!(again.arrangements[0].chart.tempo, Some(66.0));
}
