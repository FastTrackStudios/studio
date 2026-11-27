//! A rig's files where there is no disk: a page.
//!
//! On a desktop a rig reads its captures, IRs and home from disk, and this
//! module stays out of the way: [`located`] gives a path back unchanged and
//! nothing is ever wanted.
//!
//! In a browser the files are in the **rig store**, the one the desktop
//! uses (a Drive location laid out as `rig-library/`: `bass/models/…`,
//! `bass/profiles/…`, `guitar/…`), and the page streams in what its rigs
//! play. Each file has a key under [`ROOT`] (`/rigs/bass/models/Amp.nam`).
//! A desktop path a session saved (`…/bass-rig/models/Amp.nam`) maps to the
//! same key by its folder and name, so a session made on a desktop plays
//! here.
//!
//! How a rig comes to sound:
//!
//! 1. It builds with what is installed, and [`wait`]s for the rest. Its keys
//!    are then [`wanted`], and until they arrive the rig plays what it has
//!    (a straight wire when it has nothing).
//! 2. The page fetches the wanted keys from the store, [`install`]s them,
//!    and calls [`arrived`]. That builds each rig whose files are all in,
//!    on the page's thread, never the audio's.
//! 3. The rig takes its new chain at its next block ([`Slot::take`]) and
//!    hands back the old one, which is dropped on the page's thread too.
//!
//! A rig whose files name more files (a kit's preset its engines, an
//! engine its pack) waits in stages ([`wait_for`]): each time what it
//! waits for arrives, it says what it wants next, until it can be built.
//!
//! The home (its `.styx` files) is small text and is installed before a
//! rig reads it ([`install_text`]).
//!
//! ## Packs, by range
//!
//! A `.signalpack` is the one format a rig reads in part. Its key names a
//! span of it ([`range_key`]: `…/Kick.signalpack#0+64`), the page fetches
//! only that span, and [`pack`] opens the pack over the spans that have
//! come: its header, then its index (which says where every sample is),
//! then only the samples a kit plays (the mics it wants), never the whole
//! pack.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use guitar::instrument::Instrument;

/// Where every key is: the rig store's root, as a page names it.
pub const ROOT: &str = "/rigs";

/// An instrument's folder in the store (`bass`, `guitar`).
#[must_use]
pub const fn folder(instrument: Instrument) -> &'static str {
    match instrument {
        Instrument::Bass => "bass",
        Instrument::Guitar => "guitar",
    }
}

/// An instrument's rig home, where there is no disk: `/rigs/<instrument>`.
#[must_use]
pub fn home(instrument: Instrument) -> PathBuf {
    PathBuf::from(format!("{ROOT}/{}", folder(instrument)))
}

/// The key `path` is known by, for `instrument`'s rig.
///
/// A key stays as it is. A relative path is under the home. An absolute
/// path from a desktop is the file of that name in the home's folder it
/// sat in (`…/frozen/x.nam` → `/rigs/bass/frozen/x.nam`), else in
/// `models/`. On a desktop, every path is its own key.
#[must_use]
pub fn located(path: &str, instrument: Instrument) -> String {
    if cfg!(not(target_arch = "wasm32")) || path.is_empty() || path.starts_with(ROOT) {
        return path.to_owned();
    }
    let home = home(instrument);
    let given = Path::new(path);
    if !path.starts_with('/') && !path.contains(":\\") {
        return home.join(given).to_string_lossy().into_owned();
    }
    // A desktop's path, either way its slashes lean.
    let mut parts = path.rsplit(['/', '\\']);
    let file = parts.next().unwrap_or_default();
    let folder = match parts.next() {
        Some(f @ ("models" | "frozen" | "irs" | "packs")) => f,
        _ => "models",
    };
    home.join(folder).join(file).to_string_lossy().into_owned()
}

/// The store's folder of drum kits, laid out as a desktop's `Drum Kits`:
/// `drums/<library>/Presets/…`, `…/Engines/…`, `…/Packs/…`.
pub const KITS: &str = "drums";

/// The key a kit's preset `path` is known by: on a desktop, the path. In a
/// page a key stays as it is, a relative path is under [`KITS`], and a
/// desktop's path is the same file in the store's kits: what follows its
/// `Drum Kits` folder, else its library, folder and name
/// (`…/GGD Modern and Massive 2/Presets/Heavy Thwacky.signalpreset`).
#[must_use]
pub fn kit_located(path: &str) -> String {
    if cfg!(not(target_arch = "wasm32")) {
        return path.to_owned();
    }
    kit_key(path)
}

