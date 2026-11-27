//! `FTS Kit Mic`: a kit's sampled voices for some of its pieces and mics,
//! played from the track's own MIDI.
//!
//! A signal kit is a `.signalpreset`: pieces (`kick`, `snare-a`, `rtom-a`
//! …), each an engine over a pack of multi-mic recordings — one file per
//! mic per hit (`In 1`, `In 2`, `Sub`, `Overhead`, `Room Close` …) — and a
//! note map. signal plays one [`SampleEngine`] per (piece, mic), each
//! solo'd to its mic, and keeps a piece's mics together by giving every
//! one the same notes: the round robin is chosen the same way from the
//! same sequence.
//!
//! This does the same on a session's own tracks. Which pieces and mics it
//! plays is its state, so the kit lands where the recorded one is:
//!
//! - a piece's **close** mics on that piece's Sample track,
//! - every piece's **overhead** mics on a sample track into the kit's
//!   overheads, its **room** mics on one into the rooms —
//!
//! each fed the same notes for a piece, so the hits agree across them.
//!
//! With `split`, one Kit Mic plays a piece's every mic on its Sample
//! track: the close mics on the main pair, the overheads on 3/4 and the
//! rooms on 5/6 (`PluginInstance::aux_out`), which the track sends into
//! the recorded overheads and rooms — no track of their own.
//!
//! State, JSON: `{"preset": "…/Heavy Thwacky.signalpreset", "pieces":
//! ["kick"], "mics": ["close"], "gain_db": 0, "split": false}`. `pieces` absent: every
//! piece. A mic is a group — `close` (any not a bus mic), `overhead`,
//! `room` — or a mic's own name.
//!
//! **In a page** the kit's preset, engines and packs come from the rig
//! store ([`crate::files`], its kits under `drums/`, as a desktop's
//! `Drum Kits` folder). It waits for them in stages — the preset, the
//! engines it names, each pack's header and index — and then for only the
//! samples of the mics it plays, each read out of its pack by range. Until
//! they are in it is silent; once they are, it is built on the page's
//! thread and takes its voices at the next block.

use std::path::{Path, PathBuf};

use daw_standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use std::sync::Arc;

use sampler::SampleEngine;
use sampler::engine_spec::EngineSpec;
use sampler::kit_routing::KitRouting;
use sampler::preset_spec::{PresetEngineRef, PresetSpec};

use crate::files::{self, Need};

/// What the voices play, as saved.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KitState {
    /// The kit, a `.signalpreset`.
    #[serde(default)]
    pub preset: Option<PathBuf>,
    /// Which of its pieces, by the preset's ids; none named, all of them.
    #[serde(default)]
    pub pieces: Option<Vec<String>>,
    /// Which mics: `close`, `overhead`, `room`, or a mic's name.
    #[serde(default)]
    pub mics: Vec<String>,
    /// Level, dB.
    #[serde(default)]
    pub gain_db: f64,
    /// Close mics on the main pair, overheads on 3/4, rooms on 5/6 —
    /// rather than every mic on the main pair.
    #[serde(default)]
    pub split: bool,
}

/// Which output a mic plays on when split: 0 the main pair (a close
/// mic), 1 the 3/4 pair (an overhead), 2 the 5/6 pair (a room).
#[must_use]
pub fn output_of(id: &str) -> usize {
    if wants(&["overhead".to_owned()], id) {
        1
    } else if wants(&["room".to_owned()], id) {
        2
    } else {
        0
    }
}

/// Whether the mic `id` is one `wanted` names.
#[must_use]
pub fn wants(wanted: &[String], id: &str) -> bool {
    let bus = sampler::mixer::mic_is_bus(id);
    let lower = id.to_ascii_lowercase();
    wanted
        .iter()
        .any(|w| match w.trim().to_ascii_lowercase().as_str() {
            "close" => !bus,
            "overhead" | "oh" => bus && (lower == "oh" || lower.contains("overhead")),
            "room" => bus && (lower.starts_with("room") || lower.contains("ambien")),
            exact => exact == lower,
        })
}

