//! Listening, for an agent: any point of the mix measured in well under a
//! second, without rendering the whole song.
//!
//! An agent cannot hear, but it reads numbers and looks at pictures. So a
//! listen renders only what feeds the point asked for (`listen::render`:
//! a partial render on warm copies of its plugins), measures it
//! (`listen::measure`), and draws it (`listen::plot`, PNGs under
//! `/tmp/listen/`):
//!
//! - `LISTEN` — one point of one track (its input, before its FX, into or
//!   out of an FX, before or after its fader): loudness, peaks, crest, the
//!   third-octave spectrum, stereo correlation, each dynamics FX's gain
//!   reduction on the way; a waveform and a spectrogram.
//! - `LISTEN_HITS` — a piece's hits, from its hit markers: the averaged
//!   hit of its sum and of every mic feeding it, attack, decay, the first
//!   50 ms against the tail, how much the hits differ; each mic's time
//!   offset and polarity against the reference mic and what it adds to
//!   (or takes from) the sum; a picture of every mic's averaged hit.

pub mod measure;
pub mod plot;
pub mod render;

use std::collections::HashSet;
use std::path::PathBuf;

use architect::action::{ActionCaller, ActionList, ActionMeta, Effect, RawJson};
use daw::service::{Effects, FxChainContext, ProjectContext, Tracks};
use daw_standalone::audio_engine::render::offline::{Probe, ProbeAt};
use daw_standalone::sync::Standalone;
use facet::Facet;
use serde_json::{Value, json};

use crate::actions::{meta, resolve, song};
use crate::mix_measure::analysis::Bands;
use render::RATE;

/// Where the pictures go.
pub const PICTURES: &str = "/tmp/listen";

/// The words `at` takes, for the descriptions.
macro_rules! at_words {
    () => {
        "`at`: `input` (the track's own media alone), `pre-fx` (its media, its children's sum and what is sent to it, before its FX), `fx:<n>:in` / `fx:<n>:out` (into or out of the FX at place `n` — `fx:2:out` — or named `fx:Signal:out`), `post-fx` (after its FX, before its fader; also `pre-fader`), `post-fader` (what it puts into its parent; the default)"
    };
}

static LISTEN: ActionMeta = meta(
    "LISTEN",
    "listen",
    "Listen to a point of the mix",
    concat!(
        "Render offline only what feeds one point of `track` (GUID, path or name) — the track, everything inside it and everything sent into those; the rest of the song is left out — over `from`..`to` (bars `33`, `33:2`; times `12.5s`; sections `Chorus 1`, `Chorus 1 end`), answered from what an earlier listen heard when nothing it hears has changed since (any edit upstream of the point — a parameter, a plugin's state, a fader, an item — is seen and renders again; a kit piece's tracks render together, since their Signal instances hear each other), on copies of its plugins kept warm between calls, and measure it. ",
        at_words!(),
        ". `solo`: tracks (paths or names) heard alone inside what is rendered, as the solo buttons would. `fresh: true` renders everything again, playing nothing back (to check a played-back answer). Answers `figures`: `lufs` (integrated), `short_term` (LUFS, 3 s windows every second), `short_max`, `true_peak` (dBTP), `peak` and `rms` (dBFS; a full-scale sine reads -3), `crest` (dB), `bands` (dB per third-octave band, centres in `bands_hz`), `correlation` (L/R, -1…1), `width` (side/mid), `balance` (L/R dB); `dynamics`: for each FX that is on before the point on the track's own chain and compresses, gates or limits (or is a Signal instance, whose whole chain it then measures), its `makeup_db` (its resting in-to-out gain), `max_gr_db` and `mean_gr_db` (gain against that rest, dB, ≤ 0) and `gr` (every `hop_s` s; null where its input is below -60 dBFS); `latency` (samples the point was late, put right); `rendered`: how it was answered (`answered`: `replayed` — nothing it hears has changed since an earlier listen heard it, so nothing was rendered; `partly` — the tracks whose key is unchanged played back; `rendered`), the `tracks` rendered and those `replayed`, `of` the song's, plugins copied fresh and reused warm (reset), and any that would not copy; the time taken (`copy_ms`, `prime_ms`, `blocks_ms`, `analysis_ms`, `ms`); and two PNGs, `waveform` (L and R, peak and RMS) and `spectrogram` (20 Hz–20 kHz, -96…0 dBFS), under /tmp/listen/ (`png: false`: none). Touches neither playback nor the song."
    ),
    "Mix",
    "",
);

