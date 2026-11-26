//! Mix variants: several takes on one mix decision (four kick alignments,
//! three snare compressor settings), made by the agent mixing through the
//! Action List, auditioned by the owner while the song plays — blind if
//! they like — rated or picked, and the winner kept.
//!
//! A **group** (`MIX_VARIANTS_CREATE`) has a scope (tracks, each with the
//! tracks inside it; none: the whole mix and the master) and the
//! categories it varies (`only`, as MIX_SNAPSHOT's). A **variant**
//! (`MIX_VARIANT_ADD`) is a mix snapshot of that scope: the mix as it
//! stands, or a base (a snapshot, another variant, `current`) with a patch
//! of changes (`set`) applied to the snapshot alone — the live mix is not
//! touched to make one.
//!
//! **Auditioning** (`MIX_VARIANT_SWITCH`, `_NEXT`, `_PREV`) puts a variant
//! on the live mix as MIX_RESTORE would — only what differs, a plugin's
//! state loaded only when it differs, parameters and faders set in place —
//! but as no undo step: the first switch keeps the mix as it was (the
//! group's **base**), and only `MIX_VARIANT_KEEP` makes an undo step: the
//! base put back silently, then the winner put on as one step, so undoing
//! it returns to the mix before the audition. `MIX_VARIANTS_END` without a
//! keep puts the base back.
//!
//! **Loudness match** (`MIX_VARIANTS_MATCH`) renders a span offline once
//! per variant (MIX_MEASURE's render) and measures the scope's output (its
//! roots post-fader, summed; the master when unscoped): each variant gets
//! the trim that brings it down to the quietest one's integrated loudness,
//! applied to the roots' faders (or the master) only while auditioning —
//! never kept. What a variant sends to a bus outside the scope is not in
//! the measurement.
//!
//! **Blind** (`MIX_VARIANTS_BLIND`): the variants are shuffled behind
//! labels A, B, C…, and every read (`MIX_VARIANTS_STATE`, a switch's
//! answer) gives labels only — no name, note, parent, measurements,
//! loudness or trim — until revealed (`reveal: true`, blind turned off, or
//! a keep). The Action List cannot tell the owner's window from the agent,
//! so the rule is one for all: nobody reads the mapping while it is hidden,
//! and revealing is explicit and seen by everyone. The agent does not need
//! names to act: it rates, switches and keeps by label. Next and previous
//! go in label order.
//!
//! **Votes** (`MIX_VARIANT_RATE`, `MIX_VARIANT_PICK`): stars (0–5), one
//! pick per group, comments.
//!
//! Kept in the song's ext state (section `fts.mix`, beside the snapshots):
//! the group (`variants:<group>`, small: rewritten on every switch), each
//! variant's mix (`variant:<group>/<name>`) and the base while auditioning
//! (`variants_base:<group>`). None of it is an undo step. A variant is
//! named `group:name` wherever a snapshot is (MIX_RESTORE, MIX_DIFF).
//!
//! Later (not built): a variant per song section, written as automation
//! (Snapshooter's idea); a missing track substituted by name (Track
//! Snapshot's); a variants panel in mixer-view.

use std::collections::HashSet;

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ProjectContext, TrackRef, Tracks};
use daw_standalone::sync::Standalone;
use facet::Facet;

use crate::actions::{meta, resolve, song};
use crate::mix_sends::{db, gain};
use crate::mix_snapshot::{FxMix, Mix, Only, Put};

const SECTION: &str = "fts.mix";
/// The song's variant groups, a name a line.
const GROUPS: &str = "variant_groups";
/// The group the bindable actions (next, previous, pick) act on.
const ACTIVE: &str = "variants_active";

// ---------------------------------------------------------------- actions

static CREATE: ActionMeta = meta(
    "MIX_VARIANTS_CREATE",
    "mix_variants_create",
    "Start mix variants",
    "Start a group of variants of one mix decision, `group` (no `:` or `/`): `scope` (tracks — paths, names or GUIDs — each with the tracks inside it; none: the whole mix and the master) and `only` (the categories it varies: volume, pan, mute, solo, polarity, sends, fx-enabled, fx-params, fx-state; `fx` for the three; none: all) hold for every variant, on capture and on switch. `note` says what is being decided. A group of that name is replaced (`again: true`) or refused. It becomes the active group (what MIX_VARIANT_NEXT/PREV/PICK act on). Not an undo step.",
    "Mix",
    "",
);
static ADD: ActionMeta = meta(
    "MIX_VARIANT_ADD",
    "mix_variant_add",
    "Add a mix variant",
    "Add variant `name` to `group` (default: the active group), the group's scope and categories only. With neither `from` nor `set`: the mix as it stands. `from` (a snapshot, a variant of this group, `group:variant`, or `current`) is the base; `set` is a list of changes made to that copy alone, never to the live mix: each `{track, …}` sets the track's `volume_db`, `nudge_db` (relative), `pan`, `mute`, `solo`, `phase`; with `send` (its destination track; `nth` among several) the send's `level_db`/`nudge_db`, `pan`, `mute`; with `fx` (a name or part of one, or a GUID, on that track) its `on` and `params` (`id=value`). `note` and `measurements` (any text: what the agent measured) ride along. The parent is `from` (or the song's head). A variant of that name is replaced. Not an undo step.",
    "Mix",
    "",
);
static SWITCH: ActionMeta = meta(
    "MIX_VARIANT_SWITCH",
    "mix_variant_switch",
    "Switch mix variant",
    "Put a variant of `group` (default: the active group) on the live mix while it plays: by `name`, `label` (A, B, …), `index` (0-based, label order) or `next`/`prev` (label order, wrapping). Only what differs is set (faders, pans and parameters in place; a plugin's state only when it differs); the group's loudness trim is applied when matched. Not an undo step: the first switch remembers the mix as it was, MIX_VARIANT_KEEP or MIX_VARIANTS_END ends the audition. Says the label (the name too unless blind).",
    "Mix",
    "",
);
static NEXT: ActionMeta = meta(
    "MIX_VARIANT_NEXT",
    "mix_variant_next",
    "Next mix variant",
    "Switch the active variant group to its next variant (label order, wrapping), as MIX_VARIANT_SWITCH `next`. Bindable.",
    "Mix",
    "",
);
static PREV: ActionMeta = meta(
    "MIX_VARIANT_PREV",
    "mix_variant_prev",
    "Previous mix variant",
    "Switch the active variant group to its previous variant (label order, wrapping), as MIX_VARIANT_SWITCH `prev`. Bindable.",
    "Mix",
    "",
);
static PICK: ActionMeta = meta(
    "MIX_VARIANT_PICK",
    "mix_variant_pick",
    "Pick this mix variant",
    "Pick the variant of the active group that is playing (the group's one pick, as MIX_VARIANT_RATE `pick: true`). Bindable.",
    "Mix",
    "",
);
static BLIND: ActionMeta = meta(
    "MIX_VARIANTS_BLIND",
    "mix_variants_blind",
    "Blind mix variants",
    "`on: true` shuffles `group`'s variants (default: the active group) behind labels A, B, C… and hides every name, note, parent, measurement, loudness and trim from every read until revealed; `on: false` ends it, `reveal: true` shows the mapping and keeps the labels. `seed` makes the shuffle repeatable. Not an undo step.",
    "Mix",
    "",
);
static RATE: ActionMeta = meta(
    "MIX_VARIANT_RATE",
    "mix_variant_rate",
    "Rate a mix variant",
    "Vote on a variant of `group` (default: the active group), by `name` or `label` (default: the one playing): `stars` (0–5), `pick: true` (the group's one pick; `false` takes it back), `comment` (added). Not an undo step. Says the group as MIX_VARIANTS_STATE.",
    "Mix",
    "",
);
static STATE: ActionMeta = meta(
    "MIX_VARIANTS_STATE",
    "mix_variants_state",
    "Mix variants",
    "`group` (default: the active group): its note, scope, categories, whether blind, matched, auditioning, the variant playing, what was kept, and every variant in label order — label, stars, pick, comments, and (unless blind and not revealed) name, note, parent, measurements, loudness and trim. `groups` lists the song's groups. Read-only.",
    "Mix",
    "",
);
static MATCH: ActionMeta = meta(
    "MIX_VARIANTS_MATCH",
    "mix_variants_match",
    "Loudness-match mix variants",
    "Render `from`..`to` (as MIX_MEASURE: seconds, bars, markers) offline once per variant of `group` (default: the active group) and measure the scope's output (its roots post-fader, summed; the master when unscoped): each gets the trim that brings it to the quietest one's integrated loudness, applied only while auditioning. `off: true` stops applying the trims. The live mix is put back as it was. Says each one's loudness and trim (labels only when blind).",
    "Mix",
    "",
);
static KEEP: ActionMeta = meta(
    "MIX_VARIANT_KEEP",
    "mix_variant_keep",
    "Keep a mix variant",
    "Keep a variant of `group` (default: the active group) — by `name` or `label`; default the pick, else the one playing — as one undo step: the mix before the audition is put back silently, then the winner put on (its scope and categories only, without the loudness trim), so undo returns to the mix before the audition. Recorded on the group; reveals a blind group; the song's head becomes `group:name`. Says what changed.",
    "Mix",
    "",
);
static END: ActionMeta = meta(
    "MIX_VARIANTS_END",
    "mix_variants_end",
    "End mix variants",
    "End `group`'s audition (default: the active group): the mix before it is put back unless a variant was kept. `drop: true` deletes the group and its variants from the song. Not an undo step.",
    "Mix",
    "",
);

