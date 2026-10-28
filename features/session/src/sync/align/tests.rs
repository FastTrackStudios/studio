use std::collections::BTreeMap;

use super::{Renames, renames};
use crate::sync::diff;
use crate::sync::model::{EnvelopeState, FxState, ItemState, SessionModel, TakeState, TrackState};

fn fx(guid: &str, plugin: &str) -> FxState {
    FxState {
        guid: guid.into(),
        plugin: plugin.into(),
        name: plugin.into(),
        enabled: true,
        ..FxState::default()
    }
}

fn midi(track: &str, at: f64) -> ItemState {
    ItemState {
        track: track.into(),
        position: at,
        length: 2.0,
        take: TakeState {
            midi: true,
            ..TakeState::default()
        },
        ..ItemState::default()
    }
}

/// A track with two FX, one automated, and two MIDI items — under the
/// FX and item ids `ids` gives.
fn song(ids: [&str; 4]) -> SessionModel {
    let [eq, comp, verse, chorus] = ids;
    let mut keys = TrackState {
        guid: "keys".into(),
        ..TrackState::default()
    };
    keys.fx = vec![fx(eq, "ReaEQ"), fx(comp, "ReaComp")];
    keys.envelopes
        .insert(format!("fx:{comp}:3"), EnvelopeState::default());
    SessionModel {
        tracks: vec![keys],
        items: BTreeMap::from([
            (verse.to_owned(), midi("keys", 0.0)),
            (chorus.to_owned(), midi("keys", 8.0)),
        ]),
        ..SessionModel::default()
    }
}

#[test]
fn the_same_song_under_an_engines_own_ids_is_only_renamed() {
    let mut engine = song(["e1", "e2", "i1", "i2"]);
    let doc = song(["{EQ}", "{COMP}", "verse", "chorus"]);
    let found = renames(&engine, &doc);
    assert_eq!(
        found,
        Renames {
            fx: vec![("e1".into(), "{EQ}".into()), ("e2".into(), "{COMP}".into())],
            items: vec![
                ("i1".into(), "verse".into()),
                ("i2".into(), "chorus".into())
            ],
        }
    );
    found.apply(&mut engine);
    assert_eq!(engine, doc);
    assert!(diff(&engine, &doc).is_empty());
}

#[test]
fn what_really_differs_is_never_renamed() {
    let engine = song(["e1", "e2", "i1", "i2"]);
    let mut doc = song(["{EQ}", "{COMP}", "verse", "chorus"]);
    // Another plugin in the second slot; the chorus moved.
    doc.tracks[0].fx[1].plugin = "ReaXcomp".into();
    let chorus = doc.items.remove("chorus").map(|mut item| {
        item.position = 9.0;
        item
    });
    doc.items
        .extend(chorus.map(|item| ("chorus".to_owned(), item)));
    assert_eq!(
        renames(&engine, &doc),
        Renames {
            fx: vec![("e1".into(), "{EQ}".into())],
            items: vec![("i1".into(), "verse".into())],
        }
    );
}

#[test]
fn a_model_already_under_the_docs_ids_has_nothing_to_rename() {
    let doc = song(["{EQ}", "{COMP}", "verse", "chorus"]);
    assert!(renames(&doc, &doc).is_empty());
}

#[test]
fn two_equal_items_in_one_place_pair_once_each() {
    let mut engine = SessionModel::default();
    let mut doc = SessionModel::default();
    for (mine, theirs) in [("i1", "a"), ("i2", "b")] {
        engine.items.insert(mine.into(), midi("keys", 0.0));
        doc.items.insert(theirs.into(), midi("keys", 0.0));
    }
    let found = renames(&engine, &doc);
    assert_eq!(found.items.len(), 2);
    found.apply(&mut engine);
    assert_eq!(engine, doc);
}
