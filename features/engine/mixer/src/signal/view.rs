//! A Signal FX as the mixer shows it: rows, as if every block were its
//! own insert, and the live state each strip keeps of its chain.
//!
//! The rows are honest functions of a chain and a fold ([`rows`],
//! [`layout`], [`hit`]); [`Inserts`] is the one place that talks to a
//! [`SignalSource`]: it re-reads a chain when its revision moves, reads
//! meters at [`METER_HZ`], and sends edits.

use std::collections::HashMap;
use std::sync::Arc;

use web_time::{Duration, Instant};

use super::chain::{Block, BlockId, Chain, Phase, PhaseId};
use super::source::{Edit, MeterValue, ParamValue, SignalSource, SourceError, TrackId};
use crate::tone::Panel;

/// How often the meters are read: often enough to read a compressor
/// working, rare enough that a mixer of fifty Signal strips costs nothing.
pub const METER_HZ: u32 = 10;

/// The group's own row: `Signal`, its block count and the chevron.
pub const FX_H: f64 = 18.0;
/// A phase's bar, as tall as the Tone rack's (`tone::HEAD_H`).
pub const PHASE_H: f64 = crate::tone::HEAD_H + 3.0;
/// A block's row: its name and kind, then its status or key.
pub const BLOCK_H: f64 = 30.0;

/// Whether a Signal FX shows its blocks or one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// Every phase and block, each as its own insert.
    Open,
    /// Signal as one insert.
    Shut,
}

impl Fold {
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Open => Self::Shut,
            Self::Shut => Self::Open,
        }
    }

    #[must_use]
    pub const fn of(open: bool) -> Self {
        if open { Self::Open } else { Self::Shut }
    }
}

/// One row of a Signal FX in a strip's insert list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row<'a> {
    /// The FX itself: the group's header, or the one row when shut.
    Fx { chain: &'a Chain, fold: Fold },
    /// A phase's header: its name, bypass and make-up.
    Phase(&'a Phase),
    /// A block, as an insert.
    Block { phase: &'a Phase, block: &'a Block },
}

impl Row<'_> {
    /// How tall it is.
    #[must_use]
    pub const fn height(&self) -> f64 {
        match self {
            Self::Fx { .. } => FX_H,
            Self::Phase(_) => PHASE_H,
            Self::Block { .. } => BLOCK_H,
        }
    }
}

/// The rows a chain shows under `fold`, top to bottom.
#[must_use]
pub fn rows(chain: &Chain, fold: Fold) -> Vec<Row<'_>> {
    let head = std::iter::once(Row::Fx { chain, fold });
    if fold == Fold::Shut {
        return head.collect();
    }
    head.chain(chain.phases.iter().flat_map(|phase| {
        std::iter::once(Row::Phase(phase)).chain(
            phase
                .blocks
                .iter()
                .map(move |block| Row::Block { phase, block }),
        )
    }))
    .collect()
}

/// Every row with the box it is drawn in, from the top of `at` (moved up
/// by the rack's scroll already).
#[must_use]
pub fn layout<'a>(rows: &[Row<'a>], at: Panel) -> Vec<(Row<'a>, Panel)> {
    rows.iter()
        .scan(at.y, |y, row| {
            let top = *y;
            *y += row.height();
            Some((
                *row,
                Panel {
                    x: at.x,
                    y: top,
                    width: at.width,
                    height: row.height(),
                },
            ))
        })
        .collect()
}

/// How tall a chain's rows are under `fold`.
#[must_use]
pub fn tall(chain: &Chain, fold: Fold) -> f64 {
    rows(chain, fold).iter().map(Row::height).sum()
}

/// What a press on a row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    /// The group's header: open or shut it.
    Fold,
    /// A phase's switch.
    PhaseBypass(PhaseId),
    /// A phase's make-up: dragged up or down.
    Makeup(PhaseId),
    /// A block's switch.
    BlockBypass(BlockId),
    /// The rest of a block's row: its close-up.
    Open(BlockId),
}

