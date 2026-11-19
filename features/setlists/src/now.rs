//! What is playing: the setlist's running order as this extension's
//! source on Architect's playhead (`architect.playhead`, architect#589),
//! the one clock every view of a song follows.
//!
//! - **The queue** ([`queue`]): a setlist's songs in running order, an
//!   item each (`id` the song's note and its arrangement, [`item_id`];
//!   its title, artist, length and recording). `PLAYHEAD_SET` puts it on
//!   the deck; the app's Now Playing bar shows it.
//! - **The clock**: the deck's own (Architect's silent clock on a desk,
//!   the page's audio in a browser), or, while a Studio engine runs,
//!   the engine's transport (`crate::engine_clock`), which moves the deck
//!   to the song and the time Studio plays. Either way every view reads
//!   one state ([`Now`], `PLAYHEAD_STATE`), and Lighting reads it as
//!   `studio.setlists`' `now` (`STUDIO_SETLISTS_NOW`).
//!
//! Pure but for [`state`] and the controls, which go through the host.

use serde_json::{Value, json};

use crate::host::{SetlistRow, SongRow};
use crate::layout::SongRef;

/// The item id of a song of a setlist: its note's vault path, and the
/// arrangement played as its heading (`…/Know Me/Know Me.md#Extended`).
#[must_use]
pub fn item_id(at: &SongRef) -> String {
    let note = format!("{}.md", at.library().song_link_target(&at.title));
    at.arrangement
        .as_ref()
        .map_or(note.clone(), |a| format!("{note}#{a}"))
}

/// The song an item id names (its note path and arrangement).
#[must_use]
pub fn song_of(id: &str) -> Option<(String, Option<String>)> {
    let (note, arrangement) = match id.split_once('#') {
        Some((n, a)) => (n, Some(a.to_owned()).filter(|a| !a.is_empty())),
        None => (id, None),
    };
    note.ends_with(".md")
        .then(|| (note.to_owned(), arrangement))
}

/// A setlist's running order as the playhead's queue (`PLAYHEAD_SET`'s
/// `queue`): a song the vault does not know, or without a length or a
/// recording to time it by, is left out (the deck cannot play an item
/// it cannot time).
#[must_use]
pub fn queue(set: &SetlistRow, songs: &[SongRow]) -> Vec<Value> {
    set.songs
        .iter()
        .filter_map(|at| {
            let song = songs
                .iter()
                .find(|s| s.at.library == at.library && s.at.title == at.title)?;
            let mut item = serde_json::Map::new();
            item.insert("id".into(), json!(item_id(at)));
            item.insert("title".into(), json!(at.label()));
            if let Some(artist) = &song.artist {
                item.insert("subtitle".into(), json!(artist));
            }
            if let Some(length) = song.length {
                item.insert("duration".into(), json!(length));
            }
            if let Some(media) = &song.recording {
                item.insert("media".into(), json!(media));
            }
            (song.length.is_some() || song.recording.is_some()).then_some(Value::Object(item))
        })
        .collect()
}

/// The playhead's state of this extension's source, as the views read it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Now {
    /// The current item's id ([`item_id`]).
    pub item: Option<String>,
    pub title: Option<String>,
    /// Seconds into the item at `at`.
    pub time: f64,
    /// When `time` was read, ms since the epoch.
    pub at: f64,
    pub playing: bool,
    pub rate: f64,
    /// The item's length, seconds.
    pub duration: f64,
    /// The queue's item ids, in order.
    pub queue: Vec<String>,
    /// Whether the app shows this source in its bar.
    pub active: bool,
}

