//! The mix, measured and heard as it is processed: a span of the song
//! rendered offline through its whole graph — every FX (the hosted CLAPs
//! too), sends, folders, faders, delay compensation — and measured per
//! track and bus (`MIX_MEASURE`), or written to a WAV to listen to
//! (`MIX_RENDER`).
//!
//! An offline render runs copies of the song's plugins, each carrying its
//! live instance's state (daw-standalone's `render::offline`), so it never
//! pushes a frame through what playback is playing; its solos and its
//! bypasses (`bypass_signal`) are its own. It starts a second before
//! `from` (compressors and reverbs settle), primes every plugin so the
//! song's latency is known at the first block, and reads each track's tap
//! and master that many samples late — each is then the span exactly.

pub(crate) mod analysis;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ProjectContext, Tracks};
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::audio_engine::render::offline::{Copying, Offline, Tap, copy_plugin, hosted};
use daw_standalone::plugin::{PluginInstance, Plugins};
use daw_standalone::sync::Standalone;
use facet::Facet;

pub use analysis::{Bands, FLOOR, Stats, measure as analyse};

use crate::actions::{meta, resolve, song};

/// The rate a span is rendered at.
pub const RATE: u32 = 48_000;
/// Frames a block.
const BLOCK: usize = 1_024;
/// How long before `from` the render starts, seconds: what a compressor's
/// release and a reverb's tail need to be playing as they would.
const PREROLL: f64 = 1.0;
/// The longest span measured, seconds.
const MOST: f64 = 600.0;

static MEASURE: ActionMeta = meta(
    "MIX_MEASURE",
    "mix_measure",
    "Measure the mix",
    "Render `from`..`to` (bars `33`, `33:2`; times `12.5s`; sections `Chorus 1`, `Chorus 1 end`) offline through the whole mix — every FX including the hosted CLAPs (Signal), sends, folders, faders, plugin delay compensated — without touching playback or the song, and measure each of `tracks` (paths, names or GUIDs; default: the Drums folder, every track and folder in it) and the master: `lufs` (integrated), `short_max` (loudest 3 s), `true_peak` (dBTP), `peak`, `rms` (dBFS; a full-scale sine reads -3), `crest` (dB), `bands` (dB per band, the band centres in `bands_hz`: octaves 31.5 Hz–16 kHz, or thirds with `bands: third`), `correlation` and `width` (side/mid) and `balance` (L/R dB), and for a track in a folder its `share_db` of its parent's sum (K-weighted). Taken after the fader (`post_fader: false`: after the FX, before the fader). `bypass_signal: true` also measures the same span with every Signal instance bypassed, giving each track its `raw` numbers and `signal_gain_db` (processed minus raw loudness: the gain-neutral check). Says each track's latency (samples), the song's, and the time taken.",
    "Mix",
    "",
);

static RENDER: ActionMeta = meta(
    "MIX_RENDER",
    "mix_render",
    "Render the mix to a file",
    "Render `from`..`to` (as MIX_MEASURE) offline through the whole mix — what MIX_MEASURE measures — to the WAV `to_file` (48 kHz, 32-bit float, stereo): the master, or with `tracks` (paths, names or GUIDs) a solo of those (as the solo buttons would: a folder brings its children, each heard through its folders). `bypass_signal: true` renders it with every Signal instance bypassed (the raw drums). Touches neither playback nor the song. Says the file, its length, its loudness and peak, and the time taken.",
    "Mix",
    "",
);

/// What to measure.
#[derive(Facet)]
struct MeasureArg {
    tracks: Option<Vec<String>>,
    from: String,
    to: String,
    post_fader: Option<bool>,
    bypass_signal: Option<bool>,
    bands: Option<String>,
}

/// What to render.
#[derive(Facet)]
struct RenderArg {
    from: String,
    to: String,
    tracks: Option<Vec<String>>,
    solo: Option<Vec<String>>,
    to_file: String,
    bypass_signal: Option<bool>,
}

