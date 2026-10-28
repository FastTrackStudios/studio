//! `studio engine` (and `studio-desktop --session-engine`): Session's whole
//! engine, headless, in this process.
//!
//! The engine a window runs (`session_ui::studio_engine::headless`): one
//! `Standalone`, the set's one audio engine, the set itself, and the Action
//! List on MCP (127.0.0.1:47811, `FTS_MCP_PORT`) — so `studio open`,
//! `studio songs`, `studio song next` and `studio run …` drive it exactly
//! as they drive a window. Besides that, the daw facade over vox:
//!
//! - on a Unix socket (mode 0600), `$XDG_RUNTIME_DIR/fts/engine-<pid>.sock`
//!   by default (`--socket` names another) — what a local GUI attaches to;
//! - with `--listen ADDR`, over WebSocket too (`ws://ADDR/vox`), for a phone,
//!   a browser or another machine. Off unless asked: it is the network.
//!
//! Where it is goes in `$XDG_CONFIG_HOME/fts/engines/studio-<pid>.json`
//! (pid, socket, MCP and WebSocket addresses), removed when it stops
//! (Ctrl-C or SIGTERM). See `docs/design/engine-boundary.md`.
//!
//! ```text
//! studio engine &                                   # the engine, no window
//! studio open "JT's Senior Recital" --org fasttrackaudio
//! studio songs
//! studio run SESSION_PLAY_PAUSE
//! studio run SESSION_AUDIO
//! ```

use std::path::{Path, PathBuf};

use session_ui::studio_engine::link::{Found, publish};
use session_ui::studio_engine::{headless, set};

/// What the engine is asked to do as it comes up.
#[derive(Clone, Debug, Default)]
pub struct Args {
    /// Where to serve local clients (a Unix socket);
    /// `$XDG_RUNTIME_DIR/fts/engine-<pid>.sock` when `None`.
    pub socket: Option<PathBuf>,
    /// Also serve over WebSocket on this address: `ws://ADDR/vox`.
    pub listen: Option<String>,
    /// Open this as the engine comes up: a path (a song, `.session` or
    /// setlist), or a library setlist's title.
    pub open: Option<String>,
    /// The org whose library holds `open`'s setlist.
    pub org: Option<String>,
    /// One song of `open`'s setlist to open alone.
    pub song: Option<String>,
    /// Open what the last engine had open (an engine started again after
    /// one died): `last-open.json` beside the engines' files.
    pub reopen: bool,
}

impl Args {
    /// The arguments after `--session-engine` (the app run as its own
    /// engine): `--socket P`, `--listen A`, `--open X`, `--org O`,
    /// `--song S`, `--reopen`.
    #[must_use]
    pub fn parse(args: &[String]) -> Self {
        let mut out = Self::default();
        let mut at = args.iter();
        while let Some(arg) = at.next() {
            match arg.as_str() {
                "--socket" => out.socket = at.next().map(PathBuf::from),
                "--listen" => out.listen = at.next().cloned(),
                "--open" => out.open = at.next().cloned(),
                "--org" => out.org = at.next().cloned(),
                "--song" => out.song = at.next().cloned(),
                "--reopen" => out.reopen = true,
                _ => {}
            }
        }
        out
    }
}

