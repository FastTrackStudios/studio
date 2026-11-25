//! Turning a PNG into rects.
//!
//! One rect per pixel would be correct and enormous — a 63×20 button is
//! 1260 of them, and there are a thousand images. So runs of identical
//! pixels are merged horizontally, then vertically where whole runs line
//! up, which typically cuts it by an order of magnitude on theme art
//! (large flat areas, long 1px bevels).
//!
//! Fully transparent pixels are dropped rather than emitted as clear rects:
//! they are most of a nine-slice source, and they draw nothing.
//!
//! Marker pixels are dropped too — they are stamped back verbatim after
//! rasterising, so tracing them would double-draw the guides and, worse,
//! subject them to the colour ramp.

use image::RgbaImage;

use crate::art::art_data::Rect;
use crate::art::derive::MARKERS;

/// Trace an image into merged rects.
pub fn trace(img: &RgbaImage) -> Vec<Rect> {
    let (w, h) = (img.width(), img.height());

    // Horizontal runs per row.
    let mut runs: Vec<Rect> = Vec::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            let px = img.get_pixel(x, y).0;
            if px[3] == 0 || MARKERS.contains(&px) {
                x += 1;
                continue;
            }
            let start = x;
            while x + 1 < w && img.get_pixel(x + 1, y).0 == px {
                x += 1;
            }
            runs.push(Rect {
                x: start as u16,
                y: y as u16,
                w: (x - start + 1) as u16,
                h: 1,
                rgba: px,
            });
            x += 1;
        }
    }

    // Merge vertically: a run directly below one of the same x/width/colour
    // extends it. Theme art is full of tall flat columns, so this is where
    // most of the reduction comes from.
    let mut merged: Vec<Rect> = Vec::new();
    for run in runs {
        if let Some(prev) = merged
            .iter_mut()
            .rev()
            .find(|p| p.x == run.x && p.w == run.w && p.rgba == run.rgba && p.y + p.h == run.y)
        {
            prev.h += 1;
        } else {
            merged.push(run);
        }
    }
    merged
}
