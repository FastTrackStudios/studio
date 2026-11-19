//! The extension: one `linked::Extension`, linked into Studio Architect
//! or served as a process.
//!
//! `STUDIO_DAW_ENTER` reads the note (`VAULT_READ`), its songs' sessions
//! ([`crate::resolve`]), writes them as one `.setlist` in the cache, hands
//! it to Session ([`session_desktop::native::handoff::request`]: the
//! engine opens it as the set once Session's shell is up), and opens the
//! view zoomed. It reaches the app only through the host.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension, Handle};
use serde_json::{Value, json};

use crate::resolve::{self, Entered};

/// Its id: its view is `studio-daw/daw`.
pub const ID: &str = "studio-daw";

/// Its view's workspace kind (and pane id).
pub const PANE: &str = "studio-daw/daw";

/// The extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct Daw;

static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

/// Its handle to the host, once it runs (a default one, which every call
/// refuses, before).
#[must_use]
pub fn handle() -> Handle {
    HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

impl Extension for Daw {
    fn manifest(&self) -> &'static str {
        include_str!("../extension.toml")
    }

    fn build(&self, ext: &mut Builder) {
        *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(ext.handle());
        ext.view("daw", crate::view::DawView)
            .action("STUDIO_DAW_ENTER", enter)
            .action("STUDIO_DAW_LEAVE", |_| leave());
    }
}

async fn run(action: &str, args: Value) -> Result<Value, String> {
    setlists::host::run_on(&handle(), action, args).await
}

/// The note `args` names (`note`, or an opener's `path`), else the one on
/// show in the workspace.
async fn note_of(args: &Value) -> Result<String, String> {
    let given = ["note", "path"]
        .iter()
        .find_map(|k| args.get(*k).and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Some(note) = given {
        return Ok(note);
    }
    let layout = run("WORKSPACE_LAYOUT", json!({})).await?;
    layout
        .get("current_document")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "give `note`: no note is on show".to_owned())
}

/// `STUDIO_DAW_ENTER`.
async fn enter(args: Value) -> Result<Value, String> {
    let note = note_of(&args).await?;
    let span = tracing::info_span!("studio_daw.enter", daw.songs = tracing::field::Empty, daw.missing = tracing::field::Empty, daw.outcome = tracing::field::Empty);
    let _in = span.enter();
    // A `.session` opened from the navigator (its opener) is the set itself.
    let (title, sessions, missing) = if note.ends_with(".session") {
        let vault = PathBuf::from(setlists::host::vault_root_on(&handle()).await?);
        let path = if PathBuf::from(&note).is_absolute() { PathBuf::from(&note) } else { vault.join(&note) };
        let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        (title, vec![path], Vec::new())
    } else {
        sessions_of(&note).await?
    };
    span.record("daw.songs", sessions.len());
    span.record("daw.missing", missing.len());
    if sessions.is_empty() {
        span.record("daw.outcome", "no_sessions");
        tracing::warn!(daw.outcome = "no_sessions", "studio-daw: nothing to enter");
        return Err(format!(
            "{title}: none of its songs says where its session is (a song note's `session:` property)"
        ));
    }
    let file = write_set(&title, &sessions)?;
    crate::view::enter(file.clone());
    run(
        "EXT_VIEW_OPEN",
        json!({ "extension": ID, "view": "daw", "state": note, "placement": "zoomed" }),
    )
    .await?;
    span.record("daw.outcome", "entered");
    Ok(json!({
        "title": title,
        "set": file.display().to_string(),
        "sessions": sessions.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "missing": missing,
    }))
}

/// A note's title, the sessions its songs have (in order), and the songs
/// that have none.
async fn sessions_of(note: &str) -> Result<(String, Vec<PathBuf>, Vec<String>), String> {
    let read = run("VAULT_READ", json!({ "path": note })).await?;
    let props = read.get("properties").cloned().unwrap_or(Value::Null);
    let vault = PathBuf::from(setlists::host::vault_root_on(&handle()).await?);
    let home = dirs::home_dir();
    let (title, songs) = match resolve::entered(note, &props) {
        Entered::Setlist { title, songs } => (title, songs),
        Entered::Song(path) => {
            let title = props
                .get("title")
                .and_then(Value::as_str)
                .map_or_else(|| path.clone(), str::to_owned);
            (title, vec![path])
        }
    };
    let mut sessions = Vec::new();
    let mut missing = Vec::new();
    for song in songs {
        let props = if song == note {
            props.clone()
        } else {
            run("VAULT_READ", json!({ "path": song }))
                .await
                .ok()
                .and_then(|n| n.get("properties").cloned())
                .unwrap_or(Value::Null)
        };
        let at = props
            .get("session")
            .and_then(Value::as_str)
            .and_then(|v| resolve::session_path(v, &song, &vault, home.as_deref()))
            .filter(|p| p.exists());
        match at {
            Some(path) => sessions.push(path),
            None => missing.push(song),
        }
    }
    Ok((title, sessions, missing))
}

/// The set as a `.setlist` in the cache: what the engine opens.
fn write_set(title: &str, sessions: &[PathBuf]) -> Result<PathBuf, String> {
    let dir = dirs::cache_dir()
        .ok_or("no cache folder")?
        .join("studio-daw");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join(format!("{}.setlist", resolve::file_name(title)));
    std::fs::write(&file, resolve::setlist_text(title, sessions))
        .map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// `STUDIO_DAW_LEAVE`: the transport stopped, the view unzoomed and
/// closed. The engine keeps its set.
pub async fn leave() -> Result<Value, String> {
    session_desktop::native::handoff::stop();
    let _ = run("WORKSPACE_ZOOM", json!({ "pane": PANE, "mode": "off" })).await;
    run("WORKSPACE_CLOSE", json!({ "panes": [PANE] })).await?;
    Ok(json!({ "left": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_manifest_reads_and_names_what_it_builds() {
        let m = architect_extensions::manifest::Manifest::parse(Daw.manifest())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(m.id, ID);
        assert!(m.actions.iter().any(|a| a.id == "STUDIO_DAW_ENTER"));
        assert!(m.views.iter().any(|v| v.id == "daw"));
    }
}
