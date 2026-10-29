//! The Session app on Blitz (dioxus-native) — see `docs/app-on-blitz.md`.
//!
//! Studio's DAW view ([`Embedded`], behind the welcome screen). It is the
//! app as panels (`session_ui::studio::Arrangement`,
//! the progress bar, …) composed into views under one top bar, with the
//! window's own title bar made transparent so the traffic lights sit in ours.
//!
//! The window is a client of its engine, never the engine: on the desktop
//! it attaches to a running `studio engine` or spawns one
//! (`session_engine::studio_engine::link`), and starts another when that one
//! goes; on iOS (no helper process) the engine runs in this process and the
//! window reaches it over an in-memory link — the same client either way
//! (`session_engine::view_client`). The engine opens sets; the window shows
//! what it has open.
//!
//! In Remote and Cue on another system (`--audio remote|cue`,
//! `--reaper [socket]`, `FTS_AUDIO_MODE`, or the top bar's audio menu last
//! time — see `session_engine::open::launch_mode`) the window attaches to the
//! system it drives instead, and its open projects are the set
//! (`session_engine::setlist::Setlist::attach`).

#[cfg(target_os = "ios")]
mod ios_audio;

/// What Studio's welcome screen hands Session (what to open, the way back,
/// the account, the Action List).
pub mod handoff;
mod library;
/// The welcome screen's Publish to Task card (`SESSION_INGEST`).
pub mod publish;
/// The layer that notes what the paint thread runs, for the traces of the
/// frames that miss 120 Hz (`session_ui::frames::slow`): the app's
/// subscriber takes it.
pub use session_ui::frames::slow::WorkLayer as FrameWorkLayer;
mod shell;
mod start;

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
    session_engine::projects::opened(project);
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

/// What the window opens over: the set a Remote or Cue window attached to,
/// or what the engine is to open as it comes up ([`choose`]).
#[derive(Clone)]
pub struct Prepared {
    setlist: Option<session_engine::setlist::Setlist>,
    open: Option<PathBuf>,
}

/// Everything the window needs before it opens: the engine dialer, the
/// audio mode, and the link — to the engine, or to the system a Remote or
/// Cue window drives. `None` when the person chose to quit.
pub fn prepare() -> Option<Prepared> {
    // The global app menu (File, Edit, View, …) for the desktop shell's
    // top bar, on the session bus — on a thread of its own.
    #[cfg(target_os = "linux")]
    session_ui::app_menu::start();
    // How this app dials a Session engine (Remote on another Session): the
    // connector lives with the app's iroh identity (`crate::session::remote`).
    session_engine::open::set_engine_dialer(|address| {
        Box::pin(async move {
            let addr =
                crate::session::remote::EngineAddr::parse(&address).map_err(|e| eyre::eyre!(e))?;
            let engine = crate::session::remote::connect_engine_daw(&addr).await?;
            let daw = engine.daw.clone();
            Ok((
                daw,
                Box::new(engine) as session_engine::open::EngineConnection,
            ))
        })
    });
    let mode = session_engine::open::launch_mode();
    let remote = mode.target.clone().filter(|_| !mode.owns_project());
    let Some(target) = remote else {
        return engine_link();
    };
    session_engine::open::set_mode(mode);
    match attach(&target)? {
        Attached::Set(setlist) => Some(Prepared {
            setlist: Some(setlist),
            open: None,
        }),
        Attached::Engine => engine_link(),
    }
}

/// The window on its engine, with what the launch chose to open
/// ([`choose`]) — which the engine opens.
fn engine_link() -> Option<Prepared> {
    let linked = link_engine();
    linked.then(|| Prepared {
        setlist: None,
        open: choose(),
    })
}

