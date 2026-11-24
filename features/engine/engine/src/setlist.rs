//! The set on the engine: opening a setlist's songs into the one engine,
//! attaching to a remote system's projects, a song's colour. The set
//! itself, as a window reads it, is `session_engine_client::setlist`.

pub use session_engine_client::setlist::*;

#[cfg(feature = "native")]
use crate::song_model::StudioSession;
#[cfg(feature = "native")]
use crate::song_open::OpenSession as _;

#[cfg(feature = "native")]
/// What a set does on the engine: opening its songs into it, attaching to a
/// remote system's, colouring one. Methods of [`Setlist`] before the window
/// half (`session_engine_client::setlist`) moved out of this crate.
pub trait SetlistOpen: Sized {
    /// Open every song into the one engine — each prepared once and saved
    /// as `.session` if it was not already (see `StudioSession::open`) —
    /// and make the first current, with the audio.
    ///
    /// A song that fails to open is left out, with the reason logged: one
    /// broken song folder should not take the rest of the set with it.
    ///
    /// # Errors
    ///
    /// None of the songs opened.
    fn open(paths: &[std::path::PathBuf]) -> eyre::Result<Self>;

    /// [`Self::open`], for songs some of which are streamed in rather than
    /// on this disk (`crate::stream_set`): each opens the same way, its
    /// media deferred and then attached to its streams.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    fn open_streamed(songs: Vec<Opening>) -> eyre::Result<Self>;

    /// Open one more song into the engine the set is in, behind the one on
    /// screen — it does not become current, and nothing on screen moves: a
    /// streamed set's songs after the first, arriving (see [`Arrival`]).
    ///
    /// # Errors
    ///
    /// The song did not open, or could not be read back.
    fn open_behind(opening: Opening) -> eyre::Result<Song>;

    /// [`Self::open`], with no audio device — for a test, or anything that
    /// looks at a set without playing it.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    fn open_silent(paths: &[std::path::PathBuf]) -> eyre::Result<Self>;

    /// The set a Remote or Cue window drives: attach to `target`, and make
    /// a song of every project it has open, in its order, its current one
    /// current.
    ///
    /// Each project is read the way an opened song is
    /// (`StudioSession::read_current`) — which reads the CURRENT project, so
    /// the remote's projects are selected one at a time while they are read
    /// and the one that was current is selected again at the end. A project
    /// saved on this machine gets its track kinds and chart from beside it;
    /// one that is not (another machine's — any reached over the network —
    /// or never saved) opens without.
    ///
    /// # Errors
    ///
    /// The attach failed, the remote's projects could not be listed, or
    /// none could be read.
    fn attach(target: &crate::open::RemoteTarget) -> eyre::Result<Self>;

    /// Give `index` a colour by hand — or, with `None`, give it back the
    /// colour its title makes. Set on its SONG region, where REAPER shows
    /// it too, and saved into its `.session`, so it is the song's colour
    /// from now on and not this window's.
    fn recolor(&mut self, index: usize, color: Option<String>);
}

#[cfg(feature = "native")]
impl SetlistOpen for Setlist {
    fn open(paths: &[std::path::PathBuf]) -> eyre::Result<Self> {
        open_with(paths.iter().cloned().map(Opening::from).collect(), true)
    }

    fn open_streamed(songs: Vec<Opening>) -> eyre::Result<Self> {
        open_with(songs, true)
    }

