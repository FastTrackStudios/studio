//! `signal.chain/1`, as the mixer reads it: one Signal instance's phases
//! and blocks (FastTrackStudios/daw, `crates/daw/clap-ext/docs/signal-chain.md`).
//!
//! The mixer knows this document and nothing of Signal's crates. Read
//! leniently: a field the contract gains later is ignored, a field it
//! leaves out takes its documented default, and only a document that is
//! not `signal.chain/1` at all is refused.

use serde_json::Value;

/// The schema this reader speaks.
pub const SCHEMA: &str = "signal.chain/1";

/// Why a document is not a chain this reader can draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainError {
    /// Not JSON.
    Json(String),
    /// JSON, but another schema (or none).
    Schema(Option<String>),
    /// A required part is missing or the wrong shape.
    Shape(&'static str),
}

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "not JSON: {e}"),
            Self::Schema(Some(s)) => write!(f, "schema {s:?}, not {SCHEMA}"),
            Self::Schema(None) => write!(f, "no schema; {SCHEMA} expected"),
            Self::Shape(what) => write!(f, "{SCHEMA} without {what}"),
        }
    }
}

impl std::error::Error for ChainError {}

/// A block's stable id (`rescue.gate`), stable across sessions and rebuilds.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub String);

/// A phase's id (`rescue`, `tone`, …).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhaseId(pub String);

impl BlockId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PhaseId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The mix phase this is, where it is one (its tint, its bar).
    #[must_use]
    pub fn mix_phase(&self) -> Option<fts_session::mix_phases::MixPhase> {
        fts_session::mix_phases::MixPhase::from_slug(&self.0)
    }
}

/// One instance's chain.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    /// Moves on every change but a parameter's value.
    pub revision: u64,
    pub session: Option<String>,
    /// The instance's track path in the kit (`Drums/Kick/Sum/In`).
    pub port: Option<String>,
    pub mode: Mode,
    pub midi: Option<Midi>,
    /// Per-track mode's phases; empty in kit mode, whose phases are each
    /// lane's ([`Chain::lanes`]).
    pub phases: Vec<Phase>,
    /// Kit mode: each lane (a strip of the kit) and its phases.
    pub lanes: Vec<Lane>,
    pub macros: Vec<Macro>,
}

/// One lane of a kit-mode instance: a strip of the kit, by its path.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub lane: String,
    pub phases: Vec<Phase>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    PerTrack,
    Kit,
    Other(String),
}

/// What the instance's note input does.
#[derive(Clone, Debug, PartialEq)]
pub struct Midi {
    pub plays: Vec<Plays>,
    /// Notes in become the session's hits for that piece.
    pub publishes_hits: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plays {
    pub note: u8,
    pub piece: String,
    pub block: BlockId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phase {
    pub id: PhaseId,
    pub name: String,
    pub bypass: bool,
    /// The phase's gain-neutral make-up.
    pub makeup_db: f64,
    pub blocks: Vec<Block>,
    /// The phase's output (dB, K-weighted over its last block).
    pub meters: Vec<Meter>,
}

impl Phase {
    /// The `source` phase: what makes sound (a sampler) rather than
    /// processing it. Never bypassed, no make-up.
    #[must_use]
    pub fn is_source(&self) -> bool {
        self.id.as_str() == "source"
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: BlockId,
    /// `eq`, `comp`, `gate`, `saturate`, `reverb`, … — open-ended.
    pub kind: String,
    pub name: String,
    pub bypass: bool,
    /// The family algorithm it runs (`parametric`, `room`), where the
    /// chain says (`SIGNAL_ALGORITHMS` lists the others).
    pub algorithm: Option<String>,
    pub backend: Backend,
    pub status: Status,
    pub key: Option<Key>,
    pub params: Vec<Param>,
    pub meters: Vec<Meter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backend {
    Linked,
    Clap,
    Other(String),
}

/// Whether a block runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// Cannot load here, so bypassed.
    Unavailable,
    Error,
    Other(String),
}

impl Status {
    /// Whether the mixer must say so on the row.
    #[must_use]
    pub const fn is_trouble(&self) -> bool {
        !matches!(self, Self::Ok)
    }

    /// The word the row shows for it.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Ok => "ok",
            Self::Unavailable => "unavailable",
            Self::Error => "error",
            Self::Other(s) => s,
        }
    }
}

/// Where a block's key (sidechain) comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub from: Option<String>,
    pub via: Via,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    /// The session's hits for a piece.
    Hits,
    Midi,
    /// Fell back to its own input.
    OwnInput,
    Other(String),
}

