//! The pooled stack — `Stack` for param-driven topology.
//!
//! A plugin's host parameters own the topology (`fx.stack.params`): each of a
//! fixed pool of `N` stages carries "am I in use" and "which lane am I on" as
//! parameters, and those change on the audio thread between blocks. The
//! Vec-of-lanes [`crate::stack::Stack`] would need reallocation to follow; this pool
//! never moves a stage — topology is just the `in_use` / `lane_of` arrays,
//! re-read every block. Everything is fixed-size at construction, so
//! processing is allocation-free from the first block
//! (`fx.stack.process`).
//!
//! Semantics match [`crate::stack::Stack`]: within a lane, stages run serially in
//! `order` (pool order by default, [`StagePool::set_order`]); lanes run the same input in parallel and are summed by the
//! [`SumMode`] law over the active lanes; short lanes are delay-compensated
//! to the longest (`fx.stack.latency`); stage enable/disable crossfades
//! (`FADE_SAMPLES`).

use alloc::vec;
use alloc::vec::Vec;

use crate::stack::{FADE_SAMPLES, Stage, SumMode};

/// Per-lane mix controls.
#[derive(Clone, Copy, Debug)]
pub struct LaneCtl {
    /// Linear gain (1.0 = 0 dB).
    pub gain: f64,
    pub mute: bool,
    pub solo: bool,
    /// Lane placement, −1 (left) … 0 (as is) … +1 (right). At 0 the lane's
    /// stereo output passes untouched; off centre the lane is summed to mono
    /// and placed with a linear law, so ±1 is total — one lane per side is
    /// the dual-effect `Split` routing (`fx.stack.every-shell`).
    pub pan: f64,
}

impl Default for LaneCtl {
    fn default() -> Self {
        Self {
            gain: 1.0,
            mute: false,
            solo: false,
            pan: 0.0,
        }
    }
}

impl LaneCtl {
    /// The (left, right) gains a mono-summed lane is placed with, or `None`
    /// for a centred lane (stereo kept).
    fn pan_gains(&self) -> Option<(f64, f64)> {
        if self.pan == 0.0 {
            return None;
        }
        let p = self.pan.clamp(-1.0, 1.0);
        Some(((1.0 - p).min(1.0), (1.0 + p).min(1.0)))
    }
}

struct PoolSlot<S> {
    stage: S,
    /// Host-side enable. Off = the stage is not part of the topology at all
    /// (an unused pool slot); distinct from a bypassed-but-present stage.
    in_use: bool,
    /// Stage bypass (present in the topology, crossfaded to identity).
    enabled: bool,
    lane: usize,
    /// Serial position within the lane; ties fall back to pool order.
    order: usize,
    /// 0..=1 bypass crossfade (1 = fully processing).
    fade: f64,
}

struct PoolLane {
    ctl: LaneCtl,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    delay: LaneAlign,
}

/// Plain stereo delay for lane alignment (same as the Vec stack's).
struct LaneAlign {
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    pos: usize,
    delay: usize,
}

impl LaneAlign {
    fn new(max_delay: usize) -> Self {
        let len = max_delay.max(1);
        Self {
            buf_l: vec![0.0; len],
            buf_r: vec![0.0; len],
            pos: 0,
            delay: 0,
        }
    }

    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        if self.delay == 0 {
            return;
        }
        let len = self.buf_l.len();
        for i in 0..l.len() {
            let read = (self.pos + len - self.delay) % len;
            let (dl, dr) = (self.buf_l[read], self.buf_r[read]);
            self.buf_l[self.pos] = l[i];
            self.buf_r[self.pos] = r[i];
            l[i] = dl;
            r[i] = dr;
            self.pos = (self.pos + 1) % len;
        }
    }

    fn reset(&mut self) {
        self.buf_l.iter_mut().for_each(|x| *x = 0.0);
        self.buf_r.iter_mut().for_each(|x| *x = 0.0);
        self.pos = 0;
    }
}

/// A fixed pool of `N` stages arranged into up to `N` lanes by per-stage
/// `lane` assignments (`fx.stack.model`, `fx.stack.topology`).
// r[impl fx.stack.model]
// r[impl fx.stack.topology]
// r[impl fx.stack.limits]
pub struct StagePool<S> {
    slots: Vec<PoolSlot<S>>,
    lanes: Vec<PoolLane>,
    pub sum_mode: SumMode,
    /// Output trim, linear.
    pub output_trim: f64,
    /// Pre-stage snapshot for the bypass crossfade.
    fade_l: Vec<f64>,
    fade_r: Vec<f64>,
    /// The block's input, kept for [`SumMode::DryOnce`].
    dry_l: Vec<f64>,
    dry_r: Vec<f64>,
    /// Slot indices in run order (`order`, then pool index). Sized at
    /// construction; re-sorted in place when an order changes.
    run: Vec<usize>,
    max_block: usize,
    prepared: bool,
}

