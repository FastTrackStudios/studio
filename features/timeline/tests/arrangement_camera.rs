//! Characterisation tests for the arrangement's camera, before it moves
//! onto `timeline::Camera` (docs/design/timeline.md §4, steps 2 and 8).
//!
//! The arrangement's camera lives in `session-daw`'s `ArrangementPanel`
//! (`apps/session/daw/src/panel.rs`): `extent`, `limits`, `rezoom` and the
//! wheel / pinch / pan / held-`z` drag arms of `handle`. Those are methods
//! over dioxus signals, and this crate sits below `session-daw`, so
//! [`ArrangeCamera`] is a copy of that arithmetic, line for line, over
//! plain fields. It calls this crate's real `gestures::{wheel,
//! zoom_factor}`, as the panel does.
//!
//! What these tests pin is what the arrangement does TODAY. Step 8
//! replaces the fixture with `timeline::Camera` + `Bounds`
//! (`time_slack_screens = 0`, `ZoomOut::Fit`); every expectation here must
//! still hold, except where a §5 decision says otherwise.
//!
//! Units: the frame is the lanes' rectangle in panel pixels; `span` is the
//! session's size unzoomed (seconds × `PPS` across, the rows' height
//! down); `zoom` multiplies it; `scroll` / `down` are in zoomed pixels.

use timeline::gestures::{self, Wheel};

/// `panel.rs` `ZOOM_IN`: twenty pixels a sample at 48 kHz across (with
/// `studio::PPS` = 40 px a second unzoomed), one row filling the view
/// down.
const ZOOM_IN: (f64, f64) = (48_000.0 * 20.0 / 40.0, 40.0);

#[derive(Clone, Copy, Debug, PartialEq)]
struct ArrangeCamera {
    /// The lanes' frame, `ArrangementPanel::frame`.
    frame: (f64, f64),
    /// `span_x`, `span_y`.
    span: (f64, f64),
    zoom: (f64, f64),
    scroll: f64,
    down: f64,
}

impl ArrangeCamera {
    fn new(frame: (f64, f64), span: (f64, f64)) -> Self {
        Self {
            frame,
            span,
            zoom: (1.0, 1.0),
            scroll: 0.0,
            down: 0.0,
        }
    }

    /// `ArrangementPanel::extent`.
    fn extent(&self, zx: f64, zy: f64) -> (f64, f64) {
        let (fw, fh) = self.frame;
        (
            (self.span.0 * zx - fw).max(0.0),
            (self.span.1 * zy - fh).max(0.0),
        )
    }

    /// `ArrangementPanel::limits`.
    fn limits(&self) -> ((f64, f64), (f64, f64)) {
        let (fw, fh) = self.frame;
        let floor = |f: f64, s: f64| {
            if s > 0.0 && f > 0.0 {
                (f / s).min(1.0)
            } else {
                1.0
            }
        };
        (
            (floor(fw, self.span.0), ZOOM_IN.0),
            (floor(fh, self.span.1), ZOOM_IN.1),
        )
    }

    /// `ArrangementPanel::rezoom`, with `about` already in frame
    /// coordinates (the panel subtracts the track column and the ruler).
    fn rezoom(&mut self, to: (f64, f64), about: (f64, f64)) {
        let (was_x, was_y) = self.zoom;
        let (fw, fh) = self.frame;
        let ax = about.0.clamp(0.0, fw);
        let ay = about.1.clamp(0.0, fh);
        let (ex, ey) = self.extent(to.0, to.1);
        let across = zoom_about(ax, self.scroll, was_x, to.0);
        let deep = zoom_about(ay, self.down, was_y, to.1);
        self.zoom = to;
        self.scroll = across.clamp(0.0, ex);
        self.down = deep.clamp(0.0, ey);
    }

    /// `PanelEvent::Wheel`, with winit's deltas (the panel negates them).
    fn wheel(&mut self, dx: f64, dy: f64, zooming: bool, shift: bool, pointer: (f64, f64)) {
        let (zx, zy) = self.zoom;
        let ((x0, x1), (y0, y1)) = self.limits();
        match gestures::wheel(-dx, -dy, zooming, shift) {
            Wheel::ZoomTime(by) => self.rezoom(((zx * by).clamp(x0, x1), zy), pointer),
            Wheel::ZoomRows(by) => self.rezoom((zx, (zy * by).clamp(y0, y1)), pointer),
            Wheel::Scroll(sx, sy) => {
                let (ex, ey) = self.extent(zx, zy);
                self.scroll = (self.scroll + sx).clamp(0.0, ex);
                self.down = (self.down + sy).clamp(0.0, ey);
            }
        }
    }

