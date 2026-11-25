//! A streamed song heard by its reference on this device: now
//! `session_runtime::reference_play`, re-exported here. What stays is
//! the multitracks, which are the stream set's: asking for them brings
//! every song's stems in at once ([`crate::stream_set`]).

use std::sync::atomic::{AtomicBool, Ordering};

pub use session_runtime::reference_play::*;

/// Whether the multitracks were asked for: from then on every song's stems
/// come in at once, not after the references.
static MULTITRACKS: AtomicBool = AtomicBool::new(false);

/// Whether songs are heard by their stems ([`load_multitracks`]) rather
/// than their reference.
#[must_use]
pub(crate) fn multitracks() -> bool {
    MULTITRACKS.load(Ordering::Relaxed)
}

/// Ask for the multitracks now: every song's stems come in at once, and
/// each hands over from its reference as they arrive ([`crate::stream_set`]).
pub(crate) fn load_multitracks() {
    if !MULTITRACKS.swap(true, Ordering::Relaxed) {
        crate::stream_set::hear_stems();
    }
}
