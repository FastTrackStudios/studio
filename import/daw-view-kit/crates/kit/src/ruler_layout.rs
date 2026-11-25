//! The ruler's shape, as the panels around the arrangement read it: how
//! tall it is and which lanes it shows (process-wide, because every hit
//! test and offset asks), the lane rows by the FTS convention, and the
//! song's bars and beats counted through its tempo map. The ruler's
//! drawing is the arrangement's (`ruler`); this is what the rest of the
//! window measures against.

use ::timeline::time::{TempoMap, TempoPoint};

/// One beat of the project, and where it falls (in seconds: `at`,
/// `units_per_beat`).
pub use ::timeline::time::Beat;

/// The bar strip's height. REAPER's, measured.
pub const BARS_H: f64 = 28.0;

/// One ruler lane's height.
pub const LANE_H: f64 = 15.0;

/// How many ruler lanes there are, over the bars.
///
/// The FTS convention (REAPER 7.62's ruler lanes, numbered from 0 as
/// REAPER's API and the daw service do): lane 0 is the SONG, one region
/// over the whole song; lane 1 the SECTIONS, a region per verse and
/// chorus; lane 2 the MARKS — SONGSTART, SONGEND and the like. Always three, so a session with fewer still lays its rows out
/// where every other session does.
pub const LANES: usize = 3;

/// How tall the tempo strip is, above the bar numbers.
///
/// Its own row rather than a flag among the marks: a tempo change is
/// not a place in the song, it is a change to what every number below
/// it MEANS. Putting it in the marks lane would file it with the things
/// it reinterprets.
pub const TEMPO_H: f64 = 13.0;

/// The CHORDS lane's height: the song's key changes and its chords,
/// read off the Keyflow folder's KEY and CHORD tracks. A lane like the
/// others, so the ruler reads as one set of rows; the chords are lettered
/// smaller to fit it.
pub const CHORD_H: f64 = LANE_H;

