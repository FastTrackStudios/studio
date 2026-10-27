//! A set streamed in from Task, played by this window's own engine: a live
//! set joined by its link (the public demo, or one someone shared), or a
//! setlist in an org's library, as a member.
//!
//! Each song comes in the one way a song streams in ([`crate::song_stream`]):
//! its small files mirrored to a cache on disk, so it opens the one way a
//! song opens ([`Setlist::open_streamed`]) with its media deferred; then
//! each take is attached to a stream of its proxy, and one fetcher per song
//! brings their bytes in, what the playhead will reach first first
//! ([`stream`]). The first song opens the set; the rest open behind it,
//! one by one ([`take_arrivals`]). A live set is joined as well
//! ([`crate::collab::join_task`]): its people, its leader, its edits.
//!
//! Blocking, like [`Setlist::open`]: a window runs it on a thread of its
//! own, and hears each step through `progress` — the loading screen's.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use daw::standalone::audio_engine::materialize::playing_media;
use daw::standalone::audio_engine::media_fetch::StreamedTake;

use crate::collab::TaskSet;
use crate::loading::Progress;
use crate::setlist::{Arrival, Opening, Setlist};
use crate::song_stream::{FetchGuard, Keep, ShareSource, SongSource, StreamedSong, TaskSource};

/// A setlist in an org's library, as [`setlists`] lists them.
pub use session::library::Setlist as LibrarySetlist;
/// A song in an org's library, and what a list is for — for the window's
/// list editor.
pub use session::library::{Library, ListKind, Song as LibrarySong};

/// A set Task keeps, to stream in.
pub enum Remote {
    /// A live share link (`https://…/org/<org>/share/<token>`): joined as a
    /// guest called `name`, each song from its files link.
    Live { link: String, name: String },
    /// A setlist in the org's library, joined as the member signed in to
    /// it, called `name`.
    Library {
        library: session::library::Library,
        setlist: session::library::Setlist,
        name: String,
    },
}

/// The library's setlists, for picking one.
///
/// # Errors
///
/// The library could not be reached, or refused.
pub fn setlists(
    library: &session::library::Library,
) -> eyre::Result<Vec<session::library::Setlist>> {
    crate::open::engine_runtime()?.block_on(library.setlists())
}

/// `work` against the library, to its end: what a window calls on a
/// thread of its own to change its lists (see `session::library::Library`'s
/// `create_list`, `set_songs` and the rest).
///
/// # Errors
///
/// The engine's runtime did not start, or `work` failed.
pub fn with_library<T, F>(
    library: &session::library::Library,
    work: impl FnOnce(session::library::Library) -> F,
) -> eyre::Result<T>
where
    F: std::future::Future<Output = eyre::Result<T>>,
{
    crate::open::engine_runtime()?.block_on(work(library.clone()))
}

