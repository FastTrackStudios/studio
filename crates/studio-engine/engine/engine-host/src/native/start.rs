//! The start screen: what the window shows when nothing is open yet — on a
//! phone, which has no Open dialog and no song picked before launch; on a
//! desktop, when nothing was chosen or it did not open.
//!
//! The ways in:
//!
//! - **On this device** — a song or setlist in the app's documents folder
//!   (on a phone, what the Files app shows under Session), or, on a
//!   desktop, from the Open dialog; the songs opened lately; a new song.
//! - **Connect** — a REAPER's Session bridge, or a Session engine, by its
//!   address. A browser's way into a set is a room's link (studio#711).
//!
//! Opening runs in the engine (`studio engine`, or the engine in this
//! process on iOS), asked over the window's link
//! (`studio_engine::view_client::ask`): the set is the engine's, and the
//! window shows whatever set the engine has — read back over its link —
//! or its start screen when it has none ([`App`]). An open asked for by the
//! CLI or a model shows here the same way as one asked for by a click.

use std::path::PathBuf;

use dioxus::prelude::*;
use lucide_dioxus::{
    AudioLines, Check, ChevronRight, CircleAlert, CircleCheck, FileMusic, HardDriveDownload, House,
    Link, ListMusic, Play, Plus, Radio,
};
use studio_engine::setlist::{Setlist, SetlistOpen as _};
use studio_panels::loading::{Loading, Progress, mark_src};

pub(super) const BG: &str = "#0f1012";
pub(super) const BAR: &str = "#17181b";
pub(super) const RULE: &str = "#2a2c31";
pub(super) const TEXT: &str = "#e5e7eb";
pub(super) const DIM: &str = "#8b9099";
pub(super) const ACCENT: &str = "#3aa0ff";
pub(super) const WARN: &str = "#e3b341";
/// Room at the top: on macOS the window's traffic lights sit over the
/// page (its title bar is transparent, as the shell's).
pub(super) const TOP: u32 = if cfg!(target_os = "macos") { 44 } else { 28 };

/// The window: the set once one is open, the start screen until then.
#[component]
pub fn App() -> Element {
    // iOS: winit's window is handed to the app's scene (see `ios_scene`) —
    // here, at the root, whichever the window opens on.
    #[cfg(target_os = "ios")]
    {
        let window = dioxus_native::use_window();
        use_hook(move || {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::UiKit(uikit) = handle.as_raw()
            {
                studio_shell::ios_scene::window_created(uikit.ui_view);
            }
            // An audio app, as iOS knows one: its session (the loudspeaker,
            // Bluetooth, AirPlay) set before the engine opens the device,
            // and the lock screen's commands listened for.
            super::ios_audio::configure_session();
            super::ios_audio::install_commands();
            // No UIKit pinch recognizer: the arrangement reads both
            // fingers itself (a pinch zooms time across and the rows
            // down, and carries the view), and a recognizer that took the
            // gesture would cancel the touches it is made of.
        });
    }
    // A set the launch attached to (Remote: the projects the driven system
    // has open) is the engine's set from the start.
    let initial: Option<Setlist> = use_context();
    use_hook(move || {
        if let Some(setlist) = initial {
            studio_engine::view_client::show_set(setlist);
        }
    });
    let engine = use_engine_set();
    let host = try_use_context::<super::handoff::Host>();
    // Back to the start screen: the set stops, and the page is the one
    // that picks another (its library, a link, a file). Inside Studio
    // (`Host`) Back with nothing else to open is the welcome screen's; an
    // open asked for from inside the song (another version, a set picked
    // from the palette) still comes back out through the start screen,
    // which opens it behind its loading screen.
    use_context_provider(|| {
        studio_panels::shell::Back(Callback::new(move |()| {
            #[cfg(target_os = "ios")]
            super::ios_audio::clear();
            studio_engine::view_client::ask::close();
            match host {
                Some(host) if !super::handoff::pending() => host.on_leave.call(()),
                _ => {}
            }
        }))
    });
    // The engine's set, while no other is opening; the start screen (and,
    // while a set opens, its loading screen) otherwise. A new set is a new
    // shell: its views are made again for it.
    let shown = engine.read();
    let page = match (&shown.opening, shown.setlist.clone()) {
        (studio_engine::studio_engine::set::Opening::Busy(_), _) | (_, None) => {
            rsx! { Start {} }
        }
        (_, Some(setlist)) => {
            let epoch = shown.epoch;
            rsx! {
                WithSetlist { key: "{epoch}", setlist, super::shell::Shell {} }
            }
        }
    };
    drop(shown);
    rsx! {
        document::Style { {ROOT_CSS} }
        {page}
        EngineNotice {}
    }
}

/// What the window says about its engine — it stopped and was started
/// again (`studio_engine::studio_engine::link::take_notice`) — across the
/// foot of the window for a few seconds.
#[component]
fn EngineNotice() -> Element {
    let mut said = use_signal(|| None::<String>);
    use_future(move || async move {
        let mut shown_for = 0_u32;
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(500)).await;
            #[cfg(not(target_os = "ios"))]
            if let Some(text) = studio_engine::studio_engine::link::take_notice() {
                said.set(Some(text));
                shown_for = 0;
                continue;
            }
            if said.peek().is_some() {
                shown_for = shown_for.saturating_add(1);
                if shown_for >= 12 {
                    said.set(None);
                }
            }
        }
    });
    let Some(text) = said() else {
        return rsx! {};
    };
    rsx! {
        div {
            style: "position:absolute; left:0; right:0; bottom:56px; display:flex; justify-content:center;",
            div {
                style: "padding:8px 14px; border-radius:8px; background:#2a2310; border:1px solid {WARN}; \
                        color:{TEXT}; font-size:12px;",
                "{text}"
            }
        }
    }
}

