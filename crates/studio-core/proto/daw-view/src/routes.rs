//! What a track sends to, and what it receives from.
//!
//! The strip has always known WHETHER a track has sends — `send_count`
//! rides on the track and lights a lane in the corner. That is all a
//! mixer needs to draw itself, and it is deliberately all the track
//! stream carries: a bulk read of two hundred tracks must not also be a
//! bulk read of their routing.
//!
//! This is the other half, read only for the tracks something is
//! actually looking at. A route is a destination, a level, a pan, a
//! mute and a mode, and every one of those is a thing you change while
//! listening — so they arrive the way notes do: the window draws what
//! it has, asks for the rest, and redraws as the answers land.
//!
//! # Why a cache and not a fetch per frame
//!
//! Reading one track's sends is a round trip, and the panel is open
//! while you drag a send's level. Asking per frame would put the
//! network in the middle of a gesture. So the window holds the routes,
//! predicts its own edits into them, and lets the routing bus correct
//! it — the same shape the track list already uses, for the same
//! reason.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use daw_proto::routing::{RouteType, RoutingEvent, TrackRoute};

/// Where a route is: which of a track's three lists, and where in it.
///
/// The number is REAPER's, and it is only unique WITHIN its list — a
/// track can have send 0, receive 0 and hardware output 0 at once, and
/// three different things they are. Carrying the kind beside the number
/// is what stops an edit landing on the wrong one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, facet::Facet)]
pub struct At {
    pub kind: RouteType,
    pub index: u32,
}

impl At {
    #[must_use]
    pub const fn new(kind: RouteType, index: u32) -> Self {
        Self { kind, index }
    }
}

/// One track's routing, as the panel draws it.
///
/// No `PartialEq`: `TrackRoute` has none, and comparing two wirings is
/// not a question anything here asks — the cache is replaced by a read,
/// never diffed against one.
#[derive(Clone, Default, Debug)]
pub struct Wiring {
    /// Where this track sends, in REAPER's own order.
    pub sends: Vec<TrackRoute>,
    /// What sends to this track.
    ///
    /// Editable here, and it is the same route the far track's send
    /// is: REAPER keeps one object and lists it at both ends, so
    /// turning a receive down turns that send down. Worth knowing, and
    /// worth having — "this is too loud coming in" is answered where
    /// you noticed it rather than by going to find the track it came
    /// from.
    pub receives: Vec<TrackRoute>,
    /// Where the track leaves the box: an output pair on the interface.
    ///
    /// A cue mix is one of these on a headphone bus, so a panel that
    /// showed sends and not these could describe half a cue system.
    pub hardware: Vec<TrackRoute>,
}

impl Wiring {
    /// The list a kind of route lives in.
    #[must_use]
    pub fn list(&self, kind: RouteType) -> &[TrackRoute] {
        match kind {
            RouteType::Send => &self.sends,
            RouteType::Receive => &self.receives,
            RouteType::HardwareOutput => &self.hardware,
        }
    }

    /// One route, by where it is.
    #[must_use]
    pub fn at(&self, at: At) -> Option<&TrackRoute> {
        self.list(at.kind).iter().find(|r| r.index == at.index)
    }
}

/// Who is at the far end of a route, from the point of view of one
/// track.
///
/// Needed because the two backends disagree about which field holds
/// which end of a RECEIVE. REAPER stores the partner — the track that
/// is feeding you — in `dest_track_guid`, the same field a send uses
/// for its destination. daw-standalone mirrors a send into a receive
/// and leaves both ends as they were, so the "destination" of a receive
/// is the track you are already looking at.
///
/// Asking "which of these two is not me" is right on both, and stays
/// right whichever way that is settled — see daw#30.
#[must_use]
pub fn partner<'a>(route: &'a TrackRoute, mine: &str) -> Option<&'a str> {
    for end in [
        route.dest_track_guid.as_deref(),
        Some(route.source_track_guid.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        if end != mine {
            return Some(end);
        }
    }
    None
}

