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
//! (`daw_ui::studio::project::fetch_when_ready`).
//!
//! # The audio mode
//!
//! Which of the two a window is, is its audio mode ([`AudioMode`], issue
//! #142), held here as process state and read everywhere through [`mode`]:
//! **Engine** owns (the in-process engine plays), **Remote** borrows (the
//! other system plays; nothing here does), **Cue** borrows with a local
//! click and guide. It is set once at launch ([`launch_mode`],
//! [`set_mode`]) and can climb at runtime (see [`crate::audio_mode`]).
//!
//! Everything that reaches the local engine rather than the facade goes
//! through here and says what it needs ([`with_local_engine`],
//! [`LocalOnly`]): in Remote it is skipped with one warning, never run
//! against an engine that holds nothing. What the facade CAN do in Remote
//! is done through it — the song on screen ([`current_song`]), switching
//! songs ([`switch_song`]), a song's colour ([`set_song_color`]).

pub use session_engine_client::open::*;

use std::path::Path;
use std::sync::{Arc, OnceLock};

use daw::standalone::Standalone;

pub use crate::open_core::*;

pub use crate::audio_mode::{
    Assets, AudioMode, ModeState, RemoteTarget, begin_asset_load, set_assets_progress,
    set_cue_ready,
};

/// Parse the project, stand up its backend, install the facade, and
/// start audio. Returns once the panels have something to read.
pub fn open_and_serve(path: &Path) -> eyre::Result<Opened> {
    let opened = open_with_audio(path, true, Media::All)?;
    serve_actions();
    Ok(opened)
}

/// [`open_and_serve`] (with `audio`) or [`open_silent`], choosing when the
/// media loads — deferred for a song streamed in, whose takes are attached
/// to its streams once it is open (`crate::stream_set`).
///
/// # Errors
///
/// As [`open_and_serve`].
pub fn open_first_with(path: &Path, audio: bool, media: Media) -> eyre::Result<Opened> {
    let opened = open_with_audio(path, audio, media)?;
    if audio {
        serve_actions();
    }
    Ok(opened)
}

/// The Action List served to a model and the CLI
/// (`crate::studio_engine::mcp`): from
/// the window's start (`session_desktop::native::prepare`), so a model or
/// the CLI can open the first song too, and once a song is open in a window
/// that plays — never for a renderer or a test, which open silent. Once per
/// process.
pub fn serve_actions() {
    #[cfg(feature = "native")]
    {
        static STARTED: OnceLock<()> = OnceLock::new();
        STARTED.get_or_init(|| {
            // The engine's runtime, made now if no song has made it yet:
            // the server runs on it.
            if let Err(error) = engine_runtime() {
                tracing::warn!(%error, "mcp: no runtime to serve on");
                return;
            }
            let _ = crate::studio_engine::mcp::start();
        });
    }
}

/// The same, without opening an audio device.
///
/// For anything that draws a project and exits — a renderer, a
/// screenshot, a fixture. It has nothing to play and no transport to
/// run, and standing up an engine costs it a dependency on a working
/// sound server that it does not otherwise have.
///
/// That is not hypothetical. `attach_audio` is written to treat failure
/// as harmless, and it is — but cpal does not always FAIL when a host
/// is unavailable; sometimes it enumerates, and enumeration can block
/// for minutes on a busy machine. Three render tests timed out at ten
/// minutes each on a box whose audio was working fine and merely busy,
/// which is a long way to travel from "the picture is wrong".
pub fn open_silent(path: &Path) -> eyre::Result<Opened> {
    open_with_audio(path, false, Media::All)
}

