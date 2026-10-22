//! The one sign-in UI (design spec STA-379 §3–4), mounted by Signal and
//! Session on every platform: [`use_account`] (the state, the shared-file
//! watch and the flow), [`DeviceCodePanel`] (the code and its steps; inline
//! in Session's start card or inside the sheet), [`AccountSheet`] (every
//! state of §3.2 plus the signed-in view of §4.4), [`SignOutConfirm`],
//! [`AccountMenuItem`] (the iPad ⋯ menu's first item) and [`AccountBlock`]
//! (Signal desktop Settings).
//!
//! Blitz rules: inline styles, hex colours, no `position: fixed`, no
//! `overflow: auto`, no `text-overflow` (cut by count). Opening a link and
//! copying text are the app's (`on_open`, `on_copy`): a desktop opens the
//! browser, an iPad the in-app Safari sheet.

use std::time::{Duration, SystemTime};

use dioxus::prelude::*;

use crate::copy::{self, Platform};
use crate::flow::{self, Clock as _, Http, SystemClock};
use crate::{Account, App, Failure, Stamp, Started, Store, cut};

pub const SHEET: &str = "#16171b";
pub const SHEET_2: &str = "#202128";
pub const RULE: &str = "#2b2b31";
pub const INK: &str = "#f4f4f5";
/// Dim text: 7.2:1 on the sheet (never `#6b7280` / `#71717a` / `#52525b`).
pub const DIM: &str = "#9aa0a6";
pub const BLUE: &str = "#2563eb";
pub const AMBER: &str = "#f59e0b";
pub const RED: &str = "#f87171";
pub const GREEN: &str = "#22c55e";
const FONT: &str = "-apple-system, 'SF Pro Text', 'Inter', system-ui, sans-serif";
const MONO: &str = "'SF Mono', 'JetBrains Mono', ui-monospace, monospace";
/// Avatar fills: five dark tints, by [`Account::hue`].
const HUES: [&str; 5] = ["#1e3a5f", "#3b2a5c", "#1f4d3a", "#5a3a1f", "#4a1f33"];

/// Where the account is: what every surface renders.
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    /// No kept session.
    Out,
    /// Asking the account server for a code.
    Starting,
    /// The code is shown; polling (`offline`: the last poll could not be made).
    Code { started: Started, offline: bool },
    /// Approved; the session is being kept.
    Finishing,
    /// Signed in. `just_now`: the sheet shows "You're signed in";
    /// `elsewhere`: by the other app; `saved`: false shows the save warning.
    In {
        account: Account,
        just_now: bool,
        elsewhere: bool,
        saved: bool,
    },
    /// Had a session; the server refused it.
    Ended,
    /// A sign-in could not go on.
    Failed { failure: Failure, minutes: u64 },
}

/// The account, its sign-in and the shared file's watch, for one window.
#[derive(Clone, Copy, PartialEq)]
pub struct AccountState {
    pub app: App,
    pub platform: Platform,
    pub phase: Signal<Phase>,
    /// Whether the sheet is open.
    pub sheet: Signal<bool>,
    task: Signal<Option<Task>>,
    stamp: Signal<Stamp>,
}

/// The account for `app`: read from the shared file now, re-read whenever
/// the other app changes it (a stat every 5 s, a parse only on change).
pub fn use_account(app: App) -> AccountState {
    let phase = use_signal(|| crate::signed_in().map_or(Phase::Out, |a| signed(a, false)));
    let sheet = use_signal(|| false);
    let task = use_signal(|| None::<Task>);
    let stamp = use_signal(|| Store::shared().and_then(|s| s.stamp()));
    let state = AccountState {
        app,
        platform: Platform::current(),
        phase,
        sheet,
        task,
        stamp,
    };
    use_hook(move || {
        spawn(async move {
            loop {
                SystemClock.sleep(Duration::from_secs(5)).await;
                state.refresh();
            }
        })
    });
    state
}

