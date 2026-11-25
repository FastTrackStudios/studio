//! Where the mixer gets Signal chains from, and where its edits go.
//!
//! [`SignalSource`] is the whole of what the mixer asks of an engine
//! about Signal, each method named after the engine action it stands
//! for: `SIGNAL_CHAIN`, `SIGNAL_RUN`, `SIGNAL_SESSION`, `SIGNAL_ALGORITHMS`,
//! the live feed of
//! each Signal FX's revision and meters, and `FX_SET` for a parameter's
//! value on its CLAP slot. Studio's engine implements it once; until it
//! does, [`Fixture`] answers from `signal.chain/1` documents held in
//! memory and applies structural edits the way the contract says they
//! behave, so the view, its tests and its screenshots run without one.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::{Value, json};

use super::chain::{Block, BlockId, Chain, ChainError, Param, Phase, PhaseId, Status};

/// A track, by its GUID, as the engine's actions take it.
pub type TrackId = str;

/// Why a source could not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The track holds no Signal FX.
    NoSignal(String),
    /// The engine (or plugin) refused, in its words.
    Refused(String),
    /// It answered something that is not a chain.
    Chain(ChainError),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSignal(track) => write!(f, "no Signal on track {track}"),
            Self::Refused(why) => f.write_str(why),
            Self::Chain(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<ChainError> for SourceError {
    fn from(e: ChainError) -> Self {
        Self::Chain(e)
    }
}

/// A value for a parameter's CLAP slot, the way `FX_SET` takes one.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    /// 0..1.
    Normalized(f64),
    /// The plugin's own words (`-18 dB`, `On`).
    Text(String),
}

impl ParamValue {
    /// `FX_SET`'s word for it: `=0.42`, or the text as given.
    #[must_use]
    pub fn fx_set_word(&self) -> String {
        match self {
            Self::Normalized(n) => format!("={:.6}", n.clamp(0.0, 1.0)),
            Self::Text(t) => t.clone(),
        }
    }
}

/// One live meter value: a chain meter's `index` and what it reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterValue {
    pub index: u32,
    pub value: f32,
}

/// Every instance in the process that shares a session (`SIGNAL_SESSION`).
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub session: Option<String>,
    pub ports: Vec<Chain>,
}

/// One algorithm of a block family (`SIGNAL_ALGORITHMS`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Algorithm {
    pub id: String,
    pub name: String,
}

/// A structural edit of a chain, as `SIGNAL_RUN` carries it.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// `SIGNAL_SET {"block", "bypass"}`.
    BypassBlock { block: BlockId, bypass: bool },
    /// `SIGNAL_SET {"phase", "bypass"}`.
    BypassPhase { phase: PhaseId, bypass: bool },
    /// `SIGNAL_SET {"phase", "makeup_db"}`.
    Makeup { phase: PhaseId, db: f64 },
    /// `SIGNAL_SET {"block", "algorithm"}`.
    Algorithm { block: BlockId, algorithm: String },
    /// `SIGNAL_BLOCK_ADD {"phase", "kind", "after"?, "algorithm"?}`.
    Add {
        phase: PhaseId,
        kind: String,
        after: Option<BlockId>,
        algorithm: Option<String>,
    },
    /// `SIGNAL_BLOCK_REMOVE {"block"}`.
    Remove { block: BlockId },
    /// `SIGNAL_BLOCK_MOVE {"block", "phase", "after"?}`; `after: None`
    /// puts it first in the phase.
    Move {
        block: BlockId,
        phase: PhaseId,
        after: Option<BlockId>,
    },
}

