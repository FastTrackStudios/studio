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
    crate::setlist::forget_pictures();
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
    ARRIVING.store(rest.len(), std::sync::atomic::Ordering::Release);
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
                    ARRIVING
                        .fetch_update(
                            std::sync::atomic::Ordering::AcqRel,
                            std::sync::atomic::Ordering::Acquire,
                            |n| n.checked_sub(1),
                        )
                        .ok();
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
/// add to its set; `None` while another holds them, or when nothing is
/// arriving. Given back when the holder drops them: a window mounted again
/// (the welcome screen handing the set over) picks up where the last one
/// stopped — taken for good, the songs after it never arrived.
pub fn take_arrivals() -> Option<Arrivals> {
    ARRIVALS.lock().ok()?.take().map(|rx| Arrivals(Some(rx)))
}

/// The arrivals, held — given back on drop ([`take_arrivals`]).
pub struct Arrivals(Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>>);

impl Arrivals {
    /// The next song to arrive; `None` once every one has.
    pub async fn recv(&mut self) -> Option<Arrival> {
        self.0.as_mut()?.recv().await
    }
}

impl Drop for Arrivals {
    fn drop(&mut self) {
        if let (Some(rx), Ok(mut slot)) = (self.0.take(), ARRIVALS.lock())
            && slot.is_none()
        {
            *slot = Some(rx);
        }
    }
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
///
/// Every streamed song is heard from its Ogg proxies first. A song with a
/// reference ([`crate::reference`]) is heard by it the moment it opens —
/// its preview, then the reference proper — while its stems come in
/// behind it, fetched and decoded but not heard ([`Held`]); once they have
/// arrived where the song is, they take over ([`conduct`]). The tracks the
/// reference leaves out (the guide) play live throughout ([`Self::live`]).
/// At full resolution ([`crate::stream_quality`]) the originals follow,
/// once every song's proxies are here.
struct Streaming {
    song: Arc<StreamedSong>,
    /// The song as opened here.
    project: String,
    /// Where the song is, for the fetches that follow it.
    playhead: Arc<dyn Fn() -> f64 + Send + Sync>,
    /// The recordings heard whatever the song is heard by (each file once,
    /// [`whole_files`]): every one when it has no reference, else the
    /// tracks the reference leaves out.
    live: Vec<StreamedTake>,
    /// The stems a reference stands in for (each file once): heard once
    /// they take over from it.
    stems: Vec<StreamedTake>,
    /// The reference's preview and the reference proper, while the song is
    /// heard by them.
    reference: Vec<StreamedTake>,
    /// Heard by its reference: the stems, held.
    held: Option<Held>,
    /// When it was streamed in.
    since: web_time::Instant,
    /// Every take attached, and the file it plays (as the project names
    /// it): what moves to its original at full resolution.
    media: Vec<(String, String)>,
    /// The takes moved to their originals.
    on_originals: std::collections::HashSet<String>,
    _fetching: FetchGuard,
    /// The stems' own fetch, once they have taken over.
    _stems: Option<FetchGuard>,
    /// The reference's own fetch, while it is heard by it.
    _reference: Option<FetchGuard>,
}

/// A song's stems while its reference plays: attached to streams that are
/// never fed (their waveforms draw), and the streams that will play them,
/// fetched and decoded behind the reference until they take over.
struct Held {
    /// Not yet adopted: nothing decodes them.
    waiting: Vec<crate::song_stream::Detached>,
    /// Their bytes are being fetched — a fetch of their own: one that has
    /// brought everything in its list is over, so stems added to the live
    /// tracks' fetch once it had were never fetched.
    fetching: Option<FetchGuard>,
    /// Adopted (decoding) since then: each take and the stream it will play.
    decoding: Option<(
        web_time::Instant,
        Vec<(
            String,
            StreamedTake,
            daw::standalone::audio_engine::streamed::Streamed,
        )>,
    )>,
}

static STREAMS: Mutex<Vec<Streaming>> = Mutex::new(Vec::new());

/// Songs of the open still to arrive behind the first ([`open`]): until
/// they have, the set's references are not all here, nor its proxies.
static ARRIVING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Attach every take of the song just opened as `local` to a stream of its
/// proxy in `song`, and start bringing their bytes in from the song's
/// playhead on.
///
/// A song with a reference is heard by it at once (its preview first): its
/// bytes are fetched first, and the tracks it leaves out (the guide) play
/// live over it. Its stems are attached (their waveforms draw) and held;
/// their bytes come in behind the reference ([`stems_may_start`]), and they
/// take over once they have arrived where the song is ([`conduct`]). A song
/// whose stems are all here already (kept from before) is heard by them
/// straight away.
pub fn stream(song: StreamedSong, local: &str) {
    let Some(daw) = crate::open::local_engine() else {
        tracing::warn!("stream-set: no engine to stream into");
        return;
    };
    let song = Arc::new(song);
    let playhead: Arc<dyn Fn() -> f64 + Send + Sync> = match daw.sync_backend(local) {
        Some(backend) => Arc::new(move || {
            daw_transport_sync::TransportBackend::snapshot(&backend)
                .map_or(0.0, |s| s.playhead_seconds)
        }),
        None => Arc::new(|| 0.0),
    };
    let reference = song.reference();
    // What the reference leaves out plays live.
    let live_tracks = if reference.is_some() {
        crate::reference::left_out(daw, local)
    } else {
        std::collections::HashSet::new()
    };
    // What plays: each item's active take on a lane that sounds. Other
    // takes and lanes are fetched when they are needed (comping).
    let media = playing_media(daw, local);
    let mut live = Vec::new();
    let mut detached = Vec::new();
    let mut attached = Vec::new();
    for m in &media {
        let stem = reference.is_some() && !live_tracks.contains(&m.track_guid);
        let take = if stem {
            song.attach_detached(daw, local, m).map(|d| {
                let take = d.take.clone();
                detached.push(d);
                take
            })
        } else {
            song.attach(daw, local, m).map(|a| {
                live.push(a.take.clone());
                a.take
            })
        };
        if take.is_none() {
            tracing::warn!(stream.media = %m.path, "stream-set: no indexed proxy for this take");
            continue;
        }
        attached.push((m.take_guid.clone(), m.path.clone()));
    }
    let live = whole_files(live);
    let stems = whole_files(detached.iter().map(|d| d.take.clone()).collect());
    // Kept from before, every stem is here: no reference.
    let stems_here = stems.iter().all(StreamedTake::complete);
    let (held, reference_fetch, reference_takes) = match reference {
        Some(_) if stems_here => {
            hear_now(daw, local, detached);
            (None, None, Vec::new())
        }
        Some(crate::song_stream::Reference { full, preview }) => {
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
            let takes: Vec<StreamedTake> = preview_take.into_iter().chain([full.0]).collect();
            let stop = Arc::new(AtomicBool::new(false));
            song.fetch(
                Arc::new(Mutex::new(takes.clone())),
                Arc::clone(&playhead),
                Arc::clone(&stop),
            );
            let held = Held {
                waiting: detached,
                fetching: None,
                decoding: None,
            };
            (Some(held), Some(FetchGuard(stop)), takes)
        }
        None => (None, None, Vec::new()),
    };
    // Heard by its stems from the start, they are fetched from the start.
    let fetched: Vec<StreamedTake> = if held.is_some() {
        live.clone()
    } else {
        live.iter().chain(&stems).cloned().collect()
    };
    tracing::info!(
        stream.song = local,
        stream.media = media.len(),
        stream.live = live.len(),
        stream.stems = stems.len(),
        stream.by_reference = held.is_some(),
        stream.stems_here = stems_here,
        "stream-set: takes attached to their streams"
    );
    let takes = Arc::new(Mutex::new(fetched));
    let stop = Arc::new(AtomicBool::new(false));
    song.fetch(Arc::clone(&takes), Arc::clone(&playhead), Arc::clone(&stop));
    watch();
    conduct();
    if let Ok(mut streams) = STREAMS.lock() {
        streams.push(Streaming {
            song,
            project: local.to_owned(),
            playhead,
            live,
            stems,
            reference: reference_takes,
            held,
            since: web_time::Instant::now(),
            media: attached,
            on_originals: std::collections::HashSet::new(),
            _fetching: FetchGuard(stop),
            _stems: None,
            _reference: reference_fetch,
        });
    }
}

/// Hear `stems` now: each one's stream decoded and attached to its take.
fn hear_now(
    daw: &daw::standalone::Standalone,
    project: &str,
    stems: Vec<crate::song_stream::Detached>,
) {
    for d in stems {
        let source = d.feeder.source().clone();
        daw::standalone::audio_engine::streamed::butler_adopt(d.feeder);
        daw::standalone::audio_engine::materialize::attach_source(
            daw,
            project,
            &d.take_guid,
            daw::standalone::audio_engine::AudioSource::Streamed(source),
        );
    }
}

/// What decides whether a song's stems start coming in behind its
/// reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StemGate {
    /// All of its reference (and preview) is here.
    pub reference_here: bool,
    /// It is the song on screen.
    pub current: bool,
    /// Every song of the set has arrived, and every one's reference is
    /// here.
    pub every_reference_here: bool,
    /// The multitracks were asked for ([`crate::reference_play`]).
    pub asked: bool,
    /// It has waited [`STEMS_WAIT`] already.
    pub waited: bool,
}

