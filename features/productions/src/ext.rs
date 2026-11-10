//! The extension: one `linked::Extension`.
//!
//! Compiled into an Architect build that wants it and served as a
//! process the Architect app installs (`src/main.rs`, `extension.toml`'s
//! `[entry]`). It reads DAW files from disk ([`crate::find`],
//! [`crate::read`]) and reaches the vault only through the app's actions
//! ([`crate::host`]), so both copies do the same thing.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use architect_extensions::linked::{Builder, Extension};
use serde_json::{Map, Value, json};

use crate::find::{Found, find};
use crate::host::{self, Production};
use crate::model::{Daw, Stage, file_name, keys, note_path};
use crate::read::{Facts, read};
use crate::{bases, note, open, show};

/// The extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct Productions;

impl Extension for Productions {
    fn manifest(&self) -> &'static str {
        include_str!("../extension.toml")
    }

    fn build(&self, ext: &mut Builder) {
        host::connect(ext.handle());
        ext.action("STUDIO_PRODUCTIONS_SCAN", scan)
            .action("STUDIO_PRODUCTIONS_LIST", list)
            .action("STUDIO_PRODUCTIONS_SET_STAGE", set_stage)
            .action("STUDIO_PRODUCTIONS_OPEN", open_in_daw)
            .action("STUDIO_PRODUCTIONS_BASES", |_| async {
                Ok(json!({ "written": write_bases().await? }))
            });
    }
}

fn text(args: &Value, key: &str) -> Result<String, String> {
    opt(args, key).ok_or_else(|| format!("give `{key}`"))
}

fn opt(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

/// Runs `work` on a thread of its own: walking folders and parsing
/// sessions never holds up the app's actions.
async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = futures_channel::oneshot::channel();
    std::thread::Builder::new()
        .name("studio-productions-scan".to_owned())
        .spawn(move || {
            let _ = tx.send(work());
        })
        .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "the scan stopped".to_owned())
}

/// Writes the default bases that are not there: the paths written.
///
/// # Errors
///
/// `VAULT_CREATE` refused.
pub async fn write_bases() -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    for (name, yaml) in bases::defaults() {
        let path = bases::path(name);
        if host::run("BASES_READ", json!({ "path": path }))
            .await
            .is_ok()
        {
            continue;
        }
        match host::create(&path, None, &yaml).await {
            Ok(()) => written.push(path),
            // There, and not yet known as a base: a person's, left alone.
            Err(e) if e.contains("exist") || e.contains("taken") => {}
            Err(e) => return Err(e),
        }
    }
    if !written.is_empty() {
        // VAULT_CREATE indexes what it writes as a note: read the folder
        // again so Bases knows the new files are bases.
        host::run("VAULT_RELOAD", Value::Null).await?;
    }
    Ok(written)
}