impl Via {
    /// The short word a row shows.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Hits => "hits",
            Self::Midi => "MIDI",
            Self::OwnInput => "self",
            Self::Other(s) => s,
        }
    }
}

/// One parameter, exposed (or not) on a CLAP slot.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub id: String,
    /// The CLAP parameter id it is exposed on; `None`: not exposed, so
    /// not settable from a host.
    pub clap_id: Option<String>,
    pub name: String,
    pub value: f64,
    pub normalized: f64,
    /// The plugin's own words for the value.
    pub text: String,
    pub unit: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// A stepped parameter's choices; empty for a continuous one.
    pub choices: Vec<String>,
}

/// A live value the instance publishes (`signal/1`'s
/// `mod_source_value(index)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meter {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Macro {
    pub id: String,
    pub name: String,
    pub clap_id: Option<String>,
    pub targets: Vec<String>,
}

impl Chain {
    /// A chain from its JSON text.
    ///
    /// # Errors
    /// Not JSON, not `signal.chain/1`, or no `phases`.
    pub fn parse(text: &str) -> Result<Self, ChainError> {
        let doc: Value = serde_json::from_str(text).map_err(|e| ChainError::Json(e.to_string()))?;
        Self::from_value(&doc)
    }

    /// A chain from an already parsed document (an action's answer).
    ///
    /// # Errors
    /// Not `signal.chain/1`, or no `phases`.
    pub fn from_value(doc: &Value) -> Result<Self, ChainError> {
        let schema = doc.get("schema").and_then(Value::as_str);
        if schema != Some(SCHEMA) {
            return Err(ChainError::Schema(schema.map(str::to_owned)));
        }
        let phases = doc
            .get("phases")
            .and_then(Value::as_array)
            .ok_or(ChainError::Shape("phases"))?
            .iter()
            .map(phase)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            revision: doc.get("revision").and_then(Value::as_u64).unwrap_or(0),
            session: opt_text(doc, "session"),
            port: opt_text(doc, "port"),
            mode: match doc.get("mode").and_then(Value::as_str) {
                None | Some("per-track") => Mode::PerTrack,
                Some("kit") => Mode::Kit,
                Some(other) => Mode::Other(other.to_owned()),
            },
            midi: doc.get("midi").filter(|m| !m.is_null()).map(midi),
            phases,
            lanes: array(doc, "lanes")
                .iter()
                .map(|l| {
                    Ok(Lane {
                        lane: text_of(l, "lane"),
                        phases: array(l, "phases")
                            .iter()
                            .map(phase)
                            .collect::<Result<_, _>>()?,
                    })
                })
                .collect::<Result<_, ChainError>>()?,
            macros: array(doc, "macros").iter().map(macro_of).collect(),
        })
    }

    /// The block `id`, with the phase it is in.
    #[must_use]
    pub fn block(&self, id: &str) -> Option<(&Phase, &Block)> {
        self.phases.iter().find_map(|phase| {
            phase
                .blocks
                .iter()
                .find(|b| b.id.as_str() == id)
                .map(|b| (phase, b))
        })
    }

    /// The phase `id`.
    #[must_use]
    pub fn phase(&self, id: &str) -> Option<&Phase> {
        self.phases.iter().find(|p| p.id.as_str() == id)
    }

    /// How many blocks, in every phase.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.phases.iter().map(|p| p.blocks.len()).sum()
    }

    /// Every meter index the chain publishes: its blocks' and its
    /// phases' (one list, `mod_source_info`'s).
    pub fn meter_indices(&self) -> impl Iterator<Item = u32> + '_ {
        self.phases.iter().flat_map(|p| {
            p.meters
                .iter()
                .chain(p.blocks.iter().flat_map(|b| &b.meters))
                .map(|m| m.index)
        })
    }

    /// A kit-mode instance: its blocks are in [`Chain::lanes`].
    #[must_use]
    pub fn is_kit(&self) -> bool {
        self.mode == Mode::Kit || (self.phases.is_empty() && !self.lanes.is_empty())
    }
}