/// Whether the ruler carries the CHORDS lane: unset, off, on.
static CHORDS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether the ruler shows the CHORDS lane. On unless
/// `FTS_RULER_CHORDS=0` — it is where a prepared session keeps its key
/// and chords once the Keyflow folder is out of the panel.
///
/// Process-wide rather than per view because the ruler's height is: the
/// hit tests, the lanes' offset and the corner beside the ruler all ask
/// it, and a height that two of them disagreed about would put every
/// click one lane off.
#[must_use]
pub fn chords_shown() -> bool {
    use std::sync::atomic::Ordering;
    match CHORDS.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = std::env::var("FTS_RULER_CHORDS").map_or(true, |v| v != "0");
            CHORDS.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Show or hide the CHORDS lane.
pub fn show_chords(on: bool) {
    CHORDS.store(if on { 2 } else { 1 }, std::sync::atomic::Ordering::Relaxed);
}

/// The CHORDS lane's height as the ruler has it now: nothing when it is
/// hidden.
#[must_use]
pub fn chords_h() -> f64 {
    if chords_shown() { CHORD_H } else { 0.0 }
}

/// How far down the ruler a lane row starts.
///
/// SONG and SECTIONS stack from the top; the CHORDS lane, when shown,
/// sits under SECTIONS and pushes MARKS below it. The chords follow the
/// sections they are the harmony of, and the marks — the sparsest lane —
/// sit nearest the bars, where a SONGSTART flag is read against its bar.
#[must_use]
pub fn row_top(row: usize) -> f64 {
    let above = (0..row).filter(|&r| lane_shown(r)).count();
    let lanes = LANE_H * crate::num::coord(above);
    if row >= MARKS_ROW {
        lanes + chords_h()
    } else {
        lanes
    }
}

/// The whole top strip: the lanes, the chords, then the tempo and the
/// bars under them.
#[must_use]
pub fn ruler_h() -> f64 {
    let lanes = (0..LANES).filter(|&r| lane_shown(r)).count();
    bars_h() + tempo_h() + LANE_H * crate::num::coord(lanes) + chords_h()
}

/// The bar numbers' row on a slim ruler: small numbers, a short tick.
pub const SLIM_BARS_H: f64 = 13.0;

/// The bar numbers' row as the ruler has it now.
#[must_use]
pub fn bars_h() -> f64 {
    if slim() { SLIM_BARS_H } else { BARS_H }
}

/// The slim ruler, for a touchscreen: the sections and small bar numbers
/// (the chords on request), and not the song band, the marks or the
/// tempo. A finger's ruler
/// is drawn a third bigger, and six lanes of it took a sixth of an iPad
/// from the tracks; Logic's is one line.
static SLIM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Make the ruler slim, or full. A slim ruler starts without its CHORDS
/// lane — the sections and the bars are the whole of it — unless the
/// chords were asked for (the toolbar's switch, `FTS_RULER_CHORDS=1`).
pub fn set_slim(on: bool) {
    use std::sync::atomic::Ordering;
    SLIM.store(on, Ordering::Relaxed);
    if on && CHORDS.load(Ordering::Relaxed) == 0 {
        let asked = std::env::var("FTS_RULER_CHORDS").is_ok_and(|v| v == "1");
        CHORDS.store(if asked { 2 } else { 1 }, Ordering::Relaxed);
    }
}

/// Whether the ruler is slim ([`set_slim`]).
#[must_use]
pub fn slim() -> bool {
    SLIM.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether a lane row is on the ruler: every one, or on a slim ruler the
/// sections alone.
#[must_use]
pub fn lane_shown(row: usize) -> bool {
    !slim() || row == SECTIONS_ROW
}

/// The TEMPO lane's height: nothing on a slim ruler.
#[must_use]
pub fn tempo_h() -> f64 {
    if slim() { 0.0 } else { TEMPO_H }
}

/// A stretch on a grid of its own (a Grid region of the edited track's):
/// `(start, end, division)`, seconds and a whole note's division.
pub type GridRegion = (f64, f64, u32);
/// Which lane makes markers, by the FTS convention.
///
/// The third: SONG, SECTIONS, MARKS. A press in it creates a marker;
/// a press in either of the others creates a region.
pub const MARKS_ROW: usize = 2;

/// Which lane makes a song section.
///
/// The second. A region here IS a section — that is the rule the song
/// model reads the arrangement by, so it is the one this file has to
/// agree with.
pub const SECTIONS_ROW: usize = 1;

/// The lane number to store for a row.
///
/// The inverse of `lane_row`: rows and lanes both count from 0.
#[must_use]
pub const fn lane_of(row: usize) -> u32 {
    row as u32
}
/// A measure counted from the song's: `bar_one` (a measure counted from
/// the project's start) is bar 1, the ones after it 2, 3 …, the ones
/// before it −1, −2 … — the count-in counting down to the song, with no
/// bar 0, as REAPER numbers a measure offset.
#[must_use]
pub fn from_song(measure: u32, bar_one: u32) -> i64 {
    let shown = i64::from(measure) - i64::from(bar_one) + 1;
    if shown <= 0 { shown - 1 } else { shown }
}

/// Where `project`'s song starts: its `SONGSTART` marker, the ruler's
/// bar 1.
#[must_use]
pub fn song_start_of(project: &daw_ui::studio::project::Project) -> Option<f64> {
    project
        .markers
        .iter()
        .find(|m| m.name == "SONGSTART")
        .map(|m| m.at)
}

/// The bar and beat at `at`, counted from the song as the ruler counts
/// them ([`from_song`]).
#[must_use]
pub fn song_bar_beat(
    tempo: &[daw_ui::studio::project::TempoChange],
    song_start: Option<f64>,
    at: f64,
) -> (i64, u32) {
    let beats = Timeline::new(tempo).beats(at.max(song_start.unwrap_or(0.0)) + 1e-6, MAX_BEATS);
    let bar_one = song_start
        .and_then(|start| beats.iter().rev().find(|b| b.at <= start + 1e-6))
        .map_or(1, |b| b.measure);
    beats
        .iter()
        .rev()
        .find(|b| b.at <= at + 1e-6)
        .map_or((1, 1), |b| (from_song(b.measure, bar_one), b.beat))
}

/// How many beats a ruler will count before it gives up.
///
/// A tempo map cannot be trusted to be sane — a zero or a negative bpm
/// would make the walk stand still — and a ruler is not the place to
/// find that out by hanging.
///
/// Well above any real session: an hour of sixteenth notes at 200 bpm
/// is under fifty thousand. The guard above is what keeps an absurd
/// zoom from asking for them at all.
pub const MAX_BEATS: usize = 100_000;
/// The project's beat grid, walked through its tempo map.
#[derive(Clone, Copy)]
pub struct Timeline<'a> {
    changes: &'a [daw_ui::studio::project::TempoChange],
}

impl<'a> Timeline<'a> {
    #[must_use]
    pub const fn new(changes: &'a [daw_ui::studio::project::TempoChange]) -> Self {
        Self { changes }
    }

    /// The tempo map in seconds.
    #[must_use]
    pub fn map(self) -> TempoMap {
        TempoMap::new(self.changes.iter().map(point), 1.0)
    }

    /// Every beat from the start of the project up to `to`, in order —
    /// none for a project with no tempo. Stopped by `to`, and by `limit`
    /// so a corrupt tempo map cannot spin.
    #[must_use]
    pub fn beats(self, to: f64, limit: usize) -> Vec<Beat> {
        if self.changes.is_empty() {
            return Vec::new();
        }
        self.map().beats(to, limit)
    }
}

/// A project tempo change as a map point.
#[must_use]
pub const fn point(change: &daw_ui::studio::project::TempoChange) -> TempoPoint {
    TempoPoint {
        at: change.at,
        bpm: change.bpm,
        beats_per_bar: change.beats_per_bar,
        beat_unit: change.beat_unit,
    }
}
