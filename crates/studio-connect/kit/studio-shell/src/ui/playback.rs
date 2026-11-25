//! What the performer screen reads from, and does to, an app's playback:
//! the [`PlaybackModel`] adapter (worship / gig screen spec §6.2).
//!
//! Signal and Session mount the same panels ([`super::perform`]) and differ
//! only in the adapter they provide: Signal's over its gig player
//! (`GigState` + `player()`), Session's over `StudioSession` /
//! `use_reading()` / `transport()`. Neither app forks a panel.
//!
//! The model is read through one value, [`PlaybackView`] — plain data, so
//! a panel compares it, a test builds it by hand, and nothing here knows
//! which app is playing.

use std::rc::Rc;

use dioxus::prelude::*;

/// The speeds the Speed menu offers (the gig shell's `SPEEDS`): a rate
/// of 1.0 is the record as it is.
pub const SPEEDS: [f64; 6] = [0.5, 0.75, 0.9, 1.0, 1.1, 1.25];

/// How far a nudge (−5 s / +5 s) moves the playhead, in seconds.
pub const NUDGE_S: f64 = 5.0;

/// How far one press of Level − / + moves the level (0..1).
pub const LEVEL_STEP: f64 = 0.05;

/// One section of a song, as the band and the progress bar show it.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionView {
    /// Its short name (`VER 1`, `CHO`).
    pub name: String,
    /// Its colour, `#rrggbb`.
    pub colour: String,
    /// Where it starts and ends, in seconds from the song's start.
    pub start: f64,
    pub end: f64,
}

/// One song in the set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SongView {
    pub title: String,
    /// The key as the chart writes it (`E`, `F#m`); empty when unknown.
    pub key: String,
    /// Beats per minute; 0 when unknown.
    pub bpm: u32,
    /// The song's colour, `#rrggbb` (its number badge).
    pub colour: String,
    pub sections: Vec<SectionView>,
}

/// Everything the performer screen shows of playback, at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackView {
    pub songs: Vec<SongView>,
    /// The song up, by set position.
    pub current: Option<usize>,
    /// The playhead, in seconds.
    pub position: f64,
    /// The song's length, in seconds.
    pub duration: f64,
    pub playing: bool,
    /// A track is being fetched or decoded.
    pub loading: bool,
    /// The loop, when one is on: the section name it holds (`VER 2`).
    pub looping: Option<String>,
    /// The playback rate (1.0 = as recorded).
    pub speed: f64,
    /// The playback level, 0..1.
    pub level: f64,
    /// Whether this device may drive playback. False while following a
    /// peer (sync-locked): taps that would move playback are off.
    pub can_control: bool,
    /// The peer this device follows, when it does (`Studio Mac`).
    pub following: Option<String>,
    /// How many peers are connected.
    pub peers: u32,
    /// Where the playhead is in bars and beats (`41.2`), for an app that
    /// counts in bars (Session fills it from `song_bar_beat`; Signal leaves
    /// it `None`). Shown in the now band's meta line.
    pub bar_beat: Option<String>,
}

impl Default for PlaybackView {
    fn default() -> Self {
        Self {
            songs: Vec::new(),
            current: None,
            position: 0.0,
            duration: 0.0,
            playing: false,
            loading: false,
            looping: None,
            speed: 1.0,
            level: 1.0,
            can_control: true,
            following: None,
            peers: 0,
            bar_beat: None,
        }
    }
}

impl PlaybackView {
    /// The song up.
    #[must_use]
    pub fn song(&self) -> Option<&SongView> {
        self.current.and_then(|i| self.songs.get(i))
    }

    /// The song after the one up, with its set position.
    #[must_use]
    pub fn next(&self) -> Option<(usize, &SongView)> {
        let i = self.current.map_or(0, |c| c + 1);
        self.songs.get(i).map(|s| (i, s))
    }

    /// The section the playhead is in.
    #[must_use]
    pub fn section(&self) -> Option<&SectionView> {
        self.song()?
            .sections
            .iter()
            .find(|s| self.position >= s.start && self.position < s.end)
    }

