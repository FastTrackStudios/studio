//! Where a recorded performance's audio plays once its hits are moved:
//! the DAW's own algorithm and types (`daw_edit::placement`), the default a
//! swappable edit algorithm (`daw.edit`'s `PLACE`) replaces.

pub use daw_edit::placement::*;
