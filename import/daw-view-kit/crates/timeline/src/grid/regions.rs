//! A grid that differs across the timeline: a default, and stretches
//! where something more specific says otherwise (a section of
//! sixteenths, a fill's triplets inside it).

/// The grid in force: `default` everywhere the spans (sorted, apart) do
/// not say otherwise. Divisions are of a whole note: 8 eighths, 16
/// sixteenths, 12 eighth triplets. Times are the surface's units.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridRegions {
    pub default: u32,
    /// `(start, end, division)` in whole quanta.
    spans: Vec<(i64, i64, u32)>,
    /// The unit edges are compared in, as bits of an `f64`: two edges
    /// one quantum apart or less are one edge.
    quantum: u64,
}

impl GridRegions {
    /// `default` with `regions` (`(start, end, division)`) over it, the
    /// shortest holding each stretch — a fill inside a part, a figure
    /// inside a fill. Edges are compared to the nearest `quantum`.
    #[must_use]
    pub fn new(default: u32, regions: &[(f64, f64, u32)], quantum: f64) -> Self {
        let q = quantum.max(f64::MIN_POSITIVE);
        let at = |t: f64| ticks(t, q);
        let mut edges: Vec<i64> = regions
            .iter()
            .flat_map(|&(a, b, _)| [at(a), at(b)])
            .collect();
        edges.sort_unstable();
        edges.dedup();
        let mut spans: Vec<(i64, i64, u32)> = Vec::new();
        for pair in edges.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            let held = regions
                .iter()
                .filter(|&&(ra, rb, _)| at(ra) <= a && at(rb) >= b)
                .min_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0)))
                .map(|r| r.2);
            let Some(division) = held.filter(|d| *d != default) else {
                continue;
            };
            match spans.last_mut() {
                Some(last) if last.1 == a && last.2 == division => last.1 = b,
                _ => spans.push((a, b, division)),
            }
        }
        Self {
            default,
            spans,
            quantum: q.to_bits(),
        }
    }

    fn quantum(&self) -> f64 {
        f64::from_bits(self.quantum)
    }

    /// The stretches that differ from the default, `(start, end,
    /// division)`.
    #[must_use]
    pub fn spans(&self) -> Vec<(f64, f64, u32)> {
        let q = self.quantum();
        self.spans
            .iter()
            .map(|&(a, b, d)| (time(a, q), time(b, q), d))
            .collect()
    }

    /// The grid at `t`.
    #[must_use]
    pub fn at(&self, t: f64) -> u32 {
        let t = ticks(t, self.quantum());
        self.spans
            .iter()
            .find(|(a, b, _)| *a <= t && t < *b)
            .map_or(self.default, |s| s.2)
    }

    /// Each stretch from `from` to `to` with its grid, in order.
    #[must_use]
    pub fn between(&self, from: f64, to: f64) -> Vec<(f64, f64, u32)> {
        let mut out = Vec::new();
        let mut at = from;
        for (a, b, division) in self.spans() {
            if b <= at || a >= to {
                continue;
            }
            if a > at {
                out.push((at, a, self.default));
            }
            out.push((a.max(at), b.min(to), division));
            at = b;
        }
        if at < to {
            out.push((at, to, self.default));
        }
        out
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "saturating: a time beyond i64 quanta is off any timeline"
)]
fn ticks(t: f64, quantum: f64) -> i64 {
    (t / quantum).round() as i64
}

#[expect(clippy::cast_precision_loss, reason = "back from whole quanta")]
fn time(ticks: i64, quantum: f64) -> f64 {
    ticks as f64 * quantum
}

#[cfg(test)]
mod tests {
    use super::*;

    const US: f64 = 1e-6;

    #[test]
    fn the_shortest_region_says_and_eighths_elsewhere() {
        // A section of sixteenths, a fill of sixteenth triplets in it.
        let grid = GridRegions::new(8, &[(10.0, 20.0, 16), (14.0, 15.0, 24)], US);
        assert_eq!(grid.at(5.0), 8);
        assert_eq!(grid.at(12.0), 16);
        assert_eq!(grid.at(14.5), 24);
        assert_eq!(grid.at(15.0), 16);
        assert_eq!(grid.at(20.0), 8);
        assert_eq!(
            grid.between(9.0, 21.0),
            vec![
                (9.0, 10.0, 8),
                (10.0, 14.0, 16),
                (14.0, 15.0, 24),
                (15.0, 20.0, 16),
                (20.0, 21.0, 8),
            ]
        );
    }

    #[test]
    fn a_region_of_the_default_is_no_span() {
        let grid = GridRegions::new(8, &[(10.0, 20.0, 8)], US);
        assert!(grid.spans().is_empty());
        assert_eq!(grid.between(0.0, 30.0), vec![(0.0, 30.0, 8)]);
    }

    #[test]
    fn edges_within_a_quantum_are_one_edge() {
        let grid = GridRegions::new(8, &[(10.0, 20.0, 16), (20.000_000_1, 30.0, 16)], US);
        assert_eq!(grid.spans(), vec![(10.0, 30.0, 16)]);
    }
}
