//! The lyrics panel: the song's words, following the song — Session's
//! window's reading of them.
//!
//! The panel itself (its Audience, Performer and Confidence Monitor views,
//! and the switch between them) is `session::ui::components::lyrics`, which
//! the setlists extension's song workspace draws too. What is here is where
//! the window takes its words and its play position from: the lines are the
//! song's own, the Lyrics track's items, one a line, labelled with its text
//! ([`session::lyrics::stamp_lines`] puts them there from a synced `.lrc`
//! when a song is prepared), against the song's section regions; the
//! position is the transport's.

use dioxus::prelude::*;
use session::lyrics::{Lyrics, SectionSpan};
use session::ui::components::lyrics::{ColoredSpan, LyricsView, Words};
pub use session::ui::components::lyrics::{LyricsChoice, View};

use crate::song_model::StudioSession;

/// The song's lyrics as the panel shows them, read off the open session.
fn words_of(session: &StudioSession) -> Words {
    let project = &session.project;
    let lyrics = project
        .tracks
        .iter()
        .find(|t| session::lyrics::is_lyrics_track(&t.name))
        .map(|track| {
            Lyrics::from_items(project.lane(&track.guid).iter().filter_map(|item| {
                Some((
                    item.position.as_seconds(),
                    item.length.as_seconds(),
                    project.title(item)?,
                ))
            }))
        })
        .unwrap_or_default();
    let regions = project
        .sections
        .iter()
        .filter(|r| r.lane == crate::ruler::SECTIONS_ROW as u32)
        .map(|r| ColoredSpan {
            span: SectionSpan {
                name: r.name.clone(),
                start: r.start,
                end: r.end,
            },
            color: r.color.clone(),
        })
        .collect();
    let meta = session.chart.as_ref().map(|c| &c.metadata);
    Words::new(
        lyrics,
        regions,
        meta.and_then(|m| m.title.clone()).unwrap_or_default(),
        meta.and_then(|m| m.artist.clone()),
    )
}

/// The lyrics panel.
#[component]
pub fn LyricsPanel() -> Element {
    let session: StudioSession = use_context();
    let words = use_hook(|| words_of(&session));
    let reading = crate::progress::use_reading();
    // Never shown yet: the window less its bars, which is where the
    // Lyrics view is.
    let start_size = try_consume_context::<crate::shell::WindowSize>().map(|window| {
        let (w, h) = *window.0.peek();
        (
            w,
            (h - crate::shell::BAR_H - crate::shell::BOTTOM_H).max(0.0),
        )
    });
    rsx! {
        LyricsView {
            words,
            at: reading().at,
            start_size,
            empty: "No lyrics for this song yet — put a synced .lrc beside its chart \
                    (session lyrics fetch) and prepare the song again.",
        }
    }
}
