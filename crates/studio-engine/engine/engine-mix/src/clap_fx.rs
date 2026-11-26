//! CLAP plugins hosted from their bundles, beside the FTS processors: a
//! bundle found by path or by name, a parameter read and set in the
//! plugin's own words, its state read and loaded, and a rebuilt bundle
//! reloaded under every instance of it while the song plays.
//!
//! An FX loaded from a bundle is named in its chain by the bundle's path
//! (what a save writes and a reopen loads again); its plugin name is the
//! name the plugin gives itself.
//!
//! A CLAP library, once loaded, is never unloaded (daw-standalone's
//! bundle cache: a plugin's thread-local destructors would point into
//! unmapped memory), and loading the same path again hands back the
//! library already mapped. So a bundle rebuilt on disk is loaded from a
//! copy of it under a path of its own ([`fresh_copy`]); the chain keeps
//! naming the real bundle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use daw::service::{Effects, FxChainContext, FxRef, FxTarget, ProjectContext};
use daw::standalone::Standalone;

/// Where CLAP bundles are looked for by name: `CLAP_PATH` first (as the
/// CLAP spec has it), then the user's and the system's folders.
#[must_use]
pub fn search_path() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::var_os("CLAP_PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".clap"));
        #[cfg(target_os = "macos")]
        roots.push(home.join("Library/Audio/Plug-Ins/CLAP"));
    }
    roots.push(PathBuf::from("/usr/lib/clap"));
    roots.push(PathBuf::from("/usr/local/lib/clap"));
    #[cfg(target_os = "macos")]
    roots.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    roots
}

/// The CLAP bundle `what` names: a path to one (absolute, or with a `/`),
/// or a bundle's name in [`search_path`] — `Signal Dev`, `Signal Dev.clap`,
/// case aside. `None` when it is neither.
#[must_use]
pub fn bundle(what: &str) -> Option<PathBuf> {
    bundle_in(what, &search_path())
}

/// [`bundle`] over `roots`.
fn bundle_in(what: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let what = what.trim();
    if what.is_empty() {
        return None;
    }
    let expanded = what.strip_prefix("~/").map_or_else(
        || PathBuf::from(what),
        |rest| {
            std::env::var_os("HOME")
                .map_or_else(|| PathBuf::from(what), |h| PathBuf::from(h).join(rest))
        },
    );
    if expanded.is_absolute() || what.contains('/') {
        let clap = expanded
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("clap"));
        return (clap && expanded.exists())
            .then(|| std::fs::canonicalize(&expanded).unwrap_or(expanded));
    }
    let file = if Path::new(what)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("clap"))
    {
        what.to_owned()
    } else {
        format!("{what}.clap")
    };
    // The bundle as its folder spells it (a case-insensitive file system,
    // macOS's, would take `root.join(&file)` in any case), the exact name
    // first; never a link to a bundle that is gone.
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let named: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.exists()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.eq_ignore_ascii_case(&file))
            })
            .collect();
        let exact = named
            .iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(file.as_str()));
        if let Some(found) = exact.or_else(|| named.first()) {
            return Some(found.clone());
        }
    }
    None
}

/// Whether the FX is a plugin hosted from a CLAP bundle (its chain name a
/// `.clap` path) with a live instance.
#[must_use]
pub fn is_hosted_clap(daw: &Standalone, fx: &daw_proto::fx::Fx) -> bool {
    fx.plugin_type == daw_proto::fx::FxType::Clap
        && Path::new(&fx.name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("clap"))
        && daw.with_plugin_instance(&fx.guid, |_| ()).is_some()
}

/// Moved down into the core (shared with `fx_actions::target`).
pub use engine_core::fx_target::display_name;

/// The normalized (0..1) value a hosted plugin reads `text` as, for the
/// parameter at `slot` (its place in the plugin's list, as FX_PARAMS
/// numbers them): its own text-to-value, over its parameter's range.
/// `None` when there is no such plugin or parameter, or it does not read
/// the words.
#[must_use]
pub fn text_to_normalized(daw: &Standalone, fx_guid: &str, slot: u32, text: &str) -> Option<f64> {
    daw.with_plugin_instance(fx_guid, |plugin| {
        let info = plugin
            .params()
            .into_iter()
            .nth(usize::try_from(slot).ok()?)?;
        let plain = plugin.text_to_value(info.id, text)?;
        let span = info.max - info.min;
        Some(if span.abs() < f64::EPSILON {
            0.0
        } else {
            ((plain - info.min) / span).clamp(0.0, 1.0)
        })
    })
    .flatten()
}

/// Why `path` would not load as a plugin, in the host's words.
#[must_use]
pub fn load_error(path: &Path) -> String {
    match daw_standalone::plugin::load_plugin(&path.to_string_lossy()) {
        Ok(Some(_)) => "it loads, but the engine did not keep it".to_owned(),
        Ok(None) => "not a plugin bundle".to_owned(),
        Err(e) => e.to_string(),
    }
}

