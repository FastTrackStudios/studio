//! One session per song, opened and led by the engine (studio#522).
//!
//! The session primitive's document (FastTrackStudios/session: an
//! Architect document file of kind `fts.session`) sits beside the song as
//! `<song stem>.fts-session`. Studio's own `Song.session` is the song's
//! project (`session-format`), not this document, and is never opened as
//! one.
//!
//! - **Opened** with `SessionFile::open` when it is there, else made
//!   (`SessionFile::create`), and saved as a document file: what Keyflow
//!   and the org's sync read (studio#522).
//! - **Its timing is the engine's.** On open the engine writes into it the
//!   song's tempo changes (with their meters), its markers, its sections
//!   as regions, and `bar_one`: a `SONGSTART` marker, else the first
//!   section that is not an intro or a count-in.
//! - **Provided:** `fts.session` is answered for it (`DAW_READ`,
//!   `DAW_WRITE`, `DAW_STRUCTURE`, `session::extension::answer`): Keyflow's
//!   chart (written by `KEYFLOW_SESSION_FOLLOW`'s `chart_path`) and
//!   Ignition's cue lists keep their data there as callers.
//! - **Led:** `session::lead::Leader` serves its presence and the host's
//!   clock and joins as `lead`; every move of the engine's transport is
//!   published there as `lead/play` (`publish`), the same moves the app's
//!   playhead deck gets. Keyflow's chart, Ignition's room and its cue list
//!   follow it (`KEYFLOW_SESSION_FOLLOW`, `IGNITION_SESSION_ROOM`, from
//!   `DAW_STATE`'s `session`).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use fts_session::file::SessionFile;
use fts_session::lead::Leader;
use fts_session::service::Sessions;
use fts_session::time::{Position, TempoPoint, TimePosition, TimeSignature};
use fts_session::{MarkId, Marker, Region, TempoMap, Time};
use serde_json::{Value as Json, json};

/// The session open now.
struct Open {
    song: String,
    path: PathBuf,
    id: String,
    file: SessionFile,
}

static OPEN: Mutex<Option<Open>> = Mutex::new(None);
static LEADER: Mutex<Option<Leader>> = Mutex::new(None);
static PROBLEM: Mutex<Option<String>> = Mutex::new(None);

fn sessions() -> &'static Sessions {
    static SESSIONS: OnceLock<Sessions> = OnceLock::new();
    SESSIONS.get_or_init(Sessions::default)
}

fn problem(why: Option<String>) {
    if let Some(why) = &why {
        tracing::warn!(session.problem = %why, "daw: the session document");
    }
    *PROBLEM.lock().unwrap_or_else(PoisonError::into_inner) = why;
}

/// Where song `source`'s session document is: beside it, as
/// `<stem>.fts-session` (beside the project directory for a Studio
/// project, `Song.session/`).
#[must_use]
pub fn path_for(source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .map_or_else(|| "song".into(), |s| s.to_string_lossy().into_owned());
    let dir = source.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!("{stem}.fts-session"))
}

