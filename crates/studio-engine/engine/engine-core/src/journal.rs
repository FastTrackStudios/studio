//! The song's journal: every edit, whose it was, and the song's revision
//! (`apps/studio/docs/mcp.md`). Every door into the Action List notes its
//! edits here (`off_loop::run_by`), so it is the core's; the agent's
//! actions on it (`AGENT_RUN`, `SESSION_CHANGES`, …) are engine-collab's
//! `agent`, which re-exports this module.
//!
//! - **The song's revision** ([`revision`]): a number that moves on every
//!   edit of the song, whoever made it. An edit is a mutating action that
//!   moved the engine's own project revision (or changed which song is
//!   open), or an undo step the window finished outside any action (a
//!   drag). What only changes the view — a selection, a fold, a zoom, the
//!   transport — is not one ([`view_only`]), though it moves the project's
//!   revision: so an agent's edit is not refused because the person
//!   clicked a track.
//! - **What changed** ([`Journal::since`], `SESSION_CHANGES`): each edit, kept in a
//!   bounded journal ([`KEPT`]), with who made it (`person` through the
//!   window, keys and menus; `agent` through `AGENT_RUN`), the action and
//!   what it named (track, block, the fields or parameters it set).
//! - **Conditional edits** (`AGENT_RUN {id, args, if_revision}`): an
//!   action run for an agent, refused as `stale` with what changed when the
//!   song is no longer at `if_revision`.
//! - **One undo step** (`AGENT_RUN {step}`, `AGENT_STEP_END`): every step
//!   made between them is one undo step named `step`
//!   (`project_file::group_begin`).
//!
//! Every door into the engine's list for a person (`off_loop::run`: the
//! window's calls, the app's `DAW_RUN`) notes its edits here as the
//! person's; `AGENT_RUN` runs through the same door as the agent.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use architect::action::Effect;
use serde_json::{Value, json};

/// Who made an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    /// The person: the window, a key, a menu, the app's own calls.
    Person,
    /// An agent, through `AGENT_RUN` (Studio's MCP).
    Agent,
}

impl By {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Agent => "agent",
        }
    }
}

/// One edit of the song.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The revision it moved the song to.
    pub revision: u64,
    pub by: By,
    /// The action that made it; `None` for a step the window finished
    /// outside any action.
    pub action: Option<String>,
    /// The action's name, or the step's.
    pub label: String,
    /// What it changed: `track`, `fx`, `send`, `items`, `marker`, `mix`,
    /// `undo`, `song`.
    pub kind: &'static str,
    pub track: Option<String>,
    pub block: Option<String>,
    /// The fields or parameters it set (`volume_db`, `band.3.gain`).
    pub fields: Vec<String>,
    /// When, local time (`14:05:09`).
    pub at: String,
}

/// How many edits the journal keeps; a revision older than that is
/// answered with what is kept and `complete: false`.
pub const KEPT: usize = 2000;

/// The song's edits: its revision and the last [`KEPT`] of them.
#[derive(Debug, Default)]
pub struct Journal {
    revision: u64,
    /// The newest revision no longer kept (0: none forgotten).
    forgotten: u64,
    changes: VecDeque<Change>,
}

impl Journal {
    /// An empty journal, at revision 0.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            revision: 0,
            forgotten: 0,
            changes: VecDeque::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Note an edit: the song's new revision.
    pub fn note(&mut self, by: By, action: Option<&str>, label: &str, args: Option<&Value>) -> u64 {
        let (track, block, fields) = args.map(targets).unwrap_or_default();
        let kind = action.map_or_else(|| kind_of_label(label), kind_of);
        self.revision = self.revision.saturating_add(1);
        if self.changes.len() >= KEPT
            && let Some(old) = self.changes.pop_front()
        {
            self.forgotten = old.revision;
        }
        self.changes.push_back(Change {
            revision: self.revision,
            by,
            action: action.map(str::to_owned),
            label: label.to_owned(),
            kind,
            track,
            block,
            fields,
            at: chrono::Local::now().format("%H:%M:%S").to_string(),
        });
        self.revision
    }