const fn signed(account: Account, just_now: bool) -> Phase {
    Phase::In {
        account,
        just_now,
        elsewhere: false,
        saved: true,
    }
}

impl AccountState {
    /// The signed-in person, if any.
    #[must_use]
    pub fn account(&self) -> Option<Account> {
        match &*self.phase.read() {
            Phase::In { account, .. } => Some(account.clone()),
            _ => None,
        }
    }

    /// Re-read the shared file if the other app changed it.
    pub fn refresh(mut self) {
        let Some(store) = Store::shared() else { return };
        let now = store.stamp();
        if now == *self.stamp.peek() {
            return;
        }
        self.stamp.set(now);
        let kept = store.load(&crate::server()).ok().flatten();
        let phase = self.phase.peek().clone();
        match (kept, phase) {
            (Some(account), Phase::In { account: had, .. }) if had == account => {}
            (Some(account), Phase::Code { .. } | Phase::Starting) => {
                // Signed in by the other app while this one waited: stop
                // polling and say so.
                self.stop();
                self.phase.set(Phase::In {
                    account,
                    just_now: true,
                    elsewhere: true,
                    saved: true,
                });
            }
            (Some(account), Phase::Finishing) => {
                self.phase.set(signed(account, true));
            }
            (Some(account), _) => self.phase.set(signed(account, false)),
            (None, Phase::In { .. }) => {
                // Signed out elsewhere: no popup, the surface just flips.
                self.phase.set(Phase::Out);
            }
            (None, _) => {}
        }
    }

    /// Start (or restart) a sign-in.
    pub fn begin(mut self) {
        self.stop();
        self.phase.set(Phase::Starting);
        let app = self.app;
        let task = spawn(async move {
            let http = Http::new();
            let server = crate::server();
            let started = match flow::start(&http, &SystemClock, &server, app.client_id()).await {
                Ok(s) => s,
                Err(failure) => {
                    self.phase.set(Phase::Failed { failure, minutes: 10 });
                    return;
                }
            };
            let minutes = started.minutes();
            self.phase.set(Phase::Code {
                started: started.clone(),
                offline: false,
            });
            let mut phase = self.phase;
            let got = flow::finish(&http, &SystemClock, &started, app.client_id(), |offline| {
                if let Phase::Code { offline: o, .. } = &mut *phase.write() {
                    *o = offline;
                }
            })
            .await;
            match got {
                Ok(session) => {
                    self.phase.set(Phase::Finishing);
                    let saved = flow::keep(&session);
                    self.stamp.set(Store::shared().and_then(|s| s.stamp()));
                    self.phase.set(Phase::In {
                        account: Account { server, session },
                        just_now: true,
                        elsewhere: false,
                        saved,
                    });
                }
                Err(failure) => self.phase.set(Phase::Failed { failure, minutes }),
            }
        });
        self.task.set(Some(task));
    }

    fn stop(mut self) {
        if let Some(t) = self.task.write().take() {
            t.cancel();
        }
    }

    /// Cancel / Close / Not now / Done: stop any sign-in and close the sheet.
    pub fn close(mut self) {
        self.stop();
        let phase = self.phase.peek().clone();
        self.phase.set(match phase {
            Phase::In { account, saved, .. } => Phase::In {
                account,
                just_now: false,
                elsewhere: false,
                saved,
            },
            Phase::Ended => Phase::Ended,
            _ => Phase::Out,
        });
        self.sheet.set(false);
    }

    /// Sign out (both apps on a desktop): forget the kept session.
    pub fn sign_out(mut self) {
        self.stop();
        crate::sign_out();
        self.stamp.set(None);
        self.phase.set(Phase::Out);
        self.sheet.set(false);
    }

    /// The server refused the kept session (a call came back unauthorised).
    pub fn ended(mut self) {
        crate::sign_out();
        self.stamp.set(None);
        self.phase.set(Phase::Ended);
    }

