//! The apply's solve, the change lists' matching, bar.beat addresses and
//! the fixes' resolution: everything decided before the song is touched.

use super::{Edit, Limits, MIN_RATE, Stroke, farthest, rate, solve, synthetic};
use crate::edit_list::{EditList, FORMAT, ListHit, format_at, match_list, parse_at, take_of};
use crate::hit_ops::{FixKind, To, named_at, resolve};

fn stroke(id: &str, piece: &str, now: f64) -> Stroke {
    Stroke {
        id: id.to_owned(),
        piece: piece.to_owned(),
        now,
        recorded: now,
        peak_db: -6.0,
        flam_with: None,
    }
}

fn targets(pairs: &[(&str, f64)]) -> Edit {
    Edit {
        targets: pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
        ..Edit::default()
    }
}

fn by_of(s: &super::Solution, id: &str) -> f64 {
    let k = s.strokes.iter().position(|x| x.id == id).expect("a stroke");
    s.moments
        .iter()
        .find(|m| m.hits.contains(&k))
        .expect("its moment")
        .by
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_run_of_close_strokes_moves_as_one_set_whatever_the_order() {
    // Fill 1 (bar 44): a run of close strokes all taken 15 ms earlier.
    // One at a time earliest first corrupted it; solved as a set, each
    // goes where it was sent and nothing conflicts.
    let run: Vec<Stroke> = (0..8)
        .map(|i| stroke(&format!("t{i}"), "Tom 1", 10.0 + f64::from(i) * 0.047))
        .collect();
    let edit = targets(
        &run.iter()
            .rev()
            .map(|s| (s.id.as_str(), s.now - 0.015))
            .collect::<Vec<_>>(),
    );
    let s = solve(&run, &edit, Limits::default());
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    for st in &run {
        assert!(approx(by_of(&s, &st.id), -0.015));
    }
    assert_eq!(s.moved(), 8);
}

#[test]
fn a_move_and_its_reverse_are_no_move() {
    let hits = vec![stroke("k", "Kick", 2.0), stroke("s", "Snare", 2.2)];
    // +5 ms then −5 ms, said as where it ends: where it is.
    let s = solve(&hits, &targets(&[("k", 2.0)]), Limits::default());
    assert_eq!(s.moved(), 0);
    assert!(s.conflicts.is_empty());
}

#[test]
fn sending_a_hit_onto_another_is_refused_unless_a_ride_or_delete_covers_it() {
    let hits = vec![stroke("k", "Kick", 5.0), stroke("s", "Snare", 5.06)];
    let onto = targets(&[("s", 5.004)]);
    let s = solve(&hits, &onto, Limits::default());
    assert!(
        s.conflicts.iter().any(|c| c.kind == "swallow"),
        "{:?}",
        s.conflicts
    );
    // Deleting the kick covers it: nothing is swallowed.
    let mut covered = onto.clone();
    covered.delete.insert("k".to_owned());
    let s = solve(&hits, &covered, Limits::default());
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    // Riding the kick: the snare keeps its played distance instead.
    let mut ride = onto;
    ride.rides.insert("s".to_owned(), "k".to_owned());
    let s = solve(&hits, &ride, Limits::default());
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    assert!(approx(by_of(&s, "s"), 0.0));
}

#[test]
fn a_hit_sent_past_another_is_an_order_conflict() {
    let hits = vec![stroke("a", "Kick", 1.0), stroke("b", "Snare", 1.1)];
    let s = solve(&hits, &targets(&[("a", 1.2)]), Limits::default());
    assert!(s.conflicts.iter().any(|c| c.kind == "order"));
}

#[test]
fn two_hits_of_one_drum_on_one_line_are_refused() {
    let hits = vec![
        stroke("k1", "Kick", 3.0),
        stroke("h", "Hat", 3.03),
        stroke("k2", "Kick", 3.05),
    ];
    let s = solve(&hits, &targets(&[("k1", 3.045)]), Limits::default());
    assert!(
        s.conflicts.iter().any(|c| c.kind == "line"),
        "{:?}",
        s.conflicts
    );
}

#[test]
fn hits_struck_together_sent_apart_conflict_unless_one_rides() {
    let hits = vec![stroke("k", "Kick", 4.0), stroke("s", "Snare", 4.008)];
    let apart = targets(&[("k", 4.01), ("s", 3.99)]);
    let s = solve(&hits, &apart, Limits::default());
    assert!(s.conflicts.iter().any(|c| c.kind == "split"));
    let mut ride = apart;
    ride.rides.insert("k".to_owned(), "s".to_owned());
    let s = solve(&hits, &ride, Limits::default());
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    assert!(approx(by_of(&s, "k"), 3.99 - 4.008));
}

#[test]
fn a_grace_note_rides_its_main_stroke() {
    // Bar 120: a grace 35 ms before its main stroke, a moment of its own.
    let mut grace = stroke("g", "Snare", 6.0);
    grace.peak_db = -20.0;
    grace.flam_with = Some("m".to_owned());
    let hits = vec![grace, stroke("m", "Snare", 6.035)];
    let s = solve(&hits, &targets(&[("m", 6.045)]), Limits::default());
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    assert!(approx(by_of(&s, "g"), 0.010), "the grace moves with it");
}

#[test]
fn a_unison_partner_rides_its_leader() {
    let hits = vec![stroke("s", "Snare", 7.0), stroke("k", "Kick", 7.025)];
    let mut edit = targets(&[("s", 6.99)]);
    edit.rides.insert("k".to_owned(), "s".to_owned());
    let s = solve(&hits, &edit, Limits::default());
    assert!(approx(by_of(&s, "k"), -0.010));
}

#[test]
fn no_slice_is_drawn_out_slower_than_the_limit_what_follows_moves_up() {
    // Bar 128: neighbours moved apart drew a slice out to 0.11×. A
    // sixteenth's slice (0.1 s) may reach no further than `farthest`.
    let hits = vec![
        stroke("a", "Snare", 1.0),
        stroke("b", "Kick", 1.1),
        stroke("c", "Kick", 1.3),
    ];
    let s = solve(&hits, &targets(&[("a", 0.95)]), Limits::default());
    let most = farthest(0.1);
    assert!(approx(by_of(&s, "b"), (0.95 + most) - 1.1), "b moved up");
    assert_eq!(s.moved_up.len(), 1);
    assert_eq!(s.moved_up[0].id, "b");
    // Every slice plays no slower than the limit.
    let lands: Vec<f64> = s.moments.iter().map(super::Moment::lands).collect();
    for (w, m) in lands.windows(2).zip(s.moments.windows(2)) {
        let natural = m[1].recorded - m[0].recorded;
        assert!(rate(natural, w[1] - w[0]) >= MIN_RATE - 1e-9);
    }
    assert!(s.drawn.iter().all(|(_, r)| *r >= MIN_RATE - 1e-3));
}

#[test]
fn a_slice_cut_short_plays_at_its_own_speed() {
    assert!(approx(rate(0.1, 0.05), 1.0));
    assert!(rate(0.1, 0.11) < 1.0);
    assert!(approx(rate(0.1, farthest(0.1)), MIN_RATE));
}

#[test]
fn a_deleted_stroke_goes_and_the_slice_before_reaches_the_next() {
    let hits = vec![
        stroke("a", "Kick", 1.0),
        stroke("d", "Kick", 1.05),
        stroke("b", "Snare", 1.1),
    ];
    let mut edit = Edit::default();
    edit.delete.insert("d".to_owned());
    let s = solve(&hits, &edit, Limits::default());
    assert!(s.conflicts.is_empty());
    let d = s.moments.iter().find(|m| m.deleted).expect("deleted");
    assert!(approx(d.now, 1.05));
    // The kick's slice runs to the deleted attack (50 ms) and is drawn to
    // the snare (100 ms): past the limit, so the snare is moved up.
    assert_eq!(s.moved_up.len(), 1);
    assert!(approx(by_of(&s, "b"), 1.0 + farthest(0.05) - 1.1));
    // Its cut stays, to cut it out after.
    assert!(s.cut_on().contains(&1.05));
}

#[test]
fn a_stroke_deleted_beside_one_struck_with_it_conflicts() {
    let hits = vec![stroke("k", "Kick", 1.0), stroke("s", "Snare", 1.005)];
    let mut edit = Edit::default();
    edit.delete.insert("k".to_owned());
    let s = solve(&hits, &edit, Limits::default());
    assert!(s.conflicts.iter().any(|c| c.kind == "partial-delete"));
}

#[test]
fn a_removed_marker_is_no_cut_and_unknown_ids_are_said() {
    let hits = vec![stroke("a", "Kick", 1.0), stroke("x", "Kick", 1.2)];
    let mut edit = targets(&[("nope", 1.0)]);
    edit.remove.insert("x".to_owned());
    let s = solve(&hits, &edit, Limits::default());
    assert_eq!(s.unknown, vec!["nope".to_owned()]);
    assert_eq!(s.cut_on(), vec![1.0]);
}

#[test]
fn a_whole_song_solves_well_inside_a_second() {
    let (strokes, edit) = synthetic(1_400);
    assert_eq!(strokes.len(), 1_400);
    let t = std::time::Instant::now();
    let s = solve(&strokes, &edit, Limits::default());
    let took = t.elapsed();
    assert!(s.moved() > 500, "moved {}", s.moved());
    assert!(
        took.as_millis() < 250,
        "1,400 hits solved in {took:?}: the solve is the apply's cheap part"
    );
    assert!(
        s.conflicts.is_empty(),
        "{:?}",
        s.conflicts.iter().take(5).collect::<Vec<_>>()
    );
}

// ── change lists ──────────────────────────────────────────────────────

fn list(hits: Vec<ListHit>) -> EditList {
    EditList {
        format: FORMAT.to_owned(),
        song: String::new(),
        session: String::new(),
        made: String::new(),
        about: String::new(),
        tempo: None,
        sections: Vec::new(),
        hits,
    }
}

fn entry(piece: &str, take: &str, recorded: f64, to: f64) -> ListHit {
    ListHit {
        piece: piece.to_owned(),
        take: take.to_owned(),
        recorded,
        to,
        at: String::new(),
    }
}

#[test]
fn a_list_matches_by_drum_take_and_recorded_time() {
    let strokes = vec![
        stroke("abc.kick.0001@0~e0a36f", "Kick", 12.9166),
        stroke("abc.kick.0002@0~e0a36f", "Kick", 12.9196),
        stroke("abc.kick.0003@0~420134", "Kick", 12.9166),
        stroke("abc.snare.0004@0~e0a36f", "Snare", 13.0),
        stroke("abc.kick.0005@0~e0a36f", "Kick", 20.0),
    ];
    let l = list(vec![
        // 1.4 ms from 0001, 1.6 from 0002: the nearest pairs first.
        entry("Kick", "e0a36f", 12.918, 12.92),
        entry("kick", "e0a36f", 12.9198, 12.925),
        entry("Kick", "420134", 12.9175, 12.91),
        entry("Snare", "e0a36f", 13.0025, 13.0),
        entry("Tom 1", "e0a36f", 13.0, 13.0),
    ]);
    let m = match_list(&l, &strokes);
    assert_eq!(m.targets.get("abc.kick.0002@0~e0a36f"), Some(&12.925));
    assert_eq!(m.targets.get("abc.kick.0001@0~e0a36f"), Some(&12.92));
    assert_eq!(m.targets.get("abc.kick.0003@0~420134"), Some(&12.91));
    // 2.5 ms away: no match; a drum the takes have no hit of: none.
    assert_eq!(m.unmatched.len(), 2);
    // Detected and not in the list: removed.
    assert_eq!(
        m.removed,
        vec![
            "abc.snare.0004@0~e0a36f".to_owned(),
            "abc.kick.0005@0~e0a36f".to_owned()
        ]
    );
    assert_eq!(take_of("abc.kick.0005@0~e0a36f"), "e0a36f");
    assert_eq!(take_of("old-id"), "");
}

#[test]
fn a_list_reads_back_what_it_wrote() {
    let l = list(vec![entry("Kick", "e0a36f", 1.0, 1.01)]);
    let text = serde_json::to_string(&l).expect("json");
    let back: EditList = serde_json::from_str(&text).expect("read");
    assert_eq!(back, l);
}

#[test]
fn bar_beat_addresses_parse_as_the_owner_writes_them() {
    let at = parse_at("118.4.25").expect("parse");
    assert_eq!((at.bar, at.beat), (118, 4));
    assert!(approx(at.fraction, 0.25));
    let at = parse_at("150.1").expect("parse");
    assert_eq!((at.bar, at.beat, at.fraction), (150, 1, 0.0));
    assert!(approx(parse_at("8.3.444").expect("parse").fraction, 0.444));
    assert!(approx(parse_at("118.4.5").expect("parse").fraction, 0.5));
    assert_eq!(parse_at("150").expect("parse").beat, 1);
    for bad in ["", "x.1", "1.0", "1.2.x", "1.2.-5"] {
        assert!(parse_at(bad).is_err(), "{bad:?}");
    }
    assert_eq!(format_at(118, 4, 0.25), "118.4.25");
    assert_eq!(format_at(150, 1, 0.0), "150.1");
    assert_eq!(format_at(8, 3, 0.4444), "8.3.444");
    assert_eq!(format_at(8, 3, 0.9999), "8.3");
    for text in ["118.4.25", "150.1", "8.3.444", "1.2.05"] {
        let a = parse_at(text).expect("parse");
        assert_eq!(format_at(a.bar, a.beat, a.fraction), text);
    }
}

// ── fixes by address ──────────────────────────────────────────────────

#[test]
fn an_address_names_the_hit_playing_nearest_and_what_is_struck_with_it() {
    let strokes = vec![
        stroke("k", "Kick", 10.0),
        stroke("s", "Snare", 10.006),
        stroke("h", "Hat", 10.2),
    ];
    let (near, with) = named_at(&strokes, 10.03, None).expect("found");
    assert_eq!(near, 1);
    assert_eq!(with, vec![0, 1]);
    assert!(named_at(&strokes, 10.1, Some("Tom 1")).is_err());
    assert!(named_at(&strokes, 11.0, None).is_err());
}

#[test]
fn a_fix_moves_the_whole_moment_or_one_drum_with_the_rest_riding() {
    let strokes = vec![stroke("k", "Kick", 10.0), stroke("s", "Snare", 10.006)];
    let all = [(
        "118.4.25 → 118.4.5".to_owned(),
        None,
        FixKind::Move {
            at: 10.0,
            to: To::At(10.02),
        },
    )];
    let (arg, resolved) = resolve(&strokes, &all).expect("resolved");
    assert_eq!(arg.targets.get("k"), Some(&10.02));
    assert!(approx(arg.targets.get("s").copied().unwrap_or(0.0), 10.026));
    assert_eq!(resolved[0].ids.len(), 2);

    let snare = [(
        "a".to_owned(),
        Some("Snare"),
        FixKind::Move {
            at: 10.0,
            to: To::By(-4.0),
        },
    )];
    let (arg, _) = resolve(&strokes, &snare).expect("resolved");
    assert!(approx(arg.targets.get("s").copied().unwrap_or(0.0), 10.002));
    assert_eq!(arg.rides.get("k").map(String::as_str), Some("s"));

    let gone = [("150.1".to_owned(), None, FixKind::Remove(10.0))];
    let (arg, _) = resolve(&strokes, &gone).expect("resolved");
    assert_eq!(arg.remove.len(), 2);
}

#[test]
fn the_bench_reads_a_plans_targets_either_way() {
    let map = serde_json::json!({"targets": {"a": 1.0}});
    assert_eq!(
        crate::drum_bench::plan_targets(&map).map(|m| m.len()),
        Some(1)
    );
    let cols = serde_json::json!({"ids": ["a", "b"], "targets": [1.0, 2.0]});
    assert_eq!(
        crate::drum_bench::plan_targets(&cols).map(|m| m.len()),
        Some(2)
    );
    let s = crate::drum_bench::summary("apply", &[3.0, 1.0, 2.0]);
    assert_eq!((s.min_ms, s.median_ms, s.max_ms), (1.0, 2.0, 3.0));
}
