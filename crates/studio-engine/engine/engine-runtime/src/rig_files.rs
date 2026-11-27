//! A page's rigs, fed from the rig store.
//!
//! signal's rigs read their captures, IRs and homes from a desktop's disk.
//! A page has none, so it reads them from the **rig store**: the same files
//! the desktop's rigs come from, kept in Drive (a location laid out as
//! `rig-library/`: `bass/models/…`, `bass/profiles/…`, `bass/*.styx`,
//! `guitar/…`) and read through the node with the viewer's sign-in, or a
//! folder at a link. Nothing is bundled into the web build.
//!
//! - [`open`] reads the store's listing and installs its homes (the `.styx`
//!   text, a few kB) before a song's rigs read them.
//! - [`feed`] fetches what the page's rigs wait for
//!   ([`signal_fx::files::wanted`]) once a song has opened, installs each
//!   file, and has the rigs built again ([`signal_fx::files::arrived`]). A
//!   rig sounds once its files are in: the song does not wait for them.
//! - Each file is kept in the page's Cache Storage (`fts-rigs`, by its path
//!   and size), so the next load reads it from there.
//!
//! A capture (`.nam`, JSON), an IR (`.wav`), a kit's preset and engines
//! are read whole, through the source's range read. A `.signalpack` is
//! read in part: a rig wants spans of it ([`signal_fx::files::range_key`]:
//! its header, its index, the samples it plays), and each span is fetched
//! by range and cached on its own.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use futures_util::StreamExt as _;
use signal_fx::files;

use crate::source::SongSource;

/// The page's Cache Storage the rig files are kept in.
const CACHE: &str = "fts-rigs";
/// Files fetched side by side.
const AT_ONCE: usize = 4;

/// The rig store, as the page reads it.
struct Store {
    source: Arc<dyn SongSource>,
    /// Each file's size, by its path in the store.
    sizes: HashMap<String, u64>,
}

thread_local! {
    static STORE: RefCell<Option<Rc<Store>>> = const { RefCell::new(None) };
    static FEEDING: Cell<bool> = const { Cell::new(false) };
    static COUNTS: RefCell<Counts> = RefCell::default();
    /// Keys the store lacked or that did not come: not asked for again.
    static GAVE_UP: RefCell<HashSet<String>> = RefCell::default();
}

/// What is wanted that the feed has not given up on.
fn to_fetch() -> Vec<String> {
    let wanted = files::wanted();
    GAVE_UP.with(|g| {
        let g = g.borrow();
        wanted.into_iter().filter(|k| !g.contains(k)).collect()
    })
}

/// Feed the rigs if they want something and no feed runs: cheap enough to
/// call as often as the page likes (a rig asks for its files when it is
/// prepared, which may be at the first block it plays).
pub fn poke() {
    if FEEDING.with(Cell::get) || STORE.with(|s| s.borrow().is_none()) || to_fetch().is_empty() {
        return;
    }
    wasm_bindgen_futures::spawn_local(async {
        feed().await;
    });
}

/// What the feed has done.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counts {
    /// Files fetched from the store.
    pub fetched: usize,
    /// Files read from the page's cache instead.
    pub cached: usize,
    /// Keys the store does not have, or that did not come.
    pub missing: usize,
}

/// What the page's rigs and their feed are doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Status {
    /// Whether a rig store is open.
    pub store: bool,
    pub counts: Counts,
    pub rigs: files::Status,
}

/// The first `count` keys the page's rigs still want, for the page to
/// show what a rig waits on (a store that lacks them).
#[must_use]
pub fn wanted_keys(count: usize) -> Vec<String> {
    files::wanted().into_iter().take(count).collect()
}

/// The rig store's and the rigs' [`Status`].
#[must_use]
pub fn status() -> Status {
    Status {
        store: STORE.with(|s| s.borrow().is_some()),
        counts: COUNTS.with(|c| *c.borrow()),
        rigs: files::status(),
    }
}

/// Read the rig store `source`: its listing, and its homes' text installed
/// for the rigs to read. How many home files it had.
///
/// # Errors
///
/// The store did not list.
pub async fn open(source: Arc<dyn SongSource>) -> eyre::Result<usize> {
    let listing = source.list().await?;
    let sizes: HashMap<String, u64> = listing.entries.into_iter().collect();
    let store = Rc::new(Store { source, sizes });
    let homes: Vec<(String, u64)> = store
        .sizes
        .iter()
        .filter(|(path, _)| path.ends_with(".styx"))
        .map(|(path, size)| (path.clone(), *size))
        .collect();
    let cache = cache().await;
    let read = futures_util::stream::iter(homes)
        .map(|(path, size)| {
            let (store, cache) = (Rc::clone(&store), cache.clone());
            async move {
                let bytes = read(&store, cache.as_ref(), &path, (0, size), size).await;
                (path, bytes)
            }
        })
        .buffer_unordered(AT_ONCE)
        .collect::<Vec<_>>()
        .await;
    let mut installed = 0;
    for (path, bytes) in read {
        match bytes {
            Ok(bytes) => {
                files::install_text(
                    format!("{}/{path}", files::ROOT),
                    String::from_utf8_lossy(&bytes).into_owned(),
                );
                installed += 1;
            }
            Err(e) => {
                tracing::warn!(rig.file = %path, error = %e, "web: a rig home's file did not come")
            }
        }
    }
    tracing::info!(
        rig.store = %store.source.label(),
        rig.files = store.sizes.len(),
        rig.home_files = installed,
        "web: the rig store is open"
    );
    STORE.with(|s| *s.borrow_mut() = Some(store));
    Ok(installed)
}

