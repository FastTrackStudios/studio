//! The leader's transport, as the entry everyone in the room follows.
//!
//! The leader alone drives a room's transport, so its entry is not a
//! press anyone may make (as in a live session playing together) but the
//! leader's engine as it is: written when it starts or stops, changes
//! song, or jumps, and left alone while it plays along. A member follows
//! it with session-sync's `TransportSync` (its `remote` and `tick`), which
//! extrapolates the entry on the shared clock and nudges a drifting
//! engine back.

use session_sync::transport::{LocalTransport, SharedTransport, TransportMode};

/// How far the leader's playhead may be from where the entry says before
/// it is a jump, seconds.
pub const JUMP: f64 = 0.25;

/// The entry to publish for the leader's engine at `local`, at `now_ms`
/// on the room's clock (the leader's own: it hosts the room), when `prev`
/// no longer says it; `None` when it still does.
#[must_use]
pub fn entry(
    prev: Option<&SharedTransport>,
    local: &LocalTransport,
    now_ms: f64,
    me: &str,
) -> Option<SharedTransport> {
    if let Some(prev) = prev {
        let same = prev.playing == local.playing
            && prev.song == local.song
            && (prev.position_at(now_ms) - local.position).abs() <= JUMP;
        if same {
            return None;
        }
    }
    let seq = prev.map_or(1, |p| p.seq.saturating_add(1));
    Some(SharedTransport {
        mode: TransportMode::Shared,
        playing: local.playing,
        position: local.position,
        at_ms: now_ms,
        song: local.song.clone(),
        seq,
        by: me.to_owned(),
    })
}
