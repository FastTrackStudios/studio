//! Drawing a glyph into a scene: a dark halo, then the glyph.

use anyrender::{PaintScene, Scene};
use kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Stroke};
use peniko::{Color, Fill};

use super::{Cursor, Handle, SIZE, hotspot};

fn stroke(w: f64) -> Stroke {
    Stroke::new(w)
}

/// Draw `cursor` into `scene` with its hotspot at `(x, y)`.
///
/// Two passes for every shape: a dark halo underneath and the bright
/// glyph on top. A single-colour cursor disappears the moment it crosses
/// something its own colour, and the roll is full of both dark grid and
/// bright notes.
///
/// `ink` is the glyph's colour, the view's theme's brightest text (a
/// cursor has to stay legible over selected material, which is painted in
/// the accent).
pub fn draw(scene: &mut Scene, cursor: Cursor, x: f64, y: f64, ink: Color) {
    let (hx, hy) = hotspot(cursor);
    let at = Affine::translate((x - hx, y - hy));
    let halo = Color::from_rgba8(0, 0, 0, 190);
    // Halo first, fat; then the glyph, thin, on top.
    for (color, width) in [(halo, 3.4), (ink, 1.5)] {
        shape(scene, cursor, at, color, width);
    }
}

fn shape(scene: &mut Scene, cursor: Cursor, at: Affine, color: Color, w: f64) {
    let s = stroke(w);
    let c = SIZE * 0.5;
    match cursor {
        Cursor::Arrow => arrow(scene, at, color, w),
        Cursor::Crosshair => cross(scene, at, color, w, c, c, 8.0),
        Cursor::MarqueeAdd => {
            cross(scene, at, color, w, c, c, 8.0);
            plus(scene, at, color, w, SIZE - 4.0, 4.0, 3.0);
        }
        Cursor::MarqueeToggle => {
            cross(scene, at, color, w, c, c, 8.0);
            // A dot rather than a plus: toggling is not adding.
            scene.fill(
                Fill::NonZero,
                at,
                color,
                None,
                &Circle::new(Point::new(SIZE - 4.0, 4.0), w * 0.9),
            );
        }

        // `[` and `]`, drawn as REAPER draws them: a full-height bracket
        // with a vertical stem through it, so the stem marks the exact
        // sample the edge would land on.
        Cursor::EdgeLeft => bracket(scene, at, color, w, true, false),
        Cursor::EdgeRight => bracket(scene, at, color, w, false, false),
        Cursor::StretchLeft => bracket(scene, at, color, w, true, true),
        Cursor::StretchRight => bracket(scene, at, color, w, false, true),

        Cursor::Move => {
            arrow_line(scene, at, color, w, (c, c), (c - 9.0, c), 3.0);
            arrow_line(scene, at, color, w, (c, c), (c + 9.0, c), 3.0);
            arrow_line(scene, at, color, w, (c, c), (c, c - 9.0), 3.0);
            arrow_line(scene, at, color, w, (c, c), (c, c + 9.0), 3.0);
        }
        Cursor::MoveH | Cursor::RazorEdge => {
            arrow_line(scene, at, color, w, (c, c), (c - 9.0, c), 3.5);
            arrow_line(scene, at, color, w, (c, c), (c + 9.0, c), 3.5);
        }
        Cursor::MoveV | Cursor::Velocity => {
            arrow_line(scene, at, color, w, (c, c), (c, c - 9.0), 3.5);
            arrow_line(scene, at, color, w, (c, c), (c, c + 9.0), 3.5);
        }
        Cursor::Scale => {
            // The same double arrow, but pinned to a pivot bar: what
            // scaling does is spread about a line, not translate.
            arrow_line(scene, at, color, w, (c, c), (c, c - 9.0), 3.5);
            arrow_line(scene, at, color, w, (c, c), (c, c + 9.0), 3.5);
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c - 7.0, c), Point::new(c + 7.0, c)),
            );
        }
        Cursor::Copy => {
            arrow(scene, at, color, w);
            plus(scene, at, color, w, SIZE - 5.0, 5.0, 4.0);
        }

        Cursor::Pencil => pencil(scene, at, color, w),
        Cursor::Brush => {
            pencil(scene, at, color, w);
            // Three grid ticks trailing the tip: a brush lays down a run
            // of notes, not one.
            for i in 0..3 {
                let x = 5.0 + i as f64 * 4.0;
                scene.stroke(
                    &s,
                    at,
                    color,
                    None,
                    &Line::new(Point::new(x, SIZE - 1.0), Point::new(x + 2.5, SIZE - 1.0)),
                );
            }
        }
        Cursor::Curve => {
            // An S from the tip: the shaped-ramp gesture.
            let mut p = BezPath::new();
            p.move_to(Point::new(2.0, SIZE - 2.0));
            p.curve_to(
                Point::new(8.0, SIZE - 4.0),
                Point::new(10.0, 6.0),
                Point::new(SIZE - 3.0, 3.0),
            );
            scene.stroke(&s, at, color, None, &p);
        }
        Cursor::Eraser | Cursor::NoteEraser => {
            // A tilted block, and for notes a cross through it: wiping a
            // controller back to default is not the same as deleting.
            let mut p = BezPath::new();
            p.move_to(Point::new(3.0, SIZE - 6.0));
            p.line_to(Point::new(11.0, SIZE - 14.0));
            p.line_to(Point::new(SIZE - 4.0, SIZE - 20.0));
            p.line_to(Point::new(SIZE - 12.0, SIZE - 12.0));
            p.close_path();
            scene.stroke(&s, at, color, None, &p);
            if cursor == Cursor::NoteEraser {
                cross_diag(scene, at, color, w, SIZE - 6.0, 6.0, 3.5);
            }
        }

        Cursor::Hand | Cursor::HandClosed => {
            hand(scene, at, color, w, cursor == Cursor::HandClosed)
        }
        Cursor::Zoom => {
            scene.stroke(&s, at, color, None, &Circle::new(Point::new(9.0, 9.0), 6.0));
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(13.5, 13.5), Point::new(SIZE - 3.0, SIZE - 3.0)),
            );
            plus(scene, at, color, w, 9.0, 9.0, 3.0);
        }
        Cursor::Playhead | Cursor::Split => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c, 2.0), Point::new(c, SIZE - 2.0)),
            );
            // The playhead carries its flag; a split carries opposed
            // arrows, because it pushes material both ways.
            if cursor == Cursor::Playhead {
                let mut p = BezPath::new();
                p.move_to(Point::new(c, 2.0));
                p.line_to(Point::new(c + 7.0, 5.0));
                p.line_to(Point::new(c, 8.0));
                p.close_path();
                scene.fill(Fill::NonZero, at, color, None, &p);
            } else {
                arrow_line(scene, at, color, w, (c - 2.0, c), (c - 8.0, c), 3.0);
                arrow_line(scene, at, color, w, (c + 2.0, c), (c + 8.0, c), 3.0);
            }
        }
        Cursor::Audition => {
            // A speaker cone with one wave.
            let mut p = BezPath::new();
            p.move_to(Point::new(4.0, c - 3.0));
            p.line_to(Point::new(8.0, c - 3.0));
            p.line_to(Point::new(12.0, c - 8.0));
            p.line_to(Point::new(12.0, c + 8.0));
            p.line_to(Point::new(8.0, c + 3.0));
            p.line_to(Point::new(4.0, c + 3.0));
            p.close_path();
            scene.stroke(&s, at, color, None, &p);
            let mut wave = BezPath::new();
            wave.move_to(Point::new(15.0, c - 5.0));
            wave.curve_to(
                Point::new(19.0, c - 2.0),
                Point::new(19.0, c + 2.0),
                Point::new(15.0, c + 5.0),
            );
            scene.stroke(&s, at, color, None, &wave);
        }
        Cursor::Mute => {
            arrow(scene, at, color, w);
            cross_diag(scene, at, color, w, SIZE - 6.0, 6.0, 4.0);
        }
        Cursor::Text => {
            // An I-beam, at the pointer rather than the corner.
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c, 3.0), Point::new(c, SIZE - 3.0)),
            );
            for y in [3.0, SIZE - 3.0] {
                scene.stroke(
                    &s,
                    at,
                    color,
                    None,
                    &Line::new(Point::new(c - 3.0, y), Point::new(c + 3.0, y)),
                );
            }
        }
        Cursor::Razor => {
            // The cut first: a line down through the hotspot, the time it
            // cuts at, crossed by a short bar so the eye finds the point.
            scene.stroke(&s, at, color, None, &Line::new((c, c - 9.0), (c, c + 9.0)));
            scene.stroke(&s, at, color, None, &Line::new((c - 3.5, c), (c + 3.5, c)));
            // Then the blade, up and to the right of it, clear of the
            // cut: a double-edged blade, its slot down the middle.
            // The notches in its short ends are what make it read as a
            // blade rather than a pill.
            let (l, r, t, b) = (c + 3.0, c + 12.5, c - 11.5, c - 4.5);
            let mid = (t + b) * 0.5;
            let mut blade = BezPath::new();
            blade.move_to((l, t));
            blade.line_to((r, t));
            blade.line_to((r, mid - 1.5));
            blade.line_to((r - 1.8, mid));
            blade.line_to((r, mid + 1.5));
            blade.line_to((r, b));
            blade.line_to((l, b));
            blade.line_to((l, mid + 1.5));
            blade.line_to((l + 1.8, mid));
            blade.line_to((l, mid - 1.5));
            blade.close_path();
            scene.stroke(&s, at, color, None, &blade);
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new((l + 3.5, mid), (r - 3.5, mid)),
            );
        }

        Cursor::Handle(handle) => handle_glyph(scene, at, color, w, handle),

        Cursor::Forbidden => {
            scene.stroke(&s, at, color, None, &Circle::new(Point::new(c, c), 8.0));
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(c - 5.6, c + 5.6), Point::new(c + 5.6, c - 5.6)),
            );
        }
    }
}

