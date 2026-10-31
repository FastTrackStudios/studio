//! The window's side of the Action List: the actions only a window can do
//! (zoom, panels, the view, what the arrangement edits under its own
//! selection), how a key runs through the list ([`through_list`]), and the
//! window attaching to the engine ([`attach`]).
//!
//! The engine's list (`crate::actions::list`) is complete without a
//! window: transport, the set, the song's data. A window that attaches
//! registers its own actions on it (once per process) and enables them
//! while it is attached; with no window they are listed as
//! `enabled: false`, "no window attached". The actions themselves queue
//! for the window's next frame (`crate::window_queue`).

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;

use crate::actions::meta;
use crate::window_queue::{Action, queue, take_window};

/// A key's actions run through the list, and what the window is to do
/// about them.
///
/// A key is one more caller of the same actions a model runs. An action
/// the list does not have (a binding the window cannot do yet) goes
/// straight through, to be logged as it always was.
#[must_use]
pub fn through_list(actions: Vec<Action>) -> Vec<Action> {
    for action in actions {
        // A key's action, its own unit of work: what it runs — on the
        // engine over the link, or here — lands under it.
        let _span = tracing::info_span!(
            "window.key",
            window.key_action = id_and_args(&action).map_or("", |(id, _)| id),
        )
        .entered();
        match id_and_args(&action) {
            // The docks hidden or brought back, here in the window: a
            // chord, which the window's own keys may not see the
            // modifier of.
            Some(_) if action == Action::ToggleDocks && crate::actions::docked() => {
                crate::actions::request_dock(crate::actions::DockRequest::ToggleDocks);
            }
            Some((id, args)) if behind_the_window(id) => crate::actions::off_the_window(id, args),
            // A dock's panel, which the window's own keys toggled already
            // (`keys::use_window_transport_keys`): once is a toggle.
            #[cfg(feature = "native")]
            Some(_)
                if crate::actions::docked()
                    && dock_request(&action).is_some()
                    && crate::keys::window_has_transport() => {}
            // The window's own transport keys took this press already
            // (`keys::use_window_transport_keys`): once is a toggle.
            #[cfg(feature = "native")]
            Some((id, _))
                if [crate::actions::PLAY_STOP.id, crate::actions::PLAY_PAUSE.id].contains(&id)
                    && crate::keys::window_has_transport() => {}
            Some((id, args)) => {
                if let Err(e) = crate::actions::run(id, args.as_bytes()) {
                    tracing::warn!(action.id = id, error = %e, "key action refused");
                }
            }
            None => {
                queue(action);
            }
        }
    }
    take_window()
}

/// A control pressed in the window — a button, a menu item, a gesture's
/// end: its named action run through the Action List, exactly as a key's
/// is ([`through_list`]), so `watch`, a guide and the CLI see the same
/// thing a click does. The action does the work; the control only names
/// it. What the action said (JSON; `null` for nothing), or `None` when it
/// refused (said once, as a key's refusal is).
pub fn press(id: &str, args: &serde_json::Value) -> Option<serde_json::Value> {
    let _span = tracing::info_span!("window.press", window.press_action = id).entered();
    let args = if args.is_null() {
        String::new()
    } else {
        args.to_string()
    };
    match crate::actions::run(id, args.as_bytes()) {
        Ok(out) => Some(serde_json::from_slice(&out).unwrap_or(serde_json::Value::Null)),
        Err(e) => {
            tracing::warn!(action.id = id, error = %e, "control action refused");
            None
        }
    }
}

/// [`press`], behind the window: for an action that reads the whole song
/// or waits on the engine, where the frame must not wait for it. Run in
/// the order pressed, on the thread keys' slow actions run on.
pub fn press_behind(id: &'static str, args: &serde_json::Value) {
    let args = if args.is_null() {
        String::new()
    } else {
        args.to_string()
    };
    crate::actions::off_the_window(id, args);
}

/// What a pane action asks of the dock when the window is a dock: its
/// panel toggled, or every edge dock hidden or brought back.
#[must_use]
pub(crate) fn dock_request(action: &Action) -> Option<crate::actions::DockRequest> {
    use crate::actions::DockRequest;
    let panel = |kind: &str| Some(DockRequest::TogglePanel(kind.to_owned()));
    match action {
        Action::ToggleMixer => panel("mixer"),
        Action::ToggleEditor => panel("editor"),
        Action::ToggleFx => panel("fx-row"),
        Action::ToggleBrowser => panel("browser"),
        Action::ToggleDocks => Some(DockRequest::ToggleDocks),
        _ => None,
    }
}

