//! What a CLAP subscribes to over host-actions/1, and the events it gets.
//!
//! | topic | sent | event |
//! |---|---|---|
//! | `transport` | on subscribing; when it starts, stops, pauses, records, loops or seeks | `{"state": "playing"\|"stopped"\|"paused"\|"recording", "playing", "recording", "looping", "position": <s>, "bpm", "numerator", "denominator"}` |
//! | `tempo` | on subscribing; when the tempo map changes | `{"bpm": <at 0>, "points": [{"time": <s>, "bpm", "numerator"?, "denominator"?}]}` |
//! | `hits:<piece track path>` | on subscribing; after each edit step that changed them | `{"piece": "Kick", "track": "Drums/Kick", "note": 36, "count", "hits": [{"time": <s, where it plays now>, "velocity": 1..127}]}` |
//! | `job` | when an action that answered `{"job": <id>}` finishes | `{"job", "action", "ok", "result"}` or `{"job", "action", "ok": false, "error"}` |
//! | `timeline` | when the plugin's own track's timeline (`host-timeline/1`) changes | `{"revision": N}`: read it again (`clap_host::timeline`) |
//!
//! A piece is named by its track path, any tail of it, its folder's name
//! or the piece's name (`hits:Drums/Kick`, `hits:Kick/Sum/Trig`,
//! `hits:Kick`); a name that is no piece of the song's kit is an event
//! with `"error"` and no hits. Events are delivered on the engine's main
//! loop for plugins (`clap_host::pump`), under the plugin's lock.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, PoisonError};

use daw::service::{ProjectContext, TempoMap as _};
use daw::standalone::Standalone;
use daw::standalone::clap_ext::host::EventSink;
use daw_proto::transport::service::Transport as TransportService;
use serde_json::{Value, json};

/// A topic a plugin can subscribe to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    Transport,
    Tempo,
    /// A piece of the kit, by its track path.
    Hits(String),
    Job,
    /// A user's edit of one of the plugin's own lanes (host-automation/1),
    /// queued for that plugin alone.
    Automation,
    /// The subscriber's own track's timeline (`host-timeline/1`); sent by
    /// `clap_host::timeline`, per plugin.
    Timeline,
}

impl Topic {
    /// `topic` as host-actions/1 names it; `None` for one Studio has not.
    #[must_use]
    pub fn parse(topic: &str) -> Option<Self> {
        match topic.trim() {
            "transport" => Some(Self::Transport),
            "tempo" => Some(Self::Tempo),
            "job" => Some(Self::Job),
            "automation" => Some(Self::Automation),
            "timeline" => Some(Self::Timeline),
            other => other
                .strip_prefix("hits:")
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| Self::Hits(p.to_owned())),
        }
    }

    /// Its name, as the plugin subscribed to it.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Transport => "transport".into(),
            Self::Tempo => "tempo".into(),
            Self::Job => "job".into(),
            Self::Automation => "automation".into(),
            Self::Timeline => "timeline".into(),
            Self::Hits(path) => format!("hits:{path}"),
        }
    }
}

/// One subscription.
struct Sub {
    topic: Topic,
    sink: EventSink,
    /// What it was last sent, by fingerprint: `None` before its first.
    sent: Option<u64>,
    /// A `hits:` subscription's last event, to hand a render's copy of its
    /// plugin ([`replay_hits`]).
    last: Option<String>,
    /// Events waiting to go (a job's).
    queued: Vec<String>,
    /// When it was made, and whether its plugin has been seen on a chain
    /// since ([`retain_live`]).
    made: web_time::Instant,
    seen: bool,
}

/// How long a subscription of a plugin not yet seen on a chain is kept: a
/// plugin subscribes while it loads (FX_RELOAD restores its state before
/// it goes on the chain), and a render's copy, never on one, subscribes
/// too.
const UNSEEN_KEPT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Default)]
struct Subs {
    next: u64,
    by_id: HashMap<u64, Sub>,
}

static SUBS: Mutex<Option<Subs>> = Mutex::new(None);

