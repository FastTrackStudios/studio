//! What a gesture means where it lands.
//!
//! REAPER's mouse behaviour is a table: for each CONTEXT — a media
//! item, its left edge, a fade, the ruler, the empty arrange area —
//! and each modifier combination, an action. [`crate::hit`] answers
//! "what did you touch, and in which context"; this answers "and what
//! does that do", so the two stay separately configurable: a hit test
//! is geometry, a mouse map is preference.
//!
//! The table here is the FTS default. A mouse profile
//! (`keybinds::proto::MouseProfileConfig`) is the same shape — a
//! context, a modifier set, a behaviour — and is where a user's
//! overrides will load into once the profile system reaches this
//! window; the contexts are already its contexts.

use keybinds::proto::MouseModifierContext as Context;

/// The keys held during a gesture: the Editor's too.
pub use timeline::input::Mods;

/// The gesture itself.
pub use timeline::mouse::Gesture;

/// What a gesture does, and whether the grid applies to it.
///
/// Two answers and not one, because "move the item" and "move the item
/// off the grid" are the same verb done differently — pairing them as
/// separate actions doubles the table for every draggable thing and
/// then doubles it again for the next modifier. The verb is what the
/// gesture MEANS; the snap is how precisely it is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bound {
    pub action: Action,
    /// Whether the result lands on the grid: the toolbar's snapping,
    /// flipped by Shift. Shift means "the other way" on everything that
    /// can be dragged — off when snapping is on, on when it is off — the
    /// one convention worth keeping wholesale, because a modifier that
    /// means different things on different objects is a modifier nobody
    /// learns.
    pub snap: bool,
}

/// What a gesture does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Move the edit cursor (and the transport) to the time.
    SetEditCursor,
    /// Drag out a time selection.
    TimeSelection,
    /// Drag the fade-in's length.
    FadeIn,
    /// Drag the fade-out's length.
    FadeOut,
    /// Change the fade's shape rather than its length.
    FadeShape,
    /// Move the item along its lane.
    MoveItem,
    /// Leave the item where it is and drag a copy of it.
    CopyItem,
    /// Leave the item where it is and drag its CONTENTS inside it.
    SlipItem,
    /// Trim the item's left edge.
    TrimLeft,
    /// Trim the item's right edge.
    TrimRight,
    /// Select the item, and nothing else.
    SelectItem,
    /// Add the item to the selection, or take it out again — the
    /// modified click, for building a selection up a piece at a time.
    ToggleItemSelection,
    /// Sweep a box over empty lanes, selecting the items it touches —
    /// the Editor's marquee over notes, with the same rules
    /// (`timeline::gestures::Marquee`).
    MarqueeSelect,
    /// The same, adding to the selection.
    MarqueeAdd,
    /// Draw a razor area out of the drag.
    RazorArea,
    /// Move a razor area already drawn, by its middle.
    MoveRazorArea,
    /// Resize a razor area by one edge.
    ResizeRazorArea,
    /// Take a razor area out of the set.
    RemoveRazorArea,
    /// Nothing bound here.
    Nothing,
}

/// The context for a razor area already on screen.
///
/// `Custom` rather than a new variant on `MouseModifierContext`: the
/// enum lives in the daw repo and this is the arrangement's own
/// surface, which is exactly the case `Custom` was added for. REAPER
/// models razor areas as their own contexts too — area left drag, area
/// click, area edge — so the shape matches what a profile will
/// eventually load into.
pub const RAZOR_AREA: Context = Context::Custom("razor-area");

/// The context for a razor area's edge — the Editor's `RazorEdge`.
pub const RAZOR_EDGE: Context = Context::Custom("razor-edge");

/// The FTS default: what each context does under each gesture.
///
/// Stated as a table so it reads like the preference page it will
/// become, and so a profile override is a row that wins over one of
/// these rather than a branch somewhere in the window.
#[must_use]
pub fn resolve(context: Context, gesture: Gesture, mods: Mods) -> Bound {
    // The armed Razor takes the plain drags it claims, ahead of the
    // table — the Editor's tools' rule (`timeline::mouse::ToolClaims`).
    let armed = crate::options::RAZOR.get()
        && timeline::input::ModKey::of(mods) == timeline::input::ModKey::NONE
        && timeline::mouse::ToolClaims::claims(&RazorTool, &context, gesture);
    Bound {
        action: if armed {
            Action::RazorArea
        } else {
            verb(context, gesture, mods)
        },
        // Shift flips the toolbar's snapping, everywhere something can
        // be dragged. On a click there is nothing to snap, so the answer
        // is the harmless one.
        snap: crate::options::SNAP.get() != mods.shift,
    }
}

/// The arrangement's Razor tool ([`crate::options::RAZOR`]): it claims a
/// plain drag over the lanes — empty ground, an item's body, its edges,
/// its fade handles. Not the ruler, which selects time, and not a razor
/// area or its edge, which already answer a drag by moving or resizing.
pub struct RazorTool;

impl timeline::mouse::ToolClaims<Context> for RazorTool {
    fn claims(&self, context: &Context, gesture: Gesture) -> bool {
        gesture == Gesture::Drag
            && matches!(
                context,
                Context::ArrangeView
                    | Context::MediaItemBottomHalf
                    | Context::MediaItemLeftEdge
                    | Context::MediaItemRightEdge
                    | Context::MediaItemFade
            )
    }
}

