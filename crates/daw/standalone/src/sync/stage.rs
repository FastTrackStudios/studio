//! Building a project off the engine's lock: a [`Stage`].
//!
//! Every project of an engine sits behind its one state lock, so a song
//! rebuilt in place — a call per track, item, FX and envelope, each taking
//! that lock — stalls every reader of every other song for as long as it
//! takes, and two songs opening at once take turns. A stage is a private
//! engine to build the song in instead: its own state and lock, sharing
//! with its home engine what a built project refers to (the plugin
//! instances its FX are, the FX factory and media resolver they are made
//! with, the event hubs its edits are announced on). When the song is
//! built, [`Stage::land`] moves it home in one short critical section.

use std::sync::{Arc, Mutex, PoisonError};

use daw_proto::{DawError, DawResult};

use super::daw::Standalone;

/// A private engine a project is built in, then landed in its home engine
/// ([`Standalone::stage`]).
pub struct Stage {
    home: Standalone,
    engine: Standalone,
}

impl Standalone {
    /// A stage to build a project in without holding this engine's lock:
    /// an engine of its own, whose FX instances, FX factory, media
    /// resolver and event hubs are this one's.
    #[must_use]
    pub fn stage(&self) -> Stage {
        let engine = Self {
            plugin_instances: Arc::clone(&self.plugin_instances),
            loaded_plugins: Arc::clone(&self.loaded_plugins),
            fx_factory: Arc::clone(&self.fx_factory),
            bay_resolver: Arc::clone(&self.bay_resolver),
            track_events: self.track_events.clone(),
            marker_events: self.marker_events.clone(),
            region_events: self.region_events.clone(),
            tempo_map_events: self.tempo_map_events.clone(),
            bus_events: self.bus_events.clone(),
            ..Self::new()
        };
        Stage {
            home: self.clone(),
            engine,
        }
    }
}

impl Stage {
    /// The engine the project is built in: seed it, then edit it through
    /// any service, as any engine.
    #[must_use]
    pub fn engine(&self) -> &Standalone {
        &self.engine
    }

    /// Move project `guid`, as built, into the home engine — its state, its
    /// undo history and its transport's settings — in one critical section
    /// on the home engine's lock, its revision moved on once.
    ///
    /// Call it inside the async runtime the home engine's transports run
    /// on: a transport made for the project here lives on it.
    ///
    /// # Errors
    /// The stage holds no project `guid`, or the home engine already does.
    pub fn land(self, guid: &str) -> DawResult<()> {
        let (mut project, undo, redo, pending) = {
            let mut staged = lock(&self.engine.state);
            let project = staged
                .projects
                .remove(guid)
                .ok_or_else(|| DawError::not_found("Project", guid))?;
            (
                project,
                staged.undo.remove(guid),
                staged.redo.remove(guid),
                staged.pending_undo.remove(guid),
            )
        };
        let tracks = project.tracks.len();
        project.revision = project.revision.wrapping_add(1);
        {
            let mut home = lock(&self.home.state);
            if home.projects.contains_key(guid) {
                return Err(DawError::already_exists("Project", guid));
            }
            home.projects.insert(guid.to_owned(), project);
            if let Some(undo) = undo {
                home.undo.insert(guid.to_owned(), undo);
            }
            if let Some(redo) = redo {
                home.redo.insert(guid.to_owned(), redo);
            }
            if let Some(pending) = pending {
                home.pending_undo.insert(guid.to_owned(), pending);
            }
            if home.current_project_guid.is_none() {
                home.current_project_guid = Some(guid.to_owned());
            }
        }
        self.home.meters_follow_tracks(tracks);
        let staged = lock(&self.engine.transport_engines).remove(guid);
        if let Some(staged) = staged {
            let shared = &self.home.transport_engine_for(guid).shared;
            let built = &staged.shared;
            shared.set_tempo_bpm(built.tempo_bpm());
            // The region first: clearing it clears the loop too.
            shared.set_loop_region(built.loop_region());
            shared.set_looping(built.is_looping());
            shared.set_playrate(built.playrate());
            shared.set_metronome(built.metronome());
            shared.set_playhead_samples_f64(built.playhead_samples_f64());
        }
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
