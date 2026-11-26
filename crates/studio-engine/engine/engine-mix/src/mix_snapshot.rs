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
//!
//! A snapshot can be **scoped** (`scope`: tracks, each bringing the tracks
//! inside it — `Drums/Kick` is the kick's subtree; the master only when
//! unscoped) and **filtered** (`only`: the categories in [`Only`]), on save
//! and again on recall: a recall changes only what both allow. Each records
//! its **parent** (the snapshot or variant it came from, or the one last
//! restored or kept: the song's `head`), so a mix's history branches and
//! MIX_DIFF compares across it. Variants of one decision, auditioned and
//! voted on, are `crate::mix_variants`, built on this.

use std::collections::{BTreeMap, HashMap, HashSet};

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
/// The snapshot (or `group:variant`) last taken, restored or kept: a new
/// snapshot's parent unless it names one.
const HEAD: &str = "head";

static SNAPSHOT: ActionMeta = meta(
    "MIX_SNAPSHOT",
    "mix_snapshot",
    "Snapshot the mix",
    "Keep the mix as it stands on the song under `name`: every track's fader, pan, mute, solo and polarity, the master's, every send (level, pan, mute, tap), and every FX (on or off, its held parameter values, its plugin's own state — a CLAP's state blob). `scope` (tracks: paths, names or GUIDs, each with the tracks inside it) keeps only those (the master only when unscoped); `only` (volume, pan, mute, solo, polarity, sends, fx-enabled, fx-params, fx-state; `fx` for the three) keeps only those categories, and a restore then changes only them. `parent` names the snapshot it comes from (default: the one last taken, restored or kept). A snapshot of that name is replaced. Saved with the song. Says what it holds and its size.",
    "Mix",
    "",
);

static RESTORE: ActionMeta = meta(
    "MIX_RESTORE",
    "mix_restore",
    "Restore a mix snapshot",
    "Put the mix back as snapshot `name` has it (`group:variant` names a mix variant): faders, pans, mutes, solos, polarity, the master, the sends (set again, added where missing, taken off where the snapshot has none between the two tracks), every FX on or off, its parameter values and its plugin's state (loaded only where it differs). Only the snapshot's scope and categories are touched; `only` (as MIX_SNAPSHOT) narrows the categories further. Tracks and FX gone since are named, not made again. One undo step. Says what changed.",
    "Mix",
    "",
);

static DIFF: ActionMeta = meta(
    "MIX_DIFF",
    "mix_diff",
    "Compare mix snapshots",
    "What differs between snapshot `a` and `b` (a snapshot's name, `group:variant` for a mix variant, or `current` — the mix as it stands, `b` unless given): each fader, pan, mute, solo, polarity, send and FX setting that is not the same, as `what: a → b`; an FX whose plugin state differs, with both sizes. `only` (as MIX_SNAPSHOT) compares only those categories. `names: true` lists the song's snapshots instead, and `parents` each one's parent (`name ← parent`) and `head` the last taken, restored or kept. Read-only.",
    "Mix",
    "",
);

#[derive(Facet, Default)]
struct SnapshotArg {
    name: String,
    scope: Option<Vec<String>>,
    only: Option<Vec<String>>,
    parent: Option<String>,
}

#[derive(Facet, Default)]
struct RestoreArg {
    name: String,
    only: Option<Vec<String>>,
}

#[derive(Facet, Default)]
struct DiffArg {
    a: Option<String>,
    b: Option<String>,
    names: Option<bool>,
    only: Option<Vec<String>>,
}

/// The categories of a mix a snapshot keeps and a recall puts back (the
/// SWS Snapshots filters): a set of bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Only(u16);

