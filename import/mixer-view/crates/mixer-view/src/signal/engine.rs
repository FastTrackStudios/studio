//! The mixer's [`SignalSource`] over Studio's engine: each method the
//! engine action it is named after (`SIGNAL_CHAIN`, `SIGNAL_RUN`,
//! `SIGNAL_SESSION`, Signal's `SIGNAL_ALGORITHMS` through `SIGNAL_RUN`,
//! `FX_SET`), run on the engine this window is linked to — or on the
//! Action List of this process, when the engine runs here.
//!
//! The live feed (`SIGNAL_FEED`: each Signal FX's chain revision, values
//! revision and meters) is polled on a thread of its own every
//! [`FEED_EVERY`] and kept, so [`SignalSource::signal_revisions`] and
//! [`SignalSource::signal_meters`], read every painted frame, only read
//! what was kept.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};

use super::chain::Chain;
use super::source::{
    Algorithm, MeterValue, ParamValue, Session, SignalSource, SourceError, TrackId,
};

/// How often the feed is read.
pub const FEED_EVERY: Duration = Duration::from_millis(100);

/// How long the feed rests after the engine would not answer it.
const FEED_REST: Duration = Duration::from_secs(1);

/// Runs one engine action: its id and JSON arguments in, its JSON out.
pub type Runner = Arc<dyn Fn(&str, &[u8]) -> Result<String, String> + Send + Sync>;

/// The runner for this window: the linked engine's Action List, else this
/// process's (the engine runs here).
#[must_use]
pub fn engine_runner() -> Runner {
    Arc::new(|id, args| {
        if daw_view::view_client::is_linked() {
            daw_view::view_client::run(id, args)
        } else {
            daw_view::actions::list()
                .run_json(id, args)
                .map(|out| String::from_utf8_lossy(&out).into_owned())
                .map_err(|e| e.to_string())
        }
    })
}

/// One Signal FX as the feed last said it.
#[derive(Clone, Debug, Default, PartialEq)]
struct Fed {
    revision: u64,
    values_revision: Option<u64>,
    /// Its place in the track's chain (`#3`): what `FX_SET` names it by.
    place: String,
    meters: Vec<MeterValue>,
}

/// The mixer's source over the engine.
pub struct EngineSource {
    run: Runner,
    fed: Arc<Mutex<HashMap<String, Fed>>>,
}

