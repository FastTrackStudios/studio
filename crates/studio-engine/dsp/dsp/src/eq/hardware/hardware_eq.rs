//! Original hardware-style EQ models for the first FTS-EQ profile set.
//!
//! These models use FTS' existing filter designer and profile-specific
//! cascades. They do not port reference implementations; calibration targets
//! can refine the constants without changing the public settings shape.

use crate::eq::design::biquad::Coeffs;
use crate::eq::design::{self, FilterType};
use crate::eq::hardware::calibration::{
    CalibratedScalar, CalibrationParameters, FitOptions, FitReport, ResponseTarget, fit_response,
};
use crate::eq::hardware::neve_1073::apply_gain_compensated_arctan;
use crate::eq::model::{Coloration, ColorationPlacement, ModelProcessor, PreparedModel};
use crate::eq::pultec_color::PultecColoration;
use crate::eq::runtime::response::compute_magnitude_response;

#[derive(Debug, Clone, PartialEq)]
pub struct HardwareEqCalibration {
    pub pultec_low_overshoot_factor: f64,
    pub pultec_low_overshoot_step: f64,
    pub pultec_low_atten_scale: f64,
    pub pultec_low_atten_step: f64,
    pub pultec_high_air_factor: f64,
    pub pultec_high_air_step: f64,
    pub pultec_high_atten_scale: f64,
    pub pultec_high_atten_step: f64,
    pub api_low_overshoot_factor: f64,
    pub api_low_overshoot_step: f64,
    pub api_mid_skirt_factor: f64,
    pub api_mid_skirt_step: f64,
    pub api_high_presence_factor: f64,
    pub api_high_presence_step: f64,
    pub ssl_e_skirt_factor: f64,
    pub ssl_e_skirt_step: f64,
    pub ssl_g_skirt_factor: f64,
    pub ssl_g_skirt_step: f64,
}

impl Default for HardwareEqCalibration {
    fn default() -> Self {
        Self {
            pultec_low_overshoot_factor: -0.16,
            pultec_low_overshoot_step: 0.025,
            pultec_low_atten_scale: -0.78,
            pultec_low_atten_step: 0.035,
            pultec_high_air_factor: 0.18,
            pultec_high_air_step: 0.025,
            pultec_high_atten_scale: -0.85,
            pultec_high_atten_step: 0.035,
            api_low_overshoot_factor: -0.08,
            api_low_overshoot_step: 0.02,
            api_mid_skirt_factor: -0.05,
            api_mid_skirt_step: 0.015,
            api_high_presence_factor: 0.07,
            api_high_presence_step: 0.015,
            ssl_e_skirt_factor: 0.055,
            ssl_e_skirt_step: 0.01,
            ssl_g_skirt_factor: 0.035,
            ssl_g_skirt_step: 0.01,
        }
    }
}

impl CalibrationParameters for HardwareEqCalibration {
    fn len(&self) -> usize {
        9
    }

    fn scalar(&self, index: usize) -> CalibratedScalar {
        match index {
            0 => CalibratedScalar::new(
                self.pultec_low_overshoot_factor,
                -0.5,
                0.2,
                self.pultec_low_overshoot_step,
            ),
            1 => CalibratedScalar::new(
                self.pultec_low_atten_scale,
                -1.3,
                -0.3,
                self.pultec_low_atten_step,
            ),
            2 => CalibratedScalar::new(
                self.pultec_high_air_factor,
                -0.1,
                0.45,
                self.pultec_high_air_step,
            ),
            3 => CalibratedScalar::new(
                self.pultec_high_atten_scale,
                -1.3,
                -0.3,
                self.pultec_high_atten_step,
            ),
            4 => CalibratedScalar::new(
                self.api_low_overshoot_factor,
                -0.3,
                0.2,
                self.api_low_overshoot_step,
            ),
            5 => CalibratedScalar::new(
                self.api_mid_skirt_factor,
                -0.2,
                0.1,
                self.api_mid_skirt_step,
            ),
            6 => CalibratedScalar::new(
                self.api_high_presence_factor,
                -0.1,
                0.25,
                self.api_high_presence_step,
            ),
            7 => CalibratedScalar::new(self.ssl_e_skirt_factor, 0.0, 0.18, self.ssl_e_skirt_step),
            // Out-of-range indices clamp to the last scalar — a panic

            // here would abort the audio host on a bad automation index.
            _ => CalibratedScalar::new(self.ssl_g_skirt_factor, 0.0, 0.14, self.ssl_g_skirt_step),
        }
    }

