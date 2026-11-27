//! The session commands the old `studio` binary (the rig app, now removed) mounted,
//! which puts them at its top level.
//!
//! The commands people use most drive the window that is running —
//! Studio's, or Session's own — through its Action List over MCP
//! ([`app`]): `studio open`, `studio songs`, `studio song`, `studio
//! actions`, `studio run`. The rest work on files (proxies, peaks, import,
//! lyrics, the vault), on a real REAPER ([`reaper`]), or play a setlist in
//! a terminal ([`player`]).
//!
//! ```text
//! studio open Song.session                     # open a song, setlist or .session in the running app
//! studio open Set.setlist --bench               # a setlist, timed
//! studio songs                                 # the open set (SESSION_SONGS)
//! studio song next                             # go to a song (SESSION_SONG)
//! studio actions --filter drums                # what the window can do
//! studio run SESSION_PLAY                      # run one action, args as JSON
//! studio engine                                # the whole engine, no window
//! ```
//!
//! `studio engine` ([`engine`]) runs Session's engine with no window and
//! serves the same Action List, so every command above drives it the same
//! way it drives a window.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Subcommand;

pub mod app;
pub mod bench;
#[cfg(feature = "native")]
pub mod engine;
mod lyrics;
mod multitracks;
mod peaks;
pub mod player;
mod proxies;
pub mod reaper;
pub mod vault_sync;

/// Session's commands, for a command line to put where it likes (`studio`
/// flattens them into its top level).
#[derive(Subcommand)]
pub enum Command {
    /// Open a song, setlist or `.session` in the running app.
    Open {
        /// The song, setlist or `.session` to open.
        what: PathBuf,
        /// Time it: poll the window until every song is open (and local,
        /// when the set streams in), then print the timings as one JSON
        /// object. Exits non-zero if `--bench-timeout` passes first.
        #[arg(long)]
        bench: bool,
        /// How long `--bench` waits for the set, in seconds.
        #[arg(long, default_value_t = 60.0, requires = "bench")]
        bench_timeout: f64,
    },
    /// The songs of the set open in the running app (SESSION_SONGS).
    Songs,
    /// Go to a song of the open set (SESSION_SONG): its number (1 the
    /// first), its title, `next` or `previous`.
    Song { to: String },
    /// Run Session's whole engine here, with no window: the set, the audio,
    /// the Action List on MCP (what `open`, `songs`, `song` and `run` talk
    /// to) and the daw facade on a Unix socket (`--listen` adds a
    /// WebSocket). Until Ctrl-C.
    #[cfg(feature = "native")]
    Engine(engine::Args),
    /// The running app's actions, by id (`--filter` narrows them).
    Actions {
        #[arg(long)]
        filter: Option<String>,
    },
    /// Run one action in the running app: its id (`SESSION_PLAY`, `play`)
    /// and its arguments as JSON.
    Run {
        action: String,
        args: Option<String>,
    },
    /// Write Ogg Vorbis proxies of a session's media: `Media/Bass.wav` →
    /// `Media/Proxies/Bass.ogg`, beside what they stand in for, so they
    /// sync with the session. What a browser or a phone plays from.
    ///
    /// Reads the sources from the `.RPP`; a proxy newer than its source is
    /// kept unless `--force`.
    Proxies {
        /// The session's `.RPP` (or the folder holding exactly one).
        session: PathBuf,
        /// libvorbis quality, -0.2..=1.0 (0.4 ≈ 128 kbps stereo).
        #[arg(long, default_value_t = 0.4)]
        quality: f32,
        /// Rewrite proxies that are already up to date.
        #[arg(long)]
        force: bool,
        /// Only these media files, in this order: a file listing them one
        /// per line, relative to the session's folder (`-` for stdin) —
        /// what plays, first heard first, as `session-media` prints it.
        #[arg(long)]
        only: Option<PathBuf>,
    },
    /// Write the waveform peaks cache for a session's media:
    /// `Media/Bass.wav` → `Media/Peaks/Bass.wav.sessionpeaks`, beside
    /// what it describes, so it syncs and streams with the session.
    ///
    /// The file is REAPER's own format under another name: a
    /// `Media/peaks/*.reapeaks` REAPER already wrote is adopted rather
    /// than recomputed, and a cache written here is one REAPER reads.
    /// Sources with no WAV are scanned from their Ogg proxy.
    Peaks {
        /// The session's `.RPP` (or the folder holding exactly one).
        session: PathBuf,
        /// Rescan sources whose cache is already up to date.
        #[arg(long)]
        force: bool,
    },
    /// Turn folders of multitracks into sessions on the grid: the click
    /// stem gives the tempo and where bar one is, every stem is trimmed to
    /// it, and the guide's cues become regions and a chart to start from.
    Import {
        /// A song's folder, or one holding several.
        folders: Vec<PathBuf>,
        /// Where the sessions are written.
        #[arg(long, default_value = "../sessions")]
        out: PathBuf,
        /// Write over a session that is already there.
        #[arg(long)]
        force: bool,
    },
    /// Write an Ogg Vorbis copy of the FTS-GUIDE sample library — the
    /// same layout, each `.wav` a `.ogg` — for a browser to stream the
    /// click, the count and the cues from.
    GuideLibrary {
        /// The WAV library (`~/.config/fts/guide-samples`).
        library: PathBuf,
        /// Where the Ogg copy goes.
        out: PathBuf,
        /// libvorbis quality, -0.2..=1.0. Higher than a stem's: a click is
        /// all transient.
        #[arg(long, default_value_t = 0.6)]
        quality: f32,
    },
    /// Line-synced lyrics for songs, as `<Song>.lrc` beside each `.RPP`.
    Lyrics {
        #[command(subcommand)]
        command: LyricsCommand,
    },
    /// Play a setlist's multitracks in the terminal, natively (a small
    /// REPL: next, previous, play, mute, solo).
    Player(player::Args),
    /// Task's Tracks folder as vault notes: song notes, setlist notes, and
    /// a REAPER project per song.
    VaultSync(vault_sync::Args),
    /// A real REAPER's setlist: open an `.RPL` into it, list, seek, status.
    #[command(subcommand)]
    Reaper(reaper::Command),
}