static HITS: ActionMeta = meta(
    "LISTEN_HITS",
    "listen_hits",
    "Listen to a piece's hits",
    concat!(
        "A drum piece's hits (`piece`: Kick, Snare, Tom 1 … as the hit markers name them), each cut out of the mix where it plays now — from `window_pre_ms` (10) before the hit to `window_ms` after (the usual gap to the next hit, at most 400) — between `from` and `to` (bars, times or sections as LISTEN; the first 16 s of the piece's hits when not given), rendered as LISTEN does (only the piece's tracks). Reads each mic of the piece — every track feeding its sum, the folder holding its first close mic (`sum` names another) — at ",
        at_words!(),
        " (for the mics; the sum is read as it adds them, `pre-fx`). Answers the `sum`'s averaged hit: `peak_db`, `rms_db`, `attack_ms` (10 % to 90 % of its peak), `decay_20db_ms` (from its peak until 20 dB down; null if longer than the window), `head` and `tail` (the first 50 ms and the rest: `centroid_hz` and third-octave `bands`, centres in `bands_hz`) and `spread` (how much the hits differ: the standard deviation of each hit's peak, RMS and centroid); `coherence` (the sum's level, the mics' powers added — what the sum would be if they did not correlate — and `coherence_db`, the difference: negative, they cancel); and per mic its own figures and how it sits against the `reference` mic (the piece's first close mic unless given) and in the sum: `offset_samples` and `offset_ms` (positive: late; found on the attack's transients, -2…30 ms, within ±3 ms; `offset_clear: false` when it sits at that limit, no clear alignment), `polarity` (`same` or `inverted`) and `correlation` (the attack's, at that offset, against the reference), `against_rest` (its correlation with the sum of the other mics over the whole window: negative, it pulls against them), `without_db` (the sum with this mic against the sum without it: negative is a mic that cancels part of the sum) and `share` (its in-phase part of the sum's power, negative pulling against it). A PNG of the averaged hit (every mic and the sum overlaid: waveform, envelope in dB, and the first 20 ms close up) goes under /tmp/listen/. Touches neither playback nor the song."
    ),
    "Mix",
    "",
);

/// LISTEN's arguments.
#[derive(Facet)]
struct ListenArg {
    track: String,
    at: Option<String>,
    from: String,
    to: String,
    solo: Option<Vec<String>>,
    png: Option<bool>,
    fresh: Option<bool>,
}

/// LISTEN_HITS's arguments.
#[derive(Facet)]
struct HitsArg {
    piece: String,
    from: Option<String>,
    to: Option<String>,
    at: Option<String>,
    sum: Option<String>,
    reference: Option<String>,
    window_ms: Option<f64>,
    window_pre_ms: Option<f64>,
    png: Option<bool>,
    fresh: Option<bool>,
}

