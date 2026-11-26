//! What a person or an agent does by hand on a song, as one sequence of
//! steps, each timed on the wall clock (as the caller waits) and from the
//! engine's own span for it:
//!
//! 1. **open**: the set, then the song (`SESSION_OPEN`, `SESSION_SONG`),
//!    as Studio's session open does; the engine's `song.open`.
//! 2. **play**: from the first chorus, to the first audible block
//!    (`transport.play`'s `play.first_audio_ms`), then stop.
//! 3. **listen**: `LISTEN` on the snare over the chorus, cold then warm;
//!    then the snare and its reference in one exact `LISTEN`, whose shapes
//!    are compared.
//! 4. **listen_ab**: a `LISTEN_AB` that changes one EQ band and puts it
//!    back (a fader's pan where the song has no Signal), then a `LISTEN`
//!    that must be answered from the cache: the mix is as it was.
//! 5. **render**: `MIX_RENDER` from `IN` to `OUT end` into a WAV.
//! 6. **the window's buttons**, called as the mixer's and arrangement's
//!    Render and Export buttons call them (with nothing given):
//!    `MIX_RENDER {}` (**render_button**: the whole song, beside it; only
//!    where the plan says how long the song is) and `MIX_EXPORT {}`
//!    (**export**: a REAPER project beside the song).
//!
//! The checks on results come back beside the timings.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::engine::Engine;
use crate::otlp::{self, Receiver, Span};
use crate::shape;

/// How long an open may take before the run gives up on it.
const OPEN_WAIT: Duration = Duration::from_secs(10 * 60);
/// How long a step's span may take to arrive after its call answered.
const SPAN_WAIT: Duration = Duration::from_secs(15);

/// What changes in the `LISTEN_AB`.
#[derive(Clone, Debug)]
pub enum Change {
    /// The first used band of the track's Signal `tone.eq`, 3 dB up.
    SignalEq,
    /// The track's pan, as a song without Signal can always do.
    Pan,
}

/// One song's sequence.
#[derive(Clone, Debug)]
pub struct Plan {
    /// `unbreakable` or `synthetic`: its section in the budgets.
    pub song: &'static str,
    pub setlist: PathBuf,
    /// The song's title in the set (`SESSION_SONG`).
    pub title: String,
    pub track: String,
    pub reference: Option<String>,
    /// The span listened to: section names.
    pub listen: (String, String),
    pub change: Change,
    /// The span rendered.
    pub render: (String, String),
    /// How long the whole song is, seconds, when the Render button's
    /// whole-song render is cheap enough to run (`None`: not run).
    pub whole_song_s: Option<f64>,
    /// Actions run after the open, before anything is timed.
    pub setup: Vec<(String, Value)>,
    /// Where the render goes.
    pub run_dir: PathBuf,
}

/// One timed step.
#[derive(Clone, Debug)]
pub struct Step {
    pub name: &'static str,
    pub wall_ms: f64,
    /// The engine's own time for it (its span), when it has one.
    pub engine_ms: Option<f64>,
    pub detail: Value,
    /// Its span's breakdown, by span name.
    pub spans: Vec<Value>,
}

impl Step {
    /// What its budget is held to: the engine's time, else the wall's.
    #[must_use]
    pub fn ms(&self) -> f64 {
        self.engine_ms.unwrap_or(self.wall_ms)
    }
}

/// One check on a result.
#[derive(Clone, Debug)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub value: Value,
    pub said: String,
}

