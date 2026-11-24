//! The gap fill's rules, one test each, and the slices of Unbreakable's
//! approved edit that played as slow as 0.004×.

use super::*;

const CUT: Cut = Cut {
    lead: 0.005,
    guard: 0.015,
};

fn hit(at: f64, shift: f64) -> Hit {
    Hit { at, shift }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn placed(hits: &[Hit], span: (f64, f64), takes: &[Take]) -> Placed {
    let out = place_filled(hits, span, CUT, (None, None), (takes, Fill::default()));
    sound(&out, hits, Fill::default());
    out
}

/// What every placement must hold: each hit plays where it was sent, at
/// its own speed, by a piece of its shift; no piece plays an attack it is
/// not that hit's (no attack sounds twice); no stretch is slower than the
/// least rate.
fn sound(out: &Placed, hits: &[Hit], fill: Fill) {
    for h in hits {
        let t = h.at + h.shift;
        let by = out
            .pieces
            .iter()
            .find(|p| (p.shift - h.shift).abs() < 1e-9 && p.start <= t + 1e-9 && t < p.end - 1e-9);
        let p = by.unwrap_or_else(|| panic!("hit {h:?} plays in no piece: {:?}", out.pieces));
        if let Some(from) = p.drawn_from {
            assert!(
                from >= t + fill.keep - 1e-9,
                "the attack of {h:?} is stretched"
            );
        }
    }
    for p in &out.pieces {
        let (lo, hi) = (p.start - p.shift, p.end - p.shift);
        for h in hits.iter().filter(|h| (h.shift - p.shift).abs() > 1e-9) {
            assert!(
                !(lo + 1e-9 < h.at && h.at < hi - 1e-9),
                "{p:?} sounds the attack of {h:?} again"
            );
        }
        if let (Some(from), Some(to)) = (p.drawn_from, p.drawn_to) {
            let rate = (p.end - from) / (to - from);
            assert!(rate >= fill.least_rate - 1e-9, "{p:?} plays at {rate}");
        }
        assert!(p.drawn_to.is_none() || p.drawn_from.is_some(), "{p:?}");
    }
}

#[test]
fn the_piece_before_plays_on_through_its_own_decay() {
    // The snare at 1.05 played 100 ms early, before the kick at 1.0: its
    // slice plays on, as recorded, up to just before the kick's lead —
    // 100 ms past where it was cut — never into the kick's attack.
    let hits = [hit(1.0, 0.0), hit(1.05, -0.1), hit(2.0, 0.0)];
    let out = placed(&hits, (0.945, 2.5), &[]);
    let snare = &out.pieces[0];
    assert!(near(snare.shift, -0.1));
    assert!(near(snare.end, 0.995));
    assert!(snare.drawn_to.is_none());
    let kick = &out.pieces[1];
    assert!(near(kick.start, 0.995 - 0.004));
    assert_eq!(kick.crossfade, Some(0.004));
}

#[test]
fn the_piece_after_reaches_back_and_they_crossfade_where_they_overlap() {
    // The snare 30 ms later: the kick's slice plays to just before the
    // snare's own attack (1.195), the snare's reaches back into its
    // lead-in to meet it; they share 150 ms of recording and crossfade
    // the last 10 of it.
    let hits = [hit(1.0, 0.0), hit(1.2, 0.03)];
    let out = placed(&hits, (0.995, 2.0), &[]);
    let (kick, snare) = (&out.pieces[0], &out.pieces[1]);
    assert!(near(kick.end, 1.195));
    assert!(kick.drawn_to.is_none());
    assert!(near(snare.start, 1.185));
    assert_eq!(snare.crossfade, Some(0.010));
    let [fill] = out.fills.as_slice() else {
        panic!("{:?}", out.fills)
    };
    assert!(near(fill.gap, 0.03));
    assert!(near(fill.reached_back, 1.225 - 1.185));
    assert!(near(fill.crossfade, 0.010));
    assert_eq!(fill.stretched, None);
    assert!(near(fill.left, 0.0));
}

#[test]
fn only_the_last_span_of_what_is_left_is_stretched() {
    // 330 ms later after a 300 ms gap: the snare reaches back to 15 ms
    // past the kick's attack (1.345 as it now plays), the kick's slice
    // stops at 1.295: 50 ms (and the crossfade) is left. Only the last
    // 216 ms of the kick's decay is drawn, at 0.8, over it.
    let hits = [hit(1.0, 0.0), hit(1.3, 0.33)];
    let out = placed(&hits, (0.995, 2.5), &[]);
    let kick = &out.pieces[0];
    assert!(near(kick.end, 1.295));
    let from = kick.drawn_from.expect("drawn");
    let to = kick.drawn_to.expect("drawn");
    let short = 0.05 + Fill::default().crossfade;
    assert!(near(1.295 - from, short * 4.0));
    assert!(near(to, 1.345 + Fill::default().crossfade));
    let s = out.fills[0].stretched.expect("stretched");
    assert!(near(s.rate, 0.8));
    assert!(near(s.from, from));
    assert!(near(out.fills[0].left, 0.0));
    assert!(near(out.pieces[1].start, 1.345));
}

#[test]
fn past_what_a_stretch_reaches_at_the_least_rate_is_left_silent() {
    // 300 ms later after 50 ms: the kick's decay after its attack is
    // 15 ms, drawn at 0.8 it covers under 4: the rest stays a gap rather
    // than a slice at 0.004×.
    let hits = [hit(1.0, 0.0), hit(1.05, 0.3)];
    let out = placed(&hits, (0.995, 2.5), &[]);
    let fill = &out.fills[0];
    let s = fill.stretched.expect("stretched");
    assert!(near(s.rate, 0.8));
    assert!(fill.left > 0.25, "{fill:?}");
    assert_eq!(out.pieces[1].crossfade, Some(0.0));
}

#[test]
fn a_fill_stops_at_a_take_switch_when_the_other_side_covers() {
    // The kick's take ends at 1.28, the snare's began at 1.1: the kick
    // stops at its take's end, the snare's own take reaches back to meet
    // it. The kick's take's audio past 1.28 is not played.
    let takes = [
        Take {
            from: 0.0,
            to: 1.28,
        },
        Take { from: 1.1, to: 3.0 },
    ];
    let hits = [hit(1.0, 0.0), hit(1.3, 0.01)];
    let out = placed(&hits, (0.995, 2.5), &takes);
    assert!(near(out.pieces[0].end, 1.28));
    assert!(out.pieces[0].drawn_to.is_none());
    assert_eq!(out.fills[0].take_switch, TakeSwitch::Stopped);
    // The snare reaches back no further than its take's start.
    assert!(out.pieces[1].start >= 1.1 + 0.01 - 1e-9);
}

#[test]
fn a_fill_plays_through_the_comps_switch_rather_than_stretch() {
    // Both takes stop short at their switch (1.2): stopped there, 330 ms
    // is left; through the comp's own switch, as the comp plays it, the
    // fill is the one with no take switch at all.
    let takes = [Take { from: 0.0, to: 1.2 }, Take { from: 1.2, to: 3.0 }];
    let hits = [hit(1.0, 0.0), hit(1.3, 0.33)];
    let out = placed(&hits, (0.995, 2.5), &takes);
    let free = placed(&hits, (0.995, 2.5), &[]);
    assert_eq!(out.fills[0].take_switch, TakeSwitch::Crossed);
    assert_eq!(out.pieces, free.pieces);
}

#[test]
fn no_hit_moves_and_no_attack_sounds_twice_in_a_busy_edit() {
    // Sixteenths at 160 bpm, each sent up to ±20 ms (a quantize's
    // spread), a flam's grace riding its stroke: `sound` holds for every
    // piece.
    let step = 60.0 / 160.0 / 4.0;
    let hits: Vec<Hit> = (0..64_u32)
        .map(|i| {
            let k = f64::from(i);
            let off = ((k * 7.3).sin() * 0.02 * 1e4).round() / 1e4;
            hit(1.0 + k * step + off, -off)
        })
        .collect();
    let out = placed(&hits, (0.995, 1.0 + 64.0 * step), &[]);
    assert!(out.pieces.len() > 32);
    for p in &out.pieces {
        if let (Some(from), Some(to)) = (p.drawn_from, p.drawn_to) {
            assert!((p.end - from) / (to - from) >= 0.8 - 1e-9);
        }
    }
}

#[test]
fn the_span_end_is_met_the_same_way() {
    // The last piece stops before the next attack outside the span and is
    // drawn, at no less than 0.8, only over its last span.
    let hits = [hit(1.0, 0.0), hit(1.2, 0.0)];
    let out = place_filled(
        &hits,
        (0.995, 1.49),
        CUT,
        (None, Some(1.45)),
        (&[], Fill::default()),
    );
    let last = out.pieces.last().expect("a piece");
    assert!(near(last.end, 1.445));
    let from = last.drawn_from.expect("drawn");
    assert!(near(last.drawn_to.unwrap_or(0.0), 1.49));
    assert!(near((1.445 - from) / (1.49 - from), 0.8));
    assert!(near(out.fills.last().map_or(1.0, |f| f.left), 0.0));
}

/// One slice of Unbreakable's approved edit: where it starts (project
/// seconds), how long its recording runs, the rate it played at.
const UNBREAKABLE: [(f64, f64, f64); 9] = [
    (253.542, 0.025, 0.0039),
    (162.303, 0.068, 0.159),
    (215.896, 0.085, 0.221),
    (185.235, 0.043, 0.271),
    (214.016, 0.088, 0.281),
    (222.678, 0.061, 0.288),
    (240.712, 0.115, 0.329),
    (211.678, 0.072, 0.523),
    (192.818, 0.089, 0.580),
];

/// The old rule's reach for a slice of `natural` seconds at `rate`: its
/// first 30 ms as recorded, the rest slowed.
fn old_reach(natural: f64, rate: f64) -> f64 {
    let keep = 0.030_f64.min(natural / 2.0);
    keep + (natural - keep) / rate
}

#[test]
fn unbreakables_slow_slices_play_no_slower_than_point_eight_and_no_hit_moves() {
    // Each slice rebuilt: its hit, the next stroke `natural` later in the
    // recording, sent so far later that the old rule drew the slice out
    // to `rate`. However the recording lies around them, every stretch is
    // at least 0.8×, every hit plays exactly where it was sent.
    for (start, natural, rate) in UNBREAKABLE {
        let a = start + CUT.lead;
        let b = a + natural;
        // The old guard was the cut's and a 4 ms crossfade's.
        let by = start + old_reach(natural, rate) - (a + CUT.guard + 0.004);
        // The next stroke is the one after the slice in the recording:
        // the snare reaches back only into the slice's own decay.
        let hits = [hit(a - 0.4, 0.0), hit(a, 0.0), hit(b, by), hit(b + 0.4, by)];
        let span = (a - 0.4 - CUT.lead, b + 0.6 + by);
        let out = placed(&hits, span, &[]);
        let fill = out
            .fills
            .iter()
            .find(|f| near(f.recorded, b))
            .unwrap_or_else(|| panic!("no fill before {b}: {:?}", out.fills));
        if let Some(s) = fill.stretched {
            assert!(s.rate >= 0.8 - 1e-9, "{start}: {s:?}");
        }
        // The fill is never more than the gap the old rule drew over.
        assert!(fill.left <= by + 1e-9, "{start}: {fill:?}");
        // The snare comes from a phrase used elsewhere: its own take has
        // room before it, and the gap is filled with no stretch at all.
        let far = a + 5.0;
        let moved = b + by - far;
        let hits = [
            hit(a - 0.4, 0.0),
            hit(a, 0.0),
            hit(far, moved),
            hit(far + 0.4, moved),
        ];
        let out = placed(&hits, (a - 0.4 - CUT.lead, far + 0.6 + moved), &[]);
        assert!(
            out.fills
                .iter()
                .all(|f| f.stretched.is_none() && f.left == 0.0),
            "{start}: {:?}",
            out.fills
        );
        assert!(out.pieces.iter().all(|p| p.drawn_to.is_none()));
    }
}