/// How wide the switch at a row's right is.
pub const SWITCH_W: f64 = 16.0;
/// How wide a phase's make-up readout is, left of its switch.
pub const MAKEUP_W: f64 = 44.0;

/// The switch's box at the right of a row.
#[must_use]
pub fn switch_box(at: Panel) -> Panel {
    Panel {
        x: at.x + at.width - SWITCH_W,
        y: at.y,
        width: SWITCH_W,
        height: at.height,
    }
}

/// The make-up readout's box on a phase's bar.
#[must_use]
pub fn makeup_box(at: Panel) -> Panel {
    Panel {
        x: at.x + at.width - SWITCH_W - MAKEUP_W,
        y: at.y,
        width: MAKEUP_W,
        height: at.height,
    }
}

/// What `(x, y)` presses among laid-out rows.
#[must_use]
pub fn hit(laid: &[(Row<'_>, Panel)], x: f64, y: f64) -> Option<Hit> {
    let (row, at) = laid.iter().find(|(_, at)| at.contains(x, y))?;
    let on_switch = switch_box(*at).contains(x, y);
    Some(match row {
        Row::Fx { .. } => Hit::Fold,
        // The source phase is never bypassed and has no make-up.
        Row::Phase(phase) if phase.is_source() => return None,
        Row::Phase(phase) if on_switch => Hit::PhaseBypass(phase.id.clone()),
        Row::Phase(phase) if makeup_box(*at).contains(x, y) => Hit::Makeup(phase.id.clone()),
        Row::Phase(_) => return None,
        Row::Block { block, .. } if on_switch => Hit::BlockBypass(block.id.clone()),
        Row::Block { block, .. } => Hit::Open(block.id.clone()),
    })
}

/// The edit a switch's press is, against the chain as it stands.
#[must_use]
pub fn toggle_edit(chain: &Chain, hit: &Hit) -> Option<Edit> {
    match hit {
        Hit::PhaseBypass(id) => Some(Edit::BypassPhase {
            phase: id.clone(),
            bypass: !chain.phase(id.as_str())?.bypass,
        }),
        Hit::BlockBypass(id) => Some(Edit::BypassBlock {
            block: id.clone(),
            bypass: !chain.block(id.as_str())?.1.bypass,
        }),
        Hit::Fold | Hit::Makeup(_) | Hit::Open(_) => None,
    }
}

/// How many dB a pixel of make-up drag is.
pub const MAKEUP_DB_PER_PX: f64 = 0.1;

/// A make-up dragged `dy` pixels (down is less) from `was`, in tenths.
#[must_use]
pub fn dragged_makeup(was: f64, dy: f64) -> f64 {
    let to = was - dy * MAKEUP_DB_PER_PX;
    (to.clamp(-24.0, 24.0) * 10.0).round() / 10.0
}

/// The text a parameter's value reads in its unit, for a value the
/// plugin has not answered yet (the fixture's edits, a drag in flight).
#[must_use]
pub fn value_text(param: &super::chain::Param) -> String {
    if !param.choices.is_empty() {
        let last = param.choices.len().saturating_sub(1);
        let at = (param.normalized * crate::num::coord(last)).round();
        let at = num_traits::ToPrimitive::to_usize(&at)
            .unwrap_or(0)
            .min(last);
        return param.choices.get(at).cloned().unwrap_or_default();
    }
    let v = param.value;
    match param.unit.as_str() {
        "dB" => format!("{v:.1} dB"),
        "ms" if v >= 10.0 => format!("{v:.0} ms"),
        "ms" => format!("{v:.1} ms"),
        ":1" => format!("{v:.1}:1"),
        "Hz" if v >= 1000.0 => format!("{:.2} kHz", v / 1000.0),
        "Hz" => format!("{v:.0} Hz"),
        "%" => format!("{:.0}%", v * 100.0),
        "" => format!("{v:.2}"),
        unit => format!("{v:.2} {unit}"),
    }
}

/// What reading the revisions asks for: whose chains to read again, and
/// whose Signal is gone.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stale {
    pub reread: Vec<String>,
    pub gone: Vec<String>,
}

/// Which chains are stale, given those held (track, revision) and the
/// feed's (track, revision) now.
#[must_use]
pub fn stale<'a>(held: impl IntoIterator<Item = (&'a str, u64)>, now: &[(String, u64)]) -> Stale {
    let held: HashMap<&str, u64> = held.into_iter().collect();
    let reread = now
        .iter()
        .filter(|(track, rev)| held.get(track.as_str()) != Some(rev))
        .map(|(track, _)| track.clone())
        .collect();
    let gone = held
        .keys()
        .filter(|track| !now.iter().any(|(t, _)| t == *track))
        .map(|t| (*t).to_owned())
        .collect();
    Stale { reread, gone }
}

