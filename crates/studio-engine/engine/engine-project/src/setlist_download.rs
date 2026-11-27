//! A setlist downloaded ahead of a gig, to open and play with no internet
//! (studio#716) — `SETLIST_DOWNLOAD`, `SETLIST_DOWNLOADING`:
//!
//! - the setlist comes from a share link (`https://<node>/s/<code>/`,
//!   [`engine_runtime::link::LinkSet`]): its songs in running order, each a
//!   [`SongSource`];
//! - each song comes down the one way a song comes down
//!   ([`crate::song_stream::download`]): its small files mirrored, so it
//!   opens the one way songs open, and its proxies fetched whole. What
//!   else is kept is [`Kept`]: on a phone or in a browser the proxies only
//!   (or the reference and the guide), on a desktop the full files as
//!   well — every recording the link serves;
//! - the songs land side by side under one folder, beside a `.setlist`
//!   naming them in order, which `SESSION_OPEN` opens from disk like any
//!   other. A song opened from disk with no original plays its proxy (the
//!   engine's `ProjectRelativeResolver`).
//!
//! `SESSION_PREPARE` makes a song ready to be shared and downloaded in
//! place — its proxies and peaks (`crate::song_prep::prepare`), and its
//! reference when asked — what `link-prepare` does, as an action.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use architect::action::{ActionList, ActionMeta, Effect};
use engine_runtime::source::{Listing, Pending, SongSource};
use facet::Facet;
use tracing::Instrument as _;

use crate::song_stream::Offline;

static DOWNLOAD: ActionMeta = crate::actions::meta(
    "SETLIST_DOWNLOAD",
    "setlist_download",
    "Download setlist",
    "Download a setlist onto this device so it opens and plays with no internet. It comes from `from`, the setlist's share link (`https://<node>/s/<code>/`), or from `songs`, each song's `folder` (or project file) on this device and its `title`, in running order, the setlist called `name`. `keep` says what comes down: `full` (every file the link serves, the original recordings too: a desktop's default), `proxies` (each song's small files and the Ogg proxies of every track, never the originals: a phone's and a browser's default) or `reference` (the small files, the reference mix and the guide's proxies only). `to` is the folder the setlist goes in (a folder per setlist under the device's offline folder when not said). It runs in the background and returns at once; SETLIST_DOWNLOADING says how far it is, and once it is `done` its `setlist` is the `.setlist` file to SESSION_OPEN, with or without a network. A song already downloaded keeps every file that is here at its size.",
    "Setlist",
    "",
);

static DOWNLOADING: ActionMeta = crate::actions::meta(
    "SETLIST_DOWNLOADING",
    "setlist_downloading",
    "Setlist download",
    "How the last SETLIST_DOWNLOAD is going: its `state` (`idle`, `downloading`, `done` or `failed`, with its `error`), what it `keep`s, the `song` it is on of how many `songs` and its `title`, the proxies' and files' `bytes` fetched of the `total` for that song, and once done the `setlist` file to SESSION_OPEN and each song `downloaded`: its `title`, its `folder`, and what is on the device — how many `files` and `bytes`, how many `recordings` (the originals) and `proxies`. Read-only.",
    "Setlist",
    "",
);

static PREPARE: ActionMeta = crate::actions::meta(
    "SESSION_PREPARE",
    "session_prepare",
    "Prepare for sharing",
    "Make a song ready to be shared and downloaded, in place: its project file folded whole, an Ogg proxy (`Proxies/<stem>.ogg`, with its page index) and the waveform peaks of every recording it plays, and its reference mix when `reference` is true. `path` is a song folder, a `.session` or `.RPP`, or a folder of song folders. Each step keeps what is already there and current. Returns, per song, how many proxies and peaks were made or were current, and any that failed.",
    "Session",
    "",
);

/// What a download keeps on the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    /// Everything the link serves: the small files, the proxies and the
    /// original recordings. A desktop's.
    Full,
    /// The small files and every track's proxy, never an original. A
    /// phone's and a browser's.
    Proxies,
    /// The small files, the reference mix and the guide's proxies.
    Reference,
}