/// [`kit_located`]'s mapping, on every target.
pub(crate) fn kit_key(path: &str) -> String {
    let kits = format!("{ROOT}/{KITS}");
    if path.is_empty() || path.starts_with(ROOT) {
        return path.to_owned();
    }
    let drive = path.as_bytes().get(1) == Some(&b':');
    if !path.starts_with(['/', '\\']) && !drive {
        return joined(&kits, path);
    }
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let after = parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("Drum Kits"))
        .map_or_else(|| parts.len().saturating_sub(3), |at| at + 1);
    joined(&kits, &parts.get(after..).unwrap_or_default().join("/"))
}

/// Where `key` is in the store (`bass/models/Amp.nam`), when it is a key.
#[must_use]
pub fn store_path(key: &str) -> Option<&str> {
    key.strip_prefix(ROOT)?
        .strip_prefix('/')
        .filter(|p| !p.is_empty())
}

/// `key`'s bytes, when installed.
#[must_use]
pub fn bytes(key: &str) -> Option<Arc<[u8]>> {
    sampler::assets::get(key)
}

/// Whether `key` has been installed.
#[must_use]
pub fn has(key: &str) -> bool {
    sampler::assets::contains(key)
}

/// Keep `bytes` as `key`'s: what a rig reads before it would touch a disk.
/// A span of a pack ([`range_key`]) is kept for the pack to read it.
pub fn install(key: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
    let key = key.into();
    if let (whole, Some((start, len))) = range_of(&key) {
        lock(spans())
            .entry(whole.to_owned())
            .or_default()
            .insert((start, len));
    }
    sampler::assets::install(key, bytes);
}

/// The key of `len` bytes at `start` of the file `key`: what a page
/// fetches of a pack.
#[must_use]
pub fn range_key(key: &str, start: u64, len: u64) -> String {
    format!("{key}#{start}+{len}")
}

/// `key` split into its file's key and the span it names, if it names one
/// ([`range_key`]).
#[must_use]
pub fn range_of(key: &str) -> (&str, Option<(u64, u64)>) {
    let Some((whole, span)) = key.rsplit_once('#') else {
        return (key, None);
    };
    let parsed = span
        .split_once('+')
        .and_then(|(s, l)| Some((s.parse().ok()?, l.parse().ok()?)));
    match parsed {
        Some(span) => (whole, Some(span)),
        None => (key, None),
    }
}

/// The spans installed of each pack, by its key.
fn spans() -> &'static Mutex<HashMap<String, BTreeSet<(u64, u64)>>> {
    static SPANS: OnceLock<Mutex<HashMap<String, BTreeSet<(u64, u64)>>>> = OnceLock::new();
    SPANS.get_or_init(Mutex::default)
}

/// The packs opened over their spans, by the id their reader knows them by.
fn opened() -> &'static Mutex<Vec<String>> {
    static OPENED: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    OPENED.get_or_init(Mutex::default)
}

/// `dst.len()` bytes at `offset` of the pack `id`, from a span installed
/// that holds them all. Whether they were there.
fn read_span(id: u32, offset: u64, dst: &mut [u8]) -> bool {
    let Some(key) = lock(opened()).get(id as usize).cloned() else {
        return false;
    };
    let want = dst.len() as u64;
    let span = lock(spans()).get(&key).and_then(|spans| {
        spans
            .iter()
            .find(|&&(start, len)| start <= offset && offset + want <= start + len)
            .copied()
    });
    let Some((start, len)) = span else {
        return false;
    };
    let Some(bytes) = sampler::assets::get(&range_key(&key, start, len)) else {
        return false;
    };
    let Ok(at) = usize::try_from(offset - start) else {
        return false;
    };
    match bytes.get(at..at + dst.len()) {
        Some(found) => {
            dst.copy_from_slice(found);
            true
        }
        None => false,
    }
}

