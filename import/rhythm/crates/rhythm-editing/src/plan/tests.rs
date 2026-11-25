use daw_edit::PlanRequest;
use daw_edit::plan::{
    BarLine, Heard, Note, Plan, PlayedHit, Reason, Section, Settings, Zone, ZoneKind,
};

use super::plan;

/// 160 bpm, 4/4, bar 1 at 1.5 s: a beat is 0.375 s, a 16th 93.75 ms.
const BEAT: f64 = 0.375;

fn bars(n: i32) -> Vec<BarLine> {
    (1..=n)
        .map(|bar| BarLine {
            bar,
            at: 1.5 + f64::from(bar - 1) * 4.0 * BEAT,
            beats: 4,
            unit: 4,
            beat: BEAT,
        })
        .collect()
}

/// Bar `bar`, beat `beat` (1-based, fractional) in seconds.
fn at(bar: i32, beat: f64) -> f64 {
    1.5 + f64::from(bar - 1) * 4.0 * BEAT + (beat - 1.0) * BEAT
}

fn hit(id: &str, piece: &str, time: f64) -> PlayedHit {
    PlayedHit {
        id: id.to_owned(),
        piece: piece.to_owned(),
        take: "t1".to_owned(),
        time,
        peak_db: -20.0,
        heard: Heard::Both,
        flags: Vec::new(),
    }
}

fn note(piece: &str, time: f64) -> Note {
    Note {
        time,
        piece: piece.to_owned(),
        velocity: 100,
    }
}

fn request(hits: Vec<PlayedHit>, notes: Vec<Note>) -> PlanRequest {
    PlanRequest {
        hits,
        notes,
        bars: bars(8),
        sections: vec![Section {
            name: "VS".into(),
            from: 1.5,
            to: at(9, 1.0),
        }],
        zones: Vec::new(),
        settings: Settings::default(),
    }
}

fn target(plan: &Plan, id: &str) -> f64 {
    plan.hits
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.target)
        .expect(id)
}

fn reason(plan: &Plan, id: &str) -> Reason {
    plan.hits
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.reason.clone())
        .expect(id)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.0005
}

/// A backbeat played `lean` late, with 8th-note kicks.
fn groove(bar: i32, lean: f64, out: &mut Vec<PlayedHit>) {
    for (i, b) in [1.0, 1.5, 2.5, 3.0, 3.5, 4.5].iter().enumerate() {
        out.push(hit(&format!("k{bar}.{i}"), "Kick", at(bar, *b) + lean));
    }
    out.push(hit(&format!("s{bar}.2"), "Snare", at(bar, 2.0) + lean));
    out.push(hit(&format!("s{bar}.4"), "Snare", at(bar, 4.0) + lean));
}

#[test]
fn a_late_bar_is_corrected_before_its_lines_are_chosen() {
    // 55 ms late: past half a 16th, so the nearest line is the next one.
    let mut hits = Vec::new();
    for bar in 1..=4 {
        groove(bar, 0.055, &mut hits);
    }
    let p = plan(&request(hits, Vec::new()));
    assert!(
        close(target(&p, "s2.2"), at(2, 2.0)),
        "snare on 2, not the 16th after"
    );
    assert!(close(target(&p, "k3.1"), at(3, 1.5)));
    let lean = p.leans.iter().find(|l| l.bar == 2 && l.half == 0).unwrap();
    assert!((lean.lean - 0.055).abs() < 0.006, "lean {}", lean.lean);
}

#[test]
fn the_lean_changes_bar_to_bar() {
    let mut hits = Vec::new();
    groove(1, -0.040, &mut hits);
    groove(2, -0.040, &mut hits);
    groove(3, 0.060, &mut hits);
    groove(4, 0.060, &mut hits);
    let p = plan(&request(hits, Vec::new()));
    for bar in 1..=4 {
        assert!(
            close(target(&p, &format!("s{bar}.4")), at(bar, 4.0)),
            "bar {bar}"
        );
        assert!(
            close(target(&p, &format!("k{bar}.5")), at(bar, 4.5)),
            "bar {bar}"
        );
    }
}

#[test]
fn half_time_anchors_on_three() {
    let mut hits = Vec::new();
    for bar in 1..=4 {
        hits.push(hit(&format!("k{bar}"), "Kick", at(bar, 1.0) + 0.050));
        hits.push(hit(&format!("s{bar}"), "Snare", at(bar, 3.0) + 0.050));
        hits.push(hit(&format!("k{bar}b"), "Kick", at(bar, 2.75) + 0.050));
    }
    let p = plan(&request(hits, Vec::new()));
    for bar in 1..=4 {
        assert!(close(target(&p, &format!("s{bar}")), at(bar, 3.0)));
        assert!(close(target(&p, &format!("k{bar}b")), at(bar, 2.75)));
    }
}