#[derive(Subcommand)]
pub enum LyricsCommand {
    /// Find line-synced lyrics for each song and write `<Song>.lrc`.
    ///
    /// Title and artists come from the song's chart (`<Song>.kf`, first
    /// line `Title - Artist1, Artist2`); each artist is searched, then the
    /// title alone. Of the versions found, the one whose length is closest
    /// to the chart's SONGSTART→SONGEND is written. Provider: LRCLIB.
    Fetch {
        /// Songs' `.RPP` files (or folders holding exactly one).
        paths: Vec<PathBuf>,
        /// Search for this title instead of the chart's.
        #[arg(long)]
        title: Option<String>,
        /// Search under this artist instead of the chart's (repeatable).
        #[arg(long = "artist")]
        artists: Vec<String>,
        /// Take this exact LRCLIB record, skipping the search.
        #[arg(long)]
        lrclib_id: Option<String>,
        /// Take candidate N (as listed, from 1) instead of the closest length.
        #[arg(long)]
        pick: Option<usize>,
        /// Replace a `.lrc` that is already there.
        #[arg(long)]
        force: bool,
    },
}

/// Diagnostics through `tracing`, as the window has them: to stderr,
/// quiet unless asked, and — with `OTEL_EXPORTER_OTLP_ENDPOINT` set —
/// every span and metric over OTLP to a collector as `service`, where
/// a request's wide event is read whole (`just daw-dev` runs one). With no
/// collector, `RUST_LOG=info` prints each command's wide event (one
/// `lyrics.fetch` span per song) as it closes; with one, stderr keeps only
/// warnings and errors — the rest is the collector's, and an engine
/// answering a window would print a line per call. `default` is the filter
/// when `RUST_LOG` is unset.
pub fn init_tracing(service: &'static str, default: &str) {
    use tracing_subscriber::Layer as _;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;
    use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let otel = ::studio_engine::telemetry::otel(service);
    let (spans, console) = if otel.is_some() {
        (FmtSpan::NONE, LevelFilter::WARN)
    } else {
        (FmtSpan::CLOSE, LevelFilter::TRACE)
    };
    let _ = tracing_subscriber::registry()
        .with(filter())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_span_events(spans)
                .with_filter(console),
        )
        .with(otel)
        .try_init();
}

/// The service a command reports its telemetry as: the engine apart from
/// the rest of the CLI, as a collector shows them.
#[must_use]
pub const fn service_of(command: &Command) -> &'static str {
    match command {
        #[cfg(feature = "native")]
        Command::Engine(_) => "studio-engine",
        _ => "studio-cli",
    }
}