/// `#rrggbb` as the session's colour.
fn color(css: Option<&str>) -> Option<u32> {
    let hex = css?.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// The song's bar 1: a `SONGSTART` marker, else the first section that is
/// not an intro or a count-in, else the bar `FTS_DAW_BAR_ONE` names, else
/// none.
fn bar_one(project: &daw_ui::studio::project::Project) -> Option<f64> {
    if let Some(m) = project
        .markers
        .iter()
        .find(|m| m.name.trim().eq_ignore_ascii_case("songstart"))
    {
        return Some(m.at);
    }
    let mut sections: Vec<_> = project.sections.iter().collect();
    sections.sort_by(|a, b| a.start.total_cmp(&b.start));
    let lead_in = |name: &str| {
        let n = name.trim().to_ascii_lowercase();
        n.starts_with("intro") || n.starts_with("count") || n.starts_with("click")
    };
    let from_sections = match sections.first() {
        Some(first) if lead_in(&first.name) => {
            sections.iter().find(|s| !lead_in(&s.name)).map(|s| s.start)
        }
        _ => None,
    };
    // A song that marks neither (no SONGSTART, no named regions: sections
    // as text items on a track, `>` markers): where the person opening it
    // says, `FTS_DAW_BAR_ONE` (a bar of the song, from 1), on the song's
    // own tempo.
    from_sections.or_else(|| {
        let bar: u32 = std::env::var("FTS_DAW_BAR_ONE").ok()?.trim().parse().ok()?;
        Some(seconds_at_bar(project, bar))
    })
}

/// Where bar `bar` (from 1) starts, seconds, on `project`'s tempo changes.
fn seconds_at_bar(project: &daw_ui::studio::project::Project, bar: u32) -> f64 {
    let mut changes: Vec<_> = project.tempo.iter().collect();
    changes.sort_by(|a, b| a.at.total_cmp(&b.at));
    let (mut at, mut bars) = (0.0_f64, 0.0_f64);
    let (mut bpm, mut beats) = changes.first().map_or((project.bpm, 4.0), |c| {
        (c.bpm, f64::from(c.beats_per_bar.max(1)))
    });
    let want = f64::from(bar.max(1).saturating_sub(1));
    for c in changes.iter().skip(1) {
        let bar_len = beats * 60.0 / bpm.max(1.0);
        let span = (c.at - at) / bar_len;
        if bars + span >= want {
            break;
        }
        bars += span;
        at = c.at;
        bpm = c.bpm;
        beats = f64::from(c.beats_per_bar.max(1));
    }
    at + (want - bars) * beats * 60.0 / bpm.max(1.0)
}

/// The engine's timing for `project`, written into `file`'s session.
async fn write_timing(
    file: &SessionFile,
    project: &daw_ui::studio::project::Project,
) -> Result<(), String> {
    let doc = file.session();
    let mut changes: Vec<_> = project.tempo.iter().collect();
    changes.sort_by(|a, b| a.at.total_cmp(&b.at));
    let first = changes.first();
    let meter = |c: &daw_ui::studio::project::TempoChange| {
        TimeSignature::new(c.beats_per_bar.max(1), c.beat_unit.max(1))
    };
    let tempo = TempoMap::new(
        first.map_or(project.bpm, |c| c.bpm),
        first.map_or_else(|| TimeSignature::new(4, 4), |c| meter(c)),
        changes
            .iter()
            .map(|c| {
                TempoPoint::with_time_signature(
                    Position::from_time(TimePosition::from_seconds(c.at)),
                    c.bpm,
                    meter(c),
                )
            })
            .collect(),
    );
    let err = |e: &dyn std::fmt::Display| e.to_string();
    doc.set_tempo(&tempo).map_err(|e| err(&e))?;
    for id in doc.markers().keys() {
        doc.remove_marker(id).map_err(|e| err(&e))?;
    }
    for id in doc.regions().keys() {
        doc.remove_region(id).map_err(|e| err(&e))?;
    }
    for m in &project.markers {
        doc.put_marker(
            &MarkId(format!("marker-{}", m.idx)),
            &Marker {
                at: Time::Seconds(m.at),
                name: m.name.clone(),
                color: color(m.color.as_deref()),
                ruler: Some(m.lane),
            },
        )
        .map_err(|e| err(&e))?;
    }
    for s in &project.sections {
        doc.put_region(
            &MarkId(format!("region-{}", s.id)),
            &Region {
                start: Time::Seconds(s.start),
                end: Time::Seconds(s.end),
                name: s.name.clone(),
                color: color(s.color.as_deref()),
                ruler: Some(s.lane),
            },
        )
        .map_err(|e| err(&e))?;
    }
    doc.set_bar_one(bar_one(project).map(Time::Seconds))
        .map_err(|e| err(&e))?;
    file.save().await.map_err(|e| err(&e))
}

/// Song `song` (the engine's project id) is current: its session
/// document opened.
///
/// From `source` (its file), laid out as `project`: the document given
/// the engine's timing and provided. Once a song: the same song again does
/// nothing.
pub fn song(song: &str, source: Option<&Path>, project: &daw_ui::studio::project::Project) {
    let mut open = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
    if open.as_ref().is_some_and(|o| o.song == song) {
        return;
    }
    let path = std::env::var_os("FTS_DAW_SESSION")
        .map(PathBuf::from)
        .or_else(|| source.map(path_for));
    let Some(path) = path else {
        problem(Some(
            "no file to keep the session beside: open a song from a file, or set FTS_DAW_SESSION"
                .to_owned(),
        ));
        return;
    };
    let Some(rt) = session_engine::open::runtime() else {
        problem(Some("no runtime to open the session on".to_owned()));
        return;
    };
    let opened = rt.block_on(async {
        // Its own file only, never Studio's `Song.session` (studio#522):
        // opened when it is there, else made.
        let file = if path.exists() {
            SessionFile::open(&path).await
        } else {
            SessionFile::create(&path).await
        }
        .map_err(|e| e.to_string())?;
        write_timing(&file, project).await?;
        Ok::<_, String>(file)
    });
    let file = match opened {
        Ok(file) => {
            problem(None);
            file
        }
        Err(e) => {
            problem(Some(format!("{}: {e}", path.display())));
            return;
        }
    };
    let id = file.id().to_string();
    if let Some(was) = open.take() {
        sessions().close(&was.id);
    }
    sessions().open(id.clone(), file.session().clone());
    tracing::info!(session.id = %id, session.path = %path.display(), "daw: the session document is open");
    *open = Some(Open {
        song: song.to_owned(),
        path,
        id,
        file,
    });
    drop(open);
    lead();
}

/// Serves the open session's presence and the host's clock, once per
/// session, on `FTS_DAW_LEAD_PORT` (4142) or the next port free.
fn lead() {
    let Some(id) = OPEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|o| uuid::Uuid::parse_str(&o.id).ok())
    else {
        return;
    };
    let Some(rt) = session_engine::open::runtime() else {
        problem(Some(
            "no runtime to serve the session's presence on".to_owned(),
        ));
        return;
    };
    let base: u16 = std::env::var("FTS_DAW_LEAD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4142);
    let mut last = String::new();
    for port in base..base.saturating_add(10) {
        // Free: the leader's own dial would otherwise find another host.
        if std::net::TcpListener::bind(("0.0.0.0", port)).is_err() {
            continue;
        }
        match rt.block_on(Leader::serve(&format!("0.0.0.0:{port}"), id)) {
            Ok(leader) => {
                tracing::info!(session.lead = %leader.url(), "daw: the session's transport is led from here");
                *LEADER.lock().unwrap_or_else(PoisonError::into_inner) = Some(leader);
                return;
            }
            Err(e) => last = e,
        }
    }
    problem(Some(format!(
        "the session's presence could not be served: {last}"
    )));
}

/// The engine's transport now: published as the session's `lead/play`.
pub fn publish(playing: bool, at: f64, rate: f64) {
    if let Some(leader) = LEADER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        leader.publish(playing, at.max(0.0), rate);
    }
}