/// Where in a track `word` reads it (see `at_words!`); `track` the
/// track's guid, for an FX named rather than placed.
fn probe_of(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    word: &str,
) -> Result<Probe, String> {
    let w = word.trim().to_ascii_lowercase();
    match w.as_str() {
        "input" => return Ok(Probe::Input),
        "pre-fx" | "prefx" => return Ok(Probe::PreFx),
        "post-fx" | "postfx" | "pre-fader" | "prefader" => return Ok(Probe::PreFader),
        "post-fader" | "postfader" | "" => return Ok(Probe::PostFader),
        _ => {}
    }
    let rest = w.strip_prefix("fx:").ok_or_else(|| {
        format!(
            "`at` {word:?} is not one of input, pre-fx, fx:<n>:in, fx:<n>:out, post-fx, post-fader"
        )
    })?;
    let (which, side) = rest
        .rsplit_once(':')
        .ok_or_else(|| format!("`at` {word:?}: say fx:<n>:in or fx:<n>:out"))?;
    let chain = Effects::list(
        daw,
        project.clone(),
        FxChainContext::Track(track.to_owned()),
    );
    let place = match which.parse::<usize>() {
        Ok(n) if (1..=chain.len()).contains(&n) => n - 1,
        Ok(n) => return Err(format!("the track has {} FX, no #{n}", chain.len())),
        Err(_) => {
            let (_, fx) = crate::fx_actions::target(daw, project, track, which)?;
            chain
                .iter()
                .position(|f| f.guid == fx.guid)
                .ok_or_else(|| format!("{which:?} is not on the track"))?
        }
    };
    match side {
        "in" => Ok(Probe::FxIn(place)),
        "out" => Ok(Probe::FxOut(place)),
        other => Err(format!("`at` {word:?}: {other:?} is neither in nor out")),
    }
}

/// How a probe reads, in `at`'s words.
fn probe_word(p: Probe) -> String {
    match p {
        Probe::Input => "input".into(),
        Probe::PreFx => "pre-fx".into(),
        Probe::FxIn(i) => format!("fx:{}:in", i + 1),
        Probe::FxOut(i) => format!("fx:{}:out", i + 1),
        Probe::PreFader => "post-fx".into(),
        Probe::PostFader => "post-fader".into(),
    }
}

/// Whether an FX named `name` changes level over time: a compressor, a
/// gate, a limiter, an expander — or a Signal instance, whose chain may.
fn is_dynamics(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "comp",
        "gate",
        "limit",
        "expand",
        "dynamic",
        "transient",
        "signal",
        "clip",
    ]
    .iter()
    .any(|w| n.contains(w))
}

/// The FX that are on before `at` on `track`'s chain and work on its
/// dynamics: each one's place and name.
fn dynamics_before(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    at: Probe,
) -> Vec<(usize, String)> {
    let chain = Effects::list(
        daw,
        project.clone(),
        FxChainContext::Track(track.to_owned()),
    );
    let end = match at {
        Probe::Input | Probe::PreFx => 0,
        Probe::FxIn(i) => i,
        Probe::FxOut(i) => i + 1,
        Probe::PreFader | Probe::PostFader => chain.len(),
    };
    chain
        .iter()
        .enumerate()
        .take(end)
        .filter(|(_, f)| f.enabled && is_dynamics(&crate::clap_fx::display_name(f)))
        .map(|(i, f)| (i, crate::clap_fx::display_name(f)))
        .collect()
}

/// A file name for a picture: the action, a slug of what it shows, and
/// the time, so calls never overwrite each other's.
fn picture(kind: &str, what: &str) -> PathBuf {
    let slug: String = what
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let stamp = chrono::Local::now().format("%H%M%S%3f");
    PathBuf::from(PICTURES).join(format!("{kind}-{}-{stamp}.png", slug.trim_matches('-')))
}

/// Draw `svg` to `path`, answering its path (or why not).
fn drawn(svg: Result<String, String>, path: PathBuf) -> Value {
    match svg.and_then(|s| plot::write_png(&s, &path)) {
        Ok(()) => json!(path.display().to_string()),
        Err(why) => json!({ "error": why }),
    }
}

/// The third-octave band centres, as answered.
fn bands_hz() -> Vec<f64> {
    Bands::Third
        .centres()
        .iter()
        .map(|c| (c * 10.0).round() / 10.0)
        .collect()
}