    fn open_behind(opening: Opening) -> eyre::Result<Song> {
        let _opening = Opening::counted();
        let Opening {
            path,
            title,
            streamed,
        } = opening;
        let engine = crate::open::local_engine()
            .ok_or_else(|| eyre::eyre!("this window has no engine of its own to open into"))?;
        let media = if streamed.is_some() {
            crate::open::Media::Deferred
        } else {
            crate::open::Media::All
        };
        // Its stages on its `project.load` (`stream_set`'s).
        let load = tracing::Span::current();
        let began = web_time::Instant::now();
        let step = tracing::info_span!("open.engine").entered();
        let prepare = crate::prepare::Prepare::for_song(&path);
        let (opened, plan) = crate::open::open_song_into_with(engine, &path, &prepare, media)?;
        drop(step);
        load.record("open.engine_ms", began.elapsed().as_secs_f64() * 1e3);
        let began = web_time::Instant::now();
        let step = tracing::info_span!("open.media").entered();
        let guid = opened.project_guid.clone();
        // A voice of the set's engine now: it can sound the moment it is
        // picked, or while another is on screen.
        crate::open::sound(&guid);
        if let Some(streamed) = streamed {
            crate::stream_set::stream(streamed, &guid);
        } else if let Some(folder) = path.parent() {
            // A song downloaded by its reference plays it from disk.
            crate::reference_play::hear_local(&guid, folder);
        }
        // A project file (`Song.session`) opens through its doc: it has no
        // text, and its track kinds are read from the engine instead.
        let rpp_text = if crate::project_file::is_project_file(&plan.open) {
            String::new()
        } else {
            crate::open::project_text(&plan.open)?.text
        };
        let chart_text = prepare
            .chart
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        drop(step);
        load.record("open.media_ms", began.elapsed().as_secs_f64() * 1e3);
        let began = web_time::Instant::now();
        let step = tracing::info_span!("open.read").entered();
        let mut session = crate::open::engine_runtime()?.block_on(StudioSession::read_project(
            &guid,
            &opened.name,
            &rpp_text,
            chart_text.as_deref(),
            false,
        ))?;
        drop(step);
        load.record("open.read_ms", began.elapsed().as_secs_f64() * 1e3);
        load.record("song.tracks", session.project.0.tracks.len());
        session.chart_file = prepare.chart.clone();
        crate::collab::song_opened(&guid);
        // Its waveforms behind it: the song is in the set now, its
        // pictures drawn as they are read.
        pictures_later(session.clone(), guid.clone());
        Ok(song_at(&path, title, guid, session))
    }

    fn open_silent(paths: &[std::path::PathBuf]) -> eyre::Result<Self> {
        open_with(paths.iter().cloned().map(Opening::from).collect(), false)
    }

    fn attach(target: &crate::open::RemoteTarget) -> eyre::Result<Self> {
        let attached = crate::open::attach(target)?;
        // A project's path means this machine's disk only when the system
        // driven is on it: a local REAPER over its socket. A Session engine
        // or a REAPER's bridge is reached over the network, so its paths
        // are another machine's — read here they are at best missing and at
        // worst a folder the OS guards (a phone simulator on a Mac stalls on
        // a privacy prompt for its owner's Downloads).
        let local = matches!(
            target,
            crate::open::RemoteTarget::Reaper { .. } | crate::open::RemoteTarget::Engine { .. }
        );
        let listed = crate::open::facade_blocking("list projects", |daw| async move {
            let mut out = Vec::new();
            for project in daw.projects().await? {
                let info = project.info().await?;
                out.push(RemoteProject {
                    guid: info.guid,
                    name: info.name,
                    path: Some(std::path::PathBuf::from(info.path))
                        .filter(|p| local && !p.as_os_str().is_empty()),
                });
            }
            Ok(out)
        })?;
        let (projects, _) = remote_songs(listed, &attached.project_guid);
        let mut selected = attached.project_guid.clone();
        let mut songs = Vec::new();
        for project in projects {
            if project.guid != selected {
                let guid = project.guid.clone();
                if let Err(e) = crate::open::facade_blocking("select project", |daw| async move {
                    daw.select_project(guid).await.map(|_| ())
                }) {
                    tracing::error!(error = %e, "a remote project could not be selected to read; the set goes on without it");
                    continue;
                }
                selected.clone_from(&project.guid);
            }
            let chart = project
                .path
                .as_deref()
                .and_then(crate::prepare::chart_beside);
            match StudioSession::read_current(project.path.as_deref(), chart) {
                Ok(session) => songs.push(Song::of(project.song_name(), project.guid, session)),
                Err(e) => {
                    tracing::error!(error = %e, "a remote project could not be read; the set goes on without it")
                }
            }
        }
        if selected != attached.project_guid {
            let guid = attached.project_guid.clone();
            if let Err(e) = crate::open::facade_blocking("select project", |daw| async move {
                daw.select_project(guid).await.map(|_| ())
            }) {
                tracing::warn!(error = %e, "the remote's current project could not be selected again");
            }
        }
        if songs.is_empty() {
            eyre::bail!("none of the remote's projects could be read");
        }
        let at = songs
            .iter()
            .position(|s| s.project == attached.project_guid)
            .unwrap_or(0);
        crate::open::switch_song(&songs[at].project);
        Ok(Self {
            songs,
            at,
            pending: Vec::new(),
            order: Vec::new(),
        })
    }

