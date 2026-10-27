//! A session: **Review** (its mixes, version by version, played with the
//! comments pinned to the moments they are about: the page a client
//! gets) and **Status** (its production steps, Write to Master, each with
//! its subtasks per part).
//!
//! The page is a shell frame (`app_shell::ui`), so its panels (the
//! comments, the versions, what is next) sit at an edge and move to
//! another one (each panel's ⇄), and the layout is remembered.

use app_shell::ui::{
    Panel, PanelRegistry, Region, ShellFrame, use_panel_registry, use_shell_state,
};
use dioxus::prelude::*;
use lucide_dioxus::{Check, FolderOpen, Link, Pause, Play};

use super::home::Page;
use super::kit::*;
use super::{Open, edit, open, use_library, use_wide};
use crate::model::{Edit, Note, SessionId, clock};

/// Which of the session's two pages is up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Review,
    Status,
}

/// The session the panels are about, and its player.
#[derive(Clone, Copy)]
struct Here {
    id: SessionId,
    /// The mix version under review.
    version: Signal<u32>,
    /// Where the playhead is, in seconds.
    at: Signal<f32>,
    playing: Signal<bool>,
}

#[component]
pub(crate) fn SessionPage(id: SessionId) -> Element {
    let lib = use_library();
    let wide = use_wide();
    // Hooks first: the session may not have arrived yet.
    let latest = lib
        .read()
        .session(id)
        .and_then(|s| s.mixes.last().map(|m| m.n))
        .unwrap_or(0);
    let here = use_context_provider(|| Here {
        id,
        version: Signal::new(latest),
        at: Signal::new(0.0),
        playing: Signal::new(false),
    });
    let mut tab = use_signal(|| if latest > 0 { Tab::Review } else { Tab::Status });
    use_shell_state("library.session");
    use_panel_registry(|| {
        PanelRegistry::new()
            .with(Panel::new(
                "session.comments",
                "Comments",
                Region::Right,
                CommentsPanel,
            ))
            .with(Panel::new(
                "session.versions",
                "Versions",
                Region::Left,
                VersionsPanel,
            ))
            .with(Panel::new(
                "session.next",
                "Next up",
                Region::Bottom,
                NextPanel,
            ))
    });
    // The player's clock: a quarter-second tick while playing.
    let mut at = here.at;
    let mut playing = here.playing;
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(250)).await;
            if *playing.peek() {
                let length = 291.0;
                let next = *at.peek() + 0.25;
                if next >= length {
                    at.set(length);
                    playing.set(false);
                } else {
                    at.set(next);
                }
            }
        }
    });
    let l = lib.read();
    let Some(session) = l.session(id).cloned() else {
        return rsx! { Page { p { style: "{BODY} color: {INK_2}; padding-top: 40px;", "This session was deleted or has not arrived yet." } } };
    };
    let in_project = l.project_of_session(id).map(|p| p.id);
    let pad = if wide { 32 } else { 20 };
    let stage = session.stage;
    let open_notes = session.notes.iter().filter(|n| !n.resolved).count();
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            div { style: "flex-shrink: 0; padding: 18px {pad}px 0; border-bottom: 1px solid {RULE_STRONG}; background: {DESK};",
                div { style: "display: flex; align-items: flex-end; gap: 16px; flex-wrap: wrap;",
                    div { style: "flex: 1 1 320px; min-width: 0; display: flex; flex-direction: column; gap: 6px;",
                        super::project::InProject { id: in_project }
                        span { style: "{TITLE}", "{session.title}" }
                        span { style: "{META} color: {INK_3};",
                            "{stage.label()} · {session.tracks} tracks · {session.mixes.len()} mixes · last changed by {session.edited.by}, {session.edited.when}"
                        }
                    }
                    div { style: "display: flex; gap: 10px;",
                        Button { label: "Review link".to_owned(), weight: Weight::Secondary,
                            icon: rsx! { Link { size: 16, color: INK } }, on_press: move |()| {} }
                        Button { label: "Open in Session".to_owned(), weight: Weight::Primary,
                            icon: rsx! { FolderOpen { size: 16, color: "#fff" } }, on_press: move |()| open(Open::Session(id)) }
                    }
                }
                div { style: "display: flex; gap: 4px; margin-top: 14px;",
                    TabButton { label: "Review", badge: (open_notes > 0).then(|| open_notes.to_string()), on: tab() == Tab::Review, on_press: move |()| tab.set(Tab::Review) }
                    TabButton { label: "Status", badge: Some(stage.label().to_owned()), on: tab() == Tab::Status, on_press: move |()| tab.set(Tab::Status) }
                }
            }
            ShellFrame {
                main: match tab() {
                    Tab::Review => rsx! { Review { session: session.clone() } },
                    Tab::Status => rsx! { Status { session: session.clone() } },
                },
            }
        }
    }
}

