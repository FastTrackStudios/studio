mod collect;
mod extract;
mod layout;
mod scene_collect;
pub mod transform;
mod types;

#[cfg(feature = "anyrender")]
mod cache;
#[cfg(feature = "anyrender")]
mod geometry;
#[cfg(feature = "anyrender")]
mod paint;

#[cfg(test)]
mod tests;

pub use collect::{build_paint_primitives, collect_render_diagnostics};
pub use layout::build_layout_boxes;
pub use scene_collect::{build_scene_layout_boxes, build_scene_primitives};
#[cfg(feature = "svg_export")]
pub(crate) use scene_collect::fill_paint;
pub use transform::AffineTransform2D;
pub use types::{
    BlendMix, EffectKind, FillPaint, GradientStop, ImageScaleMode, LayoutBox, NodeEffect,
    PaintPrimitive, RenderDiagnostics, Rgba, StrokeAlign, StrokeCap, StrokeJoin, StrokeStyle,
    TextAlignment, TextCase,
};

#[cfg(feature = "anyrender")]
pub use paint::{
    paint_into_scene, paint_into_scene_with, paint_into_scene_with_font,
    paint_primitives_into_scene_with, paint_scene_into,
};
#[cfg(feature = "anyrender")]
pub use types::TextFontRef;
