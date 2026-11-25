//! Algorithms: what a block family (EQ, Reverb, Delay, …) holds.
//!
//! A Signal block is a **family**: one EQ block, one Reverb block. Inside
//! it, an **algorithm** is chosen; the block's parameters are that
//! algorithm's controls. An algorithm is one of:
//!
//! - **a mapping onto an engine:** controls of its own (an SSL face's
//!   twelve knobs) and an expression per engine target
//!   ([`crate::expr`]): the same DSP, different parameters looking at it;
//! - **an engine's own DSP:** a reverb's Plate, a delay's Tape. Controls
//!   are (a subset of) the engine's targets by id, and pass straight
//!   through. An engine with no algorithm file gets one of these, with
//!   every target, automatically.
//!
//! Algorithms are data, written in Styx ([`AlgorithmDef`],
//! `algorithms/*.styx`), loaded at run time and validated: Signal's own
//! ship as files through the same loader a downloaded one goes through.
//! Building one needs no compiler; running one allocates nothing.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::expr::{self, Curve, Op, Scope};
use crate::{BlockProcessor, BuildError, Midi, ModSourceSpec, ParamSpec, Unit};

// ── The file ────────────────────────────────────────────────────────────

/// An algorithm as written: what an `.styx` algorithm file holds.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "files", derive(facet::Facet))]
pub struct AlgorithmDef {
    /// Unique in its family: `ssl-e`.
    pub id: String,
    /// `eq`, `reverb`, `delay`, …
    pub family: String,
    pub name: String,
    /// The engine and its target schema version: `eq.canonical@1`.
    pub engine: String,
    #[cfg_attr(feature = "files", facet(default))]
    pub author: Option<String>,
    #[cfg_attr(feature = "files", facet(default))]
    pub version: Option<String>,
    #[cfg_attr(feature = "files", facet(default))]
    pub description: Option<String>,
    /// The face the views draw for it.
    #[cfg_attr(feature = "files", facet(default))]
    pub look: Option<String>,
    /// Its controls, in order (the CLAP form's slots follow this order).
    /// Absent: every engine target, as itself.
    #[cfg_attr(feature = "files", facet(default))]
    pub controls: Option<Vec<ControlDef>>,
    /// Engine target id → expression over the controls. Absent: each
    /// control drives the target of the same id.
    #[cfg_attr(feature = "files", facet(default))]
    pub map: Option<BTreeMap<String, String>>,
    /// Named breakpoint curves (`x0 y0 x1 y1 …`), for `curve(x, name)`.
    #[cfg_attr(feature = "files", facet(default))]
    pub curves: Option<BTreeMap<String, Vec<f32>>>,
}

/// One control as written.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "files", derive(facet::Facet))]
pub struct ControlDef {
    pub id: String,
    pub name: String,
    /// `db`, `hz`, `ms`, `percent`, or none (a plain number).
    #[cfg_attr(feature = "files", facet(default))]
    pub unit: Option<String>,
    #[cfg_attr(feature = "files", facet(default))]
    pub min: Option<f32>,
    #[cfg_attr(feature = "files", facet(default))]
    pub max: Option<f32>,
    #[cfg_attr(feature = "files", facet(default))]
    pub default: Option<f32>,
    /// A skewed taper's factor (nice-plug's `skew_factor(x)` is `2^x`).
    #[cfg_attr(feature = "files", facet(default))]
    pub skew: Option<f32>,
    /// With `skew`: skewed either side of this value.
    #[cfg_attr(feature = "files", facet(default))]
    pub center: Option<f32>,
    /// Decimals shown for a plain number.
    #[cfg_attr(feature = "files", facet(default))]
    pub decimals: Option<u8>,
    /// An on/off switch.
    #[cfg_attr(feature = "files", facet(default))]
    pub toggle: Option<bool>,
    /// A choice among these names (the value is the index).
    #[cfg_attr(feature = "files", facet(default))]
    pub choices: Option<Vec<String>>,
}

