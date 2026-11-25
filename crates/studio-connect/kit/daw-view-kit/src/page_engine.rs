//! The engine a page runs in its own loop, as the views in it reach it: a
//! page has no link and no engine runtime, so a view hands its edits to
//! the page's engine and reads its playhead from it. The app wraps its
//! page engine in [`PageEngine`].

use std::rc::Rc;

/// A page's engine, as a view uses it.
pub trait PageEngine {
    /// Apply `edit` (a view's gesture, a fader turned).
    fn send(&self, edit: crate::engine::Edit);
    /// Where the playhead is, seconds.
    fn position(&self) -> f64;
}

/// A page's engine, shared: compared by identity, as a component prop.
#[derive(Clone)]
pub struct PageEngineRef(pub Rc<dyn PageEngine>);

impl PartialEq for PageEngineRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl PageEngineRef {
    /// Apply `edit`.
    pub fn send(&self, edit: crate::engine::Edit) {
        self.0.send(edit);
    }

    /// Where the playhead is, seconds.
    #[must_use]
    pub fn position(&self) -> f64 {
        self.0.position()
    }
}
