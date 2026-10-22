//! The performer screen's own panels (worship / gig screen spec §3.1, §5):
//! the now band, the transport's second row with its Speed and Level
//! peeks, the follow card that stands in for the transport while another
//! device drives playback, and the sync pill.
//!
//! Each reads the app's playback through [`use_playback`], so Signal and
//! Session mount the same panels over their own [`super::PlaybackModel`].
//! The components the spec reuses — Session's `SongProgressBar` and
//! `TransportControlBar`, the rig's `TouchSwitches`, perform-ui's
//! `TabletSetlist` — live in crates above this one; the app passes them
//! into the slots here (`NowBand::progress`, `PerformDock`'s halves).
//!
//! Touch rules (spec §8): every control is a real `button` with a label;
//! nothing waits on a timer; nothing scrolls; 48 px is the floor and the
//! dock's cells are 60. Inline styles only, and no `position: fixed`,
//! `overflow: auto` or `text-overflow` (Blitz's gaps, CLAUDE.md): long
//! titles are cut to a character budget instead ([`budget`]).

use dioxus::prelude::*;

use super::performer::{DOCK_ROW, GAP, HIT};
use super::playback::{
    NUDGE_S, PlaybackView, SPEEDS, clock, nudged, speed_label, stepped_level,
    use_playback,
};
use crate::tokens::{
    FOCUS_FG, FONT, INK, INK_2, INK_3, LIVE, RULE_STRONG, SHEET_2, pct, tint,
};

/// The one solid button on the screen (Load next, Take control): 5.2:1
/// under white.
pub const PRIMARY: &str = "#2563eb";
/// The loop and "following" colour: amber, as the shipped bars draw it.
pub const AMBER: &str = "#f59e0b";
/// Ink on amber (8.4:1).
pub const ON_AMBER: &str = "#1c1002";
/// A disabled control's opacity: dimmed, still readable.
const DIMMED: f64 = 0.45;

/// `s` cut to `max` characters with an ellipsis — Blitz has no
/// `text-overflow`, so the band and the rows cut by a budget (spec §7,
/// long content: the band's 28 px title holds about 22).
#[must_use]
pub fn budget(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut cut: String = s.chars().take(max.saturating_sub(1)).collect();
        cut.push('…');
        cut
    }
}

/// A plain dock or band button, as an inline style: `h` tall, filled, a
/// hairline round it.
fn cell(h: u32, on: bool, enabled: bool) -> String {
    let (bg, edge) = if on {
        ("rgba(255,255,255,0.1)", FOCUS_FG)
    } else {
        (SHEET_2, RULE_STRONG)
    };
    let opacity = if enabled { 1.0 } else { DIMMED };
    format!(
        "position: relative; display: flex; flex-direction: column; align-items: center; justify-content: center; \
         gap: 2px; min-width: 0; height: {h}px; padding: 0 6px; box-sizing: border-box; border-radius: 8px; \
         border: 1px solid {edge}; background: {bg}; color: {INK}; font-family: {FONT}; \
         opacity: {opacity}; cursor: pointer;"
    )
}

// ── Now band ───────────────────────────────────────────────────────────────

