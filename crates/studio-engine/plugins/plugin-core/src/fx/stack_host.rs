//! The stack's host side, shared by every kind — spec `docs/spec/fx/stack.md`:
//! the params and the kind's view of itself, which the plugin runs with no
//! editor. The focus, the gestures and the chip strip are the editor's
//! (`plugin_ui::stack::host`, which re-exports this).
//!
//! The comp grew the first stack shell (`comp::ui::{params, stack_strip,
//! focus}`); this is that shape lifted out so the saturator, delay, reverb
//! and EQ wear the same one (`fx.stack.every-shell`):
//!
//! - [`StagePlacement`] — the three per-stage stack params (`use`, `on`,
//!   `lane`), nested bare inside each kind's stage struct, so stage 1 keeps
//!   unprefixed ids and stage *n* gets `s{n}_` like everything else in it
//!   (`fx.stack.params`).
//! - [`LaneParams`] and the stack globals ([`sum_mode_param`],
//!   [`output_trim_param`]) — appended after the stage blocks, the same ids
//!   the comp uses (`lgain_1`…, `summode`, `stktrim`).
//! - [`StackHost`] — what a kind's param tree exposes so the generic pieces
//!   can read and write it; [`sync_pool`] pushes its topology into a
//!   [`StagePool`] once per block.

use std::sync::Arc;

use crate::prelude::*;

use dsp::stack::{LaneCtl, Stage, StagePool, SumMode};

// ── Params ────────────────────────────────────────────────────────────────

/// A stage's place in the stack (`fx.stack.params`). Nested bare inside a
/// kind's stage struct, after the stage's own params.
#[derive(Params)]
pub struct StagePlacement {
    /// Whether this stage is part of the stack at all. Stage 1 defaults on
    /// (the plain plugin); the rest off until shift-click adds them
    /// (`fx.stack.add`).
    #[id = "use"]
    pub in_use: BoolParam,
    /// Stage bypass — present in the topology, crossfaded to identity.
    #[id = "on"]
    pub stage_on: BoolParam,
    /// Which parallel lane this stage feeds (`fx.stack.topology`). Within a
    /// lane, stages run serially in stage-number order.
    #[id = "lane"]
    pub lane: IntParam,
}

impl StagePlacement {
    /// `first` = stage 1, which starts in the stack; `max_stages` bounds
    /// the lane index (a stack has at most one lane per stage).
    #[must_use]
    pub fn new(first: bool, max_stages: usize) -> Self {
        Self {
            in_use: BoolParam::new("In Stack", first),
            stage_on: BoolParam::new("Stage On", true),
            lane: IntParam::new(
                "Lane",
                0,
                IntRange::Linear {
                    min: 0,
                    max: max_stages.saturating_sub(1).max(1) as i32,
                },
            ),
        }
    }

    /// The lane this stage feeds, 0-based.
    #[must_use]
    pub fn lane_index(&self) -> usize {
        self.lane.value().max(0) as usize
    }
}

/// One parallel lane's mix controls (`fx.stack.sum`). Nested as an array,
/// so the ids come out `lgain_1`…`lsolo_N`.
#[derive(Params)]
pub struct LaneParams {
    #[id = "lgain"]
    pub gain_db: FloatParam,
    #[id = "lmute"]
    pub mute: BoolParam,
    #[id = "lsolo"]
    pub solo: BoolParam,
}

