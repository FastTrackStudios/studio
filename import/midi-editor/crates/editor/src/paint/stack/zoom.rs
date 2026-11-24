//! Horizontal zoom gestures for the stacked timeline. Lane heights stay intact.
use crate::Editor;

#[derive(Clone, Copy)]
pub struct TimeZoom {
    pub origin: f64,
    pub current: f64,
    pub marquee: bool,
    base_units: f64,
    anchor: f64,
}

impl TimeZoom {
    pub fn begin(editor: &Editor, x: f64, marquee: bool) -> Self {
        Self {
            origin: x,
            current: x,
            marquee,
            base_units: editor.camera.units_per_px,
            anchor: editor.camera.t_at(x),
        }
    }

    pub fn update(&mut self, editor: &mut Editor, x: f64, fine: bool) {
        self.current = x;
        if !self.marquee {
            let gain = if fine { 800.0 } else { 200.0 };
            editor.camera.units_per_px =
                (self.base_units / ((x - self.origin) / gain).exp()).max(1e-9);
            editor.camera.t0 = self.anchor - self.origin * editor.camera.units_per_px;
            editor.settle_camera();
        }
    }

    pub fn finish(self, editor: &mut Editor) {
        if self.marquee && (self.current - self.origin).abs() > 3.0 {
            let start =
                self.anchor + (self.current.min(self.origin) - self.origin) * self.base_units;
            let span = (self.current - self.origin).abs() * self.base_units;
            editor.camera.units_per_px = (span / editor.viewport.w.max(1.0)).max(1e-9);
            editor.camera.t0 = start;
            editor.settle_camera();
        }
    }
}