/// One signal's numbers, as MIX_MEASURE says them.
#[derive(Facet, Clone, Debug)]
pub struct Numbers {
    pub lufs: f64,
    pub short_max: f64,
    pub true_peak: f64,
    pub peak: f64,
    pub rms: f64,
    pub crest: f64,
    pub bands: Vec<f64>,
    pub correlation: f64,
    pub width: f64,
    pub balance: f64,
}

impl From<&Stats> for Numbers {
    fn from(s: &Stats) -> Self {
        Self {
            lufs: s.lufs,
            short_max: s.short_max,
            true_peak: s.true_peak,
            peak: s.peak,
            rms: s.rms,
            crest: s.crest,
            bands: s.bands.clone(),
            correlation: s.correlation,
            width: s.width,
            balance: s.balance,
        }
    }
}

/// One track (or bus, or the master), measured.
#[derive(Facet, Clone, Debug)]
pub struct TrackNumbers {
    pub path: String,
    /// `track`, `folder` or `master`.
    pub kind: String,
    /// Its tap's latency, samples (what reaches it is that late, put right).
    pub latency: u32,
    pub numbers: Numbers,
    /// Its share of its parent folder's sum, dB, when the parent is
    /// measured too.
    pub share_db: Option<f64>,
    pub parent: Option<String>,
    /// With `bypass_signal`: the same, every Signal instance bypassed.
    pub raw: Option<Numbers>,
    /// With `bypass_signal`: processed minus raw integrated loudness, LU.
    pub signal_gain_db: Option<f64>,
}

/// The mix, measured.
#[derive(Facet)]
pub struct MeasureRow {
    pub from: f64,
    pub to: f64,
    pub post_fader: bool,
    pub bands_hz: Vec<f64>,
    pub tracks: Vec<TrackNumbers>,
    /// The song's plugin delay, samples.
    pub latency: u32,
    /// Plugins copied for the render, and those that would not be (rendered
    /// dry), each with why.
    pub plugins: u32,
    pub plugin_failures: Vec<String>,
    /// Signal instances bypassed for the raw pass.
    pub signal_instances: u32,
    /// Of the processed pass: copying the plugins (each made again, its
    /// state loaded), priming them, rendering the span's blocks, ms.
    pub copy_ms: f64,
    pub prime_ms: f64,
    pub blocks_ms: f64,
    pub render_ms: f64,
    pub analysis_ms: f64,
    pub ms: f64,
}

/// The mix, rendered to a file.
#[derive(Facet)]
pub struct RenderRow {
    pub file: String,
    pub seconds: f64,
    pub lufs: f64,
    pub true_peak: f64,
    pub latency: u32,
    pub plugin_failures: Vec<String>,
    pub ms: f64,
}

/// A span rendered: master and the tracks asked for, each exactly
/// `from`..`to`, interleaved stereo.
pub struct Rendered {
    pub master: Vec<f32>,
    /// By track GUID.
    pub tracks: HashMap<String, Vec<f32>>,
    /// Each tapped track's latency, samples, by GUID.
    pub latencies: HashMap<String, u32>,
    pub latency: u32,
    pub copied: u32,
    pub failures: Vec<String>,
    /// Copying the plugins, priming them, and rendering the blocks, ms.
    pub copy_ms: f64,
    pub prime_ms: f64,
    pub blocks_ms: f64,
    pub ms: f64,
}

/// Copies dropped off the action's thread: tearing a kit's instances down
/// takes seconds, and nothing waits for it.
fn drop_later(copies: Vec<Box<dyn PluginInstance>>) {
    if copies.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("mix-copies-drop".into())
        .spawn(move || drop(copies));
    if spawned.is_err() {
        tracing::warn!("mix: the copies are dropped here, a thread would not start");
    }
}

/// How a span is rendered.
pub struct Span<'a> {
    pub from: f64,
    pub to: f64,
    /// Tracks to tap, by GUID.
    pub tap: &'a HashSet<String>,
    pub post_fader: bool,
    pub bypass_fx: HashSet<String>,
    pub solo: Option<HashSet<String>>,
}

