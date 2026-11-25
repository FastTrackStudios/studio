//! The mouse map: what a gesture does, where, under which modifiers — a
//! table rather than a `match`, so a row can be rebound, overlaid from a
//! profile (REAPER's `reaper-mouse.ini`), or listed on a cheat sheet.
//!
//! Generic over what a surface calls a place (`C`, the context the
//! pointer is in) and a verb (`A`, what it does): the arrangement's items
//! and razor areas, the Editor's notes, keys and lanes.

use std::borrow::Cow;

use crate::input::{ModKey, Mods};

/// The gesture itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gesture {
    Click,
    DoubleClick,
    Drag,
    RightClick,
    MiddleClick,
}

/// One row: in `context`, `gesture` with exactly `mods` held does
/// `action`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding<C, A> {
    pub context: C,
    pub gesture: Gesture,
    pub mods: ModKey,
    pub action: A,
}

/// The row for exactly this context, gesture and chord, if there is one.
pub fn find<C: PartialEq, A: Copy>(
    bindings: &[Binding<C, A>],
    context: &C,
    gesture: Gesture,
    mods: ModKey,
) -> Option<A> {
    bindings
        .iter()
        .find(|b| b.context == *context && b.gesture == gesture && b.mods == mods)
        .map(|b| b.action)
}

/// What a press does: the row for its exact chord, else the unmodified
/// row for the same context and gesture, else nothing.
///
/// Falling back matters: without it every unbound chord is dead, and a
/// user who happens to be holding Shift gets nothing instead of the
/// obvious thing.
pub fn lookup<C: PartialEq, A: Copy>(
    bindings: &[Binding<C, A>],
    context: &C,
    gesture: Gesture,
    mods: ModKey,
) -> Option<A> {
    find(bindings, context, gesture, mods)
        .or_else(|| find(bindings, context, gesture, ModKey::NONE))
}

/// Add a row, or replace the one for the same context, gesture and chord.
pub fn bind<C: PartialEq, A>(bindings: &mut Vec<Binding<C, A>>, binding: Binding<C, A>) {
    match bindings.iter_mut().find(|b| {
        b.context == binding.context && b.gesture == binding.gesture && b.mods == binding.mods
    }) {
        Some(b) => b.action = binding.action,
        None => bindings.push(binding),
    }
}

/// Which plain gestures an armed tool takes for itself, ahead of the map.
/// A tool claims unmodified gestures only: claiming `Ctrl+drag` would
/// silently take whatever the user put there.
pub trait ToolClaims<C> {
    fn claims(&self, context: &C, gesture: Gesture) -> bool;
}

/// A named table of bindings.
#[derive(Clone, Debug, PartialEq)]
pub struct MouseMap<C, A> {
    pub name: Cow<'static, str>,
    bindings: Vec<Binding<C, A>>,
}

impl<C: PartialEq, A: Copy> MouseMap<C, A> {
    #[must_use]
    pub fn new(name: impl Into<Cow<'static, str>>, bindings: Vec<Binding<C, A>>) -> Self {
        Self {
            name: name.into(),
            bindings,
        }
    }

    /// What a press does ([`lookup`]), or `none` where nothing is bound.
    pub fn resolve(&self, context: &C, gesture: Gesture, mods: Mods, none: A) -> A {
        lookup(&self.bindings, context, gesture, ModKey::of(mods)).unwrap_or(none)
    }

    /// [`MouseMap::resolve`], an armed tool first taking the plain
    /// gestures it claims (as `claimed`).
    pub fn resolve_for(
        &self,
        context: &C,
        gesture: Gesture,
        mods: Mods,
        tool: &impl ToolClaims<C>,
        (claimed, none): (A, A),
    ) -> A {
        if ModKey::of(mods) == ModKey::NONE && tool.claims(context, gesture) {
            return claimed;
        }
        self.resolve(context, gesture, mods, none)
    }

    #[must_use]
    pub fn bindings(&self) -> &[Binding<C, A>] {
        &self.bindings
    }

    /// Add or replace one binding.
    pub fn set(&mut self, binding: Binding<C, A>) {
        bind(&mut self.bindings, binding);
    }

    /// Every binding for one context, for a cheat sheet.
    pub fn for_context<'a>(&'a self, context: &'a C) -> impl Iterator<Item = &'a Binding<C, A>> {
        self.bindings.iter().filter(move |b| b.context == *context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Place {
        Empty,
        Body,
    }

    fn map() -> MouseMap<Place, &'static str> {
        let b = |context, gesture, mods, action| Binding {
            context,
            gesture,
            mods,
            action,
        };
        MouseMap::new(
            "test",
            vec![
                b(Place::Empty, Gesture::Drag, ModKey::NONE, "marquee"),
                b(Place::Empty, Gesture::Drag, ModKey::CTRL, "razor"),
                b(Place::Body, Gesture::Drag, ModKey::NONE, "move"),
            ],
        )
    }

    fn mods(bits: u8) -> Mods {
        ModKey::from_bits(bits).into()
    }

    #[test]
    fn an_unbound_chord_falls_back_to_the_plain_gesture() {
        let m = map();
        assert_eq!(
            m.resolve(&Place::Empty, Gesture::Drag, mods(2), "-"),
            "razor"
        );
        assert_eq!(
            m.resolve(&Place::Empty, Gesture::Drag, mods(1), "-"),
            "marquee"
        );
        assert_eq!(m.resolve(&Place::Body, Gesture::Click, mods(0), "-"), "-");
    }

    #[test]
    fn set_replaces_the_row_for_the_same_chord() {
        let mut m = map();
        m.set(Binding {
            context: Place::Body,
            gesture: Gesture::Drag,
            mods: ModKey::NONE,
            action: "copy",
        });
        assert_eq!(m.resolve(&Place::Body, Gesture::Drag, mods(0), "-"), "copy");
        assert_eq!(m.for_context(&Place::Body).count(), 1);
    }

    struct Pen;
    impl ToolClaims<Place> for Pen {
        fn claims(&self, context: &Place, gesture: Gesture) -> bool {
            *context == Place::Empty && gesture == Gesture::Drag
        }
    }

    #[test]
    fn a_tool_claims_only_plain_gestures() {
        let m = map();
        let r = |bits| {
            m.resolve_for(
                &Place::Empty,
                Gesture::Drag,
                mods(bits),
                &Pen,
                ("tool", "-"),
            )
        };
        assert_eq!(r(0), "tool");
        assert_eq!(r(2), "razor");
    }
}
