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

pub mod ab;
pub mod align;
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
        "`at`: `input` (the track's own media alone), `pre-fx` (its media, its children's sum and what is sent to it, before its FX), `fx:<n>:in` / `fx:<n>:out` (into or out of the FX at place `n` — `fx:2:out` — or named `fx:Signal:out`), `post-fx` (after its FX, before its fader; also `pre-fader`), `post-fader` (what it puts into its parent; the default), `block:<id>:pre` / `block:<id>:post` (arriving at or leaving one block of the track's Signal instance, its id as SIGNAL_CHAIN gives it — `block:rescue.gate:pre`: rendered on a copy of the instance with that block, the rest of its phase and every later phase bypassed and the phase's make-up at 0; LISTEN, LISTEN_RENDER and LISTEN_AB only)"
    };
}

static LISTEN: ActionMeta = meta(
    "LISTEN",
    "listen",
    "Listen to a point of the mix",
    concat!(
        "Render offline only what feeds one point of `track` (GUID, path or name) — the track, everything inside it and everything sent into those; the rest of the song is left out — over `from`..`to` (bars `33`, `33:2`; times `12.5s`; sections `Chorus 1`, `Chorus 1 end`), answered from what an earlier listen heard when nothing it hears has changed since (any edit upstream of the point — a parameter, a plugin's state, a fader, an item — is seen and renders again; a kit piece's tracks render together, since their Signal instances hear each other), on copies of its plugins kept warm between calls, and measure it. ",
        at_words!(),
        ". `solo`: tracks (paths or names) heard alone inside what is rendered, as the solo buttons would. `fresh: true` renders everything again, playing nothing back (to check a played-back answer). A span over 6 s is rendered in up to three chunks side by side, each from a 1 s preroll of its own: about twice as fast, within about 0.05 LU of a whole render and a few tenths of a dB on peaks and crest; `exact: true` renders it in one piece, as MIX_MEASURE would to the sample (A/B differences under ~0.3 dB call for it). Answers `figures`: `lufs` (integrated), `short_term` (LUFS, 3 s windows every second), `short_max`, `true_peak` (dBTP), `peak` and `rms` (dBFS; a full-scale sine reads -3), `crest` (dB), `bands` (dB per third-octave band, centres in `bands_hz`), `correlation` (L/R, -1…1), `width` (side/mid), `balance` (L/R dB); `dynamics`: for each FX that is on before the point on the track's own chain and compresses, gates or limits (or is a Signal instance, whose whole chain it then measures), its `makeup_db` (its resting in-to-out gain), `max_gr_db` and `mean_gr_db` (gain against that rest, dB, ≤ 0) and `gr` (every `hop_s` s; null where its input is below -60 dBFS); `latency` (samples the point was late, put right); `rendered`: how it was answered (`answered`: `replayed` — nothing it hears has changed since an earlier listen heard it, so nothing was rendered; `partly` — the tracks whose key is unchanged played back; `rendered`), the `tracks` rendered and those `replayed`, `of` the song's, plugins copied fresh and reused warm (reset), and any that would not copy; the time taken (`copy_ms`, `prime_ms`, `blocks_ms`, `analysis_ms`, `ms`); and two PNGs, `waveform` (L and R, peak and RMS) and `spectrogram` (20 Hz–20 kHz, -96…0 dBFS), under /tmp/listen/ (`png: false`: none). Touches neither playback nor the song."
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

static RENDER: ActionMeta = meta(
    "LISTEN_RENDER",
    "listen_render",
    "Render a point of the mix to a file",
    concat!(
        "The audio at one point of `track` (GUID, path or name) over `from`..`to` (bars, times or sections as LISTEN), rendered as LISTEN renders it (only what feeds the point; answered from an earlier listen when nothing it hears has changed), written to the WAV `to_file` (48 kHz, 32-bit float, stereo; under /tmp/listen/ when not given). ",
        at_words!(),
        ". What a tool that needs the audio itself reads — an offline Signal block's analysis renders `block:<id>:pre` through it. Answers `file`, `seconds`, `rate`, `lufs`, `true_peak`, `latency` (samples the point was late, put right), `answered` (replayed, partly, rendered) and the time taken. Touches neither playback nor the song."
    ),
    "Mix",
    "",
);

static AB: ActionMeta = meta(
    "LISTEN_AB",
    "listen_ab",
    "Listen before and after a change",
    concat!(
        "Listen to one point of `track` over `from`..`to` (as LISTEN: ",
        at_words!(),
        "), make `change` — a list of action calls, each `{\"action\": \"FX_SET\", \"args\": {…}}` (args an object, or its JSON as a string), run in order as one undo step, stopping at the first that fails — listen again, and put the mix back as it was unless `keep: true` — every fader, pan, mute, solo, polarity, send, FX switch, held parameter value and plugin state that differs, as MIX_RESTORE does (so Signal's own edits through SIGNAL_RUN go back too; an edit to items or automation is not part of the mix and is not put back). Answers `a` and `b` (LISTEN's `figures` before and after), `difference` (b − a: every figure, and the bands band by band, dB, centres in `bands_hz`), `changed` (each call's answer), `kept`, `put_back` (what was `changed` back, and how many `plugin_states` were loaded), `a_ms`, `change_ms`, `b_ms`, `back_ms`, `ms`, and a PNG of both spectra and their difference under /tmp/listen/ (`png: false`: none). A change that failed is put back the same way, and its error said."
    ),
    "Mix",
    "",
);

static ALIGN: ActionMeta = meta(
    "LISTEN_ALIGN",
    "listen_align",
    "Align a piece's mics",
    "Find how each mic of a drum piece (`piece`, as LISTEN_HITS: its mics read where they reach the sum, its hits from its hit markers between `from` and `to`) should be moved (an offset within `max_ms`, 3 unless given), turned over (`invert: false` forbids it) and phase-rotated in one band (`rotate: false` forbids it: a band around 40–250 Hz, width Q 1, turned by up to ±180°) so that the piece's sum is fullest — scored on the sum, over the hits only, not on how alike two mics are: `level` (the sum's power), `low` (below 150 Hz) and `attack` (its transients over the first 10 ms: the power of its first difference), each in dB against the sum as it is, added into `score`; within 0.1 dB the smaller move wins. Mics are taken in turn (the most of the sum first, the reference mic left where it is), each against the sum with the ones before it set. Answers per mic: `now` (its offset and polarity against the reference, what it adds to the sum), `candidates` (the best offsets and polarities, and the best rotation, each `{delay_samples, invert, rotate_hz, rotate_deg, rotate_q, gains: {level, low, attack, score}}`), `chosen` (the best, when it gains more than 0.2 dB; else none); and `predicted` (the whole sum's gains with every chosen fix). `apply: true` makes the fixes, confirms them and keeps them only if the sum is better: on a track whose Signal instance has the `align` block, through it (added in its first phase if missing; `delay_samples`, `invert`, `rotate_hz`, `rotate_deg`, `rotate_q`), else only a polarity, through the track's polarity (the rest listed in `not_applied`); then the hits are heard again and `measured` says what the sum gained; worse on `score`, and the mix is put back as it was (as LISTEN_AB puts it back). A PNG of the sum's averaged hit before and after (and each fixed mic) goes under /tmp/listen/. `exact` as LISTEN.",
    "Mix",
    "",
);

/// LISTEN_ALIGN's arguments.
#[derive(Facet)]
struct AlignArg {
    piece: String,
    from: Option<String>,
    to: Option<String>,
    sum: Option<String>,
    reference: Option<String>,
    max_ms: Option<f64>,
    invert: Option<bool>,
    rotate: Option<bool>,
    apply: Option<bool>,
    exact: Option<bool>,
    png: Option<bool>,
}

/// LISTEN_RENDER's arguments.
#[derive(Facet)]
struct RenderArg {
    track: String,
    at: Option<String>,
    from: String,
    to: String,
    to_file: Option<String>,
    exact: Option<bool>,
}

/// LISTEN_AB's arguments, for its schema (it reads its JSON itself: a
/// call's `args` may be an object).
#[derive(Facet)]
struct AbDoc {
    track: String,
    at: Option<String>,
    from: String,
    to: String,
    /// Each `{action, args}`.
    change: Vec<String>,
    keep: Option<bool>,
    solo: Option<Vec<String>>,
    png: Option<bool>,
    exact: Option<bool>,
}

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
    exact: Option<bool>,
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
    exact: Option<bool>,
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

/// A point of a track: where along it, and for a point inside a Signal
/// instance (`block:<id>:pre|post`), the instance (FX guid) and its cut.
#[derive(Clone, Debug)]
pub struct At {
    pub probe: Probe,
    pub cut: Option<(String, render::Cut)>,
}

/// Where in `track` (its guid) `word` reads it: `at_words!`, or
/// `block:<id>:pre|post` — the signal arriving at, or leaving, one block
/// of the track's Signal instance (its id as SIGNAL_CHAIN gives it).
///
/// # Errors
/// Not one of the words, or no such FX or block on the track.
pub fn point_of(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    word: &str,
) -> Result<At, String> {
    let Some(rest) = word.trim().strip_prefix("block:") else {
        return probe_of(daw, project, track, word).map(|probe| At { probe, cut: None });
    };
    let (block, side) = rest
        .rsplit_once(':')
        .ok_or_else(|| format!("`at` {word:?}: say block:<id>:pre or block:<id>:post"))?;
    let side = match side {
        "pre" | "in" => render::Side::Pre,
        "post" | "out" => render::Side::Post,
        other => return Err(format!("`at` {word:?}: {other:?} is neither pre nor post")),
    };
    let chain = Effects::list(
        daw,
        project.clone(),
        FxChainContext::Track(track.to_owned()),
    );
    // The Signal instance on the track whose chain holds the block.
    let holds = |fx: &daw_proto::fx::Fx| {
        crate::clap_host::signal::chain(daw, project, track, Some(&fx.guid))
            .ok()
            .is_some_and(|c| {
                c["chain"]["phases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p["blocks"].as_array())
                    .flatten()
                    .any(|b| b["id"] == block)
            })
    };
    let (place, fx) = chain
        .iter()
        .enumerate()
        .find(|(_, fx)| crate::signal_kit::is_signal_bundle(&fx.name) && holds(fx))
        .ok_or_else(|| {
            format!(
                "no Signal instance on the track has a block {block:?} (SIGNAL_CHAIN lists them)"
            )
        })?;
    Ok(At {
        probe: Probe::FxOut(place),
        cut: Some((
            fx.guid.clone(),
            render::Cut {
                block: block.to_owned(),
                side,
            },
        )),
    })
}

/// The words for a point, back.
fn at_word(at: &At) -> String {
    match &at.cut {
        Some((_, cut)) => format!(
            "block:{}:{}",
            cut.block,
            match cut.side {
                render::Side::Pre => "pre",
                render::Side::Post => "post",
            }
        ),
        None => probe_word(at.probe),
    }
}

/// The cuts a point asks for, by FX guid.
fn cuts_of(at: &At) -> std::collections::HashMap<String, render::Cut> {
    at.cut.iter().cloned().collect()
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
    let names = |g: &[String]| {
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
        "chunks": h.chunks,
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
    let point = point_of(daw, project, &track, a.at.unwrap_or("post-fader"))?;
    let at = point.probe;
    let cuts = cuts_of(&point);
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
            cuts: &cuts,
            exact: a.exact,
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
    let what = format!("{path} {}", at_word(&point));
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
        "at": at_word(&point),
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
    pub exact: bool,
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

/// A piece's hits, cut out of the mix: what LISTEN_HITS and LISTEN_ALIGN
/// both read.
struct HitWindows {
    tracks: PieceTracks,
    /// The hits, project seconds.
    times: Vec<f64>,
    pre_ms: f64,
    post_ms: f64,
    /// The window's frames before and after each hit.
    pre: usize,
    post: usize,
    /// Where the mics are read.
    at: Probe,
    /// Each mic's windows (mono), in `tracks.mics` order; the sum's.
    mic_windows: Vec<Vec<Vec<f32>>>,
    sum_windows: Vec<Vec<f32>>,
    heard: render::Heard,
}

/// Render a piece's mics and sum over its hits (as LISTEN_HITS asks) and
/// cut each hit's window out of them.
///
/// # Errors
/// No such piece, no hits in the span, or the render failed.
fn hit_windows(
    daw: &Standalone,
    project: &ProjectContext,
    a: &HitsArg,
) -> Result<HitWindows, String> {
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
            cuts: &std::collections::HashMap::new(),
            exact: a.exact.unwrap_or(false),
        },
    )?;
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
    Ok(HitWindows {
        tracks,
        times,
        pre_ms,
        post_ms,
        pre,
        post,
        at,
        mic_windows,
        sum_windows,
        heard,
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
    let HitWindows {
        tracks,
        times,
        pre_ms,
        post_ms,
        pre,
        at,
        mic_windows,
        sum_windows,
        heard,
        ..
    } = hit_windows(daw, project, a)?;
    let analysed = web_time::Instant::now();
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

/// What to render at a point: [`render_at`]'s question.
pub struct RenderAt<'a> {
    /// A track's GUID, path or name.
    pub track: &'a str,
    /// Where along it, in LISTEN's words (`post-fader`, `fx:2:out`,
    /// `block:rescue.gate:pre`, …).
    pub at: &'a str,
    /// The span, seconds.
    pub from: f64,
    pub to: f64,
    /// Rendered in one piece (see LISTEN's `exact`).
    pub exact: bool,
}

/// The audio at a point.
pub struct Audio {
    /// Interleaved stereo, exactly `from`..`to`.
    pub samples: Vec<f32>,
    pub rate: u32,
    /// How late the point was, samples, put right.
    pub latency: u32,
    pub answered: render::Answered,
}

/// The audio arriving at one point of the mix over a span, rendered as
/// LISTEN renders it: only what feeds the point, answered from an earlier
/// listen when nothing it hears has changed. The entry point for any tool
/// that needs the audio itself (an offline Signal block analyses
/// `block:<id>:pre` through it; LISTEN_RENDER writes it to a file).
///
/// # Errors
/// No such track or point, a bad span, or the render failed.
pub fn render_at(
    daw: &Standalone,
    project: &ProjectContext,
    r: &RenderAt<'_>,
) -> Result<Audio, String> {
    let song = crate::fx_actions::project_guid(project)?;
    let track = resolve(daw, project, r.track)?;
    let point = point_of(daw, project, &track, r.at)?;
    let points = [ProbeAt {
        track,
        at: point.probe,
    }];
    let heard = render::render(
        daw,
        project,
        &song,
        &render::Ask {
            from: r.from,
            to: r.to,
            points: &points,
            solo: None,
            bypass_fx: HashSet::new(),
            keyed: &keyed(daw, project),
            fresh: false,
            cuts: &cuts_of(&point),
            exact: r.exact,
        },
    )?;
    Ok(Audio {
        latency: heard.latencies.first().copied().unwrap_or(0),
        samples: heard.signals.into_iter().next().unwrap_or_default(),
        rate: RATE,
        answered: heard.answered,
    })
}

/// `LISTEN_RENDER`.
///
/// # Errors
/// As [`render_at`], or the file would not be written.
fn listen_render(
    daw: &Standalone,
    project: &ProjectContext,
    a: &RenderArg,
) -> Result<Value, String> {
    let began = web_time::Instant::now();
    let (from, to) = span_of(daw, project, &a.from, &a.to)?;
    let at = a.at.as_deref().unwrap_or("post-fader");
    let audio = render_at(
        daw,
        project,
        &RenderAt {
            track: &a.track,
            at,
            from,
            to,
            exact: a.exact.unwrap_or(false),
        },
    )?;
    let file = a.to_file.as_ref().map_or_else(
        || {
            let p = picture("render", &format!("{} {at}", a.track));
            p.with_extension("wav")
        },
        PathBuf::from,
    );
    if let Some(dir) = file.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let (l, r) = measure::channels(&audio.samples);
    let stats = crate::mix_measure::analysis::measure(&l, &r, RATE, Bands::Octave);
    fts_sample::write_wav_f32(&file, RATE, &[l, r])
        .map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(json!({
        "file": file.display().to_string(),
        "seconds": to - from,
        "rate": audio.rate,
        "lufs": stats.lufs,
        "true_peak": stats.true_peak,
        "latency": audio.latency,
        "answered": audio.answered.word(),
        "ms": r2(began.elapsed().as_secs_f64() * 1e3),
    }))
}

/// LISTEN_AB's arguments, read from its JSON.
struct AbArgs {
    track: String,
    at: Option<String>,
    from: String,
    to: String,
    change: Vec<(String, String)>,
    keep: bool,
    solo: Option<Vec<String>>,
    png: bool,
    exact: bool,
}

/// Read LISTEN_AB's JSON.
fn ab_args(raw: &[u8]) -> Result<AbArgs, String> {
    let v: Value = serde_json::from_slice(if raw.is_empty() { b"{}" } else { raw })
        .map_err(|e| e.to_string())?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let need = |k: &str| text(k).ok_or_else(|| format!("`{k}` is missing"));
    let change = v
        .get("change")
        .and_then(Value::as_array)
        .ok_or("`change` is missing: a list of {action, args}")?
        .iter()
        .map(|c| {
            let action = c
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("a change has no `action`: {c}"))?;
            let args = match c.get("args") {
                None | Some(Value::Null) => "{}".to_owned(),
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
            };
            Ok((action.to_owned(), args))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(AbArgs {
        track: need("track")?,
        at: text("at"),
        from: need("from")?,
        to: need("to")?,
        change,
        keep: v.get("keep").and_then(Value::as_bool).unwrap_or(false),
        solo: v.get("solo").and_then(Value::as_array).map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        }),
        png: v.get("png").and_then(Value::as_bool).unwrap_or(true),
        exact: v.get("exact").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// `LISTEN_AB`.
///
/// # Errors
/// The first listen failed (nothing was changed then), or the second did
/// (the change is put back first).
fn listen_ab(
    daw: &Standalone,
    project: &ProjectContext,
    list: &ActionList,
    a: &AbArgs,
) -> Result<Value, String> {
    let span = tracing::info_span!(
        "LISTEN_AB",
        listen.calls = a.change.len(),
        listen.kept = a.keep,
        listen.outcome = tracing::field::Empty,
        listen.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let ask = |png: bool| ListenArgs {
        track: &a.track,
        at: a.at.as_deref(),
        from: &a.from,
        to: &a.to,
        solo: a.solo.as_deref(),
        png,
        fresh: false,
        exact: a.exact,
    };
    let before = listen(daw, project, &ask(false))?;
    let a_ms = began.elapsed().as_secs_f64() * 1e3;
    let mix = crate::mix_snapshot::current(daw, project)?;
    let changing = web_time::Instant::now();
    let changed = ab::change(daw, project, list, &a.change);
    let change_ms = changing.elapsed().as_secs_f64() * 1e3;
    let undo = |span: &tracing::Span| {
        let back = ab::put_back(daw, project, &mix);
        if let Err(why) = &back {
            span.record("listen.outcome", "put_back_failed");
            tracing::warn!(listen.why = %why, "LISTEN_AB: the mix would not go back");
        }
        back
    };
    let said = match changed {
        Ok(said) => said,
        Err((said, why)) => {
            let back = undo(&span);
            span.record("listen.outcome", "change_failed");
            return Err(format!(
                "{why} (after {} call(s) that ran; put back: {})",
                said.len(),
                match back {
                    Ok(b) => format!(
                        "{} setting(s), {} plugin state(s)",
                        b.changed.len(),
                        b.states
                    ),
                    Err(e) => format!("FAILED: {e}"),
                }
            ));
        }
    };
    let b_began = web_time::Instant::now();
    let after = listen(daw, project, &ask(false));
    let b_ms = b_began.elapsed().as_secs_f64() * 1e3;
    let backing = web_time::Instant::now();
    let back = (!a.keep || after.is_err())
        .then(|| undo(&span))
        .transpose()?;
    let back_ms = backing.elapsed().as_secs_f64() * 1e3;
    let after = after?;
    let fa = &before.answer["figures"];
    let fb = &after.answer["figures"];
    let png = if a.png {
        let centres = Bands::Third.centres();
        let bands = |f: &Value| -> Vec<f64> {
            f["bands"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .collect()
        };
        let title = format!(
            "{} {} · A (before) and B (after) · {}–{}",
            before.answer["track"].as_str().unwrap_or(""),
            before.answer["at"].as_str().unwrap_or(""),
            a.from,
            a.to
        );
        drawn(
            Ok(plot::spectra(
                &centres,
                &[
                    ("A before".into(), bands(fa)),
                    ("B after".into(), bands(fb)),
                ],
                &title,
            )),
            picture(
                "ab",
                &format!("{} {}", a.track, a.at.as_deref().unwrap_or("post-fader")),
            ),
        )
    } else {
        Value::Null
    };
    let ms = began.elapsed().as_secs_f64() * 1e3;
    span.record("listen.ms", ms);
    Ok(json!({
        "track": before.answer["track"],
        "at": before.answer["at"],
        "from": before.answer["from"],
        "to": before.answer["to"],
        "a": fa,
        "b": fb,
        "difference": ab::difference(fa, fb),
        "bands_hz": bands_hz(),
        "changed": said,
        "kept": a.keep,
        "put_back": back.map(|b| json!({ "changed": b.changed, "plugin_states": b.states })),
        "a_answered": before.answer["rendered"]["answered"],
        "b_answered": after.answer["rendered"]["answered"],
        "png": png,
        "a_ms": r2(a_ms),
        "change_ms": r2(change_ms),
        "b_ms": r2(b_ms),
        "back_ms": r2(back_ms),
        "ms": r2(ms),
    }))
}

/// A fix, as answered.
fn fix_json(c: &align::Candidate) -> Value {
    json!({
        "delay_samples": c.fix.delay,
        "delay_ms": r2(c.fix.delay as f64 * 1e3 / f64::from(RATE)),
        "invert": c.fix.invert,
        "rotate_hz": c.fix.rotate.map(|r| r.hz),
        "rotate_deg": c.fix.rotate.map(|r| r.deg),
        "rotate_q": c.fix.rotate.map(|r| r.q),
        "gains": {
            "level": r2(c.gains.level),
            "low": r2(c.gains.low),
            "attack": r2(c.gains.attack),
            "score": r2(c.gains.score()),
        },
    })
}

/// A chosen fix must gain this much on the sum, dB.
const ALIGN_LEAST: f64 = 0.2;

/// One mic's search: its candidates and the one chosen (if any gains).
struct Searched {
    candidates: Vec<align::Candidate>,
    chosen: Option<align::Candidate>,
}

/// Search one mic against `sum`.
fn search(
    w: &align::Windows,
    mic: &[Vec<f32>],
    sum: &[Vec<f32>],
    (most, invert, rotate): (i64, bool, bool),
) -> Searched {
    let offsets = align::offsets(w, mic, sum, (most, invert));
    let mut candidates: Vec<align::Candidate> = offsets.iter().take(3).copied().collect();
    let mut best = offsets.first().copied();
    if rotate && let Some(b) = best {
        if let Some(r) = align::rotations(w, mic, sum, &b.fix).first().copied() {
            candidates.push(r);
            if r.gains.score() > b.gains.score() + align::CLOSE {
                best = Some(r);
            }
        }
    }
    let chosen = best.filter(|b| b.fix != align::Fix::NONE && b.gains.score() > ALIGN_LEAST);
    Searched { candidates, chosen }
}

/// Set `fix` on `track` (guid): through its Signal instance's `align`
/// block when it has one (added if missing), else only its polarity
/// through the track's. What could not be set, in words.
fn set_fix(
    daw: &Standalone,
    project: &ProjectContext,
    list: &ActionList,
    (track, fix): (&str, &align::Fix),
) -> Result<Vec<String>, String> {
    let run = |id: &str, args: Value| -> Result<Value, String> {
        list.run_json(id, args.to_string().as_bytes())
            .map_err(|e| format!("{id}: {e}"))
            .map(|b| serde_json::from_slice(&b).unwrap_or(Value::Null))
    };
    let align_block = |chain: &Value| -> Option<Value> {
        chain["chain"]["phases"]
            .as_array()?
            .iter()
            .filter_map(|p| p["blocks"].as_array())
            .flatten()
            .find(|b| b["kind"] == "align")
            .cloned()
    };
    let chain = crate::clap_host::signal::chain(daw, project, track, None).ok();
    let mut block = chain.as_ref().and_then(align_block);
    if block.is_none()
        && let Some(c) = &chain
    {
        let phase = c["chain"]["phases"]
            .as_array()
            .and_then(|p| {
                p.iter()
                    .map(|p| p["id"].as_str())
                    .find(|id| *id != Some("source"))
                    .flatten()
            })
            .unwrap_or("rescue")
            .to_owned();
        let added = run(
            "SIGNAL_RUN",
            json!({ "track": track, "action": "SIGNAL_BLOCK_ADD", "args": { "phase": phase, "kind": "align" } }),
        );
        if added.is_ok() {
            block = crate::clap_host::signal::chain(daw, project, track, None)
                .ok()
                .as_ref()
                .and_then(align_block);
        }
    }
    let Some(block) = block else {
        // No align block: the polarity alone, through the track.
        let mut left = Vec::new();
        if fix.invert {
            let phase = Tracks::all(daw, project.clone())
                .iter()
                .find(|t| t.guid == track)
                .is_some_and(|t| t.phase_inverted);
            run("TRACK_SET", json!({ "track": track, "phase": !phase }))?;
        }
        if fix.delay != 0 {
            left.push(format!(
                "delay {} samples (no Signal align block on the track)",
                fix.delay
            ));
        }
        if let Some(r) = fix.rotate {
            left.push(format!(
                "rotation {:.0}° at {:.0} Hz (no Signal align block on the track)",
                r.deg, r.hz
            ));
        }
        return Ok(left);
    };
    // Each parameter by its CLAP id, normalized over its range.
    let value = |id: &str, v: f64| -> Option<(String, String)> {
        let p = block["params"].as_array()?.iter().find(|p| p["id"] == id)?;
        let clap = p["clap_id"].as_str()?;
        let (lo, hi) = (p["min"].as_f64()?, p["max"].as_f64()?);
        let n = if hi > lo {
            ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some((format!("@{clap}"), format!("={n:.6}")))
    };
    let r = fix.rotate.unwrap_or(align::Rotation {
        hz: 100.0,
        deg: 0.0,
        q: align::ROTATE_Q,
    });
    let set: serde_json::Map<String, Value> = [
        value("delay_samples", fix.delay as f64),
        value("invert", if fix.invert { 1.0 } else { 0.0 }),
        value("rotate_hz", r.hz),
        value("rotate_deg", r.deg),
        value("rotate_q", r.q),
    ]
    .into_iter()
    .flatten()
    .map(|(k, v)| (k, Value::String(v)))
    .collect();
    run(
        "FX_SET",
        json!({ "track": track, "fx": "Signal", "set": set }),
    )?;
    Ok(Vec::new())
}

/// `LISTEN_ALIGN`.
///
/// # Errors
/// No such piece or no hits, a render failed, or a fix would not be set
/// (the mix is put back then).
fn listen_align(
    daw: &Standalone,
    project: &ProjectContext,
    list: &ActionList,
    a: &AlignArg,
) -> Result<Value, String> {
    let span = tracing::info_span!(
        "LISTEN_ALIGN",
        listen.piece = %a.piece,
        listen.chosen = tracing::field::Empty,
        listen.applied = tracing::field::Empty,
        listen.kept = tracing::field::Empty,
        listen.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let hits_arg = HitsArg {
        piece: a.piece.clone(),
        from: a.from.clone(),
        to: a.to.clone(),
        at: None,
        sum: a.sum.clone(),
        reference: a.reference.clone(),
        window_ms: None,
        window_pre_ms: None,
        png: Some(false),
        fresh: None,
        exact: a.exact,
    };
    let hw = hit_windows(daw, project, &hits_arg)?;
    let w = align::Windows::new(RATE, hw.pre);
    let most = (a.max_ms.unwrap_or(3.0).clamp(0.0, 20.0) * f64::from(RATE) / 1e3) as i64;
    let how = (most, a.invert.unwrap_or(true), a.rotate.unwrap_or(true));
    let all = Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&all);
    let path = |g: &str| paths.get(g).cloned().unwrap_or_else(|| g.to_owned());
    // The most of the sum first; the reference stays.
    let mut order: Vec<usize> = (0..hw.tracks.mics.len())
        .filter(|i| hw.tracks.mics[*i] != hw.tracks.reference)
        .collect();
    let share = |i: usize| measure::contribution(&hw.mic_windows[i], &hw.sum_windows).1;
    order.sort_by(|x, y| share(*y).total_cmp(&share(*x)));
    let mut sum = hw.sum_windows.clone();
    let mut rows = Vec::new();
    let mut chosen: Vec<(String, align::Fix)> = Vec::new();
    let mut fixed_mics: Vec<(String, Vec<f32>, Vec<f32>)> = Vec::new();
    for i in order {
        let mic = &hw.mic_windows[i];
        let found = search(&w, mic, &sum, how);
        let (without, share) = measure::contribution(mic, &sum);
        if let Some(c) = found.chosen {
            let fixed = align::apply(&w, mic, &c.fix);
            sum = align::swap(&sum, mic, &fixed);
            chosen.push((hw.tracks.mics[i].clone(), c.fix));
            let avg = measure::average(&fixed);
            let env = measure::envelope(&avg, RATE, 5.0);
            let name = path(&hw.tracks.mics[i]);
            fixed_mics.push((
                format!("{} aligned", name.rsplit('/').next().unwrap_or(&name)),
                avg,
                env,
            ));
        }
        rows.push(json!({
            "path": path(&hw.tracks.mics[i]),
            "now": { "without_db": without, "share": share },
            "candidates": found.candidates.iter().map(fix_json).collect::<Vec<_>>(),
            "chosen": found.chosen.as_ref().map(fix_json),
        }));
    }
    let predicted = align::compare(&w, &hw.sum_windows, &sum);
    let gains_json = |g: &align::Gains| json!({ "level": r2(g.level), "low": r2(g.low), "attack": r2(g.attack), "score": r2(g.score()) });
    span.record("listen.chosen", chosen.len());
    let png = if a.png.unwrap_or(true) {
        let before = measure::average(&hw.sum_windows);
        let after = measure::average(&sum);
        let mut series = vec![
            (
                "sum now".to_owned(),
                before.clone(),
                measure::envelope(&before, RATE, 5.0),
            ),
            (
                "sum aligned".to_owned(),
                after.clone(),
                measure::envelope(&after, RATE, 5.0),
            ),
        ];
        series.extend(fixed_mics);
        let title = format!(
            "{} · {} hits · sum before and after alignment (predicted {:+.2} dB low, {:+.2} dB level)",
            a.piece,
            hw.times.len(),
            predicted.low,
            predicted.level
        );
        drawn(
            Ok(plot::hit(&series, RATE, hw.pre_ms, &title)),
            picture("align", &a.piece),
        )
    } else {
        Value::Null
    };
    let mut applied = Value::Null;
    if a.apply.unwrap_or(false) && !chosen.is_empty() {
        let before = crate::mix_snapshot::current(daw, project)?;
        let mut not_applied = Vec::new();
        let set: Result<(), String> = chosen.iter().try_for_each(|(track, fix)| {
            let left = set_fix(daw, project, list, (track, fix))?;
            not_applied.extend(left.into_iter().map(|l| format!("{}: {l}", path(track))));
            Ok(())
        });
        if let Err(why) = set {
            let _ = ab::put_back(daw, project, &before);
            return Err(format!("{why} (the mix is put back)"));
        }
        let again = hit_windows(daw, project, &hits_arg)?;
        let measured = align::compare(&w, &hw.sum_windows, &again.sum_windows);
        let keep = measured.score() > 0.0;
        if !keep {
            ab::put_back(daw, project, &before)?;
        }
        span.record("listen.applied", chosen.len());
        span.record("listen.kept", keep);
        applied = json!({
            "measured": gains_json(&measured),
            "kept": keep,
            "not_applied": not_applied,
        });
    }
    let ms = began.elapsed().as_secs_f64() * 1e3;
    span.record("listen.ms", ms);
    Ok(json!({
        "piece": a.piece,
        "hits": hw.times.len(),
        "reference": path(&hw.tracks.reference),
        "sum": path(&hw.tracks.sum),
        "mics_at": probe_word(hw.at),
        "mics": rows,
        "predicted": gains_json(&predicted),
        "apply": applied,
        "png": png,
        "rendered": rendered_json(&hw.heard, &|g: &str| path(g)),
        "ms": r2(ms),
    }))
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register_with(&ALIGN, Effect::Mutating, |_: &ActionCaller, a: AlignArg| {
        let (daw, project) = song()?;
        listen_align(daw, &project, crate::actions::list(), &a)
            .map(|v| RawJson::from(v.to_string()))
    })?;
    list.register_with(
        &RENDER,
        Effect::ReadOnly,
        |_: &ActionCaller, a: RenderArg| {
            let (daw, project) = song()?;
            listen_render(daw, &project, &a).map(|v| RawJson::from(v.to_string()))
        },
    )?;
    list.register_dynamic(
        &AB,
        Effect::Mutating,
        Some(AbDoc::SHAPE),
        None,
        std::sync::Arc::new(|raw: &[u8]| {
            let failed = |message: String| architect::action::ActionError::Failed {
                id: AB.id.to_owned(),
                message,
            };
            let a = ab_args(raw).map_err(|message| architect::action::ActionError::BadArgs {
                id: AB.id.to_owned(),
                message,
                expected: Some(architect::schema::json_schema(AbDoc::SHAPE)),
            })?;
            let (daw, project) = song().map_err(failed)?;
            listen_ab(daw, &project, crate::actions::list(), &a)
                .map(|v| v.to_string().into_bytes())
                .map_err(failed)
        }),
    )?;
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
                    exact: a.exact.unwrap_or(false),
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