/// Open `remote`'s songs into this window's engine, the first current, and
/// join the set. A song that cannot be brought in is left out, as a song
/// that will not open is.
///
/// # Errors
///
/// The set could not be joined, or none of its songs opened.
pub fn open(remote: Remote, progress: &dyn Fn(Progress)) -> eyre::Result<Setlist> {
    // Until this open says otherwise, what it opens is not a library set.
    crate::live_host::opened_from_library(false);
    use crate::task_set::until_ok;
    let runtime = crate::open::engine_runtime()?;
    // What opens a song spawns tasks (`architect::platform::spawn`): on
    // the runtime, from this thread as from the one the rest open on.
    let _entered = runtime.enter();
    let (sources, set, name) = match remote {
        Remote::Live { link, name } => {
            let set = TaskSet::parse(&format!("share:{link}"));
            progress(Progress::Joining { retry: None });
            let joined = runtime.block_on(until_ok(
                Some(5),
                || crate::task_set::join(&set.url),
                |why| progress(Progress::Joining { retry: Some(why) }),
            ))?;
            tracing::info!(live.setlist = %joined.title, live.songs = joined.songs.len(), "stream-set: joined a live set");
            let sources: Vec<(String, Source)> = joined
                .songs
                .iter()
                .filter_map(|song| Some((song.title.clone(), Source::Share(song.files.clone()?))))
                .collect();
            let set = TaskSet {
                setlist: joined.setlist,
                ..set
            };
            (sources, set, name)
        }
        Remote::Library {
            library,
            setlist,
            name,
        } => {
            // A set of the library: it is there already — auto-hosting
            // leaves it be rather than make another of the songs' names.
            crate::live_host::opened_from_library(true);
            // Each song is looked up in the library when its turn comes —
            // not all of them before the first can open.
            let sources: Vec<(String, Source)> = setlist
                .songs
                .iter()
                .map(|song| {
                    (
                        song.title.clone(),
                        Source::Library(library.clone(), song.slug.clone()),
                    )
                })
                .collect();
            let set = TaskSet {
                url: library.org_url(),
                token: library.token.clone(),
                setlist: setlist.id.clone(),
            };
            (sources, set, name)
        }
    };
    let cache = cache_dir();
    let titles: Vec<String> = sources.iter().map(|(title, _)| title.clone()).collect();
    // Every song brought in at once — each its own fetch, none waiting on
    // another's: a set of eleven took a minute one after another.
    let fetches: Vec<(String, Fetch)> = sources
        .into_iter()
        .map(|(title, source)| {
            let cache = cache.clone();
            let fetch = runtime.spawn(async move { bring_in(&source, &cache).await });
            (title, fetch)
        })
        .collect();
    let mut fetches = fetches.into_iter();
    // The set opens on its first song (in set order) to come in; the rest
    // open behind it as theirs do.
    let (mut setlist, rest) = loop {
        let Some((title, fetch)) = fetches.next() else {
            eyre::bail!("none of the set's songs could be brought in");
        };
        progress(Progress::Fetching {
            title: title.clone(),
            retry: None,
        });
        let song = match fetched(runtime, fetch) {
            Ok(song) => song,
            Err(e) => {
                tracing::warn!(song.title = %title, error = %e, "stream-set: a song could not be brought in; the set goes on without it");
                continue;
            }
        };
        progress(Progress::Opening {
            title: title.clone(),
        });
        let setlist = Setlist::open_streamed(vec![Opening {
            path: song.project.clone(),
            title: Some(title),
            streamed: Some(song),
        }])?;
        break (setlist, fetches.collect::<Vec<_>>());
    };
    setlist.pending = rest.iter().map(|(title, _)| title.clone()).collect();
    setlist.order = titles;
    crate::collab::join_task_in_background(set, name);
    let (arrive, arrivals) = tokio::sync::mpsc::unbounded_channel();
    if let Ok(mut slot) = ARRIVALS.lock() {
        *slot = Some(arrivals);
    }
    open_behind_all(runtime, rest, &arrive);
    Ok(setlist)
}

/// Where a song of the set comes from.
enum Source {
    /// A live set's song: its files link.
    Share(String),
    /// A library setlist's song: its slug in the library.
    Library(session::library::Library, String),
}

impl Source {
    /// The song, reached: a share link parsed, or the library asked for it.
    async fn reach(&self) -> eyre::Result<Arc<dyn SongSource>> {
        Ok(match self {
            Self::Share(link) => Arc::new(ShareSource::new(link)?),
            Self::Library(library, slug) => Arc::new(TaskSource::of(library.clone(), slug).await?),
        })
    }
}

/// A song reached and its small files mirrored into `cache` — tried again
/// a few times, since Task may be busy.
/// A song being brought in, on the engine's runtime.
type Fetch = tokio::task::JoinHandle<eyre::Result<StreamedSong>>;

/// Songs opened behind the first at once — the engine's loads run on
/// their own threads, never a window's.
const OPENING_AT_ONCE: usize = 4;