// ---------------------------------------------------------------- the model

/// One variant, as its group lists it (its mix is kept beside it).
#[derive(Facet, Clone, Debug, PartialEq, Default)]
pub struct Variant {
    pub name: String,
    #[facet(default)]
    pub note: String,
    #[facet(default)]
    pub measurements: String,
    #[facet(default)]
    pub parent: String,
    /// Integrated loudness of the scope's output, LUFS, when matched.
    #[facet(default)]
    pub lufs: Option<f64>,
    /// dB at the scope's output while auditioning, when matched.
    #[facet(default)]
    pub trim_db: f64,
    #[facet(default)]
    pub stars: u32,
    #[facet(default)]
    pub picked: bool,
    #[facet(default)]
    pub comments: Vec<String>,
}

/// A group of variants of one decision.
#[derive(Facet, Clone, Debug, PartialEq, Default)]
pub struct Group {
    pub name: String,
    #[facet(default)]
    pub note: String,
    /// The tracks the scope was named by, GUIDs (none: the whole mix).
    #[facet(default)]
    pub roots: Vec<String>,
    /// Every track in scope, GUIDs.
    #[facet(default)]
    pub scope: Vec<String>,
    #[facet(default)]
    pub only: Vec<String>,
    #[facet(default)]
    pub variants: Vec<Variant>,
    #[facet(default)]
    pub blind: bool,
    /// When blind: label `i` is variant `order[i]`.
    #[facet(default)]
    pub order: Vec<u32>,
    #[facet(default)]
    pub revealed: bool,
    /// Whether the loudness trims are applied while auditioning.
    #[facet(default)]
    pub matched: bool,
    /// The variant playing, while auditioning.
    #[facet(default)]
    pub current: Option<u32>,
    #[facet(default)]
    pub auditioning: bool,
    /// The trim on the scope's output now, dB.
    #[facet(default)]
    pub applied_trim_db: f64,
    #[facet(default)]
    pub kept: String,
    #[facet(default)]
    pub ended: bool,
    #[facet(default)]
    pub created: String,
}

/// Which variant: what a switch, a rating or a keep names.
#[derive(Debug, Default, Clone)]
pub struct Pick {
    pub name: Option<String>,
    pub label: Option<String>,
    pub index: Option<u32>,
    pub next: bool,
    pub prev: bool,
}

