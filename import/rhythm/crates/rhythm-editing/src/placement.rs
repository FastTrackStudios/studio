//! Where a recorded performance's audio plays once its hits are moved.
//!
//! [`place`] is a pure function of the recording and where each hit
//! should play — never of earlier edits. Each hit carries a *shift*:
//! where it plays minus where it was recorded. Hits in a row with the
//! same shift are one unbroken piece of the recording; a cut goes only
//! where the shift changes. So a hit moved and moved back is the
//! recording again, uncut, and the same placement always renders the
//! same pieces.
//!
//! Pieces follow one another in the order the hits now play; how far a
//! piece may run on or reach back goes by the recording — the attacks
//! either side of it as recorded, wherever those now play. Each seam is
//! put where both sides play natural audio: the piece before runs on
//! through its own decay up to just before the next attack in its
//! recording, the piece after reaches back into its own lead-in — never
//! back over the attack before it. A hit moved past others (a snare a beat
//! later, over the hats between) plays there, and the hole it left is
//! filled the same way. Only what neither side covers is drawn out, the
//! decay before over what is left: [`Piece::drawn_to`].

/// How a placement cuts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    /// How far before a hit's attack its piece starts: the stick's
    /// approach is in the recording just before the transient.
    pub lead: f64,
    /// How far past a hit's attack the piece after may not reach back: a
    /// hit played twice is a flam.
    pub guard: f64,
}

impl Default for Cut {
    fn default() -> Self {
        Self {
            lead: 0.005,
            guard: 0.015,
        }
    }
}

/// A hit as recorded and how far it is moved, seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Its attack, in the recording's own time.
    pub at: f64,
    /// Where it plays minus where it was recorded.
    pub shift: f64,
}

/// A stretch of the recording played somewhere on the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    /// Where it starts playing, timeline seconds.
    pub start: f64,
    /// Where it stops, timeline seconds.
    pub end: f64,
    /// Timeline minus recording time: it plays the recording from
    /// `start - shift`.
    pub shift: f64,
    /// The last resort: the recording only reaches `end` naturally, and
    /// its tail is drawn out to here to meet the next piece.
    pub drawn_to: Option<f64>,
}

impl Piece {
    /// Where it stops sounding: drawn out, or its natural end.
    #[must_use]
    pub fn reach(&self) -> f64 {
        self.drawn_to.unwrap_or(self.end)
    }
}

/// Shifts this close are one shift, seconds.
const SAME: f64 = 1e-6;

/// The pieces that play the recording over `start..end` (timeline) with
/// `hits` moved — `hits` sorted by `at`. The first piece starts at
/// `start` and the last ends at `end`, each with the shift of the hit
/// nearest it, so a span re-placed inside a longer edit meets what is
/// either side of it unchanged.
#[must_use]
pub fn place(hits: &[Hit], span: (f64, f64), cut: Cut) -> Vec<Piece> {
    place_within(hits, span, cut, (None, None))
}

/// [`place`] for a span cut out of a longer edit: `outside` are the
/// recorded attacks just before the first of `hits` and just after the
/// last, which the pieces at the span's ends may not play either — the
/// last piece stops short of the next attack (drawn out to the span's end
/// if it must), as a seam inside would.
#[must_use]
pub fn place_within(
    hits: &[Hit],
    (start, end): (f64, f64),
    cut: Cut,
    outside: (Option<f64>, Option<f64>),
) -> Vec<Piece> {
    let mut runs = runs(hits);
    if let Some(first) = runs.first_mut() {
        first.before = first.before.or(outside.0);
    }
    if let Some(last) = runs.last_mut() {
        last.after = last.after.or(outside.1);
    }
    let Some(first) = runs.first() else {
        return vec![Piece {
            start,
            end,
            shift: 0.0,
            drawn_to: None,
        }];
    };
    let mut pieces = vec![Piece {
        start,
        end,
        shift: first.shift,
        drawn_to: None,
    }];
    for pair in runs.windows(2) {
        let [before, after] = pair else {
            continue;
        };
        let Some(open) = pieces.last_mut() else {
            continue;
        };
        let seam = seam(before, after, cut);
        open.end = seam.ends;
        open.drawn_to = seam.drawn_to;
        pieces.push(Piece {
            start: seam.starts,
            end,
            shift: after.shift,
            drawn_to: None,
        });
    }
    // The last piece stops before the next attack in its recording, as a
    // seam would stop it; what is left to the span's end is drawn out.
    if let (Some(last), Some(run)) = (pieces.last_mut(), runs.last())
        && let Some(next) = run.after
    {
        let runs_to = next - cut.lead + run.shift;
        if runs_to < last.end && runs_to > last.start {
            last.drawn_to = Some(last.end);
            last.end = runs_to;
        }
    }
    pieces
}

