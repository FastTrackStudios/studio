//! The tracks' mute and solo as values: what every pane lights its
//! buttons by, in the same frame.
//!
//! An edit made in one pane (the arrangement's track panel, the mixer)
//! goes to the engine, which runs in this process; its track event comes
//! back here first, and is written into this extension's shared value
//! table ([`studio_engine::engine::track_value`]: 1 lit, 0 not) before
//! any other process hears of it. The host reads the table as it composes,
//! so every pane drawing a button from it lights it in the same frame: no
//! pane is a frame behind another, whichever was clicked. The first
//! [`VALUE_TRACKS`] of the set's current song, by project index.

use std::collections::HashMap;
use std::time::Duration;

use architect_scene::anim::Name;
use architect_scene::engine::{EngineWriter, Slot};
use daw_proto::track::TrackEvent;
use studio_engine::engine::{TRACK_PARTS, VALUE_TRACKS, track_value};

/// How often the set's current song is looked at, to follow it.
const LOOK: Duration = Duration::from_millis(250);

/// Publishes them into `writer`'s table, on a thread of its own, for as
/// long as the process runs.
pub fn publish(writer: EngineWriter) {
    let _ = std::thread::Builder::new()
        .name("studio-buttons".to_owned())
        .spawn(move || run(&Buttons::new(writer)));
}

/// The table's button slots, by track index then part.
struct Buttons {
    writer: EngineWriter,
    slots: Vec<[Option<Slot>; 2]>,
}

impl Buttons {
    fn new(writer: EngineWriter) -> Self {
        let slot = |i: usize, part: &str| {
            Name::new(&track_value(i, part))
                .ok()
                .and_then(|n| writer.table().slot(&n))
        };
        let slots = (0..VALUE_TRACKS)
            .map(|i| [slot(i, TRACK_PARTS[0]), slot(i, TRACK_PARTS[1])])
            .collect();
        Self { writer, slots }
    }

    /// Track `index`'s button `part` (0 mute, 1 solo) lit or not.
    fn write(&self, index: usize, part: usize, lit: bool) {
        if let Some(Some(slot)) = self.slots.get(index).and_then(|s| s.get(part)) {
            self.writer.write(*slot, if lit { 1.0 } else { 0.0 });
        }
    }

    /// Every track's, from `tracks` (a track past them unlit); their
    /// indices by GUID.
    fn all(&self, tracks: &[daw_proto::Track]) -> HashMap<String, usize> {
        let mut at = HashMap::new();
        let mut lit = vec![(false, false); VALUE_TRACKS];
        for t in tracks {
            if let Ok(i) = usize::try_from(t.index) {
                at.insert(t.guid.clone(), i);
                if let Some(l) = lit.get_mut(i) {
                    *l = (t.muted, t.soloed);
                }
            }
        }
        for (i, (mute, solo)) in lit.into_iter().enumerate() {
            self.write(i, 0, mute);
            self.write(i, 1, solo);
        }
        at
    }
}

fn run(buttons: &Buttons) {
    let Some(runtime) = studio_engine::open::runtime() else {
        tracing::warn!("daw: no runtime to follow the tracks' buttons on");
        return;
    };
    tracing::info!(
        values.tracks = VALUE_TRACKS,
        "daw: the tracks' mute and solo are published as values"
    );
    loop {
        if let Some(project) = crate::meters::current_project() {
            runtime.block_on(follow(buttons, &project));
        }
        std::thread::sleep(LOOK);
    }
}

/// Follow `project`'s tracks while it is the set's current song.
async fn follow(buttons: &Buttons, project: &str) {
    let Some(daw) = daw::rpc::Daw::try_get() else {
        return;
    };
    let Ok(song) = daw.project(project).await else {
        return;
    };
    let tracks = song.tracks();
    let Ok(mut events) = tracks.subscribe().await else {
        return;
    };
    let Ok(now) = tracks.all().await else {
        return;
    };
    let mut at = buttons.all(&now);
    let mut look = tokio::time::interval(LOOK);
    loop {
        tokio::select! {
            event = events.recv() => {
                let Ok(Some(event)) = event else { return };
                match &event.get().event {
                    TrackEvent::MuteChanged { guid, muted } => {
                        if let Some(i) = at.get(guid) {
                            buttons.write(*i, 0, *muted);
                        }
                    }
                    TrackEvent::SoloChanged { guid, soloed } => {
                        if let Some(i) = at.get(guid) {
                            buttons.write(*i, 1, *soloed);
                        }
                    }
                    // Tracks added, gone or moved: every index again.
                    TrackEvent::Added(_) | TrackEvent::Removed(_) | TrackEvent::Moved { .. } => {
                        if let Ok(now) = tracks.all().await {
                            at = buttons.all(&now);
                        }
                    }
                    _ => {}
                }
            }
            _ = look.tick() => {
                if crate::meters::current_project().as_deref() != Some(project) {
                    return;
                }
            }
        }
    }
}
