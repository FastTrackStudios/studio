//! Rating takes, while the band is still playing.
//!
//! The room is tracking. A pass ends, and for the next thirty seconds
//! everybody knows something the session will otherwise forget: that
//! one was the best so far, the second chorus fell apart, the fill
//! going into the bridge was the one. Thirty seconds later the next
//! pass starts and all of it is gone — and at mixdown somebody listens
//! to fourteen takes of the same song trying to rediscover what six
//! people already knew at the time.
//!
//! So this is the model behind a review screen on a tablet in front of
//! each player: a pass, a verdict, and — because "it was great except
//! for the second chorus" is the common case — marks on RANGES of it.
//!
//! # A take here is a PASS, not a DAW take
//!
//! REAPER's take is one recording on one track. What a band means by
//! "that take" is everybody's recording of one run at the song, which
//! is a span of session time with items on twenty tracks under it. That
//! is what a [`Pass`] is, and it is why a rating is not a property of
//! an item.
//!
//! # Why every role rates separately
//!
//! Because they disagree, and the disagreement is the useful part. The
//! drummer's "amazing" and the singer's "mistake" on the same pass is
//! precisely the take to comp from. A single band-wide rating would
//! average that away into a three-star nothing.

use std::collections::BTreeMap;

/// What somebody thought of what they just played.
///
/// Four, deliberately: three levels of good and one of broken. A
/// ten-point scale invites deliberation, and this is a judgement made
/// in the seconds between passes with sticks still in hand.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, facet::Facet)]
pub enum Verdict {
    /// Something went wrong here. The X.
    Mistake,
    /// A keeper. One star.
    Good,
    /// Better than the others. Two stars.
    VeryGood,
    /// The one. Three stars.
    Amazing,
}

impl Verdict {
    /// How it is written down, and read back.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Mistake => "x",
            Self::Good => "1",
            Self::VeryGood => "2",
            Self::Amazing => "3",
        }
    }

    /// Read one, or nothing.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "x" | "X" => Some(Self::Mistake),
            "1" => Some(Self::Good),
            "2" => Some(Self::VeryGood),
            "3" => Some(Self::Amazing),
            _ => None,
        }
    }

    /// How many stars, for drawing. A mistake has none.
    #[must_use]
    pub const fn stars(self) -> u8 {
        match self {
            Self::Mistake => 0,
            Self::Good => 1,
            Self::VeryGood => 2,
            Self::Amazing => 3,
        }
    }

    /// Whether this is a keeper rather than a fault.
    #[must_use]
    pub const fn is_good(self) -> bool {
        !matches!(self, Self::Mistake)
    }
}

/// A stretch of a pass, in seconds from the start of the pass.
///
/// Relative to the pass and not to the project, because a pass is the
/// thing on the screen: the waveform drawn is the pass, a drag across
/// it is a fraction of it, and a mark that stored project time would
/// have to be re-based every time it was drawn.
#[derive(Clone, Copy, PartialEq, Debug, facet::Facet)]
pub struct Span {
    pub from: f64,
    pub to: f64,
}

impl Span {
    /// A span from two points in any order, never negative.
    #[must_use]
    pub fn new(a: f64, b: f64) -> Self {
        let (from, to) = if a <= b { (a, b) } else { (b, a) };
        Self {
            from: from.max(0.0),
            to: to.max(0.0),
        }
    }

    /// The whole pass.
    ///
    /// A verdict on the take itself is a mark that covers it, rather
    /// than a second kind of thing: one list to read, one list to draw,
    /// and "the whole thing was bad but this bit was great" is two
    /// marks that overlap instead of a rating fighting an annotation.
    #[must_use]
    pub fn whole(length: f64) -> Self {
        Self::new(0.0, length.max(0.0))
    }

    #[must_use]
    pub fn length(self) -> f64 {
        (self.to - self.from).max(0.0)
    }

    /// Whether this span covers effectively all of a pass that long.
    ///
    /// Within a tenth of a second at each end: a drag that started on
    /// the first pixel and ended on the last is a verdict on the take,
    /// and a mark that missed by three milliseconds should not read as
    /// something else.
    #[must_use]
    pub fn is_whole(self, length: f64) -> bool {
        const EDGE: f64 = 0.1;
        self.from <= EDGE && self.to >= length - EDGE
    }

