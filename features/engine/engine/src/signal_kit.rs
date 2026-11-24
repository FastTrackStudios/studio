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
//! loads (a relative path is the manifest's folder's).

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
    "Put a Signal kit on the song: `manifest` (a Signal kit manifest, `/tmp/signal-kit/manifest.json` — its CLAP and, for each track, the track's path and a file of its state). For each track, the Studio track at that path gets the kit's CLAP at the end of its chain with its state loaded; with `replace: true`, a Signal instance already there is replaced in its place (without it, such a track is skipped). The FTS processors on the tracks are left as they are. `dry_run: true` says what would happen. One undo step. Says, per track, added / replaced / skipped / missing-track / failed and why, and the time taken.",
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
    added: u32,
    replaced: u32,
    skipped: u32,
    missing: u32,
    failed: u32,
    tracks: Vec<KitTrackRow>,
    ms: f64,
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

/// A kit manifest.
struct Manifest {
    session: String,
    clap: PathBuf,
    tracks: Vec<KitTrack>,
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
    Ok(Manifest {
        session,
        clap,
        tracks,
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
    };
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
    // Every instance, with its chain and whether it is on: one read.
    let (bundle, plugin) = (bundle.as_deref(), plugin.as_deref());
    let found: Vec<(FxChainContext, String, bool)> = daw
        .read_project(&song, |p| {
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
        .unwrap_or_default();
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
