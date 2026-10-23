//! The keymap studio ships, and the user's keymap file over it.
//!
//! Both are `keymap.styx` files written in the JSON subset of styx, so
//! they decode with [`KeymapConfig::from_json_str`]; architect-input reads
//! no files, so the reading happens here.

use std::path::{Path, PathBuf};

use architect_input::{ConfigError, KeymapConfig};

/// The shipped default keymap (`defaults/keymap.styx`).
pub const DEFAULT_KEYMAP: &str = include_str!("../defaults/keymap.styx");

/// Why a keymap file could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("failed to read keymap file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Config(#[from] ConfigError),
}

/// The shipped default keymap.
///
/// # Errors
///
/// [`LoadError::Config`] if the embedded file does not decode (a build
/// that ships a broken default; the tests below catch it).
pub fn load_default_config() -> Result<KeymapConfig, LoadError> {
    Ok(KeymapConfig::from_json_str(DEFAULT_KEYMAP)?)
}

/// Where the user's keymap lives on this OS, if there is a home to put
/// it in.
#[must_use]
pub fn default_user_config_path() -> Option<PathBuf> {
    user_config_path_in(&UserDirs {
        home: std::env::var_os("HOME").map(PathBuf::from),
        xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
    })
}

/// The user's keymap, from [`default_user_config_path`]. `Ok(None)` when
/// there is no such file.
///
/// # Errors
///
/// [`LoadError`] when the file exists but cannot be read or decoded.
pub fn load_user_config() -> Result<Option<KeymapConfig>, LoadError> {
    let Some(path) = default_user_config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    load_keymap_file(&path).map(Some)
}

/// Read and decode one keymap file.
///
/// # Errors
///
/// [`LoadError::Io`] if it cannot be read, [`LoadError::Config`] if it
/// does not decode.
pub fn load_keymap_file(path: &Path) -> Result<KeymapConfig, LoadError> {
    let source = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    Ok(KeymapConfig::from_json_str(&source)?)
}

/// The environment [`user_config_path_in`] reads.
struct UserDirs {
    home: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
}

#[cfg(target_os = "macos")]
fn user_config_path_in(dirs: &UserDirs) -> Option<PathBuf> {
    let base = dirs.home.as_ref()?;
    Some(base.join("Library/Application Support/FastTrackStudio/keymap.styx"))
}

#[cfg(target_os = "linux")]
fn user_config_path_in(dirs: &UserDirs) -> Option<PathBuf> {
    let config = match (&dirs.xdg_config_home, &dirs.home) {
        (Some(xdg), _) => xdg.clone(),
        (None, Some(home)) => home.join(".config"),
        (None, None) => return None,
    };
    Some(config.join("fasttrackstudio/keymap.styx"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn user_config_path_in(_dirs: &UserDirs) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use architect_input::{
        InputContext, InputProcessor, Modifiers, ScrollEvent, table_for_surface,
    };

    use super::*;

    #[test]
    fn the_shipped_defaults_load_into_a_processor() {
        let cfg = load_default_config().expect("defaults parse");
        assert_eq!(cfg.modes["insert"].display_name, "INSERT");
        assert!(cfg.modes["insert"].passthrough_text);
        let keys: InputProcessor =
            InputProcessor::from_config(cfg).expect("defaults build a processor");
        assert_eq!(keys.current_mode().as_str(), "normal");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_user_keymap_prefers_xdg_config_home() {
        let dirs = UserDirs {
            home: Some("/home/u".into()),
            xdg_config_home: Some("/xdg".into()),
        };
        assert_eq!(
            user_config_path_in(&dirs),
            Some("/xdg/fasttrackstudio/keymap.styx".into())
        );
        let dirs = UserDirs {
            home: Some("/home/u".into()),
            xdg_config_home: None,
        };
        assert_eq!(
            user_config_path_in(&dirs),
            Some("/home/u/.config/fasttrackstudio/keymap.styx".into())
        );
    }

    fn mods(ctrl: bool, alt: bool, shift: bool) -> Modifiers {
        Modifiers {
            ctrl,
            alt,
            shift,
            ..Default::default()
        }
    }

    /// What the gesture resolves to on `surface`, in the shipped defaults.
    fn resolve(surface: &str, dx: f64, dy: f64, modifiers: Modifiers) -> Option<String> {
        let cfg = load_default_config().expect("defaults parse");
        let table = table_for_surface::<String>(&cfg, surface).expect("surface table");
        let event = ScrollEvent {
            delta_x: dx,
            delta_y: dy,
            modifiers,
        };
        table.match_event(&event, &InputContext::new())
    }

    /// The scheme every surface shares: plain wheel scrolls, Shift goes
    /// sideways, Alt zooms — REAPER's own.
    ///
    /// `normal` only. The expression editor's table deliberately keeps
    /// the scrolling half and drops the zooming half: `Alt` there is the
    /// modifier that *creates a note*, and a key meaning one thing on a
    /// drag and another on a wheel is exactly the overloading that
    /// surface's map exists to remove. Its zoom moved to a held `Z`.
    ///
    /// A surface may add to the shared scheme and may decline part of
    /// it; what it must not do is *redefine* a gesture to mean something
    /// else, which is what would put the two surfaces in conflict.
    #[test]
    fn the_shared_scheme_is_the_reaper_scheme() {
        let cases = [
            (
                mods(false, false, false),
                "view.vscroll",
                "plain wheel scrolls vertically",
            ),
            (
                mods(false, false, true),
                "view.hscroll",
                "shift+wheel scrolls horizontally",
            ),
            (
                mods(false, true, false),
                "view.zoom_v",
                "alt+wheel zooms vertically",
            ),
            (
                mods(false, true, true),
                "view.zoom_h",
                "alt+shift+wheel zooms horizontally",
            ),
            (
                mods(true, true, false),
                "view.zoom_both",
                "ctrl+alt+wheel zooms both axes",
            ),
        ];
        for (m, want, why) in cases {
            assert_eq!(
                resolve("normal", 0.0, 10.0, m).as_deref(),
                Some(want),
                "{why}"
            );
        }
    }

    #[test]
    fn a_trackpad_swipe_scrolls_sideways_without_a_modifier() {
        // The axis half of the scheme: a horizontal gesture is already
        // horizontal, so it must not be read as a vertical scroll.
        assert_eq!(
            resolve("normal", 10.0, 0.0, mods(false, false, false)).as_deref(),
            Some("view.hscroll")
        );
    }

    #[test]
    fn the_editor_adds_its_own_gesture_without_changing_the_shared_ones() {
        // The expression editor exists only partly in the DAW, so it gets
        // a surface of its own — but it may only *add*. A gesture that
        // meant one thing in the arrange view and another in the editor
        // is the confusion this whole arrangement exists to avoid.
        assert_eq!(
            resolve("editor", 0.0, 10.0, mods(true, false, true)).as_deref(),
            Some("edit.nudge_time"),
            "ctrl+shift nudges notes off-grid in the editor"
        );
        assert_eq!(
            resolve("normal", 0.0, 10.0, mods(true, false, true)),
            None,
            "and means nothing in the arrange view"
        );
    }

    #[test]
    fn an_unknown_surface_is_empty_rather_than_an_error() {
        let cfg = load_default_config().expect("defaults parse");
        let table = table_for_surface::<String>(&cfg, "mixer").expect("no error");
        assert!(table.bindings().is_empty());
    }
}
