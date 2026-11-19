//! Drum editing's settings: sticky, one set for the session.
//!
//! What a quantize is dialled to stays dialled — set once, every
//! quantize after uses it, and it is still set after a restart
//! (`~/.config/fts/drum-edit.json`). A call may still carry its own for
//! that call alone ([`Patch`], flattened into its arguments), without
//! moving the ones that stay. How the hits are found is the hit
//! detector's own (`crate::hits`).

pub use editing::settings::{Fills, Patch, Settings, Target};

/// The settings as they stand.
fn store() -> &'static std::sync::Mutex<Settings> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<Settings>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new(load().unwrap_or_default()))
}

/// The settings as they stand.
#[must_use]
pub fn current() -> Settings {
    store()
        .lock()
        .map_or_else(|e| e.into_inner().clone(), |s| s.clone())
}

/// Change some, keep them (and save them), and say what they are now.
pub fn set(patch: &Patch) -> Settings {
    let now = {
        let mut held = store()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *held = held.with(patch);
        held.clone()
    };
    save(&now);
    now
}

/// Back to the defaults (and saved).
pub fn reset() -> Settings {
    let now = Settings::default();
    *store()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = now.clone();
    save(&now);
    now
}

/// Where they are kept.
#[cfg(feature = "native")]
fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("drum-edit.json"))
}

/// A page keeps them for the visit: it has no config folder.
#[cfg(not(feature = "native"))]
const fn path() -> Option<std::path::PathBuf> {
    None
}

fn load() -> Option<Settings> {
    let text = std::fs::read(path()?).ok()?;
    // Saved as a patch: a setting added since is its default.
    let patch: Patch = facet_json::from_slice(&text).ok()?;
    Some(Settings::default().with(&patch))
}

fn save(settings: &Settings) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    match facet_json::to_vec(settings) {
        Ok(bytes) => {
            if std::fs::write(&path, bytes).is_err() {
                tracing::warn!("drum edit: the settings could not be saved");
            }
        }
        Err(error) => tracing::warn!(error = %error, "drum edit: the settings would not serialize"),
    }
}