/// The now band (`perform.now`, 112 tall): the song up, its key, tempo and
/// the section playing, the clock, the song after it, and — under them —
/// the section progress bar.
///
/// `progress` is the app's `SongProgressBar` (Session's, which Signal
/// already imports), 44 px tall with its `loop_indicator` passed; without
/// one the band draws a plain section strip that takes no taps.
#[component]
pub fn NowBand(#[props(default)] progress: Option<Element>) -> Element {
    let playback = use_playback();
    let v = playback.view();
    let can = v.can_control;
    let at = v.current;
    let first = at.is_none_or(|i| i == 0);
    let last = v.next().is_none();
    let (prev_ok, next_ok) = (can && !first, can && !last);
    let ended = v.ended();
    let (p1, p2, p3) = (playback.clone(), playback.clone(), playback);
    let song = v.song().cloned();
    let title = song
        .as_ref()
        .map_or_else(|| "No song".to_string(), |s| budget(&s.title, 22));
    let number = at.map_or_else(|| "–".to_string(), |i| (i + 1).to_string());
    let colour = song
        .as_ref()
        .map_or_else(|| INK_3.to_string(), |s| s.colour.clone());
    let section = v.section().cloned();
    let next = v.next().map(|(i, s)| (i, s.clone()));
    let time = format!("{} / {}", clock(v.position), clock(v.duration));
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px; padding: 6px 12px; height: 100%; box-sizing: border-box;",
            div { style: "display: flex; align-items: center; gap: 12px; height: 48px; min-width: 0;",
                button {
                    "aria-label": "Previous song",
                    disabled: !prev_ok,
                    style: "{cell(HIT, false, prev_ok)} width: {HIT}px; flex-shrink: 0; font-size: 24px;",
                    onclick: move |_| if prev_ok { p1.step_song(-1) },
                    "‹"
                }
                span { style: "display: flex; align-items: center; justify-content: center; flex-shrink: 0; width: 32px; height: 32px; border-radius: 8px; background: {colour}; color: #0b0b0e; font-size: 18px; font-weight: 750;",
                    "{number}"
                }
                div { style: "display: flex; flex-direction: column; min-width: 0; gap: 2px;",
                    div { style: "display: flex; align-items: center; gap: 10px; min-width: 0;",
                        span { style: "font-size: 28px; font-weight: 750; line-height: 1.05; white-space: nowrap; color: {INK};", "{title}" }
                        if ended {
                            Tag { colour: INK_2, "ENDED" }
                        } else if v.playing {
                            Tag { colour: LIVE, dot: true, "NOW" }
                        }
                    }
                    div { style: "display: flex; align-items: center; gap: 10px; font-size: 16px; color: {INK_2}; white-space: nowrap;",
                        if let Some(s) = song.as_ref().filter(|s| !s.key.is_empty()) { span { "Key {s.key}" } }
                        if let Some(s) = song.as_ref().filter(|s| s.bpm > 0) { span { "{s.bpm} bpm" } }
                        if let Some(sec) = section.as_ref() {
                            span { style: "padding: 1px 6px; border-radius: 4px; font-size: 12px; font-weight: 750; background: {sec.colour}; color: #fafafa;", "{sec.name}" }
                        }
                    }
                }
                span { style: "flex: 1;" }
                span { style: "flex-shrink: 0; font-size: 28px; font-weight: 750; font-variant-numeric: tabular-nums; white-space: nowrap; color: {INK};", "{time}" }
                if let Some((i, n)) = next.clone() {
                    if ended && can {
                        button {
                            "aria-label": "Load next song: {n.title}",
                            style: "flex-shrink: 0; width: 300px; height: {HIT}px; border: none; border-radius: 8px; background: {PRIMARY}; color: #ffffff; font-family: {FONT}; font-size: 18px; font-weight: 700; white-space: nowrap; cursor: pointer;",
                            onclick: move |_| p3.load_song(i),
                            "Load next · {i + 1} {budget(&n.title, 16)}"
                        }
                    } else {
                        div { "aria-label": "Next song: {n.title}", style: "display: flex; align-items: center; gap: 8px; flex-shrink: 0; height: {HIT}px; padding: 0 12px; box-sizing: border-box; border-radius: 8px; border: 1px solid {RULE_STRONG}; background: {SHEET_2}; opacity: {pick_f(can, 1.0, DIMMED)}; white-space: nowrap;",
                            span { style: "font-size: 12px; font-weight: 750; letter-spacing: 0.08em; color: {INK_3};", "NEXT" }
                            span { style: "display: flex; align-items: center; justify-content: center; width: 22px; height: 22px; border-radius: 5px; background: {n.colour}; color: #0b0b0e; font-size: 13px; font-weight: 750;", "{i + 1}" }
                            span { style: "font-size: 18px; font-weight: 700; color: {INK};", "{budget(&n.title, 14)}" }
                            if !n.key.is_empty() || n.bpm > 0 {
                                span { style: "font-size: 14px; color: {INK_2};", "{n.key} · {n.bpm}" }
                            }
                        }
                    }
                }
                button {
                    "aria-label": "Next song",
                    disabled: !next_ok,
                    style: "{cell(HIT, false, next_ok)} width: {HIT}px; flex-shrink: 0; font-size: 24px;",
                    onclick: move |_| if next_ok { p2.step_song(1) },
                    "›"
                }
            }
            div { style: "display: flex; height: 44px; min-width: 0; opacity: {pick_f(can, 1.0, 0.7)};",
                if let Some(bar) = progress {
                    {bar}
                } else {
                    SectionStrip { view: v.clone() }
                }
            }
        }
    }
}

