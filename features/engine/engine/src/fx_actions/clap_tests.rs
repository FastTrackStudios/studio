//! A CLAP hosted from its bundle: added by path or name, set in its own
//! words, its state read and loaded, kept through a save and an open, and
//! reloaded from a rebuilt bundle.

use std::collections::HashMap;

use daw::service::{Effects as _, FxChainContext, FxRef, FxTarget};
use daw::standalone::Standalone;

use super::{AddArg, FxArg, ReloadArg, StateSetArg};
use crate::clap_fx::test_support::{a_clap, drum_song};

/// The CLAP put on `Drums/Kick/Sum/In` with FX_ADD: its guid, and the
/// track's chain.
fn added(
    daw: &Standalone,
    ctx: &daw::service::ProjectContext,
    fx: &str,
) -> (String, FxChainContext) {
    let said = super::add(
        daw,
        ctx,
        &AddArg {
            track: "Kick/Sum/In".into(),
            fx: fx.into(),
            at: None,
            on: None,
            set: HashMap::new(),
        },
    )
    .expect("added");
    assert!(said[0].contains("at #1"), "{said:?}");
    let track = crate::actions::resolve(daw, ctx, "Drums/Kick/Sum/In").expect("the track");
    let chain = FxChainContext::Track(track);
    let fx = daw.list(ctx.clone(), chain.clone())[0].guid.clone();
    (fx, chain)
}

#[test]
fn a_clap_added_by_its_path_processes_audio() {
    let Some(clap) = a_clap() else {
        return;
    };
    let daw = Standalone::new();
    let ctx = drum_song(&daw, "clap-audio");
    let (fx, _) = added(&daw, &ctx, &clap.to_string_lossy());
    let out = daw
        .with_plugin_instance(&fx, |p| {
            p.prepare(48_000.0, 512).expect("prepared");
            let input: Vec<f32> = (0..512)
                .map(|i| (f32::from(u16::try_from(i).unwrap_or(0)) * 0.05).sin() * 0.5)
                .collect();
            let (mut l, mut r) = (vec![0.0_f32; 512], vec![0.0_f32; 512]);
            // A few blocks: some plugins settle their gain in over the first.
            for _ in 0..4 {
                p.process_block(
                    &input,
                    &input,
                    &mut l,
                    &mut r,
                    &daw_standalone::plugin::PluginEvents::EMPTY,
                )
                .expect("processed");
            }
            l.iter().map(|s| s.abs()).fold(0.0_f32, f32::max)
        })
        .expect("a live instance");
    assert!(out > 0.01, "the plugin passed sound through: peak {out}");
}

#[test]
fn a_clap_is_found_by_its_name_and_set_in_its_own_words() {
    let Some(clap) = a_clap() else {
        return;
    };
    let name = clap
        .file_name()
        .and_then(|n| n.to_str())
        .expect("a name")
        .to_owned();
    let daw = Standalone::new();
    let ctx = drum_song(&daw, "clap-words");
    let (fx_guid, chain) = added(&daw, &ctx, &name);
    let target = FxTarget {
        context: chain,
        fx: FxRef::Guid(fx_guid.clone()),
    };
    let fx = daw.get(ctx.clone(), target.clone()).expect("the FX");
    assert_eq!(fx.name, clap.to_string_lossy(), "named by its bundle");
    let params = daw.parameters(ctx.clone(), target.clone());
    // The words the plugin writes for a value other than the one it has,
    // read back by the plugin into the same value.
    let param = params
        .iter()
        .find(|p| {
            crate::clap_fx::text_to_normalized(&daw, &fx_guid, p.index, &p.formatted).is_some()
        })
        .expect("a parameter the plugin reads its own words for");
    let other = if param.value > 0.5 { 0.2 } else { 0.8 };
    let words = daw
        .with_plugin_instance(&fx_guid, |p| {
            let info = p.params().into_iter().nth(param.index as usize)?;
            p.value_to_text(info.id, info.min + other * (info.max - info.min))
        })
        .flatten()
        .expect("its words");
    let value = super::value_for(&daw, &fx, param, &words).expect("read in its words");
    assert!(
        (value - other).abs() < 0.02,
        "{words:?} → {value}, not {other}"
    );
    // `=x` still sets it normalized.
    assert!((super::value_for(&daw, &fx, param, "=0.25").expect("normalized") - 0.25).abs() < 1e-9);
    // FX_SET's path: what it now reads is the plugin's own words.
    let said = super::settle(
        &daw,
        &ctx,
        (&target, &fx),
        &HashMap::from([(format!("#{}", param.index), words.clone())]),
    );
    assert!(said[0].contains(" = "), "{said:?}");
}