    /// `PanelEvent::Pinch`: time only.
    fn pinch(&mut self, by: f64, pointer: (f64, f64)) {
        let (zx, zy) = self.zoom;
        let ((x0, x1), _) = self.limits();
        let to = ((zx * (1.0 + by).max(0.1)).clamp(x0, x1), zy);
        self.rezoom(to, pointer);
    }

    /// `Drag::Pan`, one pointer move.
    fn pan(&mut self, moved: (f64, f64)) {
        let (ex, ey) = self.extent(self.zoom.0, self.zoom.1);
        self.scroll = (self.scroll - moved.0).clamp(0.0, ex);
        self.down = (self.down - moved.1).clamp(0.0, ey);
    }

    /// `Drag::Zoom` (held `z`, left drag), one pointer move: each move
    /// zooms by its own travel, about where the press was.
    fn zoom_drag(&mut self, moved: (f64, f64), anchor: (f64, f64)) {
        let (zx, zy) = self.zoom;
        let ((x0, x1), (y0, y1)) = self.limits();
        self.rezoom(
            (
                (zx * gestures::zoom_factor(moved.0)).clamp(x0, x1),
                (zy * gestures::zoom_factor(moved.1)).clamp(y0, y1),
            ),
            anchor,
        );
    }

    /// The session point (unzoomed) at a frame position.
    fn point_at(&self, at: (f64, f64)) -> (f64, f64) {
        (
            (self.scroll + at.0) / self.zoom.0,
            (self.down + at.1) / self.zoom.1,
        )
    }
}

/// `panel::zoom_about` (pinned on its own by `panel.rs`'s tests).
fn zoom_about(at: f64, scroll: f64, was: f64, to: f64) -> f64 {
    if was <= 0.0 {
        return scroll;
    }
    (scroll + at) / was * to - at
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * a.abs().max(b.abs()).max(1.0)
}

/// A ten-minute session (24 000 px unzoomed) of 40 rows of 32 px, in a
/// 1000 × 600 frame.
fn session() -> ArrangeCamera {
    ArrangeCamera::new((1000.0, 600.0), (600.0 * 40.0, 40.0 * 32.0))
}

// ── limits and extent ──────────────────────────────────────────────────

#[test]
fn zoom_out_stops_where_the_whole_session_fills_the_frame() {
    let cam = session();
    let ((x0, x1), (y0, y1)) = cam.limits();
    assert!(close(x0, 1000.0 / 24_000.0), "{x0}");
    assert!(close(y0, 600.0 / 1280.0), "{y0}");
    assert_eq!((x1, y1), ZOOM_IN);
    // At the floor the session is exactly the frame: nowhere to scroll.
    assert_eq!(cam.extent(x0, y0), (0.0, 0.0));
}

#[test]
fn a_session_shorter_than_the_frame_never_zooms_out_past_one() {
    let cam = ArrangeCamera::new((1000.0, 600.0), (400.0, 200.0));
    let ((x0, _), (y0, _)) = cam.limits();
    assert_eq!((x0, y0), (1.0, 1.0));
}

#[test]
fn an_empty_session_or_frame_leaves_the_floor_at_one() {
    let empty = ArrangeCamera::new((1000.0, 600.0), (0.0, 0.0));
    assert_eq!(empty.limits().0.0, 1.0);
    assert_eq!(empty.limits().1.0, 1.0);
    let no_frame = ArrangeCamera::new((0.0, 0.0), (24_000.0, 1280.0));
    assert_eq!(no_frame.limits().0.0, 1.0);
}

#[test]
fn the_extent_is_the_zoomed_session_less_the_frame_and_never_negative() {
    let cam = session();
    assert_eq!(cam.extent(1.0, 1.0), (23_000.0, 680.0));
    assert_eq!(cam.extent(2.0, 0.25), (47_000.0, 0.0));
}

// ── zoom about the pointer ─────────────────────────────────────────────