fn with_subs<R>(f: impl FnOnce(&mut Subs) -> R) -> R {
    let mut subs = SUBS.lock().unwrap_or_else(PoisonError::into_inner);
    f(subs.get_or_insert_with(Subs::default))
}

/// Subscribe `sink` to `topic`: its id (never 0).
pub fn subscribe(topic: Topic, sink: EventSink) -> u64 {
    with_subs(|s| {
        s.next = s.next.saturating_add(1);
        let id = s.next;
        s.by_id.insert(
            id,
            Sub {
                topic,
                sink,
                sent: None,
                last: None,
                queued: Vec::new(),
                made: web_time::Instant::now(),
                seen: false,
            },
        );
        id
    })
}

pub fn unsubscribe(id: u64) {
    with_subs(|s| s.by_id.remove(&id));
}

/// How many subscriptions there are.
#[must_use]
pub fn count() -> usize {
    with_subs(|s| s.by_id.len())
}

/// Drop every subscription whose plugin is not among `live` but was: a
/// plugin taken off its chain subscribes no longer. One whose plugin was
/// never among them — still loading, not on its chain yet — is kept for
/// [`UNSEEN_KEPT`]: dropping it would leave a reloaded plugin (one that
/// subscribed while its state was restored) without its events for good.
pub fn retain_live(live: &[EventSink]) {
    with_subs(|s| {
        s.by_id.retain(|_, sub| {
            if live.iter().any(|l| l.same_plugin(&sub.sink)) {
                sub.seen = true;
                true
            } else {
                !sub.seen && sub.made.elapsed() < UNSEEN_KEPT
            }
        });
    });
}

/// `sink`'s plugin's subscriptions to `topic`.
#[must_use]
pub fn subscriptions_of(sink: &EventSink, topic: &Topic) -> Vec<u64> {
    with_subs(|s| {
        s.by_id
            .iter()
            .filter(|(_, sub)| sub.topic == *topic && sub.sink.same_plugin(sink))
            .map(|(id, _)| *id)
            .collect()
    })
}

/// Send `copy` (a render's copy of `original`'s plugin, subscribed to
/// nothing and never reached by the pump) the last event of each of
/// `original`'s `hits:` subscriptions, as subscription 0: the copy then
/// knows the song's placed hits as the live instance does — without them
/// a Signal gate keyed by another track's trigger falls back to its own
/// input after 10 s without a hit, and a render of a long span plays the
/// bleed the live instance shuts. How many events it sent. Call it from
/// the thread that owns the copy, before it renders.
pub fn replay_hits(original: &EventSink, copy: &EventSink) -> usize {
    let events: Vec<(String, String)> = with_subs(|s| {
        s.by_id
            .values()
            .filter(|sub| matches!(sub.topic, Topic::Hits(_)) && sub.sink.same_plugin(original))
            .filter_map(|sub| Some((sub.topic.name(), sub.last.clone()?)))
            .collect()
    });
    for (topic, event) in &events {
        copy.send(0, topic, event);
    }
    events.len()
}

/// Subscription `id` was sent `event` (as [`Outbox::gather`] records it).
#[cfg(test)]
pub(super) fn record_sent(id: u64, event: &str) {
    with_subs(|s| {
        if let Some(sub) = s.by_id.get_mut(&id) {
            sub.sent = Some(fingerprint(event));
            if matches!(sub.topic, Topic::Hits(_)) {
                sub.last = Some(event.to_owned());
            }
        }
    });
}

/// A job finished: its event, to every `job` subscriber.
pub fn job_finished(event: String) {
    with_subs(|s| {
        for sub in s.by_id.values_mut().filter(|s| s.topic == Topic::Job) {
            sub.queued.push(event.clone());
        }
    });
}

/// A user edited one of `plugin`'s lanes: `event` to its `automation`
/// subscriptions.
pub fn automation_edited(plugin: daw::standalone::clap_ext::automation::Caller, event: &str) {
    with_subs(|s| {
        for sub in s
            .by_id
            .values_mut()
            .filter(|s| s.topic == Topic::Automation && s.sink.plugin_key() == plugin)
        {
            sub.queued.push(event.to_owned());
        }
    });
}

