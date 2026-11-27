//! The bass rig's **profile mode**: a signal bass profile, a Scene
//! parameter switching between its scenes.
//!
//! A scene (Clean, Crunch, Drive …) is a whole chain — gate, compressor,
//! drive, NAM amp, EQ … — played in a session's track.
//!
//! Every scene's chain is built when the rig is prepared and stays loaded,
//! so a switch is a crossfade and not a load: the incoming chain fades in
//! over [`FADE_SECS`] while the outgoing one fades out (equal power), and
//! then the outgoing one keeps running on silence until its tail has rung
//! out — a delay's repeats and a reverb's decay carry on under the new
//! scene, as on signal's own rig. Switching back to a scene still ringing
//! picks it up as it is.
//!
//! The scenes come from Session's **own copy** of signal's bass library
//! (`<config>/fts/bass-rig`), seeded from signal's shipped bass defaults
//! the first time it is read — never the player's live rig library, whose
//! scenes and order are a gig's.
//!
//! The Scene parameter is stepped, 0..1 along the profile's scenes, its
//! choices their names: automated, a lane of it switches the bass's sound
//! section by section.
//!
//! The guitar's profiles build the same way ([`ProfileRig::build_in`]):
//! in a page that is `FTS Signal Rig`, the whole guitar or bass rig, its
//! patches the scenes (`crate::signal_rig`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use daw_standalone::plugin::{PluginError, PluginEvents, PluginInstance};
use sampler::rig::{RigBlock, prepare_chain};
use sampler::rig_profile::RigProfile;

use guitar::instrument::Instrument;

/// A switch's crossfade, seconds.
pub const FADE_SECS: f64 = 0.008;
/// A tail quieter than this (−90 dBFS) …
const QUIET: f32 = 3.2e-5;
/// … for this long has rung out.
const QUIET_HOLD_SECS: f64 = 2.0;
/// No tail runs longer than this.
const MAX_TAIL_SECS: f64 = 30.0;
/// Outgoing chains ringing at once; a switch past this many stops the
/// oldest.
const MAX_RINGING: usize = 4;

/// Session's copy of signal's bass library: where it is, and its profiles
/// as the rig plays them.
pub struct Library {
    pub dir: PathBuf,
    pub profiles: Vec<RigProfile>,
}

/// Where Session keeps its copy of the bass library (`FTS_BASS_RIG_DIR`
/// overrides — a test's own folder).
#[must_use]
pub fn library_dir() -> PathBuf {
    library_dir_of(Instrument::Bass)
}

/// Where Session keeps its copy of `instrument`'s library: in a page, the
/// home it streams from the rig store; on a desktop `<config>/fts/<id>-rig`
/// (`FTS_BASS_RIG_DIR` overrides the bass's).
#[must_use]
pub fn library_dir_of(instrument: Instrument) -> PathBuf {
    if cfg!(target_arch = "wasm32") {
        return crate::files::home(instrument);
    }
    if instrument == Instrument::Bass
        && let Some(dir) = std::env::var_os("FTS_BASS_RIG_DIR").filter(|d| !d.is_empty())
    {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fts")
        .join(format!("{}-rig", instrument.id()))
}

/// The bass library ([`library_of`]).
#[must_use]
pub fn library() -> &'static Library {
    library_of(Instrument::Bass)
}

/// `instrument`'s library, read once per process — seeded from signal's
/// shipped defaults the first time.
#[must_use]
pub fn library_of(instrument: Instrument) -> &'static Library {
    static BASS: OnceLock<Library> = OnceLock::new();
    static GUITAR: OnceLock<Library> = OnceLock::new();
    let once = match instrument {
        Instrument::Bass => &BASS,
        Instrument::Guitar => &GUITAR,
    };
    once.get_or_init(|| {
        let dir = library_dir_of(instrument);
        // The instrument's library in Session's own folder — a home of its
        // own, so the other's keeps its own, and the process's instrument
        // and library are left as they are.
        let home = guitar::RigHome::new(instrument, &dir);
        let library = home.load_or_bootstrap();
        let profiles = library
            .profiles
            .iter()
            .map(|def| guitar::nodes::profile_from_library_in(&home, def, &library.drive_presets))
            .collect();
        Library { dir, profiles }
    })
}

