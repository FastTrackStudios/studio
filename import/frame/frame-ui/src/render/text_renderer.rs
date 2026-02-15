//! Text renderer — CSS-based rich text rendering.

use crate::prelude::*;
use crate::signals::FRAME_DOCUMENT;
use crate::style::css_builder::node_to_css;
use frame_proto::node::NodeKind;
use frame_proto::typography::*;
use frame_proto::NodeId;

/// Renders a text node with CSS font styling.
#[component]
pub fn TextRenderer(node_id: NodeId) -> Element {
    let doc = FRAME_DOCUMENT.read();
    let node = match doc.get_node(node_id) {
        Some(n) => n,
        None => return rsx! {},
    };

    let base_style = node_to_css(node);

    // Extract text-specific data.
    let (characters, segments, style) = match &node.kind {
        NodeKind::Text {
            characters,
            segments,
            style,
        } => (characters.clone(), segments.clone(), style.clone()),
        _ => return rsx! {},
    };

    // If there's only one segment (or no segments), render as a simple styled div.
    if segments.len() <= 1 {
        let text_css = text_style_to_css(&style);
        rsx! {
            div {
                class: "frame-node frame-text",
                style: "{base_style}{text_css}",
                "{characters}"
            }
        }
    } else {
        // Multiple segments → span per segment for mixed formatting.
        rsx! {
            div {
                class: "frame-node frame-text",
                style: "{base_style}",
                for segment in segments {
                    span {
                        style: "{text_style_to_css(&segment.style)}",
                        "{segment.text}"
                    }
                }
            }
        }
    }
}

/// Convert a TextStyle into inline CSS properties.
fn text_style_to_css(style: &TextStyle) -> String {
    let mut css = String::new();

    css.push_str(&format!("font-family: '{}'; ", style.font_family));
    css.push_str(&format!("font-size: {}px; ", style.font_size));
    css.push_str(&format!("font-weight: {}; ", style.font_weight));

    if style.italic {
        css.push_str("font-style: italic; ");
    }

    if style.letter_spacing != 0.0 {
        css.push_str(&format!("letter-spacing: {}px; ", style.letter_spacing));
    }

    match style.line_height {
        LineHeight::Auto => {}
        LineHeight::Pixels(px) => css.push_str(&format!("line-height: {px}px; ")),
        LineHeight::Percent(pct) => css.push_str(&format!("line-height: {}%; ", pct * 100.0)),
    }

    match style.text_align_horizontal {
        TextAlignHorizontal::Left => css.push_str("text-align: left; "),
        TextAlignHorizontal::Center => css.push_str("text-align: center; "),
        TextAlignHorizontal::Right => css.push_str("text-align: right; "),
        TextAlignHorizontal::Justified => css.push_str("text-align: justify; "),
    }

    match style.text_decoration {
        TextDecoration::None => {}
        TextDecoration::Underline => css.push_str("text-decoration: underline; "),
        TextDecoration::Strikethrough => css.push_str("text-decoration: line-through; "),
    }

    match style.text_case {
        TextCase::Original => {}
        TextCase::Upper => css.push_str("text-transform: uppercase; "),
        TextCase::Lower => css.push_str("text-transform: lowercase; "),
        TextCase::Title => css.push_str("text-transform: capitalize; "),
        TextCase::SmallCaps | TextCase::SmallCapsForced => {
            css.push_str("font-variant: small-caps; ");
        }
    }

    css
}
