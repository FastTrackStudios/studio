//! The Session DAW view in a browser: the web demo's bundle.
//!
//! Built with cargo and wasm-bindgen (`just web-daw`), served as static
//! files: `index.html` and the module. The song comes from a Task share
//! link to its session folder — `?share=<link>` — opened the one way songs
//! open (its prepared `.session`, mirrored into memory) with its proxies
//! streaming in by range, what will be heard first first; `&guide=<link>`
//! is the guide sample library's, for the click, the count and the cues.
//! Without a link it asks for one (`session::open_link::OpenSet`): a paste
//! box for the link Task gave, and the way to the demo. It does not guess
//! at a song: the static server answers a missing file with the site's
//! `index.html`, so a guessed one opened as a page of HTML. See
//! `session_engine::web_engine`.

use dioxus::prelude::*;

/// The compiled Tailwind the session-ui panels (the progress bar, the
/// transport buttons) are styled with — the desktop app's sheet.
const TAILWIND: &str = include_str!("../../desktop/assets/tailwind-signal.css");

/// Start the page: its logging, then the app.
pub fn run() {
    {
        console_error_panic_hook::set_once();
        // Loro's own block diagnostics come at info, a few hundred lines a
        // sync: kept to warnings.
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
    }
    // Where this deployment's Task is (`/config.json`, studio-web) is known
    // before the address's link is read.
    app_shell::web_config::load_then(|| dioxus::launch(App));
}

/// A visitor's name, when the page is not told one: Guest and four
/// letters, so two guests are told apart.
fn guest_name() -> String {
    let n = (js_sys::Math::random() * 65_536.0) as u32;
    format!("Guest {n:04x}")
}

/// What the page opens — a set Task keeps, joined by its live link
/// (`?live=<link>&name=<who>`), or the song a share link shares
/// (`?share=<link>`) — and the guide library's link (`&guide=<link>`).
/// `None` when the address names neither: the page asks for a link.
fn source() -> (Option<session_engine::web_engine::WebSource>, Option<String>) {
    use session::open_link::{AppOpen, app_open};
    use session_engine::web_engine::WebSource;
    let query = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|q| web_sys::UrlSearchParams::new_with_str(&q).ok());
    // A link given as a path (`?live=/org/<org>/share/<code>`) is a link
    // on this deployment's Task; `name` and the rest are kept as given.
    let task_url = app_shell::web_config::current().task_url;
    let (open, guide) = app_open(|key| {
        query
            .as_ref()
            .and_then(|q| q.get(key))
            .map(|v| session::open_link::param_on_task(key, &v, &task_url))
    });
    let source = match open {
        AppOpen::Live { link, name } => Some(WebSource::Live {
            link,
            name: name.unwrap_or_else(guest_name),
        }),
        AppOpen::Shared { link } => Some(WebSource::Shared { link }),
        AppOpen::Nothing => None,
    };
    (source, guide)
}

#[component]
fn App() -> Element {
    let (source, guide) = use_hook(source);
    rsx! {
        style { {TAILWIND} }
        if let Some(source) = source {
            session_ui::web_host::WebDemo { source, guide }
        } else {
            session::open_link::OpenSet { task_url: app_shell::web_config::current().task_url }
        }
    }
}
