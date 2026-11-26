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
