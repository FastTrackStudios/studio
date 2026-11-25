//! Mix variants: the model alone, then on a headless song — switched with
//! no undo step, kept as one, scoped, filtered, blind, loudness-matched.

use daw::service::{Projects, TrackRef, Tracks};
use daw_proto::ProjectContext;
use daw_standalone::sync::Standalone;

use super::*;
use crate::mix_measure::tests::song;

fn group_of(names: &[&str]) -> Group {
    let mut g = Group {
        name: "g".into(),
        ..Group::default()
    };
    for n in names {
        g.add(Variant {
            name: (*n).into(),
            note: format!("{n} note"),
            ..Variant::default()
        });
    }
    g
}

#[test]
fn labels_run_a_to_z_then_aa() {
    assert_eq!(label(0), "A");
    assert_eq!(label(25), "Z");
    assert_eq!(label(26), "AA");
    assert_eq!(label(27), "AB");
    for i in [0, 3, 25, 26, 27, 51, 52, 700] {
        assert_eq!(label_index(&label(i)), Some(i), "{i}");
    }
    assert_eq!(label_index("b"), Some(1));
    assert_eq!(label_index("B1"), None);
}

#[test]
fn a_shuffle_is_a_permutation_and_repeatable() {
    for n in [0, 1, 2, 5, 17] {
        let mut s = shuffle(n, 42);
        assert_eq!(s, shuffle(n, 42));
        s.sort_unstable();
        assert_eq!(s, (0..u32::try_from(n).unwrap()).collect::<Vec<_>>());
    }
    // Some seed moves something.
    assert!((1..20).any(|seed| shuffle(4, seed) != vec![0, 1, 2, 3]));
}

#[test]
fn next_and_prev_wrap_in_label_order() {
    let mut g = group_of(&["a", "b", "c"]);
    let next = Pick {
        next: true,
        ..Pick::default()
    };
    let prev = Pick {
        prev: true,
        ..Pick::default()
    };
    assert_eq!(g.resolve(&next), Ok(0));
    assert_eq!(g.resolve(&prev), Ok(2));
    g.current = Some(2);
    assert_eq!(g.resolve(&next), Ok(0));
    // Blind: label order is the shuffle's.
    g.blind = true;
    g.order = vec![2, 0, 1];
    g.current = Some(2); // label A
    assert_eq!(g.resolve(&next), Ok(0)); // label B
    assert_eq!(g.resolve(&prev), Ok(1)); // label C
    let by = |l: &str| {
        g.resolve(&Pick {
            label: Some(l.into()),
            ..Pick::default()
        })
    };
    assert_eq!(by("A"), Ok(2));
    assert_eq!(by("c"), Ok(1));
    assert!(by("D").is_err());
    assert_eq!(
        g.resolve(&Pick {
            index: Some(1),
            ..Pick::default()
        }),
        Ok(0)
    );
    assert!(g.resolve(&Pick::default()).is_err());
}

#[test]
fn blind_hides_names_until_revealed() {
    let mut g = group_of(&["tight", "loose", "late"]);
    g.set_blind(true, 7);
    g.rate(1, Some(4), Some(true), Some("punchy"))
        .expect("rated");
    let v = g.view(Vec::new());
    assert!(v.blind && !v.revealed);
    assert!(
        v.variants
            .iter()
            .all(|r| r.name.is_empty() && r.note.is_empty())
    );
    let labels: Vec<&str> = v.variants.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["A", "B", "C"]);
    // The vote shows, under its label.
    let l = label(g.label_of(1));
    let rated = v.variants.iter().find(|r| r.label == l).expect("its label");
    assert_eq!((rated.stars, rated.picked), (4, true));
    assert_eq!(rated.comments, vec!["punchy".to_owned()]);
    // A name not in the group is refused without listing the names.
    let e = g
        .resolve(&Pick {
            name: Some("nope".into()),
            ..Pick::default()
        })
        .unwrap_err();
    assert!(!e.contains("tight"), "{e}");
    g.revealed = true;
    let v = g.view(Vec::new());
    let shown = v.variants.iter().find(|r| r.label == l).expect("its label");
    assert_eq!(shown.name, "loose");
}

#[test]
fn one_pick_per_group_and_stars_up_to_five() {
    let mut g = group_of(&["a", "b"]);
    g.rate(0, None, Some(true), None).expect("pick a");
    g.rate(1, None, Some(true), None).expect("pick b");
    assert_eq!(g.picked(), Some(1));
    assert!(!g.variants[0].picked);
    assert!(g.rate(0, Some(6), None, None).is_err());
    // Replacing a variant keeps its votes.
    g.add(Variant {
        name: "b".into(),
        note: "again".into(),
        ..Variant::default()
    });
    assert_eq!(g.variants.len(), 2);
    assert!(g.variants[1].picked);
    assert_eq!(g.variants[1].note, "again");
}

