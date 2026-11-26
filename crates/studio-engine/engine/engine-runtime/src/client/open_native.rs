//! Opening a project, and standing its backend up.
//!
//! Two ways in, and they differ in who owns the project.
//!
//! [`open_and_serve`] **owns** it: a `.rpp` becomes an in-process
//! standalone backend that this window is the only writer of.
//!
//! [`attach_to_reaper`] **borrows** it: REAPER owns the project, the FTS
//! extension publishes its whole service surface on a Unix socket, and
//! this window becomes one more client of it — the same position the
//! CLI and the desktop app already hold. Everything above this module is
//! written against the facade rather than a backend, so only this file
//! knows which of the two it is.
//!
//! The owning path is three steps, and the order is the whole point:
//!
//! 1. Parse the `.rpp` into a [`Standalone`] backend and materialize its
//!    media through the media bay.
//! 2. Serve that backend over an in-process memory link and install the
//!    global `daw` facade every panel reads through.
//! 3. Attach the audio engine, so the transport moves audio and not just
//!    a line.
//!
//! None of it happens before there is a window. A real session takes
//! seconds to parse and its media longer, and a blank screen for the
//! duration is a self-inflicted wait — so this runs on a worker thread
//! and the UI waits for the facade to appear
//! (`daw_view::song::project::fetch_when_ready`).
//!
//! # The audio mode
//!
//! Which of the two a window is, is its audio mode ([`AudioMode`], issue
//! #142), held here as process state and read everywhere through [`mode`]:
//! **Engine** owns (the in-process engine plays), **Remote** borrows (the
//! other system plays; nothing here does), **Cue** borrows with a local
//! click and guide. It is set once at launch ([`launch_mode`],
//! [`set_mode`]) and can climb at runtime (see [`crate::client::audio_mode`]).
//!
//! Everything that reaches the local engine rather than the facade goes
//! through here and says what it needs ([`with_local_engine`],
//! [`LocalOnly`]): in Remote it is skipped with one warning, never run
//! against an engine that holds nothing. What the facade CAN do in Remote
//! is done through it — the song on screen ([`current_song`]), switching
//! songs ([`switch_song`]), a song's colour ([`set_song_color`]).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use daw::standalone::Standalone;

pub use crate::client::audio_mode::{
    Assets, AudioMode, ModeState, RemoteTarget, begin_asset_load, set_assets_progress,
    set_cue_ready,
};
pub use crate::open_core::{
    AsItIs, Media, OpenHost, Opened, ProjectText, SongPlan, is_session, project_text,
    project_text_in,
};
/// The runtime a window's link and engine calls run on, the ways sync code
/// waits on it, and the song on screen: the views' contract's
/// (`daw_view::open`), whose song on screen is this module's
/// ([`song_here`], through Studio's host).
pub use daw_view::open::{
    block_on_engine, current_song, engine_runtime, facade_blocking, in_song, runtime,
    traced_blocking,
};

/// Keeps the in-process link's acceptor alive for the process's
/// lifetime. Dropping it silently disconnects every panel.
pub static BUNDLE: OnceLock<Served> = OnceLock::new();

/// The engine's router served in this process over an in-memory vox link:
/// the daw facade and the view service (`crate::client::studio_engine::view_host`)
/// on one connection — what the engine's own code and a window in this
/// process (iOS, a test) reach it through, as a window on the desktop
/// reaches `studio engine` over its socket.
pub struct Served {
    pub caller: vox::Caller,
    /// Owns the acceptor task behind the link.
    pub scope: Arc<architect::Scope>,
}

/// The process's engine: every song this window opens lives in it, side by
/// side, one of them current — a setlist is several projects in one
/// `Standalone`, not several engines. Made by the first open.
static ENGINE: OnceLock<Standalone> = OnceLock::new();

pub fn engine() -> &'static Standalone {
    crate::client::host::install();
    ENGINE.get_or_init(|| {
        let daw = Standalone::new();
        equip(&daw);
        daw
    })
}

/// Make an engine a Session engine: what every engine a song opens into
/// needs, whoever runs it — this window, or `engine-host --engine`.
/// Today that is the instrument the Click / Count / Guide MIDI tracks
/// play through.
pub fn equip(daw: &Standalone) {
    // An effect's automated parameter reads as the effect writes it: daw has
    // no effects of its own, so it is told how Signal's (`builtin`, i.e.
    // signal-effects) say a value.
    crate::client::automation::install_param_reading(builtin::engine::reading);
    crate::guide_instrument::install(
        daw,
        crate::guide_instrument::Library::Folder(crate::guide_instrument::samples_dir()),
    );
}

