//! The app, through its actions.
//!
//! The only way the extension reaches notes, so its process copy (in the
//! Architect app) works as the linked one does. `extension.toml`'s
//! `[capabilities]` names the actions it may run. The door is the host
//! service ([`connect`]); `studio productions` puts Architect's vault and
//! Bases actions over a folder behind the same door ([`connect_with`]).

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};

use architect_extensions::hostcall::HostRequest;
use architect_extensions::linked::Handle;
use serde_json::{Map, Value, json};

use crate::bases;
use crate::model::{Daw, keys};

/// An action's answer, when it comes.
pub type Answer = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;
/// What runs an app action: its id and its arguments.
pub type Door = Arc<dyn Fn(String, Value) -> Answer + Send + Sync>;

static DOOR: Mutex<Option<Door>> = Mutex::new(None);

/// Run the app's actions through `door`.
pub fn connect_with(door: Door) {
    *DOOR.lock().unwrap_or_else(PoisonError::into_inner) = Some(door);
}

/// Run the app's actions through the host service.
pub fn connect(handle: Handle) {
    connect_with(Arc::new(move |action, args| {
        let handle = handle.clone();
        Box::pin(async move {
            handle
                .host(HostRequest::Run {
                    action,
                    args,
                    ticket: None,
                })
                .await
                .map_err(|e| e.to_string())
        })
    }));
}

/// Runs app action `action` with `args`: its answer.
///
/// # Errors
///
/// The extension is not running, or the action refused.
pub async fn run(action: &str, args: Value) -> Result<Value, String> {
    let door = DOOR
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or("the productions extension is not running")?;
    door(action.to_owned(), args).await
}

/// The vault's folder on this machine (`VAULT_STATUS`'s `root`).
pub async fn vault_root() -> Option<PathBuf> {
    run("VAULT_STATUS", Value::Null)
        .await
        .ok()?
        .get("root")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
}

/// The properties of the note at `path`; None when there is none.
pub async fn properties(path: &str) -> Option<Map<String, Value>> {
    run("VAULT_READ", json!({ "path": path }))
        .await
        .ok()?
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
}

/// A new note (or `.base`) at `path`.
///
/// # Errors
///
/// `VAULT_CREATE` refused: the path is taken, or cannot be written.
pub async fn create(
    path: &str,
    properties: Option<&Map<String, Value>>,
    body: &str,
) -> Result<(), String> {
    let mut args = json!({ "path": path, "body": body });
    if let (Some(props), Some(args)) = (properties, args.as_object_mut()) {
        args.insert(
            "properties".to_owned(),
            json!(Value::Object(props.clone()).to_string()),
        );
    }
    run("VAULT_CREATE", args).await.map(drop)
}

/// Sets property `key` of the note at `path` (`null` takes it off).
///
/// # Errors
///
/// `VAULT_SET_PROPERTY` refused.
pub async fn set(path: &str, key: &str, value: &Value) -> Result<(), String> {
    if value.is_null() {
        // Taking off one it does not have is no failure.
        let _ = run("VAULT_REMOVE_PROPERTY", json!({ "path": path, "key": key })).await;
        return Ok(());
    }
    run(
        "VAULT_SET_PROPERTY",
        json!({ "path": path, "key": key, "value": value.to_string() }),
    )
    .await
    .map(drop)
}

/// A production as its note has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Production {
    /// Its note's vault path.
    pub path: String,
    pub title: String,
    pub daw: Daw,
    pub values: Map<String, Value>,
}

impl Production {
    /// Property `key` as text (a number written out).
    #[must_use]
    pub fn text(&self, key: &str) -> Option<String> {
        match self.values.get(key)? {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    /// The folder its note is in (`Projects/Album`): its name is the
    /// last part.
    #[must_use]
    pub fn folder(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(dir, _)| dir)
    }

    /// Whether `name` is it: its title, its note's path, its folder or
    /// its folder's name, any case.
    #[must_use]
    pub fn is(&self, name: &str) -> bool {
        let name = name.trim();
        let folder = self.folder();
        let last = folder.rsplit('/').next().unwrap_or(folder);
        [self.title.as_str(), self.path.as_str(), folder, last]
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name))
    }

    #[must_use]
    pub fn json(&self) -> Value {
        let mut out = self.values.clone();
        out.insert("path".to_owned(), json!(self.path));
        out.insert("title".to_owned(), json!(self.title));
        out.insert("daw".to_owned(), json!(self.daw.label()));
        Value::Object(out)
    }
}

/// The productions in a `BASES_ROWS` answer (`{rows: {groups:
/// [{rows}]}}`), a value named `note.<key>` read as `<key>`.
#[must_use]
pub fn productions_of(out: &Value) -> Vec<Production> {
    let table = out.get("rows").unwrap_or(out);
    table
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|g| {
            g.get("rows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|r| {
            let mut values = Map::new();
            for (k, v) in r.get("values").and_then(Value::as_object)? {
                let key = k.strip_prefix("note.").unwrap_or(k);
                if !v.is_null() && !key.starts_with("file.") {
                    values.insert(key.to_owned(), v.clone());
                }
            }
            let daw = values
                .get(keys::PROJECT_TYPE)
                .and_then(Value::as_str)
                .and_then(Daw::of_project_type)?;
            let path = r.get("path").and_then(Value::as_str)?.to_owned();
            let title = values
                .get(keys::TITLE)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
                    dir.rsplit('/').next().map(str::to_owned)
                })
                .unwrap_or_default();
            Some(Production {
                path,
                title,
                daw,
                values,
            })
        })
        .collect()
}

/// Every production of the vault, by title.
///
/// # Errors
///
/// `BASES_ROWS` refused.
pub async fn productions() -> Result<Vec<Production>, String> {
    let out = run("BASES_ROWS", json!({ "source": bases::all() })).await?;
    let mut all = productions_of(&out);
    all.sort_by_key(|p| p.title.to_lowercase());
    Ok(all)
}

/// The production `name` names ([`Production::is`]).
///
/// # Errors
///
/// There is none, or `BASES_ROWS` refused.
pub async fn production(name: &str) -> Result<Production, String> {
    productions()
        .await?
        .into_iter()
        .find(|p| p.is(name))
        .ok_or_else(|| format!("no production `{name}`"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn base_rows_read_as_productions() {
        let out = json!({"rows": {"groups": [{"rows": [
            {"path": "Projects/Album/project.md", "name": "project",
             "values": {"note.title": "Album", "note.projectType": "ableton-set",
                        "note.stage": "mixing", "note.tempo": 120, "note.client": null,
                        "file.name": "project"}},
            {"path": "Projects/Site/project.md", "name": "project",
             "values": {"note.title": "Site", "note.projectType": "software"}}
        ]}]}});
        let all = productions_of(&out);
        assert_eq!(all.len(), 1);
        let album = &all[0];
        assert_eq!(album.daw, Daw::Ableton);
        assert_eq!(album.text("stage").as_deref(), Some("mixing"));
        assert_eq!(album.text("tempo").as_deref(), Some("120"));
        assert_eq!(album.text("client"), None);
        assert!(album.is("album") && album.is("Projects/Album") && !album.is("Site"));
        assert_eq!(album.json()["daw"], "Ableton Live");
    }
}