/// The song `fetch` brought in, waited for.
fn fetched(runtime: &tokio::runtime::Runtime, fetch: Fetch) -> eyre::Result<StreamedSong> {
    runtime
        .block_on(fetch)
        .map_err(|e| eyre::eyre!("its fetch stopped: {e}"))?
}

/// Open `rest` behind the set's first song, [`OPENING_AT_ONCE`] at a time,
/// each as soon as its fetch is in, every one sent to `arrive` — the window
/// puts it in its place in the set, whatever order they finish in.
fn open_behind_all(
    runtime: &'static tokio::runtime::Runtime,
    rest: Vec<(String, Fetch)>,
    arrive: &tokio::sync::mpsc::UnboundedSender<Arrival>,
) {
    let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(rest)));
    for worker in 0..OPENING_AT_ONCE {
        let (queue, arrive) = (Arc::clone(&queue), arrive.clone());
        let spawned = std::thread::Builder::new()
            .name(format!("set-open-{worker}"))
            .spawn(move || {
                let _entered = runtime.enter();
                loop {
                    let next = queue.lock().ok().and_then(|mut q| q.pop_front());
                    let Some((title, fetch)) = next else {
                        return;
                    };
                    let opened = fetched(runtime, fetch).and_then(|song| {
                        Setlist::open_behind(Opening {
                            path: song.project.clone(),
                            title: Some(title.clone()),
                            streamed: Some(song),
                        })
                    });
                    let arrival = match opened {
                        Ok(song) => Arrival::Song(song),
                        Err(e) => {
                            tracing::warn!(song.title = %title, error = %e, "stream-set: a song did not open; the set goes on without it");
                            Arrival::Failed(title)
                        }
                    };
                    if arrive.send(arrival).is_err() {
                        return;
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "stream-set: no thread to open songs on");
        }
    }
}

async fn bring_in(source: &Source, cache: &std::path::Path) -> eyre::Result<StreamedSong> {
    let reached = source.reach().await?;
    crate::task_set::until_ok(
        Some(3),
        || crate::song_stream::mirror(Arc::clone(&reached), Keep::Disk(cache.to_path_buf())),
        |_| {},
    )
    .await
}

/// The set's songs after the first, as each opens — once, for the window
/// that shows them.
static ARRIVALS: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>>> = Mutex::new(None);

/// The songs arriving behind the first (see [`open`]), for the window to
/// add to its set; `None` once taken, or when nothing is arriving.
pub fn take_arrivals() -> Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>> {
    ARRIVALS.lock().ok()?.take()
}

/// Where streamed songs' small files are kept: the same bytes at the same
/// version are not fetched twice.
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Session")
        .join("streamed")
}

/// A song's streams, kept playing for as long as the process runs.
struct Streaming {
    _song: StreamedSong,
    /// The song as opened here.
    project: String,
    /// The takes being fetched — the fetch reads this list as it goes, so
    /// what is added to it is fetched from then on.
    takes: Arc<Mutex<Vec<StreamedTake>>>,
    /// Heard by its reference ([`crate::reference_play`]): the stems,
    /// attached and not fetched, with the streams they fill.
    held: Vec<(
        StreamedTake,
        daw::standalone::audio_engine::streamed::Streamed,
    )>,
    _fetching: FetchGuard,
    /// The reference's own fetch, while it is heard by it.
    _reference: Option<FetchGuard>,
}

static STREAMS: Mutex<Vec<Streaming>> = Mutex::new(Vec::new());