/// Why an algorithm was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlgoError {
    /// The algorithm, as `family/id`.
    pub algorithm: String,
    pub why: String,
}

impl fmt::Display for AlgoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.algorithm, self.why)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for AlgoError {}

// ── Compiled ────────────────────────────────────────────────────────────

/// An algorithm, validated and compiled.
#[derive(Debug)]
pub struct Algorithm {
    pub id: &'static str,
    pub family: &'static str,
    pub name: &'static str,
    pub engine: &'static str,
    pub author: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub look: Option<String>,
    /// The block's parameters while this algorithm is chosen.
    pub controls: &'static [ParamSpec],
    /// Engine target index → its program.
    outputs: Vec<(usize, Vec<Op>)>,
    curves: Vec<Curve>,
}

/// A string for the life of the process. Algorithms are loaded once each
/// (the registry keeps them by family and id), so this is bounded by how
/// many there are, and it is what lets their controls be `ParamSpec`s.
fn intern(s: &str) -> &'static str {
    Box::leak(String::from(s).into_boxed_str())
}

fn unit_of(def: &ControlDef, choices: Option<&'static [&'static str]>) -> Result<Unit, String> {
    if let Some(c) = choices {
        return Ok(Unit::Choice(c));
    }
    if def.toggle == Some(true) {
        return Ok(Unit::Toggle);
    }
    Ok(
        match def.unit.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None | Some("") => Unit::Plain(def.decimals.unwrap_or(2)),
            Some("db") => Unit::Db,
            Some("hz") => Unit::Hz,
            Some("ms") => Unit::Ms,
            Some("percent" | "%") => Unit::Percent,
            Some(other) => return Err(format!("control {}: unknown unit `{other}`", def.id)),
        },
    )
}

fn control_spec(def: &ControlDef) -> Result<ParamSpec, String> {
    let choices: Option<&'static [&'static str]> = def.choices.as_ref().map(|c| {
        let v: Vec<&'static str> = c.iter().map(|s| intern(s)).collect();
        &*Box::leak(v.into_boxed_slice())
    });
    let unit = unit_of(def, choices)?;
    let (min, max) = match unit {
        Unit::Choice(names) => (0.0, crate::util_f32(names.len().saturating_sub(1))),
        Unit::Toggle => (0.0, 1.0),
        _ => (
            def.min
                .ok_or_else(|| format!("control {}: needs min", def.id))?,
            def.max
                .ok_or_else(|| format!("control {}: needs max", def.id))?,
        ),
    };
    if !(min < max) {
        return Err(format!("control {}: min must be below max", def.id));
    }
    let default = def.default.unwrap_or(min);
    if !(min..=max).contains(&default) {
        return Err(format!(
            "control {}: default {default} is outside {min}..{max}",
            def.id
        ));
    }
    let mut spec = ParamSpec::new(intern(&def.id), intern(&def.name), unit, min, max, default);
    match (def.skew, def.center) {
        (Some(f), Some(c)) if f > 0.0 => spec = spec.symmetrical_skewed(f, c),
        (Some(f), None) if f > 0.0 => spec = spec.skewed(f),
        (Some(_), _) => return Err(format!("control {}: skew must be above 0", def.id)),
        (None, _) => {}
    }
    Ok(spec)
}

