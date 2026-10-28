//! Stepping through the song a page of bars at a time.
//!
//! The view is filled with a few bars (2, 4, 8 …) of one instrument's
//! tracks — the kit, by default — and moved on by that many bars at a
//! press, start to end.
//!
//! What a pass over a drum edit wants: every hit looked at once, close
//! enough to see where it lands against the grid, without zooming and
//! scrolling by hand between pages. The page is counted from bar 1 (so 4
//! bars a page is 1–4, 5–8 …), and the edit cursor goes to its start while
//! the transport is stopped, so Play plays the page.
//!
//! `SESSION_STEP` is the one way in: the transport bar's buttons, `[` and
//! `]`, and a model all run it.

#[cfg(feature = "native")]
use architect::action::{ActionList, ActionMeta, Effect};
use dioxus::prelude::*;
#[cfg(feature = "native")]
use facet::Facet;

/// How much of the bars either side a page shows, as a part of a bar: a
/// beat of 4/4.

/// The page sizes the buttons offer, bars.
pub const SIZES: [i32; 5] = [1, 2, 4, 8, 16];

/// The instruments a page can frame, as the visibility manager names its
/// groups: (key, label).
pub const GROUPS: [(&str, &str); 5] = [
    ("drums", "Drums"),
    ("bass", "Bass"),
    ("guitars", "Guitars"),
    ("vocals", "Vocals"),
    ("", "All tracks"),
];

/// Where the stepping is: how many bars a page, which instrument, and the
/// page shown (its first bar), once one has been.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub bars: i32,
    /// The visibility manager's group key (`drums`); empty for every track.
    pub group: String,
    pub first: Option<i32>,
}

static STEP: std::sync::Mutex<Option<Step>> = std::sync::Mutex::new(None);

/// The page framed last: its bars' seconds, and what was put on screen
/// (the bars and a beat either side).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Page {
    pub from: f64,
    pub to: f64,
    pub shown: (f64, f64),
}

static PAGE: std::sync::Mutex<Option<Page>> = std::sync::Mutex::new(None);

/// The page framed last, if one has been.
#[must_use]
pub fn page() -> Option<Page> {
    PAGE.lock().ok().and_then(|p| *p)
}

/// A page asked for by playback and not yet framed: one ask at a time.
static TURNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Playback at `at` has left the page on screen: the page holding it,
/// as `]` would turn to it — the view goes in steps with the song rather
/// than scrolling. Once until that page is framed.
pub fn turn_to(at: f64) {
    if TURNING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    run(serde_json::json!({ "at": at }));
}

/// The stepping as it is now.
#[must_use]
pub fn get() -> Step {
    STEP.lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or_else(|| Step {
            bars: 4,
            group: "drums".to_owned(),
            first: None,
        })
}

#[cfg(feature = "native")]
fn set(step: Step) {
    if let Ok(mut slot) = STEP.lock() {
        *slot = Some(step);
    }
}

#[cfg(feature = "native")]
pub static STEP_ACTION: ActionMeta = crate::actions::meta(
    "SESSION_STEP",
    "step",
    "Step through the song",
    "Fill the view with a page of bars of one instrument's tracks and step it on: `by` pages (+1 the next, −1 the one before, 0 or left out the page the edit cursor is in); `bars` a page (1, 2, 4, 8, 16 — sticky, 4 to begin with); `group` the visibility manager's group framed down the view (`drums`, `bass`, `guitars`, `vocals`; empty for every track — sticky, drums to begin with); `bar` to go straight to the page holding that bar, `at` the page holding that moment (seconds). Pages are counted from bar 1. While stopped the edit cursor goes to the page's start, so Play plays it; while playing with follow on, the view turns to the next page as the song reaches it. Says the page.",
    "View",
    "[ / ]",
);

/// How to step.
#[cfg(feature = "native")]
#[derive(Facet)]
struct StepArg {
    by: Option<i32>,
    bars: Option<i32>,
    group: Option<String>,
    bar: Option<i32>,
    at: Option<f64>,
}

/// The page shown.
#[cfg(feature = "native")]
#[derive(Facet)]
struct StepOut {
    first_bar: i32,
    last_bar: i32,
    bars: i32,
    group: String,
    from: f64,
    to: f64,
}

/// `bar` moved on by `n` bars as the song counts them: no bar 0.
const fn add_bars(bar: i32, n: i32) -> i32 {
    let moved = bar.saturating_add(n);
    if bar < 0 && moved >= 0 {
        moved.saturating_add(1)
    } else if bar > 0 && moved <= 0 {
        moved.saturating_sub(1)
    } else {
        moved
    }
}

#[cfg(feature = "native")]
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&STEP_ACTION, Effect::Mutating, |a: StepArg| {
        let mut step = get();
        if let Some(bars) = a.bars {
            if !SIZES.contains(&bars) {
                return Err(format!("`bars` is one of {SIZES:?}"));
            }
            step.bars = bars;
        }
        if let Some(group) = a.group {
            step.group = dynamic_template::visibility::normalize_key(&group);
        }
        // The page as the engine works it out (`SESSION_PAGE`): it has the
        // tempo map, the markers and the tracks.
        let ask = serde_json::json!({
            "bars": step.bars,
            "group": step.group,
            "by": a.by,
            "bar": a.bar,
            "at": a.at,
            "first": step.first,
            "resized": a.bars.is_some(),
        });
        let said = crate::actions::run("SESSION_PAGE", ask.to_string().as_bytes())?;
        let page: crate::page::PageOut =
            facet_json::from_slice(&said).map_err(|e| format!("SESSION_PAGE: {e}"))?;
        step.first = Some(page.first_bar);
        set(step.clone());
        if let Ok(mut shown) = PAGE.lock() {
            *shown = Some(Page {
                from: page.from,
                to: page.to,
                shown: (page.shown_from, page.shown_to),
            });
        }
        TURNING.store(false, std::sync::atomic::Ordering::Release);
        let us = |s: f64| num_traits::ToPrimitive::to_i64(&(s * 1_000_000.0).round()).unwrap_or(0);
        crate::window_queue::queue(crate::window_queue::Action::Frame {
            time: Some((us(page.shown_from), us(page.shown_to))),
            tracks: page.tracks,
        });
        Ok(StepOut {
            first_bar: page.first_bar,
            last_bar: page.last_bar,
            bars: step.bars,
            group: step.group,
            from: page.from,
            to: page.to,
        })
    })?;
    Ok(())
}

