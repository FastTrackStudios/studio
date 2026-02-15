//! Dioxus render components for Frame document nodes.
//!
//! Follows the DockNodeRenderer recursive dispatch pattern:
//! FrameNodeRenderer matches NodeKind and dispatches to type-specific renderers.

pub mod ellipse_renderer;
pub mod frame_renderer;
pub mod group_renderer;
pub mod instance_renderer;
pub mod node_renderer;
pub mod rectangle_renderer;
pub mod text_renderer;
pub mod vector_renderer;

pub use node_renderer::FrameNodeRenderer;
