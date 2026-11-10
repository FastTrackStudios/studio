//! `studio productions …`: the extension's actions from a terminal.
//!
//! The same functions the extension registers ([`crate::ext`]), run over
//! a vault folder opened with Architect's own vault and Bases actions:
//! every note is still written by `VAULT_*`, never by this crate. In a
//! running Architect app the same actions are `STUDIO_PRODUCTIONS_*`
//! (`architect-desktop actions run STUDIO_PRODUCTIONS_SCAN …`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use architect::action::ActionList;
use clap::{Args, Subcommand};
use futures::executor::block_on;
use serde_json::{Value, json};

use crate::{ext, host};

/// The vault.
#[derive(Debug, Args)]
pub struct Cli {
    /// The vault (a folder of notes) the productions are kept in.
    #[arg(long, env = "STUDIO_VAULT")]
    pub vault: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Find the Ableton sets, Pro Tools sessions, REAPER and Logic
    /// projects under a folder and make or update a project note each.
    Scan {
        /// The folder (default: the vault itself).
        path: Option<PathBuf>,
        /// Leave the default bases (Productions/*.base) unwritten.
        #[arg(long)]
        no_bases: bool,
    },
    /// The productions, a line each.
    List {
        /// Only those at this stage.
        #[arg(long)]
        stage: Option<String>,
        /// Only this client's.
        #[arg(long)]
        client: Option<String>,
    },
    /// Set a production's stage: writing, tracking, editing, mixing,
    /// mastering or delivered.
    Stage { project: String, stage: String },
    /// Open a production in its DAW.
    Open { project: String },
}

/// The app's side over the folder `vault`: Architect's vault and Bases
/// actions, behind the door the extension's functions go through.
fn connect(vault: &Path) -> Result<(), String> {
    let list = Arc::new(ActionList::new());
    let open = architect_vault::actions::OpenVault::open(vault).map_err(|e| e.to_string())?;
    architect_vault::actions::register(&list, &open).map_err(|e| e.to_string())?;
    // The list keeps the view types for as long as it runs: the process.
    let types: &'static architect_bases::model::ViewTypes =
        Box::leak(Box::new(architect_bases::model::ViewTypes::new()));
    architect_bases::actions::register(&list, &open, types).map_err(|e| e.to_string())?;
    host::connect_with(Arc::new(move |action, args: Value| {
        let list = Arc::clone(&list);
        Box::pin(async move {
            let out = list
                .run_json_async(&action, args.to_string().as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::from_slice(&out).unwrap_or(Value::Null))
        })
    }));
    Ok(())
}

fn field(p: &Value, key: &str) -> Option<String> {
    match p.get(key)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `Album — mixing · Ableton Live · 120 bpm · 18 tracks`.
fn line(p: &Value) -> String {
    let mut out = field(p, "title").unwrap_or_default();
    let parts: Vec<String> = [
        field(p, "outcome"),
        field(p, "stage"),
        field(p, "daw"),
        field(p, "client").map(|c| format!("for {c}")),
        field(p, "due").map(|d| format!("due {d}")),
        field(p, "key"),
        field(p, "tempo").map(|t| format!("{t} bpm")),
        field(p, "tracks").map(|t| format!("{t} tracks")),
        field(p, "missingMedia")
            .or_else(|| field(p, "missingMediaCount"))
            .filter(|n| n != "0")
            .map(|n| format!("{n} missing")),
        field(p, "songs").map(|n| format!("a show of {n} songs")),
        field(p, "error").map(|e| format!("did not read: {e}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !parts.is_empty() {
        let _ = write!(out, " — {}", parts.join(" · "));
    }
    out
}

fn lines(projects: &Value) -> String {
    projects
        .as_array()
        .into_iter()
        .flatten()
        .map(line)
        .collect::<Vec<_>>()
        .join("\n")
}

impl Cli {
    /// Run it: what it did, a line a production.
    ///
    /// # Errors
    ///
    /// The vault cannot be opened, or the action refused.
    pub fn run(self) -> Result<String, String> {
        connect(&self.vault)?;
        match self.command {
            Command::Scan { path, no_bases } => {
                let out = block_on(ext::scan(json!({
                    "path": path.map(|p| p.display().to_string()),
                    "bases": !no_bases,
                })))?;
                let mut text = lines(out.get("projects").unwrap_or(&Value::Null));
                let bases = out.get("bases").and_then(Value::as_array);
                for base in bases.into_iter().flatten() {
                    let _ = write!(text, "\nwrote {}", base.as_str().unwrap_or_default());
                }
                Ok(text)
            }
            Command::List { stage, client } => {
                let out = block_on(ext::list(json!({ "stage": stage, "client": client })))?;
                Ok(lines(&out))
            }
            Command::Stage { project, stage } => {
                let out = block_on(ext::set_stage(
                    json!({ "project": project, "stage": stage }),
                ))?;
                Ok(format!(
                    "{} — {} (was {})",
                    field(&out, "title").unwrap_or_default(),
                    field(&out, "stage").unwrap_or_default(),
                    field(&out, "was").unwrap_or_else(|| "unset".to_owned()),
                ))
            }
            Command::Open { project } => {
                let out = block_on(ext::open_in_daw(json!({ "project": project })))?;
                Ok(format!(
                    "opened {}",
                    field(&out, "file").unwrap_or_default()
                ))
            }
        }
    }
}
