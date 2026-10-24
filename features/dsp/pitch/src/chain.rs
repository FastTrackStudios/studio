//! PitchChain — top-level processor with algorithm selection.
//!
//! Wraps all four pitch shifting algorithms behind the `Processor` trait
//! with runtime algorithm switching. Pitch is controlled via a semitone
//! slider (−24 to +24).

use dsp_core::audio::{AudioConfig, Processor};
use serde::{Deserialize, Serialize};

use crate::allpass_shift::AllpassShifter;
use crate::divider::{DivideRatio, FreqDivider};
use crate::granular::GranularShifter;
use crate::pll::{PllOctave, PllTracker, SubWaveform};
use crate::pog::{OctaveShift, PolyOctave};
use crate::psola::PsolaShifter;
#[cfg(feature = "rubberband")]
use crate::rubberband::RubberbandShifter;
use crate::spectral::SpectralShifter;
use crate::wsola::WsolaShifter;

/// Which pitch shifting algorithm to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Algorithm {
    /// Analog-style frequency divider. 0 latency, synthy character.
    /// Only supports exact octave shifts (−12 or −24 semitones).
    /// Other values are clamped to the nearest octave.
    FreqDivider,
    /// PLL tracking oscillator. 0 latency, warmer sub.
    /// Only supports exact octave shifts (−12 or −24 semitones).
    /// Other values are clamped to the nearest octave.
    Pll,
    /// Granular fixed-ratio. ~1024 sample latency, natural tone.
    /// Supports arbitrary semitone values.
    Granular,
    /// PSOLA pitch-synchronous. ~2048 sample latency, highest quality.
    /// Supports arbitrary semitone values.
    Psola,
    /// WSOLA waveform-similarity overlap-add. ~1024 sample latency.
    /// Works on any signal (monophonic or polyphonic) without pitch detection.
    Wsola,
    /// Rubber Band Library — professional pitch shifter with formant preservation.
    /// High quality, ~512+ sample latency.
    Rubberband,
    /// Allpass interpolation — Dattorro/Schroeder barberpole style.
    /// ~1024 sample latency (standard), ~256 (live). Classic hardware character.
    Allpass,
    /// Polyphonic Octave Generator — ERB filter bank phase scaling (EHX POG style).
    /// 0 sample latency, polyphonic, octave shifts only (±12, ±24 semitones).
    PolyOctave,
    /// Phase-locked phase vocoder (Laroche–Dolson peak shifting).
    /// Polyphonic, no grain warble, arbitrary ratio. Latency 2048 samples
    /// (42.7 ms @ 48 kHz); `live` = 1024 (21.3 ms).
    Spectral,
}

/// Convert semitones to pitch ratio: `2^(semitones / 12)`.
pub fn semitones_to_ratio(semitones: f64) -> f64 {
    (2.0f64).powf(semitones / 12.0)
}

/// Convert pitch ratio to semitones: `12 * log2(ratio)`.
pub fn ratio_to_semitones(ratio: f64) -> f64 {
    12.0 * ratio.log2()
}

/// Top-level pitch shifter with algorithm selection.
pub struct PitchChain {
    /// Active algorithm.
    pub algorithm: Algorithm,
    /// Pitch shift in semitones. Range: −24.0 to +24.0.
    /// 0 = no shift, −12 = octave down, +12 = octave up.
    pub semitones: f64,
    /// Dry/wet mix (0.0–1.0).
    pub mix: f64,
    /// Live mode: minimize latency at the cost of quality.
    pub live: bool,
    /// Formant shift in semitones. Range: −24.0 to +24.0.
    /// 0 = no shift. Only affects algorithms that support formant control
    /// (Rubberband).
    pub formant_semitones: f64,
    /// When true, formant shift is linked to pitch (formants stay in place).
    /// When false, formant_semitones is applied independently.
    pub formant_linked: bool,

    // -- PLL-specific settings --
    /// PLL sub-oscillator waveform.
    pub pll_waveform: SubWaveform,

    // -- Granular-specific settings --
    /// Grain size in samples.
    pub grain_size: usize,