impl Algorithm {
    /// Validate and compile `def` against the engines this build has.
    ///
    /// # Errors
    ///
    /// [`AlgoError`]: an unknown engine or target, a bad control, an
    /// expression that doesn't compile.
    pub fn compile(def: &AlgorithmDef) -> Result<Self, AlgoError> {
        let name = format!("{}/{}", def.family, def.id);
        let fail = |why: String| AlgoError {
            algorithm: name.clone(),
            why,
        };
        if def.id.is_empty() || def.family.is_empty() {
            return Err(fail("needs an id and a family".into()));
        }
        let engine = crate::engines::all()
            .into_iter()
            .find(|e| e.id == def.engine)
            .ok_or_else(|| fail(format!("no engine `{}` in this build", def.engine)))?;
        if engine.family != def.family {
            return Err(fail(format!(
                "engine {} is a {} engine, not {}",
                engine.id, engine.family, def.family
            )));
        }
        let targets = crate::engines::targets_of(engine.id)
            .ok_or_else(|| fail("engine has no targets".into()))?;

        let controls: &'static [ParamSpec] = match &def.controls {
            None => targets,
            Some(list) => {
                let mut specs = Vec::with_capacity(list.len());
                for c in list {
                    if specs.iter().any(|s: &ParamSpec| s.id == c.id) {
                        return Err(fail(format!("control {} twice", c.id)));
                    }
                    specs.push(control_spec(c).map_err(fail)?);
                }
                Box::leak(specs.into_boxed_slice())
            }
        };

        let curve_names: Vec<&str> = def
            .curves
            .iter()
            .flat_map(|c| c.keys().map(String::as_str))
            .collect();
        let curves = def
            .curves
            .iter()
            .flat_map(|c| c.iter())
            .map(|(n, flat)| Curve::from_flat(flat).map_err(|e| fail(format!("curve {n}: {e}"))))
            .collect::<Result<Vec<_>, _>>()?;
        let control_ids: Vec<&str> = controls.iter().map(|s| s.id).collect();

        let mut outputs = Vec::new();
        let mut ops_total = 0_usize;
        match &def.map {
            Some(map) => {
                for (target, src) in map {
                    let ti = targets.iter().position(|t| t.id == target).ok_or_else(|| {
                        fail(format!("map: {} has no target `{target}`", engine.id))
                    })?;
                    let choices: &[&str] = match targets.get(ti).map(|t| t.unit) {
                        Some(Unit::Choice(names)) => names,
                        _ => &[],
                    };
                    let scope = Scope {
                        controls: &control_ids,
                        choices,
                        curves: &curve_names,
                    };
                    let ops = expr::compile(src, &scope)
                        .map_err(|e| fail(format!("map {target}: {e}")))?;
                    ops_total = ops_total.saturating_add(ops.len());
                    outputs.push((ti, ops));
                }
            }
            None => {
                // Straight through, by id.
                for (ci, c) in controls.iter().enumerate() {
                    let ti = targets.iter().position(|t| t.id == c.id).ok_or_else(|| {
                        fail(format!(
                            "control {} drives no target of {} (add a map)",
                            c.id, engine.id
                        ))
                    })?;
                    outputs.push((
                        ti,
                        alloc::vec![Op::Control(u16::try_from(ci).unwrap_or(u16::MAX))],
                    ));
                }
            }
        }
        if ops_total > expr::MAX_OPS {
            return Err(fail(format!(
                "the map is {ops_total} operations; at most {}",
                expr::MAX_OPS
            )));
        }

        Ok(Self {
            id: intern(&def.id),
            family: intern(&def.family),
            name: intern(&def.name),
            engine: engine.id,
            author: def.author.clone(),
            version: def.version.clone(),
            description: def.description.clone(),
            look: def.look.clone(),
            controls,
            outputs,
            curves,
        })
    }

    /// Engine target values for `controls` (plain), as `(target, value)`.
    pub fn targets_for<'a>(
        &'a self,
        controls: &'a [f32],
    ) -> impl Iterator<Item = (usize, f32)> + 'a {
        self.outputs
            .iter()
            .map(move |(t, ops)| (*t, expr::eval(ops, controls, &self.curves)))
    }
}

// ── The family block ────────────────────────────────────────────────────

/// A family's block running one algorithm on its engine.
pub struct FamilyBlock {
    algorithm: Arc<Algorithm>,
    engine: Box<dyn BlockProcessor>,
    controls: Vec<f32>,
    /// What each target was last set to (f32 bits), per engine target.
    sent: Vec<u32>,
    targets: &'static [ParamSpec],
}

