//! Remote engines: the vox client as a runtime [`Engine`], WebSocket
//! connect/serve, and the telemetry pump that bridges an engine's streams to
//! a UI thread.

use frame::surface::proto::{Catalog, FeedInfo, FeedRequest, ParamValue, StreamFrame, TextValue};
use frame::surface::runtime::sync::{Engine, EngineError};
use frame::surface::runtime::{EngineCall, Runtime};

use crate::engine::{SurfaceEngineClient, SurfaceEngineStreamClient};

impl Engine for SurfaceEngineClient {
    async fn catalog(&self) -> Result<Catalog, EngineError> {
        SurfaceEngineClient::catalog(self)
            .await
            .map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn values(&self) -> Result<Vec<ParamValue>, EngineError> {
        SurfaceEngineClient::values(self)
            .await
            .map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn call(&self, call: EngineCall) -> Result<(), EngineError> {
        let r = match call {
            EngineCall::SetParam { addr, value, phase } => self.set_param(addr, value, phase).await,
            EngineCall::Invoke { action } => self.invoke(action).await,
            EngineCall::Select { collection, index } => self.select(collection, index).await,
            EngineCall::SetText { addr, text } => self.set_text(addr, text).await,
        };
        r.map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn request_feeds(
        &self,
        requests: Vec<FeedRequest>,
    ) -> Result<Vec<FeedInfo>, EngineError> {
        SurfaceEngineClient::request_feeds(self, requests)
            .await
            .map_err(|e| EngineError(format!("{e:?}")))
    }

    async fn texts(&self) -> Result<Vec<TextValue>, EngineError> {
        SurfaceEngineClient::texts(self)
            .await
            .map_err(|e| EngineError(format!("{e:?}")))
    }
}

/// Connect to an engine served over a WebSocket (e.g. signal's
/// `ws://127.0.0.1:4040/vox`), optionally picking a rig by instance scope
/// (`"guitar"`), and return its `SurfaceEngine` client.
///
/// # Errors
/// When the socket can't be opened or the vox handshake fails.
pub async fn connect_ws(
    url: &str,
    scope: Option<&str>,
) -> Result<SurfaceEngineClient, EngineError> {
    let link = vox_websocket::WsLink::connect(url)
        .await
        .map_err(|e| EngineError(format!("ws connect {url}: {e:?}")))?;
    let client: SurfaceEngineClient = vox_core::initiator_on(link)
        .establish()
        .await
        .map_err(|e| EngineError(format!("vox handshake: {e:?}")))?;
    Ok(match scope {
        Some(s) => architect::scope_client!(client, s.to_string()),
        None => client,
    })
}

/// Connect to the stream side of an engine served over a WebSocket — the
/// `param_changes` / `telemetry` subscriptions. A vox link carries one
/// service, so this opens its own socket next to [`connect_ws`]'s.
///
/// # Errors
/// When the socket can't be opened or the vox handshake fails.
pub async fn connect_ws_stream(
    url: &str,
    scope: Option<&str>,
) -> Result<SurfaceEngineStreamClient, EngineError> {
    let link = vox_websocket::WsLink::connect(url)
        .await
        .map_err(|e| EngineError(format!("ws connect {url}: {e:?}")))?;
    let client: SurfaceEngineStreamClient = vox_core::initiator_on(link)
        .establish()
        .await
        .map_err(|e| EngineError(format!("vox handshake: {e:?}")))?;
    Ok(match scope {
        Some(s) => architect::scope_client!(client, s.to_string()),
        None => client,
    })
}

/// Serve `router` (e.g. `Memory::demo().into_router()`, optionally
/// `merge_router_scoped`) as a WebSocket vox endpoint on `listener` —
/// the server half of [`connect_ws`] / [`connect_ws_stream`], for tests and
/// for serving an in-process engine to remote surfaces. Runs until the
/// listener fails; connections live on the current tokio runtime.
pub async fn serve_ws(listener: tokio::net::TcpListener, router: architect::LayerRouter) {
    while let Ok((tcp, _)) = listener.accept().await {
        let router = router.clone();
        tokio::spawn(async move {
            let Ok(link) = vox_websocket::WsLink::server(tcp).await else {
                return;
            };
            if let Ok(connection) = vox_core::acceptor_on(link)
                .on_lane(router.acceptor())
                .establish_connection()
                .await
            {
                let _connection = connection;
                std::future::pending::<()>().await;
            }
        });
    }
}

/// What an engine pushes to a surface: telemetry frames and parameter
/// changes (from other surfaces, MIDI, automation, or echoes of our own).
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Frame(StreamFrame),
    Param(ParamValue),
}

impl Incoming {
    /// Feed one into a runtime at `t` ms.
    pub fn apply(self, rt: &mut Runtime, t: f64) {
        match self {
            Self::Frame(frame) => rt.apply_frame(frame, t),
            Self::Param(value) => rt.apply_values([value]),
        }
    }
}

/// Bridges an engine's streams to a non-async thread: [`Telemetry::pump`]
/// subscribes to `telemetry` and `param_changes` and forwards every item
/// into a `std::sync::mpsc` channel, which a UI thread drains each frame
/// (`try_iter`) without ever blocking.
///
/// `T` is whatever the receiving side's channel carries — [`Incoming`] by
/// default, or a host event enum with `From<Incoming>` so connection
/// status and engine data arrive on one channel.
pub struct Telemetry<T = Incoming> {
    tx: std::sync::mpsc::Sender<T>,
}

impl<T> Clone for Telemetry<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

/// Why [`Telemetry::pump`] returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PumpEnd {
    /// The receiving side hung up (the UI is gone): stop for good.
    Closed,
    /// A subscription ended — the link dropped or the engine went away.
    Lost(EngineError),
}

impl Telemetry<Incoming> {
    /// A pump and the receiver a UI thread drains.
    #[must_use]
    pub fn channel() -> (Self, std::sync::mpsc::Receiver<Incoming>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Self { tx }, rx)
    }
}

