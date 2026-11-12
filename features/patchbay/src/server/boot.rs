//! Starting this machine's engine, or joining the one already running.
//!
//! One engine per machine: whoever starts the engine first (`fts-patchbay`,
//! `patchbay serve`, or any Architect app that links the Patchbay
//! extension) serves it on the listen address, and everyone after that
//! attaches to it as a client. Two engines would mean two `PipeWire`
//! connections editing one graph, or two sets of Core Audio taps rendering
//! one mix.
//!
//! [`Engine::start`] always runs an engine here (the old shell's
//! behaviour: if the port is taken, it still runs, unserved).
//! [`Engine::start_or_attach`] first asks the address whether a Patchbay
//! engine answers there and attaches if one does; otherwise it binds the
//! port **before** creating the backend, so a loser of the race never
//! opens a second `PipeWire` connection.
//!
//! The engine owns its tokio runtime (leaked, like the desktop shell's
//! was): a linked extension's actions and views can run on whatever
//! executor their host has, and [`Engine::run`] puts each call on this
//! runtime.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use architect::host::{EngineHost, WebBundle};

use crate::proto::PatchbayServiceClient;
use crate::proto::services::patchbay_service::PatchbayServiceStreamClient;
use crate::server::PatchbayBackend;
use crate::server::permissions::PermissionProvider;

/// The listen address when nothing else says: loopback, because the RPC
/// is unauthenticated (`patchbay listen lan` opens it up).
pub const DEFAULT_ADDR: &str = "127.0.0.1:4046";

/// How long [`Engine::start_or_attach`] waits for an engine that may
/// already be serving the address.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Makes the hosting app's [`PermissionProvider`], given the engine's
/// runtime.
pub type MakePermissions =
    Box<dyn FnOnce(tokio::runtime::Handle) -> Arc<dyn PermissionProvider> + Send>;

/// How to start the engine.
pub struct EngineOptions {
    /// `host:port` to serve `/vox` (and the browser remote) on.
    pub addr: String,
    /// The browser remote to serve, if any.
    pub web: Option<WebBundle>,
    /// The hosting app's permission flow (Patchbay.app on macOS), made
    /// with the engine's runtime, which its prompts run on.
    pub permissions: Option<MakePermissions>,
    /// Advertise on, and browse, the LAN for other engines.
    pub discover_peers: bool,
}

impl EngineOptions {
    /// Serve on `addr`, with no browser remote, no permission provider,
    /// and peer discovery on.
    #[must_use]
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            web: None,
            permissions: None,
            discover_peers: true,
        }
    }

    /// `PATCHBAY_ADDR`, else the saved `network.bind`, else `default`.
    #[must_use]
    pub fn configured_addr(default: &str) -> String {
        std::env::var("PATCHBAY_ADDR")
            .ok()
            .or_else(crate::server::configured_bind)
            .unwrap_or_else(|| default.to_owned())
    }
}

/// Whether this process runs the engine or reaches another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineMode {
    /// The engine runs here. `serving` is false when the address could
    /// not be bound (the engine still runs, reachable only in-process).
    Owned {
        /// The listen address.
        addr: String,
        /// Whether `/vox` is being served there.
        serving: bool,
    },
    /// Another process's engine, reached at `url`.
    Attached {
        /// Its `ws://…/vox`.
        url: String,
    },
}

/// Why the engine could not start.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The runtime could not be built.
    #[error("patchbay engine runtime: {0}")]
    Runtime(std::io::Error),
    /// The address is taken by something that is not a Patchbay engine.
    #[error("{addr} is in use by something that is not a Patchbay engine ({detail})")]
    AddressTaken {
        /// The listen address.
        addr: String,
        /// What the bind and the probe said.
        detail: String,
    },
    /// The in-process clients could not be set up.
    #[error("patchbay engine clients: {0}")]
    Clients(String),
}