/// Attach every take of the song just opened as `local` to a stream of its
/// proxy in `song`, and start bringing their bytes in from the song's
/// playhead on.
///
/// A song with a reference is heard by it until the multitracks are asked
/// for ([`crate::reference_play`]): only the reference's bytes are fetched,
/// and the tracks it leaves out (the guide) — they play live over it. The
/// other stems are attached (their waveforms draw) and held.
pub fn stream(song: StreamedSong, local: &str) {
    let Some(daw) = crate::open::local_engine() else {
        tracing::warn!("stream-set: no engine to stream into");
        return;
    };
    let playhead: Arc<dyn Fn() -> f64 + Send + Sync> = match daw.sync_backend(local) {
        Some(backend) => Arc::new(move || {
            daw_transport_sync::TransportBackend::snapshot(&backend)
                .map_or(0.0, |s| s.playhead_seconds)
        }),
        None => Arc::new(|| 0.0),
    };
    let reference = if crate::reference_play::multitracks() {
        None
    } else {
        song.reference()
    };
    let by_reference = reference.is_some();
    let reference_fetch = reference.map(|crate::song_stream::Reference { full, preview }| {
        let (preview_take, preview) = preview.unzip();
        crate::reference_play::hear(
            local,
            full.1.source().clone(),
            preview.as_ref().map(|p| p.source().clone()),
        );
        daw::standalone::audio_engine::streamed::butler_adopt(full.1);
        if let Some(preview) = preview {
            daw::standalone::audio_engine::streamed::butler_adopt(preview);
        }
        // The preview first: small, so it is heard almost at once.
        let takes = preview_take.into_iter().chain([full.0]).collect();
        let stop = Arc::new(AtomicBool::new(false));
        song.fetch(
            Arc::new(Mutex::new(takes)),
            Arc::clone(&playhead),
            Arc::clone(&stop),
        );
        FetchGuard(stop)
    });
    // Heard by its reference, the tracks it leaves out play live.
    let live = if by_reference {
        crate::reference::left_out(daw, local)
    } else {
        std::collections::HashSet::new()
    };
    // What plays: each item's active take on a lane that sounds. Other
    // takes and lanes are fetched when they are needed (comping).
    let media = playing_media(daw, local);
    let mut takes = Vec::new();
    let mut held = Vec::new();
    for m in &media {
        let Some(attached) = song.attach(daw, local, m) else {
            tracing::warn!(stream.media = %m.path, "stream-set: no indexed proxy for this take");
            continue;
        };
        if by_reference && !live.contains(&m.track_guid) {
            held.push((attached.take, attached.source));
        } else {
            takes.push(attached.take);
        }
    }
    tracing::info!(
        stream.song = local,
        stream.media = media.len(),
        stream.fetched = takes.len(),
        stream.held = held.len(),
        stream.by_reference = by_reference,
        "stream-set: takes attached to their streams"
    );
    let takes = Arc::new(Mutex::new(whole_files(takes)));
    let stop = Arc::new(AtomicBool::new(false));
    song.fetch(Arc::clone(&takes), playhead, Arc::clone(&stop));
    watch();
    if let Ok(mut streams) = STREAMS.lock() {
        streams.push(Streaming {
            _song: song,
            project: local.to_owned(),
            takes,
            held,
            _fetching: FetchGuard(stop),
            _reference: reference_fetch,
        });
    }
}

/// How long a song keeps its reference once its stems are asked for, at
/// most: a stem that will not come must not keep it on its reference.
const HANDOVER: std::time::Duration = std::time::Duration::from_secs(20);

