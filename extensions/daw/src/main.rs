//! `daw-extension`: the engine extension's process.
//!
//! - With no arguments it serves the host that started it (the
//!   manifest's `[entry]`), on a runtime of its own so the engine's
//!   threads can reach the host. `FTS_DAW_OPEN=<file.session>` opens a
//!   session as it starts.
//! - `import <project.rpp> [--song <name>] [--media-root <dir>] --out
//!   <file.session>` makes a session from a REAPER project: one song of a
//!   set, between its marker and the next `SONG END`.
//! - `play <file.session> [seconds]` plays a session with no host and
//!   prints the transport and the loudest meter twice a second: the
//!   engine's own smoke test.

use std::path::PathBuf;

use daw_extension::{Daw, RUNTIME, doc_of, import, open, rpp, save};

fn usage() -> String {
    "usage: daw-extension [import <project.rpp> [--song <name>] [--media-root <dir>] --out <file.session>]".to_owned()
}

fn import_cmd(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let source = PathBuf::from(args.next().ok_or_else(usage)?);
    let (mut song, mut root, mut out) = (None, None, None);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or_else(usage);
        match a.as_str() {
            "--song" => song = Some(value()?),
            "--media-root" => root = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            _ => return Err(usage()),
        }
    }
    let out = out.ok_or_else(usage)?;
    let text =
        std::fs::read_to_string(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let project = rpp::read(&text)?;
    let media_root = root
        .or_else(|| source.parent().map(PathBuf::from))
        .unwrap_or_default();
    let name = song.clone().unwrap_or_else(|| {
        source
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    });
    let options = import::Options { song, media_root };
    let (session, report) = import::session_of(&project, &name, &options)?;
    save(&doc_of(&session)?, &out)?;
    println!(
        "{}: {} tracks, {} clips; {} without a source, {} sources not found",
        out.display(),
        report.tracks,
        report.items,
        report.no_source,
        report.missing.len()
    );
    for m in report.missing.iter().take(10) {
        println!("  not found: {m}");
    }
    Ok(())
}

#[cfg(feature = "audio")]
fn play_cmd(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let path = PathBuf::from(args.next().ok_or_else(usage)?);
    let seconds: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5.0);
    open(&path)?;
    std::thread::sleep(std::time::Duration::from_millis(1500));
    daw_extension::engine::transport(daw_view::Transport::Play);
    let began = std::time::Instant::now();
    while began.elapsed().as_secs_f64() < seconds {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let (playing, position, duration, rate, problem, values) = daw_extension::engine::status();
        let loudest = values
            .as_deref()
            .and_then(|p| architect_scene::engine::shared::open(std::path::Path::new(p)).ok())
            .and_then(|t| {
                t.read(&architect_scene::anim::Name::new(daw_view::names::MASTER_PEAK).ok()?)
            });
        println!(
            "t={:.2}s playing={playing} position={position:.3}/{duration:.1} rate={rate} master={loudest:?} problem={problem:?}",
            began.elapsed().as_secs_f64()
        );
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    if let Some(cmd) = args.next() {
        return match cmd.as_str() {
            "import" => import_cmd(args),
            #[cfg(feature = "audio")]
            "play" => play_cmd(args),
            _ => Err(usage()),
        };
    }
    let socket = std::env::var_os(architect_extensions::proto::SOCKET)
        .ok_or("no host started this process (it is an Architect extension)")?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let _ = RUNTIME.set(rt.handle().clone());
    let guest = architect_extensions::guest::Guest::extension(&Daw);
    if let Some(path) = std::env::var_os("FTS_DAW_OPEN") {
        rt.spawn(async move {
            // Once the host has connected: the deck is the host's.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Err(e) =
                tokio::task::spawn_blocking(move || open(std::path::Path::new(&path))).await
            {
                let _ = e;
            }
        });
    }
    rt.block_on(guest.serve_at(std::path::Path::new(&socket)))
        .map_err(|e| format!("{e:?}"))
}
