//! A session: **Review** (its mixes, played with the comments pinned to
//! the moments they are about: the page a client gets) and **Status**
//! (its production steps as a track sheet: the parts down the side, the
//! stages across, each cell the part's work at that stage).
//!
//! One purpose per screen, and nothing that does not earn its place:
//! Review is the player at full width with the version history under it,
//! and the comments as the one panel (on the right when the iPad is
//! sideways, in the bottom drawer upright, and movable: `app_shell::ui`).
//! Status has no panels; the sheet is the page.

use app_shell::ui::{
    Panel, PanelRegistry, Region, ShellFrame, use_panel_registry, use_shell_state,
};
use dioxus::prelude::*;
use lucide_dioxus::{
    Check, ChevronLeft, ChevronRight, FolderOpen, Link, MessageSquare, Pause, Play,
};

use super::home::Page;
use super::kit::*;
use super::player::{Player, use_player};
use super::{Open, edit, open, use_library, use_wide};
use crate::model::{Edit, Listen, MixVersion, Note, Session, SessionId, Todo, clock};

/// Which of the session's two pages is up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Review,
    Status,
}

/// The page a session opens on, when not the one it would pick (a deep
/// link, a screenshot of one page).
#[derive(Clone, Copy)]
pub struct SessionStart(pub Tab);

/// The session the page and its panel are about, and the version under
/// review. It plays through the library's one [`Player`]: what the page
/// shows of it is the player's, while the player has it.
#[derive(Clone, Copy)]
struct Here {
    id: SessionId,
    /// The mix version under review.
    version: Signal<u32>,
    /// What the session is called, for "Playing from".
    title: Signal<String>,
}

impl Here {
    fn listen(self) -> Listen {
        Listen {
            session: self.id,
            version: (self.version)(),
        }
    }

    /// Whether the player has this version.
    fn has(self, player: Player) -> bool {
        player.current() == Some(self.listen())
    }

    /// Where the playhead is on this version: the player's while it has
    /// it, else its start.
    fn now(self, player: Player) -> f32 {
        if self.has(player) { (player.at)() } else { 0.0 }
    }

    fn playing(self, player: Player) -> bool {
        self.has(player) && (player.playing)()
    }

    /// Hear this version from `at` (the player keeps playing if it was).
    fn seek(self, player: Player, at: f32) {
        let listen = Listen {
            session: self.id,
            version: *self.version.peek(),
        };
        let was_empty = player.current().is_none();
        player.cue(listen, at, false);
        if was_empty {
            let mut from = player.from;
            from.set(self.title.peek().clone());
        }
    }

    fn play_pause(self, player: Player) {
        let listen = Listen {
            session: self.id,
            version: *self.version.peek(),
        };
        if player.current() == Some(listen) {
            player.toggle();
        } else {
            let was_empty = player.current().is_none();
            player.cue(listen, 0.0, true);
            if was_empty {
                let mut from = player.from;
                from.set(self.title.peek().clone());
            }
        }
    }
}