/// The piece the kit `preset` plays for `note`, by its id (`kick`,
/// `rtom-a`).
///
/// As its note map says — the first of them, for a layered note. `None`
/// for a preset that will not read, or a note it maps to nothing.
#[must_use]
pub fn piece_for_note(preset: &Path, note: u8) -> Option<String> {
    let spec = sampler::preset_spec::PresetSpec::from_file(preset).ok()?;
    spec.note_routing
        .iter()
        .find(|r| r.note == note)
        .and_then(|r| r.targets.first().cloned())
}

/// One piece's voices: its id, its preset settings, an engine per mic.
struct Piece {
    id: String,
    transpose: i8,
    gain: f32,
    /// Each mic's engine and the output it plays on ([`output_of`]).
    engines: Vec<(SampleEngine, usize)>,
}

/// What a kit plays, built: its pieces' voices and its note map.
#[derive(Default)]
pub struct Voices {
    pieces: Vec<Piece>,
    routing: KitRouting,
}

/// The kit's voices.
pub struct KitMic {
    state: KitState,
    sample_rate: f64,
    /// The rate the voices were built at.
    built_at: f64,
    prepared: bool,
    routing: KitRouting,
    pieces: Vec<Piece>,
    /// In a page: the voices made once their files have arrived from the
    /// rig store ([`crate::files`]), for the next block to take.
    arriving: Option<Arc<files::Slot<Voices>>>,
    /// Whether it plays samples a page streamed in (it says how loud,
    /// [`crate::files::status`]).
    streamed: bool,
    /// One engine's block, interleaved stereo.
    scratch: Vec<f32>,
    /// The 3/4 and 5/6 outputs' last block, left and right.
    aux: [(Vec<f32>, Vec<f32>); 2],
    /// How long the last block was.
    frames: usize,
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve(path: &str, base: &Path) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() { p } else { base.join(p) }
}

fn gain(db: f64) -> f32 {
    num_traits::ToPrimitive::to_f32(&10_f64.powf(db / 20.0)).unwrap_or(1.0)
}

