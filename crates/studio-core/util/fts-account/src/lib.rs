//! The FastTrackStudio account on a device.
//!
//! One person, one account, every device: an app signs in from another
//! device (`auth_client::device`, RFC 8628) — it shows a code and a link,
//! its person approves it in a browser where they are signed in, and the
//! session it gets is kept so the next launch is signed in already.
//!
//! - [`flow`]: the sign-in itself ([`flow::start`], [`flow::finish`]), over
//!   a [`flow::Transport`] and a [`flow::Clock`] so it is tested without a
//!   server or a real wait. Polling carries on through network errors until
//!   the code's wall-clock expiry (design spec STA-379 §8.1).
//! - [`Store`]: the kept session. On a desktop every FTS app (Signal,
//!   Session) reads and writes **one** file,
//!   `~/.config/fts/account/session.json`, so signing in to one signs in the
//!   other and signing out of one signs out both.
//! - [`copy`]: the words every state shows, for both apps, so they can't
//!   drift.
//! - `ui` (feature `ui`): the one sign-in UI both apps mount.
//!
//! The account server is the one the Task server trusts (`central_auth` in
//! its `/.well-known/task-server.json`); the token it gives is the session
//! every org on that server takes.

pub mod copy;
#[cfg(not(target_arch = "wasm32"))]
pub mod flow;
#[cfg(all(feature = "ui", not(target_arch = "wasm32")))]
pub mod ui;

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub use auth_client::StoredSession;
#[cfg(not(target_arch = "wasm32"))]
use auth_client::TokenStore as _;
use auth_client::device::{DeviceCode, DeviceError};

/// The Task server an app signs in to unless told another
/// (`FTS_TASK_SERVER`, as a `wss://` or `https://` base).
pub const SERVER: &str = "https://task.fasttrackstudio.app";

/// The shortest life a code is given on screen, whatever the server says.
pub const MIN_CODE_LIFE: Duration = Duration::from_secs(60);

/// Which app is signing in: what the approval page calls it, and the name
/// in the copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum App {
    Signal,
    Session,
}

impl App {
    /// The `client_id` the account server shows its person on the approval
    /// page and in their session list.
    #[must_use]
    pub const fn client_id(self) -> &'static str {
        match self {
            Self::Signal => "signal-app",
            Self::Session => "session-app",
        }
    }

    /// The app's name, as the copy says it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Signal => "Signal",
            Self::Session => "Session",
        }
    }

    /// The other desktop app sharing the session file.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Signal => Self::Session,
            Self::Session => Self::Signal,
        }
    }
}

/// The server an app signs in to: `FTS_TASK_SERVER` or [`SERVER`], as an
/// `https://` base without a trailing slash.
#[must_use]
pub fn server() -> String {
    normalize_server(
        std::env::var("FTS_TASK_SERVER")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .as_deref()
            .unwrap_or(SERVER),
    )
}

/// `wss://…/` → `https://…`.
#[must_use]
pub fn normalize_server(raw: &str) -> String {
    raw.trim()
        .trim_end_matches('/')
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
}

/// A signed-in person on a Task server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// `https://…` — the server, without the per-org path.
    pub server: String,
    pub session: StoredSession,
}

impl Account {
    /// The person, as a name to show and to be known by in a set: the
    /// email's local part, or "Me".
    #[must_use]
    pub fn name(&self) -> String {
        self.session
            .email
            .as_deref()
            .and_then(|e| e.split('@').next())
            .filter(|n| !n.is_empty())
            .unwrap_or("Me")
            .to_owned()
    }

    /// The email, or empty.
    #[must_use]
    pub fn email(&self) -> &str {
        self.session.email.as_deref().unwrap_or_default()
    }

    /// Up to two initials for the avatar (`jules.marlowe` → `JM`).
    #[must_use]
    pub fn initials(&self) -> String {
        let name = self.name();
        let mut parts = name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.chars().next());
        let first = parts.next();
        let second = parts.next();
        match (first, second) {
            (Some(a), Some(b)) => [a, b].iter().flat_map(|c| c.to_uppercase()).collect(),
            (Some(_), None) => name.chars().take(2).flat_map(char::to_uppercase).collect(),
            _ => "?".to_owned(),
        }
    }

    /// Which of five avatar tints this person gets: stable per user id.
    #[must_use]
    pub fn hue(&self) -> usize {
        let key = self
            .session
            .user_id
            .as_deref()
            .unwrap_or_else(|| self.email());
        key.bytes().fold(0usize, |h, b| {
            h.wrapping_mul(31).wrapping_add(usize::from(b))
        }) % 5
    }

    /// Whether this is the default server (the server is only shown when
    /// it is not).
    #[must_use]
    pub fn on_default_server(&self) -> bool {
        self.server == SERVER
    }
}

