//! Studio, as the views' contract asks of the app a window runs in
//! (`daw_view::host`): the song on screen, the engine in this process, the
//! set, the streaming state and the live session as Studio shows them,
//! which folders its template starts shut, and what its drum editing is
//! dialled to.
//!
//! Installed by every way a Studio window comes to an engine — an open, an
//! attach, a link ([`install`], idempotent) — and by the engine as it
//! comes up in a process.

use daw_proto::Track;

/// Studio's answers.
struct Studio;

impl daw_view::host::Host for Studio {
    fn current_song(&self) -> Option<String> {
        #[cfg(feature = "native")]
        return crate::client::open::song_here();
        #[cfg(not(feature = "native"))]
        crate::client::open::page_song()
    }

    #[cfg(feature = "native")]
    fn local_engine(&self) -> Option<&'static daw::standalone::Standalone> {
        crate::client::open::local_engine()
    }

    #[cfg(feature = "native")]
    fn set_arrived(&self, frame: crate::client::view::SetFrame) {
        crate::client::view_client::set_arrived(frame);
    }

    #[cfg(feature = "native")]
    fn streams_arrived(&self, frame: crate::client::view::StreamsFrame) {
        crate::client::stream_set::adopt_shown(frame);
    }

    fn live_arrived(&self, frame: &crate::client::view::LiveFrame) {
        crate::client::presence::publish_everyone(
            frame
                .people
                .iter()
                .map(|p| crate::client::presence::Person {
                    project: p.project.clone(),
                    song: p.song.clone(),
                    name: p.name.clone(),
                    color: p.color,
                })
                .collect(),
        );
    }

    #[cfg(feature = "native")]
    fn streaming(&self) -> (u64, std::collections::HashMap<String, f32>) {
        (
            crate::client::stream_set::version(),
            crate::client::stream_set::shown().items,
        )
    }

    #[cfg(feature = "web")]
    fn output_latency(&self) -> f64 {
        crate::client::web_audio::latency_seconds()
    }

    fn shut_by_default(&self, folder: &Track) -> bool {
        shut_by_default(&folder.name)
    }

    fn shuts_its_folders(&self, name: &str) -> bool {
        shuts_its_folders(name)
    }

    fn edit_grid(&self) -> u32 {
        crate::client::drum_settings::current().grid
    }
}

/// Whether a folder starts shut until it is opened: the template's bus
/// tree, the reference material and the rehearsal in it, and the guide's
/// rendered prints. Buses are wiring and references are for listening
/// against, not tracks you work on — opened, they were rows of nothing
/// between the instruments.
fn shut_by_default(name: &str) -> bool {
    let name = name.trim();
    [
        dynamic_template::buses::names::MIX,
        dynamic_template::apply::REFERENCES_FOLDER,
        dynamic_template::apply::REHEARSAL_FOLDER,
        dynamic_template::apply::PRINTS_FOLDER,
    ]
    .iter()
    .any(|shut| name.eq_ignore_ascii_case(shut))
}

/// Whether a folder's own folders start shut: the guitars' (`Electric
/// GTR`, `GTR E`, `Guitars`) — each a part — and the drums' — each a piece
/// of the kit — by the template's classification.
fn shuts_its_folders(name: &str) -> bool {
    dynamic_template::track_schema::classify_track(name)
        .matched_groups
        .first()
        .is_some_and(|g| g == "Guitars" || g == "Drums")
}

static STUDIO: Studio = Studio;

/// Install Studio as the app behind this process's windows. Idempotent.
pub fn install() {
    daw_view::host::install(&STUDIO);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_s_buses_and_references_start_shut() {
        assert!(shut_by_default(dynamic_template::buses::names::MIX));
        assert!(shut_by_default(dynamic_template::apply::REFERENCES_FOLDER));
        assert!(!shut_by_default("Vocals"));
    }

    #[test]
    fn a_kit_s_pieces_start_shut() {
        assert!(shuts_its_folders("Drums"));
        assert!(!shuts_its_folders("Vocals"));
    }
}
