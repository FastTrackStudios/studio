//! Does the engine still hold what the doc says it does?
//!
//! The engine is built from the song's doc and kept in step with it by the
//! bridge. An edit the engine dropped, or one made to it behind the
//! bridge's back, leaves the two apart with nothing to say so — and every
//! measurement of that engine is then a measurement of a song nobody wrote.
//! This is the check (the idea of openDAW's checksum round trip between its
//! project graph and its engine; none of its code):
//!
//! - [`digest`]: one fingerprint per part of a model — each track (its FX
//!   chains and their states, sends, routing, automation), the items, the
//!   markers, regions, tempo, take markers and the project's own settings.
//!   Two models whose digests agree hold the same song; comparing digests
//!   is all a check costs when nothing drifted.
//! - [`drift`]: where two models differ, named — the track, the FX, the
//!   field — from the steps [`crate::sync::diff`] would take from the
//!   engine's to the doc's.
//! - [`report`]: a check's outcome on its span (`engine_check.*`), one
//!   `tracing::warn!` when they differ, and the observer told of each
//!   difference (the engine counts them as a metric: [`set_observer`]).
//!
//! The bridge runs it after every apply, on what it just read back
//! (`Bridge::make_match`), and on demand against a fresh read of the
//! engine (`Bridge::check`).

use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hasher as _;
use std::sync::OnceLock;

use crate::sync::diff::{Change, TrackField, diff};
use crate::sync::model::{FxState, SessionModel, TrackState};

/// One fingerprint per part of a model: `track:<guid>` for each track,
/// then `items`, `markers`, `regions`, `tempo`, `take_markers`, `project`.
/// The chart is not the engine's, so not in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Digest {
    parts: BTreeMap<String, u64>,
}

impl Digest {
    /// The parts whose fingerprints differ between `self` and `other`, or
    /// that only one has.
    #[must_use]
    pub fn differing<'a>(&'a self, other: &'a Self) -> BTreeSet<&'a str> {
        let keys: BTreeSet<&str> = self
            .parts
            .keys()
            .chain(other.parts.keys())
            .map(String::as_str)
            .collect();
        keys.into_iter()
            .filter(|k| self.parts.get(*k) != other.parts.get(*k))
            .collect()
    }

    /// How many parts it fingerprints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// Whether it fingerprints nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// A hasher fed by `Debug` formatting, so a model is fingerprinted without
/// a string of it ever being made. Fixed keys: the same model gives the
/// same digest in every process.
struct Fingerprint(std::hash::DefaultHasher);

impl std::fmt::Write for Fingerprint {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

fn fingerprint(value: &dyn std::fmt::Debug) -> u64 {
    use std::fmt::Write as _;
    let mut h = Fingerprint(std::hash::DefaultHasher::new());
    // Writing into a hasher cannot fail.
    let _ = write!(h, "{value:?}");
    h.0.finish()
}

/// `model`'s digest.
#[must_use]
pub fn digest(model: &SessionModel) -> Digest {
    let mut parts: BTreeMap<String, u64> = model
        .tracks
        .iter()
        .enumerate()
        // A track's place in the project is part of it: a track moved is
        // a track changed.
        .map(|(at, t)| (format!("track:{}", t.guid), fingerprint(&(at, t))))
        .collect();
    for (part, print) in [
        ("items", fingerprint(&model.items)),
        ("markers", fingerprint(&model.markers)),
        ("regions", fingerprint(&model.regions)),
        ("tempo", fingerprint(&model.tempo)),
        ("take_markers", fingerprint(&model.take_markers)),
        ("project", fingerprint(&model.project)),
    ] {
        parts.insert(part.to_owned(), print);
    }
    Digest { parts }
}

/// One way the engine differs from the doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// `tracks`, `items`, `markers`, `regions`, `tempo`, `take_markers` or
    /// `project`.
    pub part: &'static str,
    /// The track it is on, by name, when it is on one.
    pub track: Option<String>,
    /// The FX it is in, by name, when it is in one.
    pub fx: Option<String>,
    /// What differs: a field (`volume`, `fx.params`, `sends`), or
    /// `missing` (the doc has it, the engine does not), `extra` (the
    /// engine has it, the doc does not), `order`.
    pub field: String,
    /// The track's, item's, marker's or FX's guid, when it has one.
    pub guid: Option<String>,
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.part)?;
        if let Some(track) = &self.track {
            write!(f, " {track:?}")?;
        }
        if let Some(fx) = &self.fx {
            write!(f, " fx {fx:?}")?;
        }
        if let Some(guid) = &self.guid
            && self.track.is_none()
        {
            write!(f, " {guid}")?;
        }
        write!(f, ": {}", self.field)
    }
}

