//! Where a surface is looking: time across ([`TimeView`]), rows down
//! ([`RowView`]), and the one clamp that holds both inside what there is
//! to show ([`Camera::constrain`]).
//!
//! Time is in the surface's units, rows in row units counted down from
//! the top (a bottom-up surface, a piano roll, flips its rows before they
//! get here). Screen positions are px from the lanes' origin.

/// The lanes' size in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub w: f64,
    pub h: f64,
}

impl Viewport {
    /// At least a pixel each way: a zero-size view has no scale.
    #[must_use]
    pub const fn new(w: f64, h: f64) -> Self {
        Self {
            w: w.max(1.0),
            h: h.max(1.0),
        }
    }
}

/// Across: the time at the left edge and how much time a pixel is. The
/// arrangement's `(scroll_x, pps)` is `t0 = scroll_x / pps`,
/// `units_per_px = 1 / pps`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeView {
    pub t0: f64,
    pub units_per_px: f64,
}

impl TimeView {
    #[must_use]
    pub fn x(&self, t: f64) -> f64 {
        (t - self.t0) / self.units_per_px
    }

    #[must_use]
    pub fn t_at(&self, x: f64) -> f64 {
        x.mul_add(self.units_per_px, self.t0)
    }

    /// `(left, right)` across a view `w` wide.
    #[must_use]
    pub fn span(&self, w: f64) -> (f64, f64) {
        (self.t0, w.mul_add(self.units_per_px, self.t0))
    }

    /// Zoom by `factor` (> 1 zooms in) keeping `anchor_t` on its pixel.
    pub fn zoom_about(&mut self, anchor_t: f64, factor: f64) {
        let x = self.x(anchor_t);
        self.units_per_px /= factor.max(1e-6);
        self.t0 = x.mul_add(-self.units_per_px, anchor_t);
    }

    /// Move the content `dx` px right (the view left).
    pub fn pan_px(&mut self, dx: f64) {
        self.t0 -= dx * self.units_per_px;
    }
}

/// Down: the row at the top edge and how tall a row unit is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowView {
    pub y0: f64,
    pub px_per_unit: f64,
}

impl RowView {
    #[must_use]
    pub fn y(&self, r: f64) -> f64 {
        (r - self.y0) * self.px_per_unit
    }

    #[must_use]
    pub fn r_at(&self, y: f64) -> f64 {
        self.y0 + y / self.px_per_unit.max(1e-12)
    }

    /// `(top, bottom)` down a view `h` tall.
    #[must_use]
    pub fn span(&self, h: f64) -> (f64, f64) {
        (self.y0, self.y0 + h / self.px_per_unit.max(1e-12))
    }

    /// Zoom by `factor` (> 1 zooms in) keeping `anchor_r` on its pixel.
    pub fn zoom_about(&mut self, anchor_r: f64, factor: f64) {
        let y = self.y(anchor_r);
        self.px_per_unit = (self.px_per_unit * factor.max(1e-6)).max(1e-12);
        self.y0 = anchor_r - y / self.px_per_unit;
    }

    /// Move the content `dy` px down (the view up).
    pub fn pan_px(&mut self, dy: f64) {
        self.y0 -= dy / self.px_per_unit.max(1e-12);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub time: TimeView,
    pub rows: RowView,
}

/// How far time may zoom out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoomOut {
    /// To `k` times the span across the view: the Editor's, so a take can
    /// be seen against what surrounds it.
    Times(f64),
    /// To the span filling the view, and never past `at_least` units a
    /// pixel (the arrangement's unzoomed scale: a short session is not
    /// shrunk to fit).
    Fit { at_least: f64 },
}

/// How small a row may get.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowsFloor {
    /// Exactly the rows filling the view: never empty space past them
    /// (the Editor's keys).
    Fit,
    /// The rows filling the view, but no taller than `px` a unit (the
    /// arrangement's tracks: a session with few is not stretched).
    FitAtMost(f64),
}

/// Where rows that do not fill the view sit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Center,
    Start,
}

/// The limits [`Camera::constrain`] holds a camera to. Clamps, not
/// magnets: hitting one discards the requested move, so they are applied
/// once, after everything else.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// The time there is to show.
    pub t_min: f64,
    pub t_max: f64,
    /// Screens of slack allowed past either end (the Editor 1, the
    /// arrangement 0).
    pub time_slack_screens: f64,
    /// And a fraction of the span past either end, whichever is more.
    pub edge_whitespace: f64,
    pub min_units_per_px: f64,
    pub max_zoom_out: ZoomOut,
    /// The rows there are, top and bottom edges in row units.
    pub r_min: f64,
    pub r_max: f64,
    pub rows_floor: RowsFloor,
    pub max_px_per_unit: f64,
    pub align: Align,
}

impl Bounds {
    /// The least and most time a pixel may be, across a view `vp` wide.
    #[must_use]
    pub fn time_scale(&self, vp: Viewport) -> (f64, f64) {
        let span = (self.t_max - self.t_min).max(1e-9);
        let w = vp.w.max(1.0);
        let most = match self.max_zoom_out {
            ZoomOut::Times(k) => span * k / w,
            ZoomOut::Fit { at_least } => (span / w).max(at_least),
        };
        let least = self.min_units_per_px.max(1e-9);
        (least, most.max(least))
    }

