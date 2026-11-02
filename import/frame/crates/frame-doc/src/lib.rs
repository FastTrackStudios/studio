//! Frame's document layer on figcraft (FastTrackStudios fork, pinned by tag).
//!
//! In the editor figcraft's `Document` is the editing model; this crate re-exports the doc-level
//! figcraft crates so the rest of Frame names them in one place. Only this crate and
//! `frame-editor` depend on figcraft; `frame` core never does (`just boundary`).

pub use figcraft_collab as collab;
pub use figcraft_color as color;
pub use figcraft_doc as doc;
pub use figcraft_geom as geom;
pub use figcraft_layout as layout;
pub use figcraft_pathops as pathops;
pub use figcraft_text as text;
