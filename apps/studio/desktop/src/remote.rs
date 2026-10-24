//! Shared "dial the engine" plumbing for every remote surface.
//!
//! The signal rig views (rig_view.rs) and the browser session player
//! (session_remote_view.rs) all reach `fasttrackstudio --engine` the same
//! way: one shared engine target (local WebSocket by default, a saved iroh
//! endpoint id for p2p) and **one connection to it**, taken from
//! [`architect::connect::pool`]. Every typed vox client is a cheap view over
//! that shared caller: the engine serves one `LayerRouter` at `/vox` (and
//! over iroh) that dispatches every service by method id, so a page that
//! opens a dozen clients opens one socket, not a dozen. This module owns
//! that plumbing so every view connects identically.

use architect::connect::{ConnectError, Endpoint, RootLane, pool};
use architect::iroh_link::iroh;

/// Where the engine core lives. Native: `SIGNAL_ENGINE_URL` (or legacy
/// `RIGD_URL`) at runtime, else the local default. Web: same-origin —
/// the engine that served this page also serves /vox — falling back to
/// the local default under a dev server ([`page_engine_url`]).
///
/// Never Task: the page's `/config.json` (`task_url`, studio-web) is where
/// Session syncs, not a rig engine. On studio-web the browser rigs run in
/// the tab; reaching a remote engine through Task is later work (#85, #89).
#[cfg(not(target_arch = "wasm32"))]
pub fn server_url() -> String {
    std::env::var("SIGNAL_ENGINE_URL")
        .or_else(|_| std::env::var("RIGD_URL"))
        .ok()
        // No env on iOS — the phone saves the engine URL from its connect UI.
        .or_else(|| utils::prefs::get_raw("signal-engine-ws-url"))
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".to_string())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn server_url() -> String {
    if let Some(saved) = utils::prefs::get_raw("signal-engine-ws-url") {
        return saved;
    }
    web_sys::window()
        .and_then(|w| {
            let loc = w.location();
            Some(page_engine_url(
                &loc.protocol().ok()?,
                &loc.host().ok()?,
                &loc.hostname().ok()?,
            ))
        })
        .unwrap_or_else(|| "ws://127.0.0.1:4040/vox".to_string())
}

/// The engine a page dials, from where the page is (`location.protocol`,
/// `.host`, `.hostname`): its own origin's `/vox`.
///
/// A dev server (`dx serve`) does NOT serve `/vox`; those KNOWN dev ports
/// point at the local engine so hot-reload iteration connects without a
/// manual `signal-engine-ws-url` override. Everything else — including an
/// engine bound to a scratch port via SIGNAL_ENGINE_ADDR (the e2e suite,
/// side-by-side engines) — is served BY the engine, so it stays
/// same-origin. (The old rule, "any local port that isn't 4040", silently
/// pointed the e2e's page at the LIVE :4040 engine — W7 chased phantom
/// schema-compat errors that were really an old binary.)
#[cfg(any(target_arch = "wasm32", test))]
fn page_engine_url(protocol: &str, host: &str, hostname: &str) -> String {
    let scheme = if protocol == "https:" { "wss" } else { "ws" };
    let is_local = hostname == "localhost" || hostname == "127.0.0.1";
    let dx_dev_port = host.ends_with(":8080") || host.ends_with(":8087");
    if is_local && dx_dev_port {
        return "ws://127.0.0.1:4040/vox".to_string();
    }
    format!("{scheme}://{host}/vox")
}

// ── Engine target (local ws vs remote iroh) ─────────────────────────────────

/// TEMPORARY default pack host for the phone: the studio engine's iroh
/// endpoint id (durable identity at ~/.local/share/fts-pack-host on the
/// studio machine), so a fresh install can list + download packs
/// immediately with zero setup. Remove once a proper host-pairing UX
/// exists; a ws URL or iroh id saved from the connect UI overrides it.
#[cfg(target_os = "ios")]
const DEFAULT_ENGINE_IROH_ID: &str =
    "9e16e3e074f7f3a94c1d9a95adcab1963c399e967719b7e632069cd75676dd70";