/// One track's Signal FX, as its strip holds it.
pub struct Strip {
    pub chain: Chain,
    pub fold: Fold,
    /// Its meters as last read, by meter index.
    pub meters: Vec<MeterValue>,
    /// Its rows drawn, and what they were drawn at: a strip draws this
    /// again only when its chain, fold, box or scroll moved.
    pub(super) picture: Option<(Picture, anyrender::Scene)>,
}

/// What a strip's rows were drawn at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picture {
    pub revision: u64,
    pub fold: Fold,
    pub at: (u64, u64, u64, u64),
}

impl Picture {
    #[must_use]
    pub fn of(strip: &Strip, at: Panel) -> Self {
        Self {
            revision: strip.chain.revision,
            fold: strip.fold,
            at: (
                at.x.to_bits(),
                at.y.to_bits(),
                at.width.to_bits(),
                at.height.to_bits(),
            ),
        }
    }
}

impl Strip {
    /// A meter's value now, by its index.
    #[must_use]
    pub fn meter(&self, index: u32) -> Option<f32> {
        self.meters
            .iter()
            .find(|m| m.index == index)
            .map(|m| m.value)
    }
}

/// What following the feed changed.
#[derive(Debug, Default)]
pub struct Followed {
    /// Tracks whose chain was read again (or for the first time).
    pub read: Vec<String>,
    pub gone: Vec<String>,
    /// Tracks whose chain could not be read, and why.
    pub failed: Vec<(String, SourceError)>,
}

impl Followed {
    #[must_use]
    pub const fn changed(&self) -> bool {
        !self.read.is_empty() || !self.gone.is_empty()
    }
}

/// Every Signal FX the mixer shows, kept current from a source.
pub struct Inserts {
    source: Option<Arc<dyn SignalSource>>,
    strips: HashMap<String, Strip>,
    /// The revision each track's chain could not be read at: not asked
    /// again until the revision moves.
    failed: HashMap<String, u64>,
    /// The `values_revision` each strip's chain was read at: read again
    /// when the feed's moves, for the plugin's own value text.
    values_seen: HashMap<String, u64>,
    meters_read: Option<Instant>,
    /// Moves whenever a chain or a fold does: the mixer's rows are stale.
    generation: u64,
}

impl Default for Inserts {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Inserts {
    /// Over `source`; none, and no track shows Signal's blocks.
    #[must_use]
    pub fn new(source: Option<Arc<dyn SignalSource>>) -> Self {
        Self {
            source,
            strips: HashMap::new(),
            failed: HashMap::new(),
            values_seen: HashMap::new(),
            meters_read: None,
            generation: 0,
        }
    }

