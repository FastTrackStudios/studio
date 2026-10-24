//! What Studio's home screen hands Session before it opens it: the songs
//! opened lately, and what to do first when the start screen comes up —
//! open a song ([`open_next`]), or join a set that is happening elsewhere
//! ([`join_on_start`]).

pub use session_daw::projects::{Recent, ago, open_next, recent};

static JOIN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Join `link` (a live link, a REAPER bridge's or an engine's address, or
/// `demo`) as soon as the start screen is shown.
pub fn join_on_start(link: String) {
    *JOIN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(link);
}

/// What was asked to be joined, once.
pub(super) fn take_join() -> Option<String> {
    JOIN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}
