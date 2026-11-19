//! Where analyses are kept: one JSON file per song, and the stems'
//! measurements held in memory while the process lives.
//!
//! An analysis is small and is the work (the beats, the hand fixes, bar
//! 1): it is saved on every change. The measurements are large and are
//! only the audio read again: they are made when first asked for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tempo_map::analysis::Analysis;
use tempo_map::features::{self, Features};
use tempo_map::map::TempoMap;
use tempo_map::rpp;

/// The analyses and measurements of one process.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    held: Mutex<HashMap<String, Analysis>>,
    measured: Mutex<HashMap<PathBuf, Arc<Features>>>,
}

/// A name as a file's stem: lower case, letters and digits, `-` between.
#[must_use]
pub fn key(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// Where analyses are kept when nothing says otherwise:
/// `STUDIO_TEMPO_MAP_DIR`, else `fts/tempo-map` under the data home.
#[must_use]
pub fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("STUDIO_TEMPO_MAP_DIR") {
        return PathBuf::from(dir);
    }
    let data = std::env::var_os("XDG_DATA_HOME").map_or_else(
        || {
            std::env::var_os("HOME").map_or_else(std::env::temp_dir, |home| {
                PathBuf::from(home).join(".local").join("share")
            })
        },
        PathBuf::from,
    );
    data.join("fts").join("tempo-map")
}

impl Store {
    #[must_use]
    pub fn at(dir: PathBuf) -> Self {
        Self {
            dir,
            held: Mutex::new(HashMap::new()),
            measured: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    /// Song `key`'s analysis: the one held, else the one saved.
    ///
    /// # Errors
    ///
    /// No analysis of that name.
    pub fn get(&self, key: &str) -> Result<Analysis, String> {
        if let Some(found) = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
        {
            return Ok(found.clone());
        }
        let path = self.file(key);
        let text = std::fs::read_to_string(&path).map_err(|_| {
            format!(
                "no analysis of `{key}`: run STUDIO_TEMPO_MAP_ANALYSE with its stems first (STUDIO_TEMPO_MAP_STATE lists the songs analysed)"
            )
        })?;
        let found: Analysis =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned(), found.clone());
        Ok(found)
    }

    /// Keeps `analysis` as song `key`'s, saved.
    ///
    /// # Errors
    ///
    /// The file could not be written.
    pub fn put(&self, key: &str, analysis: &Analysis) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let path = self.file(key);
        let text = serde_json::to_string(analysis).map_err(|e| e.to_string())?;
        // Written beside, then moved over: a reader never sees half.
        let part = self.dir.join(format!("{key}.json.part"));
        std::fs::write(&part, text).map_err(|e| format!("{}: {e}", part.display()))?;
        std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned(), analysis.clone());
        Ok(())
    }

    /// The songs analysed: the saved ones, by name.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(".json"))
                    .map(str::to_owned)
            })
            .collect();
        keys.sort();
        keys
    }

    /// The stems in `folder`, measured: once per folder while the process
    /// lives.
    ///
    /// # Errors
    ///
    /// A folder with no stems, or a stem that cannot be read.
    pub fn measured(&self, folder: &Path) -> Result<Arc<Features>, String> {
        if let Some(found) = self
            .measured
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(folder)
        {
            return Ok(found.clone());
        }
        let found = Arc::new(features::extract(folder).map_err(|e| e.to_string())?);
        let mut held = self.measured.lock().unwrap_or_else(PoisonError::into_inner);
        // Two songs' measurements at most: each is tens of megabytes.
        if held.len() >= 2 {
            held.clear();
        }
        held.insert(folder.to_owned(), found.clone());
        Ok(found)
    }

    /// A map by what names it: a file (a REAPER project, a saved analysis
    /// or map), else a song analysed here.
    ///
    /// # Errors
    ///
    /// Neither a file that holds a map nor a song analysed.
    pub fn map_of(&self, named: &str) -> Result<TempoMap, String> {
        let path = Path::new(named);
        if path.is_file() {
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("rpp"))
            {
                return rpp::reference(path);
            }
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            if let Ok(found) = serde_json::from_str::<Analysis>(&text) {
                return Ok(found.map());
            }
            return serde_json::from_str::<TempoMap>(&text)
                .map_err(|e| format!("{}: neither an analysis nor a map: {e}", path.display()));
        }
        Ok(self.get(&key(named))?.map())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_file_stems() {
        assert_eq!(key("Last Christmas"), "last-christmas");
        assert_eq!(
            key("  Go Tell It / Ain't No Mountain! "),
            "go-tell-it-ain-t-no-mountain"
        );
        assert_eq!(key("last-christmas"), "last-christmas");
    }
}
