//! The one function: a scene and a session in, a row list out.
//!
//! Everything a scene means is decided here and nowhere else. The window
//! renders the row list as it is; the REAPER applier walks the same list
//! and writes show, fold and size onto real tracks. Two appliers, one
//! resolve — which is the whole reason the row list is the output rather
//! than a pile of per-surface booleans.

use std::collections::HashMap;

use super::facts::Fact;
use super::language::Language;
use super::selector::{Rank, Role, Selector};
use super::types::{Fold, GroupBy, Resolved, Scene, Size, Surface};
use crate::golden_session::Kind;

/// What a row is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A real track.
    Track(String),
    /// A synthesised header for one performer — `flow.scenes.performer-order`.
    ///
    /// Emitted by `group_by: performer`, which is #51. The variant is
    /// here so that ticket adds the emitting and nothing else, and so
    /// that every consumer written today already skips a row it cannot
    /// apply to a track.
    PerformerHeader(String),
}

/// One row of a resolved scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What the row is about.
    pub target: Target,
    /// How deep it sits, 0 at the top.
    pub depth: u32,
    /// How much room it gets.
    pub size: Size,
    /// What happens to the rows beneath it.
    pub fold: Fold,
}

impl Row {
    /// The track a row is about, when it is about one.
    #[must_use]
    pub const fn guid(&self) -> Option<&str> {
        match &self.target {
            Target::Track(guid) => Some(guid.as_str()),
            Target::PerformerHeader(_) => None,
        }
    }
}

/// The performer group a track with neither dimension nor send falls
/// into (`flow.scenes.performer-identity`, decision #31): shown, never
/// dropped, so the view says what the Patch List says.
pub const UNASSIGNED: &str = "Unassigned";

/// How performer header rows are ordered under an instrument folder.
///
/// Compares two performer names (`UNASSIGNED` for a track with neither
/// dimension nor send). `None` — the only ordering the engine can do
/// until the patch list (#61) supplies one — orders by first appearance
/// in the track list instead, which [`resolve`] does without ever
/// calling a comparator. The patch list will pass `Some(&cmp)` ranking
/// by its own performer order.
pub type PerformerOrder<'a> = dyn Fn(&str, &str) -> std::cmp::Ordering + 'a;

/// Resolve a scene against a session, for one surface.
///
/// Rules are evaluated in order and the **last** one that matches a
/// track wins; the scene's `default` covers every track no rule matched.
/// A folder folded `Collapsed` or `Hidden` takes its rows with it, and a
/// `Hidden` folder takes its own row too.
///
/// Facts must arrive in project order — the row list is in that order,
/// and the fold walk depends on it.
///
/// `order` is `flow.scenes.performer-order`'s injected comparator: read
/// only when `scene.group_by` is [`GroupBy::Performer`], and even then
/// only to break the tie between the performers found — the project's
/// own track order (`facts`) is never touched.
///
/// # Engine invariants
///
/// Two things are true of every scene and cannot be said otherwise in
/// the table, so they are enforced here:
///
/// - **A stereo pair's half is always a rail.** The pair's folder
///   carries the processing and the width; its L and its R are one
///   signal seen twice, and a scene that opened them would be showing
///   the same thing at four times the cost.
/// - **A fold applies down the tree.** Rules say what happens to a
///   folder; what happens to its contents follows from that rather than
///   from a second rule nobody would remember to write.
///
/// `active_language` (`flow.vocals.language.active`) feeds the common
/// prelude the same way `mode` does: `None` hides nothing, `Some(lang)`
/// hides every source whose own language is neither `lang` nor
/// [`Language::All`] — see `table::prelude`.
// r[impl flow.vocals.language.active]
#[must_use]
pub fn resolve(
    scene: &Scene,
    facts: &[Fact],
    surface: Surface,
    mode: Option<&str>,
    active_language: Option<Language>,
    order: Option<&PerformerOrder<'_>>,
) -> Vec<Row> {
    // What is in the mixer only is never an arrangement row.
    let kept: Vec<Fact>;
    let facts = if surface == Surface::Arrange && facts.iter().any(|f| f.mixer_only) {
        kept = facts.iter().filter(|f| !f.mixer_only).cloned().collect();
        kept.as_slice()
    } else {
        facts
    };
    let effects = effects_for(scene, facts, surface, mode, active_language);
    let rows = walk(facts, &effects);
    if scene.group_by == GroupBy::Performer {
        group_by_performer(facts, &rows, order)
    } else {
        rows
    }
}