/// What a routing event did to the cache.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Applied {
    /// A level, a pan, a mute. Redraw.
    Field,
    /// A route appeared or went away. The list itself is different, and
    /// so is the FAR track's — a send is a receive at the other end —
    /// so both have to be read again.
    Structure,
    /// Nothing this window is holding.
    Nothing,
}

/// The routes of the tracks that have been read so far.
///
/// Absent means "not asked yet"; a `Wiring` with two empty lists means
/// "read, and it has none". A track still loading must not draw as a
/// track with no sends — the panel would say something false and then
/// quietly correct itself, which is worse than saying nothing yet.
#[derive(Clone, Default)]
pub struct Routes {
    known: Arc<Mutex<HashMap<String, Wiring>>>,
    /// Set when something has landed, so the window redraws rather than
    /// comparing a map every frame.
    fresh: Arc<std::sync::atomic::AtomicBool>,
    /// The song's tracks, once read ([`Routes::tracks`]).
    tracks: Arc<Mutex<Option<Vec<RouteTrack>>>>,
    /// When the window last predicted into each track's routing: a read
    /// begun before that lands without overwriting it.
    touched: Arc<Mutex<HashMap<String, web_time::Instant>>>,
    /// A [`Routes::reread`] is out: another waits for it rather than
    /// piling up behind a slow engine.
    reading: Arc<std::sync::atomic::AtomicBool>,
}

/// A track as a routing panel needs it: where a send can go, and what is
/// inside what (a folder's tracks feed it, so a send from the folder to
/// one of them feeds back).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteTrack {
    pub guid: String,
    pub name: String,
    pub parent: Option<String>,
}

impl Routes {
    /// One track's routing, if it has been read.
    #[must_use]
    pub fn get(&self, guid: &str) -> Option<Wiring> {
        self.known.lock().ok()?.get(guid).cloned()
    }

