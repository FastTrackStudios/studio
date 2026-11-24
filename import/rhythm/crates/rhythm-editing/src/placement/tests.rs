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
/// leftover's floor.
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
            assert!(
                rate >= fill.least_rate_leftover - 1e-9,
                "{p:?} plays at {rate}"
            );
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
fn past_what_a_stretch_reaches_at_the_floor_a_faded_dip_is_left() {
    // 300 ms later after 50 ms: the kick's decay after its attack is
    // 15 ms, drawn at 0.5 it covers 15 more: the rest is a dip, each side
    // fading within itself (a crossfade of 0), rather than a slice at
    // 0.004×. The dip is the silence itself, the crossfade not counted.
    let hits = [hit(1.0, 0.0), hit(1.05, 0.3)];
    let out = placed(&hits, (0.995, 2.5), &[]);
    let fill = &out.fills[0];
    let s = fill.stretched.expect("stretched");
    assert!(near(s.rate, 0.5));
    let kick = &out.pieces[0];
    let snare = &out.pieces[1];
    assert!(near(fill.left, snare.start - kick.reach()), "{fill:?}");
    assert!(fill.left > 0.25, "{fill:?}");
    assert_eq!(snare.crossfade, Some(0.0));
}

#[test]
fn a_stretch_past_the_least_rate_is_only_as_slow_as_it_must_be() {
    // The snare 110 ms later after 100: it reaches back to 1.125, the
    // kick's slice stops at 1.095, 34 ms short with the crossfade. At 0.8
    // that needs 136 ms of decay, the kick has 65 past its keep: all 65
    // are drawn over 99, at about 0.66 — not at the 0.5 floor.
    let hits = [hit(1.0, 0.0), hit(1.1, 0.11)];
    let out = placed(&hits, (0.995, 2.5), &[]);
    let fill = &out.fills[0];
    let s = fill.stretched.expect("stretched");
    assert!(s.rate < 0.8 && s.rate > 0.5, "{s:?}");
    let kick = &out.pieces[0];
    // The whole decay past the kick's keep, never its attack.
    assert!(near(
        kick.drawn_from.expect("drawn"),
        1.0 + Fill::default().keep
    ));
    assert!(near(fill.left, 0.0), "{fill:?}");
    assert!(near(
        kick.reach(),
        out.pieces[1].start + Fill::default().crossfade
    ));
}

