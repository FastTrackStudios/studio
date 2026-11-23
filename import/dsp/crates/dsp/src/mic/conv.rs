//! Zero-latency FIR convolution: a direct-form head plus non-uniformly
//! partitioned FFT convolution for the rest (Gardner, "Efficient
//! Convolution Without Input/Output Delay", JAES 1995).
//!
//! Taps `0..BLOCK` run as a plain FIR, sample by sample, so the output has
//! no added delay. The rest is cut into levels of growing partition size —
//! [`LEVELS`]: 64-sample partitions from tap 64, 256 from 256, 1024 from
//! 1024 and 4096 from 4096 to the end. A level of block `B` starts at tap
//! `B`, so its partitions only ever reach input at least one block back,
//! which is complete by the time their output is due: each level runs as
//! overlap-save products against a frequency-domain delay line of its own
//! past input blocks, once per `B` samples.
//!
//! The kernels are long (a mic model's are 32 704 taps): uniformly cut in
//! 64-sample partitions that was ~510 spectral products every 64 samples
//! per kernel. Growing the partitions keeps the zero-latency head and costs
//! a few products per level.
//!
//! Everything is `f64`: the point of this crate is to null against a
//! reference, and the accumulated error of single-precision FFTs over a
//! 16k-tap kernel would sit well above the target floor.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Length of the direct-form head, and the smallest partition.
pub const BLOCK: usize = 64;

/// Partition length of each level. Each level starts at the tap equal to
/// its block and runs to the next level's start (the last one to the end).
const LEVELS: [usize; 4] = [BLOCK, 4 * BLOCK, 16 * BLOCK, 64 * BLOCK];

const ZERO: Complex<f64> = Complex::new(0.0, 0.0);

/// One FIR filter, run with no latency.
pub struct Convolver {
    /// Taps `0..BLOCK`, applied directly.
    head: [f64; BLOCK],
    /// The last `BLOCK` inputs twice over, so the newest `BLOCK` are always
    /// the contiguous `history[at..at + BLOCK]`, newest first.
    history: [f64; 2 * BLOCK],
    at: usize,
    /// The partitioned levels, smallest block first.
    levels: Vec<Level>,
}

impl Convolver {
    /// A convolver for kernels up to `max_taps` long, initially silent.
    #[must_use]
    pub fn new(max_taps: usize) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let levels = level_spans(max_taps)
            .map(|(block, parts)| Level::new(block, parts, &mut planner))
            .collect();
        Self {
            head: [0.0; BLOCK],
            history: [0.0; 2 * BLOCK],
            at: 0,
            levels,
        }
    }

    /// Replace the kernel (truncated to the capacity given at
    /// construction). The input history is kept, so the output moves to
    /// the new kernel without a gap; no allocation.
    pub fn set_kernel(&mut self, kernel: &[f64]) {
        self.head.fill(0.0);
        self.head.iter_mut().zip(kernel).for_each(|(h, &k)| *h = k);
        for level in &mut self.levels {
            level.set_kernel(kernel.get(level.block..).unwrap_or_default());
        }
    }

    /// Clear all signal state (the kernel is kept).
    pub fn reset(&mut self) {
        self.history.fill(0.0);
        self.at = 0;
        self.levels.iter_mut().for_each(Level::reset);
    }

    /// One sample in, one sample out.
    pub fn process(&mut self, x: f64) -> f64 {
        let direct = self.direct(x);
        self.levels
            .iter_mut()
            .fold(direct, |acc, level| acc + level.process(x))
    }

    /// The head's FIR over the newest `BLOCK` inputs, `x` included.
    fn direct(&mut self, x: f64) -> f64 {
        self.at = self.at.checked_sub(1).unwrap_or(BLOCK - 1);
        if let Some(s) = self.history.get_mut(self.at) {
            *s = x;
        }
        if let Some(s) = self.history.get_mut(self.at.saturating_add(BLOCK)) {
            *s = x;
        }
        let recent = self
            .history
            .get(self.at..self.at.saturating_add(BLOCK))
            .unwrap_or(&[]);
        dot(&self.head, recent)
    }
}

/// Each level's block and its partition count, for kernels of `max_taps`.
fn level_spans(max_taps: usize) -> impl Iterator<Item = (usize, usize)> {
    LEVELS.iter().enumerate().filter_map(move |(i, &block)| {
        let end = match LEVELS.get(i.saturating_add(1)) {
            Some(&next) => next.min(max_taps),
            None => max_taps,
        };
        let parts = end.saturating_sub(block).div_ceil(block);
        (parts > 0).then_some((block, parts))
    })
}

/// `Σ a·b`, in four running sums so it vectorises.
fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut acc = [0.0f64; 4];
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for ((s, &p), &q) in acc.iter_mut().zip(x).zip(y) {
            *s = p.mul_add(q, *s);
        }
    }
    let [s0, s1, s2, s3] = acc;
    (s0 + s1) + (s2 + s3)
}

