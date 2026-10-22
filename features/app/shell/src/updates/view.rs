//! Settings' update block (feature `ui`): checks once when Settings opens,
//! shows what the feed said, and offers the install when one is possible.
//! Inline styles only, so it runs on Blitz and a `WebView` alike.

use dioxus::prelude::*;

use super::{App, InstallBlock, UpdateStatus};

const BUTTON: &str = "padding: 3px 10px; border-radius: 5px; background: transparent; \
                      color: #a1a1aa; border: 1px solid #27272a; font-size: 11px;";
const TEXT: &str = "color: #a1a1aa;";
const NOTE: &str = "color: #71717a; font-size: 11px;";

/// What the block is doing now.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Checking,
    Installing(String),
    Done(UpdateStatus),
}

/// The update status of `app` with a "Check again" button, and "Install"
/// when a newer release exists and this binary can replace itself.
#[component]
pub fn UpdateSettings(app: App) -> Element {
    let mut phase = use_signal(|| Phase::Checking);
    let block = use_hook(|| app.install_block());
    let mut check = move || {
        phase.set(Phase::Checking);
        spawn(async move { phase.set(Phase::Done(app.check().await)) });
    };
    use_hook(check);
    let install = move |version: String| {
        phase.set(Phase::Installing(version));
        spawn(async move { phase.set(Phase::Done(app.install().await)) });
    };

    let busy = !matches!(phase(), Phase::Done(_));
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: flex-start; gap: 6px;",
            span { style: TEXT, {line(&phase())} }
            div { style: "display: flex; gap: 6px;",
                button {
                    style: BUTTON,
                    disabled: busy,
                    onclick: move |_| check(),
                    "Check again"
                }
                if let Phase::Done(status) = phase() {
                    if let Some(version) = status.available().map(str::to_owned) {
                        InstallAction { version, block, on_install: install }
                    }
                }
            }
        }
    }
}

/// "Install vX" when nothing blocks it; otherwise why not.
#[component]
fn InstallAction(
    version: String,
    block: Option<InstallBlock>,
    on_install: EventHandler<String>,
) -> Element {
    block.map_or_else(
        || {
            rsx! {
                button {
                    style: BUTTON,
                    onclick: move |_| on_install.call(version.clone()),
                    "Install v{version}"
                }
            }
        },
        |block| {
            rsx! {
                span { style: NOTE, "{block}" }
            }
        },
    )
}

fn line(phase: &Phase) -> String {
    match phase {
        Phase::Checking => "Checking for updates…".to_owned(),
        Phase::Installing(version) => format!("Installing v{version}…"),
        Phase::Done(status) => status.to_string(),
    }
}
