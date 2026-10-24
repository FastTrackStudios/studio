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
        ((end - start) * pps >= SLOP).then(|| {
            serde_json::json!({
                "track": self.track,
                "with": self.with,
                "lane": self.called,
                "start": start,
                "end": end,
            })
            .to_string()
        })
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