impl Only {
    pub const VOLUME: Self = Self(1);
    pub const PAN: Self = Self(1 << 1);
    pub const MUTE: Self = Self(1 << 2);
    pub const SOLO: Self = Self(1 << 3);
    pub const POLARITY: Self = Self(1 << 4);
    pub const SENDS: Self = Self(1 << 5);
    pub const FX_ENABLED: Self = Self(1 << 6);
    pub const FX_PARAMS: Self = Self(1 << 7);
    pub const FX_STATE: Self = Self(1 << 8);
    pub const ALL: Self = Self((1 << 9) - 1);
    /// Each category's word, in order.
    pub const WORDS: [(&'static str, Self); 9] = [
        ("volume", Self::VOLUME),
        ("pan", Self::PAN),
        ("mute", Self::MUTE),
        ("solo", Self::SOLO),
        ("polarity", Self::POLARITY),
        ("sends", Self::SENDS),
        ("fx-enabled", Self::FX_ENABLED),
        ("fx-params", Self::FX_PARAMS),
        ("fx-state", Self::FX_STATE),
    ];

    /// The categories `words` name; none (or empty) is all of them.
    ///
    /// # Errors
    /// A word names no category.
    pub fn parse(words: Option<&[String]>) -> Result<Self, String> {
        let Some(words) = words.filter(|w| !w.is_empty()) else {
            return Ok(Self::ALL);
        };
        let mut out = Self(0);
        for w in words {
            let w = w.trim().to_ascii_lowercase().replace('_', "-");
            out.0 |= match w.as_str() {
                "all" => Self::ALL,
                "fader" | "volume" | "level" => Self::VOLUME,
                "phase" | "polarity" => Self::POLARITY,
                "send" | "sends" => Self::SENDS,
                "fx" => Self(Self::FX_ENABLED.0 | Self::FX_PARAMS.0 | Self::FX_STATE.0),
                "fx-on" | "fx-bypass" | "fx-enabled" => Self::FX_ENABLED,
                "params" | "fx-params" => Self::FX_PARAMS,
                "state" | "fx-state" => Self::FX_STATE,
                other => Self::WORDS
                    .iter()
                    .find(|(word, _)| *word == other)
                    .map(|(_, c)| *c)
                    .ok_or_else(|| {
                        format!(
                            "{other:?} is not a mix category — {:?}",
                            Self::WORDS.map(|(w, _)| w)
                        )
                    })?,
            }
            .0;
        }
        Ok(out)
    }

    /// The categories a mix kept (`only` as saved; empty is all).
    #[must_use]
    pub fn saved(words: &[String]) -> Self {
        Self::parse(Some(words)).unwrap_or(Self::ALL)
    }

    /// The words, empty for all (as saved).
    #[must_use]
    pub fn words(self) -> Vec<String> {
        if self == Self::ALL {
            return Vec::new();
        }
        Self::WORDS
            .iter()
            .filter(|(_, c)| self.has(*c))
            .map(|(w, _)| (*w).to_owned())
            .collect()
    }

    #[must_use]
    pub const fn has(self, c: Self) -> bool {
        self.0 & c.0 != 0
    }

    #[must_use]
    pub const fn and(self, o: Self) -> Self {
        Self(self.0 & o.0)
    }
}

/// A track's mix.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct TrackMix {
    pub guid: String,
    pub path: String,
    pub volume_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub phase: bool,
}

/// A send's mix.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct SendMix {
    pub from: String,
    pub to: String,
    /// Its place among the sends from `from` to `to`.
    pub nth: u32,
    pub level_db: f64,
    pub pan: f64,
    pub mute: bool,
    pub tap: String,
}

/// An FX's mix.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct FxMix {
    pub guid: String,
    /// The track whose chain it is on.
    pub track: String,
    pub name: String,
    pub on: bool,
    /// Held parameter values: `id=value`, sorted.
    pub params: Vec<String>,
    /// The plugin's state, base64 (empty when it keeps none).
    pub state: String,
}

/// A whole mix.
#[derive(Facet, Clone, Debug, PartialEq, Default)]
pub struct Mix {
    pub name: String,
    pub taken: String,
    pub master_db: f64,
    pub master_pan: f64,
    pub master_mute: bool,
    pub tracks: Vec<TrackMix>,
    pub sends: Vec<SendMix>,
    pub fx: Vec<FxMix>,
    /// The tracks it holds, by GUID, when scoped (empty: the whole mix,
    /// the master too).
    #[facet(default)]
    pub scope: Vec<String>,
    /// The categories it holds ([`Only::words`]; empty: all).
    #[facet(default)]
    pub only: Vec<String>,
    /// The snapshot or `group:variant` it came from (empty: none known).
    #[facet(default)]
    pub parent: String,
}