/// The plain walk: every fact that survives its fold, in project order,
/// each a `Target::Track` row. `resolve` regroups this by performer
/// afterwards when the scene asks for it.
fn walk(facts: &[Fact], effects: &[Resolved]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::with_capacity(facts.len());
    // The depth of the shallowest folded folder we are inside, if any.
    let mut folded: Option<u32> = None;
    // The depths of the lifted folders we are inside: each lifts what it
    // holds one level.
    let mut lifted: Vec<u32> = Vec::new();
    for (fact, effect) in facts.iter().zip(effects) {
        if let Some(at) = folded {
            if fact.depth > at {
                continue;
            }
            folded = None;
        }
        while lifted.last().is_some_and(|at| fact.depth <= *at) {
            lifted.pop();
        }
        if effect.fold == Fold::Lifted && fact.is_folder {
            lifted.push(fact.depth);
            continue;
        }
        let depth = fact
            .depth
            .saturating_sub(u32::try_from(lifted.len()).unwrap_or(u32::MAX));
        if effect.fold == Fold::Hidden {
            // A hidden leaf drops its own row; a hidden folder drops
            // everything under it as well.
            if fact.is_folder {
                folded = Some(fact.depth);
            }
            continue;
        }
        if fact.is_folder && !effect.fold.keeps_children() {
            folded = Some(fact.depth);
        }
        let size = if fact.pair_half && !fact.is_folder {
            Size::Minimum
        } else {
            effect.size
        };
        out.push(Row {
            target: Target::Track(fact.guid.clone()),
            depth,
            size,
            fold: effect.fold,
        });
    }
    out
}

/// `flow.scenes.performer-order`: under each visible instrument folder
/// (`Kind::Group` — `Drum Kit`, `Electric`, …), replace its direct
/// children with one `PerformerHeader` row per performer found among
/// them, that performer's children nested beneath, each shifted one
/// level deeper to make room for the header. The folder's own row, and
/// everything outside it, is untouched: this is a view over `rows`, not
/// a second pass over the project.
///
/// A child's performer is the first one found among its own subtree —
/// every leaf a rig sends is one performer's, so any leaf will do. A
/// child with none is `UNASSIGNED`, grouped and shown rather than
/// dropped, exactly as the Patch List shows an unassigned track rather
/// than hiding it.
///
/// r[impl flow.scenes.performer-order]
fn group_by_performer(
    facts: &[Fact],
    rows: &[Row],
    order: Option<&PerformerOrder<'_>>,
) -> Vec<Row> {
    let by_guid: HashMap<&str, &Fact> = facts.iter().map(|f| (f.guid.as_str(), f)).collect();
    let is_instrument_folder = |row: &Row| -> bool {
        row.guid()
            .and_then(|g| by_guid.get(g))
            .is_some_and(|f| f.is_folder && f.kind == Some(Kind::Group))
    };
    let performer_of_block = |block: &[Row]| -> Option<String> {
        block.iter().find_map(|row| {
            row.guid()
                .and_then(|g| by_guid.get(g))
                .and_then(|f| f.performer.clone())
        })
    };

    let mut out = Vec::with_capacity(rows.len());
    let mut i = 0_usize;
    while let Some(row) = rows.get(i) {
        if !is_instrument_folder(row) {
            out.push(row.clone());
            i = i.saturating_add(1);
            continue;
        }
        let folder_depth = row.depth;
        out.push(row.clone());
        i = i.saturating_add(1);

        // The folder's direct children, each with its whole subtree, as
        // contiguous blocks — a block is everything from one child down
        // to (not including) the next row at the child's own depth.
        let mut blocks: Vec<(Option<String>, Vec<Row>)> = Vec::new();
        while rows.get(i).is_some_and(|r| r.depth > folder_depth) {
            let child_depth = rows.get(i).map_or(folder_depth, |r| r.depth);
            let start = i;
            i = i.saturating_add(1);
            while rows.get(i).is_some_and(|r| r.depth > child_depth) {
                i = i.saturating_add(1);
            }
            let block: Vec<Row> = rows.get(start..i).map_or_else(Vec::new, <[Row]>::to_vec);
            let performer = performer_of_block(&block);
            blocks.push((performer, block));
        }
        // Group the blocks by performer, keeping the order their first
        // block appeared in — first appearance, until #61's patch list
        // supplies `order`.
        let mut seen: Vec<String> = Vec::new();
        let mut groups: HashMap<String, Vec<Vec<Row>>> = HashMap::new();
        for (performer, block) in blocks {
            let key = performer.unwrap_or_else(|| UNASSIGNED.to_owned());
            if !seen.contains(&key) {
                seen.push(key.clone());
            }
            groups.entry(key).or_default().push(block);
        }
        if let Some(cmp) = order {
            seen.sort_by(|a, b| cmp(a, b));
        }
        let header_depth = folder_depth.saturating_add(1);
        for key in seen {
            out.push(Row {
                target: Target::PerformerHeader(key.clone()),
                depth: header_depth,
                size: Size::Compact,
                fold: Fold::Open,
            });
            for block in groups.remove(&key).unwrap_or_default() {
                for row in block {
                    out.push(Row {
                        depth: row.depth.saturating_add(1),
                        ..row
                    });
                }
            }
        }
    }
    out
}