/// Hear every streamed song by its stems: the held stems are fetched, and
/// each song's reference goes once its stems have all arrived at the
/// playhead (or after [`HANDOVER`]) — so the sound never drops out between
/// the two.
pub fn hear_stems() {
    let waiting: Vec<(
        String,
        Vec<(
            StreamedTake,
            daw::standalone::audio_engine::streamed::Streamed,
        )>,
    )> = STREAMS
        .lock()
        .map(|mut streams| {
            streams
                .iter_mut()
                .filter(|s| !s.held.is_empty())
                .map(|s| {
                    let held = std::mem::take(&mut s.held);
                    if let Ok(mut takes) = s.takes.lock() {
                        takes.extend(held.iter().map(|(take, _)| take.clone()));
                    }
                    (s.project.clone(), held)
                })
                .collect()
        })
        .unwrap_or_default();
    if waiting.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("session-stems-handover".into())
        .spawn(move || {
            let started = web_time::Instant::now();
            let mut waiting = waiting;
            while !waiting.is_empty() {
                let at = crate::engine::Transport::shared().map_or(0.0, |t| t.read().0);
                let late = started.elapsed() >= HANDOVER;
                waiting.retain(|(project, stems)| {
                    let here = stems.iter().all(|(take, source)| arrived_at(take, source, at));
                    if here || late {
                        tracing::info!(stream.song = %project, stream.late = late, "stream-set: the stems take over from the reference");
                        crate::reference_play::stop(project);
                        false
                    } else {
                        true
                    }
                });
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "stream-set: no thread to hand over to the stems");
    }
}

/// Whether `take`'s audio has arrived where the song is at `at` seconds —
/// true for a take that does not play there.
fn arrived_at(
    take: &StreamedTake,
    source: &daw::standalone::audio_engine::streamed::Streamed,
    at: f64,
) -> bool {
    if at < take.start || at >= take.end {
        return true;
    }
    let seconds = (at - take.start) * take.playrate + take.source_offset;
    let frame = (seconds.max(0.0) * f64::from(source.sample_rate())) as usize;
    source.resident(frame / daw::standalone::audio_engine::streamed::CHUNK)
}

/// Where a download has got to: which song of how many, and that song's
/// proxies — bytes fetched, and in all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Downloading {
    pub title: String,
    pub song: String,
    pub index: usize,
    pub count: usize,
    pub done: u64,
    pub total: u64,
}

/// The folder downloaded songs are kept in, under a device's documents:
/// one folder per song, shared by every set that has it.
pub const DOWNLOADED_SONGS: &str = "Songs";

/// The file a list downloaded as `title` is kept as, in `into`.
#[must_use]
pub fn download_file(into: &std::path::Path, title: &str) -> PathBuf {
    let name: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = if name.trim().is_empty() {
        "Download".to_owned()
    } else {
        name
    };
    into.join(format!("{name}.setlist"))
}

/// Download `songs` from `library` onto this device, to open with no
/// connection: each song's small files and its proxies (not the originals)
/// under `into/Songs/`, and `title` as a `.setlist` in `into` listing them
/// in order — what the start screen lists, and opens as any setlist on the
/// device. Songs downloaded already are not fetched again; a song that
/// cannot be brought is left out, as a set leaves out a song that will not
/// open. Blocking: run it on a thread of its own.
///
/// # Errors
///
/// None of the songs could be downloaded, or the setlist not written.
pub fn download(
    library: &session::library::Library,
    title: &str,
    songs: &[session::library::Song],
    into: &std::path::Path,
    offline: crate::song_stream::Offline,
    progress: &(dyn Fn(Downloading) + Sync),
) -> eyre::Result<Downloaded> {
    let runtime = crate::open::engine_runtime()?;
    let _entered = runtime.enter();
    let root = into.join(DOWNLOADED_SONGS);
    std::fs::create_dir_all(&root)?;
    let count = songs.len();
    let mut lines = Vec::new();
    let mut failed = Vec::new();
    for (index, song) in songs.iter().enumerate() {
        let step = |done, total| {
            progress(Downloading {
                title: title.to_owned(),
                song: song.title.clone(),
                index,
                count,
                done,
                total,
            });
        };
        step(0, 0);
        let fetched = runtime.block_on(async {
            let source: Arc<dyn SongSource> =
                Arc::new(TaskSource::of(library.clone(), &song.slug).await?);
            crate::song_stream::download(source, &root, offline, &step).await
        });
        match fetched {
            Ok(folder) => {
                if let Ok(relative) = folder.strip_prefix(into) {
                    lines.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
            Err(e) => {
                tracing::warn!(song.title = %song.title, error = %e, "download: a song did not come down; the set goes on without it");
                failed.push((song.title.clone(), format!("{e:#}")));
            }
        }
    }
    if lines.is_empty() {
        // Why, in the words of the first song's failure: "none could be"
        // alone said nothing a person (or a bug report) could act on.
        let why = failed
            .first()
            .map_or_else(String::new, |(title, e)| format!(" — {title}: {e}"));
        eyre::bail!("none of its songs could be downloaded{why}");
    }
    let file = download_file(into, title);
    let mut text = format!("# {title}\n");
    for line in &lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(&file, text)?;
    tracing::info!(
        download.songs = lines.len(),
        download.failed = failed.len(),
        "download: a set is on this device"
    );
    Ok(Downloaded { file, failed })
}

/// A set downloaded: its `.setlist`, and the songs that did not come down
/// (title, why) — the set plays without them, and the person is told.
#[derive(Debug)]
pub struct Downloaded {
    pub file: PathBuf,
    pub failed: Vec<(String, String)>,
}

/// Take a downloaded set off this device: its `.setlist`, and every song
/// folder of it no other downloaded set still lists.
///
/// # Errors
///
/// A file could not be removed.
pub fn remove_download(into: &std::path::Path, file: &std::path::Path) -> eyre::Result<()> {
    let listed = |path: &std::path::Path| -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect()
    };
    let mine = listed(file);
    let mut kept = std::collections::HashSet::new();
    for entry in std::fs::read_dir(into)?.flatten() {
        let other = entry.path();
        if other != file
            && other
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("setlist"))
        {
            kept.extend(listed(&other));
        }
    }
    std::fs::remove_file(file)?;
    for line in mine {
        // Only what the downloads keep: never a folder outside it.
        if line.starts_with(&format!("{DOWNLOADED_SONGS}/"))
            && !line.contains("..")
            && !kept.contains(&line)
        {
            let folder = into.join(&line);
            if folder.is_dir() {
                std::fs::remove_dir_all(&folder)?;
            }
        }
    }
    Ok(())
}