/// The events of one turn of the pump, each for its plugin.
#[derive(Default)]
pub struct Outbox {
    events: Vec<(u64, EventSink, String, String)>,
}

impl Outbox {
    /// Every event due: each subscription's topic read from the song open
    /// now, sent where it changed since it was last sent (or was never
    /// sent). The hits are read again only after an edit step (`step`) or
    /// for a new subscription.
    #[must_use]
    pub fn gather(daw: &Standalone, step: bool) -> Self {
        let wanted: Vec<(u64, Topic, Option<u64>)> = with_subs(|s| {
            s.by_id
                .iter()
                .filter(|(_, sub)| {
                    !matches!(sub.topic, Topic::Job | Topic::Automation | Topic::Timeline)
                })
                .map(|(id, sub)| (*id, sub.topic.clone(), sub.sent))
                .collect()
        });
        let mut read: HashMap<Topic, Option<(u64, String)>> = HashMap::new();
        let song = crate::actions::song().ok();
        for (_, topic, sent) in &wanted {
            let due = match topic {
                Topic::Hits(_) => step || sent.is_none(),
                _ => true,
            };
            if !due || read.contains_key(topic) {
                continue;
            }
            let event = song
                .as_ref()
                .filter(|(engine, _)| std::ptr::eq(*engine, daw))
                .map(|(_, project)| event_of(daw, project, topic));
            read.insert(topic.clone(), event.map(|e| (fingerprint(&e), e)));
        }
        let mut out = Self::default();
        with_subs(|s| {
            for (id, sub) in &mut s.by_id {
                let name = sub.topic.name();
                for queued in sub.queued.drain(..) {
                    out.events.push((*id, sub.sink, name.clone(), queued));
                }
                let Some(Some((print, event))) = read.get(&sub.topic) else {
                    continue;
                };
                if sub.sent != Some(*print) {
                    sub.sent = Some(*print);
                    if matches!(sub.topic, Topic::Hits(_)) {
                        sub.last = Some(event.clone());
                    }
                    out.events.push((*id, sub.sink, name, event.clone()));
                }
            }
        });
        out
    }

    /// Send `sink`'s plugin its events. On the main thread for plugins,
    /// under its lock.
    pub fn deliver(&self, sink: &EventSink) {
        for (id, to, topic, event) in &self.events {
            if to.same_plugin(sink) {
                to.send(*id, topic, event);
            }
        }
    }

    /// Whether any of its events goes to `sink`'s plugin.
    #[must_use]
    pub fn has_for(&self, sink: &EventSink) -> bool {
        self.events.iter().any(|(_, to, _, _)| to.same_plugin(sink))
    }

