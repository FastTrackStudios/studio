//! Takes named by colour: the views' contract's (`daw_view::take_colors`),
//! with each swatch as the painters' colour.

pub use daw_view::take_colors::*;

use vello::peniko::Color;

/// The colour numbered `index`, and its name, as the painters draw it.
#[must_use]
pub fn swatch(index: usize) -> (String, Color) {
    let (name, [r, g, b]) = daw_view::take_colors::swatch(index);
    (name, Color::from_rgb8(r, g, b))
}