impl Mix {
    /// Its scope, as a set (`None`: the whole mix).
    pub fn scope_set(&self) -> Option<HashSet<String>> {
        (!self.scope.is_empty()).then(|| self.scope.iter().cloned().collect())
    }
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
    /// With `names`: each one's parent, `name ← parent`.
    pub parents: Vec<String>,
    /// The snapshot (or `group:variant`) last taken, restored or kept.
    pub head: String,
    pub ms: f64,
}

/// Two decimals.
fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// `roots` and every track inside each, by GUID, in track order.
pub fn subtree(tracks: &[daw_proto::Track], roots: &[String]) -> Vec<String> {
    let parent: HashMap<&str, Option<&str>> = tracks
        .iter()
        .map(|t| (t.guid.as_str(), t.parent_guid.as_deref()))
        .collect();
    let inside = |guid: &str| {
        let mut at = Some(guid);
        while let Some(g) = at {
            if roots.iter().any(|r| r == g) {
                return true;
            }
            at = parent.get(g).copied().flatten();
        }
        false
    };
    tracks
        .iter()
        .filter(|t| inside(&t.guid))
        .map(|t| t.guid.clone())
        .collect()
}

/// The tracks `scope` names (paths, names or GUIDs), each with the tracks
/// inside it: the roots and the whole set, by GUID. `None` or empty: no
/// scope.
///
/// # Errors
/// A name fits no track, or more than one.
pub fn scope_of(
    daw: &Standalone,
    project: &ProjectContext,
    scope: Option<&[String]>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let Some(names) = scope.filter(|s| !s.is_empty()) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let roots: Vec<String> = names
        .iter()
        .map(|n| crate::actions::resolve(daw, project, n))
        .collect::<Result<_, _>>()?;
    let all = Tracks::all(daw, project.clone());
    let set = subtree(&all, &roots);
    Ok((roots, set))
}

/// The mix as it stands on song `song`.
fn take(daw: &Standalone, project: &ProjectContext, song: &str, name: &str) -> Mix {
    take_scoped(daw, project, song, name, None)
}

/// The mix as it stands on song `song`, only the tracks in `scope` (their
/// sends and FX) when there is one.
pub fn take_scoped(
    daw: &Standalone,
    project: &ProjectContext,
    song: &str,
    name: &str,
    scope: Option<&HashSet<String>>,
) -> Mix {
    let inside = |g: &str| scope.is_none_or(|s| s.contains(g));
    let tracks: Vec<daw_proto::Track> = Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&tracks);
    let path = |g: &str| paths.get(g).cloned().unwrap_or_else(|| g.to_owned());
    let mut sends = Vec::new();
    for t in tracks.iter().filter(|t| inside(&t.guid)) {
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
                    FxChainKey::Track(t) if inside(t) => Some((t.clone(), chain)),
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
            .filter(|t| inside(&t.guid))
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
        scope: scope
            .map(|s| {
                tracks
                    .iter()
                    .filter(|t| s.contains(&t.guid))
                    .map(|t| t.guid.clone())
                    .collect()
            })
            .unwrap_or_default(),
        only: Vec::new(),
        parent: String::new(),
    }
}

fn key(name: &str) -> String {
    format!("snapshot:{}", name.trim())
}

/// A plugin state, kept once in the section by what it is: `state:<hash>`.
fn state_key(hash: &str) -> String {
    format!("state:{hash}")
}

