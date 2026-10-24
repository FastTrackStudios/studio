//! [`SpectrumTap`] → [`SpectrumAnalyzer`]: the analyser every plugin
//! shares. The audio thread only pushes samples; the control thread
//! windows (Hann) the latest `fft_size` of them, transforms, folds the FFT
//! bins into log-spaced display bins, smooths across frequency and applies
//! ballistics (instant attack, decay in dB/s).

use crate::feed::proto::{FeedKind, FeedOptions};

use crate::feed::dsp::fft::Fft;
use crate::feed::dsp::ring::{RingReader, RingWriter, ring};
use crate::feed::dsp::{FeedSource, amp_to_db};

/// How a spectrum is shaped for display.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrumConfig {
    /// Log-spaced display bins.
    pub bins: usize,
    pub f_min: f64,
    pub f_max: f64,
    /// Values clamp here (dBFS).
    pub floor_db: f64,
    /// How fast a bin falls after a peak.
    pub decay_db_per_s: f64,
    /// Frequency smoothing width in octaves (0 = none; `1/3` = third-octave).
    pub smoothing_octaves: f64,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            bins: 64,
            f_min: 20.0,
            f_max: 20_000.0,
            floor_db: -90.0,
            decay_db_per_s: 30.0,
            smoothing_octaves: 0.0,
        }
    }
}

impl SpectrumConfig {
    /// The config a feed of `kind` asks for (`None` unless a spectrum),
    /// with `options` applied (bins, smoothing).
    #[must_use]
    pub fn for_feed(kind: &FeedKind, options: &FeedOptions) -> Option<Self> {
        match kind.with_options(options) {
            FeedKind::Spectrum {
                bins,
                f_min,
                f_max,
                floor_db,
            } => Some(Self {
                bins: usize::from(bins.max(1)),
                f_min,
                f_max,
                floor_db,
                smoothing_octaves: options.smoothing.unwrap_or(0.0).max(0.0),
                ..Self::default()
            }),
            _ => None,
        }
    }
}

/// The audio-thread end: push samples, nothing else.
pub struct SpectrumTap {
    ring: RingWriter,
}

impl SpectrumTap {
    /// Push a mono block. Wait-free, no allocation.
    #[inline]
    pub fn push(&mut self, samples: &[f32]) {
        self.ring.push_slice(samples);
    }

    /// Push a stereo block as its mid (L+R)/2. Wait-free, no allocation.
    #[inline]
    pub fn push_stereo(&mut self, left: &[f32], right: &[f32]) {
        for (l, r) in left.iter().zip(right) {
            self.ring.push(0.5 * (l + r));
        }
    }
}

/// The control-thread end: turns the latest samples into display bins.
pub struct SpectrumAnalyzer {
    reader: RingReader,
    fft: Fft,
    sample_rate: f64,
    window: Box<[f64]>,
    /// `Σ window`: a full-scale sine's bin reads `Σw / 2`.
    window_sum: f64,
    samples: Vec<f32>,
    re: Vec<f64>,
    im: Vec<f64>,
    /// Linear amplitude per FFT bin (`0..=n/2`).
    amp: Vec<f64>,
    cfg: SpectrumConfig,
    /// Display bin edges, Hz (`bins + 1`).
    edges: Vec<f64>,
    /// This read's power per display bin.
    power: Vec<f64>,
    prefix: Vec<f64>,
    /// What's on screen, dB (ballistics state).
    held: Vec<f64>,
    primed: bool,
}

/// An analyser over `fft_size` samples (a power of two; 2048–8192 suits
/// 44.1–96 kHz) at `sample_rate`. Allocates here; the tap never does.
///
/// # Panics
/// When `fft_size` isn't a power of two ≥ 2.
#[must_use]
pub fn spectrum(fft_size: usize, sample_rate: f64) -> (SpectrumTap, SpectrumAnalyzer) {
    let (w, r) = ring(fft_size * 2);
    let n = fft_size;
    let window: Box<[f64]> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())
        .collect();
    let window_sum = window.iter().sum();
    let mut a = SpectrumAnalyzer {
        reader: r,
        fft: Fft::new(n),
        sample_rate,
        window,
        window_sum,
        samples: vec![0.0; n],
        re: vec![0.0; n],
        im: vec![0.0; n],
        amp: vec![0.0; n / 2 + 1],
        cfg: SpectrumConfig::default(),
        edges: Vec::new(),
        power: Vec::new(),
        prefix: Vec::new(),
        held: Vec::new(),
        primed: false,
    };
    a.configure(SpectrumConfig::default());
    (SpectrumTap { ring: w }, a)
}

impl SpectrumAnalyzer {
    #[must_use]
    pub fn config(&self) -> &SpectrumConfig {
        &self.cfg
    }