#[test]
fn a_stroke_goes_to_its_written_note_one_to_one() {
    // Two kicks a 16th apart, one note written: the first takes the note,
    // the played double goes a 16th after it, not onto the same line.
    let hits = vec![
        hit("a", "Kick", at(2, 1.5) + 0.010),
        hit("b", "Kick", at(2, 1.75) + 0.005),
        hit("s", "Snare", at(2, 2.0)),
    ];
    let notes = vec![note("Kick", at(2, 1.5)), note("Snare", at(2, 2.0))];
    let p = plan(&request(hits, notes));
    assert!(close(target(&p, "a"), at(2, 1.5)));
    assert_eq!(reason(&p, "a"), Reason::Midi);
    assert!(close(target(&p, "b"), at(2, 1.75)));
    assert_ne!(target(&p, "a"), target(&p, "b"));
}

#[test]
fn the_written_note_wins_over_a_nearer_line_within_reach() {
    // Played 60 ms early of its note, which sits a 16th after the nearest
    // line: the note, as the chart says.
    let hits = vec![hit("k", "Kick", at(3, 2.5) - 0.060)];
    let notes = vec![note("Kick", at(3, 2.5))];
    let p = plan(&request(hits, notes));
    assert!(close(target(&p, "k"), at(3, 2.5)));
}

#[test]
fn strokes_never_cross() {
    let hits = vec![
        hit("a", "Kick", at(2, 1.0) + 0.040),
        hit("b", "Kick", at(2, 1.0) + 0.060),
        hit("c", "Kick", at(2, 1.25) + 0.010),
    ];
    let p = plan(&request(hits, Vec::new()));
    let (a, b, c) = (target(&p, "a"), target(&p, "b"), target(&p, "c"));
    assert!(a < b && b < c, "{a} {b} {c}");
}

#[test]
fn unisons_ride_their_lead() {
    let hits = vec![
        // Snare first, kick 15 ms after: the snare leads.
        hit("s1", "Snare", at(2, 2.0) + 0.012),
        hit("k1", "Kick", at(2, 2.0) + 0.027),
        // Kick 5 ms before the snare: struck together, the kick leads.
        hit("k2", "Kick", at(2, 4.0) - 0.020),
        hit("s2", "Snare", at(2, 4.0) - 0.015),
        // A kick 35 ms under a snare: it rides the snare, just before.
        hit("k3", "Kick", at(3, 2.0) - 0.030),
        hit("s3", "Snare", at(3, 2.0) + 0.005),
    ];
    let p = plan(&request(hits, Vec::new()));
    assert!(close(target(&p, "s1"), at(2, 2.0)));
    assert_eq!(reason(&p, "k1"), Reason::Rides("s1".into()));
    assert!(close(target(&p, "k1"), at(2, 2.0) + 0.015));
    assert!(close(target(&p, "k2"), at(2, 4.0)));
    assert_eq!(p.rides.get("s2").map(String::as_str), Some("k2"));
    assert!(close(target(&p, "s3"), at(3, 2.0)));
    assert!(close(target(&p, "k3"), at(3, 2.0) - 0.035));
    assert!(
        !p.targets.contains_key("k3"),
        "a rider has no cut of its own"
    );
}

#[test]
fn flams_and_graces_ride() {
    let mut grace = hit("g", "Snare", at(2, 2.0) - 0.040);
    grace.peak_db = -32.0;
    let hits = vec![
        grace,
        hit("main", "Snare", at(2, 2.0) + 0.004),
        // A same-drum stroke 20 ms after: the flam's second stroke.
        hit("f1", "Tom 1", at(2, 3.0) + 0.006),
        hit("f2", "Tom 1", at(2, 3.0) + 0.026),
    ];
    let p = plan(&request(hits, Vec::new()));
    assert_eq!(reason(&p, "g"), Reason::FlamGrace("main".into()));
    assert!(close(target(&p, "g"), at(2, 2.0) - 0.044));
    assert_eq!(reason(&p, "f2"), Reason::Rides("f1".into()));
    assert!(close(target(&p, "f2"), at(2, 3.0) + 0.020));
}

