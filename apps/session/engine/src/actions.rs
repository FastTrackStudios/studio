//! Session's Action List: everything the engine can do, in the one
//! registry a window's keys and menus, the CLI and a model talking MCP all
//! run.
//!
//! [`list`] is the list (`architect::action::ActionList`), filled on first
//! use with the engine's actions — the transport, the set
//! (`crate::studio_engine::set_actions`) and the session's data: its
//! tracks, a drum's hits, a track's fader — run against the engine
//! directly, typed in and out, so a model can read the song and change it
//! with no window at all.
//!
//! A window adds its own when it attaches (`crate::window::attach`): zoom,
//! panels, the view, the edits its arrangement makes under its own
//! selection. Those queue for it (`crate::window_queue`), and are listed
//! `enabled: false` while no window is attached.
//!
//! Every engine edit is reported as a change (`crate::engine`), so a
//! model's `watch` follows what happens as it happens.

use std::sync::{Mutex, OnceLock};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ProjectContext, TempoMap as _, TrackRef, Tracks};
use facet::Facet;

pub use editing::drums::KitPiece;
use editing::drums::{KIT_MERGE, Moment, moments, weights};
use editing::quantize::shift_at;
use editing::quantize::{
    DOUBLE_STROKE, FURTHEST_PAST, GESTURE, NEAR_WRITTEN, ON_ITS_LINE, PLAYED_ON_A_LINE, Placed,
    RUN_BREAK, SEAM_ROOM, SlipMove, SlipResult, TWO_STROKES, WRITTEN_FILL, anchor_of, grid_choice,
    hold_gestures, hold_gestures_by, keeps_own_line, lock_anchored, one_stroke_a_line,
    to_lines_either_side, uncrossed,
};

static LIST: OnceLock<ActionList> = OnceLock::new();

/// The Action List, every action registered.
///
/// A registration architect refuses (a duplicate id) is a bug the
/// `the_list_registers_cleanly` test catches before it ships; a build that
/// still has one logs it and keeps the actions registered before it,
/// rather than taking the window down.
pub fn list() -> &'static ActionList {
    LIST.get_or_init(|| {
        let list = ActionList::new();
        if let Err(error) = register_all(&list) {
            tracing::error!(%error, "the action list stopped registering");
        }
        list
    })
}

/// Every action, into `list`: the window's, then the data's.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
///
/// [`ActionError::Duplicate`]: architect::action::ActionError::Duplicate
fn register_all(list: &ActionList) -> Result<(), architect::action::ActionError> {
    register_transport(list)?;
    register_data(list)?;
    crate::studio_engine::set_actions::register(list)
}

pub use crate::shared_actions::meta;
use crate::shared_actions::{
    FOLD, FoldArg, PHASE, PhaseArg, SELECT_TRACK, SelectTrackArg, TRACK_SET, TrackSetArg,
};

pub static PLAY_STOP: ActionMeta = meta(
    "SESSION_PLAY_STOP",
    "play_stop",
    "Play/stop",
    "Start playback, or stop it.",
    "Transport",
    "Space",
);
pub static PLAY_PAUSE: ActionMeta = meta(
    "SESSION_PLAY_PAUSE",
    "play_pause",
    "Play/pause",
    "Start playback, or pause it where it is.",
    "Transport",
    "",
);
pub static GO_TO_START: ActionMeta = meta(
    "SESSION_GO_TO_START",
    "go_to_start",
    "Go to start",
    "The edit cursor to the start of the song.",
    "Transport",
    "Home",
);
pub static STOP: ActionMeta = meta(
    "SESSION_STOP",
    "stop",
    "Stop",
    "Stop the transport where it is (SESSION_PLAY_STOP toggles).",
    "Transport",
    "",
);
pub static GO_TO_END: ActionMeta = meta(
    "SESSION_GO_TO_END",
    "go_to_end",
    "Go to end",
    "The edit cursor to the end of the song.",
    "Transport",
    "End",
);
pub static LOOP: ActionMeta = meta(
    "SESSION_LOOP",
    "loop",
    "Loop",
    "Turn looping over the loop range on or off.",
    "Transport",
    "",
);
pub static SEEK: ActionMeta = meta(
    "SESSION_SEEK",
    "seek",
    "Go to a time",
    "Move the play position to `at` seconds from the start of the song — what a click on a section in the navigator, the progress bar or the ruler does. Playing, it plays on from there.",
    "Transport",
    "",
);
pub static PLAY_FROM: ActionMeta = meta(
    "SESSION_PLAY_FROM",
    "play_from",
    "Play from a time",
    "Move to `at` seconds and play from there, as one step (the play cannot land before the move).",
    "Transport",
    "",
);

/// A time in the song, seconds from its start.
#[derive(Facet)]
struct AtArg {
    at: f64,
}

/// The action a press of the transport runs (`crate::engine::transport`):
/// its id and its arguments, `at` seconds where it takes a time.
#[must_use]
pub fn transport_action(command: crate::engine::Move, at: f64) -> (&'static str, String) {
    use crate::engine::Move as M;
    let at = || format!(r#"{{"at":{at}}}"#);
    match command {
        M::PlayStop => (PLAY_STOP.id, String::new()),
        M::Home => (GO_TO_START.id, String::new()),
        M::Stop => (STOP.id, String::new()),
        M::End => (GO_TO_END.id, String::new()),
        M::ToggleLoop => (LOOP.id, String::new()),
        M::ToggleRecord => (crate::record::TRANSPORT_RECORD.id, "{}".to_owned()),
        M::Seek => (SEEK.id, at()),
        M::PlayFrom => (PLAY_FROM.id, at()),
    }
}

/// The transport's actions: the engine's, not the window's — they run on
/// it at once, the window or none (headless, hidden, another monitor), and
/// a window shows what the engine does.
fn register_transport(list: &ActionList) -> Result<(), architect::action::ActionError> {
    for (meta, command) in [
        (&PLAY_STOP, crate::engine::Move::PlayStop),
        (&PLAY_PAUSE, crate::engine::Move::PlayStop),
    ] {
        list.register(meta, Effect::Mutating, move |(): ()| {
            crate::engine::transport_here(command, 0.0);
            if crate::engine::buffering::waiting() {
                let waiting = crate::open::current_song()
                    .and_then(|s| crate::stream_set::readiness(&s))
                    .map_or(0, |r| r.waiting_on);
                return Ok(format!(
                    "waiting: {waiting} recordings are not here yet — it plays by itself once they are (press again to call it off)"
                ));
            }
            Ok(format!("{}: done", meta.display_name))
        })?;
    }
    // Home is the engine's too; the window, when there is one, also takes
    // its edit cursor home (idempotent: it asks the engine again).
    list.register(&GO_TO_START, Effect::Mutating, |(): ()| {
        crate::engine::transport_here(crate::engine::Move::Home, 0.0);
        crate::window_queue::ask(crate::window_queue::Asked::GoToStart);
        Ok(format!("{}: done", GO_TO_START.display_name))
    })?;
    for (meta, command) in [
        (&STOP, crate::engine::Move::Stop),
        (&GO_TO_END, crate::engine::Move::End),
        (&LOOP, crate::engine::Move::ToggleLoop),
    ] {
        list.register(meta, Effect::Mutating, move |(): ()| {
            crate::engine::transport_here(command, 0.0);
            Ok(format!("{}: done", meta.display_name))
        })?;
    }
    for (meta, command) in [
        (&SEEK, crate::engine::Move::Seek),
        (&PLAY_FROM, crate::engine::Move::PlayFrom),
    ] {
        list.register(meta, Effect::Mutating, move |a: AtArg| {
            if !a.at.is_finite() || a.at < 0.0 {
                return Err("`at` is seconds from the start, 0 or more".to_owned());
            }
            crate::engine::transport_here(command, a.at);
            Ok(format!("{}: {:.3} s", meta.display_name, a.at))
        })?;
    }
    Ok(())
}

static TRACKS: ActionMeta = ActionMeta {
    category: "Session",
    ..meta(
        "SESSION_TRACKS",
        "tracks",
        "Tracks",
        "Every track in the open song: GUID, name, its folder path, folder or not, mute, solo, fader (dB), pan, how many items, its FX, its lanes, whether it is armed and what it records from (`record_input`: none, `audio N`, midi), and for a folder whether the window shows it open (`open`; false: folded to its row). `filter` keeps the tracks whose path contains it.",
        "Data",
        "",
    )
};
static DRUM_FUNDAMENTAL: ActionMeta = meta(
    "DRUMS_FUNDAMENTAL",
    "drum_fundamental",
    "Drum fundamental",
    "A drum's fundamental (Hz), read off its mics over the ring after each of its trigger's hits — what its Fund/Sub band-pass is tuned to. `range`: kick, snare or tom.",
    "Drums",
    "",
);

/// Which tracks.
#[derive(Facet)]
struct TracksArg {
    /// Keep the tracks whose folder path contains this (case aside).
    filter: Option<String>,
}

/// A track, as the list describes it.
#[derive(Facet)]
struct TrackRow {
    guid: String,
    name: String,
    /// The folders it is in and its name: `Drums/Drum Kit/Kick/Sum/In`.
    path: String,
    folder: bool,
    muted: bool,
    soloed: bool,
    /// Its polarity is flipped.
    inverted: bool,
    volume_db: f64,
    pan: f64,
    items: u32,
    fx: Vec<String>,
    /// Its fixed lanes, when it has them, named lanes and the playing
    /// one first — then the rest only by count.
    lanes: Vec<LaneRow>,
    /// Armed to record.
    armed: bool,
    /// What it records from: `none`, `audio 1` (an input, from 1), `midi`
    /// or `raw N`.
    record_input: String,
    /// A folder: whether the window shows what is in it (`false`: folded
    /// to its own row). None for a track.
    open: Option<bool>,
}

/// What `input` says, as [`TrackRow::record_input`] reads.
fn record_input_of(input: daw_proto::track::RecordInput) -> String {
    use daw_proto::track::RecordInput;
    match input {
        RecordInput::None => "none".to_owned(),
        RecordInput::Audio { channel } => format!("audio {}", channel.saturating_add(1)),
        RecordInput::Midi { .. } => "midi".to_owned(),
        RecordInput::Raw(raw) => format!("raw {raw}"),
    }
}

/// Which folders the window shows open, by GUID: every track planned as
/// the window plans its rows ([`crate::folds::apply`]), a folder open
/// when the row after it is one of its own.
fn open_folders(tracks: &[daw_proto::Track]) -> std::collections::HashSet<String> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let depth = |track: &daw_proto::Track| {
        let mut depth = 0_u32;
        let mut at = track.parent_guid.as_deref();
        while let Some(parent) = at.and_then(|p| by_guid.get(p)) {
            depth = depth.saturating_add(1);
            at = parent.parent_guid.as_deref();
        }
        depth
    };
    let rows: Vec<(daw_proto::Track, u32)> = tracks.iter().map(|t| (t.clone(), depth(t))).collect();
    let shown = crate::folds::apply(rows);
    shown
        .iter()
        .enumerate()
        .filter(|(i, (track, _))| track.is_folder && !crate::folds::shut(&shown, *i))
        .map(|(_, (track, _))| track.guid.clone())
        .collect()
}

/// One fixed lane.
#[derive(Facet)]
struct LaneRow {
    lane: u32,
    name: String,
    items: u32,
    plays: bool,
}

/// `track`'s lanes worth listing: those named something other than their
/// number, and those playing.
fn lane_rows(track: &daw_proto::Track, items: &[daw_proto::Item]) -> Vec<LaneRow> {
    (0..track.lane_count)
        .filter_map(|lane| {
            let name = usize::try_from(lane)
                .ok()
                .and_then(|i| track.lane_names.get(i))
                .cloned()
                .unwrap_or_default();
            let plays = lane < 64 && track.lane_play_mask & (1_u64 << lane) != 0;
            let numbered = name.trim().parse::<u32>().is_ok() || name.trim().is_empty();
            (plays || !numbered).then(|| LaneRow {
                lane,
                items: u32::try_from(
                    items
                        .iter()
                        .filter(|i| i.fixed_lane.unwrap_or(0) == lane)
                        .count(),
                )
                .unwrap_or(u32::MAX),
                name,
                plays,
            })
        })
        .collect()
}

/// Which drum.
#[derive(Facet)]
struct FundamentalArg {
    /// Its trigger: GUID, path or name.
    trigger: String,
    /// Its mics: GUIDs, paths or names.
    mics: Vec<String>,
    range: DrumRange,
}

#[derive(Facet, Clone, Copy)]
#[repr(u8)]
enum DrumRange {
    Kick,
    Snare,
    Tom,
}

/// How many hits a page holds unless asked.
const PAGE: u32 = 200;

pub static MODE: ActionMeta = meta(
    "SESSION_MODE",
    "mode",
    "Mode",
    "Put the window in a mode: Organize, Write, Produce, Record, Edit, Mix, Master, Live, Video or Scoring — what the panels show and how (Edit shows every cut on a folder's row; Live and Mix a folder's row whole).",
    "View",
    "",
);

/// Which mode.
#[derive(Facet)]
struct ModeArg {
    mode: String,
}

/// The mode a caller asked for, until the window takes it
/// ([`take_mode_request`]).
static MODE_REQUEST: Mutex<Option<session::modes::Mode>> = Mutex::new(None);

/// The mode asked for since the window last looked, if any.
pub fn take_mode_request() -> Option<session::modes::Mode> {
    MODE_REQUEST.lock().ok()?.take()
}

/// The window's in a process of its own: an action that sets what the
/// window shows (its mode, a mode's phase), run on the engine's list, is
/// asked of the window too — run there on the window's own list, where the
/// shell reads it. In the window's process no window follows it, so the
/// ask goes nowhere and nothing loops.
pub(crate) fn also_in_the_window(id: &str, args: String) {
    if crate::window_queue::engine_has_window() {
        crate::window_queue::ask(crate::window_queue::Asked::Run {
            id: id.to_owned(),
            args,
            ticket: 0,
        });
    }
}

/// The mode the window shows, as its shell last said (`note_shown_mode`).
static SHOWN_MODE: Mutex<Option<session::modes::Mode>> = Mutex::new(None);

/// The window's shell shows `mode` now.
pub fn note_shown_mode(mode: session::modes::Mode) {
    if let Ok(mut slot) = SHOWN_MODE.lock() {
        *slot = Some(mode);
    }
}

/// The mode the window shows, once its shell has said.
#[must_use]
pub fn shown_mode() -> Option<session::modes::Mode> {
    SHOWN_MODE.lock().ok().and_then(|m| *m)
}

/// Ask the window for `mode`, as `SESSION_MODE` does — for an action that
/// leaves the window in a mode of its own (New Project opens in Organize).
pub fn request_mode(mode: session::modes::Mode) {
    if let Ok(mut slot) = MODE_REQUEST.lock() {
        *slot = Some(mode);
    }
}

static SECTIONS: ActionMeta = meta(
    "SESSION_SECTIONS",
    "sections",
    "Sections",
    "The song's sections (its regions), in order: name, start and end seconds, and the bars they span — what SESSION_VIEW's `section` names.",
    "View",
    "",
);

/// One section.
#[derive(Facet)]
pub struct SectionOut {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub first_bar: i32,
    pub last_bar: i32,
}

/// The song's sections, in order.
pub fn sections(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<SectionOut> {
    let mut all: Vec<SectionOut> =
        <daw::standalone::Standalone as daw::service::region::Regions>::all(daw, project.clone())
            .into_iter()
            // The sections lane's (a region on no lane is on it): not the
            // song's own region, nor an edit zone on the Drums lane.
            .filter(|r| {
                r.lane
                    .is_none_or(|l| l == session::ruler_lanes::CoreLane::Sections.lane_index())
            })
            .map(|r| {
                let (start, end) = (r.time_range.start_seconds(), r.time_range.end_seconds());
                SectionOut {
                    name: r.name,
                    start,
                    end,
                    first_bar: crate::song_bars::musical(daw, &project, start + 1e-6).0,
                    last_bar: crate::song_bars::musical(daw, &project, (end - 1e-6).max(start)).0,
                }
            })
            .collect();
    all.sort_by(|a, b| a.start.total_cmp(&b.start));
    all
}

fn register_sections(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SECTIONS, Effect::ReadOnly, |(): ()| {
        let (daw, project) = song()?;
        Ok(sections(daw, &project))
    })?;
    Ok(())
}

fn register_phase(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&PHASE, Effect::Mutating, |a: PhaseArg| {
        let (mode, slug) = crate::shared_actions::pick_phase(a.mode.as_deref(), &a.phase)?;
        session::mode_phases::choose(mode, slug);
        also_in_the_window(
            PHASE.id,
            format!(r#"{{"phase":"{}","mode":"{}"}}"#, slug, mode.display_name()),
        );
        Ok(())
    })?;
    Ok(())
}

static WINDOW_TARGET: ActionMeta = meta(
    "SESSION_WINDOW_TARGET",
    "window_target",
    "Which window the actions drive",
    "With more than one window following the engine, which one its window actions (SESSION_VIEW, SESSION_ZOOM, SESSION_FRAMES, SESSION_WINDOW, SESSION_MODE…) go to: `window` by name (FTS_WINDOW_NAME, `main` by default); empty or left out: whichever takes them first. Says the windows seen and the one now named.",
    "View",
    "",
);

/// Which window, by name.
#[derive(Facet)]
struct WindowTargetArg {
    window: Option<String>,
}

/// What [`WINDOW_TARGET`] answers.
#[derive(Facet)]
struct WindowTarget {
    /// The window named now; `None`: whichever takes them first.
    target: Option<String>,
    /// The windows that have taken the engine's asks, newest last.
    seen: Vec<String>,
}

fn register_window_target(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&WINDOW_TARGET, Effect::Mutating, |a: WindowTargetArg| {
        if let Some(window) = a.window {
            crate::window_queue::set_target(Some(window));
        }
        Ok(WindowTarget {
            target: crate::window_queue::target(),
            seen: crate::window_queue::seen_windows(),
        })
    })?;
    Ok(())
}

fn register_mode(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&MODE, Effect::Mutating, |a: ModeArg| {
        let mode = session::modes::Mode::ALL
            .into_iter()
            .find(|m| m.display_name().eq_ignore_ascii_case(a.mode.trim()))
            .ok_or_else(|| {
                let names: Vec<&str> = session::modes::Mode::ALL
                    .iter()
                    .map(|m| m.display_name())
                    .collect();
                format!("no mode {}: one of {names:?}", a.mode)
            })?;
        if let Ok(mut slot) = MODE_REQUEST.lock() {
            *slot = Some(mode);
        }
        also_in_the_window(MODE.id, format!(r#"{{"mode":"{}"}}"#, mode.display_name()));
        Ok(())
    })?;
    Ok(())
}

pub static SCREENSET: ActionMeta = meta(
    "SESSION_SCREENSET",
    "screenset",
    "Screenset",
    "Recall a screenset — a saved layout of the window's panels — by its number or name: 1 Performance, 2 Overview, 3 Session, 4 Mix, 5 Edit, 6 Charts, 7 Signal, or one saved since.",
    "View",
    "",
);

pub static SCREENSET_SAVE: ActionMeta = meta(
    "SESSION_SCREENSET_SAVE",
    "save_screenset",
    "Save Screenset",
    "Keep the window's layout in the screenset on show, for this kind of screen (phone, tablet or desk, upright or sideways).",
    "View",
    "",
);

pub static SCREENSET_SAVE_AS: ActionMeta = meta(
    "SESSION_SCREENSET_SAVE_AS",
    "save_screenset_as",
    "Save Screenset As",
    "Keep the window's layout as a new screenset, named `name` (else numbered), and show it.",
    "View",
    "",
);

pub static ARRANGE: ActionMeta = meta(
    "SESSION_ARRANGE",
    "arrange",
    "Arrange Panels",
    "Turn arranging on or off: every panel shows its name and can be dragged to an edge, beside another, or onto it as a tab (holding W does the same while held).",
    "View",
    "",
);

pub static DOCKS: ActionMeta = meta(
    "SESSION_DOCKS",
    "docks",
    "Hide or Show the Docks",
    "Hide every edge dock of the window (left, right, top, bottom; the centre stays), or bring back the ones hidden as they were. A panel shown while they are hidden comes back on its own, and stays when the rest return.",
    "View",
    "Shift+D",
);

pub static PANEL: ActionMeta = meta(
    "SESSION_PANEL",
    "panel",
    "Show or Hide a Panel",
    "Open a panel of `kind` where that kind lives when the window shows none, or close every one it shows: arrangement, mixer, editor, fx-row, transport, progress, chart, chart-editor, lyrics, set, setlist, browser, inspector, versions, collab, rig, switches, navigator, now, gig-chart, tones.",
    "View",
    "",
);

/// Which panel kind.
#[derive(Facet)]
struct PanelArg {
    kind: String,
}

/// Which screenset: its number key or its name.
#[derive(Facet)]
struct ScreensetArg {
    screenset: String,
}

/// A name for a new screenset, if any.
#[derive(Facet)]
struct SaveAsArg {
    #[facet(default)]
    name: Option<String>,
}

/// What the window's dock is asked to do (`session_desktop`'s dock reads
/// these with [`take_dock_requests`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockRequest {
    /// Recall the screenset with this number key or name.
    Recall(String),
    /// Save the layout into the screenset on show.
    Save,
    /// Save the layout as a new screenset.
    SaveAs(Option<String>),
    /// Turn arranging on or off.
    Arrange,
    /// Open a panel of this kind, or close the ones shown.
    TogglePanel(String),
    /// Hide every edge dock, or bring back the ones hidden.
    ToggleDocks,
}

static DOCK_REQUESTS: Mutex<Vec<DockRequest>> = Mutex::new(Vec::new());

/// Asks the window's dock for `request`, as the View actions do — for a
/// menu that runs in the window's process (Studio's macOS menu bar).
pub fn request_dock(request: DockRequest) {
    if let Ok(mut queue) = DOCK_REQUESTS.lock() {
        queue.push(request);
    }
}

/// What the dock was asked since it last looked, oldest first.
pub fn take_dock_requests() -> Vec<DockRequest> {
    DOCK_REQUESTS
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default()
}

