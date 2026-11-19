//! Arranging the panes for a set: the steps `stack/run-studio.sh` ran by
//! hand, as host actions.
//!
//! A layout is applied by name ([`arrange`]): **Stage** and **Rehearse**
//! are made here each time (the panes opened, then the workspace's export
//! rewritten by [`crate::layout`] and applied), any other name is one a
//! person saved ([`save`]) and is applied as it was (the chart is pointed
//! at the engine's song after, by [`crate::follow`]). A step whose extension is not there (no lighting, no
//! chart) is skipped and said, never a failure: the rest still lays out.

use serde_json::{Value, json};

use crate::follow::Session;
use crate::host;
use crate::layout::{self, Area, Saved};

/// The reading zoom of the chart on the stage's upright display.
const CHART_ZOOM: f64 = 1.331;

/// The steps taken and those skipped, as they go.
#[derive(Default)]
struct Steps {
    done: Vec<String>,
    skipped: Vec<String>,
}

impl Steps {
    /// Runs `action`; a refusal is noted, not raised.
    async fn run(&mut self, action: &str, args: Value) -> Option<Value> {
        match host::run(action, args).await {
            Ok(out) => {
                self.done.push(action.to_owned());
                Some(out)
            }
            Err(why) => {
                self.skipped.push(format!("{action}: {why}"));
                None
            }
        }
    }

    async fn open(&mut self, extension: &str, view: &str, state: Option<String>) {
        let mut args = json!({ "extension": extension, "view": view, "placement": "split" });
        if let (Some(state), Some(fields)) = (state, args.as_object_mut()) {
            fields.insert("state".to_owned(), Value::String(state));
        }
        self.run("EXT_VIEW_OPEN", args).await;
    }

    async fn slot(&mut self, slot: &str, set: Value) {
        let mut args = json!({ "slot": slot });
        if let (Some(fields), Some(more)) = (args.as_object_mut(), set.as_object()) {
            fields.extend(more.clone());
        }
        self.run("HOME_SLOT_SET", args).await;
    }

    fn json(self, layout: &str) -> Value {
        json!({ "layout": layout, "steps": self.done.len(), "skipped": self.skipped })
    }
}

