//! Where a streamed song's files come from ([`SongSource`]).
//!
//! And the one source that needs nothing of the app around it:
//! [`ShareSource`], a Task share link. The mirror that reads a source and the streams it attaches
//! are `session_engine::song_stream`'s (re-exported there with these).

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
    /// The song's chart, when the source keeps it apart from the folder
    /// (the library's) — it replaces the folder's `.kf`.
    fn chart(&self) -> Pending<eyre::Result<Option<String>>> {
        Box::pin(async { Ok(None) })
    }
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
    /// The song's chart, when the source keeps it apart from the folder
    /// (the library's) — it replaces the folder's `.kf`.
    fn chart(&self) -> Pending<eyre::Result<Option<String>>> {
        Box::pin(async { Ok(None) })
    }
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

/// A Task share link to a song's session
/// (`https://task…/org/<org>/share/<token>`, `?pw=` when it has one).
pub struct ShareSource {
    link: reqwest::Url,
    password: Option<String>,
    http: reqwest::Client,
}

impl ShareSource {
    /// The session `link` shares.
    ///
    /// # Errors
    ///
    /// `link` is not a URL.
    pub fn new(link: &str) -> eyre::Result<Self> {
        let mut link = reqwest::Url::parse(link.trim().trim_end_matches('/'))?;
        let password = link
            .query_pairs()
            .find(|(k, _)| k == "pw")
            .map(|(_, v)| v.into_owned());
        link.set_query(None);
        Ok(Self {
            link,
            password,
            http: reqwest::Client::new(),
        })
    }

    /// The link's route `route` (`list`, `doc`, `rendition/audio`) for the
    /// song-folder path `path`, each segment escaped.
    fn url(&self, route: &str, path: &str) -> reqwest::Url {
        let mut url = self.link.clone();
        if let Ok(mut segments) = url.path_segments_mut() {
            segments
                .extend(route.split('/'))
                .extend(path.split('/').filter(|s| !s.is_empty()));
        }
        if let Some(pw) = &self.password {
            url.query_pairs_mut().append_pair("pw", pw);
        }
        url
    }

    /// The link's token, for the cache folder's name — never the password.
    fn token(&self) -> String {
        self.link
            .path_segments()
            .and_then(Iterator::last)
            .unwrap_or_default()
            .to_owned()
    }
}

/// A `docs` batch's records — `u8 status | u32le path length | path |
/// u32le length | bytes`, one per path (Task's
/// `share_documents_batch_handler`) — or `None` if the body is not one.
#[must_use]
pub fn batch_records(mut body: &[u8]) -> Option<Vec<(String, u8, Vec<u8>)>> {
    fn take<'b>(body: &mut &'b [u8], n: usize) -> Option<&'b [u8]> {
        let (head, rest) = body.split_at_checked(n)?;
        *body = rest;
        Some(head)
    }
    fn len(body: &mut &[u8]) -> Option<usize> {
        let bytes: [u8; 4] = take(body, 4)?.try_into().ok()?;
        usize::try_from(u32::from_le_bytes(bytes)).ok()
    }
    let mut out = Vec::new();
    while !body.is_empty() {
        let status = take(&mut body, 1)?[0];
        let path_len = len(&mut body)?;
        let path = String::from_utf8(take(&mut body, path_len)?.to_vec()).ok()?;
        let bytes_len = len(&mut body)?;
        let bytes = take(&mut body, bytes_len)?.to_vec();
        out.push((path, status, bytes));
    }
    Some(out)
}

/// The media path whose audio rendition is the proxy at `proxy`
/// (`Media/Proxies/Bass.ogg` → `Media/Bass.ogg`: a link streams the
/// committed proxy beside a take by the take's stem), when it is one.
fn rendition_of(proxy: &str) -> Option<String> {
    let (dir, name) = proxy.rsplit_once('/')?;
    let parent = dir.strip_suffix("Proxies")?.trim_end_matches('/');
    let name = name.strip_suffix(".ogg")?;
    Some(if parent.is_empty() {
        format!("{name}.ogg")
    } else {
        format!("{parent}/{name}.ogg")
    })
}

/// A song folder's listing, sorted, its project first — what a mirror
/// opens (`song_stream::mirror` in `session-engine`): the top-level project file (`Song.session`), else the top-level
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

impl SongSource for ShareSource {
    fn label(&self) -> String {
        format!("share-{}", self.token())
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let request = self.http.get(self.url("list", ""));
        Box::pin(async move {
            let listed: serde_json::Value =
                request.send().await?.error_for_status()?.json().await?;
            let mut entries: Vec<(String, u64)> = listed["entries"]
                .as_array()
                .ok_or_else(|| eyre::eyre!("the link lists no entries"))?
                .iter()
                .filter_map(|e| {
                    Some((
                        e["path"].as_str()?.to_owned(),
                        e["size"].as_u64().unwrap_or(0),
                    ))
                })
                .collect();
            project_first(&mut entries);
            Ok(Listing {
                entries,
                version: listed["commit"].as_str().map(str::to_owned),
            })
        })
    }

    /// The link's `docs` batch: one POST for a whole song folder's small
    /// files (a Task that has no such route answers 404/405, and the
    /// caller reads them one by one).
    fn read_many(&self, paths: Vec<String>) -> Pending<Option<Vec<(String, Option<Vec<u8>>)>>> {
        let request = self
            .http
            .post(self.url("docs", ""))
            // Plain text: a browser sends it without asking first.
            .header(reqwest::header::CONTENT_TYPE, "text/plain")
            .body(paths.join("\n"));
        Box::pin(async move {
            let response = request.send().await.ok()?.error_for_status().ok()?;
            let body = response.bytes().await.ok()?;
            let records = batch_records(&body)?;
            Some(
                records
                    .into_iter()
                    .map(|(path, status, bytes)| (path, (status == 0).then_some(bytes)))
                    .collect(),
            )
        })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let ranged = rendition_of(&path).map(|media| self.url("rendition/audio", &media));
        let request = match &ranged {
            Some(url) => self.http.get(url.clone()).header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", range.start, range.end.saturating_sub(1)),
            ),
            // A document comes whole (it is small); the range is cut here.
            None => self.http.get(self.url("doc", &path)),
        };
        let partial = ranged.is_some();
        Box::pin(async move {
            let what = |e: &dyn std::fmt::Display| format!("{path} {range:?}: {e}");
            let response = request
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| what(&e))?;
            let served_range = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            let bytes = response.bytes().await.map_err(|e| what(&e))?;
            if partial && served_range {
                return Ok(bytes.to_vec());
            }
            let from = usize::try_from(range.start).unwrap_or(usize::MAX);
            let to = usize::try_from(range.end)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            bytes
                .get(from..to)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| what(&"shorter than asked"))
        })
    }
}