    /// What changed after revision `since`, oldest first, whether that is
    /// all of it (false: older edits were forgotten, or `since` is from
    /// before this engine started), and the revision now.
    #[must_use]
    pub fn since(&self, since: u64) -> (Vec<Change>, bool, u64) {
        let changes: Vec<Change> = self
            .changes
            .iter()
            .filter(|c| c.revision > since)
            .cloned()
            .collect();
        let complete = since >= self.forgotten && since <= self.revision;
        (changes, complete, self.revision)
    }

    /// Whether a call made at `want` may run now; the refusal when it may
    /// not (`code: "stale"`, with the edits since).
    #[must_use]
    pub fn check(&self, want: u64) -> Option<Value> {
        let now = self.revision;
        if want == now {
            return None;
        }
        let (changes, complete, now) = self.since(want);
        let by_person = changes.iter().filter(|c| c.by == By::Person).count();
        let error = if want > now {
            format!(
                "stale: the song is at revision {now}, not {want} — Studio's engine restarted since that revision was read. Read the song again, then retry with if_revision: {now}"
            )
        } else {
            format!(
                "stale: the song moved on from revision {want} to {now} ({} edits since, {by_person} by the person) — nothing was run. Read what you rely on again (`changes {{since: {want}}}` says what moved), then retry with if_revision: {now}",
                changes.len()
            )
        };
        let shown: Vec<Value> = changes
            .iter()
            .skip(changes.len().saturating_sub(10))
            .map(change_json)
            .collect();
        Some(json!({
            "ok": false, "code": "stale", "error": error,
            "revision": now, "since": want, "complete": complete,
            "changes": shown, "more": changes.len().saturating_sub(10),
        }))
    }

    /// SESSION_CHANGES's answer: a page of the edits after `from`.
    #[must_use]
    pub fn page(&self, from: u64, offset: u64, limit: u64) -> Value {
        let (changes, complete, now) = self.since(from);
        let total = changes.len();
        let start = usize::try_from(offset).unwrap_or(usize::MAX).min(total);
        let end = start
            .saturating_add(usize::try_from(limit).unwrap_or(usize::MAX))
            .min(total);
        let page: Vec<Value> = changes
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .map(change_json)
            .collect();
        let by_agent = changes.iter().filter(|c| c.by == By::Agent).count();
        let mut v = json!({
            "revision": now, "since": from, "complete": complete, "total": total,
            "by_person": total - by_agent, "by_agent": by_agent,
            "offset": start, "changes": page,
        });
        if end < total
            && let Some(o) = v.as_object_mut()
        {
            o.insert("next".into(), json!(end));
        }
        v
    }
}

static JOURNAL: Mutex<Journal> = Mutex::new(Journal::new());

/// The engine's journal, locked.
pub fn journal() -> std::sync::MutexGuard<'static, Journal> {
    JOURNAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The song's revision now: how many edits this engine has seen since it
/// started (0 before the first).
#[must_use]
pub fn revision() -> u64 {
    journal().revision()
}

/// Note an edit in the engine's journal: the song's new revision.
pub fn note(by: By, action: Option<&str>, label: &str, args: Option<&Value>) -> u64 {
    journal().note(by, action, label, args)
}

/// [`Journal::check`] on the engine's journal.
#[must_use]
pub fn check(want: u64) -> Option<Value> {
    journal().check(want)
}

