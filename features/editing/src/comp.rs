//! Which take plays a stretch best: the takes of a part ranked for a
//! stretch of it — a fill, a bar, a selection — from their hits.
//!
//! The basics of a kit are quantized; what is left is choosing, fill by
//! fill, the take the drummer played it best in. Each take's version of
//! the stretch is held up to what it should be — the part as written
//! (the drum MIDI) when there is one, else the other takes — and scored:
//!
//! - **plays it**: a take with no hits there is empty there (a drummer
//!   stopping, a take that ends early) and goes last;
//! - **covers it**: how much of what is written it plays — each note met
//!   by a hit of its own piece near it — or, with nothing written, how
//!   near its count of hits is to the takes' usual count (a dropped stroke
//!   or a stray one counts against it);
//! - **tight**: how far its hits sit from where they belong once its own
//!   feel is taken out — a take pushed 20 ms all through is in time with
//!   itself; one that rushes and drags is not;
//! - **costs the edit little**: how far the quantize would move its hits.
//!
//! Times are seconds, in the song's time: every take of a song is
//! recorded from the same place.

/// One hit of a take, or one written note: which piece, when.
#[derive(Clone, Debug, PartialEq)]
pub struct Strike {
    pub piece: String,
    pub t: f64,
}

/// One take's version of the stretch.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// What names it to comp from (its lane: `2`).
    pub take: String,
    /// Its hits inside the stretch.
    pub hits: Vec<Strike>,
}

/// How a take scored.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub take: String,
    /// 0..1, higher better; what the takes are ordered by.
    pub score: f64,
    pub hits: usize,
    /// Share of what is written it plays (or of the usual count), 0..1.
    pub coverage: f64,
    /// Median distance from where its hits belong, its feel taken out, ms.
    pub spread_ms: f64,
    /// Its feel: the median offset of its hits, ms (late positive).
    pub feel_ms: f64,
    /// Plays nothing here.
    pub empty: bool,
}

/// How near its note a hit must be to play it, seconds.
const MEETS: f64 = 0.060;

/// A spread this wide is as loose as it counts, ms.
const LOOSE_MS: f64 = 25.0;

/// How much of a take's score is what it plays (the rest is how tight):
/// a fill missing two strokes is worse than one played loose, which the
/// quantize puts right.
const COVERAGE: f64 = 0.75;

fn median(mut v: Vec<f64>) -> Option<f64> {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => v.get(n / 2).copied(),
        _ => Some((v.get(n / 2 - 1)? + v.get(n / 2)?) / 2.0),
    }
}

/// Where each hit belongs: its piece's nearest written note, else the
/// nearest line of `line_of` (a grid's nearest line).
fn offsets(hits: &[Strike], written: &[Strike], line_of: &dyn Fn(f64) -> f64) -> Vec<f64> {
    hits.iter()
        .map(|h| {
            let note = written
                .iter()
                .filter(|n| n.piece == h.piece)
                .map(|n| n.t)
                .min_by(|a, b| (a - h.t).abs().total_cmp(&(b - h.t).abs()))
                .filter(|n| (n - h.t).abs() <= MEETS);
            h.t - note.unwrap_or_else(|| line_of(h.t))
        })
        .collect()
}

/// Share of `written` that `hits` play: each note met by a hit of its
/// piece within [`MEETS`], a hit meeting one note at most.
fn coverage_of(hits: &[Strike], written: &[Strike]) -> f64 {
    if written.is_empty() {
        return 0.0;
    }
    let mut used = vec![false; hits.len()];
    let mut met = 0usize;
    for note in written {
        let best = hits
            .iter()
            .enumerate()
            .filter(|(i, h)| h.piece == note.piece && !used.get(*i).copied().unwrap_or(true))
            .map(|(i, h)| (i, (h.t - note.t).abs()))
            .filter(|(_, d)| *d <= MEETS)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best {
            if let Some(slot) = used.get_mut(i) {
                *slot = true;
            }
            met = met.saturating_add(1);
        }
    }
    met as f64 / written.len() as f64
}

