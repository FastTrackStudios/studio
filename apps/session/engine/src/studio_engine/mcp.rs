//! The engine's Action List, served to a model and the CLI over MCP.
//!
//! Whatever Session can do is an action in its Action List
//! (`crate::actions`); this puts a model on it while you work — the
//! window's process or `studio engine` with no window, the same list — on
//! `http://127.0.0.1:47811/mcp` (`FTS_MCP_PORT` to move it, `FTS_MCP=0`
//! for none). Found without asking: the repository's `.mcp.json` names
//! it for Claude Code, and `~/.config/fts/session-mcp.json` says where it
//! is for any other client.
//!
//! Serving, the port and token variables and the discovery file are
//! `architect_mcp`'s (`Server::serve_loopback_or`, `Serving::publish`);
//! the committed `.mcp.json` is what `architect_mcp::project_mcp_json`
//! writes (`tests/mcp_in_process.rs` checks it).
//!
//! Only this machine reaches it: it listens on loopback and refuses a
//! browser's request from anywhere else (`architect_mcp::http`). A token
//! is optional — `FTS_MCP_TOKEN` sets one, and then a client must send it
//! as `Authorization: Bearer …`; unset, there is none, so a model on this
//! machine connects with nothing to configure.

/// Where it listens unless `FTS_MCP_PORT` says otherwise.
pub const DEFAULT_PORT: u16 = 47_811;

/// Its name: in `.mcp.json` and in `~/.config/fts/<name>-mcp.json`.
pub const DISCOVERY_NAME: &str = "session";

/// What a model is told about this app when it connects.
const INSTRUCTIONS: &str = "This is FastTrackStudio Session, the DAW the user mixes in, with the \
song they have open. Its tracks are read with SESSION_TRACKS: each has a GUID and a folder path \
(`Drums/Drum Kit/Kick/Sum/In`); actions that take a track take any of its GUID, its path or a tail \
of the path (`Kick/Sum/Trig`). The kit follows the golden template: each piece (Kick, Snare, \
Tom 1…) has its close mics, its `Trig` (what says the drum was hit — a recorded trigger or the \
kit's notes; it keys the piece's gate and is never heard) and its `Sample` (what plays from the \
trigger), with a `Fund` (or the kick's `Sub`) band-pass beside them. SESSION_* \
actions are the window's own: what its keys do (SESSION_MODE Edit and SESSION_PHASE edit put the \
window where every cut shows). TRACK_SET changes a track's fader, pan, mute or solo. Drum editing \
works from hit markers: HITS_DETECT finds every hit of the kit once (triggers and mics together, \
under a second) and writes them as markers on the recordings, which edits move with; HITS_LIST, \
HITS_EXPLAIN and AUDIO_PEAKS look at them, HITS_ADD/MOVE/REMOVE/MARK correct them. Fills are \
zones on the Drums ruler lane (ZONES_FIND proposes, ZONES_EDIT approves or rejects). \
DRUMS_EDIT_LANES puts the edits on an EDIT lane beside the COMP they are made from \
(DRUMS_PLAY_LANE to hear either); QUANTIZE then edits the whole kit as one, a section at a time \
(its settings sticky in DRUMS_EDIT_SETTINGS, any given for one call), a dry run first, with a \
check after; HITS_AUDIT checks the whole song. In Edit mode the window draws each piece's markers \
on its own rows and shades the zones. SESSION_VIEW frames bars, a section or seconds, and tracks, \
down to the samples. The drum MIDI is a guide, \
not the truth: a live drummer plays other fills.";

/// Serve the Action List on the engine's runtime, unless `FTS_MCP=0`, and
/// say where it is (`~/.config/fts/session-mcp.json`); `None` when it is
/// off or could not be served (said in the log).
pub fn start() -> Option<architect_mcp::http::Serving> {
    let runtime = crate::open::runtime()?;
    let serving = match serve(runtime) {
        Ok(Some(serving)) => serving,
        Ok(None) => return None,
        // A relaunch (`dx serve`, an update) starts before the process it
        // replaces lets go of the port: tried again behind, for a while.
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            std::thread::spawn(move || {
                for _ in 0..PORT_TRIES {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    match serve(runtime) {
                        Ok(Some(serving)) => {
                            announce(&serving);
                            return;
                        }
                        Ok(None) => return,
                        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {}
                        Err(e) => {
                            tracing::warn!(error = %e, "mcp: not served");
                            return;
                        }
                    }
                }
                tracing::warn!(error = %error, "mcp: not served, the port stayed taken");
            });
            return None;
        }
        Err(error) => {
            tracing::warn!(error = %error, "mcp: not served");
            return None;
        }
    };
    announce(&serving);
    Some(serving)
}

/// Tries at the port while another process still holds it, half a second
/// apart.
const PORT_TRIES: u32 = 30;

/// The Action List served on `runtime`, once.
fn serve(
    runtime: &'static tokio::runtime::Runtime,
) -> std::io::Result<Option<architect_mcp::http::Serving>> {
    let server = architect_mcp::Server::new(
        "FastTrackStudio Session",
        env!("CARGO_PKG_VERSION"),
        crate::actions::list().clone(),
    )
    .with_instructions(INSTRUCTIONS);
    // On a thread of its own: a caller on a runtime worker can't block on
    // the runtime.
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                runtime.block_on(server.serve_loopback_or(
                    DEFAULT_PORT,
                    "FTS_MCP_PORT",
                    "FTS_MCP_TOKEN",
                ))
            })
            .join()
    })
    .unwrap_or_else(|_| Err(std::io::Error::other("starting it panicked")))
}

/// Say where the Action List is served: in the discovery file and the log.
fn announce(serving: &architect_mcp::http::Serving) {
    if let Err(error) = serving.publish(DISCOVERY_NAME) {
        tracing::warn!(error = %error, "mcp: could not write where it is served");
    }
    tracing::info!(
        mcp.url = %serving.url,
        mcp.token = serving.token.is_some(),
        mcp.actions = crate::actions::list().len(),
        "mcp: serving the Action List"
    );
}