impl Edit {
    /// The `run_action` it is: its action id and its arguments.
    #[must_use]
    pub fn action(&self) -> (&'static str, Value) {
        match self {
            Self::BypassBlock { block, bypass } => (
                "SIGNAL_SET",
                json!({ "block": block.as_str(), "bypass": bypass }),
            ),
            Self::BypassPhase { phase, bypass } => (
                "SIGNAL_SET",
                json!({ "phase": phase.as_str(), "bypass": bypass }),
            ),
            Self::Makeup { phase, db } => (
                "SIGNAL_SET",
                json!({ "phase": phase.as_str(), "makeup_db": db }),
            ),
            Self::Algorithm { block, algorithm } => (
                "SIGNAL_SET",
                json!({ "block": block.as_str(), "algorithm": algorithm }),
            ),
            Self::Add {
                phase,
                kind,
                after,
                algorithm,
            } => {
                let mut args = serde_json::Map::new();
                args.insert("phase".into(), phase.as_str().into());
                args.insert("kind".into(), kind.as_str().into());
                if let Some(after) = after {
                    args.insert("after".into(), after.as_str().into());
                }
                if let Some(algorithm) = algorithm {
                    args.insert("algorithm".into(), algorithm.as_str().into());
                }
                ("SIGNAL_BLOCK_ADD", args.into())
            }
            Self::Remove { block } => ("SIGNAL_BLOCK_REMOVE", json!({ "block": block.as_str() })),
            Self::Move {
                block,
                phase,
                after,
            } => {
                let mut args = serde_json::Map::new();
                args.insert("block".into(), block.as_str().into());
                args.insert("phase".into(), phase.as_str().into());
                if let Some(after) = after {
                    args.insert("after".into(), after.as_str().into());
                }
                ("SIGNAL_BLOCK_MOVE", args.into())
            }
        }
    }

    /// The edit an action and its arguments name, for a source that is
    /// handed `SIGNAL_RUN`'s raw form.
    ///
    /// # Errors
    /// An action this contract has not got, or arguments it cannot take.
    pub fn from_action(action: &str, args: &Value) -> Result<Self, SourceError> {
        let text = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_owned);
        let refuse = |why: &str| SourceError::Refused(format!("{action}: {why}"));
        match action {
            "SIGNAL_SET" => {
                if let (Some(block), Some(bypass)) =
                    (text("block"), args.get("bypass").and_then(Value::as_bool))
                {
                    return Ok(Self::BypassBlock {
                        block: BlockId(block),
                        bypass,
                    });
                }
                if let (Some(block), Some(algorithm)) = (text("block"), text("algorithm")) {
                    return Ok(Self::Algorithm {
                        block: BlockId(block),
                        algorithm,
                    });
                }
                let phase = PhaseId(text("phase").ok_or_else(|| refuse("`block` or `phase`"))?);
                if let Some(bypass) = args.get("bypass").and_then(Value::as_bool) {
                    return Ok(Self::BypassPhase { phase, bypass });
                }
                let db = args
                    .get("makeup_db")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| refuse("`bypass` or `makeup_db`"))?;
                Ok(Self::Makeup { phase, db })
            }
            "SIGNAL_BLOCK_ADD" => Ok(Self::Add {
                phase: PhaseId(text("phase").ok_or_else(|| refuse("`phase`"))?),
                kind: text("kind").ok_or_else(|| refuse("`kind`"))?,
                after: text("after").map(BlockId),
                algorithm: text("algorithm"),
            }),
            "SIGNAL_BLOCK_REMOVE" => Ok(Self::Remove {
                block: BlockId(text("block").ok_or_else(|| refuse("`block`"))?),
            }),
            "SIGNAL_BLOCK_MOVE" => Ok(Self::Move {
                block: BlockId(text("block").ok_or_else(|| refuse("`block`"))?),
                phase: PhaseId(text("phase").ok_or_else(|| refuse("`phase`"))?),
                after: text("after").map(BlockId),
            }),
            other => Err(SourceError::Refused(format!("no Signal action {other}"))),
        }
    }
}

/// What the mixer asks an engine about Signal. One impl per engine.
///
/// `track` is the track's GUID; a track holds at most one Signal FX.
pub trait SignalSource: Send + Sync {
    /// `SIGNAL_CHAIN {track}`: the track's `signal.chain/1`.
    ///
    /// # Errors
    /// No Signal on the track, or the engine could not ask it.
    fn signal_chain(&self, track: &TrackId) -> Result<Chain, SourceError>;

    /// `SIGNAL_RUN {track, action, args}`: a structural edit; answers the
    /// new chain (its `revision` bumped).
    ///
    /// # Errors
    /// The plugin refused the edit, in its words.
    fn signal_run(&self, track: &TrackId, action: &str, args: &Value)
    -> Result<Chain, SourceError>;

