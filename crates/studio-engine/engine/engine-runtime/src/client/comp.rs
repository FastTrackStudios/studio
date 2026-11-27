//! The comp actions a window's controls press. Comping is the engine's
//! (`studio_engine::comp`).

use architect::action::ActionMeta;

pub static SWIPE_COMP: architect::action::ActionMeta = crate::client::actions::meta(
    "TRACK_COMP_FROM_LANE",
    "track_comp_from_lane",
    "Comp from a lane",
    "Swipe comp: lane `lane` (its name — `2`, `C1` — or its number from 1) of `track` (a path, name or GUID; `with`, more tracks swiped with it, its media-edit group) over `start`..`end` seconds put onto the track's comp lane (its comping lane, else the lane named C1 or COMP, else the one that plays): what the comp held there cut away, the lane's audio over exactly that stretch copied on with a 5 ms crossfade either end, what it held either side left, and a comp area recorded. On a folder (`Drums`), every track under it with lanes at once, the same stretch of the same lane. Says what it did, track by track. One undo step.",
    "Edit",
    "",
);

/// `TRACK_SHOW_LANES`: the window's, on every target (a page shows lanes
/// too).
pub use crate::client::actions::SHOW_LANES;
