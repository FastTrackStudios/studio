//! The quantize grid in force across the song, as the edit sees it.
//!
//! The kit's default (eighths, `drum_settings`), a part's grid where a
//! Grid region says (a section of sixteenths, the snare's quarters in a
//! breakdown), a fill's own grid inside it — the most specific wins, as
//! the quantize decides. Editing, the grid lines are drawn by it rather
//! than by the zoom (four bars across fit 32nds, which no part here
//! plays), and the transport bar says which grid the cursor is on.

use std::sync::Mutex;

use crate::ruler::GridRegion;

/// The grid in force: a default, and the stretches that differ, in
/// seconds.
pub type InForce = timeline::grid::GridRegions;

/// Span edges are compared to the microsecond, so two edges that are one
/// compare as one.
const MICROSECOND: f64 = 1e-6;

/// `default` with `regions` over it, the shortest holding each stretch —
/// a fill inside a part, a figure inside a fill.
#[must_use]
pub fn in_force(default: u32, regions: &[GridRegion]) -> InForce {
    InForce::new(default, regions, MICROSECOND)
}

/// The grid in force while editing, `None` otherwise: set by the window
/// each frame it draws, read by the transport bar.
static SHOWN: Mutex<Option<InForce>> = Mutex::new(None);

/// Show `in_force` (or nothing, not editing).
pub fn set(in_force: Option<InForce>) {
    if let Ok(mut shown) = SHOWN.lock()
        && *shown != in_force
    {
        *shown = in_force;
    }
}

/// The grid in force, while editing.
#[must_use]
pub fn current() -> Option<InForce> {
    SHOWN.lock().ok().and_then(|s| s.clone())
}
