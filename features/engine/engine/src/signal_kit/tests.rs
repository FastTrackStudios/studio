//! A Signal kit put on a song of nested drum tracks (one track missing),
//! replaced, and switched off and on at once.

use std::path::Path;

use daw::service::{Effects as _, FxChainContext, ProjectContext};
use daw::standalone::Standalone;

use crate::clap_fx::test_support::{a_clap, drum_song};

/// A manifest in `dir` for `clap`: In and Top with a state each (Top's
/// relative to the manifest), and a tom the song does not have.
fn manifest(dir: &Path, clap: &Path, state: &[u8]) -> std::path::PathBuf {
    std::fs::write(dir.join("in.state"), state).expect("a state");
    std::fs::write(dir.join("top.state"), state).expect("a state");
    let json = serde_json::json!({
        "session": "kit-1",
        "clap": clap.to_string_lossy(),
        "tracks": [
            {"path": "Drums/Kick/Sum/In", "role": "kick", "state": dir.join("in.state").to_string_lossy()},
            {"path": "Drums/Snare/Top", "role": "snare", "state": "top.state"},
            {"path": "Drums/Tom/Floor", "role": "tom", "state": ""},
        ],
    });
    let path = dir.join("manifest.json");
    std::fs::write(&path, json.to_string()).expect("a manifest");
    path
}

/// The chain of the track at `path`.
fn chain(daw: &Standalone, ctx: &ProjectContext, path: &str) -> Vec<daw_proto::fx::Fx> {
    let track = crate::actions::resolve(daw, ctx, path).expect("the track");
    daw.list(ctx.clone(), FxChainContext::Track(track))
}

/// A state of `clap`, from an instance of it.
fn a_state(clap: &Path) -> Vec<u8> {
    let mut plugin = daw_standalone::plugin::load_plugin(&clap.to_string_lossy())
        .expect("loads")
        .expect("a plugin");
    plugin.save_state().expect("a state")
}

#[test]
fn a_kit_goes_on_the_tracks_it_names_and_is_switched_at_once() {
    let Some(clap) = a_clap() else {
        return;
    };
    let dir = tempfile::tempdir().expect("a folder");
    let state = a_state(&clap);
    let path = manifest(dir.path(), &clap, &state);
    let daw = Standalone::new();
    let ctx = drum_song(&daw, "kit-song");
    // An FTS processor already on the snare, bypassed: left so.
    let snare_top = crate::actions::resolve(&daw, &ctx, "Snare/Top").expect("the track");
    let eq = daw
        .add(
            ctx.clone(),
            FxChainContext::Track(snare_top.clone()),
            "FTS EQ",
        )
        .expect("an EQ");
    daw.set_enabled(
        ctx.clone(),
        daw::service::FxTarget {
            context: FxChainContext::Track(snare_top),
            fx: daw::service::FxRef::Guid(eq.clone()),
        },
        false,
    )
    .expect("bypassed");

    // A dry run changes nothing.
    let dry = super::kit(&daw, &ctx, &path, true, false).expect("planned");
    assert_eq!(dry.tracks[0].status, "would add");
    assert_eq!(dry.tracks[1].place, "#2");
    assert!(chain(&daw, &ctx, "Kick/Sum/In").is_empty());

    let kit = super::kit(&daw, &ctx, &path, false, false).expect("put on");
    let statuses: Vec<&str> = kit.tracks.iter().map(|t| t.status.as_str()).collect();
    assert_eq!(
        statuses,
        ["added", "added", "missing-track"],
        "{:?}",
        kit.tracks.iter().map(|t| &t.why).collect::<Vec<_>>()
    );
    assert_eq!((kit.added, kit.missing, kit.failed), (2, 1, 0));
    assert_eq!(kit.tracks[0].track, "In");
    assert_eq!(
        kit.tracks[1].place, "#2",
        "at the end of the chain, after the EQ"
    );
    let top = chain(&daw, &ctx, "Drums/Snare/Top");
    assert!(
        top.iter().any(|fx| fx.guid == eq && !fx.enabled),
        "the EQ is left bypassed"
    );
    let signal = &chain(&daw, &ctx, "Drums/Kick/Sum/In")[0];
    assert_eq!(
        daw.save_plugin_state(&signal.guid).expect("its state"),
        state
    );

    // Again without `replace`: skipped; with it, replaced in its place.
    let again = super::kit(&daw, &ctx, &path, false, false).expect("again");
    assert_eq!(again.skipped, 2);
    let replaced = super::kit(&daw, &ctx, &path, false, true).expect("replaced");
    assert_eq!(replaced.replaced, 2);
    let top = chain(&daw, &ctx, "Drums/Snare/Top");
    assert_eq!(top.len(), 2, "one instance, not two");
    assert_eq!(top[1].name, clap.to_string_lossy());

    // Off at once, the EQ untouched; and on again.
    let off = super::bypass(&daw, &ctx, false, None).expect("off");
    assert_eq!((off.instances, off.changed), (2, 2));
    assert!(
        chain(&daw, &ctx, "Kick/Sum/In")
            .iter()
            .all(|fx| !fx.enabled)
    );
    let top = chain(&daw, &ctx, "Drums/Snare/Top");
    assert!(!top[0].enabled && !top[1].enabled);
    let on = super::bypass(&daw, &ctx, true, None).expect("on");
    assert_eq!(on.changed, 2);
    let top = chain(&daw, &ctx, "Drums/Snare/Top");
    assert!(!top[0].enabled, "the EQ stays bypassed");
    assert!(top[1].enabled);
    assert!(on.ms < 50.0, "cheap enough to toggle: {} ms", on.ms);
}
