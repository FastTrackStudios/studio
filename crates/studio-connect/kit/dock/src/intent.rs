//! What a dock control asks for, said once: a [`DockIntent`].
//!
//! A control in the dock host (a tab dragged and dropped, a divider let go,
//! a screenset recalled, a panel closed) does not change the layout itself:
//! it asks the [`Dock`](crate::host::Dock) for an intent. An app may route
//! intents (`Dock::set_router`) — the Session window runs each as a named
//! action, so a guide, a test or the CLI sees and can make the same change —
//! and hand them back to `Dock::apply`; with no router, the dock applies
//! them itself. Serialisable, so an intent travels as an action's
//! arguments; free of any app, so the dock depends on none.

use facet::Facet;

use crate::dockspace::{
    Chrome, DockError, Dockspace, FloatId, InstanceId, Panel, Place, Rect, Share, Shown, Side,
    SplitId, StackId, Zone,
};
use crate::screenset::ScreensetId;

/// One change a dock control makes.
#[derive(Debug, Clone, PartialEq, Facet)]
#[facet(rename_all = "snake_case")]
#[repr(u8)]
pub enum DockIntent {
    /// Panel `panel` moved to `place`.
    Move { panel: InstanceId, place: Place },
    /// Panel `panel` attached beside panel `beside`, on `side`, `px` thick.
    Attach {
        panel: InstanceId,
        beside: InstanceId,
        side: Side,
        px: f32,
    },
    /// A new panel of `kind`, where the kind lives.
    Open { kind: String },
    /// A copy of panel `panel`, named `copy`, split off to its right.
    Duplicate { panel: InstanceId, copy: InstanceId },
    /// Panel `panel` closed.
    Close { panel: InstanceId },
    /// Panel `panel` the one its stack shows.
    Show { panel: InstanceId },
    /// Panel `panel` filling the window, or back (the other way).
    Maximize { panel: InstanceId },
    /// Stack `stack`'s tab strip shown or not.
    Chrome { stack: StackId, chrome: Chrome },
    /// Split `split` divided at `share`.
    Share { split: SplitId, share: Share },
    /// The dock on `side` this thick, px.
    EdgeSize { side: Side, size: f32 },
    /// The dock on `side` open, collapsed to a rail, or hidden.
    EdgeShown { side: Side, shown: Shown },
    /// Floating window `float` at `bounds`.
    FloatBounds { float: FloatId, bounds: Rect },
    /// Screenset `set` recalled.
    Recall { set: ScreensetId },
    /// The layout saved into the screenset on show.
    Save,
    /// The layout saved as a new screenset, `name`d.
    SaveAs { name: String },
    /// Arrange turned on or off (panels grabbed anywhere).
    Arrange,
}

impl DockIntent {
    /// Carry out a change of the layout on `space`. `Ok(false)` for an
    /// intent that is not one (open, recall, save, arrange: the host's).
    ///
    /// # Errors
    /// What the dockspace refuses (no such panel, stack, split or window).
    pub fn apply_to(&self, space: &mut Dockspace) -> Result<bool, DockError> {
        match self {
            Self::Move { panel, place } => space.move_panel(*panel, *place)?,
            Self::Attach {
                panel,
                beside,
                side,
                px,
            } => space.attach(*panel, *beside, *side, *px)?,
            Self::Duplicate { panel, copy } => {
                let Some(original) = space.panel(*panel).cloned() else {
                    return Err(DockError::NoPanel(*panel));
                };
                let Some((_, stack)) = space.locate(*panel) else {
                    return Err(DockError::NoPanel(*panel));
                };
                let duplicate = Panel {
                    id: *copy,
                    title: None,
                    ..original
                };
                let place = Place::Stack(stack, Zone::Split(Side::Right, Share::Ratio(0.5)));
                space.add(duplicate, place)?;
            }
            Self::Close { panel } => {
                space.close(*panel)?;
            }
            Self::Show { panel } => space.activate(*panel)?,
            Self::Maximize { panel } => space.toggle_maximized(*panel)?,
            Self::Chrome { stack, chrome } => space.set_chrome(*stack, *chrome)?,
            Self::Share { split, share } => space.set_share(*split, *share)?,
            Self::EdgeSize { side, size } => space.set_edge_size(*side, *size),
            Self::EdgeShown { side, shown } => space.set_shown(*side, *shown),
            Self::FloatBounds { float, bounds } => space.set_float_bounds(*float, *bounds)?,
            Self::Open { .. } | Self::Recall { .. } | Self::Save | Self::SaveAs { .. } => {
                return Ok(false);
            }
            Self::Arrange => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An intent says itself as JSON and reads back as it was.
    #[test]
    fn an_intent_travels_as_json() {
        let panel = InstanceId::new();
        for intent in [
            DockIntent::Move {
                panel,
                place: Place::Edge(Side::Left),
            },
            DockIntent::Share {
                split: SplitId::new(),
                share: Share::Ratio(0.3),
            },
            DockIntent::EdgeShown {
                side: Side::Bottom,
                shown: Shown::Collapsed,
            },
            DockIntent::Recall {
                set: ScreensetId::new(),
            },
            DockIntent::Arrange,
        ] {
            let json = facet_json::to_string(&intent).expect("it says itself");
            let back: DockIntent = facet_json::from_str(&json).expect("it reads back");
            assert_eq!(back, intent, "{json}");
        }
    }

    /// A panel moved, shown and closed by intent, as by hand.
    #[test]
    fn a_layout_changes_by_intent() {
        let mut space = Dockspace::default();
        let id = space
            .add(Panel::new("mixer"), Place::Center)
            .expect("a panel in the centre");
        let moved = DockIntent::Move {
            panel: id,
            place: Place::Edge(Side::Bottom),
        };
        assert_eq!(moved.apply_to(&mut space), Ok(true));
        assert_eq!(space.edge_of(id), Some(Side::Bottom));
        assert_eq!(
            DockIntent::Close { panel: id }.apply_to(&mut space),
            Ok(true)
        );
        assert!(space.panel(id).is_none());
        assert_eq!(DockIntent::Save.apply_to(&mut space), Ok(false));
    }
}
