use super::{Observation, ReadySeen, SongSeen, SongTiming, StreamSeen, Timings, timings};

fn song(number: u32, title: &str, project: &str, current: bool, state: &str) -> SongSeen {
    SongSeen {
        number,
        title: title.to_owned(),
        project: project.to_owned(),
        current,
        state: state.to_owned(),
    }
}

fn stream(project: &str, state: &str) -> StreamSeen {
    StreamSeen {
        project: project.to_owned(),
        state: state.to_owned(),
        readiness: None,
    }
}

fn playable(project: &str, state: &str, ready: bool) -> StreamSeen {
    StreamSeen {
        readiness: Some(ReadySeen { ready }),
        ..stream(project, state)
    }
}

fn seen(elapsed_ms: u64, songs: Vec<SongSeen>, streams: Option<Vec<StreamSeen>>) -> Observation {
    Observation {
        elapsed_ms,
        songs,
        streams,
    }
}

/// What the window says, as the MCP `run` tool's text: parsed the way a
/// poll parses it.
fn songs_json(text: &str) -> Vec<SongSeen> {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

#[test]
fn a_set_that_streams_in_is_timed_to_its_first_song_every_song_open_and_every_song_local() {
    let a = song(1, "Praise", "p-a", true, "open");
    let b_arriving = song(2, "Holy Forever", "", false, "arriving");
    let b = song(2, "Holy Forever", "p-b", false, "open");
    let runs = [
        seen(40, vec![], Some(vec![])),
        seen(
            120,
            vec![a.clone(), b_arriving],
            Some(vec![stream("p-a", "arriving")]),
        ),
        seen(
            300,
            vec![a.clone(), b.clone()],
            Some(vec![stream("p-a", "arriving"), stream("p-b", "streaming")]),
        ),
        seen(
            900,
            vec![a.clone(), b.clone()],
            Some(vec![stream("p-a", "local"), stream("p-b", "arriving")]),
        ),
        seen(
            1500,
            vec![a, b],
            Some(vec![stream("p-a", "local"), stream("p-b", "local")]),
        ),
    ];

    let t = timings(Vec::new(), &runs);

    assert_eq!(
        (t.first_song_ms, t.all_open_ms, t.all_local_ms, t.songs),
        (Some(120), Some(300), Some(1500), 2)
    );
    assert_eq!(
        t.per_song(),
        [
            SongTiming {
                number: 1,
                title: "Praise".to_owned(),
                open_ms: 120
            },
            SongTiming {
                number: 2,
                title: "Holy Forever".to_owned(),
                open_ms: 300
            },
        ]
    );
    assert!(t.done());
}

#[test]
fn a_set_already_on_disk_is_done_once_every_song_is_open() {
    let runs = [seen(
        80,
        songs_json(
            r#"[{"number":1,"title":"Praise","project":"p-a","current":true,"state":"open"}]"#,
        ),
        // Streams of an earlier set are not this one's.
        Some(vec![stream("p-old", "arriving")]),
    )];

    let t = timings(Vec::new(), &runs);

    assert_eq!(
        (t.first_song_ms, t.all_open_ms, t.all_local_ms, t.done()),
        (Some(80), Some(80), None, true)
    );
}

#[test]
fn polls_that_still_show_the_set_open_before_the_request_are_not_counted() {
    let old = vec![song(1, "Old Song", "p-old", true, "open")];
    let new = song(1, "Praise", "p-a", true, "open");
    let runs = [
        seen(50, old.clone(), None),
        seen(100, old.clone(), None),
        seen(150, vec![], None),
        seen(400, vec![new], None),
    ];

    let t = timings(old, &runs);

    assert_eq!((t.first_song_ms, t.all_open_ms), (Some(400), Some(400)));
}

#[test]
fn a_set_still_arriving_or_not_yet_local_is_not_done() {
    let a = song(1, "Praise", "p-a", true, "open");
    let arriving = [seen(
        200,
        vec![a.clone(), song(2, "Holy Forever", "", false, "arriving")],
        None,
    )];
    let t = timings(Vec::new(), &arriving);
    assert_eq!(
        (t.first_song_ms, t.all_open_ms, t.done()),
        (Some(200), None, false)
    );

    let streaming = [seen(200, vec![a], Some(vec![stream("p-a", "arriving")]))];
    let t = timings(Vec::new(), &streaming);
    assert_eq!(
        (t.all_open_ms, t.all_local_ms, t.done()),
        (Some(200), None, false)
    );
}

#[test]
fn an_open_song_that_is_not_current_does_not_count_as_the_first_song() {
    let mut t = Timings::after(Vec::new());
    t.observe(&seen(
        100,
        vec![
            song(1, "Praise", "", false, "arriving"),
            song(2, "Holy Forever", "p-b", false, "open"),
        ],
        None,
    ));
    assert_eq!(t.first_song_ms, None);
}

#[test]
fn a_set_is_playable_once_every_open_song_can_play_from_its_playhead_before_it_is_local() {
    let a = song(1, "Praise", "p-a", true, "open");
    let b = song(2, "Holy Forever", "p-b", false, "open");
    let runs = [
        // The second song can play (by its reference), but has not opened.
        seen(
            100,
            vec![a.clone(), song(2, "Holy Forever", "", false, "arriving")],
            Some(vec![playable("p-a", "arriving", true)]),
        ),
        seen(
            200,
            vec![a.clone(), b.clone()],
            Some(vec![
                playable("p-a", "arriving", true),
                playable("p-b", "streaming", false),
            ]),
        ),
        seen(
            300,
            vec![a.clone(), b.clone()],
            Some(vec![
                playable("p-a", "arriving", true),
                playable("p-b", "arriving", true),
            ]),
        ),
        seen(
            900,
            vec![a, b],
            Some(vec![
                playable("p-a", "local", true),
                playable("p-b", "local", true),
            ]),
        ),
    ];

    let t = timings(Vec::new(), &runs);

    assert_eq!(
        (t.all_open_ms, t.all_playable_ms, t.all_local_ms),
        (Some(200), Some(300), Some(900))
    );
}
