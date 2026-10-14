//! Zoom: the drag ([`ZoomDrag`], shared with the gestures), the history
//! of views a surface has framed ([`ZoomHistory`]), and contextual zoom
//! ([`contextual`], MeMagic).

pub mod contextual;
mod history;

pub use crate::gestures::ZoomDrag;
pub use history::ZoomHistory;
