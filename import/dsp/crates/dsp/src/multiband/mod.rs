//! Multiband splitter core — make ANY effect multiband (`ShaperBox`
//! model: N frequency bands, an independent processor per band,
//! recombine to a flat sum).
//!
//! Topology: sequential Linkwitz-Riley 4th-order splits from the
//! lowest crossover up. After each split, every already-separated band
//! runs through the crossover's matching 2nd-order allpass (an LR4
//! LP+HP pair sums to exactly that allpass), so ALL bands arrive at
//! the recombine with matched phase and the sum is magnitude-flat.
//! Standard published crossover math (Linkwitz 1976, RBJ cookbook) —
//! no third-party code.
//!
//! Processing-core rules apply: no allocation in `process` (buffers
//! preallocated by `prepare`), no threads, no I/O.
//!
//! `Multiband` is the generic host: `process(l, r, |band, bl, br| …)`
//! hands each band's buffers to the caller's closure — wrap any inner
//! effect (a compressor per band, a pattern modulator per band à la
//! `ShaperBox`, a saturator per band…). One band = bit-exact
//! passthrough of the closure over the raw buffers (zero split cost).

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]
// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// 33 findings in this crate, held under `expect` rather than fixed one by one.
//
// These are the judgment lints — casts, indexing and integer arithmetic in
// per-sample math. The correct rewrite for each depends on whether the code
// runs on an audio callback, so editing them individually would be thousands
// of unreviewable changes to code with no characterization tests behind it.
// The plan is to restructure these algorithms into idiomatic Rust (typed
// sample indices, iterators over raw indexing, checked conversions at the
// boundary) against a golden-master harness that proves the output is
// unchanged — which removes whole classes of these at once instead of
// suppressing them.
//
// This is `allow`, not `expect`, and that is a deliberate compromise: `lib`
// and `lib test` are separate compilations, so a lint can fire in one and be
// unfulfilled in the other, and no single crate-root `expect` list satisfies
// both — it oscillates. The cost is that this block does NOT delete itself
// when the rewrite lands; it has to be removed by hand, and it will silently
// keep hiding new violations until then. Shrink it as crates are rewritten.
//
// The realtime guard and every panic lint stay DENIED here — deliberately not
// in this list. `unwrap`, `expect`, `panic`, and the disallowed-methods
// realtime guard still fail the build in this crate.
#![allow(
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

/// Maximum supported bands (5 crossovers).

pub const MAX_BANDS: usize = 6;

/// One TDF2 biquad section, stereo state.
#[derive(Debug, Clone, Copy, Default)]
struct Sos {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: [f64; 2],
    z2: [f64; 2],
}

impl Sos {
    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let y = self.b0.mul_add(x, self.z1[ch]);
        self.z1[ch] = self.b1.mul_add(x, -(self.a1 * y)) + self.z2[ch];
        self.z2[ch] = self.b2.mul_add(x, -(self.a2 * y));
        y
    }

    const fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

const Q_BUTTERWORTH: f64 = core::f64::consts::FRAC_1_SQRT_2;

fn rbj(freq: f64, q: f64, sample_rate: f64) -> (f64, f64, f64) {
    let w0 = core::f64::consts::TAU * (freq / sample_rate).clamp(1.0e-5, 0.49);
    let alpha = w0.sin() / (2.0 * q);
    (w0.cos(), alpha, 1.0 + alpha)
}

fn lowpass(freq: f64, sample_rate: f64) -> Sos {
    let (cw, alpha, a0) = rbj(freq, Q_BUTTERWORTH, sample_rate);
    Sos {
        b0: (1.0 - cw) / 2.0 / a0,
        b1: (1.0 - cw) / a0,
        b2: (1.0 - cw) / 2.0 / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
        ..Default::default()
    }
}

fn highpass(freq: f64, sample_rate: f64) -> Sos {
    let (cw, alpha, a0) = rbj(freq, Q_BUTTERWORTH, sample_rate);
    Sos {
        b0: f64::midpoint(1.0, cw) / a0,
        b1: -(1.0 + cw) / a0,
        b2: f64::midpoint(1.0, cw) / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
        ..Default::default()
    }
}

