//! Signal's offline blocks from Studio: analysed at once, no playback.
//!
//! `SIGNAL_ANALYZE` renders the audio arriving at the block offline
//! (`listen::render_at` at `block:<id>:pre`: only what feeds that point, a
//! copy of the Signal with the block and what follows bypassed, stopped or
//! playing), hands it to the plugin, which analyses it on a worker, and waits while the
//! plugin writes every lane as this FX's parameter automation through
//! `com.fasttrackstudio.host-automation/1` in one undo step. What it says
//! is timed from the call to the lanes being in the song.
//!
//! Staleness: each analysis carries the track's source revision (its
//! playing items as an edit step leaves them); after a step, every Signal
//! FX with offline blocks is told the revision now
//! (`SIGNAL_ANALYSIS_STALE`), and the plugin marks a block made from
//! another one stale.

#![cfg(feature = "native")]

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use architect::action::{ActionCaller, ActionError, ActionList, Effect, RawJson};
use daw::service::{ProjectContext, Takes as _, TrackRef};
use daw::standalone::Standalone;
use daw_proto::ItemRef;
use facet::Facet;
use serde_json::{Value, json};

use crate::actions::{meta, resolve, song};
use crate::clap_host::signal;

/// How long a call waits for the plugin's analysis.
const ANALYSIS_WAIT_S: f64 = 120.0;

/// How long an `align` search listens by default, seconds.
const ALIGN_WINDOW_S: f64 = 8.0;

static ANALYZE: architect::action::ActionMeta = meta(
    "SIGNAL_ANALYZE",
    "signal_analyze",
    "Analyse a Signal offline block",
    "Analyse an offline block (`level`: the vocal leveler's gate, de-breath, ride and de-ess lanes; `align`: delay, polarity) of the Signal FX on `track` (`fx` picks one of several; `block` its chain id, the first offline block when left out) over the audio arriving at it, read offline — no playback, the transport stopped or playing — from `from` to `to` seconds (the track's items when left out; part of the song re-analyses only that part, the rest kept). `level` takes `consistency` 0–100, `range_db`, `target_db`; `align` listens `window_s` (8) against `reference` (a track, read before its FX). The lanes are written as the FX's own parameter automation, in one undo step, the user's redrawn points kept unless `replace_user_edits`. Says how long each part took and `total_ms`, from the call to the lanes being in the song.",
    "FX",
    "",
);

static ANALYSIS: architect::action::ActionMeta = meta(
    "SIGNAL_ANALYSIS",
    "signal_analysis",
    "A Signal offline block's analysis",
    "The offline blocks of the Signal FX on `track` (`fx`, `block` narrow it): each block's analysis — `state` (none, fresh, stale, running), `range`, `at` (Unix seconds), `source_revision`, `playback` (host: its lanes are this FX's automation; self: Signal plays them), `stale` ranges, `lanes` (`param`, `clap_id`, `points`, `curve` [[seconds, value]…], `edited`: where the user redrew it) and `values`.",
    "FX",
    "",
);

pub use engine_core::signal_offline::AnalyzeArg;

/// Which Signal FX, and which block.
#[derive(Facet)]
struct AnalysisArg {
    track: String,
    fx: Option<String>,
    block: Option<String>,
}

/// The track's source revision: its playing items as they are (where,
/// how long, what of which recording, how fast). An edit upstream of the
/// FX — a comp, a slip, a quantize — moves it.
#[must_use]
pub fn source_revision(daw: &Standalone, project: &ProjectContext, track_guid: &str) -> String {
    let Some(track) =
        daw::service::Tracks::get(daw, project.clone(), TrackRef::Guid(track_guid.to_owned()))
    else {
        return String::new();
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for item in crate::vocal_lanes::playing(daw, project, &track) {
        item.guid.hash(&mut h);
        item.position.as_seconds().to_bits().hash(&mut h);
        item.length.as_seconds().to_bits().hash(&mut h);
        if let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone())) {
            take.start_offset.as_seconds().to_bits().hash(&mut h);
            take.play_rate.to_bits().hash(&mut h);
            take.source_file_path.hash(&mut h);
        }
    }
    format!("{:016x}", h.finish())
}

/// The span `track`'s playing items cover, seconds.
fn items_span(daw: &Standalone, project: &ProjectContext, track_guid: &str) -> Option<(f64, f64)> {
    let track =
        daw::service::Tracks::get(daw, project.clone(), TrackRef::Guid(track_guid.to_owned()))?;
    crate::vocal_lanes::playing(daw, project, &track)
        .iter()
        .map(|i| {
            (
                i.position.as_seconds(),
                i.position.as_seconds() + i.length.as_seconds(),
            )
        })
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
}