/// A running engine, here or elsewhere: its clients and its runtime.
pub struct Engine {
    client: PatchbayServiceClient,
    stream: PatchbayServiceStreamClient,
    mode: EngineMode,
    runtime: tokio::runtime::Handle,
    /// Keeps an in-process `LocalServer`'s acceptor and lanes alive.
    _scope: Option<Arc<architect::Scope>>,
}

impl Engine {
    /// Run the engine here and serve it on `opts.addr`. A port that is
    /// already taken is logged, and the engine runs unserved.
    ///
    /// # Errors
    /// The runtime or the in-process clients could not be set up.
    pub fn start(opts: EngineOptions) -> Result<Self, EngineError> {
        on_runtime(async move {
            let listener = tokio::net::TcpListener::bind(&opts.addr).await;
            if let Err(e) = &listener {
                tracing::error!(
                    addr = %opts.addr,
                    error = %e,
                    "patchbay engine address in use: running unserved"
                );
            }
            run_here(opts, listener.ok()).await
        })
    }

    /// Attach to the Patchbay engine serving `opts.addr`'s port if there
    /// is one; otherwise take the port and run the engine here.
    ///
    /// # Errors
    /// The port is held by something that is not a Patchbay engine, or
    /// the runtime or clients could not be set up.
    pub fn start_or_attach(opts: EngineOptions) -> Result<Self, EngineError> {
        on_runtime(async move {
            let url = probe_url(&opts.addr);
            if let Ok(engine) = attach(&url).await {
                return Ok(engine);
            }
            match tokio::net::TcpListener::bind(&opts.addr).await {
                Ok(listener) => run_here(opts, Some(listener)).await,
                // Lost a race with another engine starting: join it.
                Err(bind) => attach(&url)
                    .await
                    .map_err(|probe| EngineError::AddressTaken {
                        addr: opts.addr.clone(),
                        detail: format!("bind: {bind}; probe: {probe}"),
                    }),
            }
        })
    }

    /// The RPC client.
    #[must_use]
    pub const fn client(&self) -> &PatchbayServiceClient {
        &self.client
    }

    /// The streaming client (graph and device events).
    #[must_use]
    pub const fn stream_client(&self) -> &PatchbayServiceStreamClient {
        &self.stream
    }

    /// Where the engine runs.
    #[must_use]
    pub const fn mode(&self) -> &EngineMode {
        &self.mode
    }

    /// Run `call` with the RPC client on the engine's runtime, from any
    /// executor.
    ///
    /// # Errors
    /// The task panicked or was cancelled.
    pub async fn run<F, Fut, T>(&self, call: F) -> Result<T, String>
    where
        F: FnOnce(PatchbayServiceClient) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.spawn(call(self.client.clone())).await
    }

    /// Run `fut` on the engine's runtime (it may need tokio's reactor,
    /// as dialling does), from any executor.
    ///
    /// # Errors
    /// The task panicked or was cancelled.
    pub async fn spawn<Fut, T>(&self, fut: Fut) -> Result<T, String>
    where
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.runtime
            .spawn(fut)
            .await
            .map_err(|e| format!("patchbay engine task: {e}"))
    }
}

/// Run `start` on a new runtime that lives as long as the process (the
/// engine's tasks stay on it), from a thread of its own: the caller may
/// itself be on a runtime (an action's future), where `block_on` panics.
fn on_runtime<F>(start: F) -> Result<Engine, EngineError>
where
    F: Future<Output = Result<Engine, EngineError>> + Send + 'static,
{
    let rt = tokio::runtime::Builder::new_multi_thread()
        .thread_name("patchbay-rt")
        .enable_all()
        .build()
        .map_err(EngineError::Runtime)?;
    let rt: &'static tokio::runtime::Runtime = Box::leak(Box::new(rt));
    std::thread::Builder::new()
        .name("patchbay-start".into())
        .spawn(move || rt.block_on(start))
        .map_err(EngineError::Runtime)?
        .join()
        .map_err(|_| EngineError::Clients("the engine start panicked".to_owned()))?
}

