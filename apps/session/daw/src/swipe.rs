//! A swipe across a take's lane: the stretch to comp.
//!
//! On a row showing its lanes (its `+` open), a drag along a lane that is
//! not the comp marks a stretch of it; let go, that stretch of the lane is
//! put onto the comp lane (`TRACK_COMP_FROM_LANE`, [`crate::comp::swipe`]) —
//! on a shut folder's row, every track under it with lanes at once.
//! REAPER 7's swipe comping. This is the gesture's geometry and its
//! picture, and the lanes' names it is aimed by; the window owns the press.

use anyrender::PaintScene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::Fill;

/// Narrower than this on screen, a swipe was a click: nothing comped,
/// pixels.
pub const SLOP: f64 = 4.0;

/// A swipe under way.
#[derive(Clone, Debug, PartialEq)]
pub struct Swipe {
    /// The row it is on, and its track (a folder, or a track with lanes).
    pub row: usize,
    pub track: String,
    /// The tracks comped with it: its media-edit group, while grouping is
    /// on ([`crate::lanes::grouped_tracks`]) — a kit's mics as one.
    pub with: Vec<String>,
    /// The lane swiped along, how the row stacks its lanes
    /// ([`crate::lane_stack::Stack`]), and what it is called
    /// ([`crate::lanes::called`]) — how the comp finds it on each track
    /// under a folder.
    pub lane: u32,
    pub stack: crate::lane_stack::Stack,
    pub called: String,
    /// Where it was pressed and where the hand is now, seconds.
    pub from: f64,
    pub to: f64,
}

impl Swipe {
    /// The stretch, earlier end first.
    #[must_use]
    pub const fn span(&self) -> (f64, f64) {
        if self.from <= self.to {
            (self.from, self.to)
        } else {
            (self.to, self.from)
        }
    }

    /// The comp to run on letting go, as `TRACK_COMP_FROM_LANE`'s
    /// arguments — `None` for a swipe narrower than [`SLOP`] at `pps`.
    #[must_use]
    pub fn comp_args(&self, pps: f64) -> Option<String> {
        let (start, end) = self.span();
        ((end - start) * pps >= SLOP).then(|| self.args_over((start, end)))
    }

    /// Whether it was a click, not a swipe: narrower than [`SLOP`] at
    /// `pps`.
    #[must_use]
    pub fn is_click(&self, pps: f64) -> bool {
        let (start, end) = self.span();
        (end - start) * pps < SLOP
    }

    /// `TRACK_COMP_FROM_LANE`'s arguments for this lane over `start..end`
    /// — a click comps its lane over a stretch picked before.
    #[must_use]
    pub fn args_over(&self, (start, end): (f64, f64)) -> String {
        serde_json::json!({
            "track": self.track,
            "with": self.with,
            "lane": self.called,
            "start": start,
            "end": end,
        })
        .to_string()
    }
}

/// Which lane of a row is at `content_y`.
///
/// The row `(top, height)` stacks its lanes as `stack` does; `content_y`
/// is in session units, as the row's box is. `None` off the row.
#[must_use]
pub fn lane_at(
    (top, height): (f64, f64),
    stack: &crate::lane_stack::Stack,
    content_y: f64,
) -> Option<u32> {
    if height <= 0.0 {
        return None;
    }
    stack.lane_at((content_y - top) / height)
}

/// Whether a press on lane `lane` of a row showing `laned`'s lanes starts
/// a swipe: any lane but the comp, which is what a swipe lands on.
#[must_use]
pub fn starts_on(laned: &daw_proto::Track, lane: u32) -> bool {
    laned.lane_count > 1
        && lane < laned.lane_count
        && crate::arrange_edit::comp_lane(laned) != Some(lane)
}

/// The swipe being made: its stretch over its lane's strip, filled, its
/// ends ruled — over the lanes, `origin` where content `(0, 0)` lands.
pub fn overlay(
    painter: &mut impl PaintScene,
    palette: &crate::arrangement::Palette,
    scene: &crate::arrangement::Arrangement,
    view: crate::arrangement::Viewport,
    origin: (f64, f64),
    swipe: &Swipe,
) {
    let Some((top, height)) = scene.row_box(swipe.row) else {
        return;
    };
    let (ox, oy) = origin;
    let (b0, b1) = swipe.stack.band(swipe.lane);
    let (start, end) = swipe.span();
    let r = Rect::new(
        start.mul_add(view.pps, ox),
        b0.mul_add(height, top)
            .mul_add(view.zoom_y, oy)
            .max(crate::ruler::ruler_h()),
        end.mul_add(view.pps, ox),
        b1.mul_add(height, top).mul_add(view.zoom_y, oy),
    );
    if r.y1 <= r.y0 {
        return;
    }
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.accent.multiply_alpha(0.3),
        None,
        &r,
    );
    for edge in [r.x0, r.x1] {
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.accent,
            None,
            &Rect::new(edge - 0.75, r.y0, edge + 0.75, r.y1),
        );
    }
}

