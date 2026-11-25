//! What part of the session a frame can see, in the content space the
//! arrangement is recorded in: what every pass that places something
//! against time reads (the lanes, the ruler, the cursors, the track
//! panel's live pass, the hit test).

/// What part of the session a frame can actually see.
///
/// Held in the same content space the scene is recorded in — seconds
/// across, row pitch down — so the visibility test is the inverse of the
/// replay transform and nothing here has to know about `Affine`.
#[derive(Clone, Copy)]
pub struct Viewport {
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// Pixels per second, zoom included.
    pub pps: f64,
    pub zoom_y: f64,
    pub width: f64,
    pub height: f64,
    /// Where the lanes start: the track panel's width, in whichever
    /// shape it is in. The ONE number every pass that places something
    /// against time reads — the lanes, the ruler, the cursors, the hit
    /// test — so a narrower panel cannot move some of them and not
    /// others.
    pub panel_w: f64,
}

impl Viewport {
    /// The span of the session visible across, in seconds, with a screen
    /// of bleed either side for the same reason.
    #[must_use]
    pub fn secs(self) -> (f64, f64) {
        let pps = self.pps.max(1e-9);
        let left = (self.scroll_x - self.panel_w) / pps;
        let right = (self.scroll_x + self.width) / pps;
        (left - 1.0, right + 1.0)
    }

    /// The seconds on screen, with only a few pixels to spare — for what
    /// is drawn per pixel (the arrangement's `fine`), where [`Self::secs`]'s second either
    /// side would be half a million pixels zoomed in to the samples.
    #[must_use]
    pub fn on_screen(self) -> (f64, f64) {
        let pps = self.pps.max(1e-9);
        let spare = 8.0 / pps;
        (
            (self.scroll_x - self.panel_w) / pps - spare,
            (self.scroll_x + self.width) / pps + spare,
        )
    }
}
