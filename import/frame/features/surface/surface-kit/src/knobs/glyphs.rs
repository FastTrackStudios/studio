//! A tiny stroke font for numerals that have to *turn*.
//!
//! A Distressor's numbers are printed on its skirt and rotate with it. Only a
//! knob's `Needle` node rotates (text nodes don't), so those numerals are
//! drawn as geometry inside the needle: each glyph a few polylines on a
//! 4 × 6 grid, each segment a square-capped quad (one union path, nonzero).

/// Polylines per glyph on a 4-wide, 6-tall grid (y down).
fn strokes(c: char) -> &'static [&'static [(f64, f64)]] {
    match c {
        '0' => &[&[(0.0, 1.0), (1.0, 0.0), (3.0, 0.0), (4.0, 1.0), (4.0, 5.0), (3.0, 6.0), (1.0, 6.0), (0.0, 5.0), (0.0, 1.0)]],
        '1' => &[&[(0.8, 1.2), (2.0, 0.0), (2.0, 6.0)], &[(0.8, 6.0), (3.2, 6.0)]],
        '2' => &[&[(0.0, 1.0), (1.0, 0.0), (3.0, 0.0), (4.0, 1.0), (4.0, 2.2), (0.0, 6.0), (4.0, 6.0)]],
        '3' => &[&[(0.0, 0.0), (4.0, 0.0), (1.8, 2.5), (3.0, 2.5), (4.0, 3.5), (4.0, 5.0), (3.0, 6.0), (1.0, 6.0), (0.0, 5.0)]],
        '4' => &[&[(3.0, 6.0), (3.0, 0.0), (0.0, 4.2), (4.0, 4.2)]],
        '5' => &[&[(4.0, 0.0), (0.4, 0.0), (0.0, 2.6), (3.0, 2.6), (4.0, 3.6), (4.0, 5.0), (3.0, 6.0), (0.0, 6.0)]],
        '6' => &[&[(3.6, 0.0), (1.6, 0.0), (0.0, 1.6), (0.0, 5.0), (1.0, 6.0), (3.0, 6.0), (4.0, 5.0), (4.0, 3.6), (3.0, 2.6), (0.0, 2.6)]],
        '7' => &[&[(0.0, 0.0), (4.0, 0.0), (1.6, 6.0)]],
        '8' => &[&[
            (1.0, 0.0),
            (3.0, 0.0),
            (4.0, 1.0),
            (4.0, 2.0),
            (3.0, 3.0),
            (1.0, 3.0),
            (0.0, 4.0),
            (0.0, 5.0),
            (1.0, 6.0),
            (3.0, 6.0),
            (4.0, 5.0),
            (4.0, 4.0),
            (3.0, 3.0),
            (1.0, 3.0),
            (0.0, 2.0),
            (0.0, 1.0),
            (1.0, 0.0),
        ]],
        '9' => &[&[(4.0, 3.4), (1.0, 3.4), (0.0, 2.4), (0.0, 1.0), (1.0, 0.0), (3.0, 0.0), (4.0, 1.0), (4.0, 4.4), (2.4, 6.0), (0.4, 6.0)]],
        '-' => &[&[(0.6, 3.0), (3.4, 3.0)]],
        '+' => &[&[(0.4, 3.0), (3.6, 3.0)], &[(2.0, 1.4), (2.0, 4.6)]],
        '.' => &[&[(1.8, 5.6), (2.2, 5.6)]],
        _ => &[],
    }
}

/// Advance per glyph, in grid units.
const ADVANCE: f64 = 5.2;

/// `text` as quads, centred on (0, 0) with its top pointing to −y, `height`
/// tall, stroke `weight` — then rotated by `deg` (clockwise) and moved to
/// `at` by `place`. Unknown characters are skipped (leaving their space).
pub fn outline(text: &str, height: f64, weight: f64, deg: f64, at: (f64, f64)) -> String {
    let s = height / 6.0;
    let n = text.chars().count() as f64;
    let width = (n * ADVANCE - (ADVANCE - 4.0)) * s;
    let (sn, cs) = deg.to_radians().sin_cos();
    let map = |x: f64, y: f64| {
        let (lx, ly) = (x - width / 2.0, y - height / 2.0);
        (at.0 + lx * cs - ly * sn, at.1 + lx * sn + ly * cs)
    };
    let mut d = String::new();
    for (i, c) in text.chars().enumerate() {
        let ox = i as f64 * ADVANCE * s;
        for line in strokes(c) {
            for w in line.windows(2) {
                let (x0, y0) = (ox + w[0].0 * s, w[0].1 * s);
                let (x1, y1) = (ox + w[1].0 * s, w[1].1 * s);
                d.push_str(&quad(map(x0, y0), map(x1, y1), weight));
            }
        }
    }
    d
}

/// A segment from `a` to `b`, `w` wide, square-capped, as a closed subpath
/// (always wound the same way, so overlapping quads union under nonzero).
pub fn quad(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-6);
    let (ux, uy) = (dx / len * w / 2.0, dy / len * w / 2.0);
    let (nx, ny) = (-uy, ux);
    let p = [
        (a.0 - ux + nx, a.1 - uy + ny),
        (b.0 + ux + nx, b.1 + uy + ny),
        (b.0 + ux - nx, b.1 + uy - ny),
        (a.0 - ux - nx, a.1 - uy - ny),
    ];
    format!(
        "M {:.3} {:.3} L {:.3} {:.3} L {:.3} {:.3} L {:.3} {:.3} Z ",
        p[0].0, p[0].1, p[1].0, p[1].1, p[2].0, p[2].1, p[3].0, p[3].1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_have_strokes_and_unknowns_do_not() {
        for c in "0123456789+-.".chars() {
            assert!(!outline(&c.to_string(), 6.0, 1.0, 0.0, (0.0, 0.0)).is_empty(), "{c}");
        }
        assert!(outline("x", 6.0, 1.0, 0.0, (0.0, 0.0)).is_empty());
    }
}
