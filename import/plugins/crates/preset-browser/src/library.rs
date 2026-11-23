//! Loading a preset library from disk.
//!
//! Reads the JSON that `signal-analyzer`'s `reverb_match --save-dir` writes:
//! a translated preset plus the measurements that justify it. That file is
//! the output of a full plugin-hosted tuning pass, so it is the library
//! format rather than an export of one.
//!
//! Loading is deliberately forgiving. A library is a directory someone drops
//! files into, and one unreadable or half-written file should cost that
//! preset, not the whole bank — [`LoadReport`] carries what was skipped so a
//! UI can say so instead of silently showing a short list.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Preset;

/// Why a directory could not be read at all.
#[derive(Debug)]
pub enum LoadError {
    /// The directory itself could not be listed.
    Unreadable {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable { path, source } => {
                write!(f, "could not read {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// What a load produced, including what it could not read.
#[derive(Debug, Default)]
pub struct LoadReport {
    pub presets: Vec<Preset>,
    /// `(file, why)` for each file that was skipped.
    pub skipped: Vec<(PathBuf, String)>,
}

impl LoadReport {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }
}

// ── The on-disk shape written by `reverb_match --save-dir` ─────────────────

#[derive(Deserialize, Serialize)]
struct SavedPreset {
    /// Free-form prose about the preset — see [`Preset::description`].
    /// `default`/`skip_serializing_if` on the same terms as
    /// `SavedTarget::text_parameters`: an old file has no key and reads as
    /// `None`, and a preset with nothing to say writes no key at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    source: SavedSource,
    target: SavedTarget,
    #[serde(default)]
    measurement: Option<SavedMeasurement>,
}

#[derive(Deserialize, Serialize)]
struct SavedSource {
    preset: String,
    #[serde(default)]
    plugin: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct SavedTarget {
    #[serde(default)]
    parameters: Vec<SavedParam>,
    /// Added after the 171 Pro-Q banks were written, which is why it is
    /// `default` on both ends: an old file has no key and reads as empty, and
    /// a reader that predates the key ignores it and still gets the curve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    text_parameters: Vec<SavedTextParam>,
}

#[derive(Deserialize, Serialize)]
struct SavedParam {
    name: String,
    value: f64,
}

#[derive(Deserialize, Serialize)]
struct SavedTextParam {
    name: String,
    value: String,
}

#[derive(Deserialize, Serialize)]
struct SavedMeasurement {
    #[serde(default)]
    decay_passed: Option<bool>,
    #[serde(default)]
    worst_band_ratio_error: Option<f64>,
}

impl SavedPreset {
    fn into_preset(self) -> Preset {
        // The source plugin's own mode name is the most useful grouping we
        // have — "Plate", "Chamber1979", "Large Chamber" — because it is what
        // the preset was actually voiced as, not a folder someone filed it in.
        let match_error = self
            .measurement
            .as_ref()
            .and_then(|m| m.worst_band_ratio_error);
        let mut tags = Vec::new();
        if let Some(m) = &self.measurement {
            // Whether the translation was verified against the reference is
            // worth surfacing: a browser can show which presets are known to
            // match and which are best-effort.
            match m.decay_passed {
                Some(true) => tags.push("verified".to_string()),
                Some(false) => tags.push("approximate".to_string()),
                None => tags.push("unmeasured".to_string()),
            }
        }
        Preset {
            name: self.source.preset,
            category: self.source.mode,
            author: None,
            tags,
            origin: self.source.plugin,
            description: self.description,
            parameters: self
                .target
                .parameters
                .into_iter()
                .map(|p| (p.name, p.value))
                .collect(),
            text_parameters: self
                .target
                .text_parameters
                .into_iter()
                .map(|p| (p.name, p.value))
                .collect(),
            match_error,
        }
    }
}

impl SavedPreset {
    fn from_preset(p: &Preset) -> Self {
        Self {
            description: p.description.clone(),
            source: SavedSource {
                preset: p.name.clone(),
                plugin: p.origin.clone(),
                mode: p.category.clone(),
            },
            target: SavedTarget {
                parameters: p
                    .parameters
                    .iter()
                    .map(|(name, value)| SavedParam {
                        name: name.clone(),
                        value: *value,
                    })
                    .collect(),
                text_parameters: p
                    .text_parameters
                    .iter()
                    .map(|(name, value)| SavedTextParam {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            },
            measurement: p.match_error.map(|e| SavedMeasurement {
                decay_passed: None,
                worst_band_ratio_error: Some(e),
            }),
        }
    }
}

/// Write one preset to a file, in the same shape [`load_directory`] reads.
///
/// Carries the fields the file format has: the name, its grouping, where it
/// came from, a free-form description, the numeric parameters and the string
/// ones. `author` and `tags` are browser-side derivations rather than fields
/// of the file (tags are computed from the measurement on load), so they are
/// not written and do not come back.
///
/// # Errors
///
/// Returns an error if the preset cannot be serialized or the file cannot be
/// written.
pub fn save_preset(path: impl AsRef<Path>, preset: &Preset) -> std::io::Result<()> {
    let saved = SavedPreset::from_preset(preset);
    let json = serde_json::to_string_pretty(&saved)?;
    std::fs::write(path, json)
}

/// Load every `*.json` preset in a directory (non-recursive).
///
/// Returns a report rather than a bare `Vec`, so a caller can distinguish
/// "this bank is empty" from "this bank is broken".
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
pub fn load_directory(dir: impl AsRef<Path>) -> Result<LoadReport, LoadError> {
    let dir = dir.as_ref();
    let entries = std::fs::read_dir(dir).map_err(|source| LoadError::Unreadable {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut report = LoadReport::default();
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    // Stable order, so `SortMode::Library` means something reproducible.
    paths.sort();

    for path in paths {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<SavedPreset>(&text).map_err(|e| e.to_string()))
        {
            Ok(saved) => report.presets.push(saved.into_preset()),
            Err(why) => report.skipped.push((path, why)),
        }
    }

    Ok(report)
}
