//! The ruler, and the grid under it.
//!
//! # Why this is not in the recorded scene
//!
//! Everything else the window draws is recorded once in content space
//! and replayed under a transform, which is what makes scrolling and
//! zooming free. The grid cannot work that way: which lines exist
//! depends on the zoom. At one bar to the screen it draws sixteenths; at
//! five hundred bars it draws every sixteen bars, and a recorded scene
//! would have to hold every division at once and hide most of them.
//!
//! So this is built per frame — but only for what is on screen, which is
//! at most a screen's width divided by the closest two lines may sit,
//! and in practice a hundred or so lines. That is nothing next to the
//! twenty thousand items the recorded scene holds.
//!
//! # The division is not ours
//!
//! [`adaptive_grid`] picks it, and it already exists in this repo
//! because the expression editor needed exactly this. It follows Ilias
//! Poulakis's Adaptive Grid scripts for REAPER, so the grid coarsens and
//! refines the way a REAPER user expects it to rather than the way this
//! file would have invented.

use anyrender::PaintScene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use crate::arrangement::{Palette, TCP_WIDTH, Viewport};
use crate::text::Font;

mod bars;
mod chords;
mod hit;
mod lanes;
mod tempo;

pub use crate::ruler_layout::*;
pub use bars::*;
pub use chords::*;
pub use hit::*;
pub use lanes::*;
pub use tempo::*;

/// How wide the strip of lane names is, against the ruler's left edge:
/// the track panel's mute/solo column, so the names line up over the
/// buttons below them.
///
/// The names used to start at the corner's left edge and their rules ran
/// across it; the corner is the DAW view's main toolbar now, so the names
/// sit in this column and the rules stop at it.
pub const LABEL_W: f64 = TCP_WIDTH - daw_ui::studio::panel::TINT_W;

/// A lane's name in the label column, shrunk to fit it — centred, as the
/// mute and solo buttons under it are.
fn label(
    painter: &mut impl PaintScene,
    font: &Font,
    color: Color,
    // Where the lanes start: the label column is the LABEL_W before it.
    left: f64,
    name: &str,
    baseline: f64,
    size: f32,
) {
    let (name, size) = font.fit(name, size, 5.0, LABEL_W - 4.0);
    let x = left - LABEL_W / 2.0 - font.width(&name, size) / 2.0;
    crate::tcp::glyphs(painter, font, color, &name, x, baseline, size);
}

/// What the lanes are called, top to bottom.
pub const LANE_NAMES: [&str; LANES] = ["SONG", "SECTIONS", "MARKS"];

/// As much of a name as fits a width, or nothing.
fn fit<'a>(font: &Font, name: &'a str, size: f32, room: f64) -> Option<&'a str> {
    let mut name = name;
    while !name.is_empty() && font.width(name, size) > room {
        let mut end = name.len().saturating_sub(1);
        while end > 0 && !name.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        name = name.get(..end).unwrap_or("");
    }
    (!name.is_empty()).then_some(name)
}

/// A `#rrggbb` string as a colour — what the studio project resolves
/// region and marker colours to.
fn css_hex(css: &str) -> Option<Color> {
    let digits = css.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(digits.get(at..at.saturating_add(2))?, 16).ok();
    Some(Color::from_rgba8(
        channel(0)?,
        channel(2)?,
        channel(4)?,
        0xff,
    ))
}

/// `0.0, 1.0, 2.0, …`, up to `max`.
///
/// A float counter compared against a float bound is how a grid loop
/// becomes either an infinite one or a drifting one; this keeps the
/// counting in integers and converts once. `u32` because a count that
/// did not fit one would have stopped at `max` long before.
fn counts(max: usize) -> impl Iterator<Item = f64> {
    (0..max).map_while(|i| u32::try_from(i).ok().map(f64::from))
}

/// One flat rectangle.
fn fill(painter: &mut impl PaintScene, color: Color, rect: Rect) {
    painter.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
}

/// Draw a vertical line down the lanes at each of `times` (seconds),
/// leaving out those inside `skip`.
fn line_every(
    painter: &mut impl PaintScene,
    view: Viewport,
    times: &[f64],
    (color, width): (Color, f64),
    origin: (f64, f64),
    skip: &[GridRegion],
) {
    let (ox, oy) = origin;
    for &t in times {
        if in_grid_region(skip, t) {
            continue;
        }
        let x = t.mul_add(view.pps, view.panel_w - view.scroll_x);
        if x >= view.panel_w {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color,
                None,
                &Rect::new(ox + x, oy + ruler_h(), ox + x + width, oy + view.height),
            );
        }
    }
}

thread_local! {
    /// The grid regions the ruler and the grid draw by this frame.
    static GRID_REGIONS: std::cell::RefCell<Vec<GridRegion>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Draw the grid by `regions` from now on: inside each, its own lines in
/// place of the beats and the subdivisions.
pub fn set_grid_regions(regions: Vec<GridRegion>) {
    GRID_REGIONS.with(|r| *r.borrow_mut() = regions);
}

/// The grid regions in force.
fn grid_regions() -> Vec<GridRegion> {
    GRID_REGIONS.with(|r| r.borrow().clone())
}

/// Whether `t` is inside one of `regions` (its end not).
fn in_grid_region(regions: &[GridRegion], t: f64) -> bool {
    regions
        .iter()
        .any(|(a, b, _)| t >= *a - 1e-9 && t < *b - 1e-9)
}

// ─── What the pointer is on ─────────────────────────────────────────
