//! A mix, kept and put back: every track's fader, pan, mute, solo and
//! polarity, every send, and every FX — on or off, its held parameter
//! values and its plugin's own state (a hosted CLAP's state blob) — saved
//! on the song under a name (`MIX_SNAPSHOT`), put back (`MIX_RESTORE`),
//! and two of them (or one and the mix as it stands) compared
//! (`MIX_DIFF`): for A/B while dialling a mix in.
//!
//! Kept in the song's project ext state (section `fts.mix`), so saved and
//! undone with the song. A snapshot names tracks and FX by GUID; a track
//! or FX gone since is reported, not made again.

use std::collections::{BTreeMap, HashMap};

use architect::action::{ActionList, ActionMeta, Effect};
use base64::Engine as _;
use daw::service::{
    Effects, FxChainContext, FxRef, FxTarget, ProjectContext, RouteLocation, RouteRef, RouteType,
    Routing, TrackRef, Tracks,
};
use daw_proto::routing::SendMode;
use daw_standalone::sync::{FxChainKey, Standalone};
use facet::Facet;

use crate::actions::{meta, song};
use crate::mix_sends::{db, gain, tap_of, tap_word};

const SECTION: &str = "fts.mix";
/// The names of the song's snapshots.
const NAMES: &str = "snapshots";

static SNAPSHOT: ActionMeta = meta(
    "MIX_SNAPSHOT",
    "mix_snapshot",
    "Snapshot the mix",
    "Keep the mix as it stands on the song under `name`: every track's fader, pan, mute, solo and polarity, the master's, every send (level, pan, mute, tap), and every FX (on or off, its held parameter values, its plugin's own state — a CLAP's state blob). A snapshot of that name is replaced. Saved with the song. Says what it holds and its size.",
    "Mix",
    "",
);

static RESTORE: ActionMeta = meta(
    "MIX_RESTORE",
    "mix_restore",
    "Restore a mix snapshot",
    "Put the mix back as snapshot `name` has it: faders, pans, mutes, solos, polarity, the master, the sends (set again, added where missing, taken off where the snapshot has none between the two tracks), every FX on or off, its parameter values and its plugin's state (loaded only where it differs). Tracks and FX gone since are named, not made again. One undo step. Says what changed.",
    "Mix",
    "",
);

static DIFF: ActionMeta = meta(
    "MIX_DIFF",
    "mix_diff",
    "Compare mix snapshots",
    "What differs between snapshot `a` and `b` (a snapshot's name, or `current` — the mix as it stands, `b` unless given): each fader, pan, mute, solo, polarity, send and FX setting that is not the same, as `what: a → b`; an FX whose plugin state differs, with both sizes. `names: true` lists the song's snapshots instead. Read-only.",
    "Mix",
    "",
);

#[derive(Facet)]
struct NameArg {
    name: String,
}

#[derive(Facet)]
struct DiffArg {
    a: Option<String>,
    b: Option<String>,
    names: Option<bool>,
}

/// A track's mix.
#[derive(Facet, Clone, Debug, PartialEq)]
struct TrackMix {
    guid: String,
    path: String,
    volume_db: f64,
    pan: f64,
    mute: bool,
    solo: bool,
    phase: bool,
}

/// A send's mix.
#[derive(Facet, Clone, Debug, PartialEq)]
struct SendMix {
    from: String,
    to: String,
    /// Its place among the sends from `from` to `to`.
    nth: u32,
    level_db: f64,
    pan: f64,
    mute: bool,
    tap: String,
}

/// An FX's mix.
#[derive(Facet, Clone, Debug, PartialEq)]
struct FxMix {
    guid: String,
    /// The track whose chain it is on.
    track: String,
    name: String,
    on: bool,
    /// Held parameter values: `id=value`, sorted.
    params: Vec<String>,
    /// The plugin's state, base64 (empty when it keeps none).
    state: String,
}

/// A whole mix.
#[derive(Facet, Clone, Debug, PartialEq, Default)]
pub(crate) struct Mix {
    name: String,
    taken: String,
    master_db: f64,
    master_pan: f64,
    master_mute: bool,
    tracks: Vec<TrackMix>,
    sends: Vec<SendMix>,
    fx: Vec<FxMix>,
}

