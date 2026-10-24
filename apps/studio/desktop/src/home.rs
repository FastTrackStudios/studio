//! Studio's home: the first screen, before either side has started.
//!
//! A launch is one of two things: **play here** (a rig, a setlist, a
//! project on this device) or **join** a session that is happening
//! elsewhere (a live link, a REAPER bridge, the demo). The left column is
//! the first, the right the second, with the projects opened lately under
//! it; upright (an iPad held portrait) the right column wraps under the
//! left and the page scrolls.
//!
//! Nothing here runs an engine: a process runs one (docs/design/one-engine.md),
//! and it is the side a press goes into that starts it. So everything shown
//! is what this device has on disk — the rigs this build plays, the gigs in
//! its gigs folder, Session's recent list, the account's kept session. A
//! press hands its choice to the side it opens ([`crate::rigs::store_last`],
//! [`crate::gig::remember`], `session_desktop::native::handoff`) and goes in.
//!
//! The values are Signal Touch's (`rigs.rs`, `docs/signal/DESIGN.md`), so
//! the home and the rig menu behind it read as one app.

use dioxus::prelude::*;

use crate::front::Side;

const DESK: &str = "#0a0a0c";
const SHEET: &str = "#0e0e11";
const RULE: &str = "#222228";
const RULE_STRONG: &str = "#2b2b31";
const INK: &str = "#e4e4e7";
const INK_2: &str = "#a1a1aa";
const INK_3: &str = "#80808a";
const DIM: &str = "#3f3f46";
const LIVE: &str = "#22c55e";
const LIVE_BG: &str = "rgba(34, 197, 94, 0.10)";
const PRIMARY: &str = "#2563eb";
const FONT: &str =
    "-apple-system, 'SF Pro Text', system-ui, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif";
const PRESS_CSS: &str = ".h-press:not(:disabled):active { transform: scale(0.985); \
    background-image: linear-gradient(rgba(255,255,255,0.07), rgba(255,255,255,0.07)) !important; } \
    .h-press:focus-visible { outline: 2px solid #bfdbfe; outline-offset: -2px; }";

/// How many recent projects the home lists; Session's start screen has all.
const RECENT: usize = 6;

/// The home. `opening` is the side a press is going into (its engine may
/// take a moment on an iPad); `on_pick` goes in.
#[component]
pub fn Home(opening: Option<Side>, on_pick: EventHandler<Side>) -> Element {
    let account = crate::account::provide_account();
    rsx! {
        div {
            // Sized from the viewport: Blitz gives a box stretched only by
            // percentages of an unsized parent no height.
            style: "position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; \
                    display: flex; flex-direction: column; overflow: hidden; \
                    background: {DESK}; color: {INK}; font-family: {FONT}; \
                    font-variant-numeric: tabular-nums;",
            document::Style { {PRESS_CSS} }
            Bar { on_account: move |()| account.open() }
            div {
                style: "flex: 1; min-height: 0; overflow-y: scroll;",
                div {
                    style: "display: flex; flex-wrap: wrap; align-items: flex-start; gap: 22px; \
                            padding: 22px; box-sizing: border-box;",
                    div {
                        style: "flex: 3 1 460px; min-width: 0; display: flex; flex-direction: column; gap: 14px;",
                        PlayHere { on_pick }
                    }
                    div {
                        style: "flex: 2 1 320px; min-width: 0; display: flex; flex-direction: column; gap: 14px;",
                        Elsewhere { on_pick }
                    }
                }
            }
            if let Some(side) = opening {
                Opening { side }
            }
            crate::account::AccountOverlay { top: 56, width: 1210 }
        }
    }
}

/// The bar: the wordmark, and the account (signed in, or a Sign in).
/// Room on the bar's left for the window's buttons, which sit over the page
/// on a Mac (the title bar is drawn through); none on an iPad.
const BUTTONS: u32 = if cfg!(target_os = "macos") { 84 } else { 22 };

