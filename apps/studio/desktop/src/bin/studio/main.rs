//! `studio` — FastTrackStudio's one command line.
//!
//! Every command sits at the top level. The ones people use most drive the
//! app that is running, through its Action List — `studio open`, `studio
//! songs`, `studio song`, `studio actions`, `studio run` — and the rest work
//! on sessions, the library, the vault, a real REAPER
//! (`session_desktop::cli`), the guitar rig's library on disk ([`rig`]) and
//! the TONE3000 tone library ([`tone3000`]).
//!
//! ```console
//! studio open ~/Songs/Praise/Praise.session
//! studio open "Crescendum" --org fasttrackaudio --bench
//! studio songs
//! studio song next
//! studio run SESSION_PLAY
//! studio engine                       # Session's engine with no window
//! studio tone3000 search "vox ac30"
//! studio rig level Metal --dry-run
//! ```
//!
//! The tone library and the rig's library are files and an OAuth session on
//! this machine, so `studio tone3000` and `studio rig` open them directly
//! (the same backends the app uses) and work whether or not the app or
//! `studio-desktop --engine` is up. The one thing that collides is the
//! TONE3000 sign-in listener, which wants the redirect port the engine also
//! serves — `studio tone3000 login` says so when it is taken.
//!
//! What a command finds is printed to stdout; diagnostics go to stderr
//! through `tracing` (`RUST_LOG=info` for each command's wide event).

mod rig;
mod tone3000;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "studio", about = "FastTrackStudio from a terminal")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(flatten)]
    Session(session_desktop::cli::Command),
    /// The TONE3000 tone library — browse it, and bring captures into the
    /// local NAM library the rig plays from.
    Tone3000 {
        #[command(subcommand)]
        command: tone3000::Command,
    },
    /// The guitar rig's library on disk — profiles, patches.
    Rig {
        #[command(subcommand)]
        command: rig::Command,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (service, filter) = match &cli.command {
        Command::Session(command) => (
            session_desktop::cli::service_of(command),
            session_desktop::cli::log_filter(command),
        ),
        _ => ("studio-cli", None),
    };
    session_desktop::cli::init_tracing(service, filter.unwrap_or("warn,signal_tone3000=info"));
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("could not start the async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match cli.command {
        Command::Session(command) => match runtime.block_on(session_desktop::cli::run(command)) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("Error: {e:?}");
                ExitCode::FAILURE
            }
        },
        Command::Tone3000 { command } => runtime.block_on(tone3000::run(command)),
        Command::Rig { command } => rig::run(command),
    }
}
