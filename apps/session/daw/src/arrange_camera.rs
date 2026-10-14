//! The arrangement's camera: where the lanes are looked at from, in the
//! panel's own terms (a zoom on [`PPS`], a scroll in zoomed pixels each
//! way), with every move made on [`timeline::camera::Camera`] and held by
//! the arrangement's [`Bounds`].
//!
//! The panel keeps the numbers in signals the chrome renders from; this
//! is the arithmetic, so it can be tested without a window.

use timeline::camera::{Align, Bounds, Camera, RowView, RowsFloor, TimeView, Viewport, ZoomOut};
use timeline::gestures::{self, Wheel, ZoomDrag};

use crate::studio::PPS;

/// How far in each zoom may go: across, to twenty pixels a sample at 48
/// kHz ([`PPS`] a second unzoomed) — an edit is placed on the sample, and
/// the waveform is drawn down to them; down, until one row fills the
/// view, as framing a track does in REAPER (a row is 32 pixels unzoomed
/// at its least).
pub const ZOOM_IN: (f64, f64) = (48_000.0 * 20.0 / PPS, 40.0);

/// A held-`z` drag in flight: the shared drag, where it was pressed (what
/// it zooms about), and the zoom it started from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomGesture {
    drag: ZoomDrag,
    anchor: (f64, f64),
    base: (f64, f64),
}

/// The view of the lanes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrangeCamera {
    /// The lanes' frame, in panel pixels.
    pub frame: (f64, f64),
    /// The session unzoomed: seconds × [`PPS`] across, the rows' height
    /// down.
    pub span: (f64, f64),
    /// Multiplies `span` each way.
    pub zoom: (f64, f64),
    /// Scrolled, in zoomed pixels.
    pub scroll: f64,
    pub down: f64,
}

impl ArrangeCamera {
    #[must_use]
    pub const fn new(frame: (f64, f64), span: (f64, f64)) -> Self {
        Self {
            frame,
            span,
            zoom: (1.0, 1.0),
            scroll: 0.0,
            down: 0.0,
        }
    }

    fn viewport(&self) -> Viewport {
        Viewport::new(self.frame.0, self.frame.1)
    }

    /// As the shared camera: seconds across, unzoomed pixels down.
    #[must_use]
    pub fn camera(&self) -> Camera {
        let pps = PPS * self.zoom.0;
        Camera {
            time: TimeView {
                t0: self.scroll / pps,
                units_per_px: 1.0 / pps,
            },
            rows: RowView {
                y0: self.down / self.zoom.1,
                px_per_unit: self.zoom.1,
            },
        }
    }

    /// Back from [`ArrangeCamera::camera`].
    pub fn set_camera(&mut self, camera: Camera) {
        let pps = 1.0 / camera.time.units_per_px;
        self.zoom = (pps / PPS, camera.rows.px_per_unit);
        self.scroll = camera.time.t0 * pps;
        self.down = camera.rows.y0 * camera.rows.px_per_unit;
    }