    divider: FreqDivider,
    pll: PllTracker,
    // The engines with buffers (seconds of audio, FFT frames), built when
    // their algorithm is first selected — [`PitchChain::only`] holds one.
    // A guitar rig carries a pitch block, four chains of it, in every
    // patch it has prepared: all nine engines each was ~2.6 MB a chain.
    granular: Option<Box<GranularShifter>>,
    psola: Option<Box<PsolaShifter>>,
    wsola: Option<Box<WsolaShifter>>,
    #[cfg(feature = "rubberband")]
    rubberband: Option<Box<RubberbandShifter>>,
    allpass: Option<Box<AllpassShifter>>,
    pog: PolyOctave,
    spectral: Option<Box<SpectralShifter>>,

    /// Track previous live state to detect changes and reinitialise.
    prev_live: bool,
    sample_rate: f64,
    /// `update` has run: an engine built after it is updated as it is built.
    prepared: bool,
}

impl PitchChain {
    pub fn new() -> Self {
        Self {
            algorithm: Algorithm::FreqDivider,
            semitones: -12.0,
            mix: 1.0,
            live: false,
            formant_semitones: 0.0,
            formant_linked: true,
            pll_waveform: SubWaveform::Saw,
            grain_size: 1024,
            divider: FreqDivider::new(),
            pll: PllTracker::new(),
            granular: Some(Box::new(GranularShifter::new())),
            psola: Some(Box::new(PsolaShifter::new())),
            wsola: Some(Box::new(WsolaShifter::new())),
            #[cfg(feature = "rubberband")]
            rubberband: Some(Box::new(RubberbandShifter::new())),
            allpass: Some(Box::new(AllpassShifter::new())),
            pog: PolyOctave::new(),
            spectral: Some(Box::new(SpectralShifter::new())),
            prev_live: false,
            sample_rate: 48000.0,
            prepared: false,
        }
    }

    /// A chain holding only `algorithm`'s engine: another is built when it
    /// is [`select`](Self::select)ed. The memory of one engine, not nine.
    pub fn only(algorithm: Algorithm) -> Self {
        let mut c = Self::new();
        c.granular = None;
        c.psola = None;
        c.wsola = None;
        #[cfg(feature = "rubberband")]
        {
            c.rubberband = None;
        }
        c.allpass = None;
        c.spectral = None;
        c.select(algorithm);
        c
    }

    /// Play `algorithm`, building its engine if this chain has none —
    /// an allocation: call it where a reverb's new algorithm is built (a
    /// param write on the control thread, or before preparing), never from
    /// `process`.
    pub fn select(&mut self, algorithm: Algorithm) {
        self.algorithm = algorithm;
        let live = self.live;
        let (sr, prepared) = (self.sample_rate, self.prepared);
        match algorithm {
            Algorithm::Granular if self.granular.is_none() => {
                let mut e = Box::new(GranularShifter::new());
                if prepared {
                    e.update(sr);
                }
                self.granular = Some(e);
            }
            Algorithm::Psola if self.psola.is_none() => {
                let mut e = Box::new(PsolaShifter::new());
                e.base_window_size = if live { 512 } else { 2048 };
                if prepared {
                    e.update(sr);
                }
                self.psola = Some(e);
            }
            #[cfg(not(feature = "rubberband"))]
            Algorithm::Rubberband | Algorithm::Wsola if self.wsola.is_none() => self.build_wsola(),
            #[cfg(feature = "rubberband")]
            Algorithm::Wsola if self.wsola.is_none() => self.build_wsola(),
            #[cfg(feature = "rubberband")]
            Algorithm::Rubberband if self.rubberband.is_none() => {
                let mut e = Box::new(RubberbandShifter::new());
                e.live = live;
                if prepared {
                    e.update(sr);
                }
                self.rubberband = Some(e);
            }
            Algorithm::Allpass if self.allpass.is_none() => {
                let mut e = Box::new(AllpassShifter::new());
                if prepared {
                    e.update(sr);
                }
                self.allpass = Some(e);
            }
            Algorithm::Spectral if self.spectral.is_none() => {
                let mut e = Box::new(Self::spectral_for(sr));
                e.fft_size = if live { 1024 } else { 2048 };
                if prepared {
                    e.update(sr);
                }
                self.spectral = Some(e);
            }
            _ => {}
        }
    }

    /// A spectral engine holding the frames this chain plays at
    /// `sample_rate` (its non-live frame, the larger), not every frame.
    fn spectral_for(sample_rate: f64) -> SpectralShifter {
        SpectralShifter::with_max_frame(SpectralShifter::frame_for(2048, sample_rate))
    }

