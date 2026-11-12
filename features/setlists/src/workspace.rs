//! A song's workspace, the model: what the song view (`crate::ext`'s
//! `SongView`, drawn by `crate::workspace_view`) shows and follows.
//!
//! Session's Overview, for a song note: the progress across the top; under
//! it, the chart's Keyflow source in an editor over the song's lyrics on the
//! left, and the engraved chart, the full height, on the right. On a phone,
//! the progress, the chart and the lyrics stacked, the editor behind a
//! toggle ([`Arrangement`]).
//!
//! **One playhead.** Everything follows one position, in seconds of the
//! song's recording ([`Clock`]): the chart's bars are timed on the chart's
//! own clock (its `\downbeat`, tempo and meters, Keyflow's `bar_times`:
//! seconds into the recording), the lyrics are the LRC's (seconds into the
//! recording), and the progress bar's sections are the chart's sections as
//! the bars place them ([`Song`]). This is the Reference layer of a song: a
//! recording and the chart's clock, no engine. The clock runs on its own
//! here: Architect's app plays no media for an extension's view (its
//! playhead, `architect.playhead`, is a share page's); Studio's engine (the
//! Full layer) is what adds the sound and drives this clock from its
//! transport instead.
//!
//! **One lyrics source** ([`lyrics_source`], as `README.md` says): the
//! song's LRC when it has one, else the LRC made from its chart's timed
//! lyric lane ([`crate::follow::chart_lrc`]). Never both.

use std::sync::Arc;

use session::lyrics::{Lyrics, SectionSpan};

use crate::Result;
use crate::follow::{BeatMark, FollowPages, PlacedBar, chart_lrc, follow_pages};

/// The play position: seconds into the song's recording, moving while it
/// plays. Pure: every reading is told the time on a wall clock (seconds,
/// from any origin), so a test drives it without waiting.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Clock {
    /// Where it was put (or paused).
    at: f64,
    /// When it started playing from `at`, on the wall clock; None paused.
    since: Option<f64>,
    /// Where it stops on its own (the song's end), if known.
    end: Option<f64>,
}

impl Clock {
    /// Paused at 0, stopping at `end` when played.
    #[must_use]
    pub const fn new(end: Option<f64>) -> Self {
        Self {
            at: 0.0,
            since: None,
            end,
        }
    }

    /// Where it stops, now `end` (the song re-engraved, longer or shorter).
    pub const fn set_end(&mut self, end: Option<f64>) {
        self.end = end;
    }

    /// Whether it is playing at `wall` (it stops at the end).
    #[must_use]
    pub fn playing(&self, wall: f64) -> bool {
        self.since.is_some() && !self.ended(wall)
    }

    /// Whether it has run to the end by `wall`.
    #[must_use]
    pub fn ended(&self, wall: f64) -> bool {
        self.end.is_some_and(|end| self.now(wall) >= end)
    }

    /// The position at `wall`.
    #[must_use]
    pub fn now(&self, wall: f64) -> f64 {
        let t = self
            .since
            .map_or(self.at, |since| self.at + (wall - since).max(0.0));
        self.end.map_or(t, |end| t.min(end))
    }

    /// Play from where it is (from the top, when it is at the end).
    pub fn play(&mut self, wall: f64) {
        if self.since.is_some() && !self.ended(wall) {
            return;
        }
        let at = self.now(wall);
        self.at = if self.end.is_some_and(|end| at >= end) {
            0.0
        } else {
            at
        };
        self.since = Some(wall);
    }

    /// Stop where it is.
    pub fn pause(&mut self, wall: f64) {
        self.at = self.now(wall);
        self.since = None;
    }

    /// Play or stop.
    pub fn toggle(&mut self, wall: f64) {
        if self.playing(wall) {
            self.pause(wall);
        } else {
            self.play(wall);
        }
    }

    /// Go to `t` (playing on from there if it was playing).
    pub fn seek(&mut self, t: f64, wall: f64) {
        let playing = self.playing(wall);
        let t = t.max(0.0);
        self.at = self.end.map_or(t, |end| t.min(end));
        self.since = playing.then_some(wall);
    }
}

/// A section of the song as it is played: a run of bars of one of the
/// chart's sections, one time through.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub name: String,
    /// Its colour (CSS), by its kind.
    pub color: String,
    /// Seconds into the recording.
    pub start: f64,
    pub end: f64,
}

/// A song as the workspace follows it: its chart engraved page by page,
/// its bars and sections timed on the chart's clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub chart: FollowPages,
    /// Its sections in playing order (a repeat is a part each time).
    pub parts: Vec<Part>,
}

impl Song {
    /// Chart `source`, engraved and timed.
    ///
    /// # Errors
    ///
    /// It does not parse, or a page did not draw.
    pub fn engrave(source: &str) -> Result<Self> {
        let chart = follow_pages(source)?;
        let parts = parts(&chart);
        Ok(Self { chart, parts })
    }

