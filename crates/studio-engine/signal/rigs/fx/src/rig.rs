//! `FTS Bass Rig`: signal's bass on a DI, in one of two modes, by its
//! state.
//!
//! - **Profile** (`{"profile": "Metal", "scene": "Drive", …}`): a signal
//!   bass profile with every scene loaded, switched by the **Scene**
//!   parameter ([`crate::profile`]). What a new rig is.
//! - **Patch** (`{"amp": "…/Amp.nam", "drive": …, "cab": …}`): one fixed
//!   chain of captures ([`crate::bass::BassRig`]) — what a song saved before
//!   profiles holds, played as it was.
//!
//! The parameters are the same in both, so automation and settings saved
//! against them mean the same thing: Input and Output (dB, ±24) and Scene
//! (stepped, 0..1 along the profile's scenes; a patch has none).

use daw_standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use crate::bass::BassRig;
use crate::profile::{self, ProfileRig};

/// Parameter ids.
pub const INPUT: u32 = 0;
pub const OUTPUT: u32 = 1;
pub const SCENE: u32 = 2;

/// The trims' range, dB either way.
const TRIM_DB: f64 = 24.0;

/// What the rig plays, as saved: a profile and its scene, or a patch's
/// captures; and its trims.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RigState {
    /// The profile, by name (profile mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The scene playing, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// A patch's captures (patch mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drive: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cab: Option<std::path::PathBuf>,
    #[serde(default)]
    pub input_db: f64,
    #[serde(default)]
    pub output_db: f64,
}

impl RigState {
    /// Whether this is a patch (fixed captures) rather than a profile.
    #[must_use]
    pub const fn is_patch(&self) -> bool {
        self.profile.is_none() && (self.amp.is_some() || self.drive.is_some() || self.cab.is_some())
    }
}

/// The rig, in its mode.
pub struct Rig {
    state: RigState,
    patch: Option<BassRig>,
    profile: Option<ProfileRig>,
    sample_rate: f64,
    block_size: u32,
    prepared: bool,
    /// In a page: the chain made again once its files have arrived from the
    /// rig store ([`crate::files`]), for the next block to take.
    arriving: Option<std::sync::Arc<crate::files::Slot<Built>>>,
    /// Whether it plays files a page streamed in (it says how loud,
    /// [`crate::files::status`]).
    streamed: bool,
}

/// What a rig plays, built: a patch's chain, or a profile's scenes.
#[derive(Default)]
pub struct Built {
    patch: Option<BassRig>,
    profile: Option<ProfileRig>,
    /// The profile and scene it was built on (a profile's name as the
    /// library spells it).
    named: Option<(String, Option<String>)>,
}

/// The scene `state` names among its profile's, else the profile's default.
fn scene_in(state: &RigState) -> usize {
    let Some(found) = profile::profile(state.profile.as_deref().unwrap_or("")) else {
        return 0;
    };
    state
        .scene
        .as_deref()
        .and_then(|n| {
            found
                .patches
                .iter()
                .position(|s| s.name.eq_ignore_ascii_case(n))
        })
        .unwrap_or(found.default_patch)
}

/// What `state` says, built at `sample_rate` for blocks of `block_size`.
/// Heavy (captures loaded, IRs read): never on the audio thread.
fn built(state: &RigState, sample_rate: f64, block_size: u32) -> Result<Built, PluginError> {
    if state.is_patch() {
        let mut rig = BassRig::new(sample_rate);
        let bytes = serde_json::to_vec(&serde_json::json!({
            "amp": state.amp,
            "drive": state.drive,
            "cab": state.cab,
        }))
        .map_err(|e| PluginError::LoadFailed(e.to_string()))?;
        rig.load_state(&bytes)?;
        rig.prepare(sample_rate, block_size)?;
        return Ok(Built {
            patch: Some(rig),
            ..Built::default()
        });
    }
    let name = state.profile.clone().unwrap_or_default();
    let profile = ProfileRig::build(&name, scene_in(state), sample_rate, block_size);
    let named = profile
        .as_ref()
        .map(|rig| (rig.profile.clone(), rig.scenes().get(rig.scene()).cloned()));
    Ok(Built {
        profile,
        named,
        ..Built::default()
    })
}