/// The page itself, edge to edge. Blitz's default stylesheet gives `body`
/// an 8px margin, and Blitz places an absolutely positioned box against its
/// parent — the body — so without this every view sat 8px right and down
/// and ran off the right edge. And the canvas takes the page's background,
/// which is what fills a phone's status bar and home indicator: the bars'
/// colour (`studio_panels::shell::BAR_BG`), so the top bar runs up under the
/// status bar and the bottom bar down under the home indicator, into the
/// screen's rounded corners, rather than stopping short of a darker strip.
const ROOT_CSS: &str = "html, body { margin: 0; padding: 0; background: #17181b; }";

/// What the launch chose to open as the window opens (see
/// `super::choose`), if anything.
#[derive(Clone)]
pub struct OpenFirst(pub Option<PathBuf>);

/// Whether the launch's choice ([`OpenFirst`]) has been opened: it is the
/// first start screen's to open, not every one after a Back.
static LAUNCH_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The set, as the shell reads it.
#[component]
fn WithSetlist(setlist: Setlist, children: Element) -> Element {
    use_context_provider(|| setlist);
    // `FTS_SESSION_AUTOPLAY=1` starts playing as the set opens — for
    // measuring a session while it plays without a hand on the mouse.
    use_hook(|| {
        if std::env::var("FTS_SESSION_AUTOPLAY").is_ok_and(|v| v != "0") {
            studio_engine::engine::transport(studio_engine::engine::Move::PlayStop, 0.0);
        }
    });
    children
}

/// The engine's set, as it changes: every window shows the one set the
/// engine has, read back over its link (`studio_engine::view_client`), whoever
/// changed it.
pub(super) fn use_engine_set() -> Signal<studio_engine::studio_engine::set::SetView> {
    let mut shown = use_signal(studio_engine::view_client::set_view);
    use_future(move || async move {
        let mut changes = studio_engine::view_client::subscribe_set();
        loop {
            let now = changes.borrow_and_update().clone();
            if now.revision != shown.peek().revision {
                shown.set(now);
            }
            if changes.changed().await.is_err() {
                return;
            }
        }
    });
    shown
}

/// Where opening has got to.
#[derive(Clone, PartialEq)]
enum Opening {
    Idle,
    /// Opening: the loading screen shows where it has got to.
    Busy(Progress),
    Failed(String),
}

/// `work` on a thread of its own; its answer, awaited.
pub(super) async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (done, answer) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = done.send(work());
    });
    answer.await.ok()
}

/// A way of opening a set, run in the engine on a thread of its own: it
/// says each step to its `progress` — the loading screen's.
use studio_engine::studio_engine::set::OpenWork;

/// An error for a person: the whole chain, in a few words.
pub(super) fn brief(e: &eyre::Report) -> String {
    studio_engine::studio_engine::set::brief(&format!("{e:#}"))
}

/// Where the start screen is: its sections, down a sidebar (a tab bar
/// along the foot on a phone).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Home,
    OnDevice,
}

impl Section {
    const ALL: [Self; 2] = [Self::Home, Self::OnDevice];

    const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::OnDevice => "On this device",
        }
    }

    /// The word under its icon in a phone's tab bar.
    const fn short(self) -> &'static str {
        match self {
            Self::OnDevice => "Device",
            other => other.label(),
        }
    }
}

/// The window's width in logical pixels, as it is resized.
fn use_width() -> Signal<f64> {
    let window = dioxus_native::use_window();
    let logical = |w: &std::sync::Arc<dyn winit::window::Window>| {
        f64::from(w.surface_size().width) / w.scale_factor().max(1.0)
    };
    let mut width = use_signal(|| logical(&window));
    dioxus_native::use_window_event(move |event, _| {
        if matches!(
            event,
            winit::event::WindowEvent::SurfaceResized(_)
                | winit::event::WindowEvent::ScaleFactorChanged { .. }
        ) {
            let now = logical(&window);
            if (now - *width.peek()).abs() > 0.5 {
                width.set(now);
            }
        }
    });
    width
}

