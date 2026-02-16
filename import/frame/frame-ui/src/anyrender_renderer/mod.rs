mod collect;
mod extract;
mod layout;
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
pub use types::{
    BlendMix, EffectKind, FillPaint, GradientStop, LayoutBox, NodeEffect, PaintPrimitive,
    RenderDiagnostics, Rgba, StrokeAlign, StrokeCap, StrokeJoin, StrokeStyle, TextCase,
};

#[cfg(feature = "anyrender")]
pub use paint::{
    paint_into_scene, paint_into_scene_with, paint_into_scene_with_font,
    paint_primitives_into_scene_with,
};
#[cfg(feature = "anyrender")]
pub use types::TextFontRef;
