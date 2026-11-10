//! A production's note: the properties a scan writes, and which of them
//! changed since the last one. Pure: no host, no files.

use std::path::Path;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value, json};

use crate::find::Found;
use crate::model::{PROJECT_KIND, Stage, keys};
use crate::read::Facts;

/// A week: what "changed this week" means.
pub const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// `value` as a property: a whole number written without a fraction, the
/// rest to two places.
#[must_use]
pub fn number(value: f64) -> Value {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract() == 0.0
        && let Ok(whole) = format!("{rounded:.0}").parse::<i64>()
    {
        return json!(whole);
    }
    json!(rounded)
}

/// `path` as a note says it: relative to the vault (with `/`) when it is
/// inside it, so the note means the same on every device the vault syncs
/// to; else as it is on this machine.
#[must_use]
pub fn place(path: &Path, vault: Option<&Path>) -> String {
    vault
        .and_then(|root| path.strip_prefix(root).ok())
        .map_or_else(
            || path.display().to_string(),
            |inside| {
                inside
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            },
        )
}

/// The file `place` names: itself when absolute, else inside `vault`.
#[must_use]
pub fn file_at(place: &str, vault: Option<&Path>) -> std::path::PathBuf {
    let path = Path::new(place);
    match vault {
        Some(root) if path.is_relative() => root.join(path),
        _ => path.to_path_buf(),
    }
}

/// When `time` was, as a property (`2026-11-10T19:04:11Z`).
#[must_use]
pub fn timestamp(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// What a new production's note starts with: the `project` kind's own
/// properties, its type, and the default stage. A scan never writes
/// these again.
#[must_use]
pub fn new_note(found: &Found, title: &str) -> Map<String, Value> {
    let mut props = Map::new();
    props.insert(keys::TYPE.to_owned(), json!(PROJECT_KIND));
    props.insert(keys::TITLE.to_owned(), json!(title));
    props.insert(keys::STATE.to_owned(), json!("active"));
    props.insert(
        keys::PROJECT_TYPE.to_owned(),
        json!(found.daw.project_type()),
    );
    props.insert(keys::STAGE.to_owned(), json!(Stage::default().as_str()));
    props
}

/// What a scan read, as properties.
///
/// Where the production is, when it was saved, and each thing its
/// adapter could say. A key the adapter cannot give is left out (so a
/// person's own value stays); `readError` is `null`, to be taken off,
/// when the file read.
#[must_use]
pub fn read_fields(
    found: &Found,
    facts: &Facts,
    now: SystemTime,
    vault: Option<&Path>,
) -> Map<String, Value> {
    let mut props = Map::new();
    let mut put = |key: &str, value: Value| {
        props.insert(key.to_owned(), value);
    };
    put(keys::LOCATION, json!(place(&found.location, vault)));
    put(keys::FILE, json!(place(&found.file, vault)));
    put(keys::FILES, json!(found.files));
    if let Some(saved) = found.saved {
        put(keys::LAST_SAVED, json!(timestamp(saved)));
        let age = now.duration_since(saved).unwrap_or_default();
        put(keys::CHANGED_THIS_WEEK, json!(age <= WEEK));
    }
    if let Some(tempo) = facts.tempo {
        put(keys::TEMPO, number(tempo));
    }
    if let Some(key) = &facts.key {
        put(keys::KEY, json!(key));
    }
    if let Some(length) = facts.length {
        put(keys::LENGTH, number(length.round()));
    }
    if let Some(tracks) = facts.tracks {
        put(keys::TRACKS, json!(tracks));
    }
    if let Some(plugins) = &facts.plugins {
        put(keys::PLUGINS, json!(plugins));
    }
    if let Some(missing) = &facts.missing_media {
        put(keys::MISSING_MEDIA, json!(missing));
        put(keys::MISSING_MEDIA_COUNT, json!(missing.len()));
    }
    put(
        keys::READ_ERROR,
        facts.error.as_ref().map_or(Value::Null, |e| json!(e)),
    );
    props
}

fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    }
}

/// The properties of `read` that `existing` does not already have as
/// they are: what a re-scan writes. `null` takes a property off, and is
/// a change only when the note has it.
#[must_use]
pub fn changes(existing: &Map<String, Value>, read: &Map<String, Value>) -> Vec<(String, Value)> {
    read.iter()
        .filter(|(key, value)| {
            let old = existing.get(*key).unwrap_or(&Value::Null);
            !same(old, value)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use std::path::PathBuf;

    use super::*;
    use crate::model::Daw;

    fn found(saved: SystemTime) -> Found {
        Found {
            daw: Daw::ProTools,
            name: "Mix".to_owned(),
            location: PathBuf::from("/vault/Sessions/Mix"),
            file: PathBuf::from("/vault/Sessions/Mix/Mix.ptx"),
            files: vec!["Mix.ptx".to_owned()],
            saved: Some(saved),
        }
    }

    #[test]
    fn a_rescan_writes_only_what_changed_and_never_the_stage() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let saved = now - Duration::from_secs(3600);
        let found = found(saved);
        let facts = Facts {
            tempo: Some(120.0),
            tracks: Some(8),
            missing_media: Some(vec!["kick.wav".to_owned()]),
            ..Facts::default()
        };
        let vault = Path::new("/vault");
        let read = read_fields(&found, &facts, now, Some(vault));
        assert_eq!(read["location"], "Sessions/Mix");
        assert_eq!(read["file"], "Sessions/Mix/Mix.ptx");
        assert_eq!(read["tempo"], json!(120));
        assert_eq!(read["missingMediaCount"], json!(1));
        assert_eq!(read["changedThisWeek"], json!(true));
        // Pro Tools gives no key: nothing is written over a person's.
        assert!(!read.contains_key("key"));
        assert!(!read.contains_key("stage") && !read.contains_key("client"));

        let mut note = new_note(&found, "Mix");
        assert_eq!(note["stage"], "writing");
        assert_eq!(note["projectType"], "protools-session");
        // The first scan writes everything it read (no error to take off).
        let first = changes(&note, &read);
        assert!(first.iter().all(|(k, _)| k != "readError"));
        note.extend(first);
        note.insert("stage".to_owned(), json!("mixing"));
        note.insert("key".to_owned(), json!("Gm"));
        assert_eq!(changes(&note, &read), []);

        // Saved again, a track added, two weeks on.
        let later = now + WEEK + WEEK;
        let facts = Facts {
            tracks: Some(9),
            ..facts
        };
        let again = changes(&note, &read_fields(&found, &facts, later, Some(vault)));
        let keys: Vec<&str> = again.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["changedThisWeek", "tracks"]);
        assert_eq!(note["stage"], "mixing");
    }

    #[test]
    fn places_inside_the_vault_are_relative() {
        let vault = Path::new("/vault");
        assert_eq!(place(Path::new("/vault/A/B.als"), Some(vault)), "A/B.als");
        assert_eq!(
            place(Path::new("/music/B.als"), Some(vault)),
            "/music/B.als"
        );
        assert_eq!(
            file_at("A/B.als", Some(vault)),
            PathBuf::from("/vault/A/B.als")
        );
        assert_eq!(
            file_at("/music/B.als", Some(vault)),
            PathBuf::from("/music/B.als")
        );
        assert_eq!(number(93.456), json!(93.46));
        assert_eq!(number(120.0), json!(120));
    }
}
