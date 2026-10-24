//! `BbdDelay` — Bucket Brigade Device emulation with a true variable clock.
//!
//! The defining dBucket behavior (`TimeLine` MX spec): the delay line is a
//! fixed number of analog "stages"; delay time changes alter the read/write
//! CLOCK RATE, not the buffer contents. Audio already in the buckets is
//! preserved and re-pitched during time changes — sweep the time away and
//! back and the same audio re-emerges, unlike a digital fractional read
//! which smears or crossfades.
//!
//! Signal flow per channel:
//! Input → AA filter → `[write @ clock rate → N stages → read @ clock rate]`
//!       → reconstruction filter → output; feedback: output → tone LP →
//!       (analog-voiced, peaking near max filtering) → back into the write.
//!
//! Aliasing at long delay times is authentic and intentional: the virtual
//! clock IS low (e.g. 4096 stages / 800 ms ≈ 5.1 kHz), and the write path
//! is deliberately not oversampled.

use crate::bbd_core::{BbdCore, StageShaper};
use crate::modulation::CompanderEnv;
use crate::tilt::DecayTilt;
use dsp_core::audio::biquad::{Biquad, FilterType};
use dsp_core::audio::denormal::flush;
use dsp_core::audio::prng::XorShift32;
use dsp_core::audio::smoothing::ParamSmoother;
use dsp_core::num;

/// dBucket voice (`TimeLine` MX).
///
/// `Mx` (Brig lineage) runs 8192 virtual stages — double the clock rate at
/// any delay time, so wider bandwidth and less aliasing. `Classic`
/// (`TimeLine` v1) runs 4096 stages (MN3005-style): darker, grittier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BbdVoice {
    #[default]
    Mx,
    Classic,
}

impl BbdVoice {
    const fn stages(self) -> f64 {
        match self {
            Self::Mx => 8192.0,
            Self::Classic => 4096.0,
        }
    }
}

/// Per-bucket charge degradation (the bucket-loss model), hooked into
/// the Holters–Parker core's write path as a [`StageShaper`].
struct BucketLoss {
    /// Severity for the current audio sample (set from the knob +
    /// clock ratio each tick).
    sev: f64,
    lp: f64,
    rng: XorShift32,
}

impl BucketLoss {
    fn new() -> Self {
        Self {
            sev: 0.0,
            lp: 0.0,
            rng: XorShift32::new(0xB0CC_E77E),
        }
    }

    const fn reset(&mut self) {
        self.lp = 0.0;
    }
}

impl StageShaper for BucketLoss {
    #[inline]
    fn shape(&mut self, v: f64) -> f64 {
        if self.sev <= 0.001 {
            return v;
        }
        // HF droop: blend toward a one-pole of the written charge.
        self.lp = flush((v - self.lp).mul_add(self.sev.mul_add(-0.6, 1.0), self.lp));
        let drooped = ((self.lp - v) * self.sev).mul_add(0.7, v);
        // Charge-transfer nonlinearity + noise floor.
        let nl = (self.sev * 0.35 * drooped * drooped).mul_add(-drooped, drooped);
        (self.rng.next_bipolar() * self.sev).mul_add(0.002, nl)
    }
}

/// BBD/Analog delay with variable-clock stage buffer.
pub struct BbdDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// LFO modulation depth (0.0–1.0). Modulates the CLOCK (true BBD
    /// chorus). Zero depth = dual-mono wet; the chain offsets the R
    /// channel's LFO phase for stereo spread when depth > 0.
    pub mod_depth: f64,
    /// LFO modulation rate in Hz.
    pub mod_rate: f64,
    /// Tone / low-pass cutoff frequency in Hz. Analog-voiced: as the
    /// cutoff drops toward maximum filtering the response morphs into a
    /// resonant peak before the rolloff (perceived-bandwidth trick).
    pub tone: f64,
    /// Clock jitter amount (0.0–1.0) — random clock-rate noise.
    pub clock_jitter: f64,
    /// Bucket loss (0.0–1.0): per-stage charge-transfer inaccuracy.
    /// HF droop + soft nonlinearity + noise floor, scaling with the knob
    /// AND with delay time (slower clock = more droop per stage).
    pub bucket_loss: f64,
    /// LFO phase offset (0.0–1.0) for stereo spread across two instances.
    pub lfo_phase_offset: f64,
    /// Voice: stage count (Mx = 8192, Classic = 4096).
    pub voice: BbdVoice,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_tilt_eq: DecayTilt,
    /// Holters–Parker combined BBD core: clocked bucket queue with the
    /// AA/reconstruction filter banks evaluated at exact clock
    /// instants. Replaces the old interpolated stage buffer + biquads.
    core: BbdCore,
    loss: BucketLoss,
    /// Previous feedback value — the loop closes with one sample of
    /// latency because the core consumes input and produces output in
    /// one interleaved pass.
    fb_prev: f64,
    // Feedback tone filter
    tone_filter: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    lfo_phase: f64,
    rng: XorShift32,
    /// Smoothed jitter so clock noise wobbles instead of stepping.
    jitter_state: f64,
    /// NE570-style 2:1 compander (Raffel–Smith DAFx-10): compress into
    /// the buckets, expand at the reconstruction output. The averagers'
    /// mismatch against the DELAYED envelope is the authentic
    /// bucket-loss "breathing" on repeats, compounding per pass.
    comp_env: CompanderEnv,
    exp_env: CompanderEnv,
    /// Level-independent cubic THD constants (paper fit: a=1/8, b=1/18
    /// for 4096 stages; scaled by the voice's stage count).
    thd_a: f64,
    thd_b: f64,
    /// DC blocker after the x² term of the THD polynomial.
    thd_dc: dsp_core::audio::dc_blocker::DcBlocker,
}