/// Two decimals.
fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// A render's cost and reach, as answered (`path` names a track).
fn rendered_json(h: &render::Heard, path: &dyn Fn(&str) -> String) -> Value {
    let mut names = |g: &[String]| {
        let mut v: Vec<String> = g.iter().map(|g| path(g)).collect();
        v.sort();
        v
    };
    json!({
        "answered": h.answered.word(),
        "tracks": names(&h.rendered),
        "replayed": names(&h.replayed),
        "of": h.song_tracks,
        "plugins_copied": h.copied,
        "plugins_reused": h.reused,
        "plugin_failures": h.failures,
        "plan_ms": r2(h.plan_ms),
        "copy_ms": r2(h.copy_ms),
        "prime_ms": r2(h.prime_ms),
        "blocks_ms": r2(h.blocks_ms),
    })
}

/// A span's two ends, seconds.
fn span_of(
    daw: &Standalone,
    project: &ProjectContext,
    from: &str,
    to: &str,
) -> Result<(f64, f64), String> {
    Ok((
        crate::fx_actions::moment(daw, project, from)?,
        crate::fx_actions::moment(daw, project, to)?,
    ))
}

/// What one listen measured: the point, and the dynamics FX on the way.
pub struct Listened {
    pub answer: Value,
    /// The point's signal, over the span.
    pub signal: Vec<f32>,
}

/// `LISTEN`: render what feeds one point, measure it, draw it.
///
/// # Errors
/// The track, the point or the span is not the song's, or the render failed.
pub fn listen(
    daw: &Standalone,
    project: &ProjectContext,
    a: &ListenArgs<'_>,
) -> Result<Listened, String> {
    let began = web_time::Instant::now();
    let song = crate::fx_actions::project_guid(project)?;
    let track = resolve(daw, project, a.track)?;
    let at = probe_of(daw, project, &track, a.at.unwrap_or("post-fader"))?;
    let (from, to) = span_of(daw, project, a.from, a.to)?;
    let solo: Option<HashSet<String>> = match a.solo {
        Some(names) if !names.is_empty() => Some(
            names
                .iter()
                .map(|n| resolve(daw, project, n))
                .collect::<Result<_, _>>()?,
        ),
        _ => None,
    };
    let dynamics = dynamics_before(daw, project, &track, at);
    let mut points = vec![ProbeAt {
        track: track.clone(),
        at,
    }];
    for (i, _) in &dynamics {
        points.push(ProbeAt {
            track: track.clone(),
            at: Probe::FxIn(*i),
        });
        points.push(ProbeAt {
            track: track.clone(),
            at: Probe::FxOut(*i),
        });
    }
    let heard = render::render(
        daw,
        project,
        &song,
        &render::Ask {
            from,
            to,
            points: &points,
            solo,
            bypass_fx: HashSet::new(),
            keyed: &keyed(daw, project),
            fresh: a.fresh,
        },
    )?;
    let analysed = web_time::Instant::now();
    let signal = heard.signals.first().cloned().unwrap_or_default();
    let figures = measure::figures(&signal, RATE);
    let gains: Vec<Value> = dynamics
        .iter()
        .enumerate()
        .map(|(k, (place, name))| {
            let (i, o) = (&heard.signals[1 + 2 * k], &heard.signals[2 + 2 * k]);
            let mut g = measure::gain_reduction(i, o, RATE, 0.1);
            g["fx"] = json!(format!("#{} {name}", place + 1));
            g
        })
        .collect();
    let analysis_ms = analysed.elapsed().as_secs_f64() * 1e3;
    let all = Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&all);
    let path = paths.get(&track).cloned().unwrap_or(track.clone());
    let what = format!("{path} {}", probe_word(at));
    let pictures = if a.png {
        let title = format!("{what} · {from:.2}–{to:.2} s");
        json!({
            "waveform": drawn(Ok(plot::waveform(&signal, RATE, from, &title)), picture("waveform", &what)),
            "spectrogram": drawn(plot::spectrogram(&signal, RATE, from, &title), picture("spectrogram", &what)),
        })
    } else {
        Value::Null
    };
    let answer = json!({
        "track": path,
        "at": probe_word(at),
        "from": from,
        "to": to,
        "figures": figures,
        "bands_hz": bands_hz(),
        "dynamics": gains,
        "latency": heard.latencies.first(),
        "rendered": rendered_json(&heard, &|g: &str| paths.get(g).cloned().unwrap_or_else(|| g.to_owned())),
        "png": pictures,
        "analysis_ms": r2(analysis_ms),
        "ms": r2(began.elapsed().as_secs_f64() * 1e3),
    });
    Ok(Listened { answer, signal })
}