impl Kept {
    /// This device's: the full files on a desktop, the proxies on a phone
    /// or tablet.
    #[must_use]
    pub const fn for_this_device() -> Self {
        if cfg!(target_os = "ios") {
            Self::Proxies
        } else {
            Self::Full
        }
    }

    /// As `SETLIST_DOWNLOAD`'s `keep` names it.
    ///
    /// # Errors
    ///
    /// It names none.
    pub fn named(name: &str) -> Result<Self, String> {
        match name.trim().to_lowercase().as_str() {
            "full" | "originals" => Ok(Self::Full),
            "proxies" | "proxy" | "multitracks" => Ok(Self::Proxies),
            "reference" => Ok(Self::Reference),
            other => Err(format!("keep {other:?}: `full`, `proxies` or `reference`")),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Proxies => "proxies",
            Self::Reference => "reference",
        }
    }

    /// The proxies [`crate::song_stream::download`] fetches for it.
    const fn offline(self) -> Offline {
        match self {
            Self::Full | Self::Proxies => Offline::Multitracks,
            Self::Reference => Offline::Reference,
        }
    }
}

/// One song as it is on the device after its download.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct DownloadedSong {
    pub title: String,
    /// Its folder on the device.
    pub folder: String,
    pub files: u32,
    pub bytes: u64,
    /// Original recordings (WAV, FLAC, AIFF, …) on the device.
    pub recordings: u32,
    /// Ogg proxies on the device.
    pub proxies: u32,
}

/// What `SETLIST_DOWNLOADING` says.
#[derive(Facet, Clone, Debug, Default)]
pub struct Downloading {
    /// `idle`, `downloading`, `done` or `failed`.
    pub state: String,
    pub keep: Option<String>,
    /// The share link it comes from.
    pub from: Option<String>,
    /// The setlist's name, as the link titles it.
    pub name: Option<String>,
    /// The song coming down (from 1), and how many there are.
    pub song: u32,
    pub songs: u32,
    pub title: Option<String>,
    /// That song's bytes fetched, of its total.
    pub bytes: u64,
    pub total: u64,
    /// Once done: the `.setlist` file to open.
    pub setlist: Option<String>,
    pub downloaded: Vec<DownloadedSong>,
    pub error: Option<String>,
}

fn status() -> &'static Mutex<Downloading> {
    static STATUS: std::sync::OnceLock<Mutex<Downloading>> = std::sync::OnceLock::new();
    STATUS.get_or_init(|| {
        Mutex::new(Downloading {
            state: "idle".to_owned(),
            ..Downloading::default()
        })
    })
}

fn update(f: impl FnOnce(&mut Downloading)) {
    if let Ok(mut s) = status().lock() {
        f(&mut s);
    }
}

/// A song source under the name its folder gets on the device (the song's
/// title, not the link's code and vault path).
struct Named {
    name: String,
    inner: Arc<dyn SongSource>,
}

impl SongSource for Named {
    fn label(&self) -> String {
        self.name.clone()
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        self.inner.list()
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        self.inner.read(path, range)
    }

    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        self.inner.read_many(paths)
    }

    fn fetch_whole(
        &self,
        path: String,
        into: PathBuf,
        got: Arc<std::sync::atomic::AtomicU64>,
    ) -> Pending<Option<eyre::Result<()>>> {
        self.inner.fetch_whole(path, into, got)
    }
}

/// A name for a folder or file on any device's disk: letters, digits and
/// `-`, anything else `_` — the rule the mirror names a song's folder by
/// (its label, `engine_runtime::song_stream`), so two names are the same
/// folder exactly when they are the same name here.
fn file_name(text: &str) -> String {
    let name: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches('_');
    if name.is_empty() {
        "Setlist".to_owned()
    } else {
        name.to_owned()
    }
}