    /// Whether a moment is inside.
    #[must_use]
    pub fn holds(self, at: f64) -> bool {
        at >= self.from && at <= self.to
    }

    /// Whether two spans touch at all.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.from <= other.to && other.from <= self.to
    }
}

/// One person's judgement on one stretch of one pass.
#[derive(Clone, PartialEq, Debug, facet::Facet)]
pub struct Mark {
    /// Which ROLE made it — "Guitar 1", "Vocalist 2", "Drums".
    ///
    /// A role and not a person, because a role is what the session is
    /// actually made of: the tracks are grouped by it, the tablet on
    /// the drum riser is the drummer's whoever is sitting there, and a
    /// dep playing tonight is still Guitar 1. Marks keyed by name would
    /// scatter the moment somebody covered.
    pub role: String,
    pub span: Span,
    pub verdict: Verdict,
    /// What they said about it, if anything. Optional because the
    /// gesture has to survive being made with one hand in under a
    /// second; typing is for when there is time.
    pub note: Option<String>,
}

impl Mark {
    /// A verdict on a whole pass.
    #[must_use]
    pub fn whole(role: impl Into<String>, length: f64, verdict: Verdict) -> Self {
        Self {
            role: role.into(),
            span: Span::whole(length),
            verdict,
            note: None,
        }
    }

    /// A verdict on part of one.
    #[must_use]
    pub fn part(role: impl Into<String>, span: Span, verdict: Verdict) -> Self {
        Self {
            role: role.into(),
            span,
            verdict,
            note: None,
        }
    }

    #[must_use]
    pub fn noted(mut self, note: impl Into<String>) -> Self {
        let note = note.into();
        self.note = (!note.trim().is_empty()).then_some(note);
        self
    }
}

/// One run at a song, and what everybody thought of it.
#[derive(Clone, PartialEq, Debug, facet::Facet)]
pub struct Pass {
    /// Which pass this is, counted from one within the song. What the
    /// room calls it out loud — "take four" — so it is what the screen
    /// says.
    pub number: u32,
    /// Where it sits in the song, in seconds, so the waveform can be
    /// read and the audio found.
    pub from: f64,
    pub to: f64,
    pub marks: Vec<Mark>,
}

impl Pass {
    #[must_use]
    pub fn new(number: u32, from: f64, to: f64) -> Self {
        let span = Span::new(from, to);
        Self {
            number,
            from: span.from,
            to: span.to,
            marks: Vec::new(),
        }
    }

    #[must_use]
    pub fn length(&self) -> f64 {
        (self.to - self.from).max(0.0)
    }

    /// Add a mark, replacing that role's verdict on the SAME stretch.
    ///
    /// Re-rating is the common correction — you hit two stars and meant
    /// three — and it must not leave both. A mark on a different
    /// stretch is a different mark, even from the same person: "the
    /// take was good, the second chorus was not" is two.
    pub fn mark(&mut self, mark: Mark) {
        let same = |existing: &Mark| {
            existing.role == mark.role
                && (existing.span.from - mark.span.from).abs() < 0.001
                && (existing.span.to - mark.span.to).abs() < 0.001
        };
        match self.marks.iter_mut().find(|existing| same(existing)) {
            Some(existing) => *existing = mark,
            None => self.marks.push(mark),
        }
    }

    /// Take back a mark — the same gesture again, on the same stretch.
    pub fn unmark(&mut self, role: &str, span: Span) {
        self.marks.retain(|mark| {
            mark.role != role
                || (mark.span.from - span.from).abs() >= 0.001
                || (mark.span.to - span.to).abs() >= 0.001
        });
    }

    /// One role's verdict on the take as a whole, if it gave one.
    #[must_use]
    pub fn verdict_of(&self, role: &str) -> Option<Verdict> {
        let length = self.length();
        self.marks
            .iter()
            .find(|mark| mark.role == role && mark.span.is_whole(length))
            .map(|mark| mark.verdict)
    }

