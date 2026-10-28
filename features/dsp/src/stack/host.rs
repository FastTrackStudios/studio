//! The stack's host side, shared by every kind — spec `docs/spec/fx/stack.md`.
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
//!   below can read and write it; [`sync_pool`] pushes its topology into a
//!   [`StagePool`] once per block.
//! - [`FocusedStage`], [`add_stage`], [`remove_stage`], [`StackStrip`] — the
//!   focus signal, the shift-click gesture and the chip strip
//!   (`fx.stack.focus`, `fx.stack.add`, `fx.stack.strip`).

use std::sync::Arc;

use plugin_core::prelude::*;

use crate::stack::{LaneCtl, Stage, StagePool, SumMode};

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
    IntParam::new("Sum Mode", 0, IntRange::Linear { min: 0, max: 2 }).with_value_to_string(
        Arc::new(|v| {
            SUM_MODE_LABELS
                .get(v.max(0) as usize)
                .map_or_else(|| v.to_string(), ToString::to_string)
        }),
    )
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

// ── Focus ─────────────────────────────────────────────────────────────────

/// Which stage the editor is editing (`fx.stack.focus`), 0-based. Provided at
/// the editor root; UI state — the session persists it through
/// [`StackHost::focused_cell`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FocusedStage(pub usize);

/// The focused stage index, or stage 0 where no provider is mounted (unit
/// tests, isolated components).
#[must_use]
pub fn use_focused_stage() -> usize {
    match try_consume_context::<Signal<FocusedStage>>() {
        Some(sig) => sig.read().0,
        None => 0,
    }
}

/// The focus signal itself, for components that *set* focus.
#[must_use]
pub fn use_focus_signal() -> Option<Signal<FocusedStage>> {
    try_consume_context::<Signal<FocusedStage>>()
}

// ── Gestures ──────────────────────────────────────────────────────────────

fn set_bool(ctx: &ParamContext, param: &BoolParam, v: bool) {
    let ptr = param.as_ptr();
    ctx.begin_set_raw(ptr);
    ctx.set_normalized_raw(ptr, if v { 1.0 } else { 0.0 });
    ctx.end_set_raw(ptr);
}

fn set_int(ctx: &ParamContext, param: &IntParam, v: i32) {
    let ptr = param.as_ptr();
    ctx.begin_set_raw(ptr);
    ctx.set_normalized_raw(ptr, param.preview_normalized(v));
    ctx.end_set_raw(ptr);
}

/// The lane a shift-click adds to (`fx.stack.add`): the focused stage's
/// (serial), or with Ctrl/Cmd a fresh one (parallel). `None` when every
/// lane is taken.
#[must_use]
pub fn add_target_lane(host: &dyn StackHost, focused: usize, new_lane: bool) -> Option<usize> {
    if new_lane {
        host.first_free_lane()
    } else {
        Some(host.placement(focused).lane_index())
    }
}

/// Add a stage of style `index` to the stack: the first free pool slot, on
/// `lane`, focused (`fx.stack.add`). Returns the slot, or `None` when the
/// pool is full.
pub fn add_stage(
    host: &dyn StackHost,
    ctx: &ParamContext,
    mut focus: Signal<FocusedStage>,
    index: usize,
    lane: usize,
) -> Option<usize> {
    let slot = host.first_free_stage()?;
    set_int(ctx, host.style_param(slot), index as i32);
    host.store_style_id(slot, index);
    let p = host.placement(slot);
    set_int(ctx, &p.lane, lane as i32);
    set_bool(ctx, &p.stage_on, true);
    set_bool(ctx, &p.in_use, true);
    host.store_focused_stage(slot);
    focus.set(FocusedStage(slot));
    Some(slot)
}

/// Remove stage `idx` from the stack; focus moves to the first remaining
/// stage. The last stage cannot be removed (`fx.stack.strip`).
pub fn remove_stage(
    host: &dyn StackHost,
    ctx: &ParamContext,
    mut focus: Signal<FocusedStage>,
    idx: usize,
) {
    if host.stages_in_use().len() <= 1 {
        return;
    }
    set_bool(ctx, &host.placement(idx).in_use, false);
    if focus.peek().0 == idx {
        let next = (0..host.max_stages())
            .find(|&i| i != idx && host.placement(i).in_use.value())
            .unwrap_or(0);
        host.store_focused_stage(next);
        focus.set(FocusedStage(next));
    }
}

/// The in-use stages grouped by lane, lanes in index order, each lane's
/// stages in pool order — the strip's topology order.
#[must_use]
pub fn lanes_in_use(host: &dyn StackHost) -> Vec<(usize, Vec<usize>)> {
    let mut lanes: Vec<(usize, Vec<usize>)> = Vec::new();
    for si in host.stages_in_use() {
        let lane = host.placement(si).lane_index();
        match lanes.iter_mut().find(|(l, _)| *l == lane) {
            Some((_, v)) => v.push(si),
            None => lanes.push((lane, vec![si])),
        }
    }
    lanes.sort_by_key(|(l, _)| *l);
    lanes
}

// ── The strip ─────────────────────────────────────────────────────────────

/// A kind's param tree, as a component prop (compared by pointer).
#[derive(Clone)]
pub struct StackHandle(pub Arc<dyn StackHost>);

