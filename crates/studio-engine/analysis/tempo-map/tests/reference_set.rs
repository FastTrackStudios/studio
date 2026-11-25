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

/// This Christmas (Donny Hathaway): the owner's chart alternates a 4/4
/// bar with a 3/4 bar in every hook, the intro and the outro. With those
/// meter changes the bar lines land on the kick; read as plain 4/4 they
/// do not. The one real recording with meter changes in the set.
#[test]
fn this_christmas_s_bar_lines_follow_its_meter_changes() {
    use tempo_map::analysis::{self, Options};
    use tempo_map::features;
    use tempo_map::meter::{Meter, MeterChange};
    use tempo_map::track::TrackParams;

    let Some(stems) = alt()
        .map(|a| a.join("this-christmas"))
        .filter(|p| p.is_dir())
    else {
        eprintln!("skipped: no stems of This Christmas on this machine");
        return;
    };
    // The song bars in 3/4, from the chart: each is followed by 4/4 again.
    let threes = [2_u32, 4, 24, 44, 64, 84, 86, 88];
    let changes: Vec<MeterChange> = threes
        .iter()
        .flat_map(|bar| {
            [
                MeterChange {
                    bar: *bar,
                    meter: Meter::new(3, 4),
                },
                MeterChange {
                    bar: bar + 1,
                    meter: Meter::COMMON,
                },
            ]
        })
        .collect();
    let measured = features::extract(&stems).unwrap();
    let analyse = |changes: Vec<MeterChange>| {
        let mut found = analysis::analyse(
            &measured,
            &Options {
                track: TrackParams {
                    bpm: 93.0,
                    ..TrackParams::default()
                },
                meter: Some(Meter::COMMON),
                changes,
                no_repair: false,
            },
        );
        // Bar 1 comes after the two counted beats (the chart's `Count 2`).
        let bar1 = found.beat_near(1.148).unwrap();
        found.set_bar1(bar1).unwrap();
        found
    };
    // The bars of the chart (90) whose downbeat has a kick within 45 ms.
    let on_the_kick = |found: &analysis::Analysis| {
        let kicks: Vec<f64> = found.hits["Kick"]
            .iter()
            .filter(|h| h.strength > 0.5)
            .map(|h| h.time)
            .collect();
        found
            .map()
            .bars
            .iter()
            .take(90)
            .filter(|bar| kicks.iter().any(|k| (k - bar.time).abs() < 0.045))
            .count()
    };
    let charted = analyse(changes);
    let plain = analyse(Vec::new());
    let (with, without) = (on_the_kick(&charted), on_the_kick(&plain));
    eprintln!("kick on the downbeat: {with} of 90 bars with the chart's meters, {without} as 4/4");
    assert!(with >= 58, "{with}");
    assert!(with >= without + 12, "{with} against {without}");
    let map = charted.map();
    assert_eq!(map.bars[1].meter, Meter::new(3, 4));
    assert_eq!(map.bars[2].meter, Meter::COMMON);
    // Sixteen meter changes are sixteen points that set a meter, with the
    // count-in's.
    assert_eq!(
        map.points().iter().filter(|p| p.meter.is_some()).count(),
        17
    );
    // No beat added or dropped by the tracker along the way.
    assert!(
        charted.repair.fixed.is_empty(),
        "{:?}",
        charted.repair.fixed
    );
}