/// What the gesture means, before the question of precision: a row of
/// [`table`] for the exact chord, else the plain gesture's.
fn verb(context: Context, gesture: Gesture, mods: Mods) -> Action {
    timeline::mouse::lookup(&TABLE, &context, gesture, timeline::input::ModKey::of(mods))
        .unwrap_or(Action::Nothing)
}

static TABLE: std::sync::LazyLock<Vec<Binding>> = std::sync::LazyLock::new(table);

/// One row of the arrangement's map.
pub type Binding = timeline::mouse::Binding<Context, Action>;

/// The FTS default, row by row. A chord not listed does what the plain
/// gesture does ([`timeline::mouse::lookup`]).
///
/// The razor is on Ctrl over empty ground and over an item's body and
/// edges: a razor with a dead zone at every item boundary would be one
/// that depends on where items begin and end, which is the one thing it
/// is defined as not doing. Not the plain drag (a marquee over empty
/// lanes, as on the Editor's empty roll — time is selected on the
/// ruler), and not Alt (the zoom tool's sweep while `z` is held). The
/// fade handles are the exception, knowingly: Ctrl there is the fade's
/// SHAPE, as REAPER puts the curve on Ctrl, and Shift stays "ignore the
/// grid" as on every other draggable thing.
///
/// Copy is on Alt, not on Ctrl as in REAPER: Ctrl is the razor, which
/// has the better claim — it has to start anywhere over the lanes,
/// including on an item's edges, where copy only needs the body. Slip
/// (REAPER's plain Alt-drag) takes Ctrl+Alt, what is left.
///
/// An area already drawn answers for itself: drag moves it, click takes
/// it out; its edge resizes it.
#[must_use]
pub fn table() -> Vec<Binding> {
    use Action as A;
    use Context as C;
    use Gesture as G;
    use timeline::input::ModKey;
    const N: ModKey = ModKey::NONE;
    const S: ModKey = ModKey::SHIFT;
    const SA: ModKey = ModKey::from_bits(5);
    // Every chord with Ctrl in it, and those with Ctrl and Alt.
    const CTRL: [ModKey; 4] = [
        ModKey::CTRL,
        ModKey::from_bits(3),
        ModKey::from_bits(6),
        ModKey::from_bits(7),
    ];
    const CTRL_ALT: [ModKey; 2] = [ModKey::from_bits(6), ModKey::from_bits(7)];
    const CTRL_NOT_ALT: [ModKey; 2] = [ModKey::CTRL, ModKey::from_bits(3)];
    const ALT: [ModKey; 2] = [ModKey::ALT, ModKey::from_bits(5)];

    let b = |context, gesture, mods, action| Binding {
        context,
        gesture,
        mods,
        action,
    };
    let mut rows = vec![
        // The empty arrange area: click places the cursor, drag is a
        // marquee, Shift adds to it.
        b(C::ArrangeView, G::Click, N, A::SetEditCursor),
        b(C::ArrangeView, G::Drag, N, A::MarqueeSelect),
        b(C::ArrangeView, G::Drag, S, A::MarqueeAdd),
        b(C::ArrangeView, G::Drag, SA, A::MarqueeAdd),
        // The ruler: a click places the cursor, a drag selects time.
        b(C::Ruler, G::Click, N, A::SetEditCursor),
        b(C::Ruler, G::Drag, N, A::TimeSelection),
        // A razor area and its edges.
        b(RAZOR_AREA, G::Drag, N, A::MoveRazorArea),
        b(RAZOR_EDGE, G::Drag, N, A::ResizeRazorArea),
        b(RAZOR_AREA, G::Click, N, A::RemoveRazorArea),
        b(RAZOR_EDGE, G::Click, N, A::RemoveRazorArea),
        // A fade's handle: its length, with Ctrl its shape.
        b(C::MediaItemFade, G::Drag, N, A::FadeIn),
        // An item: click selects, drag moves, its edges trim.
        b(C::MediaItemBottomHalf, G::Click, N, A::SelectItem),
        b(C::MediaItemBottomHalf, G::Drag, N, A::MoveItem),
        b(C::MediaItemLeftEdge, G::Drag, N, A::TrimLeft),
        b(C::MediaItemRightEdge, G::Drag, N, A::TrimRight),
    ];
    for m in CTRL {
        rows.push(b(C::ArrangeView, G::Drag, m, A::RazorArea));
        rows.push(b(C::MediaItemLeftEdge, G::Drag, m, A::RazorArea));
        rows.push(b(C::MediaItemRightEdge, G::Drag, m, A::RazorArea));
        rows.push(b(C::MediaItemFade, G::Drag, m, A::FadeShape));
        rows.push(b(
            C::MediaItemBottomHalf,
            G::Click,
            m,
            A::ToggleItemSelection,
        ));
    }
    for m in CTRL_NOT_ALT {
        rows.push(b(C::MediaItemBottomHalf, G::Drag, m, A::RazorArea));
    }
    for m in CTRL_ALT {
        rows.push(b(C::MediaItemBottomHalf, G::Drag, m, A::SlipItem));
    }
    for m in ALT {
        rows.push(b(C::MediaItemBottomHalf, G::Drag, m, A::CopyItem));
    }
    rows
}