#[component]
fn Bar(on_account: EventHandler<()>) -> Element {
    let state = crate::account::use_signal_account();
    let signed = match (state.phase)() {
        fts_account::ui::Phase::In { account, .. } => Some(account),
        _ => None,
    };
    rsx! {
        div {
            style: "flex-shrink: 0; height: 56px; display: flex; align-items: center; gap: 16px; \
                    padding: 0 22px 0 {BUTTONS}px; border-bottom: 1px solid {RULE_STRONG}; background: {SHEET}; \
                    box-sizing: border-box;",
            span { style: "font-size: 24px; font-weight: 750; letter-spacing: -0.02em;", "studio" }
            span { style: "flex: 1;" }
            button {
                class: "h-press",
                style: "height: 40px; display: flex; align-items: center; gap: 10px; padding: 0 14px 0 6px; \
                        border: 1px solid {RULE_STRONG}; border-radius: 20px; background: transparent; \
                        color: {INK}; font-family: {FONT}; font-size: 15px; font-weight: 650; cursor: pointer;",
                onclick: move |_| on_account.call(()),
                if let Some(account) = signed {
                    span {
                        style: "width: 28px; height: 28px; border-radius: 14px; display: flex; \
                                align-items: center; justify-content: center; background: #1e3a8a; \
                                color: #dbeafe; font-size: 12px; font-weight: 800;",
                        "{account.initials()}"
                    }
                    span { style: "white-space: nowrap;", "{account.name()}" }
                } else {
                    span {
                        style: "width: 28px; height: 28px; border-radius: 14px; display: flex; \
                                align-items: center; justify-content: center; border: 1px solid {DIM};",
                        svg { width: "14", height: "14", view_box: "0 0 14 14",
                            circle { cx: "7", cy: "5", r: "2.6", fill: "none", stroke: INK_2, stroke_width: "1.4" }
                            path { d: "M2.2 12.6c.7-2.4 2.6-3.6 4.8-3.6s4.1 1.2 4.8 3.6", fill: "none", stroke: INK_2, stroke_width: "1.4", stroke_linecap: "round" }
                        }
                    }
                    span { style: "white-space: nowrap;", "Sign in" }
                }
            }
        }
    }
}

// ── Play here ───────────────────────────────────────────────────────────────

/// Where you were (one press from playing again), the other rigs, and the
/// setlists on this device.
#[component]
fn PlayHere(on_pick: EventHandler<Side>) -> Element {
    rsx! {
        OnNow { on_pick }
        Label { text: "Play here" }
        Rigs { on_pick }
        Label { text: "Setlists" }
        Setlists { on_pick }
    }
}

#[cfg(any(feature = "signal", feature = "signal-guitar"))]
#[component]
fn Rigs(on_pick: EventHandler<Side>) -> Element {
    use crate::rigs::{Rig, available, last_played};

    // Bass sits by the guitar it is a mode of; the rest follow the catalogue.
    let mut ready: Vec<Rig> = Rig::ALL.iter().copied().filter(|r| available(*r)).collect();
    ready.sort_by_key(|r| match r {
        Rig::Guitar => 0,
        Rig::Bass => 1,
        Rig::Drums => 2,
        _ => 3,
    });
    let coming: Vec<&'static str> = Rig::ALL
        .iter()
        .copied()
        .filter(|r| !available(*r))
        .map(Rig::label)
        .collect();
    let last = last_played()
        .filter(|r| ready.contains(r))
        .or_else(|| ready.first().copied());
    let others: Vec<Rig> = ready.iter().copied().filter(|r| Some(*r) != last).collect();
    let open = move |rig: Rig| {
        crate::rigs::store_last(Some(rig));
        #[cfg(feature = "signal")]
        crate::open_rig_workspace();
        on_pick.call(Side::Signal);
    };
    rsx! {
        if let Some(rig) = last {
            button {
                class: "h-press",
                "aria-label": "Resume {rig.label()}",
                style: "width: 100%; box-sizing: border-box; flex-shrink: 0; display: flex; align-items: center; gap: 18px; \
                        padding: 20px 22px; border: 1px solid #1f5135; border-radius: 10px; \
                        background: {LIVE_BG}; color: {INK}; font-family: {FONT}; text-align: left; cursor: pointer;",
                onclick: move |_| open(rig),
                span { style: "width: 12px; height: 12px; border-radius: 6px; flex-shrink: 0; background: {LIVE};" }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                    span { style: "font-size: 13px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: {LIVE};",
                        if last_played() == Some(rig) { "Resume" } else { "Start" }
                    }
                    span { style: "font-size: 30px; font-weight: 800; letter-spacing: -0.015em;", "{rig.label()}" }
                    span { style: "font-size: 15px; color: {INK_2}; overflow: hidden; white-space: nowrap;", "{rig.blurb()}" }
                }
                span {
                    style: "flex-shrink: 0; height: 48px; display: flex; align-items: center; padding: 0 22px; \
                            border-radius: 8px; background: {LIVE}; color: #04130a; font-size: 17px; font-weight: 800;",
                    "Play"
                }
            }
        }
        if !others.is_empty() {
            // Three across in both orientations: an 11" iPad's column is
            // ~680pt on its side and ~790pt upright.
            div { style: "flex-shrink: 0; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px;",
                for rig in others {
                    button {
                        key: "{rig.slug()}",
                        class: "h-press",
                        "aria-label": "Open {rig.label()}",
                        style: "min-width: 0; height: 76px; box-sizing: border-box; display: flex; flex-direction: column; \
                                justify-content: center; align-items: flex-start; gap: 4px; padding: 0 16px; \
                                border: 1px solid {RULE}; border-radius: 10px; background: {SHEET}; color: {INK}; \
                                font-family: {FONT}; text-align: left; cursor: pointer;",
                        onclick: move |_| open(rig),
                        span { style: "font-size: 19px; font-weight: 750; letter-spacing: -0.01em;", "{rig.label()}" }
                        span { style: "max-width: 100%; font-size: 13px; color: {INK_3}; overflow: hidden; white-space: nowrap;", "{rig.blurb()}" }
                    }
                }
            }
        }
        if !coming.is_empty() {
            div { style: "flex-shrink: 0; display: flex; align-items: baseline; gap: 10px; padding: 0 4px; white-space: nowrap; overflow: hidden;",
                span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};", "Coming" }
                span { style: "font-size: 14px; color: {INK_3}; overflow: hidden;", "{coming.join(\" · \")}" }
            }
        }
    }
}

