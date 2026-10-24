//! `OilCanDelay` — murky electrostatic oil-can echo (Tel-Ray/Adineko style).
//!
//! Per spec/timeline-mx-reference.md: the defining behavior is the
//! NO-ERASE head. The signal is written as electrostatic charge on a
//! rotating oiled disc; old charge decays with a time constant while new
//! signal is written over it. Consequences modeled here:
//!
//! - A GHOST echo recurs at the disc-rotation period even with repeats
//!   at 0 — the residual charge passes the playback head every rotation.
//! - The rotation period is LONGER than the first (record→playback head
//!   distance) echo, so the cadence is off-kilter, never on the grid.
//! - Long/Short selects the first-echo head distance only; the
//!   rotation-period echo is identical for both.
//! - Grit = rotation-speed randomization (time-domain dirt/jitter),
//!   NOT amplitude saturation.
//!
//! Plus: very low bandwidth (murk LP), heavy dual-LFO wobble, light
//! constant saturation, allpass regen splatter.

use crate::tilt::DecayTilt;
use dsp_core::audio::biquad::{Biquad, FilterType};
use dsp_core::audio::delay_line::DelayLine;
use dsp_core::audio::prng::XorShift32;
use dsp_core::audio::smoothing::ParamSmoother;
use dsp_core::audio::soft_clip::sin_clip;
use dsp_core::num;

/// Which pickup heads are engaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OilCanHeads {
    /// Single head at the full delay time.
    Long,
    /// Single head at ~55% of the delay time.
    Short,
    /// Both heads — cascading repeats.
    Both,
}

pub struct OilCanDelay {
    /// Base delay time in ms (clamped to 200–800, `TimeLine` Oil Can range).
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Head mode.
    pub heads: OilCanHeads,
    /// Wobble depth (0.0–1.0). Oil cans wobble a lot; default is high.
    pub wobble: f64,
    /// Loop darkness: filter voicing in Hz (default 2500). The range is
    /// wider than the real unit at the open end (bonus bandwidth,
    /// 300–12000); below ~1200 Hz the response morphs from a plain
    /// lowpass into a resonant band-pass with the deep lows thinned —
    /// the "extremely dark murky bandpass" at the FILTER knob's max.
    pub tone_hz: f64,
    /// Rotation LFO base rate in Hz (`TimeLine` Mod Speed). The wobble is
    /// spring-loaded: it crawls through the first half of each cycle
    /// and accelerates through the second, like a disc fighting its
    /// drive spring before letting go.
    pub mod_rate: f64,
    /// Rotation-speed randomization (0.0–1.0): time-domain jitter on the
    /// read heads. This is `TimeLine`'s Grit for Oil Can — dirt via speed
    /// uncertainty, not saturation.
    pub grit: f64,
    /// Decay EQ tilt (shared engine param).
    pub decay_tilt: f64,

    delay: DelayLine,
    lp: Biquad,
    murk_hp: Biquad,
    murk_active: bool,
    decay_tilt_eq: DecayTilt,
    // Small fixed allpass for the regen "splatter".
    splatter: DelayLine,
    splatter_g: f64,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    wow_phase: f64,
    flutter_phase: f64,
    /// Grit random-walk state (rotation-speed uncertainty).
    grit_walk: f64,
    rng: XorShift32,
}

impl Default for OilCanDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl OilCanDelay {
    pub const MIN_TIME_MS: f64 = 200.0;
    pub const MAX_TIME_MS: f64 = 800.0;
    const MAX_DELAY_S: f64 = 1.6;
    const SHORT_RATIO: f64 = 0.55;
    /// Disc-rotation period relative to the (long-head) first echo.
    /// > 1.0: the ghost recurs later than the first echo (off-kilter).
    const ROTATION_RATIO: f64 = 1.45;
    /// Write-head charge-transfer efficiency: the write only PARTIALLY
    /// re-charges the disc each revolution (`disc[w] = α·in + (1−α)·disc[w]`,
    /// Tel-Ray no-erase physics), so 1−α of last revolution's charge
    /// survives every lap — the ghost-echo recurrence falls out of the
    /// write equation instead of being a bolted-on tap.
    const WRITE_ALPHA: f64 = 0.6;

    #[must_use]
    pub fn new() -> Self {
        Self {
            time_ms: 400.0,
            feedback: 0.45,
            heads: OilCanHeads::Long,
            wobble: 0.6,
            tone_hz: 2500.0,
            mod_rate: 0.9,
            grit: 0.1,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 2),
            lp: Biquad::new(),
            murk_hp: Biquad::new(),
            murk_active: false,
            decay_tilt_eq: DecayTilt::new(),
            splatter: DelayLine::new(512),
            splatter_g: 0.45,
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            wow_phase: 0.0,
            flutter_phase: 0.25,
            grit_walk: 0.0,
            rng: XorShift32::new(0x011C_A4BE),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);