/// The takes ranked for a stretch, best first. `written` is the part as
/// written there (empty when there is none); `line_of` gives a time's
/// nearest grid line, for hits with no note to belong to.
#[must_use]
pub fn rank(
    candidates: &[Candidate],
    written: &[Strike],
    line_of: &dyn Fn(f64) -> f64,
) -> Vec<Ranked> {
    let usual = median(
        candidates
            .iter()
            .filter(|c| !c.hits.is_empty())
            .map(|c| c.hits.len() as f64)
            .collect(),
    )
    .unwrap_or(0.0);
    let mut out: Vec<Ranked> = candidates
        .iter()
        .map(|c| {
            let empty = c.hits.is_empty();
            let coverage = if empty {
                0.0
            } else if written.is_empty() {
                // Nothing written: as near the usual count as can be.
                let n = c.hits.len() as f64;
                if usual > 0.0 {
                    (1.0 - (n - usual).abs() / usual).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            } else {
                coverage_of(&c.hits, written)
            };
            let offs = offsets(&c.hits, written, line_of);
            let feel = median(offs.clone()).unwrap_or(0.0);
            let spread = median(offs.iter().map(|o| (o - feel).abs()).collect()).unwrap_or(0.0);
            let spread_ms = spread * 1e3;
            let tight = 1.0 - (spread_ms / LOOSE_MS).clamp(0.0, 1.0);
            // What is played counts most: the edit can put a stroke in
            // time, not play one the drummer left out.
            let score = if empty {
                0.0
            } else {
                COVERAGE.mul_add(coverage, (1.0 - COVERAGE) * tight)
            };
            Ranked {
                take: c.take.clone(),
                score,
                hits: c.hits.len(),
                coverage,
                spread_ms,
                feel_ms: feel * 1e3,
                empty,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.take.cmp(&b.take))
    });
    out
}

/// The take after (`by` 1) or before (-1) `current` in `ranked` — round
/// from the last to the first — skipping empty ones; the best when
/// `current` is not among them.
#[must_use]
pub fn step<'a>(ranked: &'a [Ranked], current: Option<&str>, by: i32) -> Option<&'a Ranked> {
    let playing: Vec<&Ranked> = ranked.iter().filter(|r| !r.empty).collect();
    let n = i64::try_from(playing.len()).ok().filter(|n| *n > 0)?;
    let at = current.and_then(|c| playing.iter().position(|r| r.take == c));
    let next = match at {
        Some(i) => (i64::try_from(i).ok()? + i64::from(by)).rem_euclid(n),
        None => 0,
    };
    playing.get(usize::try_from(next).ok()?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(piece: &str, t: f64) -> Strike {
        Strike {
            piece: piece.to_owned(),
            t,
        }
    }

    fn line(t: f64) -> f64 {
        (t / 0.125).round() * 0.125
    }

    /// A fill of four toms and a crash, as written.
    fn written() -> Vec<Strike> {
        vec![
            s("Tom 1", 1.0),
            s("Tom 2", 1.125),
            s("Tom 3", 1.25),
            s("Tom 4", 1.375),
            s("Kick", 1.5),
        ]
    }

    #[test]
    fn the_take_that_plays_it_tight_wins() {
        let tight = Candidate {
            take: "2".into(),
            hits: written().iter().map(|n| s(&n.piece, n.t + 0.004)).collect(),
        };
        let loose = Candidate {
            take: "1".into(),
            hits: written()
                .iter()
                .zip([0.02, -0.018, 0.025, -0.02, 0.0])
                .map(|(n, d)| s(&n.piece, n.t + d))
                .collect(),
        };
        let dropped = Candidate {
            take: "3".into(),
            hits: written().iter().take(3).map(|n| s(&n.piece, n.t)).collect(),
        };
        let empty = Candidate {
            take: "4".into(),
            hits: Vec::new(),
        };
        let ranked = rank(&[loose, dropped, tight, empty], &written(), &line);
        let order: Vec<&str> = ranked.iter().map(|r| r.take.as_str()).collect();
        assert_eq!(order, ["2", "1", "3", "4"], "{ranked:#?}");
        assert!(ranked[3].empty);
        assert!((ranked[0].coverage - 1.0).abs() < 1e-9);
        assert!((ranked[2].coverage - 0.6).abs() < 1e-9);
    }

    /// A take pushed late all through is in time with itself: its feel is
    /// taken out before its spread is judged.
    #[test]
    fn a_steady_feel_is_not_loose() {
        let pushed = Candidate {
            take: "1".into(),
            hits: written().iter().map(|n| s(&n.piece, n.t + 0.030)).collect(),
        };
        let ranked = rank(&[pushed], &written(), &line);
        assert!(ranked[0].spread_ms < 0.5, "{ranked:?}");
        assert!((ranked[0].feel_ms - 30.0).abs() < 0.5);
    }

    /// With nothing written, the grid places the hits and the takes'
    /// usual count stands for coverage.
    #[test]
    fn with_nothing_written_the_grid_and_the_count_decide() {
        let on = Candidate {
            take: "1".into(),
            hits: (0..8)
                .map(|i| s("Snare", f64::from(i) * 0.125 + 0.002))
                .collect(),
        };
        let same = Candidate {
            take: "2".into(),
            hits: (0..8)
                .map(|i| s("Snare", f64::from(i) * 0.125 + 0.002))
                .collect(),
        };
        let extra = Candidate {
            take: "3".into(),
            hits: (0..12).map(|i| s("Snare", f64::from(i) * 0.09)).collect(),
        };
        let ranked = rank(&[extra, on, same], &[], &line);
        assert_eq!(
            ranked.last().map(|r| r.take.as_str()),
            Some("3"),
            "{ranked:#?}"
        );
    }

    #[test]
    fn stepping_goes_round_and_skips_the_empty() {
        let r = |take: &str, score: f64, empty: bool| Ranked {
            take: take.into(),
            score,
            hits: 1,
            coverage: score,
            spread_ms: 0.0,
            feel_ms: 0.0,
            empty,
        };
        let ranked = [r("2", 0.9, false), r("1", 0.5, false), r("4", 0.0, true)];
        assert_eq!(
            step(&ranked, Some("2"), 1).map(|r| r.take.as_str()),
            Some("1")
        );
        assert_eq!(
            step(&ranked, Some("1"), 1).map(|r| r.take.as_str()),
            Some("2")
        );
        assert_eq!(
            step(&ranked, Some("2"), -1).map(|r| r.take.as_str()),
            Some("1")
        );
        assert_eq!(step(&ranked, None, 1).map(|r| r.take.as_str()), Some("2"));
        assert_eq!(
            step(&ranked, Some("4"), 1).map(|r| r.take.as_str()),
            Some("2")
        );
    }
}
