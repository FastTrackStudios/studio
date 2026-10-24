//! Which take plays a stretch best: the takes of a part ranked for a
//! stretch of it — a fill, a bar, a selection — from their hits.
//!
//! The basics of a kit are quantized; what is left is choosing, fill by
//! fill, the take the drummer played it best in. Each take's version of
//! the stretch is held up to where its hits belong — the part as written
//! (the drum MIDI) when there is one, else the grid — and scored:
//!
//! - **plays it**: a take with no hits there is empty there (a drummer
//!   stopping, a take that ends early) and goes last;
//! - **tight**: how far its hits sit from where they belong once its own
//!   feel is taken out — a take pushed 20 ms all through is in time with
//!   itself; one that rushes and drags is not. A few hits prove little:
//!   one hit is never off from itself, so a spread counts as it is only
//!   once a take plays enough of them.
//!
//! - **plays the part**: how alike its hits are to the reference — what
//!   the comp played there before (`before`), when that is the part the
//!   takes agree on, else the part itself: the hits most takes play
//!   ([`typical`]: the take most alike to all the others). A take where the drummer is trying something else,
//!   practising, playing twice as much, is not the part however tight it
//!   is; one close to it reaches in where the reference is not good
//!   enough.
//!
//! The part as written (the drum MIDI) only says where a hit belongs, not
//! what a take should play: how much of it a take plays (`coverage`) is
//! reported beside the score, never in it.
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
    /// How alike its hits are to the reference, 0..1 ([`similarity`]).
    pub similar: f64,
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

/// How many hits' worth of doubt a take's spread starts from: until it
/// plays more than a few, its spread is taken as half way to loose — a
/// take with one hit has no spread to speak of and proves nothing.
const DOUBT_HITS: f64 = 3.0;

/// A spread measured over `hits` hits, as judged: drawn toward half way
/// to loose by [`DOUBT_HITS`] hits' worth, ms.
fn judged_ms(spread_ms: f64, hits: usize) -> f64 {
    let n = hits as f64;
    n.mul_add(spread_ms, DOUBT_HITS * LOOSE_MS / 2.0) / (n + DOUBT_HITS)
}

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

/// How near two takes' hits of one piece must be to be one stroke of the
/// part, seconds, once the takes are lined up: nearer than one take's
/// hits wander from another's, further than a flam's two strokes are apart.
const SAME_STROKE: f64 = 0.035;

/// How far one take may sit from another all through and still be lined
/// up with it, seconds: a take played behind or ahead of another.
const LINE_UP: f64 = 0.05;
/// The step its lining up is tried in, seconds.
const LINE_UP_STEP: f64 = 0.005;

/// A take this alike to the part plays it: what the comp played before
/// is the reference only when it is at least this alike.
const PLAYS_PART: f64 = 0.6;

/// The part as the takes play it: the most typical take — the one most
/// alike to all the others ([`similarity`]), among those with hits here.
/// `None` when no two takes play anything alike: there is no part.
#[must_use]
pub fn typical(candidates: &[Candidate]) -> Option<&Candidate> {
    let playing: Vec<&Candidate> = candidates.iter().filter(|c| !c.hits.is_empty()).collect();
    playing
        .iter()
        .map(|c| {
            let alike: f64 = playing
                .iter()
                .filter(|o| o.take != c.take)
                .map(|o| similarity(&c.hits, &o.hits))
                .sum();
            (*c, alike)
        })
        .filter(|(_, alike)| *alike > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.take.cmp(&a.0.take)))
        .map(|(c, _)| c)
}

/// How alike `hits` are to `reference`, 0..1, lined up first (the best of
/// sitting up to [`LINE_UP`] behind or ahead of it all through): the
/// strokes they share — a hit of the same piece within [`SAME_STROKE`],
/// each met once — over all the strokes of both, so a hit missing and a
/// hit extra cost alike. Both empty: alike.
#[must_use]
pub fn similarity(hits: &[Strike], reference: &[Strike]) -> f64 {
    if hits.is_empty() && reference.is_empty() {
        return 1.0;
    }
    let steps = (LINE_UP / LINE_UP_STEP).round() as i32;
    let shared = (-steps..=steps)
        .map(|k| shared_strokes(hits, reference, f64::from(k) * LINE_UP_STEP))
        .max()
        .unwrap_or(0);
    2.0 * shared as f64 / (hits.len() + reference.len()) as f64
}

/// How many of `reference`'s strokes `hits`, moved by `by`, play.
fn shared_strokes(hits: &[Strike], reference: &[Strike], by: f64) -> usize {
    let mut used = vec![false; hits.len()];
    let mut shared = 0usize;
    for r in reference {
        let best = hits
            .iter()
            .enumerate()
            .filter(|(i, h)| h.piece == r.piece && !used.get(*i).copied().unwrap_or(true))
            .map(|(i, h)| (i, (h.t + by - r.t).abs()))
            .filter(|(_, d)| *d <= SAME_STROKE)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best {
            if let Some(slot) = used.get_mut(i) {
                *slot = true;
            }
            shared = shared.saturating_add(1);
        }
    }
    shared
}

/// What the takes of a stretch are held up to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference<'a> {
    /// The part as the takes play it ([`typical`]).
    Part,
    /// The take the comp played there, when it plays the part — else
    /// the part: a practice take comped by mistake is no reference.
    Before(&'a str),
    /// The take chosen for the stretch by ear, whatever the others play.
    Chosen(&'a str),
}

/// The hits `wanted` holds the takes up to; `None` when there is nothing
/// to hold them to (no part, and no chosen take with hits).
fn reference(candidates: &[Candidate], wanted: Reference<'_>) -> Option<Vec<Strike>> {
    let hits_of = |take: &str| {
        candidates
            .iter()
            .find(|c| c.take == take && !c.hits.is_empty())
            .map(|c| c.hits.clone())
    };
    if let Reference::Chosen(take) = wanted
        && let Some(hits) = hits_of(take)
    {
        return Some(hits);
    }
    let part = typical(candidates)?.hits.clone();
    let before = match wanted {
        Reference::Before(take) => hits_of(take).filter(|h| similarity(h, &part) >= PLAYS_PART),
        Reference::Part | Reference::Chosen(_) => None,
    };
    Some(before.unwrap_or(part))
}

/// The takes ranked for a stretch, best first: by how alike each is to
/// the reference ([`Reference`]) and, among the alike, how tight.
/// `written` is the part as written there (empty when there is none);
/// `line_of` gives a time's nearest grid line, for hits with no note to
/// belong to.
#[must_use]
pub fn rank(
    candidates: &[Candidate],
    written: &[Strike],
    line_of: &dyn Fn(f64) -> f64,
    held_to: Reference<'_>,
) -> Vec<Ranked> {
    let reference = reference(candidates, held_to);
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
            let tight = 1.0 - (judged_ms(spread_ms, c.hits.len()) / LOOSE_MS).clamp(0.0, 1.0);
            // No part: every take alike, judged on how it plays.
            let similar = reference.as_deref().map_or(1.0, |r| similarity(&c.hits, r));
            // Alike first: half alike is a quarter as good; then tight.
            let score = if empty {
                0.0
            } else {
                similar * similar * tight.mul_add(0.5, 0.5)
            };
            Ranked {
                take: c.take.clone(),
                score,
                hits: c.hits.len(),
                coverage,
                similar,
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