/// How long a song's stems wait on the references before coming in
/// regardless: a reference that will not come must not keep them out.
pub const STEMS_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Whether a song heard by its reference starts fetching its stems: the
/// song on screen once its own reference is here; every other once every
/// song's is — so the whole set is playable before any song's multitracks
/// take the bandwidth. Asked for, or kept waiting too long, at once.
#[must_use]
pub const fn stems_may_start(gate: StemGate) -> bool {
    gate.asked
        || gate.waited
        || (gate.reference_here && (gate.current || gate.every_reference_here))
}

/// Seconds of every stem a song needs ahead of where it is before they
/// take over from its reference: well past what a start needs
/// ([`READY_AHEAD`]), so a stem that comes in late is not heard as a gap
/// after the reference has gone.
const HANDOVER_AHEAD: f64 = 30.0;

/// Seconds a song's held stems, decoding, may take to be decoded where it
/// is before they take over all the same.
const DECODE_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

/// Look after every streaming song every quarter second, from the first
/// song streamed in, for as long as the process runs ([`conduct_once`]).
fn conduct() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("stream-conduct".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    conduct_once();
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "stream-set: no thread to look after the streams");
        }
    });
}

/// One look at every streaming song: stems whose turn has come start
/// coming in; stems that have arrived where their song is start decoding,
/// and take over from its reference once decoded there; at full
/// resolution, the originals come down once every proxy is here, and a
/// stopped song's takes move to the originals that have.
fn conduct_once() {
    let Some(daw) = crate::open::local_engine() else {
        return;
    };
    let current = crate::open::current_song();
    let arriving = ARRIVING.load(std::sync::atomic::Ordering::Acquire);
    let asked = crate::reference_play::multitracks();
    let quality = crate::stream_quality::current();
    let Ok(mut streams) = STREAMS.lock() else {
        return;
    };
    let every_reference_here = arriving == 0
        && streams
            .iter()
            .all(|s| s.held.is_none() || s.reference.iter().all(StreamedTake::complete));
    for s in streams.iter_mut() {
        let at = transport_of(&s.project).map_or(0.0, |(at, _)| at);
        let gate = StemGate {
            reference_here: s.reference.iter().all(StreamedTake::complete),
            current: current.as_deref() == Some(s.project.as_str()),
            every_reference_here,
            asked,
            waited: s.since.elapsed() >= STEMS_WAIT,
        };
        let Some(held) = s.held.as_mut() else {
            continue;
        };
        if held.fetching.is_none() && stems_may_start(gate) {
            let stop = Arc::new(AtomicBool::new(false));
            s.song.fetch(
                Arc::new(Mutex::new(s.stems.clone())),
                Arc::clone(&s.playhead),
                Arc::clone(&stop),
            );
            held.fetching = Some(FetchGuard(stop));
            tracing::info!(stream.song = %s.project, stream.asked = asked, "stream-set: the stems come in behind the reference");
        }
        if held.fetching.is_some()
            && held.decoding.is_none()
            && s.stems.iter().all(|t| t.has_at(at, HANDOVER_AHEAD))
        {
            let decoding = std::mem::take(&mut held.waiting)
                .into_iter()
                .map(|d| {
                    let source = d.feeder.source().clone();
                    source.want(frame_at(&d.take, &source, at));
                    daw::standalone::audio_engine::streamed::butler_adopt(d.feeder);
                    (d.take_guid, d.take, source)
                })
                .collect();
            held.decoding = Some((web_time::Instant::now(), decoding));
        }
        let Some((began, decoding)) = held.decoding.as_ref() else {
            continue;
        };
        for (_, take, source) in decoding {
            source.want(frame_at(take, source, at));
        }
        let decoded = decoding
            .iter()
            .all(|(_, take, source)| arrived_at(take, source, at));
        let late = began.elapsed() >= DECODE_WITHIN;
        if !(decoded || late) {
            continue;
        }
        // In one go, between two blocks: the stems in, the reference out.
        for (take_guid, _, source) in decoding {
            daw::standalone::audio_engine::materialize::attach_source(
                daw,
                &s.project,
                take_guid,
                daw::standalone::audio_engine::AudioSource::Streamed(source.clone()),
            );
        }
        crate::reference_play::stop(&s.project);
        // Their fetch goes on with the song (the rest of the stems).
        let stems_fetch = s.held.take().and_then(|h| h.fetching);
        s._stems = stems_fetch;
        s.reference.clear();
        s._reference = None;
        tracing::info!(
            stream.song = %s.project,
            stream.late = late,
            stream.after_ms = u64::try_from(s.since.elapsed().as_millis()).unwrap_or(u64::MAX),
            "stream-set: the stems take over from the reference"
        );
    }
    if quality == crate::stream_quality::Quality::Full {
        full_resolution(daw, &mut streams, arriving);
    }
}