/// A plugin made the way an instance's bundle would now load: from the
/// rebuilt copy when the bundle was reloaded (FX_RELOAD), else `None` (the
/// engine's own way).
pub(crate) fn make_fresh(name: &str) -> Option<Box<dyn daw_standalone::plugin::PluginInstance>> {
    let path = Path::new(name);
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("clap"))
    {
        return None;
    }
    let copy = crate::clap_fx::loadable(path)?;
    daw_standalone::plugin::load_plugin(&copy.to_string_lossy())
        .ok()
        .flatten()
}

/// Render `span` of song `project` offline.
///
/// # Errors
/// The span is empty or too long.
pub fn render_span(daw: &Standalone, project: &str, span: &Span<'_>) -> Result<Rendered, String> {
    let began = web_time::Instant::now();
    if !(span.to > span.from) {
        return Err(format!(
            "`to` ({:.3}s) is not after `from` ({:.3}s)",
            span.to, span.from
        ));
    }
    if span.to - span.from > MOST {
        return Err(format!(
            "{:.0} s is longer than the {MOST:.0} s measured at once",
            span.to - span.from
        ));
    }
    // Signal's copies in a session of their own: never the live kit's.
    static RENDERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tag = format!(
        "mix-{}",
        RENDERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let restate = |name: &str, state: Vec<u8>| {
        if crate::signal_kit::is_signal_bundle(name) {
            crate::signal_kit::private_session(state, &tag)
        } else {
            state
        }
    };
    let how = Copying {
        make: Some(&make_fresh),
        restate: Some(&restate),
    };
    // Fresh copies every render: a plugin's `reset` is not trusted to
    // clear everything (Signal's leaves its reverbs' tails), and a copy
    // carrying the last render's tail would measure it.
    let plugins = std::sync::Arc::new(Plugins::default());
    let mut failures: Vec<String> = Vec::new();
    let mut copied = 0u32;
    let mut fx_guids: Vec<String> = Vec::new();
    for (guid, name) in hosted(daw, project) {
        let state = daw.save_plugin_state(&guid).unwrap_or_default();
        match copy_plugin(daw, &name, state, how) {
            Ok((copy, _)) => {
                plugins.write().insert(guid.clone(), copy);
                fx_guids.push(guid);
                copied += 1;
            }
            Err(why) => failures.push(format!("{guid}: {why}")),
        }
    }
    let copy_ms = began.elapsed().as_secs_f64() * 1e3;
    let renderer = ProjectRenderer::offline(
        daw,
        project,
        RATE,
        Offline {
            plugins: plugins.clone(),
            bypass_fx: span.bypass_fx.clone(),
            solo: span.solo.clone(),
            tap: Some(if span.post_fader {
                Tap::PostFader
            } else {
                Tap::PreFader
            }),
            ..Offline::default()
        },
    );
    let primed = web_time::Instant::now();
    let latency = renderer.prime(u32::try_from(BLOCK).unwrap_or(1_024));
    let prime_ms = primed.elapsed().as_secs_f64() * 1e3;
    let blocks = web_time::Instant::now();
    let guids = renderer.track_guids();
    let track_latency = renderer.track_latencies();
    // Which tap index each wanted track is, and its latency.
    let wanted: Vec<(usize, String, usize)> = guids
        .iter()
        .enumerate()
        .filter(|(_, g)| span.tap.contains(*g))
        .map(|(i, g)| {
            (
                i,
                g.clone(),
                track_latency.get(i).copied().unwrap_or(0) as usize,
            )
        })
        .collect();
    let frame = |s: f64| (s * f64::from(RATE)).round().max(0.0) as usize;
    let (from, to) = (frame(span.from), frame(span.to));
    let start = from.saturating_sub(frame(PREROLL));
    let most_late = wanted
        .iter()
        .map(|w| w.2)
        .max()
        .unwrap_or(0)
        .max(latency as usize);
    let end = to + most_late;
    let len = (to - from) * 2;
    let mut master = vec![0.0f32; len];
    let mut tracks: HashMap<String, Vec<f32>> = wanted
        .iter()
        .map(|w| (w.1.clone(), vec![0.0f32; len]))
        .collect();
    // Frame `at` of a render, `late` samples late, is the timeline's
    // `at - late`: copied where that falls inside `from..to`.
    let place = |dst: &mut [f32], src: &[f32], at: usize, late: usize| {
        let frames = src.len() / 2;
        for f in 0..frames {
            let Some(t) = (at + f).checked_sub(late) else {
                continue;
            };
            if t < from || t >= to {
                continue;
            }
            let i = (t - from) * 2;
            dst[i] = src[f * 2];
            dst[i + 1] = src[f * 2 + 1];
        }
    };
    let mut at = start;
    while at < end {
        let n = BLOCK.min(end - at);
        let block = renderer.render_block(at as u64, n);
        place(&mut master, &block.samples, at, latency as usize);
        renderer.with_taps(|taps| {
            for (ti, guid, late) in &wanted {
                if let (Some(tap), Some(dst)) = (taps.get(*ti), tracks.get_mut(guid)) {
                    place(dst, &tap[..n * 2], at, *late);
                }
            }
        });
        at += n;
    }
    let blocks_ms = blocks.elapsed().as_secs_f64() * 1e3;
    // Torn down off this thread: it takes seconds for a kit.
    drop(renderer);
    drop_later(
        fx_guids
            .iter()
            .filter_map(|g| plugins.write().remove(g))
            .collect(),
    );
    Ok(Rendered {
        master,
        tracks,
        latencies: wanted.iter().map(|w| (w.1.clone(), w.2 as u32)).collect(),
        latency,
        copied,
        failures,
        copy_ms,
        prime_ms,
        blocks_ms,
        ms: began.elapsed().as_secs_f64() * 1e3,
    })
}

/// Split interleaved stereo.
fn channels(s: &[f32]) -> (Vec<f32>, Vec<f32>) {
    s.chunks_exact(2).map(|f| (f[0], f[1])).unzip()
}

/// The Drums folder and everything in it (every track when the song has
/// no Drums folder), by GUID.
fn drums(tracks: &[daw_proto::Track]) -> Vec<String> {
    let top = tracks
        .iter()
        .find(|t| t.parent_guid.is_none() && t.name.eq_ignore_ascii_case("drums"));
    let Some(top) = top else {
        return tracks.iter().map(|t| t.guid.clone()).collect();
    };
    within(tracks, &top.guid)
}

/// `root` and every track inside it, in track order.
fn within(tracks: &[daw_proto::Track], root: &str) -> Vec<String> {
    let parent: HashMap<&str, Option<&str>> = tracks
        .iter()
        .map(|t| (t.guid.as_str(), t.parent_guid.as_deref()))
        .collect();
    let inside = |guid: &str| {
        let mut at = Some(guid);
        while let Some(g) = at {
            if g == root {
                return true;
            }
            at = parent.get(g).copied().flatten();
        }
        false
    };
    tracks
        .iter()
        .filter(|t| inside(&t.guid))
        .map(|t| t.guid.clone())
        .collect()
}

/// The span `from`..`to` names, seconds.
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

/// The band set `word` names.
fn bands_of(word: Option<&str>) -> Result<Bands, String> {
    match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
        None | Some("octave" | "octaves" | "10") => Ok(Bands::Octave),
        Some("third" | "thirds" | "third-octave" | "1/3" | "31") => Ok(Bands::Third),
        Some(other) => Err(format!("`bands` is octave or third, not {other:?}")),
    }
}