impl<S: Stage> StagePool<S> {
    /// A pool of `stages.len()` slots. Slot 0 starts in use on lane 0 (the
    /// single-stage plugin, `fx.stack.model`); the rest start unused.
    #[must_use]
    pub fn new(stages: Vec<S>) -> Self {
        let n = stages.len();
        let slots = stages
            .into_iter()
            .enumerate()
            .map(|(i, stage)| PoolSlot {
                stage,
                in_use: i == 0,
                enabled: true,
                lane: 0,
                order: i,
                fade: 1.0,
            })
            .collect();
        Self {
            slots,
            lanes: (0..n)
                .map(|_| PoolLane {
                    ctl: LaneCtl::default(),
                    scratch_l: Vec::new(),
                    scratch_r: Vec::new(),
                    delay: LaneAlign::new(1),
                })
                .collect(),
            sum_mode: SumMode::default(),
            output_trim: 1.0,
            fade_l: Vec::new(),
            fade_r: Vec::new(),
            dry_l: Vec::new(),
            dry_r: Vec::new(),
            run: (0..n).collect(),
            max_block: 0,
            prepared: false,
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Allocate every buffer processing will touch. `max_latency` bounds the
    /// per-lane alignment delay (the kind's worst case).
    pub fn prepare(&mut self, max_block: usize, max_latency: usize) {
        self.max_block = max_block;
        for lane in &mut self.lanes {
            lane.scratch_l = vec![0.0; max_block];
            lane.scratch_r = vec![0.0; max_block];
            lane.delay = LaneAlign::new(max_latency + 1);
        }
        self.fade_l = vec![0.0; max_block];
        self.fade_r = vec![0.0; max_block];
        self.dry_l = vec![0.0; max_block];
        self.dry_r = vec![0.0; max_block];
        self.update_alignment();
        self.prepared = true;
    }

    /// The stage in slot `i` (for parameter sync).
    pub fn stage_mut(&mut self, i: usize) -> Option<&mut S> {
        self.slots.get_mut(i).map(|s| &mut s.stage)
    }

    #[must_use]
    pub fn stage(&self, i: usize) -> Option<&S> {
        self.slots.get(i).map(|s| &s.stage)
    }

    /// Set slot `i`'s topology: in the stack or not, bypassed or not, and
    /// which lane it feeds. Call every block from the param sync — cheap.
    pub fn set_slot(&mut self, i: usize, in_use: bool, enabled: bool, lane: usize) {
        let n = self.slots.len();
        if let Some(slot) = self.slots.get_mut(i) {
            // A slot entering the topology starts clean and fully faded in —
            // it was not audible before, so there is no transition to hide.
            if in_use && !slot.in_use {
                slot.stage.reset();
                slot.fade = if enabled { 1.0 } else { 0.0 };
            }
            slot.in_use = in_use;
            slot.enabled = enabled;
            slot.lane = lane.min(n - 1);
        }
    }

    /// Set slot `i`'s serial position within its lane: lower runs first,
    /// ties in pool order. Every slot starts at its own index, so a pool
    /// nobody orders runs in pool order (`fx.stack.params` — `order`).
    /// Allocation-free; re-sorts only on a change.
    pub fn set_order(&mut self, i: usize, order: usize) {
        let Some(slot) = self.slots.get_mut(i) else {
            return;
        };
        if slot.order == order {
            return;
        }
        slot.order = order;
        let slots = &self.slots;
        self.run.sort_unstable_by_key(|&k| (slots[k].order, k));
    }

    /// Set lane `l`'s mix controls. Call every block from the param sync.
    pub fn set_lane(&mut self, l: usize, ctl: LaneCtl) {
        if let Some(lane) = self.lanes.get_mut(l) {
            lane.ctl = ctl;
        }
    }

    /// Latency of lane `l`: enabled, in-use stages on it, serially
    /// (`fx.stack.latency`).
    fn lane_latency(&self, l: usize) -> usize {
        self.slots
            .iter()
            .filter(|s| s.in_use && s.enabled && s.lane == l)
            .map(|s| s.stage.latency())
            .sum()
    }

    /// Whether lane `l` is part of the topology at all.
    fn lane_populated(&self, l: usize) -> bool {
        self.slots.iter().any(|s| s.in_use && s.lane == l)
    }

    /// The stack's reported latency: the slowest populated lane.
    // r[impl fx.stack.latency]
    #[must_use]
    pub fn latency(&self) -> usize {
        (0..self.lanes.len())
            .filter(|&l| self.lane_populated(l))
            .map(|l| self.lane_latency(l))
            .max()
            .unwrap_or(0)
    }

    /// Re-derive lane alignment delays. Call after any latency-affecting
    /// change (topology or a stage's lookahead-style setting).
    pub fn update_alignment(&mut self) {
        let max = self.latency();
        for l in 0..self.lanes.len() {
            let lat = self.lane_latency(l);
            let d = max.saturating_sub(lat);
            let lane = &mut self.lanes[l];
            lane.delay.delay = d.min(lane.delay.buf_l.len().saturating_sub(1));
        }
    }

    /// Whether lane `l` currently sounds (solo beats mute across the
    /// populated lanes).
    fn lane_active(&self, l: usize) -> bool {
        if !self.lane_populated(l) {
            return false;
        }
        let any_solo =
            (0..self.lanes.len()).any(|k| self.lane_populated(k) && self.lanes[k].ctl.solo);
        if any_solo {
            self.lanes[l].ctl.solo
        } else {
            !self.lanes[l].ctl.mute
        }
    }

    /// Process one block in place — same law as [`crate::stack::Stack::process`].
    /// A pool with no populated lane passes the input through unchanged (an
    /// empty stack is a wire, not a mute — the plugin's bypass story).
    // r[impl fx.stack.process]
    // r[impl fx.stack.sum]
    pub fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        debug_assert!(self.prepared, "StagePool::process before prepare");
        debug_assert_eq!(l.len(), r.len());
        let frames = l.len().min(self.max_block);
        let n_lanes = self.lanes.len();

        if !(0..n_lanes).any(|k| self.lane_populated(k)) {
            return;
        }

        let n_active = (0..n_lanes).filter(|&k| self.lane_active(k)).count();
        let norm = self.sum_mode.norm(n_active) * self.output_trim;

        for lane_idx in 0..n_lanes {
            if !self.lane_populated(lane_idx) {
                continue;
            }
            // Fill the lane scratch with the input, then run its serial
            // stages in pool order.
            {
                let lane = &mut self.lanes[lane_idx];
                lane.scratch_l[..frames].copy_from_slice(&l[..frames]);
                lane.scratch_r[..frames].copy_from_slice(&r[..frames]);
            }
            for ri in 0..self.run.len() {
                let si = self.run[ri];
                if !(self.slots[si].in_use && self.slots[si].lane == lane_idx) {
                    continue;
                }
                let slot = &mut self.slots[si];
                let lane = &mut self.lanes[lane_idx];
                let scratch_l = &mut lane.scratch_l[..frames];
                let scratch_r = &mut lane.scratch_r[..frames];
                let target = if slot.enabled { 1.0 } else { 0.0 };
                if slot.fade == target {
                    if slot.enabled {
                        slot.stage.process(scratch_l, scratch_r);
                    }
                    continue;
                }
                // Bypass crossfade (`fx.gain-comp.continuity`).
                self.fade_l[..frames].copy_from_slice(scratch_l);
                self.fade_r[..frames].copy_from_slice(scratch_r);
                slot.stage.process(scratch_l, scratch_r);
                let step = 1.0 / FADE_SAMPLES;
                let dir = if slot.enabled { step } else { -step };
                let mut fade = slot.fade;
                for i in 0..frames {
                    fade = (fade + dir).clamp(0.0, 1.0);
                    scratch_l[i] = scratch_l[i].mul_add(fade, self.fade_l[i] * (1.0 - fade));
                    scratch_r[i] = scratch_r[i].mul_add(fade, self.fade_r[i] * (1.0 - fade));
                }
                slot.fade = fade;
                if slot.fade == 0.0 {
                    slot.stage.reset();
                }
            }
            let lane = &mut self.lanes[lane_idx];
            lane.delay
                .process(&mut lane.scratch_l[..frames], &mut lane.scratch_r[..frames]);
        }

        // Sum the audible lanes. DryOnce keeps the input and adds each
        // lane's difference from it, trimming the whole sum after.
        let dry_once = self.sum_mode == SumMode::DryOnce && n_active > 0;
        if dry_once {
            self.dry_l[..frames].copy_from_slice(&l[..frames]);
            self.dry_r[..frames].copy_from_slice(&r[..frames]);
        } else {
            l[..frames].iter_mut().for_each(|x| *x = 0.0);
            r[..frames].iter_mut().for_each(|x| *x = 0.0);
        }
        for lane_idx in 0..n_lanes {
            if !self.lane_active(lane_idx) {
                continue;
            }
            let lane = &self.lanes[lane_idx];
            let g = if dry_once {
                lane.ctl.gain
            } else {
                lane.ctl.gain * norm
            };
            let pan = lane.ctl.pan_gains();
            for i in 0..frames {
                let (mut ol, mut or) = (lane.scratch_l[i], lane.scratch_r[i]);
                if let Some((gl, gr)) = pan {
                    let mono = (ol + or) * 0.5;
                    (ol, or) = (mono * gl, mono * gr);
                }
                if dry_once {
                    ol -= self.dry_l[i];
                    or -= self.dry_r[i];
                }
                l[i] += ol * g;
                r[i] += or * g;
            }
        }
        if dry_once && self.output_trim != 1.0 {
            let t = self.output_trim;
            l[..frames].iter_mut().for_each(|x| *x *= t);
            r[..frames].iter_mut().for_each(|x| *x *= t);
        }
    }

    /// Reset every stage and delay line (parameters untouched).
    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.stage.reset();
            slot.fade = if slot.enabled { 1.0 } else { 0.0 };
        }
        for lane in &mut self.lanes {
            lane.delay.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stage that multiplies by a gain then adds an offset — order-sensitive.
    struct Affine {
        gain: f64,
        offset: f64,
    }

    impl Stage for Affine {
        fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
            for x in l.iter_mut().chain(r.iter_mut()) {
                *x = x.mul_add(self.gain, self.offset);
            }
        }
    }