    /// A closed timeline from zero to the session's end, out no further
    /// than the whole session filling the frame (and never past unzoomed),
    /// rows from the top to the last, never stretched past unzoomed.
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        Bounds {
            t_min: 0.0,
            t_max: self.span.0 / PPS,
            time_slack_screens: 0.0,
            edge_whitespace: 0.0,
            min_units_per_px: 1.0 / (PPS * ZOOM_IN.0),
            max_zoom_out: ZoomOut::Fit {
                at_least: 1.0 / PPS,
            },
            r_min: 0.0,
            r_max: self.span.1,
            rows_floor: RowsFloor::FitAtMost(1.0),
            max_px_per_unit: ZOOM_IN.1,
            align: Align::Start,
        }
    }

    /// How far the view can scroll each way, at a zoom.
    #[must_use]
    pub fn extent(&self, zx: f64, zy: f64) -> (f64, f64) {
        let (fw, fh) = self.frame;
        (
            self.span.0.mul_add(zx, -fw).max(0.0),
            self.span.1.mul_add(zy, -fh).max(0.0),
        )
    }

    /// The zoom's range each way: out no further than the whole session
    /// filling the frame (and never past unzoomed), and never above
    /// [`ZOOM_IN`] — [`ArrangeCamera::bounds`]' scales, as zooms.
    #[must_use]
    pub fn limits(&self) -> ((f64, f64), (f64, f64)) {
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

    /// Hold the scroll inside the session at the zoom it is at (which
    /// its setter held within [`ArrangeCamera::limits`]).
    pub fn clamp(&mut self) {
        let mut camera = self.camera();
        camera.constrain(&self.bounds(), self.viewport());
        // Only the scroll moves: the zoom is kept as given rather than
        // round-tripped through units a pixel.
        self.scroll = camera.time.t0 * PPS * self.zoom.0;
        self.down = camera.rows.y0 * self.zoom.1;
    }

    /// Set the zoom (already within [`ArrangeCamera::limits`]), keeping
    /// the session point under `about` (frame pixels, held to the frame)
    /// where it is on screen.
    pub fn rezoom(&mut self, to: (f64, f64), about: (f64, f64)) {
        let (fw, fh) = self.frame;
        let (ax, ay) = (about.0.clamp(0.0, fw), about.1.clamp(0.0, fh));
        let mut camera = self.camera();
        let anchor_t = camera.time.t_at(ax);
        let anchor_r = camera.rows.r_at(ay);
        camera.time.zoom_about(anchor_t, to.0 / self.zoom.0);
        camera.rows.zoom_about(anchor_r, to.1 / self.zoom.1);
        self.zoom = to;
        self.scroll = camera.time.t0 * PPS * to.0;
        self.down = camera.rows.y0 * to.1;
        self.clamp();
    }

    /// A wheel, with winit's deltas (negated here, as winit's run the
    /// other way to a browser's), at `pointer` in frame pixels.
    pub fn wheel(&mut self, dx: f64, dy: f64, zooming: bool, shift: bool, pointer: (f64, f64)) {
        let (zx, zy) = self.zoom;
        let ((x0, x1), (y0, y1)) = self.limits();
        match gestures::wheel(-dx, -dy, zooming, shift) {
            Wheel::ZoomTime(by) => self.rezoom(((zx * by).clamp(x0, x1), zy), pointer),
            Wheel::ZoomRows(by) => self.rezoom((zx, (zy * by).clamp(y0, y1)), pointer),
            Wheel::Scroll(sx, sy) => self.pan((-sx, -sy)),
        }
    }

    /// A pinch: time only, about `pointer`.
    pub fn pinch(&mut self, by: f64, pointer: (f64, f64)) {
        let (zx, zy) = self.zoom;
        let ((x0, x1), _) = self.limits();
        self.rezoom(((zx * (1.0 + by).max(0.1)).clamp(x0, x1), zy), pointer);
    }

    /// The hand: the session moves with the pointer.
    pub fn pan(&mut self, moved: (f64, f64)) {
        self.scroll -= moved.0;
        self.down -= moved.1;
        self.clamp();
    }

    /// The held-`z` drag takes hold at `at` (frame pixels).
    #[must_use]
    pub const fn zoom_press(&self, at: (f64, f64)) -> ZoomGesture {
        ZoomGesture {
            drag: ZoomDrag::press(at, false),
            anchor: at,
            base: self.zoom,
        }
    }

    /// The held-`z` drag, its pointer at `at` (frame pixels): right zooms
    /// time in, down zooms the rows in (as REAPER's zoom tool), measured
    /// from the press and about the press. At a limit the travel past it
    /// is dropped ([`ZoomDrag::rebase`]), so turning back zooms back at
    /// once.
    pub fn zoom_to(&mut self, gesture: &mut ZoomGesture, at: (f64, f64)) {
        gesture.drag.moved(at);
        let Some((fx, fy)) = gesture.drag.factors(false) else {
            return;
        };
        let ((x0, x1), (y0, y1)) = self.limits();
        let base = gesture.base;
        let to = ((base.0 * fx).clamp(x0, x1), (base.1 * fy).clamp(y0, y1));
        self.rezoom(to, gesture.anchor);
        gesture.drag.rebase((to.0 / base.0, to.1 / base.1), false);
    }

    /// The session point (unzoomed pixels) at a frame position.
    #[must_use]
    pub fn point_at(&self, at: (f64, f64)) -> (f64, f64) {
        (
            (self.scroll + at.0) / self.zoom.0,
            (self.down + at.1) / self.zoom.1,
        )
    }
}

