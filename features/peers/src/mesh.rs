//! The node: one Studio engine on its account's mesh (feature `mesh`).
//!
//! - its own iroh endpoint ([`bind`]: n0 and the LAN), served so each
//!   connection gets a router behind a gate that admits the account's
//!   devices only ([`Mesh::start`]); a device taken off the account loses
//!   its connections (`architect_devices::serve`);
//! - the account pass ([`Mesh::keep_on_account`]): this engine in the
//!   issuer's address book, the account's other engines let in;
//! - on each connection: [`StudioPeer`] (what it runs and plays) and, when
//!   the engine keeps its rig books in a `crdt::docs` driver, that driver's
//!   host (`DocsDriver::host`), which the other peers' drivers sync
//!   against ([`Mesh::sync_books`]);
//! - following another peer ([`Mesh::follow`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use architect::iroh_link::{self, PeerLookup};
use architect::{LayerRouter, PubSub};
use architect_devices::account::{self, AccountReport, FixedEndpoint, PassError, ThisDevice};
use architect_devices::{Admission, endpoint_key};
use architect_permissions::{
    Action, BoxIdentityFuture, Decision, IdentityResolver, MethodPermit, PermissionEngine,
    Principal, Resource, ServicePermits,
};
use crdt::docs::{DocsDriver, DocsLane};

use crate::follow::{Follow, FollowAction, Here};
use crate::wire::service::{
    StudioPeer, StudioPeerClient, StudioPeerStreamClient, StudioPeerStreamSource,
};
use crate::wire::{About, NowPlaying, Roles};

/// How often the account pass runs while all is well, and the wait after
/// one fails (or finds nobody else yet).
pub const ACCOUNT_EVERY: Duration = Duration::from_secs(120);
pub const ACCOUNT_RETRY: Duration = Duration::from_secs(10);

/// The least wait between passes a stranger's knock brings forward.
const KNOCK_EVERY: Duration = Duration::from_secs(3);

/// How long an admitted device that is not a Studio engine is left alone.
const NOT_STUDIO_MS: u64 = 20_000;

/// How long a dial or a call to a peer may take.
const DIAL: Duration = Duration::from_secs(8);

/// Bind this engine's mesh endpoint with `key`: n0 (relays, DNS) and the
/// LAN (mDNS, `_fts._udp`), advertised as `name`. Where the network
/// allows no multicast (an iOS build without the entitlement), n0 alone.
///
/// # Errors
///
/// The endpoint would not bind.
pub async fn bind(key: iroh::SecretKey, name: &str) -> Result<iroh::Endpoint, String> {
    let endpoint = match iroh_link::bind_endpoint_with(key.clone(), PeerLookup::N0AndLan).await {
        Ok(ep) => ep,
        Err(lan) => {
            tracing::warn!(error = %lan, "peers: no LAN lookup here, n0 only");
            iroh_link::bind_endpoint_with(key, PeerLookup::N0)
                .await
                .map_err(|e| format!("iroh bind: {e}"))?
        }
    };
    iroh_link::advertise_name(&endpoint, name);
    Ok(endpoint)
}

/// One of the account's other engines, as this one knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerView {
    pub endpoint: String,
    /// What the issuer's address book calls it.
    pub name: String,
    pub platform: String,
    /// What it says of itself, once reached.
    pub about: Option<About>,
}

/// What the app does to follow: read where this engine is, carry out a
/// step. Called from the follow's task.
pub trait FollowTarget: Send + Sync + 'static {
    fn here(&self) -> Here;
    fn apply(&self, actions: &[FollowAction]);
    /// Keep `cues` as the rig's song cues.
    ///
    /// # Errors
    ///
    /// Why they could not be kept.
    fn set_cues(&self, _cues: &crate::RigCues) -> Result<(), String> {
        Err("this engine keeps no cues".into())
    }
}

