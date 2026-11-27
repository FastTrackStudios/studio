//! The engine's door (studio#362): the permissions gate every lane an
//! engine serves beyond this user's own socket goes through.
//!
//! An engine serves its whole daw facade. Whoever reaches its `/vox`
//! WebSocket or its iroh endpoint can play, edit and save the owner's
//! songs, so a lane admits only:
//!
//! - **The owner's windows and tools on this machine**, by the engine's
//!   token: a secret minted once and kept in a file only this user can
//!   read (`<config>/fts/engine-token`, mode 0600). A client presents it at
//!   the WebSocket handshake (`Authorization: Bearer`, or the
//!   `vox.bearer.<token>` subprotocol a browser offers). [`local_token`]
//!   is how a client on this machine finds it.
//! - **The owner's devices**, by the endpoint id their iroh connection
//!   proved (`Principal::Host`), listed in `<config>/fts/engine-peers.json`
//!   (a JSON list of endpoint ids). Nothing the caller says puts it there.
//!
//! Everyone else is refused. A WebSocket without the token gets `401`
//! before it upgrades. An iroh peer not on the list gets a lane where
//! every call is refused by architect's [`PermissionsGate`] (fail-closed:
//! no service is permitted).
//!
//! The engine has one owner and serves them all of it, so the decision is
//! *who*, not *which method*: a connection is admitted once, by the
//! credential its transport carried, and every call on it runs as that
//! principal (`architect::permissions_gate::caller`). A room's lanes are
//! not served here: they have their own door, with its passes
//! (`room::gate`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use architect::LayerRouter;
use architect::host::{EngineHost, RequestHead};
use architect::http::axum::http::StatusCode;
use architect::iroh_link::iroh;
use architect::permissions_gate::{PermissionedRouter, PermissionsGate, UnlistedPolicy};
use architect_permissions::{
    Action, BoxIdentityFuture, Decision, IdentityResolver, PermissionEngine, Principal, Resource,
};

/// Who a call presenting the engine's token is: its owner, on this
/// machine.
pub const OWNER: &str = "engine-owner";

/// The engine's token, under `<config>/fts/`.
const TOKEN_FILE: &str = "engine-token";

/// The endpoints the engine admits over iroh, under `<config>/fts/`.
const PEERS_FILE: &str = "engine-peers.json";

/// What a client on this machine presents instead of the token file
/// (a test, a tool run as another user that was handed the token).
pub const TOKEN_ENV: &str = "FTS_ENGINE_TOKEN";

/// The engine's door: its token and the devices it admits.
#[derive(Clone)]
pub struct EngineGate {
    inner: Arc<Inner>,
}

struct Inner {
    token: String,
    peers: RwLock<BTreeSet<String>>,
}

impl std::fmt::Debug for EngineGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is a credential: never printed.
        f.debug_struct("EngineGate")
            .field("peers", &self.peers().len())
            .finish_non_exhaustive()
    }
}

/// A lane was refused: the caller presented no credential of this
/// engine's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

impl EngineGate {
    /// A door that opens to `token` and to the iroh endpoints `peers`.
    #[must_use]
    pub fn new(token: impl Into<String>, peers: impl IntoIterator<Item = String>) -> Self {
        Self {
            inner: Arc::new(Inner {
                token: token.into(),
                peers: RwLock::new(peers.into_iter().collect()),
            }),
        }
    }

    /// The engine's door as this user's files say: the token (minted on
    /// the first start, readable by this user only) and the devices in
    /// `engine-peers.json`.
    ///
    /// # Errors
    ///
    /// No config directory (`HOME` unset), or the token could not be read
    /// or written.
    pub fn load() -> std::io::Result<Self> {
        let dir = config_dir().ok_or_else(|| std::io::Error::other("no config directory"))?;
        Self::load_from(&dir)
    }

    /// [`load`](Self::load) from `dir` instead of `<config>/fts`.
    ///
    /// # Errors
    ///
    /// As [`load`](Self::load).
    pub fn load_from(dir: &Path) -> std::io::Result<Self> {
        let token = load_or_mint_token(&dir.join(TOKEN_FILE))?;
        let peers = std::fs::read_to_string(dir.join(PEERS_FILE))
            .ok()
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .unwrap_or_default();
        Ok(Self::new(token, peers))
    }

    /// Admit the device whose iroh endpoint is `endpoint` from now on (on
    /// this door, not in the file).
    pub fn admit(&self, endpoint: impl Into<String>) {
        self.inner
            .peers
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(endpoint.into());
    }

    /// Whether the device whose iroh endpoint is `endpoint` is admitted.
    #[must_use]
    pub fn admits(&self, endpoint: &str) -> bool {
        self.inner
            .peers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(endpoint)
    }