/// A run: its steps, checks, the open's breakdown and what went wrong.
#[derive(Debug, Default)]
pub struct Run {
    pub steps: Vec<Step>,
    pub checks: Vec<Check>,
    pub open_breakdown: Vec<Value>,
    pub open_root: Option<Value>,
    /// Values the budgets file has no expectation for yet, to store.
    pub measured: Vec<(&'static str, f64)>,
    pub errors: Vec<String>,
}

impl Run {
    fn check(&mut self, name: &'static str, ok: bool, value: Value, said: impl Into<String>) {
        self.checks.push(Check {
            name,
            ok,
            value,
            said: said.into(),
        });
    }
}

/// Run `plan` on `engine`, its spans arriving at `spans`, against the
/// expectations in `expect` (check name → value).
pub fn run(
    engine: &Engine,
    spans: &Receiver,
    plan: &Plan,
    expect: &std::collections::BTreeMap<String, f64>,
) -> Run {
    let mut out = Run::default();
    if let Err(e) = steps(engine, spans, plan, expect, &mut out) {
        out.errors.push(e);
    }
    out
}

fn steps(
    engine: &Engine,
    spans: &Receiver,
    plan: &Plan,
    expect: &std::collections::BTreeMap<String, f64>,
    out: &mut Run,
) -> Result<(), String> {
    let mcp = &engine.mcp;
    // ── open ──
    let mark = spans.mark();
    let began = Instant::now();
    mcp.run(
        "SESSION_OPEN",
        &json!({"path": plan.setlist.display().to_string()}),
    )?;
    let songs = wait_open(engine)?;
    // Then the song, as a click on its tab: the set's song whose title
    // names it (`unbreakable.comped` is listed by its song's title).
    let song = pick(&songs, &plan.title);
    if song["current"] != json!(true) {
        mcp.run("SESSION_SONG", &json!({"to": song["number"].to_string()}))?;
    }
    let wall_ms = ms(began);
    let open = spans.wait_for(mark, SPAN_WAIT, |s| s.name == "song.open");
    // The open's last spans close after it answers; give them a moment.
    std::thread::sleep(Duration::from_millis(500));
    let all = spans.spans();
    if let Some(open) = &open {
        let root = otlp::root_of(&all, open);
        out.open_breakdown = otlp::breakdown(&all, root);
        out.open_root = Some(json!({"name": root.name, "ms": otlp::round(root.ms())}));
    }
    out.steps.push(Step {
        name: "open",
        wall_ms,
        engine_ms: open.as_ref().map(Span::ms),
        detail: json!({
            "song.ready_ms": open.as_ref().and_then(|s| s.number("song.ready_ms")),
            "song.tracks": open.as_ref().and_then(|s| s.number("song.tracks")),
        }),
        spans: Vec::new(),
    });
    if open.is_none() {
        out.errors.push("no song.open span arrived".to_owned());
    }

    let sections = Sections::read(mcp.run("SESSION_SECTIONS", &json!({}))?)?;
    for (id, args) in &plan.setup {
        mcp.run(id, args)?;
    }

    // ── play ──
    let from = sections.start(&plan.listen.0)?;
    let mark = spans.mark();
    let began = Instant::now();
    mcp.run("SESSION_PLAY_FROM", &json!({"at": from}))?;
    let called_ms = ms(began);
    let heard = spans.wait_for(mark, SPAN_WAIT, |s| s.name == "transport.play");
    let wall_ms = ms(began);
    std::thread::sleep(Duration::from_millis(1000));
    let transport = mcp.run("SESSION_TRANSPORT", &json!({}))?;
    mcp.run("SESSION_STOP", &json!({}))?;
    let first_audio = heard.as_ref().and_then(|s| s.number("play.first_audio_ms"));
    out.steps.push(Step {
        name: "first_audio",
        wall_ms,
        engine_ms: first_audio,
        detail: json!({
            "call_ms": called_ms,
            "heard": heard.as_ref().and_then(|s| s.attrs.get("play.heard").cloned()),
            "plugin_too_large": heard.as_ref().and_then(|s| s.number("play.plugin_too_large")),
            "plugin_busy": heard.as_ref().and_then(|s| s.number("play.plugin_busy")),
            "transport_after_1s": transport,
        }),
        spans: Vec::new(),
    });
    out.check(
        "play_heard",
        first_audio.is_some(),
        json!(first_audio),
        if first_audio.is_some() {
            "the first audible block came".to_owned()
        } else {
            format!("no audible block after play: {heard:?}")
        },
    );
    out.check(
        "play_moves",
        transport["playing"] == json!(true),
        transport.clone(),
        "the transport played through the null output",
    );

    // ── listen ──
    let span = json!({"from": plan.listen.0, "to": plan.listen.1, "png": false});
    let listen = merge(&span, &json!({"track": plan.track}));
    let (answer, step) = timed(engine, spans, "first_listen", "LISTEN", &listen)?;
    let mut answers = vec![answer.clone()];
    out.steps.push(step.with(json!({
        "lufs": answer["figures"]["lufs"],
        "rendered": answer["rendered"],
    })));
    let (answer, step) = timed(engine, spans, "warm_listen", "LISTEN", &listen)?;
    answers.push(answer.clone());
    out.steps
        .push(step.with(json!({"answered": answer["rendered"]["answered"]})));
    if let Some(reference) = &plan.reference {
        let points = merge(
            &span,
            &json!({"points": [{"track": plan.track}, {"track": reference}], "exact": true}),
        );
        let (answer, step) = timed(engine, spans, "listen_points", "LISTEN", &points)?;
        answers.push(answer.clone());
        out.steps.push(step);
        shape_check(&answer, expect, out);
    }

    // ── listen_ab ──
    let change = change_for(mcp, plan)?;
    let ab = merge(&listen, &json!({"change": change}));
    let (answer, step) = timed(engine, spans, "listen_ab", "LISTEN_AB", &ab)?;
    answers.push(answer.clone());
    let moved = answer["difference"]["lufs"].as_f64();
    out.steps.push(step.with(json!({
        "change": change,
        "difference_lufs": moved,
        "put_back": answer["put_back"],
        "kept": answer["kept"],
    })));
    out.check(
        "ab_changed",
        moved.is_some_and(|d| d.abs() > 1e-4),
        json!(moved),
        "the change was heard",
    );
    let again = mcp.run("LISTEN", &listen)?;
    let answered = again["rendered"]["answered"].clone();
    out.check(
        "ab_put_back",
        answered == json!("replayed"),
        answered,
        "after LISTEN_AB, the mix is as it was: the same LISTEN is answered from the cache",
    );

    // ── render ──
    let file = plan.run_dir.join("mix.wav");
    let render =
        json!({"from": plan.render.0, "to": plan.render.1, "to_file": file.display().to_string()});
    let (answer, step) = timed(engine, spans, "render", "MIX_RENDER", &render)?;
    render_checks(&answer, &file, &sections, plan, expect, out);
    answers.push(answer.clone());
    out.steps.push(step.with(json!({"answer": answer})));

    // ── the window's buttons ──
    if let Some(whole) = plan.whole_song_s {
        let (answer, step) = timed(engine, spans, "render_button", "MIX_RENDER", &json!({}))?;
        render_button_checks(&answer, whole, out);
        answers.push(answer.clone());
        out.steps.push(step.with(json!({"answer": answer})));
    }
    let (answer, step) = timed(engine, spans, "export", "MIX_EXPORT", &json!({}))?;
    export_checks(&answer, out);
    out.steps.push(step.with(json!({"answer": answer})));
    plugin_check(engine, &answers, out);
    Ok(())
}

/// The Render button's render (`MIX_RENDER {}`): the whole song, `whole`
/// seconds, in a WAV under `Renders/` beside the song. The file is
/// removed once read, so a copy of the song stays as it was.
fn render_button_checks(answer: &Value, whole: f64, out: &mut Run) {
    let file = PathBuf::from(answer["file"].as_str().unwrap_or_default());
    let beside = file
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|d| d == "Renders");
    out.check(
        "render_button_file",
        beside,
        json!(file.display().to_string()),
        "the Render button's WAV goes in Renders/ beside the song",
    );
    let wav = hound::WavReader::open(&file).map(|r| {
        let spec = r.spec();
        (
            f64::from(r.duration()) / f64::from(spec.sample_rate),
            spec.sample_rate,
            spec.bits_per_sample,
            spec.channels,
        )
    });
    let ok = wav.as_ref().is_ok_and(|(s, rate, bits, ch)| {
        (s - whole).abs() < 0.05 && *rate == 48_000 && *bits == 32 && *ch == 2
    });
    out.check(
        "render_button_length",
        ok,
        json!({"expected_s": whole, "file": format!("{wav:?}"), "answer": answer["seconds"]}),
        format!(
            "the Render button renders the whole song ({whole} s) as a 48 kHz 32-bit stereo WAV"
        ),
    );
    let lufs = answer["lufs"].as_f64();
    out.check(
        "render_button_heard",
        lufs.is_some_and(|l| l.is_finite() && l > -60.0),
        json!(lufs),
        "the Render button's WAV is the mix, not silence (integrated loudness above -60 LUFS)",
    );
    let _ = std::fs::remove_file(&file);
    if let Some(dir) = file.parent() {
        let _ = std::fs::remove_dir(dir);
    }
}