/// Make `project_guid` the song on screen — what picking a setlist tab
/// does.
///
/// - **Engine**: [`switch_to`] on the process's engine; the audio moves
///   with it when the window plays at all.
/// - **Remote / Cue**: the facade's project service selects it — a REAPER
///   tab — and waits for the answer, so a play pressed next plays this
///   song. A Cue window's click and guide go with it
///   ([`cue_follow_song`]).
pub fn switch_song(project_guid: &str) {
    let state = mode();
    if state.owns_project() {
        // The engine's own, in this process: its switch (`crate::client::body`).
        crate::client::body::studio_body().switch_local(project_guid);
        return;
    }
    set_remote_current(Some(project_guid.to_owned()));
    let selected = facade_blocking("switch song", |daw| {
        let guid = project_guid.to_owned();
        async move { daw.select_project(guid).await.map(|_| ()) }
    });
    if let Err(e) = selected {
        tracing::warn!(
            audio.mode = state.requested.name(),
            song.project = project_guid,
            error = %e,
            "remote: the song could not be selected on the system this window drives"
        );
    }
    if state.requested == AudioMode::Cue {
        cue_follow_song(project_guid);
    }
}

/// A Cue window's click and guide follow the song by themselves: the Cue
/// task ([`crate::client::stream_in`]) watches the remote's current song, whoever changed
/// it (a pick here, a tab in REAPER).
fn cue_follow_song(project_guid: &str) {
    tracing::debug!(
        song.project = project_guid,
        "cue: the song changed; the Cue task follows"
    );
}

/// Something only the local engine can do — skipped, with one warning each,
/// in a window that drives another system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalOnly {
    /// Rebuilding the song's structure from an edited chart
    /// (`engine_domain::keyflow::from_chart` runs on the sync backend traits,
    /// which only an in-process backend implements).
    ChartRebuild,
    /// Writing a song's `.session` — the native format is the local
    /// engine's project; a remote system saves its own.
    SessionSave,
    /// daw-transport-sync's per-buffer backend, which followers lock to.
    /// Following over the facade is a separate issue.
    TransportSync,
}

impl LocalOnly {
    const fn name(self) -> &'static str {
        match self {
            Self::ChartRebuild => "chart_rebuild",
            Self::SessionSave => "session_save",
            Self::TransportSync => "transport_sync",
        }
    }

    const fn bit(self) -> u32 {
        match self {
            Self::ChartRebuild => 1,
            Self::SessionSave => 2,
            Self::TransportSync => 4,
        }
    }
}

/// The capabilities already warned about, so each warns once.
static WARNED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The process's engine, when this window owns its project (Engine mode);
/// `None` in Remote and Cue — for a caller that has its own remote path
/// (the Organize toolbar), so nothing is warned about.
#[must_use]
pub fn local_engine() -> Option<&'static Standalone> {
    crate::client::audio_mode::owns_project().then(engine)
}

/// Run `f` against the process's engine when this window owns its project
/// (Engine mode) — for what acts on the session directly rather than
/// through the facade. `None` in Remote and Cue, where there is no local
/// project to act on: `what` says what was skipped, once.
pub fn with_local_engine<R>(what: LocalOnly, f: impl FnOnce(&Standalone) -> R) -> Option<R> {
    if let Some(daw) = local_engine() {
        return Some(f(daw));
    }
    let first = WARNED.fetch_or(what.bit(), std::sync::atomic::Ordering::Relaxed) & what.bit() == 0;
    if first {
        let state = mode();
        tracing::warn!(
            audio.mode = state.requested.name(),
            audio.target = state.target.as_ref().map(RemoteTarget::kind),
            audio.capability = what.name(),
            "remote: this needs the local engine, which this window does not run; skipped"
        );
    }
    None
}

/// The song on screen as Studio has it (what its host answers
/// `daw_view::open::current_song` with): the engine's current project in Engine; in Remote
/// and Cue, the facade's current project — as last selected here or seen
/// on the remote (a tab picked in REAPER itself), kept fresh by a poll
/// rather than asked for on every call: panels ask every frame.
#[must_use]
pub(crate) fn song_here() -> Option<String> {
    use daw::service::Projects as _;
    if crate::client::audio_mode::owns_project() {
        return engine().current().map(|p| p.guid);
    }
    REMOTE_CURRENT.read().ok().and_then(|slot| slot.clone())
}

