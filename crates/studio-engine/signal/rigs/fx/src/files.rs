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
//! The home (its `.styx` files) is small text and is installed before a
//! rig reads it ([`install_text`]).

use std::collections::BTreeSet;
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

/// Keep `bytes` as `key`'s: what a rig reads before it would touch a disk.
pub fn install(key: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
    sampler::assets::install(key.into(), bytes);
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
        .flat_map(|p| p.keys().iter())
        .filter(|k| !sampler::assets::contains(k))
        .cloned()
        .collect();
    keys.into_iter().collect()
}

/// A rig's chain, made again once its files are in.
trait Pending: Send + Sync {
    /// The files it waits for.
    fn keys(&self) -> &[String];
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
    keys: Vec<String>,
    make: Box<dyn Fn() -> T + Send + Sync>,
    /// The new chain, then (once taken) the old one, to drop here.
    held: Mutex<Option<T>>,
    stage: AtomicU8,
}

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
    fn keys(&self) -> &[String] {
        &self.keys
    }

    fn waiting(&self) -> bool {
        self.stage.load(Ordering::Acquire) == WAITING
    }

    fn build_if_ready(&self) -> bool {
        if self.stage.load(Ordering::Acquire) != WAITING
            || !self.keys.iter().all(|k| sampler::assets::contains(k))
        {
            return false;
        }
        let built = (self.make)();
        *lock(&self.held) = Some(built);
        self.stage.store(BUILT, Ordering::Release);
        true
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
    let slot = Arc::new(Slot {
        keys,
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

/// A rig playing streamed files says how loud it was ([`heard`]).
pub(crate) fn note_heard(peak: f32) {
    let bits = peak.abs().to_bits();
    HEARD.fetch_max(bits, Ordering::Relaxed);
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
}

/// The rigs' [`Status`].
#[must_use]
pub fn status() -> Status {
    Status {
        wanted: wanted().len(),
        waiting: live().iter().filter(|p| p.waiting()).count(),
        home_files: signal_rig_host::store::memory::len(),
        heard: f32::from_bits(HEARD.load(Ordering::Relaxed)),
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