/// Label `i`: A … Z, AA, AB, …
pub fn label(i: usize) -> String {
    let mut n = i;
    let mut out = Vec::new();
    loop {
        out.push(b'A' + u8::try_from(n % 26).unwrap_or(0));
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The label index `word` names (`A` → 0, `ab` → 27).
fn label_index(word: &str) -> Option<usize> {
    let w = word.trim();
    if w.is_empty() || !w.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut n = 0usize;
    for b in w.bytes() {
        n = n * 26 + usize::from(b.to_ascii_uppercase() - b'A') + 1;
    }
    Some(n - 1)
}

/// A shuffle of `0..n` from `seed` (Fisher–Yates on a xorshift).
pub fn shuffle(n: usize, seed: u64) -> Vec<u32> {
    let mut order: Vec<u32> = (0..u32::try_from(n).unwrap_or(u32::MAX)).collect();
    let mut x = seed | 1;
    for i in (1..order.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let j = usize::try_from(x % (i as u64 + 1)).unwrap_or(0);
        order.swap(i, j);
    }
    order
}

impl Group {
    /// Whether names are hidden from reads.
    pub const fn hidden(&self) -> bool {
        self.blind && !self.revealed
    }

    /// The variant at label `l`.
    fn at_label(&self, l: usize) -> Option<usize> {
        if self.blind {
            self.order
                .get(l)
                .and_then(|&i| usize::try_from(i).ok())
                .filter(|&i| i < self.variants.len())
        } else {
            (l < self.variants.len()).then_some(l)
        }
    }

    /// Variant `i`'s label index.
    pub fn label_of(&self, i: usize) -> usize {
        if self.blind {
            self.order
                .iter()
                .position(|&o| usize::try_from(o).ok() == Some(i))
                .unwrap_or(i)
        } else {
            i
        }
    }

    /// The variants' indexes in label order.
    fn in_label_order(&self) -> Vec<usize> {
        (0..self.variants.len())
            .filter_map(|l| self.at_label(l))
            .collect()
    }

    fn by_name(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        self.variants
            .iter()
            .position(|v| v.name == name)
            .or_else(|| {
                self.variants
                    .iter()
                    .position(|v| v.name.eq_ignore_ascii_case(name))
            })
    }

    /// The variant `pick` names.
    ///
    /// # Errors
    /// It names none, or the group has none.
    pub fn resolve(&self, pick: &Pick) -> Result<usize, String> {
        let n = self.variants.len();
        if n == 0 {
            return Err(format!("group {:?} has no variants yet", self.name));
        }
        if let Some(name) = &pick.name {
            return self.by_name(name).ok_or_else(|| {
                if self.hidden() {
                    format!("no variant {name:?} in {:?}", self.name)
                } else {
                    format!(
                        "no variant {name:?} in {:?} — it has {:?}",
                        self.name,
                        self.variants.iter().map(|v| &v.name).collect::<Vec<_>>()
                    )
                }
            });
        }
        if let Some(word) = &pick.label {
            let l = label_index(word).ok_or_else(|| format!("{word:?} is not a label"))?;
            return self
                .at_label(l)
                .ok_or_else(|| format!("no variant labelled {word:?} (A…{})", label(n - 1)));
        }
        if let Some(index) = pick.index {
            let l = usize::try_from(index).unwrap_or(usize::MAX);
            return self
                .at_label(l)
                .ok_or_else(|| format!("no variant {index} (0…{})", n - 1));
        }
        if pick.next || pick.prev {
            let at = self
                .current
                .and_then(|c| usize::try_from(c).ok())
                .map(|c| self.label_of(c));
            let l = match (at, pick.next) {
                (None, true) => 0,
                (None, false) => n - 1,
                (Some(l), true) => (l + 1) % n,
                (Some(l), false) => (l + n - 1) % n,
            };
            return self.at_label(l).ok_or_else(|| "no such variant".to_owned());
        }
        Err("say which variant: `name`, `label`, `index`, `next` or `prev`".to_owned())
    }

    /// Add `v`, or replace the variant of its name; its index.
    pub fn add(&mut self, v: Variant) -> usize {
        if let Some(i) = self.by_name(&v.name) {
            let votes = (
                self.variants[i].stars,
                self.variants[i].picked,
                std::mem::take(&mut self.variants[i].comments),
            );
            self.variants[i] = v;
            (
                self.variants[i].stars,
                self.variants[i].picked,
                self.variants[i].comments,
            ) = votes;
            return i;
        }
        self.variants.push(v);
        let i = self.variants.len() - 1;
        if self.blind {
            self.order.push(u32::try_from(i).unwrap_or(u32::MAX));
        }
        i
    }

    /// Blind on (shuffled by `seed`) or off.
    pub fn set_blind(&mut self, on: bool, seed: u64) {
        self.blind = on;
        self.revealed = !on;
        self.order = if on {
            shuffle(self.variants.len(), seed)
        } else {
            Vec::new()
        };
    }

    /// Vote on variant `i`.
    ///
    /// # Errors
    /// More than five stars.
    pub fn rate(
        &mut self,
        i: usize,
        stars: Option<u32>,
        pick: Option<bool>,
        comment: Option<&str>,
    ) -> Result<(), String> {
        if let Some(s) = stars {
            if s > 5 {
                return Err(format!("{s} stars: 0 to 5"));
            }
            self.variants[i].stars = s;
        }
        if let Some(p) = pick {
            if p {
                for v in &mut self.variants {
                    v.picked = false;
                }
            }
            self.variants[i].picked = p;
        }
        if let Some(c) = comment.map(str::trim).filter(|c| !c.is_empty()) {
            self.variants[i].comments.push(c.to_owned());
        }
        Ok(())
    }

    /// The picked variant.
    pub fn picked(&self) -> Option<usize> {
        self.variants.iter().position(|v| v.picked)
    }

    /// What a read says of it (`paths`: the scope's track paths).
    pub fn view(&self, paths: Vec<String>) -> StateRow {
        let hide = self.hidden();
        let current = self
            .current
            .and_then(|c| usize::try_from(c).ok())
            .filter(|_| self.auditioning);
        StateRow {
            group: self.name.clone(),
            note: self.note.clone(),
            scope: paths,
            only: self.only.clone(),
            blind: self.blind,
            revealed: !hide,
            matched: self.matched,
            auditioning: self.auditioning,
            current: current.map(|c| label(self.label_of(c))).unwrap_or_default(),
            kept: self.kept.clone(),
            ended: self.ended,
            variants: self
                .in_label_order()
                .into_iter()
                .enumerate()
                .map(|(l, i)| {
                    let v = &self.variants[i];
                    let shown = |s: &str| if hide { String::new() } else { s.to_owned() };
                    VariantRow {
                        label: label(l),
                        name: shown(&v.name),
                        note: shown(&v.note),
                        parent: shown(&v.parent),
                        measurements: shown(&v.measurements),
                        lufs: v.lufs.filter(|_| !hide),
                        trim_db: if hide { 0.0 } else { v.trim_db },
                        stars: v.stars,
                        picked: v.picked,
                        comments: v.comments.clone(),
                        playing: current == Some(i),
                    }
                })
                .collect(),
            groups: Vec::new(),
            active: String::new(),
        }
    }
}

/// A variant, as a read says it.
#[derive(Facet, Debug, Clone)]
pub struct VariantRow {
    pub label: String,
    /// Empty while blind and not revealed (so are note, parent,
    /// measurements, loudness and trim).
    pub name: String,
    pub note: String,
    pub parent: String,
    pub measurements: String,
    pub lufs: Option<f64>,
    pub trim_db: f64,
    pub stars: u32,
    pub picked: bool,
    pub comments: Vec<String>,
    pub playing: bool,
}

/// A group, as a read says it.
#[derive(Facet, Debug, Clone)]
pub struct StateRow {
    pub group: String,
    pub note: String,
    pub scope: Vec<String>,
    pub only: Vec<String>,
    pub blind: bool,
    /// Whether names show (not blind, or revealed).
    pub revealed: bool,
    pub matched: bool,
    pub auditioning: bool,
    /// The label playing, while auditioning.
    pub current: String,
    pub kept: String,
    pub ended: bool,
    pub variants: Vec<VariantRow>,
    /// The song's groups.
    pub groups: Vec<String>,
    /// The active group.
    pub active: String,
}

/// A switch made.
#[derive(Facet, Debug)]
pub struct SwitchRow {
    pub group: String,
    pub label: String,
    /// Empty while blind.
    pub name: String,
    pub changed: u32,
    pub states_loaded: u32,
    /// Whether the mix before the audition was kept by this switch (the
    /// first of an audition).
    pub started: bool,
    pub ms: f64,
}

/// A variant added.
#[derive(Facet, Debug)]
pub struct AddRow {
    pub group: String,
    pub name: String,
    pub label: String,
    pub parent: String,
    /// What `set` changed in the copy.
    pub set: Vec<String>,
    pub tracks: u32,
    pub fx: u32,
    pub ms: f64,
}

/// A variant kept.
#[derive(Facet, Debug)]
pub struct KeepRow {
    pub group: String,
    pub name: String,
    pub label: String,
    pub changed: Vec<String>,
    pub missing: Vec<String>,
    pub states_loaded: u32,
    pub ms: f64,
}

/// Variants measured.
#[derive(Facet, Debug)]
pub struct MatchRow {
    pub group: String,
    pub matched: bool,
    /// `label: lufs LUFS, trim dB` (names too unless blind).
    pub variants: Vec<String>,
    pub target_lufs: Option<f64>,
    pub ms: f64,
}

// ---------------------------------------------------------------- patches

/// A change to a variant's copy of the mix.
#[derive(Facet, Clone, Debug, Default)]
pub struct SetOp {
    pub track: String,
    pub send: Option<String>,
    pub nth: Option<u32>,
    pub fx: Option<String>,
    pub volume_db: Option<f64>,
    pub nudge_db: Option<f64>,
    pub level_db: Option<f64>,
    pub pan: Option<f64>,
    pub mute: Option<bool>,
    pub solo: Option<bool>,
    pub phase: Option<bool>,
    pub on: Option<bool>,
    pub params: Option<Vec<String>>,
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Apply `op` to `mix` (its track resolved to `track`, its send's
/// destination to `to`): what it changed, said.
///
/// # Errors
/// The track, send or FX is not in the mix, or the change does not fit
/// what it names.
pub fn apply_set(
    mix: &mut Mix,
    op: &SetOp,
    track: &str,
    to: Option<&str>,
) -> Result<Vec<String>, String> {
    let path = mix
        .tracks
        .iter()
        .find(|t| t.guid == track)
        .map(|t| t.path.clone())
        .ok_or_else(|| format!("{} is not in the variant's scope", op.track))?;
    let mut said = Vec::new();
    if op.send.is_some() && op.fx.is_some() {
        return Err(format!("{path}: a change is to a send or an FX, not both"));
    }
    if let Some(to) = to {
        let nth = op.nth.unwrap_or(0);
        let to_path = mix
            .tracks
            .iter()
            .find(|t| t.guid == to)
            .map_or_else(|| op.send.clone().unwrap_or_default(), |t| t.path.clone());
        let s = mix
            .sends
            .iter_mut()
            .find(|s| s.from == track && s.to == to && s.nth == nth)
            .ok_or_else(|| {
                format!("{path} has no send {nth} to {to_path} (a variant changes sends, it does not make them: make it live and add the variant from the mix)")
            })?;
        let what = format!("send {path} → {to_path}");
        if let Some(l) = op.level_db.or(op.volume_db) {
            s.level_db = r2(l);
            said.push(format!("{what} level {} dB", s.level_db));
        }
        if let Some(n) = op.nudge_db {
            s.level_db = r2(s.level_db + n);
            said.push(format!("{what} level {} dB", s.level_db));
        }
        if let Some(p) = op.pan {
            s.pan = r2(p.clamp(-1.0, 1.0));
            said.push(format!("{what} pan {}", s.pan));
        }
        if let Some(m) = op.mute {
            s.mute = m;
            said.push(format!("{what} mute {m}"));
        }
        return Ok(said);
    }
    if let Some(word) = &op.fx {
        let fx = find_fx(&mut mix.fx, track, word, &path)?;
        let what = format!("fx {} on {path}", fx.name);
        if let Some(on) = op.on {
            fx.on = on;
            said.push(format!("{what} on {on}"));
        }
        for p in op.params.iter().flatten() {
            let (id, v) = p
                .split_once('=')
                .ok_or_else(|| format!("{p:?} is not `id=value`"))?;
            let id: u32 = id
                .trim()
                .parse()
                .map_err(|_| format!("{p:?}: the id is a number"))?;
            let v: f64 = v
                .trim()
                .parse()
                .map_err(|_| format!("{p:?}: the value is a number"))?;
            fx.params
                .retain(|q| q.split_once('=').is_none_or(|(i, _)| i.parse() != Ok(id)));
            fx.params.push(format!("{id}={v:.6}"));
            said.push(format!("{what} param {id} = {v}"));
        }
        fx.params.sort();
        return Ok(said);
    }
    let Some(t) = mix.tracks.iter_mut().find(|t| t.guid == track) else {
        return Err(format!("{path} is not in the variant's scope"));
    };
    if let Some(v) = op.volume_db.or(op.level_db) {
        t.volume_db = r2(v);
        said.push(format!("{path} volume {} dB", t.volume_db));
    }
    if let Some(n) = op.nudge_db {
        t.volume_db = r2(t.volume_db + n);
        said.push(format!("{path} volume {} dB", t.volume_db));
    }
    if let Some(p) = op.pan {
        t.pan = r2(p.clamp(-1.0, 1.0));
        said.push(format!("{path} pan {}", t.pan));
    }
    if let Some(m) = op.mute {
        t.mute = m;
        said.push(format!("{path} mute {m}"));
    }
    if let Some(s) = op.solo {
        t.solo = s;
        said.push(format!("{path} solo {s}"));
    }
    if let Some(p) = op.phase {
        t.phase = p;
        said.push(format!("{path} phase {p}"));
    }
    if op.on.is_some() || op.params.is_some() {
        return Err(format!("{path}: `on` and `params` need `fx`"));
    }
    Ok(said)
}

/// The FX on `track` that `word` names: its GUID, its name, or a part of
/// its name only one has.
fn find_fx<'a>(
    fx: &'a mut [FxMix],
    track: &str,
    word: &str,
    path: &str,
) -> Result<&'a mut FxMix, String> {
    let w = word.trim().to_ascii_lowercase();
    let on: Vec<usize> = (0..fx.len()).filter(|&i| fx[i].track == track).collect();
    let exact: Vec<usize> = on
        .iter()
        .copied()
        .filter(|&i| fx[i].guid == word.trim() || fx[i].name.to_ascii_lowercase() == w)
        .collect();
    let found = if exact.is_empty() {
        on.iter()
            .copied()
            .filter(|&i| fx[i].name.to_ascii_lowercase().contains(&w))
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [i] => Ok(&mut fx[*i]),
        [] => Err(format!(
            "no fx {word:?} on {path} — it has {:?}",
            on.iter().map(|&i| fx[i].name.clone()).collect::<Vec<_>>()
        )),
        _ => Err(format!(
            "{word:?} fits more than one fx on {path}: {:?} (give its GUID)",
            found
                .iter()
                .map(|&i| fx[i].name.clone())
                .collect::<Vec<_>>()
        )),
    }
}

/// `mix` cut to `scope` (GUIDs; empty: as it is) and `only`.
pub fn rescope(mut mix: Mix, scope: &[String], only: Only) -> Mix {
    if !scope.is_empty() {
        let s: HashSet<&str> = scope.iter().map(String::as_str).collect();
        mix.tracks.retain(|t| s.contains(t.guid.as_str()));
        mix.sends.retain(|x| s.contains(x.from.as_str()));
        mix.fx.retain(|f| s.contains(f.track.as_str()));
        mix.scope = scope.to_vec();
    }
    mix.only = Only::saved(&mix.only).and(only).words();
    mix
}

// ---------------------------------------------------------------- storage

fn group_key(group: &str) -> String {
    format!("variants:{group}")
}
fn mix_key(group: &str, name: &str) -> String {
    format!("variant:{group}/{name}")
}
fn base_key(group: &str) -> String {
    format!("variants_base:{group}")
}

fn get(daw: &Standalone, project: &ProjectContext, key: &str) -> Option<String> {
    daw_proto::ExtState::get_project(daw, project.clone(), SECTION, key)
}

fn set(daw: &Standalone, project: &ProjectContext, key: &str, text: &str) -> Result<(), String> {
    daw_proto::ExtState::set_project(daw, project.clone(), SECTION, key, text)
        .map_err(|e| e.to_string())
}

fn delete(daw: &Standalone, project: &ProjectContext, key: &str) {
    let _ = daw_proto::ExtState::delete_project(daw, project.clone(), SECTION, key);
}

/// The song's variant groups.
pub fn groups(daw: &Standalone, project: &ProjectContext) -> Vec<String> {
    get(daw, project, GROUPS)
        .map(|t| {
            t.lines()
                .map(str::to_owned)
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn active(daw: &Standalone, project: &ProjectContext) -> String {
    get(daw, project, ACTIVE).unwrap_or_default()
}

/// The group `name` names, or the active one.
fn group_name(
    daw: &Standalone,
    project: &ProjectContext,
    name: Option<&str>,
) -> Result<String, String> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => Ok(n.to_owned()),
        None => {
            let a = active(daw, project);
            if a.is_empty() {
                Err(format!(
                    "say which `group` (there is no active one) — the song has {:?}",
                    groups(daw, project)
                ))
            } else {
                Ok(a)
            }
        }
    }
}

pub fn load_group(
    daw: &Standalone,
    project: &ProjectContext,
    group: &str,
) -> Result<Group, String> {
    let text = get(daw, project, &group_key(group)).ok_or_else(|| {
        format!(
            "no variant group {group:?} — the song has {:?}",
            groups(daw, project)
        )
    })?;
    facet_json::from_str::<Group>(&text).map_err(|e| format!("variant group {group:?}: {e}"))
}

fn save_group(daw: &Standalone, project: &ProjectContext, g: &Group) -> Result<(), String> {
    let text = facet_json::to_string(g).map_err(|e| e.to_string())?;
    set(daw, project, &group_key(&g.name), &text)
}

fn save_mix(
    daw: &Standalone,
    project: &ProjectContext,
    key: &str,
    mix: &Mix,
) -> Result<(), String> {
    let text = facet_json::to_string(mix).map_err(|e| e.to_string())?;
    set(daw, project, key, &text)
}

/// Variant `name` of `group`'s mix.
///
/// # Errors
/// No such group or variant.
pub fn load_mix(
    daw: &Standalone,
    project: &ProjectContext,
    group: &str,
    name: &str,
) -> Result<Mix, String> {
    let g = load_group(daw, project, group)?;
    let i = g.resolve(&Pick {
        name: Some(name.to_owned()),
        ..Pick::default()
    })?;
    let text = get(daw, project, &mix_key(group, &g.variants[i].name))
        .ok_or_else(|| format!("variant {group}:{name} has lost its mix"))?;
    facet_json::from_str::<Mix>(&text).map_err(|e| format!("variant {group}:{name}: {e}"))
}

fn base_mix(daw: &Standalone, project: &ProjectContext, group: &str) -> Option<Mix> {
    get(daw, project, &base_key(group)).and_then(|t| facet_json::from_str::<Mix>(&t).ok())
}

fn seed_now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0x9e37_79b9, |d| {
            u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(1)
        })
}

fn paths_of(daw: &Standalone, project: &ProjectContext, guids: &[String]) -> Vec<String> {
    let paths = crate::mix_sends::paths(&Tracks::all(daw, project.clone()));
    guids
        .iter()
        .map(|g| paths.get(g).cloned().unwrap_or_else(|| g.clone()))
        .collect()
}

fn ms(began: web_time::Instant) -> f64 {
    r2(began.elapsed().as_secs_f64() * 1e3)
}

// ---------------------------------------------------------------- doing

#[derive(Facet, Default)]
pub struct CreateArg {
    pub group: String,
    pub scope: Option<Vec<String>>,
    pub only: Option<Vec<String>>,
    pub note: Option<String>,
    pub again: Option<bool>,
}

/// `MIX_VARIANTS_CREATE`.
pub fn create(
    daw: &Standalone,
    project: &ProjectContext,
    a: &CreateArg,
) -> Result<StateRow, String> {
    let span = tracing::info_span!("MIX_VARIANTS_CREATE", mix.scope = tracing::field::Empty);
    let _in = span.enter();
    let name = a.group.trim();
    if name.is_empty() || name.contains([':', '/', '\n']) || name.eq_ignore_ascii_case("current") {
        return Err(format!(
            "{name:?} is not a group name (no `:`, `/` or new line)"
        ));
    }
    let mut all = groups(daw, project);
    if all.iter().any(|g| g == name) {
        if !a.again.unwrap_or(false) {
            return Err(format!(
                "there is a variant group {name:?} already (`again: true` replaces it)"
            ));
        }
        end(daw, project, Some(name), true)?;
        all = groups(daw, project);
    }
    let only = Only::parse(a.only.as_deref())?;
    let (roots, scope) = crate::mix_snapshot::scope_of(daw, project, a.scope.as_deref())?;
    span.record("mix.scope", scope.len());
    let g = Group {
        name: name.to_owned(),
        note: a.note.clone().unwrap_or_default(),
        roots,
        scope,
        only: only.words(),
        created: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        ..Group::default()
    };
    save_group(daw, project, &g)?;
    all.push(name.to_owned());
    set(daw, project, GROUPS, &all.join("\n"))?;
    set(daw, project, ACTIVE, name)?;
    state(daw, project, Some(name))
}

#[derive(Facet, Default)]
pub struct AddArg {
    pub group: Option<String>,
    pub name: String,
    pub note: Option<String>,
    pub measurements: Option<String>,
    pub from: Option<String>,
    pub set: Option<Vec<SetOp>>,
}

/// `MIX_VARIANT_ADD`.
pub fn add(daw: &Standalone, project: &ProjectContext, a: &AddArg) -> Result<AddRow, String> {
    let span = tracing::info_span!(
        "MIX_VARIANT_ADD",
        mix.set = a.set.as_ref().map_or(0, Vec::len),
        mix.from = a.from.is_some(),
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let group = group_name(daw, project, a.group.as_deref())?;
    let mut g = load_group(daw, project, &group)?;
    let name = a.name.trim();
    if name.is_empty() || name.contains(['/', '\n']) {
        return Err(format!(
            "{name:?} is not a variant name (no `/` or new line)"
        ));
    }
    let song = crate::fx_actions::project_guid(project)?;
    let scope: Option<HashSet<String>> =
        (!g.scope.is_empty()).then(|| g.scope.iter().cloned().collect());
    let base = match a.from.as_deref().map(str::trim) {
        None => crate::mix_snapshot::take_scoped(daw, project, &song, name, scope.as_ref()),
        Some(w) if w.eq_ignore_ascii_case("current") => {
            crate::mix_snapshot::take_scoped(daw, project, &song, name, scope.as_ref())
        }
        Some(w) => match g.by_name(w) {
            Some(i) => load_mix(daw, project, &group, &g.variants[i].name)?,
            None => crate::mix_snapshot::load(daw, project, w)?,
        },
    };
    let mut mix = rescope(base, &g.scope, Only::saved(&g.only));
    mix.name = format!("{group}:{name}");
    mix.taken = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut said = Vec::new();
    for op in a.set.iter().flatten() {
        let track = resolve(daw, project, &op.track)?;
        let to = op
            .send
            .as_deref()
            .map(|s| resolve(daw, project, s))
            .transpose()?;
        said.extend(apply_set(&mut mix, op, &track, to.as_deref())?);
    }
    let parent = a
        .from
        .clone()
        .map(|f| {
            if g.by_name(&f).is_some() {
                format!("{group}:{}", f.trim())
            } else {
                f.trim().to_owned()
            }
        })
        .unwrap_or_else(|| crate::mix_snapshot::head(daw, project));
    mix.parent.clone_from(&parent);
    save_mix(daw, project, &mix_key(&group, name), &mix)?;
    let i = g.add(Variant {
        name: name.to_owned(),
        note: a.note.clone().unwrap_or_default(),
        measurements: a.measurements.clone().unwrap_or_default(),
        parent: parent.clone(),
        ..Variant::default()
    });
    g.ended = false;
    save_group(daw, project, &g)?;
    set(daw, project, ACTIVE, &group)?;
    span.record("mix.ms", ms(began));
    Ok(AddRow {
        group,
        name: name.to_owned(),
        label: label(g.label_of(i)),
        parent,
        set: said,
        tracks: u32::try_from(mix.tracks.len()).unwrap_or(u32::MAX),
        fx: u32::try_from(mix.fx.len()).unwrap_or(u32::MAX),
        ms: ms(began),
    })
}

/// Set the scope's output (the roots' faders; the master when unscoped)
/// to what `mix` has there plus `trim` dB: the loudness trim, while
/// auditioning. A fader `mix` does not hold keeps its own value (less the
/// trim already on it).
fn apply_trim(
    daw: &Standalone,
    project: &ProjectContext,
    g: &mut Group,
    mix: &Mix,
    trim: f64,
) -> Result<(), String> {
    if trim == 0.0 && g.applied_trim_db == 0.0 {
        return Ok(());
    }
    let volume = Only::saved(&mix.only).has(Only::VOLUME);
    if g.roots.is_empty() {
        let song = crate::fx_actions::project_guid(project)?;
        let now = daw
            .read_project(&song, |p| db(p.master_volume))
            .unwrap_or(0.0);
        let base = if volume && mix.scope.is_empty() {
            mix.master_db
        } else {
            now - g.applied_trim_db
        };
        daw.write_project(&song, |p| p.master_volume = gain(base + trim));
    } else {
        let tracks = Tracks::all(daw, project.clone());
        for root in &g.roots {
            let now = tracks
                .iter()
                .find(|t| &t.guid == root)
                .map_or(0.0, |t| db(t.volume));
            let base = mix
                .tracks
                .iter()
                .find(|t| &t.guid == root)
                .filter(|_| volume)
                .map_or(now - g.applied_trim_db, |t| t.volume_db);
            Tracks::set_volume(
                daw,
                project.clone(),
                TrackRef::Guid(root.clone()),
                gain(base + trim),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    g.applied_trim_db = trim;
    Ok(())
}

/// Start auditioning `g` (keep the mix as it is as its base), if not yet.
fn begin_audition(
    daw: &Standalone,
    project: &ProjectContext,
    g: &mut Group,
) -> Result<bool, String> {
    if g.auditioning {
        return Ok(false);
    }
    let song = crate::fx_actions::project_guid(project)?;
    let scope: Option<HashSet<String>> =
        (!g.scope.is_empty()).then(|| g.scope.iter().cloned().collect());
    let base = crate::mix_snapshot::take_scoped(
        daw,
        project,
        &song,
        "before the audition",
        scope.as_ref(),
    );
    save_mix(daw, project, &base_key(&g.name), &base)?;
    g.auditioning = true;
    g.applied_trim_db = 0.0;
    g.kept.clear();
    Ok(true)
}

/// End auditioning `g`: the base put back silently (no undo step).
fn end_audition(daw: &Standalone, project: &ProjectContext, g: &mut Group) -> Result<(), String> {
    if !g.auditioning {
        return Ok(());
    }
    if let Some(base) = base_mix(daw, project, &g.name) {
        crate::mix_snapshot::put_with(
            daw,
            project,
            "before the audition",
            &base,
            Put {
                undo: false,
                keep: false,
                only: Only::ALL,
            },
        )?;
    }
    delete(daw, project, &base_key(&g.name));
    g.auditioning = false;
    g.current = None;
    g.applied_trim_db = 0.0;
    Ok(())
}

/// Put variant `i` of `g` on the live mix, as no undo step.
fn play(
    daw: &Standalone,
    project: &ProjectContext,
    g: &mut Group,
    i: usize,
) -> Result<(u32, u32), String> {
    let mix = load_mix(daw, project, &g.name, &g.variants[i].name)?;
    let row = crate::mix_snapshot::put_with(
        daw,
        project,
        &mix.name,
        &mix,
        Put {
            undo: false,
            keep: false,
            only: Only::saved(&g.only),
        },
    )?;
    let trim = if g.matched {
        g.variants[i].trim_db
    } else {
        0.0
    };
    apply_trim(daw, project, g, &mix, trim)?;
    g.current = u32::try_from(i).ok();
    Ok((
        u32::try_from(row.changed.len()).unwrap_or(u32::MAX),
        row.states_loaded,
    ))
}

#[derive(Facet, Default)]
pub struct SwitchArg {
    pub group: Option<String>,
    pub name: Option<String>,
    pub label: Option<String>,
    pub index: Option<u32>,
    pub next: Option<bool>,
    pub prev: Option<bool>,
}

impl SwitchArg {
    fn pick(&self) -> Pick {
        Pick {
            name: self.name.clone(),
            label: self.label.clone(),
            index: self.index,
            next: self.next.unwrap_or(false),
            prev: self.prev.unwrap_or(false),
        }
    }
}

/// `MIX_VARIANT_SWITCH`.
pub fn switch(
    daw: &Standalone,
    project: &ProjectContext,
    a: &SwitchArg,
) -> Result<SwitchRow, String> {
    let span = tracing::info_span!(
        "MIX_VARIANT_SWITCH",
        mix.variant = tracing::field::Empty,
        mix.blind = tracing::field::Empty,
        mix.changed = tracing::field::Empty,
        mix.states = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let group = group_name(daw, project, a.group.as_deref())?;
    let mut g = load_group(daw, project, &group)?;
    let i = g.resolve(&a.pick())?;
    let started = begin_audition(daw, project, &mut g)?;
    let played = play(daw, project, &mut g, i);
    // Whatever was set, the group says what is playing.
    let saved = save_group(daw, project, &g);
    let (changed, states_loaded) = played?;
    saved?;
    set(daw, project, ACTIVE, &group)?;
    let l = g.label_of(i);
    span.record("mix.variant", l);
    span.record("mix.blind", g.hidden());
    span.record("mix.changed", changed);
    span.record("mix.states", states_loaded);
    span.record("mix.ms", ms(began));
    Ok(SwitchRow {
        label: label(l),
        name: if g.hidden() {
            String::new()
        } else {
            g.variants[i].name.clone()
        },
        group,
        changed,
        states_loaded,
        started,
        ms: ms(began),
    })
}

#[derive(Facet, Default)]
pub struct BlindArg {
    pub group: Option<String>,
    pub on: Option<bool>,
    pub reveal: Option<bool>,
    pub seed: Option<u64>,
}

/// `MIX_VARIANTS_BLIND`.
pub fn blind(daw: &Standalone, project: &ProjectContext, a: &BlindArg) -> Result<StateRow, String> {
    let span = tracing::info_span!("MIX_VARIANTS_BLIND", mix.on = a.on, mix.reveal = a.reveal);
    let _in = span.enter();
    let group = group_name(daw, project, a.group.as_deref())?;
    let mut g = load_group(daw, project, &group)?;
    if let Some(on) = a.on {
        if on != g.blind || on {
            g.set_blind(on, a.seed.unwrap_or_else(seed_now));
        }
    }
    if a.reveal.unwrap_or(false) {
        g.revealed = true;
    }
    if a.on.is_none() && a.reveal.is_none() {
        return Err("`on` (true or false) or `reveal: true`".to_owned());
    }
    save_group(daw, project, &g)?;
    state(daw, project, Some(&group))
}

#[derive(Facet, Default)]
pub struct RateArg {
    pub group: Option<String>,
    pub name: Option<String>,
    pub label: Option<String>,
    pub stars: Option<u32>,
    pub pick: Option<bool>,
    pub comment: Option<String>,
}

/// `MIX_VARIANT_RATE`.
pub fn rate(daw: &Standalone, project: &ProjectContext, a: &RateArg) -> Result<StateRow, String> {
    let span = tracing::info_span!(
        "MIX_VARIANT_RATE",
        mix.stars = a.stars,
        mix.pick = a.pick,
        mix.comment = a.comment.is_some(),
    );
    let _in = span.enter();
    let group = group_name(daw, project, a.group.as_deref())?;
    let mut g = load_group(daw, project, &group)?;
    let i = if a.name.is_some() || a.label.is_some() {
        g.resolve(&Pick {
            name: a.name.clone(),
            label: a.label.clone(),
            ..Pick::default()
        })?
    } else {
        g.current
            .and_then(|c| usize::try_from(c).ok())
            .filter(|_| g.auditioning)
            .ok_or("say which variant (`name` or `label`): none is playing")?
    };
    g.rate(i, a.stars, a.pick, a.comment.as_deref())?;
    save_group(daw, project, &g)?;
    state(daw, project, Some(&group))
}

/// `MIX_VARIANTS_STATE`.
pub fn state(
    daw: &Standalone,
    project: &ProjectContext,
    group: Option<&str>,
) -> Result<StateRow, String> {
    let all = groups(daw, project);
    let active = active(daw, project);
    let name = group
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .or_else(|| (!active.is_empty()).then(|| active.clone()));
    let Some(name) = name else {
        return Ok(StateRow {
            group: String::new(),
            note: String::new(),
            scope: Vec::new(),
            only: Vec::new(),
            blind: false,
            revealed: true,
            matched: false,
            auditioning: false,
            current: String::new(),
            kept: String::new(),
            ended: false,
            variants: Vec::new(),
            groups: all,
            active,
        });
    };
    let g = load_group(daw, project, &name)?;
    let mut row = g.view(paths_of(daw, project, &g.roots));
    row.groups = all;
    row.active = active;
    Ok(row)
}

#[derive(Facet, Default)]
pub struct KeepArg {
    pub group: Option<String>,
    pub name: Option<String>,
    pub label: Option<String>,
}

/// `MIX_VARIANT_KEEP`.
pub fn keep(daw: &Standalone, project: &ProjectContext, a: &KeepArg) -> Result<KeepRow, String> {
    let span = tracing::info_span!(
        "MIX_VARIANT_KEEP",
        mix.changed = tracing::field::Empty,
        mix.states = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let group = group_name(daw, project, a.group.as_deref())?;
    let mut g = load_group(daw, project, &group)?;
    let i = if a.name.is_some() || a.label.is_some() {
        g.resolve(&Pick {
            name: a.name.clone(),
            label: a.label.clone(),
            ..Pick::default()
        })?
    } else {
        g.picked()
            .or_else(|| {
                g.current
                    .and_then(|c| usize::try_from(c).ok())
                    .filter(|_| g.auditioning)
            })
            .ok_or("say which variant (`name` or `label`): none is picked or playing")?
    };
    let l = g.label_of(i);
    end_audition(daw, project, &mut g)?;
    let name = g.variants[i].name.clone();
    let mix = load_mix(daw, project, &group, &name)?;
    let row = crate::mix_snapshot::put_with(
        daw,
        project,
        &format!("{group}:{name}"),
        &mix,
        Put {
            undo: true,
            keep: true,
            only: Only::saved(&g.only),
        },
    )?;
    g.kept.clone_from(&name);
    g.revealed = true;
    save_group(daw, project, &g)?;
    crate::mix_snapshot::set_head(daw, project, &format!("{group}:{name}"));
    span.record("mix.changed", row.changed.len());
    span.record("mix.states", row.states_loaded);
    span.record("mix.ms", ms(began));
    Ok(KeepRow {
        group,
        name,
        label: label(l),
        changed: row.changed,
        missing: row.missing,
        states_loaded: row.states_loaded,
        ms: ms(began),
    })
}

/// `MIX_VARIANTS_END`.
pub fn end(
    daw: &Standalone,
    project: &ProjectContext,
    group: Option<&str>,
    drop: bool,
) -> Result<StateRow, String> {
    let span = tracing::info_span!("MIX_VARIANTS_END", mix.drop = drop);
    let _in = span.enter();
    let group = group_name(daw, project, group)?;
    let mut g = load_group(daw, project, &group)?;
    end_audition(daw, project, &mut g)?;
    g.ended = true;
    let mut row = g.view(paths_of(daw, project, &g.roots));
    if drop {
        for v in &g.variants {
            delete(daw, project, &mix_key(&group, &v.name));
        }
        delete(daw, project, &group_key(&group));
        let left: Vec<String> = groups(daw, project)
            .into_iter()
            .filter(|n| n != &group)
            .collect();
        set(daw, project, GROUPS, &left.join("\n"))?;
        if active(daw, project) == group {
            delete(daw, project, ACTIVE);
        }
        row.groups = left;
    } else {
        save_group(daw, project, &g)?;
        row.groups = groups(daw, project);
    }
    row.active = active(daw, project);
    Ok(row)
}

#[derive(Facet, Default)]
pub struct MatchArg {
    pub group: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub off: Option<bool>,
}

/// Interleaved stereo, split.
fn split(s: &[f32]) -> (Vec<f32>, Vec<f32>) {
    s.chunks_exact(2).map(|c| (c[0], c[1])).unzip()
}

/// The scope's output of the mix as it stands over `from..to`: integrated
/// loudness, LUFS.
fn scope_lufs(
    daw: &Standalone,
    song: &str,
    g: &Group,
    (from, to): (f64, f64),
) -> Result<f64, String> {
    let tap: HashSet<String> = g.roots.iter().cloned().collect();
    let r = crate::measure::render_span(
        daw,
        song,
        &crate::measure::Span {
            from,
            to,
            tap: &tap,
            post_fader: true,
            bypass_fx: HashSet::new(),
            solo: None,
        },
    )?;
    let out: Vec<f32> = if g.roots.is_empty() {
        r.master
    } else {
        let mut sum = vec![0.0f32; r.master.len()];
        for root in &g.roots {
            for (s, x) in sum.iter_mut().zip(r.tracks.get(root).into_iter().flatten()) {
                *s += x;
            }
        }
        sum
    };
    let (l, rr) = split(&out);
    Ok(crate::measure::analyse(&l, &rr, crate::measure::RATE, crate::measure::Bands::Octave).lufs)
}

/// The trims that bring each loudness down to the quietest: `None` (no
/// loudness: silent) gets none.
pub fn trims(lufs: &[Option<f64>]) -> (Option<f64>, Vec<f64>) {
    let target = lufs
        .iter()
        .flatten()
        .copied()
        .filter(|l| l.is_finite())
        .reduce(f64::min);
    let trims = lufs
        .iter()
        .map(|l| match (l, target) {
            (Some(l), Some(t)) if l.is_finite() => r2(t - l),
            _ => 0.0,
        })
        .collect();
    (target, trims)
}

/// `MIX_VARIANTS_MATCH`.
pub fn match_loudness(
    daw: &Standalone,
    project: &ProjectContext,
    a: &MatchArg,
) -> Result<MatchRow, String> {
    let span = tracing::info_span!(
        "MIX_VARIANTS_MATCH",
        mix.variants = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let group = group_name(daw, project, a.group.as_deref())?;
    let mut g = load_group(daw, project, &group)?;
    let say = |g: &Group| -> Vec<String> {
        g.in_label_order()
            .into_iter()
            .enumerate()
            .map(|(l, i)| {
                let v = &g.variants[i];
                if g.hidden() {
                    format!("{}: measured", label(l))
                } else {
                    format!(
                        "{} ({}): {} LUFS, trim {} dB",
                        label(l),
                        v.name,
                        v.lufs.map_or_else(|| "-".to_owned(), |x| r2(x).to_string()),
                        v.trim_db
                    )
                }
            })
            .collect()
    };
    if a.off.unwrap_or(false) {
        g.matched = false;
        if g.auditioning
            && let Some(i) = g.current.and_then(|c| usize::try_from(c).ok())
        {
            let mix = load_mix(daw, project, &group, &g.variants[i].name)?;
            apply_trim(daw, project, &mut g, &mix, 0.0)?;
        }
        save_group(daw, project, &g)?;
        return Ok(MatchRow {
            group,
            matched: false,
            variants: say(&g),
            target_lufs: None,
            ms: ms(began),
        });
    }
    let (Some(from), Some(to)) = (a.from.as_deref(), a.to.as_deref()) else {
        return Err("`from` and `to` (the span to measure) — or `off: true`".to_owned());
    };
    let span_s = (
        crate::fx_actions::moment(daw, project, from)?,
        crate::fx_actions::moment(daw, project, to)?,
    );
    let song = crate::fx_actions::project_guid(project)?;
    let playing = g
        .current
        .and_then(|c| usize::try_from(c).ok())
        .filter(|_| g.auditioning);
    // Nothing trimmed while measuring.
    let scope: Option<HashSet<String>> =
        (!g.scope.is_empty()).then(|| g.scope.iter().cloned().collect());
    let before = if g.auditioning {
        end_audition(daw, project, &mut g)?;
        None
    } else {
        Some(crate::mix_snapshot::take_scoped(
            daw,
            project,
            &song,
            "before matching",
            scope.as_ref(),
        ))
    };
    let silently = Put {
        undo: false,
        keep: false,
        only: Only::saved(&g.only),
    };
    let measured: Result<Vec<Option<f64>>, String> = (0..g.variants.len())
        .map(|i| {
            let mix = load_mix(daw, project, &group, &g.variants[i].name)?;
            crate::mix_snapshot::put_with(daw, project, &mix.name, &mix, silently)?;
            Ok(Some(scope_lufs(daw, &song, &g, span_s)?))
        })
        .collect();
    // The mix as it was, whatever the measuring did.
    if let Some(before) = &before {
        crate::mix_snapshot::put_with(
            daw,
            project,
            "before matching",
            before,
            Put {
                undo: false,
                keep: false,
                only: Only::ALL,
            },
        )?;
    }
    let measured = match measured {
        Ok(m) => m,
        Err(e) => {
            if let Some(i) = playing {
                begin_audition(daw, project, &mut g)?;
                let _ = play(daw, project, &mut g, i);
            }
            save_group(daw, project, &g)?;
            return Err(e);
        }
    };
    let (target, trim) = trims(&measured);
    for ((v, l), t) in g.variants.iter_mut().zip(&measured).zip(&trim) {
        v.lufs = l.map(r2);
        v.trim_db = *t;
    }
    g.matched = true;
    if let Some(i) = playing {
        begin_audition(daw, project, &mut g)?;
        play(daw, project, &mut g, i)?;
    }
    save_group(daw, project, &g)?;
    span.record("mix.variants", g.variants.len());
    span.record("mix.ms", ms(began));
    Ok(MatchRow {
        group,
        matched: true,
        variants: say(&g),
        target_lufs: target.map(r2).filter(|_| !g.hidden()),
        ms: ms(began),
    })
}

#[derive(Facet, Default)]
struct StateArg {
    group: Option<String>,
}

#[derive(Facet, Default)]
struct EndArg {
    group: Option<String>,
    drop: Option<bool>,
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&CREATE, Effect::Mutating, |a: CreateArg| {
        let (daw, project) = song()?;
        create(daw, &project, &a)
    })?;
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = song()?;
        add(daw, &project, &a)
    })?;
    list.register(&SWITCH, Effect::Mutating, |a: SwitchArg| {
        let (daw, project) = song()?;
        switch(daw, &project, &a)
    })?;
    list.register(&NEXT, Effect::Mutating, |(): ()| {
        let (daw, project) = song()?;
        switch(
            daw,
            &project,
            &SwitchArg {
                next: Some(true),
                ..SwitchArg::default()
            },
        )
    })?;
    list.register(&PREV, Effect::Mutating, |(): ()| {
        let (daw, project) = song()?;
        switch(
            daw,
            &project,
            &SwitchArg {
                prev: Some(true),
                ..SwitchArg::default()
            },
        )
    })?;
    list.register(&PICK, Effect::Mutating, |(): ()| {
        let (daw, project) = song()?;
        rate(
            daw,
            &project,
            &RateArg {
                pick: Some(true),
                ..RateArg::default()
            },
        )
    })?;
    list.register(&BLIND, Effect::Mutating, |a: BlindArg| {
        let (daw, project) = song()?;
        blind(daw, &project, &a)
    })?;
    list.register(&RATE, Effect::Mutating, |a: RateArg| {
        let (daw, project) = song()?;
        rate(daw, &project, &a)
    })?;
    list.register(&STATE, Effect::ReadOnly, |a: StateArg| {
        let (daw, project) = song()?;
        state(daw, &project, a.group.as_deref())
    })?;
    list.register(&MATCH, Effect::Mutating, |a: MatchArg| {
        let (daw, project) = song()?;
        match_loudness(daw, &project, &a)
    })?;
    list.register(&KEEP, Effect::Mutating, |a: KeepArg| {
        let (daw, project) = song()?;
        keep(daw, &project, &a)
    })?;
    list.register(&END, Effect::Mutating, |a: EndArg| {
        let (daw, project) = song()?;
        end(daw, &project, a.group.as_deref(), a.drop.unwrap_or(false))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