/// The remote's current project, as [`current_song`] reports it.
static REMOTE_CURRENT: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// Bumped by every selection made here, so a poll that was already in
/// flight when the song changed cannot write the old song back.
static REMOTE_PICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn set_remote_current(guid: Option<String>) {
    REMOTE_PICKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut slot) = REMOTE_CURRENT.write() {
        *slot = guid;
    }
}

/// Keep [`REMOTE_CURRENT`] following the remote's current project, twice a
/// second, for the life of the process. Started once, by the first attach.
fn watch_remote_current(rt: &'static tokio::runtime::Runtime) {
    static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if WATCHING.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    rt.spawn(async {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if crate::client::audio_mode::owns_project() {
                continue;
            }
            let Some(daw) = daw::rpc::Daw::try_get() else {
                continue;
            };
            let picks = REMOTE_PICKS.load(std::sync::atomic::Ordering::SeqCst);
            // A gone REAPER is the reattach loop's business; the last
            // known song stays until it answers again.
            if let Ok(project) = daw.current_project().await
                && REMOTE_PICKS.load(std::sync::atomic::Ordering::SeqCst) == picks
                && let Ok(mut slot) = REMOTE_CURRENT.write()
                && slot.as_deref() != Some(project.guid())
            {
                *slot = Some(project.guid().to_owned());
            }
        }
    });
}

/// A live project this window is attached to but does not own.
///
/// Deliberately not an [`Opened`]: that one carries the `Standalone` it
/// stood up, and there is no backend here to carry. Handing back an
/// unused `Standalone::new()` to fit the shape would be a lie the
/// compiler would happily keep.
pub struct Attached {
    pub name: String,
    pub project_guid: String,
    pub track_count: usize,
    /// Where REAPER has the project on disk, when it has been saved.
    ///
    /// Carried because the album's patch list is found by walking up
    /// from the project, and an attached window has no path of its own
    /// to walk from. `None` for a project REAPER has never saved, which
    /// is a real state and not an error: it simply has no album yet.
    pub path: Option<std::path::PathBuf>,
}

/// Keeps the vox connection alive for the process's lifetime.
///
/// The same reason as [`BUNDLE`], for a different link: the connection
/// handle owns the lanes, so dropping it disconnects every panel at
/// once — and because the panels read through a global facade rather
/// than through this value, nothing would name the cause.
/// The live connection, replaceable.
///
/// A `OnceLock` here was the same mistake `daw_control`'s global made
/// and fixed: a window attaches to one REAPER for its life only if that
/// REAPER lives as long as the window. Quit REAPER and reopen it and
/// the pid changes, so the socket changes, so the connection has to be
/// re-made — and a cell that can only be written once left every panel
/// holding a dead link with no way to re-point it, and nothing on
/// screen to say so.
///
/// The old connection is dropped when a new one replaces it, which is
/// what actually closes the dead socket.
static CONNECTION: std::sync::RwLock<Option<studio::cli::DawConnection>> =
    std::sync::RwLock::new(None);

/// The socket the last attach used, so a re-attach can repeat it.
///
/// `None` means discovery, which is the common case and the one that
/// must be repeated rather than remembered: the pid in the path is the
/// very thing that changed.
static ATTACHED_TO: std::sync::RwLock<Option<Option<std::path::PathBuf>>> =
    std::sync::RwLock::new(None);

