//! `PitchDelay` — `TimeLine` MX "Ice" machine: slices the delay buffer and
//! plays the pieces back re-pitched.
//!
//! The delay tap feeds pitch-dsp's `SpectralShifter` — a phase-locked
//! phase vocoder (Laroche–Dolson peak shifting). It replaced a dual-grain
//! delay-line shifter whose grain crossfades put a grain-rate warble and
//! doubled pick attacks into every repeat, and — because the feedback
//! re-shifts each pass — compounded them up the octave ladder (measured on
//! a steady tone at +12: non-harmonic energy −38 dB → −83 dB, spectral
//! flux ×10 lower). `blend` mixes dry↔ice ON THE DELAY LINE, pre-feedback,
//! so regeneration re-shifts every pass — the classic octave ladder.
//!
//! Slice sets the analysis frame: Short 2048, Medium 4096, Long 8192
//! samples @ 48 kHz — short slices keep the pick articulate, long ones
//! blur each repeat into a smoother, more pad-like ladder. The frame never
//! exceeds the delay time (so its latency can be compensated).
//!
//! Latency: the shifter adds exactly its frame length; the tap is read that
//! many samples early, so every repeat lands at the delay time at any
//! interval.

use crate::tilt::DecayTilt;
use dsp_core::audio::dc_blocker::DcBlocker;
use dsp_core::audio::delay_line::DelayLine;
use dsp_core::audio::smoothing::ParamSmoother;
use dsp_core::num;
use pitch::spectral::SpectralShifter;

/// `TimeLine` MX Ice interval menu. `Free` uses `PitchDelay::speed` raw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IceInterval {
    /// Use the raw `speed` ratio field (non-MX escape hatch).
    Free,
    /// Half steps, −12 (octave down) ..= +12 (octave up).
    Semitones(i8),
    /// Micro-tunings: ±25 or ±50 cents.
    Cents(i16),
    /// +1 octave and a fifth (+19 semitones).
    OctaveAndFifth,
    /// +2 octaves (+24 semitones).
    TwoOctaves,
}

impl IceInterval {
    /// Pitch ratio for the interval; `None` for `Free`.
    #[must_use]
    pub fn ratio(self) -> Option<f64> {
        match self {
            Self::Free => None,
            Self::Semitones(s) => Some((f64::from(s) / 12.0).exp2()),
            Self::Cents(c) => Some((f64::from(c) / 1200.0).exp2()),
            Self::OctaveAndFifth => Some((19.0_f64 / 12.0).exp2()),
            Self::TwoOctaves => Some(4.0),
        }
    }

    /// The 30-entry MX menu order: −12..−1, −50c, −25c, +25c, +50c,
    /// +1..+11, +12, +19, +24. Out-of-range indices clamp to the ends.
    #[must_use]
    pub const fn from_index(i: usize) -> Self {
        match i {
            0..=11 => Self::Semitones(semitone_from_index(i, 12)),
            12 => Self::Cents(-50),
            13 => Self::Cents(-25),
            14 => Self::Cents(25),
            15 => Self::Cents(50),
            16..=26 => Self::Semitones(semitone_from_index(i, 15)),
            27 => Self::Semitones(12),
            28 => Self::OctaveAndFifth,
            _ => Self::TwoOctaves,
        }
    }

    pub const MENU_LEN: usize = 30;
}

/// Slice size — scales with the delay time (per the MX manual).
/// `i - offset` as a semitone count, for the index ranges `from_index` matches.
///
/// Written as `const` arithmetic on `i8` rather than `i8::try_from(i).expect(..)`:
/// `TryFrom` is not const, `expect` is not const, and this crate denies
/// `clippy::expect_used` anyway. The callers' match arms bound `i` to 0..=26,
/// so the truncation below cannot lose information — and `saturating_sub`
/// keeps it total regardless.
#[inline]
const fn semitone_from_index(i: usize, offset: i8) -> i8 {
    // `i` is at most 26 here, so the low byte is the whole value.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "i <= 26 (guaranteed by match arms in from_index), safe narrowing to i8"
    )]
    let low = (i & 0x7F) as i8;
    low.saturating_sub(offset)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IceSlice {
    /// ~1/4 of the delay time (max 200 ms) — small regenerating fragments.
    Short,
    /// ~1/2 of the delay time (max 400 ms).
    Medium,
    /// ~the delay time (max 800 ms) — whole re-pitched phrases.
    Long,
}

impl IceSlice {
    /// Shifter analysis frame (samples at 48 kHz).
    const fn frame_48k(self) -> usize {
        match self {
            Self::Short => 2048,
            Self::Medium => 4096,
            Self::Long => 8192,
        }
    }
}

