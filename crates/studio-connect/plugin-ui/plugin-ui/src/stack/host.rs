//! The stack's editor side — spec `docs/spec/fx/stack.md`. The params, the
//! [`StackHost`] trait and [`sync_pool`] are `plugin_core::fx::stack_host`
//! (the plugin runs them with no editor), re-exported here:
//!
//! - [`FocusedStage`], [`add_stage`], [`remove_stage`], [`StackStrip`] — the
//!   focus signal, the shift-click gesture and the chip strip
//!   (`fx.stack.focus`, `fx.stack.add`, `fx.stack.strip`).

use std::sync::Arc;

use plugin_core::prelude::*;

pub use plugin_core::fx::stack_host::*;

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

/// The stage a component should edit: the focus, resolved to a stage still
/// in the stack ([`StackHost::visible_stage`]) — what the editor root shows.
#[must_use]
pub fn use_visible_stage(host: &dyn StackHost) -> usize {
    host.visible_stage(use_focused_stage())
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
        if on {
            "#0b0b0d"
        } else {
            "var(--muted-foreground)"
        },
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
        if focused {
            "#0b0b0d"
        } else {
            "var(--foreground)"
        },
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
