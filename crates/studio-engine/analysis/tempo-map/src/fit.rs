//! A weighted straight line through a few points, and small statistics.

/// The line through `(xs, ys)` evaluated at `x`, each residual counted
/// `ws` times over (numpy's `polyfit(xs, ys, 1, w=ws)`: the squared error
/// is weighted by `ws` squared). With every point at one `x`, their
/// weighted mean.
#[must_use]
pub fn line_at(xs: &[f64], ys: &[f64], ws: &[f64], x: f64) -> f64 {
    let (mut sw, mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for ((xi, yi), wi) in xs.iter().zip(ys).zip(ws) {
        let w = wi * wi;
        sw += w;
        sx += w * xi;
        sy += w * yi;
        sxx += w * xi * xi;
        sxy += w * xi * yi;
    }
    if sw <= 0.0 {
        return ys.first().copied().unwrap_or(0.0);
    }
    let det = sw * sxx - sx * sx;
    if det.abs() < 1e-12 {
        return sy / sw;
    }
    let slope = (sw * sxy - sx * sy) / det;
    let intercept = (sy - slope * sx) / sw;
    intercept + slope * x
}

/// The median of `values` (0 for none).
#[must_use]
pub fn median(values: &[f64]) -> f64 {
    quantile(values, 0.5)
}

/// The `q`th quantile of `values` (0 to 1), linear between the two
/// nearest, as numpy's `percentile`.
#[must_use]
pub fn quantile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = (sorted.len() - 1) as f64 * q.clamp(0.0, 1.0);
    let lo = at.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = at - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

/// The median, the 95th percentile and the largest of a set of errors.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spread {
    pub count: usize,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

impl Spread {
    /// Of the absolute values of `errors`.
    #[must_use]
    pub fn of(errors: &[f64]) -> Self {
        let abs: Vec<f64> = errors.iter().map(|e| e.abs()).collect();
        Self {
            count: abs.len(),
            median: quantile(&abs, 0.5),
            p95: quantile(&abs, 0.95),
            max: abs.iter().copied().fold(0.0, f64::max),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_through_points_on_a_line() {
        let xs = [0.0, 1.0, 2.0, 3.0];
        let ys = [1.0, 3.0, 5.0, 7.0];
        let at = line_at(&xs, &ys, &[1.0; 4], 1.5);
        assert!((at - 4.0).abs() < 1e-12);
    }

    #[test]
    fn a_light_point_pulls_less() {
        let xs = [0.0, 1.0, 2.0];
        let ys = [0.0, 1.0, 5.0];
        let even = line_at(&xs, &ys, &[1.0, 1.0, 1.0], 2.0);
        let light = line_at(&xs, &ys, &[1.0, 1.0, 0.1], 2.0);
        assert!(light < even);
        assert!((light - 2.0).abs() < 0.2, "{light}");
    }

    #[test]
    fn quantiles_as_numpy() {
        let v = [1.0, 2.0, 3.0, 4.0];
        assert!((median(&v) - 2.5).abs() < 1e-12);
        assert!((quantile(&v, 0.95) - 3.85).abs() < 1e-12);
    }
}