#[component]
pub(crate) fn SessionPage(id: SessionId) -> Element {
    let lib = use_library();
    let wide = use_wide();
    // Hooks first: the session may not have arrived yet.
    let player = use_player();
    let latest = lib
        .read()
        .session(id)
        .and_then(|s| s.mixes.last().map(|m| m.n))
        .unwrap_or(0);
    // The version the player has, when it has this session: the page
    // opens on what is being heard.
    let heard = player
        .current()
        .filter(|l| l.session == id)
        .map(|l| l.version);
    let title = lib
        .read()
        .session(id)
        .map(|s| s.title.clone())
        .unwrap_or_default();
    use_context_provider(|| Here {
        id,
        version: Signal::new(heard.unwrap_or(latest)),
        title: Signal::new(title),
    });
    let start = try_consume_context::<SessionStart>().map(|s| s.0);
    let mut tab =
        use_signal(|| start.unwrap_or(if latest > 0 { Tab::Review } else { Tab::Status }));
    let l = lib.read();
    let Some(session) = l.session(id).cloned() else {
        return rsx! { Page { p { style: "{BODY} color: {INK_2}; padding-top: 40px;", "This session was deleted or has not arrived yet." } } };
    };
    let in_project = l.project_of_session(id).map(|p| p.id);
    let open_notes = session.notes.iter().filter(|n| !n.resolved).count();
    let (done, total) = session
        .steps
        .iter()
        .map(Todo::progress)
        .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let pad = if wide { 24 } else { 20 };
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            // One compact header: where it is, what it is, the two pages,
            // the two ways out.
            div { style: "flex-shrink: 0; padding: 12px {pad}px 12px; border-bottom: 1px solid {RULE_STRONG}; \
                          display: flex; flex-direction: column; gap: 4px;",
                super::project::InProject { id: in_project }
                div { style: "display: flex; align-items: center; gap: 12px; flex-wrap: wrap;",
                    span { style: "font-size: 24px; font-weight: 750; letter-spacing: -0.02em;", "{session.title}" }
                    Chip { text: session.stage.label().to_owned() }
                    span { style: "flex: 1;" }
                    if wide {
                        Segmented {
                            left: format!("Review · {open_notes}"),
                            right: format!("Status · {done}/{total}"),
                            on_left: tab() == Tab::Review,
                            on_press: move |left: bool| tab.set(if left { Tab::Review } else { Tab::Status }),
                            fill: false,
                        }
                        Button { label: "Review link".to_owned(), weight: Weight::Secondary,
                            icon: rsx! { Link { size: 16, color: INK } }, on_press: move |()| {} }
                    } else {
                        // Upright the link is its icon (named for touch and
                        // screen readers), so the title keeps its line.
                        button { class: "lp", "aria-label": "Review link", title: "Review link",
                            style: "width: 44px; height: 44px; border-radius: 10px; border: 1px solid {RULE_STRONG}; background: transparent; \
                                    display: flex; align-items: center; justify-content: center;",
                            Link { size: 18, color: INK } }
                    }
                    Button { label: "Open in Session".to_owned(), weight: Weight::Primary,
                        icon: rsx! { FolderOpen { size: 16, color: "#fff" } }, on_press: move |()| open(Open::Session(id)) }
                }
                if !wide {
                    div { style: "margin-top: 8px;",
                        Segmented {
                            left: format!("Review · {open_notes}"),
                            right: format!("Status · {done}/{total}"),
                            on_left: tab() == Tab::Review,
                            on_press: move |left: bool| tab.set(if left { Tab::Review } else { Tab::Status }),
                            fill: true,
                        }
                    }
                }
            }
            match tab() {
                // Keyed by orientation: a rotation remounts the frame with
                // the comments where that orientation keeps them.
                Tab::Review => rsx! { ReviewFrame { key: "{wide}", upright: !wide, session: session.clone() } },
                Tab::Status => rsx! { StatusSheet { session: session.clone() } },
            }
        }
    }
}

/// Two choices side by side; the picked one filled.
#[component]
fn Segmented(
    left: String,
    right: String,
    on_left: bool,
    on_press: EventHandler<bool>,
    fill: bool,
) -> Element {
    let grow = if fill { "flex: 1 1 0;" } else { "" };
    let seg = |on: bool| {
        let (bg, ink, weight) = if on {
            (SHEET_2, INK, 700)
        } else {
            ("transparent", INK_2, 560)
        };
        format!(
            "{grow} height: 36px; padding: 0 14px; border: none; border-radius: 8px; background: {bg}; color: {ink}; \
             font-family: {FONT}; font-size: 14px; font-weight: {weight}; white-space: nowrap;"
        )
    };
    rsx! {
        div { style: "display: flex; gap: 4px; padding: 4px; border-radius: 12px; border: 1px solid {RULE_STRONG}; background: {DESK};",
            button { class: "lp", style: "{seg(on_left)}", onclick: move |_| on_press.call(true), "{left}" }
            button { class: "lp", style: "{seg(!on_left)}", onclick: move |_| on_press.call(false), "{right}" }
        }
    }
}