    /// `SIGNAL_SESSION`: every instance sharing a session with `track`'s.
    ///
    /// # Errors
    /// No Signal on the track.
    fn signal_session(&self, track: &TrackId) -> Result<Session, SourceError>;

    /// `SIGNAL_ALGORITHMS {kind}`: a block family's algorithms, for
    /// `SIGNAL_SET {block, algorithm}` and `SIGNAL_BLOCK_ADD`'s.
    ///
    /// # Errors
    /// No Signal on the track, or no such family.
    fn signal_algorithms(&self, track: &TrackId, kind: &str)
    -> Result<Vec<Algorithm>, SourceError>;

    /// The live feed: each track with a Signal FX and its chain's revision
    /// now. Cheap: polled every frame the mixer paints.
    fn signal_revisions(&self) -> Vec<(String, u64)>;

    /// The live feed: `track`'s meter values now.
    fn signal_meters(&self, track: &TrackId) -> Vec<MeterValue>;

    /// The live feed: each track's `values_revision` now (`signal.chain/1`
    /// addition 2), which moves when any parameter value does (a set,
    /// automation, a macro) while `revision` stays. Cheap: polled with
    /// [`Self::signal_revisions`]. A source without it answers none, and
    /// values follow structural reads only.
    fn signal_values_revisions(&self) -> Vec<(String, u64)> {
        Vec::new()
    }

    /// `FX_SET` on the Signal FX's parameter `clap_id`.
    ///
    /// # Errors
    /// The engine refused, in its words.
    fn fx_set(&self, track: &TrackId, clap_id: &str, value: &ParamValue)
    -> Result<(), SourceError>;
}

/// The arguments of the `FX_SET` a source sends for a Signal parameter.
///
/// `FX_SET` names a parameter by its name or `#index` today; a Signal
/// slot is named by its CLAP id, which is what this asks it to take
/// (`@b014`, proposed alongside `signal.chain/1`).
#[must_use]
pub fn fx_set_args(track: &TrackId, fx: &str, clap_id: &str, value: &ParamValue) -> Value {
    json!({
        "track": track,
        "fx": fx,
        "set": { format!("@{clap_id}"): value.fx_set_word() },
    })
}

/// A call a [`Fixture`] was asked, for tests to read back.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Chain(String),
    Run {
        track: String,
        action: String,
        args: Value,
    },
    Session(String),
    Algorithms(String),
    FxSet {
        track: String,
        clap_id: String,
        value: ParamValue,
    },
}

/// A source over chains held in memory: what the view runs on until the
/// engine answers `SIGNAL_CHAIN`, and what its tests drive.
#[derive(Default)]
pub struct Fixture {
    state: Mutex<FixtureState>,
}

#[derive(Default)]
struct FixtureState {
    chains: BTreeMap<String, Chain>,
    meters: BTreeMap<String, Vec<MeterValue>>,
    calls: Vec<Call>,
}

impl Fixture {
    /// A fixture holding `chains`, each by its track's GUID.
    #[must_use]
    pub fn new(chains: impl IntoIterator<Item = (String, Chain)>) -> Self {
        Self {
            state: Mutex::new(FixtureState {
                chains: chains.into_iter().collect(),
                ..FixtureState::default()
            }),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut FixtureState) -> T) -> T {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut state)
    }

    /// Replace `track`'s chain, as the plugin would after a change the
    /// mixer did not make (a preset load): its revision moves.
    pub fn replace(&self, track: &TrackId, chain: Chain) {
        self.with(|s| s.chains.insert(track.to_owned(), chain));
    }

    /// Set what `track`'s meters read.
    pub fn set_meters(&self, track: &TrackId, values: Vec<MeterValue>) {
        self.with(|s| s.meters.insert(track.to_owned(), values));
    }

    /// Every call asked so far, oldest first.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.with(|s| s.calls.clone())
    }
}

impl SignalSource for Fixture {
    fn signal_chain(&self, track: &TrackId) -> Result<Chain, SourceError> {
        self.with(|s| {
            s.calls.push(Call::Chain(track.to_owned()));
            s.chains
                .get(track)
                .cloned()
                .ok_or_else(|| SourceError::NoSignal(track.to_owned()))
        })
    }