/// The seven note handles, each drawn as what it does to the note.
///
/// They share a common frame — a short note bar with the affected part
/// marked — so the set reads as one family rather than seven unrelated
/// icons.
fn handle_glyph(scene: &mut Scene, at: Affine, color: Color, w: f64, handle: Handle) {
    let s = stroke(w);
    let c = SIZE * 0.5;
    match handle {
        // Coarse pitch: the note, moving between rows.
        Handle::Pitch => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Rect::new(5.0, c - 2.5, SIZE - 5.0, c + 2.5),
            );
            arrow_line(scene, at, color, w, (c, c - 4.0), (c, 2.0), 3.0);
            arrow_line(scene, at, color, w, (c, c + 4.0), (c, SIZE - 2.0), 3.0);
        }
        // Fine pitch: the same, dotted — cents, not rows.
        Handle::FinePitch => {
            for i in 0..4 {
                let x = 5.0 + i as f64 * 3.6;
                scene.stroke(
                    &s,
                    at,
                    color,
                    None,
                    &Line::new(Point::new(x, c), Point::new(x + 2.0, c)),
                );
            }
            arrow_line(scene, at, color, w, (c + 5.0, c - 3.0), (c + 5.0, 3.0), 2.5);
            arrow_line(
                scene,
                at,
                color,
                w,
                (c + 5.0, c + 3.0),
                (c + 5.0, SIZE - 3.0),
                2.5,
            );
        }
        // The slopes: the transition tilting in or out.
        Handle::LeftSlope | Handle::RightSlope => {
            let mut p = BezPath::new();
            if handle == Handle::LeftSlope {
                p.move_to(Point::new(3.0, SIZE - 4.0));
                p.curve_to(
                    Point::new(8.0, SIZE - 6.0),
                    Point::new(9.0, 6.0),
                    Point::new(14.0, 5.0),
                );
                p.line_to(Point::new(SIZE - 3.0, 5.0));
            } else {
                p.move_to(Point::new(3.0, 5.0));
                p.line_to(Point::new(10.0, 5.0));
                p.curve_to(
                    Point::new(15.0, 6.0),
                    Point::new(16.0, SIZE - 6.0),
                    Point::new(SIZE - 3.0, SIZE - 4.0),
                );
            }
            scene.stroke(&s, at, color, None, &p);
        }
        // Formant: the note with a shifted overtone above it.
        Handle::Formant => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Rect::new(5.0, c + 2.0, SIZE - 5.0, c + 6.0),
            );
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(Point::new(7.0, c - 4.0), Point::new(SIZE - 7.0, c - 4.0)),
            );
            arrow_line(scene, at, color, w, (c, c - 6.0), (c, 2.0), 3.0);
        }
        // Amplitude: a level bar.
        Handle::Amplitude => {
            scene.stroke(
                &s,
                at,
                color,
                None,
                &Line::new(
                    Point::new(6.0, SIZE - 4.0),
                    Point::new(SIZE - 6.0, SIZE - 4.0),
                ),
            );
            scene.fill(
                Fill::NonZero,
                at,
                color,
                None,
                &Rect::new(c - 3.0, c - 2.0, c + 3.0, SIZE - 4.0),
            );
            arrow_line(scene, at, color, w, (c, c - 4.0), (c, 3.0), 3.0);
        }
        // Vibrato: the wave, deepening.
        Handle::Vibrato => {
            let mut p = BezPath::new();
            p.move_to(Point::new(3.0, c));
            for i in 0..3 {
                let x = 3.0 + i as f64 * 6.0;
                p.curve_to(
                    Point::new(x + 1.5, c - 6.0),
                    Point::new(x + 4.5, c + 6.0),
                    Point::new(x + 6.0, c),
                );
            }
            scene.stroke(&s, at, color, None, &p);
        }
    }
}