    /// The least and most pixels a row unit may be, down a view `vp`
    /// tall.
    #[must_use]
    pub fn row_scale(&self, vp: Viewport) -> (f64, f64) {
        let fit = vp.h.max(1.0) / (self.r_max - self.r_min).max(1e-9);
        let floor = match self.rows_floor {
            RowsFloor::Fit => fit,
            RowsFloor::FitAtMost(px) => fit.min(px),
        };
        (floor, self.max_px_per_unit.max(floor))
    }
}

impl Camera {
    /// Hold the camera inside `b`.
    ///
    /// The two axes get opposite treatment where the surface asks: time
    /// may be open (slack past the ends, to see what is either side of a
    /// take) while rows are closed (fitted to what there is, never empty
    /// space past the last).
    pub fn constrain(&mut self, b: &Bounds, vp: Viewport) {
        // ── time ─────────────────────────────────────────────────────
        let span = (b.t_max - b.t_min).max(1e-9);
        let (least, most) = b.time_scale(vp);
        let t = &mut self.time;
        t.units_per_px = t.units_per_px.clamp(least, most);
        let visible = vp.w.max(1.0) * t.units_per_px;
        let slack = (visible * b.time_slack_screens).max(span * b.edge_whitespace);
        let lo = b.t_min - slack;
        let hi = (b.t_max + slack - visible).max(lo);
        t.t0 = t.t0.clamp(lo, hi);

        // ── rows ─────────────────────────────────────────────────────
        let rows = (b.r_max - b.r_min).max(1e-9);
        let (floor, ceiling) = b.row_scale(vp);
        let r = &mut self.rows;
        r.px_per_unit = r.px_per_unit.clamp(floor, ceiling);
        let visible = vp.h.max(1.0) / r.px_per_unit.max(1e-12);
        r.y0 = if visible <= rows {
            r.y0.clamp(b.r_min, b.r_max - visible)
        } else {
            match b.align {
                Align::Center => b.r_min - (visible - rows) / 2.0,
                Align::Start => b.r_min,
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Bounds {
        Bounds {
            t_min: 0.0,
            t_max: 100.0,
            time_slack_screens: 0.0,
            edge_whitespace: 0.0,
            min_units_per_px: 0.01,
            max_zoom_out: ZoomOut::Fit { at_least: 0.025 },
            r_min: 0.0,
            r_max: 1280.0,
            rows_floor: RowsFloor::FitAtMost(1.0),
            max_px_per_unit: 40.0,
            align: Align::Start,
        }
    }

    #[test]
    fn zoom_about_keeps_the_anchor_on_its_pixel() {
        let mut t = TimeView {
            t0: 10.0,
            units_per_px: 0.5,
        };
        let x = t.x(30.0);
        t.zoom_about(30.0, 4.0);
        assert!((t.x(30.0) - x).abs() < 1e-9);
        let mut r = RowView {
            y0: 5.0,
            px_per_unit: 2.0,
        };
        let y = r.y(40.0);
        r.zoom_about(40.0, 0.5);
        assert!((r.y(40.0) - y).abs() < 1e-9);
    }

    #[test]
    fn a_closed_timeline_scrolls_from_zero_to_the_last_screen() {
        let mut c = Camera {
            time: TimeView {
                t0: -5.0,
                units_per_px: 0.05,
            },
            rows: RowView {
                y0: 0.0,
                px_per_unit: 1.0,
            },
        };
        let vp = Viewport::new(1000.0, 600.0);
        c.constrain(&bounds(), vp);
        assert_eq!(c.time.t0, 0.0);
        c.time.t0 = 1e6;
        c.constrain(&bounds(), vp);
        assert!((c.time.t0 - 50.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_out_stops_at_the_fit_but_never_past_unzoomed() {
        let vp = Viewport::new(1000.0, 600.0);
        let mut c = Camera {
            time: TimeView {
                t0: 0.0,
                units_per_px: 10.0,
            },
            rows: RowView {
                y0: 0.0,
                px_per_unit: 0.01,
            },
        };
        c.constrain(&bounds(), vp);
        assert!((c.time.units_per_px - 0.1).abs() < 1e-12);
        assert!((c.rows.px_per_unit - 600.0 / 1280.0).abs() < 1e-12);
        // A short session: the floor is the unzoomed scale.
        let short = Bounds {
            t_max: 10.0,
            r_max: 200.0,
            ..bounds()
        };
        c.constrain(&short, vp);
        assert!((c.time.units_per_px - 0.025).abs() < 1e-12);
        assert!((c.rows.px_per_unit - 1.0).abs() < 1e-12);
        assert_eq!(c.rows.y0, 0.0, "rows that fit sit at the top");
    }

    #[test]
    fn centred_rows_that_fit_sit_in_the_middle() {
        let b = Bounds {
            r_min: 10.0,
            r_max: 20.0,
            rows_floor: RowsFloor::Fit,
            max_px_per_unit: 400.0,
            align: Align::Center,
            ..bounds()
        };
        let mut c = Camera {
            time: TimeView {
                t0: 0.0,
                units_per_px: 0.1,
            },
            rows: RowView {
                y0: 0.0,
                px_per_unit: 1.0,
            },
        };
        let vp = Viewport::new(1000.0, 100.0);
        c.constrain(&b, vp);
        // Fit: ten rows in 100 px, exactly the range.
        assert!((c.rows.px_per_unit - 10.0).abs() < 1e-12);
        assert!((c.rows.y0 - 10.0).abs() < 1e-12);
    }
}