impl KitMic {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self {
            state: KitState::default(),
            sample_rate,
            built_at: 0.0,
            prepared: false,
            routing: KitRouting::default(),
            pieces: Vec::new(),
            arriving: None,
            streamed: false,
            scratch: Vec::new(),
            aux: Default::default(),
            frames: 0,
        }
    }

    /// Build the voices the state names, at its rate — on the caller's
    /// thread, never the audio's. On a desktop from disk; in a page from
    /// the rig store, with what has arrived, waiting for the rest.
    fn build(&mut self) {
        self.built_at = self.sample_rate;
        self.arriving = None;
        self.streamed = false;
        let rate = num_traits::ToPrimitive::to_u32(&self.sample_rate.round()).unwrap_or(48_000);
        if cfg!(not(target_arch = "wasm32")) {
            let voices = voices_on_disk(&self.state, rate);
            self.adopt(voices);
            return;
        }
        match voices_in_store(&self.state, rate) {
            Ok(voices) => {
                self.streamed = !voices.pieces.is_empty();
                self.adopt(voices);
            }
            Err(Need::Keys(keys)) => {
                self.adopt(Voices::default());
                tracing::info!(
                    rig.files = keys.len(),
                    "kit mic: waiting for its files from the rig store"
                );
                let state = self.state.clone();
                self.arriving = Some(files::wait_for(keys, move || voices_in_store(&state, rate)));
            }
            Err(Need::Failed(why)) => {
                self.adopt(Voices::default());
                tracing::warn!(why, "kit mic: the kit would not load from the rig store");
            }
        }
    }

    /// Play `voices`, handing back what played before.
    fn adopt(&mut self, mut voices: Voices) -> Voices {
        std::mem::swap(&mut self.pieces, &mut voices.pieces);
        std::mem::swap(&mut self.routing, &mut voices.routing);
        voices
    }

    /// At a block's start: the voices their files arrived for, taken (the
    /// ones they replace handed back to be dropped off this thread).
    fn take_arrived(&mut self) {
        let Some(slot) = self.arriving.clone() else {
            return;
        };
        let Some(voices) = slot.take() else { return };
        let old = self.adopt(voices);
        slot.retire(old);
        self.arriving = None;
        self.streamed = !self.pieces.is_empty();
    }

    /// One note to the pieces it routes to.
    fn note(&mut self, note: u8, velocity: u8, on: bool) {
        let targets: Vec<(String, Option<String>)> = match self.routing.targets(note) {
            Some(targets) => targets
                .iter()
                .map(|t| {
                    (
                        t.piece.clone(),
                        (!t.articulation.is_empty()).then(|| t.articulation.clone()),
                    )
                })
                .collect(),
            None if self.routing.is_empty() => {
                self.pieces.iter().map(|p| (p.id.clone(), None)).collect()
            }
            None => Vec::new(),
        };
        for (id, articulation) in targets {
            let Some(piece) = self.pieces.iter_mut().find(|p| p.id == id) else {
                continue;
            };
            let key = u8::try_from(
                i16::from(note)
                    .saturating_add(i16::from(piece.transpose))
                    .clamp(0, 127),
            )
            .unwrap_or(note);
            for (engine, _) in &mut piece.engines {
                if on {
                    engine.note_on_articulated(key, velocity, articulation.as_deref());
                } else {
                    engine.note_off(key);
                }
            }
        }
    }

    /// One note as the main pair plays it, mono (left and right
    /// averaged), `frames` long from the note: a hit's sound, offline.
    /// Prepare first, unsplit; silence before that.
    #[must_use]
    pub fn hit_sound(&mut self, note: u8, velocity: u8, frames: usize) -> Vec<f32> {
        let block = self.scratch.len() / 2;
        let (mut l, mut r) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
        if block == 0 || self.state.split {
            return l;
        }
        self.note(note, velocity, true);
        for at in (0..frames).step_by(block) {
            self.render(&mut l, &mut r, (at, block.min(frames - at)));
        }
        l.iter().zip(&r).map(|(a, b)| (a + b) * 0.5).collect()
    }

    /// Every voice's next `frames`, summed into its output from `at`: the
    /// main pair `out`, or the aux pairs.
    fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32], (at, frames): (usize, usize)) {
        let level = gain(self.state.gain_db);
        let want = frames.saturating_mul(2);
        let Self {
            pieces,
            scratch,
            aux,
            ..
        } = self;
        for piece in pieces.iter_mut() {
            for (engine, output) in &mut piece.engines {
                let Some(scratch) = scratch.get_mut(..want) else {
                    return;
                };
                scratch.fill(0.0);
                engine.render(scratch);
                let g = piece.gain * level;
                let (to_l, to_r): (&mut [f32], &mut [f32]) = match output.checked_sub(1) {
                    None => (&mut *out_l, &mut *out_r),
                    Some(pair) => match aux.get_mut(pair) {
                        Some((l, r)) => (l.as_mut_slice(), r.as_mut_slice()),
                        None => continue,
                    },
                };
                let (Some(ls), Some(rs)) = (to_l.get_mut(at..), to_r.get_mut(at..)) else {
                    return;
                };
                for ((l, r), pair) in ls
                    .iter_mut()
                    .zip(rs.iter_mut())
                    .zip(scratch.chunks_exact(2))
                {
                    if let [left, right] = pair {
                        *l += left * g;
                        *r += right * g;
                    }
                }
            }
        }
    }
}

/// Whether `er` is a piece `state` plays.
fn plays(state: &KitState, er: &PresetEngineRef) -> bool {
    state
        .pieces
        .as_ref()
        .is_none_or(|only| only.iter().any(|p| p.eq_ignore_ascii_case(&er.id)))
}

/// The mics of `patch` that `state` wants.
fn mics_of(state: &KitState, patch: &sampler::PlayerPatch) -> Vec<String> {
    patch
        .spec
        .mics
        .iter()
        .map(|m| m.id.clone())
        .filter(|mic| wants(&state.mics, mic))
        .collect()
}

/// The samples of `mic` in `patch`, by the paths its zones name.
fn mic_paths(patch: &sampler::PlayerPatch, mic: &str) -> Vec<PathBuf> {
    patch
        .spec
        .zones
        .iter()
        .zip(patch.zone_paths.iter())
        .filter(|(z, _)| z.mic == mic)
        .map(|(_, p)| p.clone())
        .collect()
}