    /// Reshape (bins, band, floor, decay, smoothing). Resets the ballistics
    /// when the bins change.
    pub fn configure(&mut self, cfg: SpectrumConfig) {
        let bins = cfg.bins.max(1);
        let f_min = cfg.f_min.max(1e-3);
        let f_max = cfg.f_max.max(f_min * 1.0001);
        let ratio = f_max / f_min;
        self.edges = (0..=bins)
            .map(|j| f_min * ratio.powf(j as f64 / bins as f64))
            .collect();
        if self.held.len() != bins {
            self.held = vec![cfg.floor_db; bins];
            self.primed = false;
        }
        self.power.resize(bins, 0.0);
        self.prefix.resize(bins + 1, 0.0);
        self.cfg = SpectrumConfig {
            bins,
            f_min,
            f_max,
            ..cfg
        };
    }

    /// Forget the ballistics (the next read shows the spectrum as is).
    pub fn reset(&mut self) {
        self.primed = false;
    }

    /// The display bin a frequency falls in.
    #[must_use]
    pub fn bin_of(&self, hz: f64) -> Option<usize> {
        self.edges.windows(2).position(|e| hz >= e[0] && hz < e[1])
    }

    /// The display bins' centres (geometric), Hz.
    pub fn centers(&self) -> impl Iterator<Item = f64> + '_ {
        self.edges.windows(2).map(|e| (e[0] * e[1]).sqrt())
    }

    /// Analyse the latest samples: `out` becomes one dB value per display
    /// bin. `dt` is the time since the previous read (for the decay).
    pub fn analyze(&mut self, dt: f64, out: &mut Vec<f64>) {
        let n = self.fft.len();
        self.samples.fill(0.0);
        self.reader.latest(&mut self.samples);
        for i in 0..n {
            self.re[i] = f64::from(self.samples[i]) * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft.forward(&mut self.re, &mut self.im);
        let scale = 2.0 / self.window_sum;
        for (k, a) in self.amp.iter_mut().enumerate() {
            *a = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt() * scale;
        }
        self.fold();
        self.smooth();
        let floor = self.cfg.floor_db;
        let fall = self.cfg.decay_db_per_s.max(0.0) * dt.max(0.0);
        for (h, p) in self.held.iter_mut().zip(&self.power) {
            let now = amp_to_db(p.sqrt()).max(floor);
            *h = if self.primed {
                now.max(*h - fall).max(floor)
            } else {
                now
            };
        }
        self.primed = true;
        out.clear();
        out.extend_from_slice(&self.held);
    }

    /// FFT amplitudes → power per display bin: the loudest FFT bin inside
    /// a display bin, or (for display bins narrower than an FFT bin) the
    /// amplitude interpolated at its centre.
    fn fold(&mut self) {
        let n = self.fft.len() as f64;
        let hz_per_bin = self.sample_rate / n;
        let last = self.amp.len() - 1;
        for (j, e) in self.edges.windows(2).enumerate() {
            let lo = (e[0] / hz_per_bin).ceil() as usize;
            let hi = ((e[1] / hz_per_bin).ceil() as usize).min(last + 1);
            let a = if lo < hi {
                self.amp[lo..hi].iter().copied().fold(0.0, f64::max)
            } else {
                let pos = ((e[0] * e[1]).sqrt() / hz_per_bin).min(last as f64);
                let k = pos.floor() as usize;
                let t = pos - k as f64;
                let next = self.amp[(k + 1).min(last)];
                self.amp[k] * (1.0 - t) + next * t
            };
            self.power[j] = a * a;
        }
    }

    /// Average power over ±half the smoothing width (in display bins).
    fn smooth(&mut self) {
        let bins = self.power.len();
        let octaves = (self.cfg.f_max / self.cfg.f_min).log2();
        let half = (self.cfg.smoothing_octaves * bins as f64 / octaves / 2.0).round() as usize;
        if half == 0 {
            return;
        }
        self.prefix[0] = 0.0;
        for j in 0..bins {
            self.prefix[j + 1] = self.prefix[j] + self.power[j];
        }
        for j in 0..bins {
            let (a, b) = (j.saturating_sub(half), (j + half + 1).min(bins));
            self.power[j] = (self.prefix[b] - self.prefix[a]) / (b - a) as f64;
        }
    }
}

impl FeedSource for SpectrumAnalyzer {
    fn configure(&mut self, info: &crate::feed::proto::FeedInfo, options: &FeedOptions) {
        if let Some(cfg) = SpectrumConfig::for_feed(&info.kind, options) {
            self.configure(SpectrumConfig {
                decay_db_per_s: self.cfg.decay_db_per_s,
                ..cfg
            });
        }
    }

    fn read(&mut self, dt: f64, out: &mut Vec<f64>) {
        self.analyze(dt, out);
    }
}
