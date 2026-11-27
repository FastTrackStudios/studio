//! The engine, in a browser: the song Now Playing opened from its session
//! folder into the engine running in the page (`studio_engine::web_engine`:
//! daw-standalone in-process, out through an AudioWorklet), so its
//! multitracks and click play here as they do on a desktop, through the
//! same first-party FX.
//!
//! A song's files come to the page by range from wherever its session
//! folder is ([`SessionFolder`]): a location in Drive, through the node, or
//! a folder at a link (`?session=<url>` on the page's address, a folder
//! served with its `files.json`). The page streams the songs' proxies
//! (`Media/Proxy/…`), never the originals.
//!
//! The engine plays every song with a session folder. The reference
//! player (`crate::player`) plays only what the engine could not: a song
//! with no session folder the list knows, or one whose session did not
//! open, the engine itself not starting among the reasons
//! ([`WebPlay::failed`]).
//!
//! The engine the page opened is the one the session's arrangement and
//! mixer draw (`studio_panels::web_host::HostedView`): each song it
//! opens joins the hosted set, the song on screen is the one they show,
//! and a song picked in them is picked here.
//!
//! A song's signal rigs (the bass rig: its NAM captures, cab IRs and home)
//! stream in from the **rig store** ([`rig_store`]), the store the
//! desktop's rigs come from: the [`RIG_LIBRARY`] folder at the root of the
//! session's Drive location, read through the node with this person's
//! sign-in, or a folder at a link (`?rigs=<url>`). Nothing is bundled
//! into the web build; the page caches what it fetched
//! (`studio_engine::rig_files`).
//!
//! What the engine is doing is written to a hidden element (`#studio-engine`,
//! [`STATE_ID`]): its state, the song's playhead, what the worklet has put
//! out, and its rigs' files — what the browser boot test
//! (`apps/studio/tests/it/web_boot.rs`) reads.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;
use studio_engine::song_stream::{Listing, Pending, SongSource, project_first};
use studio_engine::web_engine::{self, HostedSong};
use studio_panels::web_host;

use crate::files::Hit;
use crate::files_live::web;
use crate::state::{Place, use_app};

/// The location of a session folder at a link (`?session=`): its `folder`
/// is the folder's URL.
pub const LINK: &str = "studio-link";

/// The rig store's folder at the root of a session's Drive location: laid
/// out as the desktop's `rig-library/` (`bass/models/…`, `bass/*.styx`).
pub const RIG_LIBRARY: &str = "Rig Library";

/// The hidden element the engine's state is written to.
pub const STATE_ID: &str = "studio-engine";

/// The most files a session folder is listed to: a song is a few hundred
/// (its session's objects, its proxies, their peaks).
const MOST_FILES: usize = 20_000;

/// What the engine does with which songs, for the reference player beside
/// it: the session folders it could not open (the player plays those).
#[derive(Clone, Default, PartialEq)]
pub struct WebPlay {
    pub failed: HashSet<String>,
}

impl WebPlay {
    /// Whether the engine plays the song at `place`: any song with a
    /// session folder, unless that folder did not open.
    pub fn owns(&self, place: Option<&Place>) -> bool {
        place.is_some_and(|p| !self.failed.contains(&key(p)))
    }
}

/// The page's query as it loaded (`?session=…`), before the router puts
/// the route it shows in the address and drops it.
static ADDRESS: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Called in `main`, before the app is launched: the query the page was
/// opened with, for [`session_in_address`].
pub fn remember_address() {
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    ADDRESS.get_or_init(|| search);
}

/// What registers the page's service worker (`public/sw.js`) and, once
/// the page has loaded, names its own files for it to keep: the page, its
/// script, wasm and styles — never a song's files, which the engine's
/// caches keep (`keep_offline`). `{base}` is the path the page is served
/// at.
const REGISTER_WORKER: &str = r#"
if ("serviceWorker" in navigator) {
  const base = "{base}";
  navigator.serviceWorker
    .register(base + "/sw.js", { scope: base + "/" })
    .then(() => navigator.serviceWorker.ready)
    .then((reg) => {
      const keep = () => {
        const urls = [location.href]
          .concat(performance.getEntriesByType("resource").map((e) => e.name))
          .filter((u) => u.startsWith(location.origin));
        if (reg.active) reg.active.postMessage({ keep: urls });
      };
      if (document.readyState === "complete") keep();
      else addEventListener("load", keep);
      setTimeout(keep, 5000);
    })
    .catch((e) => console.warn("studio: the page will not open offline:", e));
}
"#;

