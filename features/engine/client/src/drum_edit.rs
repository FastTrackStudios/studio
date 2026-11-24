//! Drum editing's judgement — which of a drum's hits a grid line claims,
//! and where the fills are — is the editing engine's (`editing::drums`),
//! re-exported here. The lane an edit is made on is the views'
//! contract's (`daw_view::lanes`).

pub use daw_view::lanes::{edit_lane, is_edited};
pub use editing::drums::*;