// ── Review ──────────────────────────────────────────────────────────────────

/// Review in a shell frame: the player and the history in main, the
/// comments as a panel (right when sideways, bottom drawer upright).
#[component]
fn ReviewFrame(upright: bool, session: Session) -> Element {
    use_shell_state(if upright {
        "library.review.upright"
    } else {
        "library.review"
    });
    use_panel_registry(|| {
        let home = if upright {
            Region::Bottom
        } else {
            Region::Right
        };
        PanelRegistry::new().with(Panel::new(
            "review.comments",
            "Comments",
            home,
            CommentsPanel,
        ))
    });
    rsx! {
        ShellFrame { main: rsx! { ReviewMain { session } } }
    }
}

/// The mix under review at full width (the version's facts, the waveform
/// with the comments' marks and the playhead, the transport) and, under
/// it, every version: what changed, who printed it, what was said.
#[component]
fn ReviewMain(session: Session) -> Element {
    let here = use_context::<Here>();
    let player = use_player();
    let mut version = here.version;
    let picked = version();
    let Some(mix) = session.mixes.iter().find(|m| m.n == picked).cloned() else {
        return rsx! {
            div { style: "flex: 1; display: flex; align-items: center; justify-content: center; padding: 40px;",
                div { style: "max-width: 440px; display: flex; flex-direction: column; gap: 8px;",
                    span { style: "{HEADING}", "No mixes yet" }
                    span { style: "{BODY} color: {INK_2}; line-height: 1.5;",
                        "Print a mix in Session and it lands here as v1. Everyone on the project, and anyone with the review link, can play it and comment at the moment they mean."
                    }
                    div { style: "margin-top: 8px;",
                        Button { label: "Open in Session".to_owned(), weight: Weight::Primary,
                            icon: rsx! { FolderOpen { size: 16, color: "#fff" } }, on_press: move |()| open(Open::Session(session.id)) }
                    }
                }
            }
        };
    };
    let now = here.now(player);
    let playing = here.playing(player);
    let length = mix.seconds;
    let first = session.mixes.first().map_or(0, |m| m.n);
    let last = session.mixes.last().map_or(0, |m| m.n);
    let latest = mix.n == last;
    let bins = mix.peaks.len().max(1);
    let played = ((now / length) * bins as f32) as usize;
    // The comments' marks: this version's at full strength, the other
    // versions' open ones faint (still worth hearing for).
    let marks: Vec<(f32, String, bool, bool)> = session
        .notes
        .iter()
        .filter(|n| n.version == mix.n || !n.resolved)
        .map(|n| {
            (
                n.at / length,
                n.author.clone(),
                n.client,
                n.version == mix.n,
            )
        })
        .collect();
    let counts = |n: u32| session.notes.iter().filter(|x| x.version == n).count();
    // Another version: at the same moment, if this session is playing
    // (an A/B), else from its start.
    let mut go = move |n: u32| {
        let had = here.has(player);
        version.set(n);
        if had {
            let at = *player.at.peek();
            player.cue(
                Listen {
                    session: here.id,
                    version: n,
                },
                at,
                false,
            );
        }
    };
    let (prev_ink, next_ink) = (
        if mix.n > first { INK } else { DIM },
        if mix.n < last { INK } else { DIM },
    );
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; overflow-y: scroll;",
            div { style: "max-width: 1100px; margin: 0 auto; padding: 24px; display: flex; flex-direction: column; gap: 32px;",
                // The player: the screen's one surface.
                div { style: "border-radius: 16px; background: {SHEET}; border: 1px solid {RULE}; padding: 20px; \
                              display: flex; flex-direction: column; gap: 20px;",
                    div { style: "display: flex; align-items: center; gap: 12px;",
                        button { class: "lp", "aria-label": "Older mix", disabled: mix.n <= first,
                            style: "width: 40px; height: 40px; border-radius: 10px; border: 1px solid {RULE_STRONG}; background: transparent; \
                                    display: flex; align-items: center; justify-content: center;",
                            onclick: move |_| go(mix.n.saturating_sub(1).max(first)),
                            ChevronLeft { size: 18, color: prev_ink } }
                        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                            div { style: "display: flex; align-items: center; gap: 8px;",
                                span { style: "font-size: 20px; font-weight: 750;", "Mix v{mix.n}" }
                                if latest { Chip { text: "Latest".to_owned(), strong: true } }
                            }
                            span { style: "{META} color: {INK_2}; {ONE_LINE}", "{mix.note} · {mix.by}, {mix.when}" }
                        }
                        button { class: "lp", "aria-label": "Newer mix", disabled: mix.n >= last,
                            style: "width: 40px; height: 40px; border-radius: 10px; border: 1px solid {RULE_STRONG}; background: transparent; \
                                    display: flex; align-items: center; justify-content: center;",
                            onclick: move |_| go((mix.n + 1).min(last)),
                            ChevronRight { size: 18, color: next_ink } }
                    }
                    // The waveform: a press on a bar seeks there.
                    div { style: "position: relative; height: 168px; display: flex; align-items: center; gap: 2px; margin-top: 12px;",
                        for (i, p) in mix.peaks.iter().enumerate() {
                            {
                                let h = (p * 100.0).max(4.0);
                                let c = if i < played { PICK } else { "#3f3f46" };
                                let seek = i as f32 / bins as f32 * length;
                                rsx! {
                                    span { key: "{i}", style: "flex: 1 1 0; min-width: 0; height: 100%; display: flex; align-items: center; cursor: pointer;",
                                        onclick: move |_| here.seek(player, seek),
                                        span { style: "width: 100%; height: {h}%; border-radius: 2px; background: {c};" }
                                    }
                                }
                            }
                        }
                        for (i, (x, who, client, this)) in marks.iter().cloned().enumerate() {
                            {
                                let left = x * 100.0;
                                let ring = if client { WARN } else { PICK };
                                let alpha = if this { "1" } else { "0.4" };
                                rsx! {
                                    span { key: "m{i}", style: "position: absolute; top: -14px; left: {left}%; margin-left: -10px; opacity: {alpha}; \
                                                                border: 2px solid {ring}; border-radius: 12px; display: flex;",
                                        Avatar { name: who, size: 16 }
                                    }
                                }
                            }
                        }
                        span { style: "position: absolute; top: -4px; bottom: -4px; left: {now / length * 100.0}%; width: 2px; background: {INK};" }
                    }
                    div { style: "display: flex; align-items: center; gap: 16px;",
                        button {
                            class: "lp lp-solid",
                            "aria-label": if playing { "Pause" } else { "Play" },
                            style: "width: 56px; height: 56px; border-radius: 28px; border: none; background: {PRIMARY}; \
                                    display: flex; align-items: center; justify-content: center;",
                            onclick: move |_| here.play_pause(player),
                            if playing { Pause { size: 24, color: "#fff", fill: "#fff" } } else { Play { size: 24, color: "#fff", fill: "#fff" } }
                        }
                        span { style: "font-size: 20px; font-weight: 700;", "{clock(now)}" }
                        span { style: "{META} color: {INK_3};", "/ {clock(length)}" }
                        span { style: "flex: 1;" }
                        span { style: "display: flex; align-items: center; gap: 8px; {META} color: {INK_3};",
                            span { style: "width: 12px; height: 12px; border-radius: 6px; border: 2px solid {PICK};" } "Band"
                            span { style: "width: 12px; height: 12px; border-radius: 6px; border: 2px solid {WARN}; margin-left: 8px;" } "Client"
                        }
                    }
                }
                // Every version, newest first: a press plays it.
                div { style: "display: flex; flex-direction: column;",
                    div { style: "display: flex; align-items: baseline; gap: 8px; padding-bottom: 8px;",
                        span { style: "{HEADING}", "Versions" }
                        span { style: "{META} color: {INK_3};", "{session.mixes.len()}" }
                    }
                    for m in session.mixes.iter().rev() {
                        VersionRow { key: "{m.n}", mix: m.clone(), on: m.n == mix.n, comments: counts(m.n), on_press: move |n: u32| go(n) }
                    }
                }
            }
        }
    }
}