/// The list's id for a window action that takes nothing — what a key
/// bound to it runs, and what a menu item for it runs.
#[must_use]
pub(crate) fn plain_id(action: &Action) -> Option<&'static str> {
    id_and_args(action).and_then(|(id, args)| args.is_empty().then_some(id))
}

/// An action's id in the list and its arguments, as a key produced it.
fn id_and_args(action: &Action) -> Option<(&'static str, String)> {
    use crate::zoom::Command as Z;
    let by = |by: i32| format!(r#"{{"by":{by}}}"#);
    Some(match action {
        Action::PlayStop => (crate::actions::PLAY_STOP.id, String::new()),
        Action::PlayPause => (crate::actions::PLAY_PAUSE.id, String::new()),
        Action::GoToStart => (crate::actions::GO_TO_START.id, String::new()),
        Action::ToggleRecord => (TOGGLE_RECORD.id, String::new()),
        Action::SplitAtCursor => (SPLIT.id, String::new()),
        Action::QuantizeDrums => (QUANTIZE_DRUMS.id, String::new()),
        Action::RevertDrums => (REVERT_DRUMS.id, String::new()),
        Action::LevelDrums => (LEVEL_DRUMS.id, String::new()),
        // The lanes button's own action, on the selected track.
        Action::ShowLanes => (crate::comp::SHOW_LANES.id, "{}".to_owned()),
        Action::CompCycle(by) if *by < 0 => (COMP_PREV.id, String::new()),
        Action::CompCycle(_) => (COMP_NEXT.id, String::new()),
        Action::CompBest => (COMP_BEST_KEY.id, String::new()),
        Action::CompKeep => (COMP_KEEP_KEY.id, String::new()),
        Action::RankTake { by, mouse } => (
            match (*by > 0, *mouse) {
                (true, false) => TAKE_UP.id,
                (false, false) => TAKE_DOWN.id,
                (true, true) => TAKE_UP_MOUSE.id,
                (false, true) => TAKE_DOWN_MOUSE.id,
            },
            String::new(),
        ),
        Action::QuantizePanel => (QUANTIZE_PANEL.id, String::new()),
        Action::GainStage { clear: false } => (GAIN_STAGE.id, String::new()),
        Action::GainStage { clear: true } => (GAIN_STAGE_CLEAR.id, String::new()),
        Action::CompTake(take) => (COMP_TAKE.id, format!(r#"{{"take":{take}}}"#)),
        Action::DeleteSelectedItems => (DELETE_ITEMS.id, String::new()),
        Action::SelectAllItems => (SELECT_ALL.id, String::new()),
        Action::ClearSelection => (CLEAR_SELECTION.id, String::new()),
        Action::CursorBar(n) => (CURSOR_BAR.id, by(*n)),
        Action::CursorBeat(n) => (CURSOR_BEAT.id, by(*n)),
        Action::Marker(n) => (MARKER.id, by(*n)),
        Action::TrackStep { by, extend } => {
            (TRACK_STEP.id, format!(r#"{{"by":{by},"extend":{extend}}}"#))
        }
        Action::ToggleMixer => (TOGGLE_MIXER.id, String::new()),
        #[cfg(feature = "native")]
        Action::Save => (crate::actions::SAVE.id, String::new()),
        #[cfg(not(feature = "native"))]
        Action::Save => return None,
        #[cfg(feature = "native")]
        Action::Undo => (crate::actions::UNDO.id, String::new()),
        #[cfg(feature = "native")]
        Action::Redo => (crate::actions::REDO.id, String::new()),
        #[cfg(not(feature = "native"))]
        Action::Undo | Action::Redo => return None,
        Action::ToggleSnap => (OPTION.id, r#"{"option":"snap"}"#.to_owned()),
        // The window's own: its clipboard is the arrangement's. A paste
        // commits as the gesture it is (`ARRANGE_COPY`, `settle`).
        Action::CopyItems | Action::PasteItems => return None,
        Action::ToggleEditor => (TOGGLE_EDITOR.id, String::new()),
        Action::ToggleFx => (TOGGLE_FX.id, String::new()),
        Action::ToggleBrowser => (crate::browser::BROWSER.id, String::new()),
        Action::ToggleDocks => (crate::actions::DOCKS.id, String::new()),
        Action::BarStep(n) => (crate::bar_step::STEP_ACTION.id, by(*n)),
        Action::PoiStep(n) => (crate::poi::STEP.id, by(*n)),
        Action::PoiToggle => (crate::poi::TOGGLE.id, String::new()),
        Action::Mode(name) => (crate::actions::MODE.id, format!(r#"{{"mode":"{name}"}}"#)),
        Action::FxFull => (FX_FULL.id, String::new()),
        Action::ToggleScene(slug) => (SCENE.id, format!(r#"{{"scene":"{slug}"}}"#)),
        Action::Visibility(crate::keys::Visibility::Toggle(group)) => (
            SHOW_GROUP.id,
            format!(r#"{{"group":{}}}"#, serde_json::Value::from(group.as_str())),
        ),
        Action::Visibility(crate::keys::Visibility::ShowAll) => (SHOW_ALL.id, String::new()),
        Action::Visibility(crate::keys::Visibility::HideAll) => (HIDE_ALL.id, String::new()),
        Action::View(command) => {
            let zoom = match command {
                Z::ToggleTracks => "ToggleTracks",
                Z::FitTracks => "FitTracks",
                Z::Project => "Project",
                Z::Selection => "Selection",
                Z::ToggleSelection => "ToggleSelection",
                Z::Items => "Items",
                Z::Back => "Back",
                Z::Forward => "Forward",
                Z::Step {
                    vertical: false,
                    inward: true,
                } => "InHorizontal",
                Z::Step {
                    vertical: false,
                    inward: false,
                } => "OutHorizontal",
                Z::Step {
                    vertical: true,
                    inward: true,
                } => "InVertical",
                Z::Step {
                    vertical: true,
                    inward: false,
                } => "OutVertical",
            };
            (ZOOM.id, format!(r#"{{"zoom":"{zoom}"}}"#))
        }
        // Already the list's own answer (`QUANTIZE`), or a binding the
        // window does not do: straight to the window.
        Action::Unbound(_)
        | Action::SlipDrums { .. }
        | Action::RestoreDrums { .. }
        | Action::StretchDrums { .. }
        | Action::Frame { .. }
        | Action::RemoveRange { .. } => {
            return None;
        }
    })
}

/// Whether a key's action runs behind the window rather than on its
/// thread: one that reads the whole song or writes its file (an undo, a
/// save) would hold the window still while it did.
#[cfg(feature = "native")]
fn behind_the_window(id: &str) -> bool {
    [
        crate::actions::UNDO.id,
        crate::actions::REDO.id,
        crate::actions::SAVE.id,
    ]
    .contains(&id)
}

#[cfg(not(feature = "native"))]
const fn behind_the_window(_id: &str) -> bool {
    false
}

static TOGGLE_RECORD: ActionMeta = meta(
    "SESSION_TOGGLE_RECORD",
    "toggle_record",
    "Record",
    "Start recording the armed tracks, or stop.",
    "Transport",
    "",
);

static SPLIT: ActionMeta = meta(
    "SESSION_SPLIT",
    "split",
    "Split at cursor",
    "Split the selected items at the edit cursor — or, with nothing selected, every item under it.",
    "Edit",
    "S",
);

static QUANTIZE_DRUMS: ActionMeta = meta(
    "SESSION_QUANTIZE_DRUMS",
    "quantize_drums",
    "Quantize drums",
    "QUANTIZE over the span of the selected folded rows, or the time selection: the whole kit edited from its hit markers with the settings the Quantize panel last applied (else the sticky ones, DRUMS_EDIT_SETTINGS); the window says how the check came out.",
    "Drums",
    "Q",
);

static LEVEL_DRUMS: ActionMeta = meta(
    "SESSION_LEVEL_DRUMS",
    "level_drums",
    "Level drums",
    "In Edit mode, EDIT_LEVEL over the selection: each selected piece's strokes (every piece when only time is selected) brought toward one another, each drum's own ride; over a time selection, razor or folded rows only the strokes there, the curve elsewhere kept. The window says how it came out.",
    "Drums",
    "Alt+Shift+Q",
);

static COMP_NEXT: ActionMeta = meta(
    "SESSION_COMP_NEXT",
    "comp_next",
    "Comp the next take",
    "COMP_CYCLE over the selection — a fill's hit markers, rows, a razor, a time selection — the next take in its ranking comped onto the kit.",
    "Comp",
    "T",
);

static COMP_BEST_KEY: ActionMeta = meta(
    "SESSION_COMP_BEST",
    "comp_best_selection",
    "Comp the best takes",
    "COMP_BEST over the selection — or the whole song with nothing selected: each phrase comped from its best take, as one edit.",
    "Comp",
    "Ctrl+T",
);

static COMP_KEEP_KEY: ActionMeta = meta(
    "SESSION_COMP_KEEP",
    "comp_keep_selection",
    "Keep the take comped here",
    "COMP_KEEP over the selection — the comp strip's picked spans, the selected hits, items, or the rest of it: the take comped there kept, or a kept stretch let go again.",
    "Comp",
    "Ctrl+Shift+T",
);

static TAKE_UP: ActionMeta = meta(
    "SESSION_TAKE_UPRANK",
    "take_uprank",
    "Up-rank the take",
    "Up-rank the selected takes (the last recording's, once it ends), else the take under the mouse; while recording, the take being recorded from now on (TAKES_RANK).",
    "Comp",
    "Up",
);

static TAKE_DOWN: ActionMeta = meta(
    "SESSION_TAKE_DOWNRANK",
    "take_downrank",
    "Down-rank the take",
    "Down-rank the selected takes (the last recording's, once it ends), else the take under the mouse; while recording, the take being recorded from now on (TAKES_RANK).",
    "Comp",
    "Down",
);

static TAKE_UP_MOUSE: ActionMeta = meta(
    "SESSION_TAKE_UPRANK_MOUSE",
    "take_uprank_mouse",
    "Up-rank the take under the mouse",
    "Up-rank the take under the mouse — a lane's item, the comp strip's span, the folded kit's take there — else as SESSION_TAKE_UPRANK. Mouse button 5 (forward).",
    "Comp",
    "Mouse 5",
);

static TAKE_DOWN_MOUSE: ActionMeta = meta(
    "SESSION_TAKE_DOWNRANK_MOUSE",
    "take_downrank_mouse",
    "Down-rank the take under the mouse",
    "Down-rank the take under the mouse — a lane's item, the comp strip's span, the folded kit's take there — else as SESSION_TAKE_DOWNRANK. Mouse button 4 (back).",
    "Comp",
    "Mouse 4",
);

static COMP_PREV: ActionMeta = meta(
    "SESSION_COMP_PREV",
    "comp_prev",
    "Comp the take before",
    "COMP_CYCLE over the selection, by -1: the take before in its ranking comped onto the kit.",
    "Comp",
    "Shift+T",
);

static REVERT_DRUMS: ActionMeta = meta(
    "SESSION_REVERT_DRUMS",
    "revert_drums",
    "Put the drums back as played",
    "DRUMS_REVERT over the span of the selected folded rows, or the time selection: the edit undone there, the hit markers kept to edit by hand, a no-quantize zone over it; the window says how the check came out.",
    "Drums",
    "Alt+Q",
);

static GAIN_STAGE: ActionMeta = meta(
    "SESSION_GAIN_STAGE",
    "gain_stage",
    "Gain stage",
    "Set every recorded track's item volume to the reference kit's levels — non-destructive, the audio untouched.",
    "Mix",
    "Shift+G",
);

static GAIN_STAGE_CLEAR: ActionMeta = meta(
    "SESSION_GAIN_STAGE_CLEAR",
    "gain_stage_clear",
    "Clear gain staging",
    "Put every staged item back at an item volume of 1.",
    "Mix",
    "Alt+G",
);

static COMP_TAKE: ActionMeta = meta(
    "SESSION_COMP_TAKE",
    "comp_take",
    "Comp from take",
    "Comp the razor areas from take N — on a folded row, every mic in the folder at once.",
    "Comp",
    "Ctrl+N",
);

static DELETE_ITEMS: ActionMeta = meta(
    "SESSION_DELETE_ITEMS",
    "delete_items",
    "Delete selected items",
    "Delete the selected items.",
    "Edit",
    "Delete",
);

static SELECT_ALL: ActionMeta = meta(
    "SESSION_SELECT_ALL",
    "select_all",
    "Select all items",
    "Select every item.",
    "Edit",
    "Ctrl+A",
);

static CLEAR_SELECTION: ActionMeta = meta(
    "SESSION_CLEAR_SELECTION",
    "clear_selection",
    "Clear selection",
    "No time selection, nothing selected.",
    "Edit",
    "Escape",
);

static CURSOR_BAR: ActionMeta = meta(
    "SESSION_CURSOR_BAR",
    "cursor_bar",
    "Cursor by bars",
    "Move the edit cursor by bars (negative: back).",
    "Navigate",
    "",
);

static CURSOR_BEAT: ActionMeta = meta(
    "SESSION_CURSOR_BEAT",
    "cursor_beat",
    "Cursor by beats",
    "Move the edit cursor by beats (negative: back).",
    "Navigate",
    "",
);

static MARKER: ActionMeta = meta(
    "SESSION_MARKER",
    "marker",
    "Cursor by markers",
    "Move the edit cursor to the previous (negative) or next marker.",
    "Navigate",
    "",
);

static TRACK_STEP: ActionMeta = meta(
    "SESSION_TRACK_STEP",
    "track_step",
    "Step track selection",
    "Move the track selection up (negative) or down the panel; `extend` keeps the rest selected.",
    "Navigate",
    "",
);

static TOGGLE_MIXER: ActionMeta = meta(
    "SESSION_TOGGLE_MIXER",
    "toggle_mixer",
    "Mixer",
    "Show or hide the docked mixer.",
    "View",
    "",
);

static WINDOW_STATE: ActionMeta = meta(
    "SESSION_WINDOW",
    "session_window",
    "What the window shows",
    "The window's mode and that mode's phase, as its shell shows them now — what a key or a click changed (SESSION_MODE, Alt+1…9, SESSION_PHASE). Read-only.",
    "View",
    "",
);

/// What [`WINDOW_STATE`] answers.
#[derive(Facet)]
struct WindowState {
    /// `Edit`, `Live`, …; empty before the shell has shown one.
    mode: String,
    /// The mode's phase (its slug), when it has phases.
    phase: Option<String>,
}

static QUANTIZE_PANEL: ActionMeta = meta(
    "SESSION_QUANTIZE_PANEL",
    "quantize_panel",
    "Quantize panel",
    "Show or hide the Quantize panel over the arrangement: a section, its groove, the quantize's settings, Preview and Apply — QUANTIZE and DRUMS_GROOVE by hand.",
    "View",
    "Shift+Q",
);

static PERF_METER: ActionMeta = meta(
    "SESSION_PERF_METER",
    "perf_meter",
    "Performance meter",
    "Show or hide the Performance meter over the window: the engine's CPU over the last minute (all cores, the audio callback's, the render ahead's), the longest block, dropouts, disk and RAM, and every track's FX CPU, FX count, delay compensation and media CPU — sortable by any column. SESSION_PERF reads the same numbers.",
    "View",
    "",
);

static MIXER_FX: ActionMeta = meta(
    "SESSION_MIXER_FX",
    "mixer_fx",
    "Mixer FX racks",
    "Show or hide the FX racks over the mixer's faders (`shown`: true or false; left out, the other way): hidden, the faders have the panel's whole height. Kept across launches.",
    "View",
    "",
);

/// Shown or hidden, or (left out) the other way.
#[derive(Facet)]
struct MixerFxArg {
    #[facet(default)]
    shown: Option<bool>,
}

static TOGGLE_EDITOR: ActionMeta = meta(
    "SESSION_TOGGLE_EDITOR",
    "toggle_editor",
    "Editor",
    "Show or hide the Editor under the arrangement.",
    "View",
    "E",
);

static TOGGLE_FX: ActionMeta = meta(
    "SESSION_TOGGLE_FX",
    "toggle_fx",
    "FX",
    "Show or hide the selected track's effects.",
    "View",
    "F",
);

static FX_FULL: ActionMeta = meta(
    "SESSION_FX_FULL",
    "fx_full",
    "FX, full size",
    "The FX view, full size: several tracks' chains.",
    "View",
    "Shift+F",
);

static SCENE: ActionMeta = meta(
    "SESSION_SCENE",
    "scene",
    "Scene",
    "Show the session as a scene (`overview`, `overview-top`), or, in it already, go back.",
    "View",
    "",
);

static SHOW_GROUP: ActionMeta = meta(
    "SESSION_SHOW_GROUP",
    "show_group",
    "Show/hide group",
    "Show a group of tracks if all of it is hidden, else hide it (`DRUMS`, matched loosely).",
    "View",
    "",
);

static SHOW_ALL: ActionMeta = meta(
    "SESSION_SHOW_ALL",
    "show_all",
    "Show all tracks",
    "Show every track.",
    "View",
    "",
);

static HIDE_ALL: ActionMeta = meta(
    "SESSION_HIDE_ALL",
    "hide_all",
    "Hide all tracks",
    "Hide every track.",
    "View",
    "",
);

static ZOOM: ActionMeta = meta(
    "SESSION_ZOOM",
    "zoom",
    "Zoom",
    "Zoom the arrangement: the selected tracks, every track, the whole song, the selection, the items, back/forward, or a step in or out.",
    "View",
    "",
);

/// By how many.
#[derive(Facet)]
struct By {
    /// Negative: back, up.
    by: i32,
}

/// A track selection step.
#[derive(Facet)]
struct Step {
    by: i32,
    /// Keep the tracks already selected.
    extend: Option<bool>,
}

/// Which take.
#[derive(Facet)]
struct Take {
    /// 1-based, as Ctrl+1 … Ctrl+9.
    take: u32,
}

/// Which scene.
#[derive(Facet)]
struct SceneArg {
    scene: String,
}

/// Which group of tracks.
#[derive(Facet)]
struct GroupArg {
    group: String,
}

/// A zoom.
#[derive(Facet)]
struct ZoomArg {
    zoom: Zoom,
}

#[derive(Facet, Clone, Copy)]
#[repr(u8)]
enum Zoom {
    ToggleTracks,
    FitTracks,
    Project,
    Selection,
    ToggleSelection,
    Items,
    Back,
    Forward,
    InHorizontal,
    OutHorizontal,
    InVertical,
    OutVertical,
}

/// The scenes a scene action can name: the window's `&'static` slugs.
const SCENES: &[&str] = &["overview", "overview-top"];

static OPTION: ActionMeta = meta(
    "SESSION_OPTION",
    "option",
    "Editing option",
    "Turn one of the arrangement's editing options on or off — `option`: `snap` (to the grid), `grid` (its lines), `ripple` (later items follow an edit, per track), `razor` (the razor tool), `auto_crossfade` (overlapping items crossfade), `lock` (drags scroll and move nothing); `on` true or false, the other way when left out. The main toolbar's switches. Says whether it is on now.",
    "Options",
    "",
);

/// Which option, and on or off (left out: the other way).
#[derive(Facet)]
struct OptionArg {
    option: String,
    #[facet(default)]
    on: Option<bool>,
}

fn register_window(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&OPTION, Effect::Mutating, |a: OptionArg| {
        let (name, option) = crate::options::named(&a.option).ok_or_else(|| {
            format!(
                "no option {:?}: snap, grid, ripple, razor, auto_crossfade or lock (grouping is SESSION_GROUPING)",
                a.option
            )
        })?;
        let on = a.on.unwrap_or_else(|| !option.get());
        option.set(on);
        Ok::<_, String>(format!("{name}: {}", if on { "on" } else { "off" }))
    })?;
    list.register(&WINDOW_STATE, Effect::ReadOnly, |(): ()| {
        let mode = crate::actions::shown_mode();
        Ok(WindowState {
            mode: mode
                .map(|m| m.display_name().to_owned())
                .unwrap_or_default(),
            phase: mode
                .and_then(session::mode_phases::current)
                .map(|p| p.slug.to_owned()),
        })
    })?;
    list.register(&QUANTIZE_PANEL, Effect::Mutating, |(): ()| {
        crate::quantize_panel::ask();
        Ok("the Quantize panel shown, or hidden if it was".to_owned())
    })?;
    list.register(&MIXER_FX, Effect::Mutating, |a: MixerFxArg| {
        let on = a
            .shown
            .unwrap_or_else(|| !crate::mixer_panel::racks_shown());
        crate::mixer_panel::set_racks_shown(on);
        Ok(if on {
            "the mixer's FX racks shown"
        } else {
            "the mixer's FX racks hidden"
        }
        .to_owned())
    })?;
    list.register(&PERF_METER, Effect::Mutating, |(): ()| {
        crate::perf_meter::toggle();
        Ok(if crate::perf_meter::open() {
            "the Performance meter shown"
        } else {
            "the Performance meter hidden"
        }
        .to_owned())
    })?;
    let plain: [(&'static ActionMeta, Effect, Action); 23] = [
        (&TOGGLE_RECORD, Effect::Destructive, Action::ToggleRecord),
        (&SPLIT, Effect::Mutating, Action::SplitAtCursor),
        (&QUANTIZE_DRUMS, Effect::Mutating, Action::QuantizeDrums),
        (&REVERT_DRUMS, Effect::Mutating, Action::RevertDrums),
        (&LEVEL_DRUMS, Effect::Mutating, Action::LevelDrums),
        (&COMP_NEXT, Effect::Mutating, Action::CompCycle(1)),
        (&COMP_PREV, Effect::Mutating, Action::CompCycle(-1)),
        (&COMP_BEST_KEY, Effect::Mutating, Action::CompBest),
        (&COMP_KEEP_KEY, Effect::Mutating, Action::CompKeep),
        (
            &TAKE_UP,
            Effect::Mutating,
            Action::RankTake {
                by: 1,
                mouse: false,
            },
        ),
        (
            &TAKE_DOWN,
            Effect::Mutating,
            Action::RankTake {
                by: -1,
                mouse: false,
            },
        ),
        (
            &TAKE_UP_MOUSE,
            Effect::Mutating,
            Action::RankTake { by: 1, mouse: true },
        ),
        (
            &TAKE_DOWN_MOUSE,
            Effect::Mutating,
            Action::RankTake {
                by: -1,
                mouse: true,
            },
        ),
        (
            &GAIN_STAGE,
            Effect::Mutating,
            Action::GainStage { clear: false },
        ),
        (
            &GAIN_STAGE_CLEAR,
            Effect::Mutating,
            Action::GainStage { clear: true },
        ),
        (
            &DELETE_ITEMS,
            Effect::Destructive,
            Action::DeleteSelectedItems,
        ),
        (&SELECT_ALL, Effect::Mutating, Action::SelectAllItems),
        (&CLEAR_SELECTION, Effect::Mutating, Action::ClearSelection),
        (&TOGGLE_MIXER, Effect::Mutating, Action::ToggleMixer),
        (&TOGGLE_EDITOR, Effect::Mutating, Action::ToggleEditor),
        (&TOGGLE_FX, Effect::Mutating, Action::ToggleFx),
        (&FX_FULL, Effect::Mutating, Action::FxFull),
        (
            &SHOW_ALL,
            Effect::Mutating,
            Action::Visibility(crate::keys::Visibility::ShowAll),
        ),
    ];
    list.register(&crate::browser::BROWSER, Effect::Mutating, |(): ()| {
        if !crate::actions::if_docked(crate::actions::DockRequest::TogglePanel(
            "browser".to_owned(),
        )) {
            crate::browser::request_toggle();
        }
        Ok(())
    })?;
    for (meta, effect, action) in plain {
        list.register(meta, effect, move |(): ()| {
            // In a dock, a pane is its panel.
            if !dock_request(&action).is_some_and(crate::actions::if_docked) {
                queue(action.clone());
            }
            Ok(())
        })?;
    }
    list.register(&HIDE_ALL, Effect::Mutating, |(): ()| {
        queue(Action::Visibility(crate::keys::Visibility::HideAll));
        Ok(())
    })?;
    list.register(&COMP_TAKE, Effect::Mutating, |t: Take| {
        queue(Action::CompTake(t.take));
        Ok(())
    })?;
    list.register(&CURSOR_BAR, Effect::Mutating, |b: By| {
        queue(Action::CursorBar(b.by));
        Ok(())
    })?;
    list.register(&CURSOR_BEAT, Effect::Mutating, |b: By| {
        queue(Action::CursorBeat(b.by));
        Ok(())
    })?;
    list.register(&MARKER, Effect::Mutating, |b: By| {
        queue(Action::Marker(b.by));
        Ok(())
    })?;
    list.register(&TRACK_STEP, Effect::Mutating, |s: Step| {
        queue(Action::TrackStep {
            by: s.by,
            extend: s.extend.unwrap_or(false),
        });
        Ok(())
    })?;
    list.register(&SCENE, Effect::Mutating, |s: SceneArg| {
        let slug = SCENES
            .iter()
            .find(|known| known.eq_ignore_ascii_case(&s.scene))
            .ok_or_else(|| format!("no scene {}: one of {SCENES:?}", s.scene))?;
        queue(Action::ToggleScene(slug));
        Ok(())
    })?;
    list.register(&SHOW_GROUP, Effect::Mutating, |g: GroupArg| {
        queue(Action::Visibility(crate::keys::Visibility::Toggle(g.group)));
        Ok(())
    })?;
    list.register(&ZOOM, Effect::Mutating, |z: ZoomArg| {
        use crate::zoom::Command as Z;
        let step = |vertical, inward| Z::Step { vertical, inward };
        queue(Action::View(match z.zoom {
            Zoom::ToggleTracks => Z::ToggleTracks,
            Zoom::FitTracks => Z::FitTracks,
            Zoom::Project => Z::Project,
            Zoom::Selection => Z::Selection,
            Zoom::ToggleSelection => Z::ToggleSelection,
            Zoom::Items => Z::Items,
            Zoom::Back => Z::Back,
            Zoom::Forward => Z::Forward,
            Zoom::InHorizontal => step(false, true),
            Zoom::OutHorizontal => step(false, false),
            Zoom::InVertical => step(true, true),
            Zoom::OutVertical => step(true, false),
        }));
        Ok(())
    })?;
    Ok(())
}

static VIEW: ActionMeta = meta(
    "SESSION_VIEW",
    "view",
    "View",
    "Fill the arrangement with part of the song and/or some tracks: `from`/`to` (seconds), or `bars` ([first, last], inclusive), or `section` (a section's name, `nth` for the 2nd of its name — SESSION_SECTIONS lists them); and `tracks` (GUIDs, paths or path tails — a track in a shut folder frames the folder's row). Either axis left out stays as it is. For the window's own zooms (fit every track, the whole song, back) see SESSION_ZOOM.",
    "View",
    "",
);

/// `SESSION_VIEW`: frame part of the song and/or some tracks in the
/// arrangement.
fn register_view(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&VIEW, Effect::Mutating, |a: crate::page::ViewArg| {
        // The span and tracks as the song has them (`SESSION_SPAN`, on the
        // engine).
        let asked = facet_json::to_string(&a).map_err(|e| e.to_string())?;
        let said = crate::actions::run("SESSION_SPAN", asked.as_bytes())?;
        let crate::page::SpanOut { span, tracks } =
            facet_json::from_slice(&said).map_err(|e| format!("SESSION_SPAN: {e}"))?;
        if span.is_none() && tracks.is_empty() {
            return Err(
                "nothing to view: give a span (from/to, bars, section) or tracks".to_owned(),
            );
        }
        let us = |s: f64| num_traits::ToPrimitive::to_i64(&(s * 1_000_000.0).round()).unwrap_or(0);
        queue(Action::Frame {
            time: span.map(|(from, to)| (us(from), us(to))),
            tracks,
        });
        Ok(())
    })?;
    Ok(())
}

/// Every window action, onto `list`: what [`attach`] registers.
///
/// # Errors
///
/// The first registration the list refuses (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    register_window(list)?;
    register_view(list)?;
    crate::frames::register(list)?;
    crate::bar_step::register(list)
}

/// The window actions' ids, once registered on the engine's list.
static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Attach this window to the engine: its actions registered on the
/// engine's Action List (the first time, in this process) and enabled
/// while the returned guard lives, and what it draws from the engine's
/// changes listened for. Dropping the guard detaches it; the actions stay
/// listed, `enabled: false` ("no window attached"), as no window can run
/// them.
#[must_use]
pub fn attach() -> crate::window_queue::Attached {
    IDS.get_or_init(|| {
        let list = crate::actions::list();
        let before: std::collections::HashSet<String> =
            list.infos().into_iter().map(|i| i.id).collect();
        if let Err(error) = register(list) {
            tracing::error!(%error, "window: its actions stopped registering");
        }
        let ids: Vec<String> = list
            .infos()
            .into_iter()
            .map(|i| i.id)
            .filter(|id| !before.contains(id))
            .collect();
        for id in &ids {
            let _ = list.set_enabled(id, crate::window_queue::needs_a_window);
        }
        tracing::info!(
            window.actions = ids.len(),
            "window: its actions joined the engine's list"
        );
        crate::window_queue::window_actions(ids.clone());
        // And on the engine's list, when the window drives an engine in
        // another process (it serves no list of its own).
        crate::view_client::offer_window_actions();
        ids
    });
    crate::changes::listen(heard);
    crate::window_queue::attach()
}

/// The ids [`attach`] registered (empty before the first attach).
#[must_use]
pub fn action_ids() -> &'static [String] {
    IDS.get().map_or(&[], Vec::as_slice)
}

/// A change the engine announced, taken up for what this window draws.
fn heard(change: crate::changes::Change) {
    match change {
        crate::changes::Change::Marks => crate::marks_strip::refresh(),
    }
}