    /// The song has played to its end (the between-songs state).
    #[must_use]
    pub fn ended(&self) -> bool {
        !self.playing && self.duration > 0.0 && self.position >= self.duration - 0.05
    }
}

/// An app's playback, as the performer screen drives it.
///
/// `view` is read in render, so an adapter backed by signals subscribes
/// the panel that calls it. The actions are fire-and-forget: the view
/// shows their effect when the app's state moves.
pub trait PlaybackModel {
    /// Everything shown, now.
    fn view(&self) -> PlaybackView;
    /// Load the song `by` places away in the set (−1 previous, +1 next).
    fn step_song(&self, by: i32);
    /// Load the song at a set position.
    fn load_song(&self, index: usize);
    /// Move the playhead to `seconds` from the song's start.
    fn seek(&self, seconds: f64);
    fn play_pause(&self);
    /// Move by section (then by song at either end), as Session's
    /// `back_to` / `forward_to` do.
    fn section_step(&self, by: i32);
    /// Loop the section playing, or stop looping.
    fn set_loop(&self, on: bool);
    fn set_speed(&self, rate: f64);
    /// 0..1.
    fn set_level(&self, level: f64);
    /// Stop following a peer and drive playback here.
    fn take_control(&self) {}
    /// Whether this app can jump to the song's very start and end (the
    /// transport's ⋯ menu then offers Go to start / Go to end: the iPad
    /// audit's fix 7, which the classic bar drops at compact widths).
    fn can_go_to(&self) -> bool {
        false
    }
    /// The playhead to the song's start.
    fn go_to_start(&self) {}
    /// The playhead to the song's end.
    fn go_to_end(&self) {}
}

/// The app's [`PlaybackModel`], shared by every panel through context.
///
/// Equal only to itself (the same adapter), so panels re-render on the
/// view changing, not on a new handle.
#[derive(Clone)]
pub struct Playback(pub Rc<dyn PlaybackModel>);

impl PartialEq for Playback {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::ops::Deref for Playback {
    type Target = dyn PlaybackModel;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// Provide the app's playback to the performer panels below.
pub fn provide_playback(model: impl PlaybackModel + 'static) -> Playback {
    use_context_provider(|| Playback(Rc::new(model)))
}

/// The playback the screen above provides.
///
/// # Panics
/// Without a [`provide_playback`] above: a performer panel mounted
/// outside a performer screen is a wiring bug.
#[must_use]
pub fn use_playback() -> Playback {
    use_context::<Playback>()
}

/// `m:ss`, for the clock (`1:48`). Negative or unknown is `0:00`.
#[must_use]
pub fn clock(seconds: f64) -> String {
    let s = if seconds.is_finite() {
        seconds.max(0.0).floor() as u64
    } else {
        0
    };
    format!("{}:{:02}", s / 60, s % 60)
}

/// The speed after `rate` in [`SPEEDS`], wrapping (a keyboard's step).
#[must_use]
pub fn next_speed(rate: f64) -> f64 {
    let at = SPEEDS
        .iter()
        .position(|s| (s - rate).abs() < 1e-6)
        .map_or(3, |i| (i + 1) % SPEEDS.len());
    SPEEDS[at]
}

/// A speed in words: `100%`.
#[must_use]
pub fn speed_label(rate: f64) -> String {
    format!("{}%", (rate * 100.0).round() as i64)
}

/// The playhead nudged by `by` seconds, kept inside the song.
#[must_use]
pub fn nudged(position: f64, duration: f64, by: f64) -> f64 {
    let end = if duration > 0.0 { duration } else { f64::MAX };
    (position + by).clamp(0.0, end)
}

/// A level moved by `steps` of [`LEVEL_STEP`], kept in 0..1.
#[must_use]
pub fn stepped_level(level: f64, steps: i32) -> f64 {
    let v = level + f64::from(steps) * LEVEL_STEP;
    // Whole percents, so ten presses come back to where they started.
    ((v.clamp(0.0, 1.0)) * 100.0).round() / 100.0
}
