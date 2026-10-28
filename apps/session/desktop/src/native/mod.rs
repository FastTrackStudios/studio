//! The Session app on Blitz (dioxus-native) — see `docs/app-on-blitz.md`.
//!
//! The desktop build launches this instead of the WRY app when the `native`
//! feature is on. It is the app as panels (`session_ui::studio::Arrangement`,
//! the progress bar, …) composed into views under one top bar, with the
//! window's own title bar made transparent so the traffic lights sit in ours.
//!
//! The session is opened once, before the window, on the process's
//! daw-standalone engine, prepared the way `just studio-song` prepares it
//! (organize, build from the chart, generate the click and guide), and handed
//! to every panel as context.
//!
//! That is Engine mode. In Remote and Cue (`--audio remote|cue`,
//! `--reaper [socket]`, `FTS_AUDIO_MODE`, or the top bar's audio menu last
//! time — see `session_ui::open::launch_mode`) nothing is opened here: the
//! window attaches to the system it drives and its open projects are the
//! set (`session_ui::setlist::Setlist::attach`).

#[cfg(target_os = "ios")]
mod ios_audio;

/// What Studio's welcome screen hands Session (what to open, the way back,
/// the account, the Action List).
pub mod handoff;
mod library;
mod shell;
mod start;

use std::any::Any;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;

/// The developer's example session, used when it is there and nothing
/// else has been chosen: Always On Time, from the sessions checkout beside
/// this repo.
const EXAMPLE: &str = "/Volumes/build-disk/development/sessions/Always On Time/Always On Time.RPP";

/// What to open as the window opens, in order: `FTS_SESSION_SETLIST` (a
/// setlist); then `FTS_SESSION_PROJECT` (one song); what was open last
/// time; and the example, if this machine has it. `None` when there is
/// nothing: the window stays on the start screen ([`start`]), which is
/// where a set is picked, joined, or streamed. `FTS_SESSION_START=1` opens
/// nothing, for the start screen itself.
fn choose() -> Option<PathBuf> {
    if std::env::var("FTS_SESSION_START").is_ok_and(|v| v != "0") {
        return None;
    }
    std::env::var_os("FTS_SESSION_SETLIST")
        .or_else(|| std::env::var_os("FTS_SESSION_PROJECT"))
        .map(PathBuf::from)
        .or_else(|| remembered().filter(|p| p.exists()))
        // Not on a phone: its disk is its own, and a simulator's reach into
        // this Mac's `/Volumes` blocks on a privacy prompt no one sees.
        .or_else(|| {
            Some(PathBuf::from(EXAMPLE)).filter(|p| cfg!(not(target_os = "ios")) && p.is_file())
        })
}

/// The songs `target` names: the one song it is (a project, a `.session`,
/// or a song's folder); or a setlist's (a folder of song folders, or a
/// `.setlist` file — see `session_ui::setlist::read_setlist`).
fn songs_of(target: &Path) -> Option<Vec<PathBuf>> {
    session_ui::studio_engine::set::songs_at(target)
}

/// The Open dialog: a REAPER project.
#[cfg(not(target_os = "ios"))]
fn pick() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open a session")
        .add_filter("Song or setlist", &["RPP", "rpp", "setlist"])
        .pick_file()
}

/// A phone has no Open dialog: its songs are the app's documents, which
/// the start screen lists.
#[cfg(target_os = "ios")]
fn pick() -> Option<PathBuf> {
    None
}

/// An error, and the choices `buttons` offers: the one picked, or `None`
/// (the window closed, or the last button, which is always "Quit").
#[cfg(not(target_os = "ios"))]
fn ask(title: &str, description: String, buttons: &[&str]) -> Option<String> {
    let dialog = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(description);
    let dialog = match buttons {
        [a, b, c] => dialog.set_buttons(rfd::MessageButtons::YesNoCancelCustom(
            (*a).into(),
            (*b).into(),
            (*c).into(),
        )),
        [a, b] => dialog.set_buttons(rfd::MessageButtons::OkCancelCustom(
            (*a).into(),
            (*b).into(),
        )),
        _ => dialog,
    };
    match dialog.show() {
        rfd::MessageDialogResult::Custom(choice) if buttons.last() != Some(&choice.as_str()) => {
            Some(choice)
        }
        _ => None,
    }
}