/// A mono float WAV of `samples` at `rate`.
fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data = u32::try_from(samples.len() * 4).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&3_u16.to_le_bytes()); // IEEE float
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&32_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// The left channel (a dual-capsule vocal's front capsule) of one point.
fn left(interleaved: &[f32]) -> Vec<f32> {
    interleaved.iter().step_by(2).copied().collect()
}

/// Where a render's audio goes for the plugin to read: never a repo, never
/// the song's folder.
fn scratch(fx_guid: &str, what: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("studio-signal-analysis");
    let _ = std::fs::create_dir_all(&dir);
    let safe: String = format!("{fx_guid}-{what}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{safe}.wav"))
}

/// The offline block `block` names (or the first) in a chain read by
/// SIGNAL_CHAIN: its chain id and kind.
fn offline_block(chain: &Value, block: Option<&str>) -> Option<(String, String)> {
    let phases = chain["chain"]["phases"].as_array()?;
    phases
        .iter()
        .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default())
        .filter(|b| b["mode"] == "offline")
        .find(|b| {
            block.is_none_or(|w| {
                b["id"] == w
                    || b["id"]
                        .as_str()
                        .is_some_and(|id| id.ends_with(&format!(".{w}")))
            })
        })
        .map(|b| {
            (
                b["id"].as_str().unwrap_or_default().to_owned(),
                b["kind"].as_str().unwrap_or_default().to_owned(),
            )
        })
}

