//! The render ahead: whether the engine renders ahead at all and how far
//! (`SESSION_RENDER_AHEAD`, the Settings page), and which tracks it keeps
//! live (`TRACK_RENDER_LIVE`).
//!
//! The engine renders the tracks nobody plays live ahead of the playhead,
//! on a thread of their own, and the live ones — armed and monitoring,
//! played over MIDI, a plugin's window open on them — in the audio
//! callback (`daw::standalone::audio_engine::ahead`): a busy machine eats
//! into the time rendered ahead rather than into the callback's deadline,
//! and a song too big for one block still plays. How far ahead is yours,
//! not the song's (`~/.config/fts/render-ahead.json`); a track kept live
//! always is the song's — REAPER's "prevent anticipative FX", saved with
//! it as its `PERF` line.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{TrackRef, Tracks};
use facet::Facet;

/// Whether the engine renders ahead, and how far, ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct RenderAhead {
    pub on: bool,
    pub depth_ms: u32,
}

impl Default for RenderAhead {
    fn default() -> Self {
        Self {
            on: true,
            depth_ms: 200,
        }
    }
}

/// The depths the Settings page offers, ms: REAPER's default among them.
pub const DEPTHS: [u32; 4] = [50, 100, 200, 400];

/// The longest render ahead taken, ms.
const MOST_MS: u32 = 2_000;

impl RenderAhead {
    /// The engine's prefs for it: a device opened as Session opens it.
    #[must_use]
    pub fn io_prefs(self) -> daw::standalone::audio_engine::AudioIoPrefs {
        daw::standalone::audio_engine::AudioIoPrefs {
            render_ahead_ms: self.depth_ms,
            no_render_ahead: !self.on,
            ..Default::default()
        }
    }
}

fn store() -> &'static std::sync::Mutex<RenderAhead> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<RenderAhead>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new(load().unwrap_or_default()))
}

/// The render ahead as it was last kept — what a window shows, read
/// afresh: the engine that keeps it is another process.
#[must_use]
pub fn saved() -> RenderAhead {
    load().unwrap_or_default()
}

/// The render ahead as it stands.
#[must_use]
pub fn current() -> RenderAhead {
    *store()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Change it, keep it, and play on with it: the audio device is opened
/// again, which is how an engine takes it up. Says what it is now.
pub fn set(want: RenderAhead) -> RenderAhead {
    let now = RenderAhead {
        depth_ms: want.depth_ms.clamp(1, MOST_MS),
        ..want
    };
    let changed = {
        let mut held = store()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::replace(&mut *held, now) != now
    };
    if changed {
        save(now);
        crate::open::restart_audio();
    }
    now
}

fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("render-ahead.json"))
}

fn load() -> Option<RenderAhead> {
    let text = std::fs::read(path()?).ok()?;
    facet_json::from_slice(&text).ok()
}

fn save(r: RenderAhead) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    match facet_json::to_vec(&r) {
        Ok(bytes) => {
            if std::fs::write(&path, bytes).is_err() {
                tracing::warn!("render ahead: the setting could not be saved");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "render ahead: the setting would not serialize")
        }
    }
}

static SESSION_RENDER_AHEAD: ActionMeta = crate::actions::meta(
    "SESSION_RENDER_AHEAD",
    "session_render_ahead",
    "Render ahead",
    "Whether the engine renders the tracks nobody plays live ahead of the playhead (`on`), and how far (`depth_ms`, 1–2000; 200 is REAPER's default): the slack a busy machine is absorbed in. Edits are heard at once either way. Either left out stays as it is. Kept for every song; the audio device is opened again to take it up. Says what it is now.",
    "Audio",
    "",
);

static TRACK_RENDER_LIVE: ActionMeta = crate::actions::meta(
    "TRACK_RENDER_LIVE",
    "track_render_live",
    "Render a track live",
    "Keep `track` (a path, name or GUID) rendered live always — in the audio callback, every change to it heard at once — or let it be rendered ahead of the playhead whenever nothing plays it (`on` true or false, toggled when left out). REAPER's \"prevent anticipative FX\", saved with the song. Armed, monitored, played over MIDI or with a plugin's window open, a track is live anyway. Says what it is now.",
    "Tracks",
    "",
);

/// Which track, live always or not.
#[derive(Facet)]
struct RenderLiveArg {
    track: String,
    on: Option<bool>,
}

/// Whether the track renders live always now.
#[derive(Facet)]
struct RenderLiveOut {
    track: String,
    render_live: bool,
}

fn render_live(a: &RenderLiveArg) -> Result<RenderLiveOut, String> {
    let (daw, project) = crate::actions::song()?;
    let guid = crate::actions::resolve(daw, &project, &a.track)?;
    let at = || TrackRef::Guid(guid.clone());
    let on =
        a.on.unwrap_or_else(|| !Tracks::render_live(daw, project.clone(), at()));
    Tracks::set_render_live(daw, project.clone(), at(), on).map_err(|e| e.to_string())?;
    let track = Tracks::get(daw, project.clone(), at()).ok_or("the track went")?;
    crate::actions::list().changed("tracks", &guid);
    Ok(RenderLiveOut {
        track: track.name,
        render_live: Tracks::render_live(daw, project, at()),
    })
}

/// The render ahead changed: either field left out stays.
#[derive(Facet)]
struct RenderAheadArg {
    on: Option<bool>,
    depth_ms: Option<u32>,
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(
        &SESSION_RENDER_AHEAD,
        Effect::Mutating,
        |a: RenderAheadArg| {
            let was = current();
            Ok::<_, String>(set(RenderAhead {
                on: a.on.unwrap_or(was.on),
                depth_ms: a.depth_ms.unwrap_or(was.depth_ms),
            }))
        },
    )?;
    list.register(&TRACK_RENDER_LIVE, Effect::Mutating, |a: RenderLiveArg| {
        render_live(&a)
    })?;
    Ok(())
}
