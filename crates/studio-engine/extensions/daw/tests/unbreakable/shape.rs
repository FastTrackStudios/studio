//! A track's spectral shape against a reference's, as the drum-mixing
//! skill measures it: third-octave bands (LISTEN's `bands`) normalised to
//! each track's total power, then the mean absolute difference over a
//! range of band centres (150 Hz–8 kHz: below, tuning and the splitter's
//! low cut; above, the rehearsal stem rolls off).

/// `bands` (dB) relative to their total power: what is left when the two
/// tracks' levels are taken out. Bands that are not finite (a silent
/// band, read as null) stay out of the total and come back as NaN.
#[must_use]
pub fn shape(bands: &[f64]) -> Vec<f64> {
    let power: f64 = bands
        .iter()
        .filter(|b| b.is_finite())
        .map(|b| 10f64.powf(b / 10.0))
        .sum();
    let total = 10.0 * power.log10();
    bands
        .iter()
        .map(|b| if b.is_finite() { b - total } else { f64::NAN })
        .collect()
}

/// The mean |a − b| (dB) of the shapes of `a` and `b` over the bands whose
/// centre (`hz`) lies in `lo..=hi`; `None` when no band there is finite in
/// both.
#[must_use]
pub fn distance(hz: &[f64], a: &[f64], b: &[f64], lo: f64, hi: f64) -> Option<f64> {
    let (a, b) = (shape(a), shape(b));
    let diffs: Vec<f64> = hz
        .iter()
        .zip(a.iter().zip(&b))
        .filter(|(f, _)| (lo..=hi).contains(*f))
        .map(|(_, (x, y))| (x - y).abs())
        .filter(|d| d.is_finite())
        .collect();
    (!diffs.is_empty()).then(|| diffs.iter().sum::<f64>() / diffs.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_does_not_count_only_shape() {
        let hz = [100.0, 200.0, 400.0, 800.0];
        let a = [-20.0, -10.0, -10.0, -20.0];
        let louder = a.map(|b| b + 12.0);
        assert!(distance(&hz, &a, &louder, 0.0, 1e5).unwrap() < 1e-9);
    }

    #[test]
    fn a_band_six_db_apart_is_seen_inside_the_range_only() {
        let hz = [100.0, 200.0, 400.0, 800.0];
        let a = [-10.0, -10.0, -10.0, -10.0];
        // One band 6 dB down: the others rise a little against the total.
        let b = [-10.0, -16.0, -10.0, -10.0];
        let all = distance(&hz, &a, &b, 0.0, 1e5).unwrap();
        // Worked by hand: totals 10·log10(4·0.1) = −3.979 and
        // 10·log10(3·0.1 + 0.02512) = −4.878; the moved band differs by
        // 6 − 0.899 = 5.101, the three others by 0.899: mean 1.95.
        assert!((all - 1.95).abs() < 0.01, "{all}");
        let outside = distance(&hz, &a, &b, 300.0, 1e5).unwrap();
        assert!((outside - 0.899).abs() < 0.01, "{outside}");
    }

    #[test]
    fn silent_bands_are_left_out() {
        let hz = [100.0, 200.0];
        let a = [-10.0, f64::NAN];
        let b = [-10.0, -10.0];
        assert!(distance(&hz, &a, &b, 150.0, 1e5).is_none());
    }
}
