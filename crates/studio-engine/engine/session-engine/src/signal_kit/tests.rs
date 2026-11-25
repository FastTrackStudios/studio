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
    let dry = super::kit(&daw, &ctx, &path, true, false, false).expect("planned");
    assert_eq!(dry.tracks[0].status, "would add");
    assert_eq!(dry.tracks[1].place, "#2");
    assert!(chain(&daw, &ctx, "Kick/Sum/In").is_empty());

    let kit = super::kit(&daw, &ctx, &path, false, false, false).expect("put on");
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
    let again = super::kit(&daw, &ctx, &path, false, false, false).expect("again");
    assert_eq!(again.skipped, 2);
    let replaced = super::kit(&daw, &ctx, &path, false, true, false).expect("replaced");
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

#[test]
fn the_kits_sends_are_reported_and_made_and_a_folder_into_itself_is_carried_by_its_pieces() {
    let daw = Standalone::new();
    let ctx = drum_song(&daw, "kit-sends");
    let send = |from: &str, to: &str, pre: bool| super::KitSend {
        from: from.into(),
        to: to.into(),
        gain_db: -6.0,
        pre,
    };
    // The kit (Drums, which holds Snare/Top) into Snare/Top: carried by the
    // kit's folder that does not hold it — Kick.
    let kit = send("Drums (the kit, before Process)", "Drums/Snare/Top", false);
    let rows = super::route(&daw, &ctx, &kit, false, false);
    assert_eq!(
        rows.len(),
        1,
        "{:?}",
        rows.iter().map(|r| &r.via).collect::<Vec<_>>()
    );
    assert_eq!(rows[0].via, "Drums/Kick");
    assert_eq!(rows[0].status, "missing");
    let dry = super::route(&daw, &ctx, &kit, false, true);
    assert_eq!(dry[0].status, "would add");
    let made = super::route(&daw, &ctx, &kit, true, false);
    assert_eq!(made[0].status, "added");
    let again = super::route(&daw, &ctx, &kit, true, false);
    assert_eq!(again[0].status, "exists");
    // A plain one, pre-fader.
    let pre = super::route(
        &daw,
        &ctx,
        &send("Drums/Kick/Sub", "Drums/Snare/Top", true),
        true,
        false,
    );
    assert_eq!(
        (pre[0].status.as_str(), pre[0].tap.as_str()),
        ("added", "pre-fader")
    );
    let listed = crate::mix_sends::list(&daw, &ctx, Some("Drums/Snare/Top")).expect("listed");
    assert_eq!(listed.count, 2);
    assert!(
        listed
            .sends
            .iter()
            .all(|s| (s.level_db - -6.0).abs() < 1e-6)
    );
    // A track the song does not have.
    let gone = super::route(
        &daw,
        &ctx,
        &send("Drums/Tom/Floor", "Drums/Snare/Top", false),
        true,
        false,
    );
    assert_eq!(gone[0].status, "missing-track");
}

#[test]
fn a_copy_of_a_signal_state_is_put_in_a_session_of_its_own() {
    let inner = r#"{\"version\":1,\"instance\":{\"mode\":\"port\",\"session\":\"unbreakable-drums\",\"port\":\"Drums/Kick\"}}"#;
    let json = format!(r#"{{"fields":{{"signal-state":"{inner}"}}}}"#);
    let mut state = (json.len() as u64).to_le_bytes().to_vec();
    state.extend_from_slice(json.as_bytes());
    let out = super::private_session(state.clone(), "mix-7");
    let text = String::from_utf8_lossy(&out[8..]).into_owned();
    assert!(
        text.contains(r#"\"session\":\"unbreakable-drums~mix-7\""#),
        "{text}"
    );
    assert!(text.contains(r#"\"port\":\"Drums/Kick\""#), "{text}");
    let mut head = [0u8; 8];
    head.copy_from_slice(&out[..8]);
    assert_eq!(u64::from_le_bytes(head), (out.len() - 8) as u64);
    // No session named: as it was.
    assert_eq!(
        super::private_session(b"plain".to_vec(), "x"),
        b"plain".to_vec()
    );
    // The kit's own states, when this machine has them.
    if let Ok(real) = std::fs::read("/tmp/signal-kit/Drums__Kick.state") {
        let out = super::private_session(real.clone(), "mix-1");
        assert_eq!(out.len(), real.len() + "~mix-1".len());
        assert!(String::from_utf8_lossy(&out).contains("unbreakable-drums~mix-1"));
    }
    assert!(super::is_signal_bundle("/home/x/.clap/Signal Dev.clap"));
    assert!(!super::is_signal_bundle("FTS Comp"));
}