/// What an action's arguments name: its track, its block and the fields
/// or parameters it sets.
fn targets(args: &Value) -> (Option<String>, Option<String>, Vec<String>) {
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let track = args.get("track").and_then(text).or_else(|| {
        let tracks: Vec<&str> = args
            .get("tracks")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        (!tracks.is_empty()).then(|| {
            let mut t = tracks
                .iter()
                .take(3)
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            if tracks.len() > 3 {
                t.push_str(&format!(" +{}", tracks.len() - 3));
            }
            t
        })
    });
    let block = args.get("block").and_then(text);
    const NOT_FIELDS: &[&str] = &[
        "track", "tracks", "block", "fx", "lane", "mark", "dry_run", "detail", "params",
    ];
    let mut fields: Vec<String> = match args.get("params") {
        Some(Value::Object(p)) => p.keys().cloned().collect(),
        _ => Vec::new(),
    };
    if let Some(o) = args.as_object() {
        fields.extend(
            o.keys()
                .filter(|k| !NOT_FIELDS.contains(&k.as_str()))
                .cloned(),
        );
    }
    fields.truncate(12);
    (track, block, fields)
}

/// What an action changes, by its id.
#[must_use]
pub fn kind_of(id: &str) -> &'static str {
    let has = |s: &str| id.contains(s);
    if has("UNDO") || has("REDO") || has("HISTORY_RESTORE") {
        "undo"
    } else if has("SEND") {
        "send"
    } else if has("MARKER") || has("REGION") || has("SECTION") || has("ZONES") {
        "marker"
    } else if id.starts_with("FX_") || id.starts_with("SIGNAL_") || has("_FX") || has("PLUGIN") {
        "fx"
    } else if id.starts_with("MIX_") {
        "mix"
    } else if id.starts_with("TRACK") || has("_TRACK") {
        "track"
    } else if has("ITEM")
        || has("ARRANGE")
        || has("DRUMS_")
        || has("HITS_")
        || has("QUANTIZE")
        || has("TAKE")
        || has("COMP")
        || has("LANE")
        || has("SPLIT")
        || has("FADE")
    {
        "items"
    } else {
        "song"
    }
}

/// What a step the window finished changed, by its name.
fn kind_of_label(label: &str) -> &'static str {
    let l = label.to_lowercase();
    if l.starts_with("undo") || l.starts_with("redo") {
        "undo"
    } else if l.contains("send") {
        "send"
    } else if l.contains("marker") || l.contains("region") || l.contains("section") {
        "marker"
    } else if l.contains("fx") || l.contains("plugin") || l.contains("param") {
        "fx"
    } else if l.contains("track")
        || l.contains("fader")
        || l.contains("volume")
        || l.contains("pan")
    {
        "track"
    } else {
        "items"
    }
}

/// Whether an action only changes what is shown or played, never the song:
/// a selection, a fold, a zoom, a panel, the transport, a saved copy.
#[must_use]
pub fn view_only(id: &str) -> bool {
    const SHOWN: &[&str] = &[
        "SELECT",
        "FOLD",
        "ZOOM",
        "SCROLL",
        "PANEL",
        "_VIEW",
        "SHOW_",
        "FOCUS",
        "SESSION_MODE",
        "SESSION_PHASE",
        "MIX_SNAPSHOT",
        "SESSION_SAVE",
        "BACKUP",
        "_SOLO_PREVIEW",
        "METER",
        "WINDOW",
    ];
    const PLAYED: &[&str] = &[
        "TRANSPORT",
        "PLAY",
        "STOP",
        "PAUSE",
        "SEEK",
        "LOOP",
        "UI_",
        "CURSOR",
        "LAUNCHER",
    ];
    SHOWN.iter().any(|s| id.contains(s)) || PLAYED.iter().any(|p| id.starts_with(p))
}

/// The ids of this module's own actions: never noted as edits themselves.
const OURS: &[&str] = &[
    "AGENT_RUN",
    "AGENT_STEP_BEGIN",
    "AGENT_STEP_END",
    "SESSION_CHANGES",
];