    /// Open the sheet on what the account is now.
    pub fn open(mut self) {
        self.sheet.set(true);
    }
}

// ── pieces ──────────────────────────────────────────────────────────────

fn primary_btn() -> String {
    format!(
        "height: 56px; min-width: 120px; padding: 0 24px; border: none; border-radius: 12px; \
         background: {BLUE}; color: #ffffff; font-size: 17px; font-weight: 700; cursor: pointer; font-family: {FONT};"
    )
}

fn quiet_btn() -> String {
    format!(
        "height: 48px; min-width: 48px; padding: 0 16px; border: 1px solid {RULE}; border-radius: 10px; \
         background: transparent; color: {INK}; font-size: 15px; font-weight: 650; cursor: pointer; font-family: {FONT};"
    )
}

fn text_btn() -> String {
    format!(
        "height: 48px; min-width: 48px; padding: 0 16px; border: none; border-radius: 10px; \
         background: transparent; color: {INK}; font-size: 15px; font-weight: 650; cursor: pointer; font-family: {FONT};"
    )
}

fn danger_btn() -> String {
    format!(
        "height: 48px; min-width: 48px; padding: 0 16px; border: 1px solid {RED}; border-radius: 10px; \
         background: transparent; color: {RED}; font-size: 15px; font-weight: 700; cursor: pointer; font-family: {FONT};"
    )
}

/// A round avatar with the person's initials.
#[component]
pub fn Avatar(account: Account, size: u32) -> Element {
    let fill = HUES.get(account.hue()).copied().unwrap_or(SHEET_2);
    let font = size.saturating_mul(2) / 5;
    rsx! {
        span {
            "aria-hidden": "true",
            style: "display: flex; align-items: center; justify-content: center; flex-shrink: 0; \
                    width: {size}px; height: {size}px; border-radius: 999px; background: {fill}; \
                    color: {INK}; font-size: {font}px; font-weight: 750; font-family: {FONT};",
            "{account.initials()}"
        }
    }
}

