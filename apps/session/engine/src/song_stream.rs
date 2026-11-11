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
//! - [`PeerSource`] — the engine's song folder (`daw::rpc::SongFiles`).
//! - [`TaskSource`] — the song's session File Root in the library
//!   (`session::library::TaskSong`); its library chart replaces the folder's.
//! - [`ShareSource`] — a Task share link to the song's session (the public
//!   demo: no account): its JSON listing, its documents whole, its proxies
//!   as the link's audio rendition with HTTP ranges.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use daw::standalone::audio_engine::materialize::PendingMedia;
use daw::standalone::audio_engine::media_fetch::{FetchConfig, Fetching, RangeFetch, StreamedTake};
use daw::standalone::sync::Standalone;

use crate::folder::{Folder, Memory};
use fts_sample::ogg_index::OggIndex;
use fts_sample::sparse::SparseBytes;
pub use session_runtime::listen::{Heard, Listen, WarmStep, warm_order};
pub use session_runtime::source::{
    Listing, Pending, ShareSource, SongSource, batch_records, project_first,
};

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

/// Where a mirrored song's small files are kept.
#[derive(Clone, Debug)]
pub enum Keep {
    /// A cache folder on this machine (under it, one folder per source),
    /// its proxies' bytes in `.part` files beside them.
    #[cfg(not(target_arch = "wasm32"))]
    Disk(PathBuf),
    /// Memory — a browser's; the proxies' bytes too.
    Memory(Memory),
}

impl Keep {
    /// The folder the mirrored files are read back from.
    fn folder(&self) -> Arc<dyn Folder> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => Arc::new(crate::folder::Disk),
            Self::Memory(memory) => Arc::new(memory.clone()),
        }
    }

    /// Whether a song-folder file (lower-case path) is mirrored here. A
    /// browser skips the waveform caches: its peak store reads them by
    /// disk path, which it has none of, so held in memory they would only
    /// be weight.
    fn wants(&self, lower: &str) -> bool {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => true,
            Self::Memory(_) => !lower.starts_with("media/peaks/"),
        }
    }

    /// How the fetcher holds what it brings: all of it on disk; in memory,
    /// a window around the playhead (the rest is in the browser's cache).
    fn fetch_config(&self) -> FetchConfig {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            // Kept on disk, whole: many requests in flight, each as large as
            // a request goes. A request costs about the same however small
            // (a third of a second to a local Task), and a song plays a few
            // hundred streams at once — four small ones at a time could not
            // keep up with Unbreakable's 185 (≈3 MB/s needed).
            Self::Disk(_) => FetchConfig {
                concurrency: 32,
                max_request: 1024 * 1024,
                ..FetchConfig::default()
            },
            Self::Memory(_) => FetchConfig {
                resident: Some(daw::standalone::audio_engine::media_fetch::Resident::BROWSER),
                // A seek needs every stem's block at the new place at once,
                // and over the internet each one is a round trip: more of
                // them in flight is what gets the song sounding again.
                concurrency: 8,
                ..FetchConfig::default()
            },
        }
    }

    /// Where a source labelled `label` is mirrored.
    fn base(&self, label: &str) -> PathBuf {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(root) => root.join(safe_name(label)),
            Self::Memory(_) => Path::new("/").join(safe_name(label)),
        }
    }

    /// Whether what is kept under `base` was mirrored at `version`. A file
    /// kept at the right size is only the same file at the same version: a
    /// `.session` is SQLite, whole pages, and a new one is often the old
    /// one's size (JT's songs, re-split, kept their old names). A source
    /// that names no version, and memory (fresh each run), are trusted.
    fn at_version(&self, base: &Path, version: Option<&str>) -> bool {
        match (self, version) {
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Disk(_), Some(version)) => {
                std::fs::read_to_string(base.join(VERSION_STAMP)).is_ok_and(|v| v == version)
            }
            _ => true,
        }
    }

    /// Note that what is kept under `base` is now at `version`.
    fn mirrored_at(&self, base: &Path, version: Option<&str>) -> eyre::Result<()> {
        match (self, version) {
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Disk(_), Some(version)) => self.write(&base.join(VERSION_STAMP), version.into()),
            _ => Ok(()),
        }
    }

    /// A project file about to be replaced: the log it was opened with
    /// here (`-wal`, `-shm`, `-tshm`) goes first. Left, the new file opened
    /// with the old one's log replayed over it — JT's songs, re-split,
    /// still showed the show's name. What was edited here has synced
    /// through the song's doc already; the file is only its copy.
    fn forget_log(&self, path: &Path) -> eyre::Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Self::Disk(_) = self
            && path.extension().is_some_and(|e| e == "session")
        {
            for sidecar in ["session-wal", "session-shm", "session-tshm"] {
                let log = path.with_extension(sidecar);
                if log.exists() {
                    std::fs::remove_file(&log)?;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = path;
        Ok(())
    }

    /// Whether `path` is already here at `size` bytes.
    fn has(&self, path: &Path, size: u64) -> bool {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => std::fs::metadata(path).is_ok_and(|m| m.len() == size),
            Self::Memory(memory) => memory.len_of(path) == Some(size),
        }
    }

    fn write(&self, path: &Path, bytes: Vec<u8>) -> eyre::Result<()> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, bytes)?;
            }
            Self::Memory(memory) => memory.insert(path, bytes),
        }
        Ok(())
    }

    /// Room for the `size` bytes of the proxy at `path`, as they arrive.
    fn sparse(&self, path: &Path, size: u64) -> Option<Arc<SparseBytes>> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Disk(_) => {
                let store = PathBuf::from(format!("{}.part", path.display()));
                std::fs::create_dir_all(store.parent()?).ok()?;
                SparseBytes::on_disk(size, &store).ok()
            }
            Self::Memory(_) => {
                let _ = path;
                Some(SparseBytes::in_memory(size))
            }
        }
    }
}

/// A song mirrored from a source: where it opens, and its proxies to
/// stream.
pub struct StreamedSong {
    /// The project to open (in the mirror).
    pub project: PathBuf,
    /// The mirror it opens from.
    pub folder: Arc<dyn Folder>,
    keep: Keep,
    base: PathBuf,
    source: Arc<dyn SongSource>,
    /// Proxies by their file stem, lower-case: (path in the song, size).
    proxies: HashMap<String, (String, u64)>,
    /// The original recordings a take can move to at full resolution
    /// ([`crate::stream_quality`]), by file name, lower-case (`bass.wav`):
    /// (path in the song, size). Only what the engine maps from disk (WAV).
    originals: HashMap<String, (String, u64)>,
    /// Waveform caches by their original's file name, lower-case
    /// (`bass.wav`): (path in the song, size). Ours before REAPER's.
    peaks: HashMap<String, (String, u64)>,
    /// The streams attached so far, by their original's file name.
    attached: Mutex<Vec<(String, daw::standalone::audio_engine::streamed::Streamed)>>,
    /// Each proxy's bytes and page index, once: every item cut from one
    /// recording reads the same store (a song is thousands of items over a
    /// few hundred files — one store each would truncate the file the
    /// others write, and fetch every header thousands of times).
    opened: Mutex<HashMap<PathBuf, (Arc<SparseBytes>, OggIndex)>>,
    /// Recordings whose waveform could not be built here
    /// ([`Self::build_missing_peaks`]).
    #[cfg(feature = "web")]
    unbuildable: Mutex<std::collections::HashSet<String>>,
    /// How deep the host lets a listener go ([`Listen`]): capped at
    /// Reference, no stem of the song is fetched ([`Self::media`]).
    listen: Listen,
}