/// A new, empty song in the process's engine, made current: `name`, with
/// no file behind it until it is first saved. What New Project opens
/// (`crate::project_new`).
///
/// `audio` says whether the window plays, as the first open does; `None`
/// keeps what it is — a song made from inside the window plays if the
/// window does.
///
/// # Errors
///
/// The window drives another system and has no engine of its own, or the
/// engine would not make a project.
pub fn open_untitled(name: &str, audio: Option<bool>) -> eyre::Result<Opened> {
    let state = mode();
    if !state.owns_project() {
        eyre::bail!(
            "a new song is made in this window's own engine, and this window is in {} mode",
            state.requested.name()
        );
    }
    let daw = engine();
    // The facade and its runtime first: making a project spawns on it.
    bootstrap(daw)?;
    let _inside = engine_runtime()?.enter();
    let info = daw::service::Projects::create(daw)
        .ok_or_else(|| eyre::eyre!("the engine made no project"))?;
    daw.write_project(&info.guid, |p| name.clone_into(&mut p.info.name));
    if let Some(audio) = audio {
        AUDIBLE.store(audio, std::sync::atomic::Ordering::Relaxed);
    }
    let audio = AUDIBLE.load(std::sync::atomic::Ordering::Relaxed);
    switch_to(daw, &info.guid, audio);
    if audio {
        serve_actions();
    }
    Ok(Opened {
        daw: daw.clone(),
        name: name.to_owned(),
        project_guid: info.guid,
        track_count: 0,
    })
}

fn open_with_audio(path: &Path, audio: bool, media: Media) -> eyre::Result<Opened> {
    refuse_unless_local(path)?;
    let began = web_time::Instant::now();
    let opened = load_into_with(engine(), path, media)?;
    let loaded = began.elapsed();
    bootstrap(&opened.daw)?;
    let booted = began.elapsed();
    AUDIBLE.store(audio, std::sync::atomic::Ordering::Relaxed);
    switch_to(&opened.daw, &opened.project_guid, audio);
    tracing::info!(
        start.engine_ms = loaded.as_millis(),
        start.bootstrap_ms = (booted - loaded).as_millis(),
        start.switch_ms = (began.elapsed() - booted).as_millis(),
        start.audio = audio,
        "engine started"
    );
    Ok(opened)
}

/// Whether this window plays — set by the first open (a screenshot or a
/// test opens silent), and what [`switch_song`] follows.
static AUDIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Stand the process's engine up with no song open: the engine, its
/// runtime and the daw facade over it, playing — its device opens with the
/// first song (`crate::studio_engine::headless`, `studio engine`).
///
/// # Errors
///
/// The runtime or the facade could not be started.
pub fn start_engine() -> eyre::Result<Standalone> {
    crate::install();
    let daw = engine();
    bootstrap(daw)?;
    // `FTS_ENGINE_SILENT=1`: no audio device (a test, a renderer).
    let audible = std::env::var_os("FTS_ENGINE_SILENT").is_none_or(|v| v.is_empty() || v == "0");
    AUDIBLE.store(audible, std::sync::atomic::Ordering::Relaxed);
    Ok(daw.clone())
}

/// Load another song into the engine the first open made — for a
/// setlist, after the first song. It does not become current and nothing
/// plays it until [`switch_to`] says so.
///
/// # Errors
///
/// The file could not be read or parsed, or its media did not materialize.
pub fn open_another(path: &Path) -> eyre::Result<Opened> {
    open_another_with(path, Media::All)
}

/// [`open_another`], choosing when the media loads.
///
/// # Errors
///
/// As [`open_another`].
pub fn open_another_with(path: &Path, media: Media) -> eyre::Result<Opened> {
    refuse_unless_local(path)?;
    load_into_with(engine(), path, media)
}

/// Opening a file is standing it up in the local engine — which a Remote
/// or Cue window has none of: the system it drives owns the projects.
fn refuse_unless_local(path: &Path) -> eyre::Result<()> {
    let state = mode();
    if state.owns_project() {
        return Ok(());
    }
    let target = state
        .target
        .as_ref()
        .map_or_else(|| "another system".to_owned(), RemoteTarget::describe);
    eyre::bail!(
        "{} is not opened here: this window is in {} mode, driving {target}",
        path.display(),
        state.requested.name()
    )
}