#[test]
fn a_z_wheel_zooms_time_about_the_pointer() {
    let mut cam = session();
    cam.scroll = 5_000.0;
    cam.down = 100.0;
    let pointer = (300.0, 250.0);
    let before = cam.point_at(pointer);
    // winit: +dy is a notch away from you; with z held that zooms in.
    cam.wheel(0.0, 40.0, true, false, pointer);
    assert!(cam.zoom.0 > 1.0, "{:?}", cam.zoom);
    assert_eq!(cam.zoom.1, 1.0, "rows untouched");
    let after = cam.point_at(pointer);
    assert!(close(before.0, after.0), "{before:?} → {after:?}");
    assert_eq!(cam.down, 100.0);
}

#[test]
fn a_z_shift_wheel_zooms_the_rows_about_the_pointer() {
    let mut cam = session();
    cam.down = 300.0;
    let pointer = (300.0, 250.0);
    let before = cam.point_at(pointer);
    cam.wheel(0.0, 40.0, true, true, pointer);
    assert_eq!(cam.zoom.0, 1.0, "time untouched");
    assert!(cam.zoom.1 > 1.0, "{:?}", cam.zoom);
    assert!(close(before.1, cam.point_at(pointer).1));
}

#[test]
fn a_z_wheel_towards_you_zooms_out() {
    let mut cam = session();
    cam.wheel(0.0, -40.0, true, false, (500.0, 300.0));
    assert!(cam.zoom.0 < 1.0, "{:?}", cam.zoom);
}

#[test]
fn a_pointer_outside_the_frame_zooms_about_the_nearest_edge() {
    let mut a = session();
    a.scroll = 5_000.0;
    let mut b = a;
    a.rezoom((2.0, 1.0), (-50.0, 0.0));
    b.rezoom((2.0, 1.0), (0.0, 0.0));
    assert_eq!(a, b);
    let mut c = session();
    c.scroll = 5_000.0;
    let mut d = c;
    c.rezoom((2.0, 1.0), (1500.0, 0.0));
    d.rezoom((2.0, 1.0), (1000.0, 0.0));
    assert_eq!(c, d);
}

#[test]
fn zooming_out_near_the_end_pulls_the_scroll_back_inside() {
    let mut cam = session();
    cam.zoom = (4.0, 1.0);
    cam.scroll = cam.extent(4.0, 1.0).0; // the last screen
    cam.rezoom((1.0, 1.0), (1000.0, 0.0));
    assert_eq!(cam.scroll, cam.extent(1.0, 1.0).0);
}

#[test]
fn zooming_out_to_the_floor_shows_the_whole_session_from_zero() {
    let mut cam = session();
    cam.zoom = (3.0, 2.0);
    cam.scroll = 20_000.0;
    cam.down = 500.0;
    for _ in 0..50 {
        cam.wheel(0.0, -200.0, true, false, (700.0, 0.0));
        cam.wheel(0.0, -200.0, true, true, (0.0, 400.0));
    }
    let ((x0, _), (y0, _)) = cam.limits();
    assert_eq!(cam.zoom, (x0, y0));
    assert_eq!((cam.scroll, cam.down), (0.0, 0.0));
}

#[test]
fn zoom_in_stops_at_the_cap() {
    let mut cam = session();
    for _ in 0..200 {
        cam.wheel(0.0, 400.0, true, false, (500.0, 0.0));
        cam.wheel(0.0, 400.0, true, true, (0.0, 300.0));
    }
    assert_eq!(cam.zoom, ZOOM_IN);
}

// ── wheel scroll, pinch, pan ───────────────────────────────────────────

#[test]
fn a_plain_wheel_scrolls_and_stops_at_the_ends() {
    let mut cam = session();
    // winit's +dy is up: the panel negates it, so the view moves back.
    cam.wheel(0.0, 40.0, false, false, (0.0, 0.0));
    assert_eq!((cam.scroll, cam.down), (0.0, 0.0));
    cam.wheel(0.0, -40.0, false, false, (0.0, 0.0));
    assert_eq!(cam.down, 40.0);
    cam.wheel(0.0, -10_000.0, false, false, (0.0, 0.0));
    assert_eq!(cam.down, 680.0);
    cam.wheel(-30.0, 0.0, false, false, (0.0, 0.0));
    assert_eq!(cam.scroll, 30.0);
}

#[test]
fn a_shifted_wheel_scrolls_time() {
    let mut cam = session();
    cam.wheel(0.0, -60.0, false, true, (0.0, 0.0));
    assert_eq!((cam.scroll, cam.down), (60.0, 0.0));
}