/// What [`listen`] is asked.
pub struct ListenArgs<'a> {
    pub track: &'a str,
    pub at: Option<&'a str>,
    pub from: &'a str,
    pub to: &'a str,
    pub solo: Option<&'a [String]>,
    pub png: bool,
    pub fresh: bool,
}

/// The tracks other tracks' plugins key from, which a listen runs
/// whenever it reaches what they key: the kit's triggers (each one's
/// Signal instance publishes its piece's hits to the session, and the
/// piece's gates follow them), each with its piece's mics.
fn keyed(daw: &Standalone, project: &ProjectContext) -> Vec<render::Keyed> {
    let (kit, _) = crate::hits::kit(daw, project);
    kit.into_iter()
        .map(|p| render::Keyed {
            from: p.trigger,
            by: p.mics,
        })
        .collect()
}

/// The hits of `piece` between `from` and `to`, project seconds.
fn hit_times(
    daw: &Standalone,
    project: &ProjectContext,
    piece: &str,
    span: Option<(f64, f64)>,
) -> Vec<f64> {
    crate::hits::playback(daw, project)
        .iter()
        .filter(|h| h.piece.eq_ignore_ascii_case(piece))
        .filter(|h| span.is_none_or(|(f, t)| h.time >= f && h.time < t))
        .map(|h| h.time)
        .collect()
}

/// The tracks of a piece: its sum (the folder holding its first close mic,
/// or `sum`), the mics feeding it in track order, and its reference mic.
struct PieceTracks {
    sum: String,
    mics: Vec<String>,
    reference: String,
}