/// The Export button's project (`MIX_EXPORT {}`): a REAPER project beside
/// the song that a REAPER project reader parses (dawfile-reaper, as the
/// engine opens an RPP), every track of it in the file, never the song's
/// own file. Removed once read.
fn export_checks(answer: &Value, out: &mut Run) {
    let file = PathBuf::from(answer["file"].as_str().unwrap_or_default());
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let parsed = dawfile_reaper::parse_project_text(&text);
    let tracks = parsed.as_ref().map_or(0, |p| p.tracks.len());
    let said = answer["tracks"].as_u64().unwrap_or(0);
    out.check(
        "export_rpp",
        text.trim_start().starts_with("<REAPER_PROJECT")
            && parsed.is_ok()
            && tracks > 0
            && u64::try_from(tracks).is_ok_and(|t| t == said)
            && file.extension().is_some_and(|e| e == "RPP")
            && file
                .file_stem()
                .is_some_and(|s| s.to_string_lossy().ends_with("(Studio)")),
        json!({"file": file.display().to_string(), "tracks_in_file": tracks,
               "parse": parsed.as_ref().err().map(ToString::to_string), "answer": answer}),
        "the Export button writes a REAPER project of every track beside the song",
    );
    let _ = std::fs::remove_file(&file);
}

/// Until the set is open, every song of it: its `SESSION_SONGS` rows.
fn wait_open(engine: &Engine) -> Result<Vec<Value>, String> {
    let began = Instant::now();
    loop {
        let opening = engine.mcp.run("SESSION_OPENING", &json!({}))?;
        match opening["state"].as_str() {
            Some("failed") => return Err(format!("the open failed: {opening}")),
            Some("idle") => {
                let songs = engine.mcp.run("SESSION_SONGS", &json!({}))?;
                let rows = songs.as_array().cloned().unwrap_or_default();
                if !rows.is_empty() && rows.iter().all(|r| r["state"] == json!("open")) {
                    return Ok(rows);
                }
            }
            _ => {}
        }
        if began.elapsed() > OPEN_WAIT {
            return Err(format!(
                "not open after {OPEN_WAIT:?}: {opening}\n{}",
                engine.tail(30)
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The row of `songs` whose title holds `hint` (case aside; a file name's
/// dots read as spaces), else the first.
fn pick<'a>(songs: &'a [Value], hint: &str) -> &'a Value {
    let words = |s: &str| s.to_lowercase().replace(['.', '_', '-'], " ");
    let hint = words(hint);
    let first_word = hint
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    songs
        .iter()
        .find(|r| {
            r["title"]
                .as_str()
                .is_some_and(|t| words(t).contains(&hint))
        })
        .or_else(|| {
            songs.iter().find(|r| {
                r["title"]
                    .as_str()
                    .is_some_and(|t| words(t).contains(&first_word))
            })
        })
        .unwrap_or(&songs[0])
}

/// Run `id` with `args` as step `name`: its answer, and the step timed
/// on the wall and by its `action` span.
fn timed(
    engine: &Engine,
    spans: &Receiver,
    name: &'static str,
    id: &str,
    args: &Value,
) -> Result<(Value, Step), String> {
    let mark = spans.mark();
    let began = Instant::now();
    let answer = engine.mcp.run(id, args)?;
    let wall_ms = ms(began);
    let span = spans.wait_for(mark, SPAN_WAIT, |s| is_action(s, id));
    // Where its time went: its spans by name, the longest dozen.
    let breakdown = span.as_ref().map(|span| {
        let all = spans.spans();
        otlp::breakdown(&all, span)
            .into_iter()
            .take(12)
            .collect::<Vec<_>>()
    });
    Ok((
        answer,
        Step {
            name,
            wall_ms,
            engine_ms: span.as_ref().map(Span::ms),
            detail: Value::Null,
            spans: breakdown.unwrap_or_default(),
        },
    ))
}

/// Whether `span` is action `id`'s run: the `action` span with its id, or
/// a span named for it.
fn is_action(span: &Span, id: &str) -> bool {
    (span.name == "action" && span.text("action.id") == Some(id)) || span.name == id
}

impl Step {
    fn with(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }
}

/// The `change` for the `LISTEN_AB`: one call, as LISTEN_AB takes it.
fn change_for(mcp: &crate::mcp::Mcp, plan: &Plan) -> Result<Value, String> {
    match plan.change {
        Change::Pan => {
            Ok(json!([{"action": "TRACK_SET", "args": {"track": plan.track, "pan": 0.5}}]))
        }
        Change::SignalEq => {
            let chain = mcp.run("SIGNAL_CHAIN", &json!({"track": plan.track}))?;
            let (band, gain) = first_band(&chain)
                .ok_or_else(|| format!("no used band in {}'s tone.eq", plan.track))?;
            Ok(json!([{"action": "SIGNAL_BLOCK_SET", "args": {
                "track": plan.track,
                "block": "tone.eq",
                "params": {format!("band.{band}.gain"): gain + 3.0},
            }}]))
        }
    }
}

/// The first used band of `tone.eq` in a SIGNAL_CHAIN answer that has
/// gain (a bell or a shelf the mix moved off 0 dB; a low cut's gain does
/// nothing), and that gain.
fn first_band(chain: &Value) -> Option<(u64, f64)> {
    let blocks = chain["chain"]["phases"]
        .as_array()?
        .iter()
        .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default());
    let eq = blocks.into_iter().find(|b| b["id"] == json!("tone.eq"))?;
    let params = eq["params"].as_array()?;
    let value = |id: &str| {
        params
            .iter()
            .find(|p| p["id"] == json!(id))
            .map(|p| &p["value"])
    };
    (1..=24).find_map(|n| {
        let used = value(&format!("band.{n}.used"))?;
        let on = used
            .as_bool()
            .unwrap_or_else(|| used.as_f64().is_some_and(|v| v > 0.5));
        let gain = value(&format!("band.{n}.gain"))?.as_f64()?;
        (on && gain.abs() > 0.1).then_some((n, gain))
    })
}

/// The song's sections, by name.
pub struct Sections(Vec<(String, f64, f64)>);

impl Sections {
    fn read(answer: Value) -> Result<Self, String> {
        let rows = answer
            .as_array()
            .or_else(|| answer["sections"].as_array())
            .ok_or_else(|| format!("SESSION_SECTIONS: {answer}"))?;
        let number = |r: &Value, keys: &[&str]| keys.iter().find_map(|k| r[*k].as_f64());
        Ok(Self(
            rows.iter()
                .filter_map(|r| {
                    Some((
                        r["name"].as_str()?.to_owned(),
                        number(r, &["start", "start_s", "start_seconds"])?,
                        number(r, &["end", "end_s", "end_seconds"])?,
                    ))
                })
                .collect(),
        ))
    }

    fn find(&self, name: &str) -> Result<&(String, f64, f64), String> {
        self.0
            .iter()
            .find(|(n, _, _)| n == name)
            .ok_or_else(|| format!("no section {name}: {:?}", self.0))
    }

    /// Seconds where `at` is: a section's start, or `<section> end`.
    fn start(&self, at: &str) -> Result<f64, String> {
        match at.strip_suffix(" end") {
            Some(name) => self.find(name).map(|s| s.2),
            None => self.find(at).map(|s| s.1),
        }
    }
}

fn shape_check(answer: &Value, expect: &std::collections::BTreeMap<String, f64>, out: &mut Run) {
    let numbers = |v: &Value| -> Vec<f64> {
        v.as_array()
            .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect())
            .unwrap_or_default()
    };
    let hz = numbers(&answer["bands_hz"]);
    let points = answer["points"].as_array().cloned().unwrap_or_default();
    let (Some(ours), Some(theirs)) = (points.first(), points.get(1)) else {
        out.check(
            "snare_shape",
            false,
            answer.clone(),
            "LISTEN answered no two points",
        );
        return;
    };
    let distance = shape::distance(
        &hz,
        &numbers(&ours["figures"]["bands"]),
        &numbers(&theirs["figures"]["bands"]),
        150.0,
        8000.0,
    );
    let Some(distance) = distance else {
        out.check(
            "snare_shape",
            false,
            Value::Null,
            "no band to compare in 150 Hz–8 kHz",
        );
        return;
    };
    out.measured.push(("snare_shape_db", distance));
    match (
        expect.get("snare_shape_db"),
        expect.get("snare_shape_tolerance"),
    ) {
        (Some(&want), tolerance) => {
            let tolerance = tolerance.copied().unwrap_or(0.5);
            out.check(
                "snare_shape",
                distance <= want + tolerance,
                json!(distance),
                format!("mean |shape − reference| 150 Hz–8 kHz {distance:.2} dB, stored {want} + {tolerance}"),
            );
        }
        (None, _) => out.check(
            "snare_shape",
            true,
            json!(distance),
            format!("{distance:.2} dB: no stored value yet (snare_shape_db)"),
        ),
    }
}

fn render_checks(
    answer: &Value,
    file: &Path,
    sections: &Sections,
    plan: &Plan,
    expect: &std::collections::BTreeMap<String, f64>,
    out: &mut Run,
) {
    let want = sections
        .start(&plan.render.1)
        .and_then(|end| sections.start(&plan.render.0).map(|start| end - start));
    let wav = hound::WavReader::open(file).map(|r| {
        let spec = r.spec();
        f64::from(r.duration()) / f64::from(spec.sample_rate)
    });
    match (&want, &wav) {
        (Ok(want), Ok(got)) => out.check(
            "render_length",
            (want - got).abs() < 0.05,
            json!({"expected_s": want, "file_s": got, "answer": answer["seconds"]}),
            format!(
                "the WAV is {got:.3} s, {} to {} is {want:.3} s",
                plan.render.0, plan.render.1
            ),
        ),
        _ => out.check(
            "render_length",
            false,
            json!({"expected": format!("{want:?}"), "file": format!("{wav:?}")}),
            "the render's length could not be read",
        ),
    }
    let lufs = ["lufs", "integrated_lufs"]
        .iter()
        .find_map(|k| answer[*k].as_f64())
        .or_else(|| answer["figures"]["lufs"].as_f64());
    let Some(lufs) = lufs else {
        out.check(
            "render_lufs",
            false,
            answer.clone(),
            "MIX_RENDER said no loudness",
        );
        return;
    };
    out.measured.push(("render_lufs", lufs));
    match expect.get("render_lufs") {
        Some(&want) => {
            let tolerance = expect.get("render_lufs_tolerance").copied().unwrap_or(0.5);
            out.check(
                "render_lufs",
                (lufs - want).abs() <= tolerance,
                json!(lufs),
                format!("{lufs:.2} LUFS, stored {want} ± {tolerance}"),
            );
        }
        None => out.check(
            "render_lufs",
            true,
            json!(lufs),
            format!("{lufs:.2} LUFS: no stored value yet (render_lufs)"),
        ),
    }
}

/// No plugin failed: every render's `plugin_failures` is empty (a plugin
/// that would not copy or run), and the engine's log names none that
/// failed to load or was bypassed.
fn plugin_check(engine: &Engine, answers: &[Value], out: &mut Run) {
    let mut said = Vec::new();
    for answer in answers {
        failures_in(answer, &mut said);
    }
    out.check(
        "no_render_plugin_failures",
        said.is_empty(),
        json!(said),
        if said.is_empty() {
            "every render's plugin_failures is empty".to_owned()
        } else {
            format!("{} plugin failures in the renders", said.len())
        },
    );
    let log = std::fs::read_to_string(&engine.log).unwrap_or_default();
    let failures: Vec<&str> = log
        .lines()
        .filter(|l| l.contains(" ERROR ") || l.contains(" WARN "))
        .filter(|l| {
            let l = l.to_ascii_lowercase();
            (l.contains("plugin") || l.contains("clap") || l.contains(" fx"))
                && (l.contains("fail")
                    || l.contains("could not")
                    || l.contains("would not")
                    || l.contains("bypassed"))
        })
        .take(20)
        .collect();
    out.check(
        "no_logged_plugin_failures",
        failures.is_empty(),
        json!(failures),
        if failures.is_empty() {
            "no plugin failed".to_owned()
        } else {
            format!("{} plugin failures in the engine's log", failures.len())
        },
    );
}

/// Every entry of every `plugin_failures` list in `answer`.
fn failures_in(answer: &Value, out: &mut Vec<Value>) {
    match answer {
        Value::Object(map) => {
            for (k, v) in map {
                match (k.as_str(), v) {
                    ("plugin_failures", Value::Array(list)) => out.extend(list.iter().cloned()),
                    _ => failures_in(v, out),
                }
            }
        }
        Value::Array(list) => list.iter().for_each(|v| failures_in(v, out)),
        _ => {}
    }
}

fn merge(a: &Value, b: &Value) -> Value {
    let mut out = a.clone();
    if let (Some(out), Some(b)) = (out.as_object_mut(), b.as_object()) {
        for (k, v) in b {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1e3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_used_band_is_found_in_a_chain() {
        let chain = json!({"chain": {"phases": [
            {"id": "rescue", "blocks": [{"id": "rescue.gate", "params": []}]},
            {"id": "tone", "blocks": [{"id": "tone.eq", "params": [
                {"id": "band.1.used", "value": false},
                {"id": "band.1.gain", "value": 0.0},
                {"id": "band.2.used", "value": 1.0},
                {"id": "band.2.gain", "value": 0.0},
                {"id": "band.3.used", "value": 1.0},
                {"id": "band.3.gain", "value": -6.0},
            ]}]},
        ]}});
        // Band 2 is used but flat (a low cut's gain): band 3 is the one heard.
        assert_eq!(first_band(&chain), Some((3, -6.0)));
        assert_eq!(first_band(&json!({})), None);
    }

    #[test]
    fn the_song_is_picked_by_its_title() {
        let songs = [
            json!({"number": 1, "title": "Intro"}),
            json!({"number": 2, "title": "Unbreakable - Crescendum"}),
        ];
        assert_eq!(pick(&songs, "unbreakable.comped")["number"], 2);
        assert_eq!(pick(&songs, "synth")["number"], 1);
    }

    #[test]
    fn plugin_failures_are_found_wherever_an_answer_says_them() {
        let mut said = Vec::new();
        failures_in(
            &json!({"a_rendered": {"plugin_failures": []}, "b_rendered": {"plugin_failures": ["Signal Dev: would not copy"]}}),
            &mut said,
        );
        assert_eq!(said, [json!("Signal Dev: would not copy")]);
    }

    #[test]
    fn a_section_end_is_its_end() {
        let s = Sections::read(json!([
            {"name": "IN", "start": 0.0, "end": 2.0},
            {"name": "CH 1", "start": 2.0, "end": 6.0},
        ]))
        .unwrap();
        assert_eq!(s.start("CH 1").unwrap(), 2.0);
        assert_eq!(s.start("CH 1 end").unwrap(), 6.0);
        assert!(s.start("OUT").is_err());
    }
}
