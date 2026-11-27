//! A song of known signals on the standalone engine, for the mix's tests
//! (a copy of engine-listen's `mix_measure::tests::song`: the domains share
//! no test code).

use std::sync::Arc;

use daw::service::ProjectContext;
use daw_standalone::audio_engine::decoder::DecodedAudio;
use daw_standalone::audio_engine::materialize::attach_audio_source;
use daw_standalone::media_seed::{StemSpec, seed_media_tracks};
use daw_standalone::sync::Standalone;

use crate::measure::RATE;

/// A sine `rms_db` dB RMS at `hz`, `seconds` long.
fn sine(hz: f64, rms_db: f64, seconds: f64) -> Vec<f32> {
    let amp = 10f64.powf(rms_db / 20.0) * 2f64.sqrt();
    (0..(seconds * f64::from(RATE)) as usize)
        .map(|i| (amp * (std::f64::consts::TAU * hz * i as f64 / f64::from(RATE)).sin()) as f32)
        .collect()
}

/// A plugin `late` samples late, gain `gain`, that says how late it is;
/// with a `state`, it keeps and takes one.
struct Late {
    line: Vec<[f32; 2]>,
    at: usize,
    gain: f32,
    state: Option<Vec<u8>>,
}

impl daw_standalone::plugin::PluginInstance for Late {
    fn descriptor(&self) -> daw_standalone::plugin::PluginDescriptor {
        daw_standalone::plugin::PluginDescriptor::default()
    }
    fn params(&mut self) -> Vec<daw_standalone::plugin::PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _: u32, _: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _: u32, _: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        u32::try_from(self.line.len()).unwrap_or(0)
    }
    fn prepare(&mut self, _: f64, _: u32) -> Result<(), daw_standalone::plugin::PluginError> {
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        true
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _: &daw_standalone::plugin::PluginEvents<'_>,
    ) -> Result<(), daw_standalone::plugin::PluginError> {
        for f in 0..in_l.len() {
            let (l, r) = if self.line.is_empty() {
                (in_l[f], in_r[f])
            } else {
                let [l, r] = self.line[self.at];
                self.line[self.at] = [in_l[f], in_r[f]];
                self.at = (self.at + 1) % self.line.len();
                (l, r)
            };
            out_l[f] = l * self.gain;
            out_r[f] = r * self.gain;
        }
        Ok(())
    }
    fn deactivate(&mut self) {}
    fn save_state(&mut self) -> Result<Vec<u8>, daw_standalone::plugin::PluginError> {
        self.state.clone().ok_or_else(|| {
            daw_standalone::plugin::PluginError::UnsupportedFormat(self.descriptor().format)
        })
    }
    fn load_state(&mut self, state: &[u8]) -> Result<(), daw_standalone::plugin::PluginError> {
        match &mut self.state {
            Some(kept) => {
                state.clone_into(kept);
                Ok(())
            }
            None => Err(daw_standalone::plugin::PluginError::UnsupportedFormat(
                self.descriptor().format,
            )),
        }
    }
}

/// `Late 848` (848 samples, unity, as Signal's keyed tracks report),
/// `Gain 6` (+6 dB, no latency) and `Stateful` (unity, a state of its own).
struct Plugins;

impl daw_standalone::plugin::FxFactory for Plugins {
    fn installed(&self) -> Vec<daw_proto::fx::InstalledFx> {
        Vec::new()
    }
    fn provides(&self, name: &str) -> bool {
        matches!(name, "Late 848" | "Gain 6" | "Stateful")
    }
    fn create(
        &self,
        name: &str,
        _: f64,
    ) -> Option<Box<dyn daw_standalone::plugin::PluginInstance>> {
        let (late, gain) = match name {
            "Late 848" => (848, 1.0),
            "Gain 6" => (0, 10f32.powf(6.0 / 20.0)),
            "Stateful" => (0, 1.0),
            _ => return None,
        };
        Some(Box::new(Late {
            line: vec![[0.0; 2]; late],
            at: 0,
            gain,
            state: (name == "Stateful").then(Vec::new),
        }))
    }
}

/// Drums{Kick, Snare} and a top-level Bus. Kick plays a 1 kHz sine at
/// -18 dB RMS; Snare and Bus are silent unless `snare` is given.
pub fn song(guid: &str, snare: Option<Vec<f32>>) -> (Standalone, ProjectContext) {
    song_at(guid, snare, "")
}

/// [`song`], opened from the file `path` (empty: from none).
pub fn song_at(guid: &str, snare: Option<Vec<f32>>, path: &str) -> (Standalone, ProjectContext) {
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(Plugins));
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: path.into(),
    });
    let stems = vec![
        StemSpec::new("Kick", "/x/kick.wav", Some("Drums")),
        StemSpec::new("Snare", "/x/snare.wav", Some("Drums")),
        StemSpec::new("Bus", "/x/bus.wav", None),
    ];
    seed_media_tracks(&daw, guid, &stems, 0.0, 10.0);
    let ctx = ProjectContext::Project(guid.to_owned());
    let takes: Vec<(String, String)> = daw
        .read_project(guid, |p| {
            p.tracks
                .iter()
                .flat_map(|t| {
                    p.items_by_track
                        .get(&t.guid)
                        .into_iter()
                        .flatten()
                        .filter_map(|i| p.takes.get(i))
                        .flat_map(|tl| tl.takes.iter().map(|k| (t.name.clone(), k.guid.clone())))
                        .collect::<Vec<_>>()
                })
                .collect()
        })
        .unwrap_or_default();
    for (track, take) in takes {
        let audio = match track.as_str() {
            "Kick" => sine(1_000.0, -18.0, 6.0),
            "Snare" => snare.clone().unwrap_or_else(|| vec![0.0; 1024]),
            _ => vec![0.0; 1024],
        };
        attach_audio_source(&daw, guid, &take, DecodedAudio::new(audio, 1, RATE));
    }
    (daw, ctx)
}
