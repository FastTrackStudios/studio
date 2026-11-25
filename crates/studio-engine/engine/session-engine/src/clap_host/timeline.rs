//! Studio's half of `com.fasttrackstudio.host-timeline/1` (daw-clap-ext
//! `docs/host-timeline.md`): each CLAP the engine loads reads its own
//! track's timeline — the track's MIDI items merged as the song plays
//! them (`Standalone::track_timeline`) and the tempo map — and may claim
//! the track's playback, so a sampler plays the notes ahead of the
//! playhead instead of being handed them on time.
//!
//! - **Binding.** The [`pump`](super::pump) binds every instance it finds
//!   unbound to its [`TrackTimeline`], by FX guid: the instance's track is
//!   found from the song open now each time the project moves, so an FX
//!   moved to another track reads the other track.
//! - **Revision.** Moves when what `read` returns changes: the pump
//!   rebuilds a timeline a plugin wants (it read it, or subscribed to
//!   `timeline`) only when the project's revision moved, and moves the
//!   timeline's revision only if the rebuilt one differs. So an edit of
//!   another track, or of the mixer, never sends one.
//! - **Event.** A plugin subscribed to `timeline` gets `{"revision": N}`
//!   after each change, from the pump (at most [`super::TICK`] after the
//!   edit step that made it).
//! - **Claim.** `set_playback_claimed` marks the track's MIDI claimed in
//!   the engine (`Standalone::set_track_midi_claimed`): its items' notes
//!   are no longer delivered to the chain; live MIDI still is.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use daw::standalone::Standalone;
use daw::standalone::clap_ext::host::{EventSink, HostTimeline};
use daw::standalone::clap_ext::timeline::Timeline;

/// Where an instance is: its song and track guids.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Place {
    project: String,
    track: String,
}

#[derive(Default)]
struct State {
    /// The project revision the instance was last checked at.
    checked_at: Option<u64>,
    place: Option<Place>,
    timeline: Option<Timeline>,
    /// A plugin read it or subscribed: keep it built.
    wanted: bool,
    /// The track whose MIDI this instance claimed.
    claimed: Option<String>,
    /// The revision last sent as a `timeline` event.
    sent: u64,
}

/// One instance's timeline: what the vtable answers for it.
pub struct TrackTimeline {
    daw: &'static Standalone,
    fx: String,
    revision: AtomicU64,
    state: Mutex<State>,
}