/// Each action's effect, kept once looked up (`ActionList::info` builds
/// its schemas).
fn effect_of(id: &str) -> Option<Effect> {
    static EFFECTS: std::sync::LazyLock<Mutex<HashMap<String, Effect>>> =
        std::sync::LazyLock::new(Default::default);
    if let Some(e) = EFFECTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(id)
    {
        return Some(*e);
    }
    let effect = crate::actions::list().info(id)?.effect;
    EFFECTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(id.to_owned(), effect);
    Some(effect)
}

/// Whether action `id` may change something (not read-only).
#[must_use]
pub fn mutating(id: &str) -> bool {
    effect_of(id).is_some_and(|e| !matches!(e, Effect::ReadOnly))
}

thread_local! {
    /// Actions being watched on this thread: a step finished here is theirs.
    static IN_ACTION: Cell<u32> = const { Cell::new(0) };
}

/// Async mutating actions in flight: their steps may finish on any thread.
static ASYNC_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// The song and the engine's revision of it, before or after a call.
type SongState = (Option<String>, Option<u64>);

fn song_state() -> SongState {
    let song = crate::open::current_song();
    let revision = song.as_ref().and_then(|guid| {
        crate::open::local_engine().and_then(|daw| daw.read_project(guid, |p| p.revision))
    });
    (song, revision)
}

/// A mutating action call being watched, to note it if it edited the song.
pub struct Watch {
    id: String,
    before: SongState,
}

/// Watch action `id`, when it may edit the song (not read-only, not one
/// of this module's own).
#[must_use]
pub fn watch(id: &str) -> Option<Watch> {
    if OURS.contains(&id) || !mutating(id) {
        return None;
    }
    Some(Watch {
        id: id.to_owned(),
        before: song_state(),
    })
}

impl Watch {
    /// The call ended (`ok`): note it as `by`'s edit when it moved the
    /// song. The song's revision when it did.
    pub fn finish(self, by: By, ok: bool, args: &[u8]) -> Option<u64> {
        if !ok || view_only(&self.id) {
            return None;
        }
        let after = song_state();
        let moved = after.0 != self.before.0 || after.1 != self.before.1;
        if !moved {
            return None;
        }
        let args: Option<Value> = serde_json::from_slice(args).ok();
        let label = crate::actions::list()
            .info(&self.id)
            .map_or_else(|| self.id.clone(), |i| i.display_name);
        Some(note(by, Some(&self.id), &label, args.as_ref()))
    }
}

/// While held, a step finished on this thread is the watched action's.
pub struct InAction(());

impl InAction {
    #[must_use]
    pub fn enter() -> Self {
        IN_ACTION.with(|c| c.set(c.get().saturating_add(1)));
        Self(())
    }
}

impl Drop for InAction {
    fn drop(&mut self) {
        IN_ACTION.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

/// While held, an async mutating action is in flight.
pub struct InFlight(());

impl InFlight {
    #[must_use]
    pub fn enter() -> Self {
        ASYNC_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        Self(())
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        ASYNC_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The engine finished the undo step `label`: noted as the person's when
/// no watched action made it (a drag in the window).
pub fn step_finished(label: &str) {
    if IN_ACTION.with(Cell::get) > 0 || ASYNC_IN_FLIGHT.load(Ordering::SeqCst) > 0 {
        return;
    }
    note(By::Person, None, label, None);
}

/// The JSON of one change.
#[must_use]
pub fn change_json(c: &Change) -> Value {
    let mut v = json!({"revision": c.revision, "by": c.by.as_str(), "label": c.label, "kind": c.kind, "at": c.at});
    if let Some(o) = v.as_object_mut() {
        if let Some(a) = &c.action {
            o.insert("action".into(), json!(a));
        }
        if let Some(t) = &c.track {
            o.insert("track".into(), json!(t));
        }
        if let Some(b) = &c.block {
            o.insert("block".into(), json!(b));
        }
        if !c.fields.is_empty() {
            o.insert("fields".into(), json!(c.fields));
        }
    }
    v
}

#[cfg(test)]
mod tests;
