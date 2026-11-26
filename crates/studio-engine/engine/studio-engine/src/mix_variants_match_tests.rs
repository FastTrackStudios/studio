//! Mix variants matched by loudness: the mix (engine-mix) measured by the
//! listener (engine-listen, through the engine's assembly), so here, where
//! both are installed.

use daw::service::Tracks;
use daw_proto::ProjectContext;
use daw_standalone::sync::Standalone;
use engine_mix::test_song::song;

use crate::actions::resolve;
use crate::mix_sends::db;
use crate::mix_variants::*;

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

fn kick_db(daw: &Standalone, ctx: &ProjectContext) -> f64 {
    let kick = resolve(daw, ctx, "Drums/Kick").expect("kick");
    let t = Tracks::all(daw, ctx.clone())
        .into_iter()
        .find(|t| t.guid == kick)
        .expect("kick track");
    (db(t.volume) * 100.0).round() / 100.0
}

#[test]
fn matched_variants_play_at_one_loudness_and_keep_untrimmed() {
    crate::install();
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
