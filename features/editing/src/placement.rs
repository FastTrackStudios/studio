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
//! Each seam is put where both sides play natural audio: the piece before
//! runs on through its own decay up to just before the next hit's
//! recorded attack, and the piece after reaches back into its own lead-in
//! — never back over the hit before it. Only where neither can cover the
//! gap a move left (a hit sent later than its neighbour is from it) is
//! the decay before drawn out, over what is left: [`Piece::drawn_to`].

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
/// `hits` moved — `hits` sorted by `at`, and still in that order once
/// moved (a move past another hit reorders the performance, which no
/// seam can play: the caller keeps the order, as a quantize does).
/// The first piece starts at `start` and the last ends at `end`, each with
/// the shift of the hits nearest it, so a span re-placed inside a longer
/// edit meets what is either side of it unchanged.
#[must_use]
pub fn place(hits: &[Hit], (start, end): (f64, f64), cut: Cut) -> Vec<Piece> {
    let runs = runs(hits);
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
    pieces
}

/// Hits in a row with one shift: one unbroken piece of the recording.
#[derive(Clone, Copy, Debug)]
struct Run {
    /// Its first hit's attack and its last's, recording time.
    first: f64,
    last: f64,
    shift: f64,
}

fn runs(hits: &[Hit]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for hit in hits {
        match runs.last_mut() {
            Some(run) if (run.shift - hit.shift).abs() < SAME => run.last = hit.at,
            _ => runs.push(Run {
                first: hit.at,
                last: hit.at,
                shift: hit.shift,
            }),
        }
    }
    runs
}

/// Where the piece before a seam stops and the one after starts, and how
/// far the one before is drawn out to meet it if it must be.
struct Seam {
    ends: f64,
    starts: f64,
    drawn_to: Option<f64>,
}

/// The seam between two runs. The piece after starts at its lead — or,
/// moved later than the piece before, as early as its own recording
/// allows, reaching back into its lead-in; the piece before runs on to
/// meet it, through its own decay, up to just before the next hit's
/// recorded attack. What neither covers is the gap the decay is drawn
/// over.
fn seam(before: &Run, after: &Run, cut: Cut) -> Seam {
    let lead = after.first + after.shift - cut.lead;
    // The latest the piece before may play to: its recording, up to the
    // lead into the hit after.
    let runs_to = after.first - cut.lead + before.shift;
    // The earliest the piece after may start: its recording, from just
    // past the hit before it — and never after its own lead.
    let reaches_to = (before.last + cut.guard + after.shift).min(lead);
    // Moved earlier (or the same), the piece after starts at its lead
    // and the one before is trimmed to it.
    if lead <= runs_to {
        return Seam {
            ends: lead,
            starts: lead,
            drawn_to: None,
        };
    }
    // Moved later: the piece before runs on as far as it naturally can,
    // the piece after reaches back to meet it.
    let starts = reaches_to.max(runs_to);
    if starts <= runs_to {
        return Seam {
            ends: runs_to.min(lead),
            starts: runs_to.min(lead),
            drawn_to: None,
        };
    }
    // Neither reaches: the decay before is drawn out over what is left.
    Seam {
        ends: runs_to,
        starts,
        drawn_to: Some(starts),
    }
}

#[cfg(test)]
mod tests {
    use super::{Cut, Hit, Piece, place};

    const CUT: Cut = Cut {
        lead: 0.005,
        guard: 0.015,
    };

    /// A hit every `gap` seconds from 1 s, each shifted as given.
    fn every(gap: f64, shifts: &[f64]) -> Vec<Hit> {
        shifts
            .iter()
            .enumerate()
            .map(|(i, &shift)| Hit {
                at: 1.0 + gap * i as f64,
                shift,
            })
            .collect()
    }

    /// Eighth notes at 120 bpm: a hit every 0.25 s.
    fn eighths(shifts: &[f64]) -> Vec<Hit> {
        every(0.25, shifts)
    }

    fn natural(pieces: &[Piece]) -> bool {
        pieces.iter().all(|p| p.drawn_to.is_none())
    }