/// What the arrangement's camera does, pinned before it moved onto
/// `timeline::Camera` (docs/design/timeline.md §4, steps 2 and 8).
#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6 * a.abs().max(b.abs()).max(1.0)
    }

    /// A ten-minute session (24 000 px unzoomed) of 40 rows of 32 px, in
    /// a 1000 × 600 frame.
    fn session() -> ArrangeCamera {
        ArrangeCamera::new((1000.0, 600.0), (600.0 * 40.0, 40.0 * 32.0))
    }

    /// A held-`z` drag pressed at `at` and moved by each of `moves` in
    /// turn.
    fn zoom_drag(cam: &mut ArrangeCamera, at: (f64, f64), moves: &[(f64, f64)]) {
        let mut gesture = cam.zoom_press(at);
        let mut pointer = at;
        for m in moves {
            pointer = (pointer.0 + m.0, pointer.1 + m.1);
            cam.zoom_to(&mut gesture, pointer);
        }
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
        // Q4 (decided): down zooms in, as REAPER does.
        let mut cam = session();
        zoom_drag(&mut cam, (500.0, 300.0), &[(50.0, 0.0)]);
        assert!(cam.zoom.0 > 1.0 && cam.zoom.1 == 1.0, "{:?}", cam.zoom);
        let mut cam = session();
        zoom_drag(&mut cam, (500.0, 300.0), &[(0.0, 50.0)]);
        assert!(cam.zoom.1 > 1.0 && cam.zoom.0 == 1.0, "{:?}", cam.zoom);
        let mut cam = session();
        zoom_drag(&mut cam, (500.0, 300.0), &[(-50.0, -50.0)]);
        assert!(cam.zoom.0 < 1.0 && cam.zoom.1 < 1.0, "{:?}", cam.zoom);
    }

    #[test]
    fn the_z_drag_zooms_by_the_shared_gain() {
        let mut cam = session();
        zoom_drag(&mut cam, (0.0, 0.0), &[(gestures::ZOOM_GAIN, 0.0)]);
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
        zoom_drag(&mut cam, press, &[(20.0, 10.0); 5]);
        let after = cam.point_at(press);
        assert!(
            close(before.0, after.0) && close(before.1, after.1),
            "{before:?} → {after:?}"
        );
    }

    #[test]
    fn the_z_drag_is_a_sum_of_moves_so_the_path_does_not_matter_inside_the_limits() {
        let mut one = session();
        zoom_drag(&mut one, (500.0, 300.0), &[(90.0, 30.0)]);
        let mut many = session();
        zoom_drag(
            &mut many,
            (500.0, 300.0),
            &[(30.0, 10.0), (50.0, -5.0), (10.0, 25.0)],
        );
        assert!(close(one.zoom.0, many.zoom.0) && close(one.zoom.1, many.zoom.1));
    }

    #[test]
    fn the_z_drag_turns_back_at_once_after_hitting_a_limit() {
        // Travel past a limit is dropped (`ZoomDrag::rebase`), so in one
        // drag reversing zooms back out immediately rather than first
        // unwinding the overshoot.
        let mut cam = session();
        let anchor = (500.0, 300.0);
        let mut gesture = cam.zoom_press(anchor);
        cam.zoom_to(&mut gesture, (500.0, 2_300.0)); // far past the row cap
        assert_eq!(cam.zoom.1, ZOOM_IN.1);
        cam.zoom_to(&mut gesture, (500.0, 2_290.0));
        assert!(cam.zoom.1 < ZOOM_IN.1);
        assert!(close(cam.zoom.1, ZOOM_IN.1 * gestures::zoom_factor(-10.0)));
    }

    /// The limits are the shared bounds' scales, in zooms.
    #[test]
    fn the_limits_are_the_bounds() {
        for span in [(24_000.0, 1280.0), (400.0, 200.0), (1e7, 50_000.0)] {
            let cam = ArrangeCamera::new((1000.0, 600.0), span);
            let b = cam.bounds();
            let vp = Viewport::new(1000.0, 600.0);
            let (least, most) = b.time_scale(vp);
            let (floor, ceiling) = b.row_scale(vp);
            let ((x0, x1), (y0, y1)) = cam.limits();
            assert!(close(x0, 1.0 / (PPS * most)), "{span:?}");
            assert!(close(x1, 1.0 / (PPS * least)), "{span:?}");
            assert!(close(y0, floor) && close(y1, ceiling), "{span:?}");
        }
    }
}
