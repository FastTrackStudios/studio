//! A `level` block analysed offline writes its lanes as the Signal FX's
//! automation, and they are VOCAL_LEVEL's on the same vocal: moved here
//! from engine-listen's `signal_offline` tests because it reads listen
//! (SIGNAL_ANALYZE), the mix (the CLAP host) and edit (the vocal leveler)
//! at once, which only the assembly has.
//!
//! Needs a Signal Dev CLAP with offline blocks: `SIGNAL_DEV_CLAP=<path to
//! the .clap>` (skipped without). The audio is synthetic, written under
//! the temp dir; nothing here touches a song on disk.
use std::path::PathBuf;
use std::sync::Arc;

use daw::service::{FxChainContext, ProjectContext, TrackRef};
use daw_proto::automation::{Automation, EnvelopeLocation, EnvelopeRef};
use daw_standalone::audio_engine::decoder::DecodedAudio;
use daw_standalone::audio_engine::materialize::attach_audio_source;
use daw_standalone::media_seed::{StemSpec, seed_media_tracks};
use daw_standalone::sync::Standalone;
use serde_json::{Value, json};

use crate::clap_host::{automation::StudioAutomation, bridge::StudioActions, signal};

const RATE: u32 = crate::listen::render::RATE;

fn clap() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("SIGNAL_DEV_CLAP")?);
    path.exists().then_some(path)
}

/// A synthetic vocal (never a recording): voiced phrases, verses 12 dB
/// under choruses, breaths, esses, room noise. As signal-core's
/// `synth_vocal`, written out here so this test needs no crate of Signal's.
fn vocal(seconds: f64) -> Vec<f32> {
    let sr = f64::from(RATE);
    let mut rng: u64 = 0x2545_f491_4f6c_dd1d;
    let mut noise = move || {
        rng = rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((rng >> 11) as f64 / (1_u64 << 53) as f64).mul_add(2.0, -1.0)
    };
    let n = (seconds * sr) as usize;
    let db = |d: f64| 10f64.powf(d / 20.0);
    let (mut phase, mut lp, mut prev, mut hp) = (0.0_f64, 0.0, 0.0, 0.0);
    (0..n)
        .map(|i| {
            let t = i as f64 / sr;
            let level = if (t / 16.0) as u64 % 2 == 1 {
                db(-14.0)
            } else {
                db(-26.0)
            };
            let at = t % 3.0;
            let sung = at < 2.3;
            let syl = (at * 4.0) % 1.0;
            let shape = (std::f64::consts::PI * syl).sin().max(0.0);
            let f0 = 200.0
                + 40.0 * (std::f64::consts::TAU * t / 3.0).sin()
                + 6.0 * (std::f64::consts::TAU * 5.0 * t).sin();
            phase += std::f64::consts::TAU * f0 / sr;
            let voiced: f64 = (1..=8)
                .map(|h| (phase * f64::from(h)).sin() / f64::from(h))
                .sum();
            let w = noise();
            lp += 0.05 * (w - lp);
            hp = 0.9 * (hp + w - prev);
            prev = w;
            let mut x = db(-62.0) * w;
            if sung {
                x += level * shape * voiced * 0.5;
            }
            if sung && ((at * 4.0) as u64).is_multiple_of(3) && syl < 0.18 {
                x += level * 0.6 * hp;
            }
            if !sung && at > 2.6 {
                x += level * db(-6.0) * lp * 4.0;
            }
            x as f32
        })
        .collect()
}

/// A song `guid` with `stems` (name, folder, audio), each one item from 0.
fn song(daw: &Standalone, guid: &str, stems: &[(&str, &str, Vec<f32>)]) -> ProjectContext {
    daw.seed_project(daw_proto::ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    });
    let dir = std::env::temp_dir().join(format!("signal-offline-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let mut specs = Vec::new();
    for (name, folder, audio) in stems {
        let path = dir.join(format!("{guid}-{}.wav", name.replace(' ', "_")));
        std::fs::write(&path, crate::signal_offline::wav(audio, RATE)).expect("the audio");
        specs.push(StemSpec::new(*name, path.to_string_lossy(), Some(*folder)));
    }
    let seconds = stems.iter().map(|s| s.2.len()).max().unwrap_or(0) as f64 / f64::from(RATE);
    seed_media_tracks(daw, guid, &specs, 0.0, seconds);
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
        if let Some((_, _, audio)) = stems.iter().find(|s| s.0 == track) {
            attach_audio_source(daw, guid, &take, DecodedAudio::new(audio.clone(), 1, RATE));
        }
    }
    ProjectContext::Project(guid.to_owned())
}