/// On a phone the error is logged and the launch gives up (the page it
/// opens on says so).
#[cfg(target_os = "ios")]
fn ask(title: &str, description: String, _buttons: &[&str]) -> Option<String> {
    tracing::error!(dialog.title = title, dialog.description = %description, "launch: no dialog to ask on this platform");
    None
}

/// Open `url` in the browser (a sign-in's approval page).
fn open_url(url: &str) {
    #[cfg(target_os = "ios")]
    app_shell::ios_scene::open_url(url);
    #[cfg(not(target_os = "ios"))]
    {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        if let Err(e) = std::process::Command::new(opener).arg(url).spawn() {
            tracing::warn!(error = %e, "could not open a link in the browser");
        }
    }
}

/// What is on the clipboard, when a platform lets it be read here (a
/// phone, where typing a link is the hard way).
fn pasted() -> Option<String> {
    #[cfg(target_os = "ios")]
    return app_shell::ios_scene::pasted();
    #[cfg(not(target_os = "ios"))]
    None
}

/// Where the last session's path is kept.
fn memory() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("last-session"))
}

fn remembered() -> Option<PathBuf> {
    let text = std::fs::read_to_string(memory()?).ok()?;
    let path = text.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn remember(project: &Path) {
    // The start screen's Recent, too.
    session_ui::projects::opened(project);
    let Some(file) = memory() else { return };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, project.as_os_str().as_encoded_bytes()));
    if let Err(e) = written {
        // Not fatal: the next launch asks again.
        tracing::warn!(error = %e, "could not remember the session");
    }
}

/// The window, and the setlist in it — every song in the one engine, the
/// first current. Returns when the window closes.
///
/// The window opens on the start screen ([`start`]), opening what was
/// chosen ([`choose`]) as it does — so a phone, with nothing picked before
/// launch, and a desktop take the same way in, and nothing is opened
/// before there is a window to say so. With nothing chosen, or when it
/// does not open, the start screen stays.
pub fn launch() {
    if let Some(Prepared { setlist, open }) = prepare() {
        run(setlist, open);
    }
}

/// What the window opens over: the set a Remote or Cue window attached to,
/// or (Engine mode) what to open as it comes up ([`choose`]).
#[derive(Clone)]
pub struct Prepared {
    setlist: Option<session_ui::setlist::Setlist>,
    open: Option<PathBuf>,
}

/// Everything [`launch`] does before the window: the engine dialer, the
/// audio mode, and attaching to the system a Remote or Cue window drives.
/// `None` when attaching failed and the person chose to quit.
pub fn prepare() -> Option<Prepared> {
    // The global app menu (File, Edit, View, …) for the desktop shell's
    // top bar, on the session bus — on a thread of its own.
    #[cfg(target_os = "linux")]
    session_ui::app_menu::start();
    // How this app dials a Session engine (Remote on another Session): the
    // connector lives with the app's iroh identity (`crate::remote`).
    #[cfg(feature = "session-domain")]
    session_ui::open::set_engine_dialer(|address| {
        Box::pin(async move {
            let addr = crate::remote::EngineAddr::parse(&address).map_err(|e| eyre::eyre!(e))?;
            let engine = crate::remote::connect_engine_daw(&addr).await?;
            let daw = engine.daw.clone();
            Ok((daw, Box::new(engine) as session_ui::open::EngineConnection))
        })
    });
    // The engine in another process (`FTS_ENGINE=spawn|attach`): this
    // window drives it — the engine serves the Action List and owns the set.
    if let Some(prepared) = linked() {
        return Some(prepared);
    }
    // The Action List served to a model and the CLI from the start, so
    // either can open the first song (`SESSION_OPEN`, `SESSION_OPEN_SETLIST`).
    session_ui::open::serve_actions();
    let mode = session_ui::open::launch_mode();
    session_ui::open::set_mode(mode.clone());
    let remote = if mode.owns_project() {
        None
    } else {
        mode.target
    };
    let engine = || Prepared {
        setlist: None,
        open: choose(),
    };
    match remote {
        Some(target) => match attach(&target)? {
            Attached::Set(setlist) => Some(Prepared {
                setlist: Some(setlist),
                open: None,
            }),
            Attached::Engine => Some(engine()),
        },
        None => Some(engine()),
    }
}

