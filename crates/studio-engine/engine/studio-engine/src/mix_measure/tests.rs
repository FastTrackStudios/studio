//! MIX_MEASURE, MIX_RENDER and the sends on the standalone engine: a song
//! of known signals, rendered offline and measured.

use std::collections::HashSet;
use std::sync::Arc;

use daw::service::{ProjectContext, TrackRef, Tracks as _};
use daw_standalone::audio_engine::decoder::DecodedAudio;
use daw_standalone::audio_engine::materialize::attach_audio_source;
use daw_standalone::media_seed::{StemSpec, seed_media_tracks};
use daw_standalone::sync::Standalone;

use super::{MeasureArg, RATE, RenderArg, Span, measure, render, render_span};
use crate::mix_sends::{AddArg, SetArg};

/// A sine `rms_db` dB RMS at `hz`, `seconds` long.
pub(crate) fn sine(hz: f64, rms_db: f64, seconds: f64) -> Vec<f32> {
    let amp = 10f64.powf(rms_db / 20.0) * 2f64.sqrt();
    (0..(seconds * f64::from(RATE)) as usize)
        .map(|i| (amp * (std::f64::consts::TAU * hz * i as f64 / f64::from(RATE)).sin()) as f32)
        .collect()
}

/// A plugin `late` samples late, gain `gain`, that says how late it is.
struct Late {
    line: Vec<[f32; 2]>,
    at: usize,
    gain: f32,
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
}

/// `Late 848` (848 samples, unity, as Signal's keyed tracks report) and
/// `Gain 6` (+6 dB, no latency).
struct Plugins;

impl daw_standalone::plugin::FxFactory for Plugins {
    fn installed(&self) -> Vec<daw_proto::fx::InstalledFx> {
        Vec::new()
    }
    fn provides(&self, name: &str) -> bool {
        name == "Late 848" || name == "Gain 6"
    }
    fn create(
        &self,
        name: &str,
        _: f64,
    ) -> Option<Box<dyn daw_standalone::plugin::PluginInstance>> {
        let (late, gain) = match name {
            "Late 848" => (848, 1.0),
            "Gain 6" => (0, 10f32.powf(6.0 / 20.0)),
            _ => return None,
        };
        Some(Box::new(Late {
            line: vec![[0.0; 2]; late],
            at: 0,
            gain,
        }))
    }
}

/// Drums{Kick, Snare} and a top-level Bus. Kick plays a 1 kHz sine at
/// -18 dB RMS; Snare and Bus are silent unless `snare` is given.
pub(crate) fn song(guid: &str, snare: Option<Vec<f32>>) -> (Standalone, ProjectContext) {
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(Plugins));
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
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

fn guid_of(daw: &Standalone, ctx: &ProjectContext, path: &str) -> String {
    crate::actions::resolve(daw, ctx, path).expect(path)
}

fn args(tracks: &[&str], post_fader: bool) -> MeasureArg {
    MeasureArg {
        tracks: Some(tracks.iter().map(|t| (*t).to_owned()).collect()),
        from: "1s".into(),
        to: "3s".into(),
        post_fader: Some(post_fader),
        bypass_signal: None,
        bands: None,
        master: None,
        exact: None,
        full: None,
    }
}

fn row<'a>(m: &'a super::MeasureRow, path: &str) -> &'a super::TrackNumbers {
    m.tracks.iter().find(|t| t.path == path).unwrap_or_else(|| {
        panic!(
            "{path}: {:?}",
            m.tracks.iter().map(|t| &t.path).collect::<Vec<_>>()
        )
    })
}