#[cfg(not(any(feature = "signal", feature = "signal-guitar")))]
#[component]
fn Rigs(on_pick: EventHandler<Side>) -> Element {
    let _ = on_pick;
    rsx! {}
}

/// The setlists on this device: for now its gig folders (a set, its charts
/// and practice tracks). The account's setlists join them once they sync
/// (docs/design/sync.md).
#[cfg(feature = "gig")]
#[component]
fn Setlists(on_pick: EventHandler<Side>) -> Element {
    let gigs = use_hook(crate::gig::list_gigs);
    if gigs.is_empty() {
        return rsx! {
            Quiet { text: "No setlists on this device yet. Signed in, your account's arrive here." }
        };
    }
    rsx! {
        Card {
            for (i, gig) in gigs.into_iter().enumerate() {
                Row {
                    key: "{gig.name}",
                    first: i == 0,
                    title: gig.name.clone(),
                    detail: gig_line(&gig),
                    on_press: {
                        let name = gig.name.clone();
                        move |()| {
                            open_gig(&name);
                            on_pick.call(Side::Signal);
                        }
                    },
                }
            }
        }
    }
}

#[cfg(not(feature = "gig"))]
#[component]
fn Setlists(on_pick: EventHandler<Side>) -> Element {
    let _ = on_pick;
    rsx! { Quiet { text: "No setlists on this device yet." } }
}

/// A gig opens as the guitar rig with the gig's set loaded in it (the
/// iPad's gig shell adds its charts and practice tracks around the rig).
#[cfg(feature = "gig")]
fn open_gig(name: &str) {
    crate::gig::remember(name);
    crate::gig::load_on_entry();
    #[cfg(any(feature = "signal", feature = "signal-guitar"))]
    crate::rigs::store_last(Some(crate::rigs::Rig::Guitar));
    #[cfg(feature = "signal")]
    crate::open_rig_workspace();
}

/// How long the home waits before opening a gig that is on now.
#[cfg(feature = "gig")]
const COUNTDOWN: u32 = 5;

