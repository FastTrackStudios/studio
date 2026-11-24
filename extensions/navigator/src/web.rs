//! The navigator's panes in a browser.
//!
//! The same components over the set the page's own engine has open (`session_ui::web_host`), a song picked
//! there gone to as a pick in the page's own views is
//! (`web_host::hosted_pick`: the app's playhead moves, the engine with it).

use dioxus::prelude::*;
use session_engine_client::setlist::Setlist;
use session_ui::song_model::StudioSession;
use session_ui::web_host;

/// The set the page's engine has open, once it has one.
pub fn set() -> Option<Setlist> {
    web_host::hosted_is_open().then(web_host::hosted_songs)
}

/// A song's session as a prop (keyed by the pane, so never compared).
#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The song's progress bar, filling its pane.
#[component]
pub fn ProgressPane() -> Element {
    let song = studio_view_link::use_song();
    let surface = studio_view_link::surface();
    match song() {
        Some((key, session)) => rsx! {
            div { style: "position:relative; width:100%; height:100%; background:{surface}; \
                         padding:0 8px 6px; box-sizing:border-box;",
                for key in std::iter::once(key) {
                    Bar { key: "{key}", session: Song(session.clone()) }
                }
            }
        },
        None => studio_view_link::waiting(),
    }
}

#[component]
fn Bar(session: Song) -> Element {
    use_context_provider(|| session.0.clone());
    rsx! {
        session_ui::progress::ProgressBar { height: "44px".to_owned() }
    }
}

/// The set in context for the components under it (`Signal<Setlist>`, as
/// the window keeps it), following the page's engine.
fn use_set() -> Signal<Setlist> {
    let mut shown = use_context_provider(|| Signal::new(set().unwrap_or_default()));
    use_future(move || async move {
        let mut seen = web_host::hosted_revision();
        loop {
            gloo_timers::future::TimeoutFuture::new(150).await;
            let now = web_host::hosted_revision();
            if now == seen {
                continue;
            }
            seen = now;
            if let Some(next) = set()
                && *shown.peek() != next
            {
                shown.set(next);
            }
        }
    });
    shown
}

/// The set across a bar: a tab per song, the current one raised.
#[component]
pub fn SongsPane() -> Element {
    let _shown = use_set();
    let surface = studio_view_link::surface();
    rsx! {
        div { style: "position:relative; width:100%; height:100%; min-height:36px; background:{surface}; \
                     display:flex; align-items:stretch;",
            session_ui::shell::SongTabs {
                on_pick: move |index: usize| web_host::hosted_pick(index),
                max_shown: 0,
            }
        }
    }
}

/// The set's navigator, filling its pane: a song pressed gone to.
#[component]
pub fn NavigatorPane() -> Element {
    let shown = use_set();
    let surface = studio_view_link::surface();
    if shown.read().songs.is_empty() && shown.read().pending.is_empty() {
        return studio_view_link::waiting();
    }
    rsx! {
        div { style: "position:relative; width:100%; height:100%; background:{surface};",
            session_ui::navigator::Navigator {
                on_pick: move |index: usize| web_host::hosted_pick(index),
            }
        }
    }
}