/// Each song's folder name, by its title, one folder per song even when
/// two share a title.
fn folder_names(titles: &[String]) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(titles.len());
    for (i, title) in titles.iter().enumerate() {
        let base = if title.trim().is_empty() {
            format!("Song_{}", i + 1)
        } else {
            file_name(title)
        };
        let mut name = base.clone();
        let mut n = 2;
        while names.iter().any(|taken| taken.eq_ignore_ascii_case(&name)) {
            name = format!("{base}_{n}");
            n += 1;
        }
        names.push(name);
    }
    names
}

/// The `.setlist` naming `folders` (relative to it) in order.
fn setlist_text(name: &str, from: &str, folders: &[String]) -> String {
    let mut text = format!("# {name}, downloaded from {from} to play offline.\n");
    for folder in folders {
        text.push_str(folder);
        text.push('\n');
    }
    text
}

/// Whether `path` (`/`-separated, relative to the song folder) is an Ogg
/// proxy, and whether it is a recording (an original).
fn kind_of(path: &str) -> (bool, bool) {
    const RECORDINGS: &[&str] = &[
        "wav", "wave", "aif", "aiff", "aifc", "flac", "mp3", "m4a", "w64", "caf",
    ];
    let lower = path.to_lowercase();
    let ext = Path::new(&lower)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    (ext == "ogg", RECORDINGS.contains(&ext.as_str()))
}

/// What is in `folder` on this device.
fn on_disk(title: &str, folder: &Path) -> DownloadedSong {
    fn walk(root: &Path, dir: &Path, out: &mut DownloadedSong) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                walk(root, &path, out);
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            out.files += 1;
            out.bytes += meta.len();
            match kind_of(&rel) {
                (true, _) => out.proxies += 1,
                (_, true) => out.recordings += 1,
                _ => {}
            }
        }
    }
    let mut song = DownloadedSong {
        title: title.to_owned(),
        folder: folder.display().to_string(),
        ..DownloadedSong::default()
    };
    walk(folder, folder, &mut song);
    song
}

/// `rel` (`/`-separated) under `folder`, unless it would leave it.
fn inside(folder: &Path, rel: &str) -> Option<PathBuf> {
    use std::path::Component;
    let rel = Path::new(rel);
    rel.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        .then(|| folder.join(rel))
}

/// Fetch every file `source` lists that is not in `folder` at its size:
/// the originals, once the song's small files and proxies are here.
async fn fetch_the_rest(
    source: &dyn SongSource,
    folder: &Path,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> eyre::Result<()> {
    const TRIES: u32 = 4;
    let listing = source.list().await?;
    let wanted: Vec<(String, u64)> = listing
        .entries
        .into_iter()
        .filter(|(path, size)| {
            !path.is_empty()
                && std::fs::metadata(folder.join(path)).map_or(true, |m| m.len() != *size)
        })
        .collect();
    let total: u64 = wanted.iter().map(|(_, size)| size).sum();
    let mut done = 0;
    progress(done, total);
    for (path, size) in wanted {
        let local = inside(folder, &path).ok_or_else(|| eyre::eyre!("{path}: outside the song"))?;
        let got = Arc::new(std::sync::atomic::AtomicU64::new(0));
        engine_runtime::song_stream::fetch_proxy(source, &path, size, &local, &got, TRIES).await?;
        done += size;
        progress(done, total);
    }
    Ok(())
}

/// Where a setlist comes down from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A share link (`https://<node>/s/<code>/`): its songs in list order.
    Link(String),
    /// Song folders this device can read — on its disk, on a drive, or
    /// brought here by Drive's agent: what the app downloads a set from.
    Folders {
        name: String,
        /// Each song's title and its folder (or its project file).
        songs: Vec<(String, PathBuf)>,
    },
}

impl Origin {
    /// What a person reads it as (never a link: that is a capability).
    fn said(&self) -> String {
        match self {
            Self::Link(link) => host_of(link),
            Self::Folders { name, .. } => name.clone(),
        }
    }
}

/// A setlist's name and its songs, each a title and where its files come
/// from.
type Songs = (String, Vec<(String, Arc<dyn SongSource>)>);