#[test]
fn a_sine_at_minus_18_reads_minus_18_rms_in_its_band() {
    let (daw, ctx) = song("measure-sine", None);
    let m = measure(&daw, &ctx, &args(&["Drums/Kick"], true)).expect("measured");
    let kick = row(&m, "Drums/Kick");
    assert!((kick.numbers.rms - -18.0).abs() < 0.1, "{kick:?}");
    assert!((kick.numbers.crest - 3.01).abs() < 0.1, "{kick:?}");
    // The 1 kHz octave (the sixth band) holds it all.
    let loudest = kick
        .numbers
        .bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| m.bands_hz[i]);
    assert_eq!(loudest, Some(1_000.0), "{:?}", kick.numbers.bands);
    assert!((kick.numbers.lufs - -15.0).abs() < 0.2, "{kick:?}");
    assert!((kick.numbers.correlation - 1.0).abs() < 1e-3);
    // Master: the kick through its folder, at unity.
    let master = row(&m, "MASTER");
    assert!((master.numbers.rms - -18.0).abs() < 0.1, "{master:?}");
    assert!(m.ms > 0.0 && m.render_ms > 0.0);
}

#[test]
fn a_fader_moves_the_post_fader_reading_and_not_the_pre_fader_one() {
    let (daw, ctx) = song("measure-fader", None);
    daw.set_volume(
        ctx.clone(),
        TrackRef::Guid(guid_of(&daw, &ctx, "Drums/Kick")),
        10f64.powf(-6.0 / 20.0),
    )
    .expect("the fader");
    let post = measure(&daw, &ctx, &args(&["Drums/Kick", "Drums"], true)).expect("post");
    let pre = measure(&daw, &ctx, &args(&["Drums/Kick"], false)).expect("pre");
    assert!((row(&post, "Drums/Kick").numbers.rms - -24.0).abs() < 0.1);
    assert!((row(&pre, "Drums/Kick").numbers.rms - -18.0).abs() < 0.1);
    // The kick is all of its folder: its share is 0 dB.
    let share = row(&post, "Drums/Kick").share_db.expect("a share");
    assert!(share.abs() < 0.1, "{share}");
}

#[test]
fn sends_reach_their_bus_at_their_level_pre_or_post_fader() {
    let (daw, ctx) = song("measure-sends", None);
    // The kick reaches master only through the bus.
    daw_proto::Routing::set_parent_send_enabled(
        &daw,
        ctx.clone(),
        TrackRef::Guid(guid_of(&daw, &ctx, "Drums")),
        false,
    )
    .expect("parent send off");
    daw.set_volume(
        ctx.clone(),
        TrackRef::Guid(guid_of(&daw, &ctx, "Drums/Kick")),
        10f64.powf(-10.0 / 20.0),
    )
    .expect("the fader");
    let added = crate::mix_sends::add(
        &daw,
        &ctx,
        &AddArg {
            from: "Drums/Kick".into(),
            to: "Bus".into(),
            level_db: Some(-6.0),
            pre_fader: None,
            tap: None,
            again: None,
        },
    )
    .expect("a send");
    assert_eq!(added.status, "added");
    assert_eq!(added.send.tap, "post-fader");
    assert!((added.send.level_db - -6.0).abs() < 1e-6);
    let listed = crate::mix_sends::list(&daw, &ctx, Some("Bus")).expect("listed");
    assert_eq!(listed.count, 1);
    assert_eq!(listed.sends[0].from, "Drums/Kick");
    assert_eq!(listed.sends[0].to, "Bus");
    // Post-fader: -18 - 10 - 6.
    let post = measure(&daw, &ctx, &args(&["Bus"], true)).expect("post");
    assert!(
        (row(&post, "Bus").numbers.rms - -34.0).abs() < 0.1,
        "{:?}",
        row(&post, "Bus")
    );
    assert!((row(&post, "MASTER").numbers.rms - -34.0).abs() < 0.1);
    // Pre-fader: the fader is not in it.
    let set = crate::mix_sends::set(
        &daw,
        &ctx,
        &SetArg {
            from: "Drums/Kick".into(),
            to: "Bus".into(),
            pre_fader: Some(true),
            ..SetArg::default()
        },
    )
    .expect("set");
    assert_eq!(set.tap, "pre-fader");
    let pre = measure(&daw, &ctx, &args(&["Bus"], true)).expect("pre");
    assert!(
        (row(&pre, "Bus").numbers.rms - -24.0).abs() < 0.1,
        "{:?}",
        row(&pre, "Bus")
    );
    // Muted, nothing.
    crate::mix_sends::set(
        &daw,
        &ctx,
        &SetArg {
            from: "Kick".into(),
            to: "Bus".into(),
            mute: Some(true),
            ..SetArg::default()
        },
    )
    .expect("muted");
    let muted = measure(&daw, &ctx, &args(&["Bus"], true)).expect("muted");
    assert!(row(&muted, "Bus").numbers.rms < -140.0);
    // The bus into the kit's folder is no loop (the folder sends nowhere)…
    let back = crate::mix_sends::add(
        &daw,
        &ctx,
        &AddArg {
            from: "Bus".into(),
            to: "Drums".into(),
            level_db: None,
            pre_fader: None,
            tap: None,
            again: None,
        },
    );
    assert!(back.is_ok(), "{:?}", back.err());
    // …but into the kick, which reaches the bus, it would feed back.
    crate::mix_sends::set(
        &daw,
        &ctx,
        &SetArg {
            from: "Kick".into(),
            to: "Bus".into(),
            mute: Some(false),
            ..SetArg::default()
        },
    )
    .expect("unmuted");
    let looped = crate::mix_sends::add(
        &daw,
        &ctx,
        &AddArg {
            from: "Bus".into(),
            to: "Drums/Kick".into(),
            level_db: None,
            pre_fader: None,
            tap: None,
            again: Some(true),
        },
    );
    assert!(looped.is_err_and(|e| e.contains("feed back")));
}