impl Default for BbdDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl BbdDelay {
    #[must_use]
    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            mod_depth: 0.3,
            mod_rate: 1.0,
            tone: 4000.0,
            clock_jitter: 0.3,
            bucket_loss: 0.0,
            lfo_phase_offset: 0.0,
            voice: BbdVoice::Mx,
            decay_tilt: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            core: BbdCore::new(),
            loss: BucketLoss::new(),
            fb_prev: 0.0,
            tone_filter: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            lfo_phase: 0.0,
            rng: XorShift32::new(0xDEAD_BEEF),
            jitter_state: 0.0,
            comp_env: CompanderEnv::new(),
            exp_env: CompanderEnv::new(),
            thd_a: 0.125,
            thd_b: 1.0 / 18.0,
            thd_dc: dsp_core::audio::dc_blocker::DcBlocker::new(),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.comp_env.configure(sample_rate);
        self.exp_env.configure(sample_rate);
        // THD scales with stage count (~1% per 1024 stages,
        // level-independent — the giveaway BBD flavor; NOT clipping).
        let stage_scale = self.voice.stages() / 4096.0;
        self.thd_a = 0.125 * stage_scale;
        self.thd_b = (1.0 / 18.0) * stage_scale;
        self.thd_dc.set_cutoff(10.0, sample_rate);
        self.sample_rate = sample_rate;

        // Holters–Parker core: stage count from the voice; the Juno
        // filter cutoffs scale with the nominal clock (2·stages/delay,
        // referenced to the ≈100 kHz clock the filters were designed
        // at), so long delays darken like the hardware and the Mx
        // voice's doubled clock keeps it brighter than Classic at the
        // same time setting.
        let n_stages = self.voice.stages();
        let delay_sec = (self.time_ms * 0.001).max(0.001);
        let clock_hz = 2.0 * n_stages / delay_sec;
        let cutoff_scale = (clock_hz / 100_000.0).clamp(0.22, 1.15);
        self.core
            .configure(sample_rate, num::f64_to_index(n_stages), cutoff_scale);

        // Analog-voiced tone: Q rises as the cutoff drops, so maximum
        // filtering has a resonant bump before the rolloff.
        let tone_freq = self.tone.clamp(200.0, sample_rate * 0.45);
        let norm = ((tone_freq - 200.0) / 11_800.0).clamp(0.0, 1.0);
        let q = (1.0 - norm).powi(2).mul_add(1.8, 0.707);
        self.tone_filter
            .set(FilterType::Lowpass, tone_freq, q, sample_rate);

        // Decay EQ: tilt filter in feedback path
        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        // Smooth delay-time (= clock-rate) changes; the sweep itself is
        // what produces the analog pitch-bend character.
        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick().max(16.0);

        // LFO + jitter modulate the CLOCK RATE (true BBD behavior: pitch
        // wobble on everything in the buckets, not a moving read tap).
        let lfo_inc = self.mod_rate / self.sample_rate;
        self.lfo_phase += lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo = ((self.lfo_phase + self.lfo_phase_offset) * std::f64::consts::TAU).sin();
        // Smoothed random clock noise.
        self.jitter_state += 0.002 * (self.rng.next_bipolar() - self.jitter_state);
        let jitter = self.jitter_state * self.clock_jitter * 0.01;

        let n_stages = self.voice.stages();
        let clock_factor = ((lfo * self.mod_depth).mul_add(0.04, 1.0) + jitter).clamp(0.25, 4.0);
        self.core.set_clock_samples(smooth_delay / clock_factor);

        // Bucket-loss severity: slower clock (fewer stage writes per
        // host sample) = more droop per stage.
        let ratio = (n_stages / smooth_delay).min(1.0);
        self.loss.sev = if self.bucket_loss <= 0.001 {
            0.0
        } else {
            self.bucket_loss * 0.65f64.mul_add(1.0 - ratio, 0.35)
        };

        // Loop front half: compander compress + the BBD's
        // level-independent cubic THD (f(x) = x − a·x² − b·x³,
        // DC-blocked) + charge noise (~−60 dB, inside the loop so
        // feedback > 1 self-oscillates from the floor). The Holters–
        // Parker core then handles AA filtering, the clocked buckets
        // (per-write bucket loss via the shaper), and reconstruction.
        let compressed = self.comp_env.compress(input + self.fb_prev);
        let x = compressed.clamp(-1.5, 1.5);
        let shaped = (self.thd_b * x * x).mul_add(-x, (self.thd_a * x).mul_add(-x, x));
        let write_in = self
            .thd_dc
            .tick(shaped)
            .mul_add(1.0, self.rng.next_bipolar() * 1.0e-3);

        let raw = self.core.process(write_in, &mut self.loss);
        let output = self.exp_env.expand(raw);

        // Feedback path: tone filter (analog-voiced) → tilt → limit.
        // Closes with one sample of loop latency (the core consumes
        // input and produces output in one interleaved pass).
        let mut fb = output * self.feedback;
        fb = self.tone_filter.tick(fb, ch);
        fb = self.decay_tilt_eq.tick(fb, ch);
        fb = fb.clamp(-1.5, 1.5);
        self.fb_prev = fb;
        self.feedback_sample = fb;
        output
    }

    #[must_use]
    pub const fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.loss.reset();
        self.fb_prev = 0.0;
        self.jitter_state = 0.0;
        self.comp_env.reset();
        self.exp_env.reset();
        self.thd_dc.reset();
        self.tone_filter.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.lfo_phase = 0.0;
    }
}
