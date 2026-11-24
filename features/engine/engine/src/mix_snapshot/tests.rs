//! A mix snapshot taken, the mix changed, compared and put back.

use daw::service::{Routing, TrackRef, Tracks};

use super::{DiffArg, diff, restore, snapshot};
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
                names: None
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
        },
    )
    .expect("names")
    .names;
    assert_eq!(names, vec!["A".to_owned()]);
}