/// The songs `origin` names.
async fn songs_of(origin: &Origin) -> eyre::Result<Songs> {
    match origin {
        Origin::Link(link) => {
            let set = engine_runtime::link::LinkSet::open(link).await?;
            let songs = set
                .songs
                .into_iter()
                .map(|song| {
                    let source: Arc<dyn SongSource> = Arc::new(song.clone());
                    (song.title, source)
                })
                .collect();
            Ok((set.title, songs))
        }
        Origin::Folders { name, songs } => {
            let mut out = Vec::with_capacity(songs.len());
            for (title, folder) in songs {
                let source: Arc<dyn SongSource> = Arc::new(FolderSource::at(folder)?);
                out.push((title.clone(), source));
            }
            Ok((name.clone(), out))
        }
    }
}

/// A song folder on this device, served as a source: its files listed and
/// read the way a peer serves its own (`daw_proto::song_files::folder`).
struct FolderSource {
    /// The song's project file, which names its folder.
    project: String,
}

impl FolderSource {
    /// The song at `at`: a song folder (its `.session`, else its `.RPP`), or
    /// the project file itself.
    fn at(at: &Path) -> eyre::Result<Self> {
        let project = if at.is_dir() {
            crate::setlist::song_in(at).ok_or_else(|| eyre::eyre!("no song in {}", at.display()))?
        } else if at.is_file() {
            at.to_path_buf()
        } else {
            eyre::bail!("{} is not on this device", at.display());
        };
        Ok(Self {
            project: project.display().to_string(),
        })
    }
}

impl SongSource for FolderSource {
    fn label(&self) -> String {
        self.project.clone()
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let listed = daw_proto::song_files::folder::list(&self.project)
            .map(|files| {
                files
                    .into_iter()
                    .map(|f| (f.path, f.size))
                    .collect::<Vec<_>>()
                    .into()
            })
            .map_err(|e| eyre::eyre!("{e}"));
        Box::pin(async move { listed })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let project = self.project.clone();
        Box::pin(async move {
            let mut out = Vec::new();
            let mut at = range.start;
            while at < range.end {
                let len =
                    u32::try_from((range.end - at).min(u64::from(daw_proto::song_files::MAX_READ)))
                        .unwrap_or(daw_proto::song_files::MAX_READ);
                let bytes = daw_proto::song_files::folder::read(&project, &path, at, len)
                    .map_err(|e| e.to_string())?;
                if bytes.is_empty() {
                    break;
                }
                at += bytes.len() as u64;
                out.extend(bytes);
            }
            Ok(out)
        })
    }
}

/// Download the setlist `origin` names into `to`, keeping what `kept`
/// says. Returns the `.setlist` written there and each song as it is on
/// the device.
///
/// # Errors
///
/// It names no songs, a song could not be fetched, or the disk could not
/// be written.
pub async fn download(
    origin: &Origin,
    to: &Path,
    kept: Kept,
) -> eyre::Result<(PathBuf, Vec<DownloadedSong>)> {
    let (title, songs) = songs_of(origin).await?;
    if songs.is_empty() {
        eyre::bail!("{} names no songs", origin.said());
    }
    let name = if title.trim().is_empty() {
        "Setlist".to_owned()
    } else {
        title
    };
    let titles: Vec<String> = songs.iter().map(|(t, _)| t.clone()).collect();
    let folders = folder_names(&titles);
    let count = u32::try_from(songs.len()).unwrap_or(u32::MAX);
    update(|s| {
        s.name = Some(name.clone());
        s.songs = count;
    });
    std::fs::create_dir_all(to)?;
    let mut downloaded = Vec::with_capacity(songs.len());
    let mut written = Vec::with_capacity(songs.len());
    for (i, ((title, song), folder)) in songs.into_iter().zip(&folders).enumerate() {
        let number = u32::try_from(i + 1).unwrap_or(u32::MAX);
        update(|s| {
            s.song = number;
            s.title = Some(title.clone());
            s.bytes = 0;
            s.total = 0;
        });
        let span = tracing::info_span!(
            "setlist_download.song",
            song = number,
            title = %title,
            keep = kept.name(),
            bytes = tracing::field::Empty,
            files = tracing::field::Empty,
        );
        let source: Arc<dyn SongSource> = Arc::new(Named {
            name: folder.clone(),
            inner: song,
        });
        let progress = |bytes: u64, total: u64| {
            update(|s| {
                s.bytes = bytes;
                s.total = total;
            });
        };
        let base = async {
            let base =
                crate::song_stream::download(Arc::clone(&source), to, kept.offline(), &progress)
                    .await?;
            if kept == Kept::Full {
                fetch_the_rest(&*source, &base, &progress).await?;
            }
            eyre::Ok(base)
        }
        .instrument(span.clone())
        .await
        .map_err(|e| eyre::eyre!("{title}: {e}"))?;
        written.push(base.strip_prefix(to).map_or_else(
            |_| base.display().to_string(),
            |r| r.to_string_lossy().into_owned(),
        ));
        let here = on_disk(&title, &base);
        span.record("bytes", here.bytes);
        span.record("files", here.files);
        downloaded.push(here);
    }
    let setlist = to.join(format!("{}.setlist", file_name(&name)));
    std::fs::write(&setlist, setlist_text(&name, &origin.said(), &written))?;
    Ok((setlist, downloaded))
}

