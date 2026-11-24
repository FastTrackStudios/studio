//! What the app asks a view to show: the parts of the arrangement (and,
//! later, the mixer) an app's way of working turns on or off.
//!
//! A view does not know the app's modes (Studio's Organize, Record, Edit,
//! Mix, Live): the app maps its mode onto [`Shows`] and provides it as a
//! `Memo<Shows>` context; a view reads it with [`use_shows`]. A view with
//! no app around it shows [`Shows::default`].

use dioxus::prelude::*;

/// The parts of the views an app turns on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shows {
    /// The track panel's record-arm buttons.
    pub arm: bool,
    /// Every item in its track's colour.
    pub item_colors: bool,
    /// Every cut on a folder's row, what the drum detection found drawn
    /// over the kit, and the comp strip to pick takes on.
    pub cuts: bool,
}

impl Default for Shows {
    /// A view on its own: the arm buttons, nothing else.
    fn default() -> Self {
        Self {
            arm: true,
            item_colors: false,
            cuts: false,
        }
    }
}

/// What the app asks this view to show now; [`Shows::default`] with no
/// app. Read every render, so a change re-renders.
#[must_use]
pub fn use_shows() -> Shows {
    try_use_context::<Memo<Shows>>().map_or_else(Shows::default, |shows| shows())
}
