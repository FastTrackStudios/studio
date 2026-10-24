//! An optical gain element fitted against the LA-2A Gray release captures.
//!
//! The reference stimulus falls from -6 to -20 dBFS; it is not a release into
//! silence. Captures contain applied gain including makeup, not gain reduction.
//! The extracted fixture and fitting script retain these distinctions.
//!
//! Release follows a stretched exponential. Its shape depends on the reduction
//! at release onset: a long tail under light loading, and an increasing recovery
//! rate under heavy loading. The remaining reduction determines the effective
//! position along that curve, so moving targets stay continuous. This is an
//! empirical gain-element model, not a simulation of the T4's physical circuit.

/// One measured release operating point. Shape 1 is an ordinary exponential;
/// values below 1 have a long tail; values above 1 accelerate recovery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReleasePoint {
    pub depth_db: f64,
    pub time_ms: f64,
    pub shape: f64,
}

// Fitted to the normalized second-cycle curves at 1 kHz. The depth subtracts
// applied gain from the measured idle makeup, rather than negating output gain.
const GRAY_RELEASE: &[ReleasePoint] = &[
    ReleasePoint {
        depth_db: 8.258_824,
        time_ms: 180.690_587,
        shape: 0.554_508,
    },
    ReleasePoint {
        depth_db: 16.094_118,
        time_ms: 61.080_788,
        shape: 0.979_537,
    },
    ReleasePoint {
        depth_db: 25.623_529,
        time_ms: 33.680_980,
        shape: 1.391_615,
    },
    ReleasePoint {
        depth_db: 32.823_529,
        time_ms: 26.779_560,
        shape: 1.269_979,
    },
];

/// Immutable calibration shared by instances. Custom cells can supply a static
/// measured release curve without introducing callback allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptoParams {
    pub attack_light_ms: f64,
    pub attack_deep_ms: f64,
    pub deep_db: f64,
    pub release: &'static [ReleasePoint],
}
impl Default for OptoParams {
    fn default() -> Self {
        Self {
            attack_light_ms: 14.0,
            attack_deep_ms: 8.0,
            deep_db: 32.823_529,
            release: GRAY_RELEASE,
        }
    }
}

/// Stream history for one optical attenuator, in positive dB of reduction.
#[derive(Debug, Clone)]
pub struct OptoCell {
    gr_db: f64,
    release_from_db: f64,
    sample_rate: f64,
    params: OptoParams,
}
impl OptoCell {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self {
            gr_db: 0.0,
            release_from_db: 0.0,
            sample_rate: sample_rate.max(1.0),
            params: OptoParams::default(),
        }
    }
    /// # Errors
    /// Rejects empty, unordered or non-finite calibration and nonpositive time constants.
    pub fn with_params(sample_rate: f64, params: OptoParams) -> Result<Self, crate::comp::Error> {
        let mut cell = Self::new(sample_rate);
        cell.set_params(params)?;
        Ok(cell)
    }
    /// # Errors
    /// Invalid calibration is rejected before modifying the cell.
    pub fn set_params(&mut self, params: OptoParams) -> Result<(), crate::comp::Error> {
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if !positive(params.attack_light_ms)
            || !positive(params.attack_deep_ms)
            || !positive(params.deep_db)
            || params.release.is_empty()
            || !params
                .release
                .iter()
                .all(|p| positive(p.depth_db) && positive(p.time_ms) && positive(p.shape))
            || !params
                .release
                .windows(2)
                .all(|p| matches!(p,[a,b] if a.depth_db<b.depth_db))
        {
            return Err(crate::comp::Error::InvalidControls);
        }
        self.params = params;
        Ok(())
    }
    #[must_use]
    pub const fn params(&self) -> OptoParams {
        self.params
    }
    pub const fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
    }
    pub const fn reset(&mut self) {
        self.gr_db = 0.0;
        self.release_from_db = 0.0;
    }
    #[must_use]
    pub const fn gain_reduction_db(&self) -> f64 {
        self.gr_db
    }
    fn release_law(&self) -> (f64, f64) {
        let Some(first) = self.params.release.first() else {
            return (100.0, 1.0);
        };
        let depth = self.release_from_db;
        if depth <= first.depth_db {
            return (first.time_ms, first.shape);
        }
        for pair in self.params.release.windows(2) {
            let [a, b] = pair else {
                continue;
            };
            if depth <= b.depth_db {
                let mix = (depth - a.depth_db) / (b.depth_db - a.depth_db);
                return (
                    (b.time_ms - a.time_ms).mul_add(mix, a.time_ms),
                    (b.shape - a.shape).mul_add(mix, a.shape),
                );
            }
        }
        self.params
            .release
            .last()
            .map_or((100.0, 1.0), |p| (p.time_ms, p.shape))
    }
    /// Advance toward a nonnegative target reduction. The curve is continuous
    /// when a recovering signal starts compressing again or its target moves.
    pub fn process(&mut self, target_gr_db: f64) -> f64 {
        let target = target_gr_db.max(0.0);
        if target > self.gr_db {
            let depth = (self.gr_db / self.params.deep_db).clamp(0.0, 1.0);
            let ms = (self.params.attack_deep_ms - self.params.attack_light_ms)
                .mul_add(depth, self.params.attack_light_ms);
            let a = (-1000.0 / (ms * self.sample_rate)).exp();
            self.gr_db = (self.gr_db - target).mul_add(a, target);
            self.release_from_db = self.gr_db;
        } else if self.gr_db - target > 1e-12 {
            let (ms, shape) = self.release_law();
            let span = (self.release_from_db - target).max(self.gr_db - target);
            let remaining = ((self.gr_db - target) / span).clamp(1e-15, 1.0);
            let elapsed = (-remaining.ln()).powf(shape.recip());
            let next = (-(elapsed + 1000.0 / (ms * self.sample_rate)).powf(shape)).exp();
            self.gr_db = span.mul_add(next, target);
        } else {
            self.gr_db = target;
        }
        self.gr_db
    }
}