/// Make `project_guid` the song the window shows and the transport plays:
/// the engine's current project, its meters, and — with `audio` — the
/// audio engine, moved from whichever song had it.
///
/// Moving the audio engine closes the device and opens it again on the
/// new song, a short gap. Between songs that is fine; a set that runs
/// straight on without one wants the engine to hand the stream over
/// instead, which daw-standalone does not do yet.
pub fn switch_to(daw: &Standalone, project_guid: &str, audio: bool) {
    daw.set_current_project(project_guid);
    let tracks = daw::service::Tracks::all(daw, daw::service::ProjectContext::Current).len();
    daw.set_meters(daw::standalone::metering::Meters::new(tracks));
    if audio {
        attach_audio(daw, project_guid);
    }
}

/// The song on screen, on this process's engine: [`switch_to`] it, with the
/// audio when this window plays — what `switch_song` does in Engine mode
/// (`session_engine_client::open::switch_song`, through the body).
pub fn switch_local(project_guid: &str) {
    switch_to(
        engine(),
        project_guid,
        AUDIBLE.load(std::sync::atomic::Ordering::Relaxed),
    );
}

/// The engine a Cue window plays its click and guide on: this process's
/// own — the one Engine mode plays a whole song on (Cue is its first
/// stage).
pub(crate) fn cue_engine() -> &'static Standalone {
    engine()
}

/// Colour a song's SONG region (`0xRRGGBB`; `0` clears it) and save the
/// song into `saved`, its `.session`, when it has one. A save that fails
/// is logged — the colour still holds for this run.
///
/// In Remote and Cue the colour goes through the facade's region service
/// (REAPER shows it on its region too); there is no `.session` to save —
/// the remote system saves its own project.
pub fn set_song_color(project_guid: &str, rgb: u32, saved: Option<&Path>) {
    let song_lane = session::ruler_lanes::CoreLane::Song.lane_index();
    if !crate::audio_mode::owns_project() {
        let coloured = facade_blocking("song colour", |daw| {
            let guid = project_guid.to_owned();
            async move {
                let regions = daw.project(guid).await?.regions();
                let region = regions
                    .all()
                    .await?
                    .into_iter()
                    .find(|r| r.lane == Some(song_lane));
                match region.and_then(|r| r.id) {
                    Some(id) => regions.set_color(id, rgb).await.map(|()| true),
                    None => Ok(false),
                }
            }
        });
        match coloured {
            Ok(true) => {}
            Ok(false) => tracing::warn!(song.project = project_guid, "no SONG region to colour"),
            Err(e) => tracing::warn!(error = %e, "the song's colour could not be set"),
        }
        if saved.is_some() {
            with_local_engine(LocalOnly::SessionSave, |_| ());
        }
        return;
    }
    use daw::service::Regions as _;
    let daw = engine();
    let project = daw::service::ProjectContext::Project(project_guid.to_owned());
    let region = daw
        .all(project.clone())
        .into_iter()
        .find(|r| r.lane == Some(song_lane));
    let Some(id) = region.and_then(|r| r.id) else {
        tracing::warn!(song.project = project_guid, "no SONG region to colour");
        return;
    };
    if let Err(e) = daw.set_color(project, id, rgb) {
        tracing::warn!(error = %e, "the song's colour could not be set");
        return;
    }
    if let Some(dir) = saved {
        crate::session_file::save_session_behind(daw, project_guid, dir);
    }
}

