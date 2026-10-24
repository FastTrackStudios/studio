//! Compressor chain — wrapper with lookahead delay and sidechain EQ.

use dsp_core::num;

use crate::comp::{Biquad, Detector, design_highpass_biquad, design_lowpass_biquad};
use dsp_core::audio::AudioConfig;
use dsp_core::audio::biquad::{Biquad as EqBiquad, FilterType as EqFilterType};

/// Number of sidechain EQ bands.
pub const SC_EQ_BANDS: usize = 6;

/// One sidechain EQ band — a curve on the DETECTOR key, not the audio.
///
/// (`fx.embed-eq.one-surface`: the comp stage's sidecar EQ). Boost a band
/// and the compressor listens harder there; cut it and that frequency stops
/// triggering. The classic HP/LP pair stays alongside as the coarse control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SidechainBand {
    /// 0 = Bell, 1 = Low Shelf, 2 = High Shelf, 3 = Low Cut, 4 = High Cut.
    pub shape: u32,
    pub freq_hz: f64,
    /// Ignored by the cut shapes.
    pub gain_db: f64,
    pub q: f64,
}

impl Default for SidechainBand {
    fn default() -> Self {
        Self {
            shape: 0,
            freq_hz: 1000.0,
            gain_db: 0.0,
            q: 0.707,
        }
    }
}

impl SidechainBand {
    #[must_use]
    pub fn is_active(&self) -> bool {
        match self.shape {
            3 => self.freq_hz > 21.0,
            4 => self.freq_hz < 19_500.0,
            _ => self.gain_db.abs() > 0.01,
        }
    }
}

/// Complete compressor processing chain.
/// The lookahead delay: a stereo ring the compressor reads behind.
///
/// Interleaved rather than two parallel `Vec`s with a shared cursor. The two
/// channels' read and write positions are then identical by construction
/// instead of by convention, and there is one length to keep in step with the
/// sample count instead of two.
#[derive(Debug, Clone, Default)]
struct Lookahead {
    ring: Vec<(f64, f64)>,
    pos: usize,
}

impl Lookahead {
    /// Size the ring for `samples` of delay.
    ///
    /// Allocates, so it belongs on the setter path — never in `process`.
    fn resize(&mut self, samples: usize) {
        self.ring.clear();
        self.ring.resize(samples.max(1), (0.0, 0.0));
        self.pos = 0;
    }

    /// Zero the ring without resizing it — what `reset()` wants.
    fn clear(&mut self) {
        for slot in &mut self.ring {
            *slot = (0.0, 0.0);
        }
        self.pos = 0;
    }

    /// Write the newest pair and return the one a full ring ago.
    ///
    /// An empty ring returns the input unchanged: the caller only reaches here
    /// with a non-zero lookahead, and passing the signal through is the right
    /// answer for a delay of nothing anyway.
    fn exchange(&mut self, input: (f64, f64)) -> (f64, f64) {
        let length = self.ring.len();
        let Some(slot) = self.ring.get_mut(self.pos) else {
            return input;
        };
        let delayed = *slot;
        *slot = input;
        self.pos = self.pos.saturating_add(1).checked_rem(length).unwrap_or(0);
        delayed
    }
}

pub struct CompChain {
    model: Option<crate::comp::CompressorProcessor>,
    model_enabled: bool,
    model_mix: f64,
    pub comp: super::ProC3Compressor,
    pub sidechain_freq: f64,
    pub sidechain_lowpass_freq: f64,
    sidechain_hpf_l: Biquad,
    sidechain_hpf_r: Biquad,
    sidechain_lpf_l: Biquad,
    sidechain_lpf_r: Biquad,
    /// The 6-band sidechain EQ, on the key path before the HP/LP pair.
    sc_eq: [SidechainBand; SC_EQ_BANDS],
    sc_eq_filters: [EqBiquad; SC_EQ_BANDS],
    sc_eq_on: [bool; SC_EQ_BANDS],
    sc_eq_any: bool,
    lookahead_ms: f64,
    pub lookahead_samples: usize,
    lookahead: Lookahead,
    feedback_l: f64,
    feedback_r: f64,
    detector_l: Detector,
    detector_r: Detector,
    sample_rate: f64,
}