struct Inner {
    endpoint: iroh::Endpoint,
    admission: Admission,
    about: Mutex<About>,
    now: Mutex<NowPlaying>,
    hub: PubSub<NowPlaying>,
    driver: Option<DocsDriver>,
    /// Addresses learnt out of band (tests on loopback; a LAN sighting),
    /// by endpoint key.
    hints: Mutex<HashMap<String, iroh::EndpointAddr>>,
    /// Connections to peers, one each.
    pool: architect::connect::Pool,
    /// The peer followed, and its task.
    following: Mutex<Option<(String, tokio::task::JoinHandle<()>)>>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    /// An endpoint this engine does not let in called: perhaps a device
    /// the account gained since the last pass. The pass runs early.
    knocked: Arc<tokio::sync::Notify>,
    /// Admitted devices that are not Studio engines (a server, a phone's
    /// Task): left alone until then, ms since the epoch.
    not_studio: Mutex<HashMap<String, u64>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        for task in self.tasks.get_mut().unwrap_or_else(PoisonError::into_inner).drain(..) {
            task.abort();
        }
        if let Some((_, task)) = self
            .following
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            task.abort();
        }
    }
}

/// This engine on the mesh. Cheap to clone.
#[derive(Clone)]
pub struct Mesh {
    inner: Arc<Inner>,
}

/// The [`StudioPeer`] each connection is served: a view of the mesh.
#[derive(Clone, architect::HasDispatcher)]
#[dispatch(architect::dispatch::CurrentThreadDispatcher)]
struct Serving(Arc<Inner>);

impl StudioPeer for Serving {
    fn about(&self) -> About {
        lock(&self.0.about).clone()
    }

    fn now(&self) -> NowPlaying {
        lock(&self.0.now).clone()
    }
}