/// The bass profile called `name` ([`profile_in`]).
#[must_use]
pub fn profile(name: &str) -> Option<&'static RigProfile> {
    profile_in(Instrument::Bass, name)
}

/// `instrument`'s profile called `name` (any case), or its library's first.
#[must_use]
pub fn profile_in(instrument: Instrument, name: &str) -> Option<&'static RigProfile> {
    let profiles = &library_of(instrument).profiles;
    profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
        .or_else(|| profiles.first())
}

/// A scene's place among `count`, normalized (0..1).
#[must_use]
pub fn normalized(index: usize, count: usize) -> f64 {
    let last = count.saturating_sub(1);
    if last == 0 {
        return 0.0;
    }
    f64::from(u32::try_from(index).unwrap_or(0)) / f64::from(u32::try_from(last).unwrap_or(1))
}

/// The scene at normalized `value` among `count`.
#[must_use]
pub fn scene_at(value: f64, count: usize) -> usize {
    let last = count.saturating_sub(1);
    let step = (value.clamp(0.0, 1.0) * f64::from(u32::try_from(last).unwrap_or(0))).round();
    num_traits::ToPrimitive::to_usize(&step)
        .unwrap_or(0)
        .min(last)
}

/// A path in the library, made absolute against its folder.
fn resolved(path: &str, dir: &Path) -> String {
    if path.is_empty() {
        return String::new();
    }
    let p = PathBuf::from(path);
    if p.is_relative() {
        dir.join(p).to_string_lossy().into_owned()
    } else {
        path.to_owned()
    }
}

/// A block of a scene, its capture, IR and plugin found in the library —
/// in a page, by their keys in the rig store ([`crate::files::located`]).
fn located_block(block: &RigBlock, dir: &Path, instrument: Instrument) -> RigBlock {
    let mut b = block.clone();
    b.nam = crate::files::located(&resolved(&b.nam, dir), instrument);
    b.ir = crate::files::located(&resolved(&b.ir, dir), instrument);
    b.plugin = resolved(&b.plugin, dir);
    b
}

/// Every capture and IR the profile called `name` plays, across its
/// scenes, as each block names it: what a page fetches before the profile
/// can sound.
#[must_use]
pub fn files_of(name: &str) -> Vec<String> {
    files_in(Instrument::Bass, name)
}

/// [`files_of`] for `instrument`'s profile called `name`.
#[must_use]
pub fn files_in(instrument: Instrument, name: &str) -> Vec<String> {
    let Some(found) = profile_in(instrument, name) else {
        return Vec::new();
    };
    let dir = &library_of(instrument).dir;
    let mut files: Vec<String> = found
        .patches
        .iter()
        .flat_map(|patch| patch.chain.iter())
        .filter(|b| b.has_backend() && !b.bypassed)
        .map(|b| located_block(b, dir, instrument))
        .flat_map(|b| {
            let (is_nam, is_ir) = (b.is_nam(), b.is_cab_ir());
            let nam = is_nam.then_some(b.nam);
            let ir = is_ir.then_some(b.ir);
            nam.into_iter().chain(ir)
        })
        .filter(|f| !f.is_empty())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// One scene, built: its blocks in order and its trims (linear).
pub struct Chain {
    pub name: String,
    blocks: Vec<Box<dyn PluginInstance>>,
    input: f32,
    output: f32,
}

impl Chain {
    /// The blocks a scene plays — those with a backend and not bypassed,
    /// their captures and IRs found in the library — built at `sample_rate`
    /// and prepared for blocks of `block_size`. A block that will not build
    /// leaves the scene without it, said once.
    #[must_use]
    pub fn build(
        patch: &sampler::rig_profile::RigPatch,
        (dir, instrument): (&Path, Instrument),
        sample_rate: f64,
        block_size: u32,
    ) -> Self {
        let blocks: Vec<RigBlock> = patch
            .chain
            .iter()
            .filter(|b| b.has_backend() && !b.bypassed)
            .map(|b| located_block(b, dir, instrument))
            .collect();
        let ids: Vec<String> = (0..blocks.len())
            .map(|i| format!("{}#{i}", patch.name))
            .collect();
        let rate = num_traits::ToPrimitive::to_u32(&sample_rate.round()).unwrap_or(48_000);
        let mut built = match prepare_chain(&blocks, &ids, rate) {
            Ok(chain) => chain.into_blocks().into_iter().map(|(_, b)| b).collect(),
            Err(why) => {
                tracing::warn!(rig.scene = %patch.name, why, "signal rig: a scene would not build");
                Vec::new()
            }
        };
        for block in &mut built {
            if let Err(why) = block.prepare(sample_rate, block_size) {
                tracing::warn!(rig.scene = %patch.name, why = %why, "signal rig: a block would not prepare");
            }
        }
        let gain = |db: f32| 10_f32.powf(db / 20.0);
        Self {
            name: patch.name.clone(),
            blocks: built,
            input: gain(patch.input_trim_db),
            output: gain(patch.output_trim_db),
        }
    }

    /// The chain's latency, samples.
    fn latency(&mut self) -> u32 {
        self.blocks.iter_mut().map(|b| b.latency()).sum()
    }
}

/// What an outgoing chain is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ringing {
    /// Fading out under the incoming one, so many samples in.
    Fading(usize),
    /// Running on silence: samples quiet in a row, and how long it has
    /// rung.
    Tail { quiet: usize, age: usize },
}

