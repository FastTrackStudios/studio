//! Where a dragged panel would land: pure geometry, no renderer.
//!
//! The host measures its own rectangle and every stack's, then asks
//! [`target`] for the point under the pointer. Near the window's edge a
//! panel docks to that edge; over a stack its outer bands split the stack
//! and its middle adds a tab (MaxPane's drop zones: a band is a quarter of
//! the stack, kept between 24 and 96 px).

use crate::dockspace::{Place, Rect, Share, Side, StackId, Zone};

/// How close to the window's edge a drop docks to that edge, px.
pub const EDGE_BAND: f32 = 28.0;
/// A stack's split bands: this share of its size …
pub const SPLIT_SHARE: f32 = 0.25;
/// … kept within these, px.
pub const SPLIT_MIN: f32 = 24.0;
pub const SPLIT_MAX: f32 = 96.0;

/// A drop: where the panel goes, and the rectangle to light up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub place: Place,
    pub highlight: Rect,
}

/// The drop at `(x, y)` in a host at `host` holding `stacks`, if any.
#[must_use]
pub fn target(x: f32, y: f32, host: Rect, stacks: &[(StackId, Rect)]) -> Option<Target> {
    if !contains(host, x, y) {
        return None;
    }
    edge_target(x, y, host).or_else(|| {
        stacks
            .iter()
            .find(|(_, r)| contains(*r, x, y))
            .map(|(id, r)| stack_target(*id, *r, x, y))
    })
}

fn edge_target(x: f32, y: f32, host: Rect) -> Option<Target> {
    let side = nearest_side(host, x, y, EDGE_BAND)?;
    // Light up the band a new dock would take: a quarter of the window.
    let highlight = band(host, side, (host.w.min(host.h) * 0.25).max(EDGE_BAND));
    Some(Target {
        place: Place::Edge(side),
        highlight,
    })
}

fn stack_target(id: StackId, r: Rect, x: f32, y: f32) -> Target {
    let reach = (r.w.min(r.h) * SPLIT_SHARE).clamp(SPLIT_MIN, SPLIT_MAX);
    match nearest_side(r, x, y, reach) {
        Some(side) => {
            let half = if side.axis() == crate::dockspace::Axis::Row {
                r.w
            } else {
                r.h
            } / 2.0;
            Target {
                place: Place::Stack(id, Zone::Split(side, Share::Ratio(0.5))),
                highlight: band(r, side, half),
            }
        }
        None => Target {
            place: Place::Stack(id, Zone::Tab),
            highlight: r,
        },
    }
}

/// The side of `r` within `reach` of the point, nearest first.
fn nearest_side(r: Rect, x: f32, y: f32, reach: f32) -> Option<Side> {
    [
        (Side::Left, x - r.x),
        (Side::Right, r.x + r.w - x),
        (Side::Top, y - r.y),
        (Side::Bottom, r.y + r.h - y),
    ]
    .into_iter()
    .filter(|(_, d)| *d <= reach)
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(side, _)| side)
}

/// The strip `thick` px deep along `side` of `r`.
fn band(r: Rect, side: Side, thick: f32) -> Rect {
    let thick = thick.min(if side.axis() == crate::dockspace::Axis::Row {
        r.w
    } else {
        r.h
    });
    match side {
        Side::Left => Rect::new(r.x, r.y, thick, r.h),
        Side::Right => Rect::new(r.x + r.w - thick, r.y, thick, r.h),
        Side::Top => Rect::new(r.x, r.y, r.w, thick),
        Side::Bottom => Rect::new(r.x, r.y + r.h - thick, r.w, thick),
    }
}

fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
}
