//! `FTS Signal Rig` in a page: signal's guitar or bass rig, whole, built in
//! the page's engine.
//!
//! A desktop's runs signal's rig service and a switcher thread
//! (`signal_rig.rs`). A page has neither, so the rig here is the profile
//! itself: every patch of it built as a chain ([`ProfileRig::build_in`]),
//! one playing, a switch a crossfade with the tails ringing on, as the
//! service plays them. Its home (the profiles, their stacks and patches)
//! and its captures and IRs come from the rig store ([`crate::files`],
//! `guitar/…` or `bass/…`): it builds with what has arrived, waits for the
//! rest, and takes its patches at the next block once they are in.
//!
//! The state and the Patch parameter are the desktop's, so a song saved on
//! a desktop plays here, its Patch automation meaning the same thing: the
//! patch at that step of the profile's own (a song's patches are listed
//! only while that song is up, and no song is up in a page).
//!
//! Not here: the stacks and their footswitches, and the window and the
//! actions that drive a desktop's rig service. A page plays the patches.

use std::sync::Arc;

use daw_standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use guitar::instrument::Instrument;

use crate::files::{self, Slot};
use crate::profile::{self, ProfileRig};

/// The Patch parameter's id.
pub const PATCH: u32 = 0;

/// What a rig is and plays, as saved with the song (the desktop's shape).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RigState {
    /// The rig's id, kept for the desktop that opens the song next.
    #[serde(default)]
    pub id: String,
    /// `guitar` or `bass`.
    #[serde(default)]
    pub instrument: String,
    /// The profile it plays, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The patch playing, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

impl RigState {
    fn instrument(&self) -> Instrument {
        Instrument::from_id(&self.instrument)
    }

    /// The profile's own patches, by name, in order: what the desktop's
    /// rig lists with no song up, and what the Patch parameter steps along.
    fn listed(&self) -> Vec<String> {
        let instrument = self.instrument();
        let Some(found) = profile::profile_in(instrument, self.profile_name()) else {
            return Vec::new();
        };
        let library = profile::library_of(instrument);
        found
            .patches
            .iter()
            .filter(|p| !library.is_song_patch(&found.name, &p.name))
            .map(|p| p.name.clone())
            .collect()
    }

    /// The patch the state names among the listed, else the profile's
    /// default, else the first.
    fn patch_index(&self) -> usize {
        let listed = self.listed();
        let at = |name: &str| listed.iter().position(|n| n.eq_ignore_ascii_case(name));
        let default = profile::profile_in(self.instrument(), self.profile_name())
            .and_then(|p| p.patches.get(p.default_patch))
            .map(|p| p.name.clone());
        self.patch
            .as_deref()
            .and_then(at)
            .or_else(|| default.as_deref().and_then(at))
            .unwrap_or(0)
    }

    fn profile_name(&self) -> &str {
        self.profile.as_deref().unwrap_or("")
    }
}

/// `FTS Signal Rig`, in a page.
pub struct SignalRig {
    state: RigState,
    rig: Option<ProfileRig>,
    sample_rate: f64,
    block_size: u32,
    prepared: bool,
    /// The rig made again once its files have arrived, for the next block.
    arriving: Option<Arc<Slot<Option<ProfileRig>>>>,
    /// Whether it plays files the page streamed in (it says how loud).
    streamed: bool,
}

/// `state`'s profile, every listed patch built, the one it names playing.
fn built(state: &RigState, sample_rate: f64, block_size: u32) -> Option<ProfileRig> {
    let listed = state.listed();
    ProfileRig::build_listed(
        state.instrument(),
        state.profile_name(),
        &|name| listed.iter().any(|n| n.eq_ignore_ascii_case(name)),
        state.patch_index(),
        (sample_rate, block_size),
    )
}

impl SignalRig {
    #[must_use]
    pub const fn new(sample_rate: f64) -> Self {
        Self {
            state: RigState {
                id: String::new(),
                instrument: String::new(),
                profile: None,
                patch: None,
            },
            rig: None,
            sample_rate,
            block_size: 0,
            prepared: false,
            arriving: None,
            streamed: false,
        }
    }

    /// Build what the state says with what has arrived, and wait for the
    /// rest — on the page's thread, never in a block.
    fn build(&mut self) {
        let instrument = self.state.instrument();
        let named = self.state.profile_name();
        let library = &profile::library_of(instrument).profiles;
        if !named.is_empty() && !library.iter().any(|p| p.name.eq_ignore_ascii_case(named)) {
            let have: Vec<&str> = library.iter().map(|p| p.name.as_str()).collect();
            tracing::warn!(
                rig.profile = named,
                rig.profiles = ?have,
                "signal rig: its library has no such profile; it plays the first"
            );
        }
        self.rig = built(&self.state, self.sample_rate, self.block_size);
        if let Some(rig) = &self.rig {
            self.state.profile = Some(rig.profile.clone());
        }
        self.arriving = None;
        let files = profile::files_in(self.state.instrument(), self.state.profile_name());
        let missing: Vec<String> = files.iter().filter(|k| !files::has(k)).cloned().collect();
        self.streamed = !files.is_empty() && missing.is_empty();
        if missing.is_empty() {
            return;
        }
        tracing::info!(
            rig.files = missing.len(),
            "signal rig: waiting for its files from the rig store"
        );
        let (state, rate, block) = (self.state.clone(), self.sample_rate, self.block_size);
        self.arriving = Some(files::wait(missing, move || built(&state, rate, block)));
    }

