//! One session per song, opened and led by the engine (studio#522).
//!
//! A song is one file, `<Title>.session` (`song_file`): the engine's
//! project, and the session primitive's document (FastTrackStudios/session,
//! kind `fts.session`) as a part of the same store, under the song's id.
//! Until a folder is converted, the document sits beside the song as
//! `<song stem>.fts-session` (a document file of its own): told apart by
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
//!   playhead deck gets. Keyflow's chart, Ignition's visualizer and its
//!   cue list follow it (`KEYFLOW_SESSION_FOLLOW`, `IGNITION_SESSION_VIZ`,
//!   from `DAW_STATE`'s `session`, which `SESSION_FOLLOWED` answers on the
//!   engine's own MCP).
//! - **In a room** a member's engine opens what the leader loaded and keeps
//!   its transport on the leader's (`engine_collab::room`). The moves it
//!   makes following are the engine's moves all the same, published here,
//!   so a member's chart and lighting follow the room's session with no
//!   room of their own (studio#720).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use fts_session::file::SessionFile;
use fts_session::lead::Leader;
use fts_session::service::Sessions;
use serde_json::{Value as Json, json};

/// The session at `path`: a part of the song's one file on `one_file`'s
/// branch when it is one, else the old pair's document file, made if it
/// is not there.
fn open_held(path: &Path, one_file: Option<&str>) -> Result<Held, String> {
    if let Some(branch) = one_file {
        song_file::SongFile::open_branch(path, branch)
            .map(Held::One)
            .map_err(|e| e.to_string())
    } else if path.exists() {
        SessionFile::open(path)
            .map(Held::Pair)
            .map_err(|e| e.to_string())
    } else {
        SessionFile::create(path)
            .map(Held::Pair)
            .map_err(|e| e.to_string())
    }
}

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

    fn save(&self) -> Result<(), String> {
        match self {
            Self::Pair(file) => file.save().map_err(|e| e.to_string()),
            // The session folded into the file, and the file's log too.
            Self::One(song) => song.save().map_err(|e| e.to_string()),
        }
    }
}

