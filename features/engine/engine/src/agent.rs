//! An agent's edits beside the person's (`apps/studio/docs/mcp.md`).
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

use architect::action::{ActionError, ActionList, ActionMeta, Effect};
use facet::Facet;
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

fn journal() -> std::sync::MutexGuard<'static, Journal> {
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

fn mutating(id: &str) -> bool {
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

// ── the actions ─────────────────────────────────────────────────────────

static RUN: ActionMeta = crate::actions::meta(
    "AGENT_RUN",
    "agent_run",
    "Run an action for an agent",
    "Run the action `id` with `args` (its own JSON) as an agent's call: its edits are noted as the agent's in the song's journal (SESSION_CHANGES), apart from the person's. `if_revision`: run it only if the song is still at that revision (what every answer of Studio's MCP says) — when it has moved on, nothing runs and the answer is `{ok: false, code: \"stale\", revision, since, changes}`, the edits since and who made them. `step`: the name of an undo step (`Agent: tighten snare`) this call's edits join, with every other call naming it until AGENT_STEP_END — SESSION_UNDO then takes them all back at once (a song open from its project file). Answers `{ok: true, answer, revision, edit, step}` (`edit`: whether this call moved the song on), or `{ok: false, code, error}` (`stale`, `bad_args` with `takes`, `unknown`, `failed`, `denied`, `disabled`, `confirm` with `ticket`).",
    "Agent",
    "",
);

static STEP_BEGIN: ActionMeta = crate::actions::meta(
    "AGENT_STEP_BEGIN",
    "agent_step_begin",
    "Begin an agent's undo step",
    "From now, every edit of the open song (an agent's, and the person's meanwhile) joins one undo step named `step` (`Agent: tighten snare`), until AGENT_STEP_END with the same name; SESSION_UNDO then takes it back whole, SESSION_REDO brings it back. Left open past `timeout_s` (3700 s), it is dropped and its edits stay steps of their own. A song open from its project file only: `{grouped: false, why}` otherwise. AGENT_RUN's `step` begins one by itself at its first edit.",
    "Agent",
    "",
);

static STEP_END: ActionMeta = crate::actions::meta(
    "AGENT_STEP_END",
    "agent_step_end",
    "End an agent's undo step",
    "End the undo step `step` (AGENT_STEP_BEGIN, or AGENT_RUN's `step`): `{undo, steps, revision}` — `undo` the step's name as SESSION_UNDO will say it (null when it changed nothing), `steps` how many edits it holds. Nothing open of that name answers `undo: null`.",
    "Agent",
    "",
);

static CHANGES: ActionMeta = crate::actions::meta(
    "SESSION_CHANGES",
    "session_changes",
    "What changed in the song",
    "The song's edits after revision `since` (every answer of Studio's MCP says the revision it read; 0 or none: everything kept), oldest first: each `{revision, by (person or agent), action, label, kind (track, fx, send, items, marker, mix, undo, song), track, block, fields, at}`. Paged by `offset`/`limit` (50, 500 at most). `revision` is the song's now; `complete: false` when older edits were forgotten (the last 2000 are kept) or `since` is from before the engine started. View changes (selection, zoom, transport) are not edits. Read-only.",
    "Agent",
    "",
);

/// What AGENT_RUN takes, for its schema (it reads the JSON itself: `args`
/// is any object).
#[derive(Facet)]
struct RunDoc {
    id: String,
    /// The action's own arguments, an object.
    args: Option<String>,
    if_revision: Option<u64>,
    step: Option<String>,
}

#[derive(Facet)]
struct StepDoc {
    step: String,
    timeout_s: Option<f64>,
}

#[derive(Facet)]
struct ChangesDoc {
    since: Option<u64>,
    offset: Option<u32>,
    limit: Option<u32>,
}

fn parse(id: &'static str, raw: &[u8], doc: &'static facet::Shape) -> Result<Value, ActionError> {
    serde_json::from_slice(if raw.is_empty() { b"{}" } else { raw }).map_err(|e| {
        ActionError::BadArgs {
            id: id.to_owned(),
            message: e.to_string(),
            expected: Some(architect::schema::json_schema(doc)),
        }
    })
}

fn bad(id: &'static str, message: &str, doc: &'static facet::Shape) -> ActionError {
    ActionError::BadArgs {
        id: id.to_owned(),
        message: message.to_owned(),
        expected: Some(architect::schema::json_schema(doc)),
    }
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

/// AGENT_RUN's failure, as the answer it gives.
fn refusal(id: &str, e: ActionError) -> Value {
    let (code, error, extra) = match e {
        ActionError::Unknown(id) => ("unknown", format!("no action {id}"), None),
        ActionError::BadArgs {
            id,
            message,
            expected,
        } => (
            "bad_args",
            format!("{id}: bad arguments: {message}"),
            expected
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .map(|t| ("takes", t)),
        ),
        ActionError::Failed { message, .. } => ("failed", format!("{id}: {message}"), None),
        ActionError::Denied { reason, .. } => {
            ("denied", format!("{id}: not allowed: {reason}"), None)
        }
        ActionError::Disabled { reason, .. } => {
            ("disabled", format!("{id}: disabled: {reason}"), None)
        }
        ActionError::ConfirmationRequired { ticket, .. } => (
            "confirm",
            format!("{id}: a person must approve this call first; retry with its ticket"),
            Some(("ticket", json!(ticket))),
        ),
        ActionError::Duplicate(id) => ("failed", format!("{id}: already registered"), None),
    };
    let mut v = json!({"ok": false, "code": code, "error": error, "revision": revision()});
    if let (Some((k, x)), Some(o)) = (extra, v.as_object_mut()) {
        o.insert(k.into(), x);
    }
    v
}

/// Undo steps an agent has open: by name, the song each was begun on.
fn steps_open() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    static OPEN: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
        std::sync::LazyLock::new(Default::default);
    OPEN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How long an undo step may stay open before it is dropped.
const STEP_TIMEOUT_S: f64 = 3700.0;

/// Begin the undo step `step` on the open song (if it is not already):
/// whether it is one now, or why not.
fn step_begin(step: &str, timeout_s: f64) -> Result<String, String> {
    let song = crate::open::current_song().ok_or("no song open")?;
    let had = steps_open().get(step).cloned();
    if had.as_deref() == Some(song.as_str()) && crate::project_file::group_open(&song, step) {
        return Ok(song);
    }
    if !crate::project_file::group_begin(&song, step) {
        return Err("the song is not open from its project file, so its undo has no groups: each edit is its own undo step".to_owned());
    }
    steps_open().insert(step.to_owned(), song.clone());
    let (watched, name) = (song.clone(), step.to_owned());
    let timeout = std::time::Duration::from_secs_f64(timeout_s.clamp(1.0, 86_400.0));
    let _ = std::thread::Builder::new()
        .name("agent-step-timeout".into())
        .spawn(move || {
            std::thread::sleep(timeout);
            if crate::project_file::group_abandon(&watched, &name) {
                steps_open().remove(&name);
                tracing::warn!(undo.group = %name, "an agent's undo step was never ended: dropped, its edits steps of their own");
            }
        });
    Ok(song)
}

/// End the undo step `step`: its name, when it holds any edit, and how
/// many.
fn step_end(step: &str) -> (Option<String>, usize) {
    let Some(song) = steps_open().remove(step) else {
        return (None, 0);
    };
    match crate::project_file::group_end(&song, step) {
        Some(n) if n > 0 => (Some(step.to_owned()), n),
        _ => (None, 0),
    }
}

/// AGENT_RUN's work.
async fn agent_run(a: Value) -> Result<Vec<u8>, ActionError> {
    let id = a
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(RUN.id, "`id` is missing", RunDoc::SHAPE))?
        .to_owned();
    let inner = match a.get("args") {
        None | Some(Value::Null) => json!({}),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_else(|_| json!(s)),
        Some(v) => v.clone(),
    };
    if let Some(want) = a.get("if_revision").filter(|v| !v.is_null()) {
        let want = want.as_u64().ok_or_else(|| {
            bad(
                RUN.id,
                "`if_revision` is a revision, a whole number",
                RunDoc::SHAPE,
            )
        })?;
        if let Some(stale) = check(want) {
            tracing::warn!(agent.action = %id, agent.since = want, agent.revision = revision(), "an agent's edit was refused: the song moved on");
            return Ok(stale.to_string().into_bytes());
        }
    }
    if crate::actions::list().info(&id).is_none() {
        return Ok(refusal(&id, ActionError::Unknown(id.clone()))
            .to_string()
            .into_bytes());
    }
    let step = a
        .get("step")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let grouped = match step {
        Some(step) if mutating(&id) => {
            let (step, timeout) = (step.to_owned(), STEP_TIMEOUT_S);
            let begun = crate::off_loop::blocking(move || step_begin(&step, timeout))
                .await
                .unwrap_or_else(Err);
            Some(begun.map(|_| ()))
        }
        _ => None,
    };
    let before = revision();
    let args = inner.to_string();
    let out = crate::off_loop::run_by(&id, args.as_bytes(), By::Agent).await;
    let now = revision();
    let answer = match out {
        Ok(bytes) => {
            let answer = serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
            let mut v =
                json!({"ok": true, "answer": answer, "revision": now, "edit": now != before});
            if let (Some(g), Some(o)) = (&grouped, v.as_object_mut()) {
                match g {
                    Ok(()) => o.insert("step".into(), json!(step)),
                    Err(why) => o.insert("step_why".into(), json!(why)),
                };
            }
            v
        }
        Err(e) => refusal(&id, e),
    };
    Ok(answer.to_string().into_bytes())
}

/// `AGENT_RUN`, `AGENT_STEP_BEGIN`, `AGENT_STEP_END` and `SESSION_CHANGES`.
///
/// # Errors
/// When any is already registered.
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    list.register_dynamic_async(
        &RUN,
        Effect::Mutating,
        Some(RunDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(RUN.id, raw, RunDoc::SHAPE)?;
            Ok(Box::pin(agent_run(a)) as architect::action::ActionFuture)
        }),
    )?;
    list.register_dynamic(
        &STEP_BEGIN,
        Effect::Mutating,
        Some(StepDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(STEP_BEGIN.id, raw, StepDoc::SHAPE)?;
            let step = a
                .get("step")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    bad(
                        STEP_BEGIN.id,
                        "`step` is missing (its name)",
                        StepDoc::SHAPE,
                    )
                })?;
            let timeout = a
                .get("timeout_s")
                .and_then(Value::as_f64)
                .unwrap_or(STEP_TIMEOUT_S);
            let v = match step_begin(step, timeout) {
                Ok(_) => json!({"grouped": true, "step": step, "revision": revision()}),
                Err(why) => json!({"grouped": false, "why": why, "revision": revision()}),
            };
            Ok(v.to_string().into_bytes())
        }),
    )?;
    list.register_dynamic(
        &STEP_END,
        Effect::Mutating,
        Some(StepDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(STEP_END.id, raw, StepDoc::SHAPE)?;
            let step = a
                .get("step")
                .and_then(Value::as_str)
                .ok_or_else(|| bad(STEP_END.id, "`step` is missing (its name)", StepDoc::SHAPE))?;
            let (undo, steps) = step_end(step);
            Ok(
                json!({"undo": undo, "steps": steps, "revision": revision()})
                    .to_string()
                    .into_bytes(),
            )
        }),
    )?;
    list.register_dynamic(
        &CHANGES,
        Effect::ReadOnly,
        Some(ChangesDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let a = parse(CHANGES.id, raw, ChangesDoc::SHAPE)?;
            let from = a.get("since").and_then(Value::as_u64).unwrap_or(0);
            let offset = a.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let limit = a
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 500);
            Ok(journal().page(from, offset, limit).to_string().into_bytes())
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
