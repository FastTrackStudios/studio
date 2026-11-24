//! Studio's actions, beside the window's Action List (`daw_view::actions`,
//! re-exported here under the paths this crate always had): the mode and
//! the docks a press asks of Studio's window, and the song's versions.

use std::sync::Mutex;

use architect::action::ActionMeta;
#[cfg(feature = "native")]
use facet::Facet;

pub use daw_view::actions::*;

pub static MODE: ActionMeta = meta(
    "SESSION_MODE",
    "mode",
    "Mode",
    "Put the window in a mode: Organize, Write, Produce, Record, Edit, Mix, Master, Live, Video or Scoring — what the panels show and how (Edit shows every cut on a folder's row; Live and Mix a folder's row whole).",
    "View",
    "",
);

/// The mode a caller asked for, until the window takes it
/// ([`take_mode_request`]).
static MODE_REQUEST: Mutex<Option<session::modes::Mode>> = Mutex::new(None);

/// The mode asked for since the window last looked, if any.
pub fn take_mode_request() -> Option<session::modes::Mode> {
    MODE_REQUEST.lock().ok()?.take()
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

pub static DOCKS: ActionMeta = meta(
    "SESSION_DOCKS",
    "docks",
    "Hide or Show the Docks",
    "Hide every edge dock of the window (left, right, top, bottom; the centre stays), or bring back the ones hidden as they were. A panel shown while they are hidden comes back on its own, and stays when the rest return.",
    "View",
    "Shift+D",
);

/// What the window's dock is asked to do (`session_desktop`'s dock reads
/// these with [`take_dock_requests`]).
#[derive(Debug, Clone, PartialEq)]
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
    /// A dock control's change of the layout (`dock::DockIntent`): a panel
    /// moved, opened, closed, shown, maximised, a divider or a dock's edge
    /// set, a floating window placed.
    Intent(dock::DockIntent),
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

pub static VERSIONS: ActionMeta = meta(
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

/// The open song's versions, as the engine reads them
/// (`SESSION_VERSIONS`): `None` when it cannot say.
#[cfg(feature = "native")]
#[must_use]
pub fn versions() -> Option<VersionsRead> {
    run(VERSIONS.id, b"")
        .ok()
        .and_then(|bytes| facet_json::from_slice::<VersionsRead>(&bytes).ok())
}