/// A snapshot kept.
#[derive(Facet)]
pub struct SnapshotRow {
    pub name: String,
    pub tracks: u32,
    pub sends: u32,
    pub fx: u32,
    pub states: u32,
    pub bytes: u32,
    pub ms: f64,
}

/// A snapshot put back.
#[derive(Facet)]
pub struct RestoreRow {
    pub name: String,
    pub changed: Vec<String>,
    pub missing: Vec<String>,
    pub states_loaded: u32,
    pub ms: f64,
}

/// Two mixes compared.
#[derive(Facet)]
pub struct DiffRow {
    pub a: String,
    pub b: String,
    pub differences: Vec<String>,
    pub names: Vec<String>,
    pub ms: f64,
}

/// Two decimals.
fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// The mix as it stands on song `song`.
fn take(daw: &Standalone, project: &ProjectContext, song: &str, name: &str) -> Mix {
    let tracks = Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&tracks);
    let path = |g: &str| paths.get(g).cloned().unwrap_or_else(|| g.to_owned());
    let mut sends = Vec::new();
    for t in &tracks {
        let mut nth: HashMap<String, u32> = HashMap::new();
        for r in Routing::sends(daw, project.clone(), TrackRef::Guid(t.guid.clone())) {
            let Some(to) = r.dest_track_guid.clone() else {
                continue;
            };
            let n = nth.entry(to.clone()).or_default();
            sends.push(SendMix {
                from: t.guid.clone(),
                to,
                nth: *n,
                level_db: r2(db(r.volume)),
                pan: r2(r.pan),
                mute: r.muted,
                tap: tap_word(r.send_mode).to_owned(),
            });
            *n += 1;
        }
    }
    // Every FX on a track's chain, its held values: one read.
    let held: Vec<(String, daw_proto::fx::Fx, Vec<String>)> = daw
        .read_project(song, |p| {
            p.fx_chains
                .iter()
                .filter_map(|(key, chain)| match key {
                    FxChainKey::Track(t) => Some((t.clone(), chain)),
                    _ => None,
                })
                .flat_map(|(t, chain)| {
                    chain.iter().map(move |e| {
                        let mut params: Vec<String> = e
                            .params
                            .iter()
                            .map(|(id, v)| format!("{id}={v:.6}"))
                            .collect();
                        params.sort();
                        (t.clone(), e.fx.clone(), params)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let fx = held
        .into_iter()
        .map(|(track, f, params)| FxMix {
            name: crate::clap_fx::display_name(&f),
            state: daw
                .save_plugin_state(&f.guid)
                .map(|s| base64::engine::general_purpose::STANDARD.encode(s))
                .unwrap_or_default(),
            on: f.enabled,
            guid: f.guid,
            track,
            params,
        })
        .collect();
    let (master_volume, master_pan, master_muted) = daw
        .read_project(song, |p| (p.master_volume, p.master_pan, p.master_muted))
        .unwrap_or((1.0, 0.0, false));
    Mix {
        name: name.to_owned(),
        taken: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        master_db: r2(db(master_volume)),
        master_pan: r2(master_pan),
        master_mute: master_muted,
        tracks: tracks
            .iter()
            .map(|t| TrackMix {
                guid: t.guid.clone(),
                path: path(&t.guid),
                volume_db: r2(db(t.volume)),
                pan: r2(t.pan),
                mute: t.muted,
                solo: t.soloed,
                phase: t.phase_inverted,
            })
            .collect(),
        sends,
        fx,
    }
}

fn key(name: &str) -> String {
    format!("snapshot:{}", name.trim())
}

/// The song's snapshot names.
fn names(daw: &Standalone, project: &ProjectContext) -> Vec<String> {
    daw_proto::ExtState::get_project(daw, project.clone(), SECTION, NAMES)
        .map(|t| {
            t.lines()
                .map(str::to_owned)
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Snapshot `name`.
fn load(daw: &Standalone, project: &ProjectContext, name: &str) -> Result<Mix, String> {
    let text = daw_proto::ExtState::get_project(daw, project.clone(), SECTION, &key(name))
        .ok_or_else(|| {
            format!(
                "no snapshot {name:?} — the song has {:?}",
                names(daw, project)
            )
        })?;
    facet_json::from_str::<Mix>(&text).map_err(|e| format!("snapshot {name:?}: {e}"))
}

/// `MIX_SNAPSHOT`.
fn snapshot(daw: &Standalone, project: &ProjectContext, name: &str) -> Result<SnapshotRow, String> {
    let span = tracing::info_span!(
        "MIX_SNAPSHOT",
        mix.tracks = tracing::field::Empty,
        mix.sends = tracing::field::Empty,
        mix.fx = tracing::field::Empty,
        mix.states = tracing::field::Empty,
        mix.bytes = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    if name.trim().is_empty() || name.contains('\n') || name.eq_ignore_ascii_case("current") {
        return Err(format!("{name:?} is not a snapshot name"));
    }
    let song = crate::fx_actions::project_guid(project)?;
    let mix = take(daw, project, &song, name.trim());
    let text = facet_json::to_string(&mix).map_err(|e| e.to_string())?;
    let mut all = names(daw, project);
    if !all.iter().any(|n| n == name.trim()) {
        all.push(name.trim().to_owned());
    }
    let label = format!("Mix snapshot {name}");
    daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    let kept = daw_proto::ExtState::set_project(daw, project.clone(), SECTION, &key(name), &text)
        .and_then(|()| {
            daw_proto::ExtState::set_project(daw, project.clone(), SECTION, NAMES, &all.join("\n"))
        });
    daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    kept.map_err(|e| e.to_string())?;
    let out = SnapshotRow {
        name: name.trim().to_owned(),
        tracks: u32::try_from(mix.tracks.len()).unwrap_or(u32::MAX),
        sends: u32::try_from(mix.sends.len()).unwrap_or(u32::MAX),
        fx: u32::try_from(mix.fx.len()).unwrap_or(u32::MAX),
        states: u32::try_from(mix.fx.iter().filter(|f| !f.state.is_empty()).count())
            .unwrap_or(u32::MAX),
        bytes: u32::try_from(text.len()).unwrap_or(u32::MAX),
        ms: began.elapsed().as_secs_f64() * 1e3,
    };
    span.record("mix.tracks", out.tracks);
    span.record("mix.sends", out.sends);
    span.record("mix.fx", out.fx);
    span.record("mix.states", out.states);
    span.record("mix.bytes", out.bytes);
    span.record("mix.ms", out.ms);
    Ok(out)
}

/// `MIX_RESTORE`.
fn restore(daw: &Standalone, project: &ProjectContext, name: &str) -> Result<RestoreRow, String> {
    let want = load(daw, project, name)?;
    put(daw, project, name, &want)
}

/// The mix as it stands, kept in memory (not on the song): what [`put`]
/// puts back.
///
/// # Errors
/// The song has no GUID.
pub(crate) fn current(daw: &Standalone, project: &ProjectContext) -> Result<Mix, String> {
    let song = crate::fx_actions::project_guid(project)?;
    Ok(take(daw, project, &song, "current"))
}

/// Put the mix back as `want` has it (`name` its label), as MIX_RESTORE
/// does: only what differs, one undo step.
///
/// # Errors
/// A track, send or FX would not take its setting.
pub(crate) fn put(
    daw: &Standalone,
    project: &ProjectContext,
    name: &str,
    want: &Mix,
) -> Result<RestoreRow, String> {
    let span = tracing::info_span!(
        "MIX_RESTORE",
        mix.changed = tracing::field::Empty,
        mix.missing = tracing::field::Empty,
        mix.states = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let song = crate::fx_actions::project_guid(project)?;
    let now = take(daw, project, &song, "current");
    let mut changed = Vec::new();
    let mut missing = Vec::new();
    let mut states_loaded = 0u32;
    let label = format!("Restore mix {name}");
    daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    let failed = |e: daw_proto::DawError| e.to_string();
    let done = (|| -> Result<(), String> {
        // The master.
        if (now.master_db, now.master_pan, now.master_mute)
            != (want.master_db, want.master_pan, want.master_mute)
        {
            daw.write_project(&song, |p| {
                p.master_volume = gain(want.master_db);
                p.master_pan = want.master_pan;
                p.master_muted = want.master_mute;
            });
            changed.push("MASTER".to_owned());
        }
        // The tracks.
        let current: HashMap<&str, &TrackMix> =
            now.tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
        for t in &want.tracks {
            let Some(is) = current.get(t.guid.as_str()) else {
                missing.push(format!("track {}", t.path));
                continue;
            };
            if *is == t {
                continue;
            }
            let at = || TrackRef::Guid(t.guid.clone());
            if is.volume_db != t.volume_db {
                Tracks::set_volume(daw, project.clone(), at(), gain(t.volume_db))
                    .map_err(failed)?;
            }
            if is.pan != t.pan {
                Tracks::set_pan(daw, project.clone(), at(), t.pan).map_err(failed)?;
            }
            if is.mute != t.mute {
                Tracks::set_muted(daw, project.clone(), at(), t.mute).map_err(failed)?;
            }
            if is.solo != t.solo {
                Tracks::set_soloed(daw, project.clone(), at(), t.solo).map_err(failed)?;
            }
            if is.phase != t.phase {
                Tracks::set_phase_inverted(daw, project.clone(), at(), t.phase).map_err(failed)?;
            }
            changed.push(t.path.clone());
        }
        // The sends, pair by pair of tracks.
        restore_sends(daw, project, (&now, want), (&mut changed, &mut missing))?;
        // The FX.
        let fxs: HashMap<&str, &FxMix> = now.fx.iter().map(|f| (f.guid.as_str(), f)).collect();
        for f in &want.fx {
            let Some(is) = fxs.get(f.guid.as_str()) else {
                missing.push(format!("fx {} on {}", f.name, path_in(want, &f.track)));
                continue;
            };
            if *is == f {
                continue;
            }
            let target = FxTarget {
                context: FxChainContext::Track(f.track.clone()),
                fx: FxRef::Guid(f.guid.clone()),
            };
            if is.state != f.state && !f.state.is_empty() {
                let state = base64::engine::general_purpose::STANDARD
                    .decode(&f.state)
                    .map_err(|e| e.to_string())?;
                daw.apply_plugin_state(&f.guid, &state)
                    .map_err(|e| e.to_string())?;
                states_loaded += 1;
            }
            if is.params != f.params {
                let held: HashMap<u32, f64> = f
                    .params
                    .iter()
                    .filter_map(|p| {
                        let (id, v) = p.split_once('=')?;
                        Some((id.parse().ok()?, v.parse().ok()?))
                    })
                    .collect();
                daw.write_project(&song, |p| {
                    for e in p.fx_chains.values_mut().flatten() {
                        if e.fx.guid == f.guid {
                            e.params.clone_from(&held);
                        }
                    }
                });
            }
            if is.on != f.on {
                Effects::set_enabled(daw, project.clone(), target, f.on).map_err(failed)?;
            }
            changed.push(format!("fx {} on {}", f.name, path_in(want, &f.track)));
        }
        Ok(())
    })();
    daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    done?;
    crate::actions::list().changed("track", "");
    let out = RestoreRow {
        name: name.to_owned(),
        changed,
        missing,
        states_loaded,
        ms: began.elapsed().as_secs_f64() * 1e3,
    };
    span.record("mix.changed", out.changed.len());
    span.record("mix.missing", out.missing.len());
    span.record("mix.states", out.states_loaded);
    span.record("mix.ms", out.ms);
    Ok(out)
}

/// A track's path as `mix` knew it.
fn path_in(mix: &Mix, guid: &str) -> String {
    mix.tracks
        .iter()
        .find(|t| t.guid == guid)
        .map_or_else(|| guid.to_owned(), |t| t.path.clone())
}

/// The sends put back: between every pair of tracks either mix has sends
/// between, the `nth` set as `want` has it, added where missing, taken off
/// past what `want` has.
fn restore_sends(
    daw: &Standalone,
    project: &ProjectContext,
    (now, want): (&Mix, &Mix),
    (changed, missing): (&mut Vec<String>, &mut Vec<String>),
) -> Result<(), String> {
    let pairs = |m: &Mix| -> BTreeMap<(String, String), Vec<SendMix>> {
        let mut out: BTreeMap<(String, String), Vec<SendMix>> = BTreeMap::new();
        for s in &m.sends {
            out.entry((s.from.clone(), s.to.clone()))
                .or_default()
                .push(s.clone());
        }
        out
    };
    let (is, should) = (pairs(now), pairs(want));
    let known: std::collections::HashSet<&str> =
        now.tracks.iter().map(|t| t.guid.as_str()).collect();
    let keys: std::collections::BTreeSet<&(String, String)> =
        is.keys().chain(should.keys()).collect();
    for (from, to) in keys {
        let label = format!("send {} → {}", path_in(want, from), path_in(want, to));
        if !known.contains(from.as_str()) || !known.contains(to.as_str()) {
            missing.push(label);
            continue;
        }
        let have = is
            .get(&(from.clone(), to.clone()))
            .cloned()
            .unwrap_or_default();
        let need = should
            .get(&(from.clone(), to.clone()))
            .cloned()
            .unwrap_or_default();
        if have == need {
            continue;
        }
        // Each one's index among `from`'s sends, in order.
        let indexes: Vec<u32> = Routing::sends(daw, project.clone(), TrackRef::Guid(from.clone()))
            .into_iter()
            .filter(|r| r.dest_track_guid.as_deref() == Some(to.as_str()))
            .map(|r| r.index)
            .collect();
        // Past what is wanted: off, last first.
        for &index in indexes.iter().skip(need.len()).rev() {
            Routing::remove_route(
                daw,
                project.clone(),
                RouteLocation::send(TrackRef::Guid(from.clone()), RouteRef::Index(index)),
            )
            .map_err(|e| e.to_string())?;
        }
        for (i, s) in need.iter().enumerate() {
            let mode = tap_of(Some(&s.tap), None)?.unwrap_or(SendMode::PostFader);
            let index = match indexes.get(i) {
                Some(&index) => index,
                None => crate::mix_sends::add_send(daw, project, (from, to), s.level_db, mode)?,
            };
            let at = RouteLocation::new(
                TrackRef::Guid(from.clone()),
                RouteType::Send,
                RouteRef::Index(index),
            );
            Routing::set_volume(daw, project.clone(), at.clone(), gain(s.level_db))
                .map_err(|e| e.to_string())?;
            Routing::set_pan(daw, project.clone(), at.clone(), s.pan).map_err(|e| e.to_string())?;
            Routing::set_muted(daw, project.clone(), at, s.mute).map_err(|e| e.to_string())?;
            Routing::set_send_mode(
                daw,
                project.clone(),
                TrackRef::Guid(from.clone()),
                RouteRef::Index(index),
                mode,
            )
            .map_err(|e| e.to_string())?;
        }
        changed.push(label);
    }
    Ok(())
}

/// `what: x → y`, when they differ.
fn note(out: &mut Vec<String>, what: String, x: String, y: String) {
    if x != y {
        out.push(format!("{what}: {x} → {y}"));
    }
}

/// What differs from `a` to `b`.
fn differences(a: &Mix, b: &Mix) -> Vec<String> {
    let mut out = Vec::new();
    note(
        &mut out,
        "MASTER volume".into(),
        format!("{} dB", a.master_db),
        format!("{} dB", b.master_db),
    );
    note(
        &mut out,
        "MASTER pan".into(),
        a.master_pan.to_string(),
        b.master_pan.to_string(),
    );
    note(
        &mut out,
        "MASTER mute".into(),
        a.master_mute.to_string(),
        b.master_mute.to_string(),
    );
    let theirs: HashMap<&str, &TrackMix> = b.tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    for t in &a.tracks {
        let Some(u) = theirs.get(t.guid.as_str()) else {
            out.push(format!("{}: only in {}", t.path, a.name));
            continue;
        };
        note(
            &mut out,
            format!("{} volume", t.path),
            format!("{} dB", t.volume_db),
            format!("{} dB", u.volume_db),
        );
        note(
            &mut out,
            format!("{} pan", t.path),
            t.pan.to_string(),
            u.pan.to_string(),
        );
        note(
            &mut out,
            format!("{} mute", t.path),
            t.mute.to_string(),
            u.mute.to_string(),
        );
        note(
            &mut out,
            format!("{} solo", t.path),
            t.solo.to_string(),
            u.solo.to_string(),
        );
        note(
            &mut out,
            format!("{} phase", t.path),
            t.phase.to_string(),
            u.phase.to_string(),
        );
    }
    let send_key = |s: &SendMix| (s.from.clone(), s.to.clone(), s.nth);
    let theirs: HashMap<(String, String, u32), &SendMix> =
        b.sends.iter().map(|s| (send_key(s), s)).collect();
    let ours: HashMap<(String, String, u32), &SendMix> =
        a.sends.iter().map(|s| (send_key(s), s)).collect();
    for s in &a.sends {
        let what = format!("send {} → {}", path_in(a, &s.from), path_in(a, &s.to));
        match theirs.get(&send_key(s)) {
            None => note(&mut out, what, "there".into(), "none".into()),
            Some(u) => {
                note(
                    &mut out,
                    format!("{what} level"),
                    format!("{} dB", s.level_db),
                    format!("{} dB", u.level_db),
                );
                note(
                    &mut out,
                    format!("{what} pan"),
                    s.pan.to_string(),
                    u.pan.to_string(),
                );
                note(
                    &mut out,
                    format!("{what} mute"),
                    s.mute.to_string(),
                    u.mute.to_string(),
                );
                note(
                    &mut out,
                    format!("{what} tap"),
                    s.tap.clone(),
                    u.tap.clone(),
                );
            }
        }
    }
    for s in b.sends.iter().filter(|s| !ours.contains_key(&send_key(s))) {
        note(
            &mut out,
            format!("send {} → {}", path_in(b, &s.from), path_in(b, &s.to)),
            "none".into(),
            format!("{} dB {}", s.level_db, s.tap),
        );
    }
    let theirs: HashMap<&str, &FxMix> = b.fx.iter().map(|f| (f.guid.as_str(), f)).collect();
    for f in &a.fx {
        let what = format!("fx {} on {}", f.name, path_in(a, &f.track));
        let Some(u) = theirs.get(f.guid.as_str()) else {
            out.push(format!("{what}: only in {}", a.name));
            continue;
        };
        note(
            &mut out,
            format!("{what} on"),
            f.on.to_string(),
            u.on.to_string(),
        );
        if f.params != u.params {
            let ours: std::collections::BTreeSet<&String> = f.params.iter().collect();
            let theirs: std::collections::BTreeSet<&String> = u.params.iter().collect();
            out.push(format!(
                "{what} params: {:?} → {:?}",
                ours.difference(&theirs).collect::<Vec<_>>(),
                theirs.difference(&ours).collect::<Vec<_>>()
            ));
        }
        if f.state != u.state {
            let size = |s: &str| {
                base64::engine::general_purpose::STANDARD
                    .decode(s)
                    .map_or(0, |b| b.len())
            };
            out.push(format!(
                "{what} state: {} bytes → {} bytes (differs)",
                size(&f.state),
                size(&u.state)
            ));
        }
    }
    let ours: HashMap<&str, &FxMix> = a.fx.iter().map(|f| (f.guid.as_str(), f)).collect();
    for f in b.fx.iter().filter(|f| !ours.contains_key(f.guid.as_str())) {
        out.push(format!(
            "fx {} on {}: only in {}",
            f.name,
            path_in(b, &f.track),
            b.name
        ));
    }
    out
}

/// `MIX_DIFF`.
fn diff(daw: &Standalone, project: &ProjectContext, a: &DiffArg) -> Result<DiffRow, String> {
    let span = tracing::info_span!(
        "MIX_DIFF",
        mix.differences = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let all = names(daw, project);
    if a.names.unwrap_or(false) {
        return Ok(DiffRow {
            a: String::new(),
            b: String::new(),
            differences: Vec::new(),
            names: all,
            ms: began.elapsed().as_secs_f64() * 1e3,
        });
    }
    let song = crate::fx_actions::project_guid(project)?;
    let get = |name: &str| -> Result<Mix, String> {
        if name.eq_ignore_ascii_case("current") {
            Ok(take(daw, project, &song, "current"))
        } else {
            load(daw, project, name)
        }
    };
    let an =
        a.a.clone()
            .ok_or("`a` names the snapshot to compare (or `names: true`)")?;
    let bn = a.b.clone().unwrap_or_else(|| "current".to_owned());
    let (x, y) = (get(&an)?, get(&bn)?);
    let differences = differences(&x, &y);
    span.record("mix.differences", differences.len());
    span.record("mix.ms", began.elapsed().as_secs_f64() * 1e3);
    Ok(DiffRow {
        a: an,
        b: bn,
        differences,
        names: all,
        ms: began.elapsed().as_secs_f64() * 1e3,
    })
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SNAPSHOT, Effect::Mutating, |a: NameArg| {
        let (daw, project) = song()?;
        snapshot(daw, &project, &a.name)
    })?;
    list.register(&RESTORE, Effect::Mutating, |a: NameArg| {
        let (daw, project) = song()?;
        restore(daw, &project, &a.name)
    })?;
    list.register(&DIFF, Effect::ReadOnly, |a: DiffArg| {
        let (daw, project) = song()?;
        diff(daw, &project, &a)
    })?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
