//! `studio.rig`: Signal's rig engine as an Architect extension.
//!
//! The process runs what `studio-desktop --engine` runs
//! (the old rig app's `engine_main.rs`): the guitar and keys
//! backends on one `signal_rig_host::Engine` (one project, one device:
//! `docs/design/one-engine.md`), their vox services on one router, served
//! on loopback at a free port. It says where it is
//! (`studio_rig_link::discover`), and Studio's rig views, extensions of
//! their own, are vox clients of it as every Signal GUI is. It draws
//! nothing.
//!
//! Every control is an action ([`actions`]): the process is the engine,
//! so they run with no window open.
//!
//! The guitar's audio opens at start, as the headless engine's does,
//! unless `STUDIO_RIG_AUTOSTART=0` (then `STUDIO_RIG_START` opens it).
//! `SIGNAL_RIG_DIR`, `SIGNAL_RIG_DESIGN`, `SIGNAL_RIG_SILENT` and
//! `SIGNAL_RIG_EPHEMERAL` mean what they mean to any rig engine
//! (`guitar::library`).

pub mod actions;

use std::sync::OnceLock;

use architect::host::EngineHost;
use architect::rig::RigBackend as _;
use architect_extensions::linked::{Builder, Extension};
use guitar::GuitarRigBackend;
use keys::KeysRigBackend;

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The rigs, once the engine's thread made them.
pub(crate) struct Rigs {
    pub guitar: GuitarRigBackend,
    pub keys: KeysRigBackend,
}

static RIGS: OnceLock<Rigs> = OnceLock::new();
/// Where the engine serves, or why it does not.
static SERVING: OnceLock<Result<String, String>> = OnceLock::new();

/// The rigs, when the engine is up.
pub(crate) fn rigs() -> Result<&'static Rigs, String> {
    RIGS.get().ok_or_else(|| match SERVING.get() {
        Some(Err(why)) => format!("the rig engine did not start: {why}"),
        _ => "the rig engine is starting".to_owned(),
    })
}

/// The engine's vox URL, once it serves.
#[must_use]
pub fn url() -> Option<String> {
    SERVING.get().and_then(|s| s.as_ref().ok().cloned())
}

/// Whether the guitar's audio opens as the engine starts.
fn autostart() -> bool {
    std::env::var("STUDIO_RIG_AUTOSTART").map_or(true, |v| {
        let v = v.trim();
        !(v == "0" || v.eq_ignore_ascii_case("false"))
    })
}

/// Starts the engine on a thread of its own (it serves until the process
/// ends).
pub fn start_engine() {
    let spawned = std::thread::Builder::new()
        .name("rig-engine".to_owned())
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("rig-engine")
                .build();
            match runtime {
                Ok(runtime) => runtime.block_on(async {
                    if let Err(error) = serve().await {
                        tracing::error!(%error, "studio.rig: the engine stopped");
                        let _ = SERVING.set(Err(error));
                    }
                }),
                Err(error) => {
                    let _ = SERVING.set(Err(error.to_string()));
                }
            }
        });
    if let Err(error) = spawned {
        let _ = SERVING.set(Err(error.to_string()));
    }
}

/// The rigs' services on one router: each rig's own, the shared RigCore
/// and performance surfaces under each rig's scope, the engine's daw, the
/// sample packs and TONE3000.
fn router(guitar: &GuitarRigBackend, keys: &KeysRigBackend) -> architect::LayerRouter {
    use architect::Layer as _;
    use signal_rigs_proto::Rig;
    use signal_rigs_proto::rig_core::prelude::RigCoreService;

    let router = guitar.router().merge_router(keys.router());
    let router = router.merge_router_scoped(
        Rig::Guitar.slug(),
        architect::layers![RigCoreService].provide(guitar.clone()),
    );
    let router = signal_rigs_proto::mount_perform!(router, Rig::Guitar, guitar.clone());
    let router = router.merge_router_scoped(
        Rig::Keys.slug(),
        architect::layers![RigCoreService].provide(keys.clone()),
    );
    // The engine's daw: its project, every rig's tracks, transport, peaks.
    let router = match signal_rig_host::engine::shared() {
        Some(engine) => {
            use architect::Services as _;
            router.merge_router(engine.daw().clone().into_router())
        }
        None => router,
    };
    let packs = sampler::library::PackLibraryBackend::new();
    let router = router.merge_router(packs.router());
    // TONE3000: the session and the fetching live on the engine.
    let config = sampler::rig_prefs::signal_config_dir();
    let account = std::sync::Arc::new(signal_tone3000::account::Account::new(
        signal_tone3000::account::AccountConfig::from_env(&config),
    ));
    let tone3000 =
        signal_tone3000::client::Tone3000Backend::new(signal_tone3000::client::Config::from_env(
            &config,
            signal_nam::nam_root_from_env(&config.join("nam")),
        ))
        .with_account(account.clone());
    let router = router.merge_router(tone3000.router());
    router.merge_router(signal_tone3000::account::AccountBackend::new(account).router())
}

/// Builds the rigs and serves them until the process ends.
async fn serve() -> Result<(), String> {
    // One engine for every rig in the process: the rigs join it as folders
    // of tracks, and the guitar opens its device with its audio settings.
    signal_rig_host::engine::install_shared(signal_rig_host::engine::Engine::new("Signal"));
    let guitar = GuitarRigBackend::new();
    let keys = KeysRigBackend::new();
    let router = router(&guitar, &keys);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| format!("no loopback port: {e}"))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let url = format!("ws://{addr}/vox");
    // Ungated: bound to loopback on a port the OS picks, and handed only
    // to this process's own windows, so only this machine reaches it
    // (studio#362).
    let app = EngineHost::ungated(router, addr.to_string()).into_app();
    let autostart = autostart();
    if autostart {
        guitar::proto::rig::Rig::start(&guitar);
    }
    let _ = RIGS.set(Rigs { guitar, keys });
    let published = studio_rig_link::discover::publish(&url);
    tracing::info!(
        rig.url = %url,
        rig.autostart = autostart,
        rig.published = published.is_some(),
        "studio.rig: the engine serves"
    );
    let _ = SERVING.set(Ok(url));
    axum::serve(listener, app).await.map_err(|e| e.to_string())
}

pub struct RigEngine;

impl Extension for RigEngine {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        actions::register(ext);
    }
}