impl Now {
    /// From `PLAYHEAD_STATE`'s answer (None for `null`: no source).
    #[must_use]
    pub fn of(state: &Value) -> Option<Self> {
        let item = state.get("item")?;
        let num = |k: &str| state.get(k).and_then(Value::as_f64);
        Some(Self {
            item: item.get("id").and_then(Value::as_str).map(str::to_owned),
            title: item.get("title").and_then(Value::as_str).map(str::to_owned),
            time: num("time").unwrap_or(0.0),
            at: num("at").unwrap_or(0.0),
            playing: state
                .get("playing")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rate: num("rate").filter(|r| *r > 0.0).unwrap_or(1.0),
            duration: num("duration").unwrap_or(0.0),
            queue: state
                .get("queue")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect(),
            active: state
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// Seconds into the item at `now_ms`: carried on at its rate while it
    /// plays, never past its length.
    #[must_use]
    pub fn at(&self, now_ms: f64) -> f64 {
        let t = if self.playing {
            ((now_ms - self.at) / 1000.0).mul_add(self.rate, self.time)
        } else {
            self.time
        };
        let t = t.max(0.0);
        if self.duration > 0.0 {
            t.min(self.duration)
        } else {
            t
        }
    }

    /// The song playing: its note path and arrangement.
    #[must_use]
    pub fn song(&self) -> Option<(String, Option<String>)> {
        self.item.as_deref().and_then(song_of)
    }
}

/// Now, ms since the epoch (the deck's `at`).
#[must_use]
pub fn now_ms() -> f64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Whether a Studio engine is moving the playhead now (its transport is
/// the clock: a view's play and seek are Studio's to do).
static ENGINE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a Studio engine drives the playhead.
#[must_use]
pub fn engine_drives() -> bool {
    ENGINE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Said by the engine follower as it attaches and lets go.
pub fn set_engine_drives(on: bool) {
    ENGINE.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(feature = "extension")]
mod control {
    use super::{Now, queue};
    use crate::host::{self, SetlistRow};
    use serde_json::{Value, json};

    /// This extension's source on the playhead, now.
    pub async fn state() -> Option<Now> {
        host::run("PLAYHEAD_STATE", json!({}))
            .await
            .ok()
            .as_ref()
            .and_then(Now::of)
    }

    /// Puts `set`'s running order on the playhead, unless it is there
    /// already; at `start` (an item id) when given. Answers the state.
    ///
    /// # Errors
    ///
    /// The playhead refused (no item it can time).
    pub async fn load(set: &SetlistRow, start: Option<&str>) -> Result<Option<Now>, String> {
        let songs = host::songs().await?;
        let items = queue(set, &songs);
        let ids: Vec<String> = items
            .iter()
            .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect();
        let current = state().await;
        if current.as_ref().is_some_and(|c| c.queue == ids) {
            return Ok(current);
        }
        let index = start
            .and_then(|s| ids.iter().position(|i| i == s))
            .unwrap_or(0);
        let out = host::run(
            "PLAYHEAD_SET",
            json!({ "name": set.title, "queue": items, "index": index }),
        )
        .await?;
        Ok(Now::of(&out))
    }

    /// Item `id` from `t` seconds, playing or not.
    ///
    /// # Errors
    ///
    /// The playhead refused.
    pub async fn go(id: &str, t: f64, play: bool) -> Result<(), String> {
        host::run("PLAYHEAD_ITEM", json!({ "id": id, "play": play })).await?;
        if t > 0.0 {
            host::run("PLAYHEAD_SEEK", json!({ "t": t })).await?;
        }
        Ok(())
    }

    /// To `t` seconds of the item playing.
    ///
    /// # Errors
    ///
    /// The playhead refused.
    pub async fn seek(t: f64) -> Result<(), String> {
        host::run("PLAYHEAD_SEEK", json!({ "t": t.max(0.0) }))
            .await
            .map(drop)
    }

    /// Play or pause.
    ///
    /// # Errors
    ///
    /// The playhead refused.
    pub async fn toggle() -> Result<(), String> {
        host::run("PLAYHEAD_TOGGLE", json!({})).await.map(drop)
    }
}

#[cfg(feature = "extension")]
pub use control::{go, load, seek, state, toggle};

impl Now {
    /// From `studio.setlists`' `now` (`STUDIO_SETLISTS_NOW`'s answer; None
    /// for `null`): what another extension reads.
    #[must_use]
    pub fn of_service(v: &Value) -> Option<Self> {
        let item = v.get("item").and_then(Value::as_str)?.to_owned();
        let num = |k: &str| v.get(k).and_then(Value::as_f64);
        Some(Self {
            item: Some(item),
            title: v.get("title").and_then(Value::as_str).map(str::to_owned),
            time: num("time").unwrap_or(0.0),
            at: num("at").unwrap_or(0.0),
            playing: v.get("playing").and_then(Value::as_bool).unwrap_or(false),
            rate: num("rate").filter(|r| *r > 0.0).unwrap_or(1.0),
            duration: num("duration").unwrap_or(0.0),
            queue: Vec::new(),
            active: true,
        })
    }
}

/// The service other extensions follow the playhead by.
pub const SERVICE: &str = "studio.setlists";

/// How a pane reads the playhead: setlists' own views read its source;
/// another extension's (Chart, Lyrics, Lighting) asks `studio.setlists`
/// (`now`, `seek`) as itself (its manifest `[uses]` it).
#[cfg(feature = "extension")]
#[derive(Clone, Debug, Default)]
pub enum Reader {
    #[default]
    Own,
    Service(architect_extensions::linked::Handle),
}

#[cfg(feature = "extension")]
impl PartialEq for Reader {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Own, Self::Own) | (Self::Service(_), Self::Service(_))
        )
    }
}

#[cfg(feature = "extension")]
impl Reader {
    /// The song playing and where.
    pub async fn state(&self) -> Option<Now> {
        match self {
            Self::Own => state().await,
            Self::Service(h) => h
                .service(SERVICE, "now", json!({}))
                .await
                .ok()
                .as_ref()
                .and_then(Now::of_service),
        }
    }

    /// Plays from `t` seconds of item `item` (a seek when it plays, else
    /// that song of the queue from there). Refused while Studio's engine
    /// is the clock.
    ///
    /// # Errors
    ///
    /// Why it did not move.
    pub async fn seek(&self, item: &str, t: f64) -> Result<(), String> {
        match self {
            Self::Own => seek_item(item, t).await,
            Self::Service(h) => h
                .service(SERVICE, "seek", json!({ "item": item, "t": t }))
                .await
                .map(drop)
                .map_err(|e| e.to_string()),
        }
    }
}

/// `STUDIO_SETLISTS_SEEK`: item `item` from `t` seconds.
///
/// # Errors
///
/// Studio's engine is the clock, the item is not on the playhead, or it
/// refused.
#[cfg(feature = "extension")]
pub async fn seek_item(item: &str, t: f64) -> Result<(), String> {
    if engine_drives() {
        return Err("Studio's engine is the clock: play and seek in Studio".to_owned());
    }
    match state().await {
        Some(s) if s.item.as_deref() == Some(item) => seek(t).await,
        Some(s) if s.queue.iter().any(|i| i == item) => go(item, t, false).await,
        _ => Err(format!("{item} is not on the playhead")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setlist::Set;

    fn row(title: &str, length: Option<f64>) -> SongRow {
        SongRow {
            at: SongRef::new("JT", title, None),
            path: format!("Libraries/JT/{title}/{title}.md"),
            key: None,
            key_end: None,
            key_outside: false,
            tempo: None,
            meter: None,
            artist: Some("Royal Blood".into()),
            length,
            categories: Vec::new(),
            arrangements: Vec::new(),
            recording: Some(format!("Libraries/JT/{title}/Media/x.ogg")),
        }
    }

    #[test]
    fn an_item_is_its_song_and_arrangement() {
        let at = SongRef::new("JT", "Know Me", Some("Extended"));
        let id = item_id(&at);
        assert_eq!(id, "Libraries/JT/Know Me/Know Me.md#Extended");
        assert_eq!(
            song_of(&id),
            Some((
                "Libraries/JT/Know Me/Know Me.md".to_owned(),
                Some("Extended".to_owned())
            ))
        );
        assert_eq!(
            song_of("Libraries/JT/Fire/Fire.md").map(|s| s.1),
            Some(None)
        );
        assert_eq!(song_of("not a note"), None);
    }

    #[test]
    fn the_queue_is_the_running_order_of_known_songs() {
        let set = SetlistRow {
            path: "Setlists/S.md".into(),
            title: "S".into(),
            date: None,
            songs: vec![
                SongRef::new("JT", "Fire", None),
                SongRef::new("JT", "Gone", None),
                SongRef::new("JT", "Know Me", Some("Extended")),
            ],
            sets: vec![Set {
                name: Some("Act".into()),
                len: 3,
            }],
            share: None,
        };
        let songs = vec![row("Fire", Some(206.0)), row("Know Me", Some(304.1))];
        let q = queue(&set, &songs);
        assert_eq!(q.len(), 2, "a song the vault lacks is left out");
        assert_eq!(q[1]["id"], "Libraries/JT/Know Me/Know Me.md#Extended");
        assert_eq!(q[1]["title"], "Know Me (Extended)");
        assert_eq!(q[0]["duration"], 206.0);
        assert_eq!(q[0]["subtitle"], "Royal Blood");
    }

    #[test]
    fn the_state_is_carried_on_at_its_rate() {
        let now = Now::of(&json!({
            "item": {"id": "Libraries/JT/Fire/Fire.md", "title": "Fire"},
            "time": 10.0, "at": 1_000.0, "playing": true, "rate": 1.0, "duration": 11.0,
            "queue": [{"id": "Libraries/JT/Fire/Fire.md"}], "active": true
        }))
        .unwrap_or_default();
        assert!((now.at(1_500.0) - 10.5).abs() < 1e-9);
        assert!(
            (now.at(9_000.0) - 11.0).abs() < 1e-9,
            "never past its length"
        );
        assert_eq!(now.queue.len(), 1);
        assert_eq!(Now::of(&Value::Null), None);
    }
}