fn safe_name(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The recording a waveform cache in a song's file list is of, by its
/// lowercased file name, and whether it is ours (`.sessionpeaks`, which
/// wins over REAPER's `.reapeaks`): a `Peaks/` folder beside the
/// recordings, wherever the session keeps them — `Media/Peaks/` for a
/// prepared song, `Samples/Imported/Peaks/` for one cut from a Live set.
fn peaks_cache_of(lower: &str) -> Option<(&str, bool)> {
    let (dir, name) = lower.rsplit_once('/')?;
    if !(dir == "peaks" || dir.ends_with("/peaks")) {
        return None;
    }
    let (original, ext) = name.rsplit_once('.')?;
    match ext {
        "sessionpeaks" => Some((original, true)),
        "reapeaks" => Some((original, false)),
        _ => None,
    }
}

/// Whether a song-folder path is audio the fetcher streams (a proxy) or
/// leaves alone (an original): everything else is small, and mirrored.
fn is_media(lower: &str) -> bool {
    /// A recording, wherever the session keeps it: REAPER's `Media/`,
    /// Live's `Samples/Imported/`, a folder of its own. Kept by name, a
    /// split Live set's recordings were fetched whole as "small files" —
    /// 7.9 GB for a set that streams 308 MB of proxies.
    const RECORDINGS: &[&str] = &[
        ".wav", ".wave", ".aif", ".aiff", ".aifc", ".flac", ".mp3", ".m4a", ".aac", ".ogg",
        ".opus", ".caf", ".w64", ".wv", ".mp4", ".mov",
    ];
    if lower.starts_with("media/peaks/") || lower.ends_with(".ogg.idx") {
        return false;
    }
    lower.starts_with("media/") || RECORDINGS.iter().any(|ext| lower.ends_with(ext))
}

/// Whether a recording (lower-case path) is one the engine plays straight
/// from disk — a WAV, memory-mapped — and so worth fetching at full
/// resolution. Anything else stays on its proxy.
fn is_mapped_original(lower: &str) -> bool {
    !lower.contains("/proxies/") && (lower.ends_with(".wav") || lower.ends_with(".wave"))
}

#[cfg(test)]
mod media_tests {
    use super::{is_mapped_original, is_media, project_first};

    fn listed(paths: &[&str]) -> Vec<String> {
        let mut entries = paths.iter().map(|p| ((*p).to_owned(), 1)).collect();
        project_first(&mut entries);
        entries.into_iter().map(|(p, _)| p).collect()
    }

    #[test]
    fn the_project_file_is_listed_first_whatever_its_name() {
        assert_eq!(
            listed(&[
                "Media/Peaks/Reference.wav.sessionpeaks",
                "She's American.session-wal",
                "She's American.session",
                "Bounces/Extras/Outro.wav",
            ])[0],
            "She's American.session"
        );
        // A prepared song keeps its `.RPP` beside its project file: the
        // project file is what opens.
        assert_eq!(
            listed(&["Zed.RPP", "Zed.session", "Media/a.wav"])[0],
            "Zed.session"
        );
        assert_eq!(listed(&["Media/a.wav", "Song.RPP"])[0], "Song.RPP");
        // Only a top-level one: a song's bounces are not the song.
        assert_eq!(
            listed(&["Archive/Old.session", "Media/a.wav"]),
            ["Archive/Old.session", "Media/a.wav"]
        );
    }

    #[test]
    fn only_a_wav_beside_its_proxies_is_an_original_to_fetch() {
        assert!(is_mapped_original("samples/imported/bass.wav"));
        assert!(is_mapped_original("media/vox.wave"));
        assert!(!is_mapped_original("media/proxies/vox.ogg"));
        assert!(
            !is_mapped_original("media/vox.mp3"),
            "not mapped: its proxy is as good"
        );
        assert!(!is_mapped_original("media/proxies/odd.wav"));
    }

    #[test]
    fn a_waveform_cache_is_found_beside_its_recordings_wherever_they_are() {
        assert_eq!(
            super::peaks_cache_of("media/peaks/bass.wav.sessionpeaks"),
            Some(("bass.wav", true))
        );
        assert_eq!(
            super::peaks_cache_of("samples/imported/peaks/kick 1.wav.reapeaks"),
            Some(("kick 1.wav", false))
        );
        assert_eq!(super::peaks_cache_of("media/bass.wav"), None);
        assert_eq!(super::peaks_cache_of("media/peaks/notes.txt"), None);
        assert_eq!(super::peaks_cache_of("speaks/bass.wav.sessionpeaks"), None);
    }

    #[test]
    fn a_recording_streams_wherever_the_session_keeps_it() {
        assert!(is_media("media/bass.wav"));
        assert!(is_media("samples/imported/drums_live kit.wav"));
        assert!(is_media("audio/vox.aiff"));
        assert!(
            !is_media("samples/imported/proxies/vox.ogg.idx"),
            "a page index is small, fetched"
        );
        assert!(!is_media("media/peaks/bass.wav.sessionpeaks"));
        assert!(!is_media("fire.session"));
        assert!(!is_media("chart.kf"));
    }
}

/// What a download brings onto the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offline {
    /// The song's reference (its mix but the guide, one stereo proxy of a
    /// few megabytes, and its preview) and the tracks it leaves out — the
    /// guide — which play live over it: what a phone plays by default. A
    /// song with no reference comes down whole.
    Reference,
    /// Every proxy: each track of the song, to mix.
    Multitracks,
}

/// The file beside a song's proxies that says it came down by its
/// reference only ([`Offline::Reference`]): opened from disk, it is heard
/// by its reference ([`crate::reference_play::hear_local`]), its stems not
/// being there.
pub const REFERENCE_ONLY: &str = ".reference-only";

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
        Offline::Reference if song.has_reference() => Some(reference_stems(&song.base)?),
        _ => None,
    };
    let marker = song.base.join("Media").join("Proxies").join(REFERENCE_ONLY);
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
        .proxies
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
        let local = song.base.join(&path);
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
    Ok(song.base)
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