#[component]
fn VersionRow(mix: MixVersion, on: bool, comments: usize, on_press: EventHandler<u32>) -> Element {
    let n = mix.n;
    let (bg, edge, weight) = if on {
        (PICK_BG, PICK, 700)
    } else {
        ("transparent", "transparent", 600)
    };
    rsx! {
        button { class: "lp",
            style: "width: 100%; min-height: 56px; display: grid; grid-template-columns: 48px minmax(0, 1fr) auto 64px; column-gap: 16px; \
                    align-items: center; padding: 8px 12px; border: none; border-top: 1px solid {RULE}; border-radius: 0; \
                    background: {bg}; box-shadow: inset 2px 0 0 {edge}; color: {INK}; font-family: {FONT}; text-align: left;",
            onclick: move |_| on_press.call(n),
            span { style: "font-size: 15px; font-weight: {weight};", "v{n}" }
            span { style: "{BODY} font-size: 14px; {ONE_LINE}", "{mix.note}" }
            span { style: "{META} color: {INK_3}; white-space: nowrap;", "{mix.by}, {mix.when}" }
            span { style: "display: flex; align-items: center; justify-content: flex-end; gap: 4px; {META} color: {INK_2};",
                if comments > 0 { MessageSquare { size: 14, color: INK_3 } "{comments}" }
            }
        }
    }
}