/// Whether the window is a dock now (its `SongDock` mounted): the pane
/// keys and actions (`x`, `e`, `f`, `b`) toggle its panels then, not the
/// old column's panes.
static DOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The window is a dock, or no longer one.
pub fn set_docked(on: bool) {
    DOCKED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the window is a dock.
#[must_use]
pub fn docked() -> bool {
    DOCKED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Asks the dock for `request` when the window is a dock, saying whether
/// it did — else the caller presses its pane, as before the dock.
pub fn if_docked(request: DockRequest) -> bool {
    if docked() {
        request_dock(request);
    }
    docked()
}

fn register_dock(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SCREENSET, Effect::Mutating, |a: ScreensetArg| {
        let which = a.screenset.trim().to_owned();
        if which.is_empty() {
            return Err("name a screenset: its number or its name".to_owned());
        }
        request_dock(DockRequest::Recall(which.clone()));
        let quoted = facet_json::to_string(&which).unwrap_or_default();
        also_in_the_window(SCREENSET.id, format!(r#"{{"screenset":{quoted}}}"#));
        Ok(())
    })?;
    list.register(&SCREENSET_SAVE, Effect::Mutating, |(): ()| {
        request_dock(DockRequest::Save);
        also_in_the_window(SCREENSET_SAVE.id, "{}".to_owned());
        Ok(())
    })?;
    list.register(&SCREENSET_SAVE_AS, Effect::Mutating, |a: SaveAsArg| {
        let name = a
            .name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty());
        request_dock(DockRequest::SaveAs(name.clone()));
        let args = name.map_or_else(
            || "{}".to_owned(),
            |n| {
                format!(
                    r#"{{"name":{}}}"#,
                    facet_json::to_string(&n).unwrap_or_default()
                )
            },
        );
        also_in_the_window(SCREENSET_SAVE_AS.id, args);
        Ok(())
    })?;
    list.register(&PANEL, Effect::Mutating, |a: PanelArg| {
        let kind = a.kind.trim().to_owned();
        if kind.is_empty() {
            return Err("name a panel kind".to_owned());
        }
        let quoted = facet_json::to_string(&kind).unwrap_or_default();
        request_dock(DockRequest::TogglePanel(kind));
        also_in_the_window(PANEL.id, format!(r#"{{"kind":{quoted}}}"#));
        Ok(())
    })?;
    list.register(&DOCKS, Effect::Mutating, |(): ()| {
        request_dock(DockRequest::ToggleDocks);
        also_in_the_window(DOCKS.id, "{}".to_owned());
        Ok(())
    })?;
    list.register(&ARRANGE, Effect::Mutating, |(): ()| {
        request_dock(DockRequest::Arrange);
        also_in_the_window(ARRANGE.id, "{}".to_owned());
        Ok(())
    })?;
    Ok(())
}

fn register_data(list: &ActionList) -> Result<(), architect::action::ActionError> {
    register_mode(list)?;
    register_dock(list)?;
    register_window_target(list)?;
    register_phase(list)?;
    crate::key_change::register(list)?;
    register_sections(list)?;
    register_tracks(list)?;
    register_fundamental(list)?;
    crate::hits::register(list)?;
    crate::hits_midi::register(list)?;
    crate::seams::register(list)?;
    crate::comp_keep::register(list)?;
    crate::comp_keep::register_main(list)?;
    crate::item_actions::register(list)?;
    crate::arrange_actions::register(list)?;
    crate::take_rank::register(list)?;
    register_track_set(list)?;
    register_reference(list)?;
    #[cfg(feature = "native")]
    crate::fx_actions::register(list)?;
    crate::vocal_level::register(list)?;
    crate::automation_actions::register(list)?;
    #[cfg(feature = "native")]
    crate::comp::register(list)?;
    #[cfg(feature = "native")]
    crate::record::register(list)?;
    #[cfg(feature = "native")]
    crate::render_ahead::register(list)?;
    #[cfg(feature = "native")]
    register_window_controls(list)?;
    crate::grow::register(list)?;
    crate::project_new::register(list)?;
    crate::instrument_tracks::register(list)?;
    crate::timeline_actions::register(list)?;
    crate::organize_edit::register(list)?;
    crate::page::register(list)?;
    crate::prepare::register(list)?;
    crate::edit_actions::register(list)?;
    crate::perf::register(list)?;
    crate::audio_health::register(list)?;
    crate::perf_meter::register(list)?;
    crate::trigger_from_mics::register(list)?;
    crate::comp_rank::register(list)?;
    crate::comp_song::register(list)?;
    crate::poi::register(list)?;
    crate::track_regions::register(list)?;
    crate::ingest::actions::register(list)?;
    #[cfg(feature = "signal")]
    crate::rig_actions::register(list)?;
    Ok(())
}

static REFERENCE_MAP: ActionMeta = meta(
    "DRUMS_REFERENCE_MAP",
    "drum_reference_map",
    "Drum MIDI map",
    "The song's drum MIDI (the part as written) mapped to the kit: each note number, how many notes it has, and the piece whose trigger hits its notes land on — learned from the recording, since a drum library's map is its own. Unmapped: a cymbal, an articulation, a piece with no trigger.",
    "Drums",
    "",
);
static REFERENCE: ActionMeta = meta(
    "DRUMS_REFERENCE",
    "drum_reference",
    "Drum MIDI notes",
    "The drum part as written — the song's drum MIDI — for a piece (`Kick`, `Snare`, `Tom 1`, through the learned map) or a note number: each note's time, bar, beat and velocity. Paged; `from`/`to` (seconds) narrow it.",
    "Drums",
    "",
);
static COMPARE: ActionMeta = meta(
    "DRUMS_COMPARE",
    "drum_compare",
    "Drums against the MIDI",
    "A piece's recorded hits (its trigger) held up to its notes in the drum MIDI: how many were played, missing (written, not played) and extra (played, not written), the hits' offsets from their notes (ms, late positive: median, worst), then each note — worst first — with its bar and beat. Not every fill is as written: read missing and extra as where the take departs from the plan.",
    "Drums",
    "",
);

/// Which drum MIDI.
#[derive(Facet)]
struct MapArg {
    /// The drum MIDI track; found by name (`DRUM MIDI`) when left out.
    track: Option<String>,
}

/// One note number's place.
#[derive(Facet)]
struct MapRow {
    pitch: u8,
    notes: u32,
    piece: Option<String>,
    /// The share of its notes on that piece's hits (0..1).
    agreement: f64,
}

/// Which written notes.
#[derive(Facet)]
struct ReferenceArg {
    track: Option<String>,
    /// A piece, through the learned map.
    piece: Option<String>,
    /// Or a note number.
    pitch: Option<u8>,
    from: Option<f64>,
    to: Option<f64>,
    offset: Option<u32>,
    limit: Option<u32>,
}

/// One written note.
#[derive(Facet)]
struct NoteRow {
    time: f64,
    bar: i32,
    beat: f64,
    pitch: u8,
    velocity: u8,
}

/// A page of written notes.
#[derive(Facet)]
struct NotesPage {
    total: u32,
    offset: u32,
    notes: Vec<NoteRow>,
}

/// Which piece to compare.
#[derive(Facet)]
struct CompareArg {
    /// The piece: `Kick`, `Snare`, `Tom 1`.
    piece: String,
    track: Option<String>,
    /// How far a hit may be from its note and still be it (ms; 60).
    window_ms: Option<f64>,
    from: Option<f64>,
    to: Option<f64>,
    /// How many notes to detail (worst first; 50).
    limit: Option<u32>,
}

/// A note held up to its hit.
#[derive(Facet)]
struct NoteVsHit {
    time: f64,
    bar: i32,
    beat: f64,
    /// The hit's offset from the note, ms (late positive); `None`: not
    /// played.
    offset_ms: Option<f64>,
}

/// A piece against its part.
#[derive(Facet)]
struct CompareResult {
    piece: String,
    pitches: Vec<u8>,
    notes: u32,
    hits: u32,
    matched: u32,
    missing: u32,
    extra: u32,
    median_offset_ms: Option<f64>,
    worst_offset_ms: Option<f64>,
    /// The worst-placed and the missing notes first.
    detail: Vec<NoteVsHit>,
    /// Hits with no note, their times.
    extra_hits: Vec<f64>,
}

/// A trigger whose loudest hit is under this (dBFS) is dead.
const DEAD_TRIGGER_DB: f64 = -45.0;

/// How far a written note and its hit may be, unless asked (seconds).
const NOTE_WINDOW: f64 = 0.06;

/// The drum MIDI's notes, each live piece's name, trigger and hits, and
/// the learned map.
type Reference = (
    Vec<crate::drum_reference::RefNote>,
    Vec<(String, String, Vec<f64>)>,
    Vec<crate::drum_reference::Mapped>,
);

/// The kit's pieces' hits and the learned map, for the drum MIDI `track`.
fn reference(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: Option<&str>,
) -> Result<Reference, String> {
    let midi = match track {
        Some(t) => resolve(daw, project, t)?,
        None => crate::drum_reference::find_track(daw, project)
            .ok_or("no drum MIDI track — name it with `track`")?,
    };
    let notes = crate::drum_reference::notes(daw, project, &midi);
    // A trigger whose loudest is under the floor is dead — crosstalk on
    // a piezo nobody hit — and has no hits to hold anything up to.
    let pieces: Vec<(String, String, Vec<f64>)> = crate::drum_reference::pieces(daw, project)
        .into_iter()
        .filter_map(|(name, trigger)| {
            let found = crate::fundamental::hits_of(daw, project, &trigger, 24.0);
            let loudest = found
                .iter()
                .map(|h| h.level_db)
                .fold(f64::NEG_INFINITY, f64::max);
            (loudest >= DEAD_TRIGGER_DB)
                .then(|| (name, trigger, found.into_iter().map(|h| h.time).collect()))
        })
        .collect();
    let named: Vec<(String, Vec<f64>)> = pieces
        .iter()
        .map(|(n, _, h)| (n.clone(), h.clone()))
        .collect();
    let map = crate::drum_reference::learn_map(&notes, &named, NOTE_WINDOW);
    Ok((notes, pieces, map))
}

fn register_reference(list: &ActionList) -> Result<(), architect::action::ActionError> {
    #[cfg(feature = "signal")]
    register_samples(list)?;
    #[cfg(feature = "signal")]
    register_samples_align(list)?;
    #[cfg(feature = "signal")]
    register_bass_rig(list)?;
    register_items(list)?;
    register_lanes(list)?;
    register_remove(list)?;
    #[cfg(feature = "native")]
    register_verbs(list)?;
    #[cfg(feature = "native")]
    register_save(list)?;
    #[cfg(feature = "native")]
    register_undo(list)?;
    register_style(list)?;
    register_drum_settings(list)?;
    register_peaks(list)?;
    register_reference_map(list)?;
    register_reference_notes(list)?;
    register_compare(list)?;
    register_restore_midi(list)?;
    register_copy_to_lane(list)?;
    register_review(list)?;
    register_level(list)?;
    Ok(())
}

static LEVEL: ActionMeta = meta(
    "DRUMS_LEVEL",
    "drums_level",
    "Level the drums",
    "Even each drum out before it is edited: every stroke (its hit marker's level) brought toward the drum's median — the kick most of the way, the snare a little less, the toms half — a few ms before its attack, held through its ring; ghosts, bleed and a flam's grace ride under the stroke before. Written as the take volume envelope of each drum's close mics on the comp (the same gain on every mic), replacing any earlier leveling: the envelope rides with the audio through every edit, so DRUMS_EDIT_LANES `reset` and the quantize after carry it. `pieces` (all when left out); `consistency` 0–100 % for every drum (100: every stroke brought to one level; each drum's own when left out — kick 75, snare 60, toms 50); `range_db`, the most a stroke moves (6 unless given; widen it with a high consistency); `whole_song`, toward the drum's level over the whole song rather than the playing around each stroke; `dry_run`. Says per drum the target, how many strokes went up and down, and the most.",
    "Drums",
    "",
);

/// What to level.
#[derive(Facet)]
struct LevelArg {
    #[facet(default)]
    pieces: Vec<String>,
    /// How consistent, percent: 100 every stroke one level.
    consistency: Option<f64>,
    /// The most a stroke moves, dB.
    range_db: Option<f64>,
    whole_song: Option<bool>,
    dry_run: Option<bool>,
}

fn register_level(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&LEVEL, Effect::Mutating, |a: LevelArg| {
        let (daw, project) = song()?;
        let knobs = crate::drum_level::Knobs {
            consistency: a.consistency.map(|c| (c / 100.0).clamp(0.0, 1.0)),
            range_db: a.range_db,
            whole_song: a.whole_song.unwrap_or(false),
        };
        crate::drum_level::level(
            daw,
            &project,
            (&a.pieces, &[]),
            (&knobs, a.dry_run.unwrap_or(false)),
        )
    })?;
    Ok(())
}

static REVIEW: ActionMeta = meta(
    "DRUMS_REVIEW",
    "drums_review",
    "Review the drum edit against the MIDI",
    "Every hit where it plays now, held up to the drum MIDI, section by section (`sections` by name, or `from`/`to`; the whole song when left out): each hit on its own piece's note (within 5 ms), on a note of another piece (a fill played with the pieces swapped), off its note (how far, ms, and on which line it is instead — a hit edited a sixteenth from where the chart has it), or unwritten (no note of the kit within a sixteenth: the drummer's own); and any gap an edited kit track's slices leave, where the drums cut out. Says the counts and lists the hits off their notes, worst first (`limit`, 40); `fills` true lists those in fill zones too. Read-only: what to check by ear, and where QUANTIZE and the chart disagree.",
    "Drums",
    "",
);

/// What to review.
#[derive(Facet)]
struct ReviewArg {
    #[facet(default)]
    sections: Vec<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: Option<usize>,
    fills: Option<bool>,
}

/// One hit off its note.
#[derive(Facet)]
struct OffNote {
    piece: String,
    /// Where it plays now, counted (`42.3a`), and its time.
    now: String,
    time: f64,
    /// Its note, counted.
    written: String,
    /// How far from its note it plays, ms (late positive).
    off_ms: f64,
    in_fill: bool,
}

/// A span reviewed.
#[derive(Facet)]
struct Reviewed {
    name: String,
    hits: u32,
    /// On their own piece's note.
    on_note: u32,
    /// On a note of another piece of the kit.
    on_other: u32,
    /// Near a note of theirs, off it.
    off_note: u32,
    /// No note of the kit near: the drummer's own.
    unwritten: u32,
    off: Vec<OffNote>,
    /// Where an edited kit track plays nothing between two slices: the
    /// drums cutting out (the first 20).
    gaps: Vec<String>,
}

/// How near its note a hit is on it, seconds.
const ON_NOTE_REVIEW: f64 = 0.005;

/// The times of `notes` the `map` gives piece `name` — or any piece of
/// the kit, `None` — by time.
fn piece_notes(
    notes: &[crate::drum_reference::RefNote],
    map: &[crate::drum_reference::Mapped],
    name: Option<&str>,
) -> Vec<f64> {
    let mut t: Vec<f64> = notes
        .iter()
        .filter(|n| {
            map.iter().any(|m| {
                m.pitch == n.pitch
                    && name.map_or_else(
                        || m.piece.is_some(),
                        |name| m.piece.as_deref() == Some(name),
                    )
            })
        })
        .map(|n| n.time)
        .collect();
    t.sort_by(f64::total_cmp);
    t
}

/// The time in `list` (by time) nearest `t`.
fn nearest_of(list: &[f64], t: f64) -> Option<f64> {
    let at = list.partition_point(|l| *l < t);
    [at.checked_sub(1), Some(at)]
        .into_iter()
        .flatten()
        .filter_map(|i| list.get(i).copied())
        .min_by(|x, y| (x - t).abs().total_cmp(&(y - t).abs()))
}

fn register_review(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&REVIEW, Effect::ReadOnly, |a: ReviewArg| {
        let (daw, project) = song()?;
        let kit = kit(daw, &project);
        if kit.is_empty() {
            return Err("no hit markers: find the hits first (HITS_DETECT)".to_owned());
        }
        let midi = crate::drum_reference::find_track(daw, &project).ok_or("no drum MIDI track")?;
        let notes = crate::drum_reference::notes(daw, &project, &midi);
        let named: Vec<(String, Vec<f64>)> = kit
            .iter()
            .map(|(n, _, h)| (n.clone(), h.iter().map(|h| h.time).collect()))
            .collect();
        let map = crate::drum_reference::learn_map(&notes, &named, NOTE_WINDOW);
        let of_piece = |name: &str| piece_notes(&notes, &map, Some(name));
        let all = piece_notes(&notes, &map, None);
        let nearest = nearest_of;
        let zones = crate::hits::zones(daw, &project);
        let limit = a.limit.unwrap_or(40);
        let counted_at = |t: f64| {
            let (bar, _) = in_bar(daw, &project, t + 1e-6);
            format!("{bar}.{}", counted(daw, &project, t))
        };
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        spans(daw, &project, &a.sections, (a.from, a.to))?
            .into_iter()
            .map(|(name, from, to)| {
                let (lo, hi) = (
                    from - edge(daw, &project, from),
                    to - edge(daw, &project, to),
                );
                let (mut on, mut other, mut unwritten, mut off) =
                    (0_usize, 0_usize, 0_usize, Vec::new());
                let mut hits = 0_usize;
                for (piece, _, played) in &kit {
                    let own = of_piece(piece);
                    for h in played
                        .iter()
                        .map(|h| h.time)
                        .filter(|t| *t >= lo && *t < hi)
                    {
                        hits = hits.saturating_add(1);
                        let step = sixteenth(daw, &project, h);
                        let mine = nearest(&own, h).filter(|n| (n - h).abs() <= step + 0.01);
                        let any = nearest(&all, h).filter(|n| (n - h).abs() <= ON_NOTE_REVIEW);
                        match (mine, any) {
                            (Some(n), _) if (n - h).abs() <= ON_NOTE_REVIEW => {
                                on = on.saturating_add(1_usize);
                            }
                            (_, Some(_)) => other = other.saturating_add(1_usize),
                            (Some(n), None) => off.push(OffNote {
                                piece: piece.clone(),
                                now: counted_at(h),
                                time: h,
                                written: counted_at(n),
                                off_ms: ((h - n) * 10_000.0).round() / 10.0,
                                in_fill: crate::hits::zone_holds(&zones, h, step * 4.0),
                            }),
                            (None, None) => unwritten = unwritten.saturating_add(1_usize),
                        }
                    }
                }
                let off_count = off.len();
                if !a.fills.unwrap_or(false) {
                    off.retain(|o| !o.in_fill);
                }
                off.sort_by(|x, y| y.off_ms.abs().total_cmp(&x.off_ms.abs()));
                off.truncate(limit);
                let gaps = crate::hits::gaps(daw, &project, (lo, hi))
                    .into_iter()
                    .take(20)
                    .map(|(track, a, b)| {
                        format!(
                            "{track} {}: {:.1} ms silent",
                            counted_at(a),
                            (b - a) * 1_000.0
                        )
                    })
                    .collect();
                Reviewed {
                    name,
                    gaps,
                    hits: count(hits),
                    on_note: count(on),
                    on_other: count(other),
                    off_note: count(off_count),
                    unwritten: count(unwritten),
                    off,
                }
            })
            .map(Ok)
            .collect::<Result<Vec<Reviewed>, String>>()
    })?;
    Ok(())
}

static COPY_TO_LANE: ActionMeta = meta(
    "TRACK_COPY_TO_LANE",
    "track_copy_to_lane",
    "Copy an item to a lane",
    "Copy the item `item` (its GUID) onto lane `lane` of its own track — where it is, or over `from`..`to` (seconds) playing the same recording there (its source run on or back to that span): to fill a hole in a comp from the recording either side of it. Says the copy's GUID. One undo step.",
    "Edit",
    "",
);

/// Which item, onto which lane.
#[derive(Facet)]
struct CopyToLaneArg {
    item: String,
    lane: u32,
    from: Option<f64>,
    to: Option<f64>,
}

fn register_copy_to_lane(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&COPY_TO_LANE, Effect::Mutating, |a: CopyToLaneArg| {
        let (daw, project) = song()?;
        let label = "Copy an item to a lane";
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        let copy = daw::service::Items::duplicate_item(
            daw,
            project.clone(),
            daw_proto::ItemRef::Guid(a.item.clone()),
        )
        .ok_or_else(|| format!("no item {}", a.item));
        let placed = copy.and_then(|copy| {
            let at = daw_proto::ItemRef::Guid(copy.clone());
            daw::service::Items::set_fixed_lane(daw, project.clone(), at.clone(), a.lane)
                .map_err(|e| e.to_string())?;
            if let (Some(from), Some(to)) = (a.from, a.to)
                && to > from
            {
                let item = daw::service::Items::get_item(daw, project.clone(), at.clone())
                    .ok_or("the copy went")?;
                let into = item.start_offset.as_seconds() + (from - item.position.as_seconds());
                if into < 0.0 {
                    return Err("the recording does not reach back that far".to_owned());
                }
                daw::service::Items::set_position(
                    daw,
                    project.clone(),
                    at.clone(),
                    daw_proto::PositionInSeconds::from_seconds(from),
                )
                .and_then(|()| {
                    daw::service::Items::set_length(
                        daw,
                        project.clone(),
                        at.clone(),
                        daw_proto::primitives::Duration::from_seconds(to - from),
                    )
                })
                .and_then(|()| {
                    daw_proto::Takes::set_start_offset(
                        daw,
                        project.clone(),
                        at.clone(),
                        daw_proto::TakeRef::Active,
                        daw_proto::primitives::Duration::from_seconds(into),
                    )
                })
                .map_err(|e| e.to_string())?;
            }
            Ok(copy)
        });
        daw::service::Projects::end_undo_block(daw, project, label, None);
        crate::song_model::request_resync();
        placed
    })?;
    Ok(())
}

static RESTORE_MIDI: ActionMeta = meta(
    "TRACK_RESTORE_MIDI",
    "track_restore_midi",
    "Restore a MIDI track",
    "Put a MIDI track's items back as a project file has them (`from`: the song's `.RPP`, or a `.session`): every item on its lane, at its place, offset and mute, with its notes, in place of the ones the song has now — for a part an edit damaged, like the drum MIDI a split cut short. One undo step. Says how many items and notes came back.",
    "Edit",
    "",
);

/// Which MIDI track, from which file.
#[derive(Facet)]
struct RestoreMidiArg {
    track: String,
    from: String,
}

fn register_restore_midi(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&RESTORE_MIDI, Effect::Mutating, |a: RestoreMidiArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let (items, notes) =
            crate::drum_reference::restore(daw, &project, &guid, std::path::Path::new(&a.from))?;
        Ok(format!("{items} items, {notes} notes restored"))
    })?;
    Ok(())
}

static EDIT_LANES: ActionMeta = meta(
    "DRUMS_EDIT_LANES",
    "drums_edit_lanes",
    "Comp and edit lanes",
    "Set the kit up to be edited, every drum track alike: its comping lane (the comp that plays) named COMP, and a new lane EDIT holding a copy of it, playing — so edits land on EDIT and COMP stays the comp they were made from (DRUMS_PLAY_LANE to hear either). The takes and their comp areas are untouched: EDIT is added after the last lane. A track already set up is left — unless `reset`, which starts the edit over: each EDIT lane's slices dropped and its COMP copied onto it again (one undo step). `dry_run` says what it would do.",
    "Drums",
    "",
);
static PLAY_LANE: ActionMeta = meta(
    "DRUMS_PLAY_LANE",
    "drums_play_lane",
    "Play a lane",
    "Play the kit's lane named `lane` (COMP, EDIT, or any lane's name) on every drum track at once — the edited drums against the comp they were made from.",
    "Drums",
    "",
);

/// Whether to only say.
#[derive(Facet)]
struct LanesArg {
    dry_run: Option<bool>,
    /// Start the edit over: each EDIT lane's slices dropped and its COMP
    /// copied onto it again.
    reset: Option<bool>,
}

/// What setting the lanes up did, track by track.
#[derive(Facet)]
struct LanesOut {
    /// `path: comp lane → EDIT lane (n items copied)`, or why it was left.
    tracks: Vec<String>,
    applied: bool,
}

/// A lane to play.
#[derive(Facet)]
struct PlayLaneArg {
    lane: String,
}

#[cfg(feature = "native")]
static VERBS: ActionMeta = meta(
    "DRUMS_VERBS",
    "drums_verbs",
    "Drum verbs, in time",
    "Set the kit's reverb returns (each piece's Verb folder: Kick/Verb/Room, Snare/Verb/Plate, Hall and Nonlin, Toms/Verb/Room and Nonlin) as a drum mix wants them — every tail a time the reverb takes itself (seconds, or a note value it reads against the song's tempo), every pre-delay too, low and high cuts on the snare's and toms', and each fader far down (kick −23 dB, snare −20, toms −26: felt more than heard). Their FTS Reverb is added if missing, set if there. Prepare does the same. `dry_run`.",
    "Drums",
    "",
);

/// Whether to only say.
#[cfg(feature = "native")]
#[derive(Facet)]
struct VerbsArg {
    dry_run: Option<bool>,
}