/// Fetch proxy `path` (`size` bytes) of `source` to `local`: streamed
/// whole where the source can, else read by ranges — written aside and
/// moved in whole, so a download cut short never leaves a proxy that looks
/// complete. `got` is this proxy's own count, each piece added as it lands;
/// a failed try sets it back to nothing and starts again, `tries` times at
/// most.
#[cfg(not(target_arch = "wasm32"))]
async fn fetch_proxy(
    source: &dyn SongSource,
    path: &str,
    size: u64,
    local: &Path,
    got: &Arc<std::sync::atomic::AtomicU64>,
    tries: u32,
) -> eyre::Result<()> {
    use std::io::Write as _;
    /// A range read, where the source cannot stream a file: well inside
    /// the library's per-read timeout on a slow connection.
    const RANGE: u64 = 1024 * 1024;
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = PathBuf::from(format!("{}.download", local.display()));
    let mut tried = 0;
    loop {
        tried += 1;
        got.store(0, Ordering::Relaxed);
        let outcome = match source
            .fetch_whole(path.to_owned(), part.clone(), Arc::clone(got))
            .await
        {
            Some(outcome) => outcome,
            None => {
                async {
                    let mut file = std::fs::File::create(&part)?;
                    let mut at = 0;
                    while at < size {
                        let end = (at + RANGE).min(size);
                        let bytes = source
                            .read(path.to_owned(), at..end)
                            .await
                            .map_err(|e| eyre::eyre!("{path}: {e}"))?;
                        if bytes.is_empty() {
                            eyre::bail!("{path}: the source sent nothing at {at} of {size}");
                        }
                        file.write_all(&bytes)?;
                        at += bytes.len() as u64;
                        got.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    }
                    file.flush()?;
                    Ok(())
                }
                .await
            }
        };
        match outcome {
            Ok(()) => {
                std::fs::rename(&part, local)?;
                return Ok(());
            }
            Err(e) => {
                got.store(0, Ordering::Relaxed);
                let _ = std::fs::remove_file(&part);
                if tried >= tries {
                    return Err(e);
                }
                tracing::warn!(download.path = %path, download.tried = tried, error = %e, "download: a proxy did not come down; trying again");
                architect::platform::sleep(std::time::Duration::from_millis(
                    500 * u64::from(tried),
                ))
                .await;
            }
        }
    }
}

/// Beside a mirrored song's files: the version of its source they are.
const VERSION_STAMP: &str = ".mirrored-version";

/// Mirror a song from `source` into `keep`: every small file fetched whole
/// (skipped when already there at its size), the media left to stream; the
/// source's own chart, if it keeps one, written in.
///
/// # Errors
///
/// The source lists nothing, or a file could not be fetched or kept.
pub async fn mirror(source: Arc<dyn SongSource>, keep: Keep) -> eyre::Result<StreamedSong> {
    use futures_util::{StreamExt as _, TryStreamExt as _};
    /// Files fetched at once: each is a round trip, and most are a few
    /// hundred bytes (a session's objects — one per MIDI item), so what
    /// they cost is latency, not bandwidth. At 8 a prepared song's 138
    /// objects took 17 round trips (four seconds in a browser); HTTP/2 and
    /// /3 carry this many on one connection.
    const AT_ONCE: usize = 32;
    let Listing {
        entries: list,
        version,
    } = source.list().await?;
    let first = list
        .first()
        .ok_or_else(|| eyre::eyre!("the song folder is empty"))?;
    let label = source.label();
    let base = keep.base(&label);
    let project = base.join(&first.0);
    // Kept files are only taken for the listed ones when they were kept at
    // this version; else each small file comes again.
    let current = keep.at_version(&base, version.as_deref());
    let mut proxies = HashMap::new();
    let mut originals = HashMap::new();
    let mut peaks: HashMap<String, (String, u64)> = HashMap::new();
    let mut wanted = Vec::new();
    for (path, size) in &list {
        let lower = path.to_lowercase();
        if let Some((original, ours)) = peaks_cache_of(&lower)
            && (ours || !peaks.contains_key(original))
        {
            peaks.insert(original.to_owned(), (path.clone(), *size));
        }
        // A prepared song keeps its proxies in `Media/Proxies/`; a song cut
        // from a Live set beside its recordings (`Samples/Imported/Proxies/`).
        if (lower.starts_with("proxies/") || lower.contains("/proxies/")) && lower.ends_with(".ogg")
        {
            let stem = Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            proxies.insert(stem, (path.clone(), *size));
            continue;
        }
        // A project file's log (`-wal`, `-shm`, `-tshm`) is the state of
        // whoever had it open, not part of the song: fetched, it came down
        // half-written (Cinematic Intro would not come in at all).
        let is_log = ["session-wal", "session-shm", "session-tshm"]
            .iter()
            .any(|log| lower.ends_with(log));
        if is_media(&lower) && is_mapped_original(&lower) {
            if let Some(name) = Path::new(&lower).file_name() {
                originals.insert(name.to_string_lossy().into_owned(), (path.clone(), *size));
            }
            continue;
        }
        if is_log || is_media(&lower) || *size == 0 || !keep.wants(&lower) {
            continue;
        }
        let local = base.join(path);
        if current && keep.has(&local, *size) {
            continue;
        }
        wanted.push((path.clone(), *size, local));
    }
    let fetched = wanted.len();
    let docs = Docs::new(&source, &label, version.as_deref()).await;
    // What is kept already, then the rest in one batch where the source
    // has one, then whatever the batch did not bring, one by one.
    let looked: Vec<((String, u64, PathBuf), Option<Vec<u8>>)> = futures_util::stream::iter(wanted)
        .map(|want| {
            let docs = &docs;
            async move {
                let kept = docs.cached(&want.0).await;
                (want, kept)
            }
        })
        .buffer_unordered(AT_ONCE)
        .collect()
        .await;
    let mut arrived: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut missing = Vec::new();
    for (want, kept) in looked {
        match kept {
            Some(bytes) => arrived.push((want.2, bytes)),
            None => missing.push(want),
        }
    }
    let mut batched = 0;
    if missing.len() > 1 {
        let paths = missing.iter().map(|(path, ..)| path.clone()).collect();
        if let Some(got) = source.read_many(paths).await {
            let mut got: HashMap<String, Vec<u8>> = got
                .into_iter()
                .filter_map(|(path, bytes)| Some((path, bytes?)))
                .collect();
            let mut rest = Vec::new();
            for (path, size, local) in missing {
                match got.remove(&path) {
                    Some(bytes) => {
                        docs.store(&path, &bytes).await;
                        batched += 1;
                        arrived.push((local, bytes));
                    }
                    None => rest.push((path, size, local)),
                }
            }
            missing = rest;
        }
    }
    let one_by_one: Vec<(PathBuf, Vec<u8>)> = futures_util::stream::iter(missing)
        .map(|(path, size, local)| {
            let docs = &docs;
            async move { Ok::<_, eyre::Report>((local, docs.read(path, size).await?)) }
        })
        .buffer_unordered(AT_ONCE)
        .try_collect()
        .await?;
    arrived.extend(one_by_one);
    for (local, bytes) in arrived {
        keep.forget_log(&local)?;
        keep.write(&local, bytes)?;
    }
    if let Some(chart) = source.chart().await? {
        write_chart(&keep, &project, &label, &chart)?;
    }
    keep.mirrored_at(&base, version.as_deref())?;
    tracing::info!(
        stream.source = %label,
        stream.files = list.len(),
        stream.fetched = fetched,
        stream.kept = docs.kept(),
        stream.batched = batched,
        stream.proxies = proxies.len(),
        stream.originals = originals.len(),
        "song-stream: mirrored"
    );
    Ok(StreamedSong {
        project,
        folder: keep.folder(),
        keep,
        base,
        source,
        proxies,
        originals,
        peaks,
        attached: Mutex::default(),
        opened: Mutex::default(),
        #[cfg(feature = "web")]
        unbuildable: Mutex::default(),
        listen: Listen::default(),
    })
}

