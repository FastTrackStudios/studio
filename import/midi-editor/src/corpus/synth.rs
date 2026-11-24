//! A deterministic snare, so the flam curve can be measured without a
//! 5.6 GB download.
//!
//! This is **not** a drum synthesizer and is not trying to be. It
//! exists to give [`crate::corpus::recall`] a signal with the two properties
//! the detector actually keys on: a broadband attack that produces a
//! large positive spectral flux, and a decay long enough that a second
//! strike arriving 5 to 60 ms later has to rise out of it. Everything
//! else about how a snare sounds is irrelevant here, and modelling it
//! would only add parameters nobody would tune.
//!
//! The number it produces is therefore an *optimistic* bound. A
//! synthetic strike is cleaner than a real one, and there is no bleed
//! from any other mic, so the real knee sits at least as late as the
//! synthetic one. That is the right direction for a regression test to
//! err in: it fails when the detector gets worse, and it never claims
//! the detector is better than a real kit will show. The real number
//! comes from rendering the same sweep through a real kit —
//! `fetch-corpus.sh render-sweep`.
//!
//! Determinism is a hard requirement, not a nicety: a test that
//! asserts on a recall curve cannot have the curve move because the
//! noise reseeded. So the noise generator is a fixed LCG seeded from
//! the strike's own sample position, which also means two strikes at
//! the same position in two different renders are bit-identical.

use core::f64::consts::TAU;

/// A struck snare, rendered additively into a buffer.
#[derive(Clone, Copy, Debug)]
pub struct Snare {
    pub sample_rate: f64,
    /// Fast decay of the noise, in seconds — the initial collapse.
    ///
    /// A snare does **not** decay as one exponential, and getting this
    /// wrong changes the answer rather than the flavour. A single
    /// 70 ms time constant leaves the hit only 4 dB down after 30 ms,
    /// so every grace note in the sweep lands on a wall of decay and
    /// the curve reports the detector as hopeless. Real snares drop
    /// hard and then ring: roughly 11 dB down at 30 ms, which is what
    /// this pair produces.
    pub noise_fast_tau: f64,
    /// Slow decay of the noise — the wires ringing on underneath.
    pub noise_slow_tau: f64,
    /// How much of the noise is in the slow part, 0..=1.
    pub noise_slow_mix: f64,
    /// Decay time constant of the shell tones, in seconds.
    pub body_tau: f64,
    /// Attack ramp, in seconds. Not zero: a discontinuity is broadband
    /// in a way no drum is, and it would flatter the detector.
    pub attack: f64,
    /// The two shell modes, in Hz.
    pub modes: [f64; 2],
}

impl Default for Snare {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            noise_fast_tau: 0.012,
            // Long enough that a 60 ms grace still lands on an audible
            // decay — which is the whole phenomenon under test.
            noise_slow_tau: 0.070,
            noise_slow_mix: 0.18,
            body_tau: 0.030,
            attack: 0.0008,
            modes: [185.0, 331.0],
        }
    }
}

impl Snare {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            ..Self::default()
        }
    }

    /// Add one strike at `at` samples, at `velocity` in 0..=1.
    ///
    /// Additive rather than overwriting, because a flam is precisely
    /// the case where the second strike lands on top of the first — a
    /// renderer that wrote instead of summed would erase the very
    /// masking the test is measuring.
    pub fn strike(&self, buf: &mut [f64], at: usize, velocity: f64) {
        if at >= buf.len() {
            return;
        }
        let sr = self.sample_rate.max(1.0);
        let attack = (self.attack * sr).max(1.0);
        // Seeded from the position so a render is reproducible and two
        // strikes never share a noise sequence (which would correlate
        // and cancel).
        let mut rng = Lcg::new(0x5EED_0000_0000_0001 ^ (at as u64).wrapping_mul(0x9E37_79B9));

        let tail = ((self.noise_slow_tau.max(self.body_tau)) * 8.0 * sr) as usize;
        let end = (at + tail).min(buf.len());
        for (n, out) in buf[at..end].iter_mut().enumerate() {
            let t = n as f64 / sr;
            let ramp = (n as f64 / attack).min(1.0);
            let decay = (1.0 - self.noise_slow_mix) * (-t / self.noise_fast_tau).exp()
                + self.noise_slow_mix * (-t / self.noise_slow_tau).exp();
            let noise = rng.next_bipolar() * decay;
            let body: f64 = self
                .modes
                .iter()
                .map(|f| (TAU * f * t).sin() * (-t / self.body_tau).exp())
                .sum::<f64>()
                / self.modes.len() as f64;
            // Weighted toward the noise: it is the broadband part, and
            // the broadband part is what spectral flux sees.
            *out += velocity * ramp * (0.72 * noise + 0.28 * body);
        }
    }
}

/// A 64-bit linear congruential generator.
///
/// Not a good PRNG and does not need to be — it needs to be the *same*
/// PRNG on every machine and every run, which `rand` with a default
/// seed would not be across versions.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        // Numerical Recipes' constants.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn next_bipolar(&mut self) -> f64 {
        // Top 53 bits, the ones with the best spectral properties in an
        // LCG, mapped to -1..1.
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}