    fn peers(&self) -> BTreeSet<String> {
        self.inner
            .peers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether `presented` is the engine's token. The comparison takes as
    /// long whichever byte differs.
    fn is_token(&self, presented: &str) -> bool {
        let (ours, theirs) = (self.inner.token.as_bytes(), presented.as_bytes());
        !ours.is_empty()
            && ours.len() == theirs.len()
            && ours
                .iter()
                .zip(theirs)
                .fold(0u8, |differ, (a, b)| differ | (a ^ b))
                == 0
    }

    /// Whether `who` is someone this engine serves.
    fn serves(&self, who: &Principal) -> bool {
        match who {
            Principal::Service { name } => name == OWNER,
            Principal::Host { endpoint } => self.admits(endpoint),
            _ => false,
        }
    }

    /// The gate for one connection: every call through when `admitted`,
    /// none otherwise. Its engine and resolver are this door's, so
    /// `caller()` names who the call is in either case.
    fn gate(&self, admitted: bool) -> PermissionsGate {
        PermissionsGate::new(
            Arc::new(Rules { door: self.clone() }),
            Arc::new(ByToken { door: self.clone() }),
        )
        .unlisted(if admitted {
            UnlistedPolicy::Allow
        } else {
            UnlistedPolicy::Deny
        })
    }

    /// `router` for one WebSocket connection that presented `bearer` at
    /// its handshake.
    ///
    /// # Errors
    ///
    /// [`Refused`] when `bearer` is not the engine's token.
    pub fn lane(
        &self,
        router: LayerRouter,
        bearer: Option<String>,
    ) -> Result<PermissionedRouter, Refused> {
        let who = bearer
            .as_deref()
            .map_or(Principal::Anonymous, |b| self.resolve(b));
        if !self.serves(&who) {
            tracing::warn!(
                engine.refused = %who.describe(),
                "engine: a WebSocket without the engine's token was refused"
            );
            return Err(Refused);
        }
        Ok(PermissionsGate::wrap_shared_with_bearer(
            Arc::new(self.gate(true)),
            router,
            bearer,
        ))
    }

    /// `router` for one iroh connection whose handshake proved `remote`.
    /// A device the engine does not admit gets a lane that refuses every
    /// call.
    #[must_use]
    pub fn peer(&self, router: LayerRouter, remote: &str) -> PermissionedRouter {
        let who = Principal::Host {
            endpoint: remote.to_owned(),
        };
        let admitted = self.serves(&who);
        if !admitted {
            tracing::warn!(
                engine.refused = %who.describe(),
                "engine: an iroh peer the engine does not admit was refused"
            );
        }
        PermissionsGate::wrap_shared_with_proved_caller(Arc::new(self.gate(admitted)), router, who)
    }

    fn resolve(&self, bearer: &str) -> Principal {
        if self.is_token(bearer) {
            Principal::Service {
                name: OWNER.to_owned(),
            }
        } else {
            Principal::Anonymous
        }
    }

    /// A host serving `router` on `addr` behind this door: `/vox` opens
    /// only to the token ([`lane`](Self::lane)), and every other lane the
    /// host owns refuses every call. Serve iroh with
    /// [`serve_iroh`](Self::serve_iroh), not the host's own iroh lane.
    #[must_use]
    pub fn host(&self, router: LayerRouter, addr: impl Into<String>) -> EngineHost {
        let door = self.clone();
        let lane = router.clone();
        EngineHost::new(router, self.gate(false), addr).vox_at_permissioned(
            "/vox",
            move |head: RequestHead| {
                let opened = door.lane(lane.clone(), head.bearer().map(str::to_owned));
                async move { opened.map_err(|Refused| StatusCode::UNAUTHORIZED) }
            },
        )
    }

    /// Serve `router` on `endpoint` behind this door, each peer as the
    /// endpoint its handshake proved ([`peer`](Self::peer)), until the
    /// endpoint closes.
    pub async fn serve_iroh(&self, endpoint: &iroh::Endpoint, router: LayerRouter) {
        let door = self.clone();
        architect::iroh_link::serve_peers(endpoint, move |remote| {
            door.peer(router.clone(), &remote.to_string())
        })
        .await;
    }
}

/// The door's rules: the owner and the admitted devices may do anything,
/// nobody else anything.
struct Rules {
    door: EngineGate,
}

impl PermissionEngine for Rules {
    fn check(&self, who: &Principal, _what: &Resource, _action: &Action) -> Decision {
        if self.door.serves(who) {
            Decision::Allow
        } else {
            Decision::deny(format!(
                "{} is not this engine's owner or one of its devices",
                who.describe()
            ))
        }
    }
}

/// Who a bearer is: the owner when it is the engine's token, nobody
/// otherwise.
struct ByToken {
    door: EngineGate,
}

impl IdentityResolver for ByToken {
    fn resolve<'a>(&'a self, token: Option<&'a str>) -> BoxIdentityFuture<'a> {
        let who = token.map_or(Principal::Anonymous, |t| self.door.resolve(t));
        Box::pin(async move { who })
    }
}

/// The token a client on this machine presents to the engine:
/// `FTS_ENGINE_TOKEN`, else the engine's token file. `None` before any
/// engine has started for this user.
#[must_use]
pub fn local_token() -> Option<String> {
    if let Some(token) = std::env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty()) {
        return Some(token);
    }
    let path = config_dir()?.join(TOKEN_FILE);
    std::fs::read_to_string(path)
        .ok()
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

/// `<$XDG_CONFIG_HOME or ~/.config>/fts`.
fn config_dir() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("fts"))
}

/// The token at `path`, or a new one written there, readable by this user
/// only.
fn load_or_mint_token(path: &Path) -> std::io::Result<String> {
    if let Ok(token) = std::fs::read_to_string(path) {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_owned());
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    let token = format!("{}{}", a.simple(), b.simple());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(path)?, token.as_bytes())?;
    Ok(token)
}