#[cfg(feature = "native")]
fn register_verbs(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&VERBS, Effect::Mutating, |a: VerbsArg| {
        let (daw, project) = song()?;
        let dry = a.dry_run.unwrap_or(false);
        let said =
            crate::mix_chains::drum_returns(daw, &project, dry).map_err(|e| e.to_string())?;
        if !dry {
            crate::song_model::request_resync();
            crate::actions::list().changed("drums.verbs", "the drum verbs were set");
        }
        Ok(said)
    })?;
    list.register(&GROUP, Effect::Mutating, |a: VerbsArg| {
        let (daw, project) = song()?;
        let dry = a.dry_run.unwrap_or(false);
        let grouped = crate::drum_group::group(daw, &project, dry)?;
        if !dry {
            crate::song_model::request_resync();
            crate::actions::list().changed("drums.group", "the kit was grouped");
        }
        Ok(grouped)
    })?;
    list.register(&TRIGGERS, Effect::Mutating, |_: NoArg| {
        let (daw, project) = song()?;
        let mut target = dynamic_template::apply::DawTarget::on(daw.clone(), project);
        let muted = target.mute_triggers().map_err(|e| e.to_string())?;
        crate::song_model::request_resync();
        crate::actions::list().changed("drums.triggers", "the triggers were muted");
        Ok(muted)
    })?;
    Ok(())
}

#[cfg(feature = "native")]
static GROUP: ActionMeta = meta(
    "DRUMS_GROUP",
    "drums_group",
    "Group the kit",
    "Put every recorded drum track (each mic and trigger under the top-level Drums folder; not a Sample track, not the Process or a Verb folder) in one track group, `Drums`, for record arm and media edit only: armed together, so the kit is never recorded part by part while grouping is on (the toolbar's Grouping); edited together, so a slip, cut or comp on one track is on all. Faders, mutes and solos stay each track's own. Returns the tracks grouped; `dry_run` names them and changes nothing.",
    "Drums",
    "",
);

#[cfg(feature = "native")]
static TRIGGERS: ActionMeta = meta(
    "DRUMS_TRIGGERS",
    "drums_triggers",
    "Mute the triggers",
    "Mute every drum trigger track (`Trig`, `T1 Trig`, …) — its clicks are never heard — and make each of its sends pre-fader, so the gates it keys and whatever it feeds still hear it. Prepare does the same. Returns the tracks it muted (none when all already were).",
    "Drums",
    "",
);

/// No arguments.
#[cfg(feature = "native")]
#[derive(Facet)]
struct NoArg {}

#[cfg(feature = "native")]
pub static SAVE: ActionMeta = meta(
    "SESSION_SAVE",
    "session_save",
    "Save the song",
    "Save the song as it is now — its edits, its tracks, every FX with what it holds (a sampler's kit, a rig's capture, a reverb's time) — into its project file: `Song.session` beside the `.RPP` it was opened from (what opens next time), or the alternative it is. What changed since the last save goes in as one step of the file's history. `to` saves to another file instead and leaves the song's own alone. Says where.",
    "Session",
    "",
);

/// Where to save, if not the song's own `.session`. Only `to`: a
/// misnamed argument (`dir`) would otherwise save over the song's own.
#[cfg(feature = "native")]
#[derive(Facet)]
#[facet(deny_unknown_fields)]
struct SaveArg {
    to: Option<String>,
}

#[cfg(feature = "native")]
static CHECK_SAVED: ActionMeta = meta(
    "SESSION_CHECK_SAVED",
    "session_check_saved",
    "Check a saved song",
    "Reopen a saved project file (`dir`: a `Song.session`, or an alternative `Song.session/Organized`) as a second song, as a reopen would, and set it beside the song open now as the project doc reads both: every track with its FX and what they hold, sends, automation, lanes and groups; every item with its take, stretch markers and notes; markers, regions and tempo. Says `the same` or what differs. The second song is closed again.",
    "Session",
    "",
);

/// The folder to check.
#[cfg(feature = "native")]
#[derive(Facet)]
struct CheckSavedArg {
    dir: String,
}

#[cfg(feature = "native")]
pub static UNDO: ActionMeta = meta(
    "SESSION_UNDO",
    "session_undo",
    "Undo",
    "Take back the last step — a move, a cut, a slip edit, whatever one gesture or action did, as one. Says what it undid, or that there is nothing to.",
    "Session",
    "",
);

#[cfg(feature = "native")]
pub static REDO: ActionMeta = meta(
    "SESSION_REDO",
    "session_redo",
    "Redo",
    "Do again the last step undone. Says what it redid, or that there is nothing to.",
    "Session",
    "",
);

#[cfg(feature = "native")]
static MARK_KINDS: ActionMeta = meta(
    "SESSION_MARK_KINDS",
    "session_mark_kinds",
    "Mark the kit's kinds",
    "Write on every track of the kit what the golden template says it is — piece, Sum, mic, trigger, sub, sample, reverb — matched by name, for a song prepared before kinds were kept on the track. The scenes (Edit mode's rows above all) read them there. One undo step. Says how many tracks were marked.",
    "Session",
    "",
);

static CHECKPOINT: ActionMeta = meta(
    "SESSION_CHECKPOINT",
    "session_checkpoint",
    "Save a checkpoint",
    "Save the song and fork a named alternative from it as it is now (`name`: \"Organized\", \"Mix A\") — a branch of the song's own project file (`Song.session`), sharing everything it had and keeping only what is done to it after, the way Logic keeps alternatives. The song stays open, under its own name. Open one later from the start screen's Recent or SESSION_OPEN.",
    "Session",
    "",
);

/// The checkpoint's name.
#[cfg(feature = "native")]
#[derive(Facet)]
struct CheckpointArg {
    name: String,
}

#[cfg(feature = "native")]
static ALTERNATIVES: ActionMeta = meta(
    "SESSION_ALTERNATIVES",
    "session_alternatives",
    "The song's alternatives",
    "The open song's own version and its alternatives (checkpoints saved with SESSION_CHECKPOINT), the newest first: name, folder, when saved. Read-only.",
    "Session",
    "",
);

#[cfg(feature = "native")]
static STREAMS: ActionMeta = meta(
    "SESSION_STREAMS",
    "session_streams",
    "What is streaming",
    "Every song streamed in from elsewhere (a Task library, a share link, another engine) and how far along it is: `state` local (every recording it plays is here, kept on disk), arriving, or streaming (nothing yet); its recordings, each with its bytes and how many have arrived, the least arrived first; where they are kept. `song` (the engine project) narrows it to one; `top` caps the recordings listed (all when left out). Read-only.",
    "Session",
    "",
);

#[cfg(feature = "native")]
static STREAM_QUALITY: ActionMeta = meta(
    "SESSION_STREAM_QUALITY",
    "session_stream_quality",
    "Streaming quality",
    "What streamed songs are heard from, on this device: `proxies` (the compressed proxies only; an original is never fetched) or `full` (the proxies first, then — once every song's proxies are here — the original recordings, fetched one at a time behind everything else, each take moving to its original the next time its song is stopped). Left out, says what it is. Kept for every set.",
    "Session",
    "",
);

/// The quality to stream at, or nothing to ask what it is.
#[cfg(feature = "native")]
#[derive(Facet)]
struct StreamQualityArg {
    quality: Option<String>,
}

/// Which song, and how many of its recordings to list.
#[cfg(feature = "native")]
#[derive(Facet)]
struct StreamsArg {
    song: Option<String>,
    top: Option<u32>,
}

#[cfg(feature = "native")]
static RECENT: ActionMeta = meta(
    "SESSION_RECENT",
    "session_recent",
    "Songs opened lately",
    "The songs and setlists opened lately, the newest first: name, the version that was open (the song, or which alternative), when, and the song's alternatives. Read-only.",
    "Session",
    "",
);

/// Whether the open song has changes it was not saved with; `None` when
/// that is not known.
#[cfg(feature = "native")]
#[must_use]
pub(crate) fn open_song_unsaved() -> Option<bool> {
    let (daw, ProjectContext::Project(guid)) = song().ok()? else {
        return None;
    };
    crate::projects::unsaved(daw, &guid)
}

/// The open song's versions, for the top bar's Versions.
///
/// The song's own (`"The song"`) and its checkpoints, the newest first —
/// each its name, when it was saved, where it is, and whether it is the
/// one open.
#[cfg(feature = "native")]
#[must_use]
pub(crate) fn open_song_versions() -> Vec<crate::projects::Version> {
    let Ok((daw, ProjectContext::Project(guid))) = song() else {
        return Vec::new();
    };
    let Some(open) = daw.read_project(&guid, |p| std::path::PathBuf::from(&p.info.path)) else {
        return Vec::new();
    };
    let Ok(base) = song_base(daw, &guid) else {
        return Vec::new();
    };
    let same = |a: &std::path::Path, b: &std::path::Path| {
        std::path::absolute(a).ok() == std::path::absolute(b).ok()
    };
    let mut out: Vec<crate::projects::Version> = crate::projects::main_of(&base)
        .map(|path| crate::projects::Version {
            name: "The song".to_owned(),
            when: String::new(),
            open: crate::projects::alternative_name(&open).is_none(),
            path,
        })
        .into_iter()
        .collect();
    out.extend(crate::projects::alternatives(&base).into_iter().map(|alt| {
        crate::projects::Version {
            open: same(&alt.path, &open),
            name: alt.name,
            when: crate::projects::ago(alt.saved),
            path: alt.path,
        }
    }));
    out
}

/// The open song's base ([`crate::projects::base_of`]): what its
/// alternatives are kept beside.
#[cfg(feature = "native")]
fn song_base(daw: &daw::standalone::Standalone, guid: &str) -> Result<std::path::PathBuf, String> {
    let path = daw
        .read_project(guid, |p| p.info.path.clone())
        .ok_or("no song open")?;
    crate::projects::base_of(std::path::Path::new(&path))
        .ok_or_else(|| format!("{path} is not a song's file: nowhere to keep alternatives"))
}

#[cfg(feature = "native")]

static JOBS: ActionMeta = meta(
    "SESSION_JOBS",
    "session_jobs",
    "Work in the background",
    "What is being done off the window's thread right now — reading waveforms, slicing them, opening or saving a project file — each with how far along (`done` of `total`; a total of 0 is work that does not know its length). What the bottom bar shows. Empty when nothing is running. Read-only.",
    "View",
    "",
);

/// One job, as listed.
#[derive(Facet)]
struct JobRow {
    label: String,
    done: u64,
    total: u64,
}

static ITEM_STYLE: ActionMeta = meta(
    "SESSION_ITEM_STYLE",
    "session_item_style",
    "Item style",
    "How items are drawn: `outlined: true` (the default) a faint body in the item's colour with the waveform standing out over it in the full colour and a line round each item; `false` the half-strength block of colour the waveform is read against.",
    "View",
    "",
);

/// Outlined or not.
#[derive(Facet)]
struct ItemStyleArg {
    outlined: bool,
}

static STOP_BEHAVIOUR: ActionMeta = meta(
    "SESSION_STOP_BEHAVIOUR",
    "session_stop_behaviour",
    "Stop behaviour",
    "What Stop does with the cursor: `behaviour: \"return\"` (the default) puts it back where play started, `\"stay\"` leaves it where it stopped. Any other value, or an empty one, only reads the setting.",
    "Transport",
    "",
);

/// The stop behaviour to set, or nothing to read it.
#[derive(Facet)]
struct StopBehaviourArg {
    behaviour: String,
}

fn register_style(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&STOP_BEHAVIOUR, Effect::Mutating, |a: StopBehaviourArg| {
        if let Some(stop) = crate::play_return::Stop::from_slug(&a.behaviour) {
            crate::play_return::set(stop);
        }
        Ok(crate::play_return::get().slug())
    })?;
    list.register(&JOBS, Effect::ReadOnly, |(): ()| {
        Ok(crate::jobs::running()
            .into_iter()
            .map(
                |crate::jobs::Running {
                     label, done, total, ..
                 }| JobRow {
                    label,
                    done: u64::try_from(done).unwrap_or(u64::MAX),
                    total: u64::try_from(total).unwrap_or(u64::MAX),
                },
            )
            .collect::<Vec<_>>())
    })?;
    list.register(&ITEM_STYLE, Effect::Mutating, |a: ItemStyleArg| {
        crate::options::OUTLINED.set(a.outlined);
        // The lanes are recorded with their bodies: read back, drawn anew.
        crate::song_model::request_resync();
        Ok(if a.outlined { "outlined" } else { "filled" })
    })?;
    Ok(())
}

static MULTITRACKS: ActionMeta = meta(
    "SESSION_MULTITRACKS",
    "session_multitracks",
    "Load the multitracks",
    "Hear every streamed song by its stems: they all come in now, and each song moves over from its stereo reference as they arrive. Says how the songs are heard (`stems`, and `referenced` while any still plays its reference).",
    "Streaming",
    "",
);

static GROUPING: ActionMeta = meta(
    "SESSION_GROUPING",
    "session_grouping",
    "Grouping",
    "Whether a fader, mute or solo moved on a grouped track moves its group (`on`). Says what it is now.",
    "Mix",
    "",
);

/// Grouping on or off.
#[cfg(feature = "native")]
#[derive(Facet)]
struct GroupingArg {
    on: bool,
}

static VERSIONS: ActionMeta = meta(
    "SESSION_VERSIONS",
    "session_versions",
    "The song's versions",
    "The open song's versions, the newest first: the song's own (`\"The song\"`) and its checkpoints, each its `name`, when it was saved (`when`), where it is (`path`) and whether it is the one open (`open`); and whether the song has changes not saved (`unsaved`, absent with no song or no file). Read-only.",
    "File",
    "",
);

/// The open song's versions, as the top bar's Versions lists them.
#[cfg(feature = "native")]
#[derive(Facet)]
pub struct VersionsRead {
    pub versions: Vec<VersionRow>,
    pub unsaved: Option<bool>,
}

/// One version.
#[cfg(feature = "native")]
#[derive(Facet)]
pub struct VersionRow {
    pub name: String,
    pub when: String,
    pub path: String,
    pub open: bool,
}

static ACTIONS: ActionMeta = meta(
    "SESSION_ACTIONS",
    "session_actions",
    "Actions a palette runs",
    "The actions that take nothing and run at once — what a command palette offers — each its `id`, `label`, `group`, `shortcut`, `description` and whether it can run now (`enabled`). Read-only.",
    "View",
    "",
);

/// An action a palette runs.
#[cfg(feature = "native")]
#[derive(Facet)]
pub struct PaletteRow {
    pub id: String,
    pub label: String,
    pub group: String,
    pub shortcut: String,
    pub description: String,
    pub enabled: bool,
}

/// The actions a palette offers, as the engine lists them
/// (`SESSION_ACTIONS`): none when it cannot say.
#[cfg(feature = "native")]
#[must_use]
pub fn palette() -> Vec<PaletteRow> {
    run(ACTIONS.id, b"")
        .ok()
        .and_then(|bytes| facet_json::from_slice::<Vec<PaletteRow>>(&bytes).ok())
        .unwrap_or_default()
}

/// The open song's versions, as the engine reads them
/// (`SESSION_VERSIONS`): `None` when it cannot say.
#[cfg(feature = "native")]
#[must_use]
pub fn versions() -> Option<VersionsRead> {
    run(VERSIONS.id, b"")
        .ok()
        .and_then(|bytes| facet_json::from_slice::<VersionsRead>(&bytes).ok())
}

/// What a window's controls ask of the engine: the multitracks, grouping,
/// the song's versions and the palette's actions — so a window in another
/// process (and MCP, and the CLI) reads and sets them where they live.
#[cfg(feature = "native")]
fn register_window_controls(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&MULTITRACKS, Effect::Mutating, |(): ()| {
        crate::reference_play::load_multitracks();
        Ok::<_, String>(crate::view::Listening {
            stems: crate::reference_play::multitracks(),
            referenced: crate::reference_play::any(),
        })
    })?;
    list.register(&GROUPING, Effect::Mutating, |a: GroupingArg| {
        crate::options::GROUPING.set(a.on);
        crate::options::grouping_changed(a.on);
        Ok::<_, String>(a.on)
    })?;
    list.register(&VERSIONS, Effect::ReadOnly, |(): ()| {
        Ok::<_, String>(VersionsRead {
            versions: open_song_versions()
                .into_iter()
                .map(|v| VersionRow {
                    name: v.name,
                    when: v.when,
                    path: v.path.to_string_lossy().into_owned(),
                    open: v.open,
                })
                .collect(),
            unsaved: open_song_unsaved(),
        })
    })?;
    list.register(&ACTIONS, Effect::ReadOnly, |(): ()| {
        let all = crate::actions::list();
        Ok::<_, String>(
            all.infos()
                .into_iter()
                .filter(|a| a.params_schema.is_none() && !all.is_async(&a.id))
                .map(|a| PaletteRow {
                    label: if a.display_name.is_empty() {
                        a.id.clone()
                    } else {
                        a.display_name
                    },
                    id: a.id,
                    group: if a.group.is_empty() {
                        a.category
                    } else {
                        a.group
                    },
                    shortcut: a.shortcut,
                    description: a.description,
                    enabled: a.enabled,
                })
                .collect::<Vec<_>>(),
        )
    })?;
    Ok(())
}

#[cfg(feature = "native")]
fn register_undo(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&FOLD, Effect::Mutating, |a: FoldArg| {
        // Told by the engine which folders it changed: set here as settled.
        if !a.folders.is_empty() {
            for folder in &a.folders {
                crate::folds::toggle(folder, !a.open);
            }
            return Ok(a.folders);
        }
        let (daw, project) = song()?;
        let tracks = daw::service::Tracks::all(daw, project);
        let guid = crate::shared_actions::resolve_in(&tracks, &a.track)?;
        let changed = crate::shared_actions::folds_of(&tracks, &guid, a.open);
        for (folder, _) in &changed {
            crate::folds::toggle(folder, !a.open);
        }
        let (folders, opened): (Vec<String>, Vec<String>) = changed.into_iter().unzip();
        // The folds are the window's view: run here, the window is asked
        // too, told the folders settled so the two agree.
        also_in_the_window(
            FOLD.id,
            serde_json::json!({ "track": guid, "open": a.open, "folders": folders }).to_string(),
        );
        Ok(opened)
    })?;
    list.register(&UNDO, Effect::Mutating, |_: NoArg| {
        let (daw, project) = song()?;
        // A song open from its file undoes through its doc: only this
        // peer's steps, a collaborator's edits left standing.
        if let Some(said) = undo_through_doc(daw, &project, true)? {
            return Ok(said);
        }
        let label = daw::service::Projects::last_undo_label(daw, project.clone());
        if !daw::service::Projects::undo(daw, project.clone()) {
            stepped();
            return Ok("nothing to undo".to_owned());
        }
        stepped();
        samples_follow(daw, &project, crate::hits_midi::Mode::Follow);
        crate::changes::announce(crate::changes::Change::Marks);
        crate::song_model::request_resync();
        crate::actions::list().changed("items", "undo");
        Ok(format!(
            "undid {}",
            label.unwrap_or_else(|| "a step".to_owned())
        ))
    })?;
    list.register(&REDO, Effect::Mutating, |_: NoArg| {
        let (daw, project) = song()?;
        if let Some(said) = undo_through_doc(daw, &project, false)? {
            return Ok(said);
        }
        let label = daw::service::Projects::last_redo_label(daw, project.clone());
        if !daw::service::Projects::redo(daw, project.clone()) {
            stepped();
            return Ok("nothing to redo".to_owned());
        }
        stepped();
        samples_follow(daw, &project, crate::hits_midi::Mode::Follow);
        crate::changes::announce(crate::changes::Change::Marks);
        crate::song_model::request_resync();
        crate::actions::list().changed("items", "redo");
        Ok(format!(
            "redid {}",
            label.unwrap_or_else(|| "a step".to_owned())
        ))
    })?;
    Ok(())
}

/// The kit's hits as MIDI (and so its samples) made to agree with the
/// hits after an edit step (`Reconcile`: a view edited in that step is
/// carried over to the hits), an undo or a redo (`Follow`: the views
/// written from the hits put back).
#[cfg(feature = "native")]
pub fn samples_follow(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
    mode: crate::hits_midi::Mode,
) {
    match crate::hits_midi::sync(daw, project, mode, &|t| t) {
        Ok(synced) => {
            if let Some(applied) = synced.applied {
                tracing::info!(
                    hits_midi.removed = applied.removed,
                    hits_midi.moved = applied.moved,
                    hits_midi.velocities = applied.velocities,
                    hits_midi.added = applied.added,
                    hits_midi.unplaced = applied.unplaced,
                    hits_midi.written = synced.written,
                    "drums: hits edited in the MIDI"
                );
                crate::hit_overlay::refresh();
                crate::song_model::request_resync_items();
            }
        }
        Err(e) => tracing::warn!(hits_midi.error = %e, "drums: the hits' MIDI could not follow"),
    }
    // An undo or redo may take back what the overlay draws besides hits —
    // a take's rank, a kept stretch: drawn again from the song.
    if matches!(mode, crate::hits_midi::Mode::Follow) {
        crate::hit_overlay::refresh();
    }
}

/// Session undos and redos run, ever (one that found nothing counts): what
/// a view holding a copy of the song (the Editor's take) watches to read
/// it again.
static UNDONE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many Session undos and redos have been done.
#[must_use]
pub fn undone() -> u64 {
    UNDONE.load(std::sync::atomic::Ordering::Acquire)
}

fn stepped() {
    UNDONE.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    // A window in another process counts it too.
    #[cfg(feature = "native")]
    crate::studio_engine::view_host::changed(None, crate::view::Change::Undone);
}