#[test]
fn trims_bring_every_variant_down_to_the_quietest() {
    let (target, t) = trims(&[Some(-18.0), Some(-24.0), None, Some(-20.5)]);
    assert_eq!(target, Some(-24.0));
    assert_eq!(t, vec![-6.0, 0.0, 0.0, -3.5]);
}

// ---------------------------------------------------------------- on a song

fn kick_db(daw: &Standalone, ctx: &ProjectContext) -> f64 {
    let kick = resolve(daw, ctx, "Drums/Kick").expect("kick");
    let t = Tracks::all(daw, ctx.clone())
        .into_iter()
        .find(|t| t.guid == kick)
        .expect("kick track");
    (db(t.volume) * 100.0).round() / 100.0
}

fn track(daw: &Standalone, ctx: &ProjectContext, path: &str) -> daw_proto::Track {
    let g = resolve(daw, ctx, path).expect("a track");
    Tracks::all(daw, ctx.clone())
        .into_iter()
        .find(|t| t.guid == g)
        .expect("it")
}

fn op(track: &str) -> SetOp {
    SetOp {
        track: track.into(),
        ..SetOp::default()
    }
}

/// A group on the kick, variants at 0, -6 and -12 dB made without touching
/// the live mix.
fn kick_group(daw: &Standalone, ctx: &ProjectContext) {
    create(
        daw,
        ctx,
        &CreateArg {
            group: "kick".into(),
            scope: Some(vec!["Drums/Kick".into()]),
            note: Some("how loud".into()),
            ..CreateArg::default()
        },
    )
    .expect("created");
    add(
        daw,
        ctx,
        &AddArg {
            name: "zero".into(),
            ..AddArg::default()
        },
    )
    .expect("zero");
    for (name, v) in [("minus6", -6.0), ("minus12", -12.0)] {
        let row = add(
            daw,
            ctx,
            &AddArg {
                name: name.into(),
                from: Some("zero".into()),
                set: Some(vec![SetOp {
                    volume_db: Some(v),
                    ..op("Drums/Kick")
                }]),
                ..AddArg::default()
            },
        )
        .expect("a patched variant");
        assert_eq!(row.parent, "kick:zero");
        assert_eq!(row.set, vec![format!("Drums/Kick volume {v} dB")]);
    }
    assert_eq!(kick_db(daw, ctx), 0.0, "adding never touches the live mix");
}

#[test]
fn switching_makes_no_undo_step_and_keeping_makes_one() {
    let (daw, ctx) = song("variants-keep", None);
    kick_group(&daw, &ctx);
    // Outside the scope: left alone throughout.
    let snare = resolve(&daw, &ctx, "Drums/Snare").expect("snare");
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(snare), 0.5).expect("snare");
    let before = Projects::last_undo_label(&daw, ctx.clone());

    let to = |name: &str| SwitchArg {
        name: Some(name.into()),
        ..SwitchArg::default()
    };
    let s = switch(&daw, &ctx, &to("minus6")).expect("switched");
    assert!(s.started);
    assert_eq!((s.label.as_str(), s.name.as_str()), ("B", "minus6"));
    assert_eq!(kick_db(&daw, &ctx), -6.0);
    let s = switch(
        &daw,
        &ctx,
        &SwitchArg {
            next: Some(true),
            ..SwitchArg::default()
        },
    )
    .expect("next");
    assert!(!s.started);
    assert_eq!(s.name, "minus12");
    assert_eq!(kick_db(&daw, &ctx), -12.0);
    switch(&daw, &ctx, &to("zero")).expect("back");
    assert_eq!(kick_db(&daw, &ctx), 0.0);
    switch(&daw, &ctx, &to("minus12")).expect("again");
    assert_eq!(
        Projects::last_undo_label(&daw, ctx.clone()),
        before,
        "no switch is an undo step"
    );
    let st = state(&daw, &ctx, None).expect("state");
    assert!(st.auditioning);
    assert_eq!(st.current, "C");
    assert_eq!(st.scope, vec!["Drums/Kick".to_owned()]);

    let kept = keep(
        &daw,
        &ctx,
        &KeepArg {
            name: Some("minus6".into()),
            ..KeepArg::default()
        },
    )
    .expect("kept");
    assert_eq!(kept.name, "minus6");
    assert_eq!(kick_db(&daw, &ctx), -6.0);
    assert_eq!(
        Projects::last_undo_label(&daw, ctx.clone()).as_deref(),
        Some("Keep mix variant kick:minus6")
    );
    let snare_db = (db(track(&daw, &ctx, "Drums/Snare").volume) * 100.0).round() / 100.0;
    assert_eq!(snare_db, -6.02, "outside the scope");
    let st = state(&daw, &ctx, Some("kick")).expect("state");
    assert!(!st.auditioning);
    assert_eq!(st.kept, "minus6");
    assert_eq!(crate::mix_snapshot::head(&daw, &ctx), "kick:minus6");

    // One undo: back to the mix before the audition.
    assert!(Projects::undo(&daw, ctx.clone()));
    assert_eq!(kick_db(&daw, &ctx), 0.0);
    assert_eq!(Projects::last_undo_label(&daw, ctx.clone()), before);
}