impl FamilyBlock {
    /// `algorithm` on a fresh engine.
    ///
    /// # Errors
    ///
    /// When the engine can't be built for this rate or block size.
    pub fn new(
        algorithm: Arc<Algorithm>,
        sample_rate: f32,
        max_block: usize,
    ) -> Result<Self, BuildError> {
        let engine = crate::engines::build(algorithm.engine, sample_rate, max_block)
            .ok_or_else(|| BuildError(format!("no engine {}", algorithm.engine)))??;
        let targets = engine.params();
        let controls = algorithm.controls.iter().map(|c| c.default).collect();
        let mut block = Self {
            sent: alloc::vec![u32::MAX; targets.len()],
            targets,
            algorithm,
            engine,
            controls,
        };
        block.push_all();
        // Settle the engine on its starting values (its smoothers on them,
        // not gliding from the engine's own defaults): a fresh block is then
        // in one state whoever built it, the compiled-in graph or a CLAP
        // swapping in a new algorithm.
        block.engine.reset();
        Ok(block)
    }

    #[must_use]
    pub fn algorithm(&self) -> &Arc<Algorithm> {
        &self.algorithm
    }

    fn push_all(&mut self) {
        let Self {
            algorithm,
            engine,
            controls,
            sent,
            targets,
        } = self;
        for (t, v) in algorithm.targets_for(controls) {
            let v = targets.get(t).map_or(v, |spec| spec.clamp(v));
            if let Some(last) = sent.get_mut(t) {
                if *last != v.to_bits() {
                    *last = v.to_bits();
                    engine.set_param(t, v);
                }
            }
        }
    }
}

impl BlockProcessor for FamilyBlock {
    fn params(&self) -> &'static [ParamSpec] {
        self.algorithm.controls
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(slot) = self.controls.get_mut(index) else {
            return;
        };
        if slot.to_bits() == value.to_bits() {
            return;
        }
        *slot = value;
        // Every target re-evaluated (bounded, allocation-free), and only the
        // ones whose value moved are set.
        self.push_all();
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        // An empty piece (a host splitting a block at an event at its very
        // start) must not move the engine: some advance their modulators
        // per call, not per sample.
        if left.is_empty() || right.is_empty() {
            return;
        }
        self.engine.process(left, right);
    }

    fn latency(&self) -> u32 {
        self.engine.latency()
    }

    fn midi(&mut self, event: Midi) {
        self.engine.midi(event);
    }

    fn set_key(&mut self, left: &[f32], right: &[f32]) {
        self.engine.set_key(left, right);
    }

    fn hit(&mut self, offset: u32, velocity: f32) {
        self.engine.hit(offset, velocity);
    }

    fn keyed(&mut self) {
        self.engine.keyed();
    }

    fn hits_known(&mut self, known: bool) {
        self.engine.hits_known(known);
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        self.engine.mod_sources()
    }

    fn mod_source(&self, index: usize) -> f32 {
        self.engine.mod_source(index)
    }
}

// ── The registry ────────────────────────────────────────────────────────

/// Signal's own algorithm files, shipped through the same loader as a
/// downloaded one.
#[cfg(feature = "files")]
const BUILT_IN: &[(&str, &str)] = &[
    ("eq/channel", include_str!("../algorithms/eq/channel.styx")),
    ("eq/ssl-e", include_str!("../algorithms/eq/ssl-e.styx")),
    (
        "reverb/plate",
        include_str!("../algorithms/reverb/plate.styx"),
    ),
    ("delay/tape", include_str!("../algorithms/delay/tape.styx")),
];

