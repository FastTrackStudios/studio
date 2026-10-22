//! The FastTrackStudio account in Signal (STA-361): the shared
//! `fts_account` sign-in — the same device-code flow, the same kept session
//! and the same sheet as Session. On a desktop both apps read one file
//! (`~/.config/fts/account/session.json`), so signing in to either signs in
//! both, with no second approval.
//!
//! The TONE3000 OIDC session (`AccountSettings` in main.rs, the engine's
//! `account` service) stays its own thing, labelled TONE3000.

use dioxus::prelude::*;
use fts_account::App;
use fts_account::ui::{AccountBlock as SharedBlock, AccountSheet, AccountState, use_account};

/// Put the account in context for every surface in this window.
pub fn provide_account() -> AccountState {
    let state = use_account(App::Signal);
    use_context_provider(|| state)
}

/// The window's account, from context.
pub fn use_signal_account() -> AccountState {
    use_context::<AccountState>()
}

/// Settings' Account block.
#[component]
pub fn AccountBlock() -> Element {
    let state = use_signal_account();
    rsx! { SharedBlock { state } }
}

/// The sign-in sheet over the window; `top` is where its scrim starts (72
/// on the iPad, below the app bar, so Mute and Panic stay live).
#[component]
pub fn AccountOverlay(top: u32, width: u32) -> Element {
    let state = use_signal_account();
    rsx! {
        AccountSheet {
            state,
            top,
            width,
            on_open: move |url: String| open_link(url),
            on_copy: move |text: String| copy_text(&text),
            rows: None,
        }
    }
}

/// Open the sign-in page: the browser on a desktop, the in-app Safari sheet
/// on an iPad.
fn open_link(url: String) {
    #[cfg(all(target_os = "ios", feature = "tone3000"))]
    crate::ios_safari::open(&url);
    #[cfg(not(target_os = "ios"))]
    {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "explorer"
        } else {
            "xdg-open"
        };
        if let Err(e) = std::process::Command::new(program).arg(&url).spawn() {
            tracing::warn!(error = %e, "account: the sign-in page could not be opened");
        }
    }
    #[cfg(all(target_os = "ios", not(feature = "tone3000")))]
    let _ = url;
}

/// Put `text` on the clipboard.
fn copy_text(text: &str) {
    #[cfg(all(target_os = "ios", feature = "signal-guitar"))]
    crate::ios_orientation::set_clipboard(text);
    #[cfg(all(target_os = "ios", not(feature = "signal-guitar")))]
    let _ = text;
    #[cfg(not(target_os = "ios"))]
    {
        use std::io::Write as _;
        // One call per platform's clipboard tool; the first that runs wins.
        let tools: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
            &[("pbcopy", &[])]
        } else {
            &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
        };
        for (program, args) in tools {
            let Ok(mut child) = std::process::Command::new(program)
                .args(*args)
                .stdin(std::process::Stdio::piped())
                .spawn()
            else {
                continue;
            };
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
        tracing::warn!("account: no clipboard tool to copy with");
    }
}