/// A new production's note: `Projects/<name>/project.md`, a taken folder
/// getting ` 2`, ` 3`, …. The path written.
async fn create_note(found: &Found, props: &Map<String, Value>) -> Result<String, String> {
    let name = file_name(&found.name);
    let mut last = String::new();
    for n in 1_u32..=50 {
        let folder = if n == 1 {
            name.clone()
        } else {
            format!("{name} {n}")
        };
        let path = note_path(&folder);
        let body = format!(
            "# {}\n\nFound by Studio's scan ({}). Its stage, client and due date are yours; the rest is read from the file at every scan.\n",
            found.name,
            found.daw.label()
        );
        match host::create(&path, Some(props), &body).await {
            Ok(()) => return Ok(path),
            Err(e) if e.contains("exist") || e.contains("taken") => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// Make or update the show `facts` describes: its songs' notes (made
/// once) and its setlist (its `songs` kept in step). The setlist's path.
async fn write_show(title: &str, facts: &Facts, project: &str) -> Result<String, String> {
    let show = show::notes(title, &facts.songs, project);
    for song in &show.songs {
        if host::properties(&song.path).await.is_none() {
            host::create(&song.path, Some(&song.properties), &song.body).await?;
        }
    }
    let set = &show.setlist;
    match host::properties(&set.path).await {
        None => host::create(&set.path, Some(&set.properties), &set.body).await?,
        Some(old) => {
            if old.get("songs") != Some(&show.links) {
                host::set(&set.path, "songs", &show.links).await?;
            }
        }
    }
    Ok(set.path.clone())
}

/// One production found: its note made or brought up to date.
async fn file_one(
    found: &Found,
    facts: &Facts,
    known: &[Production],
    vault: Option<&Path>,
    now: SystemTime,
) -> Result<Value, String> {
    let mut read = note::read_fields(found, facts, now, vault);
    let location = read
        .get(keys::LOCATION)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let existing = known
        .iter()
        .find(|p| p.daw == found.daw && p.text(keys::LOCATION).as_deref() == Some(&location));
    let (path, old, outcome) = if let Some(p) = existing {
        let old = host::properties(&p.path).await.unwrap_or_default();
        (p.path.clone(), old, "updated")
    } else {
        let props = note::new_note(found, &found.name);
        let path = create_note(found, &props).await?;
        (path, props, "created")
    };
    let title = old
        .get(keys::TITLE)
        .and_then(Value::as_str)
        .unwrap_or(&found.name)
        .to_owned();

    let said = old
        .get(keys::SHOW)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let setlist = if found.daw == Daw::Ableton && facts.is_show(said) {
        let at = write_show(&title, facts, &path).await?;
        read.insert(keys::SETLIST.to_owned(), json!(show::link_to(&at, &title)));
        Some(at)
    } else {
        None
    };

    let changed = note::changes(&old, &read);
    for (key, value) in &changed {
        host::set(&path, key, value).await?;
    }
    let outcome = if outcome == "updated" && changed.is_empty() {
        "unchanged"
    } else {
        outcome
    };
    Ok(json!({
        "title": title,
        "path": path,
        "type": found.daw.project_type(),
        "daw": found.daw.label(),
        "outcome": outcome,
        "changed": changed.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        "stage": old.get(keys::STAGE),
        "tempo": read.get(keys::TEMPO),
        "tracks": read.get(keys::TRACKS),
        "missingMedia": read.get(keys::MISSING_MEDIA_COUNT),
        "songs": setlist.as_ref().map(|_| facts.songs.len()),
        "setlist": setlist,
        "error": facts.error,
    }))
}

/// `STUDIO_PRODUCTIONS_SCAN {path?, bases?}`: every production under
/// `path` (absolute, or inside the vault; none: the whole vault) made a
/// project note or brought up to date, and the default bases written.
///
/// # Errors
///
/// The folder is not there, or a vault action refused.
pub async fn scan(args: Value) -> Result<Value, String> {
    let vault = host::vault_root().await;
    let path = match (opt(&args, "path"), &vault) {
        (Some(p), root) => note::file_at(&p, root.as_deref()),
        (None, Some(root)) => root.clone(),
        (None, None) => return Err("give `path`: a folder to scan".to_owned()),
    };
    if !path.exists() {
        return Err(format!("{}: no such folder", path.display()));
    }
    let walked = path.clone();
    let found: Vec<(Found, Facts)> = off_thread(move || {
        find(&walked)
            .into_iter()
            .map(|f| {
                let facts = read(&f);
                (f, facts)
            })
            .collect()
    })
    .await?;

    let known = host::productions().await?;
    let now = SystemTime::now();
    let mut projects = Vec::new();
    for (one, facts) in &found {
        projects.push(file_one(one, facts, &known, vault.as_deref(), now).await?);
    }
    let written = if args.get("bases").and_then(Value::as_bool).unwrap_or(true) {
        write_bases().await?
    } else {
        Vec::new()
    };
    Ok(json!({
        "scanned": path.display().to_string(),
        "projects": projects,
        "bases": written,
    }))
}

/// `STUDIO_PRODUCTIONS_LIST {stage?, client?, type?}`.
///
/// # Errors
///
/// `stage` is not a stage, or `BASES_ROWS` refused.
pub async fn list(args: Value) -> Result<Value, String> {
    let stage = match opt(&args, "stage") {
        Some(s) => Some(Stage::parse(&s).ok_or_else(|| no_stage(&s))?),
        None => None,
    };
    let client = opt(&args, "client").map(|c| c.to_lowercase());
    let kind = opt(&args, "type");
    Ok(json!(
        host::productions()
            .await?
            .iter()
            .filter(|p| stage.is_none_or(|s| p.text(keys::STAGE).as_deref() == Some(s.as_str())))
            .filter(|p| client.as_ref().is_none_or(|c| p
                .text(keys::CLIENT)
                .is_some_and(|v| v.to_lowercase().contains(c))))
            .filter(|p| kind
                .as_ref()
                .is_none_or(|k| p.daw.project_type().eq_ignore_ascii_case(k)))
            .map(Production::json)
            .collect::<Vec<_>>()
    ))
}

fn no_stage(given: &str) -> String {
    format!(
        "`{given}` is not a stage (there are: {})",
        Stage::ALL.map(Stage::as_str).join(", ")
    )
}

/// `STUDIO_PRODUCTIONS_SET_STAGE {project, stage}`: a property edit.
///
/// # Errors
///
/// No such production or stage, or `VAULT_SET_PROPERTY` refused.
pub async fn set_stage(args: Value) -> Result<Value, String> {
    let given = text(&args, "stage")?;
    let stage = Stage::parse(&given).ok_or_else(|| no_stage(&given))?;
    let p = host::production(&text(&args, "project")?).await?;
    host::set(&p.path, keys::STAGE, &json!(stage.as_str())).await?;
    Ok(
        json!({ "title": p.title, "path": p.path, "stage": stage.as_str(), "was": p.text(keys::STAGE) }),
    )
}

/// The file production `p` opens with, on this machine.
async fn file_of(p: &Production) -> Result<PathBuf, String> {
    let place = p
        .text(keys::FILE)
        .ok_or_else(|| format!("{}: its note names no file", p.title))?;
    Ok(note::file_at(&place, host::vault_root().await.as_deref()))
}

/// `STUDIO_PRODUCTIONS_OPEN {project, dry_run?}`: the platform's open
/// command on the production's file. `dry_run` answers the command and
/// starts nothing.
///
/// # Errors
///
/// No such production, its file is not on this machine, or the command
/// did not start.
pub async fn open_in_daw(args: Value) -> Result<Value, String> {
    let p = host::production(&text(&args, "project")?).await?;
    let file = file_of(&p).await?;
    let (program, arguments) = open::command(&file);
    let dry = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !dry {
        open::open(&file)?;
    }
    Ok(json!({
        "title": p.title,
        "file": file.display().to_string(),
        "command": std::iter::once(program).chain(arguments).collect::<Vec<_>>(),
        "opened": !dry,
    }))
}