impl TrackTimeline {
    fn new(daw: &'static Standalone, fx: &str) -> Self {
        Self {
            daw,
            fx: fx.to_owned(),
            revision: AtomicU64::new(0),
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Check the instance again if the project moved since it was last
    /// checked: where it is, and (if a plugin wants it) its timeline,
    /// rebuilt. Whether the revision moved.
    pub fn refresh(&self) -> bool {
        let mut state = self.state();
        let now = state
            .place
            .as_ref()
            .and_then(|p| self.daw.project_revision(&p.project));
        if now.is_some() && now == state.checked_at {
            return false;
        }
        let place = place_of(self.daw, &self.fx);
        let Some(place) = place else {
            let had = state.place.take().is_some();
            state.timeline = None;
            state.checked_at = None;
            if had {
                self.revision.store(0, Ordering::Release);
            }
            return had;
        };
        let moved = state.place.as_ref() != Some(&place);
        if moved && let Some(claimed) = state.claimed.clone() {
            // The claim follows the instance to its new track.
            self.daw.set_track_midi_claimed(&claimed, false);
            self.daw.set_track_midi_claimed(&place.track, true);
            state.claimed = Some(place.track.clone());
        }
        // Read after the claim moved it on: the revision it is now.
        state.checked_at = self.daw.project_revision(&place.project);
        let fresh = state
            .wanted
            .then(|| self.daw.track_timeline(&place.project, &place.track, 0))
            .flatten();
        state.place = Some(place);
        if !state.wanted {
            // Not read yet: say there is a timeline; build it when asked.
            return self
                .revision
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok();
        }
        if fresh != state.timeline || moved {
            state.timeline = fresh;
            self.revision.fetch_add(1, Ordering::AcqRel);
            return true;
        }
        false
    }

    /// Whether [`deliver`](Self::deliver) has anything to do for `sink`'s
    /// plugin: a timeline subscription not yet started, or a revision it
    /// was not sent. Read without the plugin.
    #[must_use]
    pub fn due(&self, sink: &EventSink) -> bool {
        if super::topics::subscriptions_of(sink, &super::topics::Topic::Timeline).is_empty() {
            return false;
        }
        let revision = self.revision.load(Ordering::Acquire);
        let state = self.state();
        !state.wanted || (state.sent != revision && revision != 0)
    }

    /// Send `sink`'s `timeline` subscriptions the revision, if it moved
    /// since it was last sent. Main thread for plugins.
    pub fn deliver(&self, sink: &EventSink) {
        let subs = super::topics::subscriptions_of(sink, &super::topics::Topic::Timeline);
        if subs.is_empty() {
            return;
        }
        let revision = self.revision.load(Ordering::Acquire);
        {
            let mut state = self.state();
            if !state.wanted {
                state.wanted = true;
                state.checked_at = None;
                drop(state);
                self.refresh();
            }
        }
        let revision = self.revision.load(Ordering::Acquire).max(revision);
        let mut state = self.state();
        if state.sent == revision || revision == 0 {
            return;
        }
        state.sent = revision;
        drop(state);
        let event = serde_json::json!({ "revision": revision }).to_string();
        for id in subs {
            sink.send(id, "timeline", &event);
        }
    }

    /// The track this instance is on, if it is.
    #[must_use]
    pub fn track(&self) -> Option<String> {
        self.state().place.as_ref().map(|p| p.track.clone())
    }

    /// Give back a claim (the instance went).
    fn release(&self) {
        if let Some(track) = self.state().claimed.take() {
            self.daw.set_track_midi_claimed(&track, false);
        }
    }
}

impl HostTimeline for TrackTimeline {
    fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn read(&self) -> Option<Timeline> {
        let first = {
            let mut state = self.state();
            let first = !state.wanted;
            state.wanted = true;
            first || state.timeline.is_none()
        };
        if first {
            // Built here the first time: the plugin reads on its main
            // thread, which in Studio is the pump's.
            self.state().checked_at = None;
            self.refresh();
        }
        let mut timeline = self.state().timeline.clone()?;
        timeline.revision = self.revision();
        Some(timeline)
    }

    fn set_playback_claimed(&self, claimed: bool) -> bool {
        let mut state = self.state();
        if !claimed {
            if let Some(track) = state.claimed.take() {
                self.daw.set_track_midi_claimed(&track, false);
            }
            return true;
        }
        let Some(track) = state.place.as_ref().map(|p| p.track.clone()) else {
            return false;
        };
        self.daw.set_track_midi_claimed(&track, true);
        state.claimed = Some(track.clone());
        tracing::info!(
            clap.fx = %self.fx,
            clap.track = %track,
            timeline.claimed = true,
            "clap: a plugin plays its track's timeline"
        );
        true
    }
}

/// The song and track FX `fx` is on, in any song open in `daw` (the one
/// on screen first): from [`Places`], built again only when a song's
/// revision moved or the songs open changed — once for every instance,
/// not once for each.
fn place_of(daw: &Standalone, fx: &str) -> Option<Place> {
    let songs = songs_of(daw);
    let mut places = PLACES.lock().unwrap_or_else(PoisonError::into_inner);
    let places = places.get_or_insert_with(Places::default);
    let engine = std::ptr::from_ref(daw).addr();
    if places.engine != engine || places.songs != songs {
        places.by_fx = places_in(daw, &songs);
        places.engine = engine;
        places.songs = songs;
        PLACE_BUILDS.fetch_add(1, Ordering::Relaxed);
    }
    places.by_fx.get(fx).cloned()
}

/// The songs open in `daw`, the one on screen first, each with its
/// revision as of now.
fn songs_of(daw: &Standalone) -> Vec<(String, Option<u64>)> {
    use daw::service::Projects;
    let current = crate::actions::song()
        .ok()
        .filter(|(engine, _)| std::ptr::eq(*engine, daw))
        .and_then(|(_, p)| crate::fx_actions::project_guid(&p).ok());
    let mut songs: Vec<String> = Projects::list(daw).into_iter().map(|p| p.guid).collect();
    if let Some(current) = current {
        songs.retain(|g| *g != current);
        songs.insert(0, current);
    }
    songs
        .into_iter()
        .map(|g| {
            let revision = daw.project_revision(&g);
            (g, revision)
        })
        .collect()
}

/// Every track FX of `songs`, where it is: the first song and track that
/// has it, in order. Read in place, nothing cloned but the guids.
fn places_in(daw: &Standalone, songs: &[(String, Option<u64>)]) -> HashMap<String, Place> {
    use daw::standalone::sync::FxChainKey;
    let mut by_fx = HashMap::new();
    for (project, _) in songs {
        daw.read_project(project, |p| {
            for t in &p.tracks {
                let chain = p.fx_chains.get(&FxChainKey::Track(t.guid.clone()));
                for e in chain.into_iter().flatten() {
                    by_fx.entry(e.fx.guid.clone()).or_insert_with(|| Place {
                        project: project.clone(),
                        track: t.guid.clone(),
                    });
                }
            }
        });
    }
    by_fx
}

/// Where every track FX of the open songs is, as of the songs' revisions.
#[derive(Default)]
struct Places {
    /// The engine it was read from (a process runs one; a test may run more).
    engine: usize,
    /// The songs, on screen first, at the revisions it was read at.
    songs: Vec<(String, Option<u64>)>,
    by_fx: HashMap<String, Place>,
}

static PLACES: Mutex<Option<Places>> = Mutex::new(None);

/// How many times [`Places`] was built: what the tests count.
static PLACE_BUILDS: AtomicU64 = AtomicU64::new(0);

static TIMELINES: Mutex<Option<HashMap<String, Arc<TrackTimeline>>>> = Mutex::new(None);

/// `fx`'s timeline, made the first time it is asked for.
#[must_use]
pub fn for_fx(daw: &'static Standalone, fx: &str) -> Arc<TrackTimeline> {
    let mut all = TIMELINES.lock().unwrap_or_else(PoisonError::into_inner);
    all.get_or_insert_with(HashMap::new)
        .entry(fx.to_owned())
        .or_insert_with(|| Arc::new(TrackTimeline::new(daw, fx)))
        .clone()
}

/// Forget the timelines of instances no longer loaded, giving back their
/// claims.
pub fn forget_gone(guids: &[String]) {
    let gone: Vec<Arc<TrackTimeline>> = {
        let mut all = TIMELINES.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(all) = all.as_mut() else { return };
        let gone_keys: Vec<String> = all
            .keys()
            .filter(|k| !guids.iter().any(|g| g == *k))
            .cloned()
            .collect();
        gone_keys.iter().filter_map(|k| all.remove(k)).collect()
    };
    for t in gone {
        t.release();
    }
}

#[cfg(test)]
mod tests {
    use daw::service::{Effects, FxChainContext, ProjectContext, Tracks};
    use daw::standalone::sync::{FxChainKey, FxEntry};

    use super::*;

    /// A song of `tracks` tracks, each with `per` FX.
    fn song(guid: &str, tracks: usize, per: usize) -> Standalone {
        let daw = Standalone::new();
        daw.seed_project(daw_proto::ProjectInfo {
            guid: guid.into(),
            name: guid.into(),
            path: String::new(),
        });
        daw.write_project(guid, |p| {
            for t in 0..tracks {
                let track = format!("{guid}-t{t}");
                p.tracks.push(daw_proto::Track {
                    guid: track.clone(),
                    name: format!("Track {t}"),
                    ..Default::default()
                });
                let chain = (0..per)
                    .map(|f| FxEntry {
                        fx: daw_proto::Fx {
                            guid: format!("{track}-fx{f}"),
                            name: "Signal".into(),
                            ..Default::default()
                        },
                        state_chunk: String::new(),
                        params: HashMap::new(),
                        ranges: Vec::new(),
                    })
                    .collect();
                p.fx_chains.insert(FxChainKey::Track(track), chain);
            }
        });
        daw
    }

    /// Where `fx` is, found as the pump found it before [`Places`]: every
    /// song's tracks cloned and searched for each instance.
    fn place_of_before(daw: &Standalone, fx: &str) -> Option<Place> {
        let songs = songs_of(daw);
        songs.into_iter().find_map(|(project, _)| {
            let ctx = ProjectContext::Project(project.clone());
            Tracks::all(daw, ctx.clone())
                .into_iter()
                .find(|t| {
                    Effects::list(daw, ctx.clone(), FxChainContext::Track(t.guid.clone()))
                        .iter()
                        .any(|f| f.guid == fx)
                })
                .map(|t| Place {
                    project: project.clone(),
                    track: t.guid,
                })
        })
    }

    #[test]
    fn every_instance_is_where_it_was_found_before_and_follows_a_move() {
        let daw = song("places-a", 6, 2);
        let fxs: Vec<String> = (0..6)
            .flat_map(|t| (0..2).map(move |f| format!("places-a-t{t}-fx{f}")))
            .chain(["nowhere".to_owned()])
            .collect();
        for fx in &fxs {
            assert_eq!(place_of(&daw, fx), place_of_before(&daw, fx), "{fx}");
        }
        // An FX moved to another track: found there once the song moved.
        daw.write_project("places-a", |p| {
            let fx = p
                .fx_chains
                .get_mut(&FxChainKey::Track("places-a-t0".into()))
                .unwrap()
                .remove(0);
            p.fx_chains
                .get_mut(&FxChainKey::Track("places-a-t5".into()))
                .unwrap()
                .push(fx);
        });
        let moved = place_of(&daw, "places-a-t0-fx0").expect("still on the song");
        assert_eq!(moved.track, "places-a-t5");
        assert_eq!(Some(moved), place_of_before(&daw, "places-a-t0-fx0"));
    }

    /// What finding every instance costs on a song of 120 tracks with 63
    /// Signal instances, after an edit (what a pump does each time the
    /// project's revision moved): before, every instance cloned every
    /// track; now one read for all. `--ignored --nocapture` to see it.
    #[test]
    #[ignore = "a measurement"]
    fn what_finding_every_instance_costs() {
        let daw = song("places-bench", 120, 1);
        let fxs: Vec<String> = (0..63).map(|t| format!("places-bench-t{t}-fx0")).collect();
        let edit = || daw.write_project("places-bench", |p| p.tracks[0].volume += 0.001);
        let n = 50u32;
        let start = std::time::Instant::now();
        for _ in 0..n {
            edit();
            for fx in &fxs {
                std::hint::black_box(place_of_before(&daw, fx));
            }
        }
        let before = start.elapsed() / n;
        let start = std::time::Instant::now();
        for _ in 0..n {
            edit();
            for fx in &fxs {
                std::hint::black_box(place_of(&daw, fx));
            }
        }
        let after = start.elapsed() / n;
        eprintln!("every instance found after an edit: {before:?} before, {after:?} now");
        assert!(after < before);
    }
}
