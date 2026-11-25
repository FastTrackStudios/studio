//! Signal's offline blocks in Studio, headless, end to end: a `level`
//! block analysed offline writes four lanes as the Signal FX's parameter
//! automation in one undo step, its curves are VOCAL_LEVEL's on the same
//! vocal, the song plays them back as parameter automation, a user's
//! redraw survives a new analysis and comes back as an `automation`
//! event; and the kick folder's `align` finds its shift and polarity.
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
        std::fs::write(&path, super::wav(audio, RATE)).expect("the audio");
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
    super::left(&audio.samples)
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

#[test]
fn a_redrawn_point_survives_a_new_analysis_and_its_edit_comes_back_as_an_event() {
    let Some(clap) = clap() else {
        eprintln!("skipped: SIGNAL_DEV_CLAP names no Signal Dev .clap");
        return;
    };
    let daw = engine();
    let ctx = song(daw, "offline-redraw", &[("Lead", "Vocals", vocal(40.0))]);
    let lead = guid(daw, &ctx, "Lead");
    let fx = crate::clap_fx::add(
        daw,
        &ctx,
        &FxChainContext::Track(lead.clone()),
        &clap,
        u32::MAX,
    )
    .expect("loads");
    signal::run(
        daw,
        &ctx,
        (&lead, None),
        "SIGNAL_BLOCK_ADD",
        &json!({"phase": "rescue", "kind": "level", "first": true}).to_string(),
    )
    .expect("a level block");
    let first = super::analyze(
        daw,
        &ctx,
        &super::AnalyzeArg {
            track: lead.clone(),
            ..super::AnalyzeArg::default()
        },
    )
    .expect("analysed");
    eprintln!("analysed: {}", first["total_ms"]);
    // The user drags a ride point.
    let ride_index = Automation::envelopes(daw, ctx.clone(), TrackRef::Guid(lead.clone()))
        .into_iter()
        .filter(|e| e.fx_guid.as_deref() == Some(fx.as_str()))
        .filter_map(|e| e.param_index)
        .find(|i| param_name(daw, &ctx, &lead, &fx, *i).ends_with("Ride"))
        .expect("a ride lane");
    let loc = EnvelopeLocation::new(
        TrackRef::Guid(lead.clone()),
        EnvelopeRef::FxParam {
            fx_guid: fx.clone(),
            param_index: ride_index,
        },
    );
    let pts = Automation::points(daw, ctx.clone(), loc.clone());
    let p = pts[pts.len() / 2].clone();
    let dragged_to = (p.value + 0.05).min(1.0);
    Automation::set_point(
        daw,
        ctx.clone(),
        loc.clone(),
        daw_proto::automation::SetPointParams {
            index: p.index,
            time: p.time,
            value: dragged_to,
            shape: p.shape,
        },
    );
    // The pump notices and tells the plugin; it marks the lane redrawn.
    crate::clap_host::pump(daw);
    crate::clap_host::pump(daw);
    let a = signal::run(daw, &ctx, (&lead, None), "SIGNAL_ANALYSIS", "{}").unwrap();
    let edited = a["result"]["blocks"][0]["analysis"]["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["param"] == "ride")
        .map(|l| l["edited"].clone())
        .unwrap();
    assert!(
        edited.as_array().is_some_and(|e| !e.is_empty()),
        "the edit came back: {edited}"
    );
    // Analysed again: the redrawn point stays, and it says so.
    let again = super::analyze(
        daw,
        &ctx,
        &super::AnalyzeArg {
            track: lead.clone(),
            ..super::AnalyzeArg::default()
        },
    )
    .expect("again");
    let ride = again["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["param"] == "ride")
        .cloned()
        .unwrap();
    assert!(ride["kept"].as_u64().unwrap_or(0) >= 1, "{ride}");
    let now = Automation::points(daw, ctx.clone(), loc);
    assert!(
        now.iter()
            .any(|q| (q.time.as_seconds() - p.time.as_seconds()).abs() < 1e-6
                && (q.value - dragged_to).abs() < 1e-9),
        "the redraw is still there"
    );
}

#[test]
fn the_kick_folders_align_finds_its_shift_and_polarity_offline() {
    let Some(clap) = clap() else {
        eprintln!("skipped: SIGNAL_DEV_CLAP names no Signal Dev .clap");
        return;
    };
    let daw = engine();
    let sr = f64::from(RATE);
    // A kick every half second: a click and a falling 60 Hz thump.
    let kick: Vec<f32> = (0..(30.0 * sr) as usize)
        .map(|i| {
            let t = (i as f64 / sr) % 0.5;
            let env = (-t * 18.0).exp();
            ((std::f64::consts::TAU * (60.0 + 80.0 * (-t * 30.0).exp()) * t).sin() * env * 0.6
                + if t < 0.002 {
                    0.4 * (1.0 - t / 0.002)
                } else {
                    0.0
                }) as f32
        })
        .collect();
    // The sub mic: 37 samples late, upside down.
    let sub: Vec<f32> = (0..kick.len())
        .map(|i| i.checked_sub(37).map_or(0.0, |j| -kick[j]))
        .collect();
    let ctx = song(
        daw,
        "offline-align",
        &[("Kick In", "Kick", kick), ("Kick Sub", "Kick", sub)],
    );
    let sub_guid = guid(daw, &ctx, "Kick Sub");
    crate::clap_fx::add(
        daw,
        &ctx,
        &FxChainContext::Track(sub_guid.clone()),
        &clap,
        u32::MAX,
    )
    .expect("loads");
    signal::run(
        daw,
        &ctx,
        (&sub_guid, None),
        "SIGNAL_BLOCK_ADD",
        &json!({"phase": "rescue", "kind": "align", "first": true}).to_string(),
    )
    .expect("an align block");
    let found = super::analyze(
        daw,
        &ctx,
        &super::AnalyzeArg {
            track: sub_guid.clone(),
            reference: Some("Kick In".into()),
            ..super::AnalyzeArg::default()
        },
    )
    .expect("aligned");
    eprintln!(
        "align, the kick folder: {} ms from the call to the values set; {}",
        found["total_ms"], found["made"]
    );
    assert_eq!(found["made"]["delay_samples"], -37, "{found}");
    assert_eq!(found["made"]["invert"], true);
    let chain = signal::chain(daw, &ctx, &sub_guid, None).unwrap();
    let block = &chain["chain"]["phases"][0]["blocks"][0];
    assert_eq!(block["mode"], "offline");
    assert_eq!(block["analysis"]["state"], "fresh", "{block}");
    let _: Value = block["analysis"]["values"].clone();
}

#[test]
fn listen_aligns_kick_fix_set_on_the_blocks_plays_as_it_predicted() {
    use crate::listen::align::{Fix, Section, Windows, apply, compare};
    let Some(clap) = clap() else {
        eprintln!("skipped: SIGNAL_DEV_CLAP names no Signal Dev .clap");
        return;
    };
    let daw = engine();
    let sr = f64::from(RATE);
    let hit = |t: f64, k: f64| {
        let env = (-t * 12.0).exp();
        ((std::f64::consts::TAU * (55.0 + 70.0 * (-t * 30.0).exp()) * t + k).sin() * env * 0.5
            + if t < 0.0015 { 0.3 } else { 0.0 }) as f32
    };
    let len = (12.0 * sr) as usize;
    // In; Out 136 samples late, its low end turned; Sample upside down.
    let kick_in: Vec<f32> = (0..len).map(|i| hit((i as f64 / sr) % 0.5, 0.0)).collect();
    let kick_out: Vec<f32> = (0..len)
        .map(|i| {
            i.checked_sub(136)
                .map_or(0.0, |j| 0.8 * hit((j as f64 / sr) % 0.5, -1.5))
        })
        .collect();
    let kick_sample: Vec<f32> = kick_in.iter().map(|v| -0.6 * v).collect();
    let ctx = song(
        daw,
        "offline-kick",
        &[
            ("Kick In", "Kick", kick_in.clone()),
            ("Kick Out", "Kick", kick_out.clone()),
            ("Kick Sample", "Kick", kick_sample.clone()),
        ],
    );
    // The fixes, as LISTEN_ALIGN's block ids have them.
    let fixes = [
        (
            "Kick Out",
            json!({"delay_samples": -136, "invert": 0, "allpass.0.hz": 63.0, "allpass.0.q": 2.0}),
            Fix {
                delay: -136,
                invert: false,
                allpass: vec![Section::Second { hz: 63.0, q: 2.0 }],
            },
        ),
        (
            "Kick Sample",
            json!({"invert": 1}),
            Fix {
                delay: 0,
                invert: true,
                allpass: Vec::new(),
            },
        ),
    ];
    for (track, values, _) in &fixes {
        let g = guid(daw, &ctx, track);
        crate::clap_fx::add(
            daw,
            &ctx,
            &FxChainContext::Track(g.clone()),
            &clap,
            u32::MAX,
        )
        .expect("loads");
        signal::run(
            daw,
            &ctx,
            (&g, None),
            "SIGNAL_BLOCK_ADD",
            &json!({"phase": "rescue", "kind": "align", "first": true}).to_string(),
        )
        .expect("an align block");
        signal::run(
            daw,
            &ctx,
            (&g, None),
            "SIGNAL_ANALYZE",
            &json!({"block": "align", "values": values}).to_string(),
        )
        .expect("the fix set");
    }
    let out_chain = signal::chain(daw, &ctx, "Kick Out", None).unwrap();
    let block = &out_chain["chain"]["phases"][0]["blocks"][0];
    let param = |id: &str| {
        block["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .map(|p| p["value"].clone())
    };
    assert_eq!(param("allpass.0.hz"), Some(json!(63.0)), "{block}");
    assert_eq!(param("allpass.0.q"), Some(json!(2.0)));
    // What plays: each after its Signal, lined up by the render.
    let (from, to) = (1.0, 11.0);
    let heard_of = |track: &str| {
        super::left(
            &crate::listen::render_at(
                daw,
                &ctx,
                &crate::listen::RenderAt {
                    track,
                    at: "post-fx",
                    from,
                    to,
                    exact: true,
                },
            )
            .expect("rendered")
            .samples,
        )
    };
    let (played_out, played_sample) = (heard_of("Kick Out"), heard_of("Kick Sample"));
    let pre = 960;
    let span = (0.3 * sr) as usize;
    let at = |x: &[f32], t0: f64| -> Vec<Vec<f32>> {
        (4..20)
            .map(|k| {
                let start = ((k as f64 * 0.5 - t0) * sr) as usize - pre;
                x[start..start + span].to_vec()
            })
            .collect()
    };
    let w = Windows::new(RATE, pre);
    let add = |a: &[Vec<f32>], b: &[Vec<f32>]| -> Vec<Vec<f32>> {
        a.iter()
            .zip(b)
            .map(|(a, b)| a.iter().zip(b).map(|(x, y)| x + y).collect())
            .collect()
    };
    let (wi, wo, ws) = (at(&kick_in, 0.0), at(&kick_out, 0.0), at(&kick_sample, 0.0));
    let before = add(&add(&wi, &wo), &ws);
    let predicted = compare(
        &w,
        &before,
        &add(
            &add(&wi, &apply(&w, &wo, &fixes[0].2)),
            &apply(&w, &ws, &fixes[1].2),
        ),
    );
    let heard = compare(
        &w,
        &before,
        &add(&add(&wi, &at(&played_out, from)), &at(&played_sample, from)),
    );
    eprintln!(
        "the kick's fix: LISTEN_ALIGN predicts level {:+.2} dB, low {:+.2} dB, attack {:+.2} dB; the blocks play level {:+.2} dB, low {:+.2} dB, attack {:+.2} dB",
        predicted.level, predicted.low, predicted.attack, heard.level, heard.low, heard.attack
    );
    assert!(
        (predicted.level - heard.level).abs() < 0.1,
        "level {predicted:?} vs {heard:?}"
    );
    assert!(
        (predicted.low - heard.low).abs() < 0.1,
        "low {predicted:?} vs {heard:?}"
    );
}
