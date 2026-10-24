//! Pro-C-style host processor, including its auxiliary upward/expander stages.
//!
//! Model-specific component composition is available separately in `components`
//! and `model`. This module retains the host's broader control surface.

use crate::{CompressionStyle, Detector, GainCurve, HermiteCubicSmoother, StateFuncHypothesis};
use dsp_core::{Channel, PerChannel, num};

const PARAM_SMOOTHING_MS: f64 = 10.0;

/// Channels this compressor keeps state for.
///
/// Stereo — the plugin is stereo, and a host asking for more folds onto the
/// last slot rather than growing state it will never read.
pub const CHANNELS: usize = 2;

/// Everything one channel remembers between samples.
///
/// These were eight parallel `[_; CHANNELS]` arrays reached by a clamped index
/// at every use. One struct per channel means `reset()` cannot miss a field,
/// adding state is a single edit, and the index is validated once by
/// [`Channel`] instead of at each access.
#[derive(Debug, Clone, Copy)]
struct ChannelState {
    /// Gain reduction as displayed, and as applied.
    last_gr_db: f64,
    last_gr_linear: f64,
    /// Samples left in the release-hold window.
    hold_remaining: usize,
    auto_makeup_db: f64,
    expander_gain_db: f64,
    upward_gain_db: f64,
    /// One-sample history for the character/drive stage.
    drive_previous_input: f64,
    /// One-pole split for the "bright" character mode.
    bright_lowpass: f64,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            last_gr_db: 0.0,
            // Unity, not zero: this is a linear gain, and starting at zero
            // would mute the first sample after a reset.
            last_gr_linear: 1.0,
            hold_remaining: 0,
            auto_makeup_db: 0.0,
            expander_gain_db: 0.0,
            upward_gain_db: 0.0,
            drive_previous_input: 0.0,
            bright_lowpass: 0.0,
        }
    }
}

/// Compressor core used by the plugin chain.
pub struct ProC3Compressor {
    detector: Detector,
    gain_curve: GainCurve,
    hermite_smoother: HermiteCubicSmoother,
    sample_rate: f64,
    channels: PerChannel<ChannelState>,
    crest_peak_power: f64,
    crest_rms_power: f64,
    adaptive_attack_ms: f64,
    adaptive_release_ms: f64,
    smoothed_threshold_db: f64,
    smoothed_ratio: f64,
    smoothed_knee_db: f64,
    smoothed_input_gain_db: f64,
    smoothed_output_gain_db: f64,
    smoothed_fold: f64,
    smoothed_drive: f64,
    smoothed_detector_rms_mix: f64,
    smoothed_expander_threshold_db: f64,
    smoothed_expander_ratio: f64,
    smoothed_upward_threshold_db: f64,
    smoothed_upward_ratio: f64,
    smoothed_ceiling: f64,

    // Core parameters
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub knee_db: f64,
    pub style: i32,

    // I/O parameters
    pub input_gain_db: f64,
    pub output_gain_db: f64,
    pub fold: f64,
    pub range_db: f64,
    pub expander_threshold_db: f64,
    pub expander_ratio: f64,
    pub upward_threshold_db: f64,
    pub upward_ratio: f64,
    pub drive: f64,
    pub character_mode: i32,

    // Parameters owned by the surrounding chain or auxiliary processing paths.
    pub hold_ms: f64,
    pub auto_makeup: bool,
    pub feedback: f64,
    pub channel_link: f64,
    pub detector_rms_mix: f64,
    pub inertia: f64,
    pub inertia_decay: f64,
    pub ceiling: f64,

    // Per-sample math that only changes with the rate or a setting, kept
    // rather than recomputed every sample (bit-identical results).
    /// Upward / expander ballistics, by attack and release time.
    attack_pole: OnePole,
    release_pole: OnePole,
    /// The parameter smoother's, the bright drive's and the crest
    /// detector's coefficients — functions of the sample rate alone.
    param_coeff: f64,
    bright_coeff: f64,
    crest_coeff: f64,
    /// Last `(output gain dB, linear)` and `(gain reduction, dB)`.
    out_gain_memo: (f64, f64),
    gr_db_memo: (f64, f64),
    in_gain_memo: (f64, f64),
}

