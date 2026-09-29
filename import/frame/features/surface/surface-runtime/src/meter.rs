//! Meter ballistics: engines publish levels at ~30 Hz; surfaces draw at
//! 60+ fps. Peaks attack instantly, fall at a fixed dB/s, and a peak-hold
//! marker lingers before falling too — all computed from explicit time so
//! the meter looks the same at any frame rate.

use surface_proto::Curve;

/// dB → bar position.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterScale {
    pub floor_db: f64,
    pub ceiling_db: f64,
    /// Shape over the dB span (linear-in-dB by default).
    pub curve: Curve,
}

impl Default for MeterScale {
    fn default() -> Self {
        Self { floor_db: -60.0, ceiling_db: 6.0, curve: Curve::Linear }
    }
}

impl MeterScale {
    #[must_use]
    pub fn position(&self, db: f64) -> f64 {
        let span = self.ceiling_db - self.floor_db;
        if span <= 0.0 || db.is_nan() {
            return 0.0;
        }
        self.curve.apply((db - self.floor_db) / span)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ballistics {
    /// Fall rate of the bar.
    pub release_db_per_s: f64,
    /// How long the peak marker holds before falling.
    pub hold_ms: f64,
    level_db: f64,
    level_t: f64,
    peak_db: f64,
    peak_t: f64,
}

impl Default for Ballistics {
    fn default() -> Self {
        Self::new(24.0, 1500.0)
    }
}

pub const SILENCE_DB: f64 = -120.0;

impl Ballistics {
    #[must_use]
    pub fn new(release_db_per_s: f64, hold_ms: f64) -> Self {
        Self { release_db_per_s, hold_ms, level_db: SILENCE_DB, level_t: 0.0, peak_db: SILENCE_DB, peak_t: 0.0 }
    }

    fn fallen(&self, from_db: f64, since_ms: f64) -> f64 {
        (from_db - self.release_db_per_s * since_ms.max(0.0) / 1000.0).max(SILENCE_DB)
    }

    /// A new measurement at time `t` (ms).
    pub fn push(&mut self, db: f64, t: f64) {
        let current = self.fallen(self.level_db, t - self.level_t);
        // Instant attack, timed release.
        self.level_db = db.max(current);
        self.level_t = t;
        let held = self.peak_hold_db(t);
        if db >= held {
            self.peak_db = db;
            self.peak_t = t;
        }
    }

    /// The bar's level at time `t` (ms).
    #[must_use]
    pub fn level_db(&self, t: f64) -> f64 {
        self.fallen(self.level_db, t - self.level_t)
    }

    /// The peak marker's level at time `t` (ms).
    #[must_use]
    pub fn peak_hold_db(&self, t: f64) -> f64 {
        let since = t - self.peak_t;
        if since <= self.hold_ms { self.peak_db } else { self.fallen(self.peak_db, since - self.hold_ms) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_attack_timed_release() {
        let mut b = Ballistics::new(24.0, 1000.0);
        b.push(-6.0, 0.0);
        assert_eq!(b.level_db(0.0), -6.0);
        assert!((b.level_db(500.0) - -18.0).abs() < 1e-9, "24 dB/s for 0.5 s");
        // A quieter reading doesn't pull the bar down faster than release.
        b.push(-40.0, 500.0);
        assert!((b.level_db(500.0) - -18.0).abs() < 1e-9);
        b.push(0.0, 600.0);
        assert_eq!(b.level_db(600.0), 0.0);
    }

    #[test]
    fn peak_holds_then_falls() {
        let mut b = Ballistics::new(24.0, 1000.0);
        b.push(-3.0, 0.0);
        b.push(-30.0, 100.0);
        assert_eq!(b.peak_hold_db(900.0), -3.0);
        assert!((b.peak_hold_db(1500.0) - -15.0).abs() < 1e-9);
    }

    #[test]
    fn frame_rate_independent() {
        let mut a = Ballistics::default();
        let mut c = Ballistics::default();
        a.push(-10.0, 0.0);
        c.push(-10.0, 0.0);
        // Sampling often vs once gives the same answer.
        for i in 0..60 {
            let _ = a.level_db(f64::from(i) * 16.0);
        }
        assert_eq!(a.level_db(960.0), c.level_db(960.0));
    }

    #[test]
    fn scale() {
        let s = MeterScale::default();
        assert_eq!(s.position(-60.0), 0.0);
        assert_eq!(s.position(6.0), 1.0);
        assert!((s.position(-27.0) - 0.5).abs() < 1e-12);
        assert_eq!(s.position(-200.0), 0.0);
    }
}