/// `mix` as it is kept: each plugin state stored once, under its hash, and
/// named in the mix by it (`@<hash>`).
///
/// A song's 54 snapshots held 280 MB of plugin state, 8 MB of it unique: a
/// snapshot repeats every FX's state, most of them unchanged since the
/// last. An older snapshot, its states inline, reads as it did. (A state
/// no snapshot names any more stays: the ext state cannot be listed.)
///
/// # Errors
/// A state would not be kept, or the mix would not encode.
pub(crate) fn encode_mix(
    daw: &Standalone,
    project: &ProjectContext,
    mix: &Mix,
) -> Result<String, String> {
    let mut kept = mix.clone();
    for fx in &mut kept.fx {
        // Base64 never starts with `@`.
        if fx.state.is_empty() || fx.state.starts_with('@') {
            continue;
        }
        // Half the hash (128 bits): hex, so cut on a character.
        let hash: String = blake3::hash(fx.state.as_bytes())
            .to_hex()
            .chars()
            .take(32)
            .collect();
        let hash = hash.as_str();
        let at = state_key(hash);
        if !daw_proto::ExtState::has_project(daw, project.clone(), SECTION, &at) {
            daw_proto::ExtState::set_project(daw, project.clone(), SECTION, &at, &fx.state)
                .map_err(|e| e.to_string())?;
        }
        fx.state = format!("@{hash}");
    }
    facet_json::to_string(&kept).map_err(|e| e.to_string())
}

