use session::sync::{ORIGIN_LOCAL, SessionDoc, SessionModel, TrackState};

use super::{Meeting, TaskSet, may_seed, meeting};

fn guest() -> TaskSet {
    TaskSet::parse(
        "share:http://127.0.0.1:9090/org/fasttrackaudio/share/83b5d9c800aa4de1b945a59eb5032882",
    )
}

fn member() -> TaskSet {
    TaskSet {
        url: "ws://127.0.0.1:9090/org/fasttrackaudio/vox".into(),
        token: Some("token".into()),
        setlist: "8ba8fc6d".into(),
    }
}

/// A replica the host has seeded: it holds a session.
fn seeded() -> SessionDoc {
    let doc = SessionDoc::new();
    let song = SessionModel {
        tracks: vec![TrackState {
            guid: "kick".into(),
            ..TrackState::default()
        }],
        ..SessionModel::default()
    };
    doc.write(&song, ORIGIN_LOCAL).expect("written");
    doc
}

#[test]
fn only_a_member_who_can_write_seeds_a_set() {
    assert!(guest().is_guest());
    assert!(!member().is_guest());
    assert!(may_seed(&member(), false));
    assert!(!may_seed(&member(), true), "an observer never seeds");
    assert!(
        !may_seed(&guest(), false),
        "a share link's guest never seeds"
    );
    assert!(!may_seed(&guest(), true));
}

/// A guest finding a song's doc still empty waits for the host's seed —
/// it never writes the set's doc — where a member seeds it; a seeded doc
/// is joined by either, and a song the set lacks stays this machine's.
#[test]
fn a_guest_waits_for_the_hosts_seed_and_never_writes_it() {
    assert!(matches!(
        meeting(Some(SessionDoc::new()), false),
        Meeting::Wait(_)
    ));
    assert!(matches!(
        meeting(Some(SessionDoc::new()), true),
        Meeting::Seed(_)
    ));
    for seeds in [false, true] {
        assert!(matches!(meeting(Some(seeded()), seeds), Meeting::Join(_)));
        assert!(matches!(meeting(None, seeds), Meeting::Own));
    }
    // Waiting wrote nothing into the empty replica.
    let empty = SessionDoc::new();
    let before = empty.version();
    let waited = match meeting(Some(empty), false) {
        Meeting::Wait(doc) => Some((doc.version() == before, doc.has_session())),
        _ => None,
    };
    assert_eq!(waited, Some((true, false)));
}
