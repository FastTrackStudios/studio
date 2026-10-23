//! `studio-desktop --engine` — the headless signal engine.
//!
//! The former `apps/signal-engine` binary, folded into the app: opens the
//! live audio engine (guitar in → NAM chain → out), loads the profile, and
//! mounts the rig's vox `LayerRouter` at `ws://<host>:4040/vox`. Remotes
//! (the fts web build served by this same process, other machines on the
//! network, the desktop app) drive it through the exact same generated
//! clients — the core cannot tell the difference. This mode never touches
//! dioxus/GUI and never bootstraps the in-process session engine.
//!
//! The transport/serving boilerplate — runtime, tracing, panic logger,
//! `/health`, `/vox`, iroh p2p, and the embedded SPA fallback — all lives in
//! [`architect::host`]; this file is just "build the rigs, mount the router,
//! serve." The browser remote is the studio-desktop WEB build, embedded with
//! `--features embed-web` (staged by `just rig-install` into `web-dist/`), so
//! the deployed engine is ONE artifact.
//!
//! Signal's command core (`signal_proto::commands`, run by the guitar rig —
//! `guitar::signal_commands`) is served to a model over MCP when asked for
//! (`--mcp`, or `SIGNAL_MCP=1`; `FTS_MCP=0` vetoes): on loopback only
//! (`127.0.0.1:47812`, `SIGNAL_MCP_PORT` to move it), behind a bearer token
//! minted at start. Where it is and the token are written to
//! `~/.config/fts/signal-mcp.json` (mode 0600), never to the log.

use architect::host::{self, EngineHost, WebBundle};
use architect::rig::RigBackend as _;
use guitar::GuitarRigBackend;
use guitar::proto::rig::Rig as _;

/// Default bind address; override with `SIGNAL_ENGINE_ADDR` (or the legacy
/// `RIGD_ADDR`, still honored so existing live setups keep working).
const DEFAULT_ADDR: &str = "0.0.0.0:4040";

/// Bind address: `SIGNAL_ENGINE_ADDR` wins, then the legacy `RIGD_ADDR`,
/// then the default.
fn bind_addr() -> String {
    std::env::var("SIGNAL_ENGINE_ADDR")
        .or_else(|_| std::env::var("RIGD_ADDR"))
        .unwrap_or_else(|_| DEFAULT_ADDR.to_string())
}

/// Where the MCP face listens unless `SIGNAL_MCP_PORT` says otherwise
/// (Session's window is on 47811).
const DEFAULT_MCP_PORT: u16 = 47_812;

/// The MCP face's name: `~/.config/fts/<name>-mcp.json`.
const MCP_DISCOVERY_NAME: &str = "signal";

/// The port to serve MCP on, or `None` when it was not asked for: `--mcp`
/// among `args` or `SIGNAL_MCP=1` asks; `FTS_MCP=0` (every FTS app's off
/// switch) wins over both.
///
/// # Errors
///
/// `SIGNAL_MCP_PORT` is set to something that is not a port.
fn mcp_port(args: &[String], env: impl Fn(&str) -> Option<String>) -> Result<Option<u16>, String> {
    let asked =
        args.iter().any(|a| a == "--mcp") || env("SIGNAL_MCP").is_some_and(|v| v.trim() == "1");
    let vetoed = env("FTS_MCP").is_some_and(|v| v.trim() == "0");
    if !asked || vetoed {
        return Ok(None);
    }
    env("SIGNAL_MCP_PORT")
        .filter(|p| !p.trim().is_empty())
        .map_or(Ok(DEFAULT_MCP_PORT), |p| {
            p.trim()
                .parse()
                .map_err(|_| format!("SIGNAL_MCP_PORT={p:?} is not a port"))
        })
        .map(Some)
}

