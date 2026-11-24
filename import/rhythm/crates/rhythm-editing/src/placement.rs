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
//! either side of it as recorded, wherever those now play.
//!
//! # The gap fill
//!
//! Where a hit moves later than the piece before can reach, there is a
//! gap. It is filled the way it sounds most natural, and never by moving
//! a hit ([`place_filled`]):
//!
//! 1. The piece before plays on at its own speed through its own decay, up
//!    to just before the next attack in its recording (`cut.lead` before
//!    it): that attack never sounds twice.
//! 2. The piece after reaches back into its own take before its attack,
//!    no nearer than `cut.guard` past the attack before it in the
//!    recording, never past its take's start.
//! 3. Where the two overlap they crossfade, decay into lead-in, over at
//!    most [`Fill::most_crossfade`] ending where the piece before has to
//!    stop. The same pieces go to every mic of the kit: one set of cuts
//!    and fades, the phase between the mics kept.
//! 4. Only what is still left is stretched: the decay before (a drum's
//!    tail stretches unheard; the lead-in after sits against an attack
//!    and is never slowed), only its last span — the shortest that
//!    reaches at [`Fill::least_rate`] — and never its attack
//!    ([`Fill::keep`]). Past what that reaches the rest is left silent.
//! 5. At a take switch a side first stops at its take's end; if that
//!    leaves a stretch or a silence, it plays on through the comp's own
//!    switch there instead (the comp as recorded, never past an attack):
//!    [`TakeSwitch::Crossed`].
//!
//! Each gap is reported ([`Filled`]): how far the piece before was
//! extended, how far the piece after reached back, how long they
//! crossfade, what was stretched and at what rate, and what was left.

pub use daw_edit::placement::{Cut, Fill, Filled, Hit, Piece, Stretched, Take, TakeSwitch};

/// Shifts this close are one shift, seconds.
const SAME: f64 = 1e-6;

/// A move later than this opens a gap worth reporting, seconds.
const GAP: f64 = 1e-4;

/// The pieces, and what was done at each gap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placed {
    pub pieces: Vec<Piece>,
    pub fills: Vec<Filled>,
}

/// The pieces that play the recording over `start..end` (timeline) with
/// `hits` moved — `hits` sorted by `at`. The first piece starts at
/// `start` and the last ends at `end`, each with the shift of the hit
/// nearest it, so a span re-placed inside a longer edit meets what is
/// either side of it unchanged. One take throughout, the default fill.
#[must_use]
pub fn place(hits: &[Hit], span: (f64, f64), cut: Cut) -> Vec<Piece> {
    place_within(hits, span, cut, (None, None))
}

/// [`place`] for a span cut out of a longer edit: `outside` are the
/// recorded attacks just before the first of `hits` and just after the
/// last, which the pieces at the span's ends may not play either.
#[must_use]
pub fn place_within(
    hits: &[Hit],
    span: (f64, f64),
    cut: Cut,
    outside: (Option<f64>, Option<f64>),
) -> Vec<Piece> {
    place_filled(hits, span, cut, outside, (&[], Fill::default())).pieces
}

/// [`place_within`] over a recording made of `takes` (recording time, in
/// order; none: one take), each gap filled by `fill`'s rule (the module's
/// docs) and reported.
#[must_use]
pub fn place_filled(
    hits: &[Hit],
    (start, end): (f64, f64),
    cut: Cut,
    outside: (Option<f64>, Option<f64>),
    (takes, fill): (&[Take], Fill),
) -> Placed {
    let mut runs = runs(hits);
    if let Some(first) = runs.first_mut() {
        first.before = first.before.or(outside.0);
    }
    if let Some(last) = runs.last_mut() {
        last.after = last.after.or(outside.1);
    }
    let Some(first) = runs.first() else {
        return Placed {
            pieces: vec![whole(start, end, 0.0)],
            fills: Vec::new(),
        };
    };
    let rules = Rules { cut, fill, takes };
    let mut pieces = vec![whole(start, end, first.shift)];
    let mut fills = Vec::new();
    for pair in runs.windows(2) {
        let [before, after] = pair else {
            continue;
        };
        let Some(open) = pieces.last_mut() else {
            continue;
        };
        let seam = rules.seam(before, after, open.start);
        open.end = seam.ends;
        open.drawn_to = seam.draw.map(|d| d.to);
        open.drawn_from = seam.draw.map(|d| d.from);
        if let Some(report) = seam.report {
            fills.push(report);
        }
        pieces.push(Piece {
            start: seam.starts,
            crossfade: Some(seam.crossfade),
            ..whole(seam.starts, end, after.shift)
        });
    }
    // The last piece stops before the next attack in its recording (and at
    // its take's end), as a seam would stop it; what is left to the span's
    // end is filled the same way.
    if let (Some(last), Some(run)) = (pieces.last_mut(), runs.last())
        && let Some(tail) = rules.tail(run, (last.start, last.end), outside.1)
    {
        last.end = tail.ends;
        last.drawn_to = tail.draw.map(|d| d.to);
        last.drawn_from = tail.draw.map(|d| d.from);
        if let Some(report) = tail.report {
            fills.push(report);
        }
    }
    Placed { pieces, fills }
}

