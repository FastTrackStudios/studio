//! One session per song, opened and led by the engine (studio#522).
//!
//! A song is one file, `<Title>.session` (`song_file`): the engine's
//! project, and the session primitive's document (FastTrackStudios/session,
//! kind `fts.session`) as a part of the same store, under the song's id.
//! Until a folder is converted, the document sits beside the song as
//! `<song stem>.fts-session` (an Architect document file): told apart by
//! the files' headers, never their names.
//!
//! - **Opened** inside the song's one file (`song_file::SongFile`) when the
//!   song is one: a store with no `.fts-session` beside it. Otherwise the
//!   old pair's document, with `SessionFile::open` when it is there, else
//!   made (`SessionFile::create`) beside a song that is not a store (an
//!   `.RPP`).
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

/// Where the open session is kept.
enum Held {
    /// The old pair's `.fts-session`, a document file of its own.
    Pair(SessionFile),
    /// A part of the song's one file.
    One(song_file::SongFile),
}

impl Held {
    fn file(&self) -> &SessionFile {
        match self {
            Self::Pair(file) => file,
            Self::One(song) => song.session(),
        }
    }

    async fn save(&self) -> Result<(), String> {
        match self {
            Self::Pair(file) => file.save().await.map_err(|e| e.to_string()),
            // The session folded into the file, and the file's log too.
            Self::One(song) => song.save().await.map_err(|e| e.to_string()),
        }
    }
}

/// The session open now.
struct Open {
    song: String,
    path: PathBuf,
    id: String,
    file: Held,
    /// The song's timing last written into it ([`timing_of`]).
    timing: u64,
}

/// A fingerprint of what [`write_timing`] writes for `project`: its tempo
/// changes, markers and sections. When it moves, the session is behind.
#[must_use]
pub fn timing_of(project: &daw_ui::studio::project::Project) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    project.bpm.to_bits().hash(&mut h);
    for t in &project.tempo {
        (
            t.at.to_bits(),
            t.bpm.to_bits(),
            t.beats_per_bar,
            t.beat_unit,
        )
            .hash(&mut h);
    }
    for m in &project.markers {
        (m.at.to_bits(), &m.name, &m.color, m.idx, m.lane).hash(&mut h);
    }
    for s in &project.sections {
        (
            s.id,
            s.start.to_bits(),
            s.end.to_bits(),
            &s.name,
            &s.color,
            s.lane,
        )
            .hash(&mut h);
    }
    h.finish()
}

/// Song `song`'s timing as the engine has it now: its sections (regions),
/// markers and tempo changes, the rest of the model left empty. What
/// [`write_timing`] writes, read in this process's engine under its own
/// locks (no window's model, which is a snapshot from when it opened).
#[must_use]
pub fn timing_now(song: &str) -> Option<daw_ui::studio::project::Project> {
    use daw::service::TempoMap as _;
    use daw_ui::studio::project::{Marker as M, Project, Section, TempoChange};
    let song = session_engine::song::Song::named_or_current(Some(song)).ok()?;
    let (daw, ctx) = (song.daw(), song.context());
    let css = |c: u32| format!("#{:06x}", c & 0x00ff_ffff);
    let sections =
        <daw::standalone::Standalone as daw::service::region::Regions>::all(daw, ctx.clone())
            .into_iter()
            .map(|r| Section {
                id: r.id.unwrap_or_default(),
                start: r.time_range.start_seconds(),
                end: r.time_range.end_seconds(),
                name: r.name,
                color: r.color.map(css),
                lane: r.lane.unwrap_or(0),
            })
            .collect();
    let markers = <daw::standalone::Standalone as daw::service::Markers>::all(daw, ctx.clone())
        .into_iter()
        .filter(|m| !m.name.is_empty())
        .enumerate()
        .map(|(i, m)| M {
            at: m.position.seconds().unwrap_or(0.0),
            name: m.name,
            color: m.color.map(css),
            idx: m
                .id
                .unwrap_or(u32::try_from(i).unwrap_or(0).saturating_add(1)),
            lane: m.lane.unwrap_or(0),
        })
        .collect();
    let bpm = daw.get_default_tempo(ctx.clone());
    let mut tempo: Vec<TempoChange> = daw
        .get_tempo_points(ctx)
        .into_iter()
        .filter_map(|p| {
            Some(TempoChange {
                at: p.position.time?.as_seconds(),
                bpm: p.bpm,
                beats_per_bar: p.time_signature.as_ref().map_or(4, |t| t.numerator.max(1)),
                beat_unit: p
                    .time_signature
                    .as_ref()
                    .map_or(4, |t| t.denominator.max(1)),
            })
        })
        .collect();
    tempo.sort_by(|a, b| a.at.total_cmp(&b.at));
    Some(Project {
        sections,
        markers,
        bpm,
        tempo,
        ..Project::default()
    })
}