/// A saved remote engine: `SIGNAL_ENGINE_IROH_ID` at runtime (native),
/// else the id stored from the connect screen. When set, remotes dial
/// p2p over iroh instead of the WebSocket.
pub fn engine_iroh_id() -> Option<iroh::EndpointId> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(raw) = std::env::var("SIGNAL_ENGINE_IROH_ID") {
        return raw.trim().parse().ok();
    }
    if let Some(saved) = utils::prefs::get_raw("signal-engine-iroh-id") {
        return saved.parse().ok();
    }
    // iPhone with nothing configured: fall back to the studio pack host
    // — but never shadow an explicitly-saved or launch-given ws URL.
    #[cfg(target_os = "ios")]
    if utils::prefs::get_raw("signal-engine-ws-url").is_none()
        && std::env::var("SIGNAL_ENGINE_URL").is_err()
    {
        return DEFAULT_ENGINE_IROH_ID.parse().ok();
    }
    None
}

// Only the signal rig views expose a connect form today; a session-only
// build keeps the saved-id read path but never writes one.
#[cfg_attr(not(feature = "signal"), allow(dead_code))]
pub fn store_engine_iroh_id(id: Option<&str>) {
    match id {
        Some(id) => utils::prefs::set_raw("signal-engine-iroh-id", id.trim()),
        None => utils::prefs::remove("signal-engine-iroh-id"),
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum EngineTarget {
    Ws(String),
    Iroh(iroh::EndpointId),
}

impl EngineTarget {
    pub(crate) fn current() -> Self {
        match engine_iroh_id() {
            Some(id) => Self::Iroh(id),
            None => Self::Ws(server_url()),
        }
    }

    pub(crate) fn label(&self) -> String {
        match self {
            Self::Ws(url) => url.clone(),
            Self::Iroh(id) => format!("iroh {id}"),
        }
    }

    /// The pool key for this target. Anonymous: the engine reads no
    /// credential. iroh peers are keyed `iroh://<endpoint-id>`, the
    /// convention `architect::connect` documents for non-WebSocket dials.
    fn endpoint(&self) -> Endpoint {
        match self {
            Self::Ws(url) => Endpoint::url(url.clone()),
            Self::Iroh(id) => Endpoint::url(format!("iroh://{id}")),
        }
    }
}

/// This device's iroh secret key — a stable identity per install.
/// Native keeps it at ~/.config/fts/iroh.key; the browser keeps it in
/// localStorage (hex).
#[cfg(not(target_arch = "wasm32"))]
fn device_secret_key() -> Option<iroh::SecretKey> {
    // Honor XDG_CONFIG_HOME — on iOS the app roots it under
    // Documents/FastTrackStudio (the container's ~/.config isn't
    // writable, so the HOME path would fail to create the key and no
    // iroh endpoint could ever bind).
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(xdg) => std::path::PathBuf::from(xdg),
        None => std::path::Path::new(&std::env::var_os("HOME")?).join(".config"),
    };
    let key_path = base.join("fts").join("iroh.key");
    architect::iroh_link::load_or_create_secret_key(&key_path)
        .map_err(|e| tracing::error!("iroh key {key_path:?}: {e}"))
        .ok()
}

#[cfg(target_arch = "wasm32")]
fn device_secret_key() -> Option<iroh::SecretKey> {
    if let Some(hex) = utils::prefs::get_raw("iroh-key") {
        if let Ok(key) = architect::iroh_link::secret_key_from_hex(&hex) {
            return Some(key);
        }
    }
    let key = iroh::SecretKey::generate();
    utils::prefs::set_raw("iroh-key", &architect::iroh_link::secret_key_to_hex(&key));
    Some(key)
}

/// The app's own iroh endpoint — one per process. A persistent device
/// key is preferred (stable identity), but dialing needs none — if the
/// key file can't be created (sandboxed FS surprises) an ephemeral key
/// keeps p2p working for this launch instead of failing outright.
async fn app_endpoint() -> Result<iroh::Endpoint, String> {
    static CELL: std::sync::OnceLock<iroh::Endpoint> = std::sync::OnceLock::new();
    if let Some(ep) = CELL.get() {
        return Ok(ep.clone());
    }
    let key = device_secret_key().unwrap_or_else(|| {
        tracing::warn!("no persistent iroh key — using an ephemeral one for this launch");
        iroh::SecretKey::generate()
    });
    let ep = architect::iroh_link::bind_endpoint(key)
        .await
        .map_err(|e| format!("iroh bind: {e}"))?;
    Ok(CELL.get_or_init(|| ep).clone())
}

/// A typed client for `target`, over the pooled connection.
pub async fn establish<C: vox_core::FromVoxLane>(target: &EngineTarget) -> Option<C> {
    establish_verbose(target)
        .await
        .map_err(|e| tracing::debug!("establish {}: {e}", target.label()))
        .ok()
}

/// [`establish`] with the failure reason kept — surfaces (e.g. in the
/// phone's pack Library note) instead of vanishing into a debug log.
///
/// The client is a view over the shared caller: the first call per target
/// dials (concurrent callers await that same dial), later calls reuse it,
/// and a dead connection is evicted and re-dialled on the next call.
pub async fn establish_verbose<C: vox_core::FromVoxLane>(
    target: &EngineTarget,
) -> Result<C, String> {
    let caller = shared_caller(target).await.map_err(|e| e.to_string())?;
    // The pool keeps the connection handle alive; the view needs none.
    Ok(C::from_vox_lane(caller, None))
}

/// The pooled caller for `target`: WebSocket through the pool's own dial,
/// iroh through [`dial_iroh`] with the same cache, single-flight and
/// liveness check.
async fn shared_caller(target: &EngineTarget) -> Result<vox_core::Caller, ConnectError> {
    let endpoint = target.endpoint();
    match target {
        EngineTarget::Ws(_) => pool().caller(&endpoint).await,
        EngineTarget::Iroh(id) => {
            let id = *id;
            pool()
                .caller_with(&endpoint, move |endpoint| dial_iroh(endpoint, id))
                .await
        }
    }
}

/// Dial the engine's iroh endpoint and complete the root vox handshake.
async fn dial_iroh(endpoint: Endpoint, id: iroh::EndpointId) -> Result<RootLane, ConnectError> {
    let dial_failed = |reason: String| ConnectError::Dial {
        endpoint: endpoint.to_string(),
        reason,
    };
    let ep = app_endpoint().await.map_err(dial_failed)?;
    let link = architect::iroh_link::connect(&ep, id)
        .await
        .map_err(|e| dial_failed(format!("iroh connect: {e}")))?;
    vox_core::initiator_on(link)
        .establish::<RootLane>()
        .await
        .map_err(|e| ConnectError::Handshake {
            endpoint: endpoint.to_string(),
            reason: format!("{e:?}"),
        })
}

#[cfg(test)]
mod tests {
    use super::page_engine_url;
    use app_shell::web_config::resolve;

    #[test]
    fn a_configured_task_is_not_the_rig_engine() {
        // studio-web with TASK_URL set: Session syncs there, but Signal's
        // engine URL is what it always was, the page's own /vox.
        let config = resolve(
            Some(r#"{"task_url":"https://task.fasttrackstudio.app"}"#),
            "https://studio.fasttrackstudio.app",
        );
        assert_eq!(config.task_url, "https://task.fasttrackstudio.app");
        let engine = page_engine_url(
            "https:",
            "studio.fasttrackstudio.app",
            "studio.fasttrackstudio.app",
        );
        assert_eq!(engine, "wss://studio.fasttrackstudio.app/vox");
        assert_ne!(engine, config.task_ws("/vox"));
    }

    #[test]
    fn the_page_rules_are_unchanged() {
        // The engine that served the page, on any port but the dx ones.
        assert_eq!(
            page_engine_url("http:", "192.168.1.9:4040", "192.168.1.9"),
            "ws://192.168.1.9:4040/vox"
        );
        assert_eq!(
            page_engine_url("http:", "localhost:4123", "localhost"),
            "ws://localhost:4123/vox"
        );
        // A dx dev server: the local engine.
        assert_eq!(
            page_engine_url("http:", "localhost:8080", "localhost"),
            "ws://127.0.0.1:4040/vox"
        );
    }
}