#[component]
fn Start() -> Element {
    let keys = studio_shell::ui::use_text_focus();
    let mut opening = use_signal(|| Opening::Idle);
    let mut local = use_signal(documents);
    // The songs opened lately, each with its alternatives — read again
    // each time the start screen is shown.
    let recent = use_hook(studio_engine::projects::recent);
    let mut section = use_signal(|| Section::Home);
    let width = use_width();

    // An open is the engine's (`studio_engine::set`), whoever asked for it:
    // its loading screen here says where it has got to, and how it failed
    // if it did — a failure from before this screen was shown is not news.
    let engine = use_engine_set();
    let shown_from = use_hook(|| engine.peek().revision);
    use_effect(move || {
        use studio_engine::studio_engine::set::Opening as Engine;
        let now = engine.read();
        match &now.opening {
            Engine::Busy(step) => opening.set(Opening::Busy(step.clone())),
            Engine::Failed(why) if now.revision > shown_from => {
                opening.set(Opening::Failed(why.clone()));
            }
            Engine::Idle if matches!(*opening.peek(), Opening::Busy(_)) => {
                opening.set(Opening::Idle);
            }
            Engine::Failed(_) | Engine::Idle => {}
        }
    });

    // A set this window drives on another system (a REAPER's bridge):
    // attached on a thread of its own, shown when it is read; the loading
    // screen meanwhile.
    let mut open_set = move |first: Progress, work: OpenWork| {
        opening.set(Opening::Busy(first));
        spawn(async move {
            let read = off_thread(move || work(&|_| ())).await;
            match read {
                Some(Ok(setlist)) => {
                    opening.set(Opening::Idle);
                    studio_engine::view_client::show_set(setlist);
                }
                Some(Err(e)) => opening.set(Opening::Failed(brief(&e))),
                None => opening.set(Opening::Failed("the attach stopped".to_owned())),
            }
        });
    };
    // What the engine is asked to open; the loading screen follows its
    // opening (`use_engine_set`), a refusal is said at once.
    let mut asked = move |first: Progress, refused: Result<String, String>| match refused {
        Ok(_) => opening.set(Opening::Busy(first)),
        Err(why) => opening.set(Opening::Failed(why)),
    };
    let mut open_path = move |path: PathBuf| {
        let title = path.file_stem().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let opened = studio_engine::view_client::ask::open_path(&path);
        if opened.is_ok() {
            super::remember(&path);
        }
        asked(Progress::Opening { title }, opened);
    };
    // A song from nothing (`SESSION_NEW`): untitled, in Organize mode, its
    // count-in, start and guide placed.
    let mut new_project = move || {
        let first = Progress::Opening {
            title: studio_engine::project_new::UNTITLED.to_owned(),
        };
        opening.set(Opening::Busy(first.clone()));
        spawn(async move {
            let refused = off_thread(studio_engine::view_client::ask::new_song)
                .await
                .unwrap_or_else(|| Err("the ask stopped".to_owned()));
            asked(first, refused);
        });
    };

    // What the launch chose, opened as the window opens.
    let OpenFirst(first) = use_context();
    use_hook(move || {
        // A version picked from inside a song, which came back out here to
        // open it (`studio_panels::versions`, `SESSION_OPEN`) — or, the
        // first time only, what the launch chose. Every time, that was
        // reopened whenever the start screen came back: Back from a song
        // the app was launched with went straight back into it.
        let launch =
            first.filter(|_| !LAUNCH_OPENED.swap(true, std::sync::atomic::Ordering::AcqRel));
        if let Some(path) = studio_engine::projects::take_next().or(launch) {
            open_path(path);
        } else if super::handoff::take_new() {
            // Studio's welcome screen asked for a new song.
            new_project();
        }
    });

    let mut link = use_signal(String::new);
    // Drive a REAPER through its Session bridge, at `address`: Remote mode,
    // the set its open projects are — as `FTS_AUDIO_TARGET` does at launch.
    let mut connect_bridge = move |address: String| {
        remember_bridge(&address);
        open_set(
            Progress::Joining { retry: None },
            Box::new(move |_| {
                use studio_engine::open::{ModeState, RemoteTarget};
                let target = RemoteTarget::Session { address };
                studio_engine::open::set_mode(ModeState {
                    requested: studio_engine::open::AudioMode::Remote,
                    target: Some(target.clone()),
                    ..ModeState::engine()
                });
                Setlist::attach(&target)
            }),
        );
    };
    let bridge = use_hook(remembered_bridge);
    // The set opened last, while it is still here.
    let last = use_hook(|| super::remembered().filter(|p| p.exists()));
    // What was typed or pasted: a REAPER's Session bridge (or a Session
    // engine), driven from here. A browser's way into a set is a room's
    // link (studio#711), not this.
    let connect = move |text: String| match engine_address(&text) {
        Some(address) => connect_bridge(address),
        None => opening.set(Opening::Failed(
            "that is not a REAPER bridge's or an engine's address".to_owned(),
        )),
    };
    // Studio's home (`handoff::join_on_start`) connects as the window opens.
    use_hook({
        let mut connect = connect;
        move || {
            if let Some(address) = super::handoff::take_join().filter(|l| !l.trim().is_empty()) {
                connect(address);
            }
        }
    });
    let mut connect_link = connect;
    let mut connect_pasted = connect;

    // Opening: the loading screen, whole.
    if let Opening::Busy(progress) = opening() {
        return rsx! { Loading { progress } };
    }
    // Inside Studio the welcome screen is the start page: this one only
    // shows what was handed to it opening (in the engine), and if it fails,
    // why, with the way back.
    if let Some(host) = try_consume_context::<super::handoff::Host>() {
        let (headline, detail, failed) = match opening() {
            Opening::Failed(why) => ("Could not open it".to_owned(), why, true),
            _ => ("Opening".to_owned(), String::new(), false),
        };
        return rsx! {
            studio_panels::loading::LoadingFrame { headline, detail, failed,
                button {
                    style: "margin-top:14px; height:44px; padding:0 20px; border:1px solid {RULE}; border-radius:10px; \
                            background:transparent; color:{TEXT}; font-size:15px; font-weight:600; font-family:inherit; cursor:pointer;",
                    onclick: move |_| host.on_leave.call(()),
                    "Back to the library"
                }
            }
        };
    }
    let wide = width() >= 760.0;
    let here = section();

    // What the section shows.
    let content = match here {
        Section::Home => rsx! {
            if let Opening::Failed(why) = opening() {
                Banner { good: false, text: format!("Could not open it — {why}") }
            }
            // The way in: back to the last set.
            if let Some(path) = last.clone() {
                div {
                    style: "display:flex; flex-wrap:wrap; gap:14px;",
                    Hero {
                        kicker: "Jump back in",
                        title: title_of(&path),
                        detail: kind_of(&path),
                        seed: title_of(&path),
                        on_press: move |()| open_path(path.clone()),
                        FileMusic { size: 28, color: ART_INK }
                    }
                }
            }
            // A song from nothing.
            Shortcut {
                title: "New Project".to_owned(),
                detail: "An untitled song, in Organize, the click ready".to_owned(),
                on_press: move |()| new_project(),
                Plus { size: 18, color: ACCENT }
            }
            // Connecting by address: a REAPER's bridge, or an engine.
            div {
                style: "display:flex; gap:8px; align-items:center; height:50px; padding:0 6px 0 14px; \
                        border-radius:14px; background:{BAR};",
                Link { size: 18, color: DIM }
                // The hint under the field while it is empty: Blitz
                // draws no `placeholder`.
                div {
                    style: "position:relative; flex:1; min-width:0; height:40px;",
                    if link().is_empty() {
                        span {
                            style: "position:absolute; top:0; left:4px; height:40px; display:flex; \
                                    align-items:center; font-size:15px; color:#6b7280; pointer-events:none; \
                                    white-space:nowrap; overflow:hidden;",
                            "Connect to REAPER or an engine"
                        }
                    }
                    input {
                        onfocus: move |_| keys.focus(),
                        onblur: move |_| keys.blur(),
                        style: "position:absolute; top:0; left:0; width:100%; height:40px; box-sizing:border-box; \
                                padding:0 4px; border:none; background:transparent; color:{TEXT}; \
                                font-family:inherit; font-size:15px;",
                        r#type: "text",
                        value: "{link}",
                        oninput: move |e| link.set(e.value()),
                    }
                }
                if cfg!(target_os = "ios") && link().trim().is_empty() {
                    Pill {
                        label: "Paste",
                        primary: false,
                        on_press: move |()| {
                            if let Some(text) = super::pasted() {
                                link.set(text.clone());
                                connect_pasted(text);
                            }
                        },
                    }
                } else {
                    Pill {
                        label: "Connect",
                        primary: !link().trim().is_empty(),
                        on_press: move |()| {
                            if !link().trim().is_empty() {
                                connect_link(link());
                            }
                        },
                    }
                }
            }
            // The REAPER this device drove last, through its bridge.
            if let Some(address) = bridge.clone() {
                Shortcut {
                    title: "Reconnect to REAPER".to_owned(),
                    detail: address.clone(),
                    on_press: move |()| connect_bridge(address.clone()),
                    Radio { size: 18, color: ACCENT }
                }
            }
            // The songs worked on lately: the song, or any of its
            // checkpoints.
            if !recent.is_empty() {
                Shelf { title: "Recent",
                    Rows {
                        for song in recent.iter().take(8).cloned() {
                            RecentRow {
                                key: "{song.base.display()}",
                                song: song.clone(),
                                on_open: move |path: PathBuf| open_path(path),
                            }
                        }
                    }
                }
            }
            // What opens with no connection.
            if !local().is_empty() {
                Shelf { title: "On this device", on_more: move |()| section.set(Section::OnDevice),
                    Covers {
                        for path in local().into_iter().take(8) {
                            Cover {
                                key: "{path.display()}",
                                title: title_of(&path),
                                detail: kind_of(&path),
                                seed: title_of(&path),
                                saved: true,
                                on_press: {
                                    let path = path.clone();
                                    move |()| open_path(path.clone())
                                },
                                if kind_of(&path) == "Setlist" {
                                    ListMusic { size: 34, color: ART_INK }
                                } else {
                                    AudioLines { size: 34, color: ART_INK }
                                }
                            }
                        }
                    }
                }
            }
        },
        Section::OnDevice => rsx! {
            Quiet { text: LOCAL_HINT }
            Rows {
                if local().is_empty() {
                    Quiet { text: LOCAL_EMPTY }
                }
                for path in local() {
                    LocalRow {
                        key: "{path.display()}",
                        title: title_of(&path),
                        detail: kind_of(&path),
                        on_open: {
                            let path = path.clone();
                            move |()| open_path(path.clone())
                        },
                    }
                }
                if cfg!(not(target_os = "ios")) {
                    LocalRow {
                        title: "Open a song or setlist…".to_owned(),
                        detail: "A REAPER project, a .session, or a .setlist".to_owned(),
                        on_open: move |()| {
                            if let Some(path) = super::pick() {
                                open_path(path);
                            }
                        },
                    }
                }
            }
        },
    };
    let head_top = if wide { TOP } else { TOP.max(12) };

    // The main column, beside the sidebar or over the tab bar.
    let main = rsx! {
        div {
            style: "position:relative; flex:1; min-width:0; min-height:0; display:flex; flex-direction:column;",
            div {
                style: "position:relative; z-index:5; flex:none; display:flex; align-items:center; gap:12px; \
                        padding:{head_top}px 24px 12px; background:{BG};",
                if !wide {
                    img { src: mark_src(), width: "30", height: "30", style: "border-radius:8px; flex:none;" }
                }
                span { style: "flex:1; min-width:0; font-size:26px; font-weight:800; letter-spacing:-0.02em;", "{here.label()}" }
            }
            div {
                style: "position:relative; z-index:1; flex:1; min-height:0; overflow-y:auto;",
                div {
                    style: "box-sizing:border-box; width:100%; max-width:1100px; padding:8px 24px 40px; \
                            display:flex; flex-direction:column; gap:28px;",
                    {content}
                }
            }
            if !wide {
                div {
                    style: "position:relative; z-index:5; flex:none; height:56px; display:flex; align-items:stretch; \
                            background:{BAR}; border-top:1px solid {RULE};",
                    for each in Section::ALL {
                        button {
                            key: "{each.label()}",
                            style: tab_style(here == each),
                            onclick: move |_| section.set(each),
                            SectionIcon { section: each, color: if here == each { ACCENT } else { DIM } }
                            span { style: "font-size:10px; font-weight:600;", "{each.short()}" }
                        }
                    }
                }
            }
        }
    };
    // A wide window and a narrow one are separate frames, not one patched:
    // turning a phone from portrait to landscape added the sidebar before
    // the column, and Blitz left the column where it was — under it.
    let frame = |direction: &str| {
        format!(
            "position:absolute; top:0; left:0; width:100vw; height:100vh; display:flex; \
             flex-direction:{direction}; overflow:hidden; background:{BG}; \
             color:{TEXT}; font-family:system-ui, -apple-system, sans-serif;"
        )
    };
    if wide {
        rsx! {
            div {
                style: frame("row"),
                // The sidebar: the mark and the sections.
                div {
                    style: "position:relative; z-index:5; flex:none; width:236px; display:flex; flex-direction:column; \
                            padding:{TOP}px 0 16px; box-sizing:border-box; background:{BAR};",
                    div {
                        style: "display:flex; align-items:center; gap:10px; padding:4px 18px 18px;",
                        img { src: mark_src(), width: "30", height: "30", style: "border-radius:8px; flex:none;" }
                        span { style: "font-size:18px; font-weight:750; letter-spacing:-0.01em;", "Session" }
                    }
                    for each in Section::ALL {
                        NavItem {
                            key: "{each.label()}",
                            section: each,
                            on: here == each,
                            on_press: move |()| section.set(each),
                        }
                    }
                }
                {main}
            }
        }
    } else {
        rsx! {
            div { style: frame("column"), {main} }
        }
    }
}