/// A song's documents (its small files), read whole: in a browser, through
/// its Cache Storage when the source says what version they are at — so a
/// page opening the song again (a playground set starting over, a reload)
/// fetches none of them until the song's files change.
///
/// A session's objects are cached by their own name whatever the version:
/// each is named by the hash of its bytes, so a name read once is those
/// bytes for good — in every song that shares it, at every commit.
struct Docs<'a> {
    source: &'a Arc<dyn SongSource>,
    #[cfg(target_arch = "wasm32")]
    cache: Option<web_sys::Cache>,
    /// Where this version's documents are kept in `cache`: `None` when the
    /// source does not say which version it lists.
    #[cfg(target_arch = "wasm32")]
    versioned: Option<String>,
    kept: std::sync::atomic::AtomicUsize,
}

/// A content-addressed object's name, if `path` is one: `…/objects/sha256-…`.
#[cfg(any(target_arch = "wasm32", test))]
fn object_name(path: &str) -> Option<&str> {
    let (_, name) = path.rsplit_once("/objects/")?;
    (name.starts_with("sha256-") && !name.contains('/')).then_some(name)
}

impl<'a> Docs<'a> {
    #[allow(unused_variables)]
    async fn new(source: &'a Arc<dyn SongSource>, label: &str, version: Option<&str>) -> Docs<'a> {
        #[cfg(target_arch = "wasm32")]
        let cache = CachedFetch::cache().await.ok();
        #[cfg(target_arch = "wasm32")]
        let versioned = version.map(|version| {
            format!(
                "https://fts-cache.invalid/{}/docs/{}",
                safe_name(label),
                safe_name(version)
            )
        });
        Docs {
            source,
            #[cfg(target_arch = "wasm32")]
            cache,
            #[cfg(target_arch = "wasm32")]
            versioned,
            kept: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// How many came from the cache.
    fn kept(&self) -> usize {
        self.kept.load(Ordering::Relaxed)
    }

    /// Where `path` is kept in the cache, if it is kept at all.
    #[cfg(target_arch = "wasm32")]
    fn key(&self, path: &str) -> Option<String> {
        match (object_name(path), &self.versioned) {
            (Some(name), _) => Some(format!("https://fts-cache.invalid/objects/{name}")),
            (None, Some(base)) => Some(format!("{base}/{}", url_path(path))),
            (None, None) => None,
        }
    }

    /// `path` from the cache, when it is there.
    #[allow(clippy::unused_async, unused_variables)]
    async fn cached(&self, path: &str) -> Option<Vec<u8>> {
        #[cfg(target_arch = "wasm32")]
        if let (Some(cache), Some(key)) = (&self.cache, self.key(path)) {
            let bytes = cache_get(cache, &key).await;
            if bytes.is_some() {
                self.kept.fetch_add(1, Ordering::Relaxed);
            }
            return bytes;
        }
        None
    }

    /// Keep `bytes` as `path`. A cache that is full or refused only costs
    /// a fetch next time.
    #[allow(clippy::unused_async, unused_variables)]
    async fn store(&self, path: &str, bytes: &[u8]) {
        #[cfg(target_arch = "wasm32")]
        if let (Some(cache), Some(key)) = (&self.cache, self.key(path)) {
            let mut owned = bytes.to_vec();
            if let Ok(response) = web_sys::Response::new_with_opt_u8_array(Some(&mut owned)) {
                let _ =
                    wasm_bindgen_futures::JsFuture::from(cache.put_with_str(&key, &response)).await;
            }
        }
    }

    /// `path` whole: kept, else fetched and kept.
    async fn read(&self, path: String, size: u64) -> eyre::Result<Vec<u8>> {
        if let Some(bytes) = self.cached(&path).await {
            return Ok(bytes);
        }
        let bytes = self
            .source
            .read(path.clone(), 0..size)
            .await
            .map_err(|e| eyre::eyre!(e))?;
        self.store(&path, &bytes).await;
        Ok(bytes)
    }
}

/// A path's segments, each escaped for a cache key.
#[cfg(target_arch = "wasm32")]
fn url_path(path: &str) -> String {
    path.split('/')
        .map(|s| String::from(js_sys::encode_uri_component(s)))
        .collect::<Vec<_>>()
        .join("/")
}

/// What `cache` holds under `key`, if anything.
#[cfg(target_arch = "wasm32")]
async fn cache_get(cache: &web_sys::Cache, key: &str) -> Option<Vec<u8>> {
    use wasm_bindgen::JsCast as _;
    let hit = wasm_bindgen_futures::JsFuture::from(cache.match_with_str(key))
        .await
        .ok()?;
    let response = hit.dyn_into::<web_sys::Response>().ok()?;
    let body = wasm_bindgen_futures::JsFuture::from(response.array_buffer().ok()?)
        .await
        .ok()?;
    Some(js_sys::Uint8Array::new(&body).to_vec())
}

/// The source's chart, as THE chart beside `project` (the folder's own
/// gave way to it in the source's listing).
fn write_chart(keep: &Keep, project: &Path, label: &str, chart: &str) -> eyre::Result<()> {
    match keep {
        #[cfg(feature = "native")]
        Keep::Disk(_) => {
            if let Some(folder) = project.parent() {
                session::library::write_chart(folder, label, chart)?;
            }
            Ok(())
        }
        #[allow(unreachable_patterns)]
        _ => {
            let stem = project
                .file_stem()
                .map_or_else(|| label.to_owned(), |s| s.to_string_lossy().into_owned());
            keep.write(
                &project.with_file_name(format!("{stem}.kf")),
                chart.as_bytes().to_vec(),
            )
        }
    }
}

/// A take attached as a stream of its proxy: what to fetch for it, and the
/// audio it plays from (its waveform too, once handed in).
pub struct Attached {
    pub take: StreamedTake,
    pub source: daw::standalone::audio_engine::streamed::Streamed,
}

impl StreamedSong {
    /// Attach `media` (a take of the song opened into `daw` as `local`) as
    /// a stream of its proxy — silent until its bytes arrive. `None` when
    /// the peer has no indexed proxy for it.
    /// The proxy at `at`'s bytes and page index, opened the first time any
    /// item asks for it and shared from then on.
    fn opened(&self, at: &Path, size: u64) -> Option<(Arc<SparseBytes>, OggIndex)> {
        let mut opened = self.opened.lock().ok()?;
        if let Some((bytes, index)) = opened.get(at) {
            return Some((Arc::clone(bytes), index.clone()));
        }
        let index =
            OggIndex::from_text(&self.folder.read_to_string(&OggIndex::path_for(at)).ok()?)?;
        let bytes = self.keep.sparse(at, size)?;
        opened.insert(at.to_path_buf(), (Arc::clone(&bytes), index.clone()));
        Some((bytes, index))
    }

    pub fn attach(&self, daw: &Standalone, local: &str, media: &PendingMedia) -> Option<Attached> {
        let stem = Path::new(&media.path)
            .file_stem()?
            .to_string_lossy()
            .to_lowercase();
        let (path, size) = self.proxies.get(&stem)?.clone();
        let at = self.base.join(&path);
        let (bytes, index) = self.opened(&at, size)?;
        let feeder = daw::standalone::audio_engine::materialize::attach_remote_ogg(
            daw,
            local,
            &media.take_guid,
            Arc::clone(&bytes),
            index.clone(),
        );
        let source = feeder.source().clone();
        if let (Ok(mut attached), Some(name)) =
            (self.attached.lock(), Path::new(&media.path).file_name())
        {
            attached.push((
                name.to_string_lossy().to_lowercase(),
                feeder.source().clone(),
            ));
        }
        // Natively the butler thread decodes it; in a browser, the page's
        // audio loop.
        #[cfg(not(target_arch = "wasm32"))]
        daw::standalone::audio_engine::streamed::butler_adopt(feeder);
        #[cfg(all(feature = "web", target_arch = "wasm32"))]
        crate::web_audio::adopt(local, feeder.boxed());
        #[cfg(all(not(feature = "web"), target_arch = "wasm32"))]
        drop(feeder);
        Some(Attached {
            take: StreamedTake {
                path,
                bytes,
                index,
                start: media.start,
                end: media.end,
                source_offset: media.source_offset,
                playrate: media.playrate,
            },
            source,
        })
    }

    /// Attach `media` as [`Self::attach`] does, but heard from nothing yet:
    /// the take holds a stream that is never fed (its waveform draws from
    /// the original's cache all the same), and the stream that will play
    /// is returned with its feeder, decoded by whoever adopts it and
    /// attached when it should be heard ([`Detached::source`]). What a song
    /// heard by its reference holds its stems as: fetched and decoded
    /// behind the reference, silent, until they take over with no gap and
    /// no doubling. `None` as [`Self::attach`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn attach_detached(
        &self,
        daw: &Standalone,
        local: &str,
        media: &PendingMedia,
    ) -> Option<Detached> {
        use daw::standalone::audio_engine::streamed::{RemoteOgg, StreamFeeder, Streamed};
        let stem = Path::new(&media.path)
            .file_stem()?
            .to_string_lossy()
            .to_lowercase();
        let (path, size) = self.proxies.get(&stem)?.clone();
        let (bytes, index) = self.opened(&self.base.join(&path), size)?;
        // The stand-in the take holds until it is heard: never fed.
        drop(
            daw::standalone::audio_engine::materialize::attach_remote_ogg(
                daw,
                local,
                &media.take_guid,
                Arc::clone(&bytes),
                index.clone(),
            ),
        );
        let feeder = StreamFeeder::new(
            Streamed::new(index.channels, index.sample_rate, index.frames),
            RemoteOgg::new(Arc::clone(&bytes), index.clone()),
        );
        Some(Detached {
            take_guid: media.take_guid.clone(),
            take: StreamedTake {
                path,
                bytes,
                index,
                start: media.start,
                end: media.end,
                source_offset: media.source_offset,
                playrate: media.playrate,
            },
            feeder,
        })
    }

    /// The original recording `media_path` (a take's file, as the project
    /// names it) stands for, when the song has one to fetch at full
    /// resolution: where it is in the song, its size, and where it is kept
    /// here.
    #[must_use]
    pub fn original_of(&self, media_path: &str) -> Option<Original> {
        let name = Path::new(media_path)
            .file_name()?
            .to_string_lossy()
            .to_lowercase();
        let (path, size) = self.originals.get(&name)?.clone();
        let local = self.base.join(&path);
        Some(Original { path, size, local })
    }

    /// Bring `original` down whole, beside the song's mirror — written
    /// aside and moved in complete, so a fetch cut short never leaves a
    /// file that looks whole. Kept on disk only.
    ///
    /// # Errors
    ///
    /// The source would not give it, or it could not be written.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn fetch_original(&self, original: &Original) -> eyre::Result<()> {
        /// Tries before the file is given up on for this run.
        const TRIES: u32 = 3;
        if !self.listen.hears_stems() {
            eyre::bail!("capped at Reference: no recording of the song is fetched");
        }
        if !matches!(self.keep, Keep::Disk(_)) {
            eyre::bail!("a song kept in memory keeps no originals");
        }
        let got = Arc::new(std::sync::atomic::AtomicU64::new(0));
        fetch_proxy(
            &*self.source,
            &original.path,
            original.size,
            &original.local,
            &got,
            TRIES,
        )
        .await
    }

    /// The song's reference ([`crate::reference`]) as streams of their own
    /// — attached to no take: what a page in reference mode plays instead
    /// of the stems. Its preview too, when it has one: small enough to be
    /// heard almost at once, played until the reference proper has arrived
    /// where the song is. `None` when the song has no indexed reference.
    pub fn reference(&self) -> Option<Reference> {
        Some(Reference {
            full: self.loose_stream(crate::reference::STEM)?,
            preview: self.loose_stream(crate::reference::PREVIEW_STEM),
        })
    }

    /// The proxy of stem `stem` as a stream attached to no take: its fetch
    /// (a [`StreamedTake`] spanning it from 0 s) and its feeder, silent
    /// until the bytes arrive.
    fn loose_stream(&self, stem: &str) -> Option<LooseStream> {
        use daw::standalone::audio_engine::streamed::{RemoteOgg, StreamFeeder, Streamed};
        let (path, size) = self.proxies.get(stem)?.clone();
        let at = self.base.join(&path);
        let index =
            OggIndex::from_text(&self.folder.read_to_string(&OggIndex::path_for(&at)).ok()?)?;
        let bytes = self.keep.sparse(&at, size)?;
        let seconds = index.frames as f64 / f64::from(index.sample_rate.max(1));
        let feeder = StreamFeeder::new(
            Streamed::new(index.channels, index.sample_rate, index.frames),
            RemoteOgg::new(Arc::clone(&bytes), index.clone()),
        );
        Some((
            StreamedTake {
                path,
                bytes,
                index,
                start: 0.0,
                end: seconds,
                source_offset: 0.0,
                playrate: 1.0,
            },
            feeder,
        ))
    }

    /// The song capped at `listen`, the host's to set (a share link's
    /// layer). [`Listen::Full`] — what [`mirror`] gives — is as before
    /// the cap existed.
    #[must_use]
    pub const fn with_listen(mut self, listen: Listen) -> Self {
        self.listen = listen;
        self
    }

    /// How deep the host lets a listener go into the song.
    #[must_use]
    pub const fn listen(&self) -> Listen {
        self.listen
    }

    /// What the song's proxies are fetched through: its source — refusing
    /// every stem when the song is capped at [`Listen::Reference`], so no
    /// fetcher, warming or waveform build brings one in, whoever asks.
    #[must_use]
    pub fn media(&self) -> Arc<dyn RangeFetch> {
        let refused = if self.listen.hears_stems() {
            std::collections::HashSet::new()
        } else {
            self.paths_of(Heard::Stems)
                .into_iter()
                .map(str::to_owned)
                .collect()
        };
        Arc::new(SourceFetch {
            source: Arc::clone(&self.source),
            refused,
        })
    }

    /// The proxy paths warming the song for `heard` brings in: none for
    /// what its cap does not allow.
    #[must_use]
    pub fn warm_paths(&self, heard: Heard) -> std::collections::HashSet<&str> {
        if self.listen.allows(heard) {
            self.paths_of(heard)
        } else {
            std::collections::HashSet::new()
        }
    }

    /// The proxy paths a song is heard by in `heard`.
    fn paths_of(&self, heard: Heard) -> std::collections::HashSet<&str> {
        let reference = [crate::reference::STEM, crate::reference::PREVIEW_STEM];
        self.proxies
            .iter()
            .filter(|(stem, _)| match heard {
                Heard::Preview => stem.as_str() == crate::reference::PREVIEW_STEM,
                Heard::Reference => stem.as_str() == crate::reference::STEM,
                Heard::Stems => !reference.contains(&stem.as_str()),
            })
            .map(|(_, (path, _))| path.as_str())
            .collect()
    }

    /// Whether the song has a reference to be heard by.
    #[must_use]
    pub fn has_reference(&self) -> bool {
        self.proxies.contains_key(crate::reference::STEM)
    }

    /// The bytes of the song's stems — every proxy but the reference: what
    /// loading the multitracks brings in.
    #[must_use]
    pub fn stems_bytes(&self) -> u64 {
        let stems = self.paths_of(Heard::Stems);
        self.proxies
            .values()
            .filter(|(path, _)| stems.contains(path.as_str()))
            .map(|(_, size)| size)
            .sum()
    }

    /// Fetch each attached take's waveform — its original's peaks cache,
    /// reduced for a view that never draws blocks finer than `block`
    /// samples ([`daw::standalone::reapeaks::ReaPeaks::coarsened`]) — and
    /// hand it to the take's stream, one at a time. Returns how many took
    /// one. For a holder with no disk to find the caches on (a browser); a
    /// mirror on disk has them where the peak store looks.
    pub async fn load_peaks(&self, block: u32) -> usize {
        use daw::standalone::reapeaks::ReaPeaks;
        let mut loaded = 0usize;
        // One fetch and parse per recording, shared by every take of it.
        for (original, takes) in self.attached_by_original() {
            let Some((path, size)) = self.peaks.get(&original).cloned() else {
                continue;
            };
            match self.source.read(path.clone(), 0..size).await {
                Ok(bytes) => match ReaPeaks::parse(&bytes) {
                    Ok(peaks) => {
                        let peaks = Arc::new(peaks.coarsened(block));
                        for streamed in &takes {
                            streamed.set_peaks(Arc::clone(&peaks));
                        }
                        loaded = loaded.saturating_add(takes.len());
                    }
                    Err(e) => {
                        tracing::warn!(media = %path, error = %e, "song-stream: a waveform cache did not parse")
                    }
                },
                Err(e) => {
                    tracing::warn!(media = %path, error = %e, "song-stream: a waveform cache did not arrive")
                }
            }
        }
        tracing::info!(stream.source = %self.source.label(), stream.waveforms = loaded, "song-stream: waveforms in");
        loaded
    }

    /// Build the waveform of each attached take that has none — its share
    /// shipped no cache ([`Self::load_peaks`]) — from its proxy, fetched
    /// whole into a buffer of its own (what streams in for playback is
    /// only a window round the playhead), decoded and let go: one at a
    /// time, `most` of them. Returns how many were built; 0 when none is
    /// left that can be.
    #[cfg(feature = "web")]
    pub async fn build_missing_peaks(&self, block: u32, most: usize) -> usize {
        // Capped at Reference, a stem's proxy is not fetched for its
        // waveform either: its take draws from the share's cache or not at
        // all.
        if !self.listen.hears_stems() {
            return 0;
        }
        let media = self.media();
        let mut built = 0usize;
        for (original, takes) in self.attached_by_original() {
            if built >= most {
                break;
            }
            let takes: Vec<_> = takes.into_iter().filter(|t| !t.has_peaks()).collect();
            if takes.is_empty() || self.unbuildable(&original) {
                continue;
            }
            let stem = Path::new(&original)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let Some((path, size)) = self.proxies.get(&stem).cloned() else {
                continue;
            };
            let bytes = match media.fetch(&path, 0..size).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(media = %path, error = %e, "song-stream: a proxy for its waveform did not arrive");
                    self.give_up_on(&original);
                    continue;
                }
            };
            let peaks = fts_sample::ogg_stream::OggStream::open(Arc::from(bytes))
                .ok()
                .and_then(daw::standalone::audio_engine::streamed::peaks_of_ogg);
            let Some(peaks) = peaks else {
                tracing::warn!(media = %path, "song-stream: a proxy's waveform could not be built");
                self.give_up_on(&original);
                continue;
            };
            // Once per recording, for every take cut from it.
            let peaks = Arc::new(peaks.coarsened(block));
            for streamed in &takes {
                streamed.set_peaks(Arc::clone(&peaks));
            }
            built = built.saturating_add(1);
        }
        if built > 0 {
            tracing::info!(stream.source = %self.source.label(), stream.waveforms_built = built, "song-stream: waveforms built here");
        }
        built
    }

    /// The attached takes by the recording they stream, in the order first
    /// attached: every item cut from one recording shares its waveform.
    fn attached_by_original(
        &self,
    ) -> Vec<(
        String,
        Vec<daw::standalone::audio_engine::streamed::Streamed>,
    )> {
        let attached = self.attached.lock().map(|a| a.clone()).unwrap_or_default();
        let mut grouped: Vec<(String, Vec<_>)> = Vec::new();
        for (original, streamed) in attached {
            match grouped.iter_mut().find(|(o, _)| *o == original) {
                Some((_, takes)) => takes.push(streamed),
                None => grouped.push((original, vec![streamed])),
            }
        }
        grouped
    }

    /// Whether `original`'s waveform could not be built (its proxy did not
    /// arrive or would not decode): not tried again this visit.
    #[cfg(feature = "web")]
    fn unbuildable(&self, original: &str) -> bool {
        self.unbuildable.lock().is_ok_and(|u| u.contains(original))
    }

    #[cfg(feature = "web")]
    fn give_up_on(&self, original: &str) {
        if let Ok(mut u) = self.unbuildable.lock() {
            u.insert(original.to_owned());
        }
    }

    /// Start fetching `takes`' bytes in the order they will be heard from
    /// `playhead`; stops when `stop` is set.
    pub fn fetch(
        &self,
        takes: Arc<Mutex<Vec<StreamedTake>>>,
        playhead: Arc<dyn Fn() -> f64 + Send + Sync>,
        stop: Arc<AtomicBool>,
    ) {
        let fetch = self.media();
        // In a browser, through its cache: what the window let go comes
        // back from there, and a reload streams nothing twice. (Filling the
        // cache ahead of playing is [`Self::warm`], the page's to run.)
        #[cfg(target_arch = "wasm32")]
        let fetch: Arc<dyn RangeFetch> = Arc::new(CachedFetch::new(
            fetch,
            &self.source.label(),
            self.versions(),
        ));
        let driving = daw::standalone::audio_engine::media_fetch::drive(
            takes,
            fetch,
            playhead,
            self.keep.fetch_config(),
            stop,
        );
        #[cfg(feature = "native")]
        if let Some(runtime) = crate::open::runtime() {
            runtime.spawn(driving);
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(driving);
    }
}