fn pick_f(c: bool, a: f64, b: f64) -> f64 {
    if c { a } else { b }
}

/// A word badge: NOW (with its green dot), ENDED, PLAYED.
#[component]
fn Tag(colour: &'static str, #[props(default)] dot: bool, children: Element) -> Element {
    rsx! {
        span { style: "display: flex; align-items: center; gap: 5px; flex-shrink: 0; padding: 2px 8px; border-radius: 999px; background: {tint(colour, 16)}; color: {colour}; font-size: 12px; font-weight: 750; letter-spacing: 0.08em;",
            if dot { span { style: "width: 7px; height: 7px; border-radius: 999px; background: {colour};" } }
            {children}
        }
    }
}

/// The song's sections as a read-only strip, the playhead over it: what
/// the band shows when the app gives it no progress bar.
#[component]
fn SectionStrip(view: PlaybackView) -> Element {
    let sections = view.song().map(|s| s.sections.clone()).unwrap_or_default();
    let total = if view.duration > 0.0 {
        view.duration
    } else {
        sections.last().map_or(1.0, |s| s.end.max(1.0))
    };
    let head = (view.position / total).clamp(0.0, 1.0) * 100.0;
    let label = format!("{} of {}", clock(view.position), clock(view.duration));
    rsx! {
        div { role: "progressbar", "aria-label": "Song position, {label}",
            style: "position: relative; display: flex; flex: 1; gap: 2px; min-width: 0; height: 44px; border-radius: 6px; overflow: hidden; background: {SHEET_2};",
            for (k, s) in sections.iter().enumerate() {
                {
                    let played = view.position >= s.end;
                    let here = view.position >= s.start && view.position < s.end;
                    let w = ((s.end - s.start) / total * 100.0).max(0.0);
                    let bg = if played || here { s.colour.clone() } else { tint(&s.colour, 38) };
                    let ring = if here { format!("box-shadow: 0 0 0 2px {FOCUS_FG};") } else { String::new() };
                    rsx! {
                        div { key: "{k}", style: "display: flex; align-items: center; width: {w}%; min-width: 0; padding: 0 8px; box-sizing: border-box; overflow: hidden; background: {bg}; {ring} font-size: 14px; font-weight: 750; color: #fafafa; white-space: nowrap;",
                            "{s.name}"
                        }
                    }
                }
            }
            span { style: "position: absolute; top: 0; bottom: 0; left: {head}%; width: 4px; margin-left: -2px; background: #ffffff; border-left: 1px solid #000; border-right: 1px solid #000; box-sizing: border-box;" }
        }
    }
}

// ── The transport's second row ─────────────────────────────────────────────

/// Which peek is open over the nudge row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Peek {
    Speed,
    Level,
}

