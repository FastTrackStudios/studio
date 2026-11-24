//! `listen-bench` — actions run against a song opened headless, timed:
//! the listening tools (`MIX_STATE`, `LISTEN_HITS`, `LISTEN`, `LISTEN_AB`)
//! measured on a copy of a song, never the one Studio has open.
//!
//! ```sh
//! listen-bench <song.session> <script>
//! ```
//!
//! The song opens silent (no audio device, no MCP server). `script` holds
//! one call a line, `ACTION {json args}` (`#` starts a comment, a blank
//! line is skipped); `- ` before a line runs it without printing its
//! answer. Each call prints one JSON line on stdout: the action, its
//! wall time in ms, and its answer (or its error).

use std::path::PathBuf;
use std::time::Instant;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [song, script] = args.as_slice() else {
        eyre::bail!("usage: listen-bench <song.session> <script>");
    };
    let script = std::fs::read_to_string(script)?;
    let opening = Instant::now();
    session_engine::open::open_silent(song)?;
    let list = session_engine::actions::list();
    let out = serde_json::json!({
        "action": "OPEN",
        "ms": opening.elapsed().as_secs_f64() * 1e3,
    });
    println!("{out}");
    for line in script.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (quiet, line) = match line.strip_prefix("- ") {
            Some(rest) => (true, rest.trim()),
            None => (false, line),
        };
        let (id, json) = line.split_once(' ').unwrap_or((line, "{}"));
        let began = Instant::now();
        let answer = list.run_json(id, json.trim().as_bytes());
        let ms = began.elapsed().as_secs_f64() * 1e3;
        let out = match answer {
            Ok(bytes) if quiet => serde_json::json!({"action": id, "ms": ms, "bytes": bytes.len()}),
            Ok(bytes) => {
                let answer: serde_json::Value = serde_json::from_slice(&bytes)
                    .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned().into());
                serde_json::json!({"action": id, "ms": ms, "answer": answer})
            }
            Err(e) => serde_json::json!({"action": id, "ms": ms, "error": e.to_string()}),
        };
        println!("{out}");
    }
    Ok(())
}
