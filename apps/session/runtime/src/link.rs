//! A set opened from an Architect share link ([`LinkSet`]), each of its
//! songs a [`SongSource`] ([`LinkSong`]).
//!
//! A link (`https://<node>/s/<code>/…`) lists what it serves at
//! `<link>_files.json` (Architect's `apps/desktop/src/node/shares.rs`
//! `files_json`): its notes in list order, each with the files it
//! attaches, and each file once with its size, layer and URL. The node
//! has already left out every file above the link's layer, so what is
//! listed is what this listener may fetch: a link that plays to
//! `reference` lists no stem, and [`LinkSet::listen`] is the cap the
//! engine is opened under ([`Listen`]).
//!
//! A song is a note that attaches a project file (`.session`, else
//! `.RPP`). Its folder is that file's, and its files are the link's files
//! under it: the prepared session, the chart and lyrics, each rendition
//! (`…/Proxies/<stem>.ogg`) and its page index. Every byte comes from the
//! file's own URL on the link, a rendition by `Range`, so nothing here
//! knows where the node holds it (a bucket, a storage server, its disk).
//!
//! Nothing is written back: the link's routes are reads.

use std::ops::Range;
use std::sync::Arc;

use crate::listen::Listen;
use crate::source::{Listing, Pending, SongSource, project_first};

/// A share link's set: its title, how deep it plays, and its songs in
/// list order.
#[derive(Clone)]
pub struct LinkSet {
    pub title: String,
    /// The cap the link sets: [`Listen::Full`] only when it plays to
    /// `full`.
    pub listen: Listen,
    pub songs: Vec<LinkSong>,
}

/// One song of a link: its title and its folder's files.
#[derive(Clone)]
pub struct LinkSong {
    pub title: String,
    /// The song note's vault path: what the page's playhead and charts
    /// know the song by.
    pub note: String,
    /// The vault path of the song's folder, no trailing slash.
    folder: String,
    /// The link's code: with the folder, the name of its cache.
    code: String,
    /// Each file under the folder: its path there, its size, its URL.
    files: Arc<Vec<LinkFile>>,
    http: reqwest::Client,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinkFile {
    path: String,
    size: u64,
    url: reqwest::Url,
}

/// Where a link's listing is, and the link's code: `link` cut back to
/// `<origin>/s/<code>/`, whatever page of it was given.
///
/// # Errors
///
/// `link` is not a URL, or not a share link's (`/s/<code>/…`).
pub fn listing_url(link: &str) -> eyre::Result<(reqwest::Url, String)> {
    let mut url = reqwest::Url::parse(link.trim())?;
    let code = {
        let mut segments = url
            .path_segments()
            .ok_or_else(|| eyre::eyre!("{link} is not a share link"))?;
        match (segments.next(), segments.next()) {
            (Some("s"), Some(code)) if !code.is_empty() && !code.starts_with('_') => {
                code.to_owned()
            }
            _ => eyre::bail!("{link} is not a share link (/s/<code>/)"),
        }
    };
    url.set_query(None);
    url.set_fragment(None);
    url.set_path(&format!("/s/{code}/_files.json"));
    Ok((url, code))
}

/// The songs a link's listing names (`<link>_files.json`'s body), in list
/// order, and how deep the link plays.
///
/// # Errors
///
/// The body is not a listing.
pub fn read_listing(
    body: &serde_json::Value,
    at: &reqwest::Url,
    code: &str,
    http: &reqwest::Client,
) -> eyre::Result<LinkSet> {
    let files: Vec<LinkFile> = body["files"]
        .as_array()
        .ok_or_else(|| eyre::eyre!("the link lists no files"))?
        .iter()
        .filter_map(|f| {
            Some(LinkFile {
                path: f["path"].as_str()?.to_owned(),
                size: f["size"].as_u64().unwrap_or(0),
                // A file's URL is a path on the link's node.
                url: at.join(f["url"].as_str()?).ok()?,
            })
        })
        .collect();
    let mut songs = Vec::new();
    for note in body["notes"].as_array().into_iter().flatten() {
        let attached: Vec<&str> = note["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let Some(project) = project_of(&attached) else {
            continue;
        };
        let folder = project
            .rsplit_once('/')
            .map_or("", |(dir, _)| dir)
            .to_owned();
        let under: Vec<LinkFile> = files
            .iter()
            .filter_map(|f| {
                let rest = if folder.is_empty() {
                    f.path.as_str()
                } else {
                    f.path.strip_prefix(&folder)?.strip_prefix('/')?
                };
                Some(LinkFile {
                    path: rest.to_owned(),
                    ..f.clone()
                })
            })
            .collect();
        songs.push(LinkSong {
            title: note["title"].as_str().unwrap_or_default().to_owned(),
            note: note["path"].as_str().unwrap_or_default().to_owned(),
            folder,
            code: code.to_owned(),
            files: Arc::new(under),
            http: http.clone(),
        });
    }
    Ok(LinkSet {
        title: body["title"].as_str().unwrap_or_default().to_owned(),
        listen: match body["listen"].as_str() {
            Some(layer) if layer.eq_ignore_ascii_case("full") => Listen::Full,
            _ => Listen::Reference,
        },
        songs,
    })
}

/// The project file among a note's attachments: its `.session`, else its
/// `.RPP`; a project's log is never one.
fn project_of<'a>(attached: &[&'a str]) -> Option<&'a str> {
    let with = |ext: &str| {
        attached
            .iter()
            .copied()
            .find(|p| p.to_lowercase().ends_with(ext))
    };
    with(".session").or_else(|| with(".rpp"))
}

