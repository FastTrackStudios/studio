//! A mix snapshot taken, the mix changed, compared and put back.

use daw::service::{Routing, TrackRef, Tracks};

use daw_proto::ProjectContext;
use daw_standalone::sync::Standalone;

use super::{DiffArg, Only, SnapshotArg, diff, restore, restore_with, snapshot, snapshot_with};
use crate::mix_measure::tests::song;
use crate::mix_sends::{AddArg, SetArg};

#[test]
fn a_snapshot_round_trips_faders_sends_and_fx() {
    let (daw, ctx) = song("snapshot-trip", None);
    let kick = crate::actions::resolve(&daw, &ctx, "Drums/Kick").expect("kick");
    let snare = crate::actions::resolve(&daw, &ctx, "Drums/Snare").expect("snare");
    Tracks::set_volume(
        &daw,
        ctx.clone(),
        TrackRef::Guid(kick.clone()),
        10f64.powf(-6.0 / 20.0),
    )
    .expect("fader");
    crate::mix_sends::add(
        &daw,
        &ctx,
        &AddArg {
            from: "Drums/Kick".into(),
            to: "Bus".into(),
            level_db: Some(-3.0),
            pre_fader: None,
            tap: None,
            again: None,
        },
    )
    .expect("a send");
    let fx = daw_proto::fx::Effects::add(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(kick.clone()),
        "Gain 6",
    )
    .expect("an fx");
    let kept = snapshot(&daw, &ctx, "A").expect("kept");
    assert_eq!(kept.sends, 1);
    assert_eq!(kept.fx, 1);
    assert!(
        diff(
            &daw,
            &ctx,
            &DiffArg {
                a: Some("A".into()),
                b: None,
                names: None,
                ..DiffArg::default()
            }
        )
        .expect("diffed")
        .differences
        .is_empty()
    );

    // Change it all.
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(kick.clone()), 1.0).expect("fader");
    Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(snare.clone()), true).expect("mute");
    crate::mix_sends::set(
        &daw,
        &ctx,
        &SetArg {
            from: "Drums/Kick".into(),
            to: "Bus".into(),
            level_db: Some(-12.0),
            pre_fader: Some(true),
            ..SetArg::default()
        },
    )
    .expect("send set");
    crate::mix_sends::add(
        &daw,
        &ctx,
        &AddArg {
            from: "Drums/Snare".into(),
            to: "Bus".into(),
            level_db: None,
            pre_fader: None,
            tap: None,
            again: None,
        },
    )
    .expect("another send");
    daw_proto::fx::Effects::set_enabled(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxTarget {
            context: daw_proto::fx::FxChainContext::Track(kick.clone()),
            fx: daw_proto::fx::FxRef::Guid(fx.clone()),
        },
        false,
    )
    .expect("bypassed");
    let differences = diff(
        &daw,
        &ctx,
        &DiffArg {
            a: Some("A".into()),
            b: None,
            names: None,
            ..DiffArg::default()
        },
    )
    .expect("diffed")
    .differences;
    let says = |what: &str| differences.iter().any(|d| d.contains(what));
    assert!(says("Drums/Kick volume: -6 dB → 0 dB"), "{differences:#?}");
    assert!(says("Drums/Snare mute: false → true"), "{differences:#?}");
    assert!(
        says("send Drums/Kick → Bus level: -3 dB → -12 dB"),
        "{differences:#?}"
    );
    assert!(
        says("send Drums/Kick → Bus tap: post-fader → pre-fader"),
        "{differences:#?}"
    );
    assert!(says("send Drums/Snare → Bus: none →"), "{differences:#?}");
    assert!(says("on Drums/Kick on: true → false"), "{differences:#?}");

    // And back.
    let back = restore(&daw, &ctx, "A").expect("restored");
    assert!(back.missing.is_empty(), "{:?}", back.missing);
    let left = diff(
        &daw,
        &ctx,
        &DiffArg {
            a: Some("A".into()),
            b: None,
            names: None,
            ..DiffArg::default()
        },
    )
    .expect("diffed")
    .differences;
    assert!(left.is_empty(), "{left:#?}");
    assert!(Routing::sends(&daw, ctx.clone(), TrackRef::Guid(snare)).is_empty());
    // It is kept on the song, by name.
    let names = diff(
        &daw,
        &ctx,
        &DiffArg {
            a: None,
            b: None,
            names: Some(true),
            ..DiffArg::default()
        },
    )
    .expect("names")
    .names;
    assert_eq!(names, vec!["A".to_owned()]);
}