// ── primitives ──────────────────────────────────────────────────────

/// The `[` / `]` bracket. `arrowed` adds the outward arrow that marks a
/// stretch — a gesture that reaches past the note you grabbed.
fn bracket(scene: &mut Scene, at: Affine, color: Color, w: f64, left: bool, arrowed: bool) {
    let s = stroke(w);
    let c = SIZE * 0.5;
    let top = 3.0;
    let bottom = SIZE - 3.0;
    // The stem sits on the hotspot, so the bracket's spine *is* the
    // sample the edge would snap to.
    let arm = 5.0;
    let dir = if left { 1.0 } else { -1.0 };
    let mut p = BezPath::new();
    p.move_to(Point::new(c + arm * dir, top));
    p.line_to(Point::new(c, top));
    p.line_to(Point::new(c, bottom));
    p.line_to(Point::new(c + arm * dir, bottom));
    scene.stroke(&s, at, color, None, &p);
    if arrowed {
        arrow_line(scene, at, color, w, (c, c), (c - 8.0 * dir, c), 3.0);
    }
}

fn cross(scene: &mut Scene, at: Affine, color: Color, w: f64, x: f64, y: f64, r: f64) {
    let s = stroke(w);
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x - r, y), Point::new(x + r, y)),
    );
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x, y - r), Point::new(x, y + r)),
    );
}

