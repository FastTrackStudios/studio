//! What the pointer looks like, derived from what it would do.
//!
//! REAPER's single best affordance is that the cursor answers "what
//! happens if I press here?" *before* you press — the `[` and `]`
//! brackets at the ends of an item say you are about to grab an edge,
//! the hand says you are about to pan, the pencil says you are about to
//! draw. None of it is a mode readout; it is a preview of the gesture.
//!
//! ## The one rule: the cursor is a function of the action
//!
//! This module never hit-tests and never reads modifiers. It maps
//! [`Action`] — the value [`crate::mouse::MouseMap`] already resolved
//! for the pointer's context, gesture and modifiers — onto a glyph. That
//! is deliberate and it is the whole design:
//!
//! - Rebind `Alt+drag` on a note from Copy to Erase and the cursor
//!   follows on its own. A second table keyed on context and modifiers
//!   would have to be edited in step, and the day it was not, the
//!   surface would start lying about itself.
//! - The host overlay (`reaper-mouse.ini`) and every mode preset get
//!   correct cursors for free, having never heard of this module.
//! - A new [`Action`] is a compile error here, not a silent [`Arrow`].
//!
//! The two things a resolved action cannot say — *which* end of a note
//! is under the pointer, and *which* handle — arrive as [`Aim`].
//!
//! [`Arrow`]: Cursor::Arrow

use crate::mouse::{Action, Context};

/// What the hit test knows that the action does not.
///
/// A `MoveNoteEdge` says nothing about which edge, and both slope
/// handles resolve to the same action; the glyph has to differ or it
/// tells you nothing about which one you grabbed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Aim {
    /// `Some(true)` on a note's start edge, `Some(false)` on its end.
    pub start_edge: Option<bool>,
    /// The note handle under the pointer, when there is one.
    pub handle: Option<Handle>,
}

impl Aim {
    pub const NONE: Aim = Aim {
        start_edge: None,
        handle: None,
    };

    pub fn edge(start: bool) -> Self {
        Self {
            start_edge: Some(start),
            handle: None,
        }
    }

    pub fn handle(handle: Handle) -> Self {
        Self {
            start_edge: None,
            handle: Some(handle),
        }
    }
}

/// A pointer glyph: semantic, not pictorial (`timeline::glyph`, which
/// names and draws them for every view). What a gesture of the editor's
/// gets is [`CursorFor`].
pub use timeline::glyph::Cursor;

use crate::handles::Handle;

/// The glyph the editor's gestures get.
pub trait CursorFor {
    /// The glyph for a resolved action; see the implementation.
    fn for_action(action: Action, context: Context, aim: Aim) -> Cursor;
    /// The glyph an armed tool claims a gesture with.
    fn for_tool(tool: crate::Tool, context: Context) -> Cursor;
}