/// `SETLIST_DOWNLOAD`'s arguments.
#[derive(Facet)]
struct DownloadArg {
    from: Option<String>,
    songs: Option<Vec<SongArg>>,
    name: Option<String>,
    to: Option<String>,
    keep: Option<String>,
}

/// One of `SETLIST_DOWNLOAD`'s `songs`.
#[derive(Facet)]
struct SongArg {
    title: Option<String>,
    folder: String,
}

/// Where the arguments say the setlist comes from.
fn origin_of(arg: &mut DownloadArg) -> Result<Origin, String> {
    match (arg.from.take(), arg.songs.take()) {
        (Some(from), None) => {
            let from = from.trim().to_owned();
            engine_runtime::link::listing_url(&from).map_err(|e| e.to_string())?;
            Ok(Origin::Link(from))
        }
        (None, Some(songs)) if !songs.is_empty() => Ok(Origin::Folders {
            name: arg
                .name
                .take()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Setlist".to_owned()),
            songs: songs
                .into_iter()
                .map(|s| {
                    let folder = PathBuf::from(s.folder.trim());
                    let title = s.title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
                        folder
                            .file_stem()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    });
                    (title, folder)
                })
                .collect(),
        }),
        (Some(_), Some(_)) => {
            Err("`from` (a share link) or `songs` (song folders), not both".to_owned())
        }
        _ => Err(
            "`from` (a setlist's share link) or `songs` (each `folder`, and its `title`)"
                .to_owned(),
        ),
    }
}

/// Where a setlist is downloaded when `to` is not said: under the device's
/// offline folder, by its link's code or its name.
fn default_folder(origin: &Origin) -> PathBuf {
    let root = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("FastTrackStudio")
        .join("Offline");
    let name = match origin {
        Origin::Link(link) => engine_runtime::link::listing_url(link)
            .map(|(_, code)| code)
            .unwrap_or_else(|_| "setlist".to_owned()),
        Origin::Folders { name, .. } => name.clone(),
    };
    root.join(file_name(&name))
}