    /// Over the source the engine installed ([`super::install`]).
    #[must_use]
    pub fn installed() -> Self {
        Self::new(super::installed())
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn has_source(&self) -> bool {
        self.source.is_some()
    }

    /// `track`'s Signal FX, if it has one.
    #[must_use]
    pub fn strip(&self, track: &TrackId) -> Option<&Strip> {
        self.strips.get(track)
    }

    /// `track`'s Signal FX, to draw (its picture is cached in it).
    pub fn strip_mut(&mut self, track: &TrackId) -> Option<&mut Strip> {
        self.strips.get_mut(track)
    }

    /// Read again every chain whose revision moved, and drop the strips
    /// whose Signal is gone. Cheap when nothing moved: one feed read.
    pub fn follow(&mut self) -> Followed {
        let Some(source) = self.source.clone() else {
            return Followed::default();
        };
        let now = source.signal_revisions();
        let values = source.signal_values_revisions();
        let mut plan = stale(self.held(), &now);
        plan.reread.extend(self.values_moved(&values, &plan.reread));
        let mut followed = Followed::default();
        for track in plan.gone {
            self.failed.remove(&track);
            self.values_seen.remove(&track);
            if self.strips.remove(&track).is_some() {
                followed.gone.push(track);
            }
        }
        for track in plan.reread {
            match source.signal_chain(&track) {
                Ok(chain) => {
                    self.failed.remove(&track);
                    if let Some((_, v)) = values.iter().find(|(t, _)| *t == track) {
                        self.values_seen.insert(track.clone(), *v);
                    }
                    self.take_chain(&track, chain);
                    followed.read.push(track);
                }
                Err(e) => {
                    let at = now.iter().find(|(t, _)| *t == track).map_or(0, |(_, r)| *r);
                    self.failed.insert(track.clone(), at);
                    followed.failed.push((track, e));
                }
            }
        }
        if followed.changed() {
            self.bump();
        }
        followed
    }

    /// The shown strips whose values moved in the feed since their chain
    /// was read (and not already in `planned`). A strip read before the
    /// feed carried values is caught up once.
    fn values_moved(&self, values: &[(String, u64)], planned: &[String]) -> Vec<String> {
        values
            .iter()
            .filter(|(track, v)| {
                self.strips.contains_key(track)
                    && !planned.contains(track)
                    && self.values_seen.get(track) != Some(v)
            })
            .map(|(track, _)| track.clone())
            .collect()
    }

    /// Each track's revision as held: its chain's, or the one its read
    /// failed at.
    fn held(&self) -> impl Iterator<Item = (&str, u64)> {
        self.strips
            .iter()
            .map(|(t, s)| (t.as_str(), s.chain.revision))
            .chain(self.failed.iter().map(|(t, r)| (t.as_str(), *r)))
    }

    /// Whether a painter that last drew at `seen` (a [`Inserts::generation`])
    /// should draw again: a fold or chain changed, a revision moved in the
    /// feed, or the meters are due.
    #[must_use]
    pub fn due(&self, now: Instant, seen: u64) -> bool {
        if self.generation != seen {
            return true;
        }
        let Some(source) = &self.source else {
            return false;
        };
        let meters_due = !self.strips.is_empty()
            && self
                .meters_read
                .is_none_or(|at| now.duration_since(at) >= Duration::from_secs(1) / METER_HZ);
        meters_due || {
            let plan = stale(self.held(), &source.signal_revisions());
            !plan.reread.is_empty()
                || !plan.gone.is_empty()
                || !self
                    .values_moved(&source.signal_values_revisions(), &plan.reread)
                    .is_empty()
        }
    }

    /// Show `phase`'s make-up at `db` while a drag moves it, before it is
    /// sent; the next chain read replaces it with what the plugin says.
    pub fn preview_makeup(&mut self, track: &TrackId, phase: &PhaseId, db: f64) {
        let Some(strip) = self.strips.get_mut(track) else {
            return;
        };
        if let Some(p) = strip.chain.phases.iter_mut().find(|p| &p.id == phase) {
            p.makeup_db = db;
            strip.picture = None;
            self.bump();
        }
    }

    /// Read the meters if [`METER_HZ`] says it is time; `true` when read.
    pub fn read_meters(&mut self, now: Instant) -> bool {
        let Some(source) = self.source.clone() else {
            return false;
        };
        let period = Duration::from_secs(1) / METER_HZ;
        if self
            .meters_read
            .is_some_and(|at| now.duration_since(at) < period)
        {
            return false;
        }
        self.meters_read = Some(now);
        for (track, strip) in &mut self.strips {
            strip.meters = source.signal_meters(track);
        }
        true
    }

    /// Open or shut `track`'s Signal FX; `None` flips it. `false` when
    /// the track has none.
    pub fn set_fold(&mut self, track: &TrackId, fold: Option<Fold>) -> bool {
        let Some(strip) = self.strips.get_mut(track) else {
            return false;
        };
        strip.fold = fold.unwrap_or_else(|| strip.fold.flipped());
        self.bump();
        true
    }

    /// A structural edit, through `SIGNAL_RUN`; the chain it answers is
    /// the one shown.
    ///
    /// # Errors
    /// No source, or the plugin refused.
    pub fn run(&mut self, track: &TrackId, edit: &Edit) -> Result<(), SourceError> {
        let source = self
            .source
            .clone()
            .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?;
        let (action, args) = edit.action();
        let chain = source.signal_run(track, action, &args)?;
        self.take_chain(track, chain);
        self.bump();
        Ok(())
    }

    /// A parameter's value, through `FX_SET` on its CLAP slot. Shown at
    /// once; the plugin's own text comes with the next chain read.
    ///
    /// # Errors
    /// No source, the parameter is not exposed, or the engine refused.
    pub fn set_param(
        &mut self,
        track: &TrackId,
        block: &BlockId,
        param: &str,
        value: &ParamValue,
    ) -> Result<(), SourceError> {
        let source = self
            .source
            .clone()
            .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?;
        let strip = self
            .strips
            .get_mut(track)
            .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?;
        let target = strip
            .chain
            .phases
            .iter_mut()
            .flat_map(|p| &mut p.blocks)
            .find(|b| &b.id == block)
            .and_then(|b| b.params.iter_mut().find(|p| p.id == param))
            .ok_or_else(|| {
                SourceError::Refused(format!("no parameter {param} on {}", block.as_str()))
            })?;
        let clap_id = target.clap_id.clone().ok_or_else(|| {
            SourceError::Refused(format!("{} is not exposed to the host", target.name))
        })?;
        source.fx_set(track, &clap_id, value)?;
        if let ParamValue::Normalized(n) = value {
            target.normalized = n.clamp(0.0, 1.0);
            target.value = target.min + (target.max - target.min) * target.normalized;
            target.text = value_text(target);
        }
        strip.picture = None;
        Ok(())
    }

    /// `SIGNAL_ALGORITHMS {kind}` on `track`'s Signal FX.
    ///
    /// # Errors
    /// No source, or the source could not list them.
    pub fn algorithms(
        &self,
        track: &TrackId,
        kind: &str,
    ) -> Result<Vec<super::source::Algorithm>, SourceError> {
        self.source
            .as_ref()
            .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?
            .signal_algorithms(track, kind)
    }

    /// Read `track`'s chain now, whatever its revision: a closeup opened
    /// on a value the plugin has since rewritten.
    ///
    /// # Errors
    /// No source, or no Signal on the track.
    pub fn reread(&mut self, track: &TrackId) -> Result<(), SourceError> {
        let source = self
            .source
            .clone()
            .ok_or_else(|| SourceError::NoSignal(track.to_owned()))?;
        let chain = source.signal_chain(track)?;
        self.take_chain(track, chain);
        self.bump();
        Ok(())
    }

    fn take_chain(&mut self, track: &TrackId, chain: Chain) {
        match self.strips.get_mut(track) {
            Some(strip) => {
                strip.chain = chain;
                strip.picture = None;
            }
            None => {
                self.strips.insert(
                    track.to_owned(),
                    Strip {
                        chain,
                        fold: Fold::Open,
                        meters: Vec::new(),
                        picture: None,
                    },
                );
            }
        }
    }

    const fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}
