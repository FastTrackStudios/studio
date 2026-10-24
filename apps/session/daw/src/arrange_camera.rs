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
