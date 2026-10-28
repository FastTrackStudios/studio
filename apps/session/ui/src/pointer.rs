//! What the pointer is on, and what it is doing to it.
//!
//! The panels are recorded scenes replayed under a transform, so a
//! control cannot change appearance without the whole panel being
//! re-recorded — which is the one thing the architecture exists to
//! avoid. Hover is therefore an OVERLAY: the recorded strip draws the
//! control at rest, and the one control under the pointer is drawn
//! again on top, in its hover or pressed cell.
//!
//! That costs one control per frame instead of a mixer per mouse move,
//! and it is the same pass the meters and the play cursor need — all
//! three are "something live over something recorded".
//!
//! The art already has the states: REAPER's theme sheets carry three
//! cells per control and `Interaction` names them, so hovering is
//! choosing a cell rather than inventing a highlight.

use daw_ui::art::mixer_controls::Interaction;

use crate::mcp::Control;

/// A control on a particular strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Spot {
    pub row: usize,
    pub control: Control,
}

/// A control on a particular track panel row.
///
/// The arrangement's counterpart to [`Spot`]. The two surfaces have
/// different control sets — a strip has a fader, a row has a folder
/// mark — so they cannot share one enum, but everything ABOVE the spot
/// is identical, which is why [`Pointer`] is generic over it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RowSpot {
    pub row: usize,
    pub control: crate::row::Control,
}

/// What the pointer is on and what it is doing.
#[derive(Clone, Copy, Debug)]
pub struct Pointer<S = Spot> {
    hovered: Option<S>,
    pressed: Option<S>,
}

/// Derived `Default` would demand `S: Default`, and a spot has no
/// resting value — "on nothing" is `None`, not a zeroth control.
impl<S> Default for Pointer<S> {
    fn default() -> Self {
        Self {
            hovered: None,
            pressed: None,
        }
    }
}

impl<S: Copy + PartialEq> Pointer<S> {
    /// The pointer moved onto `spot` (or off everything).
    ///
    /// Returns whether anything changed, so a window can skip a redraw
    /// on the mouse moves that do not alter the picture — which is most
    /// of them, since a pointer crossing a strip generates a move per
    /// pixel and changes control perhaps twice.
    pub fn hover(&mut self, spot: Option<S>) -> bool {
        let changed = self.hovered != spot;
        self.hovered = spot;
        changed
    }

    /// The pointer went down on what it was hovering.
    pub fn press(&mut self) {
        self.pressed = self.hovered;
    }

    /// The pointer came up.
    pub fn release(&mut self) {
        self.pressed = None;
    }

    /// How a given control should be drawn.
    ///
    /// Pressed beats hovered, and a control that is pressed keeps
    /// looking pressed while the pointer is dragged off it — that is
    /// what makes "drag away to cancel" legible: the button stays down
    /// to say it is still armed, and releasing elsewhere does nothing.
    #[must_use]
    pub fn state(&self, spot: S) -> Interaction {
        if self.pressed == Some(spot) {
            Interaction::Pressed
        } else if self.hovered == Some(spot) && self.pressed.is_none() {
            Interaction::Hover
        } else {
            Interaction::Normal
        }
    }

    /// The control to redraw, if any — what the overlay pass needs.
    ///
    /// The pressed one when there is one, because a pressed control is
    /// what the user is doing; otherwise the hovered one.
    #[must_use]
    pub fn active(&self) -> Option<(S, Interaction)> {
        self.pressed
            .map(|spot| (spot, Interaction::Pressed))
            .or_else(|| self.hovered.map(|spot| (spot, Interaction::Hover)))
    }

    /// The control a press is holding down, if one is.
    #[must_use]
    pub fn pressed(&self) -> Option<S> {
        self.pressed
    }

    #[must_use]
    pub fn hovered(&self) -> Option<S> {
        self.hovered
    }
}