/// Each lane's name at the left of its strip.
///
/// On every row on screen showing its lanes ([`crate::arrangement::Arrangement::lanes_shown`]):
/// the one that plays marked and lit, the others dim — which take is
/// which, to hear and comp from. Drawn per frame, in pixels, so the names
/// keep their size at every zoom.
pub fn labels(
    painter: &mut impl PaintScene,
    palette: &crate::arrangement::Palette,
    font: &crate::text::Font,
    scene: &crate::arrangement::Arrangement,
    view: crate::arrangement::Viewport,
) {
    /// The names' type size, and the room about them.
    const SIZE: f32 = 9.0;
    const PAD: f64 = 3.0;
    let ruler = crate::ruler::ruler_h();
    let oy = ruler - view.scroll_y;
    let size = f64::from(SIZE);
    for row in scene.visible_rows(view) {
        let (Some(laned), Some((top, height))) = (scene.lanes_shown(row), scene.row_box(row))
        else {
            continue;
        };
        let Some(stack) = scene.lane_stack(row) else {
            continue;
        };
        for &lane in stack.lanes() {
            let (b0, b1) = stack.band(lane);
            let y0 = b0.mul_add(height, top).mul_add(view.zoom_y, oy);
            let y1 = b1.mul_add(height, top).mul_add(view.zoom_y, oy);
            // A strip too thin to name, or under the ruler.
            if y1 - y0 < size + PAD || y0 < ruler || y0 > view.height {
                continue;
            }
            let plays = 1_u64
                .checked_shl(lane)
                .is_some_and(|bit| laned.lane_play_mask & bit != 0);
            // A take's lane says its colour by name, a swatch before it:
            // "the blue take".
            let take = scene.lane_take(row, lane).map(crate::take_colors::swatch);
            let called = crate::lanes::called(laned, lane);
            let name = take.as_ref().map_or_else(
                || called.clone(),
                |(color, _)| format!("{called} · {color}"),
            );
            let mark = if plays { size } else { 0.0 };
            let chip = if take.is_some() { size + 2.0 } else { 0.0 };
            let x = view.panel_w + PAD;
            let w = font.width(&name, SIZE) + mark + chip;
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.row_a.multiply_alpha(0.85),
                None,
                &Rect::new(x - 2.0, y0 + 1.0, x + w + 2.0, y0 + size + PAD + 1.0),
            );
            if plays {
                let (cy, h) = (f64::midpoint(y0 + 1.0, y0 + 1.0 + size + PAD), size / 3.0);
                let mut arrow = vello::kurbo::BezPath::new();
                arrow.move_to((x, cy - h));
                arrow.line_to((h.mul_add(1.6, x), cy));
                arrow.line_to((x, cy + h));
                arrow.close_path();
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    palette.accent,
                    None,
                    &arrow,
                );
            }
            if let Some((_, color)) = take {
                let at = x + mark;
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color,
                    None,
                    &vello::kurbo::RoundedRect::new(at, y0 + 2.5, at + size, y0 + 2.5 + size, 2.0),
                );
            }
            let ink = if plays {
                palette.text
            } else {
                palette.text_dim
            };
            crate::tcp::glyphs(
                painter,
                font,
                ink,
                &name,
                x + mark + chip,
                y0 + size + 1.0,
                SIZE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row from 100 tall 48 + two lanes: the one that plays at the top,
    /// the takes under it; off the row, none.
    #[test]
    fn a_lane_is_found_by_its_strip() {
        let track = daw_proto::Track {
            lane_count: 3,
            lane_play_mask: 0b1,
            ..daw_proto::Track::default()
        };
        let stack = crate::lane_stack::Stack::new(&track, [0, 1, 2], 48.0);
        let row = (100.0, stack.height());
        assert_eq!(lane_at(row, &stack, 100.0), Some(0));
        assert_eq!(lane_at(row, &stack, 150.0), Some(1));
        assert_eq!(lane_at(row, &stack, 100.0 + stack.height() - 0.1), Some(2));
        assert_eq!(lane_at(row, &stack, 100.0 + stack.height()), None);
        assert_eq!(lane_at(row, &stack, 99.0), None);
    }

    /// A swipe starts on a take's lane, not on the comp it lands on.
    #[test]
    fn a_swipe_starts_anywhere_but_the_comp() {
        let track = daw_proto::Track {
            lane_count: 3,
            lane_play_mask: 0b100,
            lane_names: vec!["1".into(), "COMP".into(), "EDIT".into()],
            ..daw_proto::Track::default()
        };
        assert!(starts_on(&track, 0));
        assert!(!starts_on(&track, 1), "the comp");
        assert!(starts_on(&track, 2), "the edit is a lane to take from too");
        assert!(!starts_on(&track, 3), "past the last");
        assert!(!starts_on(&daw_proto::Track::default(), 0), "no lanes");
    }

    /// Dragged backwards, the stretch is still earlier end first; too
    /// short a drag comps nothing.
    #[test]
    fn a_swipe_comps_its_stretch_once_it_is_wide_enough() {
        let swipe = Swipe {
            row: 2,
            track: "kit".into(),
            with: vec!["out".into()],
            lane: 0,
            stack: crate::lane_stack::Stack::default(),
            called: "1".into(),
            from: 6.0,
            to: 4.0,
        };
        assert_eq!(swipe.span(), (4.0, 6.0));
        let args = swipe.comp_args(100.0).expect("two seconds at 100 px/s");
        let parsed: serde_json::Value = serde_json::from_str(&args).expect("json");
        assert_eq!(parsed["track"], "kit");
        assert_eq!(parsed["with"][0], "out");
        assert_eq!(parsed["lane"], "1");
        assert_eq!(parsed["start"], 4.0);
        assert_eq!(parsed["end"], 6.0);
        let click = Swipe { to: 6.01, ..swipe };
        assert!(click.comp_args(100.0).is_none(), "a pixel: a click");
    }
}

