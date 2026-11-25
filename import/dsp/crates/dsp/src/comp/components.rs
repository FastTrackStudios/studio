//! Composable mono compressor components. Levels are dBFS; reduction is positive dB.

/// A detector owns the history needed to turn audio into a level.
pub trait LevelDetector {
    fn level_db(&mut self, sample: f64) -> f64;
    /// [`Self::level_db`], or `None` when the level is surely under
    /// `quiet` (a linear amplitude): the detector's history moves the
    /// same either way, and a quiet level needs no `log10`.
    fn level_db_unless_under(&mut self, sample: f64, quiet: f64) -> Option<f64> {
        let _ = quiet;
        Some(self.level_db(sample))
    }
    fn reset(&mut self);
}

/// A memoryless relationship between input level and requested attenuation.
pub trait GainComputer {
    fn reduction_db(&self, level_db: f64) -> f64;
    /// A level (dB) at and under which the reduction is exactly zero, if
    /// the law has one: a threshold.
    fn quiet_db(&self) -> Option<f64> {
        None
    }
}

/// A gain element's response to requested attenuation, in positive dB.
pub trait Envelope {
    fn reduction_db(&mut self, target_db: f64) -> f64;
    fn reset(&mut self);
}

/// A coloration stage; each channel owns its own instance.
pub trait Coloration {
    fn process(&mut self, sample: f64) -> f64;
    fn reset(&mut self);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Transparent;
impl Coloration for Transparent {
    fn process(&mut self, sample: f64) -> f64 {
        sample
    }
    fn reset(&mut self) {}
}

/// Peak follower with instantaneous rise and exponential decay.
#[derive(Debug, Clone)]
pub struct PeakDetector {
    coefficient: f64,
    peak: f64,
}
impl PeakDetector {
    /// Caller supplies a finite positive rate and decay time.
    #[must_use]
    pub fn new(sample_rate: f64, decay_ms: f64) -> Self {
        Self {
            coefficient: (-1.0 / (decay_ms * 0.001 * sample_rate).max(1.0)).exp(),
            peak: 0.0,
        }
    }
}
impl PeakDetector {
    fn follow(&mut self, sample: f64) -> f64 {
        let rectified = sample.abs();
        self.peak = if rectified > self.peak {
            rectified
        } else {
            (self.peak - rectified).mul_add(self.coefficient, rectified)
        };
        self.peak.max(1e-9)
    }
}
impl LevelDetector for PeakDetector {
    fn level_db(&mut self, sample: f64) -> f64 {
        20.0 * self.follow(sample).log10()
    }
    fn level_db_unless_under(&mut self, sample: f64, quiet: f64) -> Option<f64> {
        let peak = self.follow(sample);
        (peak >= quiet).then(|| 20.0 * peak.log10())
    }
    fn reset(&mut self) {
        self.peak = 0.0;
    }
}

/// Hard-knee downward compression. Use a different gain computer for a measured law.
#[derive(Debug, Clone, Copy)]
pub struct HardKnee {
    pub threshold_db: f64,
    pub ratio: f64,
}
impl GainComputer for HardKnee {
    fn reduction_db(&self, level_db: f64) -> f64 {
        (level_db - self.threshold_db).max(0.0) * (1.0 - self.ratio.recip())
    }
    fn quiet_db(&self) -> Option<f64> {
        Some(self.threshold_db)
    }
}

#[derive(Debug, Clone)]
pub struct AttackRelease {
    attack: f64,
    release: f64,
    reduction: f64,
}
impl AttackRelease {
    #[must_use]
    pub fn new(sample_rate: f64, attack_ms: f64, release_ms: f64) -> Self {
        let coefficient = |ms: f64| {
            if ms == 0.0 {
                0.0
            } else {
                (-1000.0 / (ms * sample_rate)).exp()
            }
        };
        Self {
            attack: coefficient(attack_ms),
            release: coefficient(release_ms),
            reduction: 0.0,
        }
    }
}
impl AttackRelease {
    pub(crate) const fn apply_coefficients(&mut self, prepared: &Self) {
        self.attack = prepared.attack;
        self.release = prepared.release;
    }
}
impl Envelope for AttackRelease {
    fn reduction_db(&mut self, target_db: f64) -> f64 {
        let coefficient = if target_db > self.reduction {
            self.attack
        } else {
            self.release
        };
        self.reduction = coefficient.mul_add(self.reduction, (1.0 - coefficient) * target_db);
        self.reduction
    }
    fn reset(&mut self) {
        self.reduction = 0.0;
    }
}
impl Envelope for crate::comp::opto::OptoCell {
    fn reduction_db(&mut self, target_db: f64) -> f64 {
        self.process(target_db)
    }
    fn reset(&mut self) {
        Self::reset(self);
    }
}

/// Feed-forward compressor topology with independently replaceable components.
/// A model with feedback or a different stage order can implement its own topology.
#[derive(Debug, Clone)]
pub struct Compressor<D, G, E, C = Transparent> {
    pub detector: D,
    pub gain_computer: G,
    pub envelope: E,
    pub coloration: C,
    pub makeup_db: f64,
    reduction_db: f64,
    /// The gain computer's quiet level, and the linear amplitude surely
    /// under it (`NEG_INFINITY`: none), worked out when it moves.
    quiet: (f64, f64),
}

/// How far under a quiet level, relatively, an amplitude must be to skip
/// its `log10`: a billionth (9e-9 dB), a million times the error of a
/// `log10` and a `powf`, so a skipped level is one whose reduction would
/// have been exactly zero.
const QUIET_MARGIN: f64 = 1.0 - 1e-9;
impl<D: LevelDetector, G: GainComputer, E: Envelope, C: Coloration> Compressor<D, G, E, C> {
    pub const fn new(detector: D, gain_computer: G, envelope: E, coloration: C) -> Self {
        Self {
            detector,
            gain_computer,
            envelope,
            coloration,
            makeup_db: 0.0,
            reduction_db: 0.0,
            quiet: (f64::NAN, f64::NEG_INFINITY),
        }
    }
    pub fn process(&mut self, input: f64) -> f64 {
        self.process_with_sidechain(input, input)
    }
    pub fn process_with_sidechain(&mut self, input: f64, sidechain: f64) -> f64 {
        let quiet_db = self.gain_computer.quiet_db().unwrap_or(f64::NAN);
        if quiet_db.to_bits() != self.quiet.0.to_bits() {
            let linear = if quiet_db.is_nan() {
                f64::NEG_INFINITY
            } else {
                10.0f64.powf(quiet_db / 20.0) * QUIET_MARGIN
            };
            self.quiet = (quiet_db, linear);
        }
        // Under the threshold the law asks for nothing: what it would
        // have said, without the `log10`.
        let target = self
            .detector
            .level_db_unless_under(sidechain, self.quiet.1)
            .map_or(0.0, |level| self.gain_computer.reduction_db(level));
        self.reduction_db = self.envelope.reduction_db(target);
        self.coloration
            .process(input * db_gain(self.makeup_db - self.reduction_db))
    }
    pub const fn gain_reduction_db(&self) -> f64 {
        self.reduction_db
    }
    pub fn reset(&mut self) {
        self.detector.reset();
        self.envelope.reset();
        self.coloration.reset();
        self.reduction_db = 0.0;
    }
}

/// dB to linear gain: `10^(db/20)` as `exp(db · ln 10 / 20)`, twice as fast
/// as `powf` and within an ulp or two of it; 0 dB is exactly unity.
#[inline]
fn db_gain(db: f64) -> f64 {
    if db == 0.0 {
        1.0
    } else {
        (db * (core::f64::consts::LN_10 / 20.0)).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generic(threshold_db: f64) -> Compressor<PeakDetector, HardKnee, AttackRelease> {
        Compressor::new(
            PeakDetector::new(48_000.0, 2.0),
            HardKnee {
                threshold_db,
                ratio: 4.0,
            },
            AttackRelease::new(48_000.0, 10.0, 100.0),
            Transparent,
        )
    }

    /// The compressor as it was: every level in dB, the gain by `powf`.
    fn as_it_was(c: &mut Compressor<PeakDetector, HardKnee, AttackRelease>, x: f64) -> f64 {
        let target = c.gain_computer.reduction_db(c.detector.level_db(x));
        c.reduction_db = c.envelope.reduction_db(target);
        x * 10.0f64.powf((c.makeup_db - c.reduction_db) / 20.0)
    }

    /// Skipping the quiet levels' `log10` and taking the gain by `exp`
    /// changes nothing a listener or a meter could find: the reduction
    /// bit for bit, the audio within 1e-15 of it, over hits, decays and
    /// silence, a threshold move and make-up.
    #[test]
    fn the_quiet_shortcut_compresses_as_it_did() {
        let (mut ours, mut theirs) = (generic(-24.0), generic(-24.0));
        let mut seed = 0x2545_f491_u32;
        for i in 0..96_000_u32 {
            if i == 48_000 {
                for c in [&mut ours, &mut theirs] {
                    c.gain_computer.threshold_db = -12.0;
                    c.makeup_db = 3.0;
                }
            }
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0;
            let env = if i % 12_000 < 3_000 {
                (-f64::from(i % 12_000) / 600.0).exp()
            } else {
                0.0
            };
            let x = noise * env;
            let (a, b) = (ours.process(x), as_it_was(&mut theirs, x));
            assert_eq!(
                ours.reduction_db.to_bits(),
                theirs.reduction_db.to_bits(),
                "sample {i}"
            );
            assert!(
                (a - b).abs() <= 1e-15 * b.abs().max(1e-300),
                "sample {i}: {a} vs {b}"
            );
        }
    }
}