/// Run `SESSION_STEP` with `args`, off the window's thread.
fn run(args: serde_json::Value) {
    #[cfg(feature = "native")]
    if let Some(runtime) = crate::open::runtime() {
        runtime.spawn_blocking(move || {
            if let Err(e) =
                crate::actions::list().run_json(STEP_ACTION.id, args.to_string().as_bytes())
            {
                tracing::warn!(step.error = %e, "the step was refused");
            }
            TURNING.store(false, std::sync::atomic::Ordering::Release);
        });
    }
    #[cfg(not(feature = "native"))]
    let _ = args;
}

/// How often the buttons read the stepping again (a key or a model may
/// have stepped), ms.
const READ_EVERY_MS: u64 = 150;

/// The quantize grid at the play cursor while editing (` · Grid 1/16`),
/// nothing otherwise ([`crate::grid_in_force`]).
fn grid_here() -> String {
    crate::grid_in_force::current().map_or_else(String::new, |in_force| {
        let at = crate::engine::Transport::shared().map_or(0.0, |t| t.read().0);
        format!(
            " · Grid {}",
            crate::track_regions::grid_name(in_force.at(at))
        )
    })
}

/// The stepper in the transport bar: the page before, the page size and
/// instrument (a menu), the next page. `big` for a finger.
#[component]
pub fn Stepper(big: bool) -> Element {
    use crate::shell::{DIM, RULE, SCRIM, TEXT};
    use dioxus::prelude::*;
    let mut step = use_signal(get);
    let mut grid = use_signal(grid_here);
    let mut menu = use_signal(|| false);
    use_future(move || async move {
        loop {
            let now = get();
            if *step.peek() != now {
                step.set(now);
            }
            let here = grid_here();
            if *grid.peek() != here {
                grid.set(here);
            }
            architect::platform::sleep(std::time::Duration::from_millis(READ_EVERY_MS)).await;
        }
    });
    let now = step();
    let label = GROUPS
        .iter()
        .find(|(key, _)| *key == now.group)
        .map_or("All tracks", |(_, label)| label);
    let page = now.first.map_or_else(String::new, |first| {
        let last = add_bars(first, now.bars.saturating_sub(1));
        format!(" · {first}–{last}")
    });
    let (wide, tall, size) = if big {
        (44, "100%", 13)
    } else {
        (28, "24px", 11)
    };
    let arrow = format!(
        "width:{wide}px; height:{tall}; padding:0; border:none; background:transparent; \
         color:{TEXT}; font-size:{size}px; cursor:pointer;"
    );
    let menu_side = if big { "bottom:100%;" } else { "top:100%;" };
    let option = |on: bool| {
        let (ink, wash) = if on {
            ("#34d399", "#34d3991f")
        } else {
            (TEXT, "transparent")
        };
        format!(
            "text-align:left; padding:6px 10px; border:none; border-radius:6px; font-size:12px; \
             cursor:pointer; color:{ink}; background:{wash};"
        )
    };
    rsx! {
        div {
            style: "position:relative; height:100%; flex:none; display:flex; align-items:center;",
            button {
                style: "{arrow}",
                title: "The page of bars before ([)",
                onclick: move |_| run(serde_json::json!({ "by": -1 })),
                "◀"
            }
            button {
                style: "height:{tall}; padding:0 8px; border:none; background:transparent; \
                        color:{DIM}; font-size:{size}px; cursor:pointer; white-space:nowrap; \
                        font-family:ui-monospace, monospace;",
                title: "Step through the song a page of bars at a time: the page size and what fills the view",
                onclick: move |_| menu.set(true),
                "{now.bars} bars · {label}{page}{grid}"
            }
            button {
                style: "{arrow}",
                title: "The next page of bars (])",
                onclick: move |_| run(serde_json::json!({ "by": 1 })),
                "▶"
            }
            if menu() {
                div { style: "{SCRIM} z-index:40;", onclick: move |_| menu.set(false) }
                div {
                    style: "position:absolute; left:0; {menu_side} z-index:41; min-width:180px; \
                            padding:6px; background:#17181b; border:1px solid {RULE}; \
                            border-radius:10px; display:flex; flex-direction:column; gap:2px; \
                            box-shadow:0 10px 30px rgba(0,0,0,0.5);",
                    div { style: "padding:4px 10px; font-size:10px; color:{DIM};", "BARS A PAGE" }
                    for bars in SIZES {
                        button {
                            key: "bars-{bars}",
                            style: option(now.bars == bars),
                            onclick: move |_| {
                                menu.set(false);
                                run(serde_json::json!({ "bars": bars }));
                            },
                            "{bars}"
                        }
                    }
                    div { style: "padding:8px 10px 4px; font-size:10px; color:{DIM};", "FILL THE VIEW WITH" }
                    for (key, name) in GROUPS {
                        button {
                            key: "group-{name}",
                            style: option(now.group == key),
                            onclick: move |_| {
                                menu.set(false);
                                run(serde_json::json!({ "group": key }));
                            },
                            "{name}"
                        }
                    }
                }
            }
        }
    }
}