/// The code and how to approve it (§3.1), filling its parent's width.
/// `narrow`: boxes 36 x 48 (under 440 wide).
#[component]
pub fn DeviceCodePanel(
    started: Started,
    offline: bool,
    narrow: bool,
    on_open: EventHandler<String>,
    on_copy: EventHandler<String>,
) -> Element {
    // The countdown: from the wall clock, every second (shown in 30 s
    // steps above a minute, so it reads still).
    let mut now = use_signal(SystemTime::now);
    use_hook(move || {
        spawn(async move {
            loop {
                SystemClock.sleep(Duration::from_secs(1)).await;
                now.set(SystemTime::now());
            }
        })
    });
    let left = started.left(now()).as_secs();
    let shown = if left < 60 { left } else { left.div_ceil(30).saturating_mul(30) };
    let expiry = copy::expires_in(shown);
    let urgent = left < 60;
    let (w, h, f) = if narrow { (36, 48, 24) } else { (44, 56, 28) };
    let link = started.link();
    let link_copy = link.clone();
    let code = started.code.user_code.clone();
    let spoken: String = code
        .chars()
        .filter(|c| *c != '-')
        .map(String::from)
        .collect::<Vec<_>>()
        .join(" ");
    let groups = started.groups();
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px; width: 100%; font-family: {FONT}; color: {INK};",
            div { style: "display: flex; gap: 12px;",
                span { style: "width: 24px; flex-shrink: 0; font-size: 15px; font-weight: 750; color: {DIM};", "1" }
                div { style: "display: flex; flex-direction: column; gap: 10px; align-items: flex-start;",
                    span { style: "font-size: 15px; line-height: 1.4;", "{copy::STEP_OPEN}" }
                    button { style: quiet_btn(), onclick: move |_| on_open.call(link.clone()), "{copy::OPEN_PAGE}" }
                }
            }
            div { style: "display: flex; gap: 12px;",
                span { style: "width: 24px; flex-shrink: 0; font-size: 15px; font-weight: 750; color: {DIM};", "2" }
                div { style: "display: flex; flex-direction: column; gap: 14px; align-items: flex-start;",
                    span { style: "font-size: 15px; line-height: 1.4;", "{copy::STEP_CHECK}" }
                    div {
                        role: "text",
                        "aria-label": "Code {spoken}",
                        style: "display: flex; flex-wrap: wrap; gap: 20px;",
                        for (i, group) in groups.iter().enumerate() {
                            div { key: "{i}", style: "display: flex; gap: 8px;",
                                for (j, ch) in group.chars().enumerate() {
                                    span {
                                        key: "{j}",
                                        style: "display: flex; align-items: center; justify-content: center; \
                                                width: {w}px; height: {h}px; border-radius: 8px; background: {SHEET_2}; \
                                                border: 1px solid {RULE}; font-family: {MONO}; font-size: {f}px; font-weight: 700;",
                                        "{ch}"
                                    }
                                }
                            }
                        }
                    }
                    div { style: "display: flex; gap: 8px; flex-wrap: wrap;",
                        button { style: quiet_btn(), onclick: move |_| on_copy.call(code.clone()), "{copy::COPY_CODE}" }
                        button { style: quiet_btn(), onclick: move |_| on_copy.call(link_copy.clone()), "{copy::COPY_LINK}" }
                    }
                }
            }
            span { style: "font-size: 14px; line-height: 1.4; color: {DIM};", "{copy::other_device(&started.other_device())}" }
            if offline {
                span { role: "status", style: "font-size: 14px; line-height: 1.4; color: {AMBER};", "{copy::OFFLINE_WAITING}" }
            } else {
                div { role: "status", style: "display: flex; align-items: center; gap: 8px; font-size: 14px;",
                    span { style: "width: 8px; height: 8px; border-radius: 999px; background: {AMBER}; flex-shrink: 0;" }
                    span { style: "color: {DIM};", "{copy::WAITING}" }
                    span { style: "flex: 1;" }
                    span { style: if urgent { format!("color: {AMBER};") } else { format!("color: {DIM};") }, "{expiry}" }
                }
            }
            span { style: "font-size: 14px; line-height: 1.4; color: {DIM};", "{copy::ONLY_YOURS}" }
        }
    }
}

/// The sign-out confirm (§4.5): a popover, Cancel first for the keyboard.
#[component]
pub fn SignOutConfirm(app: App, platform: Platform, on_confirm: EventHandler<()>, on_cancel: EventHandler<()>) -> Element {
    rsx! {
        div {
            role: "alertdialog",
            "aria-label": "{copy::SIGN_OUT_TITLE}",
            style: "display: flex; flex-direction: column; gap: 10px; padding: 16px; border-radius: 14px; \
                    background: {SHEET_2}; border: 1px solid {RULE}; max-width: 360px; font-family: {FONT};",
            span { style: "font-size: 17px; font-weight: 750; color: {INK};", "{copy::SIGN_OUT_TITLE}" }
            span { style: "font-size: 14px; line-height: 1.4; color: {DIM};", "{copy::sign_out_body(app, platform)}" }
            div { style: "display: flex; gap: 8px;",
                button { autofocus: true, style: text_btn(), onclick: move |_| on_cancel.call(()), "Cancel" }
                span { style: "flex: 1;" }
                button { style: danger_btn(), onclick: move |_| on_confirm.call(()), "Sign out" }
            }
        }
    }
}

