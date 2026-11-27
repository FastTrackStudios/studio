//! An organization's rooms, kept in its Rooms folder: made once, found
//! again by their link, which never changes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails loudly"
)]

use room::{Kind, Live, Loaded, Opens, Pass, Rooms};

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
        relays: vec!["https://relay.example/".to_owned()],
        pass: Pass::mint(),
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

#[test]
fn the_link_in_the_apps_scheme_is_the_same_room() {
    // What a phone is opened with: the room's link in Studio's own scheme.
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let made = rooms.create("Adult Jam", "rockstars").unwrap();
    let in_app = format!("{}{}", room::APP_JOIN_BASE, made.id);
    assert_eq!(in_app, format!("fasttrackstudio://join/{}", made.id));
    assert_eq!(room::parse_link(&in_app).unwrap(), made.id);
    assert_eq!(rooms.find(&format!("{in_app}/")).unwrap(), made);
    // Anything else in that scheme names no room.
    assert!(room::parse_link("fasttrackstudio://set/adult-jam").is_err());
    assert!(room::parse_link("fasttrackstudio://join/").is_err());
}

#[test]
fn a_pass_opens_only_with_itself() {
    let pass = Pass::mint();
    assert!(pass.matches(pass.expose()));
    assert!(!pass.matches(Pass::mint().expose()));
    assert!(!pass.matches(""));
    assert!(!pass.matches(pass.expose().get(1..).unwrap()));
    // No pass opens nothing, not even an empty one.
    assert!(!Pass::default().matches(""));
}

#[test]
fn a_pass_is_never_printed() {
    let pass = Pass::mint();
    let live = Live {
        leader: "Cody".to_owned(),
        endpoint: "abc".to_owned(),
        addrs: Vec::new(),
        relays: Vec::new(),
        pass: pass.clone(),
    };
    assert!(!format!("{live:?}").contains(pass.expose()));
}

#[test]
fn a_room_file_from_before_passes_still_reads() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let made = rooms.create("Adult Jam", "rockstars").unwrap();
    let old = r#"{"id":"ID","name":"Adult Jam","org":"rockstars","live":{"leader":"Cody","endpoint":"abc","addrs":[]}}"#
        .replace("ID", made.id.as_str());
    std::fs::write(dir.path().join(format!("{}.room", made.id)), old).unwrap();
    let found = rooms.find(made.id.as_str()).unwrap();
    assert_eq!(found.setlist, None);
    let live = found.live.unwrap();
    assert!(live.pass.is_empty());
    assert!(live.relays.is_empty());
}

fn live(leader: &str) -> Live {
    Live {
        leader: leader.to_owned(),
        endpoint: "abc".to_owned(),
        addrs: Vec::new(),
        relays: Vec::new(),
        pass: Pass::mint(),
    }
}

#[test]
fn a_room_nobody_leads_opens_its_setlist_to_practise() {
    // studio#714: the same link all week. Led, it follows the leader;
    // left, it opens what the leader last loaded, for each person alone.
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let made = rooms.create("Adult Jam", "rockstars").unwrap();
    assert_eq!(made.opens(), Opens::Nothing, "a new room has no setlist");

    let first = Loaded {
        kind: Kind::Setlist,
        path: "Setlists/Sunday.setlist".to_owned(),
        title: "Sunday".to_owned(),
    };
    let led = rooms.set_live(&made.id, Some(live("Cody"))).unwrap();
    rooms.set_setlist(&made.id, first.clone()).unwrap();
    let now = rooms.find(&made.link()).unwrap();
    assert_eq!(now.setlist.as_ref(), Some(&first));
    assert_eq!(now.opens(), Opens::Leader(led.live.as_ref().unwrap()));

    // The leader loads a lone session, then leaves: the link opens that.
    let lone = Loaded {
        kind: Kind::Session,
        path: "Songs/Amazing Grace".to_owned(),
        title: "Amazing Grace".to_owned(),
    };
    rooms.set_setlist(&made.id, lone.clone()).unwrap();
    rooms.set_live(&made.id, None).unwrap();
    // Another device reading the folder later in the week.
    let later = Rooms::at(dir.path()).find(&made.link()).unwrap();
    assert_eq!(later.live, None);
    assert_eq!(later.opens(), Opens::Practice(&lone));

    // Led again, it follows the leader again, its setlist kept.
    rooms.set_live(&made.id, Some(live("Alex"))).unwrap();
    let again = rooms.find(&made.link()).unwrap();
    assert!(matches!(again.opens(), Opens::Leader(l) if l.leader == "Alex"));
    assert_eq!(again.setlist, Some(lone));
}

#[test]
fn a_room_file_keeps_its_setlist_as_its_kind_and_path() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let made = rooms.create("Adult Jam", "rockstars").unwrap();
    let setlist = Loaded {
        kind: Kind::Setlist,
        path: "Setlists/Sunday.setlist".to_owned(),
        title: "Sunday".to_owned(),
    };
    rooms.set_setlist(&made.id, setlist).unwrap();
    let text = std::fs::read_to_string(dir.path().join(format!("{}.room", made.id))).unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        file["setlist"],
        serde_json::json!({"kind": "setlist", "path": "Setlists/Sunday.setlist", "title": "Sunday"})
    );
}

#[test]
fn a_setlist_for_no_room_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::at(dir.path());
    let nobody = room::parse_link("nobody-123456").unwrap();
    let setlist = Loaded {
        kind: Kind::Setlist,
        path: "Setlists/Sunday.setlist".to_owned(),
        title: "Sunday".to_owned(),
    };
    assert!(rooms.set_setlist(&nobody, setlist).is_err());
}
