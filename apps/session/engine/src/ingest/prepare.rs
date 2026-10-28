//! Getting a song ready to travel, here, before anything is sent: a Live
//! set split into songs, each song's project file folded whole (its
//! write-ahead log emptied into it), the Ogg proxies and waveform peaks
//! of what it plays, and its reference bounce. Each step keeps what is
//! already there and current, so a second run does nothing.

use std::path::{Path, PathBuf};

use super::inputs::SongSource;

/// What preparing one song did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prepared {
    pub proxies: Tally,
    pub peaks: Tally,
    pub reference: Done,
    /// Media the song plays from outside its folder: it does not travel
    /// with the song, so the song plays without it elsewhere.
    pub outside: Vec<String>,
}

/// How many were made, how many were current already, and how many could
/// not be (with why, for the report).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    pub made: usize,
    pub current: usize,
    pub failed: Vec<String>,
}

/// Whether a one-off step ran.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Done {
    #[default]
    Off,
    Current,
    Made,
}

/// What [`prepare`] is asked to do besides proxies and peaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Steps {
    pub reference: bool,
}

/// Prepare `song` in place.
///
/// # Errors
///
/// The song would not open, or its project file would not fold. A proxy
/// or peaks file that fails is counted, not fatal.
pub fn prepare(song: &SongSource, steps: Steps) -> eyre::Result<Prepared> {
    let reference = if steps.reference {
        reference(song)?
    } else {
        Done::Off
    };
    let (inside, outside) = playing_media(song)?;
    let proxies = in_parallel(&inside, proxy);
    let peaks = in_parallel(&inside, peaks);
    // Last: opening a song to bounce or list it writes its project file
    // (an `.RPP` gets a `.session` beside it), and what travels is the
    // file with its log folded in.
    fold_logs(&song.folder)?;
    Ok(Prepared {
        proxies,
        peaks,
        reference,
        outside,
    })
}

/// Split the Live set `set` into song folders under `into`, unless every
/// song of it is there already and newer than the set. Returns the songs
/// in show order, and whether it split.
///
/// # Errors
///
/// The set would not read, or the split failed.
#[cfg(feature = "show")]
pub fn split(set: &Path, into: &Path) -> eyre::Result<(Vec<SongSource>, Done)> {
    let live =
        ableton::io::read_live_set(set).map_err(|e| eyre::eyre!("{}: {e}", set.display()))?;
    let ranges = ableton::setlist::songs(&live);
    let set_time = modified(set);
    let current: Option<Vec<SongSource>> = ranges
        .iter()
        .map(|range| {
            let file = crate::setlist::song_in(&into.join(&range.slug))?;
            (modified(&file) >= set_time).then(|| song_from_show(&range.name, &range.slug, file))
        })
        .collect();
    if let Some(songs) = current.filter(|s| !s.is_empty()) {
        return Ok((songs, Done::Current));
    }
    let songs = crate::show::split(set, into)?
        .into_iter()
        .map(|s| song_from_show(&s.name, &s.slug, s.file))
        .collect();
    Ok((songs, Done::Made))
}

/// Without the Live-set reader there is no splitting.
///
/// # Errors
///
/// Always: this build cannot read a Live set.
#[cfg(not(feature = "show"))]
pub fn split(set: &Path, _into: &Path) -> eyre::Result<(Vec<SongSource>, Done)> {
    eyre::bail!(
        "{}: this build cannot split a Live set (session-engine's `show` feature)",
        set.display()
    )
}

#[cfg(feature = "show")]
fn song_from_show(name: &str, slug: &str, file: PathBuf) -> SongSource {
    SongSource {
        title: name.to_owned(),
        slug: slug.to_owned(),
        folder: file
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        file,
    }
}

/// Fold the write-ahead log of every project file in `folder` into it.
fn fold_logs(folder: &Path) -> eyre::Result<()> {
    let files: Vec<PathBuf> = std::fs::read_dir(folder)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| crate::project_file::is_project_file(p))
        .collect();
    files.iter().try_for_each(|f| fold_log(f))
}

/// Fold a `.session`'s write-ahead log into it, when it has one with
/// anything in it: the file alone is then the whole project (the log is
/// not uploaded).
fn fold_log(file: &Path) -> eyre::Result<()> {
    let log = file.with_extension("session-wal");
    if std::fs::metadata(&log).map_or(0, |m| m.len()) == 0 {
        return Ok(());
    }
    let store = session_format::store::Store::open(file)
        .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    store
        .checkpoint()
        .map_err(|e| eyre::eyre!("folding {}'s log: {e}", file.display()))
}

/// The media the song plays (`song_stream::playing_files`), split into
/// what is inside its folder (absolute paths) and what is not (as named).
fn playing_media(song: &SongSource) -> eyre::Result<(Vec<PathBuf>, Vec<String>)> {
    let folder = song
        .folder
        .canonicalize()
        .unwrap_or_else(|_| song.folder.clone());
    let (mut inside, mut outside) = (Vec::new(), Vec::new());
    for named in crate::song_stream::playing_files(&song.file)? {
        let path = Path::new(&named);
        if path.is_relative() {
            inside.push(folder.join(path));
        } else if path.starts_with(&folder) {
            inside.push(path.to_path_buf());
        } else {
            outside.push(named);
        }
    }
    Ok((inside, outside))
}

