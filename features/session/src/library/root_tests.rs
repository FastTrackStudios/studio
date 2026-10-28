use files_proto::model::{FileRootInfo, RootFlavor};

use super::{session_root_for, without_track_number};

fn root(name: &str, path: Option<&str>) -> FileRootInfo {
    FileRootInfo {
        id: uuid::Uuid::from_u128(path.map_or(0, |p| p.len() as u128) + name.len() as u128),
        name: name.to_owned(),
        path: path.map(str::to_owned),
        flavor: RootFlavor::Media,
        created_at: chrono::DateTime::UNIX_EPOCH,
        project_version: None,
    }
}

/// Which of `roots` (by path, or name for a pathless one) the song plays from.
fn found(roots: &[FileRootInfo], slug: &str) -> Option<String> {
    session_root_for(roots, slug).map(|r| r.path.clone().unwrap_or_else(|| r.name.clone()))
}

#[test]
fn a_song_plays_from_its_session_root_before_any_root_named_for_it() {
    let roots = [
        root("Berenstein", Some("/data/orgs/fta/projects/berenstein")),
        root(
            "Berenstein",
            Some("/mnt/storage/Task/fta/session/berenstein"),
        ),
        root(
            "Berenstein (old)",
            Some("/mnt/storage/Task/fta/xsession/berenstein"),
        ),
    ];
    assert_eq!(
        found(&roots, "berenstein").as_deref(),
        Some("/mnt/storage/Task/fta/session/berenstein")
    );
}

#[test]
fn an_album_uploaded_before_session_folders_is_found_by_its_song_folders() {
    // How Crescendum and PNG went up: one root per song folder of the
    // album, named as the folder was.
    let roots = [
        root(
            "Crescendum",
            Some("/mnt/storage/Task/fta/Projects/Crescendum"),
        ),
        root(
            "Chained expectations",
            Some("/mnt/storage/Task/fta/Projects/Crescendum/Chained expectations"),
        ),
        root(
            "Kornesque ",
            Some("/mnt/storage/Task/fta/Projects/Crescendum/Kornesque "),
        ),
        root(
            "05 LIFE GIVING WORD",
            Some("/mnt/storage/Task/tbm/Projects/PNG/05 LIFE GIVING WORD"),
        ),
        root(
            "Unbreakable",
            Some("/mnt/storage/Task/fta/Projects/Crescendum/unbreakable-v2"),
        ),
    ];
    let found = |slug| found(&roots, slug);
    assert_eq!(
        found("chained-expectations").as_deref(),
        Some("/mnt/storage/Task/fta/Projects/Crescendum/Chained expectations")
    );
    assert_eq!(
        found("kornesque").as_deref(),
        Some("/mnt/storage/Task/fta/Projects/Crescendum/Kornesque ")
    );
    assert_eq!(
        found("life-giving-word").as_deref(),
        Some("/mnt/storage/Task/tbm/Projects/PNG/05 LIFE GIVING WORD")
    );
    // The folder says otherwise; the root's own name is the song's.
    assert_eq!(
        found("unbreakable").as_deref(),
        Some("/mnt/storage/Task/fta/Projects/Crescendum/unbreakable-v2")
    );
    assert_eq!(found("crescendum-intro"), None);
}

#[test]
fn a_root_with_no_tree_on_the_server_is_no_song_s() {
    assert_eq!(found(&[root("Fire", None)], "fire"), None);
}

#[test]
fn a_track_number_is_dropped_only_when_something_follows_it() {
    assert_eq!(without_track_number("05 Empty Focus"), "Empty Focus");
    assert_eq!(without_track_number("05 - Empty Focus"), "Empty Focus");
    assert_eq!(
        without_track_number("11_Goodness of God"),
        "Goodness of God"
    );
    assert_eq!(without_track_number("1999"), "1999");
    assert_eq!(without_track_number("3am"), "3am");
    assert_eq!(without_track_number("2024 Tour"), "2024 Tour");
}
