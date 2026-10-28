//! Points of interest: a playlist of places in the song to check.
//!
//! What has come up while editing — a kick a sixteenth off, a dropout in
//! the overheads, a fill played late — each marked where it is with a
//! note, and stepped through (`{` / `}`) to listen to one after another,
//! each ticked off (`|`) once it is right. Kept with the song (its
//! project ext state, in the project doc: saved, and undone with the rest)
//! and drawn in the ruler's MARKS lane — amber while open, green once
//! done, ringed while it is the one stepped to.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(feature = "native")]
use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::ProjectContext;
use facet::Facet;

/// Where the list is kept: the song's ext state.
const SECTION: &str = "fts";
const KEY: &str = "poi";

/// One place to check.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Poi {
    pub id: String,
    /// Project seconds.
    pub at: f64,
    /// What is there to check.
    pub note: String,
    /// Checked and right.
    pub done: bool,
}

/// The list, in time order.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
struct List {
    points: Vec<Poi>,
}

/// The song's points of interest, in time order.
#[must_use]
pub fn read(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Poi> {
    daw_proto::ExtState::get_project(daw, project.clone(), SECTION, KEY)
        .and_then(|text| facet_json::from_str::<List>(&text).ok())
        .map(|l| l.points)
        .unwrap_or_default()
}

/// Keep `points` as the song's, in time order — as one undo step,
/// `label`: so it can be undone, and so it is recorded into the song's
/// file (a change the engine never finished as a step is not).
#[cfg(feature = "native")]
fn write(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    mut points: Vec<Poi>,
    label: &str,
) -> Result<Vec<Poi>, String> {
    points.sort_by(|a, b| a.at.total_cmp(&b.at));
    let text = facet_json::to_string(&List {
        points: points.clone(),
    })
    .map_err(|e| e.to_string())?;
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let set = daw_proto::ExtState::set_project(daw, project.clone(), SECTION, KEY, &text);
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    set.map_err(|e| e.to_string())?;
    publish(points.clone());
    crate::actions::list().changed("song.poi", "");
    Ok(points)
}

/// The list as the window draws it, and which one was stepped to.
static SHOWN: Mutex<Option<Arc<Vec<Poi>>>> = Mutex::new(None);
static CURRENT: Mutex<Option<String>> = Mutex::new(None);
static VERSION: AtomicU64 = AtomicU64::new(0);
static ASKED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "native")]
fn publish(points: Vec<Poi>) {
    if let Ok(mut shown) = SHOWN.lock() {
        *shown = Some(Arc::new(points));
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
    // Windows draw them from the marks the engine sends.
    crate::changes::announce(crate::changes::Change::Marks);
}

/// The points and the one stepped to, as the engine sent them with the
/// song's marks: what a window draws.
pub fn adopt(points: Vec<Poi>, stepped: Option<String>) {
    ASKED.store(true, Ordering::Release);
    if let Ok(mut shown) = SHOWN.lock() {
        *shown = Some(Arc::new(points));
    }
    if let Ok(mut c) = CURRENT.lock() {
        *c = stepped;
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
}

/// Goes up on every change: a window compares it with the one it drew.
#[must_use]
pub fn version() -> u64 {
    VERSION.load(Ordering::Relaxed)
}

/// The list, once read; read off the window's thread the first time.
pub fn shown() -> Option<Arc<Vec<Poi>>> {
    let shown = SHOWN.lock().ok().and_then(|s| s.clone());
    if shown.is_none() && !ASKED.swap(true, Ordering::AcqRel) {
        refresh();
    }
    shown
}

/// Read the list again, off the window's thread (a song opened, an undo).
pub fn refresh() {
    #[cfg(feature = "native")]
    if let Some(runtime) = crate::open::runtime() {
        runtime.spawn_blocking(|| {
            if let Ok((daw, project)) = crate::actions::song() {
                publish(read(daw, &project));
            }
        });
    }
}

pub fn current() -> Option<String> {
    CURRENT.lock().ok().and_then(|c| c.clone())
}

#[cfg(feature = "native")]
fn set_current(id: Option<String>) {
    if let Ok(mut c) = CURRENT.lock() {
        *c = id;
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
    crate::changes::announce(crate::changes::Change::Marks);
}

/// Where `at` is: a bar as the ruler writes it (`83.1`), `12.5s`, a
/// section; the edit cursor when left out.
#[cfg(feature = "native")]
fn at_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    at: Option<&str>,
) -> Result<f64, String> {
    at.map_or_else(
        || {
            Ok(crate::edit_state::current()
                .unwrap_or_else(|| crate::organize_edit::engine_edit(daw, project))
                .at)
        },
        |text| crate::track_regions::moment(daw, project, text),
    )
}

/// The one to step to from `here` by `by` (+1 the next after it, -1 the
/// one before), the open ones only when `open`.
#[cfg(feature = "native")]
fn step_from(points: &[Poi], here: f64, by: i32, open: bool) -> Option<&Poi> {
    /// Nearer than this to `here` is where it is, not after or before.
    const SAME: f64 = 0.005;
    let mut candidates = points.iter().filter(|p| !(open && p.done));
    if by >= 0 {
        candidates.find(|p| p.at > here + SAME)
    } else {
        candidates.rfind(|p| p.at < here - SAME)
    }
}

#[cfg(feature = "native")]
static LIST: ActionMeta = crate::actions::meta(
    "POI_LIST",
    "poi_list",
    "Points of interest",
    "The song's points of interest — places to check, each with its note — in time order: id, where (seconds, and the bar as the ruler writes it), the note, whether it is done, and which one was stepped to last.",
    "Session",
    "",
);
#[cfg(feature = "native")]
static ADD: ActionMeta = crate::actions::meta(
    "POI_ADD",
    "poi_add",
    "Add a point of interest",
    "Mark a place to check: `at` (a bar as the ruler writes it — `83.1`, `67.1.750` — `12.5s`, or a section; the edit cursor when left out) and `note` (what is there to check). One undo step. Its id.",
    "Session",
    "",
);
#[cfg(feature = "native")]
static EDIT: ActionMeta = crate::actions::meta(
    "POI_EDIT",
    "poi_edit",
    "Change a point of interest",
    "Change point `id` (the one stepped to last when left out): `note`, `done` (checked and right), or `at`. One undo step.",
    "Session",
    "",
);
#[cfg(feature = "native")]
static REMOVE: ActionMeta = crate::actions::meta(
    "POI_REMOVE",
    "poi_remove",
    "Remove points of interest",
    "Take points `ids` off the list; `done` true takes off every done one. One undo step. How many went.",
    "Session",
    "",
);
/// The window's bar step (`crate::bar_step::STEP_ACTION`), by id: the
/// engine does not reach into the window's modules.
#[cfg(feature = "native")]
const BAR_STEP: &str = "SESSION_STEP";

/// Step to the next or the one before (`}` / `{`).
#[cfg(feature = "native")]
pub static STEP: ActionMeta = crate::actions::meta(
    "POI_STEP",
    "poi_step",
    "Step to a point of interest",
    "Go to the next point of interest after the edit cursor (`by` 1, `}`) or the one before (`by` -1, `{`) — `open` true skips the done ones — its page of bars framed (SESSION_STEP) and the cursor on it. Says which.",
    "Session",
    "",
);
/// Tick the one stepped to off, or open it again (`|`).
#[cfg(feature = "native")]
pub static TOGGLE: ActionMeta = crate::actions::meta(
    "POI_TOGGLE",
    "poi_toggle",
    "Tick a point of interest off",
    "The point of interest stepped to last (or `id`) done, or open again if it was (`|`). One undo step. Says which, and what it is now.",
    "Session",
    "",
);

#[cfg(feature = "native")]
#[derive(Facet)]
struct ListArg {}

#[cfg(feature = "native")]
#[derive(Facet)]
struct PoiOut {
    id: String,
    at: f64,
    bar: String,
    note: String,
    done: bool,
    current: bool,
}

#[cfg(feature = "native")]
#[derive(Facet)]
struct AddArg {
    at: Option<String>,
    note: String,
}

#[cfg(feature = "native")]
#[derive(Facet)]
struct EditArg {
    id: Option<String>,
    note: Option<String>,
    done: Option<bool>,
    at: Option<String>,
}

#[cfg(feature = "native")]
#[derive(Facet)]
struct RemoveArg {
    #[facet(default)]
    ids: Vec<String>,
    done: Option<bool>,
}

#[cfg(feature = "native")]
#[derive(Facet)]
struct StepArg {
    by: Option<i32>,
    open: Option<bool>,
}

#[cfg(feature = "native")]
#[derive(Facet)]
struct ToggleArg {
    id: Option<String>,
}

#[cfg(feature = "native")]
fn out(daw: &daw::standalone::Standalone, project: &ProjectContext, p: &Poi) -> PoiOut {
    PoiOut {
        id: p.id.clone(),
        at: p.at,
        bar: crate::track_regions::bar_of(daw, project, p.at),
        note: p.note.clone(),
        done: p.done,
        current: current().as_deref() == Some(p.id.as_str()),
    }
}

/// The one an edit names: `id`, or the one stepped to last.
#[cfg(feature = "native")]
fn named(points: &[Poi], id: Option<&str>) -> Result<usize, String> {
    let id = id
        .map(str::to_owned)
        .or_else(current)
        .ok_or("no point named, and none stepped to")?;
    points
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("no point of interest {id}"))
}

#[cfg(feature = "native")]
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&LIST, Effect::ReadOnly, |_: ListArg| {
        let (daw, project) = crate::actions::song()?;
        Ok(read(daw, &project)
            .iter()
            .map(|p| out(daw, &project, p))
            .collect::<Vec<_>>())
    })?;
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = crate::actions::song()?;
        let at = at_of(daw, &project, a.at.as_deref())?;
        let id = crate::track_regions::fresh_id();
        let mut points = read(daw, &project);
        points.push(Poi {
            id: id.clone(),
            at,
            note: a.note,
            done: false,
        });
        write(daw, &project, points, "Add a point of interest")?;
        Ok(id)
    })?;
    list.register(&EDIT, Effect::Mutating, |a: EditArg| {
        let (daw, project) = crate::actions::song()?;
        let mut points = read(daw, &project);
        let k = named(&points, a.id.as_deref())?;
        let at =
            a.at.as_deref()
                .map(|t| crate::track_regions::moment(daw, &project, t))
                .transpose()?;
        let point = points.get_mut(k).ok_or("gone")?;
        if let Some(note) = a.note {
            point.note = note;
        }
        if let Some(done) = a.done {
            point.done = done;
        }
        if let Some(at) = at {
            point.at = at;
        }
        let changed = point.clone();
        write(daw, &project, points, "Edit a point of interest")?;
        Ok(out(daw, &project, &changed))
    })?;
    list.register(&REMOVE, Effect::Mutating, |a: RemoveArg| {
        let (daw, project) = crate::actions::song()?;
        let points = read(daw, &project);
        let before = points.len();
        let done = a.done.unwrap_or(false);
        let kept: Vec<Poi> = points
            .into_iter()
            .filter(|p| !(a.ids.contains(&p.id) || done && p.done))
            .collect();
        let gone = before.saturating_sub(kept.len());
        write(daw, &project, kept, "Remove a point of interest")?;
        Ok(u32::try_from(gone).unwrap_or(u32::MAX))
    })?;
    list.register(&STEP, Effect::Mutating, |a: StepArg| {
        let (daw, project) = crate::actions::song()?;
        let points = read(daw, &project);
        let here = at_of(daw, &project, None)?;
        let point = step_from(&points, here, a.by.unwrap_or(1), a.open.unwrap_or(false))
            .ok_or("no point of interest that way")?
            .clone();
        // The window pages its view to it, when one is attached (its bar
        // step, `crate::bar_step`); headless there is no view to page.
        crate::window_queue::ask(crate::window_queue::Asked::Run {
            id: BAR_STEP.to_owned(),
            args: format!(r#"{{"at":{}}}"#, point.at),
        });
        let playing = crate::engine::Transport::shared().is_some_and(|t| t.reading().playing);
        if !playing {
            daw::service::transport::service::Transport::set_position(
                daw,
                project.clone(),
                point.at,
            )
            .map_err(|e| e.to_string())?;
            crate::edit_state::ask(crate::edit_state::Edit {
                at: point.at,
                selection: None,
            });
        }
        set_current(Some(point.id.clone()));
        Ok(out(daw, &project, &point))
    })?;
    list.register(&TOGGLE, Effect::Mutating, |a: ToggleArg| {
        let (daw, project) = crate::actions::song()?;
        let mut points = read(daw, &project);
        let k = named(&points, a.id.as_deref())?;
        let point = points.get_mut(k).ok_or("gone")?;
        point.done = !point.done;
        let changed = point.clone();
        write(daw, &project, points, "Tick a point of interest")?;
        Ok(out(daw, &project, &changed))
    })?;
    Ok(())
}