/// What the songs streaming have, for whether the originals may come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetProgress {
    /// Songs still to arrive.
    pub arriving: usize,
    /// Songs still heard by their reference.
    pub by_reference: usize,
    /// Songs with a proxy not all here.
    pub proxies_missing: usize,
}

/// Whether the originals may start coming down: every song of the set has
/// arrived, none is heard by its reference, and every proxy is here — the
/// originals are the lowest priority there is.
#[must_use]
pub const fn originals_may_start(set: SetProgress) -> bool {
    set.arriving == 0 && set.by_reference == 0 && set.proxies_missing == 0
}

/// Whether the originals are being fetched.
static ORIGINALS: AtomicBool = AtomicBool::new(false);

/// Originals that would not come down this run: not tried again.
static ORIGINALS_FAILED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Full resolution, one look: once every proxy is here, the originals
/// still missing come down one at a time, the songs in the order they
/// opened, each song's in the order they are heard; and every stopped
/// song's takes whose original is here move to it — from the next block,
/// at the same place on the timeline, so the swap is not heard as a jump
/// (a playing song's wait for it to stop).
fn full_resolution(daw: &daw::standalone::Standalone, streams: &mut [Streaming], arriving: usize) {
    let progress = SetProgress {
        arriving,
        by_reference: streams.iter().filter(|s| s.held.is_some()).count(),
        proxies_missing: streams
            .iter()
            .filter(|s| !s.live.iter().chain(&s.stems).all(StreamedTake::complete))
            .count(),
    };
    if !originals_may_start(progress) {
        return;
    }
    let failed = ORIGINALS_FAILED
        .lock()
        .map(|f| f.clone())
        .unwrap_or_default();
    // What is here, by file: looked at once a look, not once a take.
    let mut here: std::collections::HashMap<PathBuf, bool> = std::collections::HashMap::new();
    let mut wanted: Vec<(Arc<StreamedSong>, crate::song_stream::Original)> = Vec::new();
    for s in streams.iter_mut() {
        let stopped = is_stopped(&s.project);
        let mut moved = 0usize;
        for (take_guid, path) in &s.media {
            let Some(original) = s.song.original_of(path) else {
                continue;
            };
            let is_here = *here
                .entry(original.local.clone())
                .or_insert_with(|| original.is_here());
            if !is_here {
                if !failed.contains(&original.local)
                    && !wanted.iter().any(|(_, o)| o.local == original.local)
                {
                    wanted.push((Arc::clone(&s.song), original));
                }
                continue;
            }
            if !stopped || s.on_originals.contains(take_guid) {
                continue;
            }
            match daw::standalone::audio_engine::PcmFile::open(&original.local) {
                Ok(pcm) => {
                    daw::standalone::audio_engine::materialize::attach_source(
                        daw,
                        &s.project,
                        take_guid,
                        daw::standalone::audio_engine::AudioSource::PcmFile(pcm),
                    );
                    moved += 1;
                }
                Err(e) => {
                    tracing::warn!(stream.original = %original.path, error = %e, "stream-set: an original would not open; its take stays on its proxy");
                }
            }
            s.on_originals.insert(take_guid.clone());
        }
        if moved > 0 {
            tracing::info!(stream.song = %s.project, stream.moved = moved, "stream-set: takes moved to their originals");
        }
    }
    if wanted.is_empty() || ORIGINALS.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let Some(runtime) = crate::open::runtime() else {
        ORIGINALS.store(false, std::sync::atomic::Ordering::Release);
        return;
    };
    runtime.spawn(async move {
        let count = wanted.len();
        let mut fetched = 0usize;
        for (song, original) in wanted {
            // Asked for proxies only meanwhile: not another original.
            if crate::stream_quality::current() != crate::stream_quality::Quality::Full {
                break;
            }
            match song.fetch_original(&original).await {
                Ok(()) => fetched += 1,
                Err(e) => {
                    tracing::warn!(stream.original = %original.path, error = %e, "stream-set: an original did not come down; its take stays on its proxy");
                    if let Ok(mut failed) = ORIGINALS_FAILED.lock() {
                        failed.push(original.local);
                    }
                }
            }
        }
        tracing::info!(stream.originals = count, stream.fetched = fetched, "stream-set: originals down");
        ORIGINALS.store(false, std::sync::atomic::Ordering::Release);
    });
}