fn engine() -> &'static Standalone {
    let daw: &'static Standalone = Box::leak(Box::new(Standalone::new()));
    daw.set_plugin_load_options(daw::standalone::plugin::LoadOptions {
        actions: Some(Arc::new(StudioActions::default())),
        automation: Some(Arc::new(StudioAutomation::new(daw))),
        ..daw::standalone::plugin::LoadOptions::default()
    });
    daw
}

fn guid(daw: &Standalone, ctx: &ProjectContext, path: &str) -> String {
    crate::actions::resolve(daw, ctx, path).expect(path)
}

/// The Signal FX's parameter lanes on `track`: (name, points), with
/// points as (seconds, normalized value).
fn fx_lanes(
    daw: &Standalone,
    ctx: &ProjectContext,
    track: &str,
    fx: &str,
) -> Vec<(String, Vec<(f64, f64)>)> {
    Automation::envelopes(daw, ctx.clone(), TrackRef::Guid(track.to_owned()))
        .into_iter()
        .filter(|e| e.fx_guid.as_deref() == Some(fx))
        .filter_map(|e| {
            let index = e.param_index?;
            let at = EnvelopeLocation::new(
                TrackRef::Guid(track.to_owned()),
                EnvelopeRef::FxParam {
                    fx_guid: fx.to_owned(),
                    param_index: index,
                },
            );
            let pts = Automation::points(daw, ctx.clone(), at)
                .iter()
                .map(|p| (p.time.as_seconds(), p.value))
                .collect::<Vec<_>>();
            (!pts.is_empty()).then(|| (param_name(daw, ctx, track, fx, index), pts))
        })
        .collect()
}

/// FX `fx`'s parameter `index`, by its own name (`Rescue · Level · Ride`).
fn param_name(daw: &Standalone, ctx: &ProjectContext, track: &str, fx: &str, index: u32) -> String {
    daw_proto::fx::Effects::parameters(
        daw,
        ctx.clone(),
        daw_proto::fx::FxTarget::new(
            FxChainContext::Track(track.to_owned()),
            daw_proto::fx::FxRef::Guid(fx.to_owned()),
        ),
    )
    .into_iter()
    .find(|p| p.index == index)
    .map_or_else(|| format!("param {index}"), |p| p.name)
}

/// A linear lane's value at `t`.
fn at(points: &[(f64, f64)], t: f64) -> f64 {
    let i = points.partition_point(|p| p.0 <= t);
    match (i.checked_sub(1).and_then(|j| points.get(j)), points.get(i)) {
        (Some(a), Some(b)) if b.0 > a.0 => a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0),
        (Some(a), _) => a.1,
        (None, Some(b)) => b.1,
        (None, None) => 0.0,
    }
}

/// The Signal block's curves as SIGNAL_ANALYSIS gives them (plain dB).
fn curves(
    daw: &Standalone,
    ctx: &ProjectContext,
    track: &str,
) -> std::collections::HashMap<String, Vec<(f64, f64)>> {
    let out = signal::run(daw, ctx, (track, None), "SIGNAL_ANALYSIS", "{}").expect("its analysis");
    out["result"]["blocks"][0]["analysis"]["lanes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| {
            let pts = l["curve"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?)))
                .collect();
            (l["param"].as_str().unwrap_or_default().to_owned(), pts)
        })
        .collect()
}

/// `track`'s signal at `at` (LISTEN's words), `from`..`to`, the left
/// channel, rendered in one piece.
fn heard(
    daw: &Standalone,
    ctx: &ProjectContext,
    track: &str,
    at: &str,
    (from, to): (f64, f64),
) -> Vec<f32> {
    let audio = crate::listen::render_at(
        daw,
        ctx,
        &crate::listen::RenderAt {
            track,
            at,
            from,
            to,
            exact: true,
        },
    )
    .expect("a render");
    crate::signal_offline::left(&audio.samples)
}

/// The power in dB of each 10 ms block of `x`.
fn block_db(x: &[f32]) -> Vec<f64> {
    x.chunks(480)
        .map(|c| {
            let p = c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / c.len().max(1) as f64;
            10.0 * p.max(1e-20).log10()
        })
        .collect()
}