/// Hits one after another both as recorded and as they play, with one
/// shift: one unbroken piece of the recording.
#[derive(Clone, Copy, Debug)]
struct Run {
    /// Its first hit's attack, recording time.
    first: f64,
    shift: f64,
    /// The recorded attacks either side of it — whoever plays where now:
    /// how far it may reach back and run on.
    before: Option<f64>,
    after: Option<f64>,
}

/// `hits` (by `at`) as runs, in the order they now play.
fn runs(hits: &[Hit]) -> Vec<Run> {
    let mut order: Vec<usize> = (0..hits.len()).collect();
    order.sort_by(|&a, &b| {
        let plays = |i: usize| hits[i].at + hits[i].shift;
        plays(a).total_cmp(&plays(b)).then(a.cmp(&b))
    });
    let at = |i: Option<usize>| i.and_then(|i| hits.get(i)).map(|h| h.at);
    let mut runs: Vec<(Run, usize)> = Vec::new();
    for i in order {
        let hit = hits[i];
        match runs.last_mut() {
            // The next as recorded, with the same shift: the same piece.
            Some((run, last)) if *last + 1 == i && (run.shift - hit.shift).abs() < SAME => {
                *last = i;
                run.after = at(Some(i + 1));
            }
            _ => runs.push((
                Run {
                    first: hit.at,
                    shift: hit.shift,
                    before: at(i.checked_sub(1)),
                    after: at(Some(i + 1)),
                },
                i,
            )),
        }
    }
    runs.into_iter().map(|(run, _)| run).collect()
}

/// Where the piece before a seam stops and the one after starts, and how
/// far the one before is drawn out to meet it if it must be.
struct Seam {
    ends: f64,
    starts: f64,
    drawn_to: Option<f64>,
}

/// The seam between two runs, in the order they play. The piece after
/// starts at its lead — or as early as its own recording allows, reaching
/// back into its lead-in, to meet the piece before; that one runs on
/// through its own decay up to just before the next attack in its
/// recording. What neither covers is the gap the decay is drawn over.
fn seam(before: &Run, after: &Run, cut: Cut) -> Seam {
    let lead = after.first + after.shift - cut.lead;
    // The latest the piece before may play to: its recording, up to the
    // lead into the next attack recorded after it.
    let runs_to = before
        .after
        .map_or(f64::INFINITY, |next| next - cut.lead + before.shift);
    // The earliest the piece after may start: its recording, from just
    // past the attack recorded before it — and never after its own lead.
    let reaches_to = after
        .before
        .map_or(f64::NEG_INFINITY, |prev| prev + cut.guard + after.shift)
        .min(lead);
    // The piece before reaches the lead: it is trimmed to it.
    if lead <= runs_to {
        return Seam {
            ends: lead,
            starts: lead,
            drawn_to: None,
        };
    }
    // It stops short: the piece after reaches back to meet it.
    if reaches_to <= runs_to {
        return Seam {
            ends: runs_to,
            starts: runs_to,
            drawn_to: None,
        };
    }
    // Neither reaches: the decay before is drawn out over what is left.
    Seam {
        ends: runs_to,
        starts: reaches_to,
        drawn_to: Some(reaches_to),
    }
}
