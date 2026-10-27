//! What the engine asks of the window it has attached — the window's own
//! actions queued for its next frame ([`queue`], [`take_window`]): a key
//! the Action List ran, an action that needs the arrangement to do it
//! (`SESSION_ZOOM`, a drum restore the arrangement slips).
//!
//! Engine side: the queue and whether a window is attached ([`attach`]).
//! With no window attached nothing is queued — there is nobody to run it —
//! and an action that needs one says so ([`attached`]).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// One slice of a stretch: its guid, where it plays and how long (µs),
/// and its stretch markers (`(played, source)`, µs).
pub type StretchEdit = (String, i64, i64, Vec<(i64, i64)>);

/// What a key asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    PlayStop,
    PlayPause,
    GoToStart,
    /// Split the selected items at the edit cursor — or, with nothing
    /// selected, every item under it.
    SplitAtCursor,
    /// Quantize what is selected (`EDIT_QUANTIZE`): for the drums the
    /// kit, edited from its hit markers.
    QuantizeDrums,
    /// T / Shift+T: comp the selection — a fill's hits, rows, a razor, a
    /// time selection — from the next (1) or previous (-1) take in its
    /// ranking (`COMP_CYCLE`).
    CompCycle(i32),
    /// Ctrl+T: comp the selection — or, with none, the whole song — from
    /// each phrase's best take, as one edit (`COMP_BEST`).
    CompBest,
    /// Ctrl+Shift+T: keep the take comped over the selection (`COMP_KEEP`),
    /// or let a kept stretch go again.
    CompKeep,
    /// Rank a take up (`by` 1) or down (-1) by hand (`TAKES_RANK`): the
    /// take under the mouse first (`mouse`, buttons 4 and 5), else the
    /// selected takes — the last recording's once it ends — else the
    /// mouse's (Up and Down); while recording, the take being recorded
    /// from where the press falls.
    RankTake {
        by: i8,
        mouse: bool,
    },
    /// Level what is selected (`EDIT_LEVEL`), in Edit mode: for the drums
    /// each selected piece's strokes brought toward one another.
    LevelDrums,
    /// Put the drums back as played over the selected folded rows' span
    /// or the time selection (`DRUMS_REVERT`): the edit undone there, the
    /// hit markers kept.
    RevertDrums,
    /// Show or hide the Quantize panel (`crate::quantize_panel`).
    QuantizePanel,
    /// Slip-edit the whole kit — every folded row of the drums folder —
    /// by these moves: `(hit, shift)`, microseconds (whole numbers, so
    /// an action compares). What `QUANTIZE` on the Action List works
    /// out and hands the window, which carries it out where it is seen.
    /// Fill the lanes with a span of time (µs from the start) and/or
    /// these tracks (by GUID): a model looking at a fill, a section, the
    /// kit.
    Frame {
        time: Option<(i64, i64)>,
        tracks: Vec<String>,
    },
    /// Take `from..to` (µs) out of these tracks' edited takes (by GUID),
    /// or the whole kit's when there are none.
    RemoveRange {
        tracks: Vec<String>,
        from_us: i64,
        to_us: i64,
    },
    /// Hits moved by stretching the audio either side of them (a hit
    /// marker's foot dragged, `HITS_STRETCH`): each slice's new place,
    /// length and stretch markers, µs — `(guid, position, length,
    /// [(played, source)])`.
    StretchDrums {
        edits: Vec<StretchEdit>,
        /// Sent back as [`crate::engine::Edit::Done`] once the edits are.
        ticket: u64,
    },
    SlipDrums {
        moves: Vec<(i64, i64)>,
        /// The cut before each hit and the seams' crossfade, µs.
        pre_roll_us: i64,
        crossfade_us: i64,
        /// A gap longer than this is stretched over (0: never).
        stretch_over_us: i64,
        /// Every hit of the kit where it plays now, µs, by time: no seam
        /// reaches back over one.
        attacks_us: Vec<i64>,
        /// Sent back as [`crate::engine::Edit::Done`] once the edits are.
        ticket: u64,
    },
    /// The kit as recorded over `from..to` (µs): its EDIT lanes made its
    /// COMP lanes again there (`arrange_edit::Editor::restore`). What
    /// `EDIT_REVERT` queues for the window.
    RestoreDrums {
        from_us: i64,
        to_us: i64,
        /// Sent back as [`crate::engine::Edit::Done`] once the edits are.
        ticket: u64,
    },
    /// Gain stage the session (Shift+G), or with `clear` (Alt+G) put every
    /// staged item back at an item volume of 1 (`crate::gain_stage`).
    GainStage {
        clear: bool,
    },
    /// Ctrl+N: comp the razor areas from take N — on a folded row, every
    /// mic in the folder at once (`arrange_edit::Editor::comp`).
    CompTake(u32),
    DeleteSelectedItems,
    SelectAllItems,
    /// Snapping on or off: the main toolbar's switch.
    ToggleSnap,
    /// The selected items onto the arrangement's clipboard.
    CopyItems,
    /// The clipboard's items at the edit cursor, each on its own track.
    PasteItems,
    /// Escape: no time selection, nothing selected.
    ClearSelection,
    /// The edit cursor by a bar, or by a beat.
    CursorBar(i32),
    CursorBeat(i32),
    /// Track selection, up or down the panel; `extend` keeps the rest.
    TrackStep {
        by: i32,
        extend: bool,
    },
    /// The edit cursor to the previous or next marker.
    Marker(i32),
    ToggleRecord,
    /// Something about the view rather than the session: a zoom.
    View(crate::zoom::Command),
    /// The visibility manager: show or hide a group of tracks.
    Visibility(Visibility),
    /// Show or hide the docked mixer.
    ToggleMixer,
    /// The Editor: docked under the arrangement, or a view of its own
    /// (`e`, the profile's `FTS_EXPRESSION_EDITOR_OPEN`).
    ToggleEditor,
    /// Show or hide the FX (`f`): the selected track's effects as devices.
    ToggleFx,
    /// Show or hide the browser (`b`): what to load on the selected track.
    ToggleBrowser,
    /// The view a page of bars on, or back (`]`, `[`): `SESSION_STEP`.
    BarStep(i32),
    /// To the next point of interest or the one before (`}`, `{`):
    /// `POI_STEP`.
    PoiStep(i32),
    /// The point of interest stepped to ticked off, or open again (`|`):
    /// `POI_TOGGLE`.
    PoiToggle,
    /// The window into a mode (`Alt+1` … `Alt+9`, `Alt+0`, and the
    /// `Alt+m` tree): `SESSION_MODE`, by the mode's name.
    Mode(&'static str),
    /// The FX's own view, full size (`F`): several tracks' chains.
    FxFull,
    /// Show the session as a scene (`v v` Overview, `v t` Overview TOP),
    /// or, in it already, go back to the view before.
    ToggleScene(&'static str),
    /// Save the song (Ctrl+S, REAPER's "Save project"): its `.session`,
    /// as `SESSION_SAVE` does.
    Save,
    /// Undo the last step (Ctrl+Z, REAPER's 40029), as `SESSION_UNDO`.
    Undo,
    /// Redo what was undone (Ctrl+Shift+Z, REAPER's 40030), as
    /// `SESSION_REDO`.
    Redo,
    /// A binding the window does not do yet, by its REAPER id.
    Unbound(String),
}

/// The visibility manager's actions (`v` and its tree).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// Show the group if all of it is hidden, else hide it. The group's
    /// name as the action spells it (`DRUMS`), matched loosely.
    Toggle(String),
    ShowAll,
    HideAll,
}

