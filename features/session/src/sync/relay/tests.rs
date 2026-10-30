use std::time::Duration;

use crate::sync::model::{FxState, SessionModel, TrackState};
use crate::sync::relay::{Relay, Side, exchange, missing, same_lineage, seed};
use crate::sync::{ORIGIN_LOCAL, SessionDoc};

fn song() -> SessionModel {
    SessionModel {
        tracks: vec![TrackState {
            guid: "verb".into(),
            name: "Verb".into(),
            volume: 0.8,
            fx: vec![FxState {
                guid: "{VERB}".into(),
                plugin: "ReaVerb".into(),
                name: "ReaVerb".into(),
                enabled: true,
                ..FxState::default()
            }],
            ..TrackState::default()
        }],
        ..SessionModel::default()
    }
}

/// A song doc with history, and a live doc seeded from it.
fn seeded() -> (SessionDoc, SessionDoc) {
    let song_doc = SessionDoc::new();
    song_doc.write(&song(), ORIGIN_LOCAL).expect("written");
    let mut louder = song();
    louder.tracks[0].volume = 0.9;
    song_doc.write(&louder, ORIGIN_LOCAL).expect("an edit");
    let live = SessionDoc::new();
    live.loro()
        .import(&seed(song_doc.loro()).expect("a seed"))
        .expect("seeded");
    (song_doc, live)
}

fn fx_count(doc: &SessionDoc) -> usize {
    doc.read().tracks.iter().map(|t| t.fx.len()).sum()
}

#[test]
fn a_seeded_doc_is_the_song_and_of_its_lineage() {
    let (song_doc, live) = seeded();
    assert_eq!(live.read(), song_doc.read());
    assert!(same_lineage(song_doc.loro(), live.loro()));
    assert!(same_lineage(live.loro(), song_doc.loro()));
    let apart = SessionDoc::new();
    apart.write(&song(), ORIGIN_LOCAL).expect("written");
    assert!(
        !same_lineage(song_doc.loro(), apart.loro()),
        "the same song written apart is another lineage"
    );
    assert!(!same_lineage(song_doc.loro(), SessionDoc::new().loro()));
    assert_eq!(missing(song_doc.loro(), live.loro()).expect("read"), None);
}

/// The regression's shape at the doc level: edits on either side, passed
/// across many times over, leave one of each FX on both.
#[test]
fn edits_passed_back_and_forth_make_nothing_twice() {
    let (song_doc, live) = seeded();
    let a = Side::imported(song_doc.loro().clone());
    let b = Side::imported(live.loro().clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        for round in 0..40_u32 {
            let side = if round % 2 == 0 { &song_doc } else { &live };
            let mut model = side.read();
            model.tracks[0].volume = f64::from(round) / 40.0;
            side.write(&model, ORIGIN_LOCAL).expect("an edit");
            exchange(&a, &b).await.expect("passed");
            // Passed again, as a relay woken twice would: nothing moves.
            assert_eq!(exchange(&a, &b).await.expect("passed"), 0);
        }
    });
    assert_eq!(song_doc.read(), live.read());
    assert_eq!(fx_count(&song_doc), 1);
    assert_eq!(fx_count(&live), 1);
}

/// Wait (up to 5 s) until `what` holds.
async fn until(what: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !what() {
        assert!(std::time::Instant::now() < deadline, "never came");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// The relay passes what either side writes, and while locked passes
/// nothing until it is unlocked.
#[test]
fn a_relay_keeps_both_in_step_and_holds_while_locked() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (song_doc, live) = seeded();
        let (lock, locked) = tokio::sync::watch::channel(false);
        let relay = Relay::start(
            Side::imported(song_doc.loro().clone()),
            Side::imported(live.loro().clone()),
            locked,
        );
        let volume = |doc: &SessionDoc| doc.read().tracks[0].volume;

        let mut model = live.read();
        model.tracks[0].volume = 0.1;
        live.write(&model, ORIGIN_LOCAL).expect("an edit there");
        until(|| volume(&song_doc) == 0.1).await;

        lock.send_replace(true);
        let mut model = song_doc.read();
        model.tracks[0].volume = 0.2;
        song_doc.write(&model, ORIGIN_LOCAL).expect("an edit here");
        until(|| relay.held()).await;
        assert_eq!(volume(&live), 0.1, "held while locked");

        lock.send_replace(false);
        until(|| volume(&live) == 0.2).await;
        assert!(!relay.held());
        assert_eq!(fx_count(&live), 1);
    });
}
