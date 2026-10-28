use super::{Lineup, PickRefused, SongPick, SongRow, position_named, resolve, rows};

/// Three songs open, the second current, one still arriving.
fn lineup() -> Lineup {
    Lineup {
        songs: vec![
            ("Unbreakable".into(), "p1".into()),
            ("Washed".into(), "p2".into()),
            ("Firm Foundation".into(), "p3".into()),
        ],
        at: 1,
        arriving: vec!["Same God".into()],
    }
}

#[test]
fn a_pick_lands_inside_the_set_and_never_on_the_song_already_current() {
    assert_eq!(SongPick::Next.index(0, 3), Some(1));
    assert_eq!(SongPick::Next.index(2, 3), None, "past the last");
    assert_eq!(SongPick::Previous.index(0, 3), None, "before the first");
    assert_eq!(SongPick::Previous.index(2, 3), Some(1));
    assert_eq!(SongPick::At(1).index(0, 3), Some(1));
    assert_eq!(SongPick::At(0).index(0, 3), None, "already there");
    assert_eq!(SongPick::At(7).index(0, 3), None);
}

#[test]
fn a_song_is_named_by_word_number_or_title() {
    let set = lineup();
    assert_eq!(resolve(&set, "next"), Ok(2));
    assert_eq!(resolve(&set, " Previous "), Ok(0));
    assert_eq!(resolve(&set, "prev"), Ok(0));
    assert_eq!(resolve(&set, "3"), Ok(2), "numbers count from 1");
    assert_eq!(resolve(&set, "unbreakable"), Ok(0), "titles ignore case");
}

#[test]
fn a_pick_that_lands_nowhere_says_why() {
    let set = lineup();
    assert_eq!(
        resolve(&set, "2"),
        Err(PickRefused::Already {
            current: "Washed".into()
        })
    );
    assert_eq!(
        resolve(&set, "Same God"),
        Err(PickRefused::Arriving("Same God".into()))
    );
    assert_eq!(
        resolve(&set, "Oceans"),
        Err(PickRefused::NoSuchSong("Oceans".into()))
    );
    assert_eq!(resolve(&set, "9"), Err(PickRefused::OutOfRange { len: 3 }));
    assert_eq!(
        resolve(&set, "0"),
        Err(PickRefused::NoSuchSong("0".into())),
        "there is no song 0"
    );
    let at_end = Lineup { at: 2, ..lineup() };
    assert_eq!(
        resolve(&at_end, "next"),
        Err(PickRefused::LastSoFar {
            current: "Firm Foundation".into()
        }),
        "the last song open, more arriving"
    );
    let all_here = Lineup {
        at: 2,
        arriving: Vec::new(),
        ..lineup()
    };
    assert_eq!(
        resolve(&all_here, "next").map_err(|e| e.to_string()),
        Err("Firm Foundation is the last song".to_owned())
    );
    let at_start = Lineup { at: 0, ..lineup() };
    assert_eq!(
        resolve(&at_start, "previous").map_err(|e| e.to_string()),
        Err("Unbreakable is the first song".to_owned())
    );
}

#[test]
fn the_rows_number_the_open_songs_then_the_arriving() {
    let row = |number, title: &str, project: &str, current, state: &str| SongRow {
        number,
        title: title.into(),
        project: project.into(),
        current,
        state: state.into(),
    };
    assert_eq!(
        rows(&lineup()),
        vec![
            row(1, "Unbreakable", "p1", false, "open"),
            row(2, "Washed", "p2", true, "open"),
            row(3, "Firm Foundation", "p3", false, "open"),
            row(4, "Same God", "", false, "arriving"),
        ]
    );
    assert_eq!(rows(&Lineup::default()), Vec::new());
}

#[test]
fn a_setlist_is_found_by_title_whatever_its_case_and_spacing() {
    let titles = ["Sunday AM", "JT's Senior Recital"];
    assert_eq!(position_named(&titles, " jt's senior recital "), Some(1));
    assert_eq!(position_named(&titles, "Sunday PM"), None);
}