/// Whether song `song`'s session holds timing other than `timing`: the
/// song's sections, markers or tempo changed since they were written.
#[must_use]
pub fn behind(song: &str, timing: u64) -> bool {
    OPEN.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|o| o.song == song && o.timing != timing)
}

/// Song `song`'s session given `project`'s timing again (its sections,
/// markers and tempo changed in the engine): what a chart or a lighting
/// cue list reading the session follows. Nothing when it is not the song
/// open.
pub fn retime(song: &str, project: &daw_ui::studio::project::Project) {
    let Some(rt) = session_engine::open::runtime() else {
        return;
    };
    // Taken out for the write (it awaits), and put back.
    let open = OPEN.lock().unwrap_or_else(PoisonError::into_inner).take();
    let Some(mut open) = open else {
        return;
    };
    if open.song == song {
        match rt.block_on(write_timing(&open.file, project)) {
            Ok(()) => open.timing = timing_of(project),
            Err(e) => tracing::warn!(error = %e, "daw: the session's timing was not written again"),
        }
    }
    let mut slot = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_none() {
        *slot = Some(open);
    }
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

/// The song file and arrangement a song path names when it is a branch of
/// a one-file song (`Song.session/Live`): its parent is the song's store,
/// and it is no file of its own.
fn arrangement_of(source: &Path) -> Option<(PathBuf, String)> {
    let file = source.parent()?;
    let branch = source.file_name()?.to_string_lossy().into_owned();
    (!source.exists()
        && file.is_file()
        && session_format::sniff_path(file) == session_format::FileFormat::Store)
        .then(|| (file.to_path_buf(), branch))
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
    held: &Held,
    project: &daw_ui::studio::project::Project,
) -> Result<(), String> {
    let doc = held.file().session();
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
    held.save().await
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
    // An arrangement (`Song.session/Live`) is a branch of its song's one
    // file: its own session, in that file.
    let branch = source
        .filter(|_| std::env::var_os("FTS_DAW_SESSION").is_none())
        .and_then(arrangement_of);
    // The song's one file holds its session, once its folder is converted:
    // a store with no `.fts-session` left beside it.
    let one_file = branch.as_ref().map(|(file, _)| file.clone()).or_else(|| {
        source
            .filter(|_| std::env::var_os("FTS_DAW_SESSION").is_none())
            .filter(|s| {
                session_format::sniff_path(s) == session_format::FileFormat::Store
                    && !path_for(s).exists()
            })
            .map(Path::to_path_buf)
    });
    let path = one_file.clone().or_else(|| {
        std::env::var_os("FTS_DAW_SESSION")
            .map(PathBuf::from)
            .or_else(|| source.map(path_for))
    });
    let Some(path) = path else {
        problem(Some(
            "no file to keep the session in: open a song from a file, or set FTS_DAW_SESSION"
                .to_owned(),
        ));
        return;
    };
    let Some(rt) = session_engine::open::runtime() else {
        problem(Some("no runtime to open the session on".to_owned()));
        return;
    };
    let opened = rt.block_on(async {
        let held = if one_file.is_some() {
            Held::One(
                song_file::SongFile::open_branch(
                    &path,
                    branch
                        .as_ref()
                        .map_or(session_format::store::MAIN, |(_, b)| b),
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        } else if path.exists() {
            Held::Pair(SessionFile::open(&path).await.map_err(|e| e.to_string())?)
        } else {
            Held::Pair(
                SessionFile::create(&path)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        };
        write_timing(&held, project).await?;
        Ok::<_, String>(held)
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
    let id = file.file().id().to_string();
    if let Some(was) = open.take() {
        sessions().close(&was.id);
    }
    sessions().open(id.clone(), file.file().session().clone());
    tracing::info!(session.id = %id, session.path = %path.display(), "daw: the session document is open");
    *open = Some(Open {
        song: song.to_owned(),
        path,
        id,
        file,
        timing: timing_of(project),
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
                // A leader that serves its own presence has a URL (one on
                // its node's presence would have none).
                tracing::info!(
                    session.lead = leader.url().unwrap_or_default(),
                    "daw: the session's transport is led from here"
                );
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
        // `false` (nothing sent) only before the host's clock has its first
        // estimate, which `Leader::serve` waits for; the engine publishes
        // again on its next move, and re-anchors while it plays.
        let _sent = leader.publish(playing, at.max(0.0), rate);
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
        .and_then(|l| l.url().map(str::to_owned));
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