    /// Where the song starts and ends: its first bar to its last.
    #[must_use]
    pub fn span(&self) -> (f64, f64) {
        let start = self.chart.bars.first().map_or(0.0, |b| b.start);
        let end = self.chart.bars.iter().map(|b| b.end).fold(start, f64::max);
        (start, end)
    }

    /// How far through the song `t` is, 0–100.
    #[must_use]
    pub fn progress(&self, t: f64) -> f64 {
        let (start, end) = self.span();
        ((t - start) / (end - start).max(f64::EPSILON) * 100.0).clamp(0.0, 100.0)
    }

    /// The bar being played at `t` (in playing order); None before the
    /// first and after the last.
    #[must_use]
    pub fn bar_at(&self, t: f64) -> Option<usize> {
        let bars = &self.chart.bars;
        let i = bars.iter().rposition(|b| b.start <= t + 1e-6)?;
        (t < bars.get(i)?.end).then_some(i)
    }

    /// The beat being played at `t`: the last begun inside the bar being
    /// played (None outside every bar).
    #[must_use]
    pub fn beat_at(&self, t: f64) -> Option<&BeatMark> {
        let bar = self.bar(self.bar_at(t)?)?;
        self.chart
            .beats
            .iter()
            .filter(|b| b.at >= bar.start - 1e-6 && b.at < bar.end)
            .rfind(|b| b.at <= t + 1e-6)
    }

    /// The bar at `index` in playing order.
    #[must_use]
    pub fn bar(&self, index: usize) -> Option<&PlacedBar> {
        self.chart.bars.get(index)
    }

    /// The part being played at `t`: the last begun (the first, before
    /// any).
    #[must_use]
    pub fn part_at(&self, t: f64) -> Option<usize> {
        self.parts
            .iter()
            .rposition(|p| p.start <= t + 1e-6)
            .or_else(|| (!self.parts.is_empty()).then_some(0))
    }

    /// The page to show at `t`: the page of the bar being played; before
    /// the first bar the first page, after the last the last bar's.
    #[must_use]
    pub fn page_at(&self, t: f64) -> usize {
        let bars = &self.chart.bars;
        self.bar_at(t)
            .or_else(|| bars.iter().rposition(|b| b.start <= t))
            .and_then(|i| bars.get(i))
            .map_or(0, |b| b.page)
    }

    /// Back from `t`: the start of this part, or of the one before when
    /// this one was only just begun (within a second).
    #[must_use]
    pub fn back_from(&self, t: f64) -> f64 {
        let (start, _) = self.span();
        match self.part_at(t) {
            Some(i) if self.parts.get(i).is_some_and(|p| t >= p.start) => {
                let here = self.parts.get(i).map_or(start, |p| p.start);
                match i.checked_sub(1) {
                    Some(before) if t - here < 1.0 => {
                        self.parts.get(before).map_or(here, |p| p.start)
                    }
                    _ => here,
                }
            }
            _ => 0.0_f64.min(start),
        }
    }

    /// On from `t`: the start of the next part, if there is one.
    #[must_use]
    pub fn forward_from(&self, t: f64) -> Option<f64> {
        self.parts
            .iter()
            .map(|p| p.start)
            .find(|start| *start > t + 1e-3)
    }

    /// `source`'s lines on this song: the last held to the end of the part
    /// it is sung in, not the song's (an LRC says when a line starts, not
    /// when the last one ends; held to the song's end, it would belong to
    /// the outro it runs into).
    #[must_use]
    pub fn lyrics(&self, source: &LyricsSource) -> Lyrics {
        let (_, end) = self.span();
        let mut lyrics = source.lyrics(end);
        if let Some(last) = lyrics.lines.last_mut() {
            let part_end = self
                .part_at(last.start + 1e-3)
                .and_then(|i| self.parts.get(i))
                .map_or(end, |p| p.end);
            if part_end > last.start {
                last.end = last.end.min(part_end);
                if let Some(word) = last.words.last_mut() {
                    word.end = word.end.min(part_end);
                }
            }
        }
        lyrics
    }

    /// The parts as the lyrics place their lines against them.
    #[must_use]
    pub fn spans(&self) -> Vec<(SectionSpan, String)> {
        self.parts
            .iter()
            .map(|p| {
                (
                    SectionSpan {
                        name: p.name.clone(),
                        start: p.start,
                        end: p.end,
                    },
                    p.color.clone(),
                )
            })
            .collect()
    }
}

/// The sections as played: each run of bars of one section and pass.
fn parts(chart: &FollowPages) -> Vec<Part> {
    let mut out: Vec<(usize, u32, Part)> = Vec::new();
    for bar in &chart.bars {
        match out.last_mut() {
            Some((section, pass, part)) if *section == bar.section && *pass == bar.pass => {
                part.end = part.end.max(bar.end);
            }
            _ => {
                let (name, color) = chart.sections.get(bar.section).cloned().unwrap_or_default();
                out.push((
                    bar.section,
                    bar.pass,
                    Part {
                        name,
                        color,
                        start: bar.start,
                        end: bar.end,
                    },
                ));
            }
        }
    }
    out.into_iter().map(|(_, _, part)| part).collect()
}