/// What the fetcher brings: each recording once, whole — not each item
/// cut from it. A song is thousands of items over a few hundred files
/// (Unbreakable: 21 775 over 185), and what streams in is kept, so the
/// file is wanted whole anyway. Each file is placed on the timeline where
/// the first item that plays it does (so the bytes under the playhead
/// still come first), and listed in the order the files are first heard.
fn whole_files(
    takes: Vec<daw::standalone::audio_engine::media_fetch::StreamedTake>,
) -> Vec<daw::standalone::audio_engine::media_fetch::StreamedTake> {
    let mut takes = takes;
    takes.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut seen = std::collections::HashSet::new();
    takes
        .into_iter()
        .filter(|t| seen.insert(t.path.clone()))
        .map(|mut t| {
            let rate = if t.playrate > 0.0 { t.playrate } else { 1.0 };
            // The file's own start on the timeline, as this item has it.
            t.start -= t.source_offset / rate;
            t.source_offset = 0.0;
            // To its end: past the last page, `bytes_for` stops there.
            t.end = t.start + 24.0 * 3600.0;
            t
        })
        .collect()
}

/// How much of one recording a streaming song has.
#[derive(Clone, Debug, PartialEq, facet::Facet)]
pub struct FileStream {
    /// The proxy, as the song names it (`Media/Proxies/Kick.ogg`).
    pub path: String,
    pub bytes: u64,
    pub arrived: u64,
    /// Every byte of it is here.
    pub complete: bool,
}

/// What a song streamed in has: how much of what it plays has arrived,
/// and where it is kept.
#[derive(Clone, Debug, PartialEq, facet::Facet)]
pub struct SongStreams {
    /// The song as opened here (its engine project).
    pub project: String,
    /// Where its files are mirrored (and its streams kept, on disk).
    pub folder: String,
    /// `local` (every recording here), `arriving` (some), or `streaming`
    /// (none yet).
    pub state: String,
    pub files: u32,
    pub complete: u32,
    pub bytes: u64,
    pub arrived: u64,
    /// Played by its reference: the stems wait until asked for.
    pub by_reference: bool,
    /// The recordings, the least arrived first.
    pub recordings: Vec<FileStream>,
    /// Whether it can play from its playhead (filled by the action).
    pub readiness: Option<Readiness>,
}

