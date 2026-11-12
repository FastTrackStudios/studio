//! The device-family contract: what a family of devices gives the hub
//! (FastTrackStudios/studio#410, question 1).
//!
//! A family is one kind of device Patchbay routes: a Yamaha TF console,
//! an Antelope Galaxy32, the Dante network, this machine's own audio.
//! It knows how to find and connect one device of its kind
//! ([`DeviceFamily::connect`]); everything after that (supervision,
//! reconnects, events, the RPC, the CLI, the generic device view,
//! snapshots) is the hub's, generic over [`DynDeviceAdapter`].
//!
//! The hub runs over a [`Families`] registry. The built-in families are
//! registered by the server today; each becomes an extension of its own
//! in #410's stack, registering itself the same way.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::device::DynDeviceAdapter;
use crate::proto::{DeviceConfig, FamilyInfo};
use crate::server::DiscoveryCache;

/// A connected device, type-erased.
pub type Adapter = Arc<dyn DynDeviceAdapter>;

/// What [`DeviceFamily::connect`] returns.
pub type ConnectFuture = Pin<Box<dyn Future<Output = Result<Adapter, ConnectError>> + Send>>;

/// Why a connect attempt failed. Connects fail soft: they return this,
/// never panic, and never block the other devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// Discovery ran and found nothing (`device list` says *not found*).
    NotFound(String),
    /// Found, or pinned, but the connection failed (*offline*).
    Failed(String),
}

impl ConnectError {
    /// The reason, as `device list` shows it.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::NotFound(m) | Self::Failed(m) => m,
        }
    }
}

/// What the hub hands a family's connect: shared state every family may
/// use.
#[derive(Clone)]
pub struct ConnectCtx {
    /// Last-found addresses of auto-discovered devices.
    pub(crate) cache: Arc<DiscoveryCache>,
}

impl ConnectCtx {
    /// A context over `cache`.
    #[must_use]
    pub(crate) const fn new(cache: Arc<DiscoveryCache>) -> Self {
        Self { cache }
    }
}

/// One family of devices.
pub trait DeviceFamily: Send + Sync + 'static {
    /// The config `kind` this family connects (`yamaha-tf`,
    /// `antelope-galaxy32`, `dante`, `system-audio`). One family per kind.
    fn kind(&self) -> &'static str;

    /// What it is, where it runs, and whether it can run here.
    fn info(&self) -> FamilyInfo;

    /// The entries it adds to a config that has no `devices` section.
    fn defaults(&self) -> Vec<DeviceConfig> {
        Vec::new()
    }

    /// Find (when `config.addr` is empty) and connect one device. Fails
    /// soft, telling "found nothing" ([`ConnectError::NotFound`]) from
    /// "found but unreachable" ([`ConnectError::Failed`]).
    fn connect(&self, config: DeviceConfig, ctx: ConnectCtx) -> ConnectFuture;
}

/// The families the hub runs over, in registration order (which is the
/// order of the default device list).
#[derive(Clone, Default)]
pub struct Families {
    list: Vec<Arc<dyn DeviceFamily>>,
}

impl Families {
    /// No families.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `family`; one already registered for its kind is replaced, in
    /// place.
    #[must_use]
    pub fn with(mut self, family: impl DeviceFamily) -> Self {
        self.register(Arc::new(family));
        self
    }

    /// Add `family`; one already registered for its kind is replaced, in
    /// place.
    pub fn register(&mut self, family: Arc<dyn DeviceFamily>) {
        match self.list.iter_mut().find(|f| f.kind() == family.kind()) {
            Some(slot) => *slot = family,
            None => self.list.push(family),
        }
    }

    /// The family for config kind `kind`.
    #[must_use]
    pub fn find(&self, kind: &str) -> Option<&Arc<dyn DeviceFamily>> {
        self.list.iter().find(|f| f.kind() == kind)
    }

    /// Every registered kind, in order.
    #[must_use]
    pub fn kinds(&self) -> Vec<&'static str> {
        self.list.iter().map(|f| f.kind()).collect()
    }

    /// The default device list: each family's defaults, in order.
    #[must_use]
    pub fn defaults(&self) -> Vec<DeviceConfig> {
        self.list.iter().flat_map(|f| f.defaults()).collect()
    }

    /// What each family is, in order (`patchbay families`).
    #[must_use]
    pub fn infos(&self) -> Vec<FamilyInfo> {
        self.list.iter().map(|f| f.info()).collect()
    }
}
