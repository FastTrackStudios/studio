//! Original Neve 1073-inspired model.
//!
//! This module intentionally does not port the GPL EQ1979 implementation. It
//! uses this crate's existing filter designer to build a stepped hardware-style
//! cascade, plus a gain-compensated arctan input stage.

use crate::eq::design::biquad::Coeffs;
use crate::eq::design::{self, FilterType};
use crate::eq::hardware::calibration::{
    CalibratedScalar, CalibrationParameters, FitOptions, FitReport, ResponseTarget, fit_response,
};
use crate::eq::runtime::response::compute_magnitude_response;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neve1073Hpf {
    Off,
    Hz50,
    Hz80,
    Hz160,
    Hz300,
}

impl Neve1073Hpf {
    #[must_use]
    pub const fn hz(self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::Hz50 => Some(50.0),
            Self::Hz80 => Some(80.0),
            Self::Hz160 => Some(160.0),
            Self::Hz300 => Some(300.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neve1073LowFreq {
    Off,
    Hz35,
    Hz60,
    Hz110,
    Hz220,
}

impl Neve1073LowFreq {
    #[must_use]
    pub const fn hz(self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::Hz35 => Some(35.0),
            Self::Hz60 => Some(60.0),
            Self::Hz110 => Some(110.0),
            Self::Hz220 => Some(220.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neve1073MidFreq {
    Off,
    Hz360,
    Hz700,
    Hz1600,
    Hz3200,
    Hz4800,
    Hz7200,
}

impl Neve1073MidFreq {
    #[must_use]
    pub const fn hz(self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::Hz360 => Some(360.0),
            Self::Hz700 => Some(700.0),
            Self::Hz1600 => Some(1600.0),
            Self::Hz3200 => Some(3200.0),
            Self::Hz4800 => Some(4800.0),
            Self::Hz7200 => Some(7200.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neve1073Settings {
    pub eq_in: bool,
    pub phase_invert: bool,
    pub trim_db: f64,
    pub drive_percent: f64,
    pub hpf: Neve1073Hpf,
    pub low_freq: Neve1073LowFreq,
    pub low_gain_db: f64,
    pub mid_freq: Neve1073MidFreq,
    pub mid_gain_db: f64,
    pub high_gain_db: f64,
}

impl Default for Neve1073Settings {
    fn default() -> Self {
        Self {
            eq_in: true,
            phase_invert: false,
            trim_db: 0.0,
            drive_percent: 0.0,
            hpf: Neve1073Hpf::Off,
            low_freq: Neve1073LowFreq::Off,
            low_gain_db: 0.0,
            mid_freq: Neve1073MidFreq::Off,
            mid_gain_db: 0.0,
            high_gain_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Neve1073Calibration {
    pub transformer_low_gain_db: f64,
    pub transformer_low_step: f64,
    pub transformer_low_q: f64,
    pub transformer_low_q_step: f64,
    pub transformer_mid_dip_db: f64,
    pub transformer_mid_dip_step: f64,
    pub transformer_presence_db: f64,
    pub transformer_presence_step: f64,
    pub transformer_high_trim_db: f64,
    pub transformer_high_trim_step: f64,
    pub hpf_bump_gain_db: f64,
    pub hpf_bump_gain_step: f64,
    pub hpf_bump_ratio: f64,
    pub hpf_bump_ratio_step: f64,
    pub low_overshoot_ratio: f64,
    pub low_overshoot_ratio_step: f64,
    pub low_overshoot_gain_factor: f64,
    pub low_overshoot_gain_step: f64,
    pub mid_lower_ratio: f64,
    pub mid_lower_ratio_step: f64,
    pub mid_lower_gain_factor: f64,
    pub mid_lower_gain_step: f64,
    pub mid_upper_ratio: f64,
    pub mid_upper_ratio_step: f64,
    pub mid_upper_gain_factor: f64,
    pub mid_upper_gain_step: f64,
    pub high_presence_gain_factor: f64,
    pub high_presence_gain_step: f64,
}

impl Default for Neve1073Calibration {
    fn default() -> Self {
        Self {
            transformer_low_gain_db: 0.45,
            transformer_low_step: 0.05,
            transformer_low_q: 0.62,
            transformer_low_q_step: 0.03,
            transformer_mid_dip_db: -0.35,
            transformer_mid_dip_step: 0.05,
            transformer_presence_db: 0.28,
            transformer_presence_step: 0.05,
            transformer_high_trim_db: -0.35,
            transformer_high_trim_step: 0.05,
            hpf_bump_gain_db: 0.65,
            hpf_bump_gain_step: 0.08,
            hpf_bump_ratio: 1.22,
            hpf_bump_ratio_step: 0.03,
            low_overshoot_ratio: 2.15,
            low_overshoot_ratio_step: 0.08,
            low_overshoot_gain_factor: -0.12,
            low_overshoot_gain_step: 0.02,
            mid_lower_ratio: 0.52,
            mid_lower_ratio_step: 0.03,
            mid_lower_gain_factor: -0.08,
            mid_lower_gain_step: 0.015,
            mid_upper_ratio: 1.88,
            mid_upper_ratio_step: 0.06,
            mid_upper_gain_factor: -0.06,
            mid_upper_gain_step: 0.015,
            high_presence_gain_factor: 0.10,
            high_presence_gain_step: 0.02,
        }
    }
}

impl CalibrationParameters for Neve1073Calibration {
    fn len(&self) -> usize {
        14
    }

    fn scalar(&self, index: usize) -> CalibratedScalar {
        match index {
            0 => CalibratedScalar::new(
                self.transformer_low_gain_db,
                -2.0,
                2.0,
                self.transformer_low_step,
            ),
            1 => CalibratedScalar::new(
                self.transformer_low_q,
                0.3,
                1.4,
                self.transformer_low_q_step,
            ),
            2 => CalibratedScalar::new(
                self.transformer_mid_dip_db,
                -2.0,
                2.0,
                self.transformer_mid_dip_step,
            ),
            3 => CalibratedScalar::new(
                self.transformer_presence_db,
                -2.0,
                2.0,
                self.transformer_presence_step,
            ),
            4 => CalibratedScalar::new(
                self.transformer_high_trim_db,
                -2.0,
                2.0,
                self.transformer_high_trim_step,
            ),
            5 => CalibratedScalar::new(self.hpf_bump_gain_db, -1.0, 3.0, self.hpf_bump_gain_step),
            6 => CalibratedScalar::new(self.hpf_bump_ratio, 0.8, 1.8, self.hpf_bump_ratio_step),
            7 => CalibratedScalar::new(
                self.low_overshoot_ratio,
                1.1,
                4.0,
                self.low_overshoot_ratio_step,
            ),
            8 => CalibratedScalar::new(
                self.low_overshoot_gain_factor,
                -0.5,
                0.5,
                self.low_overshoot_gain_step,
            ),
            9 => CalibratedScalar::new(self.mid_lower_ratio, 0.25, 0.9, self.mid_lower_ratio_step),
            10 => CalibratedScalar::new(
                self.mid_lower_gain_factor,
                -0.35,
                0.35,
                self.mid_lower_gain_step,
            ),
            11 => CalibratedScalar::new(self.mid_upper_ratio, 1.1, 3.2, self.mid_upper_ratio_step),
            12 => CalibratedScalar::new(
                self.mid_upper_gain_factor,
                -0.35,
                0.35,
                self.mid_upper_gain_step,
            ),
            // Out-of-range indices clamp to the last scalar — a panic

            // here would abort the audio host on a bad automation index.
            _ => CalibratedScalar::new(
                self.high_presence_gain_factor,
                -0.5,
                0.5,
                self.high_presence_gain_step,
            ),
        }
    }

    fn set_scalar(&mut self, index: usize, value: f64) {
        match index {
            0 => self.transformer_low_gain_db = value,
            1 => self.transformer_low_q = value,
            2 => self.transformer_mid_dip_db = value,
            3 => self.transformer_presence_db = value,
            4 => self.transformer_high_trim_db = value,
            5 => self.hpf_bump_gain_db = value,
            6 => self.hpf_bump_ratio = value,
            7 => self.low_overshoot_ratio = value,
            8 => self.low_overshoot_gain_factor = value,
            9 => self.mid_lower_ratio = value,
            10 => self.mid_lower_gain_factor = value,
            11 => self.mid_upper_ratio = value,
            12 => self.mid_upper_gain_factor = value,
            13 => self.high_presence_gain_factor = value,
            _ => {} // ignore out-of-range calibration indices
        }
    }

    fn set_step(&mut self, index: usize, step: f64) {
        match index {
            0 => self.transformer_low_step = step,
            1 => self.transformer_low_q_step = step,
            2 => self.transformer_mid_dip_step = step,
            3 => self.transformer_presence_step = step,
            4 => self.transformer_high_trim_step = step,
            5 => self.hpf_bump_gain_step = step,
            6 => self.hpf_bump_ratio_step = step,
            7 => self.low_overshoot_ratio_step = step,
            8 => self.low_overshoot_gain_step = step,
            9 => self.mid_lower_ratio_step = step,
            10 => self.mid_lower_gain_step = step,
            11 => self.mid_upper_ratio_step = step,
            12 => self.mid_upper_gain_step = step,
            13 => self.high_presence_gain_step = step,
            _ => {} // ignore out-of-range calibration indices
        }
    }
}

#[must_use]
pub fn fit_neve_1073_response(
    initial: Neve1073Calibration,
    settings: Neve1073Settings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<Neve1073Calibration> {
    fit_response(initial, target, options, |calibration| {
        build_neve_1073_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

pub struct Neve1073Model {
    settings: Neve1073Settings,
    sample_rate: f64,
    processor: Option<crate::eq::FilterProcessor>,
    last_error: Option<crate::eq::Error>,
    coeffs: crate::eq::inline::InlineVec<Coeffs>,
}

impl Neve1073Model {
    #[must_use]
    pub fn new(sample_rate: f64, settings: Neve1073Settings) -> Self {
        let mut model = Self {
            settings,
            sample_rate,
            processor: None,
            last_error: None,
            coeffs: crate::eq::inline::InlineVec::new(),
        };
        model.rebuild();
        model
    }

    #[must_use]
    pub const fn settings(&self) -> Neve1073Settings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: Neve1073Settings) {
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

    /// Validated linear cascade for shared processing or response evaluation.
    ///
    /// # Errors
    /// Returns a design error if the model contains invalid or unstable sections.
    pub fn prepared_filter(&self) -> Result<crate::eq::PreparedFilter, crate::eq::Error> {
        crate::eq::PreparedFilter::from_sections(self.sample_rate, self.coeffs())
    }

    #[must_use]
    pub fn magnitude_response_db(&self, frequencies: &[f64]) -> Vec<f64> {
        compute_magnitude_response(&self.coeffs, frequencies, self.sample_rate)
    }

    #[must_use]
    pub const fn last_design_error(&self) -> Option<crate::eq::Error> {
        self.last_error
    }

    pub fn reset(&mut self) {
        if let Some(processor) = &mut self.processor {
            processor.reset();
        }
    }

    pub fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        if let Some(processor) = &mut self.processor {
            let polarity = if self.settings.phase_invert {
                -1.0
            } else {
                1.0
            };
            for (l, r) in left.iter_mut().zip(right) {
                let frame = [*l, *r].map(|sample| {
                    polarity
                        * apply_gain_compensated_arctan(
                            sample,
                            self.settings.drive_percent,
                            self.settings.trim_db,
                        )
                });
                [*l, *r] = processor.process_frame(frame);
            }
        }
    }

    fn rebuild(&mut self) {
        let coeffs = build_neve_1073_sections(self.settings, self.sample_rate);
        match crate::eq::PreparedFilter::from_sections(self.sample_rate, &coeffs) {
            Ok(prepared) => {
                if let Some(processor) = &mut self.processor {
                    processor.install(&prepared);
                } else {
                    self.processor = Some(crate::eq::FilterProcessor::new(&prepared));
                }
                self.coeffs = coeffs;
                self.last_error = None;
            }
            Err(error) => self.last_error = Some(error),
        }
    }
}

#[must_use]
pub fn apply_gain_compensated_arctan(sample: f64, drive_percent: f64, trim_db: f64) -> f64 {
    let trim = db_to_gain(trim_db.clamp(-24.0, 24.0));
    let drive = drive_percent.clamp(0.0, 100.0) / 100.0;
    if drive <= 1.0e-9 {
        return sample * trim;
    }

    let amount = 1.0 + drive * 24.0;
    let compensation = amount.atan();
    (sample * trim * amount).atan() / compensation
}

#[must_use]
pub fn build_neve_1073_sections(
    settings: Neve1073Settings,
    sample_rate: f64,
) -> crate::eq::inline::InlineVec<Coeffs> {
    build_neve_1073_sections_with_calibration(
        settings,
        sample_rate,
        &Neve1073Calibration::default(),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "a decoded routine: one contiguous function in the binary, whose commentary cites the captured rows each branch was verified against. Splitting it would separate the arithmetic from its evidence"
)]
#[must_use]
pub fn build_neve_1073_sections_with_calibration(
    settings: Neve1073Settings,
    sample_rate: f64,
    calibration: &Neve1073Calibration,
) -> crate::eq::inline::InlineVec<Coeffs> {
    let mut sections = crate::eq::inline::InlineVec::new();

    // Always-on input/output transformer tone approximation. These gentle
    // sections supply the small LF/HF contours and midrange movement expected
    // from the channel strip, without copying another implementation's table.
    push_filter(
        &mut sections,
        FilterType::Highpass,
        18.0,
        0.58,
        0.0,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::LowShelf,
        55.0,
        calibration.transformer_low_q,
        calibration.transformer_low_gain_db,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::Peak,
        360.0,
        0.9,
        calibration.transformer_mid_dip_db,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::Peak,
        3200.0,
        0.75,
        calibration.transformer_presence_db,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::HighShelf,
        12000.0,
        0.68,
        calibration.transformer_high_trim_db,
        sample_rate,
        2,
    );

    if !settings.eq_in {
        return sections;
    }

    if let Some(freq) = settings.hpf.hz() {
        push_filter(
            &mut sections,
            FilterType::Highpass,
            freq,
            0.707,
            0.0,
            sample_rate,
            3,
        );
        // A small resonant skirt near the selected HPF point gives the response
        // the analog-style bump visible in measured/reference curves.
        push_filter(
            &mut sections,
            FilterType::Peak,
            freq * calibration.hpf_bump_ratio,
            1.2,
            calibration.hpf_bump_gain_db,
            sample_rate,
            2,
        );
    }

    if let Some(freq) = settings.low_freq.hz() {
        let gain = settings.low_gain_db.clamp(-16.0, 16.0);
        if gain.abs() > 1.0e-9 {
            push_filter(
                &mut sections,
                FilterType::LowShelf,
                freq,
                0.72,
                gain,
                sample_rate,
                2,
            );
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq * calibration.low_overshoot_ratio,
                0.85,
                calibration.low_overshoot_gain_factor * gain,
                sample_rate,
                2,
            );
        }
    }

    if let Some(freq) = settings.mid_freq.hz() {
        let gain = settings.mid_gain_db.clamp(-18.0, 18.0);
        if gain.abs() > 1.0e-9 {
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq,
                1.05,
                gain,
                sample_rate,
                2,
            );
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq * calibration.mid_lower_ratio,
                0.75,
                calibration.mid_lower_gain_factor * gain,
                sample_rate,
                2,
            );
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq * calibration.mid_upper_ratio,
                0.8,
                calibration.mid_upper_gain_factor * gain,
                sample_rate,
                2,
            );
        }
    }

    let high_gain = settings.high_gain_db.clamp(-16.0, 16.0);
    if high_gain.abs() > 1.0e-9 {
        push_filter(
            &mut sections,
            FilterType::HighShelf,
            12000.0,
            0.58,
            high_gain,
            sample_rate,
            2,
        );
        push_filter(
            &mut sections,
            FilterType::Peak,
            8200.0,
            0.72,
            calibration.high_presence_gain_factor * high_gain,
            sample_rate,
            2,
        );
    }

    sections
}

fn push_filter(
    sections: &mut crate::eq::inline::InlineVec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) {
    sections.extend(design::design_filter(
        filter_type,
        freq_hz,
        q,
        gain_db,
        sample_rate,
        order,
    ));
}

fn db_to_gain(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}