/// Every pane open now (`WORKSPACE_LAYOUT`'s tabs).
fn panes(layout: &Value) -> Vec<String> {
    layout
        .get("windows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|w| {
            w.get("groups")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|g| {
            g.get("tabs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|t| t.as_str().or_else(|| t.get("pane").and_then(Value::as_str)))
        .map(str::to_owned)
        .collect()
}

/// Waits until every pane of `wanted` is open, or a moment has passed: a
/// view asked for is not a pane yet when its open answers, and a move of
/// it then finds none. One that never comes (its extension is not there)
/// ends the wait.
async fn settled(wanted: &[&str]) {
    for _ in 0..20 {
        let open = host::run("WORKSPACE_LAYOUT", json!({}))
            .await
            .map(|l| panes(&l))
            .unwrap_or_default();
        if wanted.iter().all(|w| open.iter().any(|p| p == w)) {
            return;
        }
        futures_timer::Delay::new(std::time::Duration::from_millis(150)).await;
    }
}

/// A clean slate: the panes of the last layout closed, the engine named
/// as the session's provider.
async fn clear(steps: &mut Steps) {
    steps
        .run(
            "EXT_SERVICE_USE",
            json!({ "service": "fts.session", "provider": "daw" }),
        )
        .await;
    if let Ok(now) = host::run("WORKSPACE_LAYOUT", json!({})).await {
        let open = panes(&now);
        if !open.is_empty() {
            steps.run("WORKSPACE_CLOSE", json!({ "panes": open })).await;
        }
    }
}

/// The stage: the arrangement and mixer in the main window under the
/// song's progress bar, the set as tabs in its title row; the chart over
/// the lyrics in a window of its own; the room and its cues in another.
async fn stage(session: &Session) -> Value {
    let mut s = Steps::default();
    clear(&mut s).await;
    let chart_pane = format!("{}:session:{}", layout::CHART, session.id);
    s.run(
        "EXT_VIEW_OPEN",
        json!({ "extension": "studio.arrangement", "view": "arrangement" }),
    )
    .await;
    s.open("studio.mixer", "mixer", None).await;
    s.open("keyflow", "chart", Some(format!("session:{}", session.id)))
        .await;
    s.open("ignition", "show", None).await;
    s.open("studio.lyrics", "lyrics", None).await;
    s.open("studio.navigator", "progress", None).await;
    s.open("ignition", "cues", None).await;
    settled(&[
        layout::ARRANGEMENT,
        layout::MIXER,
        &chart_pane,
        layout::SHOW,
        layout::LYRICS,
        layout::PROGRESS,
        layout::CUES,
    ])
    .await;
    // No Now Playing bar, status bar or bottom dock: the arrangement has
    // the transport, and the rows are height the views want. The title
    // row is the set, a tab a song; no tab bars.
    s.slot("now_playing", json!({ "hidden": true })).await;
    s.slot("status", json!({ "hidden": true })).await;
    s.run(
        "WORKSPACE_DOCK_SET",
        json!({ "side": "bottom", "shown": "hidden" }),
    )
    .await;
    s.slot("title", json!({ "view": "studio.navigator/songs" }))
        .await;
    s.slot("tabs", json!({ "hidden": true })).await;
    s.run("KEYFLOW_EDIT_MODE", json!({ "on": false })).await;
    let area = host::run("WORKSPACE_LAYOUT", json!({}))
        .await
        .ok()
        .as_ref()
        .and_then(Area::of)
        .unwrap_or_default();
    for pane in [
        layout::ARRANGEMENT,
        layout::MIXER,
        layout::PROGRESS,
        layout::NAVIGATOR,
    ] {
        // Not open (the set's navigator stays closed until asked for): not a step skipped.
        let _ = host::run(
            "WORKSPACE_MOVE",
            json!({ "pane": pane, "to": { "window": { "id": "main" } } }),
        )
        .await;
    }
    s.run(
        "WORKSPACE_MOVE",
        json!({ "pane": chart_pane, "to": { "window": {} } }),
    )
    .await;
    s.run(
        "WORKSPACE_MOVE",
        json!({ "pane": layout::SHOW, "to": { "window": {} } }),
    )
    .await;
    s.run(
        "WORKSPACE_MOVE",
        json!({ "pane": layout::LYRICS, "to": { "split": { "pane": chart_pane, "side": "bottom" } } }),
    )
    .await;
    s.run(
        "WORKSPACE_MOVE",
        json!({ "pane": layout::CUES, "to": { "split": { "pane": layout::SHOW, "side": "right" } } }),
    )
    .await;
    if let Some(export) = s.run("WORKSPACE_EXPORT", json!({})).await {
        s.run(
            "WORKSPACE_APPLY",
            json!({ "layout": layout::stage(export, area) }),
        )
        .await;
    }
    s.run(
        "WORKSPACE_DOCK_SET",
        json!({ "side": "left", "shown": "collapsed" }),
    )
    .await;
    s.run("COMMANDS_RUN", json!({ "id": "SHELL_SIDEBAR_RAIL" }))
        .await;
    s.run(
        "KEYFLOW_CHART_VIEW",
        json!({ "zoom": CHART_ZOOM, "scrollY": 0 }),
    )
    .await;
    s.json("Stage")
}

/// Rehearsing: one window, the arrangement over the mixer, the chart
/// over the lyrics beside them; the app's own bars as they are.
async fn rehearse(session: &Session) -> Value {
    let mut s = Steps::default();
    clear(&mut s).await;
    s.run(
        "EXT_VIEW_OPEN",
        json!({ "extension": "studio.arrangement", "view": "arrangement" }),
    )
    .await;
    s.open("studio.mixer", "mixer", None).await;
    s.open("keyflow", "chart", Some(format!("session:{}", session.id)))
        .await;
    s.open("studio.lyrics", "lyrics", None).await;
    let chart_pane = format!("{}:session:{}", layout::CHART, session.id);
    settled(&[
        layout::ARRANGEMENT,
        layout::MIXER,
        &chart_pane,
        layout::LYRICS,
    ])
    .await;
    for slot in ["now_playing", "status", "title", "tabs"] {
        // The stage's bars put back, when it was the layout before.
        let _ = host::run("HOME_SLOT_SET", json!({ "slot": slot, "clear": true })).await;
    }
    s.run("KEYFLOW_EDIT_MODE", json!({ "on": false })).await;
    if let Some(export) = s.run("WORKSPACE_EXPORT", json!({})).await {
        s.run(
            "WORKSPACE_APPLY",
            json!({ "layout": layout::rehearse(export) }),
        )
        .await;
    }
    s.json("Rehearse")
}

/// The data key a saved layout is kept under.
fn key(name: &str) -> String {
    format!("layout:{}", name.trim().to_lowercase())
}

/// The layouts a person saved, by name.
const NAMES: &str = "layouts";

/// The names of the saved layouts.
pub async fn saved_names() -> Vec<String> {
    host::data(NAMES)
        .await
        .and_then(|v| v.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// Saves the workspace as it is now as layout `name`, on this device.
///
/// # Errors
///
/// The name is one the launcher makes itself, or the workspace refused.
pub async fn save(name: &str) -> Result<Value, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the layout a name".to_owned());
    }
    if layout::BUILT_IN
        .iter()
        .any(|b| b.eq_ignore_ascii_case(name))
    {
        return Err(format!(
            "{name} is a layout the launcher makes itself: save under another name"
        ));
    }
    let saved = Saved {
        layout: host::run("WORKSPACE_EXPORT", json!({})).await?,
    };
    host::set_data(&key(name), Some(saved.json())).await?;
    let mut names = saved_names().await;
    if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
        names.push(name.to_owned());
        host::set_data(NAMES, Some(json!(names))).await?;
    }
    Ok(json!({ "saved": name, "layouts": names }))
}

/// Applies layout `name` for the session the engine has.
///
/// # Errors
///
/// The engine has no song (a layout is of a song's panes), or no layout
/// has that name.
pub async fn arrange(name: &str) -> Result<Value, String> {
    let name = name.trim();
    let session = crate::follow::session().await;
    if name.eq_ignore_ascii_case("Stage") || name.eq_ignore_ascii_case("Rehearse") {
        let session = session.ok_or("the engine has no song yet: open a set, then arrange")?;
        return Ok(if name.eq_ignore_ascii_case("Stage") {
            stage(&session).await
        } else {
            rehearse(&session).await
        });
    }
    let saved = host::data(&key(name))
        .await
        .as_ref()
        .and_then(Saved::of)
        .ok_or_else(|| format!("no layout `{name}` (STUDIO_LAUNCHER_LAYOUTS lists them)"))?;
    // The panes of the layout before are closed first: applying a layout
    // opens what it names and leaves the rest where it was.
    let mut steps = Steps::default();
    clear(&mut steps).await;
    host::run("WORKSPACE_APPLY", json!({ "layout": saved.layout })).await?;
    steps.done.push("WORKSPACE_APPLY".to_owned());
    Ok(steps.json(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_are_every_tab_of_every_window() {
        let layout = json!({ "windows": [
            { "groups": [ { "tabs": ["a", { "pane": "b" }] }, { "tabs": [] } ] },
            { "groups": [ { "tabs": [{ "pane": "c", "title": "C" }] } ] },
        ] });
        assert_eq!(panes(&layout), ["a", "b", "c"]);
        assert_eq!(key(" Sunday Stage "), "layout:sunday stage");
    }
}
