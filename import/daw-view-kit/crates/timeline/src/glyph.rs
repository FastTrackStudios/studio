//! The pointer's glyphs, and drawing them.
//!
//! A [`Cursor`] is semantic, not pictorial: the variants name what the
//! gesture *is*, and the surface decides how to draw it. The views that
//! paint their own pointer (the arrangement's window cursor, the MIDI
//! editor's roll) share these, so a bracket over an item end and over a
//! note end is the same bracket. Deciding which glyph a gesture gets is
//! each view's (the editor's `Cursor::for_action`); this only names and
//! draws them.
//!
//! ## Why the glyphs are painted rather than named
//!
//! Blitz maps CSS `cursor` keywords onto winit's `CursorIcon` and
//! supports no `cursor: url(…)`. The keyword set has no `[`, no `]`, no
//! pencil, no razor and no eraser — which is to say it has none of the
//! shapes that carry the information. So the OS cursor is hidden and the
//! glyph is drawn into a scene ([`draw`], feature `paint`), which also
//! means it looks identical standalone, in a plugin editor and in REAPER.

#[cfg(feature = "paint")]
mod draw;
#[cfg(feature = "paint")]
pub use draw::draw;

/// One of a note's seven handles, which each get their own glyph (the MIDI
/// editor's: which handle a press landed on).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    /// The note body — coarse pitch, snapping to the row by default.
    Pitch,
    /// Fine pitch, in cents, ignoring snap.
    FinePitch,
    /// Tilt of the entry transition.
    LeftSlope,
    /// Tilt of the exit transition.
    RightSlope,
    Formant,
    /// Level, and right-click mutes.
    Amplitude,
    /// Vibrato depth — the modulation term's blend.
    Vibrato,
}

impl Handle {
    pub const ALL: [Handle; 7] = [
        Handle::Pitch,
        Handle::FinePitch,
        Handle::LeftSlope,
        Handle::RightSlope,
        Handle::Formant,
        Handle::Amplitude,
        Handle::Vibrato,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Handle::Pitch => "Pitch",
            Handle::FinePitch => "Fine",
            Handle::LeftSlope => "Slope L",
            Handle::RightSlope => "Slope R",
            Handle::Formant => "Formant",
            Handle::Amplitude => "Amplitude",
            Handle::Vibrato => "Vibrato",
        }
    }
}

/// A pointer glyph.
///
/// Semantic, not pictorial: the variants name what the gesture *is*, and
/// the surface decides how to draw it. That split is what lets the same
/// resolution run in three places that cannot share a drawing —
/// `expression-editor-ui` paints these into the Vello scene, a DOM host
/// falls back to [`Cursor::css`], and a test asserts on the variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Cursor {
    /// Nothing to say — chrome, and anything unbound.
    #[default]
    Arrow,
    /// Empty roll, where a press starts a rectangle.
    Crosshair,
    /// Marquee that adds to the selection rather than replacing it.
    MarqueeAdd,
    /// Marquee that flips what it sweeps.
    MarqueeToggle,

    /// `[` — the start edge of a note.
    EdgeLeft,
    /// `]` — the end edge.
    EdgeRight,
    /// A bracket with an arrow through it: the drag scales every
    /// selected note's length, not just this one's.
    StretchLeft,
    StretchRight,

    /// Four-way: the note follows the pointer on both axes.
    Move,
    /// Locked to time.
    MoveH,
    /// Locked to pitch.
    MoveV,
    /// Move that leaves the original behind.
    Copy,

    /// Freehand expression.
    Pencil,
    /// A shaped ramp between two points.
    Curve,
    /// Sweeping notes onto the grid.
    Brush,
    /// Wiping expression back to default.
    Eraser,
    /// Deleting notes.
    NoteEraser,

    /// Panning the view.
    Hand,
    /// The same hand, closed — a pan already under way.
    HandClosed,
    /// Anchored zoom.
    Zoom,
    /// Placing the play cursor.
    Playhead,
    /// Auditioning a row or a note.
    Audition,

    /// Vertical drag sets velocity or level.
    Velocity,
    /// Vertical drag scales a range about its own mean.
    Scale,
    /// Muting.
    Mute,
    /// Typing a syllable onto a note.
    Text,
    /// Splitting at a zone boundary or a timing separator.
    Split,

    /// Cutting out a razor rectangle.
    Razor,
    /// The edge of one.
    RazorEdge,

    /// One of the seven note handles, which each get their own glyph.
    Handle(Handle),

    /// The gesture is blocked — the drawer has the target locked.
    Forbidden,
}