impl Default for LaneParams {
    fn default() -> Self {
        Self {
            gain_db: FloatParam::new(
                "Lane Gain",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mute: BoolParam::new("Lane Mute", false),
            solo: BoolParam::new("Lane Solo", false),
        }
    }
}

impl LaneParams {
    /// This lane as the pool reads it.
    #[must_use]
    pub fn ctl(&self) -> LaneCtl {
        LaneCtl {
            gain: db_to_gain(f64::from(self.gain_db.value())),
            mute: self.mute.value(),
            solo: self.solo.value(),
            pan: 0.0,
        }
    }
}

/// Sum-law labels, in `summode` value order.
pub const SUM_MODE_LABELS: &[&str] = &["1/N", "Power", "Raw"];

/// The `summode` param: 0 = coherent (1/N), 1 = power (1/√N), 2 = raw
/// (`fx.stack.sum`).
#[must_use]
pub fn sum_mode_param() -> IntParam {
    IntParam::new("Sum Mode", 0, IntRange::Linear { min: 0, max: 2 })
        .with_value_to_string(Arc::new(|v| {
            SUM_MODE_LABELS
                .get(v.max(0) as usize)
                .map_or_else(|| v.to_string(), ToString::to_string)
        }))
        .with_string_to_value(Arc::new(|s| {
            let s = s.trim();
            SUM_MODE_LABELS
                .iter()
                .position(|l| l.eq_ignore_ascii_case(s))
                .map(|i| i as i32)
                .or_else(|| s.parse().ok())
        }))
}

/// The `stktrim` param: the stack's output trim.
#[must_use]
pub fn output_trim_param() -> FloatParam {
    FloatParam::new(
        "Stack Trim",
        0.0,
        FloatRange::Linear {
            min: -24.0,
            max: 24.0,
        },
    )
    .with_unit(" dB")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

/// A `summode` value as the pool's law.
#[must_use]
pub const fn sum_mode(v: i32) -> SumMode {
    match v {
        1 => SumMode::Power,
        2 => SumMode::Raw,
        _ => SumMode::Coherent,
    }
}

fn db_to_gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

// ── The kind's view of itself ─────────────────────────────────────────────

/// What a kind's param tree exposes to the shared stack pieces. Stage and
/// lane indices are 0-based; `max_stages` is the kind's `MAX_STAGES`
/// (`fx.stack.limits`), and there are as many lanes as stages.
pub trait StackHost: Send + Sync + 'static {
    fn max_stages(&self) -> usize;
    fn placement(&self, stage: usize) -> &StagePlacement;
    fn lane(&self, lane: usize) -> &LaneParams;
    fn sum_mode(&self) -> &IntParam;
    fn output_trim_db(&self) -> &FloatParam;
    /// The persisted focus (`fx.stack.focus`), 0-based as a string.
    fn focused_cell(&self) -> &parking_lot::RwLock<String>;

    /// Stage `stage`'s style param (the automatable index).
    fn style_param(&self, stage: usize) -> &IntParam;
    /// Stage `stage`'s style as the session resolves it (id first,
    /// `fx.stack.style-id`).
    fn resolved_style(&self, stage: usize) -> usize;
    /// Record stage `stage`'s style id for `index`.
    fn store_style_id(&self, stage: usize, index: usize);
    /// A short badge for style `index`, drawn on its chip.
    fn style_badge(&self, index: usize) -> String;
    /// The full name of style `index`, for the chip's tooltip.
    fn style_name(&self, index: usize) -> String;

    /// Indices of the stages currently in the stack.
    fn stages_in_use(&self) -> Vec<usize> {
        (0..self.max_stages())
            .filter(|&i| self.placement(i).in_use.value())
            .collect()
    }

    /// First pool slot not in the stack, if any (`fx.stack.add`).
    fn first_free_stage(&self) -> Option<usize> {
        (0..self.max_stages()).find(|&i| !self.placement(i).in_use.value())
    }

    /// First lane no in-use stage feeds — the Ctrl+Shift-click target.
    fn first_free_lane(&self) -> Option<usize> {
        (0..self.max_stages()).find(|&l| {
            !(0..self.max_stages()).any(|i| {
                let p = self.placement(i);
                p.in_use.value() && p.lane_index() == l
            })
        })
    }

    /// The focused stage (0-based), clamped to a stage that is in use.
    fn resolved_focused_stage(&self) -> usize {
        let raw: usize = self.focused_cell().read().parse().unwrap_or(0);
        let idx = raw.min(self.max_stages() - 1);
        if self.placement(idx).in_use.value() {
            idx
        } else {
            self.stages_in_use().first().copied().unwrap_or(0)
        }
    }

    fn store_focused_stage(&self, idx: usize) {
        *self.focused_cell().write() = idx.min(self.max_stages() - 1).to_string();
    }

    /// The stage the editor shows for `focus`: `focus` itself, unless that
    /// stage left the stack under the editor (host undo, automation of
    /// `use`) — then [`Self::resolved_focused_stage`].
    fn visible_stage(&self, focus: usize) -> usize {
        if focus < self.max_stages() && self.placement(focus).in_use.value() {
            focus
        } else {
            self.resolved_focused_stage()
        }
    }
}

/// Push the host's topology, lanes and globals into `pool` (no allocation).
/// Stage params are the kind's business — sync those before calling this.
pub fn sync_pool<S: Stage>(pool: &mut StagePool<S>, host: &dyn StackHost) {
    let n = host.max_stages().min(pool.len());
    for i in 0..n {
        let p = host.placement(i);
        pool.set_slot(i, p.in_use.value(), p.stage_on.value(), p.lane_index());
    }
    for l in 0..n {
        pool.set_lane(l, host.lane(l).ctl());
    }
    pool.sum_mode = sum_mode(host.sum_mode().value());
    pool.output_trim = db_to_gain(f64::from(host.output_trim_db().value()));
    pool.update_alignment();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_mode_labels_round_trip() {
        let p = sum_mode_param();
        for (i, label) in SUM_MODE_LABELS.iter().enumerate() {
            let n = p.string_to_normalized_value(label).expect(label);
            assert_eq!(p.preview_plain(n), i as i32);
            assert_eq!(p.normalized_value_to_string(n, false), *label);
        }
    }
}