impl CursorFor for Cursor {
    /// The glyph for a resolved action.
    ///
    /// `context` breaks the handful of ties an action alone cannot: the
    /// same [`Action::SelectNote`] is a crosshair over the roll and an
    /// arrow over a note, and [`Action::ActiveTool`] means whatever the
    /// armed tool means, which only the caller knows — so it is left to
    /// [`Cursor::for_tool`].
    fn for_action(action: Action, context: Context, aim: Aim) -> Cursor {
        // A handle is drawn in front of everything and pressed before
        // everything (`handle_press` runs ahead of the map), so it also
        // outranks whatever the map would have said here.
        if let Some(handle) = aim.handle {
            return Cursor::Handle(handle);
        }

        // Which bracket, for every action that grabs an end.
        let left = aim.start_edge.unwrap_or(true);

        match action {
            Action::None => match context {
                Context::PianoRoll => Cursor::Crosshair,
                _ => Cursor::Arrow,
            },

            // ── the stacked view ─────────────────────────────────────
            //
            // A hit moves only in time, so it takes the same left/right
            // cursor a note edge does: the shape says which axis is
            // live, and offering a four-way arrow would promise a
            // vertical drag that does not exist.
            // r[impl drums.mouse.contexts]
            Action::MoveHit | Action::MoveHitBothEnds => {
                if left {
                    Cursor::EdgeLeft
                } else {
                    Cursor::EdgeRight
                }
            }
            Action::SplitTake => Cursor::Crosshair,
            Action::AddHit => Cursor::Brush,
            Action::RemoveHit => Cursor::Eraser,
            Action::SnapHitToGrid | Action::SelectLane | Action::OpenMicMenu => Cursor::Arrow,

            // ── selection ────────────────────────────────────────────
            Action::MarqueeSelect | Action::SelectTouched => Cursor::Crosshair,
            Action::MarqueeAdd
            | Action::AddNoteToSelection
            | Action::SelectNoteAndLater
            | Action::SelectNoteAndLaterSameRow
            | Action::SelectAllInMeasure => Cursor::MarqueeAdd,
            Action::MarqueeToggle | Action::ToggleSelectTouched | Action::ToggleNoteSelection => {
                Cursor::MarqueeToggle
            }
            Action::SelectNote | Action::DeselectAll => match context {
                Context::PianoRoll => Cursor::Crosshair,
                _ => Cursor::Arrow,
            },
            Action::SelectRow => Cursor::Audition,

            // ── note creation ────────────────────────────────────────
            Action::InsertNote
            | Action::InsertNoteNoSnap
            | Action::InsertNoteDragToExtend
            | Action::InsertNoteDragToExtendNoSnap
            | Action::InsertNoteDragToMove
            | Action::InsertNoteDragToEditVelocity => Cursor::Pencil,
            Action::PaintNotes | Action::PaintNotesNoSnap | Action::PaintRowOfNotes => {
                Cursor::Brush
            }

            // ── note editing ─────────────────────────────────────────
            Action::MoveNote
            | Action::MoveNoteNoSnap
            | Action::MoveNoteOneAxis
            | Action::MoveNoteIgnoringSelection => Cursor::Move,
            Action::MoveNoteHorizontally => Cursor::MoveH,
            Action::MoveNoteVertically | Action::TransposeSnapped => Cursor::MoveV,
            Action::CopyNote | Action::CopyNoteNoSnap => Cursor::Copy,
            Action::MoveNoteEdge | Action::MoveNoteEdgeNoSnap => {
                if left {
                    Cursor::EdgeLeft
                } else {
                    Cursor::EdgeRight
                }
            }
            // Length and position stretching both grab an end and pull
            // the whole selection with it, so both take the arrowed
            // bracket — the extra arrow is exactly the "this reaches
            // past the note you grabbed" warning.
            Action::StretchNotes | Action::StretchNotePositions => {
                if left {
                    Cursor::StretchLeft
                } else {
                    Cursor::StretchRight
                }
            }
            Action::DoubleNoteLength | Action::HalveNoteLength => {
                if left {
                    Cursor::EdgeLeft
                } else {
                    Cursor::EdgeRight
                }
            }
            Action::EditNoteVelocity | Action::EditNoteVelocityFine => Cursor::Velocity,
            Action::EraseNote | Action::EraseNotes => Cursor::NoteEraser,
            Action::ToggleNoteMute => Cursor::Mute,
            Action::SetNoteChannelHigher | Action::SetNoteChannelLower => Cursor::MoveV,

            // ── expression ───────────────────────────────────────────
            // `ActiveTool` is deliberately the armed tool's glyph and
            // not a generic one: see `for_tool`, which the caller
            // reaches for instead.
            Action::ActiveTool => Cursor::Arrow,
            Action::PenOverride | Action::EditCcEvents => Cursor::Pencil,
            Action::DrawCcLine => Cursor::Curve,
            Action::EraseCcEvents => Cursor::Eraser,
            Action::ScaleExpression | Action::ScaleCcEvents => Cursor::Scale,

            // ── domain-specific ──────────────────────────────────────
            Action::EditLyric => Cursor::Text,
            Action::CycleArticulation | Action::CycleString => Cursor::Audition,

            // ── razor ────────────────────────────────────────────────
            Action::RazorCreate | Action::RazorAddArea => Cursor::Razor,
            Action::RazorMoveContents
            | Action::RazorMoveContentsNoSnap
            | Action::RazorMoveAreaOnly => Cursor::Move,
            Action::RazorCopyContents => Cursor::Copy,
            Action::RazorMoveVertically => Cursor::MoveV,
            Action::RazorMoveHorizontally => Cursor::MoveH,
            Action::RazorStretchContents | Action::RazorResizeArea => Cursor::RazorEdge,
            Action::RazorDeleteContents | Action::RazorRemoveArea | Action::RazorClearAll => {
                Cursor::NoteEraser
            }

            // ── navigation ───────────────────────────────────────────
            Action::Pan => Cursor::Hand,
            Action::ZoomAnchored => Cursor::Zoom,
            Action::MovePlayhead | Action::MovePlayheadNoSnap | Action::TimeSelect => {
                Cursor::Playhead
            }
            Action::ContextMenu => Cursor::Arrow,
            Action::Audition => Cursor::Audition,
        }
    }

    /// The glyph an armed tool claims a gesture with.
    ///
    /// Reached only for [`Action::ActiveTool`], which is the map saying
    /// "the tool owns this one" — see [`crate::mouse::MouseMap::resolve_for`].
    fn for_tool(tool: crate::Tool, context: Context) -> Cursor {
        match tool {
            crate::Tool::Select => match context {
                Context::PianoRoll => Cursor::Crosshair,
                _ => Cursor::Arrow,
            },
            crate::Tool::Pen => Cursor::Pencil,
            crate::Tool::Curve => Cursor::Curve,
            crate::Tool::Eraser => Cursor::Eraser,
            crate::Tool::NoteDraw => Cursor::Pencil,
            crate::Tool::NoteErase => Cursor::NoteEraser,
            crate::Tool::Razor => Cursor::Razor,
            crate::Tool::Velocity => Cursor::Velocity,
            crate::Tool::Zoom => Cursor::Zoom,
        }
    }
}
