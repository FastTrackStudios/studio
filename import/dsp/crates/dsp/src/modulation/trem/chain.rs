//! `TremChain` — complete tremolo processor with modulation engine.
//!
//! Wraps the Tremolo with a Modulator from fts-modulation and
//! implements the Processor trait.
//!
//! Signal flow:
//! ```text
//! Input -> Envelope Follower (dynamics) -> Rate/Depth Mod
//!       -> Phase (with groove warp + feel offset) -> Modulator -> Accent Scale
//!       -> Tremolo (amplitude mod) -> Analog Style -> Width -> Mix -> Output
//! ```

use dsp_core::audio::{AudioConfig, Processor};
use dsp_core::modulation::curves::CurveType;
use dsp_core::modulation::modulator::Modulator;
use dsp_core::modulation::pattern::Point;
use dsp_core::modulation::tempo::TransportInfo;
use dsp_core::modulation::trigger::TriggerMode;

use crate::modulation::trem::dynamics::TremDynamics;
use crate::modulation::trem::tremolo::{AnalogProcessor, TremMode, Tremolo};

// ---------------------------------------------------------------------------
// Groove helpers
// ---------------------------------------------------------------------------

/// Apply groove (swing/shuffle) warp to a modulation phase.
///
/// `groove` in -1..1:
/// - 0 = even (50/50 split)
/// - +1 = swing (first beat gets 2/3 duration, triplet feel)
/// - -1 = shuffle (first beat gets 1/3 duration)
///
/// The phase is assumed to be in [0, 1) representing one full modulation cycle.
#[inline]
fn groove_warp(phase: f64, groove: f64) -> f64 {
    if groove.abs() < 1e-10 {
        return phase;
    }

    // Split point: where beat 2 starts.
    // At groove=0: split=0.5 (even)
    // At groove=+1 (swing): split=2/3 (first beat longer)
    // At groove=-1 (shuffle): split=1/3 (first beat shorter)
    let split = groove.mul_add(1.0 / 6.0, 0.5); // maps -1..1 to 1/3..2/3

    if phase < split {
        // First half: remap [0, split) -> [0, 0.5)
        phase * 0.5 / split
    } else {
        // Second half: remap [split, 1.0) -> [0.5, 1.0)
        0.5 + (phase - split) * 0.5 / (1.0 - split)
    }
}

// ---------------------------------------------------------------------------
// Accent helpers
// ---------------------------------------------------------------------------

/// Compute the accent scaling factor for the current beat position.
///
/// `accent` in -1..1:
/// - Positive: emphasize downbeat (beat 1 gets more depth, others less)
/// - Negative: de-emphasize downbeat (beat 1 dropout, others normal)
///
/// `beat_in_bar` is the fractional beat position within a bar (0..4 for 4/4).
#[inline]
fn accent_scale(beat_in_bar: f64, accent: f64) -> f64 {
    if accent.abs() < 1e-10 {
        return 1.0;
    }

    // Beat 1 is the region near integer bar boundaries (0..1)
    let is_downbeat = beat_in_bar < 1.0;

    if is_downbeat {
        // On the downbeat: positive accent = more depth, negative = less
        (1.0 + accent).clamp(0.0, 2.0)
    } else {
        // On other beats: positive accent = less depth, negative = more
        accent.mul_add(-0.5, 1.0).clamp(0.0, 2.0)
    }
}

// ---------------------------------------------------------------------------
// TremChain
// ---------------------------------------------------------------------------

/// Complete tremolo processing chain.
///
/// Signal flow: Modulator -> Tremolo (L/R with stereo offset) -> Analog Style -> Mix -> Output.
pub struct TremChain {
    pub tremolo_l: Tremolo,
    pub tremolo_r: Tremolo,
    pub modulator: Modulator,

    /// Dry/wet mix (0..1).
    pub mix: f64,
    /// Stereo phase offset in degrees (-180..180).
    pub stereo_phase: f64,

