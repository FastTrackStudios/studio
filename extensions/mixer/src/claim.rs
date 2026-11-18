//! The window actions the mixer pane carries out, claimed from the engine.
//!
//! In `studio-desktop` one window shows the arrangement and the mixer, and
//! runs `MIXER_VIEW`, the racks switch and the mixer's toggle itself. Here
//! the mixer is a process of its own: it registers those three on its
//! list, claims them as the window [`WINDOW`] (`window_queue::claim_as`),
//! and the engine asks them of it alone; the arrangement's process hands
//! them over (`studio_arrangement`), so its `x` key reaches this pane
//! through the engine too.
//!
//! The toggle is the pane's: shown, it closes (where it was kept);
//! closed, it opens again there — a dock, a float, or under the pane that
//! was above it.

use std::sync::{Mutex, OnceLock, PoisonError};

use architect::action::Effect;
use architect_extensions::hostcall::HostRequest;
use architect_extensions::linked::Handle;
use serde_json::{Value as Json, json};

/// The name this process claims its actions under.
pub const WINDOW: &str = "studio.mixer";

/// The mixer pane, by its id in the workspace (one is open at most).
const PANE: &str = "studio.mixer/mixer";

/// The actions this pane claims: what the arrangement hands over.
const CLAIMED: [&str; 3] = studio_view_link::MIXER_ACTIONS;

/// The extension's way to the host, once it runs.
static HANDLE: OnceLock<Handle> = OnceLock::new();

/// Where the pane was when the toggle closed it: a `WORKSPACE_MOVE` `to`.
static WAS_AT: Mutex<Option<Json>> = Mutex::new(None);

/// Register the claimed actions on this process's list and offer them to
/// the engine under [`WINDOW`]'s name.
pub fn claim(handle: Handle) {
    if HANDLE.set(handle).is_err() {
        return;
    }
    let list = session_engine::actions::list();
    let registered = session_ui::view_actions::register_mixer(list).and_then(|()| {
        list.register_async(
            &session_ui::window::TOGGLE_MIXER,
            Effect::Mutating,
            |(): ()| async { toggle().await },
        )
    });
    if let Err(error) = registered {
        tracing::error!(%error, "studio.mixer: its window actions did not register");
        return;
    }
    // Enabled here as they are: this process is the window that runs them
    // (the engine lists them enabled while a window follows it).
    session_engine::window_queue::claim_as(WINDOW);
    session_engine::window_queue::window_actions(CLAIMED.map(str::to_owned).to_vec());
    session_engine::view_client::offer_window_actions();
}

/// Run the app action `action` with `args`: its answer.
async fn run(action: &str, args: Json) -> Result<Json, String> {
    let handle = HANDLE.get().ok_or("the extension is not running yet")?;
    handle
        .host(HostRequest::Run {
            action: action.to_owned(),
            args,
            ticket: None,
        })
        .await
        .map_err(|e| format!("{action}: {e}"))
}

/// The mixer pane shown or hidden: what the window's mixer key does. The
/// keyboard stays where it was (the arrangement's `x` toggles it again).
async fn toggle() -> Result<String, String> {
    let had_keys = run("WORKSPACE_LAYOUT", Json::Null)
        .await
        .ok()
        .and_then(|layout| layout.get("active")?.as_str().map(str::to_owned))
        .filter(|pane| pane != PANE);
    let toggled = flip().await;
    if let Some(pane) = had_keys
        && let Err(e) = run("WORKSPACE_FOCUS", json!({ "pane": pane })).await
    {
        tracing::debug!(error = %e, "studio.mixer: the keyboard not given back");
    }
    toggled
}

/// [`toggle`]'s pane closed or opened again.
async fn flip() -> Result<String, String> {
    if let Ok(pane) = run("WORKSPACE_PANE_GET", json!({ "pane": PANE })).await {
        let at = place(&pane).await;
        run("WORKSPACE_CLOSE", json!({ "panes": [PANE] })).await?;
        *WAS_AT.lock().unwrap_or_else(PoisonError::into_inner) = at;
        return Ok("the mixer hidden".to_owned());
    }
    run(
        "EXT_VIEW_OPEN",
        json!({ "extension": WINDOW, "view": "mixer" }),
    )
    .await?;
    let at = WAS_AT.lock().unwrap_or_else(PoisonError::into_inner).take();
    // Where it was may be gone (the pane above it closed too): then it
    // stays where it opened.
    if let Some(to) = at
        && let Err(e) = run("WORKSPACE_MOVE", json!({ "pane": PANE, "to": to })).await
    {
        tracing::debug!(error = %e, "studio.mixer: not put back where it was");
    }
    Ok("the mixer shown".to_owned())
}

/// Where the pane `pane` (`WORKSPACE_PANE_GET`'s answer) is, as a
/// `WORKSPACE_MOVE` `to` that puts it back.
async fn place(pane: &Json) -> Option<Json> {
    match pane.get("place").and_then(Json::as_str)? {
        "left_dock" => Some(json!({ "dock": { "side": "left" } })),
        "right_dock" => Some(json!({ "dock": { "side": "right" } })),
        "bottom_dock" => Some(json!({ "dock": { "side": "bottom" } })),
        "float" => Some(json!({ "float": { "rect": pane.get("float_rect").cloned() } })),
        _ => {
            for (toward, side) in [
                ("up", "bottom"),
                ("down", "top"),
                ("left", "right"),
                ("right", "left"),
            ] {
                let next = run(
                    "WORKSPACE_NEIGHBOR",
                    json!({ "pane": PANE, "direction": toward }),
                )
                .await
                .ok();
                if let Some(next) = next
                    .as_ref()
                    .and_then(|n| n.get("neighbor"))
                    .and_then(Json::as_str)
                {
                    return Some(json!({ "split": { "pane": next, "side": side } }));
                }
            }
            None
        }
    }
}