/// `MIX_MEASURE`.
///
/// # Errors
/// A bad span or track, or the render failed.
fn measure(
    daw: &Standalone,
    project: &ProjectContext,
    a: &MeasureArg,
) -> Result<MeasureRow, String> {
    let span = tracing::info_span!(
        "MIX_MEASURE",
        mix.tracks = tracing::field::Empty,
        mix.seconds = tracing::field::Empty,
        mix.latency = tracing::field::Empty,
        mix.plugins = tracing::field::Empty,
        mix.plugin_failures = tracing::field::Empty,
        mix.signal = tracing::field::Empty,
        mix.render_ms = tracing::field::Empty,
        mix.analysis_ms = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
        mix.bypass_signal = a.bypass_signal.unwrap_or(false),
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let guid = crate::fx_actions::project_guid(project)?;
    let (from, to) = span_of(daw, project, &a.from, &a.to)?;
    let bands = bands_of(a.bands.as_deref())?;
    let post_fader = a.post_fader.unwrap_or(true);
    let all = Tracks::all(daw, project.clone());
    let paths = crate::mix_sends::paths(&all);
    let asked: Vec<String> = match &a.tracks {
        Some(names) if !names.is_empty() => names
            .iter()
            .map(|n| resolve(daw, project, n))
            .collect::<Result<_, _>>()?,
        _ => drums(&all),
    };
    let tap: HashSet<String> = asked.iter().cloned().collect();
    let signal: Vec<String> = if a.bypass_signal.unwrap_or(false) {
        crate::signal_kit::instances(daw, &guid)
    } else {
        Vec::new()
    };
    let pass = |bypass_fx: HashSet<String>| {
        render_span(
            daw,
            &guid,
            &Span {
                from,
                to,
                tap: &tap,
                post_fader,
                bypass_fx,
                solo: None,
            },
        )
    };
    // The raw pass beside the processed one: copies of their own (each in a
    // Signal session of its own), so the two share nothing.
    let rendering = web_time::Instant::now();
    let (processed, raw) = if a.bypass_signal.unwrap_or(false) {
        let bypass: HashSet<String> = signal.iter().cloned().collect();
        std::thread::scope(|s| {
            let raw = s.spawn(|| pass(bypass));
            let processed = pass(HashSet::new());
            let raw = raw.join().map_err(|_| "the raw pass panicked".to_owned());
            (processed, raw)
        })
    } else {
        (pass(HashSet::new()), Ok(Err(String::new())))
    };
    let processed = processed?;
    let raw = match raw? {
        Ok(r) => Some(r),
        Err(why) if why.is_empty() => None,
        Err(why) => return Err(why),
    };
    let render_ms = rendering.elapsed().as_secs_f64() * 1e3;
    let analysed = web_time::Instant::now();
    let stats = |s: &[f32]| {
        let (l, r) = channels(s);
        analysis::measure(&l, &r, RATE, bands)
    };
    let by_guid: HashMap<&str, &daw_proto::Track> =
        all.iter().map(|t| (t.guid.as_str(), t)).collect();
    // Each track's numbers on a core of its own.
    use rayon::prelude::*;
    let measured: Vec<(String, Stats, Option<Stats>)> = asked
        .par_iter()
        .filter_map(|g| {
            let mine = processed.tracks.get(g)?;
            let theirs = raw.as_ref().and_then(|r| r.tracks.get(g));
            Some((g.clone(), stats(mine), theirs.map(|s| stats(s))))
        })
        .collect();
    let power: HashMap<&str, f64> = measured
        .iter()
        .map(|(g, s, _)| (g.as_str(), s.k_power))
        .collect();
    let mut rows: Vec<TrackNumbers> = measured
        .iter()
        .map(|(g, s, r)| {
            let t = by_guid.get(g.as_str());
            let parent = t.and_then(|t| t.parent_guid.clone());
            let share_db = parent
                .as_deref()
                .and_then(|p| power.get(p))
                .map(|whole| analysis::share_db(s.k_power, *whole));
            TrackNumbers {
                path: paths.get(g).cloned().unwrap_or_else(|| g.clone()),
                kind: if t.is_some_and(|t| t.is_folder) {
                    "folder"
                } else {
                    "track"
                }
                .to_owned(),
                latency: processed.latencies.get(g).copied().unwrap_or(0),
                numbers: Numbers::from(s),
                share_db: share_db.filter(|_| parent.is_some()),
                parent: parent.and_then(|p| paths.get(&p).cloned()),
                raw: r.as_ref().map(Numbers::from),
                signal_gain_db: r
                    .as_ref()
                    .map(|r| ((s.lufs - r.lufs) * 100.0).round() / 100.0),
            }
        })
        .collect();
    let (master, master_raw) = rayon::join(
        || stats(&processed.master),
        || raw.as_ref().map(|r| stats(&r.master)),
    );
    rows.push(TrackNumbers {
        path: "MASTER".into(),
        kind: "master".into(),
        latency: processed.latency,
        numbers: Numbers::from(&master),
        share_db: None,
        parent: None,
        signal_gain_db: master_raw
            .as_ref()
            .map(|r| ((master.lufs - r.lufs) * 100.0).round() / 100.0),
        raw: master_raw.as_ref().map(Numbers::from),
    });
    let analysis_ms = analysed.elapsed().as_secs_f64() * 1e3;
    let out = MeasureRow {
        from,
        to,
        post_fader,
        bands_hz: bands
            .centres()
            .iter()
            .map(|c| (c * 10.0).round() / 10.0)
            .collect(),
        latency: processed.latency,
        plugins: processed.copied,
        plugin_failures: processed.failures,
        signal_instances: u32::try_from(signal.len()).unwrap_or(u32::MAX),
        copy_ms: processed.copy_ms,
        prime_ms: processed.prime_ms,
        blocks_ms: processed.blocks_ms,
        tracks: rows,
        render_ms,
        analysis_ms,
        ms: began.elapsed().as_secs_f64() * 1e3,
    };
    span.record("mix.tracks", out.tracks.len());
    span.record("mix.seconds", to - from);
    span.record("mix.latency", out.latency);
    span.record("mix.plugins", out.plugins);
    span.record("mix.plugin_failures", out.plugin_failures.len());
    span.record("mix.signal", out.signal_instances);
    span.record("mix.render_ms", out.render_ms);
    span.record("mix.analysis_ms", out.analysis_ms);
    span.record("mix.ms", out.ms);
    if !out.plugin_failures.is_empty() {
        tracing::warn!(
            mix.plugin_failures = out.plugin_failures.len(),
            "MIX_MEASURE: plugins rendered dry (they would not be copied)"
        );
    }
    Ok(out)
}

/// `MIX_RENDER`.
///
/// # Errors
/// A bad span, track or file, or the render failed.
fn render(daw: &Standalone, project: &ProjectContext, a: &RenderArg) -> Result<RenderRow, String> {
    let span = tracing::info_span!(
        "MIX_RENDER",
        mix.seconds = tracing::field::Empty,
        mix.solo = tracing::field::Empty,
        mix.latency = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
        mix.bypass_signal = a.bypass_signal.unwrap_or(false),
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let guid = crate::fx_actions::project_guid(project)?;
    let (from, to) = span_of(daw, project, &a.from, &a.to)?;
    let names: Vec<&String> = a.tracks.iter().chain(a.solo.iter()).flatten().collect();
    let solo: Option<HashSet<String>> = if names.is_empty() {
        None
    } else {
        Some(
            names
                .iter()
                .map(|n| resolve(daw, project, n))
                .collect::<Result<_, _>>()?,
        )
    };
    span.record("mix.solo", solo.as_ref().map_or(0, HashSet::len));
    let bypass_fx: HashSet<String> = if a.bypass_signal.unwrap_or(false) {
        crate::signal_kit::instances(daw, &guid)
            .into_iter()
            .collect()
    } else {
        HashSet::new()
    };
    let rendered = render_span(
        daw,
        &guid,
        &Span {
            from,
            to,
            tap: &HashSet::new(),
            post_fader: true,
            bypass_fx,
            solo,
        },
    )?;
    let file = PathBuf::from(&a.to_file);
    if let Some(dir) = file.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let (l, r) = channels(&rendered.master);
    let stats = analysis::measure(&l, &r, RATE, Bands::Octave);
    fts_sample::write_wav_f32(&file, RATE, &[l, r])
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let out = RenderRow {
        file: file.display().to_string(),
        seconds: to - from,
        lufs: stats.lufs,
        true_peak: stats.true_peak,
        latency: rendered.latency,
        plugin_failures: rendered.failures,
        ms: began.elapsed().as_secs_f64() * 1e3,
    };
    span.record("mix.seconds", out.seconds);
    span.record("mix.latency", out.latency);
    span.record("mix.ms", out.ms);
    Ok(out)
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&MEASURE, Effect::ReadOnly, |a: MeasureArg| {
        let (daw, project) = song()?;
        measure(daw, &project, &a)
    })?;
    list.register(&RENDER, Effect::ReadOnly, |a: RenderArg| {
        let (daw, project) = song()?;
        render(daw, &project, &a)
    })?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
