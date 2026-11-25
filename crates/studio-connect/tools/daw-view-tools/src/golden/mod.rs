//! The golden session: one song that puts on screen everything the
//! arrangement, the mixer and the MIDI editor can show.
//!
//! Built by code, the same every time: no audio files, no clock, no
//! randomness (the noise is a fixed generator), nothing read from an
//! engine. It is the document the views draw —
//! [`daw_ui::studio::project::Project`] with its items' notes and
//! waveforms, its automation lanes, the edit cursor and the play cursor,
//! frozen meter levels and the Signal chains of the kick — so a picture
//! of it is a picture of the views and the theme, and of nothing else.
//!
//! The shots in `daw_theme::gallery` frame it by its track names and
//! times; [`VERSION`] is the session they were written for. Change what a
//! shot frames and bump both.
//!
//! What the views cannot show is left out rather than faked. The track
//! data has no solo-safe, no focus, no per-FX bypass or offline state, no
//! send modes, no play rate on an item and no tempo ramps, so the session
//! has none of those; `docs` in the gallery's report names each.

mod automation;
mod clips;
mod mixer;
mod timeline;
mod tracks;

use std::sync::Arc;

use daw_ui::studio::project::Project;
use daw_view_kit::automation::Lane;
use daw_view_kit::midi::{Note, Wave};

pub use mixer::Meters;

/// The session's version, as `daw_theme::gallery::SESSION` names it.
pub const VERSION: u32 = 1;

/// The song's length, in seconds.
pub const LENGTH: f64 = 96.0;

/// Everything the views draw of the golden session.
#[derive(Clone)]
pub struct Golden {
    /// The project as the engine would hand it over, before any folding.
    pub project: Project,
    /// The folders shown shut, by guid.
    pub shut: Vec<String>,
    /// Each MIDI item's notes, by item guid.
    pub notes: Vec<(String, Vec<Note>)>,
    /// Each audio item's waveform, by item guid.
    pub waves: Vec<(String, Wave)>,
    /// Each track's automation lanes, by track guid.
    pub lanes: Vec<(String, Vec<Lane>)>,
    /// The edit cursor and the time selection.
    pub cursor: arrangement_view::cursor::Edit,
    /// Where the play cursor stands, in seconds.
    pub play_at: f64,
    /// The meters, frozen.
    pub meters: Meters,
    /// The Signal chains on the kick's tracks, by track guid, and those
    /// shown shut.
    pub signal: Vec<(String, mixer_view::signal::Chain)>,
    pub signal_shut: Vec<String>,
    /// The two MIDI takes the editor opens: the drum groove and the melody.
    pub drums: Take,
    pub melody: Take,
}

/// A MIDI take the editor opens, as the engine hands one over.
#[derive(Clone)]
pub struct Take {
    /// Its item's guid.
    pub item: String,
    pub snapshot: daw_proto::midi::MidiTakeSnapshot,
}

/// Build the golden session.
///
#[must_use]
pub fn song() -> Golden {
    let tracks = tracks::all();
    let clips = clips::all(&tracks);
    let (signal, signal_shut) = mixer::signal();
    let project = Project {
        tracks: tracks.clone(),
        items: clips.items_by_track(),
        sections: timeline::sections(),
        markers: timeline::markers(),
        bpm: timeline::TEMPO.first().map_or(120.0, |t| t.bpm),
        tempo: timeline::TEMPO.to_vec(),
        length_secs: LENGTH,
        item_count: clips.items.len(),
        names: clips.names.clone(),
        midi: clips.midi.clone(),
        ..Project::default()
    };
    Golden {
        shut: tracks::shut(&tracks),
        notes: clips.notes.clone(),
        waves: clips.waves,
        lanes: automation::lanes(&tracks),
        cursor: timeline::cursor(),
        play_at: timeline::PLAY_AT,
        meters: mixer::meters(&tracks),
        signal,
        signal_shut,
        drums: clips::take(clips::DRUM_ITEM, &project),
        melody: clips::take(clips::MELODY_ITEM, &project),
        project,
    }
}

impl Golden {
    /// The track named `name`.
    #[must_use]
    pub fn track(&self, name: &str) -> Option<&daw_proto::Track> {
        self.project.tracks.iter().find(|t| t.name == name)
    }

    /// The index of the track `guid`, as its meters are kept.
    #[must_use]
    pub fn track_index(&self, guid: &str) -> Option<usize> {
        self.project.tracks.iter().position(|t| t.guid == guid)
    }

    /// Where the item `guid` starts, in seconds.
    #[must_use]
    pub fn item_start(&self, guid: &str) -> Option<f64> {
        self.project
            .items
            .values()
            .flatten()
            .find(|i| i.guid == guid)
            .map(|i| i.position.as_seconds())
    }

    /// The notes and waveforms, as the views read them.
    #[must_use]
    pub fn previews(&self) -> daw_view_kit::midi::Previews {
        let previews = daw_view_kit::midi::Previews::default();
        previews.put_notes(self.notes.iter().cloned());
        previews.put_waves(self.waves.iter().cloned());
        previews
    }

    /// The project, shared the way the views hold it.
    #[must_use]
    pub fn project_ref(&self) -> daw_ui::studio::ProjectRef {
        daw_ui::studio::ProjectRef(Arc::new(self.project.clone()))
    }
}

#[cfg(test)]
mod tests;