/// Fetch what the page's rigs wait for, install it, and have them built
/// again with it. Runs until nothing more is wanted that the store has (a
/// key it lacks is given up on, said once); a call while one runs leaves
/// it to that one. How many files came.
pub async fn feed() -> usize {
    let Some(store) = STORE.with(|s| s.borrow().clone()) else {
        return 0;
    };
    if FEEDING.with(|f| f.replace(true)) {
        return 0;
    }
    let cache = cache().await;
    let mut came = 0;
    loop {
        let wanted = to_fetch();
        if wanted.is_empty() {
            break;
        }
        let fetched = futures_util::stream::iter(wanted)
            .map(|key| {
                let (store, cache) = (Rc::clone(&store), cache.clone());
                async move {
                    let bytes = match span_in(&store, &key) {
                        Some((path, span, size)) => {
                            read(&store, cache.as_ref(), &path, span, size).await
                        }
                        None => Err(format!("the rig store has no {key}")),
                    };
                    (key, bytes)
                }
            })
            .buffer_unordered(AT_ONCE)
            .collect::<Vec<_>>()
            .await;
        for (key, bytes) in fetched {
            match bytes {
                Ok(bytes) => {
                    files::install(key, bytes);
                    came += 1;
                }
                Err(e) => {
                    tracing::warn!(rig.file = %key, error = %e, "web: a rig's file did not come; the rig plays without it");
                    COUNTS.with(|c| c.borrow_mut().missing += 1);
                    GAVE_UP.with(|g| g.borrow_mut().insert(key));
                }
            }
        }
        let built = files::arrived();
        tracing::info!(
            rig.files = came,
            rig.rebuilt = built,
            "web: rig files arrived"
        );
    }
    FEEDING.with(|f| f.set(false));
    came
}

/// Where `key` is in the store: its file's path, the span of it the key
/// names (the whole file, for a key that names none) and the file's size.
/// `None` for a file the store lacks or a span past its end.
fn span_in(store: &Store, key: &str) -> Option<(String, (u64, u64), u64)> {
    let (whole, span) = files::range_of(key);
    let path = files::store_path(whole)?;
    let size = *store.sizes.get(path)?;
    let span = span.unwrap_or((0, size));
    (span.0.checked_add(span.1)? <= size).then(|| (path.to_owned(), span, size))
}

/// `span` (start, length) of `path` (`size` bytes) from the store: kept in
/// the page's cache, else fetched by range and kept.
async fn read(
    store: &Store,
    cache: Option<&web_sys::Cache>,
    path: &str,
    (start, len): (u64, u64),
    size: u64,
) -> Result<Vec<u8>, String> {
    let key = cache_key(&store.source.label(), path, size, (start, len));
    if let Some(bytes) = match cache {
        Some(cache) => cache_get(cache, &key).await,
        None => None,
    } {
        COUNTS.with(|c| c.borrow_mut().cached += 1);
        return Ok(bytes);
    }
    let bytes = store
        .source
        .read(path.to_owned(), start..start + len)
        .await?;
    COUNTS.with(|c| c.borrow_mut().fetched += 1);
    if let Some(cache) = cache {
        cache_put(cache, &key, &bytes).await;
    }
    Ok(bytes)
}

/// Where `span` of `path` of the store `label` is kept, at `size` bytes: a
/// file that changes size is fetched again. A whole file keeps the key it
/// always had.
fn cache_key(label: &str, path: &str, size: u64, (start, len): (u64, u64)) -> String {
    let escape = |s: &str| String::from(js_sys::encode_uri_component(s));
    let path = path.split('/').map(escape).collect::<Vec<_>>().join("/");
    let span = if (start, len) == (0, size) {
        String::new()
    } else {
        format!("&span={start}+{len}")
    };
    format!(
        "https://fts-cache.invalid/rigs/{}/{path}?size={size}{span}",
        escape(label)
    )
}

/// The page's rig cache, when the browser gives one (a private window
/// may not): without it every load fetches.
async fn cache() -> Option<web_sys::Cache> {
    use wasm_bindgen::JsCast as _;
    let caches = web_sys::window()?.caches().ok()?;
    let cache = wasm_bindgen_futures::JsFuture::from(caches.open(CACHE))
        .await
        .ok()?;
    cache.dyn_into::<web_sys::Cache>().ok()
}

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

/// Keep `bytes` as `key`: a cache that is full or refused costs only a
/// fetch next time.
async fn cache_put(cache: &web_sys::Cache, key: &str, bytes: &[u8]) {
    let mut owned = bytes.to_vec();
    if let Ok(response) = web_sys::Response::new_with_opt_u8_array(Some(&mut owned)) {
        let _ = wasm_bindgen_futures::JsFuture::from(cache.put_with_str(key, &response)).await;
    }
}
