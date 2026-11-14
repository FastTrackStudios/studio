//! The song view's contract: what it reads of a song note as Studio
//! writes one, and what it asks of the playhead.

use serde_json::{Value, json};

use super::*;
use crate::queue::tests::host;
use crate::queue::{LENGTH_MAX, MESSAGE_MAX, PLAYHEAD_ITEM, PLAYHEAD_SET, SOURCE};

/// A song note's body as `setlists::song` writes it: its recording
/// embedded, its chart as a `kf` fence that names it, its LRC embedded.
const BODY: &str = "# Joy to the World\n\n*Isaac Watts*\n\n\
    ![[Libraries/Carols/Joy/Media/joy.ogg]]\n\n\
    ```kf media=Media/joy.ogg\nJoy to the World - Isaac Watts\n#D 120bpm 4/4\n\nVS 4\nD D A D\n```\n\n\
    ## Lyrics\n\n![[Libraries/Carols/Joy/Joy.lrc]]\n";

fn note(files: &Value) -> Value {
    json!({"title": "Joy to the World", "type": "song", "properties": {}, "body": BODY, "files": files})
}

fn files() -> Value {
    json!([
        {"path": "Libraries/Carols/Joy/Joy.lrc", "kind": "lyrics", "size": 40, "text": "[ar:Watts]\n[00:01.50]Joy to the world\n[00:04.00][00:20.00]The Lord is come\n"},
        {"path": "Libraries/Carols/Joy/cover.png", "kind": "image", "size": 9, "data": "data:image/png;base64,AAAA"},
        {"path": "Libraries/Carols/Joy/Joy.chart-1.png", "kind": "image", "size": 9, "data": "data:image/png;base64,BBBB"},
        {"path": "Libraries/Carols/Hark/Hark.lrc", "kind": "lyrics", "size": 20, "text": "[00:01.00]Hark"},
    ])
}

fn joy() -> Song {
    song_of_note(&note(&files())).unwrap()
}

#[test]
fn a_song_note_gives_its_chart_recording_lines_and_cover() {
    let s = joy();
    assert_eq!(s.title, "Joy to the World");
    assert!(
        s.chart
            .as_deref()
            .unwrap()
            .starts_with("Joy to the World - Isaac Watts\n#D 120bpm")
    );
    assert_eq!(
        s.recording.as_deref(),
        Some("Libraries/Carols/Joy/Media/joy.ogg")
    );
    // Its own LRC (the one its body embeds), never another song's.
    assert_eq!(
        s.lines,
        vec![
            (1.5, "Joy to the world".to_owned()),
            (4.0, "The Lord is come".to_owned()),
            (20.0, "The Lord is come".to_owned()),
        ]
    );
    // Its cover, never its chart's engraved page.
    assert_eq!(s.art.as_deref(), Some("Libraries/Carols/Joy/cover.png"));
    // No note: nothing to draw.
    assert_eq!(song_of_note(&Value::Null), None);
}

#[test]
fn without_handed_files_the_lines_come_from_its_lrc_fence() {
    let body = "```kf\nx\n```\n\n## Lyrics\n\n````lrc\n[00:02.00]Silent night\n[00:05.00]Holy <00:05.50>night\n````\n";
    let s = song_of_note(&json!({"title": "Silent Night", "body": body})).unwrap();
    assert_eq!(s.chart.as_deref(), Some("x\n"));
    assert_eq!(s.recording, None, "nothing that plays is embedded");
    assert_eq!(
        s.lines,
        vec![
            (2.0, "Silent night".to_owned()),
            (5.0, "Holy night".to_owned())
        ]
    );
}

#[test]
fn fences_close_only_on_their_own_length() {
    let f = fences("````kf media=a\n```\nin\n````\nafter\n```lrc\n[00:01]x\n```\n");
    assert_eq!(f[0], ("kf media=a".to_owned(), "```\nin\n".to_owned()));
    assert_eq!(f[1].0, "lrc");
}

