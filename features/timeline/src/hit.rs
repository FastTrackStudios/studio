//! What is under the pointer, the parts both surfaces decide the same
//! way: which part of a span (an item, a note) a screen x is on, and
//! whether it is on a razor area or that area's edge.
//!
//! The order a surface asks in is shared too — razor areas first, then
//! its own zones (fade handles, the volume dial, zone splits), then edges,
//! then bodies — but what a zone is, and which item is on top, stay the
//! surface's.

use crate::razor::RazorSet;

/// How near, in pixels, an item's or a note's edge takes the pointer
/// rather than its body.
pub const EDGE_PX: f64 = 6.0;

/// How near, in pixels, a razor area's edge takes the pointer rather than
/// the area.
pub const RAZOR_EDGE_PX: f64 = 6.0;

/// Which part of a span the pointer is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Within reach of its start (the left edge).
    Start,
    /// Within reach of its end.
    End,
    Body,
}

/// Where screen `x` falls on a span drawn from `x0` to `x1`: an edge
/// within `edge_px` (the start first, so a span too short to tell its
/// edges apart is grabbed by its start), else the body, else `None`.
#[must_use]
pub fn part(x: f64, x0: f64, x1: f64, edge_px: f64) -> Option<Part> {
    if (x - x0).abs() <= edge_px {
        Some(Part::Start)
    } else if (x - x1).abs() <= edge_px {
        Some(Part::End)
    } else if x >= x0 && x <= x1 {
        Some(Part::Body)
    } else {
        None
    }
}

/// A razor area under the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RazorHit {
    /// Inside area `index`.
    Area(usize),
    /// On area `index`'s start or end.
    Edge { index: usize, start: bool },
}

/// The razor area at time `t` and row `row`, and whether screen `x` is on
/// one of its edges (`x_of` puts a time on screen).
#[must_use]
pub fn razor_hit(
    set: &RazorSet,
    t: f64,
    row: i32,
    x: f64,
    x_of: impl Fn(f64) -> f64,
    edge_px: f64,
) -> Option<RazorHit> {
    let (index, area) = set.at(t, row)?;
    Some(match part(x, x_of(area.t0), x_of(area.t1), edge_px) {
        Some(Part::Start) => RazorHit::Edge { index, start: true },
        Some(Part::End) => RazorHit::Edge {
            index,
            start: false,
        },
        _ => RazorHit::Area(index),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::razor::RazorArea;

    #[test]
    fn edges_beat_the_body_and_the_start_beats_the_end() {
        assert_eq!(part(102.0, 100.0, 200.0, EDGE_PX), Some(Part::Start));
        assert_eq!(part(197.0, 100.0, 200.0, EDGE_PX), Some(Part::End));
        assert_eq!(part(150.0, 100.0, 200.0, EDGE_PX), Some(Part::Body));
        assert_eq!(part(250.0, 100.0, 200.0, EDGE_PX), None);
        // Too short to tell apart: the start has it.
        assert_eq!(part(101.0, 100.0, 103.0, EDGE_PX), Some(Part::Start));
    }

    #[test]
    fn a_razor_area_has_edges_and_a_middle() {
        let mut set = RazorSet::default();
        set.add(RazorArea::new(1.0, 3.0, 0, 0));
        let x_of = |t: f64| t * 100.0;
        assert_eq!(
            razor_hit(&set, 2.0, 0, 200.0, x_of, RAZOR_EDGE_PX),
            Some(RazorHit::Area(0))
        );
        assert_eq!(
            razor_hit(&set, 1.02, 0, 102.0, x_of, RAZOR_EDGE_PX),
            Some(RazorHit::Edge {
                index: 0,
                start: true
            })
        );
        assert_eq!(
            razor_hit(&set, 2.97, 0, 297.0, x_of, RAZOR_EDGE_PX),
            Some(RazorHit::Edge {
                index: 0,
                start: false
            })
        );
        assert_eq!(razor_hit(&set, 5.0, 0, 500.0, x_of, RAZOR_EDGE_PX), None);
    }
}