/// Every song streaming in this process, and how far along each is.
#[must_use]
pub fn report() -> Vec<SongStreams> {
    let Ok(streams) = STREAMS.lock() else {
        return Vec::new();
    };
    streams
        .iter()
        .map(|s| {
            let takes = s.takes.lock().map(|t| t.clone()).unwrap_or_default();
            let mut recordings: Vec<FileStream> = takes
                .iter()
                .chain(s.held.iter().map(|(t, _)| t))
                .map(|t| {
                    let (bytes, arrived) = (t.bytes.len(), t.bytes.arrived());
                    FileStream {
                        path: t.path.clone(),
                        bytes,
                        arrived,
                        complete: arrived >= bytes,
                    }
                })
                .collect();
            recordings.sort_by(|a, b| {
                let part = |f: &FileStream| f.arrived as f64 / f.bytes.max(1) as f64;
                part(a).total_cmp(&part(b))
            });
            let complete = recordings.iter().filter(|f| f.complete).count();
            let (bytes, arrived) = recordings
                .iter()
                .fold((0, 0), |(b, a), f| (b + f.bytes, a + f.arrived));
            let state = if complete == recordings.len() {
                "local"
            } else if arrived > 0 {
                "arriving"
            } else {
                "streaming"
            };
            SongStreams {
                project: s.project.clone(),
                folder: s
                    ._song
                    .project
                    .parent()
                    .map_or_else(String::new, |p| p.display().to_string()),
                state: state.to_owned(),
                files: u32::try_from(recordings.len()).unwrap_or(u32::MAX),
                complete: u32::try_from(complete).unwrap_or(u32::MAX),
                bytes,
                arrived,
                by_reference: !s.held.is_empty(),
                recordings,
                readiness: None,
            }
        })
        .collect()
}

/// How far a streamed song can play from where it is, every recording it
/// plays there.
#[derive(Clone, Debug, PartialEq, facet::Facet)]
pub struct Readiness {
    /// The song (its engine project).
    pub project: String,
    /// Where it would play from, seconds.
    pub playhead: f64,
    /// Seconds from the playhead every playing recording has (up to
    /// [`LOOK_AHEAD`]).
    pub buffered: f64,
    /// It may play: [`READY_AHEAD`] seconds of everything are here.
    pub ready: bool,
    /// Recordings missing something in the next [`READY_AHEAD`] seconds.
    pub waiting_on: u32,
}

/// Seconds of every recording a song needs ahead of the playhead to start.
pub const READY_AHEAD: f64 = 4.0;

/// How far [`readiness`] looks.
pub const LOOK_AHEAD: f64 = 30.0;

/// The transport's playhead and whether it moves, for `project`.
fn transport_of(project: &str) -> Option<(f64, bool)> {
    let daw = crate::open::local_engine()?;
    let backend = daw.sync_backend(project)?;
    let snap = daw_transport_sync::TransportBackend::snapshot(&backend)?;
    Some((snap.playhead_seconds, snap.is_playing))
}

/// Whether `project` (a song streaming in) can play from its playhead:
/// `None` when it is not streaming (local songs are always ready).
#[must_use]
pub fn readiness(project: &str) -> Option<Readiness> {
    let (playhead, _) = transport_of(project).unwrap_or((0.0, false));
    let takes = {
        let streams = STREAMS.lock().ok()?;
        let s = streams.iter().find(|s| s.project == project)?;
        s.takes.lock().ok()?.clone()
    };
    let missing = |t: f64, ahead: f64| takes.iter().filter(|k| !k.has_at(t, ahead)).count();
    let waiting_on = missing(playhead, READY_AHEAD);
    // Half-second steps out to the look-ahead, while all of it is here.
    let mut buffered = 0.0;
    while buffered < LOOK_AHEAD && missing(playhead + buffered, 0.5) == 0 {
        buffered += 0.5;
    }
    Some(Readiness {
        project: project.to_owned(),
        playhead,
        buffered,
        ready: waiting_on == 0,
        waiting_on: u32::try_from(waiting_on).unwrap_or(u32::MAX),
    })
}