#[test]
fn tracks_of_different_plugin_latency_stay_sample_aligned_at_their_folder() {
    // The snare plays the kick's sine too, polarity inverted: aligned at
    // their folder they cancel; a sample off, they would not.
    let inverted: Vec<f32> = sine(1_000.0, -18.0, 6.0).iter().map(|s| -s).collect();
    let (daw, ctx) = song("measure-latency", Some(inverted));
    let kick = guid_of(&daw, &ctx, "Drums/Kick");
    daw_proto::fx::Effects::add(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(kick.clone()),
        "Late 848",
    )
    .expect("a late plugin");
    let m = measure(
        &daw,
        &ctx,
        &args(&["Drums/Kick", "Drums/Snare", "Drums"], true),
    )
    .expect("measured");
    assert_eq!(m.latency, 848, "the song's latency");
    assert_eq!(row(&m, "Drums/Kick").latency, 848);
    assert_eq!(row(&m, "Drums/Snare").latency, 0);
    // Each alone at -18, measured over exactly the span.
    assert!((row(&m, "Drums/Kick").numbers.rms - -18.0).abs() < 0.1);
    assert!((row(&m, "Drums/Snare").numbers.rms - -18.0).abs() < 0.1);
    // Their sum: nothing.
    let drums = row(&m, "Drums").numbers.peak;
    assert!(
        drums < -100.0,
        "the folder's sum peaks at {drums} dB: not aligned"
    );
    assert!(row(&m, "MASTER").numbers.peak < -100.0);
}

#[test]
fn bypassing_a_plugin_for_the_raw_pass_shows_its_gain() {
    let (daw, ctx) = song("measure-bypass", None);
    let kick = guid_of(&daw, &ctx, "Drums/Kick");
    let fx = daw_proto::fx::Effects::add(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(kick.clone()),
        "Gain 6",
    )
    .expect("a gain");
    let tap: HashSet<String> = [kick.clone()].into();
    let span = |bypass: HashSet<String>| Span {
        from: 1.0,
        to: 3.0,
        tap: &tap,
        post_fader: true,
        bypass_fx: bypass,
        solo: None,
    };
    let guid = "measure-bypass";
    let on = render_span(&daw, guid, &span(HashSet::new())).expect("on");
    let off = render_span(&daw, guid, &span([fx.clone()].into())).expect("off");
    let rms = |s: &[f32]| {
        let (l, r) = super::channels(s);
        super::analysis::measure(&l, &r, RATE, super::Bands::Octave).rms
    };
    let gain = rms(&on.tracks[&kick]) - rms(&off.tracks[&kick]);
    assert!((gain - 6.0).abs() < 0.05, "{gain}");
    // The song's FX is still on.
    let enabled = daw_proto::fx::Effects::list(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(kick),
    )
    .iter()
    .all(|f| f.enabled);
    assert!(enabled);
}