#[test]
fn the_playhead_is_followed_only_on_this_song_of_this_source() {
    let s = joy();
    let rec = "Libraries/Carols/Joy/Media/joy.ogg";
    let on = json!({"source": SOURCE, "item": {"id": "S|Joy.md", "media": rec}, "time": 3.0, "at": 1000.0, "playing": true, "rate": 1.0, "duration": 150.0});
    assert!(following(&on, &s));
    assert!(!following(
        &json!({"source": "app:share", "item": {"media": rec}}),
        &s
    ));
    assert!(!following(
        &json!({"source": SOURCE, "item": {"media": "other.ogg"}}),
        &s
    ));
    // Extrapolated while it plays, held while paused, never past its end.
    assert!((position(&on, 3_000.0) - 5.0).abs() < 1e-9);
    let paused = json!({"time": 3.0, "at": 1000.0, "playing": false});
    assert!((position(&paused, 9_000.0) - 3.0).abs() < 1e-9);
    assert!((position(&on, 1_000_000.0) - 150.0).abs() < 1e-9);
}

#[test]
fn a_bar_is_the_last_started() {
    let starts = [0.0, 2.0, 4.0, 6.0];
    assert_eq!(at_or_before(&starts, -1.0), None);
    assert_eq!(at_or_before(&starts, 0.0), Some(0));
    assert_eq!(at_or_before(&starts, 3.9), Some(1));
    assert_eq!(at_or_before(&starts, 99.0), Some(3));
}

#[test]
fn a_press_seeks_plays_the_queued_song_or_queues_it_alone() {
    let s = joy();
    let rec = "Libraries/Carols/Joy/Media/joy.ogg";
    let set = || source_of(&s, &[], beat_of(120.0, "4/4", 0.0), 12.0);
    // Current: a seek.
    let on = json!({"source": SOURCE, "item": {"id": "S|Joy.md", "media": rec}, "duration": 150.0});
    let (asks, pending) = seek(&on, &s, 12.0, 0.0, set);
    assert_eq!(asks, vec![(PLAYHEAD_SEEK, json!({"t": 12.0}))]);
    assert_eq!(pending, None);
    // Queued by a setlist's view, not current: play it, seek once it is.
    let queued = json!({"source": SOURCE, "item": {"id": "S|Hark.md", "media": "h.ogg"},
        "queue": [{"id": "S|Hark.md", "media": "h.ogg"}, {"id": "S|Joy.md", "media": rec}]});
    let (asks, pending) = seek(&queued, &s, 12.0, 100.0, set);
    assert_eq!(
        asks,
        vec![(PLAYHEAD_ITEM, json!({"id": "S|Joy.md", "play": true}))]
    );
    let pending = pending.unwrap();
    assert_eq!(due(&queued, &s, pending, 200.0), Due::Wait);
    let now_current =
        json!({"source": SOURCE, "item": {"id": "S|Joy.md", "media": rec}, "duration": 150.0});
    assert_eq!(
        due(&now_current, &s, pending, 300.0),
        Due::Now((PLAYHEAD_SEEK, json!({"t": 12.0})))
    );
    assert_eq!(
        due(&queued, &s, pending, 100.0 + PENDING_MS + 1.0),
        Due::Dropped
    );
    // Nowhere in the source: the song alone, at the bar, playing.
    let (asks, _) = seek(&json!({"source": "app:share"}), &s, 12.0, 0.0, set);
    let (action, args) = &asks[0];
    assert_eq!(*action, PLAYHEAD_SET);
    assert_eq!(args["time"], json!(12.0));
    assert_eq!(args["play"], json!(true));
    let item = &args["queue"][0];
    assert_eq!(item["id"], json!(song_id(rec)));
    assert_eq!(item["media"], json!(rec));
    assert_eq!(item["art"], json!("Libraries/Carols/Joy/cover.png"));
    assert_eq!(
        item["marks"][0],
        json!({"at": 1.5, "text": "Joy to the world", "kind": "line"})
    );
    assert_eq!(item["tempo"], json!({"bpm": 120.0, "at": 0.0, "meter": 4}));
    // No recording: nothing to move.
    let silent = Song {
        recording: None,
        ..s
    };
    assert_eq!(seek(&on, &silent, 1.0, 0.0, || None), (Vec::new(), None));
}

#[test]
fn colours_are_hex_or_left_out() {
    assert_eq!(hex_color("#2563EB").as_deref(), Some("#2563eb"));
    assert_eq!(hex_color("rgb(37, 99, 235)").as_deref(), Some("#2563eb"));
    assert_eq!(hex_color("red"), None);
}

/// What the song view sets is a `SET` the host takes whole.
fn taken(args: &Value) -> Vec<String> {
    host::takes(PLAYHEAD_SET, args, SOURCE)
        .expect("the message fits")
        .unwrap_or_else(|e| panic!("the host refuses it: {e}"))
}

