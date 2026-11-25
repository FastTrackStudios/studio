//! `studio-mcp`: Studio's MCP over stdio, for an agent.
//!
//! ```text
//! studio-mcp [--app NAME] [--socket PATH] [--wait SECONDS] [--budget BYTES]
//! ```
//!
//! `--app` the app whose control socket to use (`studio`), `--socket` its
//! path outright (also `STUDIO_MCP_SOCKET`), `--wait` how long a call is
//! waited for before it is answered as a job (20), `--budget` the most
//! JSON one answer carries before it pages (24000). Logs nothing to
//! stdout, which is the protocol's.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use studio_mcp::server::{self, Notify};
use studio_mcp::upstream::{self, Socket};
use studio_mcp::{Bridge, Config};

fn main() -> std::process::ExitCode {
    let mut app = "studio".to_owned();
    let mut socket: Option<PathBuf> = None;
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_default();
        match a.as_str() {
            "--app" => app = value(),
            "--socket" => socket = Some(PathBuf::from(value())),
            "--wait" => {
                if let Ok(s) = value().parse::<f64>() {
                    config.wait = Duration::from_secs_f64(s.clamp(0.0, 600.0));
                }
            }
            "--budget" => {
                if let Ok(b) = value().parse::<usize>() {
                    config.budget = b.max(4000);
                }
            }
            "-h" | "--help" => {
                let _ = std::io::Write::write_all(
                    &mut std::io::stderr(),
                    b"studio-mcp [--app NAME] [--socket PATH] [--wait SECONDS] [--budget BYTES]\n\
                      Studio's MCP over stdio, on the running app's control socket.\n",
                );
                return std::process::ExitCode::SUCCESS;
            }
            _ => {}
        }
    }
    let env = |k: &str| std::env::var(k).ok();
    let path = upstream::socket_path(&app, socket.as_deref(), &env);
    // A call waits this long for Studio to come (back) up.
    let up = Arc::new(Socket::new(path, Duration::from_secs(15)));
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let to = out.clone();
    let notify: Notify = Arc::new(move |v| server::write_line(&to, &v));
    let bridge = Arc::new(Bridge::new(up, config, notify));
    match server::serve_stdio(&bridge, &out) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