/// The pack at `key`, opened over the spans of it that have arrived: its
/// header and index first, wanted (as [`range_key`]s) until they are in.
/// Its samples are read as spans too ([`entries_of`]).
///
/// # Errors
///
/// [`Need::Keys`] while the header or the index is still to come;
/// [`Need::Failed`] for a file that is not a pack.
pub fn pack(key: &str) -> Result<sampler::SignalPcmPack, Need> {
    use sampler::engine::cache::{PackFileHeader, SignalPcmPack, set_external_pack_reader};
    let head = range_key(key, 0, PackFileHeader::LEN as u64);
    let Some(raw) = bytes(&head) else {
        return Err(Need::Keys(vec![head]));
    };
    let raw: [u8; PackFileHeader::LEN] = raw
        .get(..PackFileHeader::LEN)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Need::Failed(format!("{key}: shorter than a pack's header")))?;
    let header = PackFileHeader::parse(raw).map_err(|e| Need::Failed(format!("{key}: {e}")))?;
    let index = range_key(key, header.index_offset(), header.index_len());
    if !has(&index) {
        return Err(Need::Keys(vec![index]));
    }
    // One reader for every pack opened so. Only the first set in a process
    // counts: a pack opened under someone else's would read their bytes.
    static OURS: OnceLock<bool> = OnceLock::new();
    if !*OURS.get_or_init(|| set_external_pack_reader(Box::new(read_span))) {
        return Err(Need::Failed(
            "another pack reader is installed; packs cannot be read by range".to_owned(),
        ));
    }
    let id = {
        let mut opened = lock(opened());
        let at = opened.iter().position(|k| k == key).unwrap_or_else(|| {
            opened.push(key.to_owned());
            opened.len() - 1
        });
        u32::try_from(at).map_err(|_| Need::Failed("too many packs".to_owned()))?
    };
    // The index is the last thing in a pack: its end is the pack's.
    let len = header.index_offset() + header.index_len();
    SignalPcmPack::open_external(id, len).map_err(|e| Need::Failed(format!("{key}: {e}")))
}

/// The keys of the samples `paths` names in the pack at `key` (opened,
/// [`pack`]): one span each.
#[must_use]
pub fn entries_of<'a>(
    key: &str,
    pack: &sampler::SignalPcmPack,
    paths: impl Iterator<Item = &'a Path>,
) -> Vec<String> {
    let mut keys: Vec<String> = paths
        .filter_map(|p| pack.entry_for_path(p))
        .map(|e| range_key(key, e.offset(), e.bytes()))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// What a rig waiting in stages still needs ([`wait_for`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Need {
    /// These keys, to be fetched and installed.
    Keys(Vec<String>),
    /// Nothing that can come: the rig plays without it.
    Failed(String),
}

impl Need {
    /// The keys of both: what is needed from either.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Keys(mut a), Self::Keys(b)) => {
                a.extend(b);
                Self::Keys(a)
            }
            (Self::Failed(why), _) | (_, Self::Failed(why)) => Self::Failed(why),
        }
    }
}

/// The text of the file `key`, once installed.
///
/// # Errors
///
/// [`Need::Keys`] until it is.
pub fn text(key: &str) -> Result<String, Need> {
    bytes(key)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .ok_or_else(|| Need::Keys(vec![key.to_owned()]))
}

/// `path` joined to `dir` and its `..`s taken out, as a key: where a
/// file's relative path from another lands (`/rigs/drums/Kit/Presets` +
/// `../Engines/Kick.signalengine`).
#[must_use]
pub fn joined(dir: &str, path: &str) -> String {
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        dir.split('/').filter(|p| !p.is_empty()).collect()
    };
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The folder of the key `key`.
#[must_use]
pub fn dir_of(key: &str) -> &str {
    key.rsplit_once('/').map_or("", |(d, _)| d)
}

/// Keep `text` as the home file `key` (`/rigs/bass/drive-presets.styx`).
pub fn install_text(key: impl Into<PathBuf>, text: impl Into<String>) {
    signal_rig_host::store::memory::install(key, text);
}

/// The page's side only: the audio thread takes a slot with `try_lock`.
#[allow(
    clippy::disallowed_methods,
    reason = "never on the audio thread: wanting, installing and building are the page's"
)]
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The keys the page's rigs wait for and nobody has installed, in order.
/// A rig built again before they came (its state loaded anew) waits for
/// its new ones only.
#[must_use]
pub fn wanted() -> Vec<String> {
    let keys: BTreeSet<String> = live()
        .iter()
        .filter(|p| p.waiting())
        .flat_map(|p| p.keys())
        .filter(|k| !sampler::assets::contains(k))
        .collect();
    keys.into_iter().collect()
}