// r[impl delay.pitch.shift]
// r[impl delay.pitch.granular-crossfade]
/// Ice-style pitch-shifted delay line.
pub struct PitchDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Playback speed ratio (used when `interval == Free`).
    pub speed: f64,
    /// Musical interval; non-`Free` overrides `speed` on `update()`.
    pub interval: IceInterval,
    /// Slice size; `None` uses `grain_ms` directly (pre-Ice behavior).
    pub slice: Option<IceSlice>,
    /// Dry↔ice balance on the delay line, pre-feedback (1.0 = all ice).
    pub blend: f64,
    /// Delay-line modulation LFO rate in Hz.
    pub mod_rate_hz: f64,
    /// Delay-line modulation depth (0.0–1.0; full scale ≈ ±3 ms).
    pub mod_depth: f64,
    /// Shifter frame in milliseconds when `slice` is `None` (rounded to a
    /// power-of-two frame, 1024–8192 samples @ 48 kHz).
    pub grain_ms: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_tilt_eq: DecayTilt,
    delay: DelayLine,
    shifter: SpectralShifter,
    /// Shifter frame in effect (samples at 48 kHz) and the rate it was set
    /// up for — reconfiguring clears the shifter, so only on real change.
    frame_48k: usize,
    frame_rate: f64,
    dc_blocker: DcBlocker,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    lfo_phase: f64,
}

impl PitchDelay {
    const MAX_DELAY_S: f64 = 5.0;

    #[must_use]
    pub fn new() -> Self {
        let buf_len = 48000 * 5 + 1024;
        let mut shifter = SpectralShifter::new();
        shifter.mix = 1.0;
        shifter.speed = 1.0;
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            speed: 1.0,
            interval: IceInterval::Free,
            slice: None,
            blend: 1.0,
            mod_rate_hz: 0.6,
            mod_depth: 0.0,
            grain_ms: 90.0,
            decay_tilt: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            delay: DelayLine::new(buf_len),
            shifter,
            frame_48k: 0,
            frame_rate: 0.0,
            dc_blocker: DcBlocker::new(),
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

        if let Some(ratio) = self.interval.ratio() {
            self.speed = ratio;
        }

        // Frame: slice-derived or free, capped at the delay time so the
        // early tap that compensates the shifter latency stays in range.
        let want = self.slice.map_or_else(
            || num::f64_to_index(self.grain_ms * 48.0),
            IceSlice::frame_48k,
        );
        let fit = num::f64_to_index(self.time_ms * 48.0);
        let mut frame = 1024usize;
        while frame < 8192 && frame.saturating_mul(2) <= want.min(fit).max(1024) {
            frame = frame.saturating_mul(2);
        }
        // Reconfiguring clears the shifter — only when the frame or the
        // rate actually changed (update runs at control rate).
        if frame != self.frame_48k || (sample_rate - self.frame_rate).abs() > 1e-9 {
            self.frame_48k = frame;
            self.frame_rate = sample_rate;
            self.shifter.fft_size = frame;
            self.shifter.update(sample_rate);
        }

        // Decay EQ: tilt filter in feedback path
        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.dc_blocker.set_cutoff(10.0, sample_rate);
        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    // r[impl delay.pitch.tick]
    /// Process one sample. Returns the (blended) delay-line output.
    pub fn tick(&mut self, input: f64) -> f64 {
        // Smooth delay time
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let mut smooth_delay = self.smoother.tick();
        if self.mod_depth > 0.0 {
            self.lfo_phase += self.mod_rate_hz / self.sample_rate;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            smooth_delay += (self.lfo_phase * core::f64::consts::TAU).sin()
                * self.mod_depth
                * 0.003
                * self.sample_rate;
        }

        let max_read = num::count_to_f64(self.delay.len()) - 4.0;

        // Dry path reads at the delay time. The ice path taps early by the
        // shifter's latency (its frame — constant, speed-independent), so
        // repeats land on time at every interval. (Delay times shorter than
        // the smallest frame floor at the write head: slightly late.)
        let dry_tap = self.delay.read_cubic(smooth_delay.clamp(1.0, max_read));
        let comp = num::count_to_f64(self.shifter.latency());
        let early = (smooth_delay - comp).clamp(1.0, max_read);
        let ice_tap = self.delay.read_cubic(early);

        self.shifter.speed = self.speed;
        let iced = self.shifter.tick(ice_tap);

        // Dry↔ice blend ON the line: feedback recirculates the blended
        // signal, so each pass re-shifts (octave ladder).
        let output = dry_tap * (1.0 - self.blend) + iced * self.blend;

        // Feedback with self-limiting
        let mut fb = output * self.feedback;
        fb = self.decay_tilt_eq.tick(fb, 0);
        let limited_fb = if fb.abs() > 0.001 {
            fb * fb.abs().mul_add(-2.0, 3.0).max(0.0) / 3.0
        } else {
            fb
        };
        // The nonlinear limiter (and any shifter residue) can build a
        // subsonic offset over many recirculations — block it inside the loop.
        let clamped_fb = self.dc_blocker.tick(limited_fb.clamp(-1.5, 1.5));

        self.delay.write(input + clamped_fb);
        self.feedback_sample = clamped_fb;

        output
    }

    #[must_use]
    pub const fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.decay_tilt_eq.reset();
        self.shifter.reset();
        self.dc_blocker.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.lfo_phase = 0.0;
    }
}

impl Default for PitchDelay {
    fn default() -> Self {
        Self::new()
    }
}