/// What a raised part of a bar is: the section showing.
const RAISED: &str = "#26292f";

/// A phone's tab: flat, the one showing raised.
fn tab_style(on: bool) -> String {
    let ink = if on { ACCENT } else { DIM };
    format!(
        "flex:1; min-width:0; display:flex; flex-direction:column; align-items:center; \
         justify-content:center; gap:3px; border:none; padding:0; cursor:pointer; \
         font-family:inherit; background:transparent; color:{ink};"
    )
}

/// A section's icon, in its colour outright (Blitz resolves an SVG's
/// `currentColor` once).
#[component]
fn SectionIcon(section: Section, color: &'static str) -> Element {
    let size = 20;
    match section {
        Section::Home => rsx! { House { size, color } },
        Section::OnDevice => rsx! { HardDriveDownload { size, color } },
    }
}

/// A section in the sidebar: its icon and its name, the one showing
/// raised.
#[component]
fn NavItem(section: Section, on: bool, on_press: EventHandler<()>) -> Element {
    let (bg, ink) = if on {
        (RAISED, TEXT)
    } else {
        ("transparent", DIM)
    };
    rsx! {
        button {
                        style: "display:flex; align-items:center; justify-content:flex-start; gap:12px; height:44px; \
                    margin:1px 10px; padding:0 12px; border:none; border-radius:10px; background:{bg}; color:{ink}; font-family:inherit; \
                    font-size:15px; font-weight:600; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            SectionIcon { section, color: if on { ACCENT } else { DIM } }
            "{section.label()}"
        }
    }
}

