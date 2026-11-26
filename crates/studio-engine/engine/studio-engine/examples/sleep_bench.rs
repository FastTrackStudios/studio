//! Plugin sleep, measured on a copy of a song: its actions (renders among
//! them, which are async) run headless, timed, one JSON line each.
//!
//! ```text
//! cargo run -p session-engine --features native --example sleep_bench -- <song.session> <script>
//! ```
//!
//! `script` holds one call a line, `ACTION {json args}` (`#` starts a
//! comment). Run it twice, once with `FTS_PLUGIN_SLEEP=0` (every plugin
//! called every block) and once without, and null the WAVs the renders
//! write; with `RUST_LOG=warn,daw_standalone::audio_engine::render=info`
//! each offline render's `render.offline` event (its `render.sleep_skipped`
//! and `render.sleep_saved_ms`, its `render.slowest_fx`) goes to stderr.

use std::path::PathBuf;
use std::time::Instant;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [song, script] = args.as_slice() else {
        eyre::bail!("usage: sleep_bench <song.session> <script>");
    };
    let script = std::fs::read_to_string(script)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let opening = Instant::now();
    studio_engine::open::open_silent(song)?;
    println!(
        "{}",
        serde_json::json!({"action": "OPEN", "ms": opening.elapsed().as_secs_f64() * 1e3})
    );
    let list = studio_engine::actions::list();
    for line in script.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (id, json) = line.split_once(' ').unwrap_or((line, "{}"));
        let began = Instant::now();
        let answer = runtime.block_on(list.run_json_async(id, json.trim().as_bytes()));
        let ms = began.elapsed().as_secs_f64() * 1e3;
        let out = match answer {
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