/// One piece's engines over `patch`, one per wanted mic, each preloaded
/// with its mic's samples only.
fn piece(
    state: &KitState,
    er: &PresetEngineRef,
    spec: &EngineSpec,
    patch: &sampler::PlayerPatch,
    rate: u32,
) -> Piece {
    let section = patch
        .spec
        .sections
        .first()
        .map(|s| s.id.clone())
        .unwrap_or_default();
    let mut gain_db = 0.0_f64;
    for ov in spec.block.overrides.iter().chain(er.overrides.iter()) {
        if matches!(ov.param.as_str(), "gain_db" | "gain") {
            gain_db += f64::from(ov.value);
        }
    }
    let mut engines = Vec::new();
    for mic in mics_of(state, patch) {
        let mut engine = SampleEngine::new(patch.clone(), rate, section.clone(), mic.clone());
        engine.set_solo_mic(Some(mic.clone()));
        if !er.articulation.is_empty() {
            engine.pin_articulation(Some(er.articulation.clone()));
        }
        if !er.choke_group.is_empty() {
            engine.set_choke_group(Some(&er.choke_group), &er.choke_on);
        }
        let paths = mic_paths(patch, &mic);
        let _ = engine
            .cache_handle()
            .preload(paths.iter().map(PathBuf::as_path));
        let output = if state.split { output_of(&mic) } else { 0 };
        engines.push((engine, output));
    }
    Piece {
        id: er.id.clone(),
        transpose: er.transpose,
        gain: gain(gain_db),
        engines,
    }
}

/// The voices `state` names, read from disk (a desktop's). Heavy — packs
/// read, samples decoded — so never on the audio thread.
fn voices_on_disk(state: &KitState, rate: u32) -> Voices {
    let Some(preset_path) = state.preset.clone() else {
        return Voices::default();
    };
    let preset = match PresetSpec::from_file(&preset_path) {
        Ok(p) => p,
        Err(why) => {
            tracing::warn!(kit.preset = %preset_path.display(), why = %why, "kit mic: the preset would not load");
            return Voices::default();
        }
    };
    let dir = preset_path.parent().unwrap_or_else(|| Path::new(""));
    let mut pieces = Vec::new();
    for er in preset.engines.iter().filter(|er| plays(state, er)) {
        match piece_on_disk(state, er, dir, rate) {
            Ok(piece) if !piece.engines.is_empty() => pieces.push(piece),
            Ok(_) => {}
            Err(why) => tracing::warn!(kit.piece = %er.id, why, "kit mic: a piece would not load"),
        }
    }
    tracing::info!(
        kit.preset = %preset_path.display(),
        kit.pieces = pieces.len(),
        kit.engines = pieces.iter().map(|p| p.engines.len()).sum::<usize>(),
        "kit mic: voices loaded"
    );
    Voices {
        pieces,
        routing: KitRouting::from_preset(&preset),
    }
}

/// One piece from disk: its engine spec beside the preset, its pack beside
/// the engine.
#[cfg(not(target_arch = "wasm32"))]
fn piece_on_disk(
    state: &KitState,
    er: &PresetEngineRef,
    dir: &Path,
    rate: u32,
) -> Result<Piece, String> {
    let engine_path = resolve(&er.engine, dir);
    let spec = EngineSpec::from_file(&engine_path).map_err(|e| e.to_string())?;
    let pack = resolve(
        &spec.block.pack,
        engine_path.parent().unwrap_or_else(|| Path::new("")),
    );
    let patch = sampler::PlayerPatch::from_pack(&pack).map_err(|e| e.to_string())?;
    Ok(piece(state, er, &spec, &patch, rate))
}

/// A page has no disk: its kits are in the rig store ([`voices_in_store`]).
#[cfg(target_arch = "wasm32")]
fn piece_on_disk(
    _state: &KitState,
    _er: &PresetEngineRef,
    _dir: &Path,
    _rate: u32,
) -> Result<Piece, String> {
    Err("a page has no disk".to_owned())
}

/// One piece's files as the store has them, once in: its engine spec and
/// its pack's patch (opened over the spans read of it).
struct Stored {
    er: PresetEngineRef,
    spec: EngineSpec,
    patch: sampler::PlayerPatch,
}