/// A sign-in started: what to show, and where to poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    /// The account server (`central_auth`), without a trailing slash.
    pub issuer: String,
    pub code: DeviceCode,
    /// When the code runs out, on the wall clock: an `Instant` does not
    /// advance while an iPad app is suspended, so a code could otherwise
    /// outlive its server-side life on screen.
    pub expires_at: SystemTime,
}

impl Started {
    /// `code`, from `issuer`, started at `now`.
    #[must_use]
    pub fn new(issuer: String, code: DeviceCode, now: SystemTime) -> Self {
        let life = Duration::from_secs(code.expires_in_seconds).max(MIN_CODE_LIFE);
        let expires_at = now.checked_add(life).unwrap_or(now);
        Self {
            issuer,
            code,
            expires_at,
        }
    }

    /// The link to open to approve it, the code filled in.
    #[must_use]
    pub fn link(&self) -> String {
        format!("{}{}", self.issuer, self.code.verification_uri_complete)
    }

    /// Where to type the code on another device, without the scheme
    /// (`auth.fasttrackstudio.app/auth/device`).
    #[must_use]
    pub fn other_device(&self) -> String {
        let host = self
            .issuer
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        format!("{host}{}", self.code.verification_uri)
    }

    /// The code's characters in groups, split where the server put a `-`
    /// (`QK72-M9X4` → `["QK72", "M9X4"]`); a code with no `-` is one group.
    #[must_use]
    pub fn groups(&self) -> Vec<String> {
        code_groups(&self.code.user_code)
    }

    /// How long it lasts in whole minutes (for "codes last {N} minutes").
    #[must_use]
    pub const fn minutes(&self) -> u64 {
        let m = self.code.expires_in_seconds / 60;
        if m == 0 { 1 } else { m }
    }

    /// Time left at `now`, zero once it has run out.
    #[must_use]
    pub fn left(&self, now: SystemTime) -> Duration {
        self.expires_at.duration_since(now).unwrap_or_default()
    }
}

/// `user_code` in groups split at `-` (spaces ignored).
#[must_use]
pub fn code_groups(user_code: &str) -> Vec<String> {
    user_code
        .split('-')
        .map(|g| g.chars().filter(|c| !c.is_whitespace()).collect::<String>())
        .filter(|g| !g.is_empty())
        .collect()
}

/// Why a sign-in could not go on: what the UI shows (design spec §2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The account server could not be reached to start.
    Offline,
    /// The Task server names no account server.
    NoAccountServer,
    /// The code ran out before it was approved.
    Expired,
    /// The person refused it in the browser.
    Declined,
    /// The server answered what this cannot read; its words.
    Unreadable(String),
    /// The server refused for another reason; its words.
    Other(String),
}

impl Failure {
    /// The server's words for the "Details" line, cut at 120 characters.
    #[must_use]
    pub fn brief(&self) -> Option<String> {
        match self {
            Self::Unreadable(s) | Self::Other(s) => Some(cut(s, 120)),
            _ => None,
        }
    }
}

impl From<DeviceError> for Failure {
    fn from(e: DeviceError) -> Self {
        match e {
            DeviceError::Denied => Self::Declined,
            DeviceError::Expired => Self::Expired,
            DeviceError::Unreadable(s) => Self::Unreadable(s),
            DeviceError::Failed(s) => Self::Other(s),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline => f.write_str("the account server could not be reached"),
            Self::NoAccountServer => f.write_str("the server names no account server"),
            Self::Expired => f.write_str("the code expired before it was approved"),
            Self::Declined => f.write_str("the sign-in was refused"),
            Self::Unreadable(s) => {
                write!(f, "the account server answered what this cannot read: {s}")
            }
            Self::Other(s) => write!(f, "sign-in failed: {s}"),
        }
    }
}

impl std::error::Error for Failure {}

/// `s` cut to `max` characters with "…" (no `text-overflow` in Blitz).
#[must_use]
pub fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

