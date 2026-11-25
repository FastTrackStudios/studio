//! Modulated allpass filter.
//!
//! Ported from `CloudSeedCore` ModulatedAllpass.h (MIT, Ghost Note Audio).
//! The LFO target updates every 8 samples (as in `CloudSeed`) but the
//! fractional delay ramps per-sample between updates and the buffer is
//! read with cubic interpolation — no more whole-sample stair-stepping.

use dsp_core::num;

use std::f64::consts::PI;

use dsp_core::audio::delay_line::DelayLine;

/// Default capacity: 100ms at 48kHz. Resized by `set_sample_rate`.
const DEFAULT_SAMPLE_RATE: f64 = 48000.0;
/// Modulation recalculation rate.
const MOD_UPDATE_RATE: u64 = 8;
/// Buffer length in seconds (100ms covers `CloudSeed` diffuser delays).
const BUFFER_SECONDS: f64 = 0.1;
/// The most samples [`ModulatedAllpass::process`] runs side by side.
pub const BLOCK: usize = 32;

pub struct ModulatedAllpass {
    buffer: DelayLine,
    samples_processed: u64,

    // Modulation state
    mod_phase: f64,
    /// Where the modulation starts, and starts again on a reset.
    start_phase: f64,
    current_delay: f64,
    delay_step: f64,

    // Parameters
    pub sample_delay: usize,
    pub feedback: f64,
    pub mod_amount: f64,
    pub mod_rate: f64,
    pub interpolation_enabled: bool,
    pub modulation_enabled: bool,
}

impl ModulatedAllpass {
    #[must_use]
    pub fn new() -> Self {
        let mut ap = Self {
            buffer: DelayLine::new(num::f64_to_index(DEFAULT_SAMPLE_RATE * BUFFER_SECONDS)),
            samples_processed: 0,
            mod_phase: 0.31, // Arbitrary initial phase
            start_phase: 0.31,
            current_delay: 100.0,
            delay_step: 0.0,
            sample_delay: 100,
            feedback: 0.5,
            mod_amount: 0.0,
            mod_rate: 0.0,
            interpolation_enabled: true,
            modulation_enabled: true,
        };
        ap.update_mod();
        ap
    }

    /// Create with a specific initial phase for stereo decorrelation.
    #[must_use]
    pub fn with_phase(phase: f64) -> Self {
        let mut ap = Self::new();
        ap.mod_phase = phase;
        ap.start_phase = phase;
        ap
    }

