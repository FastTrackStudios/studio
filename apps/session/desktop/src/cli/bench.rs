//! `studio open … --bench`: how long the running window takes to open a set.
//!
//! After the open is asked for, the window is polled every [`POLL`] —
//! `SESSION_SONGS` (each song `open` or `arriving`, which is `current`) and
//! `SESSION_STREAMS` (each streamed song `local`, `arriving` or
//! `streaming`) — until every song is open and, when the set streams in,
//! every song is local; or until the timeout. One JSON object says when
//! each point was reached, in ms after the request was sent.
//!
//! The timing itself is [`Timings`], a fold over what each poll saw, so it
//! is tested without a window; [`bench`] is the loop that feeds it.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::app;

/// How often the window is asked.
pub const POLL: Duration = Duration::from_millis(50);

/// One song as `SESSION_SONGS` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SongSeen {
    pub number: u32,
    pub title: String,
    /// The engine project; empty while the song is arriving.
    #[serde(default)]
    pub project: String,
    pub current: bool,
    /// `open`, or `arriving`.
    pub state: String,
}

impl SongSeen {
    fn is_open(&self) -> bool {
        self.state == "open"
    }
}

/// One streamed song as `SESSION_STREAMS` lists it (the fields used here).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct StreamSeen {
    /// The engine project it streams into.
    pub project: String,
    /// `local`, `arriving` or `streaming`.
    pub state: String,
}

/// What one poll of the window saw, `elapsed_ms` after the open was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub elapsed_ms: u64,
    pub songs: Vec<SongSeen>,
    /// `None` when the window would not say (no `SESSION_STREAMS`).
    pub streams: Option<Vec<StreamSeen>>,
}

/// When a song of the set became open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SongTiming {
    pub number: u32,
    pub title: String,
    pub open_ms: u64,
}

/// The timings so far, from the observations folded in with
/// [`Timings::observe`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timings {
    /// What `SESSION_SONGS` said before the open was asked for. Polls that
    /// still say exactly this are the old set (or none) and are not counted:
    /// the window has not started on the request yet.
    baseline: Vec<SongSeen>,
    /// A poll has differed from `baseline`.
    started: bool,
    pub first_song_ms: Option<u64>,
    pub all_open_ms: Option<u64>,
    pub all_local_ms: Option<u64>,
    /// How many songs the set had when every one was open.
    pub songs: usize,
    /// Each song's first poll seen open, by its number.
    open_at: BTreeMap<u32, SongTiming>,
    /// The set's songs stream in (the latest poll's streams name them).
    streamed: bool,
}

impl Timings {
    /// Nothing seen yet; `baseline` is what the window had open before.
    #[must_use]
    pub fn after(baseline: Vec<SongSeen>) -> Self {
        let started = baseline.is_empty();
        Self {
            baseline,
            started,
            first_song_ms: None,
            all_open_ms: None,
            all_local_ms: None,
            songs: 0,
            open_at: BTreeMap::new(),
            streamed: false,
        }
    }

    /// Fold one poll in.
    pub fn observe(&mut self, seen: &Observation) {
        if !self.started {
            if seen.songs == self.baseline {
                return;
            }
            self.started = true;
        }
        let at = seen.elapsed_ms;
        for song in seen.songs.iter().filter(|s| s.is_open()) {
            self.open_at
                .entry(song.number)
                .or_insert_with(|| SongTiming {
                    number: song.number,
                    title: song.title.clone(),
                    open_ms: at,
                });
        }
        if self.first_song_ms.is_none() && seen.songs.iter().any(|s| s.current && s.is_open()) {
            self.first_song_ms = Some(at);
        }
        let all_open = !seen.songs.is_empty() && seen.songs.iter().all(SongSeen::is_open);
        if self.all_open_ms.is_none() && all_open {
            self.all_open_ms = Some(at);
            self.songs = seen.songs.len();
        }
        // Only this set's streams: the process keeps those of sets opened
        // before it too.
        let ours: Vec<&StreamSeen> = seen
            .streams
            .iter()
            .flatten()
            .filter(|st| seen.songs.iter().any(|s| s.project == st.project))
            .collect();
        // A song that does not stream in is local already.
        self.streamed = !ours.is_empty();
        if self.all_local_ms.is_none()
            && all_open
            && self.streamed
            && ours.iter().all(|st| st.state == "local")
        {
            self.all_local_ms = Some(at);
        }
    }

