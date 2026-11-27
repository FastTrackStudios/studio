//! Where a streamed song's files come from ([`SongSource`]). The mirror
//! that reads a source and the streams it attaches are
//! `studio_engine::song_stream`'s (re-exported there with these).

use std::path::PathBuf;
use std::sync::Arc;

/// Where a song's files come from.
#[cfg(not(target_arch = "wasm32"))]
pub trait SongSource: Send + Sync + 'static {
    /// A name for its cache folder.
    fn label(&self) -> String;
    /// Every file of the song folder with its size, the project first.
    fn list(&self) -> Pending<eyre::Result<Listing>>;
    /// The bytes `range` of `path`.
    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>>;
    /// Many whole documents in one request, when the source has one: each
    /// path with its bytes, or `None` for a path to fetch on its own.
    /// `None` altogether when it has no such request (or it failed) — the
    /// caller reads them one by one.
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let _ = paths;
        Box::pin(async { None })
    }
    /// The whole of `path` written to the file `into`, in one request,
    /// adding each piece's length to `got` as it lands — when the source
    /// can stream a whole file. `None` when it cannot: a download then
    /// reads the file by ranges.
    fn fetch_whole(
        &self,
        path: String,
        into: PathBuf,
        got: Arc<std::sync::atomic::AtomicU64>,
    ) -> Pending<Option<eyre::Result<()>>> {
        let _ = (path, into, got);
        Box::pin(async { None })
    }
}

/// Where a song's files come from (see the native definition: here a
/// request is a JS promise, and not `Send`).
#[cfg(target_arch = "wasm32")]
pub trait SongSource: 'static {
    /// A name for its cache folder.
    fn label(&self) -> String;
    /// Every file of the song folder with its size, the project first.
    fn list(&self) -> Pending<eyre::Result<Listing>>;
    /// The bytes `range` of `path`.
    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>>;
    /// Many whole documents in one request, when the source has one: each
    /// path with its bytes, or `None` for a path to fetch on its own.
    /// `None` altogether when it has no such request (or it failed) — the
    /// caller reads them one by one.
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let _ = paths;
        Box::pin(async { None })
    }
}

/// A song folder's files, and the version they are at when the source
/// says (a share link's commit): the same version is the same bytes at
/// every path, so what was fetched at it is kept and not fetched again.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    /// Each file and its size, the project first.
    pub entries: Vec<(String, u64)>,
    pub version: Option<String>,
}

impl From<Vec<(String, u64)>> for Listing {
    fn from(entries: Vec<(String, u64)>) -> Self {
        Self {
            entries,
            version: None,
        }
    }
}

/// A source's answer, later.
#[cfg(not(target_arch = "wasm32"))]
pub type Pending<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;
/// A source's answer, later.
#[cfg(target_arch = "wasm32")]
pub type Pending<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T>>>;

/// A song folder's listing, sorted, its project first — what a mirror
/// opens (`song_stream::mirror` in `studio-engine`): the top-level project file (`Song.session`), else the top-level
/// `.RPP`. Sorted by path alone, a song named after `Media/` listed its
/// peaks first and opened them as the song.
pub fn project_first(entries: &mut Vec<(String, u64)>) {
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let top_level = |ext: &str| {
        entries
            .iter()
            .position(|(p, _)| !p.contains('/') && p.to_lowercase().ends_with(ext))
    };
    if let Some(at) = top_level(".session").or_else(|| top_level(".rpp")) {
        let project = entries.remove(at);
        entries.insert(0, project);
    }
}
