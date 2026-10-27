//! The loading screen: what a window or a page shows while a set opens —
//! Session's mark, where the open has got to, and a sweep while it works.
//! One screen for the app (its start screen, `apps/desktop`) and the page
//! (`web_host`), which both open sets the same ways.

use dioxus::prelude::*;

pub use crate::open_progress::Progress;

/// Session's mark (the site's favicon), as an image source that needs no
/// server: a page and a window draw it the same.
#[must_use]
pub fn mark_src() -> &'static str {
    static SRC: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SRC.get_or_init(|| {
        const SVG: &str = include_str!("../../web/public/favicon.svg");
        let mut uri = String::from("data:image/svg+xml;utf8,");
        for c in SVG.chars() {
            match c {
                '%' => uri.push_str("%25"),
                '#' => uri.push_str("%23"),
                '"' => uri.push('\''),
                '<' => uri.push_str("%3C"),
                '>' => uri.push_str("%3E"),
                '\n' | '\r' => uri.push(' '),
                c => uri.push(c),
            }
        }
        uri
    })
}

/// The loading screen for `progress`.
#[component]
pub fn Loading(progress: Progress) -> Element {
    let (headline, detail, retry) = match progress {
        Progress::Joining { retry } => (
            "Joining the live session".to_owned(),
            "Connecting…".to_owned(),
            retry,
        ),
        Progress::Fetching { title, retry } => (
            format!("Opening {title}"),
            "Bringing the song in…".to_owned(),
            retry,
        ),
        Progress::Opening { title } => (
            format!("Opening {title}"),
            "Laying out the session…".to_owned(),
            None,
        ),
    };
    let detail = retry.map_or(detail, |r| readable_failure(&r));
    rsx! {
        LoadingFrame { headline, detail, failed: false, Sweep {} }
    }
}

/// A thin sweep: working, without claiming how far.
#[component]
pub fn Sweep() -> Element {
    rsx! {
        style { "@keyframes fts-sweep {{ 0% {{ left: -40% }} 100% {{ left: 100% }} }}" }
        div {
            style: "position:relative; width:220px; height:3px; border-radius:2px; overflow:hidden; \
                    background:#1f2228; margin-top:6px;",
            div {
                style: "position:absolute; top:0; left:0; width:40%; height:100%; border-radius:2px; \
                        background:#3aa0ff; animation:fts-sweep 1.4s ease-in-out infinite;",
            }
        }
    }
}

/// The frame every loading state shares: Session's mark, a headline, a
/// line of detail, and whatever goes under it.
#[component]
pub fn LoadingFrame(headline: String, detail: String, failed: bool, children: Element) -> Element {
    let detail_color = if failed { "#f87171" } else { "#9aa0a6" };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100vw; height:100vh; display:flex; \
                    flex-direction:column; align-items:center; justify-content:center; gap:10px; \
                    background:#0f1012; color:#e5e7eb; font-family:system-ui, sans-serif; \
                    text-align:center; padding:0 24px; box-sizing:border-box;",
            img { src: mark_src(), width: "64", height: "64", style: "border-radius:15px; margin-bottom:8px;" }
            div { style: "font-size:18px; font-weight:650;", "{headline}" }
            div { style: "font-size:13px; color:{detail_color}; max-width:420px; line-height:1.5;", "{detail}" }
            {children}
        }
    }
}

/// A failure as the loading screen may say it: its first line, cut short.
/// Never a response body: a static server answers a missing file with the
/// site's `index.html`, and a page of HTML is not a reason anyone can read.
#[must_use]
pub fn readable_failure(detail: &str) -> String {
    const LONGEST: usize = 240;
    let lower = detail.to_ascii_lowercase();
    let markup = [
        "<!doctype",
        "<html",
        "<head",
        "<body",
        "<script",
        "<div",
        "<meta",
        "<title",
    ]
    .iter()
    .any(|tag| lower.contains(tag));
    if markup {
        return "The server sent back a web page instead of the session's files. \
                Check the link, or ask whoever shared it for a new one."
            .to_owned();
    }
    let line = detail
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        return "Something went wrong opening it.".to_owned();
    }
    if line.chars().count() > LONGEST {
        let cut: String = line.chars().take(LONGEST).collect();
        return format!("{}…", cut.trim_end());
    }
    line.to_owned()
}

/// Why a canvas cannot be drawn: the browser gave it no WebGL2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoWebGl2;

impl std::fmt::Display for NoWebGl2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "this browser gave the page no WebGL2 (hardware acceleration is off or unsupported)",
        )
    }
}

impl std::error::Error for NoWebGl2 {}

/// A renderer over a canvas's WebGL2 context: `context` asks the browser
/// for it (`None` when there is none), `make` builds the renderer once it
/// is there. The renderer itself would panic on a missing context
/// (`vello_hybrid::WebGlRenderer::new` expects it), so it is only made
/// once the context is known to exist.
///
/// # Errors
///
/// [`NoWebGl2`]: the browser gave no WebGL2 context.
pub fn init_renderer<C, R>(
    context: impl FnOnce() -> Option<C>,
    make: impl FnOnce(C) -> R,
) -> Result<R, NoWebGl2> {
    context().map(make).ok_or(NoWebGl2)
}

/// What the page shows where it cannot draw: the views are drawn on the
/// GPU, and this browser has none to give.
#[component]
pub fn HardwareNeeded() -> Element {
    rsx! {
        LoadingFrame {
            headline: "Hardware acceleration needed".to_owned(),
            detail: "Session draws its views with WebGL2, and this browser isn't offering it. \
                     Turn on hardware acceleration in the browser's settings (or try another \
                     browser), then reload the page."
                .to_owned(),
            failed: true,
        }
    }
}
