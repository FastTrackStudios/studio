use super::targets_current;

#[test]
fn a_batch_goes_only_to_the_song_it_was_made_for() {
    assert!(targets_current("song-a", Some("song-a")));
    assert!(
        !targets_current("song-a", Some("song-b")),
        "the window moved on first"
    );
    assert!(!targets_current("song-a", None), "no song current here");
    assert!(
        targets_current("", Some("song-b")),
        "a batch that names no song"
    );
}