/// The files `state` plays, by their keys.
fn files_of(state: &RigState) -> Vec<String> {
    if state.is_patch() {
        return crate::bass::files_of(&crate::bass::BassState {
            amp: state.amp.clone(),
            drive: state.drive.clone(),
            cab: state.cab.clone(),
            ..crate::bass::BassState::default()
        });
    }
    profile::files_of(state.profile.as_deref().unwrap_or(""))
}

fn gain(db: f64) -> f32 {
    num_traits::ToPrimitive::to_f32(&10_f64.powf(db / 20.0)).unwrap_or(1.0)
}

impl Rig {
    #[must_use]
    pub const fn new(sample_rate: f64) -> Self {
        Self {
            state: RigState {
                profile: None,
                scene: None,
                amp: None,
                drive: None,
                cab: None,
                input_db: 0.0,
                output_db: 0.0,
            },
            patch: None,
            profile: None,
            sample_rate,
            block_size: 0,
            prepared: false,
            arriving: None,
            streamed: false,
        }
    }

    /// The profile's scene names, built or not (the library's profile by
    /// the state's name, else its first).
    fn scenes(&self) -> Vec<String> {
        if let Some(rig) = &self.profile {
            return rig.scenes();
        }
        if self.state.is_patch() {
            return Vec::new();
        }
        profile::profile(self.state.profile.as_deref().unwrap_or(""))
            .map(|p| p.patches.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default()
    }

    /// The scene playing, by index: the built rig's (a switch on the audio
    /// thread names nothing), else the one the state names, else the
    /// profile's default.
    fn scene_index(&self) -> usize {
        if let Some(rig) = &self.profile {
            return rig.scene();
        }
        let scenes = self.scenes();
        let named = self
            .state
            .scene
            .as_deref()
            .and_then(|n| scenes.iter().position(|s| s.eq_ignore_ascii_case(n)));
        named.unwrap_or_else(|| {
            profile::profile(self.state.profile.as_deref().unwrap_or(""))
                .map_or(0, |p| p.default_patch)
        })
    }

    /// Build what the state says, at the size last prepared for — on the
    /// caller's thread, never the audio's. In a page, with what has
    /// arrived from the rig store, and the rest waited for.
    fn build(&mut self) -> Result<(), PluginError> {
        let built = built(&self.state, self.sample_rate, self.block_size)?;
        self.adopt(built);
        self.wait_for_files();
        Ok(())
    }

    /// Play `built`, handing back what played before.
    fn adopt(&mut self, mut built: Built) -> Built {
        std::mem::swap(&mut self.patch, &mut built.patch);
        std::mem::swap(&mut self.profile, &mut built.profile);
        if let Some((profile, scene)) = built.named.take() {
            self.state.profile = Some(profile);
            self.state.scene = scene;
        }
        built
    }

    /// In a page: wait for the files the state plays that have not arrived,
    /// to be built again with them ([`crate::files::wait`]). On a desktop
    /// every file is on disk and nothing waits.
    fn wait_for_files(&mut self) {
        self.arriving = None;
        if cfg!(not(target_arch = "wasm32")) {
            return;
        }
        let files = files_of(&self.state);
        let missing: Vec<String> = files
            .iter()
            .filter(|k| crate::files::bytes(k).is_none())
            .cloned()
            .collect();
        self.streamed = !files.is_empty() && missing.is_empty();
        if missing.is_empty() {
            return;
        }
        let (state, rate, block) = (self.state.clone(), self.sample_rate, self.block_size);
        tracing::info!(
            rig.files = missing.len(),
            "bass rig: waiting for its files from the rig store"
        );
        self.arriving = Some(crate::files::wait(missing, move || {
            built(&state, rate, block).unwrap_or_else(|e| {
                tracing::warn!(why = %e, "bass rig: built again, it would not load");
                Built::default()
            })
        }));
    }

    /// At a block's start: the chain its files arrived for, taken (the one
    /// it replaces handed back to be dropped off this thread).
    fn take_arrived(&mut self) {
        // A count taken, not a copy: the slot list holds it too, so
        // letting go of it here frees nothing.
        let Some(slot) = self.arriving.clone() else {
            return;
        };
        let Some(built) = slot.take() else { return };
        // A built profile names itself as the state does already: nothing
        // to keep from `named`, and no string made here.
        let mut built = built;
        built.named = None;
        let old = self.adopt(built);
        slot.retire(old);
        self.arriving = None;
        self.streamed = true;
    }
}

impl PluginInstance for Rig {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fts.signal.bass_rig".into(),
            name: crate::BASS_RIG.into(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        let trim = |id, name: &str| PluginParamInfo {
            id,
            name: name.into(),
            min: -TRIM_DB,
            max: TRIM_DB,
            default: 0.0,
        };
        vec![
            trim(INPUT, "Input"),
            trim(OUTPUT, "Output"),
            PluginParamInfo {
                id: SCENE,
                name: "Scene".into(),
                min: 0.0,
                max: 1.0,
                default: 0.0,
            },
        ]
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            INPUT => Some(self.state.input_db),
            OUTPUT => Some(self.state.output_db),
            SCENE => Some(profile::normalized(self.scene_index(), self.scenes().len())),
            _ => None,
        }
    }

    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        match id {
            INPUT | OUTPUT => Some(format!("{value:.1} dB")),
            SCENE => {
                let scenes = self.scenes();
                scenes.get(profile::scene_at(value, scenes.len())).cloned()
            }
            _ => None,
        }
    }

    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        match id {
            INPUT | OUTPUT => text
                .trim()
                .trim_end_matches("dB")
                .trim()
                .parse::<f64>()
                .ok()
                .map(|v| v.clamp(-TRIM_DB, TRIM_DB)),
            SCENE => {
                let scenes = self.scenes();
                scenes
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(text.trim()))
                    .map(|i| profile::normalized(i, scenes.len()))
            }
            _ => None,
        }
    }

    fn param_choices(&mut self, id: u32) -> Vec<(f64, String)> {
        if id != SCENE {
            return Vec::new();
        }
        let scenes = self.scenes();
        let count = scenes.len();
        scenes
            .into_iter()
            .enumerate()
            .map(|(i, name)| (profile::normalized(i, count), name))
            .collect()
    }

    fn latency(&mut self) -> u32 {
        match (&mut self.patch, &mut self.profile) {
            (Some(rig), _) => rig.latency(),
            (_, Some(rig)) => rig.latency(),
            _ => 0,
        }
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate;
        self.block_size = block_size;
        self.build()?;
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
        for &(id, value) in events.params {
            match id {
                INPUT => self.state.input_db = value.clamp(-TRIM_DB, TRIM_DB),
                OUTPUT => self.state.output_db = value.clamp(-TRIM_DB, TRIM_DB),
                SCENE => {
                    if let Some(rig) = &mut self.profile {
                        let count = rig.chains.len();
                        rig.switch(profile::scene_at(value, count));
                    }
                }
                _ => {}
            }
        }
        let played = if let Some(rig) = &mut self.profile {
            rig.process(
                (in_l, in_r),
                (out_l, out_r),
                (gain(self.state.input_db), gain(self.state.output_db)),
            )
        } else if let Some(rig) = &mut self.patch {
            let trims = [(INPUT, self.state.input_db), (OUTPUT, self.state.output_db)];
            rig.process_block(
                in_l,
                in_r,
                out_l,
                out_r,
                &PluginEvents {
                    params: &trims,
                    ..PluginEvents::EMPTY
                },
            )
        } else {
            out_l.copy_from_slice(in_l);
            out_r.copy_from_slice(in_r);
            Ok(())
        };
        if self.streamed {
            let peak = out_l
                .iter()
                .chain(out_r.iter())
                .fold(0.0_f32, |m, s| m.max(s.abs()));
            crate::files::note_heard(peak);
        }
        played
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.state = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("bass rig state: {e}")))?;
        if self.prepared {
            self.build()?;
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        if let Some(rig) = &self.profile {
            self.state.scene = rig.scenes().get(rig.scene()).cloned();
        }
        serde_json::to_vec(&self.state).map_err(|e| PluginError::LoadFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}
