//! Signal on a song's drum tracks: a kit Signal made — one CLAP instance
//! per track, all of them sharing one session through Signal's core
//! library — put on the tracks it names (`SIGNAL_KIT`), and every one of
//! them switched off and on at once to hear the raw drums against them
//! (`SIGNAL_BYPASS`).
//!
//! The kit is a manifest Signal writes:
//!
//! ```json
//! {"session": "…", "clap": "/home/…/.clap/Signal Dev.clap",
//!  "tracks": [{"path": "Drums/Kick/Sum/In", "role": "kick", "state": "/tmp/signal-kit/kick.state"}]}
//! ```
//!
//! each track's `state` a file holding the CLAP state blob its instance
//! loads (a relative path is the manifest's folder's), and its `routing`
//! the sends the kit assumes — `{"from": "Drums/Kick/Sum", "to":
//! "Drums/Kick/Verb/Room", "gain_db": 0.0, "pre": false}` — which
//! `sends: true` creates where they are missing (each is reported, made
//! or not). A `from` may carry a note in brackets (`Drums (the kit,
//! before Process)`); a `from` folder holding its `to` — the kit sending
//! to its own Process folder — would feed back, so each of its folders
//! that does not hold `to` sends instead: their sum is the kit before it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Effects, FxChainContext, FxRef, FxTarget, ProjectContext};
use daw::standalone::Standalone;
use facet::Facet;

use crate::actions::{meta, resolve, song};
use crate::fx_actions::{is_bundle, project_guid};

static KIT: ActionMeta = meta(
    "SIGNAL_KIT",
    "signal_kit",
    "Put Signal on the drums",
    "Put a Signal kit on the song: `manifest` (a Signal kit manifest, `/tmp/signal-kit/manifest.json` — its CLAP and, for each track, the track's path and a file of its state). For each track, the Studio track at that path gets the kit's CLAP at the end of its chain with its state loaded; with `replace: true`, a Signal instance already there is replaced in its place (without it, such a track is skipped). The FTS processors on the tracks are left as they are. The kit's `routing` (the sends it assumes) is checked in any case — each send `exists`, `missing`, or its track is not on the song — and with `sends: true` the missing ones are made (`added`), at the manifest's `gain_db`, pre-fader where it says `pre`. `dry_run: true` says what would happen. One undo step. Says, per track, added / replaced / skipped / missing-track / failed and why; per send, its status; and the time taken.",
    "FX",
    "",
);

static BYPASS: ActionMeta = meta(
    "SIGNAL_BYPASS",
    "signal_bypass",
    "Signal on or off",
    "Switch every Signal instance on the song at once: `on: false` bypasses them all (the raw drums), `on: true` puts them back — to A/B while it plays. `clap` (a bundle's path or name) names the plugin when it is not the last SIGNAL_KIT's. Touches nothing else on the tracks.",
    "FX",
    "",
);

/// A kit to put on the song.
#[derive(Facet)]
struct KitArg {
    manifest: String,
    dry_run: Option<bool>,
    replace: Option<bool>,
    sends: Option<bool>,
}

/// Signal on or off.
#[derive(Facet)]
struct BypassArg {
    on: bool,
    clap: Option<String>,
}

/// One manifest track, as it went.
#[derive(Facet, Clone)]
pub struct KitTrackRow {
    path: String,
    role: String,
    /// The Studio track it went on, by its name.
    track: String,
    /// `added`, `replaced`, `skipped`, `missing-track`, `failed` (or, in a
    /// dry run, `would add` / `would replace`).
    status: String,
    why: String,
    /// Its place in the chain (`#4`), when it is on one.
    place: String,
    state_bytes: u32,
}

/// A kit, put on the song.
#[derive(Facet)]
pub struct KitRow {
    session: String,
    clap: String,
    dry_run: bool,
    pub(crate) added: u32,
    pub(crate) replaced: u32,
    pub(crate) skipped: u32,
    pub(crate) missing: u32,
    pub(crate) failed: u32,
    pub(crate) tracks: Vec<KitTrackRow>,
    /// The kit's sends: how many were there, missing, made.
    pub(crate) sends_existing: u32,
    pub(crate) sends_missing: u32,
    pub(crate) sends_added: u32,
    sends: Vec<KitSendRow>,
    ms: f64,
}