#[test]
fn mix_render_writes_what_was_measured() {
    let (daw, ctx) = song("measure-render", None);
    let dir = tempfile::tempdir().expect("a folder");
    let file = dir.path().join("kick.wav");
    let out = render(
        &daw,
        &ctx,
        &RenderArg {
            from: "1s".into(),
            to: "3s".into(),
            tracks: Some(vec!["Drums/Kick".into()]),
            solo: None,
            to_file: file.to_string_lossy().into_owned(),
            bypass_signal: None,
            exact: None,
            full: None,
        },
    )
    .expect("rendered");
    assert!((out.seconds - 2.0).abs() < 1e-9);
    let read = fts_sample::load_audio(&file, f64::from(RATE)).expect("a wav");
    assert_eq!(read.data.len(), 2 * RATE as usize);
    assert!((out.lufs - -15.0).abs() < 0.2, "{}", out.lufs);
    // The song is not soloed by it.
    assert!(daw.all(ctx).iter().all(|t| !t.soloed));
}

/// Unbreakable's Signal kit, on a song of its manifest's tracks, measured
/// over 8 bars at 120 bpm (16 s) — the speed MIX_MEASURE runs at on the
/// real thing. Needs /tmp/signal-kit and the Signal CLAP; run by hand:
/// `cargo test -p studio-engine --lib signal_kit_speed -- --ignored --nocapture`.
#[test]
#[ignore = "needs /tmp/signal-kit/manifest.json and its CLAP"]
fn signal_kit_speed() {
    use daw_proto::item::ItemRef;
    let manifest = std::path::Path::new("/tmp/signal-kit/manifest.json");
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return;
    };
    let json: serde_json::Value = serde_json::from_str(&text).expect("a manifest");
    let mut paths: Vec<String> = Vec::new();
    let mut add = |p: &str| {
        let parts: Vec<&str> = p.split('/').collect();
        for n in 1..=parts.len() {
            let prefix = parts[..n].join("/");
            if !paths.contains(&prefix) {
                paths.push(prefix);
            }
        }
    };
    for t in json["tracks"].as_array().expect("tracks") {
        add(t["path"].as_str().unwrap_or_default());
    }
    for s in json["routing"].as_array().into_iter().flatten() {
        add(s["to"].as_str().unwrap_or_default());
    }
    // Depth first, each folder before what is in it.
    paths.sort_by(|a, b| {
        a.split('/')
            .collect::<Vec<_>>()
            .cmp(&b.split('/').collect::<Vec<_>>())
    });
    let daw = Standalone::new();
    let guid = "kit-speed";
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project(guid.into());
    let depth = |p: &str| p.split('/').count() as i32;
    let leaves: Vec<bool> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            paths
                .get(i + 1)
                .is_none_or(|n| !n.starts_with(&format!("{p}/")))
        })
        .collect();
    for (i, p) in paths.iter().enumerate() {
        let name = p.rsplit('/').next().unwrap_or(p);
        let track = daw.add(ctx.clone(), name, None).expect("a track");
        let next = paths.get(i + 1).map_or(1, |n| depth(n));
        daw.set_folder_depth(ctx.clone(), TrackRef::Guid(track.clone()), next - depth(p))
            .expect("its depth");
        if !leaves[i] {
            continue;
        }
        // Drum-like: a decaying noise burst every eighth note, 20 s.
        let loc = daw_proto::midi::Midi::create_midi_item(
            &daw,
            ctx.clone(),
            TrackRef::Guid(track.clone()),
            0.0,
            20.0,
        )
        .expect("an item");
        let ItemRef::Guid(item) = &loc.item else {
            panic!("an item guid")
        };
        let take =
            daw_proto::take::Takes::get_active_take(&daw, ctx.clone(), ItemRef::Guid(item.clone()))
                .expect("a take");
        daw.write_project(guid, |p| {
            for tl in p.takes.values_mut() {
                for t in tl.takes.iter_mut().filter(|t| t.guid == take.guid) {
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                    t.source_file_path = Some("/x/noise.wav".into());
                }
            }
        });
        let mut seed = (i as u32).wrapping_mul(2_654_435_761) | 1;
        let audio: Vec<f32> = (0..20 * RATE as usize)
            .map(|n| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let noise = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let since = (n % (RATE as usize / 4)) as f32 / RATE as f32;
                noise * 0.3 * (-since * 30.0).exp()
            })
            .collect();
        attach_audio_source(&daw, guid, &take.guid, DecodedAudio::new(audio, 1, RATE));
    }
    let kit = crate::signal_kit::kit(&daw, &ctx, manifest, false, false, true).expect("the kit");
    let m = measure(
        &daw,
        &ctx,
        &MeasureArg {
            tracks: None,
            from: "2s".into(),
            to: "18s".into(),
            post_fader: None,
            bypass_signal: Some(std::env::var_os("KIT_SPEED_RAW").is_some()),
            bands: None,
            master: None,
            exact: None,
            full: Some(std::env::var_os("KIT_SPEED_FULL").is_some()),
        },
    )
    .expect("measured");
    // Again: fresh copies render the same numbers (Signal is deterministic).
    let again = measure(
        &daw,
        &ctx,
        &MeasureArg {
            tracks: None,
            from: "2s".into(),
            to: "18s".into(),
            post_fader: None,
            bypass_signal: Some(std::env::var_os("KIT_SPEED_RAW").is_some()),
            bands: None,
            master: None,
            exact: None,
            full: Some(std::env::var_os("KIT_SPEED_FULL").is_some()),
        },
    )
    .expect("measured again");
    eprintln!(
        "again: render {:.0} ms (prime {:.0}, blocks {:.0}), total {:.0} ms",
        again.render_ms, again.prime_ms, again.blocks_ms, again.ms
    );
    let differ: Vec<String> = m
        .tracks
        .iter()
        .zip(&again.tracks)
        .filter(|(a, b)| (a.numbers.lufs - b.numbers.lufs).abs() >= 0.05)
        .map(|(a, b)| format!("{} {} vs {}", a.path, a.numbers.lufs, b.numbers.lufs))
        .collect();
    assert!(differ.is_empty(), "{differ:#?}");
    let tracks = m.tracks.len();
    let quiet = m.tracks.iter().filter(|t| t.numbers.rms < -140.0).count();
    eprintln!(
        "signal kit: {} instances added, {} failed, sends {} added / {} existing; \
         measured {tracks} tracks ({quiet} silent), latency {} samples, {} plugins copied, \
         {} failed; render {:.0} ms (copy {:.0}, prime {:.0}, blocks {:.0}), analysis {:.0} ms, total {:.0} ms",
        kit.added,
        kit.failed,
        kit.sends_added,
        kit.sends_existing,
        m.latency,
        m.plugins,
        m.plugin_failures.len(),
        m.render_ms,
        m.copy_ms,
        m.prime_ms,
        m.blocks_ms,
        m.analysis_ms,
        m.ms
    );
    for t in m
        .tracks
        .iter()
        .filter(|t| t.kind != "track" || t.latency > 0)
        .take(12)
    {
        eprintln!(
            "  {:<32} lat {:>4}  lufs {:>7.2}  raw {:>7.2}  gain {:>6.2}",
            t.path,
            t.latency,
            t.numbers.lufs,
            t.raw.as_ref().map_or(0.0, |r| r.lufs),
            t.signal_gain_db.unwrap_or(0.0)
        );
    }
    assert!(m.plugin_failures.is_empty(), "{:?}", m.plugin_failures);
    assert!(quiet < tracks, "everything silent");
}

