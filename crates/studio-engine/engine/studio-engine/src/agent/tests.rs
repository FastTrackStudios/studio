//! The song's revision and journal: a stale edit refused with what moved,
//! the edits after a revision from both sides, and what is not an edit.

use serde_json::json;

use super::{By, Journal, KEPT, kind_of, view_only};

/// A call made at the revision the song is at runs; one made before an
/// edit is refused as `stale`, saying what changed and who made it.
#[test]
fn an_edit_made_at_an_old_revision_is_refused_with_what_changed() {
    let mut j = Journal::new();
    let read = j.note(
        By::Agent,
        Some("TRACK_SET"),
        "Set track",
        Some(&json!({"track": "Kick/Sum/In", "volume_db": -6.0})),
    );
    assert!(j.check(read).is_none(), "nothing moved: it runs");
    j.note(
        By::Person,
        Some("SIGNAL_BLOCK_SET"),
        "Set a Signal block",
        Some(&json!({"track": "Snare/Top", "block": "tone.comp", "params": {"threshold": -20}})),
    );
    let stale = j.check(read).expect("refused");
    assert_eq!(stale["code"], "stale");
    assert_eq!(stale["revision"], 2);
    assert_eq!(stale["since"], 1);
    let c = &stale["changes"][0];
    assert_eq!(c["by"], "person");
    assert_eq!(c["kind"], "fx");
    assert_eq!(c["track"], "Snare/Top");
    assert_eq!(c["block"], "tone.comp");
    assert_eq!(c["fields"], json!(["threshold"]));
    assert!(
        stale["error"]
            .as_str()
            .unwrap_or_default()
            .contains("1 by the person")
    );
}

/// A revision from a later life of the engine (it restarted) is stale too.
#[test]
fn a_revision_from_before_a_restart_is_stale() {
    let j = Journal::new();
    let stale = j.check(40).expect("refused");
    assert!(
        stale["error"]
            .as_str()
            .unwrap_or_default()
            .contains("restarted")
    );
    assert_eq!(stale["complete"], false);
}

/// `changes {since}` lists both sides' edits after it, oldest first, and
/// pages.
#[test]
fn changes_since_lists_the_person_and_the_agent() {
    let mut j = Journal::new();
    let since = j.note(By::Person, None, "Move items", None);
    j.note(
        By::Agent,
        Some("TRACK_SET"),
        "Set track",
        Some(&json!({"track": "Kick", "volume_db": -3.0, "pan": 0.1})),
    );
    j.note(By::Person, None, "Add marker", None);
    j.note(
        By::Agent,
        Some("SENDS_SET"),
        "Set a send",
        Some(&json!({"track": "Snare", "to": "Verb"})),
    );
    let all = j.page(since, 0, 50);
    assert_eq!(all["total"], 3);
    assert_eq!(
        (all["by_person"].clone(), all["by_agent"].clone()),
        (json!(1), json!(2))
    );
    assert_eq!(all["complete"], true);
    let kinds: Vec<&str> = all["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| c["kind"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(kinds, ["track", "marker", "send"]);
    let mut fields: Vec<&str> = all["changes"][0]["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_str())
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["pan", "volume_db"]);
    assert_eq!(all["changes"][1]["by"], "person");
    assert!(all["changes"][1].get("action").is_none());
    let first = j.page(since, 0, 2);
    assert_eq!(first["next"], 2);
    let rest = j.page(since, 2, 2);
    assert_eq!(rest["changes"].as_array().map(Vec::len), Some(1));
    assert!(rest.get("next").is_none());
}

/// The journal is bounded: past [`KEPT`], the oldest go and a revision from
/// before them says the list is not complete.
#[test]
fn the_journal_forgets_its_oldest_and_says_so() {
    let mut j = Journal::new();
    for _ in 0..KEPT + 5 {
        j.note(By::Person, None, "Move items", None);
    }
    let (kept, complete, now) = j.since(0);
    assert_eq!(kept.len(), KEPT);
    assert!(!complete);
    assert_eq!(now, u64::try_from(KEPT + 5).unwrap_or(0));
    assert!(j.since(10).1, "nothing after 10 was forgotten");
}

#[test]
fn what_is_shown_or_played_is_not_an_edit() {
    for id in [
        "SESSION_SELECT_TRACK",
        "SESSION_FOLD",
        "SESSION_ZOOM",
        "TRANSPORT_PLAY",
        "MIX_SNAPSHOT",
    ] {
        assert!(view_only(id), "{id}");
    }
    for id in [
        "TRACK_SET",
        "FX_SET",
        "SIGNAL_BLOCK_SET",
        "QUANTIZE",
        "SESSION_UNDO",
        "MIX_RESTORE",
        "LISTEN_AB",
    ] {
        assert!(!view_only(id), "{id}");
    }
    assert_eq!(kind_of("SIGNAL_BLOCK_SET"), "fx");
    assert_eq!(kind_of("SENDS_SET"), "send");
    assert_eq!(kind_of("DRUMS_REMOVE"), "items");
    assert_eq!(kind_of("SESSION_UNDO"), "undo");
}