impl LinkSet {
    /// The set `link` shares, as its node lists it now.
    ///
    /// # Errors
    ///
    /// `link` is not a share link, its node did not answer, or it answered
    /// with something that is not a listing (`503` while the node does not
    /// know which layer each file is in).
    pub async fn open(link: &str) -> eyre::Result<Self> {
        let (at, code) = listing_url(link)?;
        let http = reqwest::Client::new();
        let body: serde_json::Value = http
            .get(at.clone())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        read_listing(&body, &at, &code, &http)
    }
}

impl LinkSong {
    /// How many renditions the link serves for this song besides its
    /// reference: none on a link that plays to `reference`.
    #[must_use]
    pub fn stems(&self) -> usize {
        self.files
            .iter()
            .filter(|f| {
                let lower = f.path.to_lowercase();
                lower.contains("proxies/")
                    && lower.ends_with(".ogg")
                    && !lower
                        .rsplit('/')
                        .next()
                        .is_some_and(|n| n.starts_with("reference"))
            })
            .count()
    }

    /// The vault path of the song's folder (no trailing slash): every file
    /// of the song the link serves is under it.
    #[must_use]
    pub fn folder(&self) -> &str {
        &self.folder
    }

    fn file(&self, path: &str) -> Option<&LinkFile> {
        self.files.iter().find(|f| f.path == path)
    }
}

impl SongSource for LinkSong {
    fn label(&self) -> String {
        format!("link-{}-{}", self.code, self.folder)
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let mut entries: Vec<(String, u64)> = self
            .files
            .iter()
            .map(|f| (f.path.clone(), f.size))
            .collect();
        Box::pin(async move {
            if entries.is_empty() {
                eyre::bail!("the link serves none of the song's files");
            }
            project_first(&mut entries);
            Ok(Listing::from(entries))
        })
    }

