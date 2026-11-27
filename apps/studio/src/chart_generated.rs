//! The chart a song with no chart of its own shows in the browser: the one
//! Keyflow generates from its session's markers (studio#742), as the
//! desktop's chart pane and `SESSION_CHART` show it (studio#693).
//!
//! When the page's engine opens a song (`crate::engine_web`), its timing is
//! written into a session in memory by the desktop's own writer
//! (`studio_engine::session_timing`), and the chart is composed from that
//! session the way the desktop composes it (`keyflow_session`'s
//! `Held::shown_source`): its sections with their bars and measure
//! numbers, its tempos and meters, its markers as cues, every bar a bar of
//! slashes. A song whose chart was found beside it (a `.kf`) keeps that
//! chart; this is only for the rest.

use dioxus::prelude::*;

use crate::session_file::Section;
use crate::state::Place;

/// A generated chart, ready to engrave.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    /// The chart, in Keyflow's language.
    pub text: String,
    /// The song's sections in seconds (its regions on the sections'
    /// ruler, in order): what its bars are timed to.
    pub sections: Vec<Section>,
}

/// Each song the page's engine opened, with its generated chart.
static GENERATED: GlobalSignal<Vec<(Place, Generated)>> = Signal::global(Vec::new);

/// The generated chart of the song at `place`, once the page's engine has
/// opened it. Read in a component, it renders again when the chart comes.
pub fn of(place: &Place) -> Option<Generated> {
    GENERATED
        .read()
        .iter()
        .find(|(p, _)| p == place)
        .map(|(_, g)| g.clone())
}

/// The song at `place` opened in the page's engine as `song`: its
/// generated chart kept for the chart page.
#[cfg(feature = "web")]
pub fn remember(place: &Place, song: &studio_engine::setlist::Song) {
    let mut span = crate::trace::Span::root("studio.chart.generate");
    span.set("studio.chart", song.name.clone());
    match generate(song) {
        Ok(g) => {
            span.set("studio.chart.sections", g.sections.len() as u64);
            span.set("studio.chart.bytes", g.text.len() as u64);
            let mut all = GENERATED.write();
            all.retain(|(p, _)| p != place);
            all.push((place.clone(), g));
        }
        Err(e) => span.fail(e),
    }
}

/// `song`'s chart, generated from its timing as the desktop's is.
#[cfg(feature = "web")]
fn generate(song: &studio_engine::setlist::Song) -> Result<Generated, String> {
    let project = &song.session.project.0;
    let doc = studio_engine::session_timing::session_of(project)?;
    let held = keyflow_session::typed::Held::of_doc(&doc);
    let ruler = held.sections_ruler();
    let mut sections: Vec<Section> = project
        .sections
        .iter()
        .filter(|s| Some(s.lane) == ruler)
        .map(|s| Section {
            name: s.name.clone(),
            start: s.start,
            end: s.end,
        })
        .collect();
    sections.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(Generated {
        text: held.shown_source(&song.name),
        sections,
    })
}
