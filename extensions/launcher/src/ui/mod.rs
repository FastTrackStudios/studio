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

/// Every box of a launcher page sized with its padding and border, as a
/// browser sizes a button (Blitz sizes them content-box, and a card at
/// 100% plus its padding runs off the right edge).
pub const BOX: &str = ".studio-launcher, .studio-launcher * { box-sizing: border-box; }";

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

/// An image to show: one the crate carries by name, else a source a page
/// can load as it is (a `data:` cover, a URL). A vault file's path is not
/// one: `None`, and the cover is drawn instead.
#[must_use]
pub fn image(src: &str) -> Option<String> {
    if let Some(url) = art(src) {
        return Some(url.to_owned());
    }
    if ["data:", "http:", "https:", "blob:"]
        .iter()
        .any(|p| src.starts_with(p))
    {
        return Some(src.to_owned());
    }
    vault_image(src)
}

/// The vault's folder on this device, once the app says it (none in a
/// browser).
static ROOT: OnceLock<String> = OnceLock::new();

/// A vault file's image (`Libraries/…/cover.jpg`) as a `data:` URL, read
/// from the vault's folder on this device, once; none in a browser or
/// when it cannot be read.
fn vault_image(path: &str) -> Option<String> {
    use base64::Engine as _;
    use std::collections::HashMap;
    use std::sync::Mutex;
    static SEEN: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);
    let root = ROOT.get()?;
    let mut seen = SEEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let seen = seen.get_or_insert_with(HashMap::new);
    if let Some(url) = seen.get(path) {
        return url.clone();
    }
    let kind = match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => return None,
    };
    let url = std::fs::read(std::path::Path::new(root).join(path))
        .ok()
        .map(|b| {
            format!(
                "data:{kind};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(b)
            )
        });
    seen.insert(path.to_owned(), url.clone());
    url
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
        if ROOT.get().is_none()
            && let Some(root) = crate::host::vault_root().await
        {
            let _ = ROOT.set(root);
        }
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

/// Opens a vault setlist (its note's path, its title) in Studio's
/// engine: its songs' projects, what the arrangement and the mixer show.
/// In a browser the page's engine opens a set by its share link
/// (`DAW_OPEN {url}`, the set's files streamed from the link), so the
/// setlist's link is found and opened; natively the engine opens the
/// setlist's songs from the vault's folder.
pub fn open_setlist(note: &str, title: &str) {
    let (note, title) = (note.to_owned(), title.to_owned());
    dioxus::core::spawn_forever(async move {
        #[cfg(target_arch = "wasm32")]
        if let Some(code) = crate::host::share_of(&note).await {
            let origin = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_default();
            let url = format!("{origin}/s/{code}/");
            // `path`: the action's one declared argument, which the page's
            // engine reads as the share link it is.
            let _ = crate::host::run("DAW_OPEN", serde_json::json!({ "path": url })).await;
            return;
        }
        let _ = title;
        // By its note's path: a page's title ("Merry & Bright") need not be
        // the note's ("Merry & Bright - CBU Christmas 2026").
        let args = serde_json::json!({ "setlist": note });
        let _ = crate::host::run("STUDIO_LAUNCHER_OPEN_SETLIST", args).await;
    });
}

/// Opens a vault note (a setlist's) in the app: its page, with its tabs.
pub fn open_note(path: &str) {
    let path = path.to_owned();
    dioxus::core::spawn_forever(async move {
        let _ = crate::host::open(&path).await;
    });
}

/// The launcher UI's revision, for the web app's rebuild.
pub const REVISION: u32 = 6;