    fn build_wsola(&mut self) {
        let mut e = Box::new(WsolaShifter::new());
        e.base_grain_size = if self.live { 256 } else { 1024 };
        if self.prepared {
            e.update(self.sample_rate);
        }
        self.wsola = Some(e);
    }

    /// Latency introduced by the currently selected algorithm.
    pub fn latency(&self) -> usize {
        match self.algorithm {
            Algorithm::FreqDivider => self.divider.latency(),
            Algorithm::Pll => self.pll.latency(),
            Algorithm::Granular => self.granular.as_ref().map_or(0, |e| e.latency()),
            Algorithm::Psola => self.psola.as_ref().map_or(0, |e| e.latency()),
            Algorithm::Wsola => self.wsola.as_ref().map_or(0, |e| e.latency()),
            #[cfg(feature = "rubberband")]
            Algorithm::Rubberband => self.rubberband.as_ref().map_or(0, |e| e.latency()),
            #[cfg(not(feature = "rubberband"))]
            Algorithm::Rubberband => self.wsola.as_ref().map_or(0, |e| e.latency()),
            Algorithm::Allpass => self.allpass.as_ref().map_or(0, |e| e.latency()),
            Algorithm::PolyOctave => self.pog.latency(),
            Algorithm::Spectral => self.spectral.as_ref().map_or(0, |e| e.latency()),
        }
    }

    /// Map semitones to the nearest octave division for divider/PLL.
    /// Returns (DivideRatio/PllOctave, direction).
    /// Divider/PLL only support down-shifting by exact octaves.
    fn nearest_octave(&self) -> (bool, bool) {
        // is_oct2: true if closer to −24 than −12.
        // is_down: true if semitones < −6 (clearly wants a sub).
        let is_down = self.semitones < -6.0;
        let is_oct2 = self.semitones < -18.0;
        (is_down, is_oct2)
    }

    /// Sync algorithm-specific parameters before processing.
    fn sync_params(&mut self) {
        // Detect live mode changes and reconfigure engines.
        if self.live != self.prev_live {
            self.prev_live = self.live;

            let (live, sr) = (self.live, self.sample_rate);
            // PSOLA: smaller analysis window in live mode.
            if let Some(e) = self.psola.as_deref_mut() {
                e.base_window_size = if live { 512 } else { 2048 };
                e.update(sr);
            }
            // WSOLA: smaller grains in live mode.
            if let Some(e) = self.wsola.as_deref_mut() {
                e.base_grain_size = if live { 256 } else { 1024 };
                e.update(sr);
            }
            // Rubberband: R2 engine + smaller blocks.
            #[cfg(feature = "rubberband")]
            if let Some(e) = self.rubberband.as_deref_mut() {
                e.live = live;
                e.update(sr);
            }
            // Allpass: no live mode toggle (not supported).
            if let Some(e) = self.allpass.as_deref_mut() {
                e.update(sr);
            }
            // Spectral: 1024-sample frames live, 2048 otherwise.
            if let Some(e) = self.spectral.as_deref_mut() {
                e.fft_size = if live { 1024 } else { 2048 };
                e.update(sr);
            }
        }

        let ratio = semitones_to_ratio(self.semitones.clamp(-24.0, 24.0));
        let (is_down, is_oct2) = self.nearest_octave();

        // Divider: only supports octave down. Bypass if shifting up.
        self.divider.ratio = if is_oct2 {
            DivideRatio::Oct2
        } else {
            DivideRatio::Oct1
        };
        self.divider.mix = if is_down { self.mix } else { 0.0 };

        // PLL: only supports octave down. Bypass if shifting up.
        self.pll.waveform = self.pll_waveform;
        self.pll.octave = if is_oct2 {
            PllOctave::Oct2
        } else {
            PllOctave::Oct1
        };
        self.pll.mix = if is_down { self.mix } else { 0.0 };

        // Granular: arbitrary ratio. In live mode, cap grain size at 256.
        let (mix, live, grain) = (self.mix, self.live, self.grain_size);
        if let Some(e) = self.granular.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
            e.grain_size = if live { grain.min(256) } else { grain };
        }