/// A gig that is on now (its soundcheck or show, from [`schedule::LEAD`]
/// before it until it ends): opened by itself after [`COUNTDOWN`] seconds,
/// unless the player stays on the home — remembered for that slot, so a
/// relaunch does not push again.
///
/// [`schedule::LEAD`]: crate::gig::schedule::LEAD
#[cfg(feature = "gig")]
#[component]
fn OnNow(on_pick: EventHandler<Side>) -> Element {
    use crate::gig::schedule;

    let due = use_hook(|| {
        let now = chrono::Utc::now();
        crate::gig::list_gigs().into_iter().find_map(|gig| {
            let slot = schedule::on_now(&gig.schedule, now)?.clone();
            Some((gig.name, slot))
        })
    });
    let Some((name, slot)) = due else {
        return rsx! {};
    };
    let key = slot.key(&name);
    let stayed = utils::prefs::get_raw("gig-stayed").as_deref() == Some(key.as_str());
    let mut left = use_signal(|| (!stayed).then_some(COUNTDOWN));
    let open = {
        let name = name.clone();
        move || {
            open_gig(&name);
            on_pick.call(Side::Signal);
        }
    };
    use_future({
        let open = open.clone();
        move || {
            let open = open.clone();
            async move {
                loop {
                    futures_timer::Delay::new(std::time::Duration::from_secs(1)).await;
                    let Some(n) = *left.peek() else { return };
                    if n <= 1 {
                        left.set(None);
                        open();
                        return;
                    }
                    left.set(Some(n - 1));
                }
            }
        }
    });
    let when = slot.when(chrono::Utc::now());
    let line = if slot.place.is_empty() {
        when
    } else {
        format!("{when} · {}", slot.place)
    };
    rsx! {
        div {
            style: "flex-shrink: 0; display: flex; flex-direction: column; gap: 14px; padding: 20px 22px; \
                    border: 1px solid #1e40af; border-radius: 10px; background: #0f1a33;",
            span { style: "font-size: 13px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: #93c5fd;",
                "On now"
            }
            div { style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
                span { style: "font-size: 30px; font-weight: 800; letter-spacing: -0.015em; overflow: hidden; white-space: nowrap;", "{name}" }
                span { style: "font-size: 16px; color: {INK_2}; overflow: hidden; white-space: nowrap;", "{line}" }
            }
            div { style: "display: flex; align-items: center; gap: 10px;",
                button {
                    class: "h-press",
                    style: "height: 48px; padding: 0 24px; border: none; border-radius: 8px; background: {PRIMARY}; \
                            color: #ffffff; font-family: {FONT}; font-size: 17px; font-weight: 800; cursor: pointer;",
                    onclick: {
                        let open = open.clone();
                        move |_| {
                            left.set(None);
                            open();
                        }
                    },
                    if let Some(n) = left() { "Opening in {n}…" } else { "Open the set" }
                }
                if left().is_some() {
                    button {
                        class: "h-press",
                        style: "height: 48px; padding: 0 20px; border: 1px solid {RULE_STRONG}; border-radius: 8px; \
                                background: transparent; color: {INK}; font-family: {FONT}; font-size: 16px; \
                                font-weight: 650; cursor: pointer;",
                        onclick: move |_| {
                            left.set(None);
                            utils::prefs::set_raw("gig-stayed", &key);
                        },
                        "Stay here"
                    }
                }
            }
        }
    }
}

#[cfg(not(feature = "gig"))]
#[component]
fn OnNow(on_pick: EventHandler<Side>) -> Element {
    let _ = on_pick;
    rsx! {}
}

/// A gig's line under its name: how many songs, and when it is played
/// (its next slot when it has a schedule, else its show date when it
/// names one, else its first).
#[cfg(feature = "gig")]
fn gig_line(gig: &crate::gig::Gig) -> String {
    let songs = match gig.songs.len() {
        1 => "1 song".to_owned(),
        n => format!("{n} songs"),
    };
    if let Some(slot) = crate::gig::schedule::next(&gig.schedule, chrono::Utc::now()) {
        return format!("{songs} · {}", slot.when(chrono::Utc::now()));
    }
    let when = gig
        .dates
        .iter()
        .find(|d| d.starts_with("Show"))
        .or_else(|| gig.dates.first())
        .map(|d| d.split_once(": ").map_or(d.as_str(), |(_, rest)| rest));
    match when {
        Some(when) => format!("{songs} · {when}"),
        None => songs,
    }
}

// ── Join, and the projects ──────────────────────────────────────────────────