/// Run the engine until Ctrl-C or SIGTERM.
///
/// Blocking, on the engine's own runtime: call it from a thread that is not
/// inside another tokio runtime.
///
/// # Errors
///
/// The engine did not start, or its socket could not be bound.
pub fn run(args: &Args) -> eyre::Result<()> {
    let engine = headless::start()?;
    let router = {
        let _inside = engine.runtime.enter();
        session_ui::studio_engine::view_host::router(&engine.daw)
    };
    let socket = args.socket.clone().unwrap_or_else(default_socket);
    let listener = {
        let _inside = engine.runtime.enter();
        bind(&socket)?
    };
    engine.runtime.spawn(serve_socket(listener, router.clone()));
    if let Some(addr) = &args.listen {
        let host = architect::host::EngineHost::new(router, addr.clone());
        let addr = addr.clone();
        engine.runtime.spawn(async move {
            if let Err(e) = host.serve().await {
                tracing::warn!(engine.ws = %addr, error = %e, "engine: the WebSocket stopped");
            }
        });
    }
    let found = Found {
        pid: std::process::id(),
        socket: socket.clone(),
        mcp: mcp_url(),
        ws: args.listen.as_ref().map(|a| format!("ws://{a}/vox")),
        build: session_ui::studio_engine::link::build_id(),
    };
    let discovery = publish(&found);
    tracing::info!(
        engine.pid = found.pid,
        engine.socket = %socket.display(),
        engine.mcp = %found.mcp,
        engine.ws = found.ws.as_deref().unwrap_or("off"),
        "engine: serving"
    );
    if let Some(what) = &args.open {
        open_first(what, args.org.clone(), args.song.clone());
    } else if args.reopen {
        session_ui::studio_engine::link::reopen_last();
    }
    engine.runtime.block_on(stopped());
    tracing::info!("engine: stopping");
    let _ = std::fs::remove_file(&socket);
    if let Some(file) = discovery {
        let _ = std::fs::remove_file(file);
    }
    Ok(())
}

/// Where an engine serves local clients unless told: in the user's runtime
/// directory (private to them), else `/tmp`.
fn default_socket() -> PathBuf {
    let pid = std::process::id();
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map_or_else(
            || PathBuf::from(format!("/tmp/fts-engine-{pid}.sock")),
            |dir| {
                PathBuf::from(dir)
                    .join("fts")
                    .join(format!("engine-{pid}.sock"))
            },
        )
}

/// The socket, bound and readable by this user only; one a dead engine
/// left behind is replaced.
fn bind(path: &Path) -> eyre::Result<tokio::net::UnixListener> {
    use std::os::unix::fs::PermissionsExt as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    let listener = tokio::net::UnixListener::bind(path)
        .map_err(|e| eyre::eyre!("binding {}: {e}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Every client that connects on `listener` served `router` — each lane it
/// opens dispatched by method id, as REAPER's bridge serves its socket.
async fn serve_socket(listener: tokio::net::UnixListener, router: daw::LayerRouter) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(serve_client(stream, router.clone()));
            }
            Err(e) => {
                tracing::warn!(error = %e, "engine: the socket stopped accepting");
                return;
            }
        }
    }
}

/// One client, until it goes.
async fn serve_client(stream: tokio::net::UnixStream, router: daw::LayerRouter) {
    let link = vox_stream::StreamLink::unix(stream);
    let lanes = vox::lane_acceptor_fn(move |_request, connection| {
        connection.handle_with(router.clone());
        Ok(())
    });
    match vox::acceptor_on(link)
        .on_lane(lanes)
        .establish_connection()
        .await
    {
        // The connection serves its lanes while it is held: hold it until
        // the client hangs up, then let the task end.
        Ok(connection) => connection.closed().await,
        Err(e) => tracing::warn!(error = ?e, "engine: a client's handshake failed"),
    }
}

/// Where the Action List is served, as `session_ui::studio_engine::mcp`
/// serves it.
fn mcp_url() -> String {
    let port = std::env::var("FTS_MCP_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(session_ui::studio_engine::mcp::DEFAULT_PORT);
    format!("http://127.0.0.1:{port}/mcp")
}

/// Open `what` as the engine's set: a path that exists, else a library
/// setlist's title.
fn open_first(what: &str, org: Option<String>, song: Option<String>) {
    let path = Path::new(what);
    let asked = if path.exists() {
        set::open_path(path.to_path_buf())
    } else {
        set::open_library(set::LibraryOpen {
            org,
            setlist: what.to_owned(),
            song,
        })
    };
    if let Err(busy) = asked {
        tracing::warn!(error = %busy, "engine: --open not taken");
    }
}

/// Until Ctrl-C or SIGTERM.
async fn stopped() {
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = terminate => {}
    }
}