    fn signal_run(
        &self,
        track: &TrackId,
        action: &str,
        args: &Value,
    ) -> Result<Chain, SourceError> {
        let edit = Edit::from_action(action, args)?;
        self.with(|s| {
            s.calls.push(Call::Run {
                track: track.to_owned(),
                action: action.to_owned(),
                args: args.clone(),
            });
            let chain = s
                .chains
                .get_mut(track)
                .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?;
            apply(chain, &edit)?;
            chain.revision = chain.revision.saturating_add(1);
            Ok(chain.clone())
        })
    }

    fn signal_session(&self, track: &TrackId) -> Result<Session, SourceError> {
        self.with(|s| {
            s.calls.push(Call::Session(track.to_owned()));
            let session = s
                .chains
                .get(track)
                .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?
                .session
                .clone();
            let ports = s
                .chains
                .values()
                .filter(|c| c.session == session)
                .cloned()
                .collect();
            Ok(Session { session, ports })
        })
    }

    fn signal_algorithms(
        &self,
        track: &TrackId,
        kind: &str,
    ) -> Result<Vec<Algorithm>, SourceError> {
        self.with(|s| {
            s.calls.push(Call::Algorithms(kind.to_owned()));
            if !s.chains.contains_key(track) {
                return Err(SourceError::NoSignal(track.to_owned()));
            }
            Ok(algorithms_of(kind)
                .iter()
                .map(|(id, name)| Algorithm {
                    id: (*id).to_owned(),
                    name: (*name).to_owned(),
                })
                .collect())
        })
    }

    fn signal_revisions(&self) -> Vec<(String, u64)> {
        self.with(|s| {
            s.chains
                .iter()
                .map(|(t, c)| (t.clone(), c.revision))
                .collect()
        })
    }

    fn signal_meters(&self, track: &TrackId) -> Vec<MeterValue> {
        self.with(|s| s.meters.get(track).cloned().unwrap_or_default())
    }

    fn fx_set(
        &self,
        track: &TrackId,
        clap_id: &str,
        value: &ParamValue,
    ) -> Result<(), SourceError> {
        self.with(|s| {
            s.calls.push(Call::FxSet {
                track: track.to_owned(),
                clap_id: clap_id.to_owned(),
                value: value.clone(),
            });
            let chain = s
                .chains
                .get_mut(track)
                .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?;
            let param = chain
                .phases
                .iter_mut()
                .flat_map(|p| &mut p.blocks)
                .flat_map(|b| &mut b.params)
                .find(|p| p.clap_id.as_deref() == Some(clap_id))
                .ok_or_else(|| SourceError::Refused(format!("no parameter @{clap_id}")))?;
            set_param(param, value);
            // A value is not a revision: the contract keeps those apart.
            Ok(())
        })
    }
}

/// The fixture's families and their algorithms (Signal answers its own).
fn algorithms_of(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "eq" => &[("parametric", "Parametric"), ("channel", "Channel")],
        "reverb" => &[("room", "Room"), ("plate", "Plate"), ("hall", "Hall")],
        "comp" => &[("vca", "VCA"), ("fet", "FET"), ("opto", "Opto")],
        "saturate" => &[("tape", "Tape"), ("tube", "Tube")],
        _ => &[],
    }
}

/// A value written into a fixture's parameter, its text redone plainly.
fn set_param(param: &mut Param, value: &ParamValue) {
    let normalized = match value {
        ParamValue::Normalized(n) => n.clamp(0.0, 1.0),
        ParamValue::Text(t) => param
            .normalized_of_choice(t)
            .or_else(|| {
                crate::tone_read::number(t)
                    .map(|v| ((v - param.min) / (param.max - param.min)).clamp(0.0, 1.0))
            })
            .unwrap_or(param.normalized),
    };
    param.normalized = normalized;
    param.value = param.min + (param.max - param.min) * normalized;
    param.text = super::view::value_text(param);
}