#[test]
fn the_piece_after_replays_the_previous_strokes_decay_before_a_stretch() {
    // A guard of 40 ms (wider than the 30 ms keep): the snare 20 ms later
    // after 60 ms reaches back past the guard, into the kick's decay, up
    // to 30 ms past the kick's attack, and crossfades with the kick's
    // slice: no stretch, no dip, the kick's attack played once.
    let cut = Cut {
        lead: 0.005,
        guard: 0.040,
    };
    let hits = [hit(1.0, 0.0), hit(1.06, 0.02)];
    let fill = Fill::default();
    let out = place_filled(&hits, (0.995, 2.0), cut, (None, None), (&[], fill));
    sound(&out, &hits, fill);
    let f = &out.fills[0];
    assert_eq!(f.stretched, None, "{f:?}");
    assert!(near(f.left, 0.0));
    // The kick plays to 1.055; the snare could reach 1.06 by the guard,
    // replays from 1.05 (crossfading the last 5 ms of the kick).
    assert!(near(out.pieces[1].start, 1.05), "{:?}", out.pieces);
    assert!(near(f.replayed_decay, 0.010), "{f:?}");
    assert!(near(f.crossfade, 0.005), "{f:?}");
    assert!(out.pieces[1].start - out.pieces[1].shift >= 1.0 + fill.keep - 1e-9);
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
fn unbreakables_slow_slices_play_no_slower_than_the_floor_and_no_hit_moves() {
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
            assert!(s.rate >= 0.5 - 1e-9, "{start}: {s:?}");
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

/// One of the 20 gaps Unbreakable's full edit (DRUMS_APPLY, replayed)
/// left silent: the hit after it (where it plays, where it was recorded),
/// the gap and the silence the 0.3.0 fill reported (`left_ms`: the real
/// silence and the crossfade it was asked to reach too), the dip left now
/// (`now_ms`), and the hits around it as the engine sends them (recorded,
/// shift; seconds, rounded to 0.1 ms).
struct Case {
    at: f64,
    recorded: f64,
    gap_ms: f64,
    left_ms: f64,
    now_ms: f64,
    hits: &'static [(f64, f64)],
}

const UNBREAKABLE_GAPS: [Case; 20] = [
    Case {
        at: 89.7188,
        recorded: 89.5847,
        gap_ms: 34.5,
        left_ms: 9.9,
        now_ms: 0.0,
        hits: &[
            (89.1864, 0.0636),
            (89.3203, 0.0234),
            (89.5341, 0.0996),
            (89.5847, 0.134),
            (89.7484, 0.0641),
            (89.8586, 0.0477),
        ],
    },
    Case {
        at: 120.75,
        recorded: 120.7301,
        gap_ms: 24.4,
        left_ms: 15.4,
        now_ms: 8.2,
        hits: &[
            (120.4716, 0.0284),
            (120.6031, 0.0219),
            (120.692, -0.0045),
            (120.7301, 0.0199),
            (120.8695, 0.0055),
            (120.9413, -0.0038),
        ],
    },
    Case {
        at: 136.5,
        recorded: 136.4081,
        gap_ms: 34.8,
        left_ms: 2.3,
        now_ms: 0.0,
        hits: &[
            (135.8933, 0.0442),
            (136.2699, 0.0426),
            (136.3511, 0.0571),
            (136.4081, 0.0919),
            (137.0339, 0.0286),
            (137.2281, 0.0219),
        ],
    },
    Case {
        at: 150.1875,
        recorded: 150.155,
        gap_ms: 35.1,
        left_ms: 0.6,
        now_ms: 0.0,
        hits: &[
            (149.8229, -0.0104),
            (150.0121, -0.0121),
            (150.0964, -0.0027),
            (150.155, 0.0325),
            (150.3526, 0.0224),
            (150.4394, 0.0293),
        ],
    },
    Case {
        at: 151.125,
        recorded: 151.1184,
        gap_ms: 36.8,
        left_ms: 4.3,
        now_ms: 0.0,
        hits: &[
            (150.8414, 0.0023),
            (150.9393, -0.0018),
            (151.0614, -0.0301),
            (151.1184, 0.0066),
            (151.2216, -0.0028),
            (151.301, 0.0115),
        ],
    },
    Case {
        at: 160.4063,
        recorded: 160.3023,
        gap_ms: 34.1,
        left_ms: 14.5,
        now_ms: 0.8,
        hits: &[
            (159.9033, 0.0342),
            (160.0504, -0.0191),
            (160.2557, 0.0699),
            (160.3023, 0.1039),
            (160.467, 0.033),
            (160.6048, 0.0827),
        ],
    },
    Case {
        at: 162.375,
        recorded: 162.3619,
        gap_ms: 39.0,
        left_ms: 44.0,
        now_ms: 39.0,
        hits: &[
            (162.1581, -0.0331),
            (162.2697, -0.0197),
            (162.3384, -0.0259),
            (162.3619, 0.0131),
            (162.4313, 0.0062),
            (162.5151, -0.0151),
        ],
    },
    Case {
        at: 181.2188,
        recorded: 181.2228,
        gap_ms: 36.6,
        left_ms: 3.9,
        now_ms: 0.0,
        hits: &[
            (180.9638, 0.0363),
            (181.0315, -0.0002),
            (181.1656, -0.0406),
            (181.2228, -0.004),
            (181.4864, 0.0136),
            (181.5645, 0.0292),
        ],
    },
    Case {
        at: 185.2832,
        recorded: 185.2832,
        gap_ms: 17.6,
        left_ms: 22.6,
        now_ms: 17.6,
        hits: &[
            (185.0977, -0.0352),
            (185.1792, -0.023),
            (185.268, -0.0176),
            (185.2832, 0.0),
            (185.3677, -0.0239),
            (185.4606, -0.0231),
        ],
    },
    Case {
        at: 192.9375,
        recorded: 192.8885,
        gap_ms: 43.6,
        left_ms: 19.6,
        now_ms: 3.1,
        hits: &[
            (192.7438, -0.0102),
            (192.7602, -0.0102),
            (192.8383, 0.0055),
            (192.8885, 0.049),
            (192.9686, 0.0626),
            (193.0876, -0.0001),
        ],
    },
    Case {
        at: 211.7824,
        recorded: 211.8348,
        gap_ms: 43.3,
        left_ms: 17.4,
        now_ms: 0.1,
        hits: &[
            (211.292, 0.0205),
            (211.496, 0.004),
            (211.7832, -0.0957),
            (211.8348, -0.0524),
            (211.8846, -0.0096),
            (212.0661, -0.0036),
        ],
    },
    Case {
        at: 211.875,
        recorded: 211.8846,
        gap_ms: 42.8,
        left_ms: 19.2,
        now_ms: 3.2,
        hits: &[
            (211.496, 0.004),
            (211.7832, -0.0957),
            (211.8348, -0.0524),
            (211.8846, -0.0096),
            (212.0661, -0.0036),
            (212.4338, 0.0037),
        ],
    },
    Case {
        at: 214.125,
        recorded: 214.0661,
        gap_ms: 52.2,
        left_ms: 38.9,
        now_ms: 29.2,
        hits: &[
            (213.8452, -0.0015),
            (213.9363, 0.0012),
            (214.0246, 0.0067),
            (214.0661, 0.0589),
            (214.4142, -0.0079),
            (214.5219, -0.0219),
        ],
    },
    Case {
        at: 216.0,
        recorded: 215.9822,
        gap_ms: 54.8,
        left_ms: 44.8,
        now_ms: 37.0,
        hits: &[
            (215.4656, -0.0281),
            (215.5878, 0.0372),
            (215.9433, -0.037),
            (215.9822, 0.0178),
            (216.1691, 0.0184),
            (216.2624, 0.0189),
        ],
    },
    Case {
        at: 222.75,
        recorded: 222.7341,
        gap_ms: 30.9,
        left_ms: 29.3,
        now_ms: 24.3,
        hits: &[
            (222.5714, -0.0089),
            (222.626, -0.001),
            (222.7025, -0.015),
            (222.7341, 0.0159),
            (222.7944, 0.0181),
            (222.8203, 0.0181),
        ],
    },
    Case {
        at: 240.8438,
        recorded: 240.7997,
        gap_ms: 55.3,
        left_ms: 47.8,
        now_ms: 41.3,
        hits: &[
            (240.2116, -0.0241),
            (240.6035, -0.041),
            (240.7627, -0.0112),
            (240.7997, 0.0441),
            (240.8943, 0.0432),
            (241.0411, -0.0098),
        ],
    },
    Case {
        at: 253.5938,
        recorded: 253.5054,
        gap_ms: 46.9,
        left_ms: 27.0,
        now_ms: 13.1,
        hits: &[
            (253.2723, 0.0402),
            (253.3735, 0.0328),
            (253.4585, 0.0415),
            (253.5054, 0.0884),
            (253.6592, 0.0283),
            (253.7609, 0.0204),
        ],
    },
    Case {
        at: 254.6508,
        recorded: 254.6989,
        gap_ms: 0.9,
        left_ms: 5.9,
        now_ms: 0.9,
        hits: &[
            (254.4854, -0.0479),
            (254.5693, -0.038),
            (254.6739, -0.0489),
            (254.6989, -0.048),
            (254.7692, -0.0504),
            (254.8784, -0.0659),
        ],
    },
    Case {
        at: 264.7506,
        recorded: 264.7524,
        gap_ms: 0.6,
        left_ms: 5.5,
        now_ms: 0.5,
        hits: &[
            (264.6338, 0.0135),
            (264.6428, 0.0135),
            (264.7273, -0.0024),
            (264.7524, -0.0018),
            (264.7917, 0.0165),
            (264.8272, 0.0165),
        ],
    },
    Case {
        at: 264.8083,
        recorded: 264.7917,
        gap_ms: 18.4,
        left_ms: 8.0,
        now_ms: 0.0,
        hits: &[
            (264.6428, 0.0135),
            (264.7273, -0.0024),
            (264.7524, -0.0018),
            (264.7917, 0.0165),
            (264.8272, 0.0165),
            (264.8987, 0.0183),
        ],
    },
];

/// The cut and fill the engine sends: 5 ms lead, a 5 ms crossfade, the
/// guard 15 ms and the crossfade.
const ENGINE_CUT: Cut = Cut {
    lead: 0.005,
    guard: 0.020,
};

fn engine_fill() -> Fill {
    Fill {
        crossfade: 0.005,
        ..Fill::default()
    }
}

/// A case placed as the engine places it, and its fill.
fn unbreakable_gap(case: &Case) -> (Placed, Filled) {
    let hits: Vec<Hit> = case
        .hits
        .iter()
        .map(|&(at, shift)| hit(at, shift))
        .collect();
    let first = hits[0];
    let last = hits[hits.len() - 1];
    let span = (
        first.at + first.shift - ENGINE_CUT.lead,
        last.at + last.shift + 0.05,
    );
    let out = place_filled(&hits, span, ENGINE_CUT, (None, None), (&[], engine_fill()));
    sound(&out, &hits, engine_fill());
    let fill = out
        .fills
        .iter()
        .find(|f| (f.recorded - case.recorded).abs() < 1e-6)
        .cloned()
        .unwrap_or_else(|| panic!("{}: no fill: {:?}", case.at, out.fills));
    (out, fill)
}

#[test]
fn unbreakables_gaps_are_rebuilt_as_the_engine_sent_them() {
    for case in &UNBREAKABLE_GAPS {
        let (_, fill) = unbreakable_gap(case);
        assert!(
            (fill.gap * 1e3 - case.gap_ms).abs() < 0.25,
            "{}: gap {} ms, was {}",
            case.at,
            fill.gap * 1e3,
            case.gap_ms
        );
        assert!((fill.at - case.at).abs() < 2e-4, "{}: {fill:?}", case.at);
    }
}

#[test]
fn unbreakables_gaps_report_the_silence_they_leave_never_more_than_the_gap() {
    // 0.3.0 said 44 ms left at 162.375 over a 39 ms gap (and 22.6 over
    // 17.6, 5.9 over 0.9, 5.5 over 0.6): it counted the crossfade the
    // stretch was asked to reach as silence. What is left is the dip
    // between the two pieces' audio, never more than the gap.
    for case in &UNBREAKABLE_GAPS {
        let (out, fill) = unbreakable_gap(case);
        let starts = out
            .pieces
            .iter()
            .position(|p| {
                near(
                    p.start - p.shift,
                    case.recorded - fill.reached_back - ENGINE_CUT.lead,
                )
            })
            .unwrap_or_else(|| panic!("{}: {:?}", case.at, out.pieces));
        let (before, after) = (&out.pieces[starts - 1], &out.pieces[starts]);
        let silence = (after.start - before.reach()).max(0.0);
        assert!(near(fill.left, silence), "{}: {fill:?}", case.at);
        assert!(fill.left <= fill.gap + 1e-9, "{}: {fill:?}", case.at);
        assert!(
            fill.left * 1e3 <= (case.left_ms - 5.0).max(0.0) + 0.3,
            "{}: {fill:?}",
            case.at
        );
        // A dip is never a hard cut: each side fades within itself.
        if fill.left > 0.0 {
            assert_eq!(after.crossfade, Some(0.0), "{}", case.at);
        }
    }
}

#[test]
fn unbreakables_gaps_are_filled_by_the_owners_rule() {
    // The previous stroke's decay replayed (none here: the engine's 20 ms
    // guard already reaches past the 30 ms keep), then what is left
    // stretched down to 0.5 over the before's decay past its keep, then a
    // dip: 381 ms reported over 20 gaps (281 ms of real silence) is now
    // 218 ms over 14, 6 of the gaps filled whole.
    let mut left = 0.0;
    for case in &UNBREAKABLE_GAPS {
        let (_, fill) = unbreakable_gap(case);
        assert!(
            (fill.left * 1e3 - case.now_ms).abs() < 0.15,
            "{}: left {} ms, not {}: {fill:?}",
            case.at,
            fill.left * 1e3,
            case.now_ms
        );
        assert!(fill.replayed_decay.abs() < 1e-9, "{}: {fill:?}", case.at);
        if let Some(s) = fill.stretched {
            assert!(s.rate >= 0.5 - 1e-9, "{}: {s:?}", case.at);
        }
        left += fill.left;
    }
    assert!((left * 1e3 - 218.4).abs() < 1.0, "{} ms left", left * 1e3);
}