/// What a measure answers for every row, to compare two measures.
fn readings(m: &super::MeasureRow) -> Vec<(String, f64, f64, f64)> {
    let mut v: Vec<(String, f64, f64, f64)> = m
        .tracks
        .iter()
        .map(|t| {
            (
                t.path.clone(),
                t.numbers.rms,
                t.numbers.lufs,
                t.numbers.peak,
            )
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn a_measure_after_a_change_renders_only_what_changed_and_reads_as_a_whole_render() {
    let (daw, ctx) = song("measure-cached", Some(sine(200.0, -20.0, 6.0)));
    let snare = guid_of(&daw, &ctx, "Drums/Snare");
    daw_proto::fx::Effects::add(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(snare),
        "Late 848",
    )
    .expect("a late plugin");
    let tracks = ["Drums/Kick", "Drums/Snare", "Drums"];
    let first = measure(&daw, &ctx, &args(&tracks, true)).expect("first");
    assert_ne!(first.answered, "replayed");
    // Nothing changed: nothing rendered.
    let again = measure(&daw, &ctx, &args(&tracks, true)).expect("again");
    assert_eq!(
        again.answered, "replayed",
        "{} rendered",
        again.tracks_rendered
    );
    assert_eq!(readings(&again), readings(&first));
    // The kick's fader moves: the kick and what it feeds render, the
    // snare plays back.
    daw.set_volume(
        ctx.clone(),
        TrackRef::Guid(guid_of(&daw, &ctx, "Drums/Kick")),
        10f64.powf(-6.0 / 20.0),
    )
    .expect("the fader");
    let changed = measure(&daw, &ctx, &args(&tracks, true)).expect("changed");
    assert_eq!(changed.answered, "partly");
    assert!(changed.tracks_replayed >= 1, "{}", changed.tracks_replayed);
    assert!((row(&changed, "Drums/Kick").numbers.rms - -24.0).abs() < 0.1);
    // A whole render, afresh, reads the same to the sample.
    let mut whole = args(&tracks, true);
    whole.full = Some(true);
    let whole = measure(&daw, &ctx, &whole).expect("whole");
    assert_eq!(whole.answered, "full");
    for (c, w) in readings(&changed).iter().zip(readings(&whole)) {
        assert_eq!(c.0, w.0);
        for (x, y) in [(c.1, w.1), (c.2, w.2), (c.3, w.3)] {
            assert!((x - y).abs() < 1e-6, "{} {x} vs {y}", c.0);
        }
    }
    // Without the master, the MASTER row is left out.
    let mut no_master = args(&["Drums/Kick"], true);
    no_master.master = Some(false);
    let m = measure(&daw, &ctx, &no_master).expect("no master");
    assert!(m.tracks.iter().all(|t| t.kind != "master"));
}

#[test]
fn the_raw_pass_renders_beside_the_processed_one_in_the_same_render() {
    let (daw, ctx) = song("measure-raw-pass", None);
    let kick = guid_of(&daw, &ctx, "Drums/Kick");
    let fx = daw_proto::fx::Effects::add(
        &daw,
        ctx.clone(),
        daw_proto::fx::FxChainContext::Track(kick.clone()),
        "Gain 6",
    )
    .expect("a gain");
    let tap: HashSet<String> = [kick.clone()].into();
    let span = Span {
        from: 1.0,
        to: 3.0,
        tap: &tap,
        post_fader: true,
        bypass_fx: HashSet::new(),
        solo: None,
    };
    let (on, off, cost) = super::render_cached(
        &daw,
        &ctx,
        "measure-raw-pass",
        &span,
        (Some([fx].into()), true, true),
    )
    .expect("rendered");
    let off = off.expect("the raw pass");
    let rms = |s: &[f32]| {
        let (l, r) = super::channels(s);
        super::analysis::measure(&l, &r, RATE, super::Bands::Octave).rms
    };
    let gain = rms(&on.tracks[&kick]) - rms(&off.tracks[&kick]);
    assert!((gain - 6.0).abs() < 0.05, "{gain}");
    let master = rms(&on.master) - rms(&off.master);
    assert!((master - 6.0).abs() < 0.05, "{master}");
    assert_ne!(cost.answered, "replayed");
}