/// The transport's second row (60 tall): −5 s, +5 s, Speed, Level, ⋯.
/// Speed and Level open a small peek over the row ([`SpeedMenu`],
/// [`LevelPeek`]) instead of cycling on every tap.
///
/// `more` opens the row's ⋯ menu (count-in, click, output: the app's,
/// and only for what its engine has); without it the ⋯ is not drawn.
#[component]
pub fn TransportNudgeRow(#[props(default)] more: Option<EventHandler<()>>) -> Element {
    let playback = use_playback();
    let v = playback.view();
    let can = v.can_control;
    let mut peek = use_signal(|| None::<Peek>);
    let (b, f, s, l) = (
        playback.clone(),
        playback.clone(),
        playback.clone(),
        playback,
    );
    let (pos, dur) = (v.position, v.duration);
    let cols = if more.is_some() { 5 } else { 4 };
    rsx! {
        div { style: "position: relative; display: grid; grid-template-columns: repeat({cols}, minmax(0, 1fr)); gap: {GAP}px; height: {DOCK_ROW}px;",
            button {
                "aria-label": "Back {NUDGE_S} seconds",
                disabled: !can,
                style: "{cell(DOCK_ROW, false, can)} font-size: 18px; font-weight: 650;",
                onclick: move |_| if can { b.seek(nudged(pos, dur, -NUDGE_S)) },
                "−5 s"
            }
            button {
                "aria-label": "Forward {NUDGE_S} seconds",
                disabled: !can,
                style: "{cell(DOCK_ROW, false, can)} font-size: 18px; font-weight: 650;",
                onclick: move |_| if can { f.seek(nudged(pos, dur, NUDGE_S)) },
                "+5 s"
            }
            div { style: "position: relative; display: flex; flex-direction: column; min-width: 0;",
                button {
                    "aria-label": "Speed, {speed_label(v.speed)}",
                    "aria-expanded": "{peek() == Some(Peek::Speed)}",
                    disabled: !can,
                    style: "{cell(DOCK_ROW, peek() == Some(Peek::Speed), can)}",
                    onclick: move |_| if can { flip(peek, Peek::Speed) },
                    span { style: "font-size: 12px; font-weight: 750; letter-spacing: 0.08em; color: {INK_2};", "SPEED" }
                    span { style: "font-size: 18px; font-weight: 700; font-variant-numeric: tabular-nums;", "{speed_label(v.speed)}" }
                }
                if peek() == Some(Peek::Speed) {
                    SpeedMenu {
                        speed: v.speed,
                        on_pick: move |rate: f64| {
                            s.set_speed(rate);
                            peek.set(None);
                        },
                        on_close: move |()| peek.set(None),
                    }
                }
            }
            div { style: "position: relative; display: flex; flex-direction: column; min-width: 0;",
                button {
                    "aria-label": "Level, {pct(v.level)}",
                    "aria-expanded": "{peek() == Some(Peek::Level)}",
                    style: "{cell(DOCK_ROW, peek() == Some(Peek::Level), true)}",
                    onclick: move |_| flip(peek, Peek::Level),
                    span { style: "font-size: 12px; font-weight: 750; letter-spacing: 0.08em; color: {INK_2};", "LEVEL" }
                    span { style: "font-size: 18px; font-weight: 700; font-variant-numeric: tabular-nums;", "{pct(v.level)}" }
                }
                if peek() == Some(Peek::Level) {
                    LevelPeek {
                        level: v.level,
                        on_set: move |lv: f64| l.set_level(lv),
                        on_close: move |()| peek.set(None),
                    }
                }
            }
            if let Some(more) = more {
                button {
                    "aria-label": "More playback options",
                    style: "{cell(DOCK_ROW, false, true)} font-size: 22px; font-weight: 700;",
                    onclick: move |_| more.call(()),
                    "⋯"
                }
            }
        }
    }
}

/// Open a peek, or close it when it is the one open.
fn flip(mut peek: Signal<Option<Peek>>, p: Peek) {
    let now = *peek.peek();
    peek.set(if now == Some(p) { None } else { Some(p) });
}

/// The popover's frame: over the button it opened from, under nothing
/// (it is absolute in its cell, so it never covers the app bar).
const PEEK: &str = "position: absolute; bottom: 68px; z-index: 20; display: flex; flex-direction: column; \
                    gap: 8px; padding: 8px; box-sizing: border-box; border-radius: 10px; \
                    border: 1px solid #2b2b31; background: #141418;";

