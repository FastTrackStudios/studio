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
use crate::setlist::{Set, Setlist, SongInfo};

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
pub const SONGS_BASE: &str = "filters:\n  and:\n    - type == \"song\"\nviews:\n  - type: table\n    name: Songs\n    order:\n      - file.name\n      - title\n      - key\n      - tempo\n      - meter\n      - categories\n      - artist\n      - arrangements\n      - media\n";

/// The base every setlist is in.
pub const SETLISTS_BASE: &str = "filters:\n  and:\n    - type == \"setlist\"\nviews:\n  - type: table\n    name: Setlists\n    order:\n      - file.name\n      - title\n      - date\n      - songs\n      - sets\n      - share\n";

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
#[derive(Debug, Clone, PartialEq)]
pub struct SongRow {
    pub at: SongRef,
    pub path: String,
    pub key: Option<String>,
    pub tempo: Option<String>,
    pub meter: Option<String>,
    pub artist: Option<String>,
    /// How long it plays, in seconds (its note's `length`).
    pub length: Option<f64>,
    pub categories: Vec<String>,
    /// Its arrangements' names.
    pub arrangements: Vec<String>,
    /// Its recording (its first audio or video), by vault path.
    pub recording: Option<String>,
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
        let recording = row
            .list("media")
            .iter()
            .map(|l| crate::frontmatter::link_target(l).to_owned())
            .find(|m| crate::song::is_playable(m));
        Some(Self {
            path: row.path.clone(),
            recording,
            key: row.text("key"),
            tempo: row.text("tempo"),
            meter: row.text("meter"),
            artist: row.text("artist"),
            length: row
                .text("length")
                .and_then(|l| l.parse::<f64>().ok())
                .filter(|l| l.is_finite() && *l > 0.0),
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

    /// What a setlist's page says of it.
    #[must_use]
    pub fn info(&self) -> SongInfo {
        SongInfo {
            summary: self.summary(),
            recording: self.recording.clone(),
            artist: self.artist.clone(),
            length: self.length,
        }
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
    /// Its songs in running order.
    pub songs: Vec<SongRef>,
    /// Its sets over `songs` (none: it has none).
    pub sets: Vec<Set>,
    pub share: Option<String>,
}

impl SetlistRow {
    #[must_use]
    pub fn of(row: &Row) -> Self {
        let refs = |links: &[String]| -> Vec<SongRef> {
            links
                .iter()
                .filter_map(|l| SongRef::from_link_target(crate::frontmatter::link_target(l)))
                .collect()
        };
        let sets: Vec<(Option<String>, Vec<SongRef>)> = match row.values.get("sets") {
            Some(Value::Array(sets)) => sets
                .iter()
                .map(|set| {
                    let name = set
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .map(str::to_owned);
                    let links: Vec<String> = set
                        .get("songs")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect();
                    (name, refs(&links))
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut setlist = Setlist::new("", refs(&row.list("songs")));
        if !sets.is_empty() {
            setlist.songs = sets.iter().flat_map(|(_, s)| s.iter().cloned()).collect();
            setlist.sets = sets
                .into_iter()
                .map(|(name, songs)| Set {
                    name,
                    len: songs.len(),
                })
                .collect();
        }
        Self {
            path: row.path.clone(),
            title: row.text("title").unwrap_or_else(|| row.name.clone()),
            date: row.text("date"),
            songs: setlist.songs,
            sets: setlist.sets,
            share: row.text("share"),
        }
    }

    /// As a [`Setlist`].
    #[must_use]
    pub fn setlist(&self) -> Setlist {
        Setlist {
            title: self.title.clone(),
            date: self.date.clone(),
            songs: self.songs.clone(),
            sets: self.sets.clone(),
            share: self.share.clone(),
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

/// The vault's folder on disk (`VAULT_STATUS`'s `root`).
///
/// # Errors
///
/// `VAULT_STATUS` refused or named no folder.
pub async fn vault_root() -> Result<String, String> {
    let out = run("VAULT_STATUS", json!({})).await?;
    out.get("root")
        .and_then(Value::as_str)
        .map(|r| r.trim_end_matches('/').to_owned())
        .ok_or_else(|| "the vault has no folder on disk".to_owned())
}

/// File `path` of the vault (a vault path: `Libraries/…/Song.kf`) on disk.
async fn vault_file(path: &str) -> Result<String, String> {
    if path.split('/').any(|p| p == "..") {
        return Err(format!("{path}: not a vault path"));
    }
    Ok(format!(
        "{}/{}",
        vault_root().await?,
        path.trim_start_matches('/')
    ))
}

/// The text of file `path` of the vault, through the host's `read_file`.
///
/// A song's `.kf` and `.lrc` are files beside its note, not notes, and the
/// vault's actions read and write notes only: the host reads them, where
/// the extension may reach (`capabilities.folders`, or a folder the person
/// picked: [`allow_vault`]).
///
/// # Errors
///
/// The host refused or the file is not there.
pub async fn read_vault_file(path: &str) -> Result<String, String> {
    let file = vault_file(path).await?;
    let h = handle().ok_or("the setlists extension is not running")?;
    h.host(HostRequest::ReadFile { path: file })
        .await
        .map_err(|e| e.to_string())?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{path}: not text"))
}

/// Writes `text` to file `path` of the vault, through the host's
/// `write_file` (see [`read_vault_file`]).
///
/// # Errors
///
/// The host refused it.
pub async fn write_vault_file(path: &str, text: &str) -> Result<(), String> {
    let file = vault_file(path).await?;
    let h = handle().ok_or("the setlists extension is not running")?;
    h.host(HostRequest::WriteFile {
        path: file,
        text: text.to_owned(),
    })
    .await
    .map(drop)
    .map_err(|e| e.to_string())
}

/// Writes `text` to `path` in the extension's own folder (the host's
/// `write_file` with a relative path: the app's data folder for it, not
/// the vault, so nothing written there is synced as a vault file).
///
/// # Errors
///
/// The host refused it, or keeps no folder for extensions.
pub async fn write_own_file(path: &str, text: &str) -> Result<(), String> {
    if path.starts_with('/') || path.split('/').any(|p| p == "..") {
        return Err(format!("{path}: not a path in the extension's folder"));
    }
    let h = handle().ok_or("the setlists extension is not running")?;
    h.host(HostRequest::WriteFile {
        path: path.to_owned(),
        text: text.to_owned(),
    })
    .await
    .map(drop)
    .map_err(|e| e.to_string())
}

/// Asks the person to pick the vault's folder, so its files may be read
/// and written for the rest of the run (a vault outside the folders the
/// manifest names).
///
/// # Errors
///
/// The person did not pick it, or the host cannot ask.
pub async fn allow_vault() -> Result<(), String> {
    use architect_extensions::hostcall::{Pick, PickKind};
    let root = vault_root().await?;
    let h = handle().ok_or("the setlists extension is not running")?;
    h.host(HostRequest::Pick(Pick {
        kind: PickKind::Folder,
        title: Some("Allow Setlists to save charts in this vault".to_owned()),
        folder: Some(root),
        ..Pick::default()
    }))
    .await
    .map(drop)
    .map_err(|e| e.to_string())
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
        assert_eq!(set.sets.len(), 0);
    }

    #[test]
    fn sets_read_from_their_property() {
        let out = json!({"rows": {"groups": [{"rows": [
            {"path": "Setlists/Concert.md", "name": "Concert",
             "values": {"note.songs": ["[[Libraries/C/A/A|A]]", "[[Libraries/C/B/B|B]]"],
                        "note.sets": [
                            {"name": "Presidents Songs", "songs": ["[[Libraries/C/A/A|A]]"]},
                            {"name": "Tree Lighting Concert Set", "songs": ["[[Libraries/C/B/B|B]]"]}
                        ]}},
            {"path": "Libraries/C/A/A.md", "name": "A",
             "values": {"note.media": ["[[Libraries/C/A/Media/notes.pdf|notes.pdf]]", "[[Libraries/C/A/Media/A.ogg|A.ogg]]"]}}
        ]}]}});
        let rows = rows_of(&out);
        let set = SetlistRow::of(&rows[0]).setlist();
        let names: Vec<_> = set
            .sections()
            .iter()
            .map(|s| (s.name.map(str::to_owned), s.songs.len()))
            .collect();
        assert_eq!(
            names,
            [
                (Some("Presidents Songs".to_owned()), 1),
                (Some("Tree Lighting Concert Set".to_owned()), 1)
            ]
        );
        assert_eq!(
            SongRow::of(&rows[1]).unwrap().recording.as_deref(),
            Some("Libraries/C/A/Media/A.ogg")
        );
    }
}