/// What differs from `a` to `b` (MIX_DIFF).
pub(crate) fn diff_of(daw: &Standalone, ctx: &ProjectContext, a: &str, b: &str) -> Vec<String> {
    diff(
        daw,
        ctx,
        &DiffArg {
            a: Some(a.into()),
            b: Some(b.into()),
            ..DiffArg::default()
        },
    )
    .expect("diffed")
    .differences
}

fn db_of(daw: &Standalone, ctx: &ProjectContext, path: &str) -> (f64, f64) {
    let g = crate::actions::resolve(daw, ctx, path).expect("track");
    let t = Tracks::all(daw, ctx.clone())
        .into_iter()
        .find(|t| t.guid == g)
        .expect("it");
    (
        (crate::mix_sends::db(t.volume) * 100.0).round() / 100.0,
        t.pan,
    )
}

#[test]
fn a_recall_with_a_filter_changes_only_those_categories() {
    let (daw, ctx) = song("snapshot-filter", None);
    let kick = crate::actions::resolve(&daw, &ctx, "Drums/Kick").expect("kick");
    snapshot(&daw, &ctx, "flat").expect("kept");
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(kick.clone()), 0.5).expect("fader");
    Tracks::set_pan(&daw, ctx.clone(), TrackRef::Guid(kick.clone()), 0.4).expect("pan");
    let only = Only::parse(Some(&["volume".to_owned()])).expect("a category");
    let back = restore_with(&daw, &ctx, "flat", only).expect("restored");
    assert_eq!(back.changed, vec!["Drums/Kick".to_owned()]);
    let (v, p) = db_of(&daw, &ctx, "Drums/Kick");
    assert_eq!(v, 0.0);
    assert!((p - 0.4).abs() < 1e-6, "pan is not recalled: {p}");
    // MIX_DIFF with the same filter sees nothing left.
    let d = diff(
        &daw,
        &ctx,
        &DiffArg {
            a: Some("flat".into()),
            only: Some(vec!["fader".into()]),
            ..DiffArg::default()
        },
    )
    .expect("diffed");
    assert!(d.differences.is_empty(), "{:#?}", d.differences);
    assert!(Only::parse(Some(&["loudness".to_owned()])).is_err());
}

#[test]
fn a_filter_on_save_holds_on_recall() {
    let (daw, ctx) = song("snapshot-only-save", None);
    let kick = crate::actions::resolve(&daw, &ctx, "Drums/Kick").expect("kick");
    snapshot_with(
        &daw,
        &ctx,
        &SnapshotArg {
            name: "pans".into(),
            only: Some(vec!["pan".into()]),
            ..SnapshotArg::default()
        },
    )
    .expect("kept");
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(kick.clone()), 0.5).expect("fader");
    Tracks::set_pan(&daw, ctx.clone(), TrackRef::Guid(kick), 0.4).expect("pan");
    restore(&daw, &ctx, "pans").expect("restored");
    let (v, p) = db_of(&daw, &ctx, "Drums/Kick");
    assert_eq!(v, -6.02, "volume was not saved");
    assert!(p.abs() < 1e-6, "{p}");
}

#[test]
fn a_scoped_snapshot_touches_only_its_tracks_and_records_its_parent() {
    let (daw, ctx) = song("snapshot-scope", None);
    let kick = crate::actions::resolve(&daw, &ctx, "Drums/Kick").expect("kick");
    let snare = crate::actions::resolve(&daw, &ctx, "Drums/Snare").expect("snare");
    snapshot(&daw, &ctx, "all").expect("kept");
    let kept = snapshot_with(
        &daw,
        &ctx,
        &SnapshotArg {
            name: "kick".into(),
            scope: Some(vec!["Drums/Kick".into()]),
            ..SnapshotArg::default()
        },
    )
    .expect("kept");
    assert_eq!(kept.tracks, 1);
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(kick), 0.5).expect("fader");
    Tracks::set_volume(&daw, ctx.clone(), TrackRef::Guid(snare), 0.5).expect("fader");
    let back = restore(&daw, &ctx, "kick").expect("restored");
    assert_eq!(back.changed, vec!["Drums/Kick".to_owned()]);
    assert_eq!(db_of(&daw, &ctx, "Drums/Kick").0, 0.0);
    assert_eq!(db_of(&daw, &ctx, "Drums/Snare").0, -6.02);
    // `current` against a scoped one compares that scope only.
    assert!(diff_of(&daw, &ctx, "kick", "current").is_empty());
    // History: `kick` came from `all` (the head then).
    let names = diff(
        &daw,
        &ctx,
        &DiffArg {
            names: Some(true),
            ..DiffArg::default()
        },
    )
    .expect("names");
    assert_eq!(names.parents, vec!["kick ← all".to_owned()]);
    assert_eq!(names.head, "kick");
}