        // PSOLA: arbitrary ratio.
        if let Some(e) = self.psola.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // WSOLA: arbitrary ratio.
        if let Some(e) = self.wsola.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // Compute effective formant shift.
        // When linked: formant_semitones cancels out the pitch shift so formants stay in place.
        // When unlinked: formant_semitones is applied as-is.
        let effective_formant_st = if self.formant_linked {
            -self.semitones.clamp(-24.0, 24.0)
        } else {
            self.formant_semitones.clamp(-24.0, 24.0)
        };
        let _formant_ratio = semitones_to_ratio(effective_formant_st);

        // Rubberband: arbitrary ratio + formant control.
        #[cfg(feature = "rubberband")]
        if let Some(e) = self.rubberband.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
            e.preserve_formants = true;
        }
        // formant_scale not yet exposed on RubberbandShifter.

        // Allpass: arbitrary ratio.
        if let Some(e) = self.allpass.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // Spectral: arbitrary ratio.
        if let Some(e) = self.spectral.as_deref_mut() {
            e.speed = ratio;
            e.mix = mix;
        }

        // PolyOctave: snap semitones to nearest octave.
        self.pog.shift = OctaveShift::from_semitones(self.semitones);
        self.pog.mix = self.mix;
    }
}

impl Default for PitchChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for PitchChain {
    fn reset(&mut self) {
        self.divider.reset();
        self.pll.reset();
        if let Some(e) = self.granular.as_deref_mut() {
            e.reset();
        }
        if let Some(e) = self.psola.as_deref_mut() {
            e.reset();
        }
        if let Some(e) = self.wsola.as_deref_mut() {
            e.reset();
        }
        #[cfg(feature = "rubberband")]
        if let Some(e) = self.rubberband.as_deref_mut() {
            e.reset();
        }
        if let Some(e) = self.allpass.as_deref_mut() {
            e.reset();
        }
        self.pog.reset();
        if let Some(e) = self.spectral.as_deref_mut() {
            e.reset();
        }
    }

    fn update(&mut self, config: AudioConfig) {
        let sr = config.sample_rate;
        self.sample_rate = sr;
        self.prepared = true;
        self.divider.update(sr);
        self.pll.update(sr);
        if let Some(e) = self.granular.as_deref_mut() {
            e.update(sr);
        }
        if let Some(e) = self.psola.as_deref_mut() {
            e.update(sr);
        }
        if let Some(e) = self.wsola.as_deref_mut() {
            e.update(sr);
        }
        #[cfg(feature = "rubberband")]
        if let Some(e) = self.rubberband.as_deref_mut() {
            e.update(sr);
        }
        if let Some(e) = self.allpass.as_deref_mut() {
            e.update(sr);
        }
        self.pog.update(sr);
        // A rate whose frames outgrow the engine's (44.1 → 96 kHz): a
        // larger one, here in `update` — preparing, not processing.
        if let Some(e) = self.spectral.as_deref_mut() {
            if e.max_frame() < SpectralShifter::frame_for(2048, sr) {
                let mut bigger = Self::spectral_for(sr);
                bigger.speed = e.speed;
                bigger.mix = e.mix;
                *e = bigger;
            }
        }
        if let Some(e) = self.spectral.as_deref_mut() {
            e.fft_size = if self.live { 1024 } else { 2048 };
            e.update(sr);
        }
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        self.sync_params();

        match self.algorithm {
            Algorithm::FreqDivider => {
                for s in left.iter_mut() {
                    *s = self.divider.tick(*s);
                }
            }
            Algorithm::Pll => {
                for s in left.iter_mut() {
                    *s = self.pll.tick(*s);
                }
            }
            Algorithm::Granular => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.granular.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::Psola => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.psola.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::Wsola => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.wsola.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            // Without the `rubberband` system library, Rubberband falls
            // back to WSOLA (closest arbitrary-ratio pure-Rust quality).
            #[cfg(feature = "rubberband")]
            Algorithm::Rubberband => {
                if let Some(e) = self.rubberband.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            #[cfg(not(feature = "rubberband"))]
            Algorithm::Rubberband => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.wsola.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::Allpass => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.allpass.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
            Algorithm::PolyOctave => {
                for s in left.iter_mut() {
                    *s = self.pog.tick(*s);
                }
            }
            Algorithm::Spectral => {
                // No engine (never selected through `select`): through,
                // rather than build one here.
                if let Some(e) = self.spectral.as_deref_mut() {
                    for s in left.iter_mut() {
                        *s = e.tick(*s);
                    }
                }
            }
        }

        // Copy mono result to right channel.
        right.copy_from_slice(left);
    }
}