/// The comments, pinned to moments: this version's, or every version's.
/// The one nearest the playhead is lit; a press on a time seeks there; the
/// box comments at the playhead on the version playing.
#[component]
fn CommentsPanel() -> Element {
    let keys = app_shell::ui::use_text_focus();
    let lib = use_library();
    let here = use_context::<Here>();
    let player = use_player();
    let mut version = here.version;
    let mut all = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let l = lib.read();
    let me = l.me.clone();
    let current = version();
    let notes: Vec<(usize, Note)> = l
        .session(here.id)
        .map(|s| {
            s.notes
                .iter()
                .cloned()
                .enumerate()
                .filter(|(_, n)| all() || n.version == current)
                .collect()
        })
        .unwrap_or_default();
    let here_count = l.session(here.id).map_or(0, |s| {
        s.notes.iter().filter(|n| n.version == current).count()
    });
    let total = l.session(here.id).map_or(0, |s| s.notes.len());
    let id = here.id;
    let mut send = move || {
        let text = draft.peek().trim().to_owned();
        if !text.is_empty() {
            edit(Edit::MixNote {
                session: id,
                version: *version.peek(),
                at: here.now(player),
                author: me.clone(),
                text,
            });
            draft.set(String::new());
        }
    };
    let mut send_key = send.clone();
    let empty = draft().trim().is_empty();
    let post_bg = if empty { SHEET_2 } else { PRIMARY };
    let post_ink = if empty { INK_3 } else { "#fff" };
    let now = here.now(player);
    let near = notes
        .iter()
        .map(|(i, n)| (*i, (n.at - now).abs()))
        .filter(|(_, d)| *d <= 4.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i);
    let filter = |on: bool| {
        let (bg, ink) = if on {
            (SHEET_2, INK)
        } else {
            ("transparent", INK_3)
        };
        format!(
            "height: 32px; padding: 0 12px; border: none; border-radius: 8px; background: {bg}; color: {ink}; font-family: {FONT}; font-size: 13px; font-weight: 650;"
        )
    };
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; background: {SHEET}; color: {INK}; font-family: {FONT};",
            div { style: "flex-shrink: 0; display: flex; gap: 4px; padding: 8px 12px; border-bottom: 1px solid {RULE};",
                button { class: "lp", style: "{filter(!all())}", onclick: move |_| all.set(false), "v{current} · {here_count}" }
                button { class: "lp", style: "{filter(all())}", onclick: move |_| all.set(true), "All versions · {total}" }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: scroll; padding: 4px 12px;",
                if notes.is_empty() {
                    p { style: "{META} color: {INK_3}; line-height: 1.5; padding: 16px 4px;",
                        if all() { "No comments on any version yet." } else { "Nothing said about v{current} yet. Play it and comment at the moment you mean." }
                    }
                }
                for (i, n) in notes {
                    {
                        let t = n.at;
                        let v = n.version;
                        let lit = near == Some(i);
                        let bg = if lit { PICK_BG } else { "transparent" };
                        let fade = if n.resolved { "opacity: 0.5;" } else { "" };
                        rsx! {
                            div { key: "{i}", style: "display: flex; gap: 12px; align-items: flex-start; padding: 12px 8px; border-radius: 10px; background: {bg}; {fade}",
                                button { class: "lp",
                                    style: "flex-shrink: 0; height: 28px; padding: 0 8px; border-radius: 8px; border: 1px solid {RULE_STRONG}; background: {DESK}; \
                                            color: {PICK}; font-family: {FONT}; font-size: 12px; font-weight: 700;",
                                    onclick: move |_| { version.set(v); here.seek(player, t); },
                                    "{clock(t)}"
                                }
                                div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                                    span { style: "{META} color: {INK_2}; font-weight: 650; display: flex; align-items: center; gap: 6px; flex-wrap: wrap;",
                                        "{n.author}"
                                        if n.client { span { style: "font-size: 11px; color: {WARN}; border: 1px solid {WARN}; border-radius: 6px; padding: 0 6px;", "Client" } }
                                        span { style: "color: {INK_3}; font-weight: 500;", "v{n.version} · {n.when}" }
                                    }
                                    span { style: "font-size: 14px; line-height: 1.45;", "{n.text}" }
                                }
                                button { class: "lp",
                                    "aria-label": if n.resolved { "Reopen" } else { "Resolve" },
                                    style: "flex-shrink: 0; width: 32px; height: 32px; border-radius: 8px; border: 1px solid {RULE_STRONG}; background: transparent; \
                                            display: flex; align-items: center; justify-content: center;",
                                    onclick: move |_| edit(Edit::ResolveNote { session: id, index: i }),
                                    Check { size: 16, color: if n.resolved { LIVE } else { INK_3 }, stroke_width: 2 }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "flex-shrink: 0; padding: 12px; border-top: 1px solid {RULE};",
                div { style: "display: flex; align-items: center; gap: 8px; height: 48px; padding: 0 8px 0 12px; border-radius: 12px; \
                              border: 1px solid {RULE_STRONG}; background: {DESK};",
                    span { style: "height: 24px; padding: 0 8px; border-radius: 6px; background: {PICK_BG}; color: {PICK}; font-size: 12px; font-weight: 700; \
                                   display: flex; align-items: center;", "{clock(now)}" }
                    div { style: "position: relative; flex: 1; min-width: 0; height: 44px; display: flex; align-items: center;",
                        if empty { span { style: "position: absolute; left: 0; top: 13px; {META} color: {INK_3}; pointer-events: none;", "Comment at this moment…" } }
                        input { r#type: "text", value: "{draft}", "aria-label": "Comment at the playhead",
                            onfocus: move |_| { crate::ui::set_field_focused(true); keys.focus(); },
                            onblur: move |_| { crate::ui::set_field_focused(false); keys.blur(); },
                            style: "position: relative; width: 100%; height: 44px; border: none; outline: none; padding: 0; background: transparent; \
                                    color: {INK}; font-family: {FONT}; font-size: 14px;",
                            oninput: move |e| draft.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter { send_key(); }
                                if !super::command::command_held(&e) { e.stop_propagation(); }
                            },
                        }
                    }
                    button { class: "lp", style: "height: 36px; padding: 0 14px; border-radius: 8px; border: none; background: {post_bg}; \
                                                  color: {post_ink}; font-family: {FONT}; font-size: 13px; font-weight: 650;",
                        disabled: empty, onclick: move |_| send(), "Post" }
                }
            }
        }
    }
}

// ── Status ──────────────────────────────────────────────────────────────────

/// One cell of the sheet: the subtasks of one stage that belong to one
/// part (or to the song as a whole), each as (step, subtask, verb, done).
type Cell = Vec<(usize, usize, String, bool)>;

/// The parts the steps are per (Drums, Bass, …): a word that ends
/// subtask titles in two stages or more.
fn parts_of(steps: &[Todo]) -> Vec<String> {
    let mut seen: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        for c in &s.children {
            let Some(last) = c.title.split_whitespace().last() else {
                continue;
            };
            match seen.iter_mut().find(|(w, _)| w == last) {
                Some((_, at)) if !at.contains(&i) => at.push(i),
                Some(_) => {}
                None => seen.push((last.to_owned(), vec![i])),
            }
        }
    }
    seen.into_iter()
        .filter(|(_, at)| at.len() >= 2)
        .map(|(w, _)| w)
        .collect()
}