#[test]
fn ending_without_a_keep_puts_the_mix_back() {
    let (daw, ctx) = song("variants-end", None);
    kick_group(&daw, &ctx);
    switch(
        &daw,
        &ctx,
        &SwitchArg {
            index: Some(2),
            ..SwitchArg::default()
        },
    )
    .expect("switched");
    assert_eq!(kick_db(&daw, &ctx), -12.0);
    let row = end(&daw, &ctx, Some("kick"), false).expect("ended");
    assert!(row.ended && !row.auditioning);
    assert_eq!(kick_db(&daw, &ctx), 0.0);
    // Dropped: gone from the song.
    let row = end(&daw, &ctx, Some("kick"), true).expect("dropped");
    assert!(row.groups.is_empty());
    assert!(load_group(&daw, &ctx, "kick").is_err());
}

#[test]
fn a_variant_is_a_snapshot_by_its_group_name() {
    let (daw, ctx) = song("variants-diff", None);
    kick_group(&daw, &ctx);
    let row = crate::mix_snapshot::tests::diff_of(&daw, &ctx, "kick:zero", "kick:minus6");
    assert_eq!(
        row,
        vec!["Drums/Kick volume: 0 dB → -6 dB".to_owned()],
        "{row:#?}"
    );
}

#[test]
fn blind_votes_by_label_and_a_keep_reveals() {
    let (daw, ctx) = song("variants-blind", None);
    kick_group(&daw, &ctx);
    let st = blind(
        &daw,
        &ctx,
        &BlindArg {
            on: Some(true),
            seed: Some(3),
            ..BlindArg::default()
        },
    )
    .expect("blind");
    assert!(st.variants.iter().all(|v| v.name.is_empty()));
    let s = switch(
        &daw,
        &ctx,
        &SwitchArg {
            label: Some("B".into()),
            ..SwitchArg::default()
        },
    )
    .expect("switched");
    assert!(s.name.is_empty(), "a blind switch names no variant");
    rate(
        &daw,
        &ctx,
        &RateArg {
            stars: Some(5),
            pick: Some(true),
            comment: Some("this one".into()),
            ..RateArg::default()
        },
    )
    .expect("rated what plays");
    let st = state(&daw, &ctx, None).expect("state");
    let b = st.variants.iter().find(|v| v.label == "B").expect("B");
    assert!(b.picked && b.playing && b.stars == 5);
    assert!(b.name.is_empty());
    // Kept with no name: the pick.
    let kept = keep(&daw, &ctx, &KeepArg::default()).expect("kept");
    assert_eq!(kept.label, "B");
    let g = load_group(&daw, &ctx, "kick").expect("group");
    let at_b = g.variants[usize::try_from(g.order[1]).unwrap()]
        .name
        .clone();
    assert_eq!(kept.name, at_b);
    let st = state(&daw, &ctx, None).expect("state");
    assert!(st.revealed);
    assert_eq!(
        st.variants
            .iter()
            .find(|v| v.label == "B")
            .map(|v| v.name.clone()),
        Some(at_b)
    );
}

#[test]
fn a_group_varies_only_its_categories() {
    let (daw, ctx) = song("variants-only", None);
    create(
        &daw,
        &ctx,
        &CreateArg {
            group: "pan".into(),
            scope: Some(vec!["Drums".into()]),
            only: Some(vec!["pan".into()]),
            ..CreateArg::default()
        },
    )
    .expect("created");
    add(
        &daw,
        &ctx,
        &AddArg {
            name: "left".into(),
            set: Some(vec![SetOp {
                pan: Some(-0.5),
                volume_db: Some(-20.0),
                ..op("Drums/Snare")
            }]),
            ..AddArg::default()
        },
    )
    .expect("left");
    switch(
        &daw,
        &ctx,
        &SwitchArg {
            name: Some("left".into()),
            ..SwitchArg::default()
        },
    )
    .expect("switched");
    let snare = track(&daw, &ctx, "Drums/Snare");
    assert!((snare.pan - -0.5).abs() < 1e-6, "{}", snare.pan);
    assert!((db(snare.volume)).abs() < 0.01, "volume is not varied");
}