fn whole(start: f64, end: f64, shift: f64) -> Piece {
    Piece {
        start,
        end,
        shift,
        drawn_to: None,
        drawn_from: None,
        crossfade: None,
    }
}

/// Hits one after another both as recorded and as they play, with one
/// shift: one unbroken piece of the recording.
#[derive(Clone, Copy, Debug)]
struct Run {
    /// Its first and last hit's attacks, recording time.
    first: f64,
    last: f64,
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
                run.last = hit.at;
                run.after = at(Some(i + 1));
            }
            _ => runs.push((
                Run {
                    first: hit.at,
                    last: hit.at,
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

/// A residual stretch: the piece's recording from `from` (timeline) to its
/// end drawn over `from..to`.
#[derive(Clone, Copy, Debug)]
struct Draw {
    from: f64,
    to: f64,
}

/// Where the piece before a seam stops (and how it is drawn out, if it
/// must be), where the one after starts and how long they crossfade.
struct Seam {
    ends: f64,
    starts: f64,
    crossfade: f64,
    draw: Option<Draw>,
    report: Option<Filled>,
}

/// The end of the span's last piece.
struct Tail {
    ends: f64,
    draw: Option<Draw>,
    report: Option<Filled>,
}

struct Rules<'a> {
    cut: Cut,
    fill: Fill,
    takes: &'a [Take],
}

impl Rules<'_> {
    /// The take holding `t` (recording time): the latest to start by it.
    fn take(&self, t: f64) -> Take {
        self.takes
            .iter()
            .rev()
            .find(|k| k.from <= t + SAME && t < k.to + SAME)
            .copied()
            .unwrap_or(Take {
                from: f64::NEG_INFINITY,
                to: f64::INFINITY,
            })
    }

    /// The latest the run may play to at its own speed, recording time:
    /// just before the next attack in its recording, and (`within`) its
    /// take's end — whether the take's end is what stops it.
    fn runs_on(&self, run: &Run, next: Option<f64>, within: bool) -> (f64, bool) {
        let attack = next.map_or(f64::INFINITY, |n| n - self.cut.lead);
        let take = self.take(run.last).to;
        if within && take < attack {
            (take, true)
        } else {
            (attack, false)
        }
    }

    /// The earliest the run may start, recording time: `cut.guard` past
    /// the attack before it in its recording, and (`within`) its take's
    /// start — whether the take's start is what stops it.
    fn reaches_back(&self, run: &Run, within: bool) -> (f64, bool) {
        let attack = run.before.map_or(f64::NEG_INFINITY, |p| p + self.cut.guard);
        let take = self.take(run.first).from;
        if within && take > attack {
            (take, true)
        } else {
            (attack, false)
        }
    }

    /// The seam between two runs, in the order they play (the module's
    /// rule); `opened` is where the piece before starts.
    fn seam(&self, before: &Run, after: &Run, opened: f64) -> Seam {
        let within = self.seam_within(before, after, opened, true);
        let leaves = |s: &Seam| s.report.as_ref().map_or(0.0, stretch_cost);
        if leaves(&within) > 0.0
            && within
                .report
                .as_ref()
                .is_some_and(|r| r.take_switch == TakeSwitch::Stopped)
        {
            let crossed = self.seam_within(before, after, opened, false);
            if leaves(&crossed) < leaves(&within) - SAME {
                return crossed;
            }
        }
        within
    }

    fn seam_within(&self, before: &Run, after: &Run, opened: f64, within: bool) -> Seam {
        let lead = after.first + after.shift - self.cut.lead;
        let (runs_rec, b_take) = self.runs_on(before, before.after, within);
        let (reach_rec, a_take) = self.reaches_back(after, within);
        let runs_to = runs_rec + before.shift;
        let reaches_to = (reach_rec + after.shift).min(lead);
        // Where the piece before was cut before the move: just before this
        // hit's old place, as it plays at the before's shift.
        let old_cut = after.first + before.shift - self.cut.lead;
        let gap = lead - old_cut;
        let switch = if !within {
            TakeSwitch::Crossed
        } else if b_take || a_take {
            TakeSwitch::Stopped
        } else {
            TakeSwitch::None
        };
        let report =
            |ends: f64, starts: f64, crossfade: f64, stretched: Option<Stretched>, left: f64| {
                (gap > GAP || left > 0.0 || stretched.is_some()).then(|| Filled {
                    at: after.first + after.shift,
                    recorded: after.first,
                    gap: gap.max(0.0),
                    extended: (ends.min(runs_to) - old_cut).max(0.0),
                    reached_back: (lead - starts).max(0.0),
                    crossfade,
                    stretched,
                    left,
                    take_switch: if stretched.is_some() || left > 0.0 || ends < lead - SAME {
                        switch
                    } else {
                        TakeSwitch::None
                    },
                })
            };
        // 1. The piece before reaches the lead: the seam is the lead, the
        // usual crossfade into the lead-in before it.
        if lead <= runs_to {
            let crossfade = self.fill.crossfade.min(lead - reaches_to).max(0.0);
            let starts = lead - crossfade;
            return Seam {
                ends: lead,
                starts,
                crossfade,
                draw: None,
                report: report(lead, starts, crossfade, None, 0.0),
            };
        }
        // 2–3. It stops short; the piece after reaches back to meet it, and
        // they crossfade over what they share.
        if reaches_to <= runs_to {
            let crossfade = (runs_to - reaches_to).min(self.fill.most_crossfade);
            let starts = runs_to - crossfade;
            return Seam {
                ends: runs_to,
                starts,
                crossfade,
                draw: None,
                report: report(runs_to, starts, crossfade, None, 0.0),
            };
        }
        // 4. Neither reaches: the decay before's last span is stretched to
        // meet the piece after, crossfading into it.
        let crossfade = self.fill.crossfade;
        let attack = (before.last + before.shift + self.fill.keep).max(opened);
        let (draw, stretched, left) = self.stretch(runs_to, attack, reaches_to + crossfade);
        let reach = draw.map_or(runs_to, |d| d.to);
        let crossfade = if left > 0.0 {
            0.0
        } else {
            (reach - reaches_to).max(0.0)
        };
        Seam {
            ends: runs_to,
            starts: reaches_to,
            crossfade,
            draw,
            report: report(runs_to, reaches_to, crossfade, stretched, left),
        }
    }

    /// The decay ending at `ends` (timeline; its attack's keep ends at
    /// `attack`) stretched to reach `to`: the shortest last span that does
    /// at [`Fill::least_rate`], or all the decay there is at that rate and
    /// the rest left.
    fn stretch(&self, ends: f64, attack: f64, to: f64) -> (Option<Draw>, Option<Stretched>, f64) {
        let short = to - ends;
        let least = self.fill.least_rate;
        if short <= SAME {
            return (None, None, 0.0);
        }
        if !(least > 0.0 && least < 1.0) {
            return (None, None, short);
        }
        let need = short * least / (1.0 - least);
        let room = (ends - attack).max(0.0);
        let span = need.min(room);
        if span <= 1e-3 {
            return (None, None, short);
        }
        let covers = span * (1.0 - least) / least;
        let covers = covers.min(short);
        let from = ends - span;
        let reach = ends + covers;
        let stretched = Stretched {
            from,
            over: reach - from,
            rate: span / (reach - from),
        };
        (
            Some(Draw { from, to: reach }),
            Some(stretched),
            (short - covers).max(0.0),
        )
    }

    /// The span's last piece, `start..end` (timeline): stopped before the
    /// next attack in its recording and at its take's end, and drawn to
    /// the span's end if it falls short.
    fn tail(&self, run: &Run, (start, end): (f64, f64), next: Option<f64>) -> Option<Tail> {
        let within = self.tail_within(run, (start, end), next, true)?;
        let cost = within.report.as_ref().map_or(0.0, stretch_cost);
        if cost > 0.0
            && within
                .report
                .as_ref()
                .is_some_and(|r| r.take_switch == TakeSwitch::Stopped)
            && let Some(crossed) = self.tail_within(run, (start, end), next, false)
            && crossed.report.as_ref().map_or(0.0, stretch_cost) < cost - SAME
        {
            return Some(crossed);
        }
        Some(within)
    }

    fn tail_within(
        &self,
        run: &Run,
        (start, end): (f64, f64),
        next: Option<f64>,
        within: bool,
    ) -> Option<Tail> {
        let (runs_rec, stopped) = self.runs_on(run, next, within);
        let runs_to = runs_rec + run.shift;
        if runs_to >= end || runs_to <= start {
            return None;
        }
        let attack = (run.last + run.shift + self.fill.keep).max(start);
        let (draw, stretched, left) = self.stretch(runs_to, attack, end);
        let switch = if !within {
            TakeSwitch::Crossed
        } else if stopped {
            TakeSwitch::Stopped
        } else {
            TakeSwitch::None
        };
        Some(Tail {
            ends: runs_to,
            draw,
            report: Some(Filled {
                at: end,
                recorded: next.unwrap_or(end - run.shift),
                gap: end - runs_to,
                extended: 0.0,
                reached_back: 0.0,
                crossfade: 0.0,
                stretched,
                left,
                take_switch: switch,
            }),
        })
    }
}

/// How bad a fill sounds, to choose between two: silence counts most, a
/// stretch by how much it slows.
fn stretch_cost(r: &Filled) -> f64 {
    r.left * 10.0 + r.stretched.map_or(0.0, |s| s.over * (1.0 - s.rate))
}

#[cfg(test)]
mod tests;