/// The open session, for `DAW_STATE`: where it is, its id, where its
/// transport is led from.
#[must_use]
pub fn state() -> Json {
    let (path, id) = OPEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|o| (o.path.display().to_string(), o.id.clone()))
        .unzip();
    let url = LEADER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|l| l.url().to_owned());
    json!({
        "path": path,
        "id": id,
        "url": url,
        "problem": PROBLEM.lock().unwrap_or_else(PoisonError::into_inner).clone(),
    })
}

/// `fts.session`'s `method`, answered for the open session (the caller in
/// the action's context); an edit is saved.
///
/// # Errors
///
/// The session crate's refusal.
pub async fn call(method: &str, args: &Json, cx: &Json) -> Result<Json, String> {
    let caller = cx.get("caller").and_then(Json::as_str);
    let reply = fts_session::extension::answer(sessions(), method, &args.to_string(), caller)?;
    if method != fts_session::service::READ {
        // The file to save, out of the lock: a save awaits.
        let saving = OPEN
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|o| o.path.clone());
        if saving.is_some() {
            save().await;
        }
    }
    serde_json::from_str(&reply).map_err(|e| e.to_string())
}

/// Saves the open session to its file.
async fn save() {
    // Taken out for the save (a save awaits; the lock is not held across
    // it), and put back.
    let open = OPEN.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(open) = open {
        if let Err(e) = open.file.save().await {
            tracing::warn!(error = %e, "daw: the session document did not save");
        }
        let mut slot = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(open);
        }
    }
}
