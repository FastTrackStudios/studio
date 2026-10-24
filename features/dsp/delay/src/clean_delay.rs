//! `CleanDelay` — the `TimeLine` MX "Digital" machine, four voices.
//!
//! One delay line, cubic interpolation, optional feedback EQ — and a
//! per-voice conversion codec in the write path so every regeneration
//! re-encodes (the hardware puts the converter inside the loop):
//!
//! - **24/96** — pristine, no coloration.
//! - **ADM** — early-'80s 1-bit adaptive delta modulation, modelled as
//!   a CVSD codec (run-of-4 coincidence rule, 5 ms syllabic filter,
//!   leaky integrator) at 8× per host sample: fast transients
//!   slope-overload then snap — the signature percussive repeat attack.
//! - **12-Bit** — NE570-style 2:1 compander around a linear 12-bit
//!   quantizer + noise floor (compress at the write head, expand at the
//!   read head — the expander's envelope-tracking noise breathing is
//!   the era sound) plus a gentle in-loop lowpass.
//! - **Classic** — the original `TimeLine` digital voice: rounder/fatter
//!   (soft one-pole rolloff + mild saturation), with the voice's
//!   morphing FILTER response (full bandwidth → analog → tape) driven
//!   by [`CleanDelay::filter_morph`].

use crate::modulation::CompanderEnv;
use crate::tilt::DecayTilt;
use dsp_core::audio::biquad::{Biquad, FilterType};
use dsp_core::audio::delay_line::DelayLine;
use dsp_core::audio::smoothing::ParamSmoother;
use dsp_core::num;

/// Digital machine voicing (`TimeLine` MX `Voice`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DigitalVoice {
    /// Modern full-bandwidth conversion; clean and pure.
    #[default]
    TwentyFour96,
    /// 1-bit adaptive delta modulation — percussive attack emphasis.
    Adm,
    /// 12-bit companded conversion — darker, warmer.
    TwelveBit,
    /// Original `TimeLine` digital — rounder/fatter, morphing filter.
    Classic,
}

impl DigitalVoice {
    pub const COUNT: usize = 4;

    #[must_use]
    pub const fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Adm,
            2 => Self::TwelveBit,
            3 => Self::Classic,
            _ => Self::TwentyFour96,
        }
    }

    #[must_use]
    pub const fn to_index(self) -> usize {
        match self {
            Self::TwentyFour96 => 0,
            Self::Adm => 1,
            Self::TwelveBit => 2,
            Self::Classic => 3,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TwentyFour96 => "24/96",
            Self::Adm => "ADM",
            Self::TwelveBit => "12-Bit",
            Self::Classic => "Classic",
        }
    }
}

/// Adaptive delta modulator — CVSD-calibrated (the early-'80s 1-bit
/// codec: MC3417/HC-55516 family). 1-bit encode/decode at `OVERSAMPLE`
/// iterations per host sample:
///
/// - **run-of-N coincidence rule**: the step only grows while the last
///   `HISTORY_BITS` output bits are all equal (slope overload);
///   otherwise the **syllabic filter** decays it toward the minimum
///   (τ ≈ 5 ms, the mil/Bluetooth spec value).
/// - **leaky ("principal") integrator**, τ ≈ 1 ms, reconstructs the
///   level.
///
/// Slew is program-dependent: fast attacks overload then snap — the
/// ADM "percussive" signature; quiet sustain rides the granular
/// min-step floor.
struct AdmCodec {
    level: f64,
    step: f64,
    history: u32,
    /// Per-clock-tick syllabic decay toward `MIN_STEP` / charge toward
    /// `MAX_STEP` (exp(-1/(τ_syllabic · `f_clock`))).
    syllabic: f64,
    /// Per-clock-tick principal-integrator leak.
    leak: f64,
}

impl AdmCodec {
    const OVERSAMPLE: usize = 8;
    const HISTORY_BITS: u32 = 4;
    const MIN_STEP: f64 = 2.0e-4;
    const MAX_STEP: f64 = 0.12;
    /// Syllabic time constant (spec: 5 ms ± 1) and principal
    /// integrator time constant (~1 ms).
    const SYLLABIC_S: f64 = 0.005;
    const PRINCIPAL_S: f64 = 0.001;

