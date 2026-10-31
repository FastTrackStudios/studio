//! Hit ids are a take's own, and the audit counts the take the comp plays:
//! two takes recorded from the same place (lanes of one song) with the
//! same hits no longer stand for each other.

use std::collections::HashMap;

use daw_proto::{HitMark, TakeMarker};

use super::{Heard, renew_markers, take_tag, tally_audit};

/// A snare hit, as a detector run of an older build wrote it: the same id
/// in every take that starts where this one does.
fn hit(id: &str, at: f64, flam_with: Option<&str>) -> TakeMarker {
    TakeMarker {
        id: id.to_owned(),
        source_position_seconds: at,
        hit: Some(HitMark {
            piece: "Snare".to_owned(),
            flam_with: flam_with.map(str::to_owned),
            ..HitMark::default()
        }),
        ..TakeMarker::default()
    }
}

#[test]
fn two_takes_from_the_same_place_give_their_hits_different_ids() {
    let teal = take_tag([
        "Media/Snare Top-260907_1321-01.wav",
        "Media/Snare Trig-260907_1321-01.wav",
    ]);
    let pink = take_tag([
        "Media/Snare Top-260907_1340-02.wav",
        "Media/Snare Trig-260907_1340-02.wav",
    ]);
    assert_ne!(teal, pink);
    // The same files in another order are the same take.
    assert_eq!(
        teal,
        take_tag([
            "Media/Snare Trig-260907_1321-01.wav",
            "Media/Snare Top-260907_1321-01.wav"
        ])
    );

    let old = || {
        vec![
            hit("9082ffc0.snare.0095@0", 1.0, None),
            hit("9082ffc0.snare.0096@0", 1.02, Some("9082ffc0.snare.0095@0")),
        ]
    };
    let (teal_marks, renamed) = renew_markers(old(), &teal);
    let (pink_marks, _) = renew_markers(old(), &pink);
    let ids = |m: &[TakeMarker]| m.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
    assert_eq!(
        ids(&teal_marks),
        [
            format!("9082ffc0.snare.0095@0~{teal}"),
            format!("9082ffc0.snare.0096@0~{teal}")
        ]
    );
    assert!(
        ids(&teal_marks)
            .iter()
            .all(|id| !ids(&pink_marks).contains(id)),
        "no id is both takes'"
    );
    assert_eq!(
        teal_marks[1].hit.as_ref().and_then(|h| h.flam_with.clone()),
        Some(format!("9082ffc0.snare.0095@0~{teal}")),
        "a flam names its partner in the same take"
    );
    assert_eq!(renamed.len(), 2);
    // Renewed again: nothing to do.
    let (again, renamed) = renew_markers(teal_marks.clone(), &teal);
    assert_eq!(again, teal_marks);
    assert!(renamed.is_empty());
}

fn heard(source: &str, id: &str, time: f64) -> Heard {
    Heard {
        source: source.to_owned(),
        id: id.to_owned(),
        piece: "Snare".to_owned(),
        time,
        peak_db: -18.0,
        soft: false,
    }
}

/// Teal plays, Pink is kept on a lane; both have a hit `a` (an id from
/// before ids were per take). Teal's `a` plays: none lost — Pink's is not
/// the comp's, and is not counted. Teal's `b` is cut away: one lost.
#[test]
fn the_audit_counts_the_playing_take_by_recording_and_id() {
    let heard_in_play = [heard("teal.wav", "a", 1.0), heard("teal.wav", "b", 2.0)];
    let plays: HashMap<(String, String), usize> = [
        (("teal.wav".to_owned(), "a".to_owned()), 1),
        // Pink's `b` plays (another stretch's comp is Pink): it
        // must not stand for Teal's `b`, nor for Teal's `a` twice.
        (("pink.wav".to_owned(), "b".to_owned()), 1),
    ]
    .into_iter()
    .collect();
    let audit = tally_audit(&heard_in_play, &plays, &|_| (1, 1.0));
    assert_eq!(audit.len(), 1);
    let snare = &audit[0];
    assert_eq!(
        (
            snare.hits,
            snare.playing,
            snare.lost,
            snare.lost_strokes,
            snare.doubled
        ),
        (2, 1, 1, 1, 0)
    );
    assert_eq!(
        snare.problems,
        ["lost: b at 2.000 s (bar 1 beat 1.00), -18.0 dBFS"]
    );
}