impl Param {
    /// The normalized value `text` would be, for a stepped parameter's
    /// choice; `None` for a continuous one or an unknown word.
    #[must_use]
    pub fn normalized_of_choice(&self, choice: &str) -> Option<f64> {
        let at = self.choices.iter().position(|c| c == choice)?;
        let last = self.choices.len().checked_sub(1)?;
        Some(if last == 0 {
            0.0
        } else {
            crate::num::coord(at) / crate::num::coord(last)
        })
    }
}

fn text_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn opt_text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn number(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn phase(v: &Value) -> Result<Phase, ChainError> {
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .ok_or(ChainError::Shape("a phase's id"))?;
    Ok(Phase {
        id: PhaseId(id.to_owned()),
        name: opt_text(v, "name").unwrap_or_else(|| id.to_owned()),
        bypass: flag(v, "bypass"),
        makeup_db: number(v, "makeup_db"),
        blocks: array(v, "blocks")
            .iter()
            .map(block)
            .collect::<Result<_, _>>()?,
        meters: meters(v),
    })
}

fn block(v: &Value) -> Result<Block, ChainError> {
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .ok_or(ChainError::Shape("a block's id"))?;
    let kind = text_of(v, "kind");
    Ok(Block {
        id: BlockId(id.to_owned()),
        name: opt_text(v, "name").unwrap_or_else(|| kind.clone()),
        kind,
        bypass: flag(v, "bypass"),
        algorithm: opt_text(v, "algorithm"),
        backend: match v.get("backend").and_then(Value::as_str) {
            None | Some("linked") => Backend::Linked,
            Some("clap") => Backend::Clap,
            Some(other) => Backend::Other(other.to_owned()),
        },
        status: match v.get("status").and_then(Value::as_str) {
            None | Some("ok") => Status::Ok,
            Some("unavailable") => Status::Unavailable,
            Some("error") => Status::Error,
            Some(other) => Status::Other(other.to_owned()),
        },
        key: v.get("key").filter(|k| !k.is_null()).map(|k| Key {
            from: opt_text(k, "from"),
            via: match k.get("via").and_then(Value::as_str) {
                Some("hits") => Via::Hits,
                Some("midi") => Via::Midi,
                None | Some("self") => Via::OwnInput,
                Some(other) => Via::Other(other.to_owned()),
            },
        }),
        params: array(v, "params").iter().map(param).collect(),
        meters: meters(v),
    })
}

fn meters(v: &Value) -> Vec<Meter> {
    array(v, "meters")
        .iter()
        .filter_map(|m| {
            Some(Meter {
                id: text_of(m, "id"),
                name: text_of(m, "name"),
                unit: text_of(m, "unit"),
                index: u32::try_from(m.get("index")?.as_u64()?).ok()?,
            })
        })
        .collect()
}

fn param(v: &Value) -> Param {
    Param {
        id: text_of(v, "id"),
        clap_id: opt_text(v, "clap_id"),
        name: text_of(v, "name"),
        value: number(v, "value"),
        normalized: number(v, "normalized"),
        text: text_of(v, "text"),
        unit: text_of(v, "unit"),
        min: number(v, "min"),
        max: number(v, "max"),
        default: number(v, "default"),
        choices: array(v, "choices")
            .iter()
            .filter_map(|c| c.as_str().map(str::to_owned))
            .collect(),
    }
}

fn midi(v: &Value) -> Midi {
    Midi {
        plays: array(v, "plays")
            .iter()
            .filter_map(|p| {
                Some(Plays {
                    note: u8::try_from(p.get("note")?.as_u64()?).ok()?,
                    piece: text_of(p, "piece"),
                    block: BlockId(text_of(p, "block")),
                })
            })
            .collect(),
        publishes_hits: flag(v, "publishes_hits"),
    }
}

fn macro_of(v: &Value) -> Macro {
    Macro {
        id: text_of(v, "id"),
        name: text_of(v, "name"),
        clap_id: opt_text(v, "clap_id"),
        targets: array(v, "targets")
            .iter()
            .filter_map(|t| t.as_str().map(str::to_owned))
            .collect(),
    }
}