/// The engine said it undid or redid a step: counted here, as one done
/// here is ([`undone`]).
pub(crate) fn undone_there() {
    UNDONE.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

/// Run `SESSION_UNDO` (`back`) or `SESSION_REDO` behind the window, as
/// the keys do: for a view whose own Undo is Session's.
#[cfg(feature = "native")]
pub fn undo_behind(back: bool) {
    off_the_window(if back { UNDO.id } else { REDO.id }, String::new());
}

/// Undo (`back`) or redo through the song's doc, when it is open from its
/// project file: what to say, or `None` for the engine's own undo.
#[cfg(feature = "native")]
fn undo_through_doc(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
    back: bool,
) -> Result<Option<String>, String> {
    use crate::project_file::Undone;
    let daw::service::ProjectContext::Project(guid) = project else {
        return Ok(None);
    };
    let done = if back {
        crate::project_file::undo(daw, guid)?
    } else {
        crate::project_file::redo(daw, guid)?
    };
    let (verb, none) = if back {
        ("undid", "nothing to undo")
    } else {
        ("redid", "nothing to redo")
    };
    Ok(match done {
        Undone::NotOpen => None,
        Undone::Nothing => {
            stepped();
            Some(none.to_owned())
        }
        Undone::Step(name) => {
            stepped();
            samples_follow(daw, project, crate::hits_midi::Mode::Follow);
            crate::changes::announce(crate::changes::Change::Marks);
            crate::song_model::request_resync();
            crate::actions::list().changed("items", if back { "undo" } else { "redo" });
            Some(format!("{verb} {name}"))
        }
    })
}

#[cfg(feature = "native")]
fn register_save(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SELECT_TRACK, Effect::Mutating, |a: SelectTrackArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let at = daw::service::TrackRef::Guid(guid.clone());
        if a.add {
            daw::service::Tracks::set_selected(daw, project, at, true)
        } else {
            daw::service::Tracks::select_exclusive(daw, project, at)
        }
        .map_err(|e| e.to_string())?;
        crate::actions::list().changed("tracks", &guid);
        Ok(guid)
    })?;
    list.register(&MARK_KINDS, Effect::Mutating, |_: NoArg| {
        let (daw, project) = song()?;
        daw::service::Projects::begin_undo_block(daw, project.clone(), "Mark the kit's kinds");
        let marked =
            dynamic_template::apply::DawTarget::on(daw.clone(), project.clone()).mark_kit_kinds();
        daw::service::Projects::end_undo_block(daw, project, "Mark the kit's kinds", None);
        let marked = marked.map_err(|e| e.to_string())?;
        crate::song_model::request_resync();
        Ok(format!("{marked} tracks marked"))
    })?;
    list.register(&CHECKPOINT, Effect::Mutating, |a: CheckpointArg| {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        let from = std::path::PathBuf::from(
            daw.read_project(guid, |p| p.info.path.clone())
                .ok_or("no song open")?,
        );
        let file = crate::project_file::fork(daw, guid, &from, &a.name)?;
        // The song was saved on the way: nothing unsaved now.
        crate::projects::mark_saved(daw, guid);
        Ok(format!(
            "saved the song and forked {} from it in {}; the song stays open",
            a.name.trim(),
            file.display()
        ))
    })?;
    list.register(&STREAMS, Effect::ReadOnly, |a: StreamsArg| {
        let mut songs = crate::stream_set::report();
        for s in &mut songs {
            s.readiness = crate::stream_set::readiness(&s.project);
        }
        if let Some(song) = &a.song {
            songs.retain(|s| &s.project == song);
        }
        if let Some(top) = a.top {
            for s in &mut songs {
                s.recordings.truncate(top as usize);
            }
        }
        Ok(songs)
    })?;
    list.register(&STREAM_QUALITY, Effect::Mutating, |a: StreamQualityArg| {
        use crate::stream_quality::{Quality, current, set};
        if let Some(word) = a.quality.as_deref() {
            let quality = Quality::parse(word)
                .ok_or_else(|| format!("`quality`: `proxies` or `full`, not {word:?}"))?;
            set(quality);
        }
        Ok(current().key().to_owned())
    })?;
    list.register(&ALTERNATIVES, Effect::ReadOnly, |(): ()| {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        let base = song_base(daw, guid)?;
        let unsaved = match crate::projects::unsaved(daw, guid) {
            Some(true) => " (unsaved changes)",
            Some(false) => " (saved)",
            None => "",
        };
        let mut out = vec![format!(
            "the song{unsaved}: {}",
            crate::projects::main_of(&base)
                .map_or_else(|| "(not saved yet)".to_owned(), |p| p.display().to_string())
        )];
        out.extend(crate::projects::alternatives(&base).into_iter().map(|alt| {
            format!(
                "{}: {} (saved {})",
                alt.name,
                alt.path.display(),
                crate::projects::ago(alt.saved)
            )
        }));
        Ok(out)
    })?;
    list.register(&RECENT, Effect::ReadOnly, |(): ()| {
        Ok(crate::projects::recent()
            .into_iter()
            .map(|r| {
                let version = r
                    .opened_alternative()
                    .map_or_else(String::new, |alt| format!(" as {alt}"));
                let alternatives: Vec<String> = crate::projects::alternatives(&r.base)
                    .into_iter()
                    .map(|a| a.name)
                    .collect();
                format!(
                    "{}{version}, {} — {}{}",
                    r.name(),
                    crate::projects::ago(r.at),
                    r.opened.display(),
                    if alternatives.is_empty() {
                        String::new()
                    } else {
                        format!(" (alternatives: {})", alternatives.join(", "))
                    }
                )
            })
            .collect::<Vec<_>>())
    })?;
    list.register(&SAVE, Effect::Mutating, |a: SaveArg| {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        // A new song has no file yet: its first save names it.
        let untitled = crate::project_new::untitled(daw, guid);
        let dir = if let Some(to) = a.to.clone() {
            std::path::PathBuf::from(to)
        } else if untitled {
            return Err(
                "the song is untitled: save it with `to`, the file it goes in — its name becomes the song's"
                    .to_owned(),
            );
        } else {
            let from = std::path::PathBuf::from(
                daw.read_project(guid, |p| p.info.path.clone())
                    .ok_or("no song open")?,
            );
            if from.extension().is_some_and(|e| e == "session") {
                from
            } else {
                from.with_extension("session")
            }
        };
        let own = a.to.is_none() || untitled;
        let at = crate::session_file::save_session(daw, guid, &dir)?;
        if untitled {
            crate::project_new::named(daw, guid, &dir, &at);
        }
        if own {
            // The song itself, saved: nothing unsaved now. (A copy saved
            // elsewhere leaves the song as unsaved as it was.)
            crate::projects::mark_saved(daw, guid);
        }
        Ok(format!("saved to {}", at.display()))
    })?;
    list.register(&CHECK_SAVED, Effect::ReadOnly, |a: CheckSavedArg| {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        let apart = crate::project_file::check(daw, guid, std::path::Path::new(&a.dir))?;
        Ok(if apart.is_empty() {
            vec!["the same".to_owned()]
        } else {
            apart
        })
    })?;
    Ok(())
}

static REMOVE: ActionMeta = meta(
    "DRUMS_REMOVE",
    "drums_remove",
    "Remove from the drums",
    "Take `from`..`to` (seconds) out of the kit's edited takes (its EDIT lane, once it has one): cut at both ends, the stretch between deleted, a 2 ms fade on each edge. `tracks` (paths or GUIDs) names which — a Trig, to take a stray or doubled trigger spike out so it fires no gate and no sample; left out, the whole kit, for a hit that should not be there at all. One undo step. Look first: AUDIO_PEAKS on the Trig and its mics, and the drum MIDI (DRUMS_REFERENCE).",
    "Drums",
    "",
);

/// What to take out.
#[derive(Facet)]
struct RemoveArg {
    from: f64,
    to: f64,
    tracks: Option<Vec<String>>,
}

fn register_remove(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&REMOVE, Effect::Destructive, |a: RemoveArg| {
        if a.to <= a.from {
            return Err("`to` must be after `from`".to_owned());
        }
        if a.to - a.from > 2.0 {
            return Err("more than two seconds: this takes out a hit, not a passage".to_owned());
        }
        let (daw, project) = song()?;
        let tracks = a
            .tracks
            .iter()
            .flatten()
            .map(|t| resolve(daw, &project, t))
            .collect::<Result<Vec<String>, String>>()?;
        let us = |s: f64| num_traits::ToPrimitive::to_i64(&(s * 1_000_000.0).round()).unwrap_or(0);
        crate::window_queue::ask(crate::window_queue::Asked::RemoveRange {
            tracks,
            from_us: us(a.from),
            to_us: us(a.to),
        });
        Ok(())
    })?;
    Ok(())
}

#[cfg(feature = "signal")]
static SAMPLES: ActionMeta = meta(
    "DRUMS_SAMPLES",
    "drums_samples",
    "Drum samples under the kit",
    "Blend signal's sampled kit under the recorded drums, from the hits the triggers caught (the drum-edit detector, on the EDIT lane once there is one): each piece's hits written as notes on its Sample track (the kit's GM note, velocity from the hit's level) with an FTS Kit Mic there playing that piece's mics — its close mics on the track, its overheads and rooms on 3/4 and 5/6, sent into the recorded overheads and rooms. A piece's voices get the same notes, so each hit is one sample on all of them. Each Sample track's polarity is then set to pull with its close mic (DRUMS_SAMPLES_ALIGN). One undo step. `preset` (a `.signalpreset`; GGD Modern and Massive 2's Heavy Thwacky unless given), `dry_run`.",
    "Drums",
    "",
);

/// Which kit.
#[cfg(feature = "signal")]
#[derive(Facet)]
struct SamplesArg {
    preset: Option<String>,
    dry_run: Option<bool>,
}

/// What it did.
#[cfg(feature = "signal")]
#[derive(Facet)]
struct SamplesOut {
    tracks: Vec<String>,
    notes: u32,
    applied: bool,
}

/// The kit samples blend in unless told otherwise.
#[cfg(feature = "signal")]
const DEFAULT_KIT: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Presets/Heavy Thwacky.signalpreset";

#[cfg(feature = "signal")]
static BASS_RIG: ActionMeta = meta(
    "BASS_RIG",
    "bass_rig",
    "Bass rig on the bass",
    "signal's bass rig (FTS Bass Rig) on each track under Bass, first in its chain: a signal bass `profile` (`Metal` unless given; from Session's own copy of signal's bass library, never the live rig's) with every scene loaded — Clean, Crunch, Drive … — starting on the scene its name holds, else the profile's default. Its Scene parameter switches them and is automated like any other (`FTS Bass Rig/Scene`, by name: `\"Crunch\"`). `patches`: true gives the older fixed patches instead (Clean: the DP3X fed low; Harmonics: the DP3X as captured; Crunch: an HM2 into it). Reran, the rig there takes it again. `dry_run`.",
    "Bass",
    "",
);

/// Whether to only say.
#[cfg(feature = "signal")]
#[derive(Facet)]
struct BassRigArg {
    profile: Option<String>,
    patches: Option<bool>,
    dry_run: Option<bool>,
}

#[cfg(feature = "signal")]
fn register_bass_rig(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&BASS_RIG, Effect::Mutating, |a: BassRigArg| {
        let (daw, project) = song()?;
        let setup = if a.patches.unwrap_or(false) {
            crate::bass_rig::Setup::Patches
        } else {
            crate::bass_rig::Setup::Profile(a.profile.unwrap_or_else(|| "Metal".to_owned()))
        };
        crate::bass_rig::set_up(daw, &project, &setup, a.dry_run.unwrap_or(false))
    })?;
    Ok(())
}

#[cfg(feature = "signal")]
static SAMPLES_ALIGN: ActionMeta = meta(
    "DRUMS_SAMPLES_ALIGN",
    "drums_samples_align",
    "Line the drum samples up with the close mics",
    "Measure each piece's sample against its close mic and set its Sample track's polarity to pull with the mic as the mix plays it. The kit's sample for the piece is struck once, as its Kit Mic plays it, and measured against the piece's clean hits on its first close mic (the recording the song plays most of). Per piece: `delay_ms`, how much later than its note the sample must start for its attack to land on the recorded one (negative: earlier; reported, not applied — the samples start at their attack); `inverted`, whether it pulls the opposite way to the recording as recorded; `agreement` 0..1, how alike the lined-up attacks are; `consistent` 0..1, the share of hits that alone say the same; `flip`, what the Sample track's polarity is set to (none: too unsure, left as it is). One undo step. `preset` as DRUMS_SAMPLES, `dry_run`.",
    "Drums",
    "",
);

/// One piece's measure.
#[cfg(feature = "signal")]
#[derive(Facet)]
struct AlignedPiece {
    piece: String,
    sample: String,
    delay_ms: Option<f64>,
    inverted: Option<bool>,
    agreement: Option<f64>,
    consistent: Option<f64>,
    hits: u32,
    flip: Option<bool>,
}

/// What lining the samples up found, and did.
#[cfg(feature = "signal")]
#[derive(Facet)]
struct AlignOut {
    pieces: Vec<AlignedPiece>,
    tracks: Vec<String>,
    applied: bool,
}

#[cfg(feature = "signal")]
fn register_samples_align(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SAMPLES_ALIGN, Effect::Mutating, |a: SamplesArg| {
        let (daw, project) = song()?;
        let preset = std::path::PathBuf::from(a.preset.as_deref().unwrap_or(DEFAULT_KIT));
        if !preset.exists() {
            return Err(format!("no kit preset at {}", preset.display()));
        }
        let measured = crate::drum_samples::measure(daw, &project, &preset);
        let dry = a.dry_run.unwrap_or(false);
        let tracks = if dry {
            Vec::new()
        } else {
            let label = "Line the drum samples up";
            daw::service::Projects::begin_undo_block(daw, project.clone(), label);
            let set = crate::drum_samples::set_polarity(daw, &project, &measured);
            daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
            set?
        };
        let round = |v: f64, places: f64| (v * places).round() / places;
        let pieces = measured
            .into_iter()
            .map(|m| AlignedPiece {
                delay_ms: m.alignment.map(|a| round(a.delay * 1_000.0, 10.0)),
                inverted: m.alignment.map(|a| a.inverted),
                agreement: m.alignment.map(|a| round(a.agreement, 100.0)),
                consistent: m.alignment.map(|a| round(a.consistent, 100.0)),
                hits: m
                    .alignment
                    .map_or(0, |a| u32::try_from(a.hits).unwrap_or(u32::MAX)),
                flip: m.flip,
                piece: m.piece,
                sample: m.sample,
            })
            .collect();
        Ok(AlignOut {
            pieces,
            tracks,
            applied: !dry,
        })
    })?;
    Ok(())
}

#[cfg(feature = "signal")]
fn register_samples(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SAMPLES, Effect::Mutating, |a: SamplesArg| {
        let (daw, project) = song()?;
        let preset = std::path::PathBuf::from(a.preset.as_deref().unwrap_or(DEFAULT_KIT));
        if !preset.exists() {
            return Err(format!("no kit preset at {}", preset.display()));
        }
        let dry = a.dry_run.unwrap_or(false);
        // A step of its own: undoing the next edit never takes the
        // samples' set-up with it.
        let label = "Set up the drum samples";
        if !dry {
            daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        }
        let report = crate::drum_samples::set_up(daw, &project, &preset, dry);
        if !dry {
            daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        }
        let report = report?;
        Ok(SamplesOut {
            tracks: report.tracks,
            notes: u32::try_from(report.notes).unwrap_or(u32::MAX),
            applied: !dry,
        })
    })?;
    Ok(())
}

static ITEMS: ActionMeta = meta(
    "TRACK_ITEMS",
    "track_items",
    "A track's items",
    "A track's items from `from` to `to` (seconds; all, unless given): each one's GUID, position, length and end, how far into its source it starts and the source's file, its lane (and whether that plays), fades — to see what an edit made: the slices, where they sit, what they play. 500 at most.",
    "Data",
    "",
);

/// Which items.
#[derive(Facet)]
struct ItemsArg {
    track: String,
    from: Option<f64>,
    to: Option<f64>,
}

/// One item.
#[derive(Facet)]
struct ItemRow {
    guid: String,
    position: f64,
    length: f64,
    end: f64,
    source_offset: f64,
    /// The recording it plays: its file's name.
    source: Option<String>,
    lane: u32,
    plays: bool,
    fade_in_ms: f64,
    fade_out_ms: f64,
    /// Drawn out: the slowest rate its stretch markers play any of it at
    /// (1 as recorded, 0.5 half speed — an octave down, as the engine
    /// stretches by rate). `None`: not stretched.
    slowest: Option<f64>,
}

/// The slowest rate between `markers`' neighbours; `None` without two.
fn slowest_rate(markers: &[daw_proto::StretchMarker]) -> Option<f64> {
    markers
        .windows(2)
        .filter_map(|pair| {
            let (a, b) = (pair.first()?, pair.get(1)?);
            let played = b.position - a.position;
            (played > 1e-9).then(|| (b.source_position - a.source_position) / played)
        })
        .reduce(f64::min)
}

fn register_items(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&ITEMS, Effect::ReadOnly, |a: ItemsArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let track = Tracks::get(daw, project.clone(), TrackRef::Guid(guid.clone()))
            .ok_or("the track went")?;
        let (from, to) = (
            a.from.unwrap_or(f64::NEG_INFINITY),
            a.to.unwrap_or(f64::INFINITY),
        );
        let mut rows: Vec<ItemRow> =
            daw::service::Items::get_items(daw, project.clone(), TrackRef::Guid(guid))
                .into_iter()
                .filter_map(|i| {
                    let position = i.position.as_seconds();
                    let end = position + i.length.as_seconds();
                    (end >= from && position <= to).then(|| ItemRow {
                        source: daw_proto::Takes::get_active_take(
                            daw,
                            project.clone(),
                            daw_proto::ItemRef::Guid(i.guid.clone()),
                        )
                        .and_then(|t| t.source_file_path)
                        .and_then(|p| {
                            std::path::Path::new(&p)
                                .file_name()
                                .map(|f| f.to_string_lossy().into_owned())
                        }),
                        lane: i.fixed_lane.unwrap_or(0),
                        plays: daw_ui::studio::project::on_playing_lane(&track, &i),
                        guid: i.guid.clone(),
                        position,
                        length: i.length.as_seconds(),
                        end,
                        source_offset: i.start_offset.as_seconds(),
                        fade_in_ms: i.fade_in_length.as_seconds() * 1_000.0,
                        fade_out_ms: i.fade_out_length.as_seconds() * 1_000.0,
                        slowest: slowest_rate(&daw_proto::StretchMarkers::get_stretch_markers(
                            daw,
                            project.clone(),
                            daw_proto::ItemRef::Guid(i.guid.clone()),
                            daw_proto::TakeRef::Active,
                        )),
                    })
                })
                .collect();
        rows.sort_by(|a, b| a.position.total_cmp(&b.position));
        rows.truncate(500);
        Ok(rows)
    })?;
    Ok(())
}

fn register_lanes(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&EDIT_LANES, Effect::Mutating, |a: LanesArg| {
        let (daw, project) = song()?;
        let dry = a.dry_run.unwrap_or(false);
        let tracks = if a.reset == Some(true) {
            crate::drum_lanes::reset_edit(daw, &project, dry)?
        } else {
            crate::drum_lanes::set_up(daw, &project, dry)?
        };
        Ok(LanesOut {
            tracks,
            applied: !dry,
        })
    })?;
    list.register(&PLAY_LANE, Effect::Mutating, |a: PlayLaneArg| {
        let (daw, project) = song()?;
        crate::drum_lanes::play(daw, &project, &a.lane)
    })?;
    Ok(())
}

fn register_reference_map(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&REFERENCE_MAP, Effect::ReadOnly, |a: MapArg| {
        let (daw, project) = song()?;
        let (_, _, map) = reference(daw, &project, a.track.as_deref())?;
        Ok(map
            .into_iter()
            .map(|m| MapRow {
                pitch: m.pitch,
                notes: u32::try_from(m.notes).unwrap_or(u32::MAX),
                piece: m.piece,
                agreement: m.agreement,
            })
            .collect::<Vec<_>>())
    })?;
    Ok(())
}

fn register_reference_notes(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&REFERENCE, Effect::ReadOnly, |a: ReferenceArg| {
        let (daw, project) = song()?;
        let (notes, _, map) = reference(daw, &project, a.track.as_deref())?;
        let pitches: Vec<u8> = match (&a.piece, a.pitch) {
            (_, Some(p)) => vec![p],
            (Some(piece), None) => map
                .iter()
                .filter(|m| {
                    m.piece
                        .as_deref()
                        .is_some_and(|p| p.eq_ignore_ascii_case(piece))
                })
                .map(|m| m.pitch)
                .collect(),
            (None, None) => map.iter().map(|m| m.pitch).collect(),
        };
        let from = a.from.unwrap_or(f64::NEG_INFINITY);
        let to = a.to.unwrap_or(f64::INFINITY);
        let all: Vec<_> = notes
            .into_iter()
            .filter(|n| pitches.contains(&n.pitch) && n.time >= from && n.time <= to)
            .collect();
        let offset = a.offset.unwrap_or(0);
        let page = all
            .iter()
            .skip(usize::try_from(offset).unwrap_or(usize::MAX))
            .take(usize::try_from(a.limit.unwrap_or(PAGE)).unwrap_or(usize::MAX))
            .map(|n| {
                let (bar, beat, frac) = crate::song_bars::musical(daw, &project, n.time);
                NoteRow {
                    time: n.time,
                    bar,
                    beat: f64::from(beat) + frac,
                    pitch: n.pitch,
                    velocity: n.velocity,
                }
            })
            .collect();
        Ok(NotesPage {
            total: u32::try_from(all.len()).unwrap_or(u32::MAX),
            offset,
            notes: page,
        })
    })?;
    Ok(())
}

fn register_compare(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&COMPARE, Effect::ReadOnly, |a: CompareArg| {
        let (daw, project) = song()?;
        let (notes, pieces, map) = reference(daw, &project, a.track.as_deref())?;
        let (name, _, hits) = pieces
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(&a.piece))
            .ok_or_else(|| {
                format!(
                    "no piece {} with a trigger: one of {:?}",
                    a.piece,
                    pieces
                        .iter()
                        .map(|(n, _, _)| n.as_str())
                        .collect::<Vec<_>>()
                )
            })?;
        let pitches: Vec<u8> = map
            .iter()
            .filter(|m| m.piece.as_deref() == Some(name.as_str()))
            .map(|m| m.pitch)
            .collect();
        let from = a.from.unwrap_or(f64::NEG_INFINITY);
        let to = a.to.unwrap_or(f64::INFINITY);
        let written: Vec<f64> = notes
            .iter()
            .filter(|n| pitches.contains(&n.pitch) && n.time >= from && n.time <= to)
            .map(|n| n.time)
            .collect();
        let played: Vec<f64> = hits
            .iter()
            .copied()
            .filter(|t| *t >= from && *t <= to)
            .collect();
        let window = a.window_ms.map_or(NOTE_WINDOW, |ms| ms / 1_000.0);
        let c = crate::drum_reference::compare(&written, &played, window);
        let mut offsets: Vec<f64> = c.matched.iter().map(|m| m.2).collect();
        offsets.sort_by(f64::total_cmp);
        let median = offsets.get(offsets.len() / 2).copied();
        let worst = offsets
            .iter()
            .copied()
            .max_by(|x, y| x.abs().total_cmp(&y.abs()));
        let at = |t: f64| {
            let (bar, beat, frac) = crate::song_bars::musical(daw, &project, t);
            (bar, f64::from(beat) + frac)
        };
        let mut detail: Vec<NoteVsHit> = c
            .missing
            .iter()
            .map(|t| {
                let (bar, beat) = at(*t);
                NoteVsHit {
                    time: *t,
                    bar,
                    beat,
                    offset_ms: None,
                }
            })
            .chain(c.matched.iter().map(|(t, _, off)| {
                let (bar, beat) = at(*t);
                NoteVsHit {
                    time: *t,
                    bar,
                    beat,
                    offset_ms: Some(*off),
                }
            }))
            .collect();
        detail.sort_by(|x, y| {
            let key = |n: &NoteVsHit| n.offset_ms.map_or(f64::INFINITY, f64::abs);
            key(y).total_cmp(&key(x))
        });
        detail.truncate(usize::try_from(a.limit.unwrap_or(50)).unwrap_or(usize::MAX));
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(CompareResult {
            piece: name.clone(),
            pitches,
            notes: count(written.len()),
            hits: count(played.len()),
            matched: count(c.matched.len()),
            missing: count(c.missing.len()),
            extra: count(c.extra.len()),
            median_offset_ms: median,
            worst_offset_ms: worst,
            detail,
            extra_hits: c.extra.into_iter().take(50).collect(),
        })
    })?;
    Ok(())
}

fn register_tracks(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&TRACKS, Effect::ReadOnly, |a: TracksArg| {
        let (daw, project) = song()?;
        let tracks = Tracks::all(daw, project.clone());
        let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
            tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
        let filter = a.filter.map(|f| f.to_lowercase());
        let open = open_folders(&tracks);
        Ok(tracks
            .iter()
            .map(|t| (t, crate::gain_stage::path_of(t, &by_guid).join("/")))
            .filter(|(_, path)| {
                filter
                    .as_deref()
                    .is_none_or(|f| path.to_lowercase().contains(f))
            })
            .map(|(t, path)| {
                let items = daw::service::Items::get_items(
                    daw,
                    project.clone(),
                    TrackRef::Guid(t.guid.clone()),
                );
                (t, path, items)
            })
            .map(|(t, path, items)| TrackRow {
                lanes: lane_rows(t, &items),
                armed: t.armed,
                record_input: record_input_of(t.record_input),
                open: t.is_folder.then(|| open.contains(&t.guid)),
                guid: t.guid.clone(),
                name: t.name.clone(),
                path,
                folder: t.is_folder,
                muted: t.muted,
                soloed: t.soloed,
                inverted: t.phase_inverted,
                volume_db: 20.0 * t.volume.max(1e-9).log10(),
                pan: t.pan,
                items: u32::try_from(items.len()).unwrap_or(u32::MAX),
                fx: daw::service::Effects::list(
                    daw,
                    project.clone(),
                    daw::service::FxChainContext::Track(t.guid.clone()),
                )
                .into_iter()
                .map(|f| {
                    if f.enabled {
                        f.plugin_name
                    } else {
                        format!("{} (bypassed)", f.plugin_name)
                    }
                })
                .collect(),
            })
            .collect::<Vec<_>>())
    })?;
    Ok(())
}