/// Scratch the chains run through: a ping-pong pair, and where a chain's
/// output is gathered before it is mixed in.
struct Scratch {
    a: [Vec<f32>; 2],
    b: [Vec<f32>; 2],
    silence: Vec<f32>,
}

impl Scratch {
    fn new(frames: usize) -> Self {
        Self {
            a: [vec![0.0; frames], vec![0.0; frames]],
            b: [vec![0.0; frames], vec![0.0; frames]],
            silence: vec![0.0; frames],
        }
    }
}

/// Run `chain` on `in_l`/`in_r` (times `level` going in), its output left
/// in the scratch pair it ends in — returned as `true` for `b`.
fn run(
    chain: &mut Chain,
    (in_l, in_r): (&[f32], &[f32]),
    level: f32,
    scratch: &mut Scratch,
    frames: usize,
) -> Result<bool, PluginError> {
    let gain = level * chain.input;
    let [al, ar] = &mut scratch.a;
    for ((l, r), (il, ir)) in al
        .iter_mut()
        .zip(ar.iter_mut())
        .zip(in_l.iter().zip(in_r))
        .take(frames)
    {
        *l = il * gain;
        *r = ir * gain;
    }
    let mut in_b = false;
    for block in &mut chain.blocks {
        let (from, to) = if in_b {
            (&scratch.b, &mut scratch.a)
        } else {
            (&scratch.a, &mut scratch.b)
        };
        let [fl, fr] = from;
        let [tl, tr] = to;
        let (Some(fl), Some(fr), Some(tl), Some(tr)) = (
            fl.get(..frames),
            fr.get(..frames),
            tl.get_mut(..frames),
            tr.get_mut(..frames),
        ) else {
            return Err(PluginError::BlockTooLarge);
        };
        block.process_block(fl, fr, tl, tr, &PluginEvents::EMPTY)?;
        in_b = !in_b;
    }
    Ok(in_b)
}

/// A signal profile (bass or guitar), every scene loaded, one playing.
pub struct ProfileRig {
    pub profile: String,
    pub chains: Vec<Chain>,
    /// The scene playing, and how far it has faded in (samples; past the
    /// fade, it is in).
    active: usize,
    fade_in: usize,
    ringing: Vec<(usize, Ringing)>,
    scratch: Scratch,
    /// Where each chain's output is mixed: left, right.
    mix: [Vec<f32>; 2],
    sample_rate: f64,
}

impl ProfileRig {
    /// The bass `profile` (by name), every scene built, `scene` playing.
    #[must_use]
    pub fn build(profile: &str, scene: usize, sample_rate: f64, block_size: u32) -> Option<Self> {
        Self::build_in(Instrument::Bass, profile, scene, sample_rate, block_size)
    }