/// The window as a view of a Studio engine in another process, when
/// `FTS_ENGINE` asks for one (`session_ui::studio_engine::link`): a running
/// engine attached to, or one spawned in a process group of its own (the
/// audio outlives the window). The window shows the engine's set, and
/// follows it as the CLI, a model or another window changes it. `None`:
/// the engine runs in this process, as it always did.
fn linked() -> Option<Prepared> {
    use session_ui::studio_engine::link;
    let found = link::choose(link::Mode::from_env())?;
    let target = session_ui::open::RemoteTarget::Engine {
        socket: found.socket.clone(),
    };
    session_ui::open::set_mode(session_ui::open::ModeState::remote(target.clone(), false));
    if let Err(e) = session_ui::open::attach(&target) {
        tracing::warn!(engine.pid = found.pid, error = %e, "engine: not attached; in this process instead");
        session_ui::open::set_mode(session_ui::open::ModeState::engine());
        return None;
    }
    tracing::info!(engine.pid = found.pid, engine.socket = %found.socket.display(), "engine: this window drives it");
    link::set_linked(found);
    link::follow(session_ui::setlist::Setlist::from_engine);
    Some(Prepared {
        setlist: None,
        open: None,
    })
}

impl Prepared {
    /// Whether the window drives another system whose set is open already
    /// (Remote, Cue): there is nothing to pick, the set is the window's.
    #[must_use]
    pub const fn attached(&self) -> bool {
        self.setlist.is_some()
    }
}

/// [`prepare`] for a host whose own page comes first (Studio's welcome
/// screen): at the host's launch, so the Action List is served from then
/// (`studio open <setlist>` from the CLI works on the welcome screen), and
/// nothing opened by itself — not the last song, not the example. What the
/// host hands Session ([`handoff::request`]) is what opens. Quitting at a
/// failed attach quits the process, as it does Session's own.
#[must_use]
pub fn prepare_hosted() -> Prepared {
    let prepared = prepare().unwrap_or_else(|| {
        tracing::info!("session: quit at the attach dialog");
        std::process::exit(0)
    });
    Prepared {
        open: None,
        ..prepared
    }
}

/// Session inside another app's window: Studio's, once something on its
/// welcome screen opened a song or a set. Takes the [`Prepared`] the host
/// provides ([`prepare_hosted`]), else prepares as [`launch`] does (on the
/// first render, on the UI thread, where a desktop's dialogs must be
/// asked); provides what `run` hands the window as root contexts, and
/// draws the app. A host that provides [`handoff::Host`] gets Back.
///
/// The host draws the window: its attributes are the host's, and on iOS
/// the host registers the scene delegate (`app_shell::ios_scene`). Quitting
/// at a failed attach quits the process, as it does Session's own.
#[component]
pub fn Embedded() -> Element {
    let Prepared { setlist, open } = use_hook(|| {
        try_consume_context::<Prepared>().unwrap_or_else(|| {
            prepare().unwrap_or_else(|| {
                tracing::info!("session: quit at the attach dialog");
                std::process::exit(0)
            })
        })
    });
    use_context_provider(move || setlist);
    use_context_provider(move || start::OpenFirst(open));
    rsx! { start::App {} }
}

/// What attaching came to: the set the other system has open, or Engine
/// mode after all (it was not there, and this window plays the set).
enum Attached {
    Set(session_ui::setlist::Setlist),
    Engine,
}

/// Attach to the system a Remote or Cue window drives. When it is not
/// there, say so and offer to try again, to open a session here in Engine
/// mode instead, or to quit — never quietly play the set here while
/// someone thinks REAPER is.
fn attach(target: &session_ui::open::RemoteTarget) -> Option<Attached> {
    loop {
        match session_ui::setlist::Setlist::attach(target) {
            Ok(setlist) => return Some(Attached::Set(setlist)),
            Err(e) => {
                tracing::error!(error = %e, audio.target = target.kind(), "could not attach to the system this window drives");
                let answer = ask(
                    "Could not reach it",
                    format!("{}\n\n{e}", target.describe()),
                    &["Try Again", "Open in Engine Mode", "Quit"],
                );
                match answer.as_deref() {
                    Some("Try Again") => {}
                    Some("Open in Engine Mode") => {
                        session_ui::open::set_mode(session_ui::open::ModeState::engine());
                        return Some(Attached::Engine);
                    }
                    _ => return None,
                }
            }
        }
    }
}

