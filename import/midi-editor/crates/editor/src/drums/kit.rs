//! Folding a recorded kit into the lanes a drum editor reads.
//!
//! Three kick mics and four snare mics are seven tracks, but while
//! editing they are one kick and one snare. This module decides which
//! role a track plays (`Kick` / `Snare` / `Toms` / `Other`), from its
//! place in the host's folder hierarchy first and its name second, and
//! rebuilds the workspace layout as four role lanes, bottom-up:
//! `Kick` at the bottom, `Snare` above it, `Toms` above that, `Other`
//! on top.
//!
//! Nothing here is drum-specific *in type*: a role is a label plus a
//! draw rule, and the fold takes `(guid, role)` pairs. A guitar
//! workspace later defines `DI` / `Amps` the same way.
//!
//! Spec: `features/editor/spec/drum-mode.md` (`drums.lanes.*`).

use crate::rows;
use crate::tracks::{Lane, LaneLayout, Workspace};

/// The role a track plays in a folded kit, read from its folders and its
/// name: the DAW's (daw-edit), shared with the arrangement and the engine.
pub use daw_edit::drums::{
    KitScore, LaneRole, folder_role, is_kit_folder, is_trigger_name, is_unused_name, kit_role,
    score_kit, tom_number,
};

/// A role's colour, as the editor paints it.
pub trait RoleColor {
    /// The role's hue — the drum map's own kit palette, so a kick is
    /// red whether it is a MIDI row or an audio lane. `Other` takes the
    /// hat cyan: hats and cymbals are most of what lives there.
    fn color(self) -> &'static str;
}

impl RoleColor for LaneRole {
    fn color(self) -> &'static str {
        match self {
            LaneRole::Kick => rows::DRUM_KICK_COLOR,
            LaneRole::Snare => rows::DRUM_SNARE_COLOR,
            LaneRole::Toms => rows::DRUM_TOM_COLOR,
            LaneRole::Other => rows::DRUM_HAT_COLOR,
        }
    }
}

/// Group a split lane's members into the ones that own a sub-row and the
/// triggers that ride on top of one, as indices into `names`.
///
/// A trigger is not another tom — it is the same drum, sensed a second
/// way — so `T3 Trig` belongs in `T3`'s row, drawn over it. Given a row
/// each, four toms with triggers read as an eight-piece kit and every
/// row is half the height it should be.
///
/// A trigger keeps its own row when nothing claims it: no tom number
/// (`Trig`, ambiguous), or a number with no matching tom (`T4 Trig`
/// where `T4` was never recorded). Folding those away silently would
/// hide a track that is really there.
// r[impl drums.lanes.trigger-overlay]
pub fn trigger_sub_rows(names: &[&str]) -> Vec<(usize, Vec<usize>)> {
    let mut rows: Vec<(usize, Vec<usize>)> = (0..names.len())
        .filter(|&i| !is_trigger_name(names[i]))
        .map(|i| (i, Vec::new()))
        .collect();

    for t in (0..names.len()).filter(|&i| is_trigger_name(names[i])) {
        let host = tom_number(names[t]).and_then(|n| {
            rows.iter_mut()
                .find(|(h, _)| tom_number(names[*h]) == Some(n))
        });
        match host {
            Some((_, overlays)) => overlays.push(t),
            None => rows.push((t, Vec::new())),
        }
    }
    rows
}

/// How much more a trigger counts than an acoustic mic in the same
/// detection unit.
///
/// A trigger is a contact mic on the drum: almost no bleed, almost no
/// decay, a near-vertical attack — better evidence of *when* the drum
/// was hit than any acoustic mic. But it is not infallible: a trigger
/// can drop out, double-fire on a rim shot, or sit slightly out of
/// alignment. So the mics keep a vote rather than sitting the detection
/// out; the trigger just outweighs them. At 4:1 one trigger outweighs
/// any realistic number of mics on one drum while still being pulled by
/// them where they agree — and where the trigger misses a hit entirely,
/// the mics can still put one there.
const TRIGGER_WEIGHT: f64 = 4.0;