/// Serve Signal's commands on `guitar` over MCP when the command line or
/// environment asks for it, and say where (the discovery file). Failing to
/// serve is logged, never fatal: the rig plays on.
async fn start_mcp(guitar: &GuitarRigBackend) {
    let args: Vec<String> = std::env::args().collect();
    let port = match mcp_port(&args, |name| std::env::var(name).ok()) {
        Ok(Some(port)) => port,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(%error, "mcp: not served");
            return;
        }
    };
    let list = match guitar::signal_commands::command_list(guitar) {
        Ok(list) => list,
        Err(error) => {
            tracing::warn!(%error, "mcp: not served, the command list did not build");
            return;
        }
    };
    let actions = list.len();
    let token = guitar::signal_commands::mint_token();
    let serving = match guitar::signal_commands::serve_mcp(list, port, token).await {
        Ok(serving) => serving,
        Err(error) => {
            tracing::warn!(%error, port, "mcp: not served");
            return;
        }
    };
    match serving.publish(MCP_DISCOVERY_NAME) {
        Ok(path) => tracing::info!(
            mcp.url = %serving.url,
            mcp.actions = actions,
            mcp.discovery = %path.display(),
            "mcp: serving Signal's commands"
        ),
        Err(error) => tracing::warn!(
            %error,
            mcp.url = %serving.url,
            "mcp: served, but where (and its token) could not be written"
        ),
    }
}

/// The staged web bundle, compiled into the binary. `just rig-install`
/// copies `target/dx/studio-desktop/release/web/public` →
/// `apps/desktop/web-dist/` before the native build; without the
/// feature no staged dir is required.
#[cfg(feature = "embed-web")]
static EMBEDDED_WEB: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/web-dist");

/// Locate the browser-remote bundle so the engine serves it itself — any
/// device on the LAN opens `http://<host>:4040/` and gets the control UI.
/// First match wins:
///
/// 1. `SIGNAL_WEB_DIST` env var (explicit override)
/// 2. assets embedded in the binary (feature `embed-web` — deployed layout)
/// 3. `<exe_dir>/signal-web` (legacy deployed layout — bundle beside the binary)
/// 4. dx dev build output relative to the workspace `target/`:
///    `target/dx/studio-desktop/{release,debug}/web/public`
///
/// A directory candidate only counts if it contains an `index.html`. `None`
/// means headless (only `/health` + `/vox`).
fn web_bundle() -> Option<WebBundle> {
    if let Ok(dir) = std::env::var("SIGNAL_WEB_DIST") {
        let p = std::path::PathBuf::from(&dir);
        if p.join("index.html").is_file() {
            return Some(WebBundle::Dir(p));
        }
        tracing::warn!("SIGNAL_WEB_DIST={dir} has no index.html — ignoring");
    }

    #[cfg(feature = "embed-web")]
    {
        return Some(WebBundle::Embedded(&EMBEDDED_WEB));
    }

    #[cfg(not(feature = "embed-web"))]
    {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))?;
        let candidates = [
            // Legacy deployed layout: bundle beside the binary.
            exe_dir.join("signal-web"),
            // Dev: exe lives in target/{debug,release}; dx output is a
            // sibling under target/dx/.
            exe_dir.join("../dx/studio-desktop/release/web/public"),
            exe_dir.join("../dx/studio-desktop/debug/web/public"),
        ];
        candidates
            .into_iter()
            .find(|p| p.join("index.html").is_file())
            .map(WebBundle::Dir)
    }
}

/// If the desktop app spawned this engine (it sets `FTS_SUPERVISOR_PID`), poll
/// that pid and exit when it disappears — the cross-platform guarantee that
/// closing the app takes its engine down too, even when the app is `SIGKILLed`
/// and never gets to reap us. No-op when run standalone (systemd unit,
/// `fts signal engine`, or a manual `--engine`): no supervisor, nothing to
/// watch, so those lifetimes are unaffected.
fn spawn_parent_watchdog() {
    let Ok(raw) = std::env::var("FTS_SUPERVISOR_PID") else {
        return;
    };
    let Ok(pid) = raw.parse::<i32>() else {
        return;
    };
    tracing::info!("supervised by pid {pid}; parent-death watchdog armed");
    std::thread::Builder::new()
        .name("parent-watchdog".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                // kill(pid, 0) probes liveness without signalling: 0 = alive;
                // ESRCH = the process is gone; EPERM = it exists (just not
                // ours to signal). Only ESRCH means the supervisor died.
                #[cfg(unix)]
                {
                    if unsafe { libc::kill(pid, 0) } != 0
                        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
                    {
                        tracing::warn!("supervisor {pid} gone — engine exiting");
                        std::process::exit(0);
                    }
                }
            }
        })
        .ok();
}

