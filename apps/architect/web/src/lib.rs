//! Studio's build of Architect's web app.
//!
//! The app is Architect's, whole ([`architect_web::run`]). What this
//! build adds is Studio's engine, in the page:
//!
//! - **The engine.** On a share link (`/s/<code>/…`) the set the link
//!   lists is opened into one engine (`session_engine::web_engine`,
//!   `WebSource::Link`): every song a project, its renditions streamed by
//!   range from the link's own file URLs, its reference where the link or
//!   the device stops short of the stems.
//! - **One clock.** The engine is given to the app as the playhead's
//!   clock ([`clock`]): for a song it has open it is loaded, played,
//!   sought and read, and the app's own player holds nothing. A song it
//!   has not opened (yet, or at all) plays on the app's player as before.
//! - **The views.** The Arrangement and Mixer tabs the island declares
//!   (`studio.transport`'s `arrangement` and `mixer`) are drawn here, in
//!   the page, by the engine's own panels ([`views`]).
//!
//! Nothing is written back: a link's routes are reads, the page joins no
//! session and keeps nothing, so a guest's mutes, solos and levels are
//! this page's engine's and end with it.
//!
//! A browser build only (as `session-daw-web`): natively the crate is
//! [`song_of`], which its tests cover.

#[cfg(target_arch = "wasm32")]
mod clock;
#[cfg(target_arch = "wasm32")]
mod views;

/// The extension whose tabs the engine's views are drawn behind: Studio's
/// island (`apps/architect/island/extension.toml`).
pub const ISLAND: &str = "studio.transport";

/// Which of `songs` (a song's folder in the vault, and what it is known
/// by) the item whose media is at vault path `media` belongs to: the
/// song whose folder holds it, the deepest when folders nest.
#[must_use]
pub fn song_of<'a>(songs: &'a [(String, String)], media: &str) -> Option<&'a str> {
    songs
        .iter()
        .filter(|(folder, _)| {
            folder.is_empty()
                || media
                    .strip_prefix(folder.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|(folder, _)| folder.len())
        .map(|(_, song)| song.as_str())
}

/// The page's start: the engine's clock and views given to the app, the
/// link's set opened behind it, then the app itself.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    use tracing_subscriber::layer::SubscriberExt as _;
    let filter = tracing_subscriber::filter::Targets::new()
        .with_default(tracing::Level::INFO)
        .with_target("loro_internal", tracing::Level::WARN)
        .with_target("loro", tracing::Level::WARN);
    let subscriber =
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_wasm::WASMLayer::new(
                tracing_wasm::WASMLayerConfigBuilder::new().build(),
            ));
    let _ = tracing::subscriber::set_global_default(subscriber);

    for (view, root) in [
        (
            "arrangement",
            views::arrangement as fn() -> dioxus::prelude::Element,
        ),
        ("mixer", views::mixer),
    ] {
        architect_web::provide_view(
            ISLAND,
            view,
            architect_web::Provided {
                root,
                when: views::has_engine,
            },
        );
    }
    clock::install();
    architect_web::run(architect_distribution::architect().named("Studio"));
}

#[cfg(test)]
mod tests {
    use super::song_of;

    fn songs() -> Vec<(String, String)> {
        vec![
            ("Songs/Hey".to_owned(), "hey".to_owned()),
            ("Songs/Heyday".to_owned(), "heyday".to_owned()),
            ("Songs/Hey/Live".to_owned(), "hey-live".to_owned()),
        ]
    }

    #[test]
    fn an_item_is_the_song_whose_folder_holds_its_media() {
        let songs = songs();
        assert_eq!(
            song_of(&songs, "Songs/Hey/Media/Proxies/Reference.ogg"),
            Some("hey")
        );
        // A folder that only begins the same is another song's.
        assert_eq!(song_of(&songs, "Songs/Heyday/Media/x.ogg"), Some("heyday"));
        // The deepest folder that holds it.
        assert_eq!(song_of(&songs, "Songs/Hey/Live/x.ogg"), Some("hey-live"));
        assert_eq!(song_of(&songs, "Recordings/else.mp3"), None);
        assert_eq!(song_of(&songs, "Songs/Hey"), None);
    }
}