impl CompChain {
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: None,
            model_enabled: false,
            model_mix: 0.0,
            comp: super::ProC3Compressor::new(48000.0),
            sidechain_freq: 0.0,
            sidechain_lowpass_freq: 0.0,
            sidechain_hpf_l: Biquad::new(),
            sidechain_hpf_r: Biquad::new(),
            sidechain_lpf_l: Biquad::new(),
            sidechain_lpf_r: Biquad::new(),
            sc_eq: [SidechainBand::default(); SC_EQ_BANDS],
            sc_eq_filters: core::array::from_fn(|_| EqBiquad::new()),
            sc_eq_on: [false; SC_EQ_BANDS],
            sc_eq_any: false,
            lookahead_ms: 0.0,
            lookahead_samples: 0,
            lookahead: Lookahead::default(),
            feedback_l: 0.0,
            feedback_r: 0.0,
            detector_l: Detector::new(),
            detector_r: Detector::new(),
            sample_rate: 48000.0,
        }
    }

    /// Select a prepared model while retaining the chain's sidechain and delay.
    /// Same-model controls preserve history; switching to/from the host core
    /// crossfades over 5 ms. Built-in model installation allocates nothing.
    pub fn set_model(&mut self, prepared: &crate::comp::PreparedCompressor) {
        if let Some(model) = &mut self.model {
            if model.apply(prepared).is_err() {
                self.model = Some(prepared.processor());
            }
        } else {
            self.model = Some(prepared.processor());
        }
        self.model_enabled = true;
    }

    /// Return to the extended host processor with a short transition.
    pub const fn clear_model(&mut self) {
        self.model_enabled = false;
    }

    /// Positive reduction from the processor currently selected for this chain.
    #[must_use]
    pub fn gain_reduction_db(&self) -> f64 {
        if self.model_enabled {
            self.model.as_ref().map_or(0.0, |model| {
                let [left, right] = model.gain_reduction_db();
                left.max(right)
            })
        } else {
            self.comp.gain_reduction_db()
        }
    }

    /// Process a single stereo sample through the full chain.
    pub fn process_sample(&mut self, left: &mut f64, right: &mut f64) {
        self.process_sample_with_sidechain(left, right, *left, *right);
    }

    /// Process a stereo sample using an explicit sidechain/key signal.
    pub fn process_sample_with_sidechain(
        &mut self,
        left: &mut f64,
        right: &mut f64,
        sidechain_l: f64,
        sidechain_r: f64,
    ) {
        // Handle lookahead delay buffer
        let (audio_l, audio_r) = if self.lookahead_samples > 0 {
            self.lookahead.exchange((*left, *right))
        } else {
            (*left, *right)
        };

        let (ff_key_l, ff_key_r) = self.sidechain_key(sidechain_l, sidechain_r);
        let feedback = self.comp.feedback.clamp(0.0, 1.0);
        let key_l = ff_key_l.mul_add(1.0 - feedback, self.feedback_l.abs() * feedback);
        let key_r = ff_key_r.mul_add(1.0 - feedback, self.feedback_r.abs() * feedback);

        let link = self.comp.channel_link.clamp(0.0, 1.0);
        let linked = key_l.max(key_r);
        let detect_l = key_l * (1.0 - link) + linked * link;
        let detect_r = key_r * (1.0 - link) + linked * link;

        let rms_mix = self.comp.detector_rms_mix;
        let level_l = self
            .detector_l
            .detect_level_with_rms_mix(detect_l.max(1e-12), rms_mix);
        let level_r = self
            .detector_r
            .detect_level_with_rms_mix(detect_r.max(1e-12), rms_mix);

        let step = 1.0 / (self.sample_rate * 0.005).max(1.0);
        self.model_mix = if self.model_enabled {
            (self.model_mix + step).min(1.0)
        } else {
            (self.model_mix - step).max(0.0)
        };
        let (mut out_l, mut out_r) = if self.model_mix < 1.0 {
            (
                self.comp.process_with_level(audio_l, level_l, 0),
                self.comp.process_with_level(audio_r, level_r, 1),
            )
        } else {
            (0.0, 0.0)
        };
        if self.model_mix > 0.0
            && let Some(model) = &mut self.model
        {
            let [ml, mr] =
                model.process_frame_with_sidechain([audio_l, audio_r], [ff_key_l, ff_key_r]);
            out_l = self.model_mix.mul_add(ml - out_l, out_l);
            out_r = self.model_mix.mul_add(mr - out_r, out_r);
        }

        *left = out_l;
        *right = out_r;
        self.feedback_l = out_l;
        self.feedback_r = out_r;
    }

    /// Set the sidechain high-pass frequency in Hz. Values at or below 20 Hz
    /// bypass the filter.
    pub fn set_sidechain_freq(&mut self, freq: f64) {
        let freq = freq.clamp(0.0, self.sample_rate * 0.45);
        if (self.sidechain_freq - freq).abs() <= 0.01 {
            return;
        }

        self.sidechain_freq = freq;
        self.rebuild_sidechain_filter();
    }

    /// Set the sidechain low-pass frequency in Hz. Values at or below 20 Hz
    /// bypass the filter.
    pub fn set_sidechain_lowpass_freq(&mut self, freq: f64) {
        let freq = freq.clamp(0.0, self.sample_rate * 0.45);
        if (self.sidechain_lowpass_freq - freq).abs() <= 0.01 {
            return;
        }

        self.sidechain_lowpass_freq = freq;
        self.rebuild_sidechain_filter();
    }

    fn rebuild_sidechain_filter(&mut self) {
        if self.sidechain_freq <= 20.0 {
            self.sidechain_hpf_l = Biquad::new();
            self.sidechain_hpf_r = Biquad::new();
        } else {
            let cutoff = (self.sidechain_freq / (self.sample_rate * 0.5)).clamp(0.001, 0.999);
            self.sidechain_hpf_l = design_highpass_biquad(cutoff);
            self.sidechain_hpf_r = design_highpass_biquad(cutoff);
        }

        if self.sidechain_lowpass_freq <= 20.0 {
            self.sidechain_lpf_l = Biquad::new();
            self.sidechain_lpf_r = Biquad::new();
        } else {
            let cutoff =
                (self.sidechain_lowpass_freq / (self.sample_rate * 0.5)).clamp(0.001, 0.999);
            self.sidechain_lpf_l = design_lowpass_biquad(cutoff);
            self.sidechain_lpf_r = design_lowpass_biquad(cutoff);
        }
    }

    /// Reserve the host's maximum delay before entering the audio callback.
    /// Subsequent changes up to this limit reuse this allocation.
    pub fn reserve_lookahead(&mut self, maximum_ms: f64) {
        let samples = num::f64_to_index((maximum_ms.max(0.0) * self.sample_rate / 1000.0).ceil());
        self.lookahead
            .ring
            .reserve(samples.saturating_sub(self.lookahead.ring.len()));
    }

    /// Set the lookahead time in ms.
    pub fn set_lookahead(&mut self, lookahead_ms: f64) {
        self.lookahead_ms = lookahead_ms;
        let n = num::f64_to_index((lookahead_ms / 1000.0 * self.sample_rate).round());
        if n != self.lookahead_samples {
            self.lookahead_samples = n;
            self.lookahead.resize(n);
        }
    }

    /// Update sample rate (used when format changes).
    pub fn update_sample_rate(&mut self, sample_rate: f64) {
        if (self.sample_rate - sample_rate).abs() > 0.01 {
            self.sample_rate = sample_rate;
            // Force a redesign of the sidechain EQ at the new rate.
            let bands = self.sc_eq;
            self.sc_eq = [SidechainBand {
                freq_hz: -1.0,
                ..SidechainBand::default()
            }; SC_EQ_BANDS];
            self.set_sidechain_eq(bands);
        }
        self.sample_rate = sample_rate;
        self.detector_l.update_sample_rate(sample_rate);
        self.detector_r.update_sample_rate(sample_rate);
        self.rebuild_sidechain_filter();
        // Rebuild lookahead buffers if needed
        if self.lookahead_ms > 0.0 {
            self.set_lookahead(self.lookahead_ms);
        }
    }

    /// Reset internal state.
    pub fn reset(&mut self) {
        if let Some(model) = &mut self.model {
            model.reset();
        }
        self.model_mix = if self.model_enabled { 1.0 } else { 0.0 };
        self.comp.reset();
        self.lookahead.clear();
        self.feedback_l = 0.0;
        self.feedback_r = 0.0;
        self.detector_l.reset();
        self.detector_r.reset();
        for f in &mut self.sc_eq_filters {
            f.reset();
        }
    }

    /// Update to new audio config (called when sample rate or buffer size changes).
    pub fn update(&mut self, config: AudioConfig) {
        self.comp.update(config.sample_rate);
        self.update_sample_rate(config.sample_rate);
    }
}