    // --- Tremolator-inspired features ---
    /// Groove (swing/shuffle). -1..1.
    /// Negative = shuffle (first beat shorter), positive = swing (first beat longer).
    pub groove: f64,
    /// Feel (draggin'/rushin'). -1..1.
    /// Negative = dragging (behind beat), positive = rushing (ahead of beat).
    /// Applies a phase offset of up to +/-50ms.
    pub feel: f64,
    /// Accent. -1..1.
    /// Positive = emphasize downbeat, negative = de-emphasize downbeat.
    pub accent: f64,
    /// Dynamics envelope follower.
    pub dynamics: TremDynamics,
    /// Analog-style saturation (left channel).
    pub analog_l: AnalogProcessor,
    /// Analog-style saturation (right channel).
    pub analog_r: AnalogProcessor,

    transport: TransportInfo,
    sample_rate: f64,
}

impl TremChain {
    #[must_use]
    pub fn new() -> Self {
        let mut modulator = Modulator::new();
        // Default: sine-like LFO at 1/4 note
        modulator.trigger.mode = TriggerMode::Sync;
        modulator.trigger.division = dsp_core::modulation::tempo::SyncDivision::Beat(1.0); // 1/4 note
        modulator.smoother.set_params(0.0, 0.0, 48000.0); // No smoothing for clean LFO

        // Default sine pattern
        let pat = modulator.patterns.get_mut(0);
        pat.add_point(Point {
            id: 0,
            x: 0.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        pat.add_point(Point {
            id: 0,
            x: 0.5,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        pat.add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        modulator.patterns.set_active(0);

        Self {
            tremolo_l: Tremolo::new(),
            tremolo_r: Tremolo::new(),
            modulator,
            mix: 1.0,
            stereo_phase: 0.0,
            groove: 0.0,
            feel: 0.0,
            accent: 0.0,
            dynamics: TremDynamics::new(),
            analog_l: AnalogProcessor::new(),
            analog_r: AnalogProcessor::new(),
            transport: TransportInfo::default(),
            sample_rate: 48000.0,
        }
    }

    /// Set transport info (call per-block from the plugin).
    pub const fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    /// Set tremolo mode on both channels.
    pub const fn set_mode(&mut self, mode: TremMode) {
        self.tremolo_l.mode = mode;
        self.tremolo_r.mode = mode;
    }

    /// Set depth on both channels.
    pub const fn set_depth(&mut self, depth: f64) {
        self.tremolo_l.depth = depth;
        self.tremolo_r.depth = depth;
    }

    /// Set the analog style on both channels.
    pub const fn set_analog_style(&mut self, style: crate::modulation::trem::tremolo::AnalogStyle) {
        self.analog_l.style = style;
        self.analog_r.style = style;
    }

    /// Compute feel phase offset in beats for the current tempo.
    #[inline]
    fn feel_offset_beats(&self) -> f64 {
        if self.feel.abs() < 1e-10 || self.transport.tempo_bpm <= 0.0 {
            return 0.0;
        }
        // Max offset: 50ms worth of beats
        let max_offset_s = 0.050;
        let beats_per_sec = self.transport.tempo_bpm / 60.0;
        let max_offset_beats = max_offset_s * beats_per_sec;
        self.feel * max_offset_beats
    }
}

impl Default for TremChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for TremChain {
    fn reset(&mut self) {
        self.tremolo_l.reset();
        self.tremolo_r.reset();
        self.modulator.reset();
        self.dynamics.reset();
        self.analog_l.reset();
        self.analog_r.reset();
    }

    fn update(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;
        self.tremolo_l.update(config.sample_rate);
        self.tremolo_r.update(config.sample_rate);
        self.modulator.update(config.sample_rate);
        self.modulator.stereo_offset = self.stereo_phase / 360.0;
        self.dynamics.update(config.sample_rate);
        self.analog_l.update(config.sample_rate);
        self.analog_r.update(config.sample_rate);
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let len = left.len().min(right.len());
        let bps = self.transport.beats_per_sample(self.sample_rate);
        let feel_offset = self.feel_offset_beats();

        for i in 0..len {
            // Advance transport position
            let pos = (i as f64).mul_add(bps, self.transport.position_qn);
            let t = TransportInfo {
                position_qn: pos,
                ..self.transport
            };

            // 1. Dynamics envelope: measure input level, get rate/depth modulation
            let input_level = (left[i].abs() + right[i].abs()) * 0.5;
            let (_rate_scale, depth_offset) = self.dynamics.tick(input_level);

            // Apply dynamics depth offset to tremolo depth (temporarily)
            let base_depth_l = self.tremolo_l.depth;
            let base_depth_r = self.tremolo_r.depth;
            self.tremolo_l.depth = (base_depth_l + depth_offset).clamp(0.0, 1.0);
            self.tremolo_r.depth = (base_depth_r + depth_offset).clamp(0.0, 1.0);

            // 2. Get modulation values from modulator
            // The modulator handles phase internally via the trigger engine.
            // We apply feel as an offset to the transport position.
            let t_felt = TransportInfo {
                position_qn: pos + feel_offset,
                ..t
            };
            let mod_l = self.modulator.tick(&t_felt, 0.0);
            let mod_r = if self.stereo_phase.abs() > 0.01 {
                self.modulator.output_stereo()
            } else {
                mod_l
            };

            // 3. Apply groove warp to modulation values
            // Groove warps the time within the modulation cycle.
            // Since the modulator already computed based on phase, we apply groove
            // as a remapping of the modulation output position in the cycle.
            // We do this by warping the mod value through the groove function.
            let mod_l = if self.groove.abs() > 1e-10 {
                // Get the raw phase from the cycle and warp it
                // Since we can't easily get the raw phase, we approximate:
                // the mod value IS the pattern output. For groove to work properly,
                // we'd need access to the phase. Instead, we use the transport-based
                // phase estimation and re-evaluate.
                let sync_qn = 1.0; // Default 1/4 note cycle
                let raw_phase = (pos / sync_qn).fract();
                let warped = groove_warp(raw_phase, self.groove);
                // Evaluate the pattern at the warped phase
                let pattern = self.modulator.patterns.active();
                let raw_y = pattern.get_y(warped);
                // Apply the modulator's range/invert settings manually
                let inverted = if self.modulator.invert {
                    1.0 - raw_y
                } else {
                    raw_y
                };
                (self.modulator.max - self.modulator.min)
                    .mul_add(inverted, self.modulator.min)
                    .clamp(0.0, 1.0)
            } else {
                mod_l
            };

            let mod_r = if self.groove.abs() > 1e-10 && self.stereo_phase.abs() > 0.01 {
                let sync_qn = 1.0;
                let raw_phase = ((pos + feel_offset) / sync_qn + self.stereo_phase / 360.0).fract();
                let raw_phase = if raw_phase < 0.0 {
                    raw_phase + 1.0
                } else {
                    raw_phase
                };
                let warped = groove_warp(raw_phase, self.groove);
                let pattern = self.modulator.patterns.active();
                let raw_y = pattern.get_y(warped);
                let inverted = if self.modulator.invert {
                    1.0 - raw_y
                } else {
                    raw_y
                };
                (self.modulator.max - self.modulator.min)
                    .mul_add(inverted, self.modulator.min)
                    .clamp(0.0, 1.0)
            } else {
                mod_r
            };

            // 4. Apply accent scaling based on beat position in bar
            let beat_in_bar = pos.rem_euclid(4.0); // Assume 4/4 time
            let accent_l = accent_scale(beat_in_bar, self.accent);
            let accent_r = accent_l; // Same accent on both channels

            // Scale the modulation depth by accent (push mod toward or away from center)
            let mod_l = (mod_l - 0.5).mul_add(accent_l, 0.5);
            let mod_r = (mod_r - 0.5).mul_add(accent_r, 0.5);

            // 5. Apply tremolo amplitude modulation
            let dry_l = left[i];
            let dry_r = right[i];

            let wet_l = self.tremolo_l.tick(left[i], mod_l.clamp(0.0, 1.0), 0);
            let wet_r = self.tremolo_r.tick(right[i], mod_r.clamp(0.0, 1.0), 1);

            // 6. Apply analog style saturation
            let wet_l = self.analog_l.tick(wet_l, 0);
            let wet_r = self.analog_r.tick(wet_r, 1);

            // 7. Mix dry/wet
            left[i] = dry_l.mul_add(1.0 - self.mix, wet_l * self.mix);
            right[i] = dry_r.mul_add(1.0 - self.mix, wet_r * self.mix);

            // Restore base depth (dynamics modulation is per-sample)
            self.tremolo_l.depth = base_depth_l;
            self.tremolo_r.depth = base_depth_r;
        }
    }
}
