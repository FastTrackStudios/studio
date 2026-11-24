//! `studio.lyrics`: Studio's lyrics panel in an Architect pane.
//!
//! Nothing is redrawn: the pane is a Studio window on Studio's engine
//! (`studio_view_link`) showing `daw_view_kit::lyrics_panel::HostedLyrics`,
//! the panel `studio-desktop`'s Lyrics view shows (its Audience, Performer
//! and Confidence Monitor views), following the engine's transport.
//!
//! **In a browser** (studio-web) the same panel follows the page's own
//! engine: `studio_view_link` reads its song from there.

use architect_extensions::linked::{Builder, Extension};
use architect_extensions_dioxus::BuilderExt as _;
use daw_view_kit::song_model::StudioSession;
use dioxus::prelude::*;
use serde_json::json;

pub const MANIFEST: &str = include_str!("../extension.toml");

pub struct Lyrics;

impl Extension for Lyrics {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        ext.view("lyrics", LyricsPane)
            .action("STUDIO_LYRICS_STATE", |_| async {
                let song = studio_view_link::current();
                let held = song
                    .as_ref()
                    .map(|(_, session)| daw_view_kit::lyrics_panel::summary(session));
                Ok(json!({
                    "linked": song.is_some(),
                    "problem": studio_view_link::problem(),
                    "lines": held.map(|h| h.0),
                    "sections": held.map(|h| h.1),
                    "regions": held.map(|h| h.2),
                    "song": song.map(|(key, _)| key),
                }))
            });
    }
}

/// The pane: the panel once the engine has a song, what it waits for
/// until then.
#[component]
fn LyricsPane() -> Element {
    let song = studio_view_link::use_song();
    let surface = studio_view_link::surface();
    match song() {
        Some((key, session)) => rsx! {
            div { style: "position:relative; width:100%; height:100%; background:{surface};",
                // A list of one, so its key is diffed: another song mounts
                // the panel afresh (a lone keyed child is patched in place).
                for key in std::iter::once(key) {
                    Words { key: "{key}", session: Song(session.clone()) }
                }
            }
        },
        None => studio_view_link::waiting(),
    }
}

/// A song's session as a prop (keyed by the pane, so never compared).
#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[component]
fn Words(session: Song) -> Element {
    use_context_provider(|| session.0.clone());
    rsx! { daw_view_kit::lyrics_panel::HostedLyrics {} }
}