/// The sign-in sheet over its window (§3): every state of §3.2, and the
/// signed-in view (§4.4). Mount it last inside a `position: relative`
/// full-window box; `top` is where the scrim starts (72 on touch layouts so
/// Mute and Panic stay live, 0 on a desktop). `rows` are the app's own rows
/// in the signed-in view (Session's organization, Signal's TONE3000).
#[component]
pub fn AccountSheet(
    state: AccountState,
    top: u32,
    width: u32,
    on_open: EventHandler<String>,
    on_copy: EventHandler<String>,
    rows: Option<Element>,
) -> Element {
    let mut confirm = use_signal(|| false);
    if !(state.sheet)() {
        return rsx! {};
    }
    let app = state.app;
    let platform = state.platform;
    let w = width.saturating_sub(32).min(480);
    let narrow = w < 440;
    let phase = (state.phase)();
    let server = crate::server();
    rsx! {
        div {
            style: "position: absolute; left: 0; right: 0; top: {top}px; bottom: 0; z-index: 90; \
                    background: #00000099; display: flex; justify-content: center; align-items: flex-start; \
                    padding-top: 16px; font-family: {FONT};",
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape { state.close(); }
            },
            div {
                role: "dialog",
                "aria-modal": "true",
                style: "display: flex; flex-direction: column; gap: 18px; width: {w}px; padding: 24px; \
                        box-sizing: border-box; border-radius: 20px; background: {SHEET}; border: 1px solid {RULE}; color: {INK};",
                match phase {
                    Phase::Code { started, offline } => rsx! {
                        Header { title: copy::CODE_TITLE, on_close: move |()| state.close() }
                        DeviceCodePanel { started, offline, narrow, on_open, on_copy }
                        div { style: "display: flex; justify-content: flex-end;",
                            button { style: text_btn(), onclick: move |_| state.close(), "Cancel" }
                        }
                    },
                    Phase::In { account, just_now: true, elsewhere, saved } => {
                        let words = copy::signed_in_now(app, platform, account.email());
                        rsx! {
                            Header { title: words.title, on_close: move |()| state.close() }
                            div { role: "status", style: "display: flex; gap: 10px; align-items: flex-start;",
                                span { style: "color: {GREEN}; font-size: 20px; font-weight: 800;", "✓" }
                                span { style: "font-size: 15px; line-height: 1.4;",
                                    if elsewhere { "{copy::signed_in_elsewhere(app)} " }
                                    "{words.body}"
                                }
                            }
                            if !saved {
                                span { style: "font-size: 14px; line-height: 1.4; color: {AMBER};", "{copy::SAVE_WARNING}" }
                            }
                            div { style: "display: flex; justify-content: flex-end;",
                                button { style: primary_btn(), onclick: move |_| state.close(), "Done" }
                            }
                        }
                    }
                    Phase::In { account, .. } => {
                        let (caption, shared) = copy::signed_in_here(platform);
                        let manage = format!("{}/account", account_host(&account));
                        rsx! {
                            div { style: "display: flex; gap: 14px; align-items: center;",
                                Avatar { account: account.clone(), size: 56 }
                                div { style: "display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0;",
                                    span { style: "font-size: 22px; font-weight: 750;", "{cut(&account.name(), 20)}" }
                                    span { style: "font-size: 14px; color: {DIM};", "{cut(account.email(), 28)}" }
                                    if !account.on_default_server() {
                                        span { style: "font-size: 14px; color: {DIM};", "{cut(&account.server, 40)}" }
                                    }
                                }
                                CloseX { on_close: move |()| state.close() }
                            }
                            div { style: "display: flex; flex-direction: column; gap: 4px; padding: 12px 0; border-top: 1px solid {RULE}; border-bottom: 1px solid {RULE};",
                                span { style: "font-size: 15px; font-weight: 650;", "{caption}" }
                                if let Some(shared) = shared {
                                    span { style: "font-size: 14px; color: {DIM};", "{shared}" }
                                }
                            }
                            {rows}
                            button {
                                style: "display: flex; align-items: center; height: 56px; padding: 0; border: none; background: transparent; \
                                        color: {INK}; font-size: 15px; font-weight: 650; cursor: pointer; text-align: left; font-family: {FONT};",
                                onclick: move |_| on_open.call(manage.clone()),
                                "Manage account on the web"
                                span { style: "flex: 1;" }
                                span { style: "color: {DIM};", "↗" }
                            }
                            if confirm() {
                                SignOutConfirm {
                                    app, platform,
                                    on_confirm: move |()| { confirm.set(false); state.sign_out(); },
                                    on_cancel: move |()| confirm.set(false),
                                }
                            } else {
                                div {
                                    button { style: danger_btn(), onclick: move |_| confirm.set(true), "Sign out" }
                                }
                            }
                        }
                    }
                    other => {
                        let words = match &other {
                            Phase::Out => copy::out(app),
                            Phase::Starting => copy::starting(),
                            Phase::Finishing => copy::finishing(),
                            Phase::Ended => copy::ended(),
                            Phase::Failed { failure, minutes } => copy::failed(app, failure, &server, *minutes),
                            Phase::Code { .. } | Phase::In { .. } => copy::starting(),
                        };
                        let detail = match &other {
                            Phase::Failed { failure, .. } => failure.brief(),
                            _ => None,
                        };
                        let busy = matches!(other, Phase::Starting | Phase::Finishing);
                        rsx! {
                            Header { title: words.title, on_close: move |()| state.close() }
                            div { role: "status", style: "display: flex; gap: 10px; align-items: center;",
                                if busy {
                                    span { "aria-hidden": "true", style: "width: 20px; height: 20px; border-radius: 999px; border: 2px solid {RULE}; border-top-color: {INK}; flex-shrink: 0;" }
                                }
                                span { style: "font-size: 15px; line-height: 1.45;", "{words.body}" }
                            }
                            if let Some(detail) = detail {
                                span { style: "font-size: 14px; color: {DIM}; line-height: 1.4;", "{detail}" }
                            }
                            div { style: "display: flex; gap: 8px; justify-content: flex-end; align-items: center;",
                                if let Some(label) = words.secondary {
                                    button { style: text_btn(), onclick: move |_| state.close(), "{label}" }
                                }
                                if let Some(label) = words.primary {
                                    button { style: primary_btn(), onclick: move |_| state.begin(), "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The account server's base for "Manage account on the web": the issuer
/// is not kept with the session, so the default one.
fn account_host(_account: &Account) -> &'static str {
    "https://auth.fasttrackstudio.app"
}

#[component]
fn Header(title: &'static str, on_close: EventHandler<()>) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 12px;",
            span { style: "flex: 1; font-size: 22px; font-weight: 750; color: {INK};", "{title}" }
            CloseX { on_close }
        }
    }
}

#[component]
fn CloseX(on_close: EventHandler<()>) -> Element {
    rsx! {
        button {
            "aria-label": "Close",
            style: "width: 48px; height: 48px; flex-shrink: 0; border: none; border-radius: 10px; background: transparent; \
                    color: {DIM}; font-size: 22px; cursor: pointer;",
            onclick: move |_| on_close.call(()),
            "✕"
        }
    }
}

/// The iPad app bar ⋯ menu's first item (§4.2): signed out, signed in, or
/// ended. `locked` (touch-lock on): dimmed and inert.
#[component]
pub fn AccountMenuItem(state: AccountState, locked: bool, on_pick: EventHandler<()>) -> Element {
    let phase = (state.phase)();
    let (lead, first, second, warn): (Element, String, String, bool) = match &phase {
        Phase::In { account, .. } => (
            rsx! { Avatar { account: account.clone(), size: 32 } },
            cut(&account.name(), 18),
            cut(account.email(), 22),
            false,
        ),
        Phase::Ended => (
            rsx! { span { style: "width: 10px; height: 10px; margin: 0 11px; border-radius: 999px; background: {AMBER};" } },
            "Sign in again".to_owned(),
            "Your sign-in ended".to_owned(),
            true,
        ),
        _ => (
            rsx! { span { style: "width: 10px; height: 10px; margin: 0 11px; border-radius: 999px; border: 2px solid {DIM}; box-sizing: border-box;" } },
            "Sign in".to_owned(),
            "Connect your devices".to_owned(),
            false,
        ),
    };
    let opacity = if locked { "0.45" } else { "1" };
    rsx! {
        button {
            role: "menuitem",
            "aria-label": "Account: {first}",
            "aria-disabled": "{locked}",
            style: "display: flex; align-items: center; gap: 10px; min-height: 52px; padding: 6px 14px; border: none; \
                    border-radius: 8px; background: transparent; text-align: left; cursor: pointer; opacity: {opacity}; font-family: {FONT};",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                if !locked {
                    on_pick.call(());
                    state.open();
                }
            },
            {lead}
            div { style: "display: flex; flex-direction: column; gap: 1px; min-width: 0;",
                span { style: "font-size: 16px; font-weight: 650; color: {INK};", "{first}" }
                span { style: if warn { format!("font-size: 14px; color: {AMBER};") } else { format!("font-size: 14px; color: {DIM};") }, "{second}" }
            }
        }
    }
}

/// Signal desktop Settings' Account block (§4.1): signed out → Sign in;
/// signed in → avatar, name, email, Account… and Sign out (with confirm).
#[component]
pub fn AccountBlock(state: AccountState) -> Element {
    let mut confirm = use_signal(|| false);
    let phase = (state.phase)();
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 10px; width: 100%; padding: 14px; box-sizing: border-box; \
                      border-radius: 12px; background: {SHEET}; border: 1px solid {RULE}; font-family: {FONT};",
            span { style: "font-size: 12px; font-weight: 750; letter-spacing: 0.06em; color: {DIM};", "ACCOUNT" }
            match phase {
                Phase::In { account, .. } => rsx! {
                    div { style: "display: flex; align-items: center; gap: 12px; min-height: 56px;",
                        Avatar { account: account.clone(), size: 40 }
                        div { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",
                            span { style: "font-size: 16px; font-weight: 700; color: {INK};", "{cut(&account.name(), 20)}" }
                            span { style: "font-size: 14px; color: {DIM};", "{cut(account.email(), 28)}" }
                        }
                    }
                    if confirm() {
                        SignOutConfirm {
                            app: state.app, platform: state.platform,
                            on_confirm: move |()| { confirm.set(false); state.sign_out(); },
                            on_cancel: move |()| confirm.set(false),
                        }
                    } else {
                        div { style: "display: flex; gap: 8px;",
                            button { style: quiet_btn(), onclick: move |_| state.open(), "Account…" }
                            span { style: "flex: 1;" }
                            button { style: quiet_btn(), onclick: move |_| confirm.set(true), "Sign out" }
                        }
                    }
                },
                Phase::Ended => rsx! {
                    div { style: "display: flex; align-items: center; gap: 12px; min-height: 56px;",
                        span { style: "width: 10px; height: 10px; border-radius: 999px; background: {AMBER}; flex-shrink: 0;" }
                        span { style: "flex: 1; font-size: 15px; color: {AMBER};", "Your sign-in ended" }
                        button { style: primary_btn(), onclick: move |_| { state.open(); state.begin(); }, "Sign in again" }
                    }
                },
                _ => rsx! {
                    div { style: "display: flex; align-items: center; gap: 12px; min-height: 56px;",
                        span { style: "width: 10px; height: 10px; border-radius: 999px; border: 2px solid {DIM}; box-sizing: border-box; flex-shrink: 0;" }
                        div { style: "display: flex; flex-direction: column; gap: 2px; flex: 1;",
                            span { style: "font-size: 15px; color: {INK};", "Not signed in" }
                            span { style: "font-size: 14px; color: {DIM};", "{state.app.name()} works without one." }
                        }
                        button { style: primary_btn(), onclick: move |_| state.open(), "Sign in" }
                    }
                },
            }
        }
    }
}
