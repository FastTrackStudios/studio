//! A song streamed in from elsewhere, when it is not on this machine —
//! from the engine this window drives, or from the Task library.
//!
//! One way, whatever the source ([`SongSource`]) and wherever it runs: the
//! small files — the prepared `.session`, the chart, the lyrics, the
//! waveform caches, each proxy's page index — are mirrored ([`Keep`]: a
//! cache on disk natively, memory in a browser), so the song opens the one
//! way songs open (`open_core::open_song_in`, through the mirror's
//! [`crate::folder::Folder`]); the proxies are not: each is attached as a
//! take that plays what has arrived (`attach_remote_ogg` over a
//! `SparseBytes`), and one fetcher per song brings their bytes in the order
//! they will be heard (`media_fetch::drive`) by range-reads on the source.
//! The originals stay where they are.
//!
//! The mirror and the streamed song ([`Keep`], [`mirror`], [`StreamedSong`])
//! are `session_runtime::song_stream`, re-exported here; [`mirror`] here
//! gives a song this window's runtime to fetch on. What stays needs the
//! engine:
//!
//! - [`PeerSource`] — the engine's song folder (`daw::rpc::SongFiles`).
//! - [`TaskSource`] — the song's session File Root in the library
//!   (`session::library::TaskSong`); its library chart replaces the folder's.
//! - [`ShareSource`] — a Task share link to the song's session (the public
//!   demo: no account): its JSON listing, its documents whole, its proxies
//!   as the link's audio rendition with HTTP ranges.

use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::Ordering;

pub use session_runtime::listen::{Heard, Listen, WarmStep, warm_order};
pub use session_runtime::song_stream::*;
pub use session_runtime::source::{
    Listing, Pending, ShareSource, SongSource, batch_records, project_first,
};

/// Mirror `source`'s song, as `session_runtime::song_stream::mirror`
/// does, its fetcher run on this window's runtime (when it has one).
///
/// # Errors
///
/// As `session_runtime::song_stream::mirror`.
pub async fn mirror(source: Arc<dyn SongSource>, keep: Keep) -> eyre::Result<StreamedSong> {
    let song = session_runtime::song_stream::mirror(source, keep).await?;
    #[cfg(feature = "native")]
    let song = song.with_spawn(Arc::new(|fetching| {
        if let Some(runtime) = crate::open::runtime() {
            runtime.spawn(fetching);
        }
    }));
    Ok(song)
}

/// The engine this window drives: its project's song folder.
#[cfg(feature = "native")]
pub struct PeerSource {
    remote: String,
    files: daw::rpc::SongFiles,
}

#[cfg(feature = "native")]
impl PeerSource {
    /// The song folder of `remote` (a project on the facade's engine).
    ///
    /// # Errors
    ///
    /// No facade, or no such project.
    pub async fn new(remote: &str) -> eyre::Result<Self> {
        let daw = daw::rpc::Daw::try_get().ok_or_else(|| eyre::eyre!("no facade"))?;
        Ok(Self {
            remote: remote.to_owned(),
            files: daw.project(remote).await?.song_files(),
        })
    }
}

#[cfg(feature = "native")]
impl SongSource for PeerSource {
    fn label(&self) -> String {
        format!("peer-{}", self.remote)
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let files = self.files.clone();
        Box::pin(async move {
            Ok(files
                .list()
                .await?
                .into_iter()
                .map(|f| (f.path, f.size))
                .collect::<Vec<_>>()
                .into())
        })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let files = self.files.clone();
        Box::pin(async move { files.read(&path, range).await.map_err(|e| e.to_string()) })
    }
}

/// A song in the Task library: its session File Root, and its chart.
#[cfg(feature = "native")]
pub struct TaskSource {
    library: session::library::Library,
    song: session::library::TaskSong,
}

#[cfg(feature = "native")]
impl TaskSource {
    /// The library song `slug` (`washed`), from the library the
    /// environment names (`FTS_TASK_SERVER`, `FTS_TASK_ORG`, the token).
    ///
    /// # Errors
    ///
    /// The library cannot be reached, or has no session for the song.
    pub async fn new(slug: &str) -> eyre::Result<Self> {
        Self::of(session::library::Library::from_env(), slug).await
    }

    /// The song `slug` in `library` — one a window signed in to.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub async fn of(library: session::library::Library, slug: &str) -> eyre::Result<Self> {
        let song = library.song(slug).await?;
        Ok(Self { library, song })
    }
}

#[cfg(feature = "native")]
impl SongSource for TaskSource {
    fn label(&self) -> String {
        format!("task-{}", self.song.slug)
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        // The session's own charts give way to the library's.
        let entries: Vec<(String, u64)> = self
            .song
            .entries
            .iter()
            .filter(|(rel, _)| rel.contains('/') || !rel.to_lowercase().ends_with(".kf"))
            .cloned()
            .collect();
        let version = self.song.version.clone();
        Box::pin(async move { Ok(Listing { entries, version }) })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let song = self.song.clone();
        Box::pin(async move { song.read(&path, range).await.map_err(|e| e.to_string()) })
    }