impl Cursor {
    /// The nearest standard CSS keyword.
    ///
    /// A fallback, not the plan: the whole point of the painted glyphs is
    /// that CSS has no `[`, no pencil and no razor, and Blitz supports no
    /// `cursor: url(…)` to supply them. This exists so a DOM host that
    /// cannot paint into the scene still gets *something* directional,
    /// and so the roll element can name a sensible cursor for the frames
    /// before the first paint.
    pub fn css(&self) -> &'static str {
        match self {
            Cursor::Arrow => "default",
            Cursor::Crosshair | Cursor::MarqueeAdd | Cursor::MarqueeToggle => "crosshair",
            Cursor::EdgeLeft | Cursor::EdgeRight => "ew-resize",
            Cursor::StretchLeft | Cursor::StretchRight => "col-resize",
            Cursor::Move => "move",
            Cursor::MoveH => "ew-resize",
            Cursor::MoveV => "ns-resize",
            Cursor::Copy => "copy",
            Cursor::Pencil | Cursor::Curve | Cursor::Brush => "crosshair",
            Cursor::Eraser | Cursor::NoteEraser => "cell",
            Cursor::Hand => "grab",
            Cursor::HandClosed => "grabbing",
            Cursor::Zoom => "zoom-in",
            Cursor::Playhead | Cursor::Split => "col-resize",
            Cursor::Audition => "pointer",
            Cursor::Velocity | Cursor::Scale => "ns-resize",
            Cursor::Mute => "pointer",
            Cursor::Text => "text",
            Cursor::Razor => "crosshair",
            Cursor::RazorEdge => "ew-resize",
            Cursor::Handle(_) => "ns-resize",
            Cursor::Forbidden => "not-allowed",
        }
    }

    /// A short name, for a status readout and for tests to assert on.
    pub fn label(&self) -> &'static str {
        match self {
            Cursor::Arrow => "Arrow",
            Cursor::Crosshair => "Select",
            Cursor::MarqueeAdd => "Add to selection",
            Cursor::MarqueeToggle => "Toggle selection",
            Cursor::EdgeLeft => "Note start",
            Cursor::EdgeRight => "Note end",
            Cursor::StretchLeft => "Stretch from start",
            Cursor::StretchRight => "Stretch from end",
            Cursor::Move => "Move",
            Cursor::MoveH => "Move in time",
            Cursor::MoveV => "Move in pitch",
            Cursor::Copy => "Copy",
            Cursor::Pencil => "Draw",
            Cursor::Curve => "Ramp",
            Cursor::Brush => "Paint",
            Cursor::Eraser => "Erase expression",
            Cursor::NoteEraser => "Erase notes",
            Cursor::Hand | Cursor::HandClosed => "Pan",
            Cursor::Zoom => "Zoom",
            Cursor::Playhead => "Play cursor",
            Cursor::Audition => "Audition",
            Cursor::Velocity => "Velocity",
            Cursor::Scale => "Scale",
            Cursor::Mute => "Mute",
            Cursor::Text => "Lyric",
            Cursor::Split => "Split",
            Cursor::Razor => "Razor",
            Cursor::RazorEdge => "Razor edge",
            Cursor::Handle(h) => h.label(),
            Cursor::Forbidden => "Locked",
        }
    }

    /// The glyph while a gesture is already running.
    ///
    /// Distinct from the hover glyph for one case that matters: a hand
    /// that has closed on the canvas reads as *held*, and every surface
    /// that pans draws it that way. Everything else keeps whatever it
    /// was pressed with, because the cursor changing mid-drag is a
    /// flinch, not information.
    pub fn while_dragging(self) -> Cursor {
        match self {
            Cursor::Hand => Cursor::HandClosed,
            other => other,
        }
    }
}

/// The glyph's box, in CSS pixels. Every shape below is drawn inside
/// `0..SIZE` on both axes and positioned by the layer.
pub const SIZE: f64 = 24.0;

/// Where inside the box the pointer actually is.
///
/// A crosshair is centred; a pencil points from its tip. Getting this
/// wrong is the difference between a cursor that aims and one that
/// hovers a few pixels off whatever you meant to hit.
pub fn hotspot(cursor: Cursor) -> (f64, f64) {
    let c = SIZE * 0.5;
    match cursor {
        // Centred: everything that means "this point".
        Cursor::Crosshair
        | Cursor::MarqueeAdd
        | Cursor::MarqueeToggle
        | Cursor::EdgeLeft
        | Cursor::EdgeRight
        | Cursor::StretchLeft
        | Cursor::StretchRight
        | Cursor::Move
        | Cursor::MoveH
        | Cursor::MoveV
        | Cursor::Velocity
        | Cursor::Scale
        | Cursor::Split
        | Cursor::Playhead
        | Cursor::Handle(_)
        | Cursor::RazorEdge => (c, c),
        // Tip-anchored: the drawing tools aim with their point, which is
        // drawn at the bottom-left of the box.
        Cursor::Pencil | Cursor::Curve | Cursor::Brush => (2.0, SIZE - 2.0),
        // The razor cuts where its line crosses its bar: the centre.
        Cursor::Razor => (c, c),
        // Everything else aims from its top-left, like an arrow.
        _ => (2.0, 2.0),
    }
}
