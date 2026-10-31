use daw_proto::{HitMark, TakeMarker};

use super::{
    CLICK_PEAK, Placed, clicks, dead_trigger_hits, made_name, merged, sure_hits, uncovered,
};

fn hit(piece: &str, at: f64, trigger: bool, mic: bool, confidence: f64, bleed: bool) -> TakeMarker {
    TakeMarker {
        source_position_seconds: at,
        hit: Some(HitMark {
            piece: piece.into(),
            from_trigger: trigger,
            from_mic: mic,
            confidence,
            bleed,
            ..HitMark::default()
        }),
        ..TakeMarker::default()
    }
}

#[test]
fn a_trigger_that_heard_none_of_its_drum_is_made_from_the_sure_mic_hits() {
    let markers = vec![
        hit("Tom 1", 6.99, false, true, 0.7, false),
        hit("Tom 1", 4.77, false, true, 0.68, false),
        // Doubted, or another drum's bleed: no click.
        hit("Tom 1", 4.56, false, true, 0.53, true),
        hit("Tom 1", 5.0, false, true, 0.3, false),
        // Another drum's hits are not this trigger's.
        hit("Tom 2", 6.99, true, true, 0.95, false),
    ];
    assert_eq!(dead_trigger_hits(&markers, "Tom 1"), Some(vec![4.77, 6.99]));
}

#[test]
fn a_trigger_that_heard_its_drum_once_is_left_alone() {
    let markers = vec![
        hit("Tom 1", 4.77, false, true, 0.9, false),
        hit("Tom 1", 12.9, true, true, 0.95, false),
    ];
    assert_eq!(dead_trigger_hits(&markers, "Tom 1"), None);
    // And nothing heard at all is nothing to make.
    assert_eq!(dead_trigger_hits(&[], "Tom 1"), None);
}

#[test]
fn each_hit_is_a_short_click_where_it_was_and_silence_between() {
    let rate = 48_000;
    let out = clicks(&[0.5, 1.0], rate as usize * 2, rate);
    let loud: Vec<usize> = out
        .iter()
        .enumerate()
        .filter(|(_, s)| s.abs() > 0.01)
        .map(|(i, _)| i)
        .collect();
    // Two clicks of ~1 ms each, starting at their hits.
    assert_eq!(loud.first(), Some(&24_000));
    assert!(
        loud.iter()
            .all(|&i| (24_000..24_048).contains(&i) || (48_000..48_048).contains(&i))
    );
    assert!(out.iter().all(|s| s.abs() <= CLICK_PEAK));
    assert!(
        out.iter().any(|s| *s > CLICK_PEAK * 0.95),
        "a click reaches its peak"
    );
}

#[test]
fn the_made_trigger_is_named_for_the_one_it_stands_for() {
    assert_eq!(
        made_name("78-T1 Trig-260907_1321-01.wav"),
        "78-T1 Trig-260907_1321-01 (from mics).wav"
    );
    // Wherever the recording is, the made one is named for its file alone:
    // it is written into the song's own Media folder.
    assert_eq!(
        made_name("/elsewhere/Set In Stone/Media/46-Trig.wav"),
        "46-Trig (from mics).wav"
    );
}

/// The comp plays a take with no trigger (its toms recorded in another
/// session) from 72.3 s for 18.7 s, in slices on the EDIT lane (35), and
/// its whole take on the COMP lane (0): those, and only those, have no
/// trigger item. A slice the trigger covers but for a seam's hair has one.
#[test]
fn the_mic_items_with_no_trigger_item_on_their_lane_are_found() {
    let edit = Some(35);
    let comp = Some(0);
    let mic: Vec<Placed> = vec![
        (edit, 60.0, 72.3),  // a take with its trigger
        (edit, 72.3, 80.0),  // the take with none…
        (edit, 80.0, 91.0),  // …in two slices
        (comp, 72.3, 91.0),  // and on the comp lane
        (edit, 91.0, 100.0), // a trigger again
    ];
    let trig: Vec<Placed> = vec![
        (edit, 60.0, 72.3),
        // On another lane: not the EDIT lane's trigger.
        (Some(7), 72.3, 91.0),
        (edit, 91.002, 100.0),
    ];
    assert_eq!(uncovered(&mic, &trig), [1, 2, 3]);
    assert_eq!(merged(&[(72.3, 80.0), (80.0, 91.0)]), [(72.3, 91.0)]);
    assert_eq!(
        merged(&[(1.0, 2.0), (5.0, 5.85)]),
        [(1.0, 2.0), (5.0, 5.85)],
        "stretches apart stay apart"
    );
}

/// A take with no trigger clicks at its mic's sure hits and at every hit
/// a hand placed or checked; not at bleed, doubted hits or other drums'.
#[test]
fn a_take_with_no_trigger_clicks_at_its_sure_and_hand_placed_hits() {
    let mut placed = hit("Tom 3", 9.0, false, false, 1.0, false);
    if let Some(h) = placed.hit.as_mut() {
        h.origin = daw_proto::MarkOrigin::Manual;
    }
    let markers = vec![
        hit("Tom 3", 8.0, false, true, 0.8, false),
        hit("Tom 3", 7.0, false, true, 0.6, true),
        hit("Tom 3", 6.0, false, true, 0.2, false),
        hit("Tom 2", 5.0, false, true, 0.9, false),
        placed,
    ];
    assert_eq!(sure_hits(&markers, "Tom 3"), [8.0, 9.0]);
}