/// The stretch a click at `t` comps, or T cycles with nothing selected:
/// the picked stretches (`picked`) when `t` is in one — all of them, so a
/// stretch swiped once is swapped whole, take after take — else the run
/// of one take the comp plays at `t` (`comps`, in time order), within the
/// song's section there (`sections`); with neither, the section alone.
#[must_use]
pub fn stretches_at(
    picked: &[(f64, f64)],
    comps: &[(f64, f64)],
    sections: &[(f64, f64)],
    t: f64,
) -> Vec<(f64, f64)> {
    let holds = |s: &(f64, f64)| s.0 <= t && t < s.1;
    if picked.iter().any(holds) {
        return picked.to_vec();
    }
    let section = sections.iter().copied().find(holds);
    let run = comps.iter().copied().find(holds);
    match (run, section) {
        (Some(r), Some(s)) => {
            let (a, b) = (r.0.max(s.0), r.1.min(s.1));
            if b > a { vec![(a, b)] } else { vec![s] }
        }
        (Some(r), None) => vec![r],
        (None, Some(s)) => vec![s],
        (None, None) => Vec::new(),
    }
}

#[cfg(test)]
mod stretch_tests {
    use super::stretches_at;

    /// Swiped over bars 42–43: a click in it takes all of it again.
    #[test]
    fn a_click_in_a_picked_stretch_takes_the_picked_ones() {
        let picked = [(61.5, 64.4)];
        assert_eq!(stretches_at(&picked, &[], &[], 62.0), vec![(61.5, 64.4)]);
    }

    /// Out of it: the take the comp plays there, within its section.
    #[test]
    fn elsewhere_the_comped_run_within_its_section() {
        let comps = [(0.0, 100.0), (100.0, 110.0)];
        let sections = [(90.0, 102.0), (102.0, 120.0)];
        assert_eq!(
            stretches_at(&[(61.5, 64.4)], &comps, &sections, 95.0),
            vec![(90.0, 100.0)]
        );
        assert_eq!(
            stretches_at(&[], &comps, &sections, 105.0),
            vec![(102.0, 110.0)]
        );
        assert_eq!(
            stretches_at(&[], &[], &sections, 105.0),
            vec![(102.0, 120.0)]
        );
        assert!(stretches_at(&[], &[], &[], 5.0).is_empty());
    }
}
