//! Static EQ response evaluated from the DSP's prepared coefficients.
//! Prepare once per graph update, then evaluate the complete frequency grid.

use super::eq_graph_model::{EqBand, EqBandShape, StereoMode};
use crate::eq::{
    BandConfig, CutSlope, EqConfig, Filter, Placement, PreparedEq, PreparedFilter, ProcessSpec,
    Steepness,
};

/// The highest frequency a filter can actually be designed at, as a fraction
/// of the sample rate.
///
/// A biquad's centre has to sit below Nyquist; at exactly Nyquist the design
/// degenerates. Sessions cross sample rates — one saved at 96 kHz and opened
/// at 44.1 has bands the new rate cannot realise — so the graph designs them
/// where they can be designed rather than refusing to draw.
const DESIGN_CEILING: f64 = 0.499;

fn config(band: &EqBand) -> BandConfig {
    let frequency_hz = f64::from(band.frequency);
    let gain_db = f64::from(band.gain);
    let q = f64::from(band.q);
    let steepness = match band.slope.map(|v| v.round()) {
        Some(0.0 | 1.0) => {
            if matches!(band.shape, EqBandShape::Bell | EqBandShape::Notch) {
                Steepness::Order2
            } else {
                Steepness::Order1
            }
        }
        Some(3.0) => Steepness::Order3,
        Some(4.0) => Steepness::Order4,
        Some(5.0) => Steepness::Order5,
        Some(6.0) => Steepness::Order6,
        Some(7.0) => Steepness::Order8,
        Some(8.0) => Steepness::Order12,
        Some(9.0 | 10.0) => Steepness::Order16,
        _ => Steepness::Order2,
    };
    let cut_slope = match band.slope.map(f64::from) {
        Some(raw) if (0.0..6.0).contains(&raw) => CutSlope::DbPerOctave(raw * 6.0),
        Some(raw) => match raw.round() {
            6.0 => CutSlope::DbPerOctave(36.0),
            7.0 => CutSlope::DbPerOctave(48.0),
            8.0 => CutSlope::DbPerOctave(72.0),
            9.0 => CutSlope::DbPerOctave(96.0),
            10.0 => CutSlope::Brickwall,
            _ => CutSlope::DbPerOctave(12.0),
        },
        None => CutSlope::DbPerOctave(12.0),
    };
    let filter = match band.shape {
        EqBandShape::Bell => Filter::Bell {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::LowShelf => Filter::LowShelf {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::HighShelf => Filter::HighShelf {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::LowCut => Filter::HighPass {
            frequency_hz,
            q,
            slope: cut_slope,
        },
        EqBandShape::HighCut => Filter::LowPass {
            frequency_hz,
            q,
            slope: cut_slope,
        },
        EqBandShape::Notch => Filter::Notch {
            frequency_hz,
            q,
            steepness,
        },
        EqBandShape::BandPass => Filter::BandPass {
            frequency_hz,
            q,
            steepness,
        },
        EqBandShape::TiltShelf => Filter::Tilt {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::FlatTilt => Filter::FlatTilt {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::AllPass => Filter::AllPass {
            frequency_hz,
            q,
            steepness,
        },
    };
    BandConfig::new(filter)
        .enabled(band.used && band.enabled)
        .placement(match band.stereo_mode {
            StereoMode::Stereo => Placement::Stereo,
            StereoMode::Left => Placement::Left,
            StereoMode::Right => Placement::Right,
            StereoMode::Mid => Placement::Mid,
            StereoMode::Side => Placement::Side,
        })
}

/// The band as the graph will design it: its own frequency, or the highest
/// this sample rate can realise.
fn realisable(band: &EqBand, sample_rate: f64) -> EqBand {
    let ceiling = (sample_rate * DESIGN_CEILING) as f32;
    if band.frequency <= ceiling {
        return band.clone();
    }
    EqBand {
        frequency: ceiling,
        ..band.clone()
    }
}

pub fn prepare_band(band: &EqBand, sample_rate: f64) -> Result<PreparedFilter, crate::eq::Error> {
    config(&realisable(band, sample_rate))
        .filter
        .prepare(sample_rate)
}

/// Prepare the visible EQ.
///
/// A band the sample rate cannot realise is designed at Nyquist, and one that
/// still will not design is left out. Neither may take the curve with it: the
/// whole combined response used to come back `NaN` if any single band was
/// out of range, so one band parked above Nyquist blanked the display for all
/// eight — and at 48 kHz the frequency control could reach 30 kHz, so getting
/// there was a drag, not an edge case.
pub fn prepare_graph(bands: &[EqBand], sample_rate: f64) -> Result<PreparedEq, crate::eq::Error> {
    let mut eq = EqConfig::with_capacity(bands.len());
    for band in bands.iter().filter(|b| b.used && b.enabled) {
        let _ = eq.add_band(config(&realisable(band, sample_rate)));
    }
    eq.prepare(ProcessSpec::new(sample_rate, 1)?)
}

/// Power response for equal-power uncorrelated stereo input. This keeps one
/// graph line meaningful even when bands mix left/right and mid/side placement.
#[must_use]
pub fn graph_magnitude(prepared: &PreparedEq, hz: f64) -> f64 {
    prepared.base_response(hz).map_or(f64::NAN, |h| {
        10.0 * ((h.ll.mag_sq() + h.lr.mag_sq() + h.rl.mag_sq() + h.rr.mag_sq()) * 0.5)
            .max(1e-30)
            .log10()
    })
}

/// Convenience evaluation for a single point; renderers should prepare once.
#[must_use]
pub fn calculate_combined_response(bands: &[EqBand], freq: f64, sample_rate: f64) -> f64 {
    prepare_graph(bands, sample_rate).map_or(f64::NAN, |eq| graph_magnitude(&eq, freq))
}

#[must_use]
pub fn calculate_band_response(band: &EqBand, freq: f64, sample_rate: f64) -> f64 {
    prepare_band(band, sample_rate)
        .and_then(|filter| filter.magnitude_db(freq))
        .unwrap_or(f64::NAN)
}