/// The Speed peek: the gig shell's speeds as six buttons, the one in use
/// ringed. A pick sets it and closes.
#[component]
pub fn SpeedMenu(speed: f64, on_pick: EventHandler<f64>, on_close: EventHandler<()>) -> Element {
    rsx! {
        div { role: "menu", "aria-label": "Playback speed", style: "{PEEK} left: 0; width: 252px;",
            div { style: "display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: {GAP}px;",
                for rate in SPEEDS {
                    {
                        let on = (rate - speed).abs() < 1e-6;
                        rsx! {
                            button {
                                key: "{rate}",
                                role: "menuitemradio",
                                "aria-checked": "{on}",
                                "aria-label": "Speed {speed_label(rate)}",
                                style: "{cell(HIT, on, true)} font-size: 16px; font-weight: 700;",
                                onclick: move |_| on_pick.call(rate),
                                "{speed_label(rate)}"
                            }
                        }
                    }
                }
            }
            button {
                "aria-label": "Close speed",
                style: "{cell(HIT, false, true)} font-size: 15px; font-weight: 650; color: {INK_2};",
                onclick: move |_| on_close.call(()),
                "Done"
            }
        }
    }
}

/// The Level peek: the level as a bar, − and + of 48 px either side, and
/// 100% to put it back. Nothing is dragged, so a brush changes nothing.
#[component]
pub fn LevelPeek(level: f64, on_set: EventHandler<f64>, on_close: EventHandler<()>) -> Element {
    let fill = pct(level);
    rsx! {
        div { role: "group", "aria-label": "Playback level", style: "{PEEK} right: 0; width: 300px;",
            div { style: "display: flex; align-items: center; gap: {GAP}px;",
                button {
                    "aria-label": "Level down",
                    style: "{cell(HIT, false, true)} width: {HIT}px; font-size: 22px;",
                    onclick: move |_| on_set.call(stepped_level(level, -1)),
                    "−"
                }
                div { role: "meter", "aria-label": "Level {fill}%", "aria-valuenow": "{fill}", "aria-valuemin": "0", "aria-valuemax": "100",
                    style: "position: relative; flex: 1; height: 12px; border-radius: 6px; background: #08080a; overflow: hidden;",
                    div { style: "height: 100%; width: {fill}%; background: {INK_2};" }
                }
                button {
                    "aria-label": "Level up",
                    style: "{cell(HIT, false, true)} width: {HIT}px; font-size: 22px;",
                    onclick: move |_| on_set.call(stepped_level(level, 1)),
                    "+"
                }
            }
            div { style: "display: flex; gap: {GAP}px;",
                button {
                    "aria-label": "Level to 100%",
                    style: "{cell(HIT, false, true)} flex: 1; font-size: 15px; font-weight: 650;",
                    onclick: move |_| on_set.call(1.0),
                    "{fill}% · reset"
                }
                button {
                    "aria-label": "Close level",
                    style: "{cell(HIT, false, true)} flex: 1; font-size: 15px; font-weight: 650; color: {INK_2};",
                    onclick: move |_| on_close.call(()),
                    "Done"
                }
            }
        }
    }
}

// ── Following a peer ───────────────────────────────────────────────────────

