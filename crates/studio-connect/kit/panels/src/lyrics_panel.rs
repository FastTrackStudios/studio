//! The lyrics panel: the song's words, following the song — Session's
//! window's reading of them.
//!
//! The panel itself (its Audience, Performer and Confidence Monitor views,
//! and the switch between them) is `studio_ui::components::lyrics`, which
//! the setlists extension's song workspace draws too. What is here is where
//! the window takes its words and its play position from: the lines are the
//! song's own, the Lyrics track's items, one a line, labelled with its text
//! ([`engine_domain::lyrics::stamp_lines`] puts them there from a synced `.lrc`
//! when a song is prepared), against the song's section regions; the
//! position is the transport's.

use dioxus::prelude::*;
use engine_domain::lyrics::{Lyrics, SectionSpan};
use studio_ui::components::lyrics::{ColoredSpan, LyricsView, Words};
pub use studio_ui::components::lyrics::{LyricsChoice, View};

use crate::song_model::StudioSession;

/// The song's lyrics as the panel shows them, read off the open session.
fn words_of(session: &StudioSession) -> Words {
    let project = &session.project;
    let lyrics = project
        .tracks
        .iter()
        .find(|t| engine_domain::lyrics::is_lyrics_track(&t.name))
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
        .filter(|r| r.lane == crate::ruler_layout::SECTIONS_ROW as u32)
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

/// What the open session holds for the panel: its timed lyric lines (the
/// Lyrics track's items), its section regions, and every region it has.
#[must_use]
pub fn summary(session: &StudioSession) -> (usize, usize, usize) {
    let project = &session.project;
    let lines = project
        .tracks
        .iter()
        .find(|t| engine_domain::lyrics::is_lyrics_track(&t.name))
        .map_or(0, |track| project.lane(&track.guid).len());
    let sections = project
        .sections
        .iter()
        .filter(|r| r.lane == crate::ruler_layout::SECTIONS_ROW as u32)
        .count();
    (lines, sections, project.sections.len())
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

/// The lyrics panel in a pane of another app (Studio's `studio.lyrics`
/// extension, an Architect pane): the same words and views, the play
/// position read on a timer, since the pane's window is not this crate's
/// (no window events to follow, no bars to leave room for). In a browser
/// (studio-web) the same, over the page's own engine.
#[cfg(any(feature = "native", feature = "web"))]
#[component]
pub fn HostedLyrics() -> Element {
    let session: StudioSession = use_context();
    let words = use_hook(|| words_of(&session));
    let mut at = use_signal(|| 0.0_f64);
    use_future(move || async move {
        loop {
            #[cfg(feature = "native")]
            futures_timer::Delay::new(std::time::Duration::from_millis(33)).await;
            #[cfg(not(feature = "native"))]
            gloo_timers::future::TimeoutFuture::new(33).await;
            let Some(now) =
                crate::engine::Transport::shared().map(crate::engine::Transport::reading)
            else {
                continue;
            };
            if (now.at - *at.peek()).abs() > 0.03 {
                at.set(now.at);
            }
        }
    });
    rsx! {
        LyricsView {
            words,
            at: at(),
            start_size: None,
            empty: "No lyrics for this song yet: put a synced .lrc beside its chart \
                    (session lyrics fetch) and prepare the song again.",
        }
    }
}