/// Register the page's service worker, served at `base` (`""` at the
/// site's root, `/studio` under a base path), so the app opens again with
/// no network (studio#716).
pub fn register_service_worker(base: &str) {
    let base = format!("/{}", base.trim_matches('/'));
    let base = if base == "/" { String::new() } else { base };
    let code = REGISTER_WORKER.replace("{base}", &base);
    if let Err(e) = js_sys::Function::new_no_args(&code).call0(&wasm_bindgen::JsValue::NULL) {
        tracing::warn!(error = ?e, "web: the service worker was not registered");
    }
}

/// A session folder, as the engine knows it.
fn key(p: &Place) -> String {
    format!("{}\u{1f}{}\u{1f}{}", p.space, p.location, p.folder)
}

/// A song's session folder, read by range: in Drive (each file through the
/// node, with this person's sign-in) or at a link (plain requests).
#[derive(Clone)]
struct SessionFolder(Rc<Folder>);

struct Folder {
    place: Place,
    /// Each listed file's URL, by its path in the folder.
    urls: RefCell<HashMap<String, String>>,
}

impl SessionFolder {
    fn new(place: Place) -> Self {
        Self(Rc::new(Folder {
            place,
            urls: RefCell::default(),
        }))
    }
}

impl Folder {
    fn at_link(&self) -> bool {
        self.place.location == LINK
    }

    /// Every file under the folder, by its path in it, with its size, URL
    /// and when it was last changed (0 when the folder does not say).
    async fn walk(&self) -> eyre::Result<Vec<(String, u64, String, i64)>> {
        if self.at_link() {
            return self.link_files().await;
        }
        let Place {
            space,
            location,
            folder,
            ..
        } = &self.place;
        let root = folder.trim_end_matches('/');
        let mut out = Vec::new();
        let mut dirs = vec![root.to_owned()];
        while let Some(dir) = dirs.pop() {
            for hit in web::list(space, location, &dir).await {
                if hit.dir {
                    dirs.push(hit.path.clone());
                    continue;
                }
                let Some(rel) = relative(root, &hit.path) else {
                    continue;
                };
                out.push((
                    rel,
                    u64::try_from(hit.size).unwrap_or(0),
                    web::file_url(&hit),
                    hit.mtime,
                ));
                if out.len() >= MOST_FILES {
                    eyre::bail!("{folder} has more than {MOST_FILES} files");
                }
            }
        }
        if out.is_empty() {
            eyre::bail!("{folder} is empty, or its computer is off");
        }
        Ok(out)
    }

    /// A link's folder: its `files.json` (`[{"path", "size"}]`), each file
    /// beside it.
    async fn link_files(&self) -> eyre::Result<Vec<(String, u64, String, i64)>> {
        let base = absolute(&format!("{}/", self.place.folder.trim_end_matches('/')));
        let (code, body) = plain(&format!("{base}files.json"), None)
            .await
            .map_err(|e| eyre::eyre!(e))?;
        if code != 200 {
            eyre::bail!("{base}files.json answered {code}");
        }
        let listed: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
        Ok(listed
            .iter()
            .filter_map(|f| {
                let path = f["path"].as_str()?.trim_start_matches('/').to_owned();
                let url = format!("{base}{}", encode_path(&path));
                let mtime = f["mtime"].as_i64().unwrap_or(0);
                Some((path, f["size"].as_u64().unwrap_or(0), url, mtime))
            })
            .collect())
    }
}

impl SongSource for SessionFolder {
    fn label(&self) -> String {
        let p = &self.0.place;
        format!("{}-{}-{}", p.space, p.location, p.folder)
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let this = Rc::clone(&self.0);
        Box::pin(async move {
            let files = this.walk().await?;
            let version = listing_version(&files);
            let mut entries = Vec::with_capacity(files.len());
            let mut urls = this.urls.borrow_mut();
            urls.clear();
            for (path, size, url, _) in files {
                entries.push((path.clone(), size));
                urls.insert(path, url);
            }
            project_first(&mut entries);
            // A version: the page keeps the folder's small files in its
            // cache under it (`song_stream`'s docs), so the song opens
            // again with no network, and a changed file is a new version.
            Ok(Listing {
                entries,
                version: Some(version),
            })
        })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let this = Rc::clone(&self.0);
        Box::pin(async move {
            let url = this
                .urls
                .borrow()
                .get(&path)
                .cloned()
                .ok_or_else(|| format!("{path} is not in the session folder"))?;
            let (code, bytes) = if this.at_link() {
                plain(&url, Some(range.clone())).await?
            } else {
                web::range(&url, range.clone()).await?
            };
            match code {
                206 => Ok(bytes),
                // The whole file: the stretch asked for out of it.
                200 => {
                    let end = usize::try_from(range.end)
                        .unwrap_or(usize::MAX)
                        .min(bytes.len());
                    let start = usize::try_from(range.start).unwrap_or(usize::MAX).min(end);
                    Ok(bytes[start..end].to_vec())
                }
                code => Err(format!("{path} answered {code}")),
            }
        })
    }
}