#[test]
fn a_patch_names_a_send_and_an_fx() {
    let (daw, ctx) = song("variants-patch", None);
    let kick = resolve(&daw, &ctx, "Drums/Kick").expect("kick");
    crate::mix_sends::add(
        &daw,
        &ctx,
        &crate::mix_sends::AddArg {
            from: "Drums/Kick".into(),
            to: "Bus".into(),
            level_db: Some(-3.0),
            pre_fader: None,
            tap: None,
            again: None,
        },
    )
    .expect("a send");
    daw_proto::fx::Effects::add(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(kick),
        "Gain 6",
    )
    .expect("an fx");
    create(
        &daw,
        &ctx,
        &CreateArg {
            group: "p".into(),
            ..CreateArg::default()
        },
    )
    .expect("created");
    let row = add(
        &daw,
        &ctx,
        &AddArg {
            name: "x".into(),
            set: Some(vec![
                SetOp {
                    send: Some("Bus".into()),
                    nudge_db: Some(-3.0),
                    ..op("Drums/Kick")
                },
                SetOp {
                    fx: Some("gain".into()),
                    on: Some(false),
                    ..op("Drums/Kick")
                },
            ]),
            ..AddArg::default()
        },
    )
    .expect("patched");
    assert_eq!(row.set.len(), 2, "{:?}", row.set);
    let d = crate::mix_snapshot::tests::diff_of(&daw, &ctx, "current", "p:x");
    assert!(
        d.iter()
            .any(|l| l == "send Drums/Kick → Bus level: -3 dB → -6 dB"),
        "{d:#?}"
    );
    assert!(
        d.iter()
            .any(|l| l.ends_with("on Drums/Kick on: true → false")),
        "{d:#?}"
    );
    // A send the mix does not have is refused.
    assert!(
        add(
            &daw,
            &ctx,
            &AddArg {
                name: "y".into(),
                set: Some(vec![SetOp {
                    send: Some("Drums/Snare".into()),
                    level_db: Some(0.0),
                    ..op("Drums/Kick")
                }]),
                ..AddArg::default()
            },
        )
        .is_err()
    );
}

#[test]
fn matched_variants_play_at_one_loudness_and_keep_untrimmed() {
    let (daw, ctx) = song("variants-match", None);
    kick_group(&daw, &ctx);
    let m = match_loudness(
        &daw,
        &ctx,
        &MatchArg {
            from: Some("1s".into()),
            to: Some("4s".into()),
            ..MatchArg::default()
        },
    )
    .expect("matched");
    assert!(m.matched);
    let g = load_group(&daw, &ctx, "kick").expect("group");
    let trim = |n: &str| {
        g.variants
            .iter()
            .find(|v| v.name == n)
            .map(|v| v.trim_db)
            .expect("variant")
    };
    assert!((trim("zero") - -12.0).abs() < 0.3, "{:?}", m.variants);
    assert!((trim("minus6") - -6.0).abs() < 0.3, "{:?}", m.variants);
    assert!(trim("minus12").abs() < 0.01, "{:?}", m.variants);
    assert_eq!(kick_db(&daw, &ctx), 0.0, "measuring puts the mix back");
    // Auditioned: trimmed at the scope's output.
    switch(
        &daw,
        &ctx,
        &SwitchArg {
            name: Some("zero".into()),
            ..SwitchArg::default()
        },
    )
    .expect("zero");
    assert!((kick_db(&daw, &ctx) - -12.0).abs() < 0.3);
    switch(
        &daw,
        &ctx,
        &SwitchArg {
            name: Some("minus12".into()),
            ..SwitchArg::default()
        },
    )
    .expect("minus12");
    assert!((kick_db(&daw, &ctx) - -12.0).abs() < 0.01);
    // Kept: the variant's own fader, no trim.
    keep(
        &daw,
        &ctx,
        &KeepArg {
            name: Some("zero".into()),
            ..KeepArg::default()
        },
    )
    .expect("kept");
    assert_eq!(kick_db(&daw, &ctx), 0.0);
}