fn register_fundamental(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&DRUM_FUNDAMENTAL, Effect::ReadOnly, |a: FundamentalArg| {
        let (daw, project) = song()?;
        let trigger = resolve(daw, &project, &a.trigger)?;
        let mics = a
            .mics
            .iter()
            .map(|m| resolve(daw, &project, m))
            .collect::<Result<Vec<_>, _>>()?;
        let range = match a.range {
            DrumRange::Kick => crate::fundamental::KICK,
            DrumRange::Snare => crate::fundamental::SNARE,
            DrumRange::Tom => crate::fundamental::TOM,
        };
        crate::fundamental::find(daw, &project, &trigger, &mics, range).ok_or_else(|| {
            "no fundamental: no hits on the trigger, or nothing on the mics at them".to_owned()
        })
    })?;
    Ok(())
}

fn register_track_set(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&TRACK_SET, Effect::Mutating, |a: TrackSetArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let at = || TrackRef::Guid(guid.clone());
        let failed = |e: daw_proto::DawError| e.to_string();
        if let Some(db) = a.volume_db {
            Tracks::set_volume(daw, project.clone(), at(), 10_f64.powf(db / 20.0))
                .map_err(failed)?;
        }
        if let Some(pan) = a.pan {
            Tracks::set_pan(daw, project.clone(), at(), pan.clamp(-1.0, 1.0)).map_err(failed)?;
        }
        if let Some(mute) = a.mute {
            Tracks::set_muted(daw, project.clone(), at(), mute).map_err(failed)?;
        }
        if let Some(solo) = a.solo {
            Tracks::set_soloed(daw, project.clone(), at(), solo).map_err(failed)?;
        }
        if let Some(monitor) = a.monitor.as_deref() {
            let mode = crate::shared_actions::monitor_mode(monitor)?;
            Tracks::set_input_monitor(daw, project.clone(), at(), mode).map_err(failed)?;
        }
        if let Some(phase) = a.phase {
            Tracks::set_phase_inverted(daw, project.clone(), at(), phase).map_err(failed)?;
        }
        if let Some(name) = a.name.as_deref() {
            Tracks::rename(daw, project, at(), name).map_err(failed)?;
        }
        crate::actions::list().changed("tracks", &guid);
        Ok(())
    })?;
    Ok(())
}

/// The engine and the open song, as a pair — for the call sites not yet
/// on [`crate::song::Song`], which new code takes instead.
pub(crate) fn song() -> Result<(&'static daw::standalone::Standalone, ProjectContext), String> {
    let song = crate::song::Song::current()?;
    Ok((song.daw(), song.context()))
}

/// A track by its GUID, its path (`Kick/Sum/Trig`, any tail of it) or its
/// name — refused, naming the candidates, when more than one fits.
pub fn resolve(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    what: &str,
) -> Result<String, String> {
    crate::shared_actions::resolve_in(&Tracks::all(daw, project.clone()), what)
}

static EDIT_SETTINGS: ActionMeta = meta(
    "DRUMS_EDIT_SETTINGS",
    "drums_edit_settings",
    "Drum edit settings",
    "The quantize settings as they stand — sticky: what QUANTIZE uses when a call gives none, kept across restarts. `grid` (a whole note's division for the strokes nothing else places — 8, eighths, unless set; finer only where it is known: a written note's own line, or a Grid region over a part, TRACK_REGION_ADD; 16, 12 for 8th triplets, 24 for 16th triplets), `grid_scan` (one hit per grid line, the loudest within `grid_tolerance_ms`), `target` (Guided: the drum MIDI says which line, else the nearest line in reach — `sure_ms` on the grid, twice that a grid coarser, down to the sixteenths: a double kick's stroke 24 ms off its sixteenth is the sixteenth's; Reference; Grid), `window_ms`, `sure_ms`, `tolerance_ms`, `strength`, `lead_ms`, `xfade_ms`, `stretch_gaps_ms`, `fills` (Skip, Include, Only), `lead` (a piece whose timing wins where strokes are moved together — `Snare` in a breakdown, so the backbeat is on its line and the sixteenth kicks move with it; empty: the loudest). Any of them can be given to one QUANTIZE call, or to one step of its `plan` (a section). How the hits are found is HITS_SETTINGS'.",
    "Drums",
    "",
);
static EDIT_SET: ActionMeta = meta(
    "DRUMS_EDIT_SET",
    "drums_edit_set",
    "Set drum edit settings",
    "Change any quantize settings (the ones given, as DRUMS_EDIT_SETTINGS names them; `reset` first goes back to the defaults) — they stay changed. Returns them.",
    "Drums",
    "",
);
static PEAKS: ActionMeta = meta(
    "AUDIO_PEAKS",
    "audio_peaks",
    "Audio peaks",
    "A track's audio from `from` to `to` (seconds) as `points` peaks (each the loudest sample in its slice, linear 0..1, through the item volume; 400 unless asked, 4000 at most) — the waveform as numbers, to read a hit, a ring, a stray, a double. `db` gives them in dBFS.",
    "Data",
    "",
);

/// The kit's live pieces and their hits, from the hit markers — each
/// hit's loudness against its piece's loudest; none before `HITS_DETECT`.
pub fn kit(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<KitPiece> {
    // Found once (HITS_DETECT), corrected by hand, never detected again
    // here (`r[quantize.reads-markers]`).
    crate::hits::kit_from_markers(daw, project).unwrap_or_default()
}

/// Every beat from the bar holding `from` to the one holding `to`.
fn beats(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (from, to): (f64, f64),
) -> Vec<crate::drum_edit::Beat> {
    let (mut bar, _, _) = daw.time_to_musical(project.clone(), from.max(0.0));
    let mut starts: Vec<(f64, u32)> = Vec::new();
    loop {
        let downbeat = daw.musical_to_time(project.clone(), bar, 1, 0.0);
        let (per_bar, _) = daw.get_time_signature_at(project.clone(), downbeat);
        for beat in 1..=per_bar.max(1) {
            let at = daw.musical_to_time(project.clone(), bar, beat, 0.0);
            starts.push((at, u32::try_from(beat.saturating_sub(1)).unwrap_or(0)));
        }
        bar = bar.saturating_add(1);
        if downbeat > to || starts.len() > 100_000 {
            break;
        }
    }
    starts
        .iter()
        .zip(starts.iter().skip(1))
        .map(|(&(start, in_bar), &(next, _))| crate::drum_edit::Beat {
            start,
            length: (next - start).max(1e-3),
            in_bar,
        })
        .collect()
}

/// The fills among the kit's hits, from `from` to `to`.
fn find_fills(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    kit: &[KitPiece],
    (from, to): (f64, f64),
) -> Vec<crate::drum_edit::Fill> {
    let hits: Vec<(crate::drum_edit::Piece, f64)> = kit
        .iter()
        .flat_map(|(_, piece, hits)| hits.iter().map(|h| (*piece, h.time)))
        .filter(|(_, t)| *t >= from && *t <= to)
        .collect();
    let Some(first) = hits.iter().map(|h| h.1).reduce(f64::min) else {
        return Vec::new();
    };
    let last = hits.iter().map(|h| h.1).fold(first, f64::max);
    crate::drum_edit::fills(&hits, &beats(daw, project, (first, last)))
}

/// The grid line nearest `h`, at `division` of a whole note.
pub fn grid_line(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    h: f64,
    division: u32,
) -> f64 {
    let step = 240.0 / daw.get_tempo_at(project.clone(), h).max(1.0) / f64::from(division.max(1));
    (h / step).round() * step
}

/// A sixteenth at `t`, seconds.
fn sixteenth(daw: &daw::standalone::Standalone, project: &ProjectContext, t: f64) -> f64 {
    15.0 / daw.get_tempo_at(project.clone(), t).max(1.0)
}

/// Half a sixteenth at `t`, seconds: how far before a section's downbeat
/// a hit still belongs to it — a downbeat pushed early is the section's
/// first hit, not the last of the one before.
fn edge(daw: &daw::standalone::Standalone, project: &ProjectContext, t: f64) -> f64 {
    sixteenth(daw, project, t) / 2.0
}

// r[impl quantize.written]
/// Each moment's target and what placed it. With `target: Guided` or
/// `Pattern` the drum MIDI places what it can: in `span`, each piece's
/// hits matched one to one with its own notes, the section's feel taken
/// out first; the rest go to the nearest line —
/// again with the feel out — when within `sure_ms` of it.
fn targets(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    (kit, moments): (&[KitPiece], &[Moment]),
    span: (f64, f64),
    prepared: &Prepared,
) -> Result<Vec<(f64, Option<f64>, Placed)>, String> {
    use crate::drum_settings::Target;
    // A part's own grid (a Grid region longer than a bar over it) in
    // place of the setting: what a stroke nothing else places goes to.
    let parts = &prepared.parts;
    let grid_of = |m: &Moment| {
        parts
            .grid(&m.pieces, m.time, sixteenth(daw, project, m.time))
            .unwrap_or(settings.grid)
    };
    if settings.target == Target::Grid {
        return Ok(moments
            .iter()
            .map(|m| {
                (
                    m.time,
                    Some(grid_line(daw, project, m.time, grid_of(m))),
                    Placed::Line,
                )
            })
            .collect());
    }
    // The drum MIDI, and which of its notes are which piece — learned
    // against these hits.
    let (notes, map) = prepared
        .written
        .as_ref()
        .ok_or("no drum MIDI track to guide by — use `target: Grid`")?;
    let _ = kit;
    let step = sixteenth(daw, project, span.0);
    let (lo, hi) = (span.0 - edge(daw, project, span.0), span.1);
    // Each piece's hits in the span and the note each is.
    let mut written: Vec<(&str, f64, f64)> = Vec::new();
    // Every piece's hits plainly their notes, `(hit, offset)`: the feel.
    let mut plain: Vec<(f64, f64)> = Vec::new();
    // The lines the chart writes a piece of the kit on.
    let mut kit_lines: Vec<f64> = Vec::new();
    // Every line the chart writes anything on — a piece of the kit, or a
    // note the map cannot name (a tom with no trigger, a cymbal on the
    // kick) — each on its line.
    let mut lines: Vec<f64> = notes
        .iter()
        .filter(|n| n.time >= lo - step && n.time < hi + step)
        .filter_map(|n| {
            [32, 24]
                .into_iter()
                .map(|division| grid_line(daw, project, n.time, division))
                .min_by(|a, b| (a - n.time).abs().total_cmp(&(b - n.time).abs()))
                .filter(|line| (line - n.time).abs() <= ON_ITS_LINE)
        })
        .collect();
    // Each piece's notes and hits in the span; first the feel, read off
    // every piece's plain matches together, then each piece's own match.
    let mut pieces: Vec<(&str, Vec<f64>, Vec<f64>)> = Vec::new();
    for (name, _, hits) in kit {
        let pitches: Vec<u8> = map
            .iter()
            .filter(|m| m.piece.as_deref() == Some(name.as_str()))
            .map(|m| m.pitch)
            .collect();
        // A note is the line it was written on — a thirty-second or a
        // sixteenth triplet; one on neither, played in by hand, is no line
        // to send a hit to.
        let mut its: Vec<f64> = notes
            .iter()
            .filter(|n| pitches.contains(&n.pitch) && n.time >= lo - step && n.time < hi + step)
            .filter_map(|n| {
                [32, 24]
                    .into_iter()
                    .map(|division| grid_line(daw, project, n.time, division))
                    .min_by(|a, b| (a - n.time).abs().total_cmp(&(b - n.time).abs()))
                    .filter(|line| (line - n.time).abs() <= ON_ITS_LINE)
            })
            .collect();
        its.sort_by(f64::total_cmp);
        its.dedup_by(|x, y| (*x - *y).abs() < 0.002);
        kit_lines.extend(its.iter().copied());
        let played: Vec<f64> = hits
            .iter()
            .map(|h| h.time)
            .filter(|t| *t >= lo && *t < hi)
            .collect();
        if !played.is_empty() && !its.is_empty() {
            plain.extend(crate::written::plain(&played, &its, step));
            pieces.push((name.as_str(), played, its));
        }
    }
    plain.sort_by(|a, b| a.0.total_cmp(&b.0));
    let feel = |t: f64| crate::written::feel_at(&plain, t);
    for (name, played, its) in &pieces {
        let aligned = crate::written::align(played, its, step, feel);
        written.extend(
            played
                .iter()
                .zip(&aligned.notes)
                .filter_map(|(h, n)| n.map(|n| (*name, *h, n))),
        );
    }
    lines.sort_by(f64::total_cmp);
    lines.dedup_by(|x, y| (*x - *y).abs() < 0.002);
    kit_lines.sort_by(f64::total_cmp);
    kit_lines.dedup_by(|x, y| (*x - *y).abs() < 0.002);
    let nearest_written = |t: f64| nearest_of(&lines, t);
    let nearest_kit = |t: f64| nearest_of(&kit_lines, t);
    let sure = settings.sure_ms / 1_000.0;
    Ok(moments
        .iter()
        .map(|m| {
            let grid = grid_of(m);
            // Its pieces' notes; where they disagree, the one most of
            // them say.
            let mut said: Vec<f64> = written
                .iter()
                .filter(|(name, h, _)| {
                    m.pieces.iter().any(|p| p == name)
                        && *h >= m.time - 1e-9
                        && *h <= m.time + KIT_MERGE
                })
                .map(|(_, _, n)| *n)
                .collect();
            said.sort_by(f64::total_cmp);
            let note = said
                .iter()
                .max_by_key(|n| said.iter().filter(|o| (*o - **n).abs() < 0.002).count())
                .copied();
            let inside = m.time >= lo && m.time < hi;
            let felt = if inside {
                m.time + feel(m.time)
            } else {
                m.time
            };
            let within = |t: f64| (t - m.time).abs() <= step + FURTHEST_PAST;
            // The playing says which line where it is clear: a stroke
            // played on a line keeps it against a written line further than
            // `sure` (a triplet charted against a sixteenth played, a chart
            // a sixteenth out); one in reach of a line ([`grid_choice`])
            // keeps that against a written line more than half a sixteenth
            // off — moving it there rewrites the drummer, not the timing.
            let played_on = grid_line(daw, project, felt, grid);
            let on_a_line = (played_on - felt).abs() <= PLAYED_ON_A_LINE;
            let own_line = if on_a_line {
                Some(played_on)
            } else {
                grid_choice(
                    |division| grid_line(daw, project, felt, division),
                    felt,
                    grid,
                    sure,
                )
            };
            let coarse = if grid.is_multiple_of(3) { 12 } else { 16 };
            let keeps_its_line = |line: f64| {
                keeps_own_line(
                    (felt, line),
                    own_line.map(|own| (own, grid_line(daw, project, own, coarse))),
                    on_a_line,
                    (sure, step),
                )
            };
            // In a part with a grid of its own, a written note off that
            // grid is not the part: a triplet charted in a part of
            // sixteenths is the chart's, not the drummer's.
            // Distrusted only across the feel: a triplet note in a straight
            // part or a straight one in a triplet part. A sixteenth written
            // in a part of quarters (a snare's backbeat part) is still one.
            let on_the_part = |n: f64| {
                parts
                    .grid(&m.pieces, m.time, sixteenth(daw, project, m.time))
                    .is_none_or(|part| {
                        let fine = if part.is_multiple_of(3) { 48 } else { 32 };
                        (grid_line(daw, project, n, fine) - n).abs() <= ON_ITS_LINE
                    })
            };
            let note = note.filter(|n| on_the_part(*n));
            if let Some(note) = note {
                // A fill plays a written figure with the pieces swapped —
                // a kick where the chart has a snare. Nearer another
                // piece's note than its own, the hit is that line's (a
                // piece of the kit: a hat on every sixteenth says nothing
                // of where a kick goes).
                let line = nearest_kit(felt)
                    .filter(|w| (w - felt).abs() < (note - felt).abs())
                    .unwrap_or(note);
                if within(line) && keeps_its_line(line) {
                    return (m.time, own_line, Placed::Line);
                }
                if within(line) {
                    return (m.time, Some(line), Placed::Written);
                }
            }
            // No note of its own: a line the chart writes something on,
            // near.
            if inside
                && let Some(line) = nearest_written(felt).filter(|w| {
                    (w - felt).abs() <= NEAR_WRITTEN * step
                        && within(*w)
                        && !keeps_its_line(*w)
                        && on_the_part(*w)
                })
            {
                return (m.time, Some(line), Placed::Near);
            }
            // Nothing written: the part's grid is what is known to be
            // there, so the stroke goes to its nearest line — an eighth
            // unless the part says sixteenths. Only a grid finer than the
            // sixteenths (or their triplets) asks how sure the stroke is.
            if grid <= 16 || grid == 24 {
                // Within a sixteenth: a part of quarters (the snare's in a
                // breakdown) does not drag a roll's strokes an eighth.
                let line = grid_line(daw, project, felt, grid);
                if (line - felt).abs() <= step {
                    return (m.time, Some(line), Placed::Line);
                }
            }
            grid_choice(
                |division| grid_line(daw, project, felt, division),
                felt,
                grid,
                sure,
            )
            .map_or((m.time, None, Placed::Nothing), |line| {
                (m.time, Some(line), Placed::Line)
            })
        })
        .collect())
}

/// What the kit's own regions say of a hit's line: a figure's grid (a
/// Grid region of a bar or less), or a fill's own (`ZONES_EDIT` `grid`).
struct Overrides {
    regions: Vec<crate::track_regions::TrackRegion>,
    zones: Vec<crate::hits::Zone>,
}

/// What a quantize reads once, for every span it edits: the kit's hits, its
/// parts' grids, the regions and zones that put a hit on a line of their
/// own, and the drum MIDI with which pitch is which piece — learned once
/// against the kit. Read per span, a whole song's quantize read the song
/// twenty times over.
struct Prepared {
    kit: Vec<KitPiece>,
    parts: crate::track_regions::PartGrids,
    overrides: Overrides,
    zones: Vec<crate::hits::Zone>,
    written: Option<(
        Vec<crate::drum_reference::RefNote>,
        Vec<crate::drum_reference::Mapped>,
    )>,
}

impl Prepared {
    fn read(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Self {
        let kit = kit(daw, project);
        let written = crate::drum_reference::find_track(daw, project).map(|midi| {
            let notes = crate::drum_reference::notes(daw, project, &midi);
            let named: Vec<(String, Vec<f64>)> = kit
                .iter()
                .map(|(n, _, h)| (n.clone(), h.iter().map(|h| h.time).collect()))
                .collect();
            let map = crate::drum_reference::learn_map(&notes, &named, NOTE_WINDOW);
            (notes, map)
        });
        Self {
            parts: crate::track_regions::PartGrids::of(daw, project),
            overrides: Overrides::of(daw, project),
            zones: crate::hits::zones(daw, project),
            kit,
            written,
        }
    }
}

impl Overrides {
    fn of(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Self {
        Self {
            regions: crate::track_regions::kit_regions(daw, project),
            zones: crate::hits::zones(daw, project),
        }
    }

    /// The fills (not turned down) holding `h`, a hit half a sixteenth
    /// before one's end being the next's.
    fn fills_at<'a>(
        &'a self,
        daw: &'a daw::standalone::Standalone,
        project: &'a ProjectContext,
        h: f64,
    ) -> impl Iterator<Item = &'a crate::hits::Zone> + 'a {
        self.zones.iter().filter(move |z| {
            z.kind == crate::hits::ZoneKind::Fill
                && z.origin != crate::hits::ZoneOrigin::Rejected
                && h >= z.start - edge(daw, project, z.start)
                && h < z.end - edge(daw, project, z.end)
        })
    }

    /// The figure that holds `h`, if one does.
    fn figure(
        &self,
        daw: &daw::standalone::Standalone,
        project: &ProjectContext,
        h: f64,
    ) -> Option<&crate::track_regions::TrackRegion> {
        crate::track_regions::figure_at(&self.regions, h, sixteenth(daw, project, h))
    }

    /// Whether a figure says where `h` goes over the fill it is in: one
    /// shorter than the fill (or in none) is about that stretch.
    fn figure_over_fill(
        &self,
        daw: &daw::standalone::Standalone,
        project: &ProjectContext,
        h: f64,
    ) -> bool {
        let fill = self
            .fills_at(daw, project, h)
            .map(|z| z.end - z.start)
            .fold(f64::INFINITY, f64::min);
        self.figure(daw, project, h)
            .is_some_and(|r| r.end - r.start < fill)
    }

    /// The grid of the innermost fill holding `h` quantized on one.
    fn fill_grid(
        &self,
        daw: &daw::standalone::Standalone,
        project: &ProjectContext,
        h: f64,
    ) -> Option<u32> {
        self.fills_at(daw, project, h)
            .filter_map(|z| z.grid.map(|g| (z.end - z.start, g)))
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, g)| g)
    }

    /// The grid `h` goes to: a figure's over its fill, else the fill's own.
    fn division(
        &self,
        daw: &daw::standalone::Standalone,
        project: &ProjectContext,
        h: f64,
    ) -> Option<u32> {
        if self.figure_over_fill(daw, project, h) {
            self.figure(daw, project, h).and_then(|r| r.division)
        } else {
            self.fill_grid(daw, project, h)
        }
    }
}

/// What the settings make of the kit: its pieces, its moments (with
/// their targets), its fills.
struct Detected {
    kit: Vec<KitPiece>,
    moments: Vec<Moment>,
    targets: Vec<(f64, Option<f64>)>,
    fills: Vec<crate::drum_edit::Fill>,
    /// The moments the groove sent to a line other than the nearest:
    /// `(time, its line, the nearest)`.
    by_groove: Vec<(f64, f64, f64)>,
    /// The moments the drum MIDI sent to a line other than the nearest:
    /// `(time, its note, the nearest)`.
    by_written: Vec<(f64, f64, f64)>,
    /// What placed each moment, in the moments' order.
    placed_by: Vec<Placed>,
}