    /// Every song is open, and local too when the set streams in.
    #[must_use]
    pub fn done(&self) -> bool {
        self.all_open_ms.is_some() && (!self.streamed || self.all_local_ms.is_some())
    }

    /// Each song's open time, in set order.
    #[must_use]
    pub fn per_song(&self) -> Vec<SongTiming> {
        self.open_at.values().cloned().collect()
    }
}

/// [`Timings`] over a whole run of observations, in order.
#[must_use]
pub fn timings(baseline: Vec<SongSeen>, observations: &[Observation]) -> Timings {
    let mut timings = Timings::after(baseline);
    for seen in observations {
        timings.observe(seen);
    }
    timings
}

/// The JSON `--bench` prints.
#[derive(Debug, Serialize)]
struct Report<'a> {
    requested_at: String,
    action: &'a str,
    /// What the window answered the open with (its JSON, or the text).
    said: Value,
    first_song_ms: Option<u64>,
    all_open_ms: Option<u64>,
    all_local_ms: Option<u64>,
    song_count: usize,
    songs: Vec<SongTiming>,
    polls: u64,
    timed_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
}

/// Ask the window at `url` to run `action` (an open), then poll it until the
/// set is open (and local) or `timeout` has passed; print the [`Report`].
///
/// # Errors
///
/// When the window cannot be reached before the request, refuses the open,
/// or the timeout passes first (the report is printed all the same).
pub fn bench(url: &str, action: &str, args: Value, timeout: Duration) -> eyre::Result<()> {
    let call_timeout = app::CALL_TIMEOUT.min(timeout);
    let baseline = songs(url, call_timeout)?;
    let requested_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let start = Instant::now();
    let said = app::run_within(url, action, args, call_timeout)?;
    let mut timings = Timings::after(baseline);
    let mut polls = 0_u64;
    let mut last_error = None;
    while !timings.done() {
        let Some(left) = timeout.checked_sub(start.elapsed()) else {
            break;
        };
        match observe(url, left.min(call_timeout), start) {
            Ok(seen) => {
                polls = polls.saturating_add(1);
                timings.observe(&seen);
            }
            Err(e) => last_error = Some(format!("{e}")),
        }
        if !timings.done() {
            std::thread::sleep(POLL.min(timeout.saturating_sub(start.elapsed())));
        }
    }
    let timed_out = !timings.done();
    let report = Report {
        requested_at,
        action,
        said: serde_json::from_str(&said).unwrap_or(Value::String(said)),
        first_song_ms: timings.first_song_ms,
        all_open_ms: timings.all_open_ms,
        all_local_ms: timings.all_local_ms,
        song_count: timings.songs,
        songs: timings.per_song(),
        polls,
        timed_out,
        last_error: if timed_out { last_error } else { None },
    };
    println!("{}", serde_json::to_string(&report)?);
    if timed_out {
        eyre::bail!("the set was not open after {} s", timeout.as_secs_f64());
    }
    Ok(())
}

/// One poll: the set's songs, then its streams.
fn observe(url: &str, timeout: Duration, start: Instant) -> eyre::Result<Observation> {
    let songs = songs(url, timeout)?;
    // A window without SESSION_STREAMS times the songs alone.
    let streams = app::run_within(url, "SESSION_STREAMS", json!({"top": 0}), timeout)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    Ok(Observation {
        elapsed_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        songs,
        streams,
    })
}

/// The window's `SESSION_SONGS`.
///
/// # Errors
///
/// When the window cannot be reached, or says something that is not a list
/// of songs.
pub fn songs(url: &str, timeout: Duration) -> eyre::Result<Vec<SongSeen>> {
    let text = app::run_within(url, "SESSION_SONGS", Value::Null, timeout)?;
    serde_json::from_str(&text).map_err(|e| eyre::eyre!("SESSION_SONGS said {text:?}: {e}"))
}

#[cfg(test)]
mod tests;