/// A mix as [`encode_mix`] kept it, its plugin states looked up.
///
/// # Errors
/// The text is not a mix, or a state it names is gone.
pub(crate) fn decode_mix(
    daw: &Standalone,
    project: &ProjectContext,
    text: &str,
) -> Result<Mix, String> {
    let mut mix = facet_json::from_str::<Mix>(text).map_err(|e| e.to_string())?;
    for fx in &mut mix.fx {
        if let Some(hash) = fx.state.strip_prefix('@') {
            fx.state =
                daw_proto::ExtState::get_project(daw, project.clone(), SECTION, &state_key(hash))
                    .ok_or_else(|| format!("{}: its plugin state {hash} is gone", fx.name))?;
        }
    }
    Ok(mix)
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

/// Snapshot `name`, or — with no snapshot of that name — variant
/// `group:variant` of a mix variant group.
pub fn load(daw: &Standalone, project: &ProjectContext, name: &str) -> Result<Mix, String> {
    let Some(text) = daw_proto::ExtState::get_project(daw, project.clone(), SECTION, &key(name))
    else {
        if let Some((group, variant)) = name.split_once(':') {
            return crate::mix_variants::load_mix(daw, project, group.trim(), variant.trim());
        }
        return Err(format!(
            "no snapshot {name:?} — the song has {:?}",
            names(daw, project)
        ));
    };
    decode_mix(daw, project, &text).map_err(|e| format!("snapshot {name:?}: {e}"))
}

/// The snapshot (or `group:variant`) last taken, restored or kept.
pub fn head(daw: &Standalone, project: &ProjectContext) -> String {
    daw_proto::ExtState::get_project(daw, project.clone(), SECTION, HEAD).unwrap_or_default()
}

/// Say `name` is the song's head now (not an undo step: bookkeeping).
pub fn set_head(daw: &Standalone, project: &ProjectContext, name: &str) {
    if let Err(e) = daw_proto::ExtState::set_project(daw, project.clone(), SECTION, HEAD, name) {
        tracing::warn!(mix.error = %e, "mix: the head would not be kept");
    }
}

/// `MIX_SNAPSHOT`, of the whole mix.
#[cfg(test)]
fn snapshot(daw: &Standalone, project: &ProjectContext, name: &str) -> Result<SnapshotRow, String> {
    snapshot_with(
        daw,
        project,
        &SnapshotArg {
            name: name.to_owned(),
            ..SnapshotArg::default()
        },
    )
}

/// `MIX_SNAPSHOT`.
fn snapshot_with(
    daw: &Standalone,
    project: &ProjectContext,
    a: &SnapshotArg,
) -> Result<SnapshotRow, String> {
    let name = a.name.as_str();
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
    let only = Only::parse(a.only.as_deref())?;
    let (_, scope) = scope_of(daw, project, a.scope.as_deref())?;
    let scope: Option<HashSet<String>> = (!scope.is_empty()).then(|| scope.into_iter().collect());
    let song = crate::fx_actions::project_guid(project)?;
    let mut mix = take_scoped(daw, project, &song, name.trim(), scope.as_ref());
    mix.only = only.words();
    mix.parent = a
        .parent
        .clone()
        .unwrap_or_else(|| head(daw, project))
        .trim()
        .to_owned();
    let mut all = names(daw, project);
    if !all.iter().any(|n| n == name.trim()) {
        all.push(name.trim().to_owned());
    }
    let label = format!("Mix snapshot {name}");
    daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    let kept = encode_mix(daw, project, &mix).and_then(|text| {
        daw_proto::ExtState::set_project(daw, project.clone(), SECTION, &key(name), &text)
            .and_then(|()| {
                daw_proto::ExtState::set_project(
                    daw,
                    project.clone(),
                    SECTION,
                    NAMES,
                    &all.join("\n"),
                )
            })
            .map(|()| text.len())
            .map_err(|e| e.to_string())
    });
    daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    let kept_bytes = kept?;
    set_head(daw, project, name.trim());
    let out = SnapshotRow {
        name: name.trim().to_owned(),
        tracks: u32::try_from(mix.tracks.len()).unwrap_or(u32::MAX),
        sends: u32::try_from(mix.sends.len()).unwrap_or(u32::MAX),
        fx: u32::try_from(mix.fx.len()).unwrap_or(u32::MAX),
        states: u32::try_from(mix.fx.iter().filter(|f| !f.state.is_empty()).count())
            .unwrap_or(u32::MAX),
        bytes: u32::try_from(kept_bytes).unwrap_or(u32::MAX),
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

/// `MIX_RESTORE`, every category.
#[cfg(test)]
fn restore(daw: &Standalone, project: &ProjectContext, name: &str) -> Result<RestoreRow, String> {
    restore_with(daw, project, name, Only::ALL)
}

/// `MIX_RESTORE`, only the categories `only` allows.
fn restore_with(
    daw: &Standalone,
    project: &ProjectContext,
    name: &str,
    only: Only,
) -> Result<RestoreRow, String> {
    let want = load(daw, project, name)?;
    let row = put_with(
        daw,
        project,
        name,
        &want,
        Put {
            undo: true,
            keep: false,
            only,
        },
    )?;
    set_head(daw, project, name.trim());
    Ok(row)
}

/// The mix as it stands, kept in memory (not on the song): what [`put`]
/// puts back.
///
/// # Errors
/// The song has no GUID.
pub fn current(daw: &Standalone, project: &ProjectContext) -> Result<Mix, String> {
    let song = crate::fx_actions::project_guid(project)?;
    Ok(take(daw, project, &song, "current"))
}

/// How [`put_with`] puts a mix back.
#[derive(Clone, Copy, Debug)]
pub struct Put {
    /// As one undo step (MIX_RESTORE, a variant kept), or as no step at
    /// all (a variant auditioned: switched to dozens of times).
    pub undo: bool,
    /// Its undo step says a variant was kept, not a snapshot restored.
    pub keep: bool,
    /// The categories put back (with what `want` itself holds).
    pub only: Only,
}

/// Put the mix back as `want` has it (`name` its label), as MIX_RESTORE
/// does: only what differs, one undo step.
///
/// # Errors
/// A track, send or FX would not take its setting.
pub fn put(
    daw: &Standalone,
    project: &ProjectContext,
    name: &str,
    want: &Mix,
) -> Result<RestoreRow, String> {
    put_with(
        daw,
        project,
        name,
        want,
        Put {
            undo: true,
            keep: false,
            only: Only::ALL,
        },
    )
}

/// Put the mix back as `want` has it: only `want`'s scope (the master
/// when it has none), only the categories both `how` and `want` allow,
/// only what differs; one undo step or none ([`Put::undo`]).
///
/// # Errors
/// A track, send or FX would not take its setting.
pub fn put_with(
    daw: &Standalone,
    project: &ProjectContext,
    name: &str,
    want: &Mix,
    how: Put,
) -> Result<RestoreRow, String> {
    let only = how.only.and(Only::saved(&want.only));
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
    let scope = want.scope_set();
    let now = take_scoped(daw, project, &song, "current", scope.as_ref());
    let known: HashSet<String> = Tracks::all(daw, project.clone())
        .into_iter()
        .map(|t| t.guid)
        .collect();
    let mut changed = Vec::new();
    let mut missing = Vec::new();
    let mut states_loaded = 0u32;
    let label = if how.keep {
        format!("Keep mix variant {name}")
    } else {
        format!("Restore mix {name}")
    };
    if how.undo {
        daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    }
    let failed = |e: daw_proto::DawError| e.to_string();
    let done = (|| -> Result<(), String> {
        // The master, when the mix is unscoped.
        let master = |m: &Mix| {
            (
                only.has(Only::VOLUME).then_some(m.master_db),
                only.has(Only::PAN).then_some(m.master_pan),
                only.has(Only::MUTE).then_some(m.master_mute),
            )
        };
        if scope.is_none() && master(&now) != master(want) {
            daw.write_project(&song, |p| {
                if only.has(Only::VOLUME) {
                    p.master_volume = gain(want.master_db);
                }
                if only.has(Only::PAN) {
                    p.master_pan = want.master_pan;
                }
                if only.has(Only::MUTE) {
                    p.master_muted = want.master_mute;
                }
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
            let mut did = false;
            if only.has(Only::VOLUME) && is.volume_db != t.volume_db {
                Tracks::set_volume(daw, project.clone(), at(), gain(t.volume_db))
                    .map_err(failed)?;
                did = true;
            }
            if only.has(Only::PAN) && is.pan != t.pan {
                Tracks::set_pan(daw, project.clone(), at(), t.pan).map_err(failed)?;
                did = true;
            }
            if only.has(Only::MUTE) && is.mute != t.mute {
                Tracks::set_muted(daw, project.clone(), at(), t.mute).map_err(failed)?;
                did = true;
            }
            if only.has(Only::SOLO) && is.solo != t.solo {
                Tracks::set_soloed(daw, project.clone(), at(), t.solo).map_err(failed)?;
                did = true;
            }
            if only.has(Only::POLARITY) && is.phase != t.phase {
                Tracks::set_phase_inverted(daw, project.clone(), at(), t.phase).map_err(failed)?;
                did = true;
            }
            if did {
                changed.push(t.path.clone());
            }
        }
        // The sends, pair by pair of tracks.
        if only.has(Only::SENDS) {
            restore_sends(
                daw,
                project,
                (&now, want, &known),
                (&mut changed, &mut missing),
            )?;
        }
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
            let mut did = false;
            if only.has(Only::FX_STATE) && is.state != f.state && !f.state.is_empty() {
                let state = base64::engine::general_purpose::STANDARD
                    .decode(&f.state)
                    .map_err(|e| e.to_string())?;
                daw.apply_plugin_state(&f.guid, &state)
                    .map_err(|e| e.to_string())?;
                states_loaded += 1;
                did = true;
            }
            if only.has(Only::FX_PARAMS) && is.params != f.params {
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
                did = true;
            }
            if only.has(Only::FX_ENABLED) && is.on != f.on {
                Effects::set_enabled(daw, project.clone(), target, f.on).map_err(failed)?;
                did = true;
            }
            if did {
                changed.push(format!("fx {} on {}", f.name, path_in(want, &f.track)));
            }
        }
        Ok(())
    })();
    if how.undo {
        daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    }
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
    (now, want, known): (&Mix, &Mix, &HashSet<String>),
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
    let keys: std::collections::BTreeSet<&(String, String)> =
        is.keys().chain(should.keys()).collect();
    for (from, to) in keys {
        let label = format!("send {} → {}", path_in(want, from), path_in(want, to));
        if !known.contains(from) || !known.contains(to) {
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

/// What differs from `a` to `b`, in the categories `only` (and both
/// mixes) allow; the master only when neither is scoped.
pub fn differences(a: &Mix, b: &Mix, only: Only) -> Vec<String> {
    let mut out = Vec::new();
    let only = only.and(Only::saved(&a.only)).and(Only::saved(&b.only));
    // `note`, when `c` is compared.
    let note = |out: &mut Vec<String>, c: Only, what: String, x: String, y: String| {
        if only.has(c) {
            note(out, what, x, y);
        }
    };
    let unscoped = a.scope.is_empty() && b.scope.is_empty();
    note(
        &mut out,
        if unscoped { Only::VOLUME } else { Only(0) },
        "MASTER volume".into(),
        format!("{} dB", a.master_db),
        format!("{} dB", b.master_db),
    );
    note(
        &mut out,
        if unscoped { Only::PAN } else { Only(0) },
        "MASTER pan".into(),
        a.master_pan.to_string(),
        b.master_pan.to_string(),
    );
    note(
        &mut out,
        if unscoped { Only::MUTE } else { Only(0) },
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
            Only::VOLUME,
            format!("{} volume", t.path),
            format!("{} dB", t.volume_db),
            format!("{} dB", u.volume_db),
        );
        note(
            &mut out,
            Only::PAN,
            format!("{} pan", t.path),
            t.pan.to_string(),
            u.pan.to_string(),
        );
        note(
            &mut out,
            Only::MUTE,
            format!("{} mute", t.path),
            t.mute.to_string(),
            u.mute.to_string(),
        );
        note(
            &mut out,
            Only::SOLO,
            format!("{} solo", t.path),
            t.solo.to_string(),
            u.solo.to_string(),
        );
        note(
            &mut out,
            Only::POLARITY,
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
            None => note(&mut out, Only::SENDS, what, "there".into(), "none".into()),
            Some(u) => {
                note(
                    &mut out,
                    Only::SENDS,
                    format!("{what} level"),
                    format!("{} dB", s.level_db),
                    format!("{} dB", u.level_db),
                );
                note(
                    &mut out,
                    Only::SENDS,
                    format!("{what} pan"),
                    s.pan.to_string(),
                    u.pan.to_string(),
                );
                note(
                    &mut out,
                    Only::SENDS,
                    format!("{what} mute"),
                    s.mute.to_string(),
                    u.mute.to_string(),
                );
                note(
                    &mut out,
                    Only::SENDS,
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
            Only::SENDS,
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
            Only::FX_ENABLED,
            format!("{what} on"),
            f.on.to_string(),
            u.on.to_string(),
        );
        if only.has(Only::FX_PARAMS) && f.params != u.params {
            let ours: std::collections::BTreeSet<&String> = f.params.iter().collect();
            let theirs: std::collections::BTreeSet<&String> = u.params.iter().collect();
            out.push(format!(
                "{what} params: {:?} → {:?}",
                ours.difference(&theirs).collect::<Vec<_>>(),
                theirs.difference(&ours).collect::<Vec<_>>()
            ));
        }
        if only.has(Only::FX_STATE) && f.state != u.state {
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
    let only = Only::parse(a.only.as_deref())?;
    if a.names.unwrap_or(false) {
        let parents = all
            .iter()
            .filter_map(|n| {
                let parent = load(daw, project, n).ok()?.parent;
                (!parent.is_empty()).then(|| format!("{n} ← {parent}"))
            })
            .collect();
        return Ok(DiffRow {
            a: String::new(),
            b: String::new(),
            differences: Vec::new(),
            names: all,
            parents,
            head: head(daw, project),
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
    let (mut x, mut y) = (get(&an)?, get(&bn)?);
    // `current` against a scoped mix: the same scope.
    if an.eq_ignore_ascii_case("current") && !y.scope.is_empty() {
        x = take_scoped(daw, project, &song, "current", y.scope_set().as_ref());
    }
    if bn.eq_ignore_ascii_case("current") && !x.scope.is_empty() {
        y = take_scoped(daw, project, &song, "current", x.scope_set().as_ref());
    }
    let differences = differences(&x, &y, only);
    span.record("mix.differences", differences.len());
    span.record("mix.ms", began.elapsed().as_secs_f64() * 1e3);
    Ok(DiffRow {
        a: an,
        b: bn,
        differences,
        names: all,
        parents: Vec::new(),
        head: head(daw, project),
        ms: began.elapsed().as_secs_f64() * 1e3,
    })
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SNAPSHOT, Effect::Mutating, |a: SnapshotArg| {
        let (daw, project) = song()?;
        snapshot_with(daw, &project, &a)
    })?;
    list.register(&RESTORE, Effect::Mutating, |a: RestoreArg| {
        let (daw, project) = song()?;
        let only = Only::parse(a.only.as_deref())?;
        restore_with(daw, &project, &a.name, only)
    })?;
    list.register(&DIFF, Effect::ReadOnly, |a: DiffArg| {
        let (daw, project) = song()?;
        diff(daw, &project, &a)
    })?;
    Ok(())
}

#[cfg(test)]
pub mod tests;
