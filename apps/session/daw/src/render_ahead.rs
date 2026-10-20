//! The render ahead, per track: `TRACK_RENDER_LIVE`.
//!
//! The engine renders the tracks nobody plays live ahead of the playhead,
//! on a thread of their own, and the live ones — armed and monitoring,
//! played over MIDI, a plugin's window open on them — in the audio
//! callback (`daw::standalone::audio_engine::ahead`). A track can be kept
//! live always: REAPER's "prevent anticipative FX", saved with the song
//! as its `PERF` line.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{TrackRef, Tracks};
use facet::Facet;

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

pub(crate) fn register(list: &ActionList) {
    list.register(&TRACK_RENDER_LIVE, Effect::Mutating, |a: RenderLiveArg| {
        render_live(&a)
    });
}