/// Clear the parameter values the engine holds for `fx_guid` over its
/// plugin's own: after a state is loaded, the state is what it is set to.
/// (The engine sends its held values to the plugin every block.)
pub fn forget_overrides(daw: &Standalone, project_guid: &str, fx_guid: &str) {
    daw.write_project(project_guid, |p| {
        for entry in p.fx_chains.values_mut().flatten() {
            if entry.fx.guid == fx_guid {
                entry.params.clear();
            }
        }
    });
}

/// Let go of the values the host holds over the plugin `fx_guid`'s
/// parameters `ids` (CLAP ids): the plugin's own values are heard again.
/// How many it held.
pub fn forget_params(daw: &Standalone, project_guid: &str, fx_guid: &str, ids: &[u32]) -> usize {
    if ids.is_empty() {
        return 0;
    }
    daw.write_project(project_guid, |p| {
        let mut let_go = 0;
        for entry in p.fx_chains.values_mut().flatten() {
            if entry.fx.guid == fx_guid {
                for id in ids {
                    let_go += usize::from(entry.params.remove(id).is_some());
                }
            }
        }
        let_go
    })
    .unwrap_or(0)
}

/// Load `state` into the hosted plugin `fx_guid` and make it what the
/// plugin is set to ([`forget_overrides`]).
///
/// # Errors
/// No plugin instance, or the plugin refused the state.
pub fn load_state(
    daw: &Standalone,
    project_guid: &str,
    fx_guid: &str,
    state: &[u8],
) -> Result<(), String> {
    daw.apply_plugin_state(fx_guid, state)
        .map_err(|e| e.to_string())?;
    forget_overrides(daw, project_guid, fx_guid);
    Ok(())
}

/// The copies of rebuilt bundles loaded so far: a bundle's path → the
/// copy new instances of it load from, and the bundle's modification
/// time when it was copied.
fn copies() -> &'static Mutex<HashMap<PathBuf, (std::time::SystemTime, PathBuf)>> {
    static COPIES: std::sync::OnceLock<Mutex<HashMap<PathBuf, (std::time::SystemTime, PathBuf)>>> =
        std::sync::OnceLock::new();
    COPIES.get_or_init(Mutex::default)
}

/// A copy of the bundle at `bundle` under a path no library was loaded
/// from (a temp folder), so loading it maps the new build — not the one
/// already loaded from `bundle`. Remembered as the copy new instances of
/// `bundle` load from ([`loadable`]).
///
/// # Errors
/// The bundle would not be read or copied.
pub fn fresh_copy(bundle: &Path) -> Result<PathBuf, String> {
    let modified = std::fs::metadata(bundle)
        .and_then(|m| m.modified())
        .map_err(|e| format!("{}: {e}", bundle.display()))?;
    let dir = std::env::temp_dir().join("studio-clap-reload");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem = bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plugin")
        .replace(' ', "-");
    let nanos = web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let to = dir.join(format!("{stem}-{nanos}.clap"));
    copy_bundle(bundle, &to)?;
    copies()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(bundle.to_path_buf(), (modified, to.clone()));
    Ok(to)
}

/// Copy a bundle: a file (Linux, Windows), or a folder (macOS).
fn copy_bundle(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let entries = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
        for entry in entries.flatten() {
            copy_bundle(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| format!("{} → {}: {e}", from.display(), to.display()))
    }
}

/// The path a new instance of `bundle` loads from: the bundle itself, or
/// the copy [`fresh_copy`] made of its rebuilt build — a fresh copy again
/// when the bundle changed since then.
#[must_use]
pub fn loadable(bundle: &Path) -> Option<PathBuf> {
    let (copied_at, copy) = copies()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(bundle)
        .cloned()?;
    let modified = std::fs::metadata(bundle).and_then(|m| m.modified()).ok();
    if modified.is_some_and(|m| m != copied_at) {
        return fresh_copy(bundle).ok();
    }
    Some(copy)
}

/// Swap the instance behind `fx_guid` for one loaded from `from`, the old
/// one's state carried over. The FX keeps its place, its name (the real
/// bundle's path), its switches and its held parameter values; the new
/// instance is prepared by the renderer on its first block, as a newly
/// added FX is. Returns the state's size.
///
/// # Errors
/// The new build would not load, or refused the old one's state (then the
/// old instance stays).
pub fn swap_instance(daw: &Standalone, fx_guid: &str, from: &Path) -> Result<usize, String> {
    let state = daw.save_plugin_state(fx_guid).unwrap_or_default();
    // Offered what every plugin this engine loads is (Studio's actions).
    let mut fresh = daw_standalone::plugin::load_plugin_with(
        &from.to_string_lossy(),
        &daw.plugin_load_options(),
    )
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("{} is not a plugin bundle", from.display()))?;
    if !state.is_empty() {
        fresh
            .load_state(&state)
            .map_err(|e| format!("the new build refused the state: {e}"))?;
    }
    // One write of the map, between two blocks; the old instance is
    // deactivated and dropped here, off the audio thread.
    if let Some(mut old) = daw.insert_plugin_instance(fx_guid.to_owned(), fresh) {
        old.deactivate();
        drop(old);
    }
    Ok(state.len())
}

