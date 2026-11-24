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
//! 4. If they still do not meet, the piece after replays a bit of the
//!    previous stroke's decay: it reaches back past `cut.guard`, into that
//!    stroke's decay in its own recording, but never into its first
//!    [`Fill::keep`] after the attack (no attack sounds twice), and
//!    crossfades where it overlaps the piece before
//!    ([`Filled::replayed_decay`]).
//! 5. Only what is still left is stretched: the decay before (a drum's
//!    tail stretches unheard; the lead-in after sits against an attack
//!    and is never slowed), only its last span — the shortest that
//!    reaches at [`Fill::least_rate`] — and never its attack
//!    ([`Fill::keep`]). Where all of that decay at [`Fill::least_rate`]
//!    does not reach, all of it is drawn only as slow as it must be, down
//!    to [`Fill::least_rate_leftover`].
//! 6. Only past both is a dip left ([`Filled::left`]): the piece after
//!    starts with a `crossfade` of `Some(0.0)`, each side fading out or in
//!    within itself, never a hard cut.
//! 7. At a take switch a side first stops at its take's end; if that
//!    leaves a stretch or a silence, it plays on through the comp's own
//!    switch there instead (the comp as recorded, never past an attack):
//!    [`TakeSwitch::Crossed`].
//!
//! Each gap is reported ([`Filled`]): how far the piece before was
//! extended, how far the piece after reached back (and how much of that
//! replays the previous stroke's decay), how long they crossfade, what
//! was stretched and at what rate, and what was left.

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

/// How a seam came out: where the piece before ends, where the one after
/// starts, how long they crossfade, the stretch and the dip left.
#[derive(Clone, Copy)]
struct Done {
    ends: f64,
    starts: f64,
    crossfade: f64,
    stretched: Option<Stretched>,
    left: f64,
}

impl Done {
    /// The two meet: no stretch, nothing left.
    const fn met(ends: f64, starts: f64, crossfade: f64) -> Self {
        Self {
            ends,
            starts,
            crossfade,
            stretched: None,
            left: 0.0,
        }
    }
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

    /// The earliest the run may start, recording time: `past` (the
    /// guard, or the replay's keep) after the attack before it in its
    /// recording, and (`within`) its take's start — whether the take's
    /// start is what stops it.
    fn reaches_back(&self, run: &Run, past: f64, within: bool) -> (f64, bool) {
        let attack = run.before.map_or(f64::NEG_INFINITY, |p| p + past);
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
        let (reach_rec, a_take) = self.reaches_back(after, self.cut.guard, within);
        let (replay_rec, r_take) = self.reaches_back(after, self.fill.keep, within);
        let runs_to = runs_rec + before.shift;
        let reaches_to = (reach_rec + after.shift).min(lead);
        // 4. How far it may reach replaying the previous stroke's decay:
        // past the guard, never into that stroke's keep.
        let replays_to = (replay_rec + after.shift).min(reaches_to);
        // Where the piece before was cut before the move: just before this
        // hit's old place, as it plays at the before's shift.
        let old_cut = after.first + before.shift - self.cut.lead;
        let gap = lead - old_cut;
        let switch = if !within {
            TakeSwitch::Crossed
        } else if b_take || a_take || (r_take && replays_to < reaches_to) {
            TakeSwitch::Stopped
        } else {
            TakeSwitch::None
        };
        let report = |done: Done| {
            (gap > GAP || done.left > 0.0 || done.stretched.is_some()).then(|| Filled {
                at: after.first + after.shift,
                recorded: after.first,
                gap: gap.max(0.0),
                extended: (done.ends.min(runs_to) - old_cut).max(0.0),
                reached_back: (lead - done.starts).max(0.0),
                replayed_decay: (reaches_to - done.starts).max(0.0),
                crossfade: done.crossfade,
                stretched: done.stretched,
                left: done.left,
                take_switch: if done.stretched.is_some()
                    || done.left > 0.0
                    || done.ends < lead - SAME
                {
                    switch
                } else {
                    TakeSwitch::None
                },
            })
        };
        let seam = |done: Done, draw: Option<Draw>| Seam {
            ends: done.ends,
            starts: done.starts,
            crossfade: done.crossfade,
            draw,
            report: report(done),
        };
        // 1. The piece before reaches the lead: the seam is the lead, the
        // usual crossfade into the lead-in before it.
        if lead <= runs_to {
            let crossfade = self.fill.crossfade.min(lead - reaches_to).max(0.0);
            return seam(Done::met(lead, lead - crossfade, crossfade), None);
        }
        // 2–4. It stops short; the piece after reaches back to meet it —
        // past the guard into the previous stroke's decay if it must — and
        // they crossfade over what they share.
        if replays_to <= runs_to {
            let from = if reaches_to <= runs_to {
                reaches_to
            } else {
                replays_to
            };
            let crossfade = (runs_to - from).min(self.fill.most_crossfade);
            return seam(Done::met(runs_to, runs_to - crossfade, crossfade), None);
        }
        // 5. Neither reaches: the decay before's last span is stretched to
        // meet the piece after (reaching back as far as it may), crossfading
        // into it; 6. what not even that reaches is a dip.
        let attack = (before.last + before.shift + self.fill.keep).max(opened);
        let stretch = self.stretch(runs_to, attack, replays_to + self.fill.crossfade);
        let reach = stretch.map_or(runs_to, |(d, _)| d.to);
        let left = replays_to - reach;
        let (left, crossfade) = if left > SAME {
            (left, 0.0)
        } else {
            (0.0, (reach - replays_to).max(0.0))
        };
        seam(
            Done {
                ends: runs_to,
                starts: replays_to,
                crossfade,
                stretched: stretch.map(|(_, s)| s),
                left,
            },
            stretch.map(|(d, _)| d),
        )
    }

    /// The decay ending at `ends` (timeline; its attack's keep ends at
    /// `attack`) stretched to reach `to`: the shortest last span that does
    /// at [`Fill::least_rate`]; else all the decay there is, only as slow
    /// as it must be, down to [`Fill::least_rate_leftover`] — what that
    /// does not reach is left. None: no decay to stretch, or nothing
    /// short.
    fn stretch(&self, ends: f64, attack: f64, to: f64) -> Option<(Draw, Stretched)> {
        let short = to - ends;
        let least = self.fill.least_rate;
        if short <= SAME || !(least > 0.0 && least < 1.0) {
            return None;
        }
        let floor = self.fill.least_rate_leftover;
        let floor = if floor > 0.0 && floor <= least {
            floor
        } else {
            least
        };
        let room = (ends - attack).max(0.0);
        if room <= 1e-3 {
            return None;
        }
        let need = short * least / (1.0 - least);
        let (span, covers) = if need <= room {
            (need, short)
        } else if room / (room + short) >= floor {
            (room, short)
        } else {
            (room, room * (1.0 - floor) / floor)
        };
        let from = ends - span;
        let reach = ends + covers;
        Some((
            Draw { from, to: reach },
            Stretched {
                from,
                over: reach - from,
                rate: span / (reach - from),
            },
        ))
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
        let stretch = self.stretch(runs_to, attack, end);
        let (draw, stretched) = (stretch.map(|(d, _)| d), stretch.map(|(_, s)| s));
        let left = end - draw.map_or(runs_to, |d| d.to);
        let left = if left > SAME { left } else { 0.0 };
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
                replayed_decay: 0.0,
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