#[test]
fn a_clap_s_state_round_trips_and_is_kept_by_a_save() {
    let Some(clap) = a_clap() else {
        return;
    };
    let dir = tempfile::tempdir().expect("a folder");
    let daw = Standalone::new();
    let ctx = drum_song(&daw, "clap-state");
    let (fx_guid, chain) = added(&daw, &ctx, &clap.to_string_lossy());
    let arg = FxArg {
        track: "Drums/Kick/Sum/In".into(),
        fx: "#1".into(),
    };
    let target = FxTarget {
        context: chain,
        fx: FxRef::Guid(fx_guid.clone()),
    };
    // Moved off its defaults, then read.
    super::settle(
        &daw,
        &ctx,
        (
            &target,
            &daw.get(ctx.clone(), target.clone()).expect("the FX"),
        ),
        &HashMap::from([("#0".to_owned(), "=0.8".to_owned())]),
    );
    // The held value reaches the plugin as a render would send it.
    let held: Vec<(u32, f64)> = daw
        .read_project("clap-state", |p| {
            p.fx_chains
                .values()
                .flatten()
                .find(|e| e.fx.guid == fx_guid)
                .map(|e| e.params.iter().map(|(k, v)| (*k, *v)).collect())
        })
        .flatten()
        .unwrap_or_default();
    daw.with_plugin_instance(&fx_guid, |p| {
        p.prepare(48_000.0, 64).expect("prepared");
        let silence = vec![0.0_f32; 64];
        let (mut l, mut r) = (vec![0.0_f32; 64], vec![0.0_f32; 64]);
        let events = daw_standalone::plugin::PluginEvents {
            params: &held,
            ..daw_standalone::plugin::PluginEvents::EMPTY
        };
        p.process_block(&silence, &silence, &mut l, &mut r, &events)
            .expect("processed");
    });
    let set = super::state(&daw, &ctx, &arg).expect("its state");
    assert!(set.bytes > 0);

    // Another setting, then the read state loaded back: as it was.
    let other = super::state_bytes(None, Some(&set.state_b64)).expect("decoded");
    super::settle(
        &daw,
        &ctx,
        (
            &target,
            &daw.get(ctx.clone(), target.clone()).expect("the FX"),
        ),
        &HashMap::from([("#0".to_owned(), "=0.1".to_owned())]),
    );
    let file = dir.path().join("eq.state");
    std::fs::write(&file, &other).expect("a state file");
    let loaded = super::state_set(
        &daw,
        &ctx,
        &StateSetArg {
            track: arg.track.clone(),
            fx: arg.fx.clone(),
            state_file: Some(file.to_string_lossy().into_owned()),
            state_b64: None,
        },
    )
    .expect("loaded");
    assert_eq!(loaded.bytes, set.bytes);
    assert_eq!(
        super::state(&daw, &ctx, &arg).expect("read").state_b64,
        set.state_b64
    );
    let first = &daw.parameters(ctx.clone(), target.clone())[0];
    assert!(
        (first.value - 0.8).abs() < 0.02,
        "the state's value, not the held 0.1: {}",
        first.value
    );

    // Saved, and opened again from the file: the CLAP and its state.
    let song = dir.path().join("Clap.session");
    crate::project_file::save(
        &daw,
        "clap-state",
        &song,
        session_format::store::MAIN,
        "Save",
    )
    .expect("saved");
    let reopened = crate::project_file::load(&daw, "Again", &song, session_format::store::MAIN)
        .expect("opened");
    let again = daw
        .read_project(&reopened, |p| {
            p.fx_chains
                .values()
                .flatten()
                .find(|e| e.fx.name == clap.to_string_lossy())
                .map(|e| e.fx.guid.clone())
        })
        .flatten()
        .expect("the CLAP, back on its track");
    assert_eq!(
        daw.save_plugin_state(&again).expect("its state"),
        other,
        "the reopened instance holds the saved state"
    );
}

#[test]
fn a_rebuilt_clap_is_reloaded_under_its_instances_with_their_state() {
    let Some(clap) = a_clap() else {
        return;
    };
    let dir = tempfile::tempdir().expect("a folder");
    // A bundle of its own, so "rebuilding" it touches nothing else.
    let bundle = dir.path().join("Rebuilt.clap");
    std::fs::copy(&clap, &bundle).expect("a bundle");
    let daw = Standalone::new();
    let ctx = drum_song(&daw, "clap-reload");
    let (fx_guid, chain) = added(&daw, &ctx, &bundle.to_string_lossy());
    let before = daw.save_plugin_state(&fx_guid).expect("a state");
    // Rebuilt: a new file at the same path.
    std::fs::remove_file(&bundle).expect("removed");
    std::fs::copy(&clap, &bundle).expect("rebuilt");
    let row = super::reload(
        &daw,
        &ctx,
        &ReloadArg {
            path: bundle.to_string_lossy().into_owned(),
        },
    )
    .expect("reloaded");
    assert_eq!((row.instances, row.reloaded), (1, 1), "{:?}", row.failed);
    assert_ne!(row.copy, row.path, "loaded from a copy of its own");
    assert_eq!(daw.save_plugin_state(&fx_guid).expect("its state"), before);
    let fx = daw.list(ctx, chain)[0].clone();
    assert_eq!(
        fx.name,
        std::fs::canonicalize(&bundle)
            .expect("canonical")
            .to_string_lossy()
    );
}
