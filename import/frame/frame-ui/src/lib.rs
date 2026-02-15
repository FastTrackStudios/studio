//! Frame UI built on projection models from frame-proto.
//!
//! This crate provides two output paths over the same source:
//! - Dioxus component tree (`dioxus_renderer`)
//! - Anyrender-target paint primitives (`anyrender_renderer`)

pub mod anyrender_renderer;
pub mod dioxus_renderer;
pub mod model;

pub use anyrender_renderer::{build_paint_primitives, PaintPrimitive};
pub use dioxus_renderer::ProjectionTree;
pub use model::{build_ui_nodes, build_ui_pages, UiNode, UiPage};
