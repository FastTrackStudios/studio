//! T / Shift+T shown before the engine has comped: which take the press
//! comps in, from the ranking the engine last gave for the stretch
//! (`COMP_CANDIDATES`, asked as it is picked, or the last `COMP_CYCLE`'s
//! reply), and the comp as it will be once it has.
//!
//! The engine still does the comp, through `COMP_CYCLE`; this only lets
//! the window draw it on the frame of the key.
//!
//! ```ignore
//! let mut ranks = Rankings::default();
//! ranks.learn(&spans, &reply);                  // a COMP_CANDIDATES or COMP_CYCLE reply
//! let next = ranks.step(&spans, 1);             // [(from, to, "260906_1808-01")]
//! let comps = splice(&overlay.comps, &next);    // the comp as it will be
//! ```

use crate::hit_overlay::CompSpan;

/// One take of a stretch's ranking, as the engine said it.
#[derive(Clone, Debug, PartialEq)]
struct Rank {
    /// Its pass: what the comp is made from.
    take: String,
    /// Whether T steps to it: it holds audio there.
    plays: bool,
}

/// A stretch the engine ranked: where it is, its takes in order, and the
/// one the comp plays there.
#[derive(Clone, Debug, PartialEq)]
struct Ranked {
    span: (f64, f64),
    takes: Vec<Rank>,
    current: Option<String>,
}

/// The rankings the engine gave, by the stretches the window asked about.
#[derive(Clone, Debug, Default)]
pub struct Rankings {
    /// The spans asked for, and what came back for them: the last few.
    known: Vec<(Vec<(f64, f64)>, Vec<Ranked>)>,
}

/// How many asks are remembered: a T steps over the stretch just picked.
const KEPT: usize = 8;

fn same(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x.0 - y.0).abs() < 1e-3 && (x.1 - y.1).abs() < 1e-3)
}

impl Rankings {
    /// Whether `asked`'s ranking is known.
    #[must_use]
    pub fn knows(&self, asked: &[(f64, f64)]) -> bool {
        self.known.iter().any(|(a, _)| same(a, asked))
    }

    /// What the engine said for `asked`: a `COMP_CANDIDATES` or
    /// `COMP_CYCLE` reply, a list of `{from, to, takes: [{take, current,
    /// empty, unplayable, rating}]}`. Not a list of those: nothing learnt.
    pub fn learn(&mut self, asked: &[(f64, f64)], reply: &serde_json::Value) {
        let Some(stretches) = reply.as_array() else {
            return;
        };
        let ranked: Vec<Ranked> = stretches
            .iter()
            .filter_map(|s| {
                let span = (s.get("from")?.as_f64()?, s.get("to")?.as_f64()?);
                let mut current = None;
                let takes = s
                    .get("takes")?
                    .as_array()?
                    .iter()
                    .filter_map(|t| {
                        let take = t.get("take")?.as_str()?.to_owned();
                        let flag = |k: &str| t.get(k).and_then(serde_json::Value::as_bool);
                        if flag("current") == Some(true) {
                            current = Some(take.clone());
                        }
                        let rated_out = t
                            .get("rating")
                            .and_then(serde_json::Value::as_i64)
                            .is_some_and(|r| r < 0);
                        let plays = flag("empty") != Some(true)
                            && flag("unplayable") != Some(true)
                            && !rated_out;
                        Some(Rank { take, plays })
                    })
                    .collect();
                Some(Ranked {
                    span,
                    takes,
                    current,
                })
            })
            .collect();
        self.known.retain(|(a, _)| !same(a, asked));
        self.known.push((asked.to_vec(), ranked));
        if self.known.len() > KEPT {
            self.known.remove(0);
        }
    }

    /// The take a press `by` (1: T, -1: Shift+T) comps in over each of
    /// `asked`'s stretches — the step `COMP_CYCLE` takes
    /// (`editing::comp::step`) — remembered as now playing there, so a
    /// second press before the engine answers steps on from it. Empty
    /// when `asked` was never ranked.
    pub fn step(&mut self, asked: &[(f64, f64)], by: i32) -> Vec<(f64, f64, String)> {
        let Some((_, ranked)) = self.known.iter_mut().find(|(a, _)| same(a, asked)) else {
            return Vec::new();
        };
        ranked
            .iter_mut()
            .filter_map(|r| {
                let playing: Vec<&str> = r
                    .takes
                    .iter()
                    .filter(|t| t.plays)
                    .map(|t| t.take.as_str())
                    .collect();
                let n = i64::try_from(playing.len()).ok().filter(|n| *n > 0)?;
                let at = r
                    .current
                    .as_deref()
                    .and_then(|c| playing.iter().position(|t| *t == c));
                let next = match at {
                    Some(i) => (i64::try_from(i).ok()? + i64::from(by)).rem_euclid(n),
                    None => 0,
                };
                let take = (*playing.get(usize::try_from(next).ok()?)?).to_owned();
                r.current = Some(take.clone());
                Some((r.span.0, r.span.1, take))
            })
            .collect()
    }
}

/// `comps` with each of `comped`'s stretches played from its take: the
/// spans it covers cut back around it, in time order.
#[must_use]
pub fn splice(comps: &[CompSpan], comped: &[(f64, f64, String)]) -> Vec<CompSpan> {
    let mut out: Vec<CompSpan> = comps.to_vec();
    for (from, to, take) in comped {
        let mut next = Vec::with_capacity(out.len() + 2);
        for span in out {
            if span.end <= *from || span.start >= *to {
                next.push(span);
                continue;
            }
            if span.start < *from {
                next.push(CompSpan {
                    end: *from,
                    ..span.clone()
                });
            }
            if span.end > *to {
                next.push(CompSpan { start: *to, ..span });
            }
        }
        next.push(CompSpan {
            start: *from,
            end: *to,
            take: take.clone(),
        });
        next.sort_by(|a, b| a.start.total_cmp(&b.start));
        out = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{CompSpan, Rankings, splice};

    fn span(start: f64, end: f64, take: &str) -> CompSpan {
        CompSpan {
            start,
            end,
            take: take.to_owned(),
        }
    }

    #[test]
    fn a_press_steps_through_the_takes_that_play_as_the_engine_does() {
        let asked = [(10.0, 12.0)];
        let mut ranks = Rankings::default();
        assert!(ranks.step(&asked, 1).is_empty(), "never ranked: no guess");
        ranks.learn(
            &asked,
            &serde_json::json!([{ "from": 10.1, "to": 12.0, "takes": [
                { "take": "A", "current": true },
                { "take": "B", "empty": true },
                { "take": "C", "rating": 2 },
                { "take": "D", "rating": -1 },
            ]}]),
        );
        assert_eq!(ranks.step(&asked, 1), vec![(10.1, 12.0, "C".to_owned())]);
        assert_eq!(
            ranks.step(&asked, 1),
            vec![(10.1, 12.0, "A".to_owned())],
            "round again, from the take the last press comped"
        );
        assert_eq!(ranks.step(&asked, -1), vec![(10.1, 12.0, "C".to_owned())]);
    }

    #[test]
    fn a_comped_stretch_cuts_the_spans_it_covers() {
        let comps = [
            span(0.0, 5.0, "A"),
            span(5.0, 9.0, "B"),
            span(9.0, 20.0, "A"),
        ];
        assert_eq!(
            splice(&comps, &[(4.0, 10.0, "C".to_owned())]),
            vec![
                span(0.0, 4.0, "A"),
                span(4.0, 10.0, "C"),
                span(10.0, 20.0, "A")
            ]
        );
    }
}
