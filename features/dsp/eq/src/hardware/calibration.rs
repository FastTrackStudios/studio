//! Frequency-response calibration helpers for hardware-style EQ models.
//!
//! The calibration layer is intentionally data-oriented: a profile provides
//! response snapshots and a small set of tunable model parameters, then this
//! module evaluates fit error and runs a bounded coordinate search.

use crate::design::biquad::Coeffs;
use crate::runtime::response::compute_magnitude_response;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResponsePoint {
    pub freq_hz: f64,
    pub magnitude_db: f64,
    pub weight: f64,
}

impl ResponsePoint {
    #[must_use]
    pub const fn new(freq_hz: f64, magnitude_db: f64) -> Self {
        Self {
            freq_hz,
            magnitude_db,
            weight: 1.0,
        }
    }

    #[must_use]
    pub const fn weighted(freq_hz: f64, magnitude_db: f64, weight: f64) -> Self {
        Self {
            freq_hz,
            magnitude_db,
            weight,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponseTarget {
    pub id: String,
    pub sample_rate: f64,
    pub points: Vec<ResponsePoint>,
}

impl ResponseTarget {
    pub fn new(id: impl Into<String>, sample_rate: f64, points: Vec<ResponsePoint>) -> Self {
        Self {
            id: id.into(),
            sample_rate,
            points,
        }
    }

    #[must_use]
    pub fn frequencies(&self) -> Vec<f64> {
        self.points.iter().map(|point| point.freq_hz).collect()
    }

    /// Compute weighted root mean square (RMS) error and maximum absolute error.
    ///
    /// # Panics
    ///
    /// Panics if `predicted_db` length does not match the number of target points.
    #[must_use]
    pub fn weighted_error_db(&self, predicted_db: &[f64]) -> CalibrationError {
        assert_eq!(
            self.points.len(),
            predicted_db.len(),
            "target point count must match predicted magnitude count"
        );

        let mut weighted_sse = 0.0;
        let mut weight_sum = 0.0;
        let mut max_abs_db = 0.0_f64;

        for (point, &predicted) in self.points.iter().zip(predicted_db) {
            let weight = point.weight.max(0.0);
            let error = predicted - point.magnitude_db;
            weighted_sse += weight * error * error;
            weight_sum += weight;
            max_abs_db = max_abs_db.max(error.abs());
        }

        let rms_db = if weight_sum > 0.0 {
            (weighted_sse / weight_sum).sqrt()
        } else {
            0.0
        };

        CalibrationError { rms_db, max_abs_db }
    }

    #[must_use]
    pub fn evaluate_sections(&self, sections: &[Coeffs]) -> CalibrationError {
        let freqs = self.frequencies();
        let predicted = compute_magnitude_response(sections, &freqs, self.sample_rate);
        self.weighted_error_db(&predicted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibrationError {
    pub rms_db: f64,
    pub max_abs_db: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibratedScalar {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl CalibratedScalar {
    #[must_use]
    pub const fn new(value: f64, min: f64, max: f64, step: f64) -> Self {
        Self {
            value,
            min,
            max,
            step,
        }
    }

    #[must_use]
    pub const fn clamp_value(&self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }
}

pub trait CalibrationParameters: Clone {
    fn len(&self) -> usize;
    fn scalar(&self, index: usize) -> CalibratedScalar;
    fn set_scalar(&mut self, index: usize, value: f64);
    fn set_step(&mut self, index: usize, step: f64);

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitOptions {
    pub iterations: usize,
    pub shrink: f64,
    pub min_step: f64,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            iterations: 24,
            shrink: 0.55,
            min_step: 1.0e-4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitReport<P> {
    pub params: P,
    pub error: CalibrationError,
    pub iterations: usize,
}

pub fn fit_response<P, F, S>(
    initial: P,
    target: &ResponseTarget,
    options: FitOptions,
    build_sections: F,
) -> FitReport<P>
where
    P: CalibrationParameters,
    F: Fn(&P) -> S,
    S: AsRef<[Coeffs]>,
{
    let mut current = initial;
    let mut current_error = target.evaluate_sections(build_sections(&current).as_ref());
    let mut iterations_run = 0;

    for iteration in 0..options.iterations {
        iterations_run = iteration.saturating_add(1);
        let mut improved = false;

        for index in 0..current.len() {
            let scalar = current.scalar(index);
            if scalar.step <= options.min_step {
                continue;
            }

            let mut best_value = scalar.value;
            let mut best_error = current_error;

            for direction in [-1.0, 1.0] {
                let candidate_value =
                    scalar.clamp_value(scalar.step.mul_add(direction, scalar.value));
                if (candidate_value - scalar.value).abs() <= f64::EPSILON {
                    continue;
                }

                let mut candidate = current.clone();
                candidate.set_scalar(index, candidate_value);
                let candidate_error = target.evaluate_sections(build_sections(&candidate).as_ref());
                if candidate_error.rms_db < best_error.rms_db {
                    best_value = candidate_value;
                    best_error = candidate_error;
                }
            }

            if best_error.rms_db < current_error.rms_db {
                current.set_scalar(index, best_value);
                current_error = best_error;
                improved = true;
            } else {
                let next_step = (scalar.step * options.shrink).max(options.min_step);
                current.set_step(index, next_step);
            }
        }

        if !improved {
            break;
        }
    }

    FitReport {
        params: current,
        error: current_error,
        iterations: iterations_run,
    }
}