impl StreamedSong {
    /// Bring what the song is `heard` by into the browser's cache before it
    /// is played: every block — so a seek anywhere in the song is local — or
    /// only the first `blocks` of each (the song's opening, for the song
    /// after the one on screen), until done or `stop`.
    #[cfg(target_arch = "wasm32")]
    pub async fn warm(&self, heard: Heard, blocks: Option<u64>, stop: Arc<AtomicBool>) {
        let fetch = self.media();
        let wanted = self.warm_paths(heard);
        let mut files = self.versions();
        files.retain(|path, _| wanted.contains(path.as_str()));
        CachedFetch::new(fetch, &self.source.label(), files)
            .warm(blocks, stop)
            .await;
    }

    /// Each proxy's size and version (its page index's hash: a proxy made
    /// again is indexed again), by its path in the song.
    #[cfg(target_arch = "wasm32")]
    fn versions(&self) -> HashMap<String, (u64, u64)> {
        self.proxies
            .values()
            .map(|(path, size)| {
                let index = self
                    .folder
                    .read(&OggIndex::path_for(&self.base.join(path)))
                    .unwrap_or_default();
                (path.clone(), (*size, fnv(&index)))
            })
            .collect()
    }
}

/// A take attached to a stream that is not heard yet
/// ([`StreamedSong::attach_detached`]): its fetch, and the stream that will
/// play it with its feeder.
#[cfg(not(target_arch = "wasm32"))]
pub struct Detached {
    pub take_guid: String,
    pub take: StreamedTake,
    pub feeder: daw::standalone::audio_engine::streamed::StreamFeeder<
        daw::standalone::audio_engine::streamed::RemoteOgg,
    >,
}