    fn pool(stages: Vec<Affine>) -> StagePool<Affine> {
        let mut p = StagePool::new(stages);
        p.prepare(16, 0);
        p
    }

    fn run(p: &mut StagePool<Affine>, l: f64, r: f64) -> (f64, f64) {
        let (mut bl, mut br) = ([l; 4], [r; 4]);
        p.process(&mut bl, &mut br);
        (bl[3], br[3])
    }

    fn two() -> Vec<Affine> {
        vec![
            Affine {
                gain: 2.0,
                offset: 0.0,
            },
            Affine {
                gain: 1.0,
                offset: 1.0,
            },
        ]
    }

    #[test]
    fn single_stage_is_the_stage() {
        let mut p = pool(two());
        assert_eq!(run(&mut p, 0.25, -0.5), (0.5, -1.0));
    }

    #[test]
    fn order_swaps_a_lane_serial_chain() {
        let mut p = pool(two());
        p.set_slot(1, true, true, 0);
        // Pool order: ×2 then +1.
        assert_eq!(run(&mut p, 1.0, 1.0), (3.0, 3.0));
        // Slot 1 first: +1 then ×2.
        p.set_order(0, 1);
        p.set_order(1, 0);
        assert_eq!(run(&mut p, 1.0, 1.0), (4.0, 4.0));
    }