/// The ink over a cover's art.
const ART_INK: &str = "#ffffffe6";

/// A cover's two colours, picked by its name: the same list always the
/// same, neighbours told apart. None (no name) is a plain tile.
fn palette(seed: &str) -> (&'static str, &'static str) {
    const PAIRS: [(&str, &str); 8] = [
        ("#3b82f6", "#1e2f6b"),
        ("#8b5cf6", "#35155e"),
        ("#ec4899", "#57142f"),
        ("#f97316", "#5e2408"),
        ("#10b981", "#073f31"),
        ("#06b6d4", "#0b3b57"),
        ("#eab308", "#533706"),
        ("#ef4444", "#561010"),
    ];
    if seed.is_empty() {
        return ("#1f2227", "#1a1c20");
    }
    // FNV-1a: stable across runs and builds, unlike std's hasher.
    let hash = seed.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    PAIRS[hash as usize % PAIRS.len()]
}

/// A cover's art at a fixed size: its gradient, its icon over it.
#[component]
fn Art(seed: String, size: u32, radius: u32, children: Element) -> Element {
    let (from, to) = palette(&seed);
    rsx! {
        div {
            style: "flex:none; width:{size}px; height:{size}px; border-radius:{radius}px; display:flex; \
                    align-items:center; justify-content:center; \
                    background:linear-gradient(135deg, {from} 0%, {to} 100%);",
            {children}
        }
    }
}