/// Attach to a REAPER that is already running the FTS extension.
///
/// `socket` is the extension's Unix socket; `None` discovers one in
/// `/tmp`. Returns what the live project turned out to be, read back
/// through the facade rather than assumed — a window that reports what
/// it asked for rather than what it got is how a silent
/// wrong-project-attached bug survives.
///
/// # What this deliberately does not do
///
/// No media materialising, no meters, no audio engine. REAPER owns the
/// audio; standing a second engine up beside it would be two things
/// playing the same project.
///
/// # Errors
///
/// When no socket is found, the connection cannot be established, or
/// the facade was already installed by the owning path — the two are
/// mutually exclusive by construction, since there is one global facade
/// and it can only point at one backend.
pub fn attach_to_reaper(socket: Option<std::path::PathBuf>) -> eyre::Result<Attached> {
    crate::client::host::install();
    if BUNDLE.get().is_some() {
        eyre::bail!("this window already owns a project; it cannot also attach to REAPER");
    }
    let rt = engine_runtime()?;
    let socket_for_retry = socket.clone();
    let socket_for_mode = socket.clone();
    let connection = rt
        .block_on(studio::cli::connect(socket))
        .map_err(|e| eyre::eyre!("could not attach to REAPER: {e}"))?;

    let block_on_rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(connection.daw.clone(), block_on_rt);
    if let Ok(mut slot) = CONNECTION.write() {
        // Assigning drops whatever was there, which is what closes a
        // socket to a REAPER that has gone.
        *slot = Some(connection);
    }
    if let Ok(mut slot) = ATTACHED_TO.write() {
        *slot = Some(socket_for_retry);
    }

    // Read the project back rather than reporting what we hoped for. A
    // window that prints the project it asked for, instead of the one it
    // got, is how an attached-to-the-wrong-REAPER bug survives a whole
    // session unnoticed.
    read_back(
        rt,
        RemoteTarget::Reaper {
            socket: socket_for_mode,
        },
    )
}

// ── which of the two ways in ─────────────────────────────────────────

/// Where this window's project comes from.
///
/// Two, and they are exclusive: the window either owns a project it
/// parsed, or it is one client of a REAPER that owns one. There is a
/// single global facade and it points at one backend, so "both" is not
/// a state this can be in.
pub enum Source {
    /// A `.rpp` this window parses and is the only writer of.
    Own(std::path::PathBuf),
    /// A REAPER already running the FTS extension. `None` discovers its
    /// socket in `/tmp`; `Some` names one, which is what the test
    /// harness needs when several REAPERs are up at once.
    Reaper(Option<std::path::PathBuf>),
}

/// Read the source from the command line and the environment.
///
/// Remote on REAPER ([`launch_mode`]) wins over a path, because asking for
/// both is a mistake worth answering rather than a preference worth
/// guessing at — and the live one is the one you would have meant.
pub fn source() -> Option<Source> {
    if let Some(RemoteTarget::Reaper { socket }) = launch_mode().target {
        return Some(Source::Reaper(socket));
    }
    std::env::args()
        .nth(1)
        .filter(|a| !a.is_empty() && !a.starts_with("--"))
        .or_else(|| std::env::var("SESSION_DAW_PROJECT").ok())
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .map(Source::Own)
}

/// Attach to a live REAPER rather than opening a file.
pub const REAPER_ENV: &str = crate::client::audio_mode::REAPER_ENV;
/// Which REAPER, when more than one is running.
pub const SOCKET_ENV: &str = crate::client::audio_mode::SOCKET_ENV;

// ── the audio mode ───────────────────────────────────────────────────

/// The process's audio mode: requested, effective, the target, loading.
#[must_use]
pub fn mode() -> ModeState {
    crate::client::audio_mode::state()
}

/// Set the process's audio mode — at launch, before the first open or
/// attach.
pub fn set_mode(state: ModeState) {
    crate::client::audio_mode::set(state);
}

/// The mode this launch asks for, from the command line
/// (`--audio engine|remote|cue`, `--reaper [socket]`), the environment
/// (`FTS_AUDIO_MODE`, `SESSION_DAW_REAPER` + `FTS_SOCKET`,
/// `FTS_AUDIO_TARGET`), whether a project was named
/// (`FTS_SESSION_PROJECT` / `FTS_SESSION_SETLIST` / `SESSION_DAW_PROJECT`
/// or a path argument — Engine), and what the picker last chose — in that
/// order (see [`crate::client::audio_mode::from_launch`]). Does not install it.
#[must_use]
pub fn launch_mode() -> ModeState {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |key: &str| std::env::var(key).ok();
    let named = [
        "FTS_SESSION_PROJECT",
        "FTS_SESSION_SETLIST",
        "SESSION_DAW_PROJECT",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some_and(|v| !v.is_empty()))
        || args
            .first()
            .is_some_and(|a| !a.starts_with("--") && Path::new(a).exists());
    crate::client::audio_mode::from_launch(&args, &env, named, remembered_mode())
}

/// Where the picker's choice is kept between launches.
fn mode_memory() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("audio-mode"))
}

fn remembered_mode() -> Option<AudioMode> {
    AudioMode::parse(&std::fs::read_to_string(mode_memory()?).ok()?)
}