/// Each family's algorithm when a preset names none.
pub const DEFAULTS: &[(&str, &str)] = &[
    ("eq", "parametric"),
    ("dyn_eq", "dynamic"),
    ("reverb", "plate"),
    ("delay", "tape"),
    ("comp", "generic"),
    ("drive", "preamp"),
    ("gate", "drum"),
    ("transient", "designer"),
    ("gain", "gain"),
];

/// The id of the automatic, straight-through algorithm for an engine:
/// `eq.canonical@1` → `parametric` (the FTS EQ's own face), `reverb.hall@1`
/// → `hall`.
fn identity_id(engine_id: &str) -> &str {
    match engine_id {
        "eq.canonical@1" => "parametric",
        "dyn_eq.bands@1" => "dynamic",
        "gain@1" => "gain",
        id => id
            .split_once('.')
            .map_or(id, |(_, rest)| rest)
            .split_once('@')
            .map_or(id, |(name, _)| name),
    }
}

#[cfg(feature = "std")]
mod registry {
    use super::*;
    use std::sync::{OnceLock, PoisonError, RwLock};

    pub(super) fn all() -> &'static RwLock<Vec<Arc<Algorithm>>> {
        static ALL: OnceLock<RwLock<Vec<Arc<Algorithm>>>> = OnceLock::new();
        ALL.get_or_init(|| {
            let mut list: Vec<Arc<Algorithm>> = Vec::new();
            #[cfg(feature = "files")]
            for (name, text) in BUILT_IN {
                match super::parse_styx(text).and_then(|d| Algorithm::compile(&d)) {
                    Ok(a) => list.push(Arc::new(a)),
                    // A broken built-in is this crate's bug; its test catches it.
                    Err(e) => tracing_free_note(name, &e),
                }
            }
            for engine in crate::engines::all() {
                let id = identity_id(engine.id);
                if list.iter().any(|a| a.family == engine.family && a.id == id) {
                    continue;
                }
                let def = AlgorithmDef {
                    id: id.into(),
                    family: engine.family.into(),
                    name: engine.name.into(),
                    engine: engine.id.into(),
                    ..AlgorithmDef::default()
                };
                if let Ok(a) = Algorithm::compile(&def) {
                    list.push(Arc::new(a));
                }
            }
            RwLock::new(list)
        })
    }

    /// A built-in that doesn't compile is skipped; the crate's own test
    /// asserts every built-in compiles, so this never ships.
    fn tracing_free_note(_name: &str, _e: &AlgoError) {}

    #[allow(clippy::disallowed_methods, reason = "control thread: the registry")]
    pub(super) fn read<R>(f: impl FnOnce(&[Arc<Algorithm>]) -> R) -> R {
        f(&all().read().unwrap_or_else(PoisonError::into_inner))
    }

    #[allow(clippy::disallowed_methods, reason = "control thread: the registry")]
    pub(super) fn insert(a: Arc<Algorithm>) {
        let mut list = all().write().unwrap_or_else(PoisonError::into_inner);
        list.retain(|x| !(x.family == a.family && x.id == a.id));
        list.push(a);
    }
}

/// Every algorithm of `family`.
#[cfg(feature = "std")]
#[must_use]
pub fn family(family: &str) -> Vec<Arc<Algorithm>> {
    registry::read(|all| all.iter().filter(|a| a.family == family).cloned().collect())
}

/// Every family with at least one algorithm.
#[cfg(feature = "std")]
#[must_use]
pub fn families() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = registry::read(|all| all.iter().map(|a| a.family).collect());
    out.sort_unstable();
    out.dedup();
    out
}

/// Algorithm `id` of `family`, or the family's default when `id` is `None`.
#[cfg(feature = "std")]
#[must_use]
pub fn find(family: &str, id: Option<&str>) -> Option<Arc<Algorithm>> {
    let id = id.or_else(|| DEFAULTS.iter().find(|(f, _)| *f == family).map(|(_, a)| *a))?;
    registry::read(|all| {
        all.iter()
            .find(|a| a.family == family && a.id == id)
            .cloned()
    })
}

