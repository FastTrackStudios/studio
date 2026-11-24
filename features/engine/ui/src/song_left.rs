//! Each open song as its arrangement last had it, kept across the panels
//! built for it and edited from outside one (Mix mode muting the guide):
//! what an arrangement plans its rows from when it is shown again.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

thread_local! {
    /// Each open song as its arrangement last had it — the tracks with the
    /// mutes, renames and visibility changed in it — by the song (held
    /// weakly: a song closed is forgotten). A panel rebuilt on the way
    /// back from another view starts from this, not from the song as it
    /// opened.
    static LEFT: RefCell<Vec<(std::sync::Weak<daw_ui::studio::project::Project>, Left)>> =
        const { RefCell::new(Vec::new()) };
}

/// A song as its arrangement has it, shared by every arrangement built for
/// it ([`song_as_left`]).
pub type Left = Rc<RefCell<daw_ui::studio::project::Project>>;

thread_local! {
    /// How many times a song's kept copy was edited from outside an
    /// arrangement ([`edit_song`]): one showing it compares this with the
    /// value it planned at, as it does the folds.
    static EDITED: Cell<u64> = const { Cell::new(0) };
}

/// Change `planner`'s song as the arrangement has it, from outside one —
/// Mix mode muting the guide. `f` says what changed (`Some`), and an
/// arrangement showing the song plans its rows again; `None`, nothing did.
pub fn edit_song<R>(
    planner: &crate::song_model::Planner,
    f: impl FnOnce(&mut daw_ui::studio::project::Project) -> Option<R>,
) -> Option<R> {
    let raw = song_as_left(planner);
    let out = f(&mut raw.borrow_mut());
    if out.is_some() {
        EDITED.with(|edited| edited.set(edited.get().wrapping_add(1)));
    }
    out
}

/// `planner`'s song as its arrangement left it, kept from now on.
pub fn song_as_left(planner: &crate::song_model::Planner) -> Left {
    LEFT.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|(song, _)| song.strong_count() > 0);
        if let Some((_, raw)) = all
            .iter()
            .find(|(song, _)| std::ptr::eq(song.as_ptr(), std::sync::Arc::as_ptr(&planner.raw)))
        {
            return Rc::clone(raw);
        }
        let raw = Rc::new(RefCell::new((*planner.raw).clone()));
        all.push((std::sync::Arc::downgrade(&planner.raw), Rc::clone(&raw)));
        raw
    })
}

/// The rows to open `planner`'s arrangement with, when it has been shown
/// before.
///
/// Planned again from the song as it was left, with the window's folds as
/// they are now. `None` the first time — the rows the song opened with
/// are those.
#[must_use]
pub fn rows_as_left(
    planner: &crate::song_model::Planner,
) -> Option<(daw_ui::studio::ProjectRef, daw_ui::studio::RowsRef)> {
    let raw = LEFT.with(|all| {
        all.borrow()
            .iter()
            .find(|(song, _)| std::ptr::eq(song.as_ptr(), std::sync::Arc::as_ptr(&planner.raw)))
            .map(|(_, raw)| Rc::clone(raw))
    })?;
    let as_left = planner.plan(&raw.borrow());
    Some(as_left)
}

/// How many times a song's kept copy has been edited from outside an
/// arrangement ([`edit_song`]).
#[must_use]
pub fn edited() -> u64 {
    EDITED.with(Cell::get)
}