/// The sheet's rows: the song's own work first, then one per part; each a
/// cell per stage.
fn sheet(steps: &[Todo]) -> (Vec<String>, Vec<Vec<Cell>>) {
    let parts = parts_of(steps);
    let mut rows: Vec<String> = vec!["Song".to_owned()];
    rows.extend(parts.iter().cloned());
    let mut cells: Vec<Vec<Cell>> = vec![vec![Vec::new(); steps.len()]; rows.len()];
    for (i, s) in steps.iter().enumerate() {
        for (j, c) in s.children.iter().enumerate() {
            let last = c.title.split_whitespace().last().unwrap_or("");
            let (row, verb) = match parts.iter().position(|p| p == last) {
                Some(p) => (p + 1, c.title.trim_end_matches(last).trim().to_owned()),
                None => (0, c.title.clone()),
            };
            cells[row][i].push((i, j, verb, c.is_done()));
        }
    }
    (rows, cells)
}

/// Status: what is next, then the track sheet.
#[component]
fn StatusSheet(session: Session) -> Element {
    let id = session.id;
    let current = session.steps.iter().position(|s| !s.is_done());
    let (rows, cells) = sheet(&session.steps);
    let columns = format!("104px repeat({}, minmax(0, 1fr))", session.steps.len());
    let next: Vec<(usize, usize, String)> = current
        .map(|i| {
            session.steps[i]
                .children
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.is_done())
                .map(|(j, c)| (i, j, c.title.clone()))
                .collect()
        })
        .unwrap_or_default();
    let next_title = current.map_or(String::new(), |i| session.steps[i].title.clone());
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; overflow-y: scroll;",
            div { style: "max-width: 1180px; margin: 0 auto; padding: 24px; display: flex; flex-direction: column; gap: 32px;",
                // What is next: the open work of the stage the session is on.
                if !next.is_empty() {
                    div { style: "display: flex; flex-direction: column; gap: 12px;",
                        div { style: "display: flex; align-items: baseline; gap: 8px;",
                            span { style: "{HEADING}", "Next up" }
                            span { style: "{META} color: {INK_3};", "{next_title}" }
                        }
                        div { style: "display: flex; flex-wrap: wrap; gap: 8px;",
                            for (i, j, title) in next {
                                button { key: "{i}-{j}", class: "lp",
                                    style: "height: 44px; padding: 0 16px; border-radius: 10px; border: 1px solid {PICK}; background: {PICK_BG}; \
                                            color: {INK}; font-family: {FONT}; font-size: 14px; font-weight: 600; display: flex; align-items: center; gap: 8px;",
                                    onclick: move |_| edit(Edit::ToggleStep { session: id, step: i, sub: Some(j) }),
                                    span { style: "width: 16px; height: 16px; border-radius: 5px; border: 1.5px solid {PICK};" }
                                    "{title}"
                                }
                            }
                        }
                    }
                } else {
                    span { style: "{HEADING} color: {LIVE};", "Every stage is done." }
                }
                // The track sheet.
                div { style: "display: flex; flex-direction: column;",
                    div { style: "display: grid; grid-template-columns: {columns}; column-gap: 8px; align-items: end; padding-bottom: 8px;",
                        span {}
                        for (i, s) in session.steps.iter().enumerate() {
                            {
                                let (d, t) = s.progress();
                                let finished = s.is_done();
                                let now = current == Some(i);
                                let (bar, ink) = if finished { (LIVE, INK_2) } else if now { (PICK, INK) } else { (RULE_STRONG, INK_3) };
                                rsx! {
                                    div { key: "{i}", style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
                                        span { style: "height: 4px; border-radius: 2px; background: {bar};" }
                                        span { style: "font-size: 13px; font-weight: 700; color: {ink}; {ONE_LINE}", "{s.title}" }
                                        span { style: "font-size: 12px; color: {INK_3};", "{d}/{t}" }
                                    }
                                }
                            }
                        }
                    }
                    for (r, name) in rows.iter().enumerate() {
                        div { key: "{name}", style: "display: grid; grid-template-columns: {columns}; column-gap: 8px; align-items: stretch; \
                                                     min-height: 56px; border-top: 1px solid {RULE};",
                            span { style: "display: flex; align-items: center; {BODY} font-size: 14px;", "{name}" }
                            for (i, cell) in cells[r].iter().cloned().enumerate() {
                                div { key: "{i}", style: "display: flex; flex-direction: column; justify-content: center; gap: 4px; padding: 8px 0; min-width: 0;",
                                    if cell.is_empty() {
                                        span { style: "height: 2px; width: 12px; border-radius: 1px; background: {RULE_STRONG};" }
                                    }
                                    for (step, sub, verb, done) in cell {
                                        {
                                            let (bg, border, ink) = if done { ("rgba(34,197,94,0.10)", "rgba(34,197,94,0.35)", INK_2) } else { ("transparent", RULE_STRONG, INK) };
                                            rsx! {
                                                button { key: "{sub}", class: "lp",
                                                    "aria-label": "{verb} {name}",
                                                    style: "width: 100%; min-height: 32px; padding: 6px 8px; border-radius: 8px; border: 1px solid {border}; background: {bg}; \
                                                            color: {ink}; font-family: {FONT}; font-size: 12px; font-weight: 600; line-height: 1.25; \
                                                            display: flex; align-items: center; justify-content: flex-start; gap: 6px; text-align: left; overflow: hidden; overflow-wrap: break-word;",
                                                    onclick: move |_| edit(Edit::ToggleStep { session: id, step, sub: Some(sub) }),
                                                    if done { Check { size: 13, color: LIVE, stroke_width: 3 } }
                                                    span { style: "min-width: 0;", "{verb}" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_puts_each_subtask_under_its_part_and_stage() {
        let steps = crate::model::production_steps(&["Drums", "Bass", "Vocals"]);
        let (rows, cells) = sheet(&steps);
        assert_eq!(rows, ["Song", "Drums", "Bass", "Vocals"]);
        let edit = steps.iter().position(|s| s.title == "Edit").unwrap();
        let vocals_edit: Vec<&str> = cells[3][edit].iter().map(|c| c.2.as_str()).collect();
        assert_eq!(
            vocals_edit,
            ["Edit", "Comp"],
            "Comp Vocals sits beside Edit Vocals"
        );
        let write = steps.iter().position(|s| s.title == "Write").unwrap();
        assert_eq!(
            cells[0][write].len(),
            2,
            "Lyrics and Chart are the song's own"
        );
    }
}
