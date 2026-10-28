//! Ingest on the Action List: `SESSION_INGEST` starts one on a thread of
//! its own and returns, `SESSION_INGEST_STATUS` says how it goes and,
//! once it is done, what it did. What the welcome screen's Publish and an
//! agent over MCP drive; `studio ingest` calls [`super::run`] itself.

use std::path::PathBuf;
use std::sync::Mutex;

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;

use super::{Media, Report, Request, Status};
use crate::actions::meta;

static INGEST: ActionMeta = meta(
    "SESSION_INGEST",
    "session_ingest",
    "Publish songs to Task",
    "Put songs from this machine into an org's Task library, ready to open anywhere — what `studio ingest` does: `inputs` the paths (a Live set `.als` to split, a song's folder, `.session` or `.RPP`, or a folder of song folders), `org` (FTS_TASK_ORG, else the one last picked), `project` an event or album project with `form` (album, concert, ep, lp, single, live-set), `setlist` its title (the project's when left out), `proxies_only` to leave the WAVs here, `link` to mint a public live link, `dry_run` to only say what would be sent. Prepares each song (proxies, peaks, reference), uploads what the server does not have, makes the setlist and projects; run again, it only updates. Returns at once — SESSION_INGEST_STATUS says how it goes.",
    "Session",
    "",
);

static STATUS: ActionMeta = meta(
    "SESSION_INGEST_STATUS",
    "session_ingest_status",
    "How the publish is going",
    "The ingest SESSION_INGEST started: `state` idle, running (with `status`: its phase, the song, files and bytes so far), done (with `report`: every song's root, what was sent and skipped, the setlist, the projects, the link) or failed (with `why`). Read-only.",
    "Session",
    "",
);

/// `SESSION_INGEST`'s arguments.
#[derive(Facet)]
struct IngestArg {
    inputs: Vec<String>,
    org: Option<String>,
    project: Option<String>,
    form: Option<String>,
    setlist: Option<String>,
    proxies_only: Option<bool>,
    link: Option<bool>,
    dry_run: Option<bool>,
}

/// How the last ingest stands.
#[derive(Debug, Clone, Default, PartialEq, Facet)]
pub struct IngestRow {
    /// `idle`, `running`, `done` or `failed`.
    pub state: String,
    pub status: Option<Status>,
    pub report: Option<Report>,
    pub why: Option<String>,
}

/// The one ingest this engine runs at a time, as it stands.
static NOW: Mutex<Option<IngestRow>> = Mutex::new(None);

/// How the ingest stands (idle when none has run).
#[must_use]
pub fn current() -> IngestRow {
    NOW.lock()
        .ok()
        .and_then(|now| now.clone())
        .unwrap_or_else(|| IngestRow {
            state: "idle".into(),
            ..IngestRow::default()
        })
}

fn set(row: IngestRow) {
    if let Ok(mut now) = NOW.lock() {
        *now = Some(row);
    }
}

/// Start ingesting `request` into `org`'s library as the person signed in
/// on this machine, on a thread of its own; [`current`] follows it.
///
/// # Errors
///
/// One is running already, nobody is signed in, or no org was named.
pub fn start(request: Request, org: Option<String>) -> Result<String, String> {
    use crate::task_account::AccountLibrary as _;
    if current().state == "running" {
        return Err("an ingest is running; SESSION_INGEST_STATUS says how it goes".into());
    }
    let account =
        crate::task_account::signed_in().ok_or("not signed in to Task on this machine")?;
    let org = org
        .or_else(|| std::env::var("FTS_TASK_ORG").ok().filter(|o| !o.is_empty()))
        .or_else(crate::task_account::remembered_org)
        .ok_or("which org? (`org`, or FTS_TASK_ORG)")?;
    let library = account.library(&org);
    let data = dirs::data_local_dir().ok_or("no data folder on this machine")?;
    let said = format!(
        "ingesting {} input(s) into {org}{}",
        request.inputs.len(),
        if request.dry_run { " (dry run)" } else { "" }
    );
    set(IngestRow {
        state: "running".into(),
        status: Some(Status::default()),
        ..IngestRow::default()
    });
    std::thread::Builder::new()
        .name("session-ingest".into())
        .spawn(move || {
            let progress = |status: &Status| {
                set(IngestRow {
                    state: "running".into(),
                    status: Some(status.clone()),
                    ..IngestRow::default()
                });
            };
            let row = match super::run(&library, &request, &data, &progress) {
                Ok(report) => IngestRow {
                    state: "done".into(),
                    report: Some(report),
                    ..IngestRow::default()
                },
                Err(e) => {
                    tracing::warn!(ingest.org = %org, error = %e, "ingest: failed");
                    IngestRow {
                        state: "failed".into(),
                        why: Some(format!("{e:#}")),
                        ..IngestRow::default()
                    }
                }
            };
            set(row);
        })
        .map_err(|e| e.to_string())?;
    Ok(said)
}

/// `SESSION_INGEST`'s arguments as a request and the org.
fn request_of(a: IngestArg) -> Result<(Request, Option<String>), String> {
    if a.inputs.is_empty() {
        return Err("`inputs`: the songs, folders or Live sets to publish".into());
    }
    let form = a
        .form
        .as_deref()
        .map(|f| super::Form::parse(f).ok_or_else(|| format!("`{f}` is not a project form")))
        .transpose()?;
    let mut request = Request::new(a.inputs.into_iter().map(PathBuf::from).collect());
    request.project = a.project;
    request.form = form;
    request.setlist = a.setlist;
    request.media = if a.proxies_only.unwrap_or(false) {
        Media::ProxiesOnly
    } else {
        Media::Originals
    };
    request.link = a.link.unwrap_or(false);
    request.dry_run = a.dry_run.unwrap_or(false);
    Ok((request, a.org))
}

/// The ingest actions, into `list`.
///
/// # Errors
///
/// The first registration the list refuses (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&INGEST, Effect::Mutating, |a: IngestArg| {
        let (request, org) = request_of(a)?;
        start(request, org)
    })?;
    list.register(&STATUS, Effect::ReadOnly, |(): ()| Ok(current()))?;
    Ok(())
}
