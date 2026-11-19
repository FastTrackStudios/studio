//! Prepare a song once and save it as a `.session`.
//!
//! ```sh
//! prepare "Song/Song.RPP" [Song/Song.kf] [--as "Song Title"]
//! ```
//!
//! Organizes the multitrack, builds the song from its chart and generates
//! the click and guide — what the app does the first time it opens a
//! multitrack — and writes `Song/Song.session` beside it, which the app
//! then opens instead. The chart defaults to the one `.kf` beside the
//! project. One song per run: the engine's facade is process-wide.
//!
//! This is how a REAPER project is imported. With no chart, the song is
//! the project's own: its tempo map and meters, its regions as the
//! sections, its markers as marks. A record with its separation (the mix,
//! its stems, a click) comes in with the stems in instrument folders and
//! the mix in a top-level `Reference` folder, heard by the reference.
//! `--as` names the song when the project file's name is not its title
//! (`Last Christmas - Tempo Mapped.RPP`): the session is `<title>.session`.
//!
//! ```sh
//! prepare --recording "Song/Song.mp3" --as "Song Title" [--bpm 93]
//! ```
//!
//! A song that is only a recording: a session holding it as the song's
//! reference (`session_engine::prepare::reference_session`).

use std::path::PathBuf;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,session_engine=info")),
        )
        .init();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut title: Option<String> = None;
    let mut recording: Option<PathBuf> = None;
    let mut bpm: Option<f64> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--as" {
            title = args.next().map(|t| t.to_string_lossy().into_owned());
        } else if arg == "--recording" {
            recording = args.next().map(PathBuf::from);
        } else if arg == "--bpm" {
            bpm = args.next().and_then(|b| b.to_string_lossy().parse().ok());
        } else {
            files.push(PathBuf::from(arg));
        }
    }
    if let Some(recording) = recording {
        let title = title.ok_or_else(|| eyre::eyre!("--recording needs --as <title>"))?;
        let saved = session_engine::prepare::reference_session(&recording, &title, bpm)?;
        tracing::info!(session.saved = %saved.display(), "prepared");
        return Ok(());
    }
    let mut files = files.into_iter();
    let rpp = files
        .next()
        .ok_or_else(|| eyre::eyre!("usage: prepare <Song.RPP> [Song.kf] [--as <title>]"))?;
    let chart = files
        .next()
        .or_else(|| session_engine::prepare::chart_beside(&rpp));
    let saved = session_engine::prepare::prepare_and_save_as(&rpp, chart, title.as_deref())?;
    tracing::info!(session.saved = %saved.display(), "prepared");
    Ok(())
}