/// A rig's chain, made again once its files are in.
trait Pending: Send + Sync {
    /// The files it waits for.
    fn keys(&self) -> Vec<String>;
    /// Whether it is still to be made.
    fn waiting(&self) -> bool;
    /// Build, if every file is in and it has not been built. Whether it was.
    fn build_if_ready(&self) -> bool;
    /// Whether the rig took its chain: nothing is left to do.
    fn done(&self) -> bool;
}

/// Where a rig's new chain waits for it: made by [`arrived`], taken by the
/// rig on the audio thread ([`take`](Self::take)).
pub struct Slot<T> {
    /// What it waits for now: a staged rig's next files once these come.
    keys: Mutex<Vec<String>>,
    make: Box<dyn Fn() -> Result<T, Need> + Send + Sync>,
    /// The new chain, then (once taken) the old one, to drop here.
    held: Mutex<Option<T>>,
    stage: AtomicU8,
}

/// The most stages a rig is let go through at once.
const STAGES: usize = 8;

const WAITING: u8 = 0;
const BUILT: u8 = 1;
const TAKEN: u8 = 2;

impl<T: Send> Slot<T> {
    /// The new chain, once it is made: for the rig's audio thread. It never
    /// waits on the lock, and allocates and frees nothing.
    pub fn take(&self) -> Option<T> {
        if self.stage.load(Ordering::Acquire) != BUILT {
            return None;
        }
        let mut held = self.held.try_lock().ok()?;
        let built = held.take()?;
        drop(held);
        Some(built)
    }

    /// The chain the rig played before, handed back to be dropped off the
    /// audio thread ([`arrived`] drops it).
    pub fn retire(&self, old: T) {
        if let Ok(mut held) = self.held.try_lock() {
            *held = Some(old);
        }
        self.stage.store(TAKEN, Ordering::Release);
    }
}

impl<T: Send> Pending for Slot<T> {
    fn keys(&self) -> Vec<String> {
        lock(&self.keys).clone()
    }

    fn waiting(&self) -> bool {
        self.stage.load(Ordering::Acquire) == WAITING
    }

    fn build_if_ready(&self) -> bool {
        if self.stage.load(Ordering::Acquire) != WAITING
            || !self.keys().iter().all(|k| sampler::assets::contains(k))
        {
            return false;
        }
        // A stage whose files are in already (another rig fetched them)
        // goes straight on to the next; a rig has a few stages, never many.
        for _ in 0..STAGES {
            match (self.make)() {
                Ok(built) => {
                    *lock(&self.held) = Some(built);
                    self.stage.store(BUILT, Ordering::Release);
                    return true;
                }
                // The next stage: what these files named.
                Err(Need::Keys(next)) => {
                    let ready = next.iter().all(|k| sampler::assets::contains(k));
                    *lock(&self.keys) = next;
                    if !ready {
                        return false;
                    }
                }
                Err(Need::Failed(why)) => {
                    tracing::warn!(
                        why,
                        "a rig's files would not build it; it plays without them"
                    );
                    lock(&self.keys).clear();
                    self.stage.store(TAKEN, Ordering::Release);
                    return false;
                }
            }
        }
        tracing::warn!(
            stages = STAGES,
            "a rig's files named more stages than any rig has; it plays without them"
        );
        lock(&self.keys).clear();
        self.stage.store(TAKEN, Ordering::Release);
        false
    }

    fn done(&self) -> bool {
        self.stage.load(Ordering::Acquire) == TAKEN
    }
}

fn waiting() -> &'static Mutex<Vec<Arc<dyn Pending>>> {
    static SLOTS: OnceLock<Mutex<Vec<Arc<dyn Pending>>>> = OnceLock::new();
    SLOTS.get_or_init(Mutex::default)
}

/// Make a rig's chain again with `make` once every one of `keys` is
/// installed, and want them meanwhile. The rig keeps the slot and takes
/// what it holds ([`Slot::take`]).
pub fn wait<T: Send + 'static>(
    keys: Vec<String>,
    make: impl Fn() -> T + Send + Sync + 'static,
) -> Arc<Slot<T>> {
    let first = keys.clone();
    wait_for(first, move || {
        if keys.iter().all(|k| sampler::assets::contains(k)) {
            Ok(make())
        } else {
            Err(Need::Keys(keys.clone()))
        }
    })
}