    /// `instrument`'s `profile` (by name), every scene (its patches) built,
    /// `scene` playing.
    #[must_use]
    pub fn build_in(
        instrument: Instrument,
        profile: &str,
        scene: usize,
        sample_rate: f64,
        block_size: u32,
    ) -> Option<Self> {
        let found = profile_in(instrument, profile)?;
        let dir = &library_of(instrument).dir;
        let chains: Vec<Chain> = found
            .patches
            .iter()
            .map(|patch| Chain::build(patch, (dir, instrument), sample_rate, block_size))
            .collect();
        let frames = usize::try_from(block_size).unwrap_or(4_096).max(1);
        Some(Self {
            profile: found.name.clone(),
            active: scene.min(chains.len().saturating_sub(1)),
            fade_in: usize::MAX,
            chains,
            ringing: Vec::with_capacity(MAX_RINGING),
            scratch: Scratch::new(frames),
            mix: [vec![0.0; frames], vec![0.0; frames]],
            sample_rate,
        })
    }

    /// The scene playing.
    #[must_use]
    pub const fn scene(&self) -> usize {
        self.active
    }

    /// The scenes' names, in order.
    #[must_use]
    pub fn scenes(&self) -> Vec<String> {
        self.chains.iter().map(|c| c.name.clone()).collect()
    }

    fn fade_len(&self) -> usize {
        num_traits::ToPrimitive::to_usize(&(FADE_SECS * self.sample_rate).round())
            .unwrap_or(384)
            .max(1)
    }

    /// Switch to scene `to`: it fades in (or, still ringing, carries on and
    /// fades up), the one playing fades out and rings on. Allocation-free.
    pub fn switch(&mut self, to: usize) {
        if to == self.active || to >= self.chains.len() {
            return;
        }
        self.ringing.retain(|(i, _)| *i != to);
        if self.ringing.len() == MAX_RINGING {
            self.ringing.remove(0);
        }
        self.ringing.push((self.active, Ringing::Fading(0)));
        self.active = to;
        self.fade_in = 0;
    }

    /// The playing chain's latency, samples.
    pub fn latency(&mut self) -> u32 {
        let active = self.active;
        self.chains.get_mut(active).map_or(0, Chain::latency)
    }

    /// The equal-power fade curve, `pos` samples into a fade of `fade`.
    fn curve(pos: usize, fade: usize) -> f32 {
        let p = f64::from(u32::try_from(pos.min(fade)).unwrap_or(0))
            / f64::from(u32::try_from(fade).unwrap_or(1).max(1));
        num_traits::ToPrimitive::to_f32(&(p * std::f64::consts::FRAC_PI_2).sin()).unwrap_or(1.0)
    }

    /// Mix the chain output left in the scratch (`in_b`: its second pair)
    /// into the mix, each sample at `gain(i)`; the loudest sample mixed.
    fn mix_in(&mut self, in_b: bool, frames: usize, gain: impl Fn(usize) -> f32) -> f32 {
        let (ol, or) = if in_b {
            (&self.scratch.b[0], &self.scratch.b[1])
        } else {
            (&self.scratch.a[0], &self.scratch.a[1])
        };
        let [ml, mr] = &mut self.mix;
        let mut loudest = 0.0_f32;
        for i in 0..frames {
            let g = gain(i);
            for (m, s) in [(ml.get_mut(i), ol.get(i)), (mr.get_mut(i), or.get(i))] {
                if let (Some(m), Some(s)) = (m, s) {
                    *m += s * g;
                    loudest = loudest.max((s * g).abs());
                }
            }
        }
        loudest
    }