#[component]
fn TabButton(
    label: &'static str,
    badge: Option<String>,
    on: bool,
    on_press: EventHandler<()>,
) -> Element {
    let (ink, edge, weight) = if on {
        (INK, PICK, 700)
    } else {
        (INK_2, "transparent", 560)
    };
    rsx! {
        button {
            class: "lp",
            style: "height: 46px; padding: 0 14px; border: none; border-bottom: 2px solid {edge}; border-radius: 0; \
                    background: transparent; color: {ink}; font-family: {FONT}; font-size: 15px; font-weight: {weight}; \
                    display: flex; align-items: center; gap: 8px;",
            onclick: move |_| on_press.call(()),
            "{label}"
            if let Some(b) = badge {
                span { style: "height: 20px; padding: 0 7px; border-radius: 10px; background: {SHEET_2}; border: 1px solid {RULE}; \
                               {META} font-size: 12px; color: {INK_2}; display: flex; align-items: center;", "{b}" }
            }
        }
    }
}

// ── Review ──────────────────────────────────────────────────────────────────

/// The mix under review: its waveform with the playhead and the comments'
/// marks, the transport, and what changed in this version.
#[component]
fn Review(session: crate::model::Session) -> Element {
    let here = use_context::<Here>();
    let mut at = here.at;
    let mut playing = here.playing;
    let version = (here.version)();
    let Some(mix) = session.mixes.iter().find(|m| m.n == version).cloned() else {
        return rsx! {
            div { style: "flex: 1; display: flex; align-items: center; justify-content: center; padding: 40px;",
                div { style: "max-width: 420px; text-align: center; display: flex; flex-direction: column; gap: 8px;",
                    span { style: "{HEADING}", "No mixes yet" }
                    span { style: "{META} color: {INK_3}; line-height: 1.5;",
                        "When the first mix is printed it shows up here, and everyone on the project, clients included, can listen and comment at the moment they mean."
                    }
                }
            }
        };
    };
    let now = at();
    let latest = session.mixes.last().map(|m| m.n) == Some(mix.n);
    let marks: Vec<(f32, String, bool, bool)> = session
        .notes
        .iter()
        .filter(|n| n.version == mix.n || !n.resolved)
        .map(|n| {
            (
                n.at / mix.seconds,
                n.author.clone(),
                n.client,
                n.version == mix.n,
            )
        })
        .collect();
    let bins = mix.peaks.len().max(1);
    let played = ((now / mix.seconds) * bins as f32) as usize;
    let length = mix.seconds;
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; overflow-y: scroll;",
            div { style: "max-width: 1100px; margin: 0 auto; padding: 22px 24px 40px; display: flex; flex-direction: column; gap: 18px;",
                div { style: "display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap;",
                    span { style: "{HEADING} font-size: 20px;", "Mix v{mix.n}" }
                    if latest { Chip { text: "Latest".to_owned(), strong: true } }
                    span { style: "{META} color: {INK_3};", "{mix.note} · {mix.by}, {mix.when}" }
                }
                // The waveform: a press seeks there.
                div {
                    style: "position: relative; height: 150px; border-radius: 14px; background: {SHEET}; border: 1px solid {RULE_STRONG}; \
                            padding: 18px 14px; box-sizing: border-box;",
                    div { style: "position: relative; width: 100%; height: 100%; display: flex; align-items: center; gap: 2px;",
                        for (i, p) in mix.peaks.iter().enumerate() {
                            {
                                let h = (p * 100.0).max(4.0);
                                let c = if i < played { PICK } else { "#3f3f46" };
                                // A press on a bar seeks to it.
                                let seek = i as f32 / bins as f32 * length;
                                rsx! {
                                    span { key: "{i}", style: "flex: 1 1 0; min-width: 0; height: 100%; display: flex; align-items: center; cursor: pointer;",
                                        onclick: move |_| at.set(seek),
                                        span { style: "width: 100%; height: {h}%; border-radius: 2px; background: {c};" }
                                    }
                                }
                            }
                        }
                        // The comments' marks, along the top edge.
                        for (i, (x, who, client, this)) in marks.iter().cloned().enumerate() {
                            {
                                let left = x * 100.0;
                                let ring = if client { WARN } else { PICK };
                                let alpha = if this { "1" } else { "0.45" };
                                rsx! {
                                    span { key: "m{i}", style: "position: absolute; top: -14px; left: {left}%; margin-left: -9px; opacity: {alpha}; \
                                                                border: 2px solid {ring}; border-radius: 12px; display: flex;",
                                        Avatar { name: who, size: 16 }
                                    }
                                }
                            }
                        }
                        span { style: "position: absolute; top: -6px; bottom: -6px; left: {now / length * 100.0}%; width: 2px; background: {INK};" }
                    }
                }
                div { style: "display: flex; align-items: center; gap: 16px;",
                    button {
                        class: "lp lp-solid",
                        "aria-label": if playing() { "Pause" } else { "Play" },
                        style: "width: 64px; height: 64px; border-radius: 32px; border: none; background: {PRIMARY}; \
                                display: flex; align-items: center; justify-content: center;",
                        onclick: move |_| playing.toggle(),
                        if playing() { Pause { size: 26, color: "#fff", fill: "#fff" } } else { Play { size: 26, color: "#fff", fill: "#fff" } }
                    }
                    div { style: "display: flex; flex-direction: column; gap: 2px;",
                        span { style: "font-size: 22px; font-weight: 700;", "{clock(now)}" }
                        span { style: "{META} color: {INK_3};", "of {clock(length)}" }
                    }
                    span { style: "flex: 1;" }
                    span { style: "display: flex; align-items: center; gap: 8px; {META} color: {INK_3};",
                        span { style: "width: 10px; height: 10px; border-radius: 5px; border: 2px solid {PICK};" } "band"
                        span { style: "width: 10px; height: 10px; border-radius: 5px; border: 2px solid {WARN}; margin-left: 8px;" } "client"
                    }
                }
                p { style: "{META} color: {INK_3}; line-height: 1.5; max-width: 640px;",
                    "Anyone with the review link (a client, the label) hears these mixes and comments at a moment; they see this page and its comments, nothing else."
                }
            }
        }
    }
}