    /// How many events it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

fn fingerprint(event: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    event.hash(&mut h);
    h.finish()
}

/// `topic`'s event, read from `project` now.
fn event_of(daw: &Standalone, project: &ProjectContext, topic: &Topic) -> String {
    match topic {
        Topic::Transport => transport(daw, project),
        Topic::Tempo => tempo(daw, project),
        Topic::Hits(path) => hits(daw, project, path),
        Topic::Job | Topic::Automation | Topic::Timeline => Value::Null,
    }
    .to_string()
}

/// Where the playhead was last seen, to tell a seek from playing on.
static LAST_PLAYHEAD: Mutex<Option<(f64, web_time::Instant, u64)>> = Mutex::new(None);

/// How far the playhead may stray from where playing on puts it before it
/// counts as a seek.
const SEEK_SLACK_S: f64 = 0.25;

/// The transport as its event says it: its position is in the event only
/// as of its last change (play, stop, seek) — a plugin keeps time itself.
fn transport(daw: &Standalone, project: &ProjectContext) -> Value {
    use daw_proto::PlayState;
    let t = TransportService::get_state(daw, project.clone());
    let position = t.playhead_position.seconds().unwrap_or(0.0);
    let playing = matches!(t.play_state, PlayState::Playing | PlayState::Recording);
    let seeks = {
        let mut last = LAST_PLAYHEAD.lock().unwrap_or_else(PoisonError::into_inner);
        let now = web_time::Instant::now();
        let (was, at, seeks) = last.unwrap_or((position, now, 0));
        let expected = if playing {
            was + now.duration_since(at).as_secs_f64() * t.playrate.max(0.0)
        } else {
            was
        };
        let seeks = if (position - expected).abs() > SEEK_SLACK_S {
            seeks.wrapping_add(1)
        } else {
            seeks
        };
        *last = Some((position, now, seeks));
        seeks
    };
    let state = match t.play_state {
        PlayState::Stopped => "stopped",
        PlayState::Playing => "playing",
        PlayState::Paused => "paused",
        PlayState::Recording => "recording",
    };
    // The position is quoted as of the last change: it is in the
    // fingerprint through `seeks` and the state, not its every move.
    let anchor = json!({
        "state": state,
        "looping": t.looping,
        "bpm": t.tempo.bpm,
        "numerator": t.time_signature.numerator,
        "denominator": t.time_signature.denominator,
        "seeks": seeks,
    });
    let mut out = anchor;
    out["playing"] = Value::Bool(playing);
    out["recording"] = Value::Bool(matches!(t.play_state, PlayState::Recording));
    out["position"] = json!(anchored_position(state, seeks, position));
    out
}

/// The position an event quotes: the playhead when the state or the seek
/// count last changed, so playing on sends nothing new.
fn anchored_position(state: &str, seeks: u64, position: f64) -> f64 {
    static ANCHOR: Mutex<Option<(String, u64, f64)>> = Mutex::new(None);
    let mut anchor = ANCHOR.lock().unwrap_or_else(PoisonError::into_inner);
    match anchor.as_ref() {
        Some((s, k, at)) if s == state && *k == seeks => *at,
        _ => {
            *anchor = Some((state.to_owned(), seeks, position));
            position
        }
    }
}

/// The tempo map as its event says it.
fn tempo(daw: &Standalone, project: &ProjectContext) -> Value {
    let points: Vec<Value> = daw
        .get_tempo_points(project.clone())
        .iter()
        .map(|p| {
            let mut point = json!({
                "time": p.position.seconds().unwrap_or(0.0),
                "bpm": p.bpm,
            });
            if let Some(sig) = &p.time_signature {
                point["numerator"] = json!(sig.numerator);
                point["denominator"] = json!(sig.denominator);
            }
            point
        })
        .collect();
    json!({ "bpm": daw.get_tempo_at(project.clone(), 0.0), "points": points })
}

/// A piece's hits as its event says them.
fn hits(daw: &Standalone, project: &ProjectContext, path: &str) -> Value {
    let Some(piece) = piece_of(daw, project, path) else {
        return json!({ "track": path, "error": format!("{path} is no piece of the song's kit"), "hits": [] });
    };
    let played = crate::hits::playback(daw, project);
    let notes: Vec<Value> = crate::hits_midi::hit_notes(&played, &|t| t)
        .into_iter()
        .filter(|n| n.piece == piece)
        .map(|n| json!({ "time": n.time, "velocity": n.velocity }))
        .collect();
    json!({
        "piece": piece,
        "track": path,
        "note": crate::hits_midi::note_for(&piece),
        "count": notes.len(),
        "hits": notes,
    })
}

/// The kit's piece `path` names: a track of the piece (its trigger, a
/// close mic), its folder, or its name.
fn piece_of(daw: &Standalone, project: &ProjectContext, path: &str) -> Option<String> {
    let (pieces, _) = crate::hits::kit(daw, project);
    let named = |name: &str| {
        pieces
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
            .map(|p| p.name.clone())
    };
    if let Ok(guid) = crate::actions::resolve(daw, project, path) {
        if let Some(p) = pieces.iter().find(|p| p.tracks().any(|t| *t == guid)) {
            return Some(p.name.clone());
        }
        let track =
            daw::service::Tracks::get(daw, project.clone(), daw::service::TrackRef::Guid(guid));
        if let Some(found) = track.and_then(|t| named(&t.name)) {
            return Some(found);
        }
    }
    path.rsplit('/').find_map(named)
}