/// `edit` applied to `chain`, as Signal applies it.
fn apply(chain: &mut Chain, edit: &Edit) -> Result<(), SourceError> {
    let missing = |what: &str| SourceError::Refused(format!("no {what}"));
    match edit {
        Edit::BypassBlock { block, bypass } => {
            block_mut(chain, block)
                .ok_or_else(|| missing(&format!("block {}", block.as_str())))?
                .bypass = *bypass;
        }
        Edit::BypassPhase { phase, bypass } => {
            phase_mut(chain, phase)
                .ok_or_else(|| missing(&format!("phase {}", phase.as_str())))?
                .bypass = *bypass;
        }
        Edit::Makeup { phase, db } => {
            phase_mut(chain, phase)
                .ok_or_else(|| missing(&format!("phase {}", phase.as_str())))?
                .makeup_db = *db;
        }
        Edit::Algorithm { block, algorithm } => {
            block_mut(chain, block)
                .ok_or_else(|| missing(&format!("block {}", block.as_str())))?
                .algorithm = Some(algorithm.clone());
        }
        Edit::Add {
            phase,
            kind,
            after,
            algorithm,
        } => {
            let mut block = new_block(chain, phase, kind);
            block.algorithm.clone_from(algorithm);
            insert(chain, phase, after.as_ref(), block)?;
        }
        Edit::Remove { block } => {
            take(chain, block).ok_or_else(|| missing(&format!("block {}", block.as_str())))?;
        }
        Edit::Move {
            block,
            phase,
            after,
        } => {
            if after.as_ref() == Some(block) {
                return Ok(());
            }
            let mut taken =
                take(chain, block).ok_or_else(|| missing(&format!("block {}", block.as_str())))?;
            // `<phase>.<block>`: a move to another phase changes the prefix.
            taken.id = moved_id(&taken.id, phase);
            insert(chain, phase, after.as_ref(), taken)?;
        }
    }
    Ok(())
}

/// A block's id once it is in `phase`.
#[must_use]
pub fn moved_id(id: &BlockId, phase: &PhaseId) -> BlockId {
    let own = id.as_str().split_once('.').map_or(id.as_str(), |(_, b)| b);
    BlockId(format!("{}.{own}", phase.as_str()))
}

fn block_mut<'a>(chain: &'a mut Chain, id: &BlockId) -> Option<&'a mut Block> {
    chain
        .phases
        .iter_mut()
        .flat_map(|p| &mut p.blocks)
        .find(|b| &b.id == id)
}

fn phase_mut<'a>(chain: &'a mut Chain, id: &PhaseId) -> Option<&'a mut Phase> {
    chain.phases.iter_mut().find(|p| &p.id == id)
}

fn take(chain: &mut Chain, id: &BlockId) -> Option<Block> {
    chain.phases.iter_mut().find_map(|p| {
        let at = p.blocks.iter().position(|b| &b.id == id)?;
        Some(p.blocks.remove(at))
    })
}

fn insert(
    chain: &mut Chain,
    phase: &PhaseId,
    after: Option<&BlockId>,
    block: Block,
) -> Result<(), SourceError> {
    let into = phase_mut(chain, phase)
        .ok_or_else(|| SourceError::Refused(format!("no phase {}", phase.as_str())))?;
    let at = match after {
        None => 0,
        Some(after) => {
            into.blocks
                .iter()
                .position(|b| &b.id == after)
                .ok_or_else(|| {
                    SourceError::Refused(format!(
                        "no block {} in {}",
                        after.as_str(),
                        phase.as_str()
                    ))
                })?
                + 1
        }
    };
    into.blocks.insert(at, block);
    Ok(())
}

/// A fresh block of `kind` in `phase`, its id unused in the chain.
fn new_block(chain: &Chain, phase: &PhaseId, kind: &str) -> Block {
    // A phase holds a handful of blocks: a thousand names is never short.
    let id = (1..=1000_u32)
        .map(|n| {
            if n == 1 {
                format!("{}.{kind}", phase.as_str())
            } else {
                format!("{}.{kind}_{n}", phase.as_str())
            }
        })
        .find(|id| chain.block(id).is_none())
        .unwrap_or_default();
    let mut name = kind.to_owned();
    if let Some(first) = name.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    Block {
        id: BlockId(id),
        kind: kind.to_owned(),
        name,
        bypass: false,
        algorithm: None,
        backend: super::chain::Backend::Linked,
        status: Status::Ok,
        key: None,
        params: Vec::new(),
        meters: Vec::new(),
    }
}