/// Run one of Session's commands; a tokio runtime must be current (the
/// caller's `block_on`).
///
/// # Errors
///
/// When the command fails; it says why.
pub async fn run(command: Command) -> eyre::Result<ExitCode> {
    match command {
        Command::Open {
            what,
            bench,
            bench_timeout,
        } => open(
            &what,
            bench.then(|| Duration::from_secs_f64(bench_timeout.max(0.0))),
        ),
        Command::Songs => songs(),
        Command::Song { to } => {
            println!(
                "{}",
                app::run("SESSION_SONG", serde_json::json!({ "to": to }))?
            );
            Ok(())
        }
        #[cfg(feature = "native")]
        Command::Engine(args) => run_engine(args),
        Command::Actions { filter } => app::list(filter.as_deref()),
        Command::Run { action, args } => {
            let args = match args {
                Some(text) => serde_json::from_str(&text)
                    .map_err(|e| eyre::eyre!("the arguments are not JSON: {e}"))?,
                None => serde_json::Value::Null,
            };
            println!("{}", app::run(&action, args)?);
            Ok(())
        }
        Command::Proxies {
            session,
            quality,
            force,
            only,
        } => {
            let only = only.map(|list| media_list(&list)).transpose()?;
            proxies::write(&session, quality, force, only)
        }
        Command::Peaks { session, force } => peaks::write(&session, force),
        Command::GuideLibrary {
            library,
            out,
            quality,
        } => proxies::guide_library(&library, &out, quality),
        Command::Import {
            folders,
            out,
            force,
        } => import(&folders, &out, force),
        Command::Lyrics {
            command:
                LyricsCommand::Fetch {
                    paths,
                    title,
                    artists,
                    lrclib_id,
                    pick,
                    force,
                },
        } => lyrics::fetch(&lyrics::FetchArgs {
            paths,
            title,
            artists,
            lrclib_id,
            pick,
            force,
        }),
        Command::Player(args) => return Ok(player::run(args)),
        Command::VaultSync(args) => return Ok(vault_sync::run(args)),
        Command::Reaper(command) => reaper::run(command).await,
    }
    .map(|()| ExitCode::SUCCESS)
}

/// The log filter `command` wants when `RUST_LOG` is unset, if not the
/// command line's own: the engine says what it does (it is a server).
#[must_use]
pub fn log_filter(command: &Command) -> Option<&'static str> {
    match command {
        #[cfg(feature = "native")]
        // loro's spans are its internals: ~1500 per collab import.
        Command::Engine(_) => Some(
            "info,wgpu=warn,naga=warn,symphonia=warn,iroh=warn,vox=warn,loro=warn,loro_internal=warn",
        ),
        _ => None,
    }
}

/// `studio engine`, on a thread of its own: the engine runs its own
/// runtime and blocks on it, which it may not do on this one's.
#[cfg(feature = "native")]
fn run_engine(args: engine::Args) -> eyre::Result<()> {
    std::thread::Builder::new()
        .name("studio-engine".into())
        .spawn(move || engine::run(&args))?
        .join()
        .map_err(|_| eyre::eyre!("the engine stopped with a panic"))?
}

/// `studio open`: the song, setlist or `.session` at `path`
/// (`SESSION_OPEN`). With `bench`, timed until the set is open
/// ([`bench::bench`]).
fn open(path: &Path, bench: Option<Duration>) -> eyre::Result<()> {
    if !path.exists() {
        eyre::bail!("{} is not there", path.display());
    }
    let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (action, args) = (
        "SESSION_OPEN",
        serde_json::json!({"path": full.to_string_lossy()}),
    );
    match bench {
        Some(timeout) => bench::bench(&app::window()?, action, args, timeout),
        None => {
            println!("{}", app::run(action, args)?);
            Ok(())
        }
    }
}

/// `studio songs`: the open set, a song a line.
fn songs() -> eyre::Result<()> {
    let songs = bench::songs(&app::window()?, app::CALL_TIMEOUT)?;
    if songs.is_empty() {
        println!("no set is open — `studio open <path>`");
        return Ok(());
    }
    let width = songs
        .iter()
        .map(|s| s.title.chars().count())
        .max()
        .unwrap_or(0);
    for song in &songs {
        println!(
            "{} {:>3}  {:<width$}  {:<8}  {}",
            if song.current { '>' } else { ' ' },
            song.number,
            song.title,
            song.state,
            song.project,
        );
    }
    Ok(())
}

/// The media list `studio proxies --only` reads: one path per line, from
/// a file or stdin (`-`); blank lines skipped.
fn media_list(from: &Path) -> eyre::Result<Vec<PathBuf>> {
    let text = if from.as_os_str() == "-" {
        std::io::read_to_string(std::io::stdin())?
    } else {
        std::fs::read_to_string(from)?
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// `studio import`: each folder named, or each song folder inside one.
fn import(folders: &[PathBuf], out: &Path, force: bool) -> eyre::Result<()> {
    let mut songs: Vec<PathBuf> = Vec::new();
    for folder in folders {
        // A folder of folders (the downloads directory) imports each song
        // in it; a folder with stems in it is one song.
        let holds_stems = std::fs::read_dir(folder)?.flatten().any(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
        });
        if holds_stems {
            songs.push(folder.clone());
        } else {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(folder)?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            inside.sort();
            songs.extend(inside);
        }
    }
    let mut failed = Vec::new();
    for song in &songs {
        if let Err(e) = multitracks::import(song, out, force) {
            failed.push(format!("{}: {e}", song.display()));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!(
            "{} did not import:\n{}",
            failed.len(),
            failed.join("\n")
        ))
    }
}
