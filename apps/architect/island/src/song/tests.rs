//! The song view's contract: what it reads of a song note as Studio
//! writes one, and what it asks of the playhead.

use serde_json::{Value, json};

use super::*;
use crate::queue::{PLAYHEAD_ITEM, PLAYHEAD_SET, SOURCE};

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
    let set = || source_of(&s, &[], Some((120.0, 4)), 12.0);
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