/// Make a rig's chain with `make` once it can be, in stages: `keys` first,
/// then whatever `make` says it [`Need`]s once they are in, until it makes
/// one. The rig keeps the slot and takes what it holds ([`Slot::take`]).
pub fn wait_for<T: Send + 'static>(
    keys: Vec<String>,
    make: impl Fn() -> Result<T, Need> + Send + Sync + 'static,
) -> Arc<Slot<T>> {
    let slot = Arc::new(Slot {
        keys: Mutex::new(keys),
        make: Box::new(make),
        held: Mutex::new(None),
        stage: AtomicU8::new(WAITING),
    });
    let pending: Arc<dyn Pending> = slot.clone();
    lock(waiting()).push(pending);
    slot
}

/// The rigs' slots still in play, the rest let go of (and dropped here,
/// with any chain a rig handed back): a slot its rig took, or whose rig
/// is gone or was built again (only this list holds it).
fn live() -> Vec<Arc<dyn Pending>> {
    let mut list = lock(waiting());
    list.retain(|p| !p.done() && Arc::strong_count(p) > 1);
    list.clone()
}

/// Files have arrived: make each rig whose files are all in, here, and let
/// go of the ones done. How many were made.
pub fn arrived() -> usize {
    live().iter().filter(|p| p.build_if_ready()).count()
}

/// The loudest a rig built from streamed files has put out (linear peak).
static HEARD: AtomicU32 = AtomicU32::new(0);
/// The loudest a kit voice on streamed samples has put out.
static KIT_HEARD: AtomicU32 = AtomicU32::new(0);

/// A rig playing streamed files says how loud it was ([`Status::heard`]).
pub(crate) fn note_heard(peak: f32) {
    HEARD.fetch_max(peak.abs().to_bits(), Ordering::Relaxed);
}

/// A kit voice playing streamed samples says how loud it was
/// ([`Status::kit_heard`]).
pub(crate) fn note_heard_kit(peak: f32) {
    KIT_HEARD.fetch_max(peak.abs().to_bits(), Ordering::Relaxed);
}

/// What the page's rigs are doing, for the page to show (and its boot test
/// to read).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Status {
    /// Keys wanted and not yet installed.
    pub wanted: usize,
    /// Rigs still waiting for their files.
    pub waiting: usize,
    /// Home files held.
    pub home_files: usize,
    /// The loudest a rig on streamed files has played (linear peak).
    pub heard: f32,
    /// The loudest a kit voice on streamed samples has played.
    pub kit_heard: f32,
}

/// The rigs' [`Status`].
#[must_use]
pub fn status() -> Status {
    Status {
        wanted: wanted().len(),
        waiting: live().iter().filter(|p| p.waiting()).count(),
        home_files: signal_rig_host::store::memory::len(),
        heard: f32::from_bits(HEARD.load(Ordering::Relaxed)),
        kit_heard: f32::from_bits(KIT_HEARD.load(Ordering::Relaxed)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_its_store_path_under_the_root() {
        assert_eq!(
            store_path("/rigs/bass/models/Amp.nam"),
            Some("bass/models/Amp.nam")
        );
        assert_eq!(store_path("/rigs/"), None);
        assert_eq!(store_path("/Users/x/Amp.nam"), None);
    }

    #[test]
    fn a_desktop_keeps_its_paths() {
        assert_eq!(
            located("/Users/x/bass-rig/models/Amp.nam", Instrument::Bass),
            "/Users/x/bass-rig/models/Amp.nam"
        );
    }

    #[test]
    fn a_slot_is_made_once_its_files_are_in_and_taken_once() {
        let key = "/rigs/bass/models/slot-test.nam".to_owned();
        let slot = wait(vec![key.clone()], || 7_u32);
        assert!(wanted().contains(&key));
        assert_eq!(arrived(), 0, "nothing is made before its files");
        assert_eq!(slot.take(), None);
        install(key.clone(), vec![1_u8, 2, 3]);
        assert!(!wanted().contains(&key));
        assert_eq!(arrived(), 1);
        assert_eq!(status().waiting, 0);
        assert_eq!(slot.take(), Some(7));
        slot.retire(3);
        assert!(slot.done());
        assert_eq!(arrived(), 0, "a taken slot is not made again");
    }

    #[test]
    fn a_rig_built_again_stops_wanting_what_it_waited_for() {
        let key = "/rigs/bass/models/replaced.nam".to_owned();
        let slot = wait(vec![key.clone()], || 1_u8);
        assert!(wanted().contains(&key));
        drop(slot);
        assert!(!wanted().contains(&key));
    }
}