/// The song's reference bounce (`crate::reference`), unless its proxies
/// and peaks are there and newer than the song.
fn reference(song: &SongSource) -> eyre::Result<Done> {
    let media = song.folder.join("Media");
    let made = [
        media.join("Proxies").join("Reference.ogg"),
        media.join("Proxies").join("Reference Preview.ogg"),
        media.join("Peaks").join("Reference.wav.sessionpeaks"),
    ];
    let song_time = modified(&song.file);
    if made.iter().all(|p| p.is_file() && modified(p) >= song_time) {
        return Ok(Done::Current);
    }
    crate::reference::bounce(&song.file)?;
    Ok(Done::Made)
}

/// What one proxy or peaks file came to.
enum One {
    Made,
    Current,
    /// Nothing to make it from (not a WAV, not there).
    None,
}

/// `each` over `media` on a few threads, tallied.
fn in_parallel(media: &[PathBuf], each: fn(&Path) -> eyre::Result<One>) -> Tally {
    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .clamp(1, 8);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::with_capacity(media.len()));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(path) = media.get(at) else { break };
                    let result = each(path).map_err(|e| format!("{}: {e}", path.display()));
                    if let Ok(mut all) = results.lock() {
                        all.push(result);
                    }
                }
            });
        }
    });
    let results = results.into_inner().unwrap_or_default();
    results.into_iter().fold(Tally::default(), |mut tally, r| {
        match r {
            Ok(One::Made) => tally.made += 1,
            Ok(One::Current) => tally.current += 1,
            Ok(One::None) => {}
            Err(why) => tally.failed.push(why),
        }
        tally
    })
}

/// Frames between a proxy's index points: about a second (as `studio
/// proxies` writes them).
const INDEX_STEP: u64 = 48_000;
/// libvorbis quality of a stem's proxy (0.4 ≈ 128 kbps stereo).
const QUALITY: f32 = 0.4;

/// `media`'s Ogg proxy in `Proxies/` beside it, and its page index: kept
/// when newer than the media, else written aside and moved into place.
fn proxy(media: &Path) -> eyre::Result<One> {
    let is_wav = media
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("wav") || x.eq_ignore_ascii_case("wave"));
    let Some(proxy) = proxy_of(media).filter(|_| is_wav && media.is_file()) else {
        return Ok(One::None);
    };
    let index = fts_sample::ogg_index::OggIndex::path_for;
    if proxy.is_file() && modified(&proxy) >= modified(media) {
        if !index(&proxy).exists() {
            fts_sample::cache::write_ogg_index(&proxy, INDEX_STEP)
                .map_err(|e| eyre::eyre!("{e}"))?;
        }
        return Ok(One::Current);
    }
    if let Some(dir) = proxy.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = proxy.with_extension("ogg.partial");
    fts_sample::cache::write_ogg_proxy(media, &partial, QUALITY).map_err(|e| eyre::eyre!("{e}"))?;
    std::fs::rename(&partial, &proxy)?;
    std::fs::rename(index(&partial), index(&proxy))?;
    Ok(One::Made)
}

/// Where a media file's proxy lives: `Proxies/<stem>.ogg` beside it (the
/// rule Task's share links and `studio proxies` follow); none for a proxy.
fn proxy_of(media: &Path) -> Option<PathBuf> {
    let dir = media.parent()?;
    if dir.file_name().is_some_and(|n| n == "Proxies") {
        return None;
    }
    Some(
        dir.join("Proxies")
            .join(format!("{}.ogg", media.file_stem()?.to_string_lossy())),
    )
}

/// `media`'s waveform cache (`Peaks/<name>.sessionpeaks` beside it): kept
/// when current, adopted from REAPER's own when that is current, else
/// scanned — from the WAV, or from its proxy when there is no WAV (as
/// `studio peaks` does).
fn peaks(media: &Path) -> eyre::Result<One> {
    use dawfile_reaper::reapeaks::ReaPeaks;
    use dawfile_reaper::sessionpeaks;
    let Some(cache) = sessionpeaks::cache_path(media) else {
        return Ok(One::None);
    };
    let exists = media.is_file();
    if let Ok(kept) = ReaPeaks::read(&cache)
        && (!exists || sessionpeaks::is_current(&kept, media))
    {
        return Ok(One::Current);
    }
    if exists
        && let Some(theirs) = sessionpeaks::candidates(media)
            .into_iter()
            .skip(1)
            .filter_map(|c| ReaPeaks::read(&c).ok())
            .find(|p| sessionpeaks::is_current(p, media))
    {
        sessionpeaks::write(media, &theirs)?;
        return Ok(One::Made);
    }
    let proxy = proxy_of(media).filter(|p| p.is_file());
    let mut built = if exists {
        sessionpeaks::build(media)?
    } else if let Some(proxy) = proxy {
        let bytes: std::sync::Arc<[u8]> = std::fs::read(&proxy)?.into();
        sessionpeaks::build_from_ogg(bytes)?
    } else {
        return Ok(One::None);
    };
    built.source_stamp = sessionpeaks::media_stamp(media).unwrap_or(built.source_stamp);
    sessionpeaks::write(media, &built)?;
    Ok(One::Made)
}

/// When `path` last changed; the epoch when that cannot be read.
fn modified(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}