/// Where a song's timed lyrics come from: its one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LyricsSource {
    /// The song's LRC.
    Lrc(String),
    /// Made from its chart's timed lyric lane.
    ChartLane(String),
}

impl LyricsSource {
    /// The LRC, whichever made it.
    #[must_use]
    pub fn lrc(&self) -> &str {
        match self {
            Self::Lrc(text) | Self::ChartLane(text) => text,
        }
    }

    /// Its lines on the recording's clock, the last ending at `end`.
    #[must_use]
    pub fn lyrics(&self, end: f64) -> Lyrics {
        Lyrics::from_lrc(self.lrc(), 0.0, end)
    }
}

/// The song's one source of timed lyrics: its LRC (`lrc`, when it has one
/// with a line in it), else its chart's timed lyric lane; None when it has
/// neither.
#[must_use]
pub fn lyrics_source(lrc: Option<&str>, chart: Option<&str>) -> Option<LyricsSource> {
    if let Some(lrc) = lrc.filter(|l| l.lines().any(has_timed_text)) {
        return Some(LyricsSource::Lrc(lrc.to_owned()));
    }
    chart
        .and_then(|source| chart_lrc(source).ok().flatten())
        .map(LyricsSource::ChartLane)
}

/// An LRC line with a time and words (`[00:12.50]Silent night`), not a
/// tag (`[ti:…]`).
fn has_timed_text(line: &str) -> bool {
    let line = line.trim();
    line.strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .is_some_and(|(stamp, text)| {
            stamp.starts_with(|c: char| c.is_ascii_digit()) && !text.trim().is_empty()
        })
}

/// How wide the workspace is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Form {
    /// Two columns under the progress: the editor over the lyrics, and
    /// the chart.
    #[default]
    Wide,
    /// One column (a phone): the progress, the chart and the lyrics, the
    /// editor behind a toggle.
    Narrow,
}

/// Narrower than this (CSS pixels), the workspace is one column.
pub const NARROW_BELOW: f64 = 720.0;

impl Form {
    /// The form for a pane `width` wide (0: not measured yet, wide).
    #[must_use]
    pub fn of(width: f64) -> Self {
        if width > 0.0 && width < NARROW_BELOW {
            Self::Narrow
        } else {
            Self::Wide
        }
    }
}

/// A pane of the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Progress,
    Editor,
    Chart,
    Lyrics,
}

/// Where the panes go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arrangement {
    /// The progress across the top; under it `left` top to bottom (the
    /// editor over the lyrics) and the chart on the right, the full
    /// height.
    Columns { left: Vec<Pane>, right: Vec<Pane> },
    /// One column, top to bottom.
    Stack(Vec<Pane>),
}

impl Arrangement {
    /// The panes for `form`, the editor shown or not.
    #[must_use]
    pub fn of(form: Form, editor: bool) -> Self {
        match form {
            Form::Wide => Self::Columns {
                left: if editor {
                    vec![Pane::Editor, Pane::Lyrics]
                } else {
                    vec![Pane::Lyrics]
                },
                right: vec![Pane::Chart],
            },
            Form::Narrow => {
                let mut panes = vec![Pane::Progress];
                if editor {
                    panes.push(Pane::Editor);
                }
                panes.extend([Pane::Chart, Pane::Lyrics]);
                Self::Stack(panes)
            }
        }
    }
}

/// A song as loaded for its workspace: what its note names, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub title: String,
    pub artist: Option<String>,
    /// The chart's source, and the vault path of its `.kf` (where an edit
    /// is saved), when it has one.
    pub source: String,
    pub chart_file: Option<String>,
    /// Its one lyrics source.
    pub lyrics: Option<LyricsSource>,
}

/// A song engraved for its workspace, shared by the panes that read it.
pub type Shared = Arc<Song>;

/// The chart's source as a song's page carries it: the body of its first
/// ```` ```kf ```` fence ([`crate::song::CHART_FENCE`]), any length of
/// backticks. What the workspace edits when the `.kf` itself cannot be
/// read.
#[must_use]
pub fn chart_fence(body: &str) -> Option<String> {
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        if ticks < 3 {
            continue;
        }
        let info = trimmed.get(ticks..).unwrap_or_default().trim();
        let lang = info.split_whitespace().next().unwrap_or_default();
        if !lang.eq_ignore_ascii_case(crate::song::CHART_FENCE) {
            continue;
        }
        let fence = "`".repeat(ticks);
        let mut out = String::new();
        for line in lines.by_ref() {
            if line.trim() == fence {
                return Some(out);
            }
            out.push_str(line);
            out.push('\n');
        }
        return Some(out);
    }
    None
}