        let max_len = num::f64_to_index(sample_rate * Self::MAX_DELAY_S).saturating_add(1024);
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }
        let splat_len = num::f64_to_index(sample_rate * 0.008).saturating_add(8); // ~8 ms
        if self.splatter.len() < splat_len {
            self.splatter = DelayLine::new(splat_len);
        }

        // Filter voicing: plain LP over most of the travel, morphing to
        // a resonant, low-thinned bandpass below ~1200 Hz.
        let tone = self.tone_hz.clamp(300.0, 12000.0);
        if tone >= 1200.0 {
            self.lp.set(FilterType::Lowpass, tone, 0.707, sample_rate);
            self.murk_active = false;
        } else {
            let murk = ((1200.0 - tone) / 900.0).clamp(0.0, 1.0);
            self.lp
                .set(FilterType::Lowpass, tone, 0.707 + murk * 2.3, sample_rate);
            self.murk_hp
                .set(FilterType::Highpass, 120.0 + murk * 280.0, 0.9, sample_rate);
            self.murk_active = true;
        }

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    #[inline]
    fn splatter_tick(&mut self, input: f64) -> f64 {
        // Schroeder allpass over a fixed ~6 ms delay.
        let d = (self.sample_rate * 0.006).min(num::count_to_f64(self.splatter.len()) - 2.0);
        let delayed = self.splatter.read_linear(d);
        let v = self.splatter_g.mul_add(-delayed, input);
        self.splatter.write(v);
        self.splatter_g.mul_add(v, delayed)
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        // Heavy dual-LFO wobble: slow wow (Mod Speed, up to ±1.2%) plus
        // faster flutter (7× Mod Speed, up to ±0.25%). The wow phase is
        // spring-loaded — it advances at half rate near the cycle start
        // and ~1.5× near the end (average 1×), the fights-the-spring-
        // then-accelerates character of real units.
        let spring = 0.5 + self.wow_phase;
        self.wow_phase += self.mod_rate * spring / self.sample_rate;
        if self.wow_phase >= 1.0 {
            self.wow_phase -= 1.0;
        }
        self.flutter_phase += self.mod_rate * 7.0 / self.sample_rate;
        if self.flutter_phase >= 1.0 {
            self.flutter_phase -= 1.0;
        }
        let wow = (std::f64::consts::TAU * self.wow_phase).sin() * 0.012;
        let flutter = (std::f64::consts::TAU * self.flutter_phase).sin() * 0.0025;

        // Grit: rotation-speed uncertainty as a bounded random walk —
        // fast time-domain jitter, the electrostatic 'dirt'.
        self.grit_walk += self.rng.next_bipolar() * 0.02;
        self.grit_walk = self.grit_walk.clamp(-1.0, 1.0);
        self.grit_walk *= 0.999;
        let grit_jitter = self.grit_walk * self.grit * 0.004;

        let factor = (wow + flutter).mul_add(self.wobble, 1.0) + grit_jitter;

        let max_read = num::count_to_f64(self.delay.len()) - 4.0;
        let long_pos = (smooth_delay * factor).clamp(1.0, max_read);
        let short_pos = (smooth_delay * Self::SHORT_RATIO * factor).clamp(1.0, max_read);
        // Disc rotation: identical for both head modes, longer than the
        // first echo.
        let rotation_pos = (smooth_delay * Self::ROTATION_RATIO * factor).clamp(1.0, max_read);

        // Pickup makeup: writes are α-scaled by the partial re-charge,
        // the playback amp brings the first echo back to unity.
        let makeup = 1.0 / Self::WRITE_ALPHA;
        let output = match self.heads {
            OilCanHeads::Long => self.delay.read_cubic(long_pos) * makeup,
            OilCanHeads::Short => self.delay.read_cubic(short_pos) * makeup,
            OilCanHeads::Both => {
                self.delay
                    .read_cubic(short_pos)
                    .mul_add(0.8, self.delay.read_cubic(long_pos))
                    * makeup
                    / 1.4
            }
        };

        // Regen: light constant saturation → splatter allpass → tilt.
        //
        // The disc has two loops, and they add: this regen, and the
        // residue the partial re-charge leaves (`1 − WRITE_ALPHA` of what
        // was written one rotation ago). With Repeats taken as the regen
        // gain alone, the two summed past unity from about 0.6 up and the
        // tank grew until it clipped. Repeats spans the regen the residue
        // leaves room for, so the top of the knob is the longest sustain
        // that still dies away.
        let mut fb = output * self.feedback * (Self::WRITE_ALPHA - 0.02);
        fb = sin_clip(fb * 1.2) / 1.2;
        fb = self.splatter_tick(fb);
        fb = self.decay_tilt_eq.tick(fb, ch);
        fb = fb.clamp(-1.5, 1.5);

        // No-erase write: the head only partially re-charges the disc,
        // so (1−α) of the charge deposited exactly one revolution ago
        // survives underneath — the ghost that recurs at the rotation
        // period (with repeats at 0) IS this residue re-passing the
        // pickup, and it interacts with feedback self-consistently.
        // The disc medium itself is band-limited: the murk filter sits
        // in the RECORD path, so the first echo is already dark and
        // every rotation compounds it (matches FILTER acting at
        // Repeats = 0 on the real unit).
        let residue = self.delay.read_cubic(rotation_pos);
        let mut record = self.lp.tick(input + fb, ch);
        if self.murk_active {
            record = self.murk_hp.tick(record, ch);
        }
        self.delay
            .write(Self::WRITE_ALPHA.mul_add(record, (1.0 - Self::WRITE_ALPHA) * residue));
        self.feedback_sample = fb;

        output
    }

    #[must_use]
    pub const fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.splatter.clear();
        self.lp.reset();
        self.murk_hp.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.wow_phase = 0.0;
        self.flutter_phase = 0.25;
        self.grit_walk = 0.0;
    }
}