/// Hear every streamed song by its stems as soon as they are here: every
/// song's stems come in now, whatever the references are doing (the
/// Multitracks switch). Each keeps its reference until its stems have
/// arrived and decoded where it is ([`conduct`]), so the sound never drops
/// out or doubles between the two.
pub fn hear_stems() {
    conduct();
}

/// The frame of `source` that `take` plays at the timeline moment `at` —
/// its first, before the take starts.
fn frame_at(
    take: &StreamedTake,
    source: &daw::standalone::audio_engine::streamed::Streamed,
    at: f64,
) -> u64 {
    let seconds = (at.max(take.start) - take.start) * take.playrate + take.source_offset;
    (seconds.max(0.0) * f64::from(source.sample_rate())) as u64
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
    let frame = usize::try_from(frame_at(take, source, at)).unwrap_or(usize::MAX);
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
    /// Heard by its reference now: its stems are coming in behind it.
    pub by_reference: bool,
    /// The original recordings it can move to at full resolution, and how
    /// many of them are here.
    pub originals: u32,
    pub originals_here: u32,
    /// Takes moved to their originals.
    pub on_originals: u32,
    /// The recordings, the least arrived first.
    pub recordings: Vec<FileStream>,
    /// Whether it can play from its playhead (filled by the action).
    pub readiness: Option<Readiness>,
}