impl EngineSource {
    /// A source over `run`, its feed polled from now on a thread of its own
    /// for as long as the source lives.
    #[must_use]
    pub fn new(run: Runner) -> Arc<Self> {
        let source = Arc::new(Self {
            run,
            fed: Arc::default(),
        });
        let weak = Arc::downgrade(&source);
        let spawned = std::thread::Builder::new()
            .name("signal-feed".into())
            .spawn(move || {
                while let Some(source) = weak.upgrade() {
                    let rest = if source.poll().is_ok() {
                        FEED_EVERY
                    } else {
                        FEED_REST
                    };
                    drop(source);
                    std::thread::sleep(rest);
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "mixer: no thread to read Signal's feed on");
        }
        source
    }

    /// A source over the engine this window runs against.
    #[must_use]
    pub fn engine() -> Arc<Self> {
        Self::new(engine_runner())
    }

    /// Read the feed once and keep it.
    ///
    /// # Errors
    /// The engine would not answer `SIGNAL_FEED`.
    pub fn poll(&self) -> Result<(), String> {
        let out = (self.run)("SIGNAL_FEED", b"{}")?;
        let fed = parse_feed(&out)?;
        *self.fed.lock().unwrap_or_else(PoisonError::into_inner) = fed;
        Ok(())
    }

    /// Each track's values revision as last fed (`values_revision`, from
    /// Signal builds that send it): read a chain's values again when it
    /// moved. [`SignalSource`] has no place for it yet.
    #[must_use]
    pub fn values_revisions(&self) -> Vec<(String, u64)> {
        self.fed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(|(t, f)| Some((t.clone(), f.values_revision?)))
            .collect()
    }

    fn run_json(&self, id: &str, args: &Value) -> Result<Value, SourceError> {
        let out = (self.run)(id, args.to_string().as_bytes()).map_err(|why| refusal(&why))?;
        serde_json::from_str(&out).map_err(|e| SourceError::Refused(format!("{id}: {e}")))
    }

    /// `SIGNAL_RUN` of Signal's own `action` on `track`'s instance: the
    /// plugin's answer.
    fn signal_action(
        &self,
        track: &TrackId,
        action: &str,
        args: &Value,
    ) -> Result<Value, SourceError> {
        let ran = self.run_json(
            "SIGNAL_RUN",
            &json!({ "track": track, "action": action, "args": args }),
        )?;
        Ok(ran.get("result").cloned().unwrap_or(Value::Null))
    }

    /// The place of `track`'s Signal FX, as the feed (or its chain) says.
    fn place_of(&self, track: &TrackId) -> Result<String, SourceError> {
        if let Some(place) = self
            .fed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(track)
            .map(|f| f.place.clone())
        {
            return Ok(place);
        }
        let read = self.run_json("SIGNAL_CHAIN", &json!({ "track": track }))?;
        read["fx"]["place"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| SourceError::NoSignal(track.to_owned()))
    }
}

/// An engine's refusal, said as the source's error: no Signal on the
/// track is its own.
fn refusal(why: &str) -> SourceError {
    if why.contains("no Signal instance") || why.contains("no signal/1") {
        SourceError::NoSignal(why.to_owned())
    } else {
        SourceError::Refused(why.to_owned())
    }
}

/// `SIGNAL_FEED`'s answer, by track GUID.
fn parse_feed(out: &str) -> Result<HashMap<String, Fed>, String> {
    let feed: Value = serde_json::from_str(out).map_err(|e| format!("SIGNAL_FEED: {e}"))?;
    let rows = feed["fx"].as_array().ok_or("SIGNAL_FEED: no `fx`")?;
    let mut fed = HashMap::new();
    for row in rows {
        let Some(track) = row["track_guid"].as_str() else {
            continue;
        };
        let meters = row["meters"]
            .as_array()
            .map(|m| {
                m.iter()
                    .filter_map(|m| {
                        Some(MeterValue {
                            index: u32::try_from(m["index"].as_u64()?).ok()?,
                            #[allow(clippy::cast_possible_truncation)]
                            value: m["value"].as_f64()? as f32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        // One Signal FX per track: the first the feed names.
        fed.entry(track.to_owned()).or_insert(Fed {
            revision: row["revision"].as_u64().unwrap_or(0),
            values_revision: row["values_revision"].as_u64(),
            place: row["place"].as_str().unwrap_or_default().to_owned(),
            meters,
        });
    }
    Ok(fed)
}

/// A block family's algorithms, from Signal's `SIGNAL_ALGORITHMS`
/// answer: a list of `{id, name}`, or an object holding one as
/// `algorithms`.
fn parse_algorithms(answer: &Value) -> Vec<Algorithm> {
    let list = answer
        .as_array()
        .or_else(|| answer.get("algorithms").and_then(Value::as_array));
    list.map(|l| {
        l.iter()
            .filter_map(|a| {
                let id = a.get("id").and_then(Value::as_str).or_else(|| a.as_str())?;
                let name = a.get("name").and_then(Value::as_str).unwrap_or(id);
                Some(Algorithm {
                    id: id.to_owned(),
                    name: name.to_owned(),
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

impl SignalSource for EngineSource {
    fn signal_chain(&self, track: &TrackId) -> Result<Chain, SourceError> {
        let read = self.run_json("SIGNAL_CHAIN", &json!({ "track": track }))?;
        Ok(Chain::from_value(&read["chain"])?)
    }

    fn signal_run(
        &self,
        track: &TrackId,
        action: &str,
        args: &Value,
    ) -> Result<Chain, SourceError> {
        let answer = self.signal_action(track, action, args)?;
        // A structural change answers the new chain; a build that answers
        // less is read again.
        match Chain::from_value(&answer) {
            Ok(chain) => Ok(chain),
            Err(_) => self.signal_chain(track),
        }
    }

    fn signal_session(&self, track: &TrackId) -> Result<Session, SourceError> {
        let all = self.run_json("SIGNAL_SESSION", &json!({}))?;
        let sessions = all["sessions"].as_array().cloned().unwrap_or_default();
        let ours = sessions
            .iter()
            .find(|s| {
                s["ports"]
                    .as_array()
                    .is_some_and(|p| p.iter().any(|p| p["track_guid"] == track))
            })
            .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?;
        let ports = ours["ports"]
            .as_array()
            .map(|p| {
                p.iter()
                    .map(|p| Chain::from_value(&p["chain"]))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Session {
            session: ours["session"].as_str().map(str::to_owned),
            ports,
        })
    }

    fn signal_algorithms(
        &self,
        track: &TrackId,
        kind: &str,
    ) -> Result<Vec<Algorithm>, SourceError> {
        let answer = self.signal_action(track, "SIGNAL_ALGORITHMS", &json!({ "kind": kind }))?;
        Ok(parse_algorithms(&answer))
    }

    fn signal_revisions(&self) -> Vec<(String, u64)> {
        self.fed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(t, f)| (t.clone(), f.revision))
            .collect()
    }

    fn signal_values_revisions(&self) -> Vec<(String, u64)> {
        self.values_revisions()
    }

    fn signal_meters(&self, track: &TrackId) -> Vec<MeterValue> {
        self.fed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(track)
            .map(|f| f.meters.clone())
            .unwrap_or_default()
    }

    fn fx_set(
        &self,
        track: &TrackId,
        clap_id: &str,
        value: &ParamValue,
    ) -> Result<(), SourceError> {
        let place = self.place_of(track)?;
        let args = super::source::fx_set_args(track, &place, clap_id, value);
        let said =
            (self.run)("FX_SET", args.to_string().as_bytes()).map_err(|why| refusal(&why))?;
        // FX_SET says, per parameter, what it reads now or why not.
        let lines: Vec<String> = serde_json::from_str(&said).unwrap_or_default();
        match lines
            .iter()
            .find(|l| l.starts_with(&format!("@{clap_id}: ")))
        {
            Some(why) => Err(SourceError::Refused(why.clone())),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_is_kept_by_track() {
        let fed = parse_feed(
            r##"{"at_ms":1,"fx":[{"track":"Drums/Kick","track_guid":"t1","fx_guid":"f","place":"#2","name":"Signal","revision":7,"values_revision":3,"meters":[{"id":"gr","name":"GR","unit":"dB","index":0,"value":-4.5}]}]}"##,
        )
        .unwrap();
        let t1 = &fed["t1"];
        assert_eq!(
            (t1.revision, t1.values_revision, t1.place.as_str()),
            (7, Some(3), "#2")
        );
        assert_eq!(
            t1.meters,
            vec![MeterValue {
                index: 0,
                value: -4.5
            }]
        );
    }

    /// An engine that answers SIGNAL_FEED, SIGNAL_CHAIN, SIGNAL_RUN and
    /// FX_SET from the Kick folder's chains (each track's path standing
    /// for its GUID), keeping every call.
    fn scripted() -> (Runner, Arc<Mutex<Vec<(String, Value)>>>) {
        let doc: Value = serde_json::from_str(super::super::fixtures::KICK_KIT).unwrap();
        let chains: HashMap<String, Value> = doc["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (t["path"].as_str().unwrap().to_owned(), t["chain"].clone()))
            .collect();
        let chains = Arc::new(Mutex::new(chains));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&calls);
        let run: Runner = Arc::new(move |id, args| {
            let args: Value = serde_json::from_slice(args).map_err(|e| e.to_string())?;
            kept.lock().unwrap().push((id.to_owned(), args.clone()));
            let mut chains = chains.lock().unwrap();
            let track = args["track"].as_str().unwrap_or_default().to_owned();
            match id {
                "SIGNAL_FEED" => Ok(json!({ "at_ms": 0, "fx": chains.iter().map(|(t, c)| json!({
                    "track": t, "track_guid": t, "fx_guid": format!("fx-{t}"), "place": "#1",
                    "name": "Signal", "revision": c["revision"], "values_revision": 1,
                    "meters": [{ "id": "gr", "name": "GR", "unit": "dB", "index": 0, "value": -3.0 }],
                })).collect::<Vec<_>>() }).to_string()),
                "SIGNAL_CHAIN" => chains
                    .get(&track)
                    .map(|c| json!({ "fx": { "place": "#1" }, "chain": c }).to_string())
                    .ok_or_else(|| format!("no Signal instance (no signal/1) on {track}")),
                "SIGNAL_RUN" => {
                    let chain = chains.get_mut(&track).ok_or("no Signal instance")?;
                    let rev = chain["revision"].as_u64().unwrap_or(0) + 1;
                    chain["revision"] = json!(rev);
                    Ok(json!({ "track": track, "result": chain.clone() }).to_string())
                }
                "FX_SET" => Ok(json!(["@b014 = -30.0 dB"]).to_string()),
                other => Err(format!("no action {other}")),
            }
        });
        (run, calls)
    }

    /// SIGNAL_INSERTS, run on an Action List as a model or a key would,
    /// opens and shuts a strip drawn from the engine's chains; the engine's
    /// actions carry the mixer's edits.
    #[test]
    fn signal_inserts_drives_the_mixer_over_the_engine() {
        let (run, calls) = scripted();
        let source = EngineSource::new(run);
        source.poll().expect("the feed");
        let kick = "Drums/Kick";
        assert!(source.signal_revisions().iter().any(|(t, _)| t == kick));
        assert_eq!(
            source.signal_meters(kick),
            vec![MeterValue {
                index: 0,
                value: -3.0
            }]
        );
        assert_eq!(source.values_revisions().len(), 7);

        let dyn_source: Arc<dyn SignalSource> = source.clone();
        let mut inserts = super::super::Inserts::new(Some(dyn_source));
        let first = inserts.follow();
        assert_eq!(
            first.read.len(),
            7,
            "every chain read once, through SIGNAL_CHAIN"
        );
        assert!(
            inserts.follow().read.is_empty(),
            "nothing again while the feed holds"
        );

        let list = architect::action::ActionList::new();
        super::super::register(&list).expect("SIGNAL_INSERTS registers");
        let mut seen = 0;
        let fold = |inserts: &super::super::Inserts| inserts.strip(kick).expect("the kick").fold;
        let was = fold(&inserts);
        list.run_json(
            "SIGNAL_INSERTS",
            json!({ "track": kick }).to_string().as_bytes(),
        )
        .expect("SIGNAL_INSERTS");
        assert!(super::super::take_asks(&mut inserts, &mut seen));
        assert_ne!(fold(&inserts), was, "turned the other way");
        list.run_json(
            "SIGNAL_INSERTS",
            json!({ "track": kick, "open": true })
                .to_string()
                .as_bytes(),
        )
        .unwrap();
        super::super::take_asks(&mut inserts, &mut seen);
        assert_eq!(fold(&inserts), super::super::Fold::Open);

        // A switch, then a value, go out as the engine's actions.
        let chain = source
            .signal_run(
                kick,
                "SIGNAL_SET",
                &json!({ "block": "rescue.gate", "bypass": true }),
            )
            .expect("the new chain");
        source.poll().unwrap();
        assert!(
            inserts.follow().read.contains(&kick.to_owned()),
            "its revision moved: read again"
        );
        assert!(chain.revision > 0);
        source
            .fx_set(kick, "b014", &ParamValue::Text("-30 dB".into()))
            .expect("set");
        let calls = calls.lock().unwrap();
        let (_, run) = calls.iter().find(|(id, _)| id == "SIGNAL_RUN").unwrap();
        assert_eq!(run["action"], "SIGNAL_SET");
        assert_eq!(run["args"]["block"], "rescue.gate");
        let (_, set) = calls.iter().find(|(id, _)| id == "FX_SET").unwrap();
        assert_eq!(set["set"]["@b014"], "-30 dB");
        assert_eq!(set["fx"], "#1");
    }

    #[test]
    fn algorithms_read_as_a_list_or_an_object() {
        let want = vec![Algorithm {
            id: "vca".into(),
            name: "VCA".into(),
        }];
        assert_eq!(
            parse_algorithms(&json!([{"id": "vca", "name": "VCA"}])),
            want
        );
        assert_eq!(
            parse_algorithms(&json!({"algorithms": [{"id": "vca", "name": "VCA"}]})),
            want
        );
    }
}