/// Entry point for `studio-desktop --engine`: builds the multi-thread tokio
/// runtime and never returns until the server dies.
pub fn run() {
    // Console logs (RUST_LOG-filtered fmt, same as host::init_tracing) plus
    // telemetry: Sentry when TASK_SENTRY_DSN is set, and OTLP export of
    // traces/logs/metrics when OTEL_EXPORTER_OTLP_ENDPOINT is set
    // (http/protobuf → the local collector on :4318). Both guards must live
    // for the whole process — dropping them flushes and stops the exporters.
    let (_sentry, _otel) = architect_telemetry::init_tracing_full("signal-engine", "info");
    // One panic hook, the same flight recorder the app window installs:
    // each panic (thread, backtrace) appended to the engine's own
    // `<data dir>/Signal/engine/crash.log`, beside the window's. Panics
    // stay unwinding — control-plane panics are caught and survived (the
    // rig's meter pump self-heals; audio keeps playing) — but none die
    // silently.
    app_shell::log_ring::install_panic_hook("Signal/engine");

    // If the desktop app spawned us (FTS_SUPERVISOR_PID), self-terminate when
    // it goes away — even if it was SIGKILLed and never reaped us.
    spawn_parent_watchdog();

    host::block_on(async_main());
}

async fn async_main() {
    // ── Rigs ──────────────────────────────────────────────────────────────
    // Every rig backend mounts its own service bundle onto ONE shared router
    // (`merge_router`) served at `/vox`; adding a rig (keys, …) is one more
    // backend + `.merge_router(rig.router())` here, plus a matching view in the
    // app. Each rig keeps its own dispatcher/PubSub — only the routing is
    // shared. Per-rig start policy differs (the worship guitar auto-opens
    // audio; other rigs stay dormant until a remote starts them), but the
    // wiring is identical.
    // One engine for every rig in the process (docs/design/one-engine.md):
    // the rigs join it as folders of tracks, and the guitar rig opens its
    // device with the guitar's audio settings.
    signal_rig_host::engine::install_shared(signal_rig_host::engine::Engine::new("Signal"));
    let guitar = GuitarRigBackend::new();
    guitar.start(); // worship rig: open audio + load profile off-thread
    // Signal's commands for a model, when asked (`--mcp` / `SIGNAL_MCP=1`).
    start_mcp(&guitar).await;
    let drums = drums::DrumRigBackend::new(); // dormant until started
    let keys = keys::KeysRigBackend::new(); // dormant until started
    let synth = signal_synth_rig::SynthRigBackend::new(); // dormant until started

    let router = guitar
        .router()
        .merge_router(drums.router())
        .merge_router(keys.router())
        .merge_router(synth.router());

    // The shared RigCore surface (start/stop/presets/load/MIDI) — ONE trait,
    // mounted once per rig under an instance scope. Clients pick a rig with
    // `architect::scope_client!(client, "keys")`; the per-rig protos keep
    // only what is genuinely that rig's own.
    // The scope strings come from the rig catalogue rather than being spelled
    // out here, so the name a rig answers to on the wire is the same one the
    // menus and `--rig` use — they used to agree only by convention.
    use architect::Layer as _;
    use signal_rigs_proto::Rig;
    use signal_rigs_proto::rig_core::prelude::RigCoreService;
    macro_rules! mount_core {
        ($router:expr, $rig:expr, $backend:expr) => {{
            let rig: Rig = $rig;
            debug_assert!(rig.has_rig_core(), "{} has no RigCore", rig.slug());
            $router.merge_router_scoped(
                rig.slug(),
                architect::layers![RigCoreService].provide($backend),
            )
        }};
    }
    let router = mount_core!(router, Rig::Guitar, guitar.clone());
    // The shared performance surface (presets, profiles, setlists) — the
    // same way, under each rig's scope.
    let router = signal_rigs_proto::mount_perform!(router, Rig::Guitar, guitar.clone());
    let router = signal_rigs_proto::mount_perform!(router, Rig::Drums, drums.clone());
    let router = mount_core!(router, Rig::Drums, drums.clone());
    let router = mount_core!(router, Rig::Keys, keys.clone());
    let router = mount_core!(router, Rig::Synth, synth.clone());

    // The engine's daw — its project, every rig's tracks, transport, peaks —
    // over the same /vox, so a daw client (Session's UI among them) sees
    // the rigs as the tracks they are.
    let router = match signal_rig_host::engine::shared() {
        Some(engine) => {
            use architect::Services as _;
            router.merge_router(engine.daw().clone().into_router())
        }
        None => router,
    };

    // ── Pack library ──────────────────────────────────────────────────────
    // Sample-pack distribution: serves the built `.signalpack` trees
    // (proxy + full) so remotes — the phone keys rig above all — can list
    // and download packs over the same `/vox` (and iroh p2p) endpoint.
    let packs = sampler::library::PackLibraryBackend::new();
    let router = router.merge_router(packs.router());

    // ── Content (everything a device can fetch from this one) ─────────────
    // The sample libraries at their own paths, and each rig's library under
    // `Signal/<rig>`: presets, profiles, songs, setlists, captures. A device
    // saves each file at its id, so what it fetches plays where it lands.
    let signal_dir = signal_rig_host::store::signal_config_dir();
    let content = signal_content_library::ContentLibraryBackend::with_roots(vec![
        (
            String::new(),
            std::env::var("FTS_PACK_LIBRARY")
                .unwrap_or_else(|_| "/run/media/AudioHaven/Signal/Libraries".into())
                .into(),
        ),
        ("Signal/guitar".into(), signal_dir.join("rig")),
        ("Signal/bass".into(), signal_dir.join("bass-rig")),
        ("Signal/nam".into(), signal_dir.join("nam")),
        ("Signal/drums".into(), signal_dir.join("drums")),
        ("Signal/ekit".into(), signal_dir.join("ekit")),
    ]);
    let router = router.merge_router(content.router());

    // ── TONE3000 (the public NAM/IR library) ──────────────────────────────
    // The session and the fetching live on the engine, not in a GUI: a
    // plugin editor cannot host an authorization page, and the browser
    // remote is served over plain HTTP on the LAN where WebCrypto (and so
    // PKCE) is unavailable. Every GUI is a courier for a URL in one
    // direction and a callback in the other.
    let config_dir_t3k = sampler::rig_prefs::signal_config_dir();
    // The FastTrackStudio account. Linking TONE3000 to it once means every
    // machine signed in to that account can download captures without its
    // own authorization — the issuer holds the refresh token and rotates it
    // in one place.
    let account = std::sync::Arc::new(signal_tone3000::account::Account::new(
        signal_tone3000::account::AccountConfig::from_env(&config_dir_t3k),
    ));
    let tone3000 =
        signal_tone3000::client::Tone3000Backend::new(signal_tone3000::client::Config::from_env(
            &config_dir_t3k,
            signal_nam::nam_root_from_env(&config_dir_t3k.join("nam")),
        ))
        .with_account(account.clone());
    let router = router.merge_router(tone3000.router());
    // The account's own sign-in RPC — a GUI's way to link this device to
    // auth.fasttrackstudio.app, so TONE3000 (and whatever else the account
    // gathers) works without a second, per-machine authorization. Wraps the
    // SAME `Arc<Account>` the callback route below redeems into, so a
    // sign-in finished in the browser is visible on the next `status()`
    // call with nothing else to keep in step.
    let account_rpc = signal_tone3000::account::AccountBackend::new(account.clone());
    let router = router.merge_router(account_rpc.router());

    // ── Sample space (similarity maps over the sample libraries, #77) ────
    let space = signal_space::engine::service::SpaceBackend::new();
    let router = router.merge_router(space.router());

    // ── Electronic Kit (pad grid over the sample space, #77) ─────────────
    let ekit = ekit::EkitBackend::new();
    let router = router.merge_router(ekit.router());
    let router = mount_core!(router, Rig::Ekit, ekit.clone());
    let router = signal_rigs_proto::mount_perform!(router, Rig::Ekit, ekit.clone());

    // ── Session (the setlist player) ─────────────────────────────────────
    // ── Watch bridge ──────────────────────────────────────────────────────
    // watchOS can't speak vox over WebSocket (TN3135), so the watch remote
    // gets a thin HTTP+SSE JSON surface (engine_watch.rs). The bridge is an
    // ordinary vox client over an in-process LocalServer against the SAME
    // router — the core can't tell a watch from a browser.
    let watch_scope = architect::Scope::new();
    let local = architect::LocalServer::serve(router.clone(), watch_scope.clone());
    let watch_routes = crate::engine_watch::router(local.clone()).await;

    // Serve the router over axum (`/vox` + `/health`) and iroh p2p, with the
    // browser remote as the HTTP fallback — all of it in `architect::host`.
    let config_dir = sampler::rig_prefs::signal_config_dir();
    let mut engine_host = EngineHost::new(router, bind_addr());
    if let Some(routes) = watch_routes {
        engine_host = engine_host.extend(routes);
        tracing::info!("watch bridge mounted at /watch/v1");
    }
    // The registered OAuth redirect lands on this same server (see
    // engine_tone3000.rs for why it can only be the engine).
    let engine_host = crate::engine_tone3000::extend(engine_host, tone3000);
    let engine_host = crate::engine_tone3000::extend_account(engine_host, account);
    engine_host
        .iroh(
            config_dir.join("iroh.key"),
            Some(config_dir.join("iroh-endpoint-id")),
        )
        .web(web_bundle())
        // COOP/COEP — the browser keys rig's audio path needs
        // SharedArrayBuffer (shared wasm memory + streamer threads, W13),
        // and the browser only grants it to a cross-origin-isolated page.
        // Safe here: the engine serves its own bundle and packs, so every
        // subresource is same-origin.
        .cross_origin_isolated(true)
        .serve()
        .await;
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_MCP_PORT, mcp_port};

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn mcp_is_off_unless_asked_for() {
        assert_eq!(mcp_port(&args(&["x", "--engine"]), env(&[])), Ok(None));
        assert_eq!(
            mcp_port(&args(&["x", "--engine"]), env(&[("SIGNAL_MCP", "0")])),
            Ok(None)
        );
    }

    #[test]
    fn the_flag_or_the_variable_asks_and_fts_mcp_0_vetoes() {
        let on = Ok(Some(DEFAULT_MCP_PORT));
        assert_eq!(mcp_port(&args(&["x", "--engine", "--mcp"]), env(&[])), on);
        assert_eq!(mcp_port(&args(&["x"]), env(&[("SIGNAL_MCP", "1")])), on);
        assert_eq!(
            mcp_port(&args(&["x", "--mcp"]), env(&[("FTS_MCP", "0")])),
            Ok(None)
        );
    }

    #[test]
    fn the_port_variable_moves_it_and_garbage_is_refused() {
        let asked = args(&["x", "--mcp"]);
        assert_eq!(
            mcp_port(&asked, env(&[("SIGNAL_MCP_PORT", "5000")])),
            Ok(Some(5000))
        );
        assert!(mcp_port(&asked, env(&[("SIGNAL_MCP_PORT", "nope")])).is_err());
    }
}
