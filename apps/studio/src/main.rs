//! Studio, remade from the launcher prototype: one Dioxus app for the
//! phone first, and later the desktop and the web. The content is made up
//! and in memory; on the web its sets are live on Drive (`live_sync`), and
//! its library and songs are Drive documents, each song a session
//! (`songs`, on `session_sync`); each org has its own library, team and
//! settings, and shares songs with others (`orgs`, `team`); its sets are
//! Drive documents too (`setlist` the model, `sets` live).

mod data;
mod files;
mod files_live;
mod icons;
mod keys;
mod live_sync;
mod orgs;
mod pages;
mod pieces;
mod set_sheets;
mod setlist;
mod sets;
mod sheets;
mod shell;
mod songs;
mod state;
mod team;

use dioxus::prelude::*;

use pages::home::Home;
use pages::library::{Library, LibraryHome};
use pages::live::{Chart, Lyrics};
use pages::orgs::{AllOrgs, OrgSettings, People};
use pages::print::PrintSet;
use pages::records::{Album, Records, TrackPage};
use pages::set::{OrgSet, SetPage};
use pages::setlists::{Every, Series, Setlists};
use pages::signal::Signal;
use pages::song::{SharedSong, SongPage};
use shell::Shell;

const STYLE: Asset = asset!("/assets/style.css");
const APP_STYLE: Asset = asset!("/assets/app.css");
const FONT: &str = "https://fonts.googleapis.com/css2?family=Instrument+Sans:wdth,wght@75..100,400..700&display=swap";

/// Every page, as its place in the app. The four tabs are the roots; the
/// rest are pushed over them.
#[derive(Clone, Debug, PartialEq, Routable)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/")]
        Home {},
        #[route("/records")]
        Records {},
        #[route("/records/:id")]
        Album { id: String },
        #[route("/track/:idx")]
        TrackPage { idx: usize },
        #[route("/setlists")]
        Setlists {},
        #[route("/setlists/every")]
        Every {},
        #[route("/series/:id")]
        Series { id: String },
        #[route("/set/:id")]
        SetPage { id: String },
        #[route("/set/:id/chart")]
        Chart { id: String },
        #[route("/set/:id/lyrics")]
        Lyrics { id: String },
        #[route("/at/:owner/:id")]
        OrgSet { owner: String, id: String },
        #[route("/print/:owner/:id")]
        PrintSet { owner: String, id: String },
        #[route("/library")]
        LibraryHome {},
        #[route("/library/:tag")]
        Library { tag: String },
        #[route("/song/:id")]
        SongPage { id: String },
        #[route("/shared/:owner/:id")]
        SharedSong { owner: String, id: String },
        #[route("/signal")]
        Signal {},
        #[route("/orgs")]
        AllOrgs {},
        #[route("/people")]
        People {},
        #[route("/org")]
        OrgSettings {},
}

/// The four tabs at the foot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Records,
    Setlists,
    Signal,
}

impl Route {
    /// The tab a page belongs to.
    pub fn tab(&self) -> Tab {
        match self {
            Route::Home {} => Tab::Home,
            Route::Records {} | Route::Album { .. } | Route::TrackPage { .. } => Tab::Records,
            Route::Signal {} => Tab::Signal,
            Route::AllOrgs {} | Route::People {} | Route::OrgSettings {} => Tab::Home,
            _ => Tab::Setlists,
        }
    }

    /// The set a page shows, if it is one of a set's views.
    pub fn set_id(&self) -> Option<&str> {
        match self {
            Route::SetPage { id }
            | Route::Chart { id }
            | Route::Lyrics { id }
            | Route::OrgSet { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Where to open a set or a show, by its id.
    pub fn set(id: &str) -> Route {
        Route::SetPage { id: id.to_string() }
    }

    /// Where an id from the prototype's data opens: a record, a track, a
    /// recurring event, a set (a built-in show in its own org).
    pub fn open(id: &str) -> Route {
        if data::album(id).is_some() {
            Route::Album { id: id.into() }
        } else if let Some(t) = id.strip_prefix("t:").and_then(data::track) {
            Route::TrackPage { idx: t.idx }
        } else if data::series(id).is_some() {
            Route::Series { id: id.into() }
        } else if let Some(owner) = setlist::builtin_home(id) {
            Route::OrgSet {
                owner: owner.into(),
                id: id.into(),
            }
        } else {
            Route::set(id)
        }
    }
}

fn main() {
    // Warnings and up: Loro's encoder logs every block at info.
    dioxus::logger::init(dioxus::logger::tracing::Level::WARN).ok();
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(state::AppState::new);
    rsx! {
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        document::Meta { name: "theme-color", content: "#000000" }
        document::Title { "Studio" }
        document::Link { rel: "preconnect", href: "https://fonts.googleapis.com" }
        document::Stylesheet { href: FONT }
        document::Stylesheet { href: STYLE }
        document::Stylesheet { href: APP_STYLE }
        live_sync::Provider { Router::<Route> {} }
    }
}
