//! Where an offline render runs: in the engine's own process, or in the
//! render worker. Pure decisions, tested without a process.
//!
//! The rule (`Mode::Auto`, the default): an agent's render runs in the
//! worker, so a plugin that crashes takes only the worker down; the app's
//! own (a key, a menu, a view) runs in process, as before. `Mode::Worker`
//! and `Mode::Process` send every render one way. `FTS_RENDER_WORKER`
//! (`auto`, `worker`, `process`; `off` is `process`) overrides the kept
//! setting for the life of the process.

use std::path::PathBuf;

use architect::action::{ActionCaller, InvocationSource};
use serde_json::Value;

/// The environment override of the kept setting.
pub const ENV: &str = "FTS_RENDER_WORKER";

/// The argument a caller names its origin with (`"agent"` or `"app"`),
/// taken off the arguments before the action reads them. Every hop an
/// agent's call takes (Studio's control socket, its forwarder, the daw
/// extension's `DAW_RUN`) passes arguments through untouched, so this is
/// the one signal that survives all of them.
pub const ORIGIN_ARG: &str = "_origin";

/// A per-call override of where it runs (`"worker"` or `"process"`),
/// taken off the arguments as [`ORIGIN_ARG`] is.
pub const WHERE_ARG: &str = "_render_in";

/// The actions that render offline on copies of the song's plugins: the
/// ones a crashing plugin can take the process down in.
pub const RENDERS: &[&str] = &[
    "LISTEN",
    "LISTEN_BLOCK",
    "LISTEN_AB",
    "LISTEN_RENDER",
    "MIX_MEASURE",
    "MIX_RENDER",
];

/// The setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// An agent's renders in the worker, the app's in process.
    Auto,
    /// Every render in the worker.
    Worker,
    /// Every render in process (the worker never starts).
    Process,
}

impl Mode {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Worker => "worker",
            Self::Process => "process",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(Self::Auto),
            "worker" | "on" | "1" => Some(Self::Worker),
            "process" | "off" | "0" | "in-process" => Some(Self::Process),
            _ => None,
        }
    }
}

/// Who asked for a render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The app itself: a key, a menu, a view, its own code.
    App,
    /// A model or an agent: MCP, the control socket, a script it sent.
    Agent,
}

impl Origin {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Agent => "agent",
        }
    }
}

/// Where a render runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    Process,
    Worker,
}

impl Where {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Worker => "worker",
        }
    }
}

/// A call's arguments with the routing words taken off, and what they said.
#[derive(Debug, PartialEq)]
pub struct Routed {
    /// The arguments the action reads, byte for byte as given when there
    /// was nothing to take off (or they were not a JSON object).
    pub args: Vec<u8>,
    pub origin: Option<Origin>,
    pub asked: Option<Where>,
}

/// Takes [`ORIGIN_ARG`] and [`WHERE_ARG`] off `raw`.
#[must_use]
pub fn strip(raw: &[u8]) -> Routed {
    let untouched = || Routed {
        args: raw.to_vec(),
        origin: None,
        asked: None,
    };
    if !raw.windows(2).any(|w| w == b"\"_") {
        return untouched();
    }
    let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(raw) else {
        return untouched();
    };
    let origin = map.remove(ORIGIN_ARG).and_then(|v| match v.as_str() {
        Some("agent") => Some(Origin::Agent),
        Some("app") => Some(Origin::App),
        _ => None,
    });
    let asked = map.remove(WHERE_ARG).and_then(|v| match v.as_str() {
        Some("worker") => Some(Where::Worker),
        Some("process") => Some(Where::Process),
        _ => None,
    });
    Routed {
        args: Value::Object(map).to_string().into_bytes(),
        origin,
        asked,
    }
}

/// Who `caller` is: an agent when it carries a principal (the engine's
/// MCP door runs every call as one) or says it was invoked by one; the
/// app otherwise.
#[must_use]
pub fn origin_of(caller: &ActionCaller) -> Origin {
    let by_agent = caller
        .invocation()
        .is_some_and(|i| i.source == InvocationSource::Agent);
    if by_agent || caller.principal().is_some() {
        Origin::Agent
    } else {
        Origin::App
    }
}

/// Where action `id`, asked by `origin` (or as `asked` per call), runs
/// under `mode`. Only [`RENDERS`] ever leave the process; `LISTEN_AB`
/// with `keep: true` stays, since its change must land in the live song.
#[must_use]
pub fn decide(mode: Mode, origin: Origin, id: &str, asked: Option<Where>, args: &[u8]) -> Where {
    if !RENDERS.contains(&id) || keeps_change(id, args) {
        return Where::Process;
    }
    if let Some(asked) = asked {
        return asked;
    }
    match (mode, origin) {
        (Mode::Process, _) | (Mode::Auto, Origin::App) => Where::Process,
        (Mode::Worker, _) | (Mode::Auto, Origin::Agent) => Where::Worker,
    }
}

/// Whether call `id` with `args` changes the live song for good
/// (`LISTEN_AB` with `keep: true`).
fn keeps_change(id: &str, args: &[u8]) -> bool {
    id == "LISTEN_AB"
        && serde_json::from_slice::<Value>(args)
            .ok()
            .and_then(|v| v.get("keep").and_then(Value::as_bool))
            == Some(true)
}

/// Where the kept setting lives.
fn setting_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("render-worker.json"))
}

/// The mode in force: the environment's, else the kept setting, else
/// [`Mode::Auto`].
#[must_use]
pub fn mode() -> Mode {
    if let Some(m) = std::env::var(ENV).ok().as_deref().and_then(Mode::parse) {
        return m;
    }
    kept().unwrap_or(Mode::Auto)
}

/// Whether the environment decides (a set mode is then not in force).
#[must_use]
pub fn overridden() -> bool {
    std::env::var(ENV)
        .ok()
        .as_deref()
        .and_then(Mode::parse)
        .is_some()
}

/// The kept setting, when there is one.
#[must_use]
pub fn kept() -> Option<Mode> {
    let text = std::fs::read_to_string(setting_path()?).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    Mode::parse(v.get("mode")?.as_str()?)
}

/// Keep `mode` across restarts.
///
/// # Errors
/// The settings folder could not be written.
pub fn keep(mode: Mode) -> Result<(), String> {
    let path = setting_path().ok_or("no settings folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::json!({ "mode": mode.name() }).to_string(),
    )
    .map_err(|e| format!("{}: {e}", path.display()))
}
