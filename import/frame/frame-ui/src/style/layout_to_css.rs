//! AutoLayout → CSS flex properties.

use frame_proto::layout::*;

/// Convert auto-layout configuration to CSS flex properties.
pub fn auto_layout_to_css(layout: &AutoLayout) -> String {
    if !layout.has_auto_layout() {
        return String::new();
    }

    let mut css = String::new();
    css.push_str("display: flex; ");

    match layout.direction {
        LayoutDirection::Horizontal => css.push_str("flex-direction: row; "),
        LayoutDirection::Vertical => css.push_str("flex-direction: column; "),
        LayoutDirection::Wrap => css.push_str("flex-direction: row; flex-wrap: wrap; "),
        LayoutDirection::None => {}
    }

    // justify-content (primary axis)
    match layout.primary_axis_align {
        PrimaryAxisAlign::Min => css.push_str("justify-content: flex-start; "),
        PrimaryAxisAlign::Center => css.push_str("justify-content: center; "),
        PrimaryAxisAlign::Max => css.push_str("justify-content: flex-end; "),
        PrimaryAxisAlign::SpaceBetween => css.push_str("justify-content: space-between; "),
    }

    // align-items (counter axis)
    match layout.counter_axis_align {
        CounterAxisAlign::Min => css.push_str("align-items: flex-start; "),
        CounterAxisAlign::Center => css.push_str("align-items: center; "),
        CounterAxisAlign::Max => css.push_str("align-items: flex-end; "),
        CounterAxisAlign::Baseline => css.push_str("align-items: baseline; "),
    }

    // gap
    if layout.item_spacing > 0.0 {
        css.push_str(&format!("gap: {}px; ", layout.item_spacing));
    }

    // padding
    if layout.padding_top > 0.0
        || layout.padding_right > 0.0
        || layout.padding_bottom > 0.0
        || layout.padding_left > 0.0
    {
        css.push_str(&format!(
            "padding: {}px {}px {}px {}px; ",
            layout.padding_top, layout.padding_right, layout.padding_bottom, layout.padding_left
        ));
    }

    css
}
