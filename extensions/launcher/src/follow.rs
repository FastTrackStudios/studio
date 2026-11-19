//! The chart and the lighting follow the song the engine has.
//!
//! Studio's arrangement, mixer, lyrics and navigator read the engine's set
//! themselves; Keyflow's chart and Ignition's room are told which session
//! to follow (`KEYFLOW_SESSION_FOLLOW`, `IGNITION_SESSION_SONG`,
//! `IGNITION_SESSION_ROOM`), and must be told again when the engine goes
//! to another song. `stack/follow-song.sh` did that from outside, with a
//! table of the names that differ between a session's folder, its song's
//! note and its cue file. Here the vault says it: the song whose session
//! the engine has is a place in a setlist ([`target`]), and that place has
//! its chart and its cues.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{Found, Loaded, find, host};

/// The session the engine has: its document's id, the address its
/// transport is led from, and its path (`DAW_STATE`'s `session`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub url: String,
    pub path: String,
}

impl Session {
    /// From a `DAW_STATE` answer, when the engine has a song.
    #[must_use]
    pub fn of(state: &Value) -> Option<Self> {
        let text = |k: &str| {
            state
                .pointer(&format!("/session/{k}"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        };
        Some(Self {
            id: text("id")?,
            url: text("url")?,
            path: text("path")?,
        })
    }
}

/// The session the engine has now.
pub async fn session() -> Option<Session> {
    Session::of(&host::run("DAW_STATE", json!({})).await.ok()?)
}

/// What a song's chart and lighting are pointed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// How its setlist calls it.
    pub label: String,
    /// Its chart's file.
    pub chart: Option<PathBuf>,
    /// Its lighting cues' file.
    pub cues: Option<PathBuf>,
    /// The Ableton set its lighting reads its locators from.
    pub live_set: Option<PathBuf>,
}

/// A session's path without its extension: a song's `.session`, `.RPP`
/// and the `.fts-session` the engine keeps beside it are one song.
fn song_of(path: &Path) -> PathBuf {
    path.with_extension("")
}

/// The place in a setlist whose session is `session_path`, with its chart
/// and its cues: the setlist showing first, then the rest.
#[must_use]
pub fn target(loaded: &Loaded, session_path: &str, showing: Option<&str>) -> Option<Target> {
    let vault = PathBuf::from(loaded.vault_root.as_deref()?);
    let want = song_of(Path::new(session_path));
    let place = |found: &Found| {
        let at = found
            .sessions
            .iter()
            .position(|s| s.as_deref().is_some_and(|p| song_of(p) == want))?;
        let entry = found.setlist.entries().nth(at)?;
        let project = find::project_root(&vault, &found.setlist.project);
        let chart = entry
            .song
            .as_ref()
            .and_then(|s| s.chart_path.as_deref())
            .map(|c| vault.join(c))
            .filter(|c| c.exists())
            .or_else(|| find::chart_beside(&vault.join(&entry.note)));
        Some(Target {
            label: entry.label.clone(),
            chart,
            cues: find::cues(&vault, &found.setlist.project, &entry.label),
            live_set: find::live_set(&project),
        })
    };
    let first = loaded
        .setlists
        .iter()
        .filter(|f| Some(f.setlist.path.as_str()) == showing);
    let rest = loaded
        .setlists
        .iter()
        .filter(|f| Some(f.setlist.path.as_str()) != showing);
    first.chain(rest).find_map(place)
}

/// The pane that shows Keyflow's chart, in a `WORKSPACE_LAYOUT` answer.
#[must_use]
pub fn chart_pane(layout: &Value) -> Option<String> {
    layout
        .get("windows")?
        .as_array()?
        .iter()
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
        .find(|pane| pane.starts_with(crate::layout::CHART))
        .map(str::to_owned)
}

/// Points the chart and the lighting at `target`, the song of `session`:
/// what was told, and what was not there to tell.
pub async fn point(session: &Session, target: &Target) -> Value {
    let mut told = Vec::new();
    let mut skipped = Vec::new();
    match &target.chart {
        Some(chart) => {
            let follow = host::run(
                "KEYFLOW_SESSION_FOLLOW",
                json!({ "url": session.url, "id": session.id, "chart_path": chart }),
            )
            .await;
            match follow {
                Ok(_) => told.push("chart"),
                Err(why) => skipped.push(format!("chart: {why}")),
            }
            // The chart's pane goes to this session, when one is open.
            if let Some(pane) = host::run("WORKSPACE_LAYOUT", json!({}))
                .await
                .ok()
                .as_ref()
                .and_then(chart_pane)
            {
                let _ = host::run(
                    "WORKSPACE_NAVIGATE",
                    json!({ "pane": pane, "state": format!("session:{}", session.id) }),
                )
                .await;
            }
        }
        None => skipped.push("chart: the song has none".to_owned()),
    }
    let lane = target
        .cues
        .as_deref()
        .and_then(Path::file_stem)
        .and_then(|s| s.to_str())
        .map(str::to_owned);
    match (&target.cues, lane, &target.live_set) {
        (Some(cues), Some(lane), Some(live_set)) => {
            let locator = Path::new(&session.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&target.label);
            let song = host::run(
                "IGNITION_SESSION_SONG",
                json!({
                    "doc": session.id, "lane": lane, "live_set": live_set,
                    "locator": locator, "cues": cues, "tempo": false,
                }),
            )
            .await;
            let room = host::run(
                "IGNITION_SESSION_ROOM",
                json!({ "url": session.url, "doc": session.id, "song": lane, "lane": lane }),
            )
            .await;
            match song.and(room) {
                Ok(_) => told.push("lighting"),
                Err(why) => skipped.push(format!("lighting: {why}")),
            }
        }
        (None, ..) => skipped.push("lighting: the project has no cues for it".to_owned()),
        (_, _, None) => skipped.push("lighting: the project has no Live set".to_owned()),
        (Some(_), None, _) => skipped.push("lighting: the cue file has no name".to_owned()),
    }
    json!({ "song": target.label, "told": told, "skipped": skipped })
}

/// The session last pointed at, and when the engine was last asked.
static LAST: Mutex<(Option<String>, Option<Instant>)> = Mutex::new((None, None));

/// How often the engine is asked which song it has, at most: the deck
/// reports many times a second while it plays.
const EVERY: Duration = Duration::from_millis(900);

/// Forgets what was pointed at, so the next [`follow`] points again.
pub fn forget() {
    *LAST.lock().unwrap_or_else(PoisonError::into_inner) = (None, None);
}

/// Points the chart and the lighting at the engine's song when it is
/// another than the one last pointed at (`force`: whatever it is).
///
/// # Errors
///
/// The vault could not be read.
pub async fn follow(force: bool) -> Result<Value, String> {
    {
        let mut last = LAST.lock().unwrap_or_else(PoisonError::into_inner);
        if !force && last.1.is_some_and(|at| at.elapsed() < EVERY) {
            return Ok(json!({ "followed": false, "why": "asked a moment ago" }));
        }
        last.1 = Some(Instant::now());
    }
    let Some(session) = session().await else {
        return Ok(json!({ "followed": false, "why": "the engine has no song" }));
    };
    let same = LAST
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .0
        .as_deref()
        == Some(session.id.as_str());
    if same && !force {
        return Ok(json!({ "followed": false, "why": "the same song" }));
    }
    LAST.lock().unwrap_or_else(PoisonError::into_inner).0 = Some(session.id.clone());
    let loaded = crate::load().await?;
    let Some(target) = target(&loaded, &session.path, crate::chosen().as_deref()) else {
        return Ok(json!({
            "followed": false,
            "why": "the engine's song is not a song of a setlist in this vault",
            "session": session.path,
        }));
    };
    let mut said = point(&session, &target).await;
    if let Some(fields) = said.as_object_mut() {
        fields.insert("followed".to_owned(), Value::Bool(true));
    }
    Ok(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_read_from_the_engines_state() {
        let state = json!({ "up": true, "session": { "id": "abc", "url": "ws://x/vox", "path": "/p/A.fts-session" } });
        assert_eq!(
            Session::of(&state),
            Some(Session {
                id: "abc".to_owned(),
                url: "ws://x/vox".to_owned(),
                path: "/p/A.fts-session".to_owned()
            })
        );
        assert_eq!(
            Session::of(&json!({ "session": { "id": null, "path": null } })),
            None
        );
    }

    #[test]
    fn the_chart_pane_is_found_in_the_layout() {
        let layout = json!({ "windows": [
            { "groups": [ { "tabs": ["studio.mixer/mixer"] } ] },
            { "groups": [ { "tabs": [ { "pane": "keyflow/chart:session:abc" } ] } ] },
        ] });
        assert_eq!(
            chart_pane(&layout).as_deref(),
            Some("keyflow/chart:session:abc")
        );
        assert_eq!(chart_pane(&json!({ "windows": [] })), None);
    }

    #[test]
    fn a_song_is_its_session_whatever_the_extension() {
        assert_eq!(
            song_of(Path::new("/p/a/A.fts-session")),
            song_of(Path::new("/p/a/A.session"))
        );
        assert_ne!(
            song_of(Path::new("/p/a/A.RPP")),
            song_of(Path::new("/p/b/A.RPP"))
        );
    }
}