/// SIGNAL_ANALYZE (see its description).
///
/// # Errors
/// No Signal FX or offline block, nothing to read, or the plugin's refusal.
pub fn analyze(
    daw: &Standalone,
    project: &ProjectContext,
    a: &AnalyzeArg,
) -> Result<Value, String> {
    let began = web_time::Instant::now();
    let span = tracing::info_span!(
        "SIGNAL_ANALYZE",
        signal.track = a.track.as_str(),
        signal.block = tracing::field::Empty,
        signal.outcome = tracing::field::Empty,
        signal.total_ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let chain = signal::chain(daw, project, &a.track, a.fx.as_deref())?;
    let (block, kind) = offline_block(&chain, a.block.as_deref()).ok_or_else(|| {
        format!(
            "{}: the Signal FX there has no offline block{}",
            a.track,
            a.block
                .as_deref()
                .map_or(String::new(), |b| format!(" {b}"))
        )
    })?;
    span.record("signal.block", block.as_str());
    let track_guid = chain["track_guid"].as_str().unwrap_or_default().to_owned();
    let fx_guid = chain["fx"]["guid"].as_str().unwrap_or_default().to_owned();
    let (from, to) = match (a.from, a.to) {
        (Some(f), Some(t)) => (f, t),
        (f, t) => {
            let (s, e) = items_span(daw, project, &track_guid)
                .ok_or_else(|| format!("{}: no items to read; give `from` and `to`", a.track))?;
            (f.unwrap_or(s), t.unwrap_or(e))
        }
    };
    let (from, to) = if kind == "align" {
        (from, to.min(from + a.window_s.unwrap_or(ALIGN_WINDOW_S)))
    } else {
        (from, to)
    };
    // The audio arriving at the block: a copy of the track's Signal with
    // the block and everything after it bypassed (the live one untouched),
    // rendered in one piece (`exact`: the chunked render is approximate).
    let rendered = web_time::Instant::now();
    let at = crate::listen::render_at(
        daw,
        project,
        &crate::listen::RenderAt {
            track: &track_guid,
            at: &format!("block:{block}:pre"),
            from,
            to,
            exact: true,
        },
    )?;
    let reference = a
        .reference
        .as_deref()
        .map(|r| {
            crate::listen::render_at(
                daw,
                project,
                &crate::listen::RenderAt {
                    track: r,
                    at: "pre-fx",
                    from,
                    to,
                    exact: true,
                },
            )
        })
        .transpose()?;
    let render_ms = rendered.elapsed().as_secs_f64() * 1e3;
    let rate = at.rate;
    let write = |what: &str, samples: &[f32]| -> Result<String, String> {
        let path = scratch(&fx_guid, what);
        std::fs::write(&path, wav(samples, rate))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path.to_string_lossy().into_owned())
    };
    let audio = write("in", &left(&at.samples))?;
    let mut args = json!({
        "block": block,
        "audio": { "path": audio, "sample_rate": rate, "start_s": from },
        "source_revision": source_revision(daw, project, &track_guid),
        "replace_user_edits": a.replace_user_edits.unwrap_or(false),
    });
    if a.from.is_some() || a.to.is_some() {
        args["from"] = json!(from);
        args["to"] = json!(to);
    }
    for (k, v) in [
        ("consistency", a.consistency),
        ("range_db", a.range_db),
        ("target_db", a.target_db),
    ] {
        if let Some(v) = v {
            args[k] = json!(v);
        }
    }
    if let Some(reference) = &reference {
        args["reference"] = json!({
            "path": write("reference", &left(&reference.samples))?,
            "sample_rate": rate,
        });
    }
    let asked = signal::run(
        daw,
        project,
        (&a.track, a.fx.as_deref()),
        "SIGNAL_ANALYZE",
        &args.to_string(),
    )?;
    let job = asked["result"]["job"]
        .as_u64()
        .ok_or_else(|| format!("the plugin did not start an analysis: {}", asked["result"]))?;
    // The plugin lands the result on its next main-thread call: ask until
    // it says the job finished.
    let waited = web_time::Instant::now();
    let finished = loop {
        let out = signal::run(
            daw,
            project,
            (&a.track, a.fx.as_deref()),
            "SIGNAL_ANALYSIS",
            &json!({ "block": block }).to_string(),
        )?;
        if let Some(done) = out["result"]["finished"]
            .as_array()
            .and_then(|f| f.iter().find(|j| j["job"].as_u64() == Some(job)).cloned())
        {
            break (done, out["result"]["blocks"][0].clone());
        }
        if waited.elapsed().as_secs_f64() > ANALYSIS_WAIT_S {
            span.record("signal.outcome", "timeout");
            return Err(format!("{block}: no analysis after {ANALYSIS_WAIT_S:.0} s"));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let (done, analysed) = finished;
    if done["ok"] != true {
        span.record("signal.outcome", "failed");
        return Err(format!(
            "{block}: {}",
            done["error"].as_str().unwrap_or("the analysis failed")
        ));
    }
    let total_ms = began.elapsed().as_secs_f64() * 1e3;
    span.record("signal.outcome", "ok");
    span.record("signal.total_ms", total_ms);
    crate::song_model::request_resync();
    remember(&fx_guid, &track_guid);
    Ok(json!({
        "track": chain["track"],
        "fx_guid": fx_guid,
        "block": block,
        "kind": kind,
        "from": from,
        "to": to,
        "made": done["made"],
        "lanes": done["lanes_written"],
        "playback": analysed["analysis"]["playback"],
        "render_ms": render_ms,
        "analysis_ms": done["analysis_ms"],
        "total_ms": total_ms,
    }))
}

/// Signal FX with offline blocks, by FX guid, and the revision each was
/// last told: what an edit step checks.
static WATCHED: Mutex<Option<HashMap<String, (String, String)>>> = Mutex::new(None);

fn remember(fx_guid: &str, track_guid: &str) {
    let mut w = WATCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    w.get_or_insert_with(HashMap::new)
        .insert(fx_guid.to_owned(), (track_guid.to_owned(), String::new()));
}

/// After an edit step: each watched Signal FX told its track's source
/// revision, where it moved (the plugin marks blocks made from another
/// one stale).
pub fn after_step(daw: &Standalone, project: &ProjectContext) {
    let watched: Vec<(String, String, String)> = WATCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .map(|w| {
            w.iter()
                .map(|(f, (t, r))| (f.clone(), t.clone(), r.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (fx, track, told) in watched {
        let now = source_revision(daw, project, &track);
        if now == told {
            continue;
        }
        let ran = signal::run(
            daw,
            project,
            (&track, Some(&fx)),
            "SIGNAL_ANALYSIS_STALE",
            &json!({ "source_revision": now }).to_string(),
        );
        if ran.is_ok() {
            if let Some(w) = WATCHED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                w.insert(fx, (track, now));
            }
        }
    }
}

pub fn register(list: &ActionList) -> Result<(), ActionError> {
    list.register_with(
        &ANALYZE,
        Effect::Mutating,
        |_: &ActionCaller, a: AnalyzeArg| {
            let (daw, project) = song()?;
            analyze(daw, &project, &a).map(|v| RawJson::from(v.to_string()))
        },
    )?;
    list.register_with(
        &ANALYSIS,
        Effect::ReadOnly,
        |_: &ActionCaller, a: AnalysisArg| {
            let (daw, project) = song()?;
            let args = a
                .block
                .as_ref()
                .map_or_else(|| json!({}), |b| json!({ "block": b }));
            let out = signal::run(
                daw,
                &project,
                (&a.track, a.fx.as_deref()),
                "SIGNAL_ANALYSIS",
                &args.to_string(),
            )?;
            Ok::<_, String>(RawJson::from(out["result"].to_string()))
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