    /// Resize the buffer for the actual sample rate. Allocates; call from
    /// setup, never from the audio tick.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let len = num::f64_to_index(sample_rate * BUFFER_SECONDS).max(256);
        if len > self.buffer.len() {
            self.buffer = DelayLine::new(len);
        }
        self.samples_processed = 0;
        self.current_delay = num::count_to_f64(self.sample_delay);
        self.delay_step = 0.0;
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    #[inline(always)]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.modulation_enabled {
            self.tick_with_mod(input)
        } else {
            self.tick_no_mod(input)
        }
    }

    #[inline]
    fn tick_no_mod(&mut self, input: f64) -> f64 {
        let delay = self
            .sample_delay
            .min(self.buffer.len().saturating_sub(2))
            .max(1);
        let buf_out = self.buffer.read(delay);
        self.allpass_step(input, buf_out)
    }

    #[inline(always)]
    fn tick_with_mod(&mut self, input: f64) -> f64 {
        if self.samples_processed >= MOD_UPDATE_RATE {
            self.update_mod();
            self.samples_processed = 0;
        }

        self.current_delay += self.delay_step;
        let max_delay = num::count_to_f64(self.buffer.len().saturating_sub(4));
        let pos = self.current_delay.clamp(1.0, max_delay);

        let buf_out = if self.interpolation_enabled {
            self.buffer.read_cubic(pos)
        } else {
            self.buffer.read(num::f64_to_index(pos))
        };

        self.samples_processed = self.samples_processed.saturating_add(1);
        self.allpass_step(input, buf_out)
    }

    /// [`tick`](Self::tick) over every sample of `buf`, in place, with the
    /// same arithmetic in the same order for every sample.
    ///
    /// A block of up to [`BLOCK`] samples at a time. When the block's read
    /// position keeps one whole-sample part throughout (the modulation
    /// moves it a few samples over seconds, so nearly always), reaches
    /// back further than the block is long, and its taps do not straddle
    /// the end of the ring, the four taps of every sample are four
    /// contiguous runs of the ring that the block does not write: the
    /// interpolation and the allpass then run for the whole block side by
    /// side in vector registers, and the writes follow. Any other block
    /// runs sample by sample.
    #[inline]
    pub fn process(&mut self, buf: &mut [f64]) {
        for chunk in buf.chunks_mut(BLOCK) {
            if !(self.modulation_enabled && self.interpolation_enabled && self.process_wide(chunk))
            {
                for x in chunk.iter_mut() {
                    *x = self.tick(*x);
                }
            }
        }
    }

    /// The block path of [`process`](Self::process); `false`, with nothing
    /// changed, when the block is not one it takes.
    #[inline]
    fn process_wide(&mut self, buf: &mut [f64]) -> bool {
        let n = buf.len().min(BLOCK);
        let Some(buf) = buf.get_mut(..n) else {
            return false;
        };
        // Where each sample reads: the modulation advanced exactly as
        // `tick_with_mod` advances it, sample by sample.
        let saved = (
            self.mod_phase,
            self.current_delay,
            self.delay_step,
            self.samples_processed,
        );
        let max_delay = num::count_to_f64(self.buffer.len().saturating_sub(4));
        let mut pos = [0.0f64; BLOCK];
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in pos.iter_mut().take(n) {
            if self.samples_processed >= MOD_UPDATE_RATE {
                self.update_mod();
                self.samples_processed = 0;
            }
            self.current_delay += self.delay_step;
            *p = self.current_delay.clamp(1.0, max_delay);
            lo = lo.min(*p);
            hi = hi.max(*p);
            self.samples_processed = self.samples_processed.saturating_add(1);
        }
        // `read_cubic`'s whole part, the same for every sample of the block.
        let whole = num::f64_to_index(lo);
        let (data, write_pos) = self.buffer.raw();
        let len = data.len();
        // Sample k's newest tap (`read(whole - 1)`) is ring index
        // `newest + k`; its oldest (`read(whole + 2)`) `newest + k - 3`.
        let newest = if write_pos.saturating_add(1) >= whole.saturating_add(3) {
            Some(write_pos.saturating_add(1).saturating_sub(whole))
        } else {
            write_pos
                .saturating_add(1)
                .saturating_add(len)
                .checked_sub(whole)
        };
        let runs = newest.filter(|&i| {
            num::f64_to_index(hi) == whole
                // Older than anything the block writes.
                && whole >= BLOCK.saturating_add(2)
                && i >= 3
                && i.saturating_add(n) <= len
        });
        let taps = runs.and_then(|i| {
            Some((
                data.get(i..i.saturating_add(n))?,
                data.get(i.saturating_sub(1)..i.saturating_add(n).saturating_sub(1))?,
                data.get(i.saturating_sub(2)..i.saturating_add(n).saturating_sub(2))?,
                data.get(i.saturating_sub(3)..i.saturating_add(n).saturating_sub(3))?,
            ))
        });
        let Some((t0, t1, t2, t3)) = taps else {
            (
                self.mod_phase,
                self.current_delay,
                self.delay_step,
                self.samples_processed,
            ) = saved;
            return false;
        };

        // Catmull-Rom and the allpass, as `read_cubic` and `allpass_step`.
        let wf = num::count_to_f64(whole);
        let fb = self.feedback;
        let mut written = [0.0f64; BLOCK];
        for (((((x, wr), &p), &y0), &y1), (&y2, &y3)) in buf
            .iter_mut()
            .zip(written.iter_mut())
            .zip(pos.iter())
            .zip(t0.iter())
            .zip(t1.iter())
            .zip(t2.iter().zip(t3.iter()))
        {
            let f = p - wf;
            let a0 = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
            let a1 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let a2 = -0.5 * y0 + 0.5 * y2;
            let a3 = y1;
            let buf_out = ((a0 * f + a1) * f + a2) * f + a3;
            let in_val = buf_out.mul_add(fb, *x);
            *wr = in_val;
            *x = in_val.mul_add(-fb, buf_out);
        }
        if let Some(written) = written.get(..n) {
            self.buffer.write_block(written);
        }
        true
    }

    /// Shared Schroeder allpass update: write `input + fb`, output `delayed - fb * written`.
    #[inline(always)]
    fn allpass_step(&mut self, input: f64, buf_out: f64) -> f64 {
        let in_val = buf_out.mul_add(self.feedback, input);
        self.buffer.write(in_val);
        in_val.mul_add(-self.feedback, buf_out)
    }

    fn update_mod(&mut self) {
        self.mod_phase += self.mod_rate * num::u64_to_f64(MOD_UPDATE_RATE);
        if self.mod_phase > 1.0 {
            self.mod_phase %= 1.0;
        }

        let modulation = (self.mod_phase * 2.0 * PI).sin();

        // Prevent modulation from taking delay negative
        let effective_mod = self
            .mod_amount
            .min(num::count_to_f64(self.sample_delay) - 1.0);
        let target = (num::count_to_f64(self.sample_delay) + effective_mod * modulation).max(1.0);

        // Spread the move over the next update window.
        self.delay_step = (target - self.current_delay) / num::u64_to_f64(MOD_UPDATE_RATE);
    }

    /// Convenience: set feedback coefficient.
    #[expect(
        clippy::missing_const_for_fn,
        reason = "const fn cannot have &mut self parameter"
    )]
    pub fn set_feedback(&mut self, g: f64) {
        self.feedback = g;
    }

    /// Convenience: set delay in samples.
    pub fn set_delay(&mut self, samples: f64) {
        self.sample_delay = num::f64_to_index(samples)
            .min(self.buffer.len().saturating_sub(2))
            .max(1);
    }

    /// Convenience: set delay in integer samples.
    pub fn set_delay_samples(&mut self, samples: usize) {
        self.sample_delay = samples.min(self.buffer.len().saturating_sub(2)).max(1);
    }

    /// Convenience: set modulation rate and depth.
    /// `rate_hz` is the modulation frequency, `depth` is in samples,
    /// `sample_rate` is used to normalize rate.
    pub fn set_modulation(&mut self, rate_hz: f64, depth: f64, sample_rate: f64) {
        self.mod_rate = rate_hz / sample_rate;
        self.mod_amount = depth;
        self.modulation_enabled = depth > 0.0;
    }

    /// Convenience: set modulation phase (0.0 to 1.0).
    #[expect(
        clippy::missing_const_for_fn,
        reason = "const fn cannot have &mut self parameter"
    )]
    pub fn set_phase(&mut self, phase: f64) {
        self.mod_phase = phase;
        self.start_phase = phase;
    }

    pub fn reset(&mut self) {
        self.clear();
        self.samples_processed = 0;
        self.mod_phase = self.start_phase;
        self.current_delay = num::count_to_f64(self.sample_delay);
        self.delay_step = 0.0;
    }
}

impl Default for ModulatedAllpass {
    fn default() -> Self {
        Self::new()
    }
}
