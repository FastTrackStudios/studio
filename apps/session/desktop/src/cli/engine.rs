//! `studio engine`: Session's whole engine, headless, in this process —
//! the command line's door into `crate::engine_host` (the same engine the
//! app runs as `studio-desktop --session-engine`).
//!
//! ```text
//! studio engine &                                   # the engine, no window
//! studio open "JT's Senior Recital" --org fasttrackaudio
//! studio songs
//! studio run SESSION_PLAY_PAUSE
//! studio run SESSION_AUDIO
//! ```

use std::path::PathBuf;

use clap::Args as ClapArgs;

/// What `studio engine` was asked to do.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Where to serve the daw facade to local clients (a Unix socket);
    /// `$XDG_RUNTIME_DIR/fts/engine-<pid>.sock` when left out.
    #[arg(long)]
    pub socket: Option<PathBuf>,
    /// Also serve the facade over WebSocket on this address
    /// (`127.0.0.1:4040`, `0.0.0.0:4040` for the network): `ws://ADDR/vox`.
    #[arg(long)]
    pub listen: Option<String>,
    /// Open this as the engine comes up: a path (a song, `.session` or
    /// setlist), or a library setlist's title.
    #[arg(long)]
    pub open: Option<String>,
    /// The org whose library holds `--open`'s setlist.
    #[arg(long, requires = "open")]
    pub org: Option<String>,
    /// One song of `--open`'s setlist to open alone.
    #[arg(long, requires = "open")]
    pub song: Option<String>,
    /// Open what the last engine had open (what a window does when the
    /// engine it drove died and it starts another).
    #[arg(long, conflicts_with = "open")]
    pub reopen: bool,
}

/// Run the engine until Ctrl-C or SIGTERM.
///
/// # Errors
///
/// The engine did not start, or its socket could not be bound.
pub fn run(args: &Args) -> eyre::Result<()> {
    crate::engine_host::run(&crate::engine_host::Args {
        socket: args.socket.clone(),
        listen: args.listen.clone(),
        open: args.open.clone(),
        org: args.org.clone(),
        song: args.song.clone(),
        reopen: args.reopen,
    })
}