/// The signals a lane detects on: groups of `(member index, weight)`
/// into `names`, each group blended into one signal the detector runs
/// over. Weights within a unit sum to 1, so every unit's signal comes
/// out at a comparable level regardless of how many mics it has.
///
/// Two rules, and both exist because the alternative loses hits.
///
/// **A trigger is weighted over the mics it shares a drum with** — see
/// [`TRIGGER_WEIGHT`]. It dominates the onset without silencing the
/// mics that corroborate it.
///
/// **Toms detect per tom, not as a lane.** A summed toms signal answers
/// "a tom was hit" when the question is "which one". With four triggers
/// the toms are four clean independent signals, so they are four units.
/// Un-numbered tom tracks share a unit of their own rather than being
/// dropped.
///
/// `Unused` members are excluded everywhere (r[drums.lanes.toms-split]),
/// and a lane that is not a detection source yields nothing.
// r[impl drums.group.detection-source]
pub fn detection_units(role: LaneRole, names: &[&str]) -> Vec<Vec<(usize, f64)>> {
    if !role.is_detection_source() {
        return Vec::new();
    }
    let live: Vec<usize> = (0..names.len())
        .filter(|&i| !is_unused_name(names[i]))
        .collect();

    // Toms split by which tom; every other role is one unit.
    let units: Vec<Vec<usize>> = if role.splits_members() {
        let mut keys: Vec<Option<u8>> = Vec::new();
        for &i in &live {
            let k = tom_number(names[i]);
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        keys.into_iter()
            .map(|k| {
                live.iter()
                    .copied()
                    .filter(|&i| tom_number(names[i]) == k)
                    .collect()
            })
            .collect()
    } else {
        vec![live]
    };

    units
        .into_iter()
        .filter(|unit| !unit.is_empty())
        .map(|unit| {
            let weight = |i: usize| {
                if is_trigger_name(names[i]) {
                    TRIGGER_WEIGHT
                } else {
                    1.0
                }
            };
            let total: f64 = unit.iter().map(|&i| weight(i)).sum();
            unit.into_iter().map(|i| (i, weight(i) / total)).collect()
        })
        .collect()
}

impl Workspace {
    /// Rebuild the layout as role lanes over `members` (`(guid, role)`),
    /// top to bottom `Other`, `Toms`, `Snare`, `Kick`; roles with no
    /// members get no lane. Member order within a lane follows
    /// `members`. Tracks not mentioned keep a lane of their own below.
    ///
    /// The fold is an inference, not an arrangement: it does not mark the
    /// layout arranged, so a later hand merge/split still wins and is the
    /// thing that gets persisted.
    // r[impl drums.lanes.roles]
    // r[impl drums.lanes.heights]
    pub fn fold_roles(&mut self, members: &[(String, LaneRole)]) {
        let mut layout = LaneLayout::default();
        for role in LaneRole::ALL {
            let guids: Vec<String> = members
                .iter()
                .filter(|(g, r)| *r == role && self.track_by_guid(g).is_some())
                .map(|(g, _)| g.clone())
                .collect();
            if guids.is_empty() {
                continue;
            }
            // Equal weight per role lane: the kick, the snare, the toms
            // and the rest share the stack evenly, and the toms divide
            // their share among themselves.
            layout.push(Lane::role(role, guids, 1.0));
        }
        for t in self.tracks() {
            if layout.lane_of(&t.guid).is_none() {
                layout.push(Lane::single(t.guid.clone(), t.mode.stack_weight()));
            }
        }
        *self.layout_mut() = layout;
    }

    /// The role lane a track is in, if its lane has one.
    pub fn role_of(&self, guid: &str) -> Option<LaneRole> {
        self.layout()
            .lane_of(guid)
            .and_then(|i| self.layout().lane(i))
            .and_then(|l| l.role)
    }

    /// The guids in the lane with `role`, in draw order.
    pub fn role_members(&self, role: LaneRole) -> Vec<String> {
        self.layout()
            .lanes()
            .iter()
            .find(|l| l.role == Some(role))
            .map(|l| l.tracks.clone())
            .unwrap_or_default()
    }
}
