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
use architect::action::{ActionList, ActionMeta, Effect};

use daw::service::{ProjectContext, TrackRef, Tracks};

use facet::Facet;

pub use editing::drums::KitPiece;

pub use engine_runtime::client::actions::*;

/// The Action List, every action registered: the window's (when a window
/// runs here) and the engine's, filled into the window's list
/// (`engine_runtime::client::actions::list`) on first use.
///
/// A registration architect refuses (a duplicate id) is a bug the
/// `the_list_registers_cleanly` test catches before it ships; a build that
/// still has one logs it and keeps the actions registered before it,
/// rather than taking the window down.
pub fn list() -> &'static ActionList {
    crate::install();
    engine_runtime::client::actions::list()
}

/// The engine's actions, into the list: what [`crate::install`] hands the
/// window's list to fill it with.
pub(crate) fn fill(list: &ActionList) {
    if let Err(error) = register_all(list) {
        tracing::error!(%error, "the action list stopped registering");
    }
}

/// Every action, into `list`, a domain at a time: each domain registers
/// its own (`register`), and this only collects them. A domain's actions
/// move with its crate; its line here moves with them.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
///
/// [`ActionError::Duplicate`]: architect::action::ActionError::Duplicate
fn register_all(list: &ActionList) -> Result<(), architect::action::ActionError> {
    base::register(list)?;
    // Each domain: its crate's `register`, and what is still here until
    // its modules move (then the second line goes, with `actions/<domain>.rs`).
    engine_edit::register(list)?;
    engine_mix::register(list)?;
    engine_listen::register(list)?;
    listen::register(list)?;
    engine_project::register(list)?;
    project::register(list)?;
    engine_collab::register(list)?;
    collab::register(list)?;
    Ok(())
}

// Each domain's actions and its registration (`register`). The domains'
// own modules register theirs (`crate::hits::register`, …) from these.
mod base;
mod collab;
mod listen;
mod project;
#[allow(
    unused_imports,
    reason = "the domains' items, under the paths they had in actions.rs"
)]
pub use self::base::*;
/// The edit domain's, from engine-edit.
pub use engine_edit::actions::edit::*;

pub use crate::shared_actions::meta;
/// The open song and a track in it, as every action reads them: the core's.
pub use engine_core::actions::{resolve, song};

use crate::shared_actions::{
    FOLD, FoldArg, PHASE, PhaseArg, SELECT_TRACK, SelectTrackArg, TRACK_SET, TrackSetArg,
};

/// An action run here asked of the window too: the core's.
pub(crate) use engine_core::lane_actions::also_in_the_window;

/// No arguments.
#[cfg(feature = "native")]
#[derive(Facet)]
struct NoArg {}

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
        "SESSION_PANEL_MOVE",
        "SESSION_PANEL_OPEN",
        "SESSION_PANEL_CLOSE",
        "SESSION_PANEL_SHOW",
        "SESSION_PANEL_MAXIMIZE",
        "SESSION_PANEL_TABS",
        "SESSION_DOCK_SPLIT",
        "SESSION_DOCK_EDGE",
        "SESSION_DOCK_FLOAT",
        "AUTOMATION_POINTS_EDIT",
        "AUTOMATION_ITEMS_EDIT",
        "AUTOMATION_LANE_SET",
        "NOTES_WRITE",
        "SESSION_TOGETHER_LEAVE",
        "SESSION_TOGETHER_TRANSPORT",
        "SESSION_TOGETHER_LOCK",
        "LIBRARY_LISTS",
        "LIBRARY_LIST_CREATE",
        "LIBRARY_LIST_RENAME",
        "LIBRARY_LIST_DELETE",
        "LIBRARY_LIST_DUPLICATE",
        "LIBRARY_LIST_SONGS",
        "LIBRARY_SONG_SAVE",
        "LIBRARY_SONG_DELETE",
        "LIBRARY_ORG",
        "ACCOUNT_SIGN_OUT",
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

macro_rules! dock_action {
    ($name:ident, $id:literal, $method:literal, $display:literal, $what:literal) => {
        pub static $name: ActionMeta = meta(
            $id,
            $method,
            $display,
            concat!(
                $what,
                " `intent`: the dock's own words for it (`dock_model::DockIntent`, by panel, stack, split and window ids as SESSION_WINDOW gives them). The window's dock carries it out."
            ),
            "Dock",
            "",
        );
    };
}

dock_action!(
    PANEL_MOVE,
    "SESSION_PANEL_MOVE",
    "panel_move",
    "Move a panel",
    "Move a dock panel — a tab dropped onto a stack, an edge, the centre or a new floating window (`move`), or attached beside another (`attach`)."
);
dock_action!(
    PANEL_OPEN,
    "SESSION_PANEL_OPEN",
    "panel_open",
    "Open a panel",
    "Open a new panel of a kind where the kind lives (`open`), or a copy of one beside it (`duplicate`). SESSION_PANEL toggles a kind instead."
);
dock_action!(
    PANEL_CLOSE,
    "SESSION_PANEL_CLOSE",
    "panel_close",
    "Close a panel",
    "Close one dock panel (`close`)."
);
dock_action!(
    PANEL_SHOW,
    "SESSION_PANEL_SHOW",
    "panel_show",
    "Show a panel",
    "Make a panel the one its stack shows (`show`): a tab pressed, a rail's button."
);
dock_action!(
    PANEL_MAXIMIZE,
    "SESSION_PANEL_MAXIMIZE",
    "panel_maximize",
    "Maximise a panel",
    "A panel filling the window, or back (`maximize`): a tab double-clicked, Restore."
);
dock_action!(
    PANEL_TABS,
    "SESSION_PANEL_TABS",
    "panel_tabs",
    "Show a stack's tabs",
    "Show or hide a stack's tab strip (`chrome`)."
);
dock_action!(
    DOCK_SPLIT,
    "SESSION_DOCK_SPLIT",
    "dock_split",
    "Set a divider",
    "Where a divider between two panels sits (`share`): dragged and let go, or double-clicked back to halves."
);
dock_action!(
    DOCK_EDGE,
    "SESSION_DOCK_EDGE",
    "dock_edge",
    "Set an edge dock",
    "An edge dock's thickness (`edge_size`), or whether it is open, collapsed to a rail or hidden (`edge_shown`)."
);
dock_action!(
    DOCK_FLOAT,
    "SESSION_DOCK_FLOAT",
    "dock_float",
    "Place a floating panel",
    "Where a floating panel window is and how big (`float_bounds`): dragged or resized and let go."
);
