//! An organization's rooms, kept in its Rooms folder: made once, found
//! again by their link, which never changes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails loudly"
)]

use room::{Live, Rooms};

#[test]
fn a_room_made_once_is_found_again_by_its_link() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let made = rooms.create("Adult Jam", "rockstars").unwrap();
    assert!(
        made.link()
            .starts_with("https://fasttrackstudio.app/join/adult-jam-"),
        "{}",
        made.link()
    );
    assert_eq!(made.name, "Adult Jam");
    assert_eq!(made.org, "rockstars");

    // Another device reading the same folder, a week later.
    let found = Rooms::at(dir.path()).find(&made.link()).unwrap();
    assert_eq!(found, made);
    // The bare id and the link without its scheme are the same room.
    assert_eq!(rooms.find(made.id.as_str()).unwrap(), made);
    assert_eq!(
        rooms
            .find(&format!("fasttrackstudio.app/join/{}", made.id))
            .unwrap(),
        made
    );
}

#[test]
fn two_rooms_of_one_name_have_their_own_links() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let first = rooms.create("Adult Jam", "rockstars").unwrap();
    let second = rooms.create("Adult Jam", "rockstars").unwrap();
    assert_ne!(first.link(), second.link());
    let mut listed: Vec<_> = rooms.list().unwrap().into_iter().map(|r| r.id).collect();
    listed.sort();
    let mut made = vec![first.id, second.id];
    made.sort();
    assert_eq!(listed, made);
}

#[test]
fn going_live_and_ending_keep_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let made = rooms.create("Adult Jam", "rockstars").unwrap();
    let live = Live {
        leader: "Cody".to_owned(),
        endpoint: "abc".to_owned(),
        addrs: vec!["127.0.0.1:4433".to_owned()],
    };
    rooms.set_live(&made.id, Some(live.clone())).unwrap();
    let now = rooms.find(&made.link()).unwrap();
    assert_eq!(now.link(), made.link());
    assert_eq!(now.live, Some(live));
    rooms.set_live(&made.id, None).unwrap();
    assert_eq!(rooms.find(&made.link()).unwrap().live, None);
}

#[test]
fn a_link_to_no_room_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    assert!(
        rooms
            .find("https://fasttrackstudio.app/join/nobody-123456")
            .is_err()
    );
    assert!(rooms.find("https://example.com/somewhere").is_err());
    assert!(rooms.create("  ", "rockstars").is_err());
    assert!(rooms.create("Adult Jam", "").is_err());
}

#[test]
fn a_file_that_is_no_room_leaves_the_others_listed() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let made = rooms.create("Adult Jam", "rockstars").unwrap();
    std::fs::write(dir.path().join("Not A Room.room"), "{").unwrap();
    std::fs::write(dir.path().join("broken-123456.room"), "{").unwrap();
    let listed: Vec<_> = rooms.list().unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(listed, vec![made.id]);
}