    fn recolor(&mut self, index: usize, color: Option<String>) {
        let Some(song) = self.songs.get_mut(index) else {
            return;
        };
        let rgb = color.as_deref().and_then(hex_rgb);
        song.color = color
            .filter(|_| rgb.is_some())
            .unwrap_or_else(|| title_color(&song.name));
        crate::open::set_song_color(&song.project, rgb.unwrap_or(0), song.saved.as_deref());
    }
}

#[cfg(feature = "native")]
fn open_with(openings: Vec<Opening>, audio: bool) -> eyre::Result<Setlist> {
    let count = openings.len();
    // On the set's `set.open` (`studio_engine::set::open_with`), when
    // this is one; each song its own `project.load` under it.
    let set = tracing::Span::current();
    set.record("set.songs", count);
    let mut songs = Vec::new();
    for (index, opening) in openings.into_iter().enumerate() {
        if let Some(song) = load(opening, index, songs.is_empty().then_some(audio)) {
            songs.push(song);
        }
    }
    set.record("set.opened", songs.len());
    let first = songs
        .first()
        .ok_or_else(|| eyre::eyre!("none of the {count} songs opened"))?;
    crate::open::switch_song(&first.project);
    Ok(Setlist::of(songs))
}

#[cfg(feature = "native")]
/// One song of a set, loaded: opened in the engine (`song.open`) and
/// its media attached — streamed, or played from disk — so it can play.
/// `first` is `Some(audio)` for the song that stands the engine up.
/// `None` when it did not open (the set goes on without it).
fn load(opening: Opening, index: usize, first: Option<bool>) -> Option<Song> {
    let Opening {
        path,
        title,
        streamed,
    } = opening;
    let load = tracing::info_span!(
        "project.load",
        song.index = index,
        song.name = title.as_deref().unwrap_or_default(),
        song.media = if streamed.is_some() {
            "streamed"
        } else {
            "disk"
        },
        song.tracks = tracing::field::Empty,
        song.items = tracing::field::Empty,
    );
    let _load = load.enter();
    let prepare = crate::prepare::Prepare::for_song(&path);
    let media = if streamed.is_some() {
        crate::open::Media::Deferred
    } else {
        crate::open::Media::All
    };
    match StudioSession::open_first_as(&path, &prepare, first, media) {
        Ok((session, guid)) => {
            load.record("song.tracks", session.project.0.tracks.len());
            load.record(
                "song.items",
                session.project.0.items.values().flatten().count(),
            );
            if let Some(streamed) = streamed {
                crate::stream_set::stream(streamed, &guid);
            } else if let Some(folder) = path.parent() {
                // A song downloaded by its reference plays it from disk.
                crate::reference_play::hear_local(&guid, folder);
            }
            Some(song_at(&path, title, guid, session))
        }
        Err(e) => {
            tracing::error!(
                setlist.song = %path.display(),
                error = %e,
                "a song did not open; the set goes on without it"
            );
            None
        }
    }
}