impl CompChain {
    /// Re-design the sidechain EQ when the band table moved. Change-detected:
    /// call once per block for free.
    pub fn set_sidechain_eq(&mut self, bands: [SidechainBand; SC_EQ_BANDS]) {
        if bands == self.sc_eq {
            return;
        }
        self.sc_eq = bands;
        self.sc_eq_any = false;
        for ((band, on), filter) in bands
            .iter()
            .zip(&mut self.sc_eq_on)
            .zip(&mut self.sc_eq_filters)
        {
            *on = band.is_active();
            if !band.is_active() {
                continue;
            }
            self.sc_eq_any = true;
            let f = band.freq_hz.clamp(20.0, self.sample_rate * 0.45);
            let q = band.q.clamp(0.1, 18.0);
            let gain_db = band.gain_db.clamp(-24.0, 24.0);
            let ftype = match band.shape {
                1 => EqFilterType::LowShelf { gain_db },
                2 => EqFilterType::HighShelf { gain_db },
                3 => EqFilterType::Highpass,
                4 => EqFilterType::Lowpass,
                _ => EqFilterType::Peak { gain_db },
            };
            filter.set(ftype, f, q, self.sample_rate);
            filter.reset();
        }
    }

    /// The sidechain EQ bands as set.
    #[must_use]
    pub const fn sidechain_eq(&self) -> &[SidechainBand; SC_EQ_BANDS] {
        &self.sc_eq
    }