/// A folder listing's version: every file's path, size and time, hashed.
/// The same files at the same sizes and times are the same version.
fn listing_version(files: &[(String, u64, String, i64)]) -> String {
    let mut sorted: Vec<(&str, u64, i64)> = files
        .iter()
        .map(|(path, size, _, mtime)| (path.as_str(), *size, *mtime))
        .collect();
    sorted.sort_unstable();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (path, size, mtime) in sorted {
        for b in path
            .bytes()
            .chain(size.to_le_bytes())
            .chain(mtime.to_le_bytes())
            .chain([0])
        {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Bring the song at `place` onto this device, for the page to open with
/// no network: its folder listed (the service worker keeps the listing),
/// its small files kept in the page's cache, and every proxy of it — the
/// reference, its preview and each stem — fetched whole into the cache.
/// Marked kept ([`is_kept`]) once every block of every proxy is in the
/// cache. The proxies' bytes now in the cache.
///
/// # Errors
///
/// The folder did not list, the song would not mirror, or not all of its
/// proxies came down.
pub async fn keep_offline(place: &Place) -> Result<u64, String> {
    use studio_engine::song_stream::{Heard, Keep, Listen, mirror};
    let source: Arc<dyn SongSource> = Arc::new(SessionFolder::new(place.clone()));
    let song = mirror(
        Arc::clone(&source),
        Keep::Memory(studio_engine::folder::Memory::new()),
    )
    .await
    .map_err(|e| e.to_string())?
    .with_listen(Listen::Full);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let all = [Heard::Preview, Heard::Reference, Heard::Stems];
    for heard in all {
        song.warm(heard, None, Arc::clone(&stop)).await;
    }
    // Kept only when every block landed: a cache that was full, or a
    // connection that dropped, leaves a song that would be silent offline.
    for heard in all {
        if !song.is_warm(heard).await {
            remember_kept(place, false);
            return Err(
                "not all of its audio came down (the connection dropped, or the browser's storage is full)"
                    .to_owned(),
            );
        }
    }
    let bytes: u64 = song.proxies().values().map(|(_, size)| size).sum();
    remember_kept(place, true);
    Ok(bytes)
}

/// The browser's note of which session folders are kept on this device.
const KEPT: &str = "studio-offline-kept";

/// Note `place` as kept (or no longer).
fn remember_kept(place: &Place, on: bool) {
    let Some(store) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let mut kept: Vec<String> = store
        .get_item(KEPT)
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let k = key(place);
    kept.retain(|x| x != &k);
    if on {
        kept.push(k);
    }
    if let Ok(text) = serde_json::to_string(&kept) {
        let _ = store.set_item(KEPT, &text);
    }
}

/// Whether the song at `place` was kept on this device.
pub fn is_kept(place: &Place) -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|store| store.get_item(KEPT).ok().flatten())
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .is_some_and(|kept| kept.contains(&key(place)))
}

/// `path` under `root`, as a path in the folder.
fn relative(root: &str, path: &str) -> Option<String> {
    let rest = path.strip_prefix(root)?.trim_start_matches('/');
    (!rest.is_empty()).then(|| rest.to_owned())
}

/// Each segment of a path in a folder, escaped for a URL.
fn encode_path(path: &str) -> String {
    path.split('/').map(web::enc).collect::<Vec<_>>().join("/")
}

/// A URL the page's own origin is the root of, made absolute: Drive's
/// helpers would take a bare `/path` for the node's.
fn absolute(url: &str) -> String {
    if !url.starts_with('/') {
        return url.to_owned();
    }
    let origin = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default();
    format!("{origin}{url}")
}

/// A plain GET (no sign-in sent): the bytes `range` when given.
async fn plain(url: &str, range: Option<std::ops::Range<u64>>) -> Result<(u16, Vec<u8>), String> {
    let wanted = range.map(|r| format!("bytes={}-{}", r.start, r.end.saturating_sub(1)));
    let headers: Vec<(&str, &str)> = wanted.iter().map(|w| ("range", w.as_str())).collect();
    drive_dioxus::env::http_bytes("GET", url, &headers, None)
        .await
        .ok_or_else(|| format!("{url} did not answer"))
}

/// What the page's engine is doing, for [`STATE_ID`].
#[derive(Clone, Debug, PartialEq)]
enum Up {
    /// Nothing asked of it yet.
    Idle,
    Opening(String),
    /// The song on screen is open: its project and its length in seconds.
    Ready {
        project: String,
        len: f64,
    },
    Failed(String),
}

/// The engine for the page: mounted once, in the shell, beside the
/// reference player.
#[component]
pub fn EnginePlayer() -> Element {
    let mut app = use_app();
    let mut play = use_context::<Signal<WebPlay>>();
    let mut up = use_signal(|| Up::Idle);
    // Each session folder the engine has open: its project and length.
    let opened = use_hook(|| Rc::new(RefCell::new(HashMap::<String, (String, f64)>::new())));
    // Whether the page's engine has been made (`web_engine::open`, once).
    let started = use_hook(|| Rc::new(std::cell::Cell::new(false)));
    // The rig stores handed to the engine (one a Drive location).
    let rigs_from = use_hook(|| Rc::new(RefCell::new(HashSet::<String>::new())));
    // `?session=…&keep=1`: the session kept on this device once it is open
    // (`data-kept`: `keeping`, `kept`, or why it was not).
    let mut kept = use_signal(String::new);

    // A session at a link (`?session=<url>`), opened as the page loads.
    use_hook(move || {
        if let Some(place) = session_in_address() {
            let title = place
                .folder
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("Session")
                .to_owned();
            let song = crate::data::song_named(&title);
            app.np.set(Some(crate::state::NowPlaying::of_places(
                vec![(song, place)],
                "Link".into(),
                0,
                false,
            )));
        }
        // A browser starts audio only inside a gesture: the page's first
        // press or key (the worklet is there once the engine is).
        studio_engine::web_audio::unlock_on_first_gesture();
    });

    // A song picked in the arrangement or the mixer: Now Playing goes to
    // it, and the engine follows as for any other pick.
    {
        let opened = opened.clone();
        use_hook(move || {
            web_host::on_hosted_pick(move |project| {
                let Some(k) = opened
                    .borrow()
                    .iter()
                    .find(|(_, (p, _))| p == project)
                    .map(|(k, _)| k.clone())
                else {
                    return;
                };
                let at = app.np.peek().as_ref().and_then(|n| {
                    n.places
                        .iter()
                        .position(|place| place.as_ref().is_some_and(|p| key(p) == k))
                });
                if let Some(j) = at {
                    crate::session::go_to(app, j);
                }
            });
        });
    }

    // The song Now Playing is at, into the engine and on screen.
    let place = (app.np)().and_then(|n| n.place().cloned());
    let owned = play.read().owns(place.as_ref());
    {
        let (opened, started, rigs_from) = (opened.clone(), started.clone(), rigs_from.clone());
        use_effect(use_reactive!(|place, owned| {
            let Some(place) = place.filter(|_| owned) else {
                return;
            };
            let k = key(&place);
            if let Some((project, len)) = opened.borrow().get(&k).cloned() {
                web_engine::switch_song(&project);
                web_host::hosted_show(&project);
                // Back on a song opened before: its length, which a pick
                // (`session::go_to`) cleared.
                app.np.with_mut(|n| {
                    if let Some(n) = n {
                        n.dur = len;
                        n.source = crate::state::Source::Ready;
                    }
                });
                up.set(Up::Ready { project, len });
                return;
            }
            let (opened, started, rigs_from) = (opened.clone(), started.clone(), rigs_from.clone());
            spawn(async move {
                let title = place
                    .folder
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                up.set(Up::Opening(title.clone()));
                if place.arrangement.is_some() {
                    tracing::info!(song.title = %title, "web: an arrangement opens as recorded (the page reads the song's main branch)");
                }
                let song = HostedSong {
                    title,
                    source: Arc::new(SessionFolder::new(place.clone())),
                };
                // Its rigs' store, opened beside it the first time.
                if let Some(store) = rig_store(&place)
                    && rigs_from.borrow_mut().insert(key(&store))
                {
                    web_engine::use_rig_store(Arc::new(SessionFolder::new(store)));
                }
                // Each song the engine opens is one the session's views can
                // show (`web_host`'s hosted set).
                let opening = if started.replace(true) {
                    web_engine::add_song(song).await.inspect(|song| {
                        web_host::host_added(song.clone());
                    })
                } else {
                    web_engine::open(&web_engine::WebSource::Hosted(vec![song]), None, |_| {})
                        .await
                        .and_then(|(engine, setlist)| {
                            let first = setlist
                                .songs
                                .first()
                                .cloned()
                                .ok_or_else(|| eyre::eyre!("the session opened no song"))?;
                            web_host::host_opened(engine, setlist);
                            Ok(first)
                        })
                };
                match opening {
                    Ok(song) => {
                        crate::chart_generated::remember(&place, &song);
                        let len = song.span.1.max(0.0);
                        opened.borrow_mut().insert(k, (song.project.clone(), len));
                        // Still the song on screen: shown, and played if asked.
                        let now = app.np.peek().as_ref().and_then(|n| n.place().map(key));
                        if now.as_deref() == Some(&key(&place)) {
                            web_engine::switch_song(&song.project);
                            web_host::hosted_show(&song.project);
                            app.np.with_mut(|n| {
                                if let Some(n) = n {
                                    n.dur = len;
                                    n.source = crate::state::Source::Ready;
                                }
                            });
                            if app.np.peek().as_ref().is_some_and(|n| n.playing) {
                                web_engine::set_playing(true);
                            }
                            up.set(Up::Ready {
                                project: song.project,
                                len,
                            });
                        }
                        if place.location == LINK && keep_in_address() && kept.peek().is_empty() {
                            kept.set("keeping".to_owned());
                            let place = place.clone();
                            spawn(async move {
                                kept.set(match keep_offline(&place).await {
                                    Ok(_) => "kept".to_owned(),
                                    Err(e) => format!("not kept: {e}"),
                                });
                            });
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, folder = %place.folder, "web: the session did not open; its reference plays");
                        // The first open failed before the engine was made:
                        // the next song tries to make it again.
                        if !web_engine::is_up() {
                            started.set(false);
                        }
                        up.set(Up::Failed(e.to_string()));
                        play.with_mut(|p| {
                            p.failed.insert(k);
                        });
                    }
                }
            });
        }));
    }

    // Play and pause as Now Playing says, once the song is open.
    let playing = (app.np)().is_some_and(|n| n.playing);
    let ready = matches!(up(), Up::Ready { .. });
    use_effect(use_reactive!(|playing, ready, owned| {
        if ready && owned {
            web_engine::set_playing(playing);
        }
    }));

    // A jump asked for (the progress bar, a section, an action).
    use_effect(move || {
        let Some(t) = (app.seek)() else { return };
        let owns = play
            .peek()
            .owns(app.np.peek().as_ref().and_then(|n| n.place()));
        if !owns || !matches!(*up.peek(), Up::Ready { .. }) {
            return;
        }
        web_engine::seek(t);
        app.np.with_mut(|n| {
            if let Some(n) = n {
                let (_, len) = n.secs();
                if len > 0.0 {
                    n.pos = (t / len).clamp(0.0, 1.0);
                }
            }
        });
        app.seek.set(None);
    });

    // What the engine does, to Now Playing and the state element.
    use_future(move || async move {
        loop {
            crate::state::sleep_ms(100).await;
            let state = up.peek().clone();
            write_state(&state, &kept.peek());
            // A rig prepared as it first plays asks for its files then.
            studio_engine::rig_files::poke();
            let Up::Ready { len, .. } = state else {
                continue;
            };
            let owns = play
                .peek()
                .owns(app.np.peek().as_ref().and_then(|n| n.place()));
            if !owns {
                continue;
            }
            let (at, moving) = web_engine::position();
            let Some(n) = app.np.peek().clone() else {
                continue;
            };
            let ended = n.playing && len > 0.0 && at >= len - 0.05;
            let moved = len > 0.0 && ((n.pos * len) - at).abs() > 0.05;
            if !ended && !moved {
                continue;
            }
            app.np.with_mut(|n| {
                let Some(n) = n else { return };
                if ended {
                    // The set plays on: the next song, from its start.
                    web_engine::set_playing(false);
                    if !n.next() {
                        n.playing = false;
                        n.pos = 1.0;
                    }
                    return;
                }
                if len > 0.0 && (moving || !n.playing) {
                    n.pos = (at / len).clamp(0.0, 1.0);
                }
            });
        }
    });

    rsx! {
        div { id: STATE_ID, style: "display:none" }
    }
}

/// The engine's state on [`STATE_ID`]'s attributes: `data-state`
/// (`idle`, `opening`, `ready`, `failed`), `data-why`, `data-at` and
/// `data-playing` (the song's playhead as heard), what the worklet has
/// put out (`data-loud-frames`, `data-peak-ever`, `data-dry-frames`), and
/// whether `?keep=1` kept the session on this device (`data-kept`).
fn write_state(up: &Up, kept: &str) {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(STATE_ID))
    else {
        return;
    };
    let (state, why) = match up {
        Up::Idle => ("idle", String::new()),
        Up::Opening(title) => ("opening", title.clone()),
        Up::Ready { project, .. } => ("ready", project.clone()),
        Up::Failed(why) => ("failed", why.clone()),
    };
    let (at, playing) = web_engine::position();
    let probe = studio_engine::web_audio::probe();
    let rigs = studio_engine::rig_files::status();
    for (name, value) in [
        ("data-state", state.to_owned()),
        ("data-why", why),
        ("data-at", format!("{at:.3}")),
        ("data-playing", playing.to_string()),
        ("data-loud-frames", probe.loud_frames.to_string()),
        ("data-dry-frames", probe.dry_frames.to_string()),
        ("data-peak-ever", format!("{:.4}", probe.peak_ever)),
        // The rigs' files: the store open, what came from it and from the
        // page's cache, what it lacked, what is still wanted, the rigs
        // waiting, and how loud the rigs (and kit voices) on streamed files
        // have played.
        ("data-rig-store", rigs.store.to_string()),
        ("data-rig-home", rigs.rigs.home_files.to_string()),
        ("data-rig-fetched", rigs.counts.fetched.to_string()),
        ("data-rig-cached", rigs.counts.cached.to_string()),
        ("data-rig-missing", rigs.counts.missing.to_string()),
        ("data-rig-wanted", rigs.rigs.wanted.to_string()),
        ("data-rig-waiting", rigs.rigs.waiting.to_string()),
        ("data-rig-heard", format!("{:.4}", rigs.rigs.heard)),
        ("data-rig-kit-heard", format!("{:.4}", rigs.rigs.kit_heard)),
        // The first of the files still wanted: what a rig waits on.
        (
            "data-rig-wanted-keys",
            studio_engine::rig_files::wanted_keys(8).join(" | "),
        ),
        (
            "data-rig-signal-heard",
            format!("{:.4}", rigs.rigs.signal_rig_heard),
        ),
        ("data-kept", kept.to_owned()),
    ] {
        let _ = el.set_attribute(name, &value);
    }
}