#[cfg(feature = "native")]
/// The song opened from `path` into the engine as `project`: named by
/// `title`, or its file's; saved into its `.session` (a project file,
/// `crate::project_file`), when it has one.
fn song_at(
    path: &std::path::Path,
    title: Option<String>,
    project: String,
    session: StudioSession,
) -> Song {
    let name = title.unwrap_or_else(|| {
        path.file_stem()
            .map_or_else(|| path.to_string_lossy(), |s| s.to_string_lossy())
            .into_owned()
    });
    let mut song = Song::of(name, project, session);
    song.saved = Some(if crate::open::is_session(path) {
        path.to_path_buf()
    } else {
        path.with_extension("session")
    })
    .filter(|saved| saved.exists());
    song
}

#[cfg(feature = "native")]
/// Songs opening behind the one on screen, right now: the pictures wait
/// for none to be ([`pictures_later`]).
#[cfg(feature = "native")]
static OPENING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(feature = "native")]
impl Opening {
    /// One more song opening, until the guard drops.
    fn counted() -> OpeningGuard {
        OPENING.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        OpeningGuard
    }
}

#[cfg(feature = "native")]
/// A song opening ([`OPENING`]), counted while it lives.
#[cfg(feature = "native")]
struct OpeningGuard;

#[cfg(feature = "native")]
impl Drop for OpeningGuard {
    fn drop(&mut self) {
        OPENING.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg(feature = "native")]
/// The songs whose pictures are still to read, oldest first.
#[cfg(feature = "native")]
static PICTURES: std::sync::Mutex<std::collections::VecDeque<(StudioSession, String)>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

#[cfg(feature = "native")]
/// Forget the pictures still to read: a new set is opening, and the songs
/// it replaces are not worth reading.
#[cfg(feature = "native")]
pub fn forget_pictures() {
    if let Ok(mut queue) = PICTURES.lock() {
        queue.clear();
    }
}

#[cfg(feature = "native")]
/// Read `session`'s pictures (notes, waveforms) later: one song at a time,
/// on a thread of their own, and only while no song is opening — they go
/// through the engine the opens need, and read beside them they made every
/// song after the first wait seconds.
#[cfg(feature = "native")]
fn pictures_later(session: StudioSession, project: String) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RUNNING: AtomicBool = AtomicBool::new(false);
    if let Ok(mut queue) = PICTURES.lock() {
        queue.push_back((session, project));
    }
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("set-pictures".into())
        .spawn(|| {
            let Ok(runtime) = crate::open::engine_runtime() else {
                RUNNING.store(false, Ordering::Release);
                return;
            };
            loop {
                while OPENING.load(Ordering::Acquire) > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let next = PICTURES.lock().ok().and_then(|mut q| q.pop_front());
                let Some((session, project)) = next else {
                    RUNNING.store(false, Ordering::Release);
                    // One queued as this let go: take it up again.
                    if PICTURES.lock().is_ok_and(|q| !q.is_empty())
                        && !RUNNING.swap(true, Ordering::AcqRel)
                    {
                        continue;
                    }
                    return;
                };
                runtime.block_on(session.fill_previews(&project));
            }
        });
    if spawned.is_err() {
        RUNNING.store(false, Ordering::Release);
        tracing::warn!("set: no thread to read the songs' pictures on");
    }
}

#[cfg(feature = "native")]
/// A song for a set to open: its project — and, when it is streamed in
/// rather than on this disk, the stream its media plays from (the project
/// is then in the stream's mirror) and the title the set gives it.
#[cfg(feature = "native")]
pub struct Opening {
    pub path: std::path::PathBuf,
    pub title: Option<String>,
    pub streamed: Option<crate::song_stream::StreamedSong>,
}

#[cfg(feature = "native")]
impl From<std::path::PathBuf> for Opening {
    fn from(path: std::path::PathBuf) -> Self {
        Self {
            path,
            title: None,
            streamed: None,
        }
    }
}

#[cfg(feature = "native")]
/// `#rrggbb` as `0xRRGGBB`.
#[cfg(feature = "native")]
fn hex_rgb(css: &str) -> Option<u32> {
    let hex = css.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}