/// A share link's host, for telemetry: the link itself is a capability.
fn host_of(link: &str) -> String {
    engine_runtime::link::listing_url(link)
        .ok()
        .and_then(|(url, _)| url.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Start a download in the background.
fn start(mut arg: DownloadArg) -> Result<String, String> {
    let kept = arg
        .keep
        .as_deref()
        .map_or(Ok(Kept::for_this_device()), Kept::named)?;
    let origin = origin_of(&mut arg)?;
    let from = origin.said();
    let to = arg
        .to
        .filter(|t| !t.trim().is_empty())
        .map_or_else(|| default_folder(&origin), PathBuf::from);
    {
        let mut s = status().lock().map_err(|e| e.to_string())?;
        if s.state == "downloading" {
            return Err(format!(
                "a setlist is already downloading ({}): wait for SETLIST_DOWNLOADING to be done",
                s.from.as_deref().unwrap_or_default()
            ));
        }
        *s = Downloading {
            state: "downloading".to_owned(),
            keep: Some(kept.name().to_owned()),
            from: Some(from.clone()),
            ..Downloading::default()
        };
    }
    let into = to.clone();
    let said = from.clone();
    std::thread::Builder::new()
        .name("setlist-download".to_owned())
        .spawn(move || {
            let span = tracing::info_span!(
                "setlist_download",
                from = %said,
                to = %into.display(),
                keep = kept.name(),
                songs = tracing::field::Empty,
                error = tracing::field::Empty,
            );
            let _entered = span.enter();
            let outcome = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(eyre::Report::from)
                .and_then(|rt| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        rt.block_on(download(&origin, &into, kept))
                    }))
                    .unwrap_or_else(|_| Err(eyre::eyre!("the download panicked")))
                });
            match outcome {
                Ok((setlist, songs)) => {
                    span.record("songs", songs.len());
                    update(|s| {
                        s.state = "done".to_owned();
                        s.setlist = Some(setlist.display().to_string());
                        s.downloaded = songs;
                    });
                }
                Err(e) => {
                    span.record("error", tracing::field::display(&e));
                    tracing::error!(error = %e, "the setlist did not download");
                    update(|s| {
                        s.state = "failed".to_owned();
                        s.error = Some(format!("{e:#}"));
                    });
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "downloading {from} into {} ({}): SETLIST_DOWNLOADING says when it is done and which setlist to SESSION_OPEN",
        to.display(),
        kept.name()
    ))
}

/// `SESSION_PREPARE`'s arguments.
#[derive(Facet)]
struct PrepareArg {
    path: String,
    reference: Option<bool>,
}

/// One song, as `SESSION_PREPARE` says it.
#[derive(Facet)]
struct PreparedRow {
    title: String,
    folder: String,
    proxies_made: u32,
    proxies_current: u32,
    peaks_made: u32,
    peaks_current: u32,
    reference: String,
    failed: Vec<String>,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn prepare(arg: &PrepareArg) -> Result<Vec<PreparedRow>, String> {
    use crate::song_prep::inputs::{Input, classify, song_at, songs_of};
    use crate::song_prep::prepare::{Done, Steps, prepare};
    let path = PathBuf::from(arg.path.trim());
    let songs = match classify(&path).map_err(|e| e.to_string())? {
        Input::Show(set) => {
            return Err(format!(
                "{}: a Live set is split into songs first",
                set.display()
            ));
        }
        Input::Song(file) => vec![song_at(&file)],
        folder @ Input::Folder(_) => songs_of(&folder).map_err(|e| e.to_string())?,
    };
    let steps = Steps {
        reference: arg.reference.unwrap_or(false),
    };
    songs
        .iter()
        .map(|song| {
            let done = prepare(song, steps).map_err(|e| format!("{}: {e}", song.title))?;
            let mut failed = done.proxies.failed.clone();
            failed.extend(done.peaks.failed.iter().cloned());
            Ok(PreparedRow {
                title: song.title.clone(),
                folder: song.folder.display().to_string(),
                proxies_made: count(done.proxies.made),
                proxies_current: count(done.proxies.current),
                peaks_made: count(done.peaks.made),
                peaks_current: count(done.peaks.current),
                reference: match done.reference {
                    Done::Off => "off",
                    Done::Current => "current",
                    Done::Made => "made",
                }
                .to_owned(),
                failed,
            })
        })
        .collect()
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&DOWNLOAD, Effect::Mutating, start)?;
    list.register(&DOWNLOADING, Effect::ReadOnly, |(): ()| {
        status()
            .lock()
            .map(|s| s.clone())
            .map_err(|e| e.to_string())
    })?;
    list.register(&PREPARE, Effect::Mutating, |a: PrepareArg| prepare(&a))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_song_gets_a_folder_of_its_own_by_its_title() {
        let titles = [
            "Silent Night",
            "O Holy Night",
            "silent night",
            "",
            "A/B: C",
            "A_B_ C",
        ]
        .map(str::to_owned);
        assert_eq!(
            folder_names(&titles),
            [
                "Silent_Night",
                "O_Holy_Night",
                "silent_night_2",
                "Song_4",
                "A_B__C",
                "A_B__C_2",
            ]
        );
    }

