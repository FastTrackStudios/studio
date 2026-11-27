//! What a live room shares: what its leader has loaded, as every replica
//! of the room's doc reads it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails loudly"
)]

use std::path::Path;

use room::{Kind, Loaded, RoomDoc};

/// `to` brought up to date with `from`, as the room's sync does.
fn sync(from: &RoomDoc, to: &RoomDoc) {
    let bytes = from
        .loro()
        .export(loro::ExportMode::updates(&to.loro().oplog_vv()))
        .unwrap();
    to.loro().import(&bytes).unwrap();
}

#[test]
fn a_member_reads_each_load_the_leader_makes() {
    let leader = RoomDoc::new();
    let member = RoomDoc::new();
    assert_eq!(member.loaded(), None);

    let root = Path::new("/home/a/Drive/Studio");
    let first = Loaded::of_path(&root.join("Setlists/Sunday.setlist"), Some(root));
    leader.load(&first).unwrap();
    sync(&leader, &member);
    let (got, seq) = member.loaded().unwrap();
    assert_eq!(got, first);
    assert_eq!(got.kind, Kind::Setlist);
    assert_eq!(got.title, "Sunday");

    let second = Loaded::of_path(&root.join("Songs/Washed/Washed.session"), Some(root));
    leader.load(&second).unwrap();
    sync(&leader, &member);
    let (got, next) = member.loaded().unwrap();
    assert_eq!(got, second);
    assert_eq!(got.kind, Kind::Session);
    assert!(next > seq, "each load is news: {seq} then {next}");

    // The same setlist loaded again is news too: the member opens it again.
    leader.load(&second).unwrap();
    sync(&leader, &member);
    assert!(member.loaded().unwrap().1 > next);
}

#[test]
fn a_loaded_path_means_the_same_file_on_every_device() {
    let leader_root = Path::new("/home/a/Drive/Studio");
    let member_root = Path::new("/Users/b/Drive/Studio");
    let loaded = Loaded::of_path(
        &leader_root.join("Setlists/Sunday.setlist"),
        Some(leader_root),
    );
    assert_eq!(loaded.path, "Setlists/Sunday.setlist");
    assert_eq!(
        loaded.resolve(Some(member_root)),
        member_root.join("Setlists/Sunday.setlist")
    );

    // Outside the Studio folder it is the path as it is.
    let elsewhere = Loaded::of_path(Path::new("/srv/songs/Washed.RPP"), Some(leader_root));
    assert_eq!(elsewhere.path, "/srv/songs/Washed.RPP");
    assert_eq!(
        elsewhere.resolve(Some(member_root)),
        Path::new("/srv/songs/Washed.RPP")
    );
}

#[test]
fn a_member_reads_where_the_leader_shares_each_load() {
    let leader = RoomDoc::new();
    let member = RoomDoc::new();
    assert_eq!(member.shared(), None);

    let share = room::Share {
        load: 1,
        ticket: "fts-session:abc/6a1f0c3e-8d2b-4f5a-9e7c-1b3d5f7a9c2e".to_owned(),
        addrs: vec![
            "192.168.1.20:41000".to_owned(),
            "127.0.0.1:41000".to_owned(),
        ],
    };
    leader.share(&share).unwrap();
    sync(&leader, &member);
    assert_eq!(member.shared(), Some(share));

    // The next load's share replaces it.
    let next = room::Share {
        load: 2,
        ticket: "fts-session:def/6a1f0c3e-8d2b-4f5a-9e7c-1b3d5f7a9c2f".to_owned(),
        addrs: Vec::new(),
    };
    leader.share(&next).unwrap();
    sync(&leader, &member);
    assert_eq!(member.shared(), Some(next));
}