/// Uniformly partitioned overlap-save convolution of one span of the
/// kernel: partitions `1..=parts` of block `block`, i.e. taps
/// `block..block·(parts + 1)`.
struct Level {
    block: usize,
    /// Spectra of the partitions, `bins` each, in order.
    parts: Vec<Complex<f64>>,
    /// Spectra of past input block pairs, `bins` each, as a ring.
    fdl: Vec<Complex<f64>>,
    /// Slot in `fdl` holding the newest block pair.
    newest: usize,
    /// Partitions in use for the current kernel (≤ the capacity).
    used: usize,
    /// The previous and the current input block, back to back.
    input: Vec<f64>,
    /// Position within the current block.
    pos: usize,
    /// Output of this level for the current block.
    tail: Vec<f64>,
    fwd: Arc<dyn RealToComplex<f64>>,
    inv: Arc<dyn ComplexToReal<f64>>,
    time: Vec<f64>,
    spec: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
}

impl Level {
    fn new(block: usize, parts: usize, planner: &mut RealFftPlanner<f64>) -> Self {
        let fft_len = block.saturating_mul(2);
        let bins = block.saturating_add(1);
        let fwd = planner.plan_fft_forward(fft_len);
        let inv = planner.plan_fft_inverse(fft_len);
        let scratch_len = fwd.get_scratch_len().max(inv.get_scratch_len());
        Self {
            block,
            parts: vec![ZERO; parts.saturating_mul(bins)],
            fdl: vec![ZERO; parts.saturating_mul(bins)],
            newest: 0,
            used: 0,
            input: vec![0.0; fft_len],
            pos: 0,
            tail: vec![0.0; block],
            fwd,
            inv,
            time: vec![0.0; fft_len],
            spec: vec![ZERO; bins],
            scratch: vec![ZERO; scratch_len],
        }
    }

    const fn bins(&self) -> usize {
        self.block.saturating_add(1)
    }

    /// Take this level's partitions from `taps` (the kernel from tap
    /// `block` on); what `taps` does not reach is silent.
    fn set_kernel(&mut self, taps: &[f64]) {
        let bins = self.bins();
        let mut chunks = taps.chunks(self.block);
        let mut used = 0usize;
        for slot in self.parts.chunks_exact_mut(bins) {
            let Some(part) = chunks.next() else {
                slot.fill(ZERO);
                continue;
            };
            self.time.fill(0.0);
            self.time.iter_mut().zip(part).for_each(|(t, &k)| *t = k);
            // A length mismatch is impossible here (buffers are sized for
            // this plan); a failed transform leaves the slot silent rather
            // than half-written.
            if self
                .fwd
                .process_with_scratch(&mut self.time, slot, &mut self.scratch)
                .is_err()
            {
                slot.fill(ZERO);
            }
            used = used.saturating_add(1);
        }
        self.used = used;
    }

    fn reset(&mut self) {
        self.fdl.fill(ZERO);
        self.input.fill(0.0);
        self.tail.fill(0.0);
        self.pos = 0;
    }

    /// One input sample in, this level's share of the output out.
    fn process(&mut self, x: f64) -> f64 {
        let out = self.tail.get(self.pos).copied().unwrap_or(0.0);
        if let Some(c) = self.input.get_mut(self.block.saturating_add(self.pos)) {
            *c = x;
        }
        self.pos = self.pos.saturating_add(1);
        if self.pos >= self.block {
            self.block_done();
            self.pos = 0;
        }
        out
    }

    /// A block of input is complete: fold it into the delay line and
    /// compute the next block's contribution from this level's partitions.
    fn block_done(&mut self) {
        let bins = self.bins();
        let capacity = self.fdl.len().checked_div(bins).unwrap_or(0);
        if capacity == 0 {
            return;
        }
        self.newest = self
            .newest
            .checked_sub(1)
            .unwrap_or(capacity.saturating_sub(1));
        self.time.copy_from_slice(&self.input);
        if let Some(slot) = self.fdl.chunks_exact_mut(bins).nth(self.newest)
            && self
                .fwd
                .process_with_scratch(&mut self.time, slot, &mut self.scratch)
                .is_err()
        {
            slot.fill(ZERO);
        }
        self.input.copy_within(self.block.., 0);

        // Newest block pair meets partition 1, the one before it
        // partition 2, …: walk the ring from `newest` forward (older).
        self.spec.fill(ZERO);
        let (before, from) = self.fdl.split_at(self.newest.saturating_mul(bins));
        let ring = from.chunks_exact(bins).chain(before.chunks_exact(bins));
        for (x, h) in ring.zip(self.parts.chunks_exact(bins)).take(self.used) {
            for ((a, xv), hv) in self.spec.iter_mut().zip(x).zip(h) {
                *a += xv * hv;
            }
        }
        if self
            .inv
            .process_with_scratch(&mut self.spec, &mut self.time, &mut self.scratch)
            .is_err()
        {
            self.tail.fill(0.0);
            return;
        }
        let norm = 1.0 / dsp_core::count_to_f64(self.block.saturating_mul(2));
        self.tail
            .iter_mut()
            .zip(self.time.iter().skip(self.block))
            .for_each(|(t, &v)| *t = v * norm);
    }
}
