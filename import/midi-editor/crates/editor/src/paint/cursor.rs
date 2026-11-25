//! Resolving the pointer glyph, and drawing it.
//!
//! [`crate::cursor`] decides what a resolved action
//! *means*; this decides what is under the pointer in the first place,
//! and draws the result.
//!
//! ## Why the glyphs are painted rather than named
//!
//! Blitz maps CSS `cursor` keywords onto winit's `CursorIcon` and
//! supports no `cursor: url(…)` (`blitz-dom::stylo_to_cursor_icon`). The
//! keyword set has no `[`, no `]`, no pencil, no razor and no eraser —
//! which is to say it has none of the shapes that carry the information.
//! `ew-resize` over a note end tells you an edge resizes; it does not
//! tell you *which* end you have, and grabbing the wrong one is the
//! mistake the bracket exists to prevent.
//!
//! So the OS cursor is hidden over the roll and the glyph is drawn into
//! a scene, which also means it looks identical standalone, in a VST3
//! editor and in REAPER — the same reason the roll itself is painted.
//!
//! The layer that puts it on screen in the dioxus editor is
//! `crate::ui::cursor::CursorLayer`; a host that owns its
//! window (the session) rasterizes [`draw`] into a real window cursor.
//!
//! ## Why the UI draws it in its own layer
//!
//! A pointer move is the highest-frequency event the surface sees. If
//! the roll's own component read the hover position, every move would
//! rebuild the whole roll scene — every note, every curve, every label —
//! to move one 24-pixel glyph. [`CursorLayer`] is the only component
//! that reads hover, so a move re-renders that and nothing else, and it
//! sits under `pointer-events: none` so the roll underneath still gets
//! every event.

use crate::Editor;
use crate::cursor::{Aim, Cursor, CursorFor as _};
use crate::mouse::{Action, Context, Gesture};
use crate::tools::{Hit, Mods};
use anyrender::Scene;

use crate::paint::canvas;
use crate::paint::interaction;
use crate::paint::theme;

/// The glyph for the pointer at roll-local `(x, y)`.
///
/// Mirrors [`interaction::pointer_down`]'s precedence exactly, and that
/// is not incidental — a cursor resolved by a different order than the
/// press is a cursor that lies. The order is: chrome, then timing
/// separators, then note handles, then the mouse map.
///
/// `locked` is the drawer's modal state, which blocks editing gestures
/// while leaving navigation alone.
pub fn cursor_at(ed: &Editor, x: f64, y: f64, mods: Mods, locked: bool) -> Cursor {
    // Chrome first: the ruler and the key gutter are outside the roll's
    // coordinate space, and `roll.rs` routes presses there before
    // `interaction` ever sees them.
    if y < 0.0 {
        return Cursor::Playhead;
    }
    if x < 0.0 {
        return Cursor::Audition;
    }

    // Timing separators outrank the notes they sit between, the way
    // `separator_press` does.
    if ed.timing_mode {
        let grab = canvas::SEPARATOR_GRAB_PX;
        if canvas::separators(ed)
            .iter()
            .any(|s| (s.x - x).abs() <= grab)
        {
            return if locked {
                Cursor::Forbidden
            } else {
                Cursor::Split
            };
        }
    }

    // Handles are drawn in front and pressed first, so they are resolved
    // first here too.
    if ed.mode.has_handles()
        && let Some(handle) = canvas::note_handles(ed)
            .iter()
            .find_map(|s| crate::handles::hit(&s.rects, x, y))
    {
        return if locked {
            Cursor::Forbidden
        } else {
            Cursor::for_action(Action::None, Context::Note, Aim::handle(handle))
        };
    }

    let context = interaction::context_at(ed, x, y);
    // `Gesture::Drag` is the honest question to ask the map: a press
    // that never moves is a Click, but nothing knows that yet at hover
    // time, and the drag binding is the one with consequences.
    let action = ed.mouse.resolve_for(context, Gesture::Drag, mods, ed.tool);

    // A *view* tool is never forbidden. `is_edit` is asked of the
    // resolved action, and an armed tool resolves to `ActiveTool` — one
    // answer for all seven tools, which is right for six of them and
    // wrong for zoom. The lock exists to stop you changing the material
    // while a preview is up; looking around is not that.
    if locked && action.is_edit() && !ed.tool.is_view() {
        return Cursor::Forbidden;
    }

    // Which end of a note the pointer has, for the brackets.
    let aim = match ed.hit_test(x, y) {
        Hit::NoteEdge { start_edge, .. } => Aim::edge(start_edge),
        _ => Aim::NONE,
    };

    match action {
        // The map handed the gesture to the armed tool; only the tool
        // knows what that looks like.
        Action::ActiveTool => Cursor::for_tool(ed.tool, context),
        other => Cursor::for_action(other, context, aim),
    }
}

// ── drawing ─────────────────────────────────────────────────────────

/// The glyph's box and where in it the pointer is (`timeline::glyph`).
pub use timeline::glyph::{SIZE, hotspot};

/// Draw `cursor` into `scene` with its hotspot at `(x, y)`, in the
/// brightest text colour, not the accent: the cursor has to stay legible
/// over selected notes, which are painted in the accent.
pub fn draw(scene: &mut Scene, cursor: Cursor, x: f64, y: f64) {
    timeline::glyph::draw(
        scene,
        cursor,
        x,
        y,
        crate::paint::paint::color(theme::TEXT_BRIGHT),
    );
}