    #[test]
    fn pan_places_each_lane_mono_on_one_side() {
        let mut p = pool(two());
        p.set_slot(1, true, true, 1);
        p.sum_mode = SumMode::Raw;
        p.set_lane(
            0,
            LaneCtl {
                pan: -1.0,
                ..LaneCtl::default()
            },
        );
        p.set_lane(
            1,
            LaneCtl {
                pan: 1.0,
                ..LaneCtl::default()
            },
        );
        // Lane 0 (×2) mono → left only; lane 1 (+1) mono → right only.
        let (l, r) = run(&mut p, 1.0, 3.0);
        assert_eq!(l, (2.0 + 6.0) * 0.5);
        assert_eq!(r, (2.0 + 4.0) * 0.5);
    }

    #[test]
    fn dry_once_counts_the_dry_once() {
        let mut p = pool(two());
        p.set_slot(1, true, true, 1);
        p.sum_mode = SumMode::DryOnce;
        // a + b − dry, with a = 2x, b = x + 1.
        let x = 0.75;
        assert_eq!(run(&mut p, x, x).0, 2.0 * x + (x + 1.0) - x);
        // One lane: the lane, untouched.
        p.set_slot(1, false, true, 1);
        assert_eq!(run(&mut p, x, x).0, 2.0 * x);
    }
}