/// The voices `state` names, from the rig store: what a page plays. Each
/// stage's files are wanted together (every piece's engine, then every
/// pack's header, its index, then the samples of the mics played), so a
/// kit comes in a few round trips, whatever its size. A piece whose files
/// are not a kit's is left out, as on a desktop.
///
/// # Errors
///
/// [`Need::Keys`] for the next stage's files; [`Need::Failed`] for a
/// preset that will not read.
pub fn voices_in_store(state: &KitState, rate: u32) -> Result<Voices, Need> {
    let Some(preset_path) = &state.preset else {
        return Ok(Voices::default());
    };
    let key = files::kit_located(&preset_path.to_string_lossy());
    let preset =
        PresetSpec::parse(&files::text(&key)?).map_err(|e| Need::Failed(format!("{key}: {e}")))?;
    let dir = files::dir_of(&key);
    let chosen: Vec<&PresetEngineRef> = preset
        .engines
        .iter()
        .filter(|er| plays(state, er))
        .collect();

    // The engines.
    let mut need: Option<Need> = None;
    let mut engines = Vec::new();
    for er in chosen {
        let at = files::joined(dir, &er.engine);
        match files::text(&at) {
            Ok(text) => match EngineSpec::parse(&text) {
                Ok(spec) => engines.push((er, at, spec)),
                Err(e) => {
                    tracing::warn!(kit.piece = %er.id, why = %e, "kit mic: an engine would not read")
                }
            },
            Err(n) => {
                need = Some(match need {
                    Some(m) => m.and(n),
                    None => n,
                })
            }
        }
    }
    if let Some(need) = need {
        return Err(need);
    }

    // Their packs, opened over their header and index.
    let mut stored = Vec::new();
    let mut need: Option<Need> = None;
    for (er, at, spec) in engines {
        let pack_key = files::joined(files::dir_of(&at), &spec.block.pack);
        let opened = files::pack(&pack_key).and_then(|pack| {
            sampler::PlayerPatch::from_opened_pack(pack.clone())
                .map(|patch| (pack, patch))
                .map_err(|e| Need::Failed(format!("{pack_key}: {e}")))
        });
        match opened {
            Ok((pack, patch)) => stored.push((
                pack_key,
                pack,
                Stored {
                    er: er.clone(),
                    spec,
                    patch,
                },
            )),
            Err(Need::Failed(why)) => {
                tracing::warn!(kit.piece = %er.id, why, "kit mic: a pack would not open")
            }
            Err(n) => {
                need = Some(match need {
                    Some(m) => m.and(n),
                    None => n,
                })
            }
        }
    }
    if let Some(need) = need {
        return Err(need);
    }

    // The samples of the mics it plays: only those, by range.
    let missing: Vec<String> = stored
        .iter()
        .flat_map(|(pack_key, pack, piece)| {
            let paths: Vec<PathBuf> = mics_of(state, &piece.patch)
                .iter()
                .flat_map(|mic| mic_paths(&piece.patch, mic))
                .collect();
            files::entries_of(pack_key, pack, paths.iter().map(PathBuf::as_path))
        })
        .filter(|k| !files::has(k))
        .collect();
    if !missing.is_empty() {
        return Err(Need::Keys(missing));
    }

    let pieces: Vec<Piece> = stored
        .iter()
        .map(|(_, _, p)| piece(state, &p.er, &p.spec, &p.patch, rate))
        .filter(|p| !p.engines.is_empty())
        .collect();
    tracing::info!(
        kit.preset = %key,
        kit.pieces = pieces.len(),
        kit.engines = pieces.iter().map(|p| p.engines.len()).sum::<usize>(),
        "kit mic: voices loaded from the rig store"
    );
    Ok(Voices {
        pieces,
        routing: KitRouting::from_preset(&preset),
    })
}