    /// Has anything arrived since this was last asked? Asking clears it.
    pub fn take_fresh(&self) -> bool {
        self.fresh.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Forget a track's routing, so the next fetch reads it again.
    ///
    /// Forgetting rather than patching, for the structural changes: a
    /// send that appeared changed the numbering of nothing, but a send
    /// that went away renumbered every one after it at BOTH ends, and
    /// guessing at that is how a panel ends up editing the wrong send.
    pub fn forget(&self, guid: &str) {
        if let Ok(mut known) = self.known.lock() {
            known.remove(guid);
        }
    }

    /// Put one track's routing in, as read.
    pub fn put(&self, guid: &str, wiring: Wiring) {
        if let Ok(mut known) = self.known.lock() {
            known.insert(guid.to_owned(), wiring);
        }
        self.fresh.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Change one of a track's sends, before the engine confirms it.
    ///
    /// The prediction half of a drag: a fader that waits for a round
    /// trip is a fader that lags, and the correction is coming either
    /// way. `None` if that send is not held — nothing to predict into,
    /// and the read that lands will be right.
    pub fn predict(&self, guid: &str, at: At, change: impl FnOnce(&mut TrackRoute)) {
        let Ok(mut known) = self.known.lock() else {
            return;
        };
        if let Some(route) = known.get_mut(guid).and_then(|wiring| {
            let list = match at.kind {
                RouteType::Send => &mut wiring.sends,
                RouteType::Receive => &mut wiring.receives,
                RouteType::HardwareOutput => &mut wiring.hardware,
            };
            list.iter_mut().find(|r| r.index == at.index)
        }) {
            change(route);
            self.fresh.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        drop(known);
        self.touch(guid);
    }

    /// Take one of a track's routes out before the engine confirms it,
    /// numbering the ones after it down as the engine will: the panel
    /// keeps showing the rest while the read that settles it is out.
    pub fn predict_removed(&self, guid: &str, at: At) {
        let Ok(mut known) = self.known.lock() else {
            return;
        };
        if let Some(wiring) = known.get_mut(guid) {
            let list = match at.kind {
                RouteType::Send => &mut wiring.sends,
                RouteType::Receive => &mut wiring.receives,
                RouteType::HardwareOutput => &mut wiring.hardware,
            };
            list.retain(|r| r.index != at.index);
            for later in list.iter_mut().filter(|r| r.index > at.index) {
                later.index -= 1;
            }
            self.fresh.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        drop(known);
        self.touch(guid);
    }

    /// The window changed `guid`'s routing now.
    fn touch(&self, guid: &str) {
        if let Ok(mut touched) = self.touched.lock() {
            touched.insert(guid.to_owned(), web_time::Instant::now());
        }
    }

    /// Apply one routing event from the bus.
    ///
    /// Field changes are written in place. Structural ones are reported
    /// rather than applied, because the caller is the one that can go
    /// and read both ends again.
    pub fn apply(&self, event: &RoutingEvent) -> Applied {
        let applied = {
            let Ok(mut known) = self.known.lock() else {
                return Applied::Nothing;
            };
            apply_to(&mut known, event)
        };
        if applied != Applied::Nothing {
            self.fresh.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        applied
    }

    /// Read the routing of every track in the list that is not held yet.
    ///
    /// Spawns and returns; the window keeps drawing.
    pub fn fetch(&self, wanted: Vec<String>) {
        let missing: Vec<String> = {
            let Ok(known) = self.known.lock() else { return };
            wanted
                .into_iter()
                .filter(|guid| !known.contains_key(guid))
                .collect()
        };
        self.read_in(missing, false);
    }

    /// Read these tracks' routing again, held or not, keeping what is held
    /// until the new read lands: what a panel open on a track does to
    /// follow changes made elsewhere, without blanking while it reads.
    /// One at a time: called again while one is out, it does nothing.
    pub fn reread(&self, guids: Vec<String>) {
        if self.reading.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return;
        }
        self.read_in(guids, true);
    }

    /// The song's tracks, in order, once read ([`Self::fetch_tracks`]):
    /// where a send can go.
    #[must_use]
    pub fn tracks(&self) -> Option<Vec<RouteTrack>> {
        self.tracks.lock().ok()?.clone()
    }

    /// Read the song's tracks ([`Self::tracks`]). Spawns and returns.
    pub fn fetch_tracks(&self) {
        let tracks = Arc::clone(&self.tracks);
        let fresh = Arc::clone(&self.fresh);
        spawn_on_engine("studio-engine-route-tracks", async move {
            let Some(daw) = daw::rpc::Daw::try_get() else {
                return;
            };
            let Ok(project) = daw.current_project().await else {
                return;
            };
            let Ok(all) = project.tracks().all().await else {
                return;
            };
            let named = all
                .into_iter()
                .map(|t| RouteTrack {
                    guid: t.guid,
                    name: t.name,
                    parent: t.parent_guid,
                })
                .collect();
            if let Ok(mut held) = tracks.lock() {
                *held = Some(named);
            }
            fresh.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }

    /// Read `guids`' routing into the cache, off the caller.
    /// `rereading`: this is [`Self::reread`]'s, whose flag it lets go.
    fn read_in(&self, guids: Vec<String>, rereading: bool) {
        let reading = rereading.then(|| Arc::clone(&self.reading));
        if guids.is_empty() {
            if let Some(reading) = reading {
                reading.store(false, std::sync::atomic::Ordering::Release);
            }
            return;
        }
        let known = Arc::clone(&self.known);
        let fresh = Arc::clone(&self.fresh);
        let touched = Arc::clone(&self.touched);
        spawn_on_engine("studio-engine-routes", async move {
            // However it ends, the next reread may go.
            struct Done(Option<Arc<std::sync::atomic::AtomicBool>>);
            impl Drop for Done {
                fn drop(&mut self) {
                    if let Some(reading) = &self.0 {
                        reading.store(false, std::sync::atomic::Ordering::Release);
                    }
                }
            }
            let _done = Done(reading);
            let began = web_time::Instant::now();
            let Some(daw) = daw::rpc::Daw::try_get() else {
                return;
            };
            let Ok(project) = daw.current_project().await else {
                return;
            };
            for guid in guids {
                let wiring = read(&project, &guid).await;
                // Predicted into since this read began: the prediction is
                // newer than what the engine said, and the next read
                // settles it.
                let stale = touched
                    .lock()
                    .ok()
                    .and_then(|t| t.get(&guid).copied())
                    .is_some_and(|at| at > began);
                if stale {
                    continue;
                }
                let Ok(mut known) = known.lock() else { return };
                known.insert(guid, wiring);
                fresh.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }
}

/// Run `work` where the facade can be awaited: a thread of its own on the
/// engine's runtime in a native window. Nothing runs with no runtime.
#[cfg(feature = "native")]
fn spawn_on_engine(name: &str, work: impl std::future::Future<Output = ()> + Send + 'static) {
    let Some(runtime) = crate::open::runtime() else {
        return;
    };
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || runtime.block_on(work))
        .ok();
}

/// A page's: on its event loop, where the engine in the page answers.
#[cfg(not(feature = "native"))]
fn spawn_on_engine(_name: &str, work: impl std::future::Future<Output = ()> + 'static) {
    #[cfg(feature = "web")]
    wasm_bindgen_futures::spawn_local(work);
    #[cfg(not(feature = "web"))]
    drop(work);
}

/// The cache half of [`Routes::apply`], with no lock and no thread.
///
/// Separated so what an event DOES is testable without a project: this
/// is the part that can be wrong, and the part that is wrong the day a
/// send is removed from under a numbering somebody was holding.
fn apply_to(known: &mut HashMap<String, Wiring>, event: &RoutingEvent) -> Applied {
    match event {
        // Both ends change, and neither is patchable from here: the
        // route arrives with a source but a receive is numbered at the
        // destination, which this event does not carry.
        RoutingEvent::RouteCreated {
            source_track_guid, ..
        }
        | RoutingEvent::RouteDeleted {
            source_track_guid, ..
        } => {
            // Structure either way, held or not: the far end of this
            // route is a track this event does not name, and only a
            // read can say which numbering changed.
            known.remove(source_track_guid);
            Applied::Structure
        }
        RoutingEvent::VolumeChanged {
            source_track_guid,
            route_type,
            route_index,
            volume,
            ..
        } => field(known, source_track_guid, *route_type, *route_index, |r| {
            r.volume = *volume;
        }),
        RoutingEvent::PanChanged {
            source_track_guid,
            route_type,
            route_index,
            pan,
            ..
        } => field(known, source_track_guid, *route_type, *route_index, |r| {
            r.pan = *pan;
        }),
        RoutingEvent::MuteChanged {
            source_track_guid,
            route_type,
            route_index,
            muted,
            ..
        } => field(known, source_track_guid, *route_type, *route_index, |r| {
            r.muted = *muted;
        }),
        // The track's own, not a route's — it rides on the track list
        // and the strip already draws it.
        RoutingEvent::ParentSendChanged { .. } => Applied::Nothing,
    }
}

/// Write one field of one route, if the window is holding it.
fn field(
    known: &mut HashMap<String, Wiring>,
    guid: &str,
    route_type: RouteType,
    index: u32,
    change: impl FnOnce(&mut TrackRoute),
) -> Applied {
    let Some(wiring) = known.get_mut(guid) else {
        return Applied::Nothing;
    };
    let list = match route_type {
        RouteType::Send => &mut wiring.sends,
        RouteType::Receive => &mut wiring.receives,
        RouteType::HardwareOutput => &mut wiring.hardware,
    };
    match list.iter_mut().find(|r| r.index == index) {
        Some(route) => {
            change(route);
            Applied::Field
        }
        None => Applied::Nothing,
    }
}

/// Read one track's sends and receives.
///
/// An empty answer is a real answer: a track with no sends is the
/// common case, and holding that is what stops it being asked again.
async fn read(project: &daw_control::Project, guid: &str) -> Wiring {
    let Ok(Some(track)) = project.tracks().by_guid(guid).await else {
        return Wiring::default();
    };
    Wiring {
        sends: track.sends().all().await.unwrap_or_default(),
        receives: track.receives().all().await.unwrap_or_default(),
        hardware: track.hardware_outputs().all().await.unwrap_or_default(),
    }
}