/// Keep `mode` as the one the next launch opens in (when the launch does
/// not name one — see [`launch_mode`]).
pub fn remember_mode(mode: AudioMode) {
    let Some(file) = mode_memory() else { return };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, mode.name().to_ascii_lowercase()));
    if let Err(e) = written {
        tracing::warn!(error = %e, audio.requested = mode.name(), "the audio mode could not be remembered");
    }
}

/// Change the mode while the window runs, as far as it can be changed
/// without re-pointing the facade: Remote and Cue drive the same backend,
/// so either becomes the other at once. Engine and Remote are different
/// backends — the window's projects live in one or the other — so that
/// change is remembered for the next launch and `false` comes back.
pub fn request_mode(to: AudioMode) -> bool {
    remember_mode(to);
    let state = mode();
    if state.requested == to {
        return true;
    }
    let live = !state.owns_project() && to != AudioMode::Engine;
    if live {
        crate::client::audio_mode::update_requested(to);
    }
    live
}

/// Attach to the system `target` names — the Remote and Cue way in.
///
/// # Errors
///
/// As [`attach_to_reaper`]; a Session engine target is not wired yet
/// (#142 follow-up) and says so rather than attaching to something else.
pub fn attach(target: &RemoteTarget) -> eyre::Result<Attached> {
    crate::client::host::install();
    let attached = match target {
        RemoteTarget::Reaper { socket } => attach_to_reaper(socket.clone()),
        RemoteTarget::Session { address } => attach_to_engine(address),
        RemoteTarget::Engine { socket } => attach_to_local_engine(socket),
    }?;
    // Idle unless Cue is asked for; then the click and guide play here,
    // locked to what this window drives.
    crate::client::body::studio_body().cue_start();
    Ok(attached)
}

/// Something that keeps a dialed engine's connection open for as long
/// as it lives (dropping it closes the connection).
pub type EngineConnection = Box<dyn std::any::Any + Send + Sync>;