/// The effect each track ends up with, before the fold walk drops rows.
///
/// The common prelude goes first, so a scene that wants the Guide folder
/// open only has to say so — it is data, and it is overridable.
fn effects_for(
    scene: &Scene,
    facts: &[Fact],
    surface: Surface,
    mode: Option<&str>,
    active_language: Option<Language>,
) -> Vec<Resolved> {
    let mut effects = vec![Resolved::from(scene.default); facts.len()];
    let prelude = super::table::prelude(mode, active_language);
    for rule in prelude.iter().chain(&scene.rules) {
        let effect = rule.for_surface(surface);
        for i in matching(&rule.selector, facts) {
            if let Some(slot) = effects.get_mut(i) {
                *slot = effect.over(*slot);
            }
        }
    }
    effects
}

/// The indices of every fact a selector matches.
fn matching(selector: &Selector, facts: &[Fact]) -> Vec<usize> {
    let base: Vec<usize> = facts
        .iter()
        .enumerate()
        .filter(|(_, fact)| matches_but_for_rank(selector, fact))
        .map(|(i, _)| i)
        .collect();
    match selector.rank {
        Rank::All => base,
        Rank::TopmostPerInstrument => topmost_per_instrument(&base, facts),
    }
}

fn matches_but_for_rank(selector: &Selector, fact: &Fact) -> bool {
    let role_ok = match selector.role {
        Role::Any => true,
        Role::Bus => fact.is_folder,
        Role::Leaf => !fact.is_folder,
    };
    if !role_ok {
        return false;
    }
    if !path_starts_with(&fact.path, &selector.group) {
        return false;
    }
    if let Some(kind) = &selector.kind {
        if !fact
            .kind
            .is_some_and(|k| k.as_str().eq_ignore_ascii_case(kind))
        {
            return false;
        }
    }
    if selector.depth.is_some_and(|depth| fact.depth != depth) {
        return false;
    }
    if let Some(language) = selector.language {
        if fact.language != Some(language) {
            return false;
        }
    }
    let dimensions = [
        (&selector.performer, fact.performer.as_deref()),
        (&selector.layer, fact.layer.as_deref()),
        (&selector.channel, fact.channel.as_deref()),
        (&selector.multi_mic, fact.multi_mic.as_deref()),
        (&selector.arrangement, fact.arrangement.as_deref()),
        (&selector.name, Some(fact.name.as_str())),
    ];
    dimensions.into_iter().all(|(want, have)| {
        want.as_ref()
            .is_none_or(|want| have.is_some_and(|have| have.eq_ignore_ascii_case(want)))
    })
}

/// Whether a taxonomy path begins with a selector's group path.
fn path_starts_with(path: &[super::facts::Segment], group: &[String]) -> bool {
    if group.len() > path.len() {
        return false;
    }
    group
        .iter()
        .zip(path)
        .all(|(want, segment)| segment.answers_to(want))
}

/// Keep only the lowest-index track per taxonomy group.
fn topmost_per_instrument(base: &[usize], facts: &[Fact]) -> Vec<usize> {
    let mut best: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for &i in base {
        let Some(fact) = facts.get(i) else { continue };
        let key = fact.group_key();
        match best.get(&key) {
            Some(&j) if facts.get(j).is_some_and(|f| f.index <= fact.index) => {}
            _ => {
                best.insert(key, i);
            }
        }
    }
    let mut out: Vec<usize> = best.into_values().collect();
    out.sort_unstable();
    out
}