    /// Everything said about part of the take rather than all of it.
    #[must_use]
    pub fn parts(&self) -> Vec<&Mark> {
        let length = self.length();
        self.marks
            .iter()
            .filter(|mark| !mark.span.is_whole(length))
            .collect()
    }

    /// What the band thought of it, as a whole.
    ///
    /// The WORST whole-take verdict wins, and that is the point: one
    /// player's mistake is the take's mistake, however much everyone
    /// else liked it. A mean would hide exactly the thing the screen
    /// exists to catch.
    #[must_use]
    pub fn consensus(&self) -> Option<Verdict> {
        let length = self.length();
        self.marks
            .iter()
            .filter(|mark| mark.span.is_whole(length))
            .map(|mark| mark.verdict)
            .min()
    }

    /// How many people have given the take a verdict.
    #[must_use]
    pub fn voices(&self) -> usize {
        let length = self.length();
        let mut who: Vec<&str> = self
            .marks
            .iter()
            .filter(|mark| mark.span.is_whole(length))
            .map(|mark| mark.role.as_str())
            .collect();
        who.sort_unstable();
        who.dedup();
        who.len()
    }

    /// The good stretches inside a take nobody kept.
    ///
    /// "The whole thing was bad but there is a really cool thing here"
    /// — the reason a bad take is still worth keeping, and the thing a
    /// star rating on its own throws away.
    #[must_use]
    pub fn keepers(&self) -> Vec<&Mark> {
        self.parts()
            .into_iter()
            .filter(|mark| mark.verdict.is_good())
            .collect()
    }
}

/// Every pass at one song.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Review {
    passes: BTreeMap<u32, Pass>,
}

impl Review {
    /// Start a pass, or return the one already there.
    pub fn begin(&mut self, from: f64, to: f64) -> &mut Pass {
        let number = self.next_number();
        self.passes
            .entry(number)
            .or_insert_with(|| Pass::new(number, from, to))
    }

    /// A review from passes that came over the wire.
    ///
    /// The wire carries the passes; the map is the index this side
    /// builds over them, and rebuilding it here is cheaper than
    /// teaching the wire about a `BTreeMap`.
    #[must_use]
    pub fn from_passes(passes: impl IntoIterator<Item = Pass>) -> Self {
        Self {
            passes: passes.into_iter().map(|pass| (pass.number, pass)).collect(),
        }
    }

    /// The number the next pass gets: one past the highest, so a
    /// deleted take does not hand its number to the next one.
    #[must_use]
    pub fn next_number(&self) -> u32 {
        self.passes
            .keys()
            .next_back()
            .map_or(1, |n| n.saturating_add(1))
    }

    #[must_use]
    pub fn pass(&self, number: u32) -> Option<&Pass> {
        self.passes.get(&number)
    }

    pub fn pass_mut(&mut self, number: u32) -> Option<&mut Pass> {
        self.passes.get_mut(&number)
    }

    /// Every pass, oldest first.
    #[must_use]
    pub fn passes(&self) -> Vec<&Pass> {
        self.passes.values().collect()
    }

    /// The one just played — the screen's subject the moment a pass
    /// ends.
    #[must_use]
    pub fn latest(&self) -> Option<&Pass> {
        self.passes.values().next_back()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }

    /// The takes worth listening to, best first.
    ///
    /// Ordered by the band's consensus, then by how many people
    /// bothered to say so — a three-star take four people agreed on
    /// beats a three-star take one person rated — then by recency,
    /// because a later pass at the same standard is usually the more
    /// settled performance.
    ///
    /// Takes nobody rated are left out entirely. Silence is not a
    /// verdict, and a list of "best takes" padded with unheard ones is
    /// a list you stop trusting.
    #[must_use]
    pub fn best(&self) -> Vec<&Pass> {
        let mut rated: Vec<&Pass> = self
            .passes
            .values()
            .filter(|pass| pass.consensus().is_some())
            .collect();
        rated.sort_by(|a, b| {
            b.consensus()
                .cmp(&a.consensus())
                .then(b.voices().cmp(&a.voices()))
                .then(b.number.cmp(&a.number))
        });
        rated
    }