#[test]
fn a_seam_double_rides_the_first_take() {
    let mut new = hit("new", "Kick", at(3, 1.0) + 0.008);
    new.take = "t2".into();
    let hits = vec![hit("old", "Kick", at(3, 1.0) - 0.004), new, {
        let mut h = hit("next", "Kick", at(3, 1.5));
        h.take = "t2".into();
        h
    }];
    let p = plan(&request(hits, Vec::new()));
    assert_eq!(reason(&p, "new"), Reason::SeamDouble);
    assert_eq!(p.rides.get("new").map(String::as_str), Some("old"));
    assert!(close(target(&p, "old"), at(3, 1.0)));
}

#[test]
fn a_double_kick_run_takes_consecutive_sixteenths_in_order() {
    // Eight strokes, 30 ms late and uneven, the third and fourth close
    // enough that nearest-line would double one line and skip the next.
    let offs = [0.030, 0.040, 0.075, 0.020, 0.035, 0.028, 0.044, 0.031];
    let mut hits: Vec<PlayedHit> = offs
        .iter()
        .enumerate()
        .map(|(i, o)| hit(&format!("r{i}"), "Kick", at(4, 3.0 + 0.25 * i as f64) + o))
        .collect();
    hits.push(hit("s2", "Snare", at(4, 2.0) + 0.030));
    hits.push(hit("s4", "Snare", at(4, 4.0) + 0.030));
    let p = plan(&request(hits, Vec::new()));
    // The stroke under the snare on 4 rides it, just after.
    assert_eq!(reason(&p, "r4"), Reason::Rides("s4".into()));
    assert!(close(target(&p, "r4"), at(4, 4.0) + 0.005));
    for i in (0..offs.len()).filter(|i| *i != 4) {
        assert!(
            close(target(&p, &format!("r{i}")), at(4, 3.0 + 0.25 * i as f64)),
            "stroke {i} at {}",
            target(&p, &format!("r{i}"))
        );
    }
    assert!(
        close(target(&p, "s4"), at(4, 4.0)),
        "the bar's snares stay exact"
    );
}

#[test]
fn sixteenth_triplets_are_chosen_from_the_spacing() {
    // Six strokes a sixth of a beat apart (about 0.17 beat) on beat 3.
    let mut hits: Vec<PlayedHit> = (0..6)
        .map(|i| {
            hit(
                &format!("t{i}"),
                "Tom 1",
                at(5, 3.0 + f64::from(i) / 6.0) + 0.006,
            )
        })
        .collect();
    hits.push(hit("s2", "Snare", at(5, 2.0)));
    let p = plan(&request(hits, Vec::new()));
    for i in 0..6 {
        assert!(
            close(
                target(&p, &format!("t{i}")),
                at(5, 3.0 + f64::from(i) / 6.0)
            ),
            "stroke {i}"
        );
    }
}

#[test]
fn a_lone_triplet_note_is_the_chart_written_loosely() {
    // Unbreakable bar 61: the chart has a kick on the triplet, the kick
    // was played between; it goes to the 16th (1.75).
    let hits = vec![
        hit("k1", "Kick", at(2, 1.0)),
        hit("k", "Kick", at(2, 1.71)),
        hit("s", "Snare", at(2, 2.0)),
    ];
    let notes = vec![
        note("Kick", at(2, 1.0)),
        note("Kick", at(2, 1.0 + 2.0 / 3.0)),
        note("Snare", at(2, 2.0)),
    ];
    let p = plan(&request(hits, notes));
    assert!(close(target(&p, "k"), at(2, 1.75)), "{}", target(&p, "k"));
}

#[test]
fn a_triplet_figure_in_the_chart_is_kept() {
    // Two triplet notes in the bar (Unbreakable bar 78): the strokes go to
    // them, though a 16th is nearer one of them.
    let hits = vec![
        hit("a", "Kick", at(2, 3.62)),
        hit("b", "Kick", at(2, 4.28)),
        hit("s", "Snare", at(2, 2.0)),
    ];
    let notes = vec![
        note("Kick", at(2, 3.0 + 2.0 / 3.0)),
        note("Kick", at(2, 4.0 + 1.0 / 3.0)),
    ];
    let p = plan(&request(hits, notes));
    assert!(close(target(&p, "a"), at(2, 3.0 + 2.0 / 3.0)));
    assert!(close(target(&p, "b"), at(2, 4.0 + 1.0 / 3.0)));
}