#[test]
fn a_level_block_writes_its_lanes_at_once_as_the_fxs_automation_and_they_are_vocal_levels() {
    let Some(clap) = clap() else {
        eprintln!("skipped: SIGNAL_DEV_CLAP names no Signal Dev .clap");
        return;
    };
    let daw = engine();
    let seconds = 240.0;
    let audio = vocal(seconds);
    let ctx = song(
        daw,
        "offline-level",
        &[
            ("Lead", "Vocals", audio.clone()),
            ("Lead Plain", "Vocals", audio.clone()),
        ],
    );
    let lead = guid(daw, &ctx, "Lead");
    let plain = guid(daw, &ctx, "Lead Plain");
    let fx = crate::clap_fx::add(
        daw,
        &ctx,
        &FxChainContext::Track(lead.clone()),
        &clap,
        u32::MAX,
    )
    .expect("Signal Dev loads");

    // VOCAL_LEVEL on the vocal with Signal: a level block, analysed offline.
    let track = daw::service::Tracks::get(daw, ctx.clone(), TrackRef::Guid(lead.clone())).unwrap();
    let began = web_time::Instant::now();
    let leveled = crate::vocal_level::level_with_signal(
        daw,
        &ctx,
        &track,
        &crate::vocal_level::Knobs::default(),
    )
    .expect("a Signal FX")
    .expect("leveled");
    let wall_ms = began.elapsed().as_secs_f64() * 1e3;
    assert_eq!(leveled.via, "signal");
    eprintln!(
        "level, a {seconds:.0} s vocal: {:.0} ms from the call to the lanes in the song ({wall_ms:.0} ms with the block added); target {:.1} dB, makeup {:.1} dB",
        leveled.total_ms, leveled.target_db, leveled.makeup_db
    );

    // The lanes are the FX's own parameter automation: four of them, shown.
    let lanes = fx_lanes(daw, &ctx, &lead, &fx);
    let names: Vec<&str> = lanes.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(lanes.len(), 4, "{names:?}");
    for want in ["Gate", "De-Breath", "Ride", "De-Ess"] {
        assert!(
            names.iter().any(|n| n.ends_with(want)),
            "{want} in {names:?}"
        );
    }
    let shown = Automation::envelopes(daw, ctx.clone(), TrackRef::Guid(lead.clone()))
        .into_iter()
        .filter(|e| e.fx_guid.as_deref() == Some(fx.as_str()))
        .all(|e| e.visible);
    assert!(shown, "the lanes are shown");

    // The same curves as VOCAL_LEVEL's (take envelopes) on the same vocal.
    let plain_track =
        daw::service::Tracks::get(daw, ctx.clone(), TrackRef::Guid(plain.clone())).unwrap();
    let said = crate::vocal_level::level(
        daw,
        &ctx,
        &plain_track,
        (&crate::vocal_level::Knobs::default(), false),
    )
    .expect("leveled");
    assert!(
        (said.target_db - leveled.target_db).abs() < 1e-6,
        "{} vs {}",
        said.target_db,
        leveled.target_db
    );
    assert!(
        (said.makeup_db - leveled.makeup_db).abs() < 0.01,
        "{} vs {}",
        said.makeup_db,
        leveled.makeup_db
    );
    let item = crate::vocal_level::playing(daw, &ctx, &plain_track)[0]
        .guid
        .clone();
    let kept = crate::vocal_level::kept(daw, &ctx, &plain, &item).expect("its lanes");
    let made = curves(daw, &ctx, &lead);
    let env = |pts: &[dsp::level::envelope::EnvPoint]| -> Vec<(f64, f64)> {
        pts.iter().map(|p| (p.t_s, p.db)).collect()
    };
    let mut worst: Vec<(&str, f64)> = Vec::new();
    for (name, theirs) in [("ride", env(&kept.ride)), ("breath", env(&kept.breath))] {
        let ours = &made[name];
        let w = (0..(seconds * 100.0) as usize)
            .map(|k| k as f64 / 100.0 + 0.005)
            .map(|t| (at(ours, t) - at(&theirs, t)).abs())
            .fold(0.0_f64, f64::max);
        worst.push((name, w));
    }
    // The gate holds: compare away from its steps.
    let gate_theirs = env(&kept.gate);
    let steps: Vec<f64> = kept.gate.iter().map(|p| p.t_s).collect();
    let w = (0..(seconds * 100.0) as usize)
        .map(|k| k as f64 / 100.0 + 0.005)
        .filter(|t| steps.iter().all(|s| (s - t).abs() > 0.002))
        .map(|t| {
            let held = gate_theirs[..gate_theirs.partition_point(|p| p.0 <= t)]
                .last()
                .map_or(gate_theirs[0].1, |p| p.1);
            (at(&made["gate"], t) - held).abs()
        })
        .fold(0.0_f64, f64::max);
    worst.push(("gate", w));
    // The esses: each span at its gain.
    let ess_w = kept
        .sibilance
        .iter()
        .map(|s| (at(&made["ess"], f64::midpoint(s.from_s, s.to_s)) - s.db).abs())
        .fold(0.0_f64, f64::max);
    worst.push(("ess", ess_w));
    eprintln!("the curves against VOCAL_LEVEL's, worst dB: {worst:?}");
    for (name, w) in &worst {
        assert!(*w < 0.01, "{name}: {w} dB from VOCAL_LEVEL's");
    }

    // What the host plays is those curves: the FX's automation read back
    // through the parameter's range is the analysis.
    let ride = lanes.iter().find(|(n, _)| n.ends_with("Ride")).unwrap();
    let plain_ride: Vec<(f64, f64)> = ride.1.iter().map(|(t, v)| (*t, -48.0 + 96.0 * v)).collect();
    let w = made["ride"]
        .iter()
        .map(|(t, db)| (at(&plain_ride, *t) - db).abs())
        .fold(0.0_f64, f64::max);
    assert!(w < 0.01, "the ride as automation: {w} dB off");

    // Playback: the Signal track through its FX (the host playing its
    // automation) against the fallback's take envelopes, 10 ms blocks
    // where it is sung, away from the esses (a band on one, the whole
    // vocal on the other) and the gate's edges.
    let span = (0.0, 60.0);
    let ours = block_db(&heard(daw, &ctx, &lead, "post-fx", span));
    let theirs = block_db(&heard(daw, &ctx, &plain, "post-fx", span));
    let dry = block_db(&audio[..(60 * RATE) as usize]);
    // The engine plays an FX parameter's automation once per 1024-frame
    // block (21 ms), a take envelope sample by sample: within a block of a
    // gate's or a breath's edge, or of an ess, they differ by design.
    let block_s = 1024.0 / f64::from(RATE) + 0.01;
    let edges: Vec<f64> = steps
        .iter()
        .copied()
        .chain(kept.breath.iter().map(|p| p.t_s))
        .collect();
    let near = |t: f64| {
        kept.sibilance
            .iter()
            .any(|s| t > s.from_s - block_s && t < s.to_s + block_s)
            || edges.iter().any(|g| (g - t).abs() < block_s)
    };
    let diffs: Vec<f64> = (0..ours.len().min(theirs.len()).min(dry.len()))
        .filter(|k| dry[*k] > -45.0 && !near(*k as f64 * 0.01 + 0.005))
        .map(|k| (ours[k] - theirs[k]).abs())
        .collect();
    let worst = diffs.iter().copied().fold(0.0_f64, f64::max);
    {
        let mut d = diffs.clone();
        d.sort_by(f64::total_cmp);
        let q = |f: f64| {
            d.get(((d.len() as f64 - 1.0) * f) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        // Where the worst are, and what the lanes say should play there.
        let mut idx: Vec<usize> = (0..ours.len().min(theirs.len()).min(dry.len()))
            .filter(|k| dry[*k] > -45.0 && !near(*k as f64 * 0.01 + 0.005))
            .collect();
        idx.sort_by(|a, b| {
            (ours[*b] - theirs[*b])
                .abs()
                .total_cmp(&(ours[*a] - theirs[*a]).abs())
        });
        let sum = |t: f64| {
            ["gate", "breath", "ride", "ess"]
                .iter()
                .map(|n| at(&made[*n], t))
                .sum::<f64>()
        };
        eprintln!(
            "diffs p50 {:.3} p90 {:.3} p99 {:.3} dB",
            q(0.5),
            q(0.9),
            q(0.99)
        );
        for k in idx.iter().take(6) {
            let t = *k as f64 * 0.01 + 0.005;
            eprintln!(
                "  t {t:.3}s: dry {:.1} ours {:.1} theirs {:.1}; lanes sum {:.1} dB",
                dry[*k],
                ours[*k],
                theirs[*k],
                sum(t)
            );
        }
    }
    let moved = ours
        .iter()
        .zip(&dry)
        .filter(|(o, d)| (*o - *d).abs() > 1.0)
        .count();
    eprintln!(
        "played through the FX vs the take envelopes: worst {worst:.3} dB over {} sung 10 ms blocks; {moved} of {} blocks moved > 1 dB by the leveler",
        diffs.len(),
        ours.len()
    );
    assert!(
        moved > 100,
        "the FX's automation plays: the vocal was leveled"
    );
    // Played by the host, an FX parameter's lane is a staircase of 21 ms
    // steps (the engine's automation is block-rate), so where the ride
    // moves fast — a phrase's onset — it is up to a dB from the take
    // envelope; everywhere else within 0.1 dB. (Signal's own playback is
    // exact: signal-core's tests.)
    let mut sorted = diffs.clone();
    sorted.sort_by(f64::total_cmp);
    let q = |f: f64| sorted[((sorted.len() - 1) as f64 * f) as usize];
    assert!(q(0.9) < 0.1, "p90 {} dB", q(0.9));
    assert!(q(0.99) < 0.5, "p99 {} dB", q(0.99));
    assert!(worst < 1.5, "{worst} dB");

    // One undo step: the analysis's four lanes go together.
    assert!(
        daw::service::Projects::undo(daw, ctx.clone()),
        "an undo step"
    );
    let after = fx_lanes(daw, &ctx, &lead, &fx);
    assert!(
        after.is_empty(),
        "one undo takes all four lanes: {:?}",
        after.iter().map(|l| &l.0).collect::<Vec<_>>()
    );
}
