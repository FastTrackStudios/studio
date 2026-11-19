//! What this set plays, told to the account's other Studio engines.
//!
//! The mesh node of this machine is the rig engine (`studio-desktop
//! --engine`, `studio_peers`); Session's engine is a window onto a set in
//! another process. While it plays, it publishes the set, the song up and
//! its transport through that engine's `PeersControl::publish`, which makes
//! the engine a source of tracks any peer may follow. With no engine to
//! reach, nothing (and it tries again).

use std::time::Duration;

use studio_peers::{NowPlaying, PeersControlClient};

/// The rig engine's `/vox` (`SIGNAL_ENGINE_URL`).
fn engine_url() -> String {
    std::env::var("SIGNAL_ENGINE_URL").unwrap_or_else(|_| "ws://127.0.0.1:4040/vox".into())
}

/// What the set plays now, if one is open.
fn now() -> Option<NowPlaying> {
    let lineup = super::set::lineup()?;
    let (title, project) = lineup.songs.get(lineup.at)?.clone();
    let (position, playing) = crate::engine::playhead_of(&project).unwrap_or((0.0, false));
    let bpm = crate::open::local_engine()
        .map(|daw| daw.transport_engine_for(&project).shared.tempo_bpm())
        .unwrap_or(0.0);
    Some(NowPlaying {
        song: title,
        song_index: u32::try_from(lineup.at).unwrap_or(u32::MAX),
        playing,
        position,
        bpm,
        ..NowPlaying::default()
    })
}

/// Publish the set's playing to the rig engine: at once on a change,
/// each second while it plays. Once per process, on the engine's runtime.
pub fn start(runtime: &tokio::runtime::Runtime) {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    STARTED.get_or_init(|| {
        runtime.spawn(publish_loop());
    });
}

async fn publish_loop() {
    let pool = architect::connect::Pool::new();
    let target = architect::connect::Endpoint::url(engine_url());
    let mut last: Option<NowPlaying> = None;
    let mut ticks = 0u32;
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let Some(now) = now() else {
            last = None;
            continue;
        };
        ticks += 1;
        let moved = last.as_ref().is_none_or(|l| {
            NowPlaying {
                position: l.position,
                at_ms: l.at_ms,
                ..now.clone()
            } != *l
        });
        if !(moved || (now.playing && ticks >= 5) || ticks >= 25) {
            continue;
        }
        ticks = 0;
        let Ok(control) = pool.client::<PeersControlClient>(&target).await else {
            continue;
        };
        if control.publish(now.clone()).await.is_err() {
            pool.evict(&target);
            continue;
        }
        last = Some(now);
    }
}