    fn new() -> Self {
        let mut c = Self {
            level: 0.0,
            step: Self::MIN_STEP,
            history: 0,
            syllabic: 0.999,
            leak: 0.997,
        };
        c.configure(48000.0);
        c
    }

    fn configure(&mut self, sample_rate: f64) {
        let clock = sample_rate * num::count_to_f64(Self::OVERSAMPLE);
        self.syllabic = (-1.0 / (Self::SYLLABIC_S * clock)).exp();
        self.leak = (-1.0 / (Self::PRINCIPAL_S * clock)).exp();
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let mask = (1u32 << Self::HISTORY_BITS) - 1;
        for _ in 0..Self::OVERSAMPLE {
            let bit = x > self.level;
            self.history = ((self.history << 1) | u32::from(bit)) & mask;
            let run = self.history == 0 || self.history == mask;
            self.step = if run {
                // Charge toward MAX at the syllabic rate.
                (self.step - Self::MAX_STEP).mul_add(self.syllabic, Self::MAX_STEP)
            } else {
                // Decay toward MIN at the syllabic rate.
                (self.step - Self::MIN_STEP).mul_add(self.syllabic, Self::MIN_STEP)
            };
            self.level = self
                .level
                .mul_add(self.leak, if bit { self.step } else { -self.step });
        }
        self.level
    }

    const fn reset(&mut self) {
        self.level = 0.0;
        self.step = Self::MIN_STEP;
        self.history = 0;
    }
}

/// Linear 12-bit quantizer (the era's DAC was linear — the dynamics
/// came from the external NE570 compander around it).
#[inline]
fn quantize_12bit(x: f64) -> f64 {
    (x.clamp(-1.0, 1.0) * 2048.0).round() / 2048.0
}

/// Clean digital delay — the Digital machine's four voices.
pub struct CleanDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,
    /// Conversion voicing.
    pub voice: DigitalVoice,
    /// Classic voice's morphing FILTER position (0.0 = full bandwidth,
    /// 0.5 ≈ analog-delay response, 1.0 ≈ tape-delay response). Ignored
    /// by the other voices (they use `hicut_freq`).
    pub filter_morph: f64,
    /// Delay-line modulation LFO rate in Hz ("classic rack-style
    /// modulated delay").
    pub mod_rate_hz: f64,
    /// Delay-line modulation depth (0.0–1.0; full scale ≈ ±3 ms).
    pub mod_depth: f64,

    decay_tilt_eq: DecayTilt,
    delay: DelayLine,
    hicut: Biquad,
    locut: Biquad,
    /// Classic-morph filter stages (lowpass + low-cut contour).
    morph_lp: Biquad,
    morph_hp: Biquad,
    /// 12-Bit voice's in-loop darkening.
    twelve_lp: Biquad,
    /// 12-Bit voice: NE570-style 2:1 compander (compress at the write
    /// head, expand at the read head — envelope-modulated noise floor).
    comp_env: CompanderEnv,
    exp_env: CompanderEnv,
    noise: dsp_core::audio::prng::XorShift32,
    adm: AdmCodec,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    lfo_phase: f64,
}

impl Default for CleanDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl CleanDelay {
    const MAX_DELAY_S: f64 = 5.0;
    /// Full-depth modulation excursion in seconds (≈ ±3 ms).
    const MOD_RANGE_S: f64 = 0.003;

    #[must_use]
    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            hicut_freq: 0.0,
            locut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            voice: DigitalVoice::TwentyFour96,
            filter_morph: 0.0,
            mod_rate_hz: 0.6,
            mod_depth: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            morph_lp: Biquad::new(),
            morph_hp: Biquad::new(),
            twelve_lp: Biquad::new(),
            comp_env: CompanderEnv::new(),
            exp_env: CompanderEnv::new(),
            noise: dsp_core::audio::prng::XorShift32::new(0x0D16_17A1),
            adm: AdmCodec::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            lfo_phase: 0.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = num::f64_to_index(sample_rate * Self::MAX_DELAY_S).saturating_add(1024);
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        if self.hicut_freq > 0.0 {
            self.hicut.set(
                FilterType::Lowpass,
                self.hicut_freq,
                self.filter_q,
                sample_rate,
            );
        }
        if self.locut_freq > 0.0 {
            self.locut.set(
                FilterType::Highpass,
                self.locut_freq,
                self.filter_q,
                sample_rate,
            );
        }