/// The window's actions waiting for it.
static WINDOW: Mutex<VecDeque<Action>> = Mutex::new(VecDeque::new());

/// The window's actions run since it last looked, in order.
pub fn take_window() -> Vec<Action> {
    let taken: Vec<Action> = WINDOW
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default();
    if !taken.is_empty() {
        FOLLOW_UP.store(true, Ordering::Release);
    }
    taken
}

/// Whether any are waiting.
pub fn window_pending() -> bool {
    WINDOW.lock().is_ok_and(|q| !q.is_empty())
}

/// Whether the window owes a frame for actions it just carried out —
/// taken, so one frame is asked for each time. For the shell's frame
/// loop alone.
///
/// The window sends its edits to the engine at the start of a frame and
/// carries out these actions while painting one, so what they edited
/// goes on the frame AFTER. No event is coming to ask for it.
pub fn frame_owed() -> bool {
    FOLLOW_UP.swap(false, Ordering::AcqRel)
}

/// A frame owed for actions just carried out.
static FOLLOW_UP: AtomicBool = AtomicBool::new(false);

/// Queue `action` for the attached window's next frame; dropped when no
/// window is attached (nobody would ever take it). Whether it was queued.
pub fn queue(action: Action) -> bool {
    if !attached() {
        tracing::debug!(?action, "window action dropped: no window attached");
        return false;
    }
    if let Ok(mut q) = WINDOW.lock() {
        q.push_back(action);
        return true;
    }
    false
}

// ── The windows attached ─────────────────────────────────────────────

/// The ids of the window's own actions, once a window registered them.
static WINDOW_IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// The window's own actions are `ids` (what it registered on the list).
pub fn window_actions(ids: Vec<String>) {
    let _ = WINDOW_IDS.set(ids);
}

/// Whether `id` is one of the window's own actions — what runs in the
/// window's process even when the engine is in another.
#[must_use]
pub fn is_window_action(id: &str) -> bool {
    WINDOW_IDS
        .get()
        .is_some_and(|ids| ids.iter().any(|i| i == id))
}

/// How many windows are attached to this engine.
static WINDOWS: AtomicUsize = AtomicUsize::new(0);

/// A window attached to the engine, for as long as it is held: its
/// actions run ([`queue`]) and the window actions on the Action List are
/// enabled. Dropping it detaches the window.
#[derive(Debug)]
pub struct Attached(());

/// Attach a window.
#[must_use]
pub fn attach() -> Attached {
    WINDOWS.fetch_add(1, Ordering::AcqRel);
    Attached(())
}

impl Drop for Attached {
    fn drop(&mut self) {
        let before = WINDOWS.fetch_sub(1, Ordering::AcqRel);
        if before <= 1 {
            // The last window went: what it had not taken goes with it.
            if let Ok(mut q) = WINDOW.lock() {
                q.clear();
            }
        }
    }
}

/// Whether a window is attached.
#[must_use]
pub fn attached() -> bool {
    WINDOWS.load(Ordering::Acquire) > 0
}

/// Why a window action cannot run now, if it cannot: what the Action List
/// says of each while no window is attached.
///
/// # Errors
///
/// No window is attached.
pub fn needs_a_window() -> Result<(), String> {
    if attached() {
        Ok(())
    } else {
        Err("no window attached: this is the window's action".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_is_open_only_while_a_window_is_attached() {
        // One test owns the process's seat, so the steps run in order.
        assert!(!attached());
        assert!(needs_a_window().is_err());
        assert!(!queue(Action::GoToStart), "nobody to take it");
        let first = attach();
        let second = attach();
        assert!(queue(Action::GoToStart));
        drop(first);
        assert!(attached(), "one window is still there");
        assert!(window_pending());
        drop(second);
        assert!(!attached());
        assert!(!window_pending(), "the last window's queue goes with it");
        assert!(needs_a_window().is_err());
    }
}
