//! Where on a view a mouse gesture lands, as the mouse map binds it: the
//! DAW's mouse contexts (REAPER's names), and any a view names itself.

/// Where a mouse gesture lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MouseContext {
    /// An item's left edge.
    #[default]
    MediaItemLeftEdge,
    /// An item's right edge.
    MediaItemRightEdge,
    /// An item's lower half.
    MediaItemBottomHalf,
    /// An item's fade or crossfade.
    MediaItemFade,
    /// An envelope point.
    EnvelopePoint,
    /// An envelope segment.
    EnvelopeSegment,
    /// The track control panel.
    TrackControlPanel,
    /// The arrange view's empty area.
    ArrangeView,
    /// A MIDI note.
    MidiNote,
    /// A MIDI CC lane.
    MidiCCLane,
    /// The timeline ruler, where the edit cursor is set and a time
    /// selection dragged.
    Ruler,
    /// A mixer strip's body.
    MixerStrip,
    /// A context a view names itself.
    Custom(&'static str),
}