fn allpass(freq: f64, sample_rate: f64) -> Sos {
    let (cw, alpha, a0) = rbj(freq, Q_BUTTERWORTH, sample_rate);
    Sos {
        b0: (1.0 - alpha) / a0,
        b1: -2.0 * cw / a0,
        b2: (1.0 + alpha) / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
        ..Default::default()
    }
}

/// One LR4 crossover: LP/HP each = two cascaded Butterworth-2 sections;
/// plus the matching 2nd-order allpass for phase-correcting bands that
/// were split off earlier.
#[derive(Debug, Clone)]
struct Crossover {
    freq: f64,
    lp: [Sos; 2],
    hp: [Sos; 2],
    /// One allpass per earlier band (each needs its own state).
    correction: [Sos; MAX_BANDS],
}

impl Crossover {
    fn new(freq: f64, sample_rate: f64) -> Self {
        Self {
            freq,
            lp: [lowpass(freq, sample_rate); 2],
            hp: [highpass(freq, sample_rate); 2],
            correction: [allpass(freq, sample_rate); MAX_BANDS],
        }
    }

    fn reset(&mut self) {
        for s in self.lp.iter_mut().chain(self.hp.iter_mut()) {
            s.reset();
        }
        for s in &mut self.correction {
            s.reset();
        }
    }
}

/// The splitter: crossover frequencies (sorted ascending) → N = k+1
/// bands. Band buffers are caller-provided (`Multiband` owns a set).
#[derive(Debug, Clone)]
pub struct BandSplitter {
    crossovers: Vec<Crossover>,
    sample_rate: f64,
}

impl BandSplitter {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self {
            crossovers: Vec::with_capacity(MAX_BANDS - 1),
            sample_rate: sample_rate.max(1.0),
        }
    }

    /// Configure crossover frequencies (unsorted OK; deduped, clamped,
    /// capped at `MAX_BANDS−1`). Allocation-free after the first call at
    /// full size. Resets filter state.
    pub fn set_crossovers(&mut self, freqs: &[f64]) {
        let mut sorted: [f64; MAX_BANDS - 1] = [0.0; MAX_BANDS - 1];
        let mut n = 0;
        for &f in freqs.iter().take(MAX_BANDS - 1) {
            sorted[n] = f.clamp(20.0, self.sample_rate * 0.45);
            n += 1;
        }
        sorted[..n].sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.crossovers.clear();
        for &f in &sorted[..n] {
            // Skip near-duplicates (< 1/12 octave apart).
            if self.crossovers.last().is_some_and(|c| f / c.freq < 1.06) {
                continue;
            }
            self.crossovers.push(Crossover::new(f, self.sample_rate));
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        let freqs: Vec<f64> = self.crossovers.iter().map(|c| c.freq).collect();
        self.set_crossovers(&freqs);
    }

    #[must_use]
    pub const fn num_bands(&self) -> usize {
        self.crossovers.len() + 1
    }

    /// Split one stereo sample into `bands` (caller slices sized ≥
    /// `num_bands`). Band 0 = lowest.
    #[inline]
    pub fn tick(&mut self, l: f64, r: f64, bands_l: &mut [f64], bands_r: &mut [f64]) {
        let (mut rem_l, mut rem_r) = (l, r);
        let n = self.crossovers.len();
        for k in 0..n {
            let c = &mut self.crossovers[k];
            // Split the lowest remaining range off.
            let low_l = {
                let s = c.lp[0].tick(0, rem_l);
                c.lp[1].tick(0, s)
            };
            let low_r = {
                let s = c.lp[0].tick(1, rem_r);
                c.lp[1].tick(1, s)
            };
            let hi_l = {
                let s = c.hp[0].tick(0, rem_l);
                c.hp[1].tick(0, s)
            };
            let hi_r = {
                let s = c.hp[0].tick(1, rem_r);
                c.hp[1].tick(1, s)
            };
            bands_l[k] = low_l;
            bands_r[k] = low_r;
            rem_l = hi_l;
            rem_r = hi_r;
            // Phase-match every earlier band through this crossover's
            // allpass so the final sum stays flat.
            for j in 0..k {
                bands_l[j] = c.correction[j].tick(0, bands_l[j]);
                bands_r[j] = c.correction[j].tick(1, bands_r[j]);
            }
        }
        bands_l[n] = rem_l;
        bands_r[n] = rem_r;
    }

    pub fn reset(&mut self) {
        for c in &mut self.crossovers {
            c.reset();
        }
    }
}