/// Whether the page's address asks for its session to be kept on this
/// device (`?keep=1`), to open again with no network.
fn keep_in_address() -> bool {
    ADDRESS
        .get()
        .and_then(|a| web_sys::UrlSearchParams::new_with_str(a).ok())
        .and_then(|p| p.get("keep"))
        .is_some_and(|v| v == "1" || v == "true")
}

/// The session folder the page's address names (`?session=<url>`).
fn session_in_address() -> Option<Place> {
    linked_in_address("session")
}

/// A folder at a link the page's address names (`?<name>=<url>`).
fn linked_in_address(name: &str) -> Option<Place> {
    let params = web_sys::UrlSearchParams::new_with_str(ADDRESS.get()?).ok()?;
    let folder = params.get(name).filter(|s| !s.is_empty())?;
    Some(Place {
        space: String::new(),
        location: LINK.to_owned(),
        folder,
        chart: None::<Hit>,
        arrangement: None,
    })
}

/// The rig store the session at `session` plays its rigs from: a folder
/// at a link (`?rigs=<url>`), else [`RIG_LIBRARY`] at the root of the
/// session's Drive location (read with this person's sign-in). A session
/// at a link with no `?rigs=` has none: its rigs play as straight wires.
fn rig_store(session: &Place) -> Option<Place> {
    if let Some(linked) = linked_in_address("rigs") {
        return Some(linked);
    }
    (session.location != LINK).then(|| Place {
        space: session.space.clone(),
        location: session.location.clone(),
        folder: RIG_LIBRARY.to_owned(),
        chart: None::<Hit>,
        arrangement: None,
    })
}