#[test]
fn a_pinch_zooms_time_only_about_the_pointer() {
    let mut cam = session();
    cam.scroll = 2_000.0;
    let pointer = (400.0, 200.0);
    let before = cam.point_at(pointer);
    cam.pinch(0.5, pointer);
    assert!(close(cam.zoom.0, 1.5));
    assert_eq!(cam.zoom.1, 1.0);
    assert!(close(before.0, cam.point_at(pointer).0));
    // A pinch never scales by less than a tenth in one step.
    cam.pinch(-5.0, pointer);
    assert!(close(cam.zoom.0, (1.5 * 0.1f64).max(cam.limits().0.0)));
}

#[test]
fn the_hand_moves_the_session_with_it_and_stops_at_the_ends() {
    let mut cam = session();
    cam.scroll = 1_000.0;
    cam.down = 100.0;
    cam.pan((200.0, 50.0));
    assert_eq!((cam.scroll, cam.down), (800.0, 50.0));
    cam.pan((5_000.0, 5_000.0));
    assert_eq!((cam.scroll, cam.down), (0.0, 0.0));
    cam.pan((-1e9, -1e9));
    assert_eq!((cam.scroll, cam.down), cam.extent(1.0, 1.0));
}

// ── the held-z drag ────────────────────────────────────────────────────

#[test]
fn the_z_drag_right_zooms_time_in_and_down_zooms_rows_in() {
    // Q4 (decided): down zooms in, as REAPER does. `ZoomDrag` is up-in
    // today and flips in step 10; this is the side that already agrees.
    let mut cam = session();
    cam.zoom_drag((50.0, 0.0), (500.0, 300.0));
    assert!(cam.zoom.0 > 1.0 && cam.zoom.1 == 1.0, "{:?}", cam.zoom);
    let mut cam = session();
    cam.zoom_drag((0.0, 50.0), (500.0, 300.0));
    assert!(cam.zoom.1 > 1.0 && cam.zoom.0 == 1.0, "{:?}", cam.zoom);
    let mut cam = session();
    cam.zoom_drag((-50.0, -50.0), (500.0, 300.0));
    assert!(cam.zoom.0 < 1.0 && cam.zoom.1 < 1.0, "{:?}", cam.zoom);
}

#[test]
fn the_z_drag_zooms_by_the_shared_gain() {
    let mut cam = session();
    cam.zoom_drag((gestures::ZOOM_GAIN, 0.0), (0.0, 0.0));
    assert!(close(cam.zoom.0, std::f64::consts::E));
}

#[test]
fn the_z_drag_keeps_the_press_point_still_not_the_pointer() {
    let mut cam = session();
    cam.scroll = 3_000.0;
    cam.down = 200.0;
    let press = (400.0, 300.0);
    let before = cam.point_at(press);
    // The pointer travels; the zoom is about where the press was.
    for _ in 0..5 {
        cam.zoom_drag((20.0, 10.0), press);
    }
    let after = cam.point_at(press);
    assert!(
        close(before.0, after.0) && close(before.1, after.1),
        "{before:?} → {after:?}"
    );
}

#[test]
fn the_z_drag_is_a_sum_of_moves_so_the_path_does_not_matter_inside_the_limits() {
    let mut one = session();
    one.zoom_drag((90.0, 30.0), (500.0, 300.0));
    let mut many = session();
    for _ in 0..3 {
        many.zoom_drag((30.0, 10.0), (500.0, 300.0));
    }
    assert!(close(one.zoom.0, many.zoom.0) && close(one.zoom.1, many.zoom.1));
}

#[test]
fn the_z_drag_turns_back_at_once_after_hitting_a_limit() {
    // Each move zooms from where the last one left off, clamped: travel
    // past a limit is lost, so reversing zooms back out immediately. A
    // drag measured from the press (`ZoomDrag`) would first have to
    // unwind the overshoot. Step 10 picks one; this pins A today.
    let mut cam = session();
    let anchor = (500.0, 300.0);
    cam.zoom_drag((0.0, 2_000.0), anchor); // far past the row cap
    assert_eq!(cam.zoom.1, ZOOM_IN.1);
    cam.zoom_drag((0.0, -10.0), anchor);
    assert!(cam.zoom.1 < ZOOM_IN.1);
    assert!(close(cam.zoom.1, ZOOM_IN.1 * gestures::zoom_factor(-10.0)));
}