/// `piece`'s tracks in song `project`.
fn piece_tracks(
    daw: &Standalone,
    project: &ProjectContext,
    piece: &str,
    (sum, reference): (Option<&str>, Option<&str>),
) -> Result<PieceTracks, String> {
    let (kit, _) = crate::hits::kit(daw, project);
    let found = kit.iter().find(|p| p.name.eq_ignore_ascii_case(piece));
    let all = Tracks::all(daw, project.clone());
    let first_mic = found.and_then(|p| p.mics.first()).cloned();
    let sum = match sum {
        Some(s) => resolve(daw, project, s)?,
        None => first_mic
            .as_deref()
            .and_then(|m| all.iter().find(|t| t.guid == m))
            .and_then(|t| t.parent_guid.clone())
            .ok_or_else(|| {
                format!(
                    "no piece {piece:?} with a close mic in a folder — the kit has {}",
                    kit.iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?,
    };
    let mics: Vec<String> = all
        .iter()
        .filter(|t| t.parent_guid.as_deref() == Some(sum.as_str()) && t.parent_send)
        .map(|t| t.guid.clone())
        .collect();
    let reference = match reference {
        Some(r) => resolve(daw, project, r)?,
        None => first_mic
            .filter(|m| mics.contains(m))
            .or_else(|| mics.first().cloned())
            .ok_or("the sum has no mics")?,
    };
    Ok(PieceTracks {
        sum,
        mics,
        reference,
    })
}

/// `LISTEN_HITS`.
///
/// # Errors
/// No such piece, no hits in the span, or the render failed.
fn listen_hits(daw: &Standalone, project: &ProjectContext, a: &HitsArg) -> Result<Value, String> {
    let span = tracing::info_span!(
        "LISTEN_HITS",
        listen.piece = %a.piece,
        listen.hits = tracing::field::Empty,
        listen.mics = tracing::field::Empty,
        listen.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let song = crate::fx_actions::project_guid(project)?;
    let tracks = piece_tracks(
        daw,
        project,
        &a.piece,
        (a.sum.as_deref(), a.reference.as_deref()),
    )?;
    let asked = match (&a.from, &a.to) {
        (Some(f), Some(t)) => Some(span_of(daw, project, f, t)?),
        (Some(f), None) => Some((crate::fx_actions::moment(daw, project, f)?, f64::MAX)),
        (None, Some(t)) => Some((0.0, crate::fx_actions::moment(daw, project, t)?)),
        (None, None) => None,
    };
    let mut times = hit_times(daw, project, &a.piece, asked);
    if asked.is_none()
        && let Some(first) = times.first().copied()
    {
        times.retain(|t| *t < first + 16.0);
    }
    if times.is_empty() {
        return Err(format!("no {} hits in the span", a.piece));
    }
    let pre_ms = a.window_pre_ms.unwrap_or(10.0).clamp(0.0, 200.0);
    // Up to the next hit, as they usually come, so the average is of one
    // stroke and not of the next one too.
    let post_ms = a
        .window_ms
        .unwrap_or_else(|| measure::median_gap(&times).map_or(400.0, |g| (g * 1e3).min(400.0)))
        .clamp(20.0, 4_000.0);
    let (pre, post) = (
        (pre_ms * f64::from(RATE) / 1e3) as usize,
        (post_ms * f64::from(RATE) / 1e3) as usize,
    );
    let from = times[0] - pre_ms / 1e3;
    let to = times[times.len() - 1] + post_ms / 1e3;
    let at = probe_of(
        daw,
        project,
        &tracks.reference,
        a.at.as_deref().unwrap_or("post-fader"),
    )?;
    let mut points: Vec<ProbeAt> = tracks
        .mics
        .iter()
        .map(|m| ProbeAt {
            track: m.clone(),
            at,
        })
        .collect();
    points.push(ProbeAt {
        track: tracks.sum.clone(),
        at: Probe::PreFx,
    });
    let heard = render::render(
        daw,
        project,
        &song,
        &render::Ask {
            from,
            to,
            points: &points,
            solo: None,
            bypass_fx: HashSet::new(),
            keyed: &keyed(daw, project),
            fresh: a.fresh.unwrap_or(false),
        },
    )?;
    let analysed = web_time::Instant::now();
    let frames: Vec<usize> = times
        .iter()
        .map(|t| ((t - from) * f64::from(RATE)).round().max(0.0) as usize)
        .collect();
    let cut = |signal: &[f32]| measure::windows(&measure::mid(signal), &frames, (pre, post));
    let mic_windows: Vec<Vec<Vec<f32>>> = heard.signals[..tracks.mics.len()]
        .iter()
        .map(|s| cut(s))
        .collect();
    let sum_windows = cut(&heard.signals[tracks.mics.len()]);
    let sum_avg = measure::average(&sum_windows);
    let reference = tracks
        .mics
        .iter()
        .position(|m| *m == tracks.reference)
        .unwrap_or(0);
    let ref_avg = measure::average(&mic_windows[reference]);
    let all = Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&all);
    let path = |g: &str| paths.get(g).cloned().unwrap_or_else(|| g.to_owned());
    let most_lag = (0.003 * f64::from(RATE)) as usize;
    // The attack, where mics line up: 2 ms before the hit to 30 ms after.
    let ms_frames = |ms: f64| (ms * f64::from(RATE) / 1e3) as usize;
    let attack = pre.saturating_sub(ms_frames(2.0))..(pre + ms_frames(30.0));
    let mut drawn_mics: Vec<(String, Vec<f32>, Vec<f32>)> = Vec::new();
    let mics: Vec<Value> = tracks
        .mics
        .iter()
        .zip(&mic_windows)
        .map(|(m, w)| {
            let avg = measure::average(w);
            let (lag, clear, corr) = measure::alignment(&ref_avg, &avg, attack.clone(), most_lag);
            let (without, share) = measure::contribution(w, &sum_windows);
            let mut row = measure::hit_figures(&avg, w, RATE, pre);
            let name = path(m);
            row["path"] = json!(name);
            row["offset_samples"] = json!(lag);
            row["offset_ms"] = json!(r2(lag as f64 * 1e3 / f64::from(RATE)));
            row["offset_clear"] = json!(clear);
            row["polarity"] = json!(if corr < 0.0 { "inverted" } else { "same" });
            row["correlation"] = json!(corr);
            row["against_rest"] = json!(measure::against_rest(w, &sum_windows));
            row["without_db"] = json!(without);
            row["share"] = json!(share);
            let env = measure::envelope(&avg, RATE, 5.0);
            drawn_mics.push((
                name.rsplit('/').next().unwrap_or(&name).to_owned(),
                avg,
                env,
            ));
            row
        })
        .collect();
    let refs: Vec<&[Vec<f32>]> = mic_windows.iter().map(Vec::as_slice).collect();
    let coherence = measure::coherence(&refs, &sum_windows);
    let sum_name = path(&tracks.sum);
    let sum_env = measure::envelope(&sum_avg, RATE, 5.0);
    drawn_mics.push((
        format!("{} (sum)", sum_name.rsplit('/').next().unwrap_or(&sum_name)),
        sum_avg.clone(),
        sum_env,
    ));
    let analysis_ms = analysed.elapsed().as_secs_f64() * 1e3;
    let picture_path = if a.png.unwrap_or(true) {
        let title = format!(
            "{} · {} hits averaged · mics at {} · {:.2}–{:.2} s",
            a.piece,
            times.len(),
            probe_word(at),
            times[0],
            times[times.len() - 1]
        );
        drawn(
            Ok(plot::hit(&drawn_mics, RATE, pre_ms, &title)),
            picture("hits", &a.piece),
        )
    } else {
        Value::Null
    };
    let ms = began.elapsed().as_secs_f64() * 1e3;
    span.record("listen.hits", times.len());
    span.record("listen.mics", tracks.mics.len());
    span.record("listen.ms", ms);
    Ok(json!({
        "piece": a.piece,
        "hits": times.len(),
        "from": times[0],
        "to": times[times.len() - 1],
        "window_ms": [-pre_ms, post_ms],
        "mics_at": probe_word(at),
        "reference": path(&tracks.reference),
        "sum": { "path": sum_name, "figures": measure::hit_figures(&sum_avg, &sum_windows, RATE, pre) },
        "coherence": coherence,
        "mics": mics,
        "bands_hz": bands_hz(),
        "png": picture_path,
        "rendered": rendered_json(&heard, &|g: &str| paths.get(g).cloned().unwrap_or_else(|| g.to_owned())),
        "analysis_ms": r2(analysis_ms),
        "ms": r2(ms),
    }))
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register_with(
        &LISTEN,
        Effect::ReadOnly,
        |_: &ActionCaller, a: ListenArg| {
            let (daw, project) = song()?;
            let span = tracing::info_span!(
                "LISTEN",
                listen.at = a.at.as_deref().unwrap_or("post-fader"),
                listen.ms = tracing::field::Empty,
            );
            let _in = span.enter();
            let heard = listen(
                daw,
                &project,
                &ListenArgs {
                    track: &a.track,
                    at: a.at.as_deref(),
                    from: &a.from,
                    to: &a.to,
                    solo: a.solo.as_deref(),
                    png: a.png.unwrap_or(true),
                    fresh: a.fresh.unwrap_or(false),
                },
            )?;
            span.record("listen.ms", heard.answer["ms"].as_f64().unwrap_or(0.0));
            Ok::<_, String>(RawJson::from(heard.answer.to_string()))
        },
    )?;
    list.register_with(&HITS, Effect::ReadOnly, |_: &ActionCaller, a: HitsArg| {
        let (daw, project) = song()?;
        listen_hits(daw, &project, &a).map(|v| RawJson::from(v.to_string()))
    })?;
    Ok(())
}