fn cross_diag(scene: &mut Scene, at: Affine, color: Color, w: f64, x: f64, y: f64, r: f64) {
    let s = stroke(w);
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x - r, y - r), Point::new(x + r, y + r)),
    );
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(x - r, y + r), Point::new(x + r, y - r)),
    );
}

fn plus(scene: &mut Scene, at: Affine, color: Color, w: f64, x: f64, y: f64, r: f64) {
    cross(scene, at, color, w, x, y, r);
}

/// The plain pointer, drawn from its tip at the top-left.
fn arrow(scene: &mut Scene, at: Affine, color: Color, w: f64) {
    let mut p = BezPath::new();
    p.move_to(Point::new(2.0, 2.0));
    p.line_to(Point::new(2.0, 16.0));
    p.line_to(Point::new(6.0, 12.5));
    p.line_to(Point::new(9.0, 18.5));
    p.line_to(Point::new(11.5, 17.0));
    p.line_to(Point::new(8.5, 11.5));
    p.line_to(Point::new(13.0, 11.0));
    p.close_path();
    scene.stroke(&stroke(w), at, color, None, &p);
}

/// A line from `from` to `to` with a head at `to`.
fn arrow_line(
    scene: &mut Scene,
    at: Affine,
    color: Color,
    w: f64,
    from: (f64, f64),
    to: (f64, f64),
    head: f64,
) {
    let s = stroke(w);
    let (fx, fy) = from;
    let (tx, ty) = to;
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(fx, fy), Point::new(tx, ty)),
    );
    let (dx, dy) = (tx - fx, ty - fy);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    // Perpendicular, for the two barbs.
    let (px, py) = (-uy, ux);
    let mut p = BezPath::new();
    p.move_to(Point::new(tx, ty));
    p.line_to(Point::new(
        tx - ux * head + px * head * 0.7,
        ty - uy * head + py * head * 0.7,
    ));
    p.line_to(Point::new(
        tx - ux * head - px * head * 0.7,
        ty - uy * head - py * head * 0.7,
    ));
    p.close_path();
    scene.fill(Fill::NonZero, at, color, None, &p);
}

fn pencil(scene: &mut Scene, at: Affine, color: Color, w: f64) {
    let s = stroke(w);
    // Body.
    let mut body = BezPath::new();
    body.move_to(Point::new(6.0, SIZE - 6.0));
    body.line_to(Point::new(SIZE - 5.0, 3.0));
    body.line_to(Point::new(SIZE - 2.0, 6.0));
    body.line_to(Point::new(9.0, SIZE - 3.0));
    body.close_path();
    scene.stroke(&s, at, color, None, &body);
    // Tip, at the hotspot.
    let mut tip = BezPath::new();
    tip.move_to(Point::new(2.0, SIZE - 2.0));
    tip.line_to(Point::new(6.0, SIZE - 6.0));
    tip.line_to(Point::new(9.0, SIZE - 3.0));
    tip.close_path();
    scene.fill(Fill::NonZero, at, color, None, &tip);
}

fn hand(scene: &mut Scene, at: Affine, color: Color, w: f64, closed: bool) {
    let s = stroke(w);
    // Palm.
    let palm = Rect::new(6.0, 11.0, SIZE - 5.0, SIZE - 3.0);
    scene.stroke(&s, at, color, None, &palm);
    // Fingers, short when the hand has closed on the canvas.
    let top = if closed { 8.0 } else { 3.0 };
    for i in 0..3 {
        let x = 8.0 + i as f64 * 4.0;
        scene.stroke(
            &s,
            at,
            color,
            None,
            &Line::new(Point::new(x, 11.0), Point::new(x, top)),
        );
    }
    // Thumb.
    scene.stroke(
        &s,
        at,
        color,
        None,
        &Line::new(Point::new(6.0, 13.0), Point::new(2.5, 16.0)),
    );
}
