//! The app, through the host service.
//!
//! The only way the extension reaches notes and shares, so its process
//! copy (in the Architect app) works as the linked one (in Studio) does. `extension.toml`'s `[capabilities]`
//! names the actions it may run.

use std::sync::{Mutex, PoisonError};

use architect_extensions::hostcall::HostRequest;
use architect_extensions::linked::Handle;
use serde_json::{Map, Value, json};

use crate::layout::SongRef;

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

pub fn connect(handle: Handle) {
    *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(handle);
}

fn handle() -> Option<Handle> {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Runs app action `action` with `args`: its answer.
///
/// # Errors
///
/// The extension is not running, or the action refused.
pub async fn run(action: &str, args: Value) -> Result<Value, String> {
    let h = handle().ok_or("the setlists extension is not running")?;
    h.host(HostRequest::Run {
        action: action.to_owned(),
        args,
        ticket: None,
    })
    .await
    .map_err(|e| e.to_string())
}

/// A note as a base's row: its path, name and properties.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub path: String,
    pub name: String,
    pub values: Map<String, Value>,
}

impl Row {
    /// Property `key` as text (a number or a link written out).
    #[must_use]
    pub fn text(&self, key: &str) -> Option<String> {
        match self.values.get(key)? {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    /// Property `key` as a list of text (a single value as one).
    #[must_use]
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.values.get(key) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    }
}

/// The rows of a `BASES_ROWS` answer (`{rows: {groups: [{rows}]}}`), a
/// value named `note.<key>` read as `<key>`.
#[must_use]
pub fn rows_of(out: &Value) -> Vec<Row> {
    let table = out.get("rows").unwrap_or(out);
    table
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|g| {
            g.get("rows")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .map(|r| {
            let text = |k: &str| {
                r.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let mut values = Map::new();
            if let Some(m) = r.get("values").and_then(Value::as_object) {
                for (k, v) in m {
                    if !v.is_null() {
                        values.insert(k.strip_prefix("note.").unwrap_or(k).to_owned(), v.clone());
                    }
                }
            }
            Row {
                path: text("path"),
                name: text("name"),
                values,
            }
        })
        .collect()
}

/// The base every song is in.
pub const SONGS_BASE: &str = "filters:\n  and:\n    - type == \"song\"\nviews:\n  - type: table\n    name: Songs\n    order:\n      - file.name\n      - title\n      - key\n      - tempo\n      - meter\n      - categories\n      - artist\n      - arrangements\n";

/// The base every setlist is in.
pub const SETLISTS_BASE: &str = "filters:\n  and:\n    - type == \"setlist\"\nviews:\n  - type: table\n    name: Setlists\n    order:\n      - file.name\n      - title\n      - date\n      - songs\n      - share\n";

/// The rows of a base written as YAML.
///
/// # Errors
///
/// `BASES_ROWS` refused.
pub async fn rows(source: &str) -> Result<Vec<Row>, String> {
    let out = run("BASES_ROWS", json!({ "source": source })).await?;
    Ok(rows_of(&out))
}

/// A song as the navigator and the actions show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongRow {
    pub at: SongRef,
    pub path: String,
    pub key: Option<String>,
    pub tempo: Option<String>,
    pub meter: Option<String>,
    pub artist: Option<String>,
    pub categories: Vec<String>,
    /// Its arrangements' names.
    pub arrangements: Vec<String>,
}

impl SongRow {
    /// A song's row; None for a note outside `Libraries/<L>/<S>/<S>.md`.
    #[must_use]
    pub fn of(row: &Row) -> Option<Self> {
        let at = SongRef::from_link_target(&row.path)?;
        let arrangements = match row.values.get("arrangements") {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.get("name").and_then(Value::as_str).or_else(|| v.as_str()))
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        Some(Self {
            path: row.path.clone(),
            key: row.text("key"),
            tempo: row.text("tempo"),
            meter: row.text("meter"),
            artist: row.text("artist"),
            categories: row.list("categories"),
            arrangements,
            at,
        })
    }

    /// `A · 83 bpm · 3/4`.
    #[must_use]
    pub fn summary(&self) -> String {
        [
            self.key.clone(),
            self.tempo.as_ref().map(|t| format!("{t} bpm")),
            self.meter.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

/// Every song of the vault's libraries.
///
/// # Errors
///
/// `BASES_ROWS` refused.
pub async fn songs() -> Result<Vec<SongRow>, String> {
    Ok(rows(SONGS_BASE)
        .await?
        .iter()
        .filter_map(SongRow::of)
        .collect())
}

/// A setlist as the navigator and the actions show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetlistRow {
    pub path: String,
    pub title: String,
    pub date: Option<String>,
    pub songs: Vec<SongRef>,
    pub share: Option<String>,
}

impl SetlistRow {
    #[must_use]
    pub fn of(row: &Row) -> Self {
        Self {
            path: row.path.clone(),
            title: row.text("title").unwrap_or_else(|| row.name.clone()),
            date: row.text("date"),
            songs: row
                .list("songs")
                .iter()
                .filter_map(|l| SongRef::from_link_target(crate::frontmatter::link_target(l)))
                .collect(),
            share: row.text("share"),
        }
    }
}

/// Every setlist of the vault, by title.
///
/// # Errors
///
/// `BASES_ROWS` refused.
pub async fn setlists() -> Result<Vec<SetlistRow>, String> {
    let mut all: Vec<SetlistRow> = rows(SETLISTS_BASE)
        .await?
        .iter()
        .map(SetlistRow::of)
        .collect();
    all.sort_by_key(|s| s.title.to_lowercase());
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_rows_read_as_songs_and_setlists() {
        let out = json!({"rows": {"groups": [{"rows": [
            {"path": "Libraries/Christmas/Silent Night/Silent Night.md", "name": "Silent Night",
             "values": {"note.key": "A", "note.tempo": 83, "note.categories": ["Christmas", "Hymns"],
                        "note.arrangements": [{"name": "Acoustic"}]}},
            {"path": "Setlists/Eve.md", "name": "Eve",
             "values": {"note.title": "Christmas Eve",
                        "note.songs": ["[[Libraries/Christmas/Silent Night/Silent Night#Acoustic|Silent Night (Acoustic)]]"],
                        "note.share": null}}
        ]}]}});
        let rows = rows_of(&out);
        let song = SongRow::of(&rows[0]).unwrap();
        assert_eq!(song.summary(), "A · 83 bpm");
        assert_eq!(song.categories, ["Christmas", "Hymns"]);
        assert_eq!(song.arrangements, ["Acoustic"]);
        assert!(SongRow::of(&rows[1]).is_none());
        let set = SetlistRow::of(&rows[1]);
        assert_eq!(set.title, "Christmas Eve");
        assert_eq!(
            set.songs,
            [SongRef::new("Christmas", "Silent Night", Some("Acoustic"))]
        );
        assert_eq!(set.share, None);
    }
}