/// After adding the bundle `bundle` as `fx_guid`: when a rebuilt copy of
/// it is what new instances load ([`loadable`]), the instance just made
/// from the stale library is swapped for one from the copy.
///
/// # Errors
/// As [`swap_instance`].
pub fn freshen(daw: &Standalone, fx_guid: &str, bundle: &Path) -> Result<(), String> {
    match loadable(bundle) {
        Some(copy) if copy != bundle => swap_instance(daw, fx_guid, &copy).map(|_| ()),
        _ => Ok(()),
    }
}

/// Put the CLAP at `bundle` on `chain` at `index` (`u32::MAX`: the end),
/// a live instance from its newest build. Its guid.
///
/// # Errors
/// The plugin would not load (the FX the engine made in its place is
/// taken off again).
pub fn add(
    daw: &Standalone,
    project: &ProjectContext,
    chain: &FxChainContext,
    bundle: &Path,
    index: u32,
) -> Result<String, String> {
    let name = bundle.to_string_lossy().into_owned();
    let fx_guid = Effects::add_at(
        daw,
        project.clone(),
        daw_proto::fx::AddFxAtRequest {
            context: chain.clone(),
            name,
            index,
        },
    )
    .ok_or_else(|| format!("{} could not be added", bundle.display()))?;
    if daw.with_plugin_instance(&fx_guid, |_| ()).is_none() {
        let _ = Effects::remove(
            daw,
            project.clone(),
            FxTarget {
                context: chain.clone(),
                fx: FxRef::Guid(fx_guid),
            },
        );
        return Err(format!(
            "{} would not load: {}",
            bundle.display(),
            load_error(bundle)
        ));
    }
    freshen(daw, &fx_guid, bundle)?;
    Ok(fx_guid)
}

#[cfg(test)]
mod tests {
    use super::bundle_in;

    #[test]
    fn a_bundle_is_found_by_its_name_or_path() {
        let dir = tempfile::tempdir().expect("a folder");
        let clap = dir.path().join("Signal Dev.clap");
        std::fs::write(&clap, b"").expect("a bundle");
        let roots = vec![dir.path().to_path_buf()];
        assert_eq!(bundle_in("Signal Dev", &roots), Some(clap.clone()));
        assert_eq!(bundle_in("signal dev.clap", &roots), Some(clap.clone()));
        assert_eq!(
            bundle_in(&clap.to_string_lossy(), &roots),
            Some(std::fs::canonicalize(&clap).expect("canonical"))
        );
        assert_eq!(bundle_in("FTS Nothing", &roots), None);
        assert_eq!(bundle_in("/no/such.clap", &roots), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_bundle_that_is_gone_is_not_found() {
        let dir = tempfile::tempdir().expect("a folder");
        std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("FTS EQ.clap"))
            .expect("a link");
        let roots = vec![dir.path().to_path_buf()];
        assert_eq!(bundle_in("FTS EQ", &roots), None);
        assert_eq!(bundle_in("fts eq", &roots), None);
    }
}

/// What the CLAP tests share: a CLAP to host, and a song of nested tracks.
#[cfg(test)]
pub mod test_support {
    use daw::service::{ProjectContext, TrackRef, Tracks as _};
    use daw::standalone::Standalone;

    /// A CLAP on the search path that passes audio through (an EQ at its
    /// defaults), when this machine has one; the tests skip without. Its
    /// canonical path, as adding it by path names it (a linked install,
    /// `just plugins-link`'s, resolved).
    pub fn a_clap() -> Option<std::path::PathBuf> {
        ["FTS EQ", "ZamEQ2", "DragonflyRoomReverb"]
            .iter()
            .find_map(|name| super::bundle(name))
            .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
    }

    /// `clap` copied to `to`: a file (Linux), or a bundle's folder (macOS).
    pub fn copy_bundle(clap: &std::path::Path, to: &std::path::Path) {
        super::copy_bundle(clap, to).expect("a bundle");
    }

    /// `bundle` removed, a file or a folder.
    pub fn remove_bundle(bundle: &std::path::Path) {
        if bundle.is_dir() {
            std::fs::remove_dir_all(bundle).expect("removed");
        } else {
            std::fs::remove_file(bundle).expect("removed");
        }
    }

    /// Song `guid`, its tracks in folders: `Drums/Kick/Sum/In`,
    /// `Drums/Kick/Sub`, `Drums/Snare/Top`. Its context.
    pub fn drum_song(daw: &Standalone, guid: &str) -> ProjectContext {
        daw.seed_project(daw_proto::ProjectInfo {
            guid: guid.into(),
            name: guid.into(),
            path: String::new(),
        });
        let ctx = ProjectContext::Project(guid.to_owned());
        for (name, depth) in [
            ("Drums", 1),
            ("Kick", 1),
            ("Sum", 1),
            ("In", -1),
            ("Sub", -1),
            ("Snare", 1),
            ("Top", -2),
        ] {
            let track = daw.add(ctx.clone(), name, None).expect("a track");
            daw.set_folder_depth(ctx.clone(), TrackRef::Guid(track), depth)
                .expect("its depth");
        }
        ctx
    }
}
