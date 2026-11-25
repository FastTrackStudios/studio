//! What a window shows of the songs streaming in: how much of each has
//! arrived, whether a play waits, how they are heard — as the engine last
//! told it (`crate::view_client`), or as this process's own engine keeps it
//! (`studio_engine::stream_set`, which streams them).

/// How far a streamed song can play from where it is: the engine's body
/// answers it (`daw_view::body::Readiness`).
pub use daw_view::body::Readiness;

/// What the window shows of each streaming song, refreshed every half
/// second ([`watch`]): how much of the recording each playing item reads
/// has arrived (by item guid), and the song's own state. [`version`] moves
/// when anything shown changes.
#[derive(Clone, Debug, Default)]
pub struct Shown {
    /// Item guid → share of its recording that has arrived (0..1), for the
    /// items whose recording is not all here.
    pub items: std::collections::HashMap<String, f32>,
    /// Song (engine project) → its state.
    pub songs: std::collections::HashMap<String, SongShown>,
}

/// A streaming song, as its tab shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SongShown {
    /// Share of everything it plays that has arrived (0..1).
    pub arrived: f32,
    /// It can play from its playhead now.
    pub ready: bool,
    /// Recordings missing near the playhead.
    pub waiting_on: u32,
}

static SHOWN: std::sync::RwLock<Option<Shown>> = std::sync::RwLock::new(None);
static SHOWN_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// What the engine told this window streams in (`crate::view::StreamsFrame`,
/// over its link): shown from now on.
pub fn adopt_shown(frame: crate::view::StreamsFrame) {
    let shown = Shown {
        items: frame.items.into_iter().collect(),
        songs: frame
            .songs
            .into_iter()
            .map(|(song, s)| {
                (
                    song,
                    SongShown {
                        arrived: s.arrived,
                        ready: s.ready,
                        waiting_on: s.waiting_on,
                    },
                )
            })
            .collect(),
    };
    keep_shown(shown, frame.waiting, frame.listening);
}

/// This process's [`shown`] (and whether a play waits), moved on.
pub fn keep_shown(shown: Shown, waiting: bool, listening: crate::view::Listening) {
    if let Ok(mut s) = SHOWN.write() {
        *s = Some(shown);
    }
    WAITING.store(waiting, std::sync::atomic::Ordering::Release);
    STEMS.store(listening.stems, std::sync::atomic::Ordering::Release);
    REFERENCED.store(listening.referenced, std::sync::atomic::Ordering::Release);
    SHOWN_VERSION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

/// Whether a play pressed on a song still streaming waits for it, as the
/// engine last told this window.
#[must_use]
pub fn play_waiting() -> bool {
    WAITING.load(std::sync::atomic::Ordering::Acquire)
}

static WAITING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How the streamed songs are heard, as the engine last told this window:
/// by their stems, and whether any is heard by its reference.
#[must_use]
pub fn listening() -> crate::view::Listening {
    crate::view::Listening {
        stems: STEMS.load(std::sync::atomic::Ordering::Acquire),
        referenced: REFERENCED.load(std::sync::atomic::Ordering::Acquire),
    }
}

static STEMS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static REFERENCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What is shown now ([`Shown`]); empty when nothing streams.
#[must_use]
pub fn shown() -> Shown {
    SHOWN
        .read()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or_default()
}

/// Moves when [`shown`] changes: what a window redraws by.
#[must_use]
pub fn version() -> u64 {
    SHOWN_VERSION.load(std::sync::atomic::Ordering::Acquire)
}