/// The bar's Click counts from the chart's `\downbeat`, where its bars
/// are timed from (review M1): Silent Night's is half a second in.
#[test]
fn the_click_starts_at_the_charts_downbeat() {
    let chart = "Silent Night - Franz Gruber\n72bpm 3/4 #A\n\\downbeat = 0.5\n\nVS 12\nA A A A\nE E A A\nD D A A\n";
    let b = setlists::follow::chart_beat(chart).unwrap();
    let beat = beat_of(b.bpm, &b.meter, b.downbeat).unwrap();
    assert_eq!(
        beat,
        Beat {
            bpm: 72.0,
            meter: 3,
            at: 0.5
        }
    );
    let args = source_of(&joy(), &[], Some(beat), 0.0).unwrap();
    assert_eq!(args["queue"][0]["tempo"]["at"], json!(b.downbeat));
    assert_eq!(
        args["queue"][0]["tempo"],
        json!({"bpm": 72.0, "at": 0.5, "meter": 3})
    );
    taken(&args);
    // A downbeat past a day is the host's last second, never a refusal.
    let far = Beat { at: 1e9, ..beat };
    let args = source_of(&joy(), &[], Some(far), 0.0).unwrap();
    assert_eq!(args["queue"][0]["tempo"]["at"], json!(LENGTH_MAX));
    taken(&args);
    // 6/8 counts its dotted beats; a tempo the host refuses is left out.
    assert_eq!(
        beat_of(60.0, "6/8", 1.0),
        Some(Beat {
            bpm: 60.0,
            meter: 2,
            at: 1.0
        })
    );
    assert_eq!(beat_of(9000.0, "4/4", 0.0), None);
}

/// Times past a day are the host's last second (review M3): a section's
/// end, the time pressed, a seek.
#[test]
fn the_songs_set_keeps_its_times_within_a_day() {
    let s = joy();
    let sections = [
        Played {
            name: "VS".to_owned(),
            start: 0.0,
            end: 20.0,
            color: Some("#2563eb".to_owned()),
        },
        Played {
            name: "Long".to_owned(),
            start: 20.0,
            end: 1e9,
            color: None,
        },
        Played {
            name: "Past".to_owned(),
            start: 1e9,
            end: 2e9,
            color: None,
        },
    ];
    let args = source_of(&s, &sections, None, 1e12).unwrap();
    assert_eq!(args["time"], json!(LENGTH_MAX));
    let played = args["queue"][0]["sections"].as_array().unwrap();
    assert_eq!(played.len(), 2, "a section starting past a day is left out");
    assert_eq!(played[1]["end"], json!(LENGTH_MAX));
    taken(&args);
    let on = json!({"source": SOURCE, "item": {"id": song_id("x"), "media": s.recording}});
    let (asks, _) = seek(&on, &s, 1e12, 0.0, || None);
    assert_eq!(asks, vec![(PLAYHEAD_SEEK, json!({"t": LENGTH_MAX}))]);
}

/// A song with every mark the host allows, each as long as it allows,
/// is cut to one message the host reads (review M3): its last marks go
/// first, then its last sections.
#[test]
fn the_songs_set_is_cut_to_one_message() {
    let words = "w".repeat(300);
    let s = Song {
        lines: (0..2000).map(|n| (f64::from(n), words.clone())).collect(),
        ..joy()
    };
    let sections: Vec<Played> = (0..200)
        .map(|n| Played {
            name: "x".repeat(40),
            start: f64::from(n),
            end: f64::from(n) + 1.0,
            color: Some("#2563eb".to_owned()),
        })
        .collect();
    let args = source_of(&s, &sections, beat_of(120.0, "4/4", 0.0), 3.0).unwrap();
    let size = crate::queue::message(PLAYHEAD_SET, &args).len();
    assert!(size <= MESSAGE_MAX, "{size}");
    let marks = args["queue"][0]["marks"].as_array().unwrap();
    assert!(!marks.is_empty() && marks.len() < 2000, "{}", marks.len());
    assert_eq!(marks[0]["at"], json!(0.0), "the first marks are kept");
    assert_eq!(
        args["queue"][0]["sections"].as_array().map(Vec::len),
        Some(200),
        "the sections fit once the marks are cut"
    );
    assert_eq!(taken(&args), vec![song_id(s.recording.as_deref().unwrap())]);
    // Whatever fits: every case above, the host takes.
    let small = source_of(&joy(), &sections, None, 0.0).unwrap();
    taken(&small);
}