fn detect(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    span: (f64, f64),
    prepared: &Prepared,
) -> Result<Detected, String> {
    let mut stages = crate::stages::Stages::start();
    let kit = &prepared.kit;
    stages.mark("detect.kit_ms");
    let line_of = |h: f64| grid_line(daw, project, h, settings.grid);
    let by_line: &dyn Fn(f64) -> f64 = &line_of;
    let tolerance = settings
        .grid_tolerance_ms
        .map_or(f64::INFINITY, |ms| ms / 1_000.0);
    let moments = moments(kit, settings.grid_scan.then_some((by_line, tolerance)));
    let loudness = weights(&moments, settings);
    stages.mark("detect.moments_ms");
    let by = targets(daw, project, settings, (kit, &moments), span, prepared)?;
    stages.mark("detect.targets_ms");
    let mut placed: Vec<(f64, Option<f64>)> = by.iter().map(|(h, t, _)| (*h, *t)).collect();
    let mut by_groove = Vec::new();
    let mut by_written = Vec::new();
    for (h, t, b) in &by {
        if let (Placed::Written, Some(note)) = (b, t) {
            let nearest = grid_line(daw, project, *h, settings.grid);
            if (note - nearest).abs() > 0.002 {
                by_written.push((*h, *note, nearest));
            }
        }
    }
    stages.mark("detect.written_ms");
    let lo = span.0 - edge(daw, project, span.0);
    let hi = span.1 - edge(daw, project, span.1);
    if settings.target == crate::drum_settings::Target::Pattern
        && let Some(groove) = groove_of(daw, project, (kit, settings.grid.is_multiple_of(3)), span)
    {
        for ((moment, (h, to)), (_, _, b)) in moments.iter().zip(placed.iter_mut()).zip(&by) {
            // What the drum MIDI places, it places: the groove is for
            // the hits it has no note for.
            if *b == Placed::Written {
                continue;
            }
            if *h >= lo
                && *h < hi
                && let Some(line) = groove
                    .target(
                        daw,
                        project,
                        &kit,
                        (moment, grid_line(daw, project, *h, settings.grid)),
                    )
                    // Never out of the span: the next section's first hit
                    // is where its own edit starts.
                    .filter(|line| *line >= span.0 && *line < span.1 - SEAM_ROOM)
                    .filter(|line| (line - *h).abs() <= sixteenth(daw, project, *h) + FURTHEST_PAST)
            {
                let nearest = grid_line(daw, project, *h, settings.grid);
                if (line - nearest).abs() > 0.002 {
                    by_groove.push((*h, line, nearest));
                }
                *to = Some(line);
            }
        }
    }
    // A target past the span's end is the next section's line: this one
    // leaves it as played rather than run its edit into that one.
    for (h, to) in &mut placed {
        if *h >= lo && *h < hi && to.is_some_and(|t| t >= span.1 - SEAM_ROOM) {
            *to = None;
        }
    }
    stages.mark("detect.groove_ms");
    let mut by_kind: Vec<Placed> = by.iter().map(|(_, _, b)| *b).collect();
    // A figure, or a fill quantized on a grid of its own, puts its hits on
    // its grid — before strokes sent to one line are settled and gestures
    // held together, so two strokes of a fast fill sent to one sixteenth
    // are not two slices on top of each other. What it places is placed
    // as surely as a written note.
    let overrides = &prepared.overrides;
    for ((h, to), kind) in placed.iter_mut().zip(by_kind.iter_mut()) {
        if let Some(division) = overrides.division(daw, project, *h) {
            *to = Some(grid_line(daw, project, *h, division));
            *kind = Placed::Written;
        }
    }
    // Strokes sent to one line take the lines either side of it, in order,
    // where they can: a double kick played into the snare on 2 is the "&"
    // and the "a" before it, not one stroke between the others.
    stages.mark("detect.overrides_ms");
    let parts = &prepared.parts;
    stages.mark("detect.parts_ms");
    // Only on a grid of sixteenths or finer: a double is two sixteenths.
    // On eighths (strokes played between them, two sent to one) the lines
    // either side are an eighth away — the strokes shoved 200 ms, a bar's
    // audio scrambled (Unbreakable bar 40).
    let step_of = |i: usize| {
        let m = moments.get(i)?;
        let grid = overrides
            .division(daw, project, m.time)
            .or_else(|| parts.grid(&m.pieces, m.time, sixteenth(daw, project, m.time)))
            .unwrap_or(settings.grid);
        if grid < 16 {
            return None;
        }
        Some(240.0 / daw.get_tempo_at(project.clone(), m.time).max(1.0) / f64::from(grid.max(1)))
    };
    let on_a_beat = |t: f64| (grid_line(daw, project, t, 4) - t).abs() <= ON_ITS_LINE;
    // Played with the moment after: another drum, a gesture before it.
    let with_next = |k: usize, next: usize| {
        moments.get(k).zip(moments.get(next)).is_some_and(|(m, n)| {
            n.time - m.time < GESTURE && !m.pieces.iter().any(|p| n.pieces.contains(p))
        })
    };
    to_lines_either_side(&mut placed, &by_kind, (step_of, on_a_beat, with_next));
    one_stroke_a_line(&mut placed, &by_kind);
    // Two strokes of one drum alone, a double kick's, sent to lines of
    // their own are two strokes: not one gesture moved by one shift, which
    // kept a double played 40 ms apart 40 ms apart, off every line. The
    // seams keep the first's attack from being played twice.
    let apart = |k: usize| {
        let (Some(prev), Some(next)) = (
            k.checked_sub(1).and_then(|p| moments.get(p)),
            moments.get(k),
        ) else {
            return false;
        };
        let (Some((_, Some(a))), Some((_, Some(b)))) =
            (k.checked_sub(1).and_then(|p| placed.get(p)), placed.get(k))
        else {
            return false;
        };
        let gap = next.time - prev.time;
        let own = prev.pieces.len() == 1 && prev.pieces == next.pieces;
        // One drum's two strokes a double's way apart; or two drums far
        // enough apart to be two strokes, not a flam — a kick into the
        // snare on the next sixteenth, each on its own line.
        (gap >= if own { DOUBLE_STROKE } else { TWO_STROKES }) && b - a >= DOUBLE_STROKE
    };
    let targets = hold_gestures_by(&placed, &loudness, apart);
    let fills = find_fills(daw, project, &kit, span);
    Ok(Detected {
        kit: kit.clone(),
        moments,
        targets,
        fills,
        by_groove,
        by_written,
        placed_by: by.iter().map(|(_, _, b)| *b).collect(),
    })
}

/// How far from a slot's hit a written note may be and still vote for its
/// line, beats: more than a drift the groove still places.
const VOTE_WINDOW_BEATS: f64 = 0.25;

/// A span's groove, with the kit's hits it was learned from.
struct SpanGroove {
    groove: crate::groove::Groove,
    /// Each hit as the groove placed it, and its time (seconds).
    hits: Vec<(crate::groove::Hit, f64)>,
}

/// `t` seconds as a bar and how far into it (beats from its downbeat).
fn in_bar(daw: &daw::standalone::Standalone, project: &ProjectContext, t: f64) -> (i32, f64) {
    let (bar, beat, frac) = crate::song_bars::musical(daw, project, t);
    (bar, f64::from(beat.saturating_sub(1)) + frac)
}

/// The groove of `kit` over `span` (`crate::groove`), its edit zones —
/// the fills — left out of the learning: none with too few bars.
fn groove_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (kit, triplets): (&[KitPiece], bool),
    (from, to): (f64, f64),
) -> Option<SpanGroove> {
    let last = kit
        .iter()
        .flat_map(|(_, _, hits)| hits.iter().map(|h| h.time))
        .fold(f64::NEG_INFINITY, f64::max);
    let (from, to) = (from.max(0.0), to.min(last + 0.001));
    if to <= from {
        return None;
    }
    let (first_bar, _) = in_bar(daw, project, from + 1e-6);
    let (last_bar, _) = in_bar(daw, project, to - 1e-6);
    let start = crate::song_bars::time(daw, project, first_bar, 1, 0.0);
    let next = crate::song_bars::time(daw, project, first_bar.saturating_add(1), 1, 0.0);
    let beat_len = 60.0 / daw.get_tempo_at(project.clone(), start).max(1.0);
    let beats = ((next - start) / beat_len).round().max(1.0);
    let zones = crate::hits::zones(daw, project);
    let hits: Vec<(crate::groove::Hit, f64)> = kit
        .iter()
        .enumerate()
        .flat_map(|(piece, (_, _, hits))| {
            hits.iter()
                .map(|h| h.time)
                .filter(|t| *t >= from && *t < to)
                .filter(|t| !crate::hits::zone_holds(&zones, *t, beat_len))
                .map(move |t| (piece, t))
        })
        .map(|(piece, t)| {
            let (bar, beat) = in_bar(daw, project, t);
            (crate::groove::Hit { piece, bar, beat }, t)
        })
        .collect();
    let bars = u32::try_from(last_bar.saturating_sub(first_bar).saturating_add(1)).ok()?;
    let placed: Vec<crate::groove::Hit> = hits.iter().map(|h| h.0).collect();
    let mut groove = crate::groove::Groove::learn_on(&placed, beats, (first_bar, bars), triplets)?;
    // Which line a slot is on, where the drummer sits between two: what
    // the drum MIDI writes there, when it mostly agrees.
    if let Some(midi) = crate::drum_reference::find_track(daw, project) {
        let notes = crate::drum_reference::notes(daw, project, &midi);
        let named: Vec<(String, Vec<f64>)> = kit
            .iter()
            .map(|(n, _, h)| (n.clone(), h.iter().map(|h| h.time).collect()))
            .collect();
        let map = crate::drum_reference::learn_map(&notes, &named, NOTE_WINDOW);
        let written: Vec<crate::groove::Hit> = notes
            .iter()
            .filter(|n| n.time >= from - 1.0 && n.time < to + 1.0)
            .filter_map(|n| {
                let name = map.iter().find(|m| m.pitch == n.pitch)?.piece.as_ref()?;
                let piece = kit.iter().position(|(k, _, _)| k == name)?;
                let (bar, beat) = in_bar(daw, project, n.time);
                Some(crate::groove::Hit { piece, bar, beat })
            })
            .collect();
        groove.settle_lines(&placed, &written, VOTE_WINDOW_BEATS);
    }
    Some(SpanGroove { groove, hits })
}

impl SpanGroove {
    /// Where `moment` goes by the groove: the line of the slot one of its
    /// pieces is in — the most played such slot, when its pieces
    /// disagree — or none, in no slot. A slot's line other than the
    /// `nearest` is taken only for a hit nearer where that slot is
    /// usually played than it is to the nearest line: a kick 34 ms before
    /// 2 is beat 2's, not a "1a" played 61 ms late, however wide the
    /// "1a"s wander.
    fn target(
        &self,
        daw: &daw::standalone::Standalone,
        project: &ProjectContext,
        kit: &[KitPiece],
        (moment, nearest): (&Moment, f64),
    ) -> Option<f64> {
        let (bar, beat) = in_bar(daw, project, moment.time);
        let beat_len = 60.0 / daw.get_tempo_at(project.clone(), moment.time).max(1.0);
        let to_nearest = (nearest - moment.time) / beat_len;
        moment
            .pieces
            .iter()
            .filter_map(|name| kit.iter().position(|(n, _, _)| n == name))
            .filter_map(|piece| {
                let hit = crate::groove::Hit { piece, bar, beat };
                let slot = self.groove.slot_of(hit)?;
                let pull = self.groove.pull(hit)?;
                let agrees = (pull - to_nearest).abs() < 1e-3;
                let nearer = self
                    .groove
                    .from_middle(hit)
                    .is_some_and(|m| m.abs() < to_nearest.abs());
                (agrees || nearer).then_some((slot.cycles, pull))
            })
            .max_by_key(|(cycles, _)| *cycles)
            .map(|(_, pull)| pull.mul_add(beat_len, moment.time))
    }
}

/// Whether the hit at `h` is in one of `fills`.
fn in_a_fill(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    fills: &[crate::drum_edit::Fill],
    h: f64,
) -> bool {
    let beat = 60.0 / daw.get_tempo_at(project.clone(), h).max(1.0);
    fills.iter().any(|f| f.holds(h, beat))
}

static QUANTIZE: ActionMeta = meta(
    "QUANTIZE",
    "quantize",
    "Quantize the drums",
    "Quantize the kit from its hit markers (HITS_DETECT first): every drum track cut `lead_ms` before each hit and moved together so the HIT lands on its target — the item starts before the beat, the attack is on it — the seams crossfaded (`xfade_ms`). Over `sections` (by name, from the SECTIONS lane: `Verse 1`, `Chorus`) or `from`/`to` (seconds), each with these settings — so a verse and a chorus can each be quantized their own way. The edit zones (ZONES_LIST) are left as played — `fills: Include` edits them too (a fill that keeps a grid, on that grid: `grid: 12` for 8th triplets, 24 for 16th triplets), `Only` edits only them; the drum-edit settings (grid, strength, target, tolerance, lead, crossfade…) can be given for this call alone. `target: Pattern` places each hit by the section's own groove (DRUMS_GROOVE): a hit in a slot the section plays bar after bar goes to that slot's line, however far it drifted, and `by_groove` lists each sent to a line other than the nearest. `dry_run` says what it would do; `brief` leaves out the lists of moves and keeps three of each span's calls. After an edit, a check from the markers: hits lost, hits doubled (a double attack), and how far the moved hits landed from where they were sent.",
    "Drums",
    "",
);
static ZONES_FIND: ActionMeta = meta(
    "ZONES_FIND",
    "zones_find",
    "Find the fills",
    "Find where the drummer leaves the groove, from the hit markers, and propose each fill as an `auto` zone on the ruler's Drums lane, in `from`/`to` (seconds; the whole song when left out): the auto zones there replaced, none laid over one already decided. Approve or reject them with ZONES_EDIT.",
    "Drums",
    "",
);

static GROOVE: ActionMeta = meta(
    "DRUMS_GROOVE",
    "drums_groove",
    "The drums' groove",
    "Each section's groove, learned from its hit markers (its fill zones left out): where in the bar each piece plays bar after bar — each slot counted as a drummer counts (`2a`; `2:3&` in a two-bar cycle), in how many bars of how many it is played, the feel there (ms behind its line, late positive) and how much that wanders — and the bars that depart from it: hits in no slot (`extra`) and slots not played (`missing`), where a fill or a variation is. Over `sections` (by name) or `from`/`to` (seconds). QUANTIZE with `target: Pattern` sends each hit in a slot to the slot's line, however far it drifted. Read-only.",
    "Drums",
    "",
);

/// Where to read the groove.
#[derive(Facet)]
struct GrooveArg {
    #[facet(default)]
    sections: Vec<String>,
    from: Option<f64>,
    to: Option<f64>,
    /// Let a slot be a triplet's line where it is plainly nearer one —
    /// what `target: Pattern` does on a triplet grid (`grid: 12`, 24).
    /// Left out, every slot is on a 16th.
    triplets: Option<bool>,
}

/// One slot of a groove.
#[derive(Facet)]
struct SlotOut {
    /// Counted as a drummer counts it: `1`, `2e`, `3&`, `4a`, a
    /// triplet's `2t`/`2l`; `2:3&` for the second bar of a two-bar cycle.
    at: String,
    played: u32,
    of: u32,
    /// Where it is played against its line, ms (late positive).
    feel_ms: f64,
    /// How much that wanders, ms.
    spread_ms: f64,
}

/// One piece's part in a groove.
#[derive(Facet)]
struct PieceGroove {
    piece: String,
    slots: Vec<SlotOut>,
}

/// One bar the section departs from its groove in.
#[derive(Facet)]
struct Departure {
    bar: i32,
    /// Hits in no slot: `Snare 3.62` (the piece, the beat it plays on).
    extra: Vec<String>,
    /// Slots not played: `Kick 2a`.
    missing: Vec<String>,
}

/// A section's groove.
#[derive(Facet)]
struct GrooveOut {
    name: String,
    /// Bars in a cycle of it: 1, or 2.
    cycle_bars: i32,
    cycles: u32,
    /// The share of the section's hits a slot places, and of the slots'
    /// bars that play them.
    placed: f64,
    regular: f64,
    pieces: Vec<PieceGroove>,
    departures: Vec<Departure>,
}

/// What `DRUMS_GROOVE` says of one span.
fn groove_out(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (kit, triplets): (&[KitPiece], bool),
    (name, from, to): (String, f64, f64),
) -> Result<GrooveOut, String> {
    let found = groove_of(daw, project, (kit, triplets), (from, to))
        .ok_or_else(|| format!("{name}: too few bars of hits to learn a groove from"))?;
    let g = &found.groove;
    let beat_len = 60.0 / daw.get_tempo_at(project.clone(), from.max(0.0)).max(1.0);
    let ms = |beats: f64| (beats * beat_len * 10_000.0).round() / 10.0;
    let name_of = |piece: usize| kit.get(piece).map_or_else(String::new, |p| p.0.clone());
    let pieces = (0..kit.len())
        .filter_map(|piece| {
            let mut slots: Vec<&crate::groove::Slot> =
                g.slots.iter().filter(|s| s.piece == piece).collect();
            slots.sort_by(|a, b| a.line.total_cmp(&b.line));
            (!slots.is_empty()).then(|| PieceGroove {
                piece: name_of(piece),
                slots: slots
                    .into_iter()
                    .map(|s| SlotOut {
                        at: crate::groove::count(s.line, g.beats, g.cycle_bars),
                        played: s.cycles,
                        of: g.cycles,
                        feel_ms: ms(s.feel),
                        spread_ms: ms(s.spread),
                    })
                    .collect(),
            })
        })
        .collect();
    let hits: Vec<crate::groove::Hit> = found.hits.iter().map(|h| h.0).collect();
    let departures = g
        .departures(&hits)
        .into_iter()
        .map(|(cycle, extra, missing)| Departure {
            bar: cycle
                .saturating_mul(g.cycle_bars)
                .saturating_add(g.first_bar),
            extra: extra
                .iter()
                .map(|h| format!("{} {}:{:.2}", name_of(h.piece), h.bar, h.beat + 1.0))
                .collect(),
            missing: missing
                .iter()
                .map(|s| {
                    format!(
                        "{} {}",
                        name_of(s.piece),
                        crate::groove::count(s.line, g.beats, g.cycle_bars)
                    )
                })
                .collect(),
        })
        .collect();
    Ok(GrooveOut {
        name,
        cycle_bars: g.cycle_bars,
        cycles: g.cycles,
        placed: (g.placed * 100.0).round() / 100.0,
        regular: (g.regular * 100.0).round() / 100.0,
        pieces,
        departures,
    })
}

#[derive(Facet)]
pub struct QuantizeArg {
    /// Settings for this quantize alone.
    #[facet(flatten, default)]
    pub with: crate::drum_settings::Patch,
    #[facet(default)]
    pub sections: Vec<String>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub dry_run: Option<bool>,
    /// Lock this piece's hits in order to the grid (`Kick` for a double
    /// kick): each the line after the last, the kit moved with them;
    /// moments without it stay where they were played.
    pub lock: Option<String>,
    /// Leave out each span's lists of moves and hits to check: the
    /// counts, the groove's calls and the check stay.
    pub brief: Option<bool>,
    /// Several spans, each with settings of its own over the call's —
    /// the verses on eighths, a fill on triplets, a double kick locked —
    /// edited as one slip, every seam closed with its neighbours.
    #[facet(default)]
    pub plan: Vec<PlanStep>,
}

/// How many of a span's groove and MIDI calls a brief answer keeps:
/// `DRUMS_REVIEW` holds every hit up to the chart.
const BRIEF_CALLS: usize = 3;

/// One step of a quantize's plan.
#[derive(Facet)]
pub struct PlanStep {
    /// Settings for this step alone, over the call's.
    #[facet(flatten, default)]
    pub with: crate::drum_settings::Patch,
    #[facet(default)]
    pub sections: Vec<String>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub lock: Option<String>,
}

/// What a quantize did over one stretch.
#[derive(Facet)]
pub struct QuantizeSpan {
    /// The section's name, or `from`–`to`.
    pub name: String,
    pub from: f64,
    pub to: f64,
    pub result: SlipResult,
    /// With `target: Pattern`: the hits the groove sent to a line other
    /// than the nearest — `bar`/`beat` where each was played, `line` the
    /// groove's (counted as a drummer counts) and `nearest` the line it
    /// was nearest. The decisions to listen to.
    pub by_groove: Vec<GrooveCall>,
    /// The hits the drum MIDI sent to a line other than the nearest: its
    /// piece's note there, the feel taken out — a hit pushed past the
    /// line before its own.
    pub by_written: Vec<GrooveCall>,
    /// How the edit left the hits (none for a dry run).
    pub check: Option<crate::hits::Check>,
}

/// One hit the groove placed against the nearest line.
#[derive(Facet)]
pub struct GrooveCall {
    time: f64,
    bar: i32,
    beat: f64,
    line: String,
    nearest: String,
    /// How far it moves, ms.
    shift_ms: f64,
}

/// How a quantize's edit of `span` left the hits ([`crate::hits::check`]),
/// each moved hit held up to where it was sent: its target, or as far
/// toward it as the strength takes it.
fn checked(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (settings, pairs): (&crate::drum_settings::Settings, &[(f64, Option<f64>)]),
    before: &[crate::hits::Played],
    span: (f64, f64),
) -> crate::hits::Check {
    let tolerance = settings.tolerance_ms / 1_000.0;
    let sent: Vec<f64> = pairs
        .iter()
        .filter_map(|(h, t)| {
            let t = (*t)?;
            ((t - h).abs() > tolerance).then(|| (t - h).mul_add(settings.strength, *h))
        })
        .collect();
    crate::hits::check(daw, project, before, &sent, span)
}

/// The groove's calls against the nearest line in `span`, as a model
/// reads them.
fn groove_calls(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    calls: &[(f64, f64, f64)],
    (from, to): (f64, f64),
) -> Vec<GrooveCall> {
    calls
        .iter()
        .filter(|(h, _, _)| *h >= from && *h < to)
        .map(|&(h, line, nearest)| {
            let (bar, beat) = in_bar(daw, project, h);
            GrooveCall {
                time: h,
                bar,
                beat: beat + 1.0,
                line: counted(daw, project, line),
                nearest: counted(daw, project, nearest),
                shift_ms: ((line - h) * 10_000.0).round() / 10.0,
            }
        })
        .collect()
}

/// `t` seconds counted as a drummer counts it in its bar: `2a`.
fn counted(daw: &daw::standalone::Standalone, project: &ProjectContext, t: f64) -> String {
    let (bar, beat) = in_bar(daw, project, t + 1e-6);
    let next = crate::song_bars::time(daw, project, bar.saturating_add(1), 1, 0.0);
    let (_, last) = in_bar(daw, project, next - 1e-6);
    crate::groove::count(beat, last.ceil().max(1.0), 1)
}

/// The stretches a quantize covers: each named section (as the SECTIONS
/// lane has it), or the span given.
fn quantize_spans(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    a: &QuantizeArg,
) -> Result<Vec<(String, f64, f64)>, String> {
    spans(daw, project, &a.sections, (a.from, a.to))
}

/// The stretches `sections` name (by name, from the SECTIONS lane), or
/// else the one from `from` to `to` (the song's start, its end).
pub fn spans(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    sections: &[String],
    (from, to): (Option<f64>, Option<f64>),
) -> Result<Vec<(String, f64, f64)>, String> {
    if sections.is_empty() {
        let (from, to) = (from.unwrap_or(0.0), to.unwrap_or(f64::INFINITY));
        return Ok(vec![(format!("{from:.2}–{to:.2}"), from, to)]);
    }
    let regions: Vec<daw_proto::Region> = daw::service::Regions::all(daw, project.clone())
        .into_iter()
        .filter(|r| {
            r.lane
                .is_none_or(|l| l == session::ruler_lanes::CoreLane::Sections.lane_index())
        })
        .collect();
    sections
        .iter()
        .map(|want| {
            regions
                .iter()
                .find(|r| r.name.trim().eq_ignore_ascii_case(want.trim()))
                .map(|r| {
                    (
                        r.name.clone(),
                        r.time_range.start_seconds(),
                        r.time_range.end_seconds(),
                    )
                })
                .ok_or_else(|| {
                    format!(
                        "no section {want}: one of {:?}",
                        regions.iter().map(|r| &r.name).collect::<Vec<_>>()
                    )
                })
        })
        .collect()
}

/// The longest an edit queued for the window is waited for, seconds: a
/// whole song's slip (thousands of edits) included.
const SLIP_LANDS: u64 = 30;

/// Wait until the window has made the edit `ticket` was queued with and
/// the engine has applied every edit of it ([`crate::engine::Edit::Done`]).
/// No polling: woken the moment it lands.
pub fn landed(ticket: u64) {
    if !crate::engine::wait_done(ticket, std::time::Duration::from_secs(SLIP_LANDS)) {
        tracing::warn!(
            edit.ticket = ticket,
            "an edit queued for the window did not land in time"
        );
    }
}