/// Where to look for an engine already serving `addr`'s port: loopback,
/// which also reaches an engine bound to every interface.
fn probe_url(addr: &str) -> String {
    let port = addr.rsplit_once(':').map_or(addr, |(_, p)| p);
    format!("ws://127.0.0.1:{port}/vox")
}

/// Dial the engine at `url` (`ws://host:port/vox`): its RPC and its
/// streaming client, the two links a remote opens.
///
/// # Errors
/// The connection or a handshake failed.
pub async fn dial(
    url: &str,
) -> Result<(PatchbayServiceClient, PatchbayServiceStreamClient), String> {
    let link = vox_websocket::WsLink::connect(url)
        .await
        .map_err(|e| format!("connect {url}: {e}"))?;
    let client: PatchbayServiceClient = vox_core::initiator_on(link)
        .establish()
        .await
        .map_err(|e| format!("handshake with {url}: {e:?}"))?;
    let stream_link = vox_websocket::WsLink::connect(url)
        .await
        .map_err(|e| format!("connect (stream) {url}: {e}"))?;
    let stream: PatchbayServiceStreamClient = vox_core::initiator_on(stream_link)
        .establish()
        .await
        .map_err(|e| format!("handshake (stream) with {url}: {e:?}"))?;
    Ok((client, stream))
}

/// Join the engine at `url`: both links, and one call only a Patchbay
/// engine answers.
async fn attach(url: &str) -> Result<Engine, String> {
    let probe = async {
        let (client, stream) = dial(url).await?;
        client
            .listen_address()
            .await
            .map_err(|e| format!("{url} is not a Patchbay engine: {e}"))?;
        Ok::<_, String>((client, stream))
    };
    let (client, stream) = tokio::time::timeout(PROBE_TIMEOUT, probe)
        .await
        .map_err(|_| format!("no answer from {url}"))??;
    Ok(Engine {
        client,
        stream,
        mode: EngineMode::Attached {
            url: url.to_owned(),
        },
        runtime: tokio::runtime::Handle::current(),
        _scope: None,
    })
}

/// Create the backend and its in-process clients, and serve it on
/// `listener` when there is one.
async fn run_here(
    opts: EngineOptions,
    listener: Option<tokio::net::TcpListener>,
) -> Result<Engine, EngineError> {
    let backend = PatchbayBackend::new();
    if let Some(make) = opts.permissions {
        backend.set_permission_provider(make(tokio::runtime::Handle::current()));
    }

    let scope = architect::Scope::new();
    let server = architect::LocalServer::serve(backend.router(), Arc::clone(&scope));
    let caller = server
        .caller()
        .await
        .map_err(|e| EngineError::Clients(format!("caller: {e:?}")))?;
    let client = PatchbayServiceClient::new(caller);
    let stream = server
        .establish::<PatchbayServiceStreamClient>()
        .await
        .map_err(|e| EngineError::Clients(format!("stream client: {e:?}")))?;

    let serving = listener.is_some();
    // What we serve, so `patchbay listen` can report it.
    crate::server::record_bound(
        &opts.addr,
        if opts.web.is_some() {
            ""
        } else {
            "no browser remote in this build (install `dx` and rebuild, \
             or set PATCHBAY_WEB_DIST) — /health and /vox still work"
        },
    );
    // Advertise to (when LAN-reachable) and look for other engines, so a
    // UI attached here can offer to switch to them.
    if opts.discover_peers {
        crate::server::start_peer_discovery(&opts.addr);
    }
    if let Some(listener) = listener {
        // Ungated as before: no PermissionsGate on this engine yet (FastTrackStudios/studio#362).
        let app = EngineHost::ungated(backend.router(), opts.addr.clone())
            .web(opts.web)
            .into_app();
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!(error = %e, "patchbay engine server ended");
            }
        });
    }

    Ok(Engine {
        client,
        stream,
        mode: EngineMode::Owned {
            addr: opts.addr,
            serving,
        },
        runtime: tokio::runtime::Handle::current(),
        _scope: Some(scope),
    })
}
