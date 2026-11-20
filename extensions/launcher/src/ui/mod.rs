//! The launcher's pages as Studio's phone app draws them
//! (docs/design/launcher/prototype): Home, Setlists, Records and Signal,
//! each a view the shell shows as a tab, and the pages pushed inside them
//! (a recurring event, a week's set, a show, a record, a track).
//!
//! Drawn with inline styles alone and the CSS Blitz draws (no `oklch`, no
//! `:has`, no sticky or fixed boxes), so the same tree is the browser's
//! (studio-web) and the desktop window's.

pub mod data;
mod home;
mod parts;
mod records;
mod setlists;
mod signal;

use std::sync::OnceLock;

use dioxus::prelude::*;

pub use home::HomeView;
pub use records::RecordsView;
pub use setlists::SetlistsView;
pub use signal::SignalView;

use data::{Day, Studio};

// ── The theme: true black, the shell's dark pill colours ──────────────────

pub const BLACK: &str = "#000";
pub const SURFACE: &str = "#111114";
pub const SURFACE2: &str = "#1f1f23";
pub const RULE: &str = "#1f1f24";
pub const EDGE: &str = "#2b2b31";
pub const INK: &str = "#f4f4f5";
pub const INK2: &str = "#a1a1aa";
pub const INK3: &str = "#8b8b94";
pub const LIVE: &str = "#f0c24b";
pub const OK: &str = "#22c55e";
pub const ACCENT: &str = "#2dd4bf";
pub const FONT: &str = "Instrument Sans, -apple-system, system-ui, sans-serif";

/// A colour round the wheel, at the design's saturation and lightness.
#[must_use]
pub fn hsl(h: u16, s: u8, l: u8) -> String {
    format!("hsl({h}, {s}%, {l}%)")
}

/// The page every view draws on: black, the design's type, scrolling.
pub const PAGE: &str = "flex: 1; min-height: 0; display: block; overflow-y: scroll; \
    background: #000; color: #f4f4f5; font-family: Instrument Sans, -apple-system, system-ui, sans-serif; \
    padding: 4px 20px 32px;";

// ── The images the crate carries ──────────────────────────────────────────

/// An image the launcher carries (a ministry's logo, a record's cover), by
/// its name, as a `data:` URL.
#[must_use]
pub fn art(name: &str) -> Option<&'static str> {
    use base64::Engine as _;
    static ART: OnceLock<Vec<(&'static str, String)>> = OnceLock::new();
    let all = ART.get_or_init(|| {
        let enc = |b: &[u8]| {
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(b)
            )
        };
        vec![
            ("jt", enc(include_bytes!("../../art/jt.jpg"))),
            (
                "crescendum",
                enc(include_bytes!("../../art/crescendum.jpg")),
            ),
            ("bviolent", enc(include_bytes!("../../art/bviolent.jpg"))),
            ("ghost", enc(include_bytes!("../../art/ghost.jpg"))),
            ("main", enc(include_bytes!("../../art/main.jpg"))),
            ("cya", enc(include_bytes!("../../art/cya.jpg"))),
            ("hsm", enc(include_bytes!("../../art/hsm.jpg"))),
            ("jhm", enc(include_bytes!("../../art/jhm.jpg"))),
        ]
    });
    all.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, url)| url.as_str())
}

/// An image to show: one the crate carries by name, else the source as it is
/// (a song note's `data:` cover).
#[must_use]
pub fn image(src: &str) -> String {
    art(src).map_or_else(|| src.to_owned(), str::to_owned)
}

// ── Pages pushed inside a tab ─────────────────────────────────────────────

/// A page pushed over a tab's own: what it shows, by its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Series(String),
    Week(String),
    Show(String),
    Every,
    Record(String),
    Track(String),
}

/// A tab's stack of pushed pages.
pub type Nav = Signal<Vec<Route>>;

/// The page on top of `nav`, or the tab's own (`root`).
#[must_use]
pub fn stacked(studio: &Studio, mut nav: Nav, root: Element) -> Element {
    let Some(top) = nav.read().last().cloned() else {
        return root;
    };
    let back = EventHandler::new(move |()| {
        nav.write().pop();
    });
    match top {
        Route::Series(id) => setlists::series_page(studio, &id, nav, back),
        Route::Week(id) => setlists::week_page(studio, &id, back),
        Route::Show(id) => setlists::show_page(studio, &id, back),
        Route::Every => setlists::every_page(studio, nav, back),
        Route::Record(id) => records::record_page(studio, &id, nav, back),
        Route::Track(id) => records::track_page(studio, &id, back),
    }
}

/// Pushes `route` on `nav`.
pub fn push(mut nav: Nav, route: Route) {
    nav.write().push(route);
}

// ── The model, read once and shared by the views ──────────────────────────

/// The view model, from the vault (re-read every few seconds) and the demo.
pub fn use_studio() -> Signal<Studio> {
    let mut studio = use_signal(|| data::demo(Day::today()));
    use_future(move || async move {
        loop {
            if let Ok(loaded) = crate::load().await {
                let lists: Vec<_> = loaded.setlists.iter().map(|f| f.setlist.clone()).collect();
                let now = Studio::of(&lists, Day::today());
                if *studio.peek() != now {
                    studio.set(now);
                }
            }
            futures_timer::Delay::new(std::time::Duration::from_secs(4)).await;
        }
    });
    studio
}

/// Opens a vault setlist (by its title) in Studio's engine: the old
/// launcher's Open set.
pub fn open_setlist(title: &str) {
    let args = serde_json::json!({ "setlist": title });
    dioxus::core::spawn_forever(async move {
        let _ = crate::host::run("STUDIO_LAUNCHER_OPEN_SETLIST", args).await;
    });
}

/// The launcher UI's revision, for the web app's rebuild.
pub const REVISION: u32 = 3;