/// The way in, large: its art, what it is, and a play button.
#[component]
fn Hero(
    kicker: &'static str,
    title: String,
    detail: String,
    seed: String,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    let (from, _) = palette(&seed);
    rsx! {
        button {
            style: "flex:1 1 300px; min-width:0; display:flex; align-items:center; gap:16px; box-sizing:border-box; \
                    padding:14px; border:none; border-radius:20px; cursor:pointer; text-align:left; \
                    background:linear-gradient(120deg, {from}40 0%, {BAR} 65%); color:{TEXT}; font-family:inherit;",
            onclick: move |_| on_press.call(()),
            Art { seed: seed.clone(), size: 72, radius: 14, {children} }
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:5px;",
                span {
                    style: "font-size:11px; font-weight:750; letter-spacing:0.09em; color:{DIM};",
                    "{kicker.to_uppercase()}"
                }
                span { style: "font-size:19px; font-weight:750; letter-spacing:-0.01em; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                span { style: "font-size:13px; color:#a3aab4; line-height:1.4; overflow:hidden; max-height:37px;", "{detail}" }
            }
            div {
                style: "flex:none; width:46px; height:46px; border-radius:23px; display:flex; align-items:center; \
                        justify-content:center; background:{TEXT};",
                Play { size: 20, color: "#0b0c0e" }
            }
        }
    }
}

/// A shelf: its title, a way to all of it, and what is on it.
#[component]
fn Shelf(title: &'static str, on_more: Option<EventHandler<()>>, children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:12px;",
            div {
                style: "display:flex; align-items:center; gap:12px;",
                span { style: "flex:1; min-width:0; font-size:20px; font-weight:750; letter-spacing:-0.01em;", "{title}" }
                if let Some(on_more) = on_more {
                    button {
                        style: "flex:none; display:flex; align-items:center; gap:2px; height:32px; padding:0 4px 0 10px; \
                                border:none; background:transparent; color:{ACCENT}; font-family:inherit; \
                                font-size:14px; font-weight:600; cursor:pointer;",
                        onclick: move |_| on_more.call(()),
                        "See all"
                        ChevronRight { size: 16, color: ACCENT }
                    }
                }
            }
            {children}
        }
    }
}

/// Covers, as many across as fit, each column the same width.
#[component]
fn Covers(children: Element) -> Element {
    rsx! {
        div {
            style: "display:grid; grid-template-columns:repeat(auto-fill, minmax(136px, 1fr)); gap:18px 14px;",
            {children}
        }
    }
}

/// A list or a song as a cover: square art, its name, a line under it —
/// and a mark when it is on this device.
#[component]
fn Cover(
    title: String,
    detail: String,
    seed: String,
    saved: bool,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    let (from, to) = palette(&seed);
    rsx! {
        button {
            style: "min-width:0; display:flex; flex-direction:column; gap:8px; padding:0; border:none; \
                    background:transparent; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            div {
                style: "position:relative; width:100%; aspect-ratio:1 / 1; border-radius:14px; display:flex; \
                        align-items:center; justify-content:center; \
                        background:linear-gradient(135deg, {from} 0%, {to} 100%);",
                {children}
                if saved {
                    div {
                        style: "position:absolute; right:8px; bottom:8px; width:22px; height:22px; border-radius:11px; \
                                display:flex; align-items:center; justify-content:center; background:#0b0c0ecc;",
                        Check { size: 13, color: "#4ac26b" }
                    }
                }
            }
            div {
                style: "display:flex; flex-direction:column; gap:2px; min-width:0; padding:0 2px;",
                span { style: "font-size:14px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                span { style: "font-size:12px; color:{DIM}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{detail}" }
            }
        }
    }
}

/// A one-line way in: an icon, what it does, where to.
#[component]
fn Shortcut(
    title: String,
    detail: String,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        button {
            style: "display:flex; align-items:center; gap:12px; height:56px; padding:0 14px; border:none; \
                    border-radius:14px; background:{BAR}; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            {children}
            span { style: "flex:none; font-size:15px; font-weight:650;", "{title}" }
            span { style: "flex:1; min-width:0; font-size:12px; color:{DIM}; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{detail}" }
            ChevronRight { size: 18, color: DIM }
        }
    }
}

/// Initials in a circle: who is signed in.
#[component]
fn Avatar(email: String, size: u32) -> Element {
    let (from, to) = palette(&email);
    let letter = email
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let font = size * 2 / 5;
    let radius = size / 2;
    rsx! {
        div {
            style: "flex:none; width:{size}px; height:{size}px; border-radius:{radius}px; display:flex; \
                    align-items:center; justify-content:center; font-size:{font}px; font-weight:750; color:#fff; \
                    background:linear-gradient(135deg, {from} 0%, {to} 100%);",
            "{letter}"
        }
    }
}