/// Whether `project` is stopped (or not a song here).
#[must_use]
pub fn is_stopped(project: &str) -> bool {
    transport_of(project).is_none_or(|(_, playing)| !playing)
}

/// What the window shows of each streaming song, refreshed every half
/// second ([`watch`]): how much of the recording each playing item reads
/// has arrived (by item guid), and the song's own state. [`version`] moves
/// when anything shown changes.
#[derive(Clone, Debug, Default)]
pub struct Shown {
    /// Item guid → share of its recording that has arrived (0..1), for the
    /// items whose recording is not all here.
    pub items: std::collections::HashMap<String, f32>,
    /// Song (engine project) → its state.
    pub songs: std::collections::HashMap<String, SongShown>,
}

/// A streaming song, as its tab shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SongShown {
    /// Share of everything it plays that has arrived (0..1).
    pub arrived: f32,
    /// It can play from its playhead now.
    pub ready: bool,
    /// Recordings missing near the playhead.
    pub waiting_on: u32,
}

static SHOWN: std::sync::RwLock<Option<Shown>> = std::sync::RwLock::new(None);
static SHOWN_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// What is shown now ([`Shown`]); empty when nothing streams.
#[must_use]
pub fn shown() -> Shown {
    SHOWN
        .read()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or_default()
}

/// Moves when [`shown`] changes: what a window redraws by.
#[must_use]
pub fn version() -> u64 {
    SHOWN_VERSION.load(std::sync::atomic::Ordering::Acquire)
}

/// Keep [`shown`] up to date while songs stream: once per process, from the
/// first song streamed in.
fn watch() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("stream-shown".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let next = compute_shown();
                    let changed = SHOWN
                        .read()
                        .map(|s| s.as_ref().map(rounded) != Some(rounded(&next)))
                        .unwrap_or(true);
                    if changed {
                        if let Ok(mut s) = SHOWN.write() {
                            *s = Some(next);
                        }
                        SHOWN_VERSION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    }
                }
            });
    });
}

/// [`Shown`] at the precision it is drawn at: a change too small to see
/// does not ask for a frame.
fn rounded(s: &Shown) -> (Vec<(String, u8)>, Vec<(String, u8, bool, u32)>) {
    let pct = |f: f32| (f * 50.0).round() as u8;
    let mut items: Vec<(String, u8)> = s.items.iter().map(|(k, v)| (k.clone(), pct(*v))).collect();
    items.sort();
    let mut songs: Vec<(String, u8, bool, u32)> = s
        .songs
        .iter()
        .map(|(k, v)| (k.clone(), pct(v.arrived), v.ready, v.waiting_on))
        .collect();
    songs.sort();
    (items, songs)
}

fn compute_shown() -> Shown {
    let Some(daw) = crate::open::local_engine() else {
        return Shown::default();
    };
    let mut shown = Shown::default();
    for song in report() {
        // A recording's share arrived, by its file stem (what an item's
        // original and its proxy share).
        let by_stem: std::collections::HashMap<String, f32> = song
            .recordings
            .iter()
            .map(|r| {
                let stem = std::path::Path::new(&r.path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                (stem, r.arrived as f32 / r.bytes.max(1) as f32)
            })
            .collect();
        for m in playing_media(&daw, &song.project) {
            let stem = std::path::Path::new(&m.path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if let Some(&part) = by_stem.get(&stem)
                && part < 1.0
            {
                shown.items.insert(m.item_guid, part);
            }
        }
        let ready = readiness(&song.project);
        shown.songs.insert(
            song.project.clone(),
            SongShown {
                arrived: song.arrived as f32 / song.bytes.max(1) as f32,
                ready: ready.as_ref().is_none_or(|r| r.ready),
                waiting_on: ready.map_or(0, |r| r.waiting_on),
            },
        );
    }
    shown
}