/// The session open now.
struct Open {
    song: String,
    /// The song's own file (a branch's: its song's store), beside which
    /// its written chart is: where the session is kept may be elsewhere.
    source: Option<PathBuf>,
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
    let song = studio_engine::song::Song::named_or_current(Some(song)).ok()?;
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
    let (num, den) = daw.get_default_time_signature(ctx.clone());
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
    // A song with no tempo points has its project's tempo and meter from
    // the start (an RPP's `TEMPO 90 3 4`): a point at zero says so, as the
    // window's model has one (`daw_view::song::project`). Without it the
    // session was retimed in 4/4, and a 3/4 song's chart counted its bars
    // a third too long.
    if tempo.first().is_none_or(|first| first.at > 0.0) {
        tempo.insert(
            0,
            TempoChange {
                at: 0.0,
                bpm,
                beats_per_bar: u32::try_from(num.max(1)).unwrap_or(4),
                beat_unit: u32::try_from(den.max(1)).unwrap_or(4),
            },
        );
    }
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
    // Taken out for the write (it saves the file), and put back.
    let open = OPEN.lock().unwrap_or_else(PoisonError::into_inner).take();
    let Some(mut open) = open else {
        return;
    };
    if open.song == song {
        match write_timing(&open.file, project) {
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

/// The engine's timing for `project`, written into `file`'s session
/// (`studio_engine::session_timing`, which the web build writes its
/// in-memory session with too).
fn write_timing(held: &Held, project: &daw_ui::studio::project::Project) -> Result<(), String> {
    studio_engine::session_timing::write(held.file().session(), project)?;
    held.save()
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
    let one_file_branch = one_file.as_ref().map(|_| {
        branch
            .as_ref()
            .map_or(session_format::store::MAIN, |(_, b)| b.as_str())
    });
    let opened = open_held(&path, one_file_branch)
        .and_then(|held| write_timing(&held, project).map(|()| held));
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
        source: branch
            .map(|(file, _)| file)
            .or_else(|| source.map(Path::to_path_buf)),
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
///
/// On loopback unless `FTS_DAW_LEAD_BIND` names another address: the
/// presence is ungated, and whoever reaches it can write the transport
/// everyone follows. A room shares the transport with other machines
/// through its own door (`room::gate`), not this (studio#362). A lighting
/// desk on the network that follows the session's clock (Ignition's
/// `--clock session:ws://<this machine>:4142/vox#…`) needs
/// `FTS_DAW_LEAD_BIND=0.0.0.0`, on a network the owner trusts.
fn lead() {
    let Some(id) = OPEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|o| uuid::Uuid::parse_str(&o.id).ok())
    else {
        return;
    };
    let Some(rt) = studio_engine::open::runtime() else {
        problem(Some(
            "no runtime to serve the session's presence on".to_owned(),
        ));
        return;
    };
    let base: u16 = std::env::var("FTS_DAW_LEAD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4142);
    let host = std::env::var("FTS_DAW_LEAD_BIND")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_owned());
    let mut last = String::new();
    for port in base..base.saturating_add(10) {
        // Free: the leader's own dial would otherwise find another host.
        if std::net::TcpListener::bind((host.as_str(), port)).is_err() {
            continue;
        }
        match rt.block_on(Leader::serve(&format!("{host}:{port}"), id)) {
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

/// The open session's document and the song's own file, when it is song
/// `song`'s (the engine's project id): what the chart it shows is read
/// from (`crate::chart`).
#[must_use]
pub fn doc_of(song: &str) -> Option<(fts_session::doc::SessionDoc, Option<PathBuf>)> {
    OPEN.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .filter(|o| o.song == song)
        .map(|o| (o.file.file().session().clone(), o.source.clone()))
}

static FOLLOWED: architect::action::ActionMeta = architect::action::ActionMeta {
    id: "SESSION_FOLLOWED",
    trait_name: "",
    method_name: "followed",
    display_name: "The session followed",
    description: "The session document this engine has open for the current song, the one Keyflow's chart and Ignition's lighting follow (the app hands it to KEYFLOW_SESSION_FOLLOW and IGNITION_SESSION_VIZ, as DAW_STATE's `session`): its `path`, `id`, the `url` its transport is led from (the lighting's clock) and `chart` (a `.kf` beside the song). In a room a member's is the session the leader has open, opened here, its transport kept on the leader's, so the lighting follows the room. Read-only.",
    category: "View",
    group: "",
    shortcut: "",
    toggleable: false,
    undo: false,
};

/// Puts `SESSION_FOLLOWED` in the engine's Action List (once: a second
/// call finds it there), so an engine without a window answers what the
/// chart and the lighting follow over its own MCP.
pub fn register(list: &architect::action::ActionList) {
    use architect::action::{ActionCaller, ActionError, Effect, RawJson};
    match list.register_with(&FOLLOWED, Effect::ReadOnly, |_: &ActionCaller, (): ()| {
        Ok::<_, String>(RawJson::from(state().to_string()))
    }) {
        Ok(()) | Err(ActionError::Duplicate(_)) => {}
        Err(e) => tracing::error!(error = %e, "daw: SESSION_FOLLOWED did not register"),
    }
}

/// The open session, for `DAW_STATE` and `SESSION_FOLLOWED`: where it
/// is, its id, where its transport is led from.
#[must_use]
pub fn state() -> Json {
    let (path, id, chart) = OPEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map_or((None, None, None), |o| {
            (
                Some(o.path.display().to_string()),
                Some(o.id.clone()),
                // The chart written beside the song, for the chart pane
                // to type into a session nobody has charted yet.
                o.source
                    .as_deref()
                    .and_then(Path::parent)
                    .and_then(crate::chart::chart_in)
                    .map(|c| c.display().to_string()),
            )
        });
    let url = LEADER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|l| l.url().map(str::to_owned));
    json!({
        "path": path,
        "id": id,
        "chart": chart,
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
        // The file to save, out of the lock: a save writes the file.
        let saving = OPEN
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|o| o.path.clone());
        if saving.is_some() {
            save();
        }
    }
    serde_json::from_str(&reply).map_err(|e| e.to_string())
}

/// Saves the open session to its file.
fn save() {
    // Taken out for the save (the lock is not held across it), and put
    // back.
    let open = OPEN.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(open) = open {
        if let Err(e) = open.file.save() {
            tracing::warn!(error = %e, "daw: the session document did not save");
        }
        let mut slot = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(open);
        }
    }
}
