//! Zoom: the drag ([`ZoomDrag`], shared with the gestures), and the
//! history of views a surface has framed ([`ZoomHistory`]).

mod history;

pub use crate::gestures::ZoomDrag;
pub use history::ZoomHistory;