/// An original recording of a streamed song ([`StreamedSong::original_of`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Original {
    /// Its path in the song folder.
    pub path: String,
    pub size: u64,
    /// Where it is kept on this device.
    pub local: PathBuf,
}

impl Original {
    /// Whether all of it is here.
    #[must_use]
    pub fn is_here(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::fs::metadata(&self.local).is_ok_and(|m| m.len() == self.size)
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }
}

/// A stream attached to no take: its fetch and its feeder.
pub type LooseStream = (
    StreamedTake,
    daw::standalone::audio_engine::streamed::StreamFeeder<
        daw::standalone::audio_engine::streamed::RemoteOgg,
    >,
);

/// A song's reference, as streams: the reference proper, and its preview.
pub struct Reference {
    pub full: LooseStream,
    pub preview: Option<LooseStream>,
}

/// FNV-1a: a stable, dependency-free hash for cache keys.
#[cfg(target_arch = "wasm32")]
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A song's proxies through the browser's Cache Storage — off the wasm
/// heap, and kept across reloads: each proxy in aligned blocks, a block
/// answered from the cache when it is there and fetched (then kept) when
/// not. What the resident window lets go comes back from here, and
/// [`Self::warm`] fills the cache with the whole song in the background,
/// so a seek anywhere is local once it has.
#[cfg(target_arch = "wasm32")]
pub struct CachedFetch {
    inner: Arc<dyn RangeFetch>,
    label: String,
    /// Size and version by path.
    files: HashMap<String, (u64, u64)>,
}