/// Step two: serve the backend and install the facade.
///
/// The runtime is leaked multi-threaded with 16 MiB worker stacks — vox's
/// debug-build channel encode recurses deeply enough to overflow tokio's
/// default 2 MiB, which presents as a stack overflow somewhere entirely
/// unrelated the first time a panel subscribes to anything.
fn bootstrap(standalone: &Standalone) -> eyre::Result<()> {
    crate::install();
    // Every CLAP the process's engine loads is offered Studio's actions
    // (host-actions/1), before its first song opens.
    if std::ptr::eq(standalone, engine()) {
        crate::clap_host::install(engine());
    }
    if BUNDLE.get().is_some() {
        return Ok(());
    }
    let rt = engine_runtime()?;

    let router = {
        let _inside = rt.enter();
        crate::studio_engine::view_host::router(standalone)
    };
    let scope = architect::Scope::new();
    let caller = rt.block_on(architect::LocalServer::serve(router, Arc::clone(&scope)).caller())?;
    let bundle = Served { caller, scope };
    let facade = daw::rpc::Daw::new(bundle.caller.clone());
    // A separate current-thread runtime for `daw::block_on` in sync
    // contexts, so it can never be entered from inside the engine
    // runtime's own workers.
    let block_on_rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(facade, block_on_rt);
    BUNDLE
        .set(bundle)
        .map_err(|_| eyre::eyre!("the daw facade was bootstrapped twice"))?;
    // Every finished edit step heard, whether or not the song is open from
    // its project file: what is made from the recording (the kit's
    // samples) follows each one, and a song open from its file records it.
    crate::project_file::record_steps();
    Ok(())
}

/// Step three: real playback.
///
/// The engine renders the project graph into the default output and
/// drives the transport clock sample-accurately. Leaked for the
/// process's life — dropping it stops the stream. Failure is not fatal:
/// the soft clock still moves the playhead, silently, which is enough to
/// read a session by.
/// The set's one audio engine ([`daw::standalone::audio_engine::SetAudio`]):
/// every song opened is a voice in it, each on its own transport, mixed in
/// one callback — a song left (another tab picked) plays on, and songs
/// hand over on the frame. Opened once, by the first song made audible;
/// `current` is the song on screen (its meters, its keys, the reference
/// hook), not what sounds.
static AUDIO: std::sync::Mutex<Option<daw::standalone::audio_engine::SetAudio>> =
    std::sync::Mutex::new(None);

/// Bumped by every [`attach_audio`]: the song made current is the last one
/// asked for, whichever thread gets there first.
static AUDIO_ASKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Make `project_guid` the current song of the set's engine — on a thread
/// of its own: opening the device the first time is the sound server's
/// time, not the window's (on a busy machine finding PipeWire took the
/// better part of a minute and a half). The song opens and draws now; the
/// transport plays once the device is there, and runs silent if it never
/// is.
fn attach_audio(daw: &Standalone, project_guid: &str) {
    let asked = AUDIO_ASKED.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    let (daw, project_guid) = (daw.clone(), project_guid.to_owned());
    let spawned = std::thread::Builder::new()
        .name("audio-attach".into())
        .spawn(move || attach_audio_now(&daw, &project_guid, asked));
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "no thread to open the audio device on; the transport will run silent");
    }
}

/// `project_guid` as a voice of the set's engine, when the window plays:
/// its renderer made and its snapshot built now, as it opens — so a song
/// opened behind the first can sound the moment it is picked, or before.
/// Nothing when nothing plays yet: the first [`attach_audio`] adds it.
pub fn sound(project_guid: &str) {
    if !AUDIBLE.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let slot = AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(audio) = slot.as_ref()
        && let Err(e) = audio.add_voice(project_guid)
    {
        tracing::warn!(song.project = project_guid, error = %e, "the song could not join the set's engine");
    }
}

/// Hand the set's audio from song `from` to song `to` on one frame: `to`
/// starts from its beginning at the moment `from` reaches `from_end`
/// seconds, and `from` stops there (`SetAudio::play_after`). The moment,
/// sync clock µs.
///
/// # Errors
///
/// No audio engine is open, either song is not a voice of it, or `from`
/// is not playing or already past `from_end`.
pub fn play_after(from: &str, to: &str, from_end: f64) -> Result<f64, String> {
    let slot = AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let audio = slot.as_ref().ok_or("no audio engine is open")?;
    audio
        .play_after(from, to, from_end)
        .map_err(|e| e.to_string())
}