// ── Status ──────────────────────────────────────────────────────────────────

/// The production steps, Write to Master: where the session is, and each
/// step's subtasks to tick.
#[component]
fn Status(session: crate::model::Session) -> Element {
    let id = session.id;
    let current = session.steps.iter().position(|s| !s.is_done());
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; overflow-y: scroll;",
            div { style: "max-width: 1100px; margin: 0 auto; padding: 22px 24px 40px; display: flex; flex-direction: column; gap: 18px;",
                // The stepper.
                // Wraps onto a second line when the panels leave it narrow.
                div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                    for (i, s) in session.steps.iter().enumerate() {
                        {
                            let (done, total) = s.progress();
                            let finished = s.is_done();
                            let now = current == Some(i);
                            let (bg, border, ink) = if finished {
                                ("rgba(34,197,94,0.12)", "rgba(34,197,94,0.45)", INK)
                            } else if now {
                                (PICK_BG, PICK, INK)
                            } else {
                                ("transparent", RULE, INK_3)
                            };
                            rsx! {
                                div { key: "{i}", style: "flex: 1 1 104px; min-width: 104px; min-height: 64px; border-radius: 12px; border: 1px solid {border}; background: {bg}; \
                                                         padding: 10px 12px; display: flex; flex-direction: column; justify-content: center; gap: 4px;",
                                    span { style: "font-size: 14px; font-weight: 700; color: {ink}; display: flex; align-items: center; gap: 6px;",
                                        if finished { Check { size: 14, color: LIVE, stroke_width: 3 } }
                                        "{s.title}"
                                    }
                                    span { style: "{META} font-size: 12px; color: {INK_3};", "{done}/{total}" }
                                }
                            }
                        }
                    }
                }
                for (i, s) in session.steps.iter().enumerate() {
                    {
                        let (done, total) = s.progress();
                        let pct = if total == 0 { 0 } else { done * 100 / total };
                        let finished = s.is_done();
                        let title_ink = if finished { INK_3 } else { INK };
                        let (box_edge, box_bg) = if finished { (LIVE, LIVE) } else { (DIM, "transparent") };
                        rsx! {
                            div { key: "{i}", style: "border-top: 1px solid {RULE}; padding-top: 14px; display: flex; flex-direction: column; gap: 10px;",
                                div { style: "display: flex; align-items: center; gap: 12px;",
                                    button { class: "lp",
                                        "aria-label": if finished { "Reopen {s.title}" } else { "Finish {s.title}" },
                                        style: "width: 26px; height: 26px; border-radius: 7px; border: 1.5px solid {box_edge}; \
                                                background: {box_bg}; display: flex; align-items: center; justify-content: center;",
                                        onclick: move |_| edit(Edit::ToggleStep { session: id, step: i, sub: None }),
                                        if finished { Check { size: 16, color: "#04210f", stroke_width: 3 } }
                                    }
                                    span { style: "{HEADING} color: {title_ink};", "{s.title}" }
                                    div { style: "flex: 0 0 160px; height: 6px; border-radius: 3px; background: {SHEET_2}; border: 1px solid {RULE}; overflow: hidden;",
                                        div { style: "width: {pct}%; height: 100%; background: {LIVE};" }
                                    }
                                    span { style: "{META} color: {INK_3};", "{done} of {total}" }
                                }
                                div { style: "display: flex; flex-wrap: wrap; gap: 8px; padding-left: 38px;",
                                    for (j, c) in s.children.iter().enumerate() {
                                        {
                                            let on = c.is_done();
                                            let (bg, border, ink) = if on { ("rgba(34,197,94,0.10)", "rgba(34,197,94,0.4)", INK_2) } else { ("transparent", RULE_STRONG, INK) };
                                            rsx! {
                                                button { key: "{j}", class: "lp",
                                                    style: "height: 40px; padding: 0 14px; border-radius: 10px; border: 1px solid {border}; background: {bg}; \
                                                            color: {ink}; font-family: {FONT}; font-size: 14px; font-weight: 560; display: flex; align-items: center; gap: 8px;",
                                                    onclick: move |_| edit(Edit::ToggleStep { session: id, step: i, sub: Some(j) }),
                                                    if on { Check { size: 15, color: LIVE, stroke_width: 3 } }
                                                    "{c.title}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Panels ──────────────────────────────────────────────────────────────────

/// Comments, pinned to moments: oldest moment first, each with the version
/// it was heard on; a press on the time seeks there. The box comments at
/// the playhead, on the version playing.
#[component]
fn CommentsPanel() -> Element {
    let lib = use_library();
    let here = use_context::<Here>();
    let mut at = here.at;
    let mut version = here.version;
    let l = lib.read();
    let me = l.me.clone();
    let notes: Vec<(usize, Note)> = l
        .session(here.id)
        .map(|s| s.notes.iter().cloned().enumerate().collect())
        .unwrap_or_default();
    let mut draft = use_signal(String::new);
    let id = here.id;
    let mut send = move || {
        let text = draft.peek().trim().to_owned();
        if !text.is_empty() {
            edit(Edit::MixNote {
                session: id,
                version: *version.peek(),
                at: *at.peek(),
                author: me.clone(),
                text,
            });
            draft.set(String::new());
        }
    };
    let mut send_key = send.clone();
    let empty = draft().trim().is_empty();
    let post_bg = if empty { SHEET } else { PRIMARY };
    let now = clock(at());
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; background: {SHEET}; color: {INK}; font-family: {FONT};",
            div { style: "flex: 1; min-height: 0; overflow-y: scroll; padding: 4px 14px;",
                if notes.is_empty() {
                    p { style: "{META} color: {INK_3}; padding: 14px 0;", "No comments yet. Play the mix and say something at the moment you mean." }
                }
                for (i, n) in notes {
                    {
                        let t = n.at;
                        let v = n.version;
                        let strike = if n.resolved { "opacity: 0.55;" } else { "" };
                        rsx! {
                            div { key: "{i}", style: "display: flex; gap: 10px; align-items: flex-start; padding: 12px 0; border-bottom: 1px solid {RULE}; {strike}",
                                button { class: "lp",
                                    style: "flex-shrink: 0; height: 26px; padding: 0 8px; border-radius: 7px; border: 1px solid {RULE_STRONG}; background: {SHEET_2}; \
                                            color: {PICK}; font-family: {FONT}; font-size: 12px; font-weight: 700;",
                                    onclick: move |_| { version.set(v); at.set(t); },
                                    "{clock(t)}"
                                }
                                div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                    span { style: "{META} color: {INK_2}; font-weight: 650; display: flex; align-items: center; gap: 6px; flex-wrap: wrap;",
                                        "{n.author}"
                                        if n.client { span { style: "font-size: 11px; color: {WARN}; border: 1px solid {WARN}; border-radius: 5px; padding: 0 5px;", "client" } }
                                        span { style: "color: {INK_3}; font-weight: 500;", "v{n.version} · {n.when}" }
                                    }
                                    span { style: "font-size: 14px; line-height: 1.45;", "{n.text}" }
                                }
                                button { class: "lp",
                                    "aria-label": if n.resolved { "Reopen" } else { "Resolve" },
                                    style: "flex-shrink: 0; width: 30px; height: 30px; border-radius: 8px; border: 1px solid {RULE_STRONG}; background: transparent; \
                                            display: flex; align-items: center; justify-content: center;",
                                    onclick: move |_| edit(Edit::ResolveNote { session: id, index: i }),
                                    Check { size: 15, color: if n.resolved { LIVE } else { INK_3 }, stroke_width: 2 }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "flex-shrink: 0; padding: 10px 12px; border-top: 1px solid {RULE};",
                div { style: "display: flex; align-items: center; gap: 8px; height: 44px; padding: 0 6px 0 10px; border-radius: 10px; \
                              border: 1px solid {RULE_STRONG}; background: {SHEET_2};",
                    span { style: "height: 24px; padding: 0 7px; border-radius: 6px; background: {PICK_BG}; color: {PICK}; font-size: 12px; font-weight: 700; \
                                   display: flex; align-items: center;", "{now}" }
                    div { style: "position: relative; flex: 1; min-width: 0; height: 40px; display: flex; align-items: center;",
                        if empty { span { style: "position: absolute; left: 0; top: 11px; {META} color: {INK_3}; pointer-events: none;", "Comment at this moment…" } }
                        input { r#type: "text", value: "{draft}", "aria-label": "Comment at the playhead",
                            style: "position: relative; width: 100%; height: 40px; border: none; outline: none; padding: 0; background: transparent; \
                                    color: {INK}; font-family: {FONT}; font-size: 14px;",
                            oninput: move |e| draft.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter { send_key(); }
                                if !super::command::command_held(&e) { e.stop_propagation(); }
                            },
                        }
                    }
                    button { class: "lp", style: "height: 32px; padding: 0 12px; border-radius: 8px; border: none; background: {post_bg}; \
                                                  color: #fff; font-family: {FONT}; font-size: 13px; font-weight: 650;",
                        disabled: empty, onclick: move |_| send(), "Post" }
                }
            }
        }
    }
}

/// The mix versions, newest first: a press puts one under review.
#[component]
fn VersionsPanel() -> Element {
    let lib = use_library();
    let here = use_context::<Here>();
    let mut version = here.version;
    let mut at = here.at;
    let mixes = lib
        .read()
        .session(here.id)
        .map(|s| s.mixes.clone())
        .unwrap_or_default();
    let picked = version();
    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: scroll; background: {SHEET}; color: {INK}; font-family: {FONT}; padding: 6px;",
            if mixes.is_empty() { p { style: "{META} color: {INK_3}; padding: 10px;", "No mixes yet." } }
            for m in mixes.iter().rev() {
                {
                    let n = m.n;
                    let on = n == picked;
                    let bg = if on { PICK_BG } else { "transparent" };
                    let edge = if on { PICK } else { "transparent" };
                    rsx! {
                        button { key: "{n}", class: "lp",
                            style: "width: 100%; min-height: 56px; display: flex; flex-direction: column; justify-content: center; align-items: flex-start; gap: 3px; padding: 8px 10px; \
                                    border: none; border-radius: 10px; background: {bg}; box-shadow: inset 2px 0 0 {edge}; color: {INK}; font-family: {FONT}; text-align: left;",
                            onclick: move |_| { version.set(n); at.set(0.0); },
                            span { style: "font-size: 14px; font-weight: 700;", "v{n}" }
                            span { style: "{META} font-size: 12px; color: {INK_3}; {ONE_LINE}", "{m.note}" }
                            span { style: "{META} font-size: 11px; color: {INK_3};", "{m.by} · {m.when}" }
                        }
                    }
                }
            }
        }
    }
}

/// What is next: the open subtasks of the step the session is on.
#[component]
fn NextPanel() -> Element {
    let lib = use_library();
    let here = use_context::<Here>();
    let id = here.id;
    let l = lib.read();
    let Some(s) = l.session(id) else {
        return rsx! {};
    };
    let Some(step) = s.steps.iter().position(|t| !t.is_done()) else {
        return rsx! { p { style: "{META} color: {INK_3}; padding: 12px 16px; background: {SHEET};", "Every step is done." } };
    };
    let title = s.steps[step].title.clone();
    let open: Vec<(usize, String)> = s.steps[step]
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_done())
        .map(|(j, c)| (j, c.title.clone()))
        .collect();
    let stage = s.stage;
    rsx! {
        div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 10px 16px; background: {SHEET}; color: {INK}; font-family: {FONT};",
            span { style: "{META} color: {INK_3}; font-weight: 650;", "{title} ({stage.label()}):" }
            for (j, t) in open {
                button { key: "{j}", class: "lp",
                    style: "height: 36px; padding: 0 12px; border-radius: 9px; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; \
                            font-family: {FONT}; font-size: 13px; font-weight: 560; display: flex; align-items: center; gap: 8px;",
                    onclick: move |_| edit(Edit::ToggleStep { session: id, step, sub: Some(j) }),
                    span { style: "width: 14px; height: 14px; border-radius: 4px; border: 1.5px solid {DIM};" }
                    "{t}"
                }
            }
        }
    }
}