    fn set_scalar(&mut self, index: usize, value: f64) {
        match index {
            0 => self.pultec_low_overshoot_factor = value,
            1 => self.pultec_low_atten_scale = value,
            2 => self.pultec_high_air_factor = value,
            3 => self.pultec_high_atten_scale = value,
            4 => self.api_low_overshoot_factor = value,
            5 => self.api_mid_skirt_factor = value,
            6 => self.api_high_presence_factor = value,
            7 => self.ssl_e_skirt_factor = value,
            8 => self.ssl_g_skirt_factor = value,
            _ => {} // ignore out-of-range calibration indices
        }
    }

    fn set_step(&mut self, index: usize, step: f64) {
        match index {
            0 => self.pultec_low_overshoot_step = step,
            1 => self.pultec_low_atten_step = step,
            2 => self.pultec_high_air_step = step,
            3 => self.pultec_high_atten_step = step,
            4 => self.api_low_overshoot_step = step,
            5 => self.api_mid_skirt_step = step,
            6 => self.api_high_presence_step = step,
            7 => self.ssl_e_skirt_step = step,
            8 => self.ssl_g_skirt_step = step,
            _ => {} // ignore out-of-range calibration indices
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PultecEqp1aSettings {
    pub eq_in: bool,
    pub low_freq_hz: f64,
    pub low_boost_db: f64,
    pub low_atten_db: f64,
    pub high_boost_freq_hz: f64,
    pub high_boost_db: f64,
    pub high_bandwidth: f64,
    pub high_atten_freq_hz: f64,
    pub high_atten_db: f64,
    pub drive_percent: f64,
    pub trim_db: f64,
}

impl Default for PultecEqp1aSettings {
    fn default() -> Self {
        Self {
            eq_in: true,
            low_freq_hz: 60.0,
            low_boost_db: 0.0,
            low_atten_db: 0.0,
            high_boost_freq_hz: 5000.0,
            high_boost_db: 0.0,
            high_bandwidth: 5.0,
            high_atten_freq_hz: 10000.0,
            high_atten_db: 0.0,
            drive_percent: 0.0,
            trim_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Api550aSettings {
    pub eq_in: bool,
    pub low_freq_hz: f64,
    pub low_gain_db: f64,
    pub mid_freq_hz: f64,
    pub mid_gain_db: f64,
    pub high_freq_hz: f64,
    pub high_gain_db: f64,
    pub drive_percent: f64,
    pub trim_db: f64,
}

impl Default for Api550aSettings {
    fn default() -> Self {
        Self {
            eq_in: true,
            low_freq_hz: 100.0,
            low_gain_db: 0.0,
            mid_freq_hz: 1500.0,
            mid_gain_db: 0.0,
            high_freq_hz: 10000.0,
            high_gain_db: 0.0,
            drive_percent: 0.0,
            trim_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SslChannelSettings {
    pub eq_in: bool,
    pub e_series: bool,
    pub hpf_hz: f64,
    pub lpf_hz: f64,
    pub lf_freq_hz: f64,
    pub lf_gain_db: f64,
    pub lmf_freq_hz: f64,
    pub lmf_gain_db: f64,
    pub hmf_freq_hz: f64,
    pub hmf_gain_db: f64,
    pub hf_freq_hz: f64,
    pub hf_gain_db: f64,
    pub drive_percent: f64,
    pub trim_db: f64,
}

impl SslChannelSettings {
    #[must_use]
    pub const fn e_series() -> Self {
        Self {
            eq_in: true,
            e_series: true,
            hpf_hz: 20.0,
            lpf_hz: 22000.0,
            lf_freq_hz: 100.0,
            lf_gain_db: 0.0,
            lmf_freq_hz: 800.0,
            lmf_gain_db: 0.0,
            hmf_freq_hz: 3000.0,
            hmf_gain_db: 0.0,
            hf_freq_hz: 8000.0,
            hf_gain_db: 0.0,
            drive_percent: 0.0,
            trim_db: 0.0,
        }
    }

    #[must_use]
    pub const fn g_series() -> Self {
        Self {
            e_series: false,
            ..Self::e_series()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HardwareEqSettings {
    Pultec(PultecEqp1aSettings),
    Api(Api550aSettings),
    Ssl(SslChannelSettings),
}

/// Host adapter using the same prepared cascade and coloration as the public API.
pub struct HardwareEqModel {
    settings: HardwareEqSettings,
    sample_rate: f64,
    coeffs: crate::eq::inline::InlineVec<Coeffs>,
    processor: Option<ModelProcessor<HardwareColoration>>,
    last_error: Option<crate::eq::Error>,
}

#[derive(Clone)]
enum HardwareColoration {
    Pultec(PultecColoration),
    Approximation { drive: f64 },
}
impl Coloration for HardwareColoration {
    fn process(&mut self, sample: f64) -> f64 {
        match self {
            Self::Pultec(stage) => stage.process(sample),
            Self::Approximation { drive } => apply_gain_compensated_arctan(sample, *drive, 0.0),
        }
    }
    fn reset(&mut self) {
        if let Self::Pultec(stage) = self {
            stage.reset();
        }
    }
    fn update(&mut self, prepared: &Self) {
        match (self, prepared) {
            (Self::Pultec(stage), Self::Pultec(next)) => stage.update(next),
            (stage, next) => *stage = next.clone(),
        }
    }
}
impl HardwareEqModel {
    #[must_use]
    pub fn new(sample_rate: f64, settings: HardwareEqSettings) -> Self {
        let mut model = Self {
            settings,
            sample_rate,
            coeffs: crate::eq::inline::InlineVec::new(),
            processor: None,
            last_error: None,
        };
        model.rebuild();
        model
    }
    #[must_use]
    pub const fn settings(&self) -> HardwareEqSettings {
        self.settings
    }
    #[must_use]
    pub const fn last_design_error(&self) -> Option<crate::eq::Error> {
        self.last_error
    }
    pub fn set_settings(&mut self, settings: HardwareEqSettings) {
        if self.settings != settings {
            let previous = self.settings;
            self.settings = settings;
            self.rebuild();
            if self.last_error.is_some() {
                self.settings = previous;
            }
        }
    }
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if self.sample_rate.to_bits() != sample_rate.to_bits() {
            let previous = self.sample_rate;
            self.sample_rate = sample_rate;
            self.rebuild();
            if self.last_error.is_some() {
                self.sample_rate = previous;
            }
        }
    }
    #[must_use]
    pub fn coeffs(&self) -> &[Coeffs] {
        &self.coeffs
    }
    /// # Errors
    /// Returns a design error for an invalid cascade.
    pub fn prepared_filter(&self) -> Result<crate::eq::PreparedFilter, crate::eq::Error> {
        crate::eq::PreparedFilter::from_sections(self.sample_rate, self.coeffs())
    }
    #[must_use]
    pub fn magnitude_response_db(&self, frequencies: &[f64]) -> Vec<f64> {
        compute_magnitude_response(&self.coeffs, frequencies, self.sample_rate)
    }
    pub fn reset(&mut self) {
        if let Some(processor) = &mut self.processor {
            processor.reset();
        }
    }
    pub fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        if let Some(processor) = &mut self.processor {
            for (l, r) in left.iter_mut().zip(right) {
                [*l, *r] = processor.process_frame([*l, *r]);
            }
        }
    }
    fn rebuild(&mut self) {
        if let Err(error) = self.try_rebuild() {
            self.last_error = Some(error);
        }
    }
    fn try_rebuild(&mut self) -> Result<(), crate::eq::Error> {
        let spec = crate::eq::ProcessSpec::new(self.sample_rate, 1)?;
        let (coeffs, coloration, placement, trim) = match self.settings {
            HardwareEqSettings::Pultec(settings) => {
                // Validate native controls through the public preparation boundary.
                settings.prepare(spec)?;
                (
                    build_pultec_eqp1a_sections(settings, self.sample_rate),
                    HardwareColoration::Pultec(PultecColoration::new(
                        settings.drive_percent,
                        self.sample_rate,
                    )?),
                    ColorationPlacement::AfterFilters,
                    settings.trim_db,
                )
            }
            HardwareEqSettings::Api(settings) => (
                build_api_550a_sections(settings, self.sample_rate),
                HardwareColoration::Approximation {
                    drive: settings.drive_percent,
                },
                ColorationPlacement::BeforeFilters,
                settings.trim_db,
            ),
            HardwareEqSettings::Ssl(settings) => (
                build_ssl_channel_sections(settings, self.sample_rate),
                HardwareColoration::Approximation {
                    drive: settings.drive_percent,
                },
                ColorationPlacement::BeforeFilters,
                settings.trim_db,
            ),
        };
        let filter = crate::eq::PreparedFilter::from_sections(self.sample_rate, &coeffs)?;
        let prepared = PreparedModel::new(filter, coloration, placement, trim, spec)?;
        let applied = self
            .processor
            .as_mut()
            .is_some_and(|processor| processor.apply(&prepared).is_ok());
        if !applied {
            self.processor = Some(prepared.processor());
        }
        self.coeffs = coeffs;
        self.last_error = None;
        Ok(())
    }
}

fn add_pultec_low_sections(
    sections: &mut crate::eq::inline::InlineVec<Coeffs>,
    settings: &PultecEqp1aSettings,
    calibration: &HardwareEqCalibration,
    sample_rate: f64,
) {
    let low_freq = settings.low_freq_hz.clamp(20.0, 200.0);
    let low_boost = settings.low_boost_db.clamp(0.0, 13.0);
    let low_atten = settings.low_atten_db.clamp(0.0, 13.0);
    if low_boost > 1.0e-9 {
        push(
            sections,
            FilterType::LowShelf,
            low_freq,
            0.52,
            low_boost,
            sample_rate,
            2,
        );
        push(
            sections,
            FilterType::Peak,
            low_freq * 2.2,
            0.62,
            calibration.pultec_low_overshoot_factor * low_boost,
            sample_rate,
            2,
        );
    }
    if low_atten > 1.0e-9 {
        push(
            sections,
            FilterType::LowShelf,
            low_freq * 1.55,
            0.82,
            calibration.pultec_low_atten_scale * low_atten,
            sample_rate,
            2,
        );
        if low_boost > 1.0e-9 {
            push(
                sections,
                FilterType::Peak,
                low_freq * 5.0,
                0.55,
                -0.10 * low_atten,
                sample_rate,
                2,
            );
        }
    }
}

fn add_pultec_high_sections(
    sections: &mut crate::eq::inline::InlineVec<Coeffs>,
    settings: &PultecEqp1aSettings,
    calibration: &HardwareEqCalibration,
    sample_rate: f64,
) {
    let high_boost = settings.high_boost_db.clamp(0.0, 16.0);
    if high_boost > 1.0e-9 {
        let bandwidth = settings.high_bandwidth.clamp(0.0, 10.0);
        let q = 0.35 + bandwidth * 0.115;
        let freq = settings.high_boost_freq_hz.clamp(1000.0, 16000.0);
        push(
            sections,
            FilterType::Peak,
            freq,
            q,
            high_boost,
            sample_rate,
            2,
        );
        push(
            sections,
            FilterType::HighShelf,
            freq * 1.2,
            0.58,
            calibration.pultec_high_air_factor * high_boost,
            sample_rate,
            2,
        );
    }

    let high_atten = settings.high_atten_db.clamp(0.0, 16.0);
    if high_atten > 1.0e-9 {
        push(
            sections,
            FilterType::HighShelf,
            settings.high_atten_freq_hz.clamp(4000.0, 20000.0),
            0.55,
            calibration.pultec_high_atten_scale * high_atten,
            sample_rate,
            2,
        );
    }
}

#[must_use]
pub fn build_pultec_eqp1a_sections(
    settings: PultecEqp1aSettings,
    sample_rate: f64,
) -> crate::eq::inline::InlineVec<Coeffs> {
    build_pultec_eqp1a_sections_with_calibration(
        settings,
        sample_rate,
        &HardwareEqCalibration::default(),
    )
}

#[must_use]
pub fn build_pultec_eqp1a_sections_with_calibration(
    settings: PultecEqp1aSettings,
    sample_rate: f64,
    calibration: &HardwareEqCalibration,
) -> crate::eq::inline::InlineVec<Coeffs> {
    let mut sections = crate::eq::inline::InlineVec::new();
    push(
        &mut sections,
        FilterType::Highpass,
        14.0,
        0.58,
        0.0,
        sample_rate,
        2,
    );
    push(
        &mut sections,
        FilterType::Lowpass,
        28000.0,
        0.62,
        0.0,
        sample_rate,
        2,
    );
    push(
        &mut sections,
        FilterType::Peak,
        900.0,
        0.45,
        -0.18,
        sample_rate,
        2,
    );

    if !settings.eq_in {
        return sections;
    }

    add_pultec_low_sections(&mut sections, &settings, calibration, sample_rate);
    add_pultec_high_sections(&mut sections, &settings, calibration, sample_rate);

    sections
}

#[must_use]
pub fn fit_pultec_eqp1a_response(
    initial: HardwareEqCalibration,
    settings: PultecEqp1aSettings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<HardwareEqCalibration> {
    fit_response(initial, target, options, |calibration| {
        build_pultec_eqp1a_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

#[must_use]
pub fn build_api_550a_sections(
    settings: Api550aSettings,
    sample_rate: f64,
) -> crate::eq::inline::InlineVec<Coeffs> {
    build_api_550a_sections_with_calibration(
        settings,
        sample_rate,
        &HardwareEqCalibration::default(),
    )
}

#[must_use]
pub fn build_api_550a_sections_with_calibration(
    settings: Api550aSettings,
    sample_rate: f64,
    calibration: &HardwareEqCalibration,
) -> crate::eq::inline::InlineVec<Coeffs> {
    let mut sections = crate::eq::inline::InlineVec::new();
    push(
        &mut sections,
        FilterType::Highpass,
        18.0,
        0.6,
        0.0,
        sample_rate,
        2,
    );
    push(
        &mut sections,
        FilterType::HighShelf,
        18000.0,
        0.65,
        -0.18,
        sample_rate,
        2,
    );

    if !settings.eq_in {
        return sections;
    }

    let low = settings.low_gain_db.clamp(-12.0, 12.0);
    if low.abs() > 1.0e-9 {
        let q = proportional_q(low, 0.58, 1.05);
        push(
            &mut sections,
            FilterType::LowShelf,
            settings.low_freq_hz,
            q,
            low,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.low_freq_hz * 2.4,
            0.8,
            calibration.api_low_overshoot_factor * low,
            sample_rate,
            2,
        );
    }

    let mid = settings.mid_gain_db.clamp(-12.0, 12.0);
    if mid.abs() > 1.0e-9 {
        let q = proportional_q(mid, 0.72, 1.85);
        push(
            &mut sections,
            FilterType::Peak,
            settings.mid_freq_hz,
            q,
            mid,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.mid_freq_hz * 0.58,
            0.75,
            calibration.api_mid_skirt_factor * mid,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.mid_freq_hz * 1.72,
            0.75,
            calibration.api_mid_skirt_factor * mid,
            sample_rate,
            2,
        );
    }

    let high = settings.high_gain_db.clamp(-12.0, 12.0);
    if high.abs() > 1.0e-9 {
        let q = proportional_q(high, 0.55, 1.25);
        push(
            &mut sections,
            FilterType::HighShelf,
            settings.high_freq_hz,
            q,
            high,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.high_freq_hz * 0.72,
            0.85,
            calibration.api_high_presence_factor * high,
            sample_rate,
            2,
        );
    }

    sections
}

#[must_use]
pub fn fit_api_550a_response(
    initial: HardwareEqCalibration,
    settings: Api550aSettings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<HardwareEqCalibration> {
    fit_response(initial, target, options, |calibration| {
        build_api_550a_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

#[must_use]
pub fn build_ssl_channel_sections(
    settings: SslChannelSettings,
    sample_rate: f64,
) -> crate::eq::inline::InlineVec<Coeffs> {
    build_ssl_channel_sections_with_calibration(
        settings,
        sample_rate,
        &HardwareEqCalibration::default(),
    )
}

#[must_use]
pub fn build_ssl_channel_sections_with_calibration(
    settings: SslChannelSettings,
    sample_rate: f64,
    calibration: &HardwareEqCalibration,
) -> crate::eq::inline::InlineVec<Coeffs> {
    let mut sections = crate::eq::inline::InlineVec::new();
    push(
        &mut sections,
        FilterType::Highpass,
        16.0,
        0.58,
        0.0,
        sample_rate,
        1,
    );
    push(
        &mut sections,
        FilterType::HighShelf,
        22000.0,
        0.62,
        -0.08,
        sample_rate,
        1,
    );

    if !settings.eq_in {
        return sections;
    }

    if settings.e_series {
        push_cut_if_active(
            &mut sections,
            FilterType::Highpass,
            settings.hpf_hz,
            18.0,
            350.0,
            sample_rate,
        );
        push_cut_if_active(
            &mut sections,
            FilterType::Lowpass,
            settings.lpf_hz,
            3000.0,
            21500.0,
            sample_rate,
        );
    }

    let (low_q, lmid_q, hmid_q, high_q, skirt) = if settings.e_series {
        (0.72, 1.25, 1.35, 0.74, calibration.ssl_e_skirt_factor)
    } else {
        (0.58, 0.85, 0.95, 0.58, calibration.ssl_g_skirt_factor)
    };

    push_gain(
        &mut sections,
        FilterType::LowShelf,
        settings.lf_freq_hz,
        low_q,
        settings.lf_gain_db,
        sample_rate,
    );
    push_gain(
        &mut sections,
        FilterType::Peak,
        settings.lmf_freq_hz,
        lmid_q,
        settings.lmf_gain_db,
        sample_rate,
    );
    push_gain(
        &mut sections,
        FilterType::Peak,
        settings.hmf_freq_hz,
        hmid_q,
        settings.hmf_gain_db,
        sample_rate,
    );
    push_gain(
        &mut sections,
        FilterType::HighShelf,
        settings.hf_freq_hz,
        high_q,
        settings.hf_gain_db,
        sample_rate,
    );

    for (freq, gain) in [
        (settings.lmf_freq_hz * 0.48, -skirt * settings.lmf_gain_db),
        (settings.lmf_freq_hz * 2.1, -skirt * settings.lmf_gain_db),
        (settings.hmf_freq_hz * 0.52, -skirt * settings.hmf_gain_db),
        (settings.hmf_freq_hz * 1.9, -skirt * settings.hmf_gain_db),
    ] {
        push_gain(
            &mut sections,
            FilterType::Peak,
            freq,
            0.7,
            gain,
            sample_rate,
        );
    }

    sections
}

#[must_use]
pub fn fit_ssl_channel_response(
    initial: HardwareEqCalibration,
    settings: SslChannelSettings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<HardwareEqCalibration> {
    fit_response(initial, target, options, |calibration| {
        build_ssl_channel_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

fn push_gain(
    sections: &mut crate::eq::inline::InlineVec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
) {
    let gain = gain_db.clamp(-15.0, 15.0);
    if gain.abs() > 1.0e-9 {
        push(sections, filter_type, freq_hz, q, gain, sample_rate, 2);
    }
}

fn push_cut_if_active(
    sections: &mut crate::eq::inline::InlineVec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    off_edge: f64,
    active_edge: f64,
    sample_rate: f64,
) {
    if (filter_type == FilterType::Highpass && freq_hz > off_edge + 1.0)
        || (filter_type == FilterType::Lowpass && freq_hz < active_edge)
    {
        push(sections, filter_type, freq_hz, 0.707, 0.0, sample_rate, 3);
    }
}

fn proportional_q(gain_db: f64, base: f64, max: f64) -> f64 {
    (max - base).mul_add((gain_db.abs() / 12.0).clamp(0.0, 1.0), base)
}

fn push(
    sections: &mut crate::eq::inline::InlineVec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) {
    let nyquist_safe = sample_rate * 0.46;
    sections.extend(design::design_filter(
        filter_type,
        freq_hz.clamp(8.0, nyquist_safe),
        q.clamp(0.2, 8.0),
        gain_db,
        sample_rate,
        order,
    ));
}