/// One send the kit assumes, as it went.
#[derive(Facet, Clone)]
pub struct KitSendRow {
    /// As the manifest has it.
    from: String,
    to: String,
    /// The track that sends (the manifest's `from`, or one of its folders
    /// when it would feed back).
    via: String,
    level_db: f64,
    tap: String,
    /// `exists`, `missing`, `added`, `would add`, `missing-track`, `failed`.
    status: String,
    why: String,
}

/// Signal switched.
#[derive(Facet)]
pub struct BypassRow {
    on: bool,
    instances: u32,
    changed: u32,
    ms: f64,
}

/// A kit manifest's track.
struct KitTrack {
    path: String,
    role: String,
    state: Option<PathBuf>,
}

/// A send a kit assumes.
struct KitSend {
    from: String,
    to: String,
    gain_db: f64,
    pre: bool,
}

/// A kit manifest.
struct Manifest {
    session: String,
    clap: PathBuf,
    tracks: Vec<KitTrack>,
    routing: Vec<KitSend>,
}

/// The manifest at `path`, its state files resolved against its folder.
fn manifest(path: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let session = match &json["session"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    let clap = json["clap"]
        .as_str()
        .ok_or("the manifest names no `clap`")?;
    let clap = crate::clap_fx::bundle(clap)
        .ok_or_else(|| format!("the kit's CLAP {clap} is not a CLAP bundle on disk"))?;
    let tracks = json["tracks"]
        .as_array()
        .ok_or("the manifest has no `tracks`")?
        .iter()
        .map(|t| KitTrack {
            path: t["path"].as_str().unwrap_or_default().to_owned(),
            role: t["role"].as_str().unwrap_or_default().to_owned(),
            state: t["state"].as_str().filter(|s| !s.is_empty()).map(|s| {
                let p = PathBuf::from(s);
                if p.is_absolute() { p } else { dir.join(p) }
            }),
        })
        .collect();
    let routing = json["routing"]
        .as_array()
        .map(|r| {
            r.iter()
                .map(|s| KitSend {
                    from: s["from"].as_str().unwrap_or_default().to_owned(),
                    to: s["to"].as_str().unwrap_or_default().to_owned(),
                    gain_db: s["gain_db"].as_f64().unwrap_or(0.0),
                    pre: s["pre"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Manifest {
        session,
        clap,
        tracks,
        routing,
    })
}

/// The last kit put on: its bundle, and the plugin's name for itself.
fn last_kit() -> &'static Mutex<Option<(PathBuf, String)>> {
    static LAST: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);
    &LAST
}

/// Whether `fx` is an instance of the kit's plugin: its bundle, a bundle
/// of the same file name elsewhere, or the same plugin by name hosted
/// from a bundle (an FTS processor of the same name is not it).
fn is_kit_fx(fx: &daw_proto::fx::Fx, clap: &Path, plugin: Option<&str>) -> bool {
    let named = Path::new(&fx.name);
    let from_bundle = named
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("clap"));
    is_bundle(&fx.name, clap)
        || (from_bundle && named.file_name() == clap.file_name())
        || (from_bundle && plugin.is_some_and(|p| !p.is_empty() && fx.plugin_name == p))
}

/// One track's chain.
fn chain_of(track: &str) -> FxChainContext {
    FxChainContext::Track(track.to_owned())
}

/// `SIGNAL_KIT` on `project`.
///
/// # Errors
/// The manifest would not be read, or names no CLAP on disk.
pub fn kit(
    daw: &Standalone,
    project: &ProjectContext,
    manifest_path: &Path,
    dry_run: bool,
    replace: bool,
    sends: bool,
) -> Result<KitRow, String> {
    let span = tracing::info_span!(
        "SIGNAL_KIT",
        fx.tracks = tracing::field::Empty,
        fx.added = tracing::field::Empty,
        fx.replaced = tracing::field::Empty,
        fx.missing = tracing::field::Empty,
        fx.failed = tracing::field::Empty,
        fx.state_bytes = tracing::field::Empty,
        fx.ms = tracing::field::Empty,
        fx.dry_run = dry_run,
        fx.sends_existing = tracing::field::Empty,
        fx.sends_missing = tracing::field::Empty,
        fx.sends_added = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let song = project_guid(project)?;
    let kit = manifest(manifest_path)?;
    span.record("fx.tracks", kit.tracks.len());
    let mut plugin: Option<String> = last_kit()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .filter(|(clap, _)| *clap == kit.clap)
        .map(|(_, plugin)| plugin.clone());
    let label = "Signal kit";
    if !dry_run {
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    }
    let mut rows = Vec::with_capacity(kit.tracks.len());
    let mut state_total: u64 = 0;
    for t in &kit.tracks {
        let row = put_on(
            daw,
            (project, &song),
            (&kit.clap, &mut plugin),
            t,
            (dry_run, replace),
        );
        state_total = state_total.saturating_add(u64::from(row.state_bytes));
        rows.push(row);
    }
    let send_rows: Vec<KitSendRow> = kit
        .routing
        .iter()
        .flat_map(|s| route(daw, project, s, sends && !dry_run, dry_run && sends))
        .collect();
    if !dry_run {
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        *last_kit()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((kit.clap.clone(), plugin.unwrap_or_default()));
    }
    let count = |status: &str| {
        u32::try_from(rows.iter().filter(|r| r.status == status).count()).unwrap_or(u32::MAX)
    };
    let out = KitRow {
        session: kit.session,
        clap: kit.clap.display().to_string(),
        dry_run,
        added: count("added"),
        replaced: count("replaced"),
        skipped: count("skipped"),
        missing: count("missing-track"),
        failed: count("failed"),
        ms: began.elapsed().as_secs_f64() * 1e3,
        tracks: rows,
        sends_existing: sent(&send_rows, "exists"),
        sends_missing: sent(&send_rows, "missing") + sent(&send_rows, "would add"),
        sends_added: sent(&send_rows, "added"),
        sends: send_rows,
    };
    span.record("fx.sends_existing", out.sends_existing);
    span.record("fx.sends_missing", out.sends_missing);
    span.record("fx.sends_added", out.sends_added);
    span.record("fx.added", out.added);
    span.record("fx.replaced", out.replaced);
    span.record("fx.missing", out.missing);
    span.record("fx.failed", out.failed);
    span.record("fx.state_bytes", state_total);
    span.record("fx.ms", out.ms);
    if out.failed > 0 || out.missing > 0 {
        tracing::warn!(
            fx.failed = out.failed,
            fx.missing = out.missing,
            "the Signal kit did not go on every track"
        );
    }
    Ok(out)
}

/// How many of `rows` went `status`.
fn sent(rows: &[KitSendRow], status: &str) -> u32 {
    u32::try_from(rows.iter().filter(|r| r.status == status).count()).unwrap_or(u32::MAX)
}

/// A manifest track path with its note in brackets taken off:
/// `Drums (the kit, before Process)` is `Drums`.
fn bare(path: &str) -> &str {
    path.split_once(" (").map_or(path, |(p, _)| p).trim()
}

/// The tracks that carry the kit's send `s`, by GUID and path: its `from`,
/// or — when `from` is a folder holding `to` (a send there would feed
/// back) — each folder or track directly in it that does not hold `to`.
fn senders(
    tracks: &[daw_proto::Track],
    paths: &std::collections::HashMap<String, String>,
    from: &str,
    to: &str,
) -> Vec<(String, String)> {
    let parent: std::collections::HashMap<&str, Option<&str>> = tracks
        .iter()
        .map(|t| (t.guid.as_str(), t.parent_guid.as_deref()))
        .collect();
    let holds = |outer: &str, inner: &str| {
        let mut at = Some(inner);
        while let Some(g) = at {
            if g == outer {
                return true;
            }
            at = parent.get(g).copied().flatten();
        }
        false
    };
    let path = |g: &str| paths.get(g).cloned().unwrap_or_default();
    if !holds(from, to) {
        return vec![(from.to_owned(), path(from))];
    }
    tracks
        .iter()
        .filter(|t| t.parent_guid.as_deref() == Some(from) && !holds(&t.guid, to))
        .map(|t| (t.guid.clone(), path(&t.guid)))
        .collect()
}

/// One send of the kit's routing: whether it is on the song, made when
/// `make` and missing. A row per track that carries it.
fn route(
    daw: &Standalone,
    project: &ProjectContext,
    s: &KitSend,
    make: bool,
    dry_run: bool,
) -> Vec<KitSendRow> {
    use daw_proto::routing::SendMode;
    let mode = if s.pre {
        SendMode::PostFx
    } else {
        SendMode::PostFader
    };
    let row = |via: String, status: &str, why: String| KitSendRow {
        from: s.from.clone(),
        to: s.to.clone(),
        via,
        level_db: s.gain_db,
        tap: crate::mix_sends::tap_word(mode).to_owned(),
        status: status.to_owned(),
        why,
    };
    let (from, to) = match (
        resolve(daw, project, bare(&s.from)),
        resolve(daw, project, bare(&s.to)),
    ) {
        (Ok(f), Ok(t)) => (f, t),
        (Err(why), _) | (_, Err(why)) => return vec![row(String::new(), "missing-track", why)],
    };
    let tracks = daw::service::Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&tracks);
    let carriers = senders(&tracks, &paths, &from, &to);
    if carriers.is_empty() {
        return vec![row(
            String::new(),
            "failed",
            format!(
                "{} holds {} and nothing else in it could send",
                s.from, s.to
            ),
        )];
    }
    carriers
        .into_iter()
        .map(|(guid, path)| {
            if !crate::mix_sends::between(daw, project, &guid, &to).is_empty() {
                return row(path, "exists", String::new());
            }
            if dry_run {
                return row(path, "would add", String::new());
            }
            if !make {
                return row(path, "missing", "`sends: true` makes it".to_owned());
            }
            if crate::mix_sends::feeds_back(daw, project, &guid, &to) {
                return row(path, "failed", "it would feed back".to_owned());
            }
            match crate::mix_sends::add_send(daw, project, (&guid, &to), s.gain_db, mode) {
                Ok(_) => {
                    crate::actions::list().changed("track", &guid);
                    row(path, "added", String::new())
                }
                Err(why) => row(path, "failed", why),
            }
        })
        .collect()
}

/// One manifest track: the kit's CLAP put on its Studio track (or
/// replaced there), its state loaded. `plugin` is learned from the first
/// instance loaded: the plugin's name for itself.
fn put_on(
    daw: &Standalone,
    (project, song): (&ProjectContext, &str),
    (clap, plugin): (&Path, &mut Option<String>),
    t: &KitTrack,
    (dry_run, replace): (bool, bool),
) -> KitTrackRow {
    let mut row = KitTrackRow {
        path: t.path.clone(),
        role: t.role.clone(),
        track: String::new(),
        status: String::new(),
        why: String::new(),
        place: String::new(),
        state_bytes: 0,
    };
    let fail = |mut row: KitTrackRow, status: &str, why: String| {
        row.status = status.to_owned();
        row.why = why;
        row
    };
    let track = match resolve(daw, project, &t.path) {
        Ok(guid) => guid,
        Err(why) if why.starts_with("no track") => return fail(row, "missing-track", why),
        Err(why) => return fail(row, "failed", why),
    };
    row.track = daw
        .read_project(song, |p| {
            p.tracks
                .iter()
                .find(|x| x.guid == track)
                .map(|x| x.name.clone())
        })
        .flatten()
        .unwrap_or_default();
    let state = match &t.state {
        Some(file) => match std::fs::read(file) {
            Ok(bytes) => Some(bytes),
            Err(e) => return fail(row, "failed", format!("its state {}: {e}", file.display())),
        },
        None => None,
    };
    row.state_bytes = state
        .as_ref()
        .map_or(0, |s| u32::try_from(s.len()).unwrap_or(u32::MAX));
    let context = chain_of(&track);
    let chain = Effects::list(daw, project.clone(), context.clone());
    let earlier = chain
        .iter()
        .position(|fx| is_kit_fx(fx, clap, plugin.as_deref()));
    let index = match (earlier, replace) {
        (Some(at), false) => {
            row.place = format!("#{}", at.saturating_add(1));
            return fail(
                row,
                "skipped",
                "a Signal instance is on it already — `replace: true` replaces it".to_owned(),
            );
        }
        (Some(at), true) => Some(at),
        (None, _) => None,
    };
    if dry_run {
        row.status = if index.is_some() {
            "would replace"
        } else {
            "would add"
        }
        .to_owned();
        row.place = format!("#{}", index.unwrap_or(chain.len()).saturating_add(1));
        return row;
    }
    if let Some(at) = index {
        let removed = Effects::remove(
            daw,
            project.clone(),
            FxTarget {
                context: context.clone(),
                fx: FxRef::Guid(chain[at].guid.clone()),
            },
        );
        if let Err(e) = removed {
            return fail(
                row,
                "failed",
                format!("the earlier instance would not come off: {e}"),
            );
        }
    }
    let at = index.map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
    let fx_guid = match crate::clap_fx::add(daw, project, &context, clap, at) {
        Ok(guid) => guid,
        Err(why) => return fail(row, "failed", why),
    };
    if let Some(state) = &state
        && let Err(why) = crate::clap_fx::load_state(daw, song, &fx_guid, state)
    {
        let _ = Effects::remove(
            daw,
            project.clone(),
            FxTarget {
                context,
                fx: FxRef::Guid(fx_guid),
            },
        );
        return fail(row, "failed", format!("its state would not load: {why}"));
    }
    if plugin.is_none() {
        *plugin = Effects::get(
            daw,
            project.clone(),
            FxTarget {
                context: context.clone(),
                fx: FxRef::Guid(fx_guid.clone()),
            },
        )
        .map(|fx| fx.plugin_name);
    }
    row.place = Effects::list(daw, project.clone(), context)
        .iter()
        .position(|fx| fx.guid == fx_guid)
        .map_or_else(String::new, |i| format!("#{}", i.saturating_add(1)));
    row.status = if index.is_some() { "replaced" } else { "added" }.to_owned();
    crate::actions::list().changed("fx", &fx_guid);
    row
}

/// `SIGNAL_BYPASS` on `project`: every Signal instance on it switched.
///
/// # Errors
/// No song, or `clap` names no bundle.
pub fn bypass(
    daw: &Standalone,
    project: &ProjectContext,
    on: bool,
    clap: Option<&str>,
) -> Result<BypassRow, String> {
    let span = tracing::info_span!(
        "SIGNAL_BYPASS",
        fx.on = on,
        fx.instances = tracing::field::Empty,
        fx.changed = tracing::field::Empty,
        fx.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let song = project_guid(project)?;
    let (bundle, plugin) = match clap {
        Some(what) => (
            Some(
                crate::clap_fx::bundle(what)
                    .ok_or_else(|| format!("{what} is not a CLAP bundle"))?,
            ),
            None,
        ),
        None => last_kit()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .map_or((None, None), |(b, p)| (Some(b), Some(p))),
    };
    let found = found(daw, &song, bundle.as_deref(), plugin.as_deref());
    let mut changed = 0_u32;
    for (context, fx_guid, enabled) in &found {
        if *enabled == on {
            continue;
        }
        let target = FxTarget {
            context: context.clone(),
            fx: FxRef::Guid(fx_guid.clone()),
        };
        if Effects::set_enabled(daw, project.clone(), target, on).is_ok() {
            changed = changed.saturating_add(1);
        }
    }
    let out = BypassRow {
        on,
        instances: u32::try_from(found.len()).unwrap_or(u32::MAX),
        changed,
        ms: began.elapsed().as_secs_f64() * 1e3,
    };
    span.record("fx.instances", out.instances);
    span.record("fx.changed", out.changed);
    span.record("fx.ms", out.ms);
    Ok(out)
}

/// Every Signal instance on song `song` — of the kit's `bundle` (and its
/// `plugin` name), or with none named, any CLAP calling itself Signal —
/// with its chain and whether it is on: one read.
fn found(
    daw: &Standalone,
    song: &str,
    bundle: Option<&Path>,
    plugin: Option<&str>,
) -> Vec<(FxChainContext, String, bool)> {
    daw.read_project(song, |p| {
        p.fx_chains
            .iter()
            .flat_map(|(key, chain)| {
                chain.iter().filter_map(move |e| {
                    let signal = match bundle {
                        Some(b) => is_kit_fx(&e.fx, b, plugin),
                        // No kit named: a plugin that calls itself Signal.
                        None => {
                            e.fx.plugin_type == daw_proto::fx::FxType::Clap
                                && (e.fx.plugin_name.to_lowercase().contains("signal")
                                    || Path::new(&e.fx.name)
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .is_some_and(|s| s.to_lowercase().contains("signal")))
                        }
                    };
                    signal.then(|| (chain_context(key), e.fx.guid.clone(), e.fx.enabled))
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Every Signal instance on song `song` (as SIGNAL_BYPASS finds them), by
/// FX guid.
#[must_use]
pub fn instances(daw: &Standalone, song: &str) -> Vec<String> {
    let kit = last_kit()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let (bundle, plugin) = kit.map_or((None, None), |(b, p)| (Some(b), Some(p)));
    found(daw, song, bundle.as_deref(), plugin.as_deref())
        .into_iter()
        .map(|(_, guid, _)| guid)
        .collect()
}

/// Whether the FX named `name` in its chain is a Signal CLAP (its bundle
/// calls itself Signal).
#[must_use]
pub fn is_signal_bundle(name: &str) -> bool {
    let path = Path::new(name);
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("clap"))
        && path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.to_lowercase().contains("signal"))
}

/// A Signal instance's state with its session renamed `<session>~<tag>`.
///
/// Signal's instances find each other through a session named in their
/// state, in a registry the whole process shares: a copy of a kit loaded
/// with the live states would join the live kit's session (and hear, and
/// be heard by, the instances playback runs). Renamed, a copy of the kit
/// is a kit of its own. The state is an 8-byte length (little endian)
/// and the JSON it counts, the instance's own JSON quoted inside it; the
/// name is changed wherever the state names its session, and the length
/// put right. A state that names no session comes back as it was.
#[must_use]
pub fn private_session(state: Vec<u8>, tag: &str) -> Vec<u8> {
    let key = b"session";
    let mut out: Vec<u8> = Vec::with_capacity(state.len() + 64);
    let mut i = 0;
    let mut renamed = false;
    while i < state.len() {
        if state[i..].starts_with(key) {
            // `session`, its quotes (escaped however deep) and colon, then
            // the name up to its closing quote.
            let mut j = i + key.len();
            let opened = j;
            while j < state.len() && matches!(state[j], b'\\' | b'"' | b':' | b' ') {
                j += 1;
            }
            let start = j;
            while j < state.len() && !matches!(state[j], b'\\' | b'"') {
                j += 1;
            }
            let colon = state[opened..start].contains(&b':');
            if colon && j > start && j < state.len() {
                out.extend_from_slice(&state[i..start]);
                out.extend_from_slice(&state[start..j]);
                out.push(b'~');
                out.extend_from_slice(tag.as_bytes());
                renamed = true;
                i = j;
                continue;
            }
        }
        out.push(state[i]);
        i += 1;
    }
    if !renamed {
        return state;
    }
    // The length in front, when the state has one.
    if state.len() >= 8 {
        let mut head = [0u8; 8];
        head.copy_from_slice(&state[..8]);
        if u64::from_le_bytes(head) == (state.len() - 8) as u64 {
            let len = (out.len() - 8) as u64;
            out[..8].copy_from_slice(&len.to_le_bytes());
        }
    }
    out
}

/// The chain a key names.
fn chain_context(key: &daw_standalone::sync::FxChainKey) -> FxChainContext {
    use daw_standalone::sync::FxChainKey;
    match key {
        FxChainKey::Track(guid) => FxChainContext::Track(guid.clone()),
        FxChainKey::Input(guid) => FxChainContext::Input(guid.clone()),
        FxChainKey::Monitoring => FxChainContext::Monitoring,
    }
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&KIT, Effect::Mutating, |a: KitArg| {
        let (daw, project) = song()?;
        kit(
            daw,
            &project,
            Path::new(&a.manifest),
            a.dry_run.unwrap_or(false),
            a.replace.unwrap_or(false),
            a.sends.unwrap_or(false),
        )
    })?;
    list.register(&BYPASS, Effect::Mutating, |a: BypassArg| {
        let (daw, project) = song()?;
        bypass(daw, &project, a.on, a.clap.as_deref())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
