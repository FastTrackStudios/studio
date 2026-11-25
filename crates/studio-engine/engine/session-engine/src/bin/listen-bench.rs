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
//! answer. Each call prints one JSON line on stdout: the action, its wall
//! time in ms, and its answer (or its error).
//!
//! Two lines stand in for what a person does in Studio:
//! `@play <from seconds> <seconds>` plays the song from there on a thread
//! of its own, block by block at real time — what the audio device's
//! callback does — and prints when its first audible block came (`@play`,
//! `first_audio_ms` from the play, `since_open_ms` from the open's start);
//! `@wait <seconds>` lets time pass. With `RUST_LOG=info` every span's
//! time goes to stderr as it closes.

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
    let mut players = Vec::new();
    for line in script.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("@play ") {
            let mut words = rest
                .split_whitespace()
                .filter_map(|w| w.parse::<f64>().ok());
            let (from, seconds) = (words.next().unwrap_or(0.0), words.next().unwrap_or(5.0));
            players.push(play(from, seconds, opening)?);
            continue;
        }
        if let Some(rest) = line.strip_prefix("@wait ") {
            let seconds: f64 = rest.trim().parse().unwrap_or(1.0);
            std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
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
    for p in players {
        let _ = p.join();
    }
    Ok(())
}

/// Play the open song from `from` for `seconds`, block by block at real
/// time on a thread of its own: the audio callback's renders, with no
/// device.
fn play(from: f64, seconds: f64, opening: Instant) -> eyre::Result<std::thread::JoinHandle<()>> {
    use daw::service::ProjectContext;
    use daw_standalone::audio_engine::render::ProjectRenderer;
    const RATE: u32 = 48_000;
    const BLOCK: usize = 512;
    let song =
        session_engine::song::Song::named_or_current(None).map_err(|e| eyre::eyre!("{e:?}"))?;
    let ProjectContext::Project(guid) = song.context() else {
        eyre::bail!("no song open");
    };
    let daw = song.daw();
    Ok(std::thread::Builder::new()
        .name("bench-play".into())
        .spawn(move || {
            let renderer = ProjectRenderer::new(daw, &guid, RATE);
            // As the audio callback's: its lane pool, never waiting on a lock.
            renderer.set_realtime();
            let began = Instant::now();
            let start = (from * f64::from(RATE)) as u64;
            let blocks = (seconds * f64::from(RATE) / BLOCK as f64) as u64;
            let mut heard = false;
            for b in 0..blocks {
                let block = renderer.render_block(start + b * BLOCK as u64, BLOCK);
                if !heard && block.samples.iter().any(|s| s.abs() > 1e-5) {
                    heard = true;
                    let out = serde_json::json!({
                        "action": "@play",
                        "first_audio_ms": began.elapsed().as_secs_f64() * 1e3,
                        "since_open_ms": opening.elapsed().as_secs_f64() * 1e3,
                        "block": b,
                    });
                    println!("{out}");
                }
                let due = (b + 1) as f64 * BLOCK as f64 / f64::from(RATE);
                let ahead = due - began.elapsed().as_secs_f64();
                if ahead > 0.0 {
                    std::thread::sleep(std::time::Duration::from_secs_f64(ahead));
                }
            }
        })?)
}