/// The moments' targets with `piece` locked in order over `span`: a
/// moment holding one of its hits goes where that hit is locked to (the
/// moment's other hits with it); any other moment has none, and is cut
/// where it was played. Gestures held whole.
fn locked_targets(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    found: &Detected,
    (piece, (from, to)): (&str, (f64, f64)),
) -> Result<Vec<(f64, Option<f64>)>, String> {
    let hits: Vec<f64> = found
        .kit
        .iter()
        .find(|(name, _, _)| name.eq_ignore_ascii_case(piece))
        .ok_or_else(|| format!("no piece {piece} to lock"))?
        .2
        .iter()
        .map(|h| h.time)
        .filter(|t| *t >= from && *t < to)
        .collect();
    let mid = f64::midpoint(from, to.min(from + 600.0));
    let step =
        240.0 / daw.get_tempo_at(project.clone(), mid).max(1.0) / f64::from(settings.grid.max(1));
    // Anchors: the locked piece's hits played with the snare (on 2 and
    // 4, the hands keeping time), on the snare's line.
    let others: Vec<f64> = found
        .kit
        .iter()
        .filter(|(name, _, _)| !name.eq_ignore_ascii_case(piece))
        .filter(|(name, _, _)| name.eq_ignore_ascii_case("snare"))
        .flat_map(|(_, _, h)| h.iter().map(|h| h.time))
        .collect();
    // And where each run starts, after a rest: the first hit of it is
    // where it is meant to begin (a double kick's downbeat).
    let mut anchors: Vec<(usize, f64)> = hits
        .iter()
        .enumerate()
        .filter_map(|(i, h)| {
            let starts = i == 0
                || hits
                    .get(i.saturating_sub(1))
                    .is_some_and(|p| h - p > RUN_BREAK * step);
            others
                .iter()
                .find(|o| anchor_of(&hits, **o, step) == Some(i))
                .map(|o| (i, grid_line(daw, project, *o, settings.grid)))
                .or_else(|| starts.then(|| (i, grid_line(daw, project, *h, settings.grid))))
        })
        .collect();
    // An anchor at or before the one before it says nothing usable.
    let mut last_line = f64::NEG_INFINITY;
    anchors.retain(|(_, line)| {
        let keep = *line > last_line;
        if keep {
            last_line = *line;
        }
        keep
    });
    let locked = lock_anchored(&hits, &anchors, step, |h| {
        grid_line(daw, project, h, settings.grid)
    });
    let raw: Vec<(f64, Option<f64>)> = found
        .moments
        .iter()
        .map(|m| {
            let mine = m.pieces.iter().any(|p| p.eq_ignore_ascii_case(piece));
            let to = mine
                .then(|| {
                    locked
                        .iter()
                        .filter(|(h, _)| (h - m.time).abs() <= KIT_MERGE)
                        .min_by(|a, b| (a.0 - m.time).abs().total_cmp(&(b.0 - m.time).abs()))
                        .map(|(h, t)| m.time + (t - h))
                })
                .flatten();
            (m.time, to)
        })
        .collect();
    let loudness = weights(&found.moments, settings);
    Ok(hold_gestures(&uncrossed(raw), &loudness))
}

// r[impl flow.drums.editing.quantize]
// r[impl quantize.reads-markers]
// r[impl quantize.check]
/// One step of a quantize: its settings, its spans (`(name, from, to)`)
/// and the piece it locks, if any.
type PlanOf<'a> = (
    crate::drum_settings::Settings,
    Vec<(String, f64, f64)>,
    Option<&'a str>,
);

/// What one span's quantize would move.
struct Planned {
    /// `(hit, shift)`, microseconds: its moves, and the hits cut in place.
    cuts: Vec<(i64, i64)>,
    /// Each hit and where it is sent, strength applied in the check.
    sent: Vec<(f64, Option<f64>)>,
}

/// One span of a quantize: its targets, by `settings` — or `lock`ed, a
/// double kick — what it would move, and what it says; carried out with
/// the rest of the call's spans in one slip.
fn quantize_span(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    (name, from, to): (&str, f64, f64),
    (lock, dry): (Option<&str>, bool),
    prepared: &Prepared,
) -> Result<(QuantizeSpan, Planned), String> {
    let mut stages = crate::stages::Stages::start();
    let zones = &prepared.zones;
    stages.mark("span.zones_ms");
    let found = detect(daw, project, settings, (from, to), prepared)?;
    stages.mark("span.detect_ms");
    // A hit half a sixteenth before the downbeat is this section's pushed
    // first hit; one that close before the end is the next's.
    let (lo, hi) = (from - edge(daw, project, from), to - edge(daw, project, to));
    let inside = |h: f64| h >= lo && h < hi;
    let in_fill = |h: f64| {
        if zones.is_empty() {
            in_a_fill(daw, project, &found.fills, h)
        } else {
            // On the same edge as every other zone here: a hit half a
            // sixteenth before a fill's end is the next bar's downbeat. A
            // narrower edge held the kick 43 ms before 113.1 as the fill's
            // while the fill's grid left it to the bar — nobody moved it.
            zones.iter().any(|z| {
                z.origin != crate::hits::ZoneOrigin::Rejected
                    && h >= z.start - edge(daw, project, z.start)
                    && h < z.end - edge(daw, project, z.end)
            })
        }
    };
    // What stays as played: the fills, unless they are asked for (a fill
    // that keeps a grid — a run of 16ths, a triplet fill — is edited on
    // its own grid); with only the fills asked for, everything else.
    // A fill the drum MIDI writes — most of its hits its notes — is
    // edited to them like the rest of the section: only what the chart
    // does not have is the drummer's own, and left as played.
    let written_fills: Vec<&crate::hits::Zone> = zones
        .iter()
        .filter(|z| {
            z.kind == crate::hits::ZoneKind::Fill && z.origin != crate::hits::ZoneOrigin::Rejected
        })
        .filter(|z| {
            let shift = edge(daw, project, z.start);
            let (inside, written) = found
                .moments
                .iter()
                .zip(&found.placed_by)
                .filter(|(m, _)| m.time >= z.start - shift && m.time < z.end - shift)
                .fold((0_usize, 0_usize), |(n, w), (_, by)| {
                    (
                        n.saturating_add(1),
                        w.saturating_add(usize::from(*by == Placed::Written)),
                    )
                });
            inside >= 3 && crate::drum_reference::share(written, inside) >= WRITTEN_FILL
        })
        .collect();
    let in_written_fill = |h: f64| {
        written_fills.iter().any(|z| {
            h >= z.start - edge(daw, project, z.start) && h < z.end - edge(daw, project, z.end)
        })
    };
    // A no-quantize zone (a stretch put back as played, to edit by hand)
    // is never quantized, whatever is asked of the fills.
    let excepted = |h: f64| {
        zones.iter().any(|z| {
            z.kind == crate::hits::ZoneKind::Exception
                && z.origin != crate::hits::ZoneOrigin::Rejected
                && h >= z.start - edge(daw, project, z.start)
                && h < z.end - edge(daw, project, z.end)
        })
    };
    let overrides = &prepared.overrides;
    let held = |h: f64| {
        excepted(h)
            || match settings.fills {
                crate::drum_settings::Fills::Skip => {
                    in_fill(h)
                        && !in_written_fill(h)
                        && !overrides.figure_over_fill(daw, project, h)
                        && overrides.fill_grid(daw, project, h).is_none()
                }
                crate::drum_settings::Fills::Include => false,
                crate::drum_settings::Fills::Only => !in_fill(h),
            }
    };
    let mut targets = match lock {
        Some(piece) => locked_targets(daw, project, settings, &found, (piece, (from, to)))?,
        None => found.targets.clone(),
    };
    stages.mark("span.targets_ms");
    // In a written fill, a hit the chart does not have stays as played.
    if settings.fills == crate::drum_settings::Fills::Skip {
        for ((h, to), by) in targets.iter_mut().zip(&found.placed_by) {
            if *by != Placed::Written && in_written_fill(*h) {
                *to = None;
            }
        }
    }
    let pairs: Vec<(f64, Option<f64>)> = targets
        .iter()
        .filter(|(h, _)| inside(*h) && !held(*h))
        .copied()
        .collect();
    let mut left_out: Vec<f64> = found
        .targets
        .iter()
        .filter(|(h, _)| inside(*h) && held(*h))
        .map(|(h, _)| *h)
        .collect();
    let in_fills = u32::try_from(left_out.len()).unwrap_or(u32::MAX);
    // r[impl quantize.stays-in-span]
    // The first hit after the span cut where it is: else the slice holding
    // the span's last moved hit runs on to the take's end, and the rest of
    // the song moves with it.
    if let Some(after) = found.targets.iter().map(|(h, _)| *h).find(|h| *h >= hi) {
        left_out.push(after);
    }
    stages.mark("span.held_ms");
    let (mut result, cuts) = judge(daw, project, settings, (&pairs, &left_out), dry);
    stages.mark("span.judge_ms");
    if settings.fills == crate::drum_settings::Fills::Skip {
        result.in_fills = in_fills;
    }
    Ok((
        QuantizeSpan {
            name: name.to_owned(),
            from,
            to,
            result,
            by_groove: groove_calls(daw, project, &found.by_groove, (lo, hi)),
            by_written: groove_calls(
                daw,
                project,
                &found
                    .by_written
                    .iter()
                    .filter(|(h, _, _)| !held(*h))
                    .copied()
                    .collect::<Vec<_>>(),
                (lo, hi),
            ),
            check: None,
        },
        Planned { cuts, sent: pairs },
    ))
}

static PLACE: ActionMeta = meta(
    "HITS_PLACE",
    "hits_place",
    "Place a drum hit",
    "Move the hit `id` so it plays at `to` (project seconds, as the song plays now) or `by_ms` later: the whole kit cut a lead before it and moved together — every mic, in phase — the hits either side left where they play, the seams crossfaded. `snap` (the default) puts it on the nearest thirty-second or sixteenth triplet within 12 ms of where it is dropped. A hand fixing one hit the quantize placed wrong — what dragging a hit's flag does. Says the check, as QUANTIZE does.",
    "Drums",
    "",
);

/// Which hit, and where.
#[derive(Facet)]
struct PlaceArg {
    id: Option<String>,
    /// Several hits, moved together (a fill taken earlier): `to`/`by_ms`
    /// say where the earliest goes.
    #[facet(default)]
    ids: Vec<String>,
    to: Option<f64>,
    by_ms: Option<f64>,
    snap: Option<bool>,
}

/// How near a line a dropped hit snaps to it, seconds.
const PLACE_SNAP: f64 = 0.012;

static STRETCH: ActionMeta = meta(
    "HITS_STRETCH",
    "hits_stretch",
    "Stretch a drum hit into place",
    "Move the hit `id` to `to` (project seconds, as the song plays now) or `by_ms` later by stretching, not slipping: on every mic of the kit, in phase, the slice before it drawn out or squeezed to end where it now starts and its own slice squeezed or drawn out to end where it did — the hits either side stay where they play, each attack plays as recorded, the take is played at its own pitch. What dragging a hit marker's foot does. Refused where a slice would play slower than 0.1 or faster than 4 times, or be shorter than its attack. One undo step.",
    "Drums",
    "",
);

/// Which hit, and where.
#[derive(Facet)]
struct StretchArg {
    id: String,
    to: Option<f64>,
    by_ms: Option<f64>,
}

/// What a stretch did.
#[derive(Facet)]
struct Stretched {
    /// Where the hit plays now, project seconds.
    at: f64,
    /// The slices stretched, two a mic.
    slices: u32,
}

/// How near its slice's attack a hit is that slice's, seconds.
const OWN_SLICE: f64 = 0.004;

/// Every kit mic's two slices around the hit at `hit`, stretched to move
/// it `by` seconds ([`crate::stretch::stretch_around`]).
///
/// # Errors
/// A mic not cut at this hit, with no slice before it, or past what a
/// stretch can do.
fn stretch_edits(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    hit: f64,
    by: f64,
) -> Result<Vec<(String, crate::stretch::Warped)>, String> {
    use crate::stretch::Warped;
    let lead = crate::drum_settings::current().lead_ms / 1_000.0;
    let mut edits: Vec<(String, Warped)> = Vec::new();
    for track in crate::drum_lanes::kit_tracks(daw, project) {
        let mut items: Vec<daw_proto::Item> = daw::service::Items::get_items(
            daw,
            project.clone(),
            TrackRef::Guid(track.guid.clone()),
        )
        .into_iter()
        .filter(|i| !i.muted && crate::drum_edit::is_edited(&track, i))
        .collect();
        // Audio only: a Sample track's notes follow the hit as the slip's
        // do (`drum_samples::follow`), not by their slices.
        let audio = items.first().is_some_and(|i| {
            daw_proto::Takes::get_active_take(
                daw,
                project.clone(),
                daw_proto::ItemRef::Guid(i.guid.clone()),
            )
            .is_some_and(|t| t.source_file_path.is_some())
        });
        if !audio {
            continue;
        }
        items.sort_by(|x, y| x.position.as_seconds().total_cmp(&y.position.as_seconds()));
        // How far into a slice its hit is: its snap offset, else the lead.
        let attack = |i: &daw_proto::Item| {
            let snap = i.snap_offset.as_seconds();
            if snap > 0.0 { snap } else { lead }
        };
        let k = items
            .iter()
            .position(|i| (i.position.as_seconds() + attack(i) - hit).abs() <= OWN_SLICE)
            .ok_or_else(|| {
                format!(
                    "{} is not cut at this hit: place it (HITS_PLACE) or quantize first",
                    track.name
                )
            })?;
        let (Some(before), Some(own)) = (k.checked_sub(1).and_then(|p| items.get(p)), items.get(k))
        else {
            return Err(format!(
                "{}: no slice before this hit to stretch",
                track.name
            ));
        };
        let warped = |i: &daw_proto::Item| {
            let markers: Vec<(f64, f64)> = daw_proto::StretchMarkers::get_stretch_markers(
                daw,
                project.clone(),
                daw_proto::ItemRef::Guid(i.guid.clone()),
                daw_proto::TakeRef::Active,
            )
            .into_iter()
            .map(|m| (m.position, m.source_position))
            .collect();
            let (offset, length) = (i.start_offset.as_seconds(), i.length.as_seconds());
            Warped {
                position: i.position.as_seconds(),
                length,
                markers: if markers.len() > 1 {
                    markers
                } else {
                    vec![(0.0, offset), (length, offset + length)]
                },
            }
        };
        let (drawn, moved) = crate::stretch::stretch_around(
            &warped(before),
            &warped(own),
            (attack(before), attack(own)),
            by,
        )
        .ok_or_else(|| {
            format!(
                "{}: that far is past what a stretch can do (0.1–4 times, the attack kept)",
                track.name
            )
        })?;
        edits.push((before.guid.clone(), drawn));
        edits.push((own.guid.clone(), moved));
    }
    Ok(edits)
}

fn register_stretch(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&STRETCH, Effect::Mutating, |a: StretchArg| {
        let (daw, project) = song()?;
        let hit = crate::hits::playback(daw, &project)
            .into_iter()
            .find(|p| p.id == a.id)
            .ok_or_else(|| format!("{} plays nowhere now", a.id))?
            .time;
        let to = match (a.to, a.by_ms) {
            (Some(to), _) => to,
            (None, Some(ms)) => hit + ms / 1_000.0,
            (None, None) => return Err("say where: `to` or `by_ms`".to_owned()),
        };
        let by = to - hit;
        let edits = stretch_edits(daw, &project, hit, by)?;
        if edits.is_empty() {
            return Err("no kit track to stretch".to_owned());
        }
        // The samples move with the hit, as they do with a slip.
        let moved = |t: f64| {
            if (t - hit).abs() <= OWN_SLICE {
                t + by
            } else {
                t
            }
        };
        if let Err(why) =
            crate::hits_midi::sync(daw, &project, crate::hits_midi::Mode::Follow, &moved)
        {
            tracing::warn!(why, "drums: the samples could not follow the stretch");
        }
        crate::hits::mark_placed(daw, &project, &a.id);
        let us = |s: f64| num_traits::ToPrimitive::to_i64(&(s * 1_000_000.0).round()).unwrap_or(0);
        let slices = u32::try_from(edits.len()).unwrap_or(u32::MAX);
        let ticket = crate::engine::ticket();
        crate::window_queue::ask(crate::window_queue::Asked::StretchDrums {
            ticket,
            edits: edits
                .into_iter()
                .map(|(guid, w)| {
                    let markers = w.markers.iter().map(|(p, s)| (us(*p), us(*s))).collect();
                    (guid, us(w.position), us(w.length), markers)
                })
                .collect(),
        });
        landed(ticket);
        Ok(Stretched { at: to, slices })
    })?;
    Ok(())
}

static REVERT: ActionMeta = meta(
    "DRUMS_REVERT",
    "drums_revert",
    "Put the drums back as played",
    "Undo the drum edit over a stretch — `from`/`to` (seconds, as the song plays now) or `sections` — without undoing anything else: every hit there moved back to where it was played, the whole kit together, the seams crossfaded as a quantize closes them, so the stretch plays as recorded. The hit markers stay, to drag (HITS_PLACE) or quantize again. With `hold` (the default) a no-quantize zone goes over it, so a later quantize leaves it as played; delete the zone (ZONES_REMOVE) to let one in. Says the check, as QUANTIZE does.",
    "Drums",
    "",
);

/// Which stretch to put back.
#[derive(Facet)]
pub struct RevertArg {
    #[facet(default)]
    pub sections: Vec<String>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub hold: Option<bool>,
}

/// The kit's moments in `lo..hi` (`moments`, as they play now — the hits
/// a quantize cuts on), each with where it was played: `(now, as
/// recorded)`, read off the hit it starts with.
fn as_played(
    moments: &[f64],
    rows: &[crate::hits::HitRow],
    (lo, hi): (f64, f64),
) -> Vec<(f64, Option<f64>)> {
    let mut played: Vec<(f64, f64)> = rows
        .iter()
        .filter_map(|r| r.now.map(|n| (n, r.time)))
        .collect();
    played.sort_by(|a, b| a.0.total_cmp(&b.0));
    moments
        .iter()
        .filter(|m| **m >= lo && **m < hi)
        .filter_map(|m| {
            let at = played.partition_point(|(n, _)| *n < m - 1e-6);
            played
                .get(at)
                .filter(|(n, _)| (n - m).abs() <= 1e-6)
                .map(|(n, was)| (*n, Some(*was)))
        })
        .collect()
}

// r[impl hits.revert]
/// `DRUMS_REVERT`: the kit put back as played over `a`'s spans — see the
/// action's description. What `EDIT_REVERT` runs for the drums.
///
/// # Errors
///
/// A section that is not there, a zone that would not save, or no engine.
pub fn revert(a: RevertArg) -> Result<Vec<QuantizeSpan>, String> {
    let (daw, project) = song()?;
    let settings = crate::drum_settings::current();
    let mut out: Vec<QuantizeSpan> = Vec::new();
    let before = crate::hits::playback(daw, &project);
    let rows = crate::hits::hits(daw, &project, &[]);
    let moments: Vec<f64> = moments(&kit(daw, &project), None)
        .iter()
        .map(|m| m.time)
        .collect();
    let mut cuts: Vec<(i64, i64)> = Vec::new();
    let mut sent: Vec<(f64, Option<f64>)> = Vec::new();
    let mut whole = (f64::INFINITY, f64::NEG_INFINITY);
    for (name, from, to) in spans(daw, &project, &a.sections, (a.from, a.to))? {
        let (lo, hi) = (
            from - edge(daw, &project, from),
            to - edge(daw, &project, to),
        );
        let pairs = as_played(&moments, &rows, (lo, hi));
        // The first hit after it stays where it plays: the rest of
        // the song does not move.
        let after: Vec<f64> = moments
            .iter()
            .copied()
            .find(|t| *t >= hi)
            .into_iter()
            .collect();
        let (result, planned) = judge(daw, &project, &settings, (&pairs, &after), false);
        cuts.extend(planned);
        sent.extend(pairs);
        whole = (whole.0.min(lo), whole.1.max(hi));
        if a.hold.unwrap_or(true) {
            crate::hits::add_zone(
                daw,
                &project,
                (from, to),
                (
                    crate::hits::ZoneKind::Exception,
                    crate::hits::ZoneOrigin::Manual,
                    "put back as played: edit by hand",
                ),
            )?;
        }
        out.push(QuantizeSpan {
            name,
            from,
            to,
            result,
            by_groove: Vec::new(),
            by_written: Vec::new(),
            check: None,
        });
    }
    if cuts.iter().any(|c| c.1 != 0) {
        apply_cuts(daw, &project, &settings, cuts)?;
        let check = checked(daw, &project, (&settings, &sent), &before, whole);
        if let Some(last) = out.last_mut() {
            last.check = Some(check);
        }
    }
    for span in &mut out {
        span.result.biggest.clear();
        span.result.to_check.clear();
    }
    crate::hit_overlay::refresh();
    Ok(out)
}

fn register_revert(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&REVERT, Effect::Mutating, revert)?;
    Ok(())
}

// r[impl hits.place]
fn register_place(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&PLACE, Effect::Mutating, |a: PlaceArg| {
        let (daw, project) = song()?;
        let mut stages = crate::stages::Stages::start();
        let before = crate::hits::playback(daw, &project);
        stages.mark("edit.read_ms");
        let ids: Vec<String> = a.id.iter().cloned().chain(a.ids.iter().cloned()).collect();
        let chosen: Vec<f64> = before
            .iter()
            .filter(|p| ids.contains(&p.id))
            .map(|p| p.time)
            .collect();
        if chosen.is_empty() {
            return Err(format!("none of {ids:?} plays now: say `id` or `ids`"));
        }
        let earliest = chosen.iter().copied().fold(f64::INFINITY, f64::min);
        let latest = chosen.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut to = match (a.to, a.by_ms) {
            (Some(to), _) => to,
            (None, Some(ms)) => earliest + ms / 1_000.0,
            (None, None) => return Err("say where: `to` or `by_ms`".to_owned()),
        };
        if a.snap.unwrap_or(true) {
            let line = [32, 24]
                .into_iter()
                .map(|division| grid_line(daw, &project, to, division))
                .min_by(|x, y| (x - to).abs().total_cmp(&(y - to).abs()));
            if let Some(line) = line.filter(|l| (l - to).abs() <= PLACE_SNAP) {
                to = line;
            }
        }
        let shift = to - earliest;
        let settings = crate::drum_settings::current();
        // Every moment of the kit from the earliest hit to the latest moves by
        // as much, together — a fill taken earlier whole — and the
        // moments either side stay where they play.
        let kit = kit(daw, &project);
        let times: Vec<f64> = moments(&kit, None).iter().map(|m| m.time).collect();
        let group: Vec<(f64, Option<f64>)> = times
            .iter()
            .filter(|t| **t >= earliest - KIT_MERGE && **t <= latest + KIT_MERGE)
            .map(|t| (*t, Some(*t + shift)))
            .collect();
        let pairs: Vec<(f64, Option<f64>)> = if group.is_empty() {
            vec![(earliest, Some(to))]
        } else {
            group
        };
        let lo = times.partition_point(|t| *t < earliest - KIT_MERGE);
        let hi = times.partition_point(|t| *t <= latest + KIT_MERGE);
        let neighbours: Vec<f64> = [lo.checked_sub(1), Some(hi)]
            .into_iter()
            .flatten()
            .filter_map(|i| times.get(i).copied())
            .collect();
        stages.mark("edit.kit_ms");
        let (result, cuts) = judge(daw, &project, &settings, (&pairs, &neighbours), false);
        stages.mark("edit.judge_ms");
        let span = (earliest.min(to) - 1.0, (latest + shift).max(latest) + 1.0);
        if result.applied {
            for id in &ids {
                crate::hits::mark_placed(daw, &project, id);
            }
            apply_cuts(daw, &project, &settings, cuts)?;
            stages.mark("edit.place_ms");
        }
        let out = checked(daw, &project, (&settings, &pairs), &before, span);
        stages.mark("edit.check_ms");
        Ok(out)
    })?;
    Ok(())
}

static REQUANTIZE: ActionMeta = meta(
    "DRUMS_REQUANTIZE",
    "drums_requantize",
    "Start the drum edit over and quantize it",
    "Clear every drum edit (the EDIT lanes copied from COMP again) and quantize the whole song fresh, as QUANTIZE does with `plan` (each step its sections or span and settings — the snare leading in a breakdown) — the plan kept with the song, so a call without one uses the last given (none: the whole song on the sticky settings). The parts', figures' and fills' grids and the zones are read as always. `keep_manual`: the hits a hand placed (HITS_PLACE) are put back where they were placed afterwards. Says the quantize's check and how many hand placements were put back.",
    "Drums",
    "",
);

