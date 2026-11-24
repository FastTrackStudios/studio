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
const CLAIMED: [&str; 4] = studio_view_link::MIXER_ACTIONS;

/// The extension's way to the host, once it runs.
static HANDLE: OnceLock<Handle> = OnceLock::new();

/// Where the pane was when the toggle closed it: `WORKSPACE_MOVE` `to`s
/// to try in order ([`place`]).
static WAS_AT: Mutex<Vec<Json>> = Mutex::new(Vec::new());

/// Register the claimed actions on this process's list and offer them to
/// the engine under [`WINDOW`]'s name.
pub fn claim(handle: Handle) {
    if HANDLE.set(handle).is_err() {
        return;
    }
    let list = session_engine_client::actions::list();
    let registered = mixer_view::mixer_actions::register_mixer(list).and_then(|()| {
        list.register_async(
            &daw_view_kit::window::TOGGLE_MIXER,
            Effect::Mutating,
            |(): ()| async { toggle().await },
        )
        .and_then(|()| {
            list.register_async(
                &mixer_view::mixer_actions::CLOSEUP,
                Effect::Mutating,
                |a: mixer_view::mixer_actions::CloseupArg| async move { closeup(a).await },
            )
        })
    });
    if let Err(error) = registered {
        tracing::error!(%error, "studio.mixer: its window actions did not register");
        return;
    }
    // Enabled here as they are: this process is the window that runs them
    // (the engine lists them enabled while a window follows it).
    session_engine_client::window_queue::claim_as(WINDOW);
    session_engine_client::window_queue::window_actions(CLAIMED.map(str::to_owned).to_vec());
    session_engine_client::view_client::offer_window_actions();
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
        let at = place(PANE, &pane).await;
        run("WORKSPACE_CLOSE", json!({ "panes": [PANE] })).await?;
        *WAS_AT.lock().unwrap_or_else(PoisonError::into_inner) = at;
        return Ok("the mixer hidden".to_owned());
    }
    run(
        "EXT_VIEW_OPEN",
        json!({ "extension": WINDOW, "view": "mixer" }),
    )
    .await?;
    let at = std::mem::take(&mut *WAS_AT.lock().unwrap_or_else(PoisonError::into_inner));
    put_back(PANE, at).await;
    Ok("the mixer shown".to_owned())
}

/// Pane `id` moved to the first of `places` ([`place`]) that takes it.
/// Where it was may be gone (the pane above it closed too): then it stays
/// where it opened.
async fn put_back(id: &str, places: Vec<Json>) {
    for to in places {
        if run("WORKSPACE_MOVE", json!({ "pane": id, "to": to }))
            .await
            .is_ok()
        {
            return;
        }
    }
    tracing::debug!(pane = id, "studio.mixer: not put back where it was");
}

/// Where pane `id` (`pane`, `WORKSPACE_PANE_GET`'s answer) is, as
/// `WORKSPACE_MOVE` `to`s that put it back, to try in order: a dock or a
/// float; in the middle its group, at its index (while the group stays, its
/// other tabs keeping it), else split beside the pane next to it.
async fn place(id: &str, pane: &Json) -> Vec<Json> {
    let Some(at) = pane.get("place").and_then(Json::as_str) else {
        return Vec::new();
    };
    match at {
        "left_dock" => vec![json!({ "dock": { "side": "left" } })],
        "right_dock" => vec![json!({ "dock": { "side": "right" } })],
        "bottom_dock" => vec![json!({ "dock": { "side": "bottom" } })],
        "float" => vec![json!({ "float": { "rect": pane.get("float_rect").cloned() } })],
        _ => {
            let mut places = Vec::new();
            if let Some(group) = pane.get("group").and_then(Json::as_u64) {
                places.push(json!({ "group": {
                    "group": group,
                    "index": pane.get("index").and_then(Json::as_u64),
                } }));
            }
            for (toward, side) in [
                ("up", "bottom"),
                ("down", "top"),
                ("left", "right"),
                ("right", "left"),
            ] {
                let next = run(
                    "WORKSPACE_NEIGHBOR",
                    json!({ "pane": id, "direction": toward }),
                )
                .await
                .ok();
                if let Some(next) = next
                    .as_ref()
                    .and_then(|n| n.get("neighbor"))
                    .and_then(Json::as_str)
                {
                    places.push(json!({ "split": { "pane": next, "side": side } }));
                    break;
                }
            }
            places
        }
    }
}

/// The close-up pane (`closeup`, one is open at most).
const CLOSEUP_PANE: &str = "studio.mixer/closeup";

/// `SESSION_CLOSEUP` in the pane: a track's rack panel shown in the
/// close-up pane (opened, or moved to it), or the pane closed. What the
/// window's close-up layer does over the window; here the pane is its
/// own, and a strip's expand mark asks for it as in the window.
async fn closeup(a: mixer_view::mixer_actions::CloseupArg) -> Result<String, String> {
    if a.close {
        if run("WORKSPACE_PANE_GET", json!({ "pane": CLOSEUP_PANE }))
            .await
            .is_ok()
        {
            run("WORKSPACE_CLOSE", json!({ "panes": [CLOSEUP_PANE] })).await?;
        }
        return Ok("closed".to_owned());
    }
    let track = a.track.ok_or("`track` (its GUID), or `close`")?;
    let panel = a.panel.as_deref().unwrap_or("eq");
    let which = mixer_view::tone::Which::from_slug(panel).ok_or_else(|| {
        let all: Vec<String> = mixer_view::tone::Which::ALL
            .iter()
            .map(|w| w.slug())
            .collect();
        format!("no panel {panel:?}: one of {all:?}")
    })?;
    let state = mixer_view::closeup::rack_state(&track, which);
    // The pane shows what was asked last, told in this process (a process
    // view does not take up a state its pane is given once open:
    // architect#896); the pane keeps it too, so a saved layout opens on it.
    crate::closeup::ask(&track, which);
    if run("WORKSPACE_PANE_GET", json!({ "pane": CLOSEUP_PANE }))
        .await
        .is_ok()
    {
        run(
            "WORKSPACE_NAVIGATE",
            json!({ "pane": CLOSEUP_PANE, "state": state }),
        )
        .await?;
    } else {
        run(
            "EXT_VIEW_OPEN",
            json!({ "extension": WINDOW, "view": "closeup", "state": state }),
        )
        .await?;
    }
    Ok(format!("{} close up", which.slug()))
}
