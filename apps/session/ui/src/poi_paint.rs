//! The points of interest (`crate::poi`) drawn in the ruler's MARKS lane.

use anyrender::PaintScene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use crate::poi::{current, shown};

/// The points of interest in the ruler's MARKS lane: a flag at each, its
/// note beside it — amber open, green done, ringed when stepped to.
pub fn paint(
    painter: &mut impl PaintScene,
    (palette, font): (&crate::arrangement::Palette, &crate::text::Font),
    view: crate::arrangement::Viewport,
    origin: (f64, f64),
) {
    /// The notes' size, as the ruler's lanes write theirs.
    const SIZE: f32 = 8.0;
    const OPEN: Color = Color::from_rgba8(0xf5, 0x9e, 0x0b, 0xff);
    const DONE: Color = Color::from_rgba8(0x34, 0xd3, 0x99, 0xff);
    let Some(points) = shown() else {
        return;
    };
    let row = crate::ruler::MARKS_ROW;
    if !crate::ruler::lane_shown(row) {
        return;
    }
    let (ox, oy) = origin;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    let y0 = oy + crate::ruler::row_top(row) + 1.0;
    let y1 = y0 + crate::ruler::LANE_H - 2.0;
    let current = current();
    for p in points.iter() {
        let x = p.at.mul_add(view.pps, left - view.scroll_x);
        if x < left - 2.0 || x > right {
            continue;
        }
        let color = if p.done { DONE } else { OPEN };
        if current.as_deref() == Some(p.id.as_str()) {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.text,
                None,
                &Rect::new(x - 3.5, y0 - 1.0, x + 3.5, y1 + 1.0),
            );
        }
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &Rect::new(x - 2.0, y0, x + 2.0, y1),
        );
        crate::tcp::glyphs(painter, font, color, &p.note, x + 5.0, y1 - 2.0, SIZE);
    }
}