    fn chart(&self) -> Pending<eyre::Result<Option<String>>> {
        let library = self.library.clone();
        let slug = self.song.slug.clone();
        Box::pin(async move { library.song_chart(&slug).await })
    }

    /// The song's small files in one request (the server's
    /// `files/{root}/docs`) — its session's hundreds of objects and its
    /// waveforms, which read one by one took 19 s a song. `None` when the
    /// server has no such request, or it failed: they are read one by one.
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let url = self.song.docs_url();
        let token = self.song.token().map(str::to_owned);
        Box::pin(async move {
            let mut request = reqwest::Client::new()
                .post(url)
                .header(reqwest::header::CONTENT_TYPE, "text/plain")
                .body(paths.join("\n"));
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            let response = request.send().await.ok()?;
            if !response.status().is_success() {
                tracing::warn!(
                    http.status = response.status().as_u16(),
                    "library: the documents batch was refused; reading them one by one"
                );
                return None;
            }
            let body = response.bytes().await.ok()?;
            Some(
                batch_records(&body)?
                    .into_iter()
                    .map(|(path, status, bytes)| (path, (status == 0).then_some(bytes)))
                    .collect(),
            )
        })
    }

    fn fetch_whole(
        &self,
        path: String,
        into: PathBuf,
        got: Arc<std::sync::atomic::AtomicU64>,
    ) -> Pending<Option<eyre::Result<()>>> {
        let song = self.song.clone();
        Box::pin(async move {
            Some(
                async {
                    use std::io::Write as _;
                    let mut file = std::io::BufWriter::new(std::fs::File::create(&into)?);
                    let mut failed = None;
                    song.read_to(&path, |piece| {
                        if failed.is_some() {
                            return;
                        }
                        match file.write_all(piece) {
                            Ok(()) => {
                                got.fetch_add(
                                    piece.len() as u64,
                                    std::sync::atomic::Ordering::Relaxed,
                                );
                            }
                            Err(e) => failed = Some(e),
                        }
                    })
                    .await?;
                    if let Some(e) = failed {
                        return Err(e.into());
                    }
                    file.flush()?;
                    Ok(())
                }
                .await,
            )
        })
    }
}

/// Bring a song onto this device, to open with no connection: its small
/// files mirrored into `root` (as [`mirror`] does, the source's chart
/// written in), and its proxies fetched whole beside them — every one, or
/// only its reference and guide (`offline`). The originals are never
/// fetched: a song opened from disk plays `Media/Proxies/<name>.ogg` where
/// its original is missing (the engine's `ProjectRelativeResolver`). A
/// file already here at its size is kept. `progress` hears the proxies'
/// bytes: fetched so far, and in all. Returns the song's folder.
///
/// # Errors
///
/// The source lists nothing, or a file could not be fetched or written.
#[cfg(not(target_arch = "wasm32"))]
pub async fn download(
    source: Arc<dyn SongSource>,
    root: &Path,
    offline: Offline,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> eyre::Result<PathBuf> {
    use futures_util::StreamExt as _;
    /// Proxies fetched at once: each is its own request, so a few side by
    /// side keep the connection full while one waits on the server.
    const AT_ONCE: usize = 4;
    /// Tries at one proxy before the song is given up on: a phone's
    /// connection drops a request now and then.
    const TRIES: u32 = 4;
    let keep = Keep::Disk(root.to_path_buf());
    let song = mirror(Arc::clone(&source), keep.clone()).await?;
    // By its reference: the reference, its preview, and the guide's own
    // proxies — or, a song with no reference, all of them.
    let only: Option<std::collections::HashSet<String>> = match offline {
        Offline::Reference if song.has_reference() => Some(reference_stems(song.base())?),
        _ => None,
    };
    let marker = song
        .base()
        .join("Media")
        .join("Proxies")
        .join(REFERENCE_ONLY);
    match &only {
        Some(_) => {
            if let Some(dir) = marker.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(
                &marker,
                "The stems of this song are not on this device: it plays its reference.\n",
            )?;
        }
        None => {
            let _ = std::fs::remove_file(&marker);
        }
    }
    let mut proxies: Vec<(String, u64)> = song
        .proxies()
        .iter()
        .filter(|(stem, _)| {
            only.as_ref()
                .is_none_or(|only| only.contains(stem.as_str()))
        })
        .map(|(_, proxy)| proxy.clone())
        .collect();
    proxies.sort();
    let total: u64 = proxies.iter().map(|(_, size)| size).sum();
    // What has arrived: the proxies already here, and each one being
    // fetched by its own count (a failed try zeroes its own, not the rest).
    let mut here = 0;
    let mut wanted = Vec::new();
    for (path, size) in proxies {
        let local = song.base().join(&path);
        if keep.has(&local, size) {
            here += size;
        } else {
            wanted.push((
                path,
                size,
                local,
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
            ));
        }
    }
    let counts: Vec<Arc<std::sync::atomic::AtomicU64>> =
        wanted.iter().map(|(.., got)| Arc::clone(got)).collect();
    let done = || {
        (here
            + counts
                .iter()
                .map(|c| c.load(Ordering::Relaxed))
                .sum::<u64>())
        .min(total)
    };
    progress(done(), total);
    let fetching = futures_util::stream::iter(wanted)
        .map(|(path, size, local, got)| {
            let source = Arc::clone(&source);
            async move { fetch_proxy(&*source, &path, size, &local, &got, TRIES).await }
        })
        .buffer_unordered(AT_ONCE)
        .collect::<Vec<eyre::Result<()>>>();
    tokio::pin!(fetching);
    let outcomes = loop {
        tokio::select! {
            outcomes = &mut fetching => break outcomes,
            () = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                progress(done(), total);
            }
        }
    };
    progress(done(), total);
    for outcome in outcomes {
        outcome?;
    }
    Ok(song.base().to_path_buf())
}

