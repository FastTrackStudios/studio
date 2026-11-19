//! The recordings the method was worked out on, against the maps the
//! owner accepted for them. The audio is not in git: these read it from
//! where it lives and pass with a note when it is not there.
//!
//! - `TEMPO_MAP_SONGS`: the songs (a folder per song: stems and its
//!   `*Tempo Mapped*.RPP`).
//! - `TEMPO_MAP_ALT`: the same songs' stems from another separator.
//! - `TEMPO_MAP_BENCH=all`: every song, not only the quick one.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::panic,
    reason = "a test"
)]

use std::path::PathBuf;

use tempo_map::bench::{self, Row};

fn songs() -> Option<PathBuf> {
    let path = std::env::var_os("TEMPO_MAP_SONGS").map_or_else(
        || {
            PathBuf::from(
                "/run/media/Development/vaults/tombrooksmusic/Projects/Merry and Bright 2026/songs",
            )
        },
        PathBuf::from,
    );
    path.is_dir().then_some(path)
}

fn alt() -> Option<PathBuf> {
    let path = std::env::var_os("TEMPO_MAP_ALT").map_or_else(
        || PathBuf::from("/run/media/Development/tools/stem-sep/out2"),
        PathBuf::from,
    );
    path.is_dir().then_some(path)
}

fn row<'a>(rows: &'a [Row], song: &str, stems: &str, method: &str) -> Option<&'a Row> {
    rows.iter()
        .find(|r| r.song == song && r.stems == stems && r.method == method)
}

/// Last Christmas: the scripts' beats were accepted as they came, so the
/// port must give the accepted map back, from either stem set.
#[test]
fn last_christmas_is_the_accepted_map() {
    let Some(songs) = songs() else {
        eprintln!("skipped: the test set is not on this machine");
        return;
    };
    let one = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        songs.join("last-christmas"),
        one.path().join("last-christmas"),
    )
    .unwrap();
    let report = bench::run(one.path(), alt().as_deref(), None, |_| {});
    for method in ["port", "studio"] {
        let own = row(&report.rows, "last-christmas", "set", method).unwrap();
        assert_eq!((own.slips, own.differing), (0, 0), "{own:?}");
        assert!(own.max_ms < 1.0, "{own:?}");
        assert_eq!(own.bars, 117);
        if let Some(other) = row(&report.rows, "last-christmas", "alt", method) {
            assert_eq!((other.slips, other.differing), (0, 0), "{other:?}");
            assert!(other.median_ms < 2.0, "{other:?}");
        }
    }
    let own = row(&report.rows, "last-christmas", "set", "studio").unwrap();
    assert_eq!(own.bar1_guess, Some(0), "bar 1 is the best candidate");
    for agree in &report.agreement {
        assert_eq!((agree.slips, agree.differing), (0, 0), "{agree:?}");
    }
}

/// Every song: the numbers the branch was left at. A change that makes
/// one worse fails here; one that makes it better moves the line.
#[test]
fn the_whole_set_holds_its_numbers() {
    if std::env::var("TEMPO_MAP_BENCH").as_deref() != Ok("all") {
        eprintln!("skipped: TEMPO_MAP_BENCH=all runs every song");
        return;
    }
    let Some(songs) = songs() else {
        eprintln!("skipped: the test set is not on this machine");
        return;
    };
    let report = bench::run(&songs, alt().as_deref(), None, |song| eprintln!("{song}"));
    eprintln!("{}", report.markdown());
    // Beats added or dropped against the accepted map, per song and stem
    // set: only Go Tell It's ritard ending (placed by hand in the accepted
    // map) and the held-back intro of What Christmas Means to Me from the
    // other separator are left.
    let allowed = |song: &str, stems: &str| match (song, stems) {
        ("go-tell-it-aint-no-mountain", _) | ("what-christmas-means-to-me", "alt") => 1,
        _ => 0,
    };
    for r in report.rows.iter().filter(|r| r.method == "studio") {
        assert!(r.slips <= allowed(&r.song, &r.stems), "{r:?}");
        if allowed(&r.song, &r.stems) == 0 {
            assert!(r.median_ms < 2.0 && r.differing <= 6, "{r:?}");
        }
    }
}