/// The window: over `setlist` when it is open already (Remote, Cue);
/// otherwise the start screen, opening `open` — the window is up at once,
/// and the song comes in behind its loading note (`start`).
fn run(setlist: Option<session_ui::setlist::Setlist>, open: Option<PathBuf>) {
    // iOS 27 stops an app that has not adopted scenes: ours is registered
    // before UIKit starts, for Info.plist to name.
    #[cfg(target_os = "ios")]
    app_shell::ios_scene::register(app_shell::ios_scene::Opening::AsHeld);
    let attributes = window_attributes();
    let contexts: Vec<Box<dyn Fn() -> Box<dyn Any> + Send + Sync>> = vec![
        Box::new(move || Box::new(setlist.clone()) as Box<dyn Any>),
        Box::new(move || Box::new(start::OpenFirst(open.clone())) as Box<dyn Any>),
    ];
    dioxus_native::launch_cfg(start::App, contexts, vec![Box::new(attributes)]);
}

/// The window: our top bar IS the title bar. On macOS the native one is made
/// transparent and the content runs up under it, so the traffic lights sit
/// inside the app's own bar, as in the Claude app.
/// A phone's window is its screen: no size of our own to start from (a
/// desktop size taken literally leaves the first surface a different shape
/// from the screen, and the picture stays squeezed after the resize).
///
/// And no status bar: its row is the app's, for the top bar's controls,
/// the way a full-screen instrument app takes it (Logic on iPad). A phone
/// with a notch keeps the notch's inset whatever the bar does.
#[cfg(target_os = "ios")]
fn window_attributes() -> winit::window::WindowAttributes {
    winit::window::WindowAttributes::default()
        .with_title("Session")
        .with_platform_attributes(Box::new(
            winit::platform::ios::WindowAttributesIos::default()
                .with_prefers_status_bar_hidden(true),
        ))
}

#[cfg(not(target_os = "ios"))]
fn window_attributes() -> winit::window::WindowAttributes {
    // `FTS_WINDOW_POS="x,y"` / `FTS_WINDOW_SIZE="WxH"` (logical pixels)
    // place the window instead of maximizing it — what `just duo` uses to
    // put two collaborating windows side by side.
    fn pair(var: &str, sep: char) -> Option<(f64, f64)> {
        let raw = std::env::var(var).ok()?;
        let (a, b) = raw.split_once(sep)?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }
    let (pos, size) = (pair("FTS_WINDOW_POS", ','), pair("FTS_WINDOW_SIZE", 'x'));
    let (w, h) = size.unwrap_or((1600.0, 1000.0));
    let mut attributes = winit::window::WindowAttributes::default()
        .with_title("Session")
        // Opens filling the screen unless placed; this is the size it
        // restores to when un-maximized.
        .with_maximized(pos.is_none() && size.is_none())
        .with_surface_size(winit::dpi::LogicalSize::new(w, h))
        // Small enough to be phone-sized: below ~700 wide (or ~500 tall)
        // the app takes its small-screen layout (`session_ui::compact`).
        .with_min_surface_size(winit::dpi::LogicalSize::new(320.0, 300.0));
    if let Some((x, y)) = pos {
        attributes = attributes.with_position(winit::dpi::LogicalPosition::new(x, y));
    }
    #[cfg(target_os = "macos")]
    let attributes = attributes.with_platform_attributes(Box::new(
        winit::platform::macos::WindowAttributesMacOS::default()
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true),
    ));
    // The app id a desktop knows the window by (`session`): what a window
    // rule matches to put it on its workspace, on Wayland and X11 alike.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let attributes = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        attributes.with_platform_attributes(Box::new(
            winit::platform::wayland::WindowAttributesWayland::default().with_name(APP_ID, APP_ID),
        ))
    } else {
        attributes.with_platform_attributes(Box::new(
            winit::platform::x11::WindowAttributesX11::default().with_name(APP_ID, APP_ID),
        ))
    };
    attributes
}

/// The window's app id on Linux (Wayland's `app_id`, X11's `WM_CLASS`).
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
const APP_ID: &str = "session";