impl PluginInstance for KitMic {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fts.signal.kit_mic".into(),
            name: crate::KIT_MIC.into(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }

    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }

    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }

    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }

    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate;
        if (self.built_at - sample_rate).abs() > f64::EPSILON {
            self.build();
        }
        let frames = usize::try_from(block_size).unwrap_or(4_096);
        self.scratch.resize(frames.saturating_mul(2), 0.0);
        for (l, r) in &mut self.aux {
            l.resize(frames, 0.0);
            r.resize(frames, 0.0);
        }
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        use daw_proto::MidiEvent;
        self.take_arrived();
        let frames = out_l.len().min(out_r.len());
        out_l.fill(0.0);
        out_r.fill(0.0);
        self.frames = frames;
        for (l, r) in &mut self.aux {
            if l.len() < frames || r.len() < frames {
                // A block longer than prepared: grown once, not per block.
                l.resize(frames, 0.0);
                r.resize(frames, 0.0);
            }
            l.fill(0.0);
            r.fill(0.0);
        }
        // Each note where it falls in the block: the voices rendered up to
        // it, then the note.
        let mut at = 0_usize;
        for event in events.midi {
            let offset = usize::try_from(event.offset).unwrap_or(0).min(frames);
            if offset > at {
                self.render(out_l, out_r, (at, offset.saturating_sub(at)));
                at = offset;
            }
            match &event.message {
                MidiEvent::NoteOn { key, velocity, .. } => {
                    let on = velocity.get() > 0;
                    self.note(key.get(), velocity.get(), on);
                }
                MidiEvent::NoteOff { key, .. } => self.note(key.get(), 0, false),
                _ => {}
            }
        }
        if frames > at {
            self.render(out_l, out_r, (at, frames.saturating_sub(at)));
        }
        if self.streamed {
            let peak = out_l
                .iter()
                .chain(out_r.iter())
                .fold(0.0_f32, |m, s| m.max(s.abs()));
            files::note_heard_kit(peak);
        }
        Ok(())
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.state = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("kit mic state: {e}")))?;
        self.build();
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        serde_json::to_vec(&self.state).map_err(|e| PluginError::LoadFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }

    fn aux_out(&self, pair: usize) -> Option<(&[f32], &[f32])> {
        if !self.state.split {
            return None;
        }
        let (l, r) = self.aux.get(pair)?;
        Some((l.get(..self.frames)?, r.get(..self.frames)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.signalpack` of raw 16-bit PCM (kind 7): one kick hit per mic, a
    /// close and a room, each a short decaying pulse, the room's long.
    fn pack() -> (Vec<u8>, [(u64, u64); 3]) {
        let spec = r#"name "Test Kick"
sections ( { id main, label Main, lowest_note "C-1", highest_note "G9" } )
mics ( { id Close, label Close, kind close } { id Room, label Room, kind room } )
articulations ( { id hit, label Hit, kind @Short, rr 1 } )
zones (
    { file "close.wav", key_min 36, key_max 36, root_key 36, vel_min 0, vel_max 127, articulation hit, mic Close }
    { file "room.wav", key_min 36, key_max 36, root_key 36, vel_min 0, vel_max 127, articulation hit, mic Room }
)
"#;
        let hit = |frames: usize| -> Vec<u8> {
            (0..frames)
                .flat_map(|i| {
                    let level = 0.8 * (-(i as f64) / 200.0).exp();
                    ((level * f64::from(i16::MAX)) as i16).to_le_bytes()
                })
                .collect()
        };
        let (close, room) = (hit(2_000), hit(20_000));
        let mut out = vec![0_u8; 64];
        let close_at = (64_u64, close.len() as u64);
        out.extend_from_slice(&close);
        let room_at = (out.len() as u64, room.len() as u64);
        out.extend_from_slice(&room);
        let index_at = out.len() as u64;
        let mut index =
            format!("# signalpack-index-v1\n# spec_format\tstyx\n# spec_begin\n{spec}# spec_end\n");
        index.push_str(&format!(
            "close.wav\t{}\t{}\t1\t48000\t2000\t2000\n",
            close_at.0, close_at.1
        ));
        index.push_str(&format!(
            "room.wav\t{}\t{}\t1\t48000\t20000\t20000\n",
            room_at.0, room_at.1
        ));
        out.extend_from_slice(index.as_bytes());
        let header =
            sampler::engine::cache::PackFileHeader::new(7, index_at, index.len() as u64, 2);
        out[..64].copy_from_slice(header.as_bytes());
        (out, [close_at, room_at, (index_at, index.len() as u64)])
    }

    #[test]
    fn a_kit_comes_from_the_store_in_stages_with_only_its_mics_samples() {
        let root = "/rigs/drums/Test Kit";
        let preset = format!("{root}/Presets/Kit.signalpreset");
        let engine = format!("{root}/Engines/Kick.signalengine");
        let pack_key = format!("{root}/Packs/Kick.signalpack");
        let (bytes, [close, room, index]) = pack();
        let store: Vec<(String, Vec<u8>)> = vec![
            (
                preset.clone(),
                br#"name "Test Kit"
engines ( { id kick, engine "../Engines/Kick.signalengine" } )
note_routing ( { note 36, targets ( kick ) } )
"#
                .to_vec(),
            ),
            (
                engine.clone(),
                br#"name "Kick"
block { pack "../Packs/Kick.signalpack" }
"#
                .to_vec(),
            ),
        ];
        let state = KitState {
            preset: Some(preset.clone().into()),
            mics: vec!["close".to_owned()],
            ..KitState::default()
        };

        let mut stages = Vec::new();
        let voices = loop {
            match voices_in_store(&state, 48_000) {
                Ok(voices) => break voices,
                Err(Need::Keys(keys)) => {
                    assert!(stages.len() < 6, "too many stages: {stages:?}");
                    for key in &keys {
                        let (whole, span) = files::range_of(key);
                        let got = match span {
                            Some((start, len)) => {
                                assert_eq!(whole, pack_key, "a span of the pack only");
                                bytes[start as usize..(start + len) as usize].to_vec()
                            }
                            None => store
                                .iter()
                                .find(|(k, _)| k == key)
                                .unwrap_or_else(|| panic!("the store has no {key}"))
                                .1
                                .clone(),
                        };
                        files::install(key.clone(), got);
                    }
                    stages.push(keys);
                }
                Err(Need::Failed(why)) => panic!("the kit failed: {why}"),
            }
        };

        assert_eq!(
            stages,
            vec![
                vec![preset],
                vec![engine],
                vec![files::range_key(&pack_key, 0, 64)],
                vec![files::range_key(&pack_key, index.0, index.1)],
                vec![files::range_key(&pack_key, close.0, close.1)],
            ],
            "the preset, its engine, the pack's header and index, then the close mic's samples"
        );
        assert!(
            !stages
                .concat()
                .contains(&files::range_key(&pack_key, room.0, room.1)),
            "the room mic it does not play was never wanted"
        );
        assert_eq!(voices.pieces.len(), 1);
        assert_eq!(
            voices.pieces[0].engines.len(),
            1,
            "one engine: the close mic"
        );

        let mut kit = KitMic::new(48_000.0);
        kit.prepare(48_000.0, 256).expect("prepared");
        kit.adopt(voices);
        let sound = kit.hit_sound(36, 127, 1_024);
        let peak = sound.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(
            peak > 0.1,
            "the kick sounds from the streamed samples: peak {peak}"
        );
    }

    #[test]
    fn a_desktop_kit_path_is_the_same_kit_in_the_store() {
        assert_eq!(
            files::kit_key(
                "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Presets/Heavy Thwacky.signalpreset"
            ),
            "/rigs/drums/GGD Modern and Massive 2/Presets/Heavy Thwacky.signalpreset"
        );
        assert_eq!(
            files::kit_key("D:\\Kits\\Mine\\Presets\\Big.signalpreset"),
            "/rigs/drums/Mine/Presets/Big.signalpreset"
        );
        assert_eq!(
            files::kit_key("D:/Kits/Mine/Presets/Big.signalpreset"),
            "/rigs/drums/Mine/Presets/Big.signalpreset"
        );
        assert_eq!(
            files::kit_key("Mine/Presets/Big.signalpreset"),
            "/rigs/drums/Mine/Presets/Big.signalpreset",
            "a relative path is under the kits"
        );
        assert_eq!(
            files::joined("/rigs/drums/Mine/Presets", "../Packs/Kick/A b.signalpack"),
            "/rigs/drums/Mine/Packs/Kick/A b.signalpack"
        );
    }
}