/// Rows between hairlines.
#[component]
fn Rows(children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; border-top:1px solid {RULE};",
            {children}
        }
    }
}

/// A quiet line of text.
#[component]
fn Quiet(text: &'static str) -> Element {
    rsx! {
        span { style: "font-size:13px; color:{DIM}; line-height:1.5; padding:6px 2px;", "{text}" }
    }
}

/// A message: something went well, or did not.
#[component]
fn Banner(good: bool, text: String) -> Element {
    let (bg, border, ink) = if good {
        ("#12261a", "#1f4a2e", "#4ac26b")
    } else {
        ("#2a1f12", "#5b4219", WARN)
    };
    rsx! {
        div {
            style: "display:flex; gap:10px; align-items:flex-start; padding:12px 14px; border-radius:12px; \
                    background:{bg}; border:1px solid {border}; color:{ink}; font-size:13px; line-height:1.45;",
            if good {
                CircleCheck { size: 18, color: ink }
            } else {
                CircleAlert { size: 18, color: ink }
            }
            span { style: "flex:1; min-width:0;", "{text}" }
        }
    }
}

/// A flat icon button a finger can hit, its icon in the given colour.
#[component]
fn Act(title: &'static str, on_press: EventHandler<()>, children: Element) -> Element {
    rsx! {
        button {
            title,
            style: "flex:none; width:44px; height:44px; display:flex; align-items:center; justify-content:center; \
                    border:none; border-radius:10px; background:transparent; cursor:pointer; padding:0;",
            onclick: move |_| on_press.call(()),
            {children}
        }
    }
}

/// Something on this device, to open.
#[component]
fn LocalRow(title: String, detail: String, on_open: EventHandler<()>) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:14px; min-height:64px; padding:6px 4px; border-bottom:1px solid {RULE};",
            button {
                style: "flex:1; min-width:0; display:flex; align-items:center; gap:14px; border:none; background:transparent; \
                        color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer; padding:0;",
                onclick: move |_| on_open.call(()),
                Art { seed: title.clone(), size: 48, radius: 10, HardDriveDownload { size: 20, color: ART_INK } }
                div {
                    style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:3px;",
                    span { style: "font-size:16px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                    span { style: "font-size:12px; color:{DIM};", "{detail}" }
                }
            }
            Act { title: "Open", on_press: on_open, ChevronRight { size: 18, color: DIM } }
        }
    }
}

/// A song opened lately: its name, which version was open and when, and
/// a pill for each version to open — the song itself and every
/// checkpoint saved beside it (`studio_engine::projects`).
#[component]
fn RecentRow(song: studio_engine::projects::Recent, on_open: EventHandler<PathBuf>) -> Element {
    let name = song.name();
    let alternatives = studio_engine::projects::alternatives(&song.base);
    let main = studio_engine::projects::main_of(&song.base);
    let detail = match (song.is_song(), song.opened_alternative()) {
        (_, Some(alt)) => format!("{alt} · {}", studio_engine::projects::ago(song.at)),
        (true, None) => format!("Song · {}", studio_engine::projects::ago(song.at)),
        (false, None) => format!(
            "{} · {}",
            kind_of(&song.opened),
            studio_engine::projects::ago(song.at)
        ),
    };
    let last = song.opened.clone();
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:8px; padding:10px 4px; border-bottom:1px solid {RULE};",
            button {
                style: "display:flex; align-items:center; gap:14px; border:none; background:transparent; \
                        color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer; padding:0;",
                onclick: move |_| on_open.call(last.clone()),
                Art { seed: name.clone(), size: 48, radius: 10, FileMusic { size: 20, color: ART_INK } }
                div {
                    style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:3px;",
                    span { style: "font-size:16px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{name}" }
                    span { style: "font-size:12px; color:{DIM};", "{detail}" }
                }
                ChevronRight { size: 18, color: DIM }
            }
            if !alternatives.is_empty() {
                div {
                    style: "display:flex; flex-wrap:wrap; gap:8px; padding-left:62px;",
                    if let Some(main) = main.clone() {
                        Chip {
                            label: "The song".to_owned(),
                            on: song.opened_alternative().is_none(),
                            on_press: move |()| on_open.call(main.clone()),
                        }
                    }
                    for alt in alternatives {
                        Chip {
                            key: "{alt.name}",
                            label: alt.name.clone(),
                            on: song.opened_alternative().as_deref() == Some(alt.name.as_str()),
                            on_press: move |()| on_open.call(alt.path.clone()),
                        }
                    }
                }
            }
        }
    }
}

/// The address of a REAPER's Session bridge (or a Session engine) in
/// `text`: a `ws://`/`wss://` URL, `fts-engine:<id>`, or a bare iroh id —
/// what the bridge shows when REAPER starts.
fn engine_address(text: &str) -> Option<String> {
    let text = text.trim();
    let bare_id = text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit());
    (text.starts_with("ws://")
        || text.starts_with("wss://")
        || text.starts_with("fts-engine:")
        || bare_id)
        .then(|| text.to_owned())
}

/// Where the last bridge's address is kept.
fn bridge_memory() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("reaper-bridge"))
}

fn remember_bridge(address: &str) {
    let Some(file) = bridge_memory() else { return };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, address));
    if let Err(e) = written {
        tracing::warn!(error = %e, "start: the bridge's address could not be kept");
    }
}