/// Dials a Session engine: its address as a person pastes it
/// (`fts-engine:<id>`, `ws://host:4040/vox`) in, its daw facade out.
///
/// Installed by the app (the dialer lives with the app's network
/// identity — its iroh endpoint and key — not here).
pub type EngineDialer = fn(
    String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = eyre::Result<(daw::rpc::Daw, EngineConnection)>> + Send>,
>;

static ENGINE_DIALER: OnceLock<EngineDialer> = OnceLock::new();

/// The connection to the Session engine this window is attached to.
static ENGINE_CONNECTION: std::sync::RwLock<Option<EngineConnection>> =
    std::sync::RwLock::new(None);

/// Install how this app dials a Session engine (once, at launch).
pub fn set_engine_dialer(dialer: EngineDialer) {
    let _ = ENGINE_DIALER.set(dialer);
}

/// Attach to a Session engine (`engine-host --engine`) the way
/// [`attach_to_reaper`] attaches to REAPER: the engine owns the project
/// and plays it; this window drives it through the daw facade.
///
/// # Errors
///
/// When this window already owns a project, no dialer is installed, the
/// engine cannot be reached, or its project cannot be read back.
pub fn attach_to_engine(address: &str) -> eyre::Result<Attached> {
    crate::client::host::install();
    if BUNDLE.get().is_some() {
        eyre::bail!("this window already owns a project; it cannot also drive a Session engine");
    }
    let dial = ENGINE_DIALER
        .get()
        .ok_or_else(|| eyre::eyre!("this build cannot dial a Session engine"))?;
    let rt = engine_runtime()?;
    let (daw, connection) = rt
        .block_on(dial(address.to_owned()))
        .map_err(|e| eyre::eyre!("could not reach the Session engine at {address}: {e}"))?;
    let block_on_rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(daw, block_on_rt);
    if let Ok(mut slot) = ENGINE_CONNECTION.write() {
        *slot = Some(connection);
    }
    read_back(
        rt,
        RemoteTarget::Session {
            address: address.to_owned(),
        },
    )
}

/// Link a window in this process to the engine in it, over the in-memory
/// link the engine serves itself on: the same client a desktop window has
/// to `studio engine` over its socket (`crate::client::view_client`) — what iOS
/// (no helper process) and a test do. Starts the engine when it is not up.
///
/// # Errors
///
/// The engine could not be started.
pub fn link_window_here() -> eyre::Result<()> {
    crate::client::host::install();
    if BUNDLE.get().is_none() {
        crate::client::body::studio_body().start_engine()?;
    }
    let served = BUNDLE
        .get()
        .ok_or_else(|| eyre::eyre!("the engine's link did not come up"))?;
    crate::client::view_client::connect(served.caller.clone());
    Ok(())
}

/// Attach to a Studio engine on this machine (`studio engine`, found or
/// spawned by `crate::client::studio_engine::link`) over its Unix socket — the
/// lanes REAPER's bridge serves, so the facade is the same one. The engine
/// owns the set and plays it; this window drives it. Unlike REAPER it may
/// have nothing open yet: then there is no project to read back, and the
/// window shows its start screen until the engine opens a set.
///
/// # Errors
///
/// When this window already owns a project, or the socket does not answer.
pub fn attach_to_local_engine(socket: &Path) -> eyre::Result<Attached> {
    crate::client::host::install();
    if BUNDLE.get().is_some() {
        eyre::bail!("this window already owns a project; it cannot also drive a Studio engine");
    }
    let rt = engine_runtime()?;
    let connection = rt
        .block_on(studio::cli::connect(Some(socket.to_path_buf())))
        .map_err(|e| {
            eyre::eyre!(
                "could not reach the Studio engine at {}: {e}",
                socket.display()
            )
        })?;
    let block_on_rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(connection.daw.clone(), block_on_rt);
    // The engine's view of itself, over the same connection.
    crate::client::view_client::connect(connection.daw.caller().clone());
    if let Ok(mut slot) = CONNECTION.write() {
        *slot = Some(connection);
    }
    let target = RemoteTarget::Engine {
        socket: socket.to_path_buf(),
    };
    if mode().owns_project() {
        set_mode(ModeState::remote(target.clone(), false));
    }
    match read_back(rt, target) {
        Ok(attached) => Ok(attached),
        // Nothing open in the engine yet: attached all the same.
        Err(e) => {
            tracing::info!(engine.socket = %socket.display(), why = %e, "attached to a Studio engine with no song open");
            watch_remote_current(rt);
            Ok(Attached {
                name: String::new(),
                project_guid: String::new(),
                track_count: 0,
                path: None,
            })
        }
    }
}

/// After an attach: read the project back (never report what was hoped
/// for), and record this window as Remote on `target`.
fn read_back(rt: &'static tokio::runtime::Runtime, target: RemoteTarget) -> eyre::Result<Attached> {
    let daw = daw::rpc::Daw::try_get().ok_or_else(|| eyre::eyre!("the facade did not install"))?;
    let (name, guid, path, track_count) = rt.block_on(async {
        let project = daw.current_project().await?;
        let info = project.info().await?;
        let tracks = project.tracks().all().await?;
        Ok::<_, daw::rpc::Error>((info.name, info.guid, info.path, tracks.len()))
    })?;
    // Attached is Remote, whatever the launch said — a caller that attaches
    // without choosing a mode first (a test) is not left reporting Engine.
    // A Cue window stays Cue.
    if mode().owns_project() {
        set_mode(ModeState::remote(target, false));
    }
    set_remote_current(Some(guid.clone()));
    watch_remote_current(rt);
    Ok(Attached {
        name,
        project_guid: guid,
        path: Some(path).filter(|p| !p.is_empty()).map(Into::into),
        track_count,
    })
}

/// Attach again, to whatever REAPER is there now.
///
/// Called when the window notices its subscriptions have ended, which
/// is what a quit REAPER looks like from this side. It repeats the
/// discovery rather than the path it found last time, because the pid
/// in that path is exactly what changed.
///
/// # Errors
///
/// When this window owns a project rather than attaching to one, when
/// no REAPER was ever attached to, or when nothing answers — the last
/// of which is the ordinary case while REAPER is starting up, and is a
/// reason to try again rather than a reason to stop.
pub fn reattach() -> eyre::Result<Attached> {
    let socket = ATTACHED_TO
        .read()
        .ok()
        .and_then(|slot| slot.clone())
        .ok_or_else(|| eyre::eyre!("this window never attached to a REAPER"))?;
    attach_to_reaper(socket)
}

/// Is this window attached to a REAPER rather than owning a file?
#[must_use]
pub fn is_attached() -> bool {
    ATTACHED_TO.read().is_ok_and(|slot| slot.is_some())
}