/// Stands in the transport's box while another device drives playback
/// (the sync-locked state, spec §7): who drives it, that the rig is still
/// yours, and Take control — which asks once before it takes it.
#[component]
pub fn FollowCard() -> Element {
    let playback = use_playback();
    let v = playback.view();
    let peer = v.following.clone().unwrap_or_else(|| "another device".to_string());
    let mut asking = use_signal(|| false);
    rsx! {
        div { role: "group", "aria-label": "Following {peer}",
            style: "display: flex; align-items: center; gap: 12px; flex: 1; min-width: 0; padding: 0 16px; box-sizing: border-box; border-radius: 10px; border: 2px solid {AMBER}; background: {tint(AMBER, 10)};",
            div { style: "display: flex; flex-direction: column; gap: 4px; flex: 1; min-width: 0;",
                span { style: "font-size: 18px; font-weight: 750; color: {INK}; white-space: nowrap;", "Following {budget(&peer, 18)}" }
                span { style: "font-size: 14px; color: {INK_2};",
                    if asking() { "Take control from {budget(&peer, 18)}?" } else { "Playback is controlled there. Your rig stays yours." }
                }
            }
            if asking() {
                button {
                    "aria-label": "Take control from {peer}",
                    style: "height: {HIT}px; padding: 0 18px; border: none; border-radius: 8px; background: {PRIMARY}; color: #ffffff; font-family: {FONT}; font-size: 16px; font-weight: 700; cursor: pointer;",
                    onclick: move |_| {
                        asking.set(false);
                        playback.take_control();
                    },
                    "Take control"
                }
                button {
                    "aria-label": "Keep following {peer}",
                    style: "{cell(HIT, false, true)} padding: 0 18px; font-size: 16px; font-weight: 650;",
                    onclick: move |_| asking.set(false),
                    "Cancel"
                }
            } else {
                button {
                    "aria-label": "Take control",
                    style: "height: {HIT}px; padding: 0 18px; border: none; border-radius: 8px; background: {PRIMARY}; color: #ffffff; font-family: {FONT}; font-size: 16px; font-weight: 700; cursor: pointer;",
                    onclick: move |_| asking.set(true),
                    "Take control"
                }
            }
        }
    }
}

/// What the sync pill says, and in which colour.
#[must_use]
pub fn sync_state(v: &PlaybackView) -> (String, &'static str) {
    match (&v.following, v.peers) {
        (Some(peer), _) => (format!("Following {}", budget(peer, 16)), AMBER),
        (None, 0) => ("No peers".to_string(), INK_3),
        (None, 1) => ("1 peer".to_string(), LIVE),
        (None, n) => (format!("{n} peers"), LIVE),
    }
}

/// The app bar's sync pill: how many peers are connected, or whom this
/// device follows (amber). A tap opens the app's sync details.
#[component]
pub fn SyncPill(#[props(default)] on_open: Option<EventHandler<()>>) -> Element {
    let v = use_playback().view();
    let (words, colour) = sync_state(&v);
    let ink = if colour == AMBER { ON_AMBER } else { INK };
    let bg = if colour == AMBER {
        AMBER.to_string()
    } else {
        "rgba(255,255,255,0.08)".to_string()
    };
    rsx! {
        button {
            "aria-label": "Sync: {words}",
            style: "display: flex; align-items: center; gap: 8px; height: 36px; margin: 6px 0; padding: 0 14px; border: none; border-radius: 999px; background: {bg}; color: {ink}; font-family: {FONT}; font-size: 15px; font-weight: 650; white-space: nowrap; cursor: pointer;",
            onclick: move |_| if let Some(open) = on_open { open.call(()) },
            if colour != AMBER {
                span { style: "width: 8px; height: 8px; border-radius: 999px; background: {colour};" }
            }
            "{words}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_cuts_long_titles_with_an_ellipsis() {
        assert_eq!(budget("Lantern Hymn", 22), "Lantern Hymn");
        assert_eq!(budget("A Very Long Song Title That Keeps Going", 10), "A Very Lo…");
        assert_eq!(budget("Été", 3), "Été", "counts characters, not bytes");
    }

    #[test]
    fn the_pill_says_peers_or_whom_it_follows() {
        let mut v = PlaybackView::default();
        assert_eq!(sync_state(&v).0, "No peers");
        v.peers = 2;
        assert_eq!(sync_state(&v), ("2 peers".to_string(), LIVE));
        v.following = Some("Studio Mac".into());
        assert_eq!(sync_state(&v), ("Following Studio Mac".to_string(), AMBER));
    }
}