#[cfg(target_arch = "wasm32")]
impl CachedFetch {
    /// Bytes per cached block.
    const BLOCK: u64 = 256 * 1024;
    /// The Cache Storage cache the blocks live in.
    const CACHE: &'static str = "fts-session-media";

    fn new(inner: Arc<dyn RangeFetch>, label: &str, files: HashMap<String, (u64, u64)>) -> Self {
        Self {
            inner,
            label: safe_name(label),
            files,
        }
    }

    /// The cache key of block `block` of `path` (a URL: Cache Storage keys
    /// are requests; this origin is never fetched).
    fn key(&self, path: &str, version: u64, block: u64) -> String {
        let path = url_path(path);
        format!(
            "https://fts-cache.invalid/{}/{version:016x}/{path}/{block}",
            self.label
        )
    }

    async fn cache() -> Result<web_sys::Cache, String> {
        use wasm_bindgen::JsCast as _;
        let caches = web_sys::window()
            .ok_or("no window")?
            .caches()
            .map_err(|e| format!("no cache storage: {e:?}"))?;
        let cache = wasm_bindgen_futures::JsFuture::from(caches.open(Self::CACHE))
            .await
            .map_err(|e| format!("{e:?}"))?;
        cache.dyn_into().map_err(|_| "not a cache".to_owned())
    }

    /// Block `block` of `path`: from the cache, or fetched and kept.
    async fn block(
        &self,
        cache: &web_sys::Cache,
        path: &str,
        block: u64,
    ) -> Result<Vec<u8>, String> {
        let (size, version) = self
            .files
            .get(path)
            .copied()
            .ok_or_else(|| format!("{path}: not a proxy of this song"))?;
        let key = self.key(path, version, block);
        if let Some(bytes) = cache_get(cache, &key).await {
            return Ok(bytes);
        }
        let start = block * Self::BLOCK;
        let mut bytes = self
            .inner
            .fetch(path, start..(start + Self::BLOCK).min(size))
            .await?;
        if let Ok(response) = web_sys::Response::new_with_opt_u8_array(Some(&mut bytes)) {
            // A cache that is full or refused only costs a fetch next time.
            let _ = wasm_bindgen_futures::JsFuture::from(cache.put_with_str(&key, &response)).await;
        }
        Ok(bytes)
    }

    /// Whether block `block` of `path` is in the cache already.
    async fn cached(&self, cache: &web_sys::Cache, path: &str, block: u64) -> bool {
        let Some((_, version)) = self.files.get(path).copied() else {
            return false;
        };
        wasm_bindgen_futures::JsFuture::from(cache.match_with_str(&self.key(path, version, block)))
            .await
            .is_ok_and(|hit| !hit.is_undefined())
    }

