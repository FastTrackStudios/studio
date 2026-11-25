//! Frame's document layer on figcraft (FastTrackStudios fork, pinned by tag).
//!
//! In the editor figcraft's `Document` is the editing model; this crate re-exports the doc-level
//! figcraft crates so the rest of Frame names them in one place. Only this crate and
//! `frame-editor` depend on figcraft; `frame` core never does (`just boundary`).
//!
//! Frame's own surface data (controls, bindings, slots, motion, shaders, emission) rides on
//! figcraft nodes as JSON under `frame:*` keys of `Node::plugin_data` ([`keys`]), read through
//! [`FrameData`] / [`FrameCache`]; [`scene_to_doc`] and [`doc_to_scene`] convert v1 scenes.

mod convert;
mod data;
pub mod keys;

pub use figcraft_collab as collab;
pub use figcraft_color as color;
pub use figcraft_doc as doc;
pub use figcraft_geom as geom;
pub use figcraft_layout as layout;
pub use figcraft_pathops as pathops;
pub use figcraft_text as text;

pub use convert::{derived_uuid, doc_to_scene, scene_to_doc};
pub use data::{
    DataError, ExternSlot, FrameCache, FrameData, SlotMark, SlotTarget, control_of, controls,
    effective, effective_map, frame_id, set_data, slot_index, slots_of,
};

/// What converting or editing Frame data can fail with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No node with this id in the document.
    NoNode(figcraft_doc::NodeId),
    /// The scene's root isn't one of its nodes.
    NoRoot,
    /// A value that doesn't (de)serialize, or a document edit figcraft refused.
    Data(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoNode(id) => write!(f, "no node {id}"),
            Self::NoRoot => f.write_str("the scene's root is not one of its nodes"),
            Self::Data(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Data(e.to_string())
    }
}
