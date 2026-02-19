//! PaintLayer → CSS background/border values.

use frame_proto::paint::{Paint, PaintLayer};

/// Convert fill paint layers to a CSS background value.
pub fn fills_to_css_background(fills: &[PaintLayer]) -> Option<String> {
    let visible_fills: Vec<_> = fills.iter().filter(|f| f.visible).collect();
    if visible_fills.is_empty() {
        return None;
    }

    // Multiple fills stack (last in list = frontmost = first in CSS).
    let backgrounds: Vec<String> = visible_fills
        .iter()
        .rev()
        .filter_map(|layer| match &layer.paint {
            Paint::Solid { color } => {
                let mut c = *color;
                c.a *= layer.opacity;
                Some(c.to_css_rgba())
            }
            Paint::Gradient(_) | Paint::Image(_) => None, // handled by Anyrender
        })
        .collect();

    if backgrounds.is_empty() {
        None
    } else {
        Some(backgrounds.join(", "))
    }
}

/// Convert stroke paint layers to a CSS border value.
pub fn strokes_to_css_border(
    strokes: &[PaintLayer],
    weight: f64,
) -> Option<String> {
    let visible = strokes.iter().find(|s| s.visible)?;
    match &visible.paint {
        Paint::Solid { color } => {
            let mut c = *color;
            c.a *= visible.opacity;
            Some(format!("{weight}px solid {}", c.to_css_rgba()))
        }
        _ => None,
    }
}