    /// Fill the cache with every proxy's blocks — all of them, or the
    /// first `limit` of each — until done or `stop`: several requests at
    /// once (each is a round trip), block by block across every file so
    /// the whole band's next stretch comes before any one track's end.
    /// A block already there is only looked up, not read.
    async fn warm(&self, limit: Option<u64>, stop: Arc<AtomicBool>) {
        use futures_util::StreamExt as _;
        /// Blocks in flight at once.
        const AT_ONCE: usize = 8;
        let Ok(cache) = Self::cache().await else {
            return;
        };
        let mut files: Vec<(&String, &(u64, u64))> = self.files.iter().collect();
        files.sort();
        let blocks = files
            .iter()
            .map(|(_, (size, _))| size.div_ceil(Self::BLOCK))
            .max()
            .unwrap_or(0);
        let blocks = limit.map_or(blocks, |limit| blocks.min(limit));
        let wanted = (0..blocks).flat_map(|block| {
            files
                .iter()
                .filter(move |(_, (size, _))| block * Self::BLOCK < *size)
                .map(move |(path, _)| (path.as_str(), block))
        });
        let cache = &cache;
        let stop = &stop;
        futures_util::stream::iter(wanted)
            .map(|(path, block)| async move {
                if stop.load(Ordering::Relaxed) || self.cached(cache, path, block).await {
                    return;
                }
                if let Err(e) = self.block(cache, path, block).await {
                    tracing::debug!(media = %path, block, error = %e, "song-stream: warming the cache skipped a block");
                }
            })
            .buffer_unordered(AT_ONCE)
            .for_each(|()| async {})
            .await;
        if !stop.load(Ordering::Relaxed) {
            tracing::info!(stream.source = %self.label, stream.files = files.len(), stream.blocks = ?limit, "song-stream: in the browser's cache");
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl RangeFetch for CachedFetch {
    fn fetch(&self, path: &str, range: std::ops::Range<u64>) -> Fetching {
        let this = Self {
            inner: Arc::clone(&self.inner),
            label: self.label.clone(),
            files: self.files.clone(),
        };
        let path = path.to_owned();
        Box::pin(async move {
            if range.is_empty() {
                return Ok(Vec::new());
            }
            let cache = Self::cache().await?;
            let mut out = Vec::with_capacity(usize::try_from(range.end - range.start).unwrap_or(0));
            for block in range.start / Self::BLOCK..=(range.end - 1) / Self::BLOCK {
                let bytes = this.block(&cache, &path, block).await?;
                let at = block * Self::BLOCK;
                let from = usize::try_from(range.start.saturating_sub(at)).unwrap_or(0);
                let to = usize::try_from((range.end - at).min(bytes.len() as u64)).unwrap_or(0);
                out.extend_from_slice(
                    bytes
                        .get(from..to)
                        .ok_or_else(|| format!("{path}: block {block} is short"))?,
                );
            }
            Ok(out)
        })
    }
}

/// How many of `takes` have all they play.
#[must_use]
pub fn arrived(takes: &Mutex<Vec<StreamedTake>>) -> usize {
    takes
        .lock()
        .map(|t| t.iter().filter(|t| t.complete()).count())
        .unwrap_or(0)
}

/// Bytes from a song's source, but none of the paths it refuses (a song
/// capped at Reference: its stems).
struct SourceFetch {
    source: Arc<dyn SongSource>,
    refused: std::collections::HashSet<String>,
}

impl RangeFetch for SourceFetch {
    fn fetch(&self, path: &str, range: std::ops::Range<u64>) -> Fetching {
        if self.refused.contains(path) {
            let refused = format!("{path}: capped at Reference, its stems are not fetched");
            return Box::pin(async move { Err(refused) });
        }
        self.source.read(path.to_owned(), range)
    }
}

/// Stop a song's fetcher when dropped.
pub struct FetchGuard(pub Arc<AtomicBool>);

impl Drop for FetchGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The listening cap, counted at the source: what a song capped at
/// Reference asks its source for, against what a full one does.
#[cfg(test)]
#[cfg(feature = "native")]
mod cap_tests {
    use std::sync::{Arc, Mutex};

    use super::{Keep, Listen, Listing, Pending, SongSource, WarmStep, mirror, warm_order};

    /// A song's source that answers every read (with zeros) and counts it.
    struct Counting {
        files: Vec<(String, u64)>,
        reads: Arc<Mutex<Vec<String>>>,
    }

    impl SongSource for Counting {
        fn label(&self) -> String {
            "counting".to_owned()
        }

        fn list(&self) -> Pending<eyre::Result<Listing>> {
            let entries = self.files.clone();
            Box::pin(async move { Ok(entries.into()) })
        }

        fn read(
            &self,
            path: String,
            range: std::ops::Range<u64>,
        ) -> Pending<Result<Vec<u8>, String>> {
            self.reads.lock().expect("reads").push(path);
            let len = usize::try_from(range.end.saturating_sub(range.start)).unwrap_or(0);
            Box::pin(async move { Ok(vec![0; len]) })
        }
    }

    /// The stems' proxies, and the recording one is cut from.
    const STEMS: [&str; 2] = ["Media/Proxies/Kick.ogg", "Media/Proxies/Bass.ogg"];
    const ORIGINAL: &str = "Media/Kick.wav";
    const REFERENCE: [&str; 2] = [
        "Media/Proxies/Reference.ogg",
        "Media/Proxies/Reference Preview.ogg",
    ];

    /// A prepared song's folder: its project, its stems' proxies and the
    /// reference's, each with its page index, and one original.
    fn files() -> Vec<(String, u64)> {
        let mut files = vec![("Song.RPP".to_owned(), 64), (ORIGINAL.to_owned(), 4096)];
        for proxy in STEMS.iter().chain(&REFERENCE) {
            files.push(((*proxy).to_owned(), 1024));
            files.push((format!("{proxy}.idx"), 16));
        }
        files
    }

    /// Open the song from the counting source under `listen` and do what a
    /// page does after: warm it in [`warm_order`], fetch every proxy it
    /// attaches as it plays (the stems held behind the reference too, as
    /// [`super::StreamedSong::fetch`] is handed them), and ask for its
    /// original (full resolution). Returns every path the source was read
    /// for.
    fn opened_and_played(listen: Listen) -> Vec<String> {
        let reads = Arc::new(Mutex::new(Vec::new()));
        let source = Arc::new(Counting {
            files: files(),
            reads: Arc::clone(&reads),
        });
        let cache = tempfile::tempdir().expect("tempdir");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let song = mirror(source, Keep::Disk(cache.path().to_path_buf()))
                .await
                .expect("mirrored")
                .with_listen(listen);
            assert!(song.has_reference());
            let media = song.media();
            for step in warm_order(&[song.has_reference()], listen) {
                if let WarmStep::Warm { heard, .. } = step {
                    for path in song.warm_paths(heard) {
                        let _ = media.fetch(path, 0..16).await;
                    }
                }
            }
            for path in STEMS.iter().chain(&REFERENCE) {
                let _ = media.fetch(path, 0..16).await;
            }
            if let Some(original) = song.original_of(ORIGINAL) {
                let _ = song.fetch_original(&original).await;
            }
        });
        reads.lock().expect("reads").clone()
    }

    #[test]
    fn capped_at_reference_opening_a_song_fetches_no_stem() {
        let reads = opened_and_played(Listen::Reference);
        for stem in STEMS.iter().chain([&ORIGINAL]) {
            assert!(
                !reads.iter().any(|r| r == stem),
                "{stem} was fetched: {reads:?}"
            );
        }
        for reference in REFERENCE {
            assert!(
                reads.iter().any(|r| r == reference),
                "{reference} did not come: {reads:?}"
            );
        }
    }

    #[test]
    fn full_still_warms_and_fetches_the_stems() {
        let reads = opened_and_played(Listen::Full);
        for wanted in STEMS.iter().chain(&REFERENCE).chain([&ORIGINAL]) {
            assert!(
                reads.iter().any(|r| r == wanted),
                "{wanted} was not fetched: {reads:?}"
            );
        }
    }
}
