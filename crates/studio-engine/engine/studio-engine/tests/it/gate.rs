//! The engine's door (studio#362): a client on the network that does not
//! present the engine's token is refused at `/vox`, an iroh peer the
//! engine does not admit is refused on every call, and the engine's own
//! window (the token) and its devices (admitted endpoints) are served.
//!
//! Over a real WebSocket and real iroh endpoints on loopback (no relay, no
//! internet), behind the same door `studio engine --listen` and
//! `engine-host --engine` serve with. The router is the session clock: one
//! method, so a call that answers is a call the door let through.

use std::time::Duration;

use architect::LayerRouter;
use architect::connect::{Endpoint, Pool};
use architect::iroh_link::{VOX_ALPN, iroh};
use engine_sync::clock::{
    ClockHost, SessionClockClient, SessionClockDispatcher, session_clock_service_descriptor,
};
use studio_engine::gate::EngineGate;

/// The engine's token in these tests.
const TOKEN: &str = "0f5b2c7e9a1d4e6f8b3c5a7d9e1f2b4c6d8e0a2b4c6d8e0f1a3b5c7d9e1f3a5b";

/// What the engine serves: its clock.
fn router() -> LayerRouter {
    LayerRouter::new().with(
        session_clock_service_descriptor(),
        SessionClockDispatcher::new(ClockHost),
    )
}

/// `call` within ten seconds, or why not.
async fn within<T>(
    call: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::time::timeout(Duration::from_secs(10), call)
        .await
        .map_err(|_| "no answer within 10 s".to_owned())?
}

/// The engine's `/vox` behind `gate`, on a loopback port: its URL.
async fn serve_ws(gate: &EngineGate) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = gate.host(router(), addr.to_string()).into_app();
    tokio::spawn(async move { architect::http::axum::serve(listener, app).await });
    format!("ws://{addr}/vox")
}

/// The engine's clock, asked over its WebSocket at `url`, presenting
/// `bearer` at the handshake.
async fn clock_over_ws(url: &str, bearer: Option<&str>) -> Result<f64, String> {
    within(async {
        let endpoint = Endpoint::url(url).with_bearer(bearer.map(str::to_owned));
        let clock: SessionClockClient = Pool::new()
            .client(&endpoint)
            .await
            .map_err(|e| e.to_string())?;
        clock.now().await.map_err(|e| format!("{e:?}"))
    })
    .await
}

/// An endpoint on loopback with no relay and no lookup.
async fn endpoint() -> iroh::Endpoint {
    iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .secret_key(iroh::SecretKey::generate())
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![VOX_ALPN.to_vec()])
        .bind()
        .await
        .unwrap()
}

/// Where a peer on this machine dials `endpoint`.
fn at(endpoint: &iroh::Endpoint) -> iroh::EndpointAddr {
    endpoint.bound_sockets().into_iter().fold(
        iroh::EndpointAddr::new(endpoint.id()),
        |at, socket| {
            let ip: std::net::IpAddr = if socket.is_ipv4() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                std::net::Ipv6Addr::LOCALHOST.into()
            };
            at.with_ip_addr(std::net::SocketAddr::new(ip, socket.port()))
        },
    )
}

/// The engine's iroh lane behind `gate`: the endpoint it serves on.
async fn serve_iroh(gate: &EngineGate) -> iroh::Endpoint {
    let engine = endpoint().await;
    let (gate, serving) = (gate.clone(), engine.clone());
    tokio::spawn(async move { gate.serve_iroh(&serving, router()).await });
    engine
}

/// The engine's clock, asked over iroh from `peer`.
async fn clock_over_iroh(engine: &iroh::Endpoint, peer: &iroh::Endpoint) -> Result<f64, String> {
    within(async {
        let link = architect::iroh_link::connect(peer, at(engine))
            .await
            .map_err(|e| e.to_string())?;
        let clock = vox_core::initiator_on(link)
            .establish::<SessionClockClient>()
            .await
            .map_err(|e| format!("{e:?}"))?;
        clock.now().await.map_err(|e| format!("{e:?}"))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_websocket_without_the_engines_token_is_refused() {
    let url = serve_ws(&EngineGate::new(TOKEN, [])).await;
    assert!(
        clock_over_ws(&url, None).await.is_err(),
        "a client on the network with no token reached the engine"
    );
    assert!(
        clock_over_ws(&url, Some("not-the-engines-token"))
            .await
            .is_err(),
        "a client with another token reached the engine"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_window_with_the_engines_token_is_served() {
    let url = serve_ws(&EngineGate::new(TOKEN, [])).await;
    clock_over_ws(&url, Some(TOKEN))
        .await
        .expect("the engine's own window, with its token, is served");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_iroh_peer_the_engine_does_not_admit_is_refused() {
    let gate = EngineGate::new(TOKEN, []);
    let engine = serve_iroh(&gate).await;
    let stranger = endpoint().await;
    assert!(
        clock_over_iroh(&engine, &stranger).await.is_err(),
        "an iroh peer the engine does not know reached it"
    );
    // One of the owner's devices: admitted by the endpoint its handshake
    // proves, and served.
    let device = endpoint().await;
    gate.admit(device.id().to_string());
    clock_over_iroh(&engine, &device)
        .await
        .expect("an admitted device is served");
    assert!(
        clock_over_iroh(&engine, &stranger).await.is_err(),
        "admitting one device let another in"
    );
}

#[test]
fn the_token_is_minted_once_and_readable_by_this_user_only() {
    let dir = tempfile::tempdir().unwrap();
    let gate = EngineGate::load_from(dir.path()).unwrap();
    let file = dir.path().join("engine-token");
    let token = std::fs::read_to_string(&file).unwrap();
    assert!(token.len() >= 32, "a token too short to be a secret");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the token is readable by others");
    }
    // The next start keeps it, so a window that read it is still let in.
    let again = EngineGate::load_from(dir.path()).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), token);
    for door in [&gate, &again] {
        assert!(door.lane(router(), Some(token.clone())).is_ok());
        assert!(door.lane(router(), None).is_err());
    }
}