    fn read(&self, path: String, range: Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let Some(file) = self.file(&path) else {
            return Box::pin(async move { Err(format!("{path}: not served by the link")) });
        };
        // The whole of a small file is asked for plainly: a node answers
        // it from memory, and a `Range` the size of the file is the same
        // bytes with a preflight a browser would not otherwise send.
        let whole = range.start == 0 && range.end >= file.size;
        let mut request = self.http.get(file.url.clone());
        if !whole {
            request = request.header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", range.start, range.end.saturating_sub(1)),
            );
        }
        Box::pin(async move {
            let what = |e: &dyn std::fmt::Display| format!("{path} {range:?}: {e}");
            let response = request
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| what(&e))?;
            let ranged = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            let bytes = response.bytes().await.map_err(|e| what(&e))?;
            if ranged {
                return Ok(bytes.to_vec());
            }
            // A node that answered the whole file: the range is cut here.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn listing() -> serde_json::Value {
        serde_json::json!({
            "link": "/s/recitalstack0000/",
            "title": "JT's Senior Recital",
            "listen": "full",
            "max": "full",
            "notes": [
                { "path": "Setlists/Recital.md", "title": "Recital", "type": "setlist", "files": [] },
                { "path": "Songs/Hey/Hey.md", "title": "Hey!", "type": "song", "files": [
                    "Songs/Hey/Hey!.session",
                    "Songs/Hey/Samples/Imported/Proxies/Reference.ogg",
                    "Songs/Hey/Samples/Imported/Proxies/BASS - Live Bass.ogg",
                    "Songs/Hey/Samples/Imported/Proxies/BASS - Live Bass.ogg.idx",
                ] },
                { "path": "Songs/Fire/Fire.md", "title": "Fire", "type": "song", "files": [
                    "Songs/Fire/Fire.session",
                ] },
            ],
            "files": [
                { "path": "Songs/Hey/Hey!.session", "url": "/s/recitalstack0000/Songs/Hey/Hey!.session", "size": 589_824, "layer": "reference" },
                { "path": "Songs/Hey/Samples/Imported/Proxies/Reference.ogg", "url": "/s/recitalstack0000/Songs/Hey/Samples/Imported/Proxies/Reference.ogg", "size": 4_000_000, "layer": "reference" },
                { "path": "Songs/Hey/Samples/Imported/Proxies/BASS - Live Bass.ogg", "url": "/s/recitalstack0000/Songs/Hey/Samples/Imported/Proxies/BASS%20-%20Live%20Bass.ogg", "size": 3_000_000, "layer": "full" },
                { "path": "Songs/Hey/Samples/Imported/Proxies/BASS - Live Bass.ogg.idx", "url": "/s/recitalstack0000/Songs/Hey/Samples/Imported/Proxies/BASS%20-%20Live%20Bass.ogg.idx", "size": 2_000, "layer": "full" },
                { "path": "Songs/Fire/Fire.session", "url": "/s/recitalstack0000/Songs/Fire/Fire.session", "size": 400_000, "layer": "reference" },
                { "path": "Songs/Heyday/other.ogg", "url": "/s/recitalstack0000/Songs/Heyday/other.ogg", "size": 1, "layer": "full" },
            ],
        })
    }

    fn set() -> LinkSet {
        let (at, code) =
            listing_url("http://127.0.0.1:7422/s/recitalstack0000/setlists/recital/?x=1#top")
                .expect("a share link");
        assert_eq!(
            at.as_str(),
            "http://127.0.0.1:7422/s/recitalstack0000/_files.json"
        );
        read_listing(&listing(), &at, &code, &reqwest::Client::new()).expect("a listing")
    }

    #[test]
    fn a_link_is_cut_back_to_its_listing_and_anything_else_is_refused() {
        assert!(listing_url("http://127.0.0.1:7422/app/").is_err());
        assert!(listing_url("http://127.0.0.1:7422/s/_signin").is_err());
        assert!(listing_url("not a url").is_err());
    }

    #[test]
    fn the_songs_are_the_notes_with_a_project_in_list_order() {
        let set = set();
        assert_eq!(set.title, "JT's Senior Recital");
        assert_eq!(set.listen, Listen::Full);
        let titles: Vec<&str> = set.songs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Hey!", "Fire"]);
        assert_eq!(set.songs[0].note, "Songs/Hey/Hey.md");
    }

    #[test]
    fn a_song_has_the_files_under_its_own_folder_and_no_others() {
        let set = set();
        let hey = &set.songs[0];
        let paths: Vec<&str> = hey.files.iter().map(|f| f.path.as_str()).collect();
        // `Songs/Heyday/` begins with `Songs/Hey` and is not in it.
        assert_eq!(
            paths,
            [
                "Hey!.session",
                "Samples/Imported/Proxies/Reference.ogg",
                "Samples/Imported/Proxies/BASS - Live Bass.ogg",
                "Samples/Imported/Proxies/BASS - Live Bass.ogg.idx",
            ]
        );
        assert_eq!(hey.stems(), 1);
        assert_eq!(set.songs[1].stems(), 0);
        assert_eq!(
            hey.file("Samples/Imported/Proxies/BASS - Live Bass.ogg")
                .map(|f| f.url.as_str()),
            Some(
                "http://127.0.0.1:7422/s/recitalstack0000/Songs/Hey/Samples/Imported/Proxies/BASS%20-%20Live%20Bass.ogg"
            )
        );
    }

    #[test]
    fn a_link_that_plays_to_reference_caps_the_engine_there() {
        let mut body = listing();
        body["listen"] = "reference".into();
        let (at, code) = listing_url("http://127.0.0.1:7422/s/recitalstack0000/").expect("link");
        let set = read_listing(&body, &at, &code, &reqwest::Client::new()).expect("a listing");
        assert_eq!(set.listen, Listen::Reference);
    }
}
