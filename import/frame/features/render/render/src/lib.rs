//! Frame renderer.
//!
//! One source, two backends:
//! - paint primitives (`anyrender_renderer`) painted into a vello scene
//!   (`anyrender` feature)
//! - SVG export over the same primitives (`svg_export` feature)

pub mod anyrender_renderer;
#[cfg(feature = "svg_export")]
pub mod svg;

pub use anyrender_renderer::{
    build_layout_boxes, build_paint_primitives, build_scene_layout_boxes, build_scene_primitives,
    collect_render_diagnostics, LayoutBox,
    PaintPrimitive, RenderDiagnostics, Rgba,
};
#[cfg(feature = "anyrender")]
pub use anyrender_renderer::{
    paint_into_scene, paint_into_scene_with, paint_into_scene_with_font,
    paint_primitives_into_scene_with, paint_scene_into, TextFontRef,
};

#[cfg(feature = "anyrender")]
pub mod offscreen;