fn remembered_bridge() -> Option<String> {
    let text = std::fs::read_to_string(bridge_memory()?).ok()?;
    engine_address(&text)
}

/// What the Downloads section says it holds.
#[cfg(target_os = "ios")]
const LOCAL_HINT: &str = "Sets and songs on this iPhone open with no connection: any you put in Files → On My iPhone → Session.";
#[cfg(not(target_os = "ios"))]
const LOCAL_HINT: &str =
    "Sets and songs on this computer open with no connection: any in Documents/Session.";

#[cfg(target_os = "ios")]
const LOCAL_EMPTY: &str = "Nothing here yet. Put a song folder or a setlist in Files → On My iPhone → Session, and it shows here.";
#[cfg(not(target_os = "ios"))]
const LOCAL_EMPTY: &str = "No songs in Documents/Session yet.";

/// The songs and setlists in the app's documents folder: each entry that is
/// a song folder, a setlist, or a project — newest first.
pub(super) fn documents() -> Vec<PathBuf> {
    let Some(dir) = documents_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            p.is_dir() || matches!(ext.as_str(), "setlist" | "rpp")
        })
        // Hidden files left out.
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .map(|p| {
            let at = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (at, p)
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Where the app keeps its songs: on a phone, its own Documents (what the
/// Files app shows as On My iPhone → Session); on a desktop, a Session
/// folder in the user's Documents.
fn documents_dir() -> Option<PathBuf> {
    let docs = dirs::document_dir()?;
    if cfg!(target_os = "ios") {
        Some(docs)
    } else {
        Some(docs.join("Session"))
    }
}

/// An entry's name, as a person reads it: a `.setlist` or a project
/// without its extension.
pub(super) fn title_of(path: &std::path::Path) -> String {
    // A checkpoint is its song's: "Unbreakable · Organized".
    if let (Some(alt), Some(base)) = (
        studio_engine::projects::alternative_name(path),
        studio_engine::projects::base_of(path),
    ) {
        let song = base
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        return format!("{song} · {alt}");
    }
    let is_list = path.extension().is_some_and(|e| {
        e.eq_ignore_ascii_case("setlist")
            || e.eq_ignore_ascii_case("rpp")
            || e.eq_ignore_ascii_case("session")
    });
    let name = if is_list {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What an entry is, in a word or two.
fn kind_of(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "setlist" => "Setlist".to_owned(),
        "rpp" => "REAPER project".to_owned(),
        "session" => "Song".to_owned(),
        _ if studio_engine::setlist::song_in(path).is_some() => "Song".to_owned(),
        _ => "Setlist".to_owned(),
    }
}

/// A section's heading.
#[component]
pub(super) fn Heading(label: String) -> Element {
    rsx! {
        span {
            style: "font-size:12px; font-weight:700; letter-spacing:0.08em; color:{DIM}; padding-left:4px;",
            "{label.to_uppercase()}"
        }
    }
}

/// A panel holding one thing.
#[component]
fn Card(children: Element) -> Element {
    rsx! {
        div {
            style: "padding:18px; border-radius:18px; background:{BAR};",
            {children}
        }
    }
}

/// A row of a list: a file, or the Open dialog.
#[component]
fn ListRow(first: bool, title: String, detail: String, on_press: EventHandler<()>) -> Element {
    let rule = if first {
        "none".to_owned()
    } else {
        format!("1px solid {RULE}")
    };
    rsx! {
        button {
            style: "display:flex; align-items:center; gap:12px; width:100%; box-sizing:border-box; min-height:56px; padding:10px 14px; \
                    border:none; border-top:{rule}; background:transparent; color:{TEXT}; \
                    font-family:inherit; text-align:left; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            div {
                style: "flex:none; color:{DIM}; display:flex;",
                FileMusic { size: 20, color: "currentColor" }
            }
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:2px;",
                span { style: "font-size:15px; font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{title}" }
                span { style: "font-size:12px; color:{DIM};", "{detail}" }
            }
            ChevronRight { size: 18, color: "#6b7280" }
        }
    }
}

/// A rounded button: the primary one filled.
#[component]
pub(super) fn Pill(label: String, primary: bool, on_press: EventHandler<()>) -> Element {
    let (fg, bg, border) = if primary {
        ("#0b0c0e", ACCENT, ACCENT)
    } else {
        (TEXT, "#23262c", RULE)
    };
    rsx! {
        button {
            style: "flex:none; height:38px; padding:0 16px; border-radius:19px; border:1px solid {border}; \
                    background:{bg}; color:{fg}; font-family:inherit; font-size:14px; font-weight:650; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            "{label}"
        }
    }
}

#[component]
pub(super) fn Chip(label: String, on: bool, on_press: EventHandler<()>) -> Element {
    let (fg, bg, border) = if on {
        (ACCENT, "#3aa0ff1f", "#3aa0ff66")
    } else {
        (DIM, "transparent", RULE)
    };
    rsx! {
        button {
            style: "height:32px; padding:0 14px; border-radius:16px; border:1px solid {border}; \
                    background:{bg}; color:{fg}; font-family:inherit; font-size:13px; cursor:pointer;",
            onclick: move |_| on_press.call(()),
            "{label}"
        }
    }
}