impl StudioPeerStreamSource for Serving {
    fn playing_hub(&self) -> &PubSub<NowPlaying> {
        &self.0.hub
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The gate's engine: the account's devices (and anything else the
/// admission lets in) may call; nobody else.
#[derive(Clone)]
struct Devices(Admission, Arc<tokio::sync::Notify>);

impl PermissionEngine for Devices {
    fn check(&self, who: &Principal, _: &Resource, _: &Action) -> Decision {
        match who {
            Principal::Host { endpoint } if self.0.admits(endpoint) => Decision::Allow,
            Principal::Host { .. } => {
                self.1.notify_one();
                Decision::deny("not one of the account's devices")
            }
            other => Decision::deny(format!("{} is not a peer", other.describe())),
        }
    }
}

/// Who calls on one connection: the endpoint its handshake proved.
struct Proved(String);

impl IdentityResolver for Proved {
    fn resolve<'a>(&'a self, _bearer: Option<&'a str>) -> BoxIdentityFuture<'a> {
        let who = Principal::Host {
            endpoint: self.0.clone(),
        };
        Box::pin(async move { who })
    }
}

/// Every method of `descriptor`, permitted to whoever the gate's engine
/// lets in (one rule: the account's devices). Leaked once per service.
fn permit_all(descriptor: &'static vox::ServiceDescriptor) -> &'static ServicePermits {
    let methods: Vec<MethodPermit> = descriptor
        .methods
        .iter()
        .map(|m| MethodPermit::new(m.method_name, "write", "peers/**"))
        .collect();
    Box::leak(Box::new(ServicePermits {
        service: descriptor.service_name,
        methods: Box::leak(methods.into_boxed_slice()),
    }))
}

/// The services each connection gets, and their descriptors for the gate.
fn services(
    serving: &Serving,
    books: Option<&crdt::docs::DriverHost>,
) -> (LayerRouter, Vec<&'static vox::ServiceDescriptor>) {
    use crate::wire::service::{
        studio_peer_rpc_service_descriptor, studio_peer_stream_service_descriptor,
    };
    use architect::Layer as _;
    let mut router = LayerRouter::new().merge_router(
        architect::layers![
            crate::wire::service::Service,
            crate::wire::service::StreamService
        ]
        .provide(serving.clone()),
    );
    let mut descriptors = vec![
        studio_peer_rpc_service_descriptor(),
        studio_peer_stream_service_descriptor(),
    ];
    if let Some(host) = books {
        use crdt::docs::{docs_rpc_service_descriptor, docs_stream_service_descriptor};
        use crdt::sync::{DocSyncDispatcher, doc_sync_service_descriptor};
        router = router
            .with(docs_rpc_service_descriptor(), crdt::docs::serve(host.clone()))
            .merge(crdt::docs::stream_layer(host.clone()))
            .with(doc_sync_service_descriptor(), DocSyncDispatcher::new(host.clone()));
        descriptors.extend([
            docs_rpc_service_descriptor(),
            docs_stream_service_descriptor(),
            doc_sync_service_descriptor(),
        ]);
    }
    (router, descriptors)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl Mesh {
    /// Serve `endpoint` as this engine (`about`), its rig books' `driver`
    /// on the same connections when it has one. Must run in a tokio
    /// runtime; the serving lives as long as a clone of the mesh.
    ///
    /// # Errors
    ///
    /// The books' driver would not serve.
    pub async fn start(
        endpoint: iroh::Endpoint,
        about: About,
        driver: Option<DocsDriver>,
    ) -> Result<Self, String> {
        let admission = Admission::new();
        let inner = Arc::new(Inner {
            endpoint: endpoint.clone(),
            admission: admission.clone(),
            about: Mutex::new(about),
            now: Mutex::new(NowPlaying::default()),
            hub: PubSub::sliding(64),
            driver: driver.clone(),
            hints: Mutex::new(HashMap::new()),
            pool: architect::connect::Pool::new(),
            following: Mutex::new(None),
            tasks: Mutex::new(Vec::new()),
            knocked: Arc::new(tokio::sync::Notify::new()),
            not_studio: Mutex::new(HashMap::new()),
        });
        let books = match &driver {
            Some(driver) => {
                let admitted = admission.clone();
                Some(
                    driver
                        .host(move |who| {
                            matches!(who, Some(Principal::Host { endpoint }) if admitted.admits(endpoint))
                        })
                        .await
                        .map_err(|e| format!("the books' host: {e:?}"))?,
                )
            }
            None => None,
        };
        let serving = Serving(inner.clone());
        let (router, descriptors) = services(&serving, books.as_ref());
        let permits: Vec<_> = descriptors.into_iter().map(|d| (d, permit_all(d))).collect();
        let engine = Arc::new(Devices(admission.clone(), inner.knocked.clone()));
        // The serve task holds a weak view: dropping the last mesh ends it.
        let task = tokio::spawn({
            let admission = admission.clone();
            async move {
                architect_devices::serve(&endpoint, admission, move |remote| {
                    let gate = permits.iter().fold(
                        architect::permissions_gate::PermissionsGate::new(
                            engine.clone(),
                            Arc::new(Proved(endpoint_key(&remote.to_string()))),
                        ),
                        |gate, (d, p)| gate.permit(d, p),
                    );
                    gate.wrap(router.clone())
                })
                .await;
            }
        });
        lock(&inner.tasks).push(task);
        Ok(Self { inner })
    }

    /// This engine's endpoint id, as the address book keys it.
    #[must_use]
    pub fn id(&self) -> String {
        endpoint_key(&self.inner.endpoint.id().to_string())
    }

    #[must_use]
    pub fn endpoint(&self) -> &iroh::Endpoint {
        &self.inner.endpoint
    }

    /// Who this engine lets in.
    #[must_use]
    pub fn admission(&self) -> &Admission {
        &self.inner.admission
    }

    /// What it runs now changed.
    pub fn set_roles(&self, roles: Roles) {
        lock(&self.inner.about).roles = roles;
    }

    #[must_use]
    pub fn about(&self) -> About {
        lock(&self.inner.about).clone()
    }

    /// What it plays now: kept for `now`, published to its followers when
    /// anything but the clock moved, or every call while it plays (the
    /// caller paces it: a second is plenty).
    pub fn publish(&self, mut now: NowPlaying) {
        if now.at_ms == 0 {
            now.at_ms = now_ms();
        }
        let mut held = lock(&self.inner.now);
        let same = NowPlaying {
            revision: held.revision,
            position: held.position,
            at_ms: held.at_ms,
            ..now.clone()
        } == *held;
        if same && !now.playing && held.revision > 0 {
            return;
        }
        now.revision = held.revision + 1;
        *held = now.clone();
        drop(held);
        self.inner.hub.publish(now);
    }

    #[must_use]
    pub fn now(&self) -> NowPlaying {
        lock(&self.inner.now).clone()
    }

    /// Where `endpoint` can be reached, learnt some other way than the
    /// network's lookup (a test's loopback socket).
    pub fn learn_addr(&self, addr: iroh::EndpointAddr) {
        let key = endpoint_key(&addr.id.to_string());
        lock(&self.inner.hints).insert(key, addr);
    }

    // ── The account ───────────────────────────────────────────────────

    /// One account pass: this engine in the address book, at its endpoint;
    /// the account's other devices let in.
    ///
    /// # Errors
    ///
    /// The issuer refused or could not be reached.
    pub async fn account_pass(
        &self,
        devices: &auth_proto::DeviceServiceClient,
        token: &str,
        me: &ThisDevice,
    ) -> Result<AccountReport, PassError> {
        account::pass(
            devices,
            token,
            me,
            &FixedEndpoint(self.id()),
            &self.inner.admission,
        )
        .await
    }

    /// Run the account pass against the issuer at `issuer` (its `https://`
    /// base) now and again, while `token` gives a session; until the mesh
    /// is dropped. `report` hears each outcome.
    pub fn keep_on_account(
        &self,
        issuer: &str,
        token: impl Fn() -> Option<String> + Send + Sync + 'static,
        me: ThisDevice,
        report: impl Fn(&Result<AccountReport, PassError>) + Send + Sync + 'static,
    ) {
        let target = architect::connect::Endpoint::url(issuer_vox(issuer));
        let weak = Arc::downgrade(&self.inner);
        let task = tokio::spawn(async move {
            let pool = architect::connect::Pool::new();
            loop {
                let Some(inner) = weak.upgrade() else { return };
                let mesh = Self { inner };
                let wait = match token() {
                    None => ACCOUNT_RETRY,
                    Some(token) => {
                        let outcome = match pool.client::<auth_proto::DeviceServiceClient>(&target).await {
                            Ok(devices) => mesh.account_pass(&devices, &token, &me).await,
                            Err(e) => Err(PassError::Issuer(e.to_string())),
                        };
                        report(&outcome);
                        match outcome {
                            Ok(r) if !r.admitted.is_empty() => ACCOUNT_EVERY,
                            Ok(_) => ACCOUNT_RETRY,
                            Err(_) => {
                                pool.evict(&target);
                                ACCOUNT_RETRY
                            }
                        }
                    }
                };
                let knocked = mesh.inner.knocked.clone();
                drop(mesh);
                // Early when a stranger knocks, but never more than every
                // few seconds.
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = knocked.notified() => {
                        tokio::time::sleep(KNOCK_EVERY).await;
                    }
                }
            }
        });
        lock(&self.inner.tasks).push(task);
    }

    // ── Peers ─────────────────────────────────────────────────────────

    fn pool_key(endpoint: &str) -> architect::connect::Endpoint {
        architect::connect::Endpoint::url(format!("iroh://{}", endpoint_key(endpoint)))
    }

    /// A connection to `endpoint`: the pool's, dialled once and shared by
    /// every client (a dead one is dialled again).
    async fn caller(&self, endpoint: &str) -> Result<vox_core::Caller, String> {
        let key = endpoint_key(endpoint);
        let addr = match lock(&self.inner.hints).get(&key).cloned() {
            Some(addr) => addr,
            None => {
                let id: iroh::EndpointId =
                    key.parse().map_err(|e| format!("endpoint {key}: {e}"))?;
                iroh::EndpointAddr::from(id)
            }
        };
        let ep = self.inner.endpoint.clone();
        self.inner
            .pool
            .caller_with(&Self::pool_key(&key), move |target| dial(ep, addr, target))
            .await
            .map_err(|e| e.to_string())
    }

    fn forget(&self, endpoint: &str) {
        self.inner.pool.evict(&Self::pool_key(endpoint));
    }

    /// The [`StudioPeer`] client for `endpoint`.
    ///
    /// # Errors
    ///
    /// It could not be reached.
    pub async fn peer(&self, endpoint: &str) -> Result<StudioPeerClient, String> {
        Ok(StudioPeerClient::new(self.caller(endpoint).await?))
    }

    /// What `endpoint` says it is, if it is a Studio engine and answers.
    async fn ask_about(&self, endpoint: &str) -> Option<About> {
        let client = self.peer(endpoint).await.ok()?;
        match tokio::time::timeout(DIAL, client.about()).await {
            Ok(Ok(about)) => {
                lock(&self.inner.not_studio).remove(endpoint);
                Some(about)
            }
            _ => {
                self.forget(endpoint);
                None
            }
        }
    }

    /// The account's other engines, each asked what it is (all at once,
    /// briefly: one not answering is listed without).
    pub async fn peers(&self) -> Vec<PeerView> {
        let mut asks = tokio::task::JoinSet::new();
        for (i, peer) in self.inner.admission.list().into_iter().enumerate() {
            let mesh = self.clone();
            asks.spawn(async move {
                let about = mesh.ask_about(&peer.endpoint).await;
                (
                    i,
                    PeerView {
                        endpoint: peer.endpoint.clone(),
                        name: peer.name.clone(),
                        platform: peer.platform.clone(),
                        about,
                    },
                )
            });
        }
        let mut out: Vec<(usize, PeerView)> = asks.join_all().await;
        out.sort_by_key(|(i, _)| *i);
        out.into_iter().map(|(_, p)| p).collect()
    }

    // ── The rig books ─────────────────────────────────────────────────

    /// Keep this engine's books in step with every peer it lets in: a
    /// `run_peer` session per peer, started again when one ends, checked
    /// every `every`. Nothing without a driver.
    pub fn sync_books(&self, every: Duration) {
        let Some(driver) = self.inner.driver.clone() else {
            return;
        };
        let weak = Arc::downgrade(&self.inner);
        let task = tokio::spawn(async move {
            let mut running: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
            loop {
                let Some(inner) = weak.upgrade() else { return };
                let mesh = Self { inner };
                running.retain(|_, t| !t.is_finished());
                let admitted: Vec<String> = mesh
                    .inner
                    .admission
                    .list()
                    .into_iter()
                    .map(|p| p.endpoint)
                    .collect();
                running.retain(|k, t| {
                    let keep = admitted.contains(k);
                    if !keep {
                        t.abort();
                    }
                    keep
                });
                for endpoint in admitted {
                    if running.contains_key(&endpoint) {
                        continue;
                    }
                    let resting = lock(&mesh.inner.not_studio)
                        .get(&endpoint)
                        .is_some_and(|until| *until > now_ms());
                    if resting {
                        continue;
                    }
                    // Only a Studio engine keeps rig books.
                    if mesh.ask_about(&endpoint).await.is_none() {
                        lock(&mesh.inner.not_studio).insert(endpoint, now_ms() + NOT_STUDIO_MS);
                        continue;
                    }
                    let Ok(caller) = mesh.caller(&endpoint).await else {
                        continue;
                    };
                    let lane = DocsLane::over(&caller);
                    let (driver, key) = (driver.clone(), endpoint.clone());
                    running.insert(
                        endpoint,
                        tokio::spawn(async move {
                            if let Err(e) = driver.run_peer(&key, &lane).await {
                                tracing::debug!(peer = %key, error = ?e, "peers: a book session ended");
                            }
                        }),
                    );
                }
                drop(mesh);
                tokio::time::sleep(every).await;
            }
        });
        lock(&self.inner.tasks).push(task);
    }

    // ── Following ─────────────────────────────────────────────────────

    /// Follow `endpoint`'s song and transport onto `target`, until
    /// [`Self::unfollow`] or another follow. Reconnects when the link drops.
    pub fn follow(&self, endpoint: &str, target: Arc<dyn FollowTarget>) {
        let key = endpoint_key(endpoint);
        let weak = Arc::downgrade(&self.inner);
        let followed = key.clone();
        let task = tokio::spawn(async move {
            let mut follow = Follow::new();
            loop {
                let Some(inner) = weak.upgrade() else { return };
                let mesh = Self { inner };
                if let Err(why) = mesh.follow_once(&followed, &mut follow, &*target).await {
                    tracing::debug!(peer = %followed, %why, "peers: the follow dropped; again");
                }
                mesh.forget(&followed);
                drop(mesh);
                follow.reset();
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        lock(&self.inner.about).following.clone_from(&key);
        if let Some((_, old)) = lock(&self.inner.following).replace((key, task)) {
            old.abort();
        }
    }

    /// Stop following.
    pub fn unfollow(&self) {
        lock(&self.inner.about).following.clear();
        if let Some((_, task)) = lock(&self.inner.following).take() {
            task.abort();
        }
    }

    /// The endpoint followed, if any.
    #[must_use]
    pub fn following(&self) -> Option<String> {
        lock(&self.inner.following).as_ref().map(|(k, _)| k.clone())
    }

    async fn follow_once(
        &self,
        endpoint: &str,
        follow: &mut Follow,
        target: &dyn FollowTarget,
    ) -> Result<(), String> {
        let caller = self.caller(endpoint).await?;
        let peer = StudioPeerClient::new(caller.clone());
        let stream = StudioPeerStreamClient::new(caller);
        let (tx, mut rx) = vox::channel::<NowPlaying>();
        // The subscribe call is in flight for the subscription's life.
        let sub = tokio::spawn(async move { stream.playing(tx).await });
        let _guard = AbortOnDrop(sub);
        // The hub replays nothing: the state now, then each change.
        let first = peer.now().await.map_err(|e| format!("now: {e:?}"))?;
        apply(follow, &first, target);
        loop {
            match rx.recv().await {
                Ok(Some(event)) => {
                    let mut np = None;
                    let _ = event.map(|e| np = Some(e));
                    let Some(np) = np else { continue };
                    apply(follow, &np, target);
                }
                Ok(None) => return Err("the stream ended".into()),
                Err(e) => return Err(format!("the stream: {e:?}")),
            }
        }
    }
}

struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn apply(follow: &mut Follow, source: &NowPlaying, target: &dyn FollowTarget) {
    let actions = follow.step(source, &target.here(), now_ms());
    if !actions.is_empty() {
        target.apply(&actions);
    }
}

/// Dial `addr` and complete the root handshake.
async fn dial(
    ep: iroh::Endpoint,
    addr: iroh::EndpointAddr,
    target: architect::connect::Endpoint,
) -> Result<architect::connect::RootLane, architect::connect::ConnectError> {
    use architect::connect::ConnectError;
    let failed = |reason: String| ConnectError::Dial {
        endpoint: target.to_string(),
        reason,
    };
    let link = tokio::time::timeout(DIAL, iroh_link::connect(&ep, addr))
        .await
        .map_err(|_| failed("dial timed out".into()))?
        .map_err(|e| failed(format!("iroh connect: {e}")))?;
    tokio::time::timeout(
        DIAL,
        vox_core::initiator_on(link).establish::<architect::connect::RootLane>(),
    )
    .await
    .map_err(|_| failed("handshake timed out".into()))?
    .map_err(|e| ConnectError::Handshake {
        endpoint: target.to_string(),
        reason: format!("{e:?}"),
    })
}

/// The issuer's vox endpoint: `https://x` → `wss://x/vox`.
#[must_use]
pub fn issuer_vox(issuer: &str) -> String {
    let issuer = issuer.trim_end_matches('/');
    let ws = issuer
        .strip_prefix("https://")
        .map(|rest| format!("wss://{rest}"))
        .or_else(|| issuer.strip_prefix("http://").map(|rest| format!("ws://{rest}")))
        .unwrap_or_else(|| issuer.to_owned());
    format!("{ws}/vox")
}

/// This engine as the issuer's address book lists it: its own row beside
/// any other app on the same machine (an Architect node has its own), so
/// its id is the install's and Studio's together.
#[must_use]
pub fn this_engine(install: uuid::Uuid, account: &str, name: &str, platform: &str) -> ThisDevice {
    let studio = uuid::Uuid::new_v5(&install, b"studio-engine");
    ThisDevice {
        device_id: uuid::Uuid::new_v5(&studio, account.as_bytes()),
        name: format!("{name} · Studio"),
        platform: platform.to_owned(),
        relay_url: None,
        retire: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_issuer_is_dialled_at_its_vox() {
        assert_eq!(
            issuer_vox("https://auth.fasttrackstudio.app/"),
            "wss://auth.fasttrackstudio.app/vox"
        );
        assert_eq!(issuer_vox("http://127.0.0.1:3000"), "ws://127.0.0.1:3000/vox");
    }

    #[test]
    fn studio_has_its_own_row_beside_the_node_on_one_machine() {
        let install = uuid::Uuid::from_u128(1);
        let a = this_engine(install, "user-1", "battleship", "linux");
        assert_eq!(a, this_engine(install, "user-1", "battleship", "linux"));
        assert_ne!(
            a.device_id,
            uuid::Uuid::new_v5(&install, b"user-1"),
            "not the node's id"
        );
        assert_ne!(
            a.device_id,
            this_engine(install, "user-2", "battleship", "linux").device_id
        );
        assert_eq!(a.name, "battleship · Studio");
    }
}

// ── The engine's handle ───────────────────────────────────────────────

/// The engine's side of its mesh, whether or not the mesh is up: what
/// [`PeersControl`](crate::PeersControl) serves on the engine's local
/// router, and where the app hands its mesh once it has one.
#[derive(Clone, architect::HasDispatcher)]
#[dispatch(architect::dispatch::CurrentThreadDispatcher)]
pub struct Node {
    inner: Arc<NodeInner>,
}

struct NodeInner {
    mesh: Mutex<Option<Mesh>>,
    why: Mutex<String>,
    account: Mutex<String>,
    target: Arc<dyn FollowTarget>,
    /// What a window adds: its roles, and when it last published.
    window: Mutex<(Roles, u64)>,
    rigs: Mutex<Vec<String>>,
    /// Follow the one peer playing tracks by itself, until told otherwise.
    auto: std::sync::atomic::AtomicBool,
}

/// How long a window's publish counts as "it plays tracks".
const WINDOW_FRESH_MS: u64 = 30_000;

impl Node {
    /// A node with no mesh yet (`why`), following onto `target` once it has
    /// one.
    #[must_use]
    pub fn new(target: Arc<dyn FollowTarget>, why: &str) -> Self {
        Self {
            inner: Arc::new(NodeInner {
                mesh: Mutex::new(None),
                why: Mutex::new(why.to_owned()),
                account: Mutex::new(String::new()),
                target,
                window: Mutex::new((Roles::default(), 0)),
                rigs: Mutex::new(Vec::new()),
                auto: std::sync::atomic::AtomicBool::new(true),
            }),
        }
    }

    /// The mesh is up.
    pub fn set_mesh(&self, mesh: Mesh) {
        *lock(&self.inner.mesh) = Some(mesh);
        lock(&self.inner.why).clear();
        self.refresh_roles();
    }

    /// The mesh is not up, and why.
    pub fn set_down(&self, why: &str) {
        *lock(&self.inner.why) = why.to_owned();
    }

    #[must_use]
    pub fn mesh(&self) -> Option<Mesh> {
        lock(&self.inner.mesh).clone()
    }

    /// What the last account pass said.
    pub fn set_account(&self, said: &str) {
        *lock(&self.inner.account) = said.to_owned();
    }

    /// The rigs this engine runs now.
    pub fn set_rigs(&self, rigs: Vec<String>) {
        *lock(&self.inner.rigs) = rigs;
        self.refresh_roles();
    }

    /// Whether a window in another process plays this engine's tracks now
    /// (it published lately): its playing is then this engine's, not the
    /// rig's own.
    #[must_use]
    pub fn window_plays(&self) -> bool {
        let (roles, at) = lock(&self.inner.window).clone();
        roles.tracks && now_ms().saturating_sub(at) < WINDOW_FRESH_MS
    }

    fn refresh_roles(&self) {
        let Some(mesh) = self.mesh() else { return };
        let mut roles = Roles {
            tracks: self.window_plays(),
            rigs: lock(&self.inner.rigs).clone(),
        };
        roles.rigs.sort();
        roles.rigs.dedup();
        if mesh.about().roles != roles {
            mesh.set_roles(roles);
        }
    }

    /// Whether this engine follows the one peer playing tracks by itself.
    pub fn set_auto_follow(&self, on: bool) {
        self.inner.auto.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// One look for a peer to follow by itself: when auto-follow is on,
    /// this engine plays no tracks and follows nobody, and exactly one
    /// reachable peer plays tracks. Returns the peer it followed.
    pub async fn auto_follow_once(&self) -> Option<String> {
        if !self.inner.auto.load(std::sync::atomic::Ordering::Relaxed) || self.window_plays() {
            return None;
        }
        let mesh = self.mesh()?;
        if mesh.following().is_some() {
            return None;
        }
        let playing: Vec<PeerView> = mesh
            .peers()
            .await
            .into_iter()
            .filter(|p| p.about.as_ref().is_some_and(|a| a.roles.tracks && a.following.is_empty()))
            .collect();
        let [one] = playing.as_slice() else {
            return None;
        };
        mesh.follow(&one.endpoint, self.inner.target.clone());
        tracing::info!(peers.follow = %one.endpoint, peers.name = %one.name, "peers: following the peer that plays tracks");
        Some(one.endpoint.clone())
    }

    /// [`Self::auto_follow_once`] every `every`, until the node is dropped.
    pub fn auto_follow(&self, every: Duration) {
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                let Some(inner) = weak.upgrade() else { return };
                let _ = Node { inner }.auto_follow_once().await;
            }
        });
    }

    /// The peer `name` names: its endpoint id (or a prefix of it), or the
    /// address book's name for it, or what it calls itself.
    async fn resolve(&self, mesh: &Mesh, name: &str) -> Result<String, String> {
        let want = name.trim().to_lowercase();
        if want.is_empty() {
            return Err("name a peer".into());
        }
        let peers = mesh.peers().await;
        let hits: Vec<&PeerView> = peers
            .iter()
            .filter(|p| {
                p.endpoint.starts_with(&want)
                    || p.name.to_lowercase().contains(&want)
                    || p.about.as_ref().is_some_and(|a| a.name.to_lowercase().contains(&want))
            })
            .collect();
        match hits.as_slice() {
            [one] => Ok(one.endpoint.clone()),
            [] => Err(format!("no peer of this account is called {name:?}")),
            many => Err(format!(
                "{name:?} could be any of {}",
                many.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
            )),
        }
    }
}

impl crate::wire::control::PeersControl for Node {
    async fn status(&self) -> crate::PeersStatus {
        self.refresh_roles();
        let Some(mesh) = self.mesh() else {
            return crate::PeersStatus {
                up: false,
                why: lock(&self.inner.why).clone(),
                account: lock(&self.inner.account).clone(),
                ..crate::PeersStatus::default()
            };
        };
        let peers = mesh
            .peers()
            .await
            .into_iter()
            .map(|p| crate::PeerRow {
                endpoint: p.endpoint,
                name: p.name,
                platform: p.platform,
                reachable: p.about.is_some(),
                about: p.about.unwrap_or_default(),
            })
            .collect();
        crate::PeersStatus {
            up: true,
            why: String::new(),
            id: mesh.id(),
            about: mesh.about(),
            now: mesh.now(),
            peers,
            account: lock(&self.inner.account).clone(),
        }
    }

    async fn follow(&self, peer: String) -> String {
        let Some(mesh) = self.mesh() else {
            return format!("the mesh is not up: {}", lock(&self.inner.why));
        };
        match self.resolve(&mesh, &peer).await {
            Ok(endpoint) => {
                self.set_auto_follow(false);
                mesh.follow(&endpoint, self.inner.target.clone());
                tracing::info!(peers.follow = %endpoint, "peers: following");
                String::new()
            }
            Err(why) => why,
        }
    }

    async fn unfollow(&self) {
        self.set_auto_follow(false);
        if let Some(mesh) = self.mesh() {
            mesh.unfollow();
        }
    }

    async fn publish(&self, now: NowPlaying) {
        {
            let mut window = lock(&self.inner.window);
            window.0.tracks = true;
            window.1 = now_ms();
        }
        self.refresh_roles();
        if let Some(mesh) = self.mesh() {
            mesh.publish(now);
        }
    }

    async fn set_cues(&self, cues: crate::RigCues) -> String {
        match self.inner.target.set_cues(&cues) {
            Ok(()) => String::new(),
            Err(why) => why,
        }
    }

    async fn set_roles(&self, roles: Roles) {
        {
            let mut window = lock(&self.inner.window);
            window.0 = roles;
            window.1 = now_ms();
        }
        self.refresh_roles();
    }
}

impl Node {
    /// [`PeersControl`](crate::PeersControl) over this node, to merge into
    /// the engine's local router.
    #[must_use]
    pub fn router(&self) -> LayerRouter {
        use architect::Layer as _;
        LayerRouter::new()
            .merge_router(architect::layers![crate::wire::control::Service].provide(self.clone()))
    }
}
