//! The DAW view: Session's shell (`session_desktop::native::Embedded`),
//! reused as Studio's own window mounts it, inside an Architect pane.
//!
//! Session's side of the engine is prepared once per process
//! ([`session_desktop::native::prepare_hosted`]: attach to the Studio
//! engine, else spawn one; serve the Action List), on the first entry.
//! Each entry is a fresh shell (its start page opens what was handed to
//! it, behind its loading screen), as each going into the DAW is in
//! Studio's window. Back (Session's own) leaves for the library.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use dioxus::prelude::*;
use session_desktop::native::Prepared;
use session_desktop::native::handoff::{self, Entry};

/// Bumped on every entry: the view mounts a fresh shell for it.
static GENERATION: AtomicU32 = AtomicU32::new(0);

/// Session's side of the one engine, once per process (`None`: none
/// could be had; the view says so, and Architect stays up).
static PREPARED: OnceLock<Option<Prepared>> = OnceLock::new();

/// Hand `set` (a `.setlist`) to Session and have the view mount a fresh
/// shell for it.
pub fn enter(set: PathBuf) {
    handoff::request(Entry::Path(set));
    handoff::entering();
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// The view.
#[allow(non_snake_case)]
pub fn DawView() -> Element {
    let prepared = use_hook(|| {
        PREPARED
            .get_or_init(|| {
                // Text fields hold the keyboard switch: Session's hotkeys
                // stand aside while typing.
                app_shell::ui::keyboard::follow_text_fields();
                let prepared = session_desktop::native::prepare_in_host();
                if prepared.is_none() {
                    tracing::warn!(daw.outcome = "no_engine", "studio-daw: no Studio engine to attach to");
                }
                prepared
            })
            .clone()
    });
    let Some(prepared) = prepared else {
        return rsx! {
            div {
                style: "padding: 24px; opacity: 0.75;",
                "Studio's engine did not start, so there is no DAW to enter. Its log: ~/.cache/fts/studio-engine.log."
            }
        };
    };
    use_context_provider(move || prepared);
    use_context_provider(|| handoff::Host {
        on_leave: Callback::new(|()| {
            spawn(async {
                if let Err(error) = crate::ext::leave().await {
                    tracing::warn!(daw.outcome = "leave_refused", %error, "studio-daw: Back did not close the view");
                }
            });
        }),
    });
    let mut generation = use_signal(|| GENERATION.load(Ordering::Acquire));
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(250)).await;
            let now = GENERATION.load(Ordering::Acquire);
            if *generation.peek() != now {
                generation.set(now);
            }
        }
    });
    rsx! {
        div {
            style: "position: relative; width: 100%; height: 100%; overflow: hidden; display: flex; flex-direction: column;",
            session_desktop::native::Embedded { key: "{generation}" }
        }
    }
}
