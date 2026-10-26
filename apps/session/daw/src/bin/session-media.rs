//! The media a song plays — each item's active take, on a lane that
//! sounds — each file once, first heard first: what to make proxies of and
//! fetch before anything else (see `session_daw::song_stream::playing_files`).
//!
//! ```sh
//! session-media "Song/Song.session"   # a project file, an .RPP, or a folder
//! ```

use std::path::PathBuf;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let Some(song) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eyre::bail!("usage: session-media <song folder | .session | .RPP>");
    };
    for file in session_daw::song_stream::playing_files(&song)? {
        println!("{file}");
    }
    Ok(())
}
