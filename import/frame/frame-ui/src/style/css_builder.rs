//! Master CSS builder — converts a FrameNode to inline CSS.

use crate::style::effect_to_css::*;
use crate::style::layout_to_css::*;
use crate::style::paint_to_css::*;
use frame_proto::node::{FrameNode, NodeKind};
use frame_proto::paint::BlendMode;

/// Build an inline CSS style string from a FrameNode.
pub fn node_to_css(node: &FrameNode) -> String {
    let mut css = String::new();

    // Position and size
    css.push_str(&format!(
        "width: {}px; height: {}px; ",
        node.size.width, node.size.height
    ));

    // Opacity
    if node.opacity < 1.0 {
        css.push_str(&format!("opacity: {}; ", node.opacity));
    }

    // Fills → background
    if let Some(bg) = fills_to_css_background(&node.fills) {
        css.push_str(&format!("background: {bg}; "));
    }

    // Strokes → border
    if let Some(border) = strokes_to_css_border(&node.strokes, node.stroke_style.weight) {
        css.push_str(&format!("border: {border}; "));
    }

    // Corner radii → border-radius
    if let Some(radius) = corner_radii_to_css(node) {
        css.push_str(&format!("border-radius: {radius}; "));
    }

    // Effects → box-shadow
    if let Some(shadow) = effects_to_css_box_shadow(&node.effects) {
        css.push_str(&format!("box-shadow: {shadow}; "));
    }

    // Effects → filter (blur)
    if let Some(filter) = effects_to_css_filter(&node.effects) {
        css.push_str(&format!("filter: {filter}; "));
    }

    // Effects → backdrop-filter
    if let Some(backdrop) = effects_to_css_backdrop_filter(&node.effects) {
        css.push_str(&format!("backdrop-filter: {backdrop}; "));
    }

    // Auto-layout → flexbox
    if let NodeKind::Frame {
        auto_layout,
        clip_content,
        ..
    } = &node.kind
    {
        css.push_str(&auto_layout_to_css(auto_layout));
        if *clip_content {
            css.push_str("overflow: hidden; ");
        }
    }

    // Blend mode
    if node.blend_mode != BlendMode::PassThrough && node.blend_mode != BlendMode::Normal {
        if let Some(mode) = blend_mode_to_css(node.blend_mode) {
            css.push_str(&format!("mix-blend-mode: {mode}; "));
        }
    }

    css
}

fn corner_radii_to_css(node: &FrameNode) -> Option<String> {
    let radii = match &node.kind {
        NodeKind::Frame { corner_radii, .. } => corner_radii,
        NodeKind::Rectangle { corner_radii } => corner_radii,
        _ => return None,
    };

    if radii.is_zero() {
        return None;
    }

    if radii.is_uniform() {
        Some(format!("{}px", radii.top_left))
    } else {
        Some(format!(
            "{}px {}px {}px {}px",
            radii.top_left, radii.top_right, radii.bottom_right, radii.bottom_left
        ))
    }
}

fn blend_mode_to_css(mode: BlendMode) -> Option<&'static str> {
    match mode {
        BlendMode::Multiply => Some("multiply"),
        BlendMode::Screen => Some("screen"),
        BlendMode::Overlay => Some("overlay"),
        BlendMode::Darken => Some("darken"),
        BlendMode::Lighten => Some("lighten"),
        BlendMode::ColorDodge => Some("color-dodge"),
        BlendMode::ColorBurn => Some("color-burn"),
        BlendMode::HardLight => Some("hard-light"),
        BlendMode::SoftLight => Some("soft-light"),
        BlendMode::Difference => Some("difference"),
        BlendMode::Exclusion => Some("exclusion"),
        BlendMode::Hue => Some("hue"),
        BlendMode::Saturation => Some("saturation"),
        BlendMode::ColorBlend => Some("color"),
        BlendMode::Luminosity => Some("luminosity"),
        _ => None,
    }
}