// ── the kept session ────────────────────────────────────────────────────

/// Where the one desktop session lives:
/// `FTS_ACCOUNT_FILE`, else `~/.config/fts/account/session.json` (the same
/// path on Linux and macOS, so both apps agree), else — iOS, where each
/// app is its own sandbox — the app's data dir.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn session_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("FTS_ACCOUNT_FILE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if cfg!(target_os = "ios") {
        return Some(dirs::data_dir()?.join("fts/account/session.json"));
    }
    Some(dirs::home_dir()?.join(".config/fts/account/session.json"))
}

/// Session's own file before the shared one (`<data>/Session/task-session.json`):
/// read once, so a person signed in to Session stays signed in.
#[cfg(not(target_arch = "wasm32"))]
fn legacy_session_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("task-session.json"))
}

/// The kept session: one file.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct Store {
    file: auth_client::FileTokenStore,
    legacy: Option<PathBuf>,
}

/// What [`Store::stamp`] reads: changes whenever the file is written or
/// removed, without parsing it.
pub type Stamp = Option<(SystemTime, u64)>;

#[cfg(not(target_arch = "wasm32"))]
impl Store {
    /// The shared desktop store ([`session_path`]), reading Session's old
    /// file the first time when there is no shared one yet.
    #[must_use]
    pub fn shared() -> Option<Self> {
        Some(Self {
            file: auth_client::FileTokenStore::new(session_path()?),
            legacy: legacy_session_path(),
        })
    }

    /// A store at `path` and nothing else (tests, tools).
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            file: auth_client::FileTokenStore::new(path),
            legacy: None,
        }
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The person signed in, if a session is kept, on `server`.
    ///
    /// # Errors
    ///
    /// The file is there but could not be read.
    pub fn load(&self, server: &str) -> Result<Option<Account>, auth_client::TokenStoreError> {
        let mut session = self.file.load()?;
        if session.is_none()
            && let Some(legacy) = &self.legacy
        {
            session = auth_client::FileTokenStore::new(legacy)
                .load()
                .ok()
                .flatten();
            if let Some(s) = &session {
                // Moved once: the shared file is the one from now on.
                if let Err(e) = self.file.save(s) {
                    tracing::warn!(error = %e, "account: the old session could not be moved");
                }
                let _ = auth_client::FileTokenStore::new(legacy).clear();
            }
        }
        Ok(session.map(|session| Account {
            server: server.to_owned(),
            session,
        }))
    }

    /// Keep `session`, replacing whatever was kept.
    ///
    /// # Errors
    ///
    /// It could not be written.
    pub fn save(&self, session: &StoredSession) -> Result<(), auth_client::TokenStoreError> {
        self.file.save(session)
    }

    /// Forget the kept session (both apps are signed out).
    ///
    /// # Errors
    ///
    /// It could not be removed.
    pub fn clear(&self) -> Result<(), auth_client::TokenStoreError> {
        if let Some(legacy) = &self.legacy {
            let _ = auth_client::FileTokenStore::new(legacy).clear();
        }
        self.file.clear()
    }

    /// The file's modification time and length, `None` when there is none:
    /// a stat, not a parse, for the 5 s "did the other app change it" check.
    #[must_use]
    pub fn stamp(&self) -> Stamp {
        let meta = std::fs::metadata(self.path()).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }
}

/// The account's own runtime: the sign-in's HTTP (reqwest needs tokio)
/// runs here, whatever executor the app's UI is on. One small worker.
#[cfg(not(target_arch = "wasm32"))]
pub fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    static RUNTIME: std::sync::OnceLock<Option<tokio::runtime::Runtime>> =
        std::sync::OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("fts-account")
                .enable_all()
                .build()
                .inspect_err(|e| tracing::warn!(error = %e, "account: no runtime"))
                .ok()
        })
        .as_ref()
}

/// The person signed in on this device, if a session is kept.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn signed_in() -> Option<Account> {
    Store::shared()?
        .load(&server())
        .inspect_err(|e| tracing::warn!(error = %e, "account: the kept session could not be read"))
        .ok()?
}

/// Forget the kept session.
#[cfg(not(target_arch = "wasm32"))]
pub fn sign_out() {
    if let Some(Err(e)) = Store::shared().map(|s| s.clear()) {
        tracing::warn!(error = %e, "account: the kept session could not be cleared");
    }
}