    #[test]
    fn nothing_moved_is_the_recording_uncut() {
        let pieces = place(&eighths(&[0.0; 8]), (0.0, 4.0), CUT);
        assert_eq!(
            pieces,
            [Piece {
                start: 0.0,
                end: 4.0,
                shift: 0.0,
                drawn_to: None
            }]
        );
    }

    #[test]
    fn a_hit_moved_and_moved_back_is_the_recording_again() {
        let mut shifts = [0.0; 8];
        shifts[3] = 0.25;
        let moved = place(&eighths(&shifts), (0.0, 4.0), CUT);
        assert!(moved.len() > 1, "a moved hit is cut out: {moved:?}");
        shifts[3] = 0.0;
        let back = place(&eighths(&shifts), (0.0, 4.0), CUT);
        assert_eq!(back, place(&eighths(&[0.0; 8]), (0.0, 4.0), CUT));
    }

    #[test]
    fn a_hit_moved_later_by_less_than_its_gap_needs_no_stretch() {
        let mut shifts = [0.0; 8];
        shifts[3] = 0.2;
        let pieces = place(&eighths(&shifts), (0.0, 4.0), CUT);
        assert!(natural(&pieces), "{pieces:?}");
        assert_gapless(&pieces);
    }

    #[test]
    fn a_snare_moved_an_eighth_later_between_quarter_notes_plays_natural_audio() {
        // Quarter notes; the snare on the fourth sent an eighth later.
        // The piece before runs on through its decay, the snare's reaches
        // back into its own lead-in, and between them they cover the gap.
        let mut shifts = [0.0; 8];
        shifts[3] = 0.25;
        let pieces = place(&every(0.5, &shifts), (0.0, 6.0), CUT);
        assert!(natural(&pieces), "{pieces:?}");
        assert_eq!(pieces.len(), 3, "cut out, nothing else: {pieces:?}");
        assert_gapless(&pieces);
    }

    #[test]
    fn a_hit_moved_almost_onto_the_next_is_drawn_out_only_over_what_is_left() {
        // Sent later by nearly all of its gap to the next hit: more than
        // the recording either side of the hole it leaves can cover.
        let mut shifts = [0.0; 8];
        shifts[3] = 0.49;
        let pieces = place(&every(0.5, &shifts), (0.0, 6.0), CUT);
        let drawn: Vec<&Piece> = pieces.iter().filter(|p| p.drawn_to.is_some()).collect();
        assert_eq!(drawn.len(), 1, "one decay drawn out: {pieces:?}");
        let drawn_over = drawn[0].reach() - drawn[0].end;
        assert!(
            drawn_over < 0.02,
            "drawn out over the leftover only, {drawn_over} s: {pieces:?}"
        );
        assert_gapless(&pieces);
    }

    #[test]
    fn a_hit_moved_earlier_starts_at_its_lead() {
        let mut shifts = [0.0; 8];
        shifts[3] = -0.02;
        let pieces = place(&eighths(&shifts), (0.0, 4.0), CUT);
        let snare = pieces
            .iter()
            .find(|p| (p.shift + 0.02).abs() < 1e-9)
            .expect("the moved hit's piece");
        assert!(
            (snare.start - (1.75 - 0.02 - CUT.lead)).abs() < 1e-9,
            "{pieces:?}"
        );
        assert!(natural(&pieces));
        assert_gapless(&pieces);
    }

    #[test]
    fn a_piece_never_plays_the_hit_before_it_twice() {
        let mut shifts = [0.0; 8];
        shifts[3] = 0.2;
        for piece in place(&eighths(&shifts), (0.0, 4.0), CUT) {
            let from = piece.start - piece.shift;
            for hit in eighths(&[0.0; 8]) {
                let played_at = hit.at + piece.shift;
                let in_piece = played_at > piece.start && played_at < piece.end;
                // A hit inside a piece is that piece's own (its shift is
                // the hit's); a reach-back stops past the hit before.
                assert!(
                    !in_piece || hit.at - from >= -1e-9,
                    "{piece:?} replays {hit:?}"
                );
            }
        }
    }

    fn assert_gapless(pieces: &[Piece]) {
        for pair in pieces.windows(2) {
            assert!(
                (pair[0].reach() - pair[1].start).abs() < 1e-9,
                "a gap or overlap: {pieces:?}"
            );
        }
    }
}