/// `Volume` → `volume`, `VisibleInTcp` → `visible_in_tcp`.
fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Where `engine` (what the engine shows) differs from `doc` (what the
/// doc says it should): empty when they agree. The chart is not compared.
#[must_use]
pub fn drift(engine: &SessionModel, doc: &SessionModel) -> Vec<Drift> {
    let track_name = |guid: &str| {
        doc.track(guid)
            .or_else(|| engine.track(guid))
            .map(|t| t.name.clone())
    };
    let mut out = Vec::new();
    for change in diff(engine, doc) {
        match change {
            Change::TrackAdded { track, .. } => out.push(Drift {
                part: "tracks",
                track: Some(track.name),
                fx: None,
                field: "missing".into(),
                guid: Some(track.guid),
            }),
            Change::TrackRemoved { guid } => out.push(Drift {
                part: "tracks",
                track: track_name(&guid),
                fx: None,
                field: "extra".into(),
                guid: Some(guid),
            }),
            Change::TrackMoved { guid, .. } => out.push(Drift {
                part: "tracks",
                track: track_name(&guid),
                fx: None,
                field: "order".into(),
                guid: Some(guid),
            }),
            Change::TrackChanged { track, fields } => {
                let shown = engine.track(&track.guid);
                for field in fields {
                    match (field, shown) {
                        (TrackField::Fx, Some(shown)) => {
                            fx_drift(&track, &shown.fx, &track.fx, "fx", &mut out);
                        }
                        (TrackField::InputFx, Some(shown)) => {
                            fx_drift(
                                &track,
                                &shown.input_fx,
                                &track.input_fx,
                                "input_fx",
                                &mut out,
                            );
                        }
                        _ => out.push(Drift {
                            part: "tracks",
                            track: Some(track.name.clone()),
                            fx: None,
                            field: snake(&format!("{field:?}")),
                            guid: Some(track.guid.clone()),
                        }),
                    }
                }
            }
            Change::ItemAdded { guid, .. } => out.push(item(guid, "missing".into())),
            Change::ItemRemoved { guid } => out.push(item(guid, "extra".into())),
            Change::ItemChanged { guid, fields, .. } => {
                for field in fields {
                    out.push(item(guid.clone(), snake(&format!("{field:?}"))));
                }
            }
            Change::MarkerSet { guid, .. } => out.push(mark("markers", guid, &engine.markers)),
            Change::MarkerRemoved { guid } => out.push(mark("markers", guid, &engine.markers)),
            Change::RegionSet { guid, .. } => out.push(mark("regions", guid, &engine.regions)),
            Change::RegionRemoved { guid } => out.push(mark("regions", guid, &engine.regions)),
            Change::TempoChanged(_) => out.push(whole("tempo")),
            Change::TakeMarkersSet { source, .. } => out.push(Drift {
                guid: Some(source),
                ..whole("take_markers")
            }),
            Change::ProjectChanged(_) => out.push(whole("project")),
            // The chart is the doc's alone.
            Change::ChartChanged(_) => {}
        }
    }
    out
}

fn whole(part: &'static str) -> Drift {
    Drift {
        part,
        track: None,
        fx: None,
        field: "differs".into(),
        guid: None,
    }
}

fn item(guid: String, field: String) -> Drift {
    Drift {
        part: "items",
        track: None,
        fx: None,
        field,
        guid: Some(guid),
    }
}

fn mark<V>(part: &'static str, guid: String, engine: &BTreeMap<String, V>) -> Drift {
    let field = if engine.contains_key(&guid) {
        "differs"
    } else {
        "missing"
    };
    Drift {
        part,
        track: None,
        fx: None,
        field: field.into(),
        guid: Some(guid),
    }
}

/// How the chain `engine` differs from `doc`'s, FX by FX, on `track`.
fn fx_drift(
    track: &TrackState,
    engine: &[FxState],
    doc: &[FxState],
    chain: &str,
    out: &mut Vec<Drift>,
) {
    let mut found = Vec::new();
    let mut push = |fx: &FxState, field: String| {
        found.push(Drift {
            part: "tracks",
            track: Some(track.name.clone()),
            fx: Some(fx.name.clone()),
            field,
            guid: Some(fx.guid.clone()),
        });
    };
    for want in doc {
        let Some(have) = engine.iter().find(|f| f.guid == want.guid) else {
            push(want, format!("{chain}.missing"));
            continue;
        };
        for (field, differs) in [
            ("plugin", have.plugin != want.plugin),
            ("name", have.name != want.name),
            ("enabled", have.enabled != want.enabled),
            ("offline", have.offline != want.offline),
            ("state", have.state != want.state),
            ("params", have.params != want.params),
            ("param_count", have.param_count != want.param_count),
            (
                "preset",
                have.preset != want.preset || have.preset_name != want.preset_name,
            ),
            ("config", have.config != want.config),
        ] {
            if differs {
                push(want, format!("{chain}.{field}"));
            }
        }
    }
    for have in engine {
        if !doc.iter().any(|f| f.guid == have.guid) {
            push(have, format!("{chain}.extra"));
        }
    }
    // The same FX, each as it should be, in another order.
    if found.is_empty() {
        found.push(Drift {
            part: "tracks",
            track: Some(track.name.clone()),
            fx: None,
            field: format!("{chain}.order"),
            guid: Some(track.guid.clone()),
        });
    }
    out.extend(found);
}

/// Told of every difference a check finds: when it was checked (`apply`,
/// `idle`, `action`, `build`) and what differed.
pub type Observer = fn(&'static str, &Drift);

static OBSERVER: OnceLock<Observer> = OnceLock::new();

/// Tell `observer` of every difference found from now on (the engine's
/// metric). The first one set holds.
pub fn set_observer(observer: Observer) {
    let _ = OBSERVER.set(observer);
}

/// How many differences a report names, in its span fields and its warning.
const NAMED: usize = 8;

/// A check's outcome, `drifts` found `when`: recorded on the current span
/// when it declares the `engine_check.*` fields (see [`span`]), one
/// warning when there is any, and each difference given to the observer.
pub fn report(when: &'static str, drifts: &[Drift]) {
    let current = tracing::Span::current();
    current.record("engine_check.ok", drifts.is_empty());
    current.record("engine_check.mismatches", drifts.len());
    if drifts.is_empty() {
        return;
    }
    let parts: BTreeSet<&str> = drifts.iter().map(|d| d.part).collect();
    let parts = parts.into_iter().collect::<Vec<_>>().join(",");
    let tracks: BTreeSet<&str> = drifts.iter().filter_map(|d| d.track.as_deref()).collect();
    let tracks = tracks.into_iter().take(NAMED).collect::<Vec<_>>().join(",");
    let named = drifts
        .iter()
        .take(NAMED)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    current.record("engine_check.parts", parts.as_str());
    current.record("engine_check.tracks", tracks.as_str());
    current.record("engine_check.first", named.as_str());
    tracing::warn!(
        engine_check.when = when,
        engine_check.mismatches = drifts.len(),
        engine_check.parts = %parts,
        engine_check.tracks = %tracks,
        engine_check.first = %named,
        "the engine does not match the song's doc"
    );
    if let Some(observe) = OBSERVER.get() {
        for d in drifts {
            observe(when, d);
        }
    }
}

/// The span a check runs in, `when` it runs: the wide event of one check,
/// its fields filled by [`report`] and the check itself.
#[must_use]
pub fn span(when: &'static str) -> tracing::Span {
    tracing::info_span!(
        "engine.check",
        engine_check.when = when,
        engine_check.ok = tracing::field::Empty,
        engine_check.mismatches = tracing::field::Empty,
        engine_check.parts = tracing::field::Empty,
        engine_check.tracks = tracing::field::Empty,
        engine_check.first = tracing::field::Empty,
        engine_check.skipped = tracing::field::Empty,
        engine_check.read = tracing::field::Empty,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> SessionModel {
        let track = |guid: &str, name: &str| TrackState {
            guid: guid.into(),
            name: name.into(),
            volume: 1.0,
            fx: vec![FxState {
                guid: format!("{guid}-eq"),
                plugin: "ReaEQ".into(),
                name: "EQ".into(),
                enabled: true,
                ..FxState::default()
            }],
            ..TrackState::default()
        };
        SessionModel {
            tracks: vec![track("k", "Kick"), track("s", "Snare")],
            chart: "chart".into(),
            ..SessionModel::default()
        }
    }

    #[test]
    fn the_same_song_has_the_same_digest_and_no_drift() {
        let a = song();
        let mut b = song();
        // The chart is the doc's, not the engine's.
        b.chart = "another".into();
        assert_eq!(digest(&a), digest(&b));
        assert!(drift(&a, &b).is_empty());
    }

    #[test]
    fn a_track_and_an_fx_that_differ_are_named() {
        let doc = song();
        let mut engine = song();
        engine.tracks[0].volume = 0.5;
        engine.tracks[1].fx[0].enabled = false;
        engine.tracks[1].fx[0].params.insert(3, 0.25);
        let differing = digest(&engine).differing(&digest(&doc)).len();
        assert_eq!(differing, 2, "both tracks' fingerprints");
        let found = drift(&engine, &doc);
        assert!(
            found.contains(&Drift {
                part: "tracks",
                track: Some("Kick".into()),
                fx: None,
                field: "volume".into(),
                guid: Some("k".into()),
            }),
            "{found:?}"
        );
        let fx: Vec<&str> = found
            .iter()
            .filter(|d| d.fx.as_deref() == Some("EQ"))
            .map(|d| d.field.as_str())
            .collect();
        assert_eq!(fx, ["fx.enabled", "fx.params"], "{found:?}");
    }

    #[test]
    fn what_only_one_side_has_is_missing_or_extra() {
        let doc = song();
        let mut engine = song();
        engine.tracks[1].fx.clear();
        engine.tracks.push(TrackState {
            guid: "x".into(),
            name: "Stray".into(),
            ..TrackState::default()
        });
        let found: Vec<String> = drift(&engine, &doc)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            found.contains(&"tracks \"Snare\" fx \"EQ\": fx.missing".to_owned()),
            "{found:?}"
        );
        assert!(
            found.contains(&"tracks \"Stray\": extra".to_owned()),
            "{found:?}"
        );
    }

    #[test]
    fn snake_names() {
        assert_eq!(snake("VisibleInTcp"), "visible_in_tcp");
        assert_eq!(snake("Volume"), "volume");
    }
}