/// Call off a hand-over [`play_after`] scheduled that has not landed (a
/// stop before the moment): `to` does not start by itself.
///
/// # Errors
///
/// No audio engine is open, or either song is not a voice of it.
pub fn call_off_handoff(from: &str, to: &str) -> Result<(), String> {
    let slot = AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let audio = slot.as_ref().ok_or("no audio engine is open")?;
    audio.call_off(from, to).map_err(|e| e.to_string())
}

/// The set's engine opened again: how a change to how it renders (the
/// render ahead) is taken up. Every song stays a voice; the current one
/// stays current. Nothing open, nothing to do.
pub(crate) fn restart_audio() {
    let playing = AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let Some(audio) = playing else {
        return;
    };
    let (voices, current) = (audio.voices(), audio.current());
    drop(audio);
    let Some(daw) = local_engine() else {
        return;
    };
    let mut slot = AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(audio) = open_set_audio(daw) else {
        return;
    };
    for guid in &voices {
        if let Err(e) = audio.add_voice(guid) {
            tracing::warn!(song.project = %guid, error = %e, "the song did not rejoin the set's engine");
        }
    }
    if let Some(guid) = current {
        make_current(&audio, &guid);
    }
    *slot = Some(audio);
}

/// The set's engine on the device, its health watched.
fn open_set_audio(daw: &Standalone) -> Option<daw::standalone::audio_engine::SetAudio> {
    let began = web_time::Instant::now();
    // Rendering ahead as kept, on the device kept (`audio_device`).
    let prefs = crate::audio_device::current().applied(crate::render_ahead::current().io_prefs());
    match daw.attach_set_audio_with_prefs(&prefs) {
        Ok(audio) => {
            tracing::info!(
                audio.attach_ms = began.elapsed().as_millis(),
                "audio device attached: the set's engine"
            );
            crate::audio_health::watch(daw.clone(), audio.stats(), audio.sample_rate());
            crate::perf_meter::watch(daw.clone(), audio.stats(), audio.sample_rate());
            Some(audio)
        }
        Err(e) => {
            tracing::warn!(error = %e, "no audio engine; the transport will run silent");
            None
        }
    }
}

/// `project_guid` current in `audio`: its meters and keys, and — a song
/// streamed in and heard by its reference — the reference mixed in after
/// its tracks.
fn make_current(audio: &daw::standalone::audio_engine::SetAudio, project_guid: &str) {
    if let Err(e) = audio.set_current(Some(project_guid)) {
        tracing::warn!(song.project = project_guid, error = %e, "the song could not be made current");
        return;
    }
    audio.set_aux_renderer(crate::reference_play::renderer(project_guid.to_owned()));
}

fn attach_audio_now(daw: &Standalone, project_guid: &str, asked: u64) {
    let Some(rt) = runtime() else {
        return;
    };
    let _guard = rt.enter();
    let mut slot = AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Another song asked since: it is the one to make current.
    if AUDIO_ASKED.load(std::sync::atomic::Ordering::Acquire) != asked {
        return;
    }
    if slot.is_none() {
        *slot = open_set_audio(daw);
        // Every song already open joins it: a set's songs open while the
        // device is still being found.
        if let Some(audio) = slot.as_ref() {
            for song in daw::service::Projects::list(daw) {
                if let Err(e) = audio.add_voice(&song.guid) {
                    tracing::warn!(song.project = %song.guid, error = %e, "the song could not join the set's engine");
                }
            }
        }
    }
    let Some(audio) = slot.as_ref() else {
        return;
    };
    if let Err(e) = audio.add_voice(project_guid) {
        tracing::warn!(song.project = project_guid, error = %e, "the song could not join the set's engine");
        return;
    }
    if audio.current().as_deref() != Some(project_guid) {
        make_current(audio, project_guid);
        tracing::info!(song.project = project_guid, "audio: the song is current");
    }
}

// ── Attaching to a live REAPER ───────────────────────────────────────
