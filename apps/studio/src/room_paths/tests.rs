use super::*;

#[test]
fn a_path_under_a_folder_takes_out_dots() {
    assert_eq!(
        under("Studio", "Setlists/../Songs/a"),
        "Studio/Songs/a",
        "a Drive location's path"
    );
    assert_eq!(
        under("/sessions/studio", "Rooms/./x.room"),
        "/sessions/studio/Rooms/x.room",
        "a path on the page's own site keeps its root"
    );
    assert_eq!(
        under(
            "http://127.0.0.1:4000/sessions/studio/",
            "Setlists/../Songs/a"
        ),
        "http://127.0.0.1:4000/sessions/studio/Songs/a",
        "a URL keeps its scheme and host"
    );
    assert_eq!(
        under("https://example.app", "a/b"),
        "https://example.app/a/b"
    );
    assert_eq!(under("Studio", "../../a"), "a", "no climbing past the top");
}

#[test]
fn a_room_file_says_its_studio_folder() {
    assert_eq!(
        studio_of("Drive/Studio/Rooms/adult-jam-3f9a1c.room").as_deref(),
        Some("Drive/Studio")
    );
    assert_eq!(studio_of("Rooms/adult-jam.room").as_deref(), Some(""));
    assert_eq!(studio_of("Studio/Setlists/adult-jam.room"), None);
    assert_eq!(
        room_file("/sessions/studio", "adult-jam-3f9a1c"),
        "/sessions/studio/Rooms/adult-jam-3f9a1c.room"
    );
}

#[test]
fn a_song_is_its_folder() {
    assert_eq!(song_folder("Songs/Hymn"), "Songs/Hymn");
    assert_eq!(song_folder("Songs/Hymn/"), "Songs/Hymn");
    assert_eq!(song_folder("Songs/Hymn/Hymn.RPP"), "Songs/Hymn");
    assert_eq!(song_folder("Songs/Hymn/Hymn.session"), "Songs/Hymn");
}

#[test]
fn a_setlists_lines_are_relative_to_it() {
    let setlist = "/sessions/studio/Setlists/first.setlist";
    assert_eq!(
        song_of_line(setlist, "../sessions/one").as_deref(),
        Some("/sessions/studio/sessions/one")
    );
    assert_eq!(
        song_of_line(setlist, "../Songs/Hymn @Live").as_deref(),
        Some("/sessions/studio/Songs/Hymn"),
        "the arrangement is not part of the path"
    );
    assert_eq!(
        song_of_line(setlist, "../Songs/Hymn/Hymn.RPP").as_deref(),
        Some("/sessions/studio/Songs/Hymn")
    );
    assert_eq!(song_of_line(setlist, "= Amazing Grace"), None);
    assert_eq!(song_of_line(setlist, "  "), None);
}