        // Classic morph: full bandwidth (20 kHz) → analog bucket-brigade
        // rolloff (~2.8 kHz at noon) → tape response (steeper, darker,
        // plus a low contour that thins the deep lows).
        let morph = self.filter_morph.clamp(0.0, 1.0);
        let lp_freq = 20000.0 * (2800.0f64 / 20000.0).powf(morph);
        self.morph_lp
            .set(FilterType::Lowpass, lp_freq.max(900.0), 0.707, sample_rate);
        let hp_freq = 20.0 + morph * 100.0;
        self.morph_hp
            .set(FilterType::Highpass, hp_freq, 0.707, sample_rate);

        self.twelve_lp
            .set(FilterType::Lowpass, 9500.0, 0.707, sample_rate);
        self.adm.configure(sample_rate);
        self.comp_env.configure(sample_rate);
        self.exp_env.configure(sample_rate);

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    /// Per-voice conversion applied to everything entering the delay
    /// line (input + regeneration), so the character compounds with
    /// each repeat exactly like a codec inside the hardware loop.
    #[inline]
    fn encode(&mut self, x: f64, ch: usize) -> f64 {
        match self.voice {
            DigitalVoice::TwentyFour96 => x,
            DigitalVoice::Adm => self.adm.process(x),
            DigitalVoice::TwelveBit => {
                // Compress (feedback 2:1) → 12-bit linear quantize with
                // a ~1.5-LSB noise floor → store. The expander at the
                // read head turns that fixed floor into the era's
                // signal-tracking breathing.
                let compressed = self.comp_env.compress(x);
                let noisy = self.noise.next_bipolar().mul_add(1.5 / 2048.0, compressed);
                self.twelve_lp.tick(quantize_12bit(noisy), ch)
            }
            DigitalVoice::Classic => {
                // Rounder/fatter: gentle saturation + the morph filter.
                let softened = (x * 1.1).tanh() / 1.1f64.tanh();
                let lp = self.morph_lp.tick(softened, ch);
                if self.filter_morph > 0.55 {
                    // Tape territory: engage the low contour.
                    self.morph_hp.tick(lp, ch)
                } else {
                    lp
                }
            }
        }
    }

    /// Read-head decode: the 12-Bit voice expands (2:1) what the write
    /// head compressed; other voices pass through.
    #[inline]
    fn decode(&mut self, x: f64) -> f64 {
        match self.voice {
            DigitalVoice::TwelveBit => self.exp_env.expand(x),
            _ => x,
        }
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        // Rack-style delay-line modulation.
        let mod_off = if self.mod_depth > 0.0 {
            self.lfo_phase += self.mod_rate_hz / self.sample_rate;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            (self.lfo_phase * core::f64::consts::TAU).sin()
                * self.mod_depth
                * Self::MOD_RANGE_S
                * self.sample_rate
        } else {
            0.0
        };

        let max_read = num::count_to_f64(self.delay.len()) - 4.0;
        let read_pos = (smooth_delay + mod_off).clamp(1.0, max_read);
        let output = self.decode(self.delay.read_cubic(read_pos));

        // Feedback path: output → filter → limit
        let mut fb = output * self.feedback;

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }

        fb = self.decay_tilt_eq.tick(fb, ch);

        fb = fb.clamp(-1.5, 1.5);

        let encoded = self.encode(input + fb, ch);
        self.delay.write(encoded);
        self.feedback_sample = fb;

        output
    }

    #[must_use]
    pub const fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.locut.reset();
        self.morph_lp.reset();
        self.morph_hp.reset();
        self.twelve_lp.reset();
        self.comp_env.reset();
        self.exp_env.reset();
        self.adm.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.lfo_phase = 0.0;
    }
}