/// Per-band output controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandControls {
    pub gain_db: f64,
    pub mute: bool,
    pub solo: bool,
}

impl Default for BandControls {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            mute: false,
            solo: false,
        }
    }
}

/// Generic multiband host: split → per-band closure → recombine.
///
/// The closure receives `(band_index, left, right)` block slices; run
/// any effect on them. With no crossovers configured the closure gets
/// the raw buffers directly (band 0) — zero split cost.
pub struct Multiband {
    pub splitter: BandSplitter,
    pub controls: [BandControls; MAX_BANDS],
    band_l: Vec<Vec<f64>>,
    band_r: Vec<Vec<f64>>,
    block: usize,
}

impl Multiband {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self {
            splitter: BandSplitter::new(sample_rate),
            controls: [BandControls::default(); MAX_BANDS],
            band_l: (0..MAX_BANDS).map(|_| Vec::new()).collect(),
            band_r: (0..MAX_BANDS).map(|_| Vec::new()).collect(),
            block: 0,
        }
    }

    /// Preallocate band buffers. Call from setup, never the audio path.
    pub fn prepare(&mut self, max_block: usize) {
        self.block = max_block.max(1);
        for b in self.band_l.iter_mut().chain(self.band_r.iter_mut()) {
            b.resize(self.block, 0.0);
        }
    }

    #[must_use]
    pub const fn num_bands(&self) -> usize {
        self.splitter.num_bands()
    }

    /// Process a stereo block: split into bands, run `f` per band,
    /// apply per-band gain/mute/solo, sum back into `l`/`r`.
    pub fn process<F>(&mut self, l: &mut [f64], r: &mut [f64], mut f: F)
    where
        F: FnMut(usize, &mut [f64], &mut [f64]),
    {
        let n_bands = self.splitter.num_bands();
        if n_bands == 1 {
            // Single band: the closure runs on the raw buffers.
            f(0, l, r);
            let c = &self.controls[0];
            let g = if c.mute {
                0.0
            } else {
                10.0f64.powf(c.gain_db / 20.0)
            };
            if (g - 1.0).abs() > 1.0e-12 {
                for i in 0..l.len() {
                    l[i] *= g;
                    r[i] *= g;
                }
            }
            return;
        }
        let len = l.len().min(r.len()).min(self.block);
        // Split.
        let mut sl = [0.0; MAX_BANDS];
        let mut sr = [0.0; MAX_BANDS];
        for i in 0..len {
            self.splitter.tick(l[i], r[i], &mut sl, &mut sr);
            for b in 0..n_bands {
                self.band_l[b][i] = sl[b];
                self.band_r[b][i] = sr[b];
            }
        }
        // Per-band processing.
        for b in 0..n_bands {
            f(b, &mut self.band_l[b][..len], &mut self.band_r[b][..len]);
        }
        // Recombine with gain/mute/solo.
        let any_solo = self.controls[..n_bands].iter().any(|c| c.solo);
        for i in 0..len {
            let mut ol = 0.0;
            let mut or = 0.0;
            for b in 0..n_bands {
                let c = &self.controls[b];
                let audible = !c.mute && (!any_solo || c.solo);
                if audible {
                    let g = 10.0f64.powf(c.gain_db / 20.0);
                    ol += self.band_l[b][i] * g;
                    or += self.band_r[b][i] * g;
                }
            }
            l[i] = ol;
            r[i] = or;
        }
    }

    pub fn reset(&mut self) {
        self.splitter.reset();
    }
}