    /// The playing scene into the mix, fading in after a switch.
    fn play(
        &mut self,
        input: (&[f32], &[f32]),
        level_in: f32,
        frames: usize,
    ) -> Result<(), PluginError> {
        let fade = self.fade_len();
        let active = self.active;
        let Some(chain) = self.chains.get_mut(active) else {
            return Ok(());
        };
        let in_b = run(chain, input, level_in, &mut self.scratch, frames)?;
        let (from, output) = (self.fade_in, chain.output);
        self.mix_in(in_b, frames, |i| {
            if from >= fade {
                output
            } else {
                Self::curve(from.saturating_add(i), fade) * output
            }
        });
        self.fade_in = self.fade_in.saturating_add(frames);
        Ok(())
    }

    /// Every outgoing scene into the mix: fading out on the input, then on
    /// silence until it has rung out (quiet for [`QUIET_HOLD_SECS`], or
    /// [`MAX_TAIL_SECS`] old).
    fn ring_out(
        &mut self,
        input: (&[f32], &[f32]),
        level_in: f32,
        frames: usize,
    ) -> Result<(), PluginError> {
        let fade = self.fade_len();
        let quiet_hold = num_traits::ToPrimitive::to_usize(&(QUIET_HOLD_SECS * self.sample_rate))
            .unwrap_or(96_000);
        let longest = num_traits::ToPrimitive::to_usize(&(MAX_TAIL_SECS * self.sample_rate))
            .unwrap_or(1_440_000);
        let mut k = 0_usize;
        while let Some(&(index, state)) = self.ringing.get(k) {
            let Some(chain) = self.chains.get_mut(index) else {
                self.ringing.remove(k);
                continue;
            };
            let output = chain.output;
            // Fading, on the input; ringing out, on silence (the scratch's
            // own, lent out for the run and put back).
            let in_b = match state {
                Ringing::Fading(_) => run(chain, input, level_in, &mut self.scratch, frames)?,
                Ringing::Tail { .. } => {
                    let silence = std::mem::take(&mut self.scratch.silence);
                    let out = run(
                        chain,
                        (&silence, &silence),
                        level_in,
                        &mut self.scratch,
                        frames,
                    );
                    self.scratch.silence = silence;
                    out?
                }
            };
            let loudest = self.mix_in(in_b, frames, |i| match state {
                Ringing::Fading(pos) => {
                    let p = pos.saturating_add(i);
                    Self::curve(fade.saturating_sub(p), fade) * output
                }
                Ringing::Tail { .. } => output,
            });
            let next = match state {
                Ringing::Fading(pos) => {
                    let pos = pos.saturating_add(frames);
                    Some(if pos >= fade {
                        Ringing::Tail { quiet: 0, age: 0 }
                    } else {
                        Ringing::Fading(pos)
                    })
                }
                Ringing::Tail { quiet, age } => {
                    let quiet = if loudest < QUIET {
                        quiet.saturating_add(frames)
                    } else {
                        0
                    };
                    let age = age.saturating_add(frames);
                    (quiet < quiet_hold && age < longest).then_some(Ringing::Tail { quiet, age })
                }
            };
            match next {
                Some(state) => {
                    if let Some(slot) = self.ringing.get_mut(k) {
                        slot.1 = state;
                    }
                    k = k.saturating_add(1);
                }
                None => {
                    self.ringing.remove(k);
                }
            }
        }
        Ok(())
    }

    /// One block: the playing scene, and every ringing one under it.
    ///
    /// # Errors
    ///
    /// A block longer than the rig was prepared for, or a block of a chain
    /// that fails.
    pub fn process(
        &mut self,
        (in_l, in_r): (&[f32], &[f32]),
        (out_l, out_r): (&mut [f32], &mut [f32]),
        (level_in, level_out): (f32, f32),
    ) -> Result<(), PluginError> {
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        if self.mix[0].len() < frames {
            return Err(PluginError::BlockTooLarge);
        }
        for m in &mut self.mix {
            m.iter_mut().take(frames).for_each(|s| *s = 0.0);
        }
        self.play((in_l, in_r), level_in, frames)?;
        self.ring_out((in_l, in_r), level_in, frames)?;
        for (o, s) in out_l.iter_mut().zip(&self.mix[0]).take(frames) {
            *o = s * level_out;
        }
        for (o, s) in out_r.iter_mut().zip(&self.mix[1]).take(frames) {
            *o = s * level_out;
        }
        Ok(())
    }
}
