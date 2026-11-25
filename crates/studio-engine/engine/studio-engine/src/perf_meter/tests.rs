use std::collections::HashMap;

use daw::standalone::audio_engine::perf::TrackCost;

use super::{TrackInfo, cpu_seconds, field, rows};

#[test]
fn a_tracks_row_is_its_share_of_a_core_over_the_interval_with_its_fx_and_their_pdc() {
    let tracks = vec![
        TrackInfo {
            guid: "kick".into(),
            name: "Kick In".into(),
            fx: vec!["comp".into(), "eq".into()],
        },
        TrackInfo {
            guid: "bass".into(),
            name: "Bass".into(),
            fx: Vec::new(),
        },
    ];
    let before = vec![
        TrackCost {
            media_us: 1_000,
            fx_us: 2_000,
        },
        TrackCost {
            media_us: 0,
            fx_us: 0,
        },
    ];
    // Half a second: 250 ms of media on the kick is half a core.
    let now = vec![
        TrackCost {
            media_us: 251_000,
            fx_us: 52_000,
        },
        TrackCost {
            media_us: 5_000,
            fx_us: 0,
        },
    ];
    let pdc = HashMap::from([("comp".to_owned(), 64), ("eq".to_owned(), 0)]);
    let r = rows(&before, &now, 500_000.0, &tracks, &pdc);
    assert_eq!(r.len(), 2);
    assert_eq!(
        (r[0].name.as_str(), r[0].fx_count, r[0].pdc_samples),
        ("Kick In", 2, 64)
    );
    assert!(
        (r[0].media_cpu_pct - 50.0).abs() < 1e-9,
        "{}",
        r[0].media_cpu_pct
    );
    assert!((r[0].fx_cpu_pct - 10.0).abs() < 1e-9, "{}", r[0].fx_cpu_pct);
    assert!((r[1].media_cpu_pct - 1.0).abs() < 1e-9);
    assert_eq!((r[1].fx_count, r[1].pdc_samples), (0, 0));
}

#[test]
fn the_process_cpu_and_the_kernels_fields_are_read_as_they_are_written() {
    // utime 1234 and stime 66 ticks; a name with a space and a parenthesis.
    let stat = "4242 (studio (x) eng) S 1 2 3 4 5 6 7 8 9 10 1234 66 0 0 20 0 30";
    assert_eq!(cpu_seconds(stat), Some(13.0));
    let meminfo = "MemTotal:       195000000 kB\nMemAvailable:   97000000 kB\n";
    assert_eq!(field(meminfo, "MemAvailable"), Some(97_000_000 * 1024));
    let io = "rchar: 10\nread_bytes: 4096\nwrite_bytes: 512\n";
    assert_eq!(field(io, "read_bytes"), Some(4096));
    assert_eq!(field(io, "write_bytes"), Some(512));
    assert_eq!(field(io, "cancelled_write_bytes"), None);
}