/// iOS: the engine in this process — the phone has no helper process —
/// reached over an in-memory link, the window's client the same as on the
/// desktop. It serves the Action List (MCP) as `studio engine` does.
#[cfg(target_os = "ios")]
fn link_engine() -> bool {
    session_engine::open::set_mode(session_engine::open::ModeState::engine());
    session_engine::open::serve_actions();
    if let Err(e) = session_engine::open::link_window_here() {
        tracing::error!(error = %e, "engine: it did not start in this process");
        return false;
    }
    true
}

/// The desktop: the window as a client of a Studio engine in another
/// process (`session_engine::studio_engine::link`) — a running one attached
/// to, else one spawned in a process group of its own (the audio outlives
/// the window), and another started if it goes. Asked again, or to quit,
/// when none can be had.
#[cfg(not(target_os = "ios"))]
fn link_engine() -> bool {
    use session_engine::studio_engine::link;
    let mode = link::Mode::from_env();
    let mut waiting = false;
    loop {
        let linked = link::choose(mode, std::time::Duration::from_secs(30))
            .and_then(|found| attach_engine(&found).map(|()| found));
        match linked {
            Ok(found) => {
                tracing::info!(engine.pid = found.pid, engine.socket = %found.socket.display(), "engine: this window drives it");
                link::keep_linked(mode, attach_engine);
                return true;
            }
            // Attaching only, there is nothing to start: the engine is
            // someone else's to bring up (`just dev` builds it first), so
            // the window waits for it however long that takes.
            Err(e) if mode == link::Mode::Attach => {
                if !waiting {
                    tracing::warn!(error = %e, "engine: none to attach to yet; waiting for one");
                    waiting = true;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "engine: this window has none to drive");
                let answer = ask(
                    "Studio's engine did not start",
                    format!(
                        "{e}\n\nThe engine's log is {}.",
                        link::log_file()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ),
                    &["Try Again", "Quit"],
                );
                if answer.as_deref() != Some("Try Again") {
                    return false;
                }
            }
        }
    }
}

/// Attach this window to `found`: the daw facade and the view link over
/// its socket, and it is the engine this window drives.
#[cfg(not(target_os = "ios"))]
fn attach_engine(found: &session_engine::studio_engine::link::Found) -> Result<(), String> {
    let target = session_engine::open::RemoteTarget::Engine {
        socket: found.socket.clone(),
    };
    session_engine::open::set_mode(session_engine::open::ModeState::remote(
        target.clone(),
        false,
    ));
    session_engine::open::attach(&target).map_err(|e| e.to_string())?;
    session_engine::studio_engine::link::set_linked(found.clone());
    Ok(())
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
/// provides ([`prepare_hosted`]), else [`prepare`]s (on the first render,
/// on the UI thread, where a desktop's dialogs must be asked); provides
/// the set and what to open as root contexts, and draws the app. A host that provides [`handoff::Host`] gets Back.
///
/// The host draws the window: its attributes are the host's, and on iOS
/// the host registers the scene delegate (`app_shell::ios_scene`). Quitting
/// at a failed attach quits the process.
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

/// What attaching came to: the set the other system has open, or Studio's
/// engine after all (it was not there).
enum Attached {
    Set(session_engine::setlist::Setlist),
    Engine,
}

/// Attach to the system a Remote or Cue window drives. When it is not
/// there, say so and offer to try again, to open a session here in Engine
/// mode instead, or to quit — never quietly play the set here while
/// someone thinks REAPER is.
fn attach(target: &session_engine::open::RemoteTarget) -> Option<Attached> {
    loop {
        match session_engine::setlist::Setlist::attach(target) {
            Ok(setlist) => return Some(Attached::Set(setlist)),
            Err(e) => {
                tracing::error!(error = %e, audio.target = target.kind(), "could not attach to the system this window drives");
                let answer = ask(
                    "Could not reach it",
                    format!("{}\n\n{e}", target.describe()),
                    &["Try Again", "Use Studio's Engine", "Quit"],
                );
                match answer.as_deref() {
                    Some("Try Again") => {}
                    Some("Use Studio's Engine") => return Some(Attached::Engine),
                    _ => return None,
                }
            }
        }
    }
}