    fn sidechain_key(&mut self, left: f64, right: f64) -> (f64, f64) {
        // The 6-band sidechain EQ shapes what the detector HEARS, on the raw
        // key before rectification (a filter after abs() would be wrong).
        let (mut left, mut right) = (left, right);
        if self.sc_eq_any {
            for (on, filter) in self.sc_eq_on.iter().zip(&mut self.sc_eq_filters) {
                if *on {
                    left = filter.tick(left, 0);
                    right = filter.tick(right, 1);
                }
            }
        }

        let (mut key_l, mut key_r) = if self.sidechain_freq > 20.0 {
            (
                self.sidechain_hpf_l.tick(left, 0),
                self.sidechain_hpf_r.tick(right, 0),
            )
        } else {
            (left, right)
        };

        // Both filters act on signed audio. Rectification before the low-pass
        // creates DC from every carrier, which a low-pass cannot reject.
        if self.sidechain_lowpass_freq > 20.0 {
            key_l = self.sidechain_lpf_l.tick(key_l, 0);
            key_r = self.sidechain_lpf_r.tick(key_r, 0);
        }
        let (key_l, key_r) = (key_l.abs(), key_r.abs());

        (key_l, key_r)
    }
}

impl Default for CompChain {
    fn default() -> Self {
        Self::new()
    }
}