/// Add (or replace) an algorithm: one a person downloaded, or wrote.
///
/// # Errors
///
/// [`AlgoError`] when it doesn't validate.
#[cfg(feature = "std")]
pub fn register(def: &AlgorithmDef) -> Result<Arc<Algorithm>, AlgoError> {
    let a = Arc::new(Algorithm::compile(def)?);
    registry::insert(a.clone());
    Ok(a)
}

/// Read an algorithm file.
///
/// # Errors
///
/// [`AlgoError`] when it isn't valid Styx for an algorithm.
#[cfg(feature = "files")]
pub fn parse_styx(text: &str) -> Result<AlgorithmDef, AlgoError> {
    facet_styx::from_str(text).map_err(|e| AlgoError {
        algorithm: "(file)".into(),
        why: format!("{e}"),
    })
}

/// Read, validate and register an algorithm file.
///
/// # Errors
///
/// [`AlgoError`] from parsing or validation.
#[cfg(feature = "files")]
pub fn load_styx(text: &str) -> Result<Arc<Algorithm>, AlgoError> {
    register(&parse_styx(text)?)
}

/// Build `family`'s block on algorithm `id` (its default when `None`).
/// `None` when there is no such family or algorithm.
///
/// # Errors
///
/// When the engine can't be built for this rate or block size.
#[cfg(feature = "std")]
pub fn build(
    family: &str,
    id: Option<&str>,
    sample_rate: f32,
    max_block: usize,
) -> Option<Result<FamilyBlock, BuildError>> {
    let algo = find(family, id)?;
    Some(FamilyBlock::new(algo, sample_rate, max_block))
}

#[cfg(all(test, feature = "files"))]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_algorithm_compiles() {
        for (name, text) in BUILT_IN {
            let def = parse_styx(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            Algorithm::compile(&def).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        for (family, id) in DEFAULTS {
            assert!(
                find(family, Some(id)).is_some(),
                "{family}'s default {id} is missing"
            );
        }
        // Every engine has an algorithm: a reverb's every space is there.
        for engine in crate::engines::all() {
            assert!(
                find(engine.family, Some(identity_id(engine.id))).is_some(),
                "{}",
                engine.id
            );
        }
    }

    #[test]
    fn an_ssl_face_maps_onto_the_canonical_eq() {
        let ssl = find("eq", Some("ssl-e")).unwrap_or_else(|| panic!("ssl-e"));
        assert!(ssl.controls.len() < 20, "a face, not the whole engine");
        let mut block =
            FamilyBlock::new(ssl.clone(), 48_000.0, 256).unwrap_or_else(|e| panic!("{e}"));
        let i = ssl
            .controls
            .iter()
            .position(|c| c.id == "lf_gain")
            .unwrap_or_else(|| panic!("lf_gain"));
        block.set_param(i, 9.0);
        let mut l = alloc::vec![0.1_f32; 256];
        let mut r = l.clone();
        block.process(&mut l, &mut r);
        assert!(l.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn a_downloaded_algorithm_is_validated_then_runs() {
        let file = r#"
id tilt
family eq
name "Tilt (community)"
engine "eq.canonical@1"
author someone
controls (
    {id tilt, name Tilt, unit db, min -6, max 6, default 0}
)
map {
    band.1.used "1"
    band.1.shape tilt_shelf
    band.1.freq "650"
    band.1.gain tilt
}
"#;
        let a = load_styx(file).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.controls.len(), 1);
        assert!(find("eq", Some("tilt")).is_some());
        let bad = file.replace("band.1.gain tilt", "band.1.gane tilt");
        assert!(
            load_styx(&bad)
                .map_err(|e| e.why)
                .err()
                .unwrap_or_default()
                .contains("no target `band.1.gane`")
        );
        let bad = file.replace("band.1.gain tilt", "band.1.gain \"tilt +\"");
        assert!(load_styx(&bad).is_err());
    }
}