/// How to start the drum edit over.
#[derive(Facet)]
struct RequantizeArg {
    keep_manual: Option<bool>,
    #[facet(default)]
    plan: Vec<PlanStep>,
}

/// What starting the drum edit over did.
#[derive(Facet)]
struct Requantized {
    /// The spans quantized and the hits moved.
    spans: u32,
    moved: u32,
    /// The check over the whole song: none lost, none doubled, no gap.
    ok: bool,
    problems: Vec<String>,
    /// Hand placements put back (`keep_manual`).
    placed_back: u32,
}

/// Where the plan a whole-song quantize was last given is kept: on the
/// kit's folder, with the song.
const PLAN_KEY: &str = "fts.quantize_plan";

fn register_requantize(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&REQUANTIZE, Effect::Mutating, |a: RequantizeArg| {
        let (daw, project) = song()?;
        let folder = crate::track_regions::kit_folder(daw, &project).ok_or("no kit")?;
        let at = daw::service::TrackRef::Guid(folder);
        // The plan: the one given, kept; else the one kept.
        let plan = if a.plan.is_empty() {
            Tracks::ext_state(daw, project.clone(), at)
                .into_iter()
                .find(|e| e.key == PLAN_KEY)
                .map_or_else(|| "[]".to_owned(), |e| e.value)
        } else {
            let text = facet_json::to_string(&a.plan).map_err(|e| e.to_string())?;
            Tracks::set_ext_state(daw, project.clone(), at, PLAN_KEY, &text)
                .map_err(|e| e.to_string())?;
            text
        };
        // Where the hands put each hit they placed, to put back.
        let keep: Vec<(String, f64)> = if a.keep_manual.unwrap_or(false) {
            let placed = crate::hits::placed(daw, &project);
            crate::hits::playback(daw, &project)
                .into_iter()
                .filter(|p| placed.contains(&p.id))
                .map(|p| (p.id, p.time))
                .collect()
        } else {
            Vec::new()
        };
        crate::drum_lanes::reset_edit(daw, &project, false)?;
        let said = self::list()
            .run_json(
                QUANTIZE.id,
                format!(r#"{{"brief":true,"plan":{plan}}}"#).as_bytes(),
            )
            .map_err(|e| e.to_string())?;
        let spans: Vec<serde_json::Value> = serde_json::from_slice(&said).unwrap_or_default();
        let moved = spans
            .iter()
            .filter_map(|s| s.get("result")?.get("moved")?.as_u64())
            .sum::<u64>();
        let check = spans
            .iter()
            .rev()
            .find_map(|s| s.get("check").filter(|c| !c.is_null()));
        let mut placed_back = 0_u32;
        for (id, to) in keep {
            let args = format!(r#"{{"id":"{id}","to":{to},"snap":false}}"#);
            if self::list().run_json(PLACE.id, args.as_bytes()).is_ok() {
                placed_back = placed_back.saturating_add(1);
            }
        }
        Ok(Requantized {
            spans: u32::try_from(spans.len()).unwrap_or(u32::MAX),
            moved: u32::try_from(moved).unwrap_or(u32::MAX),
            ok: check
                .is_some_and(|c| c.get("ok").and_then(serde_json::Value::as_bool) == Some(true)),
            problems: check
                .and_then(|c| c.get("problems")?.as_array())
                .map(|p| {
                    p.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            placed_back,
        })
    })?;
    Ok(())
}

/// `QUANTIZE`: the kit, over `a`'s spans or plan, from its hit markers —
/// see the action's description. What `EDIT_QUANTIZE` runs for the drums.
///
/// # Errors
///
/// No hit markers, a section that is not there, or no engine.
pub fn quantize(a: QuantizeArg) -> Result<Vec<QuantizeSpan>, String> {
    let (daw, project) = song()?;
    let mut stages = crate::stages::Stages::start();
    if crate::hits::kit_from_markers(daw, &project).is_none() {
        return Err("no hit markers: find the hits first (HITS_DETECT)".to_owned());
    }
    let base = crate::drum_settings::current().with(&a.with);
    let dry = a.dry_run.unwrap_or(false);
    // The steps: the plan's, each with its own settings over the
    // call's, or the call itself as one.
    let steps: Vec<PlanOf<'_>> = if a.plan.is_empty() {
        vec![(
            base.clone(),
            quantize_spans(daw, &project, &a)?,
            a.lock.as_deref(),
        )]
    } else {
        a.plan
            .iter()
            .map(|step| {
                Ok((
                    base.with(&step.with),
                    spans(daw, &project, &step.sections, (step.from, step.to))?,
                    step.lock.as_deref(),
                ))
            })
            .collect::<Result<_, String>>()?
    };
    let before = crate::hits::playback(daw, &project);
    let prepared = Prepared::read(daw, &project);
    stages.mark("edit.read_ms");
    let mut out: Vec<QuantizeSpan> = Vec::new();
    let mut cuts: Vec<(i64, i64)> = Vec::new();
    let mut sent: Vec<(f64, Option<f64>)> = Vec::new();
    let mut whole = (f64::INFINITY, f64::NEG_INFINITY);
    for (settings, spans, lock) in &steps {
        for (name, from, to) in spans {
            let (span, planned) = quantize_span(
                daw,
                &project,
                settings,
                (name, *from, *to),
                (*lock, dry),
                &prepared,
            )?;
            if !dry {
                cuts.extend(planned.cuts);
                sent.extend(planned.sent);
            }
            whole = (whole.0.min(span.from), whole.1.max(span.to));
            out.push(span);
        }
    }
    stages.mark("edit.plan_ms");
    if !dry && cuts.iter().any(|c| c.1 != 0) {
        apply_cuts(daw, &project, &base, cuts)?;
        stages.mark("edit.place_ms");
        // One edit, one check: every hit sent, held up to where it
        // was sent; none outside the edit moved.
        let check = checked(daw, &project, (&base, &sent), &before, whole);
        stages.mark("edit.check_ms");
        if let Some(last) = out.last_mut() {
            last.check = Some(check);
        }
        for span in &mut out {
            span.result.applied = true;
        }
    }
    if a.brief.unwrap_or(false) {
        for span in &mut out {
            span.result.biggest.clear();
            span.result.to_check.clear();
            span.by_groove.truncate(BRIEF_CALLS);
            span.by_written.truncate(BRIEF_CALLS);
        }
    }
    Ok(out)
}

fn register_quantize(list: &ActionList) -> Result<(), architect::action::ActionError> {
    register_requantize(list)?;
    register_place(list)?;
    register_stretch(list)?;
    register_revert(list)?;
    list.register(&QUANTIZE, Effect::Mutating, quantize)?;
    list.register(&GROOVE, Effect::ReadOnly, |a: GrooveArg| {
        let (daw, project) = song()?;
        let kit = kit(daw, &project);
        if kit.is_empty() {
            return Err("no hit markers: find the hits first (HITS_DETECT)".to_owned());
        }
        spans(daw, &project, &a.sections, (a.from, a.to))?
            .into_iter()
            .map(|span| groove_out(daw, &project, (&kit, a.triplets.unwrap_or(false)), span))
            .collect::<Result<Vec<GrooveOut>, String>>()
    })?;
    list.register(&ZONES_FIND, Effect::Mutating, |a: SpanArg| {
        let (daw, project) = song()?;
        let kit = kit(daw, &project);
        let span = a.span();
        let fills = find_fills(daw, &project, &kit, span);
        let proposed = crate::hits::find_zones(daw, &project, &fills, span)?;
        Ok(proposed)
    })?;
    Ok(())
}

fn register_drum_settings(list: &ActionList) -> Result<(), architect::action::ActionError> {
    register_quantize(list)?;
    list.register(&EDIT_SETTINGS, Effect::ReadOnly, |(): ()| {
        Ok(crate::drum_settings::current())
    })?;
    list.register(&EDIT_SET, Effect::Mutating, |a: SetArg| {
        if a.reset.unwrap_or(false) {
            crate::drum_settings::reset();
        }
        let settings = crate::drum_settings::set(&a.with);
        crate::actions::list().changed("drums.settings", "the drum edit settings changed");
        Ok(settings)
    })?;
    Ok(())
}

/// Where.
#[derive(Facet)]
struct SpanArg {
    from: Option<f64>,
    to: Option<f64>,
}

impl SpanArg {
    fn span(&self) -> (f64, f64) {
        (
            self.from.unwrap_or(f64::NEG_INFINITY),
            self.to.unwrap_or(f64::INFINITY),
        )
    }
}

/// Settings to keep.
#[derive(Facet)]
struct SetArg {
    #[facet(flatten, default)]
    with: crate::drum_settings::Patch,
    /// Back to the defaults first.
    reset: Option<bool>,
}

/// The moves `pairs` (hit, target) make, and — unless a dry run — the
/// slip queued for the window.
fn judge(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    (pairs, left_out): (&[(f64, Option<f64>)], &[f64]),
    dry: bool,
) -> (SlipResult, Vec<(i64, i64)>) {
    let tolerance = settings.tolerance_ms / 1_000.0;
    let strength = settings.strength;
    let at = |hit: f64, off: f64, shift: f64| {
        let (bar, beat, frac) = crate::song_bars::musical(daw, project, hit);
        SlipMove {
            time: hit,
            bar,
            beat: f64::from(beat) + frac,
            off_ms: off * 1_000.0,
            shift_ms: shift * 1_000.0,
        }
    };
    let to_check: Vec<SlipMove> = pairs
        .iter()
        .filter(|(_, t)| t.is_none())
        .take(50)
        .map(|(h, _)| at(*h, 0.0, 0.0))
        .collect();
    let mut moves: Vec<(f64, f64, f64)> = Vec::new();
    let (mut in_place, mut unmatched) = (0_u32, 0_u32);
    for (hit, target) in pairs {
        match target {
            None => unmatched = unmatched.saturating_add(1),
            Some(t) if (t - hit).abs() <= tolerance => in_place = in_place.saturating_add(1),
            Some(t) => moves.push((*hit, t - hit, (t - hit) * strength)),
        }
    }
    let mut offs: Vec<f64> = moves.iter().map(|m| m.1 * 1_000.0).collect();
    offs.sort_by(|x, y| x.abs().total_cmp(&y.abs()));
    let mut biggest: Vec<&(f64, f64, f64)> = moves.iter().collect();
    biggest.sort_by(|x, y| y.1.abs().total_cmp(&x.1.abs()));
    let us = |s: f64| num_traits::ToPrimitive::to_i64(&(s * 1_000_000.0).round()).unwrap_or(0);
    // Every hit not moved is cut where it is, unmoved: a hit left in a
    // slice rides with it, and the slice before it moving would take it
    // along — off its mark by that hit's shift.
    let cuts: Vec<(i64, i64)> = moves
        .iter()
        .map(|(h, _, shift)| (us(*h), us(*shift)))
        .chain(
            pairs
                .iter()
                .filter(|(h, t)| t.is_none_or(|t| (t - h).abs() <= tolerance))
                .map(|(h, _)| (us(*h), 0))
                .chain(left_out.iter().map(|h| (us(*h), 0))),
        )
        .collect();
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let result = SlipResult {
        hits: count(pairs.len()),
        moved: count(moves.len()),
        in_place,
        unmatched,
        in_fills: 0,
        median_off_ms: offs.get(offs.len() / 2).copied(),
        worst_off_ms: offs.last().copied(),
        applied: !dry && !moves.is_empty(),
        biggest: biggest
            .into_iter()
            .take(30)
            .map(|(hit, off, shift)| at(*hit, *off, *shift))
            .collect(),
        to_check,
    };
    (result, cuts)
}

/// Carry out `cuts` (`(hit, shift)`, microseconds, by hit) as one step,
/// the drum samples following: placed again from the recording on the
/// engine — a hit moved back is the recording again, nothing drawn out
/// unless nothing else fills a gap — or, for a kit without its recording
/// kept on lanes, slipped where it plays by the window.
fn apply_cuts(
    daw: &'static daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    cuts: Vec<(i64, i64)>,
) -> Result<(), String> {
    let mut stages = crate::stages::Stages::start();
    let song = crate::song::Song::current()?;
    stages.mark("cuts.song_ms");
    if !crate::hit_place::ready(&song) {
        landed(slip_now(daw, project, settings, cuts));
        stages.mark("cuts.slip_ms");
        return Ok(());
    }
    follow_samples(daw, project, settings, &cuts);
    stages.mark("cuts.samples_ms");
    let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e6;
    let moves: Vec<(f64, f64)> = cuts
        .iter()
        .map(|&(hit, by)| (seconds(hit), seconds(by)))
        .collect();
    let joins = crate::hit_place::Joins {
        lead: settings.lead_ms / 1_000.0,
        crossfade: settings.xfade_ms / 1_000.0,
    };
    crate::hit_place::place(&song, &moves, joins)?;
    stages.mark("cuts.place_ms");
    crate::song_model::request_resync_items();
    crate::hit_overlay::refresh();
    stages.mark("cuts.refresh_ms");
    Ok(())
}

/// The drum samples moved with `cuts` (`(hit, shift)`, microseconds):
/// each hit's sample where its slice is going.
fn follow_samples(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    cuts: &[(i64, i64)],
) {
    let lead = settings.lead_ms / 1_000.0;
    let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1_000_000.0;
    let slices: Vec<(f64, f64)> = cuts
        .iter()
        .map(|c| (seconds(c.0) - lead, seconds(c.1)))
        .collect();
    let goes_to = |t: f64| t + shift_at(&slices, t);
    match crate::hits_midi::sync(daw, project, crate::hits_midi::Mode::Follow, &goes_to) {
        Ok(s) if s.written > 0 => {
            tracing::info!(
                samples.tracks = s.written,
                "drums: the samples follow the slip"
            );
        }
        Ok(_) => {}
        Err(why) => tracing::warn!(why, "drums: the samples could not follow the slip"),
    }
}

/// Carry out `cuts` (`(hit, shift)`, microseconds, by hit): the window
/// slips the kit by them, as one step, the drum samples following.
fn slip_now(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    settings: &crate::drum_settings::Settings,
    mut cuts: Vec<(i64, i64)>,
) -> u64 {
    let us = |s: f64| num_traits::ToPrimitive::to_i64(&(s * 1_000_000.0).round()).unwrap_or(0);
    cuts.sort_unstable();
    // A hit two spans both cut is moved where either moves it.
    cuts.dedup_by(|later, earlier| {
        if later.0 != earlier.0 {
            return false;
        }
        if earlier.1 == 0 {
            earlier.1 = later.1;
        }
        true
    });
    // The samples first, so they and the recording move in the one step
    // the slip makes: each hit where its slice is going.
    follow_samples(daw, project, settings, &cuts);
    let mut attacks_us: Vec<i64> = crate::hits::playback(daw, project)
        .iter()
        .map(|p| us(p.time))
        .collect();
    attacks_us.sort_unstable();
    let ticket = crate::engine::ticket();
    crate::window_queue::ask(crate::window_queue::Asked::SlipDrums {
        ticket,
        attacks_us,
        moves: cuts,
        pre_roll_us: us(settings.lead_ms / 1_000.0),
        crossfade_us: us(settings.xfade_ms / 1_000.0),
        stretch_over_us: settings.stretch_gaps_ms.map_or(0, |ms| us(ms / 1_000.0)),
    });
    ticket
}

/// Which audio.
#[derive(Facet)]
struct PeaksArg {
    track: String,
    from: f64,
    to: f64,
    points: Option<u32>,
    db: Option<bool>,
}

/// A track's audio as peaks.
#[derive(Facet)]
struct PeaksResult {
    from: f64,
    /// Seconds a point stands for.
    step: f64,
    peaks: Vec<f64>,
}

static TRACK_SAMPLES: ActionMeta = meta(
    "TRACK_SAMPLES",
    "track_samples",
    "A track's samples",
    "A track's audio from `from` to `to` (seconds) as samples: its playing items mixed as they play into it — each on the lane that plays, unmuted, through its volume and fades — before its FX (the engine's track accessor). `rate` (Hz, 8000 unless given) and `channels` (1: mixed to mono, unless given). At most 96 000 values; a longer span is refused. Read-only.",
    "Data",
    "",
);

/// Which stretch of a track, read how.
#[derive(Facet)]
struct TrackSamplesArg {
    track: String,
    from: f64,
    to: f64,
    rate: Option<f64>,
    channels: Option<u32>,
}

/// A track's samples, interleaved.
#[derive(Facet)]
struct TrackSamplesOut {
    from: f64,
    rate: f64,
    channels: u32,
    samples: Vec<f64>,
}

/// The most values `TRACK_SAMPLES` gives.
const MOST_SAMPLES: f64 = 96_000.0;

fn register_peaks(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&TRACK_SAMPLES, Effect::ReadOnly, |a: TrackSamplesArg| {
        use daw_proto::AudioAccessors as _;
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        if a.to <= a.from {
            return Err("`to` must be after `from`".to_owned());
        }
        let rate = a.rate.unwrap_or(8_000.0).clamp(100.0, 192_000.0);
        let channels = a.channels.unwrap_or(1).clamp(1, 8);
        let frames = ((a.to - a.from) * rate).ceil();
        if frames * f64::from(channels) > MOST_SAMPLES {
            return Err(format!(
                "{frames} frames of {channels} channels is past {MOST_SAMPLES} values: a shorter span, a lower `rate`"
            ));
        }
        let accessor = daw
            .create_track_accessor(project, daw::service::TrackRef::Guid(guid))
            .ok_or("the track plays nothing to read")?;
        let data = daw.get_samples(daw_proto::GetSamplesRequest {
            accessor_id: accessor.clone(),
            sample_rate: rate,
            num_channels: channels,
            start_time: a.from,
            num_samples: num_traits::ToPrimitive::to_u32(&frames).unwrap_or(0),
        });
        daw.destroy_accessor(&accessor);
        Ok(TrackSamplesOut {
            from: a.from,
            rate: data.sample_rate,
            channels: data.num_channels,
            samples: data.samples,
        })
    })?;
    list.register(&PEAKS, Effect::ReadOnly, |a: PeaksArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        if a.to <= a.from {
            return Err("`to` must be after `from`".to_owned());
        }
        let points = a.points.unwrap_or(400).clamp(1, 4_000);
        let n = usize::try_from(points).unwrap_or(400);
        let mut peaks = crate::fundamental::envelope(daw, &project, &guid, (a.from, a.to), n);
        if a.db.unwrap_or(false) {
            for p in &mut peaks {
                *p = (20.0 * p.max(1e-6).log10() * 10.0).round() / 10.0;
            }
        } else {
            for p in &mut peaks {
                *p = (*p * 10_000.0).round() / 10_000.0;
            }
        }
        Ok(PeaksResult {
            from: a.from,
            step: (a.to - a.from) / f64::from(points),
            peaks,
        })
    })?;
    Ok(())
}

/// Run `id` with `args` (JSON) where the engine is: this process's list,
/// or — a window driving an engine in another process — that engine's
/// (`crate::studio_engine::link`). A window's own actions always run here.
///
/// # Errors
///
/// The action refused, or the engine did not answer.
#[cfg(feature = "native")]
pub fn run(id: &str, args: &[u8]) -> Result<Vec<u8>, String> {
    // A window's: every action but its own runs on its engine, over its
    // link — in another process or this one alike.
    if crate::view_client::is_linked() && !crate::window_queue::is_window_action(id) {
        return crate::view_client::run(id, args).map(String::into_bytes);
    }
    let ran = match list().run_json(id, args) {
        // An async action (the arrangement's gestures await the engine):
        // awaited here, off any runtime — a window with no link (driving
        // a REAPER) runs the engine's actions itself.
        Err(architect::action::ActionError::Failed { message, .. })
            if message.contains("run_json_async")
                && tokio::runtime::Handle::try_current().is_err() =>
        {
            crate::open::runtime()
                .ok_or_else(|| "no runtime to run it on".to_owned())
                .and_then(|rt| {
                    rt.block_on(list().run_json_async(id, args))
                        .map_err(|e| e.to_string())
                })
        }
        other => other.map_err(|e| e.to_string()),
    };
    // The window's own, run here: said on the engine's list too, where
    // whoever watches the engine looks.
    if ran.is_ok() && crate::view_client::is_linked() {
        crate::view_client::ran_in_window(id, args);
    }
    ran
}

/// Run a key's action on the one thread that runs them behind the window,
/// in the order they were pressed; the window redraws when each is done,
/// as it does for a model's.
pub fn off_the_window(id: &'static str, args: String) {
    type Job = (&'static str, String, tracing::Span);
    static WORKER: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<Job>>> =
        std::sync::OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let _ = std::thread::Builder::new()
            .name("key-actions".into())
            .spawn(move || {
                // Each under the span it was asked in (the key's): its
                // trace runs from the key to the engine.
                for (id, args, asked_in) in rx {
                    let _in = asked_in.entered();
                    if let Err(e) = run(id, args.as_bytes()) {
                        tracing::warn!(action.id = id, error = %e, "key action refused");
                    }
                }
            });
        std::sync::Mutex::new(tx)
    });
    if let Ok(tx) = worker.lock() {
        let _ = tx.send((id, args, tracing::Span::current()));
    }
}

#[cfg(all(test, feature = "native"))]
mod presses {
    use super::list;
    use crate::engine::Move;

    /// What the window's controls run (`docs/design/actions-everywhere.md`):
    /// each must be on the engine's list, or a press runs nothing.
    const PRESSED: &[&str] = &[
        "TRACK_SET",
        "TRACK_ARM",
        "SESSION_SELECT_TRACK",
        "TRACK_LANE_STEP",
        "TRACK_SHOW_LANES",
        "SESSION_FOLD",
        "SESSION_MODE",
        "SESSION_PHASE",
        "SESSION_PANEL",
        "SESSION_ARRANGE",
        "SESSION_GROUPING",
        "ARRANGE_MOVE",
        "ARRANGE_TRIM",
        "ARRANGE_SLIP",
        "ARRANGE_FADE",
        "ARRANGE_SPLIT",
        "ARRANGE_COPY",
        "ARRANGE_DELETE",
        "ARRANGE_MARKERS",
        "ARRANGE_ITEM_VOLUME",
        "ARRANGE_TAKE_VOLUME",
        "ARRANGE_EDIT",
        "ITEMS_SELECT",
    ];

    #[test]
    fn every_control_names_an_action_on_the_list() {
        let missing: Vec<&&str> = PRESSED
            .iter()
            .filter(|id| list().info(id).is_none())
            .collect();
        assert!(missing.is_empty(), "not on the list: {missing:?}");
    }

    /// The FX view's rig blocks and racks commit through these.
    #[test]
    fn the_fx_views_commits_are_on_the_list() {
        let ids: &[&str] = &[
            "FX_SET",
            "FX_ENABLE",
            #[cfg(feature = "signal")]
            "RIG_BLOCK_SET",
            #[cfg(feature = "signal")]
            "RIG_BLOCKS",
        ];
        let missing: Vec<&&str> = ids.iter().filter(|id| list().info(id).is_none()).collect();
        assert!(missing.is_empty(), "not on the list: {missing:?}");
    }

    #[test]
    fn every_transport_press_runs_an_action() {
        for command in [
            Move::PlayStop,
            Move::Home,
            Move::Seek,
            Move::Stop,
            Move::End,
            Move::ToggleLoop,
            Move::ToggleRecord,
            Move::PlayFrom,
        ] {
            let (id, _) = super::transport_action(command, 1.5);
            assert!(
                list().info(id).is_some(),
                "{command:?} runs {id}, not on the list"
            );
        }
    }

    #[test]
    fn the_list_registers_cleanly() {
        let list = list();
        assert!(list.validate().is_ok(), "{:?}", list.validate());
        assert!(
            list.probe_hostile_args().is_ok(),
            "{:?}",
            list.probe_hostile_args()
        );
    }
}