/// Every song streaming in this process, and how far along each is: its
/// live tracks, its stems and — while it is heard by it — its reference.
#[must_use]
pub fn report() -> Vec<SongStreams> {
    let Ok(streams) = STREAMS.lock() else {
        return Vec::new();
    };
    streams.iter().map(song_report).collect()
}

fn song_report(s: &Streaming) -> SongStreams {
    let mut recordings: Vec<FileStream> = s
        .live
        .iter()
        .chain(&s.stems)
        .chain(&s.reference)
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
    let mut originals: Vec<crate::song_stream::Original> = Vec::new();
    for (_, path) in &s.media {
        if let Some(o) = s.song.original_of(path)
            && !originals.iter().any(|seen| seen.local == o.local)
        {
            originals.push(o);
        }
    }
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    SongStreams {
        project: s.project.clone(),
        folder: s
            .song
            .project
            .parent()
            .map_or_else(String::new, |p| p.display().to_string()),
        state: state.to_owned(),
        files: count(recordings.len()),
        complete: count(complete),
        bytes,
        arrived,
        by_reference: s.held.is_some(),
        originals: count(originals.len()),
        originals_here: count(originals.iter().filter(|o| o.is_here()).count()),
        on_originals: count(s.on_originals.len()),
        recordings,
        readiness: None,
    }
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

/// Whether `project` (a song streaming in) can play from its playhead —
/// by what it is heard by now: its live tracks, and its stems or its
/// reference (the preview will do). `None` when it is not streaming
/// (local songs are always ready).
#[must_use]
pub fn readiness(project: &str) -> Option<Readiness> {
    let (playhead, _) = transport_of(project).unwrap_or((0.0, false));
    let (heard, reference) = {
        let streams = STREAMS.lock().ok()?;
        let s = streams.iter().find(|s| s.project == project)?;
        let mut heard = s.live.clone();
        if s.held.is_none() {
            heard.extend(s.stems.iter().cloned());
        }
        (heard, s.held.as_ref().map(|_| s.reference.clone()))
    };
    Some(readiness_of(
        project,
        playhead,
        &heard,
        reference.as_deref(),
    ))
}

/// [`readiness`], of what a song is heard by: `heard` (each recording that
/// sounds) and, heard by its reference, its preview and reference proper
/// (`reference`) — either of which will do.
fn readiness_of(
    project: &str,
    playhead: f64,
    heard: &[StreamedTake],
    reference: Option<&[StreamedTake]>,
) -> Readiness {
    let reference_at = |t: f64, ahead: f64| {
        reference.is_none_or(|r| r.is_empty() || r.iter().any(|k| k.has_at(t, ahead)))
    };
    let missing = |t: f64, ahead: f64| {
        heard.iter().filter(|k| !k.has_at(t, ahead)).count() + usize::from(!reference_at(t, ahead))
    };
    let waiting_on = missing(playhead, READY_AHEAD);
    // Half-second steps out to the look-ahead, while all of it is here.
    let mut buffered = 0.0;
    while buffered < LOOK_AHEAD && missing(playhead + buffered, 0.5) == 0 {
        buffered += 0.5;
    }
    Readiness {
        project: project.to_owned(),
        playhead,
        buffered,
        ready: waiting_on == 0,
        waiting_on: u32::try_from(waiting_on).unwrap_or(u32::MAX),
    }
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

#[cfg(test)]
mod gate_tests {
    use super::{SetProgress, StemGate, originals_may_start, stems_may_start};

    const WAITING: StemGate = StemGate {
        reference_here: false,
        current: false,
        every_reference_here: false,
        asked: false,
        waited: false,
    };

    #[test]
    fn the_song_on_screen_takes_its_stems_once_its_reference_is_here_the_rest_once_every_one_is() {
        // Its reference not here yet: the reference first, on screen or not.
        assert!(!stems_may_start(StemGate {
            current: true,
            ..WAITING
        }));
        let here = StemGate {
            reference_here: true,
            ..WAITING
        };
        assert!(stems_may_start(StemGate {
            current: true,
            ..here
        }));
        assert!(
            !stems_may_start(here),
            "another song's reference is still coming"
        );
        assert!(stems_may_start(StemGate {
            every_reference_here: true,
            ..here
        }));
        // Asked for, or waited long enough: at once.
        assert!(stems_may_start(StemGate {
            asked: true,
            ..WAITING
        }));
        assert!(stems_may_start(StemGate {
            waited: true,
            ..WAITING
        }));
    }

    #[test]
    fn originals_come_only_once_every_song_has_arrived_with_every_proxy_and_none_on_its_reference()
    {
        let done = SetProgress {
            arriving: 0,
            by_reference: 0,
            proxies_missing: 0,
        };
        assert!(originals_may_start(done));
        assert!(!originals_may_start(SetProgress {
            arriving: 1,
            ..done
        }));
        assert!(!originals_may_start(SetProgress {
            by_reference: 1,
            ..done
        }));
        assert!(!originals_may_start(SetProgress {
            proxies_missing: 3,
            ..done
        }));
    }
}
