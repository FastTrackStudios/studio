//! Bounce each song's reference: its whole mix but the guide, as one
//! stereo stream (see `session_engine::reference`).
//!
//! ```sh
//! reference "Song" ["Other Song" …]   # a song's folder, .session or .RPP
//! ```
//!
//! Or render one song's recording to an Ogg file: the track named by
//! `--track` alone (everything else muted), over the song's whole length
//! from 0:00 — the whole mix when the song has no such track or nothing on
//! it. Prints `track` or `mix`, then the seconds rendered.
//!
//! ```sh
//! reference --track "SONG REFERENCE" --to recordings/song.ogg "Song"
//! ```

use std::path::PathBuf;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,session_engine=info")),
        )
        .init();
    let mut track: Option<String> = None;
    let mut to: Option<PathBuf> = None;
    let mut songs: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--track") => {
                track = Some(
                    args.next()
                        .and_then(|a| a.into_string().ok())
                        .ok_or_else(|| eyre::eyre!("--track takes a track name"))?,
                );
            }
            Some("--to") => {
                to = Some(
                    args.next()
                        .map(PathBuf::from)
                        .ok_or_else(|| eyre::eyre!("--to takes a file"))?,
                );
            }
            _ => songs.push(PathBuf::from(arg)),
        }
    }
    if let Some(ogg) = to {
        let [song] = songs.as_slice() else {
            eyre::bail!("usage: reference [--track <name>] --to <file.ogg> <song>");
        };
        let (recorded, seconds) = session_engine::reference::record(song, track.as_deref(), &ogg)?;
        let what = match recorded {
            session_engine::reference::Recorded::Track(_) => "track",
            session_engine::reference::Recorded::Mix => "mix",
        };
        println!("{what}\t{seconds:.3}");
        return Ok(());
    }
    if track.is_some() {
        eyre::bail!("--track renders one song's recording: give it --to <file.ogg>");
    }
    if songs.is_empty() {
        eyre::bail!("usage: reference <song folder | .session | .RPP> …");
    }
    for song in songs {
        session_engine::reference::bounce(&song)?;
    }
    Ok(())
}