    #[test]
    fn the_setlist_names_its_folders_in_running_order() {
        let text = setlist_text("Gig", "https://n/s/x/", &["B".into(), "A".into()]);
        let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(lines, ["B", "A"]);
    }

    #[test]
    fn a_desktop_keeps_the_full_files_and_names_are_taken() {
        assert_eq!(Kept::named("Full"), Ok(Kept::Full));
        assert_eq!(Kept::named("proxies"), Ok(Kept::Proxies));
        assert_eq!(Kept::named("reference"), Ok(Kept::Reference));
        assert!(Kept::named("everything").is_err());
        #[cfg(not(target_os = "ios"))]
        assert_eq!(Kept::for_this_device(), Kept::Full);
    }

    #[test]
    fn proxies_and_recordings_are_told_apart() {
        assert_eq!(kind_of("Media/Proxies/Bass.ogg"), (true, false));
        assert_eq!(kind_of("Media/Bass.WAV"), (false, true));
        assert_eq!(kind_of("Song.session"), (false, false));
        assert_eq!(kind_of("Media/Proxies/Bass.ogg.idx"), (false, false));
    }

    #[test]
    fn a_song_folder_on_this_device_is_listed_and_read_as_a_source() {
        let dir = tempfile::tempdir().expect("a folder");
        let song = dir.path().join("Waltz");
        std::fs::create_dir_all(song.join("Media")).expect("its media folder");
        std::fs::write(song.join("Waltz.RPP"), b"<REAPER_PROJECT\n>\n").expect("its project");
        let bytes: Vec<u8> = (0..=255u8).cycle().take(3 * 1024 * 1024 + 7).collect();
        std::fs::write(song.join("Media/Bass.wav"), &bytes).expect("its recording");
        let source = FolderSource::at(&song).expect("a song");
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        let listing = rt.block_on(source.list()).expect("it lists");
        assert_eq!(listing.entries[0].0, "Waltz.RPP", "the project first");
        assert!(
            listing
                .entries
                .contains(&("Media/Bass.wav".to_owned(), bytes.len() as u64))
        );
        let read = rt
            .block_on(source.read("Media/Bass.wav".to_owned(), 5..bytes.len() as u64))
            .expect("it reads past one request's most");
        assert_eq!(read, bytes[5..]);
        assert!(FolderSource::at(&dir.path().join("Gone")).is_err());
    }

    #[test]
    fn a_setlist_comes_from_a_link_or_from_song_folders_not_both() {
        let mut both = DownloadArg {
            from: Some("https://n/s/abc/".into()),
            songs: Some(vec![SongArg {
                title: None,
                folder: "/a".into(),
            }]),
            name: None,
            to: None,
            keep: None,
        };
        assert!(origin_of(&mut both).is_err());
        let mut folders = DownloadArg {
            from: None,
            songs: Some(vec![SongArg {
                title: None,
                folder: "/songs/Silent Night".into(),
            }]),
            name: Some("Christmas".into()),
            to: None,
            keep: None,
        };
        assert_eq!(
            origin_of(&mut folders),
            Ok(Origin::Folders {
                name: "Christmas".into(),
                songs: vec![("Silent Night".into(), PathBuf::from("/songs/Silent Night"))],
            })
        );
        let mut link = DownloadArg {
            from: Some(" https://n/s/abc/ ".into()),
            songs: None,
            name: None,
            to: None,
            keep: None,
        };
        assert_eq!(
            origin_of(&mut link),
            Ok(Origin::Link("https://n/s/abc/".into()))
        );
    }

    #[test]
    fn a_listed_path_never_leaves_the_song() {
        let folder = Path::new("/songs/a");
        assert_eq!(
            inside(folder, "Media/Bass.wav"),
            Some(folder.join("Media/Bass.wav"))
        );
        assert_eq!(inside(folder, "../b/x.wav"), None);
        assert_eq!(inside(folder, "/etc/passwd"), None);
    }
}