    /// Every good stretch anybody marked, across every pass.
    ///
    /// What the comp is built from: the moments somebody said were
    /// worth keeping, with the pass they are in.
    #[must_use]
    pub fn highlights(&self) -> Vec<(&Pass, &Mark)> {
        self.passes
            .values()
            .flat_map(|pass| pass.keepers().into_iter().map(move |mark| (pass, mark)))
            .collect()
    }
}

// ─── Keeping it with the song ───────────────────────────────────────

/// Where a song's review lives.
///
/// Project ext state on the SONG's project, because that is what a
/// review is about and what it must travel with: open the song
/// anywhere and the takes are rated. It is also how six tablets see
/// each other's marks — they are all looking at the same project
/// through the same service.
pub const SECTION: &str = "fasttrackstudio.review";

/// The one key, holding every pass and every mark.
pub const KEY: &str = "takes";

impl Review {
    /// How it is written into ext state.
    ///
    /// Lines, because this grows all evening and a format that rewrites
    /// the whole document to add one mark is a format that loses marks:
    ///
    /// ```text
    /// p|4|0|182.5
    /// m|4|Cody|0|182.5|3|
    /// m|4|Cody|64|72|3|fill into the bridge
    /// ```
    ///
    /// The note is last and keeps whatever is in it, pipes included —
    /// it is the one field a person types.
    #[must_use]
    pub fn stored(&self) -> String {
        let mut lines = Vec::new();
        for pass in self.passes.values() {
            lines.push(format!("p|{}|{}|{}", pass.number, pass.from, pass.to));
            for mark in &pass.marks {
                let note = mark.note.as_deref().unwrap_or("");
                // One line per mark, so a newline in a note cannot
                // invent a second one.
                let note = note.replace(['\n', '\r'], " ");
                lines.push(format!(
                    "m|{}|{}|{}|{}|{}|{}",
                    pass.number,
                    mark.role,
                    mark.span.from,
                    mark.span.to,
                    mark.verdict.token(),
                    note
                ));
            }
        }
        lines.join("\n")
    }

    /// Read one back.
    ///
    /// Unreadable lines are skipped, and a mark whose pass is missing
    /// is skipped with it: half a review is worth more than none, and
    /// an evening's ratings should not be lost to one bad line.
    #[must_use]
    pub fn from_stored(text: &str) -> Self {
        let mut review = Self::default();
        for line in text.lines() {
            let mut parts = line.split('|');
            match parts.next() {
                Some("p") => {
                    let Some(number) = parts.next().and_then(|n| n.parse::<u32>().ok()) else {
                        continue;
                    };
                    let from = parts.next().and_then(|n| n.parse::<f64>().ok());
                    let to = parts.next().and_then(|n| n.parse::<f64>().ok());
                    let (Some(from), Some(to)) = (from, to) else {
                        continue;
                    };
                    if !from.is_finite() || !to.is_finite() {
                        continue;
                    }
                    review
                        .passes
                        .entry(number)
                        .or_insert_with(|| Pass::new(number, from, to));
                }
                Some("m") => {
                    // The note is whatever is left, pipes and all.
                    let mut parts = line.splitn(7, '|').skip(1);
                    let number = parts.next().and_then(|n| n.parse::<u32>().ok());
                    let role = parts.next().map(str::to_owned);
                    let from = parts.next().and_then(|n| n.parse::<f64>().ok());
                    let to = parts.next().and_then(|n| n.parse::<f64>().ok());
                    let verdict = parts.next().and_then(Verdict::parse);
                    let note = parts.next().unwrap_or("");
                    let (Some(number), Some(role), Some(from), Some(to), Some(verdict)) =
                        (number, role, from, to, verdict)
                    else {
                        continue;
                    };
                    if role.is_empty() || !from.is_finite() || !to.is_finite() {
                        continue;
                    }
                    let Some(pass) = review.passes.get_mut(&number) else {
                        continue;
                    };
                    pass.mark(Mark {
                        role,
                        span: Span::new(from, to),
                        verdict,
                        note: (!note.is_empty()).then(|| note.to_owned()),
                    });
                }
                _ => {}
            }
        }
        review
    }
}