#[test]
fn a_stroke_at_a_bar_line_leans_as_the_bar_it_lands_in() {
    // Unbreakable bars 115-116: bar 1 played 40 ms late, bar 2 20 ms
    // early; the kick just before bar 2 is its downbeat, not a 16th early.
    let mut hits = Vec::new();
    groove(1, 0.040, &mut hits);
    for (i, b) in [1.5, 1.75, 2.5, 2.75, 3.5].iter().enumerate() {
        hits.push(hit(&format!("n{i}"), "Kick", at(2, *b) - 0.020));
    }
    hits.push(hit("s2", "Snare", at(2, 2.0) - 0.020));
    hits.push(hit("s4", "Snare", at(2, 4.0) - 0.020));
    hits.push(hit("down", "Kick", at(2, 1.0) - 0.022));
    let p = plan(&request(hits, Vec::new()));
    assert!(
        close(target(&p, "down"), at(2, 1.0)),
        "{}",
        target(&p, "down")
    );
}

#[test]
fn a_fill_is_left_as_played_unless_asked() {
    let hits = vec![hit("t", "Tom 1", at(2, 3.1)), hit("s", "Snare", at(2, 2.0))];
    let mut req = request(hits, Vec::new());
    req.zones.push(Zone {
        start: at(2, 3.0),
        end: at(3, 1.0),
        kind: ZoneKind::Fill,
        grid: None,
    });
    let p = plan(&req);
    assert_eq!(reason(&p, "t"), Reason::Kept);
    assert!(close(target(&p, "t"), at(2, 3.1)));
    req.settings.plan_fills = true;
    let p = plan(&req);
    assert!(close(target(&p, "t"), at(2, 3.0)));
    assert!(
        p.doubts.iter().any(|d| d.id == "t"),
        "the fill is one question"
    );
    req.zones[0].grid = Some(24);
    let p = plan(&req);
    assert_eq!(reason(&p, "t"), Reason::FillGrid(24));
}

#[test]
fn false_doubles_are_removed_not_moved() {
    let mut double = hit("d", "Snare", at(2, 2.0) + 0.035);
    double.heard = Heard::Trigger;
    double.peak_db = -30.0;
    let mut loud_mic = hit("m", "Snare", at(2, 4.0) + 0.035);
    loud_mic.heard = Heard::Both;
    loud_mic.peak_db = -30.0;
    let mut bleed = hit("b", "Tom 4", at(3, 1.0) + 0.004);
    bleed.heard = Heard::Mic;
    bleed.peak_db = -36.0;
    bleed.flags = vec!["unsure".into()];
    let mut flagged = hit("f", "Kick", at(3, 2.5));
    flagged.flags = vec!["false-double".into()];
    let hits = vec![
        hit("s", "Snare", at(2, 2.0)),
        double,
        hit("s4", "Snare", at(2, 4.0)),
        loud_mic,
        hit("t3", "Tom 3", at(3, 1.0)),
        bleed,
        flagged,
    ];
    let p = plan(&request(hits, Vec::new()));
    for id in ["d", "b", "f"] {
        assert!(p.remove.iter().any(|r| r == id), "{id} removed");
        assert_eq!(reason(&p, id), Reason::FalseDouble);
        assert!(!p.targets.contains_key(id));
    }
    // Heard by its mic too: a real stroke (a flam), not removed.
    assert!(!p.remove.iter().any(|r| r == "m"));
}

#[test]
fn a_tie_between_lines_is_a_doubt_with_both() {
    // Exactly between two 16ths, no chart, no lean to say.
    let hits = vec![
        hit("k", "Kick", at(2, 1.375)),
        hit("s2", "Snare", at(2, 2.0)),
        hit("s4", "Snare", at(2, 4.0)),
    ];
    let p = plan(&request(hits, Vec::new()));
    let d = p.doubts.iter().find(|d| d.id == "k").expect("a doubt");
    assert!(close(d.candidates[0], target(&p, "k")));
    let other = if close(d.candidates[0], at(2, 1.25)) {
        at(2, 1.5)
    } else {
        at(2, 1.25)
    };
    assert!(close(d.candidates[1], other));
}

#[test]
fn the_answer_is_in_an_applys_shape() {
    let mut hits = Vec::new();
    groove(1, 0.020, &mut hits);
    let p = plan(&request(hits.clone(), Vec::new()));
    assert_eq!(p.hits.len(), hits.len());
    let leaders = p
        .hits
        .iter()
        .filter(|h| {
            !matches!(
                h.reason,
                Reason::Rides(_) | Reason::FalseDouble | Reason::Kept
            )
        })
        .count();
    assert_eq!(p.targets.len(), leaders);
    let text = serde_json::to_string(&p).unwrap();
    assert_eq!(serde_json::from_str::<Plan>(&text).unwrap(), p);
}