impl<T: From<Incoming> + Send + 'static> Telemetry<T> {
    /// Forward into an existing channel.
    #[must_use]
    pub fn new(tx: std::sync::mpsc::Sender<T>) -> Self {
        Self { tx }
    }

    /// Subscribe to both streams of `client` and forward until either
    /// subscription ends or the receiver is dropped. Spawn it on the
    /// runtime that owns the link (`tokio::spawn(async move { t.pump(&c).await })`).
    pub async fn pump(&self, client: &SurfaceEngineStreamClient) -> PumpEnd {
        use architect::vox;
        let (frame_tx, mut frame_rx) = vox::channel::<StreamFrame>();
        let (param_tx, mut param_rx) = vox::channel::<ParamValue>();
        let frames_call = client.telemetry(frame_tx);
        let params_call = client.param_changes(param_tx);
        let frames = async {
            while let Ok(Some(item)) = frame_rx.recv().await {
                if self.tx.send(T::from(Incoming::Frame(owned(item)))).is_err() {
                    return PumpEnd::Closed;
                }
            }
            PumpEnd::Lost(EngineError("telemetry stream ended".into()))
        };
        let params = async {
            while let Ok(Some(item)) = param_rx.recv().await {
                if self.tx.send(T::from(Incoming::Param(owned(item)))).is_err() {
                    return PumpEnd::Closed;
                }
            }
            PumpEnd::Lost(EngineError("param_changes stream ended".into()))
        };
        tokio::select! {
            end = frames => end,
            end = params => end,
            r = frames_call => PumpEnd::Lost(EngineError(format!("telemetry subscription ended: {r:?}"))),
            r = params_call => PumpEnd::Lost(EngineError(format!("param_changes subscription ended: {r:?}"))),
        }
    }
}

/// Move an owned (non-borrowing) value out of a received `SelfRef`.
fn owned<T: 'static>(item: architect::vox::SelfRef<T>) -> T {
    let mut out = None;
    let _ = item.map(|v| out = Some(v));
    out.expect("SelfRef::map runs its closure")
}