impl PartialEq for StackHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The stack strip (`fx.stack.strip`): one chip per stage in topology
/// order, lanes separated by a divider carrying the lane's mute/solo.
/// Renders nothing while the stack is a single stage — the plain plugin
/// look. Gestures:
///
/// - click a chip: focus that stage (`fx.stack.focus`)
/// - Alt-click a chip: bypass / enable the stage (crossfaded)
/// - click a chip's ×: remove the stage
/// - click a lane's M/S: mute / solo the lane
///
/// Everything a chip writes is a host parameter, through the same gesture
/// path as any knob. Needs a [`FocusedStage`] signal in context.
#[component]
pub fn StackStrip(host: StackHandle, frame: u64) -> Element {
    let _ = frame;
    let ctx = use_param_context();
    let Some(focus) = use_focus_signal() else {
        return rsx! {};
    };
    let host = host.0;
    if host.stages_in_use().len() <= 1 {
        return rsx! {};
    }
    let focused = focus.read().0;
    let lanes = lanes_in_use(host.as_ref());
    let parallel = lanes.len() > 1;

    rsx! {
        div {
            "data-testid": "stack-strip",
            // Centred without a transform and stacked without z-index —
            // both break blitz hit-testing (see the comp's strip). Mount it
            // as the LAST sibling of the face so document order puts it on
            // top.
            style: "position:absolute; top:30px; right:12px; \
                    width:max-content; \
                    display:flex; align-items:center; gap:6px; \
                    padding:4px 8px; border-radius:8px; \
                    background:color-mix(in oklab, var(--card, #101216) 88%, transparent); \
                    border:1px solid var(--border, rgba(148,163,184,0.3));",

            for (lane_pos, (lane, stages)) in lanes.iter().enumerate() {
                if lane_pos > 0 {
                    div {
                        style: "width:1px; height:22px; \
                                background:var(--border, rgba(148,163,184,0.4));",
                    }
                }
                if parallel {
                    {
                        let lp = host.lane(*lane);
                        let muted = lp.mute.value();
                        let soloed = lp.solo.value();
                        let (ctx_m, ctx_s) = (ctx.clone(), ctx.clone());
                        let (host_m, host_s) = (host.clone(), host.clone());
                        let lane = *lane;
                        rsx! {
                            div {
                                style: "display:flex; flex-direction:column; gap:2px;",
                                span {
                                    "data-testid": "lane-mute-{lane}",
                                    style: lane_dot_style(muted, "#e5484d"),
                                    title: "Mute lane",
                                    onmousedown: move |_| {
                                        let lp = host_m.lane(lane);
                                        set_bool(&ctx_m, &lp.mute, !lp.mute.value());
                                    },
                                    "M"
                                }
                                span {
                                    "data-testid": "lane-solo-{lane}",
                                    style: lane_dot_style(soloed, "#f5d90a"),
                                    title: "Solo lane",
                                    onmousedown: move |_| {
                                        let lp = host_s.lane(lane);
                                        set_bool(&ctx_s, &lp.solo, !lp.solo.value());
                                    },
                                    "S"
                                }
                            }
                        }
                    }
                }

                for &si in stages.iter() {
                    {
                        let style_idx = host.resolved_style(si);
                        let badge = host.style_badge(style_idx);
                        let name = host.style_name(style_idx);
                        let enabled = host.placement(si).stage_on.value();
                        let is_focused = si == focused;
                        let (ctx_chip, ctx_x) = (ctx.clone(), ctx.clone());
                        let (host_chip, host_x) = (host.clone(), host.clone());
                        let mut focus_chip = focus;
                        let focus_x = focus;
                        rsx! {
                            div {
                                "data-testid": "stage-chip-{si + 1}",
                                "data-focused": "{is_focused}",
                                "data-enabled": "{enabled}",
                                title: format!(
                                    "Stage {} — {name} · click: edit · Alt-click: bypass",
                                    si + 1,
                                ),
                                style: chip_style(is_focused, enabled),
                                // mousedown, not click: blitz's click
                                // synthesis is unreliable for overlapped
                                // floats. The × stops propagation so a
                                // remove does not also refocus.
                                onmousedown: move |evt: MouseEvent| {
                                    if evt.modifiers().alt() {
                                        let p = host_chip.placement(si);
                                        set_bool(&ctx_chip, &p.stage_on, !p.stage_on.value());
                                        return;
                                    }
                                    host_chip.store_focused_stage(si);
                                    focus_chip.set(FocusedStage(si));
                                },
                                "{badge}"
                                span {
                                    "data-testid": "stage-remove-{si + 1}",
                                    style: "font-size:9px; opacity:0.7; cursor:pointer; padding:0 1px;",
                                    title: "Remove stage",
                                    onmousedown: move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        remove_stage(host_x.as_ref(), &ctx_x, focus_x, si);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn lane_dot_style(on: bool, colour: &str) -> String {
    format!(
        "font-size:8px; font-weight:800; cursor:pointer; \
         padding:0 3px; border-radius:3px; color:{}; background:{};",
        if on { "#0b0b0d" } else { "var(--muted-foreground)" },
        if on { colour } else { "transparent" },
    )
}

fn chip_style(focused: bool, enabled: bool) -> String {
    format!(
        "display:flex; align-items:center; gap:4px; cursor:pointer; \
         padding:3px 6px; border-radius:6px; font-size:10px; \
         font-weight:{}; letter-spacing:0.02em; \
         color:{}; background:{}; border:1px solid {}; opacity:{};",
        if focused { 800 } else { 600 },
        if focused { "#0b0b0d" } else { "var(--foreground)" },
        if focused {
            "var(--primary, #c8a86e)"
        } else {
            "color-mix(in oklab, var(--primary, #c8a86e) 12%, transparent)"
        },
        if focused {
            "var(--primary, #c8a86e)"
        } else {
            "var(--border, rgba(148,163,184,0.3))"
        },
        if enabled { "1.0" } else { "0.5" },
    )
}