    /// At a block's start: the rig its files arrived for, taken (the one it
    /// replaces handed back to be dropped off this thread).
    fn take_arrived(&mut self) {
        let Some(slot) = self.arriving.clone() else {
            return;
        };
        let Some(mut rig) = slot.take() else { return };
        self.arriving = None;
        // A rebuild that made nothing keeps what plays.
        if rig.is_none() {
            slot.retire(rig);
            return;
        }
        // The patch playing now carries over (a switch made while waiting).
        if let (Some(new), Some(old)) = (&mut rig, &self.rig) {
            new.switch(old.scene());
        }
        std::mem::swap(&mut self.rig, &mut rig);
        slot.retire(rig);
        self.streamed = true;
    }

    /// The profile's patches, by name, in order: the Patch parameter's
    /// choices.
    fn patch_names(&self) -> Vec<String> {
        if let Some(rig) = &self.rig {
            return rig.scenes();
        }
        self.state.listed()
    }
}

impl PluginInstance for SignalRig {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fts.signal.rig".into(),
            name: crate::SIGNAL_RIG.into(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        vec![PluginParamInfo {
            id: PATCH,
            name: "Patch".into(),
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }]
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        let rig = self.rig.as_ref().filter(|_| id == PATCH)?;
        Some(profile::normalized(rig.scene(), rig.chains.len()))
    }

    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        if id != PATCH {
            return None;
        }
        let names = self.patch_names();
        names.get(profile::scene_at(value, names.len())).cloned()
    }

    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        if id != PATCH {
            return None;
        }
        let names = self.patch_names();
        names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(text.trim()))
            .map(|i| profile::normalized(i, names.len()))
    }

    fn param_choices(&mut self, id: u32) -> Vec<(f64, String)> {
        if id != PATCH {
            return Vec::new();
        }
        let names = self.patch_names();
        let count = names.len();
        names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (profile::normalized(i, count), name))
            .collect()
    }

    /// None reported, as the desktop's rig reports none: a patch switch
    /// does not move the track's compensation.
    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate;
        self.block_size = block_size;
        self.build();
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        self.take_arrived();
        let Some(rig) = &mut self.rig else {
            out_l.iter_mut().zip(in_l).for_each(|(o, i)| *o = *i);
            out_r.iter_mut().zip(in_r).for_each(|(o, i)| *o = *i);
            return Ok(());
        };
        // The Patch parameter (automation, a write): switched here, a
        // crossfade with nothing allocated.
        if let Some(&(_, value)) = events.params.iter().rev().find(|(id, _)| *id == PATCH) {
            let count = rig.chains.len();
            rig.switch(profile::scene_at(value, count));
        }
        rig.process((in_l, in_r), (out_l, out_r), (1.0, 1.0))?;
        if self.streamed {
            let peak = out_l
                .iter()
                .chain(out_r.iter())
                .fold(0.0_f32, |m, s| m.max(s.abs()));
            files::note_heard_signal_rig(peak);
        }
        Ok(())
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        let read: RigState = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("signal rig state: {e}")))?;
        self.state = RigState {
            id: if read.id.is_empty() {
                std::mem::take(&mut self.state.id)
            } else {
                read.id
            },
            ..read
        };
        if self.prepared {
            self.build();
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        if let Some(rig) = &self.rig {
            self.state.patch = rig.scenes().get(rig.scene()).cloned();
        }
        serde_json::to_vec(&self.state).map_err(|e| PluginError::LoadFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktops_state_comes_back_as_it_went_in() {
        let saved = br#"{"id":"rig-1","instrument":"guitar","profile":"Worship","patch":"Lead"}"#;
        let mut rig = SignalRig::new(48_000.0);
        rig.load_state(saved).expect("the state loads");
        let back: RigState =
            serde_json::from_slice(&rig.save_state().expect("it saves")).expect("it reads");
        assert_eq!(
            back,
            RigState {
                id: "rig-1".to_owned(),
                instrument: "guitar".to_owned(),
                profile: Some("Worship".to_owned()),
                patch: Some("Lead".to_owned()),
            }
        );
    }

    #[test]
    fn a_state_with_no_id_keeps_the_rigs_own() {
        let mut rig = SignalRig::new(48_000.0);
        rig.load_state(br#"{"id":"rig-1","instrument":"bass"}"#)
            .expect("the state loads");
        rig.load_state(br#"{"instrument":"bass","patch":"Drive"}"#)
            .expect("the state loads");
        let back: RigState =
            serde_json::from_slice(&rig.save_state().expect("it saves")).expect("it reads");
        assert_eq!(back.id, "rig-1");
        assert_eq!(back.patch.as_deref(), Some("Drive"));
    }
}