#[cfg(feature = "session-app")]
#[component]
fn Elsewhere(on_pick: EventHandler<Side>) -> Element {
    use session_desktop::native::handoff;

    let mut link = use_signal(String::new);
    let join = move |text: String| {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        handoff::join_on_start(text);
        on_pick.call(Side::Session);
    };
    let empty = link().trim().is_empty();
    let join_opacity = if empty { "0.45" } else { "1" };
    let recent = use_hook(handoff::recent);
    let shown = recent.len().min(RECENT);
    rsx! {
        Label { text: "Join" }
        Card {
            div { style: "display: flex; flex-direction: column; gap: 12px; padding: 18px 20px;",
                span { style: "font-size: 20px; font-weight: 800; letter-spacing: -0.01em;", "A session happening elsewhere" }
                span { style: "font-size: 14px; color: {INK_3}; line-height: 1.4;",
                    "Paste a live link someone shared, or a REAPER bridge's address."
                }
                div { style: "display: flex; gap: 10px;",
                    input {
                        r#type: "text",
                        "aria-label": "Live link or address",
                        value: "{link}",
                        style: "flex: 1; min-width: 0; height: 48px; padding: 0 14px; border: 1px solid {RULE_STRONG}; \
                                border-radius: 8px; background: {DESK}; color: {INK}; font-family: {FONT}; font-size: 16px; \
                                box-sizing: border-box;",
                        oninput: move |e| link.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                join(link());
                            }
                        },
                    }
                    button {
                        class: "h-press",
                        disabled: empty,
                        style: "flex-shrink: 0; height: 48px; padding: 0 22px; border: none; border-radius: 8px; \
                                background: {PRIMARY}; color: #ffffff; font-family: {FONT}; font-size: 16px; \
                                font-weight: 750; cursor: pointer; opacity: {join_opacity};",
                        onclick: move |_| join(link()),
                        "Join"
                    }
                }
            }
            Row {
                first: false,
                title: "The demo".to_owned(),
                detail: "A live set anyone can join, reset every so often".to_owned(),
                on_press: move |()| join("demo".to_owned()),
            }
        }
        Label { text: "Projects" }
        Card {
            Row {
                first: true,
                title: "New project".to_owned(),
                detail: "Or open one from Session's library".to_owned(),
                on_press: move |()| on_pick.call(Side::Session),
            }
            for song in recent.into_iter().take(shown) {
                Row {
                    key: "{song.opened.display()}",
                    first: false,
                    title: song.name(),
                    detail: match song.opened_alternative() {
                        Some(alt) => format!("{alt} · {}", handoff::ago(song.at)),
                        None => handoff::ago(song.at),
                    },
                    on_press: {
                        let path = song.opened.clone();
                        move |()| {
                            handoff::open_next(path.clone());
                            on_pick.call(Side::Session);
                        }
                    },
                }
            }
        }
        if shown == 0 {
            Quiet { text: "Projects you open show up here." }
        }
    }
}

#[cfg(not(feature = "session-app"))]
#[component]
fn Elsewhere(on_pick: EventHandler<Side>) -> Element {
    let _ = on_pick;
    rsx! {}
}

// ── Pieces ──────────────────────────────────────────────────────────────────

#[component]
fn Label(text: &'static str) -> Element {
    rsx! {
        div { style: "flex-shrink: 0; padding: 6px 4px 0; font-size: 12px; font-weight: 700; \
                      letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};",
            "{text}"
        }
    }
}

#[component]
fn Card(children: Element) -> Element {
    rsx! {
        div { style: "flex-shrink: 0; display: flex; flex-direction: column; border: 1px solid {RULE}; \
                      border-radius: 10px; background: {SHEET}; overflow: hidden;",
            {children}
        }
    }
}

/// One press-to-open row: a title, a line under it, a chevron.
#[component]
fn Row(first: bool, title: String, detail: String, on_press: EventHandler<()>) -> Element {
    let edge = if first {
        "none".to_owned()
    } else {
        format!("1px solid {RULE}")
    };
    rsx! {
        button {
            class: "h-press",
            "aria-label": "Open {title}",
            style: "width: 100%; min-height: 68px; flex-shrink: 0; display: flex; align-items: center; gap: 16px; \
                    padding: 12px 20px; border: none; border-top: {edge}; background: transparent; color: {INK}; \
                    font-family: {FONT}; text-align: left; cursor: pointer; box-sizing: border-box;",
            onclick: move |_| on_press.call(()),
            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "font-size: 19px; font-weight: 750; letter-spacing: -0.01em; overflow: hidden; white-space: nowrap;", "{title}" }
                span { style: "font-size: 14px; color: {INK_3}; overflow: hidden; white-space: nowrap;", "{detail}" }
            }
            svg { width: "10", height: "16", view_box: "0 0 10 16", style: "width: 10px; height: 16px; flex-shrink: 0;",
                path { d: "M2 2l6 6-6 6", fill: "none", stroke: INK_3, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
            }
        }
    }
}

#[component]
fn Quiet(text: &'static str) -> Element {
    rsx! {
        div { style: "flex-shrink: 0; padding: 14px 4px; font-size: 14px; line-height: 1.4; color: {INK_3};", "{text}" }
    }
}

/// Over the page while a side starts: what is opening.
#[component]
fn Opening(side: Side) -> Element {
    rsx! {
        div {
            style: "position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; display: flex; \
                    align-items: center; justify-content: center; background: rgba(10, 10, 12, 0.82);",
            span { style: "font-size: 20px; font-weight: 700; color: {INK_2};", "Opening {side.name()}…" }
        }
    }
}