/// The proxies a song mirrored into `folder` is heard by, by its
/// reference: the reference and its preview, and the proxies of the tracks
/// the reference leaves out (the guide) — lower-case file stems, as a
/// streamed song keys its proxies. Found by opening the song, its media
/// left unloaded, into an engine of its own.
///
/// # Errors
///
/// The song could not be found in `folder` or opened.
#[cfg(feature = "native")]
fn reference_stems(folder: &Path) -> eyre::Result<std::collections::HashSet<String>> {
    let song = crate::setlist::song_in(folder)
        .ok_or_else(|| eyre::eyre!("no song in {}", folder.display()))?;
    let daw = daw::standalone::Standalone::new();
    let prepare = crate::prepare::Prepare::for_song(&song);
    let (opened, _) =
        crate::open::open_song_into_with(&daw, &song, &prepare, crate::open::Media::Deferred)?;
    let project = opened.project_guid;
    let live = crate::reference::left_out(&daw, &project);
    let mut stems: std::collections::HashSet<String> =
        [crate::reference::STEM, crate::reference::PREVIEW_STEM]
            .into_iter()
            .map(str::to_owned)
            .collect();
    stems.extend(
        daw::standalone::audio_engine::materialize::pending_media(&daw, &project)
            .into_iter()
            .filter(|m| live.contains(&m.track_guid))
            .filter_map(|m| {
                Path::new(&m.path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_lowercase())
            }),
    );
    Ok(stems)
}

/// The files that play in a song (each item's active take, on a lane that
/// sounds), each once, first heard first: the media a song needs before
/// anything else — what to make proxies of and fetch first. `song` is a
/// project file (`Song.session`, opened through its doc), an `.RPP`, or a
/// folder (its song, as [`crate::setlist::song_in`] finds it). Relative to
/// the song's folder when they are under it.
///
/// # Errors
///
/// No song there, or it would not open.
#[cfg(feature = "native")]
pub fn playing_files(song: &Path) -> eyre::Result<Vec<String>> {
    let daw = daw::standalone::Standalone::new();
    let (project, folder) = if crate::project_file::is_project_file(song) {
        let name = song
            .file_stem()
            .map_or_else(|| "Song".to_owned(), |s| s.to_string_lossy().into_owned());
        let guid = crate::project_file::load(&daw, &name, song, session_format::store::MAIN)
            .map_err(|e| eyre::eyre!("{}: {e}", song.display()))?;
        (
            guid,
            song.parent().map(Path::to_path_buf).unwrap_or_default(),
        )
    } else {
        let folder = if song.is_dir() {
            song.to_path_buf()
        } else {
            song.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        let found = if song.is_dir() {
            crate::setlist::song_in(song)
        } else {
            crate::setlist::song_in(&folder)
        }
        .ok_or_else(|| eyre::eyre!("no song in {}", song.display()))?;
        let prepare = crate::prepare::Prepare::for_song(&found);
        let (opened, _) =
            crate::open::open_song_into_with(&daw, &found, &prepare, crate::open::Media::Deferred)?;
        (opened.project_guid, folder)
    };
    let media = daw::standalone::audio_engine::materialize::playing_media(&daw, &project);
    let base = folder.canonicalize().unwrap_or(folder);
    Ok(
        daw::standalone::audio_engine::materialize::files_first_heard(&media)
            .into_iter()
            .map(|f| {
                Path::new(&f)
                    .strip_prefix(&base)
                    .map_or(f.clone(), |rel| rel.to_string_lossy().into_owned())
            })
            .collect(),
    )
}

/// Without the engine to open a song in, a song downloaded by its
/// reference comes down whole.
#[cfg(not(feature = "native"))]
fn reference_stems(_folder: &Path) -> eyre::Result<std::collections::HashSet<String>> {
    eyre::bail!("no engine here to find a song's guide in")
}