/// `1 − e^(−1/samples)` for a time in ms, remembered for the last rate and
/// time: the ballistics ask for it every sample, and it changes only when
/// the time or the rate does.
#[derive(Clone, Copy, Debug, Default)]
struct OnePole {
    key: (f64, f64),
    coeff: f64,
}

impl OnePole {
    fn get(&mut self, sample_rate: f64, time_ms: f64) -> f64 {
        if self.key != (sample_rate, time_ms) || self.coeff == 0.0 {
            let samples = (sample_rate * time_ms / 1000.0).max(1.0);
            self.coeff = 1.0 - (-1.0 / samples).exp();
            self.key = (sample_rate, time_ms);
        }
        self.coeff
    }
}

impl ProC3Compressor {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut comp = Self {
            detector: Detector::new(),
            gain_curve: GainCurve::new(sample_rate),
            hermite_smoother: HermiteCubicSmoother::new(StateFuncHypothesis::Identity),
            sample_rate,
            channels: PerChannel::filled(ChannelState::default()),
            crest_peak_power: 0.0,
            crest_rms_power: 0.0,
            adaptive_attack_ms: 10.0,
            adaptive_release_ms: 50.0,
            smoothed_threshold_db: -20.0,
            smoothed_ratio: 4.0,
            smoothed_knee_db: 2.0,
            smoothed_input_gain_db: 0.0,
            smoothed_output_gain_db: 0.0,
            smoothed_fold: 1.0,
            smoothed_drive: 0.0,
            smoothed_detector_rms_mix: 0.0,
            smoothed_expander_threshold_db: -80.0,
            smoothed_expander_ratio: 1.0,
            smoothed_upward_threshold_db: -60.0,
            smoothed_upward_ratio: 1.0,
            smoothed_ceiling: 0.0,
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 50.0,
            knee_db: 2.0,
            style: 0,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            fold: 1.0,
            range_db: 60.0,
            expander_threshold_db: -80.0,
            expander_ratio: 1.0,
            upward_threshold_db: -60.0,
            upward_ratio: 1.0,
            drive: 0.0,
            character_mode: 0,
            hold_ms: 0.0,
            auto_makeup: false,
            feedback: 0.0,
            channel_link: 1.0,
            detector_rms_mix: 0.0,
            inertia: 0.0,
            inertia_decay: 0.0,
            ceiling: 0.0,
            attack_pole: OnePole::default(),
            release_pole: OnePole::default(),
            param_coeff: 0.0,
            bright_coeff: 0.0,
            crest_coeff: 0.0,
            out_gain_memo: (f64::NAN, 1.0),
            gr_db_memo: (f64::NAN, 0.0),
            in_gain_memo: (f64::NAN, 1.0),
        };
        comp.rate_coeffs();
        comp
    }

    /// The coefficients that depend on the sample rate alone.
    fn rate_coeffs(&mut self) {
        let samples = (self.sample_rate * PARAM_SMOOTHING_MS / 1000.0).max(1.0);
        self.param_coeff = 1.0 - (-1.0 / samples).exp();
        self.bright_coeff = 1.0 - (-2.0 * std::f64::consts::PI * 8_000.0 / self.sample_rate).exp();
        self.crest_coeff = (-1.0 / (self.sample_rate * 0.2)).exp();
    }

    /// Process a mono sample through the compressor core.
    pub fn process(&mut self, input: f64, channel: usize) -> f64 {
        // Step 0: Apply input gain
        self.smoothed_input_gain_db =
            self.smooth_parameter(self.smoothed_input_gain_db, self.input_gain_db);
        let gain_db = self.smoothed_input_gain_db;
        if self.in_gain_memo.0.to_bits() != gain_db.to_bits() {
            self.in_gain_memo = (gain_db, dsp_core::audio::db::db_to_linear(gain_db));
        }
        let input_linear = input * self.in_gain_memo.1;
        self.smoothed_detector_rms_mix =
            self.smooth_parameter(self.smoothed_detector_rms_mix, self.detector_rms_mix);

        // Step 1: detect level
        let level_db = self
            .detector
            .detect_level_with_rms_mix(input_linear.abs(), self.smoothed_detector_rms_mix);

        self.process_with_level(input_linear, level_db, channel)
    }

    /// Process a sample with an externally computed sidechain level.
    ///
    /// This lets the chain share a linked/filtered stereo detector while keeping
    /// per-channel gain smoothing and metering state.
    pub fn process_with_level(&mut self, input_linear: f64, level_db: f64, channel: usize) -> f64 {
        // Fold onto the stereo width first, so a host asking for channel 5
        // lands on the slot it always did.
        let channel = Channel::new(channel.min(CHANNELS - 1));
        self.smooth_auxiliary_params();

        // Step 2: COMPUTE GAIN REDUCTION
        // Apply threshold/ratio/knee in log domain
        self.smooth_gain_computer_params();
        self.update_program_dependent_ballistics(level_db);
        let mut gr_instant = self.gain_curve.compute_gr(level_db);

        // Hold freezes release for a short window after gain reduction deepens.
        // This keeps short gaps from pumping the detector while leaving attacks
        // fully responsive.
        if gr_instant < self.channels[channel].last_gr_linear {
            self.channels[channel].hold_remaining = self.hold_samples();
        } else if self.channels[channel].hold_remaining > 0 {
            gr_instant = self.channels[channel].last_gr_linear;
            self.channels[channel].hold_remaining =
                self.channels[channel].hold_remaining.saturating_sub(1);
        }

        // Step 3: SMOOTH GAIN REDUCTION WITH HERMITE CUBIC
        // Reference-informed smoothing:
        // - Use attack/release coefficients
        // - Compare gr_inst with prior GR history values
        // - Detect change with a small relative threshold
        let gr_smoothed = self.hermite_smoother.process(
            gr_instant,
            self.gain_curve.attack_coeff,
            self.gain_curve.release_coeff,
            channel,
        );

        // Step 4: APPLY TO AUDIO
        let mut output = input_linear * gr_smoothed;
        // 0 dB is exactly unity: skip the pow when a stage is idle.
        let upward_db = self.process_upward(level_db, channel);
        if upward_db != 0.0 {
            output *= dsp_core::audio::db::db_to_linear(upward_db);
        }
        let expander_db = self.process_expander(level_db, channel);
        if expander_db != 0.0 {
            output *= dsp_core::audio::db::db_to_linear(expander_db);
        }

        // Step 5: OUTPUT GAIN
        if self.gr_db_memo.0.to_bits() != gr_smoothed.to_bits() {
            self.gr_db_memo = (
                gr_smoothed,
                -dsp_core::audio::db::linear_to_db(gr_smoothed.max(1e-10)).min(0.0),
            );
        }
        let gr_db = self.gr_db_memo.1;
        self.channels[channel].auto_makeup_db = if self.auto_makeup {
            // Use half of the current reduction for conservative gain matching.
            // Full compensation tends to over-brighten and overload transients.
            0.995f64.mul_add(
                self.channels[channel].auto_makeup_db,
                0.005 * (gr_db * 0.5).min(24.0),
            )
        } else {
            0.995 * self.channels[channel].auto_makeup_db
        };
        self.smoothed_output_gain_db =
            self.smooth_parameter(self.smoothed_output_gain_db, self.output_gain_db);
        let gain_db = self.smoothed_output_gain_db + self.channels[channel].auto_makeup_db;
        if self.out_gain_memo.0.to_bits() != gain_db.to_bits() {
            self.out_gain_memo = (gain_db, dsp_core::audio::db::db_to_linear(gain_db));
        }
        let output_gain = self.out_gain_memo.1;
        output *= output_gain;
        output = self.apply_drive(output, channel);

        // Step 6: SOFT CEILING (optional)
        if self.smoothed_ceiling > 0.0 {
            output = (output / self.smoothed_ceiling).tanh() * self.smoothed_ceiling;
        }

        // Step 7: PARALLEL COMPRESSION (fold parameter)
        let compressed = output;
        self.smoothed_fold = self.smooth_parameter(self.smoothed_fold, self.fold);
        output = compressed * self.smoothed_fold + input_linear * (1.0 - self.smoothed_fold);

        // Track GR for metering
        self.channels[channel].last_gr_linear = gr_smoothed;
        self.channels[channel].last_gr_db = gr_db;

        output
    }

    /// Update to new sample rate
    pub fn update(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 0.1 {
            self.sample_rate = sample_rate;
            self.rate_coeffs();
            self.detector.update_sample_rate(sample_rate);
            self.gain_curve = GainCurve::new(sample_rate);
            self.hermite_smoother.reset();
            // Re-apply current parameters
            self.set_threshold(self.threshold_db);
            self.set_ratio(self.ratio);
            self.set_knee(self.knee_db);
            self.set_attack_ms(self.attack_ms);
            self.set_release_ms(self.release_ms);
        }
    }

    /// Gain reduction on one channel, in dB.
    #[must_use]
    pub fn gain_reduction_db_for(&self, channel: Channel) -> f64 {
        self.channels[channel].last_gr_db
    }

    /// Get current gain reduction in dB
    #[must_use]
    pub fn gain_reduction_db(&self) -> f64 {
        self.channels[Channel::LEFT]
            .last_gr_db
            .max(self.channels[Channel::RIGHT].last_gr_db)
    }

    /// Set threshold in dB
    pub const fn set_threshold(&mut self, threshold_db: f64) {
        self.threshold_db = threshold_db;
        self.gain_curve.threshold_db = threshold_db;
    }

    /// Set ratio (e.g., 4.0 = 4:1)
    pub const fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio;
        self.gain_curve.ratio = ratio;
    }

    /// Set knee width in dB
    pub const fn set_knee(&mut self, knee_db: f64) {
        self.knee_db = knee_db;
        self.gain_curve.knee_db = knee_db;
    }

    /// Set maximum gain-reduction range in dB.
    pub const fn set_range_db(&mut self, range_db: f64) {
        self.range_db = range_db.clamp(0.0, 120.0);
        self.gain_curve.range_db = self.range_db;
    }

    fn hold_samples(&self) -> usize {
        num::f64_to_index((self.hold_ms.max(0.0) * self.sample_rate / 1000.0).round())
    }

    fn smooth_parameter(&self, current: f64, target: f64) -> f64 {
        if (current - target).abs() <= 1e-12 {
            return target;
        }

        (target - current).mul_add(self.param_coeff, current)
    }

    fn smooth_gain_computer_params(&mut self) {
        self.smoothed_threshold_db =
            self.smooth_parameter(self.smoothed_threshold_db, self.threshold_db);
        self.smoothed_ratio = self.smooth_parameter(self.smoothed_ratio, self.ratio);
        self.smoothed_knee_db = self.smooth_parameter(self.smoothed_knee_db, self.knee_db);

        self.gain_curve.threshold_db = self.smoothed_threshold_db;
        self.gain_curve.ratio = self.smoothed_ratio;
        self.gain_curve.knee_db = self.smoothed_knee_db;
    }

    fn smooth_auxiliary_params(&mut self) {
        self.smoothed_expander_threshold_db = self.smooth_parameter(
            self.smoothed_expander_threshold_db,
            self.expander_threshold_db,
        );
        self.smoothed_expander_ratio =
            self.smooth_parameter(self.smoothed_expander_ratio, self.expander_ratio);
        self.smoothed_upward_threshold_db =
            self.smooth_parameter(self.smoothed_upward_threshold_db, self.upward_threshold_db);
        self.smoothed_upward_ratio =
            self.smooth_parameter(self.smoothed_upward_ratio, self.upward_ratio);
        self.smoothed_ceiling = self.smooth_ceiling_parameter(self.smoothed_ceiling, self.ceiling);
    }

    fn smooth_ceiling_parameter(&self, current: f64, target: f64) -> f64 {
        if target <= 1e-6 {
            return 0.0;
        }
        if current <= 1e-6 {
            return target;
        }
        self.smooth_parameter(current, target)
    }

    fn process_expander(&mut self, level_db: f64, channel: Channel) -> f64 {
        let ratio = self.smoothed_expander_ratio.clamp(1.0, 20.0);
        let target_db = if ratio <= 1.0001 || level_db >= self.smoothed_expander_threshold_db {
            0.0
        } else {
            let below_threshold = self.smoothed_expander_threshold_db - level_db;
            (below_threshold * (1.0 - ratio)).max(-120.0)
        };

        // Off and settled: nothing to move (the result is exactly 0 dB).
        if target_db == 0.0 && self.channels[channel].expander_gain_db == 0.0 {
            return 0.0;
        }
        let moving_deeper = target_db < self.channels[channel].expander_gain_db;
        let coeff = if moving_deeper {
            self.attack_pole
                .get(self.sample_rate, self.attack_ms.max(0.1))
        } else {
            self.release_pole
                .get(self.sample_rate, self.release_ms.max(1.0))
        };
        self.channels[channel].expander_gain_db +=
            (target_db - self.channels[channel].expander_gain_db) * coeff;
        self.channels[channel].expander_gain_db
    }

    fn process_upward(&mut self, level_db: f64, channel: Channel) -> f64 {
        let ratio = self.smoothed_upward_ratio.clamp(1.0, 20.0);
        let target_db = if ratio <= 1.0001 || level_db >= self.smoothed_upward_threshold_db {
            0.0
        } else {
            let below_threshold = self.smoothed_upward_threshold_db - level_db;
            (below_threshold * (1.0 - 1.0 / ratio)).min(36.0)
        };

        if target_db == 0.0 && self.channels[channel].upward_gain_db == 0.0 {
            return 0.0;
        }
        let moving_louder = target_db > self.channels[channel].upward_gain_db;
        let coeff = if moving_louder {
            self.attack_pole
                .get(self.sample_rate, self.attack_ms.max(0.1))
        } else {
            self.release_pole
                .get(self.sample_rate, self.release_ms.max(1.0))
        };
        self.channels[channel].upward_gain_db +=
            (target_db - self.channels[channel].upward_gain_db) * coeff;
        self.channels[channel].upward_gain_db
    }

    fn apply_drive(&mut self, sample: f64, channel: Channel) -> f64 {
        self.smoothed_drive = self.smooth_parameter(self.smoothed_drive, self.drive);
        let drive = self.smoothed_drive.clamp(0.0, 1.0);
        if drive <= 1e-6 {
            self.channels[channel].drive_previous_input = sample;
            return sample;
        }

        let pre_gain = 1.0 + drive * 11.0;
        let mode = self.character_mode.clamp(0, 6);
        if mode == 3 {
            return self.apply_bright_drive(sample, pre_gain, channel);
        }

        let previous = self.channels[channel].drive_previous_input;
        self.channels[channel].drive_previous_input = sample;

        let normalization = Self::drive_transfer_raw(pre_gain, mode).abs().max(1e-9);
        let delta = sample - previous;

        if delta.abs() <= 1e-8 {
            return Self::drive_transfer(sample, pre_gain, normalization, mode);
        }

        (Self::drive_antiderivative(sample, pre_gain, normalization, mode)
            - Self::drive_antiderivative(previous, pre_gain, normalization, mode))
            / delta
    }

    fn apply_bright_drive(&mut self, sample: f64, pre_gain: f64, channel: Channel) -> f64 {
        let coeff = self.bright_coeff;
        self.channels[channel].bright_lowpass +=
            (sample - self.channels[channel].bright_lowpass) * coeff;

        let low = self.channels[channel].bright_lowpass;
        let high = sample - low;
        let previous_high = self.channels[channel].drive_previous_input;
        self.channels[channel].drive_previous_input = high;

        let normalization = Self::drive_transfer_raw(pre_gain, 0).abs().max(1e-9);
        let delta = high - previous_high;
        let saturated_high = if delta.abs() <= 1e-8 {
            Self::drive_transfer(high, pre_gain, normalization, 0)
        } else {
            (Self::drive_antiderivative(high, pre_gain, normalization, 0)
                - Self::drive_antiderivative(previous_high, pre_gain, normalization, 0))
                / delta
        };

        low + saturated_high
    }

    fn drive_transfer(sample: f64, pre_gain: f64, normalization: f64, mode: i32) -> f64 {
        Self::drive_transfer_raw(sample * pre_gain, mode) / normalization
    }

    fn drive_transfer_raw(x: f64, mode: i32) -> f64 {
        match mode {
            1 => x.atan(),
            2 => x / (1.0 + x.abs()),
            4 => cubic_drive_raw(x),
            5 => x.clamp(-1.0, 1.0),
            6 if x < 0.0 => 0.75 * x.tanh(),
            _ => x.tanh(),
        }
    }

    fn drive_antiderivative(sample: f64, pre_gain: f64, normalization: f64, mode: i32) -> f64 {
        let x = sample * pre_gain;
        let integral = match mode {
            1 => x.mul_add(x.atan(), -(0.5 * (x * x).ln_1p())),
            2 if x >= 0.0 => x - x.ln_1p(),
            2 => -x - (1.0 - x).ln(),
            4 => cubic_drive_antiderivative_raw(x),
            5 => hard_clip_antiderivative_raw(x),
            6 if x < 0.0 => 0.75 * stable_log_cosh(x),
            _ => stable_log_cosh(x),
        };
        integral / (pre_gain * normalization)
    }

    fn update_program_dependent_ballistics(&mut self, level_db: f64) {
        let amount = self.inertia.abs().clamp(0.0, 1.0);
        if amount <= 1e-6 {
            return;
        }

        let amp = dsp_core::audio::db::db_to_linear(level_db).clamp(0.0, 16.0);
        let power = amp * amp;
        let coeff = self.crest_coeff;
        self.crest_peak_power = power.max(coeff * self.crest_peak_power + (1.0 - coeff) * power);
        self.crest_rms_power = coeff * self.crest_rms_power + (1.0 - coeff) * power;

        let crest = (self.crest_peak_power / self.crest_rms_power.max(1e-12)).clamp(1.0, 64.0);
        let transient = ((crest.sqrt() - 1.0) / 7.0).clamp(0.0, 1.0);

        let target_attack =
            self.attack_ms * (amount * 0.85).mul_add(-transient, 1.0).clamp(0.05, 2.0);
        let release_shape = (1.0 + amount * 1.5f64.mul_add(transient, -0.4)).clamp(0.25, 4.0);
        let style_shape =
            Self::style_auto_release_multiplier(CompressionStyle::from_id(self.style), transient);
        let target_release = self.release_ms * (release_shape * style_shape).clamp(0.1, 6.0);

        let smoothing = self.inertia_decay.clamp(0.0, 0.999);
        self.adaptive_attack_ms =
            smoothing * self.adaptive_attack_ms + (1.0 - smoothing) * target_attack;
        self.adaptive_release_ms =
            smoothing * self.adaptive_release_ms + (1.0 - smoothing) * target_release;

        self.gain_curve
            .set_time_constants_ms(self.adaptive_attack_ms, self.adaptive_release_ms);
    }

    fn style_auto_release_multiplier(style: CompressionStyle, transient: f64) -> f64 {
        let transient = transient.clamp(0.0, 1.0);
        let sustained = 1.0 - transient;
        match style {
            CompressionStyle::Fet => 0.45f64.mul_add(-transient, 1.0),
            CompressionStyle::Vca => 0.15f64.mul_add(sustained, 0.2f64.mul_add(-transient, 1.0)),
            CompressionStyle::Optical => 0.9f64.mul_add(sustained, 0.35f64.mul_add(transient, 1.0)),
            _ => 1.0,
        }
        .clamp(0.25, 2.5)
    }

    /// Set attack time in milliseconds
    pub fn set_attack_ms(&mut self, attack_ms: f64) {
        // CRITICAL: Only reset Hermite smoother if value ACTUALLY changed!
        // We're called every sample from sync_params, so must check before reset
        if (self.attack_ms - attack_ms).abs() > 1e-6 {
            self.hermite_smoother.reset();
        }
        self.attack_ms = attack_ms;
        self.adaptive_attack_ms = attack_ms.max(0.1);
        self.gain_curve.set_attack_ms(attack_ms);
    }

    /// Set release time in milliseconds
    pub fn set_release_ms(&mut self, release_ms: f64) {
        // CRITICAL: Only reset Hermite smoother if value ACTUALLY changed!
        // We're called every sample from sync_params, so must check before reset
        if (self.release_ms - release_ms).abs() > 1e-6 {
            self.hermite_smoother.reset();
        }
        self.release_ms = release_ms;
        self.adaptive_release_ms = release_ms.max(0.1);
        self.gain_curve.set_release_ms(release_ms);
    }

    /// Set attack time in seconds (for compatibility)
    pub fn set_attack(&mut self, attack_s: f64) {
        self.set_attack_ms(attack_s * 1000.0);
    }

    /// Set release time in seconds (for compatibility)
    pub fn set_release(&mut self, release_s: f64) {
        self.set_release_ms(release_s * 1000.0);
    }

    /// Set parallel compression fold parameter (0-1)
    pub const fn set_fold(&mut self, fold: f64) {
        self.fold = fold.clamp(0.0, 1.0);
    }

    /// Set compression style (affects attack/release response)
    pub fn set_style(&mut self, style_id: i32) {
        let style = CompressionStyle::from_id(style_id);
        self.style = style.id();
        self.gain_curve.set_style(style);
    }

    /// Reset internal state
    pub fn reset(&mut self) {
        self.detector.reset();
        self.hermite_smoother.reset();
        self.channels.fill(ChannelState::default());
        self.crest_peak_power = 0.0;
        self.crest_rms_power = 0.0;
        self.adaptive_attack_ms = self.attack_ms;
        self.adaptive_release_ms = self.release_ms;
        self.smoothed_threshold_db = self.threshold_db;
        self.smoothed_ratio = self.ratio;
        self.smoothed_knee_db = self.knee_db;
        self.smoothed_input_gain_db = self.input_gain_db;
        self.smoothed_output_gain_db = self.output_gain_db;
        self.smoothed_fold = self.fold;
        self.smoothed_drive = self.drive;
        self.smoothed_detector_rms_mix = self.detector_rms_mix;
        self.smoothed_expander_threshold_db = self.expander_threshold_db;
        self.smoothed_expander_ratio = self.expander_ratio;
        self.smoothed_upward_threshold_db = self.upward_threshold_db;
        self.smoothed_upward_ratio = self.upward_ratio;
        self.smoothed_ceiling = self.ceiling;
    }
}

fn stable_log_cosh(x: f64) -> f64 {
    let abs_x = x.abs();
    if abs_x > 20.0 {
        abs_x - std::f64::consts::LN_2
    } else {
        x.cosh().ln()
    }
}

fn cubic_drive_raw(x: f64) -> f64 {
    if x <= -1.0 {
        -2.0 / 3.0
    } else if x >= 1.0 {
        2.0 / 3.0
    } else {
        x - x.powi(3) / 3.0
    }
}

fn cubic_drive_antiderivative_raw(x: f64) -> f64 {
    if x <= -1.0 {
        (-2.0_f64 / 3.0).mul_add(x, -0.25)
    } else if x >= 1.0 {
        (2.0_f64 / 3.0).mul_add(x, -0.25)
    } else {
        (0.5 * x).mul_add(x, -(x.powi(4) / 12.0))
    }
}

fn hard_clip_antiderivative_raw(x: f64) -> f64 {
    if x <= -1.0 {
        -x - 0.5
    } else if x >= 1.0 {
        x - 0.5
    } else {
        0.5 * x * x
    }
}
