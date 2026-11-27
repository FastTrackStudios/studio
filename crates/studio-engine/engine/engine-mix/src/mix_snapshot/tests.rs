//! A mix snapshot taken, the mix changed, compared and put back.

use daw::service::{Routing, TrackRef, Tracks};

use daw_proto::ProjectContext;
use daw_standalone::sync::Standalone;

use super::{DiffArg, Only, SnapshotArg, diff, restore, restore_with, snapshot, snapshot_with};
use crate::mix_sends::{AddArg, SetArg};
use crate::test_song::song;

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
pub fn diff_of(daw: &Standalone, ctx: &ProjectContext, a: &str, b: &str) -> Vec<String> {
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

#[test]
fn a_plugin_state_is_kept_once_however_many_mixes_hold_it() {
    use super::{FxMix, Mix, decode_mix, encode_mix};
    let (daw, ctx) = song("snapshot-states", None);
    let state = "c3RhdGUgb2YgYSBwbHVnaW4=".repeat(1_000);
    let fx = |guid: &str| FxMix {
        guid: guid.into(),
        track: "kick".into(),
        name: "Comp".into(),
        on: true,
        params: Vec::new(),
        state: state.clone(),
    };
    let mix = Mix {
        name: "A".into(),
        fx: vec![fx("{A}"), fx("{B}")],
        ..Mix::default()
    };
    let text = encode_mix(&daw, &ctx, &mix).expect("kept");
    assert!(
        text.len() < state.len(),
        "the state is not in the mix: {} bytes",
        text.len()
    );
    assert_eq!(text.matches("\"state\":\"@").count(), 2);
    assert_eq!(decode_mix(&daw, &ctx, &text).expect("read"), mix);

    // A second mix of the same states adds nothing to keep.
    let again = encode_mix(&daw, &ctx, &mix).expect("kept again");
    assert_eq!(again, text);

    // A snapshot from before, its states inline, reads as it did.
    let inline = facet_json::to_string(&mix).expect("inline");
    assert_eq!(decode_mix(&daw, &ctx, &inline).expect("read"), mix);
}

/// The hashes of the plugin states kept in the song's mix section.
fn kept_states(daw: &Standalone, ctx: &ProjectContext) -> std::collections::BTreeSet<String> {
    daw_proto::ExtState::keys_project(daw, ctx.clone(), engine_core::mix_states::SECTION)
        .into_iter()
        .filter_map(|k| k.strip_prefix("state:").map(str::to_owned))
        .collect()
}

/// The hash a plugin state `bytes` is kept under.
fn hash_of(bytes: &str) -> String {
    use base64::Engine as _;
    let text = base64::engine::general_purpose::STANDARD.encode(bytes);
    blake3::hash(text.as_bytes()).to_hex()[..engine_core::mix_states::HASH_LEN].to_owned()
}

#[test]
fn a_plugin_state_no_mix_names_goes_with_the_next_save() {
    use crate::mix_variants::{AddArg as VariantArg, CreateArg, add, create, end};
    let dir = tempfile::tempdir().expect("a folder");
    let (daw, ctx) = song("snapshot-sweep", None);
    let kick = crate::actions::resolve(&daw, &ctx, "Drums/Kick").expect("kick");
    let fx = daw_proto::fx::Effects::add(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(kick),
        "Stateful",
    )
    .expect("an fx");
    let hold = |state: &str| {
        daw.apply_plugin_state(&fx, state.as_bytes())
            .expect("the plugin takes its state");
    };

    hold("replaced");
    snapshot(&daw, &ctx, "A").expect("A");
    hold("kept by B");
    snapshot(&daw, &ctx, "B").expect("B");
    hold("kept by A");
    snapshot(&daw, &ctx, "A").expect("A, replaced");
    hold("a dropped variant's");
    create(
        &daw,
        &ctx,
        &CreateArg {
            group: "g".into(),
            ..CreateArg::default()
        },
    )
    .expect("a group");
    add(
        &daw,
        &ctx,
        &VariantArg {
            name: "v".into(),
            ..VariantArg::default()
        },
    )
    .expect("a variant");
    hold("a kept variant's");
    create(
        &daw,
        &ctx,
        &CreateArg {
            group: "h".into(),
            ..CreateArg::default()
        },
    )
    .expect("another group");
    add(
        &daw,
        &ctx,
        &VariantArg {
            name: "w".into(),
            ..VariantArg::default()
        },
    )
    .expect("another variant");
    end(&daw, &ctx, Some("g"), true).expect("g dropped");
    let all = |names: &[&str]| names.iter().map(|n| hash_of(n)).collect();
    assert_eq!(
        kept_states(&daw, &ctx),
        all(&[
            "replaced",
            "kept by B",
            "kept by A",
            "a dropped variant's",
            "a kept variant's"
        ]),
        "until a save, every state stays"
    );

    let file = dir.path().join("Sweep.session");
    crate::project_file::save(
        &daw,
        "snapshot-sweep",
        &file,
        session_format::store::MAIN,
        "Save",
    )
    .expect("saved");
    let named = all(&["kept by B", "kept by A", "a kept variant's"]);
    assert_eq!(
        kept_states(&daw, &ctx),
        named,
        "the replaced snapshot's and the dropped group's states are gone"
    );
    for mix in ["A", "B", "h:w"] {
        super::load(&daw, &ctx, mix).unwrap_or_else(|e| panic!("{mix} reads: {e}"));
    }

    // The file holds what the song does.
    let again = crate::project_file::load(&daw, "Again", &file, session_format::store::MAIN)
        .expect("opened");
    assert_eq!(
        kept_states(&daw, &ProjectContext::Project(again)),
        named,
        "the saved song keeps only the named states"
    );
}
