//! What is checked on a fixture, over the engine's MCP door only: each
//! check a name, whether it held, and what was seen.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::engine::Engine;
use crate::fixture::{ChartPoint, Fixture};
use crate::mcp::Mcp;

/// How long an open may take.
const OPEN_WAIT: Duration = Duration::from_secs(120);
/// How long the transport runs before it is read.
const PLAY_FOR: Duration = Duration::from_millis(800);

/// One check.
#[derive(Debug)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub said: String,
}

/// The checks of one run, in order.
#[derive(Debug, Default)]
pub struct Checks(pub Vec<Check>);

impl Checks {
    pub fn add(&mut self, name: impl Into<String>, ok: bool, said: impl Into<String>) {
        self.0.push(Check {
            name: name.into(),
            ok,
            said: said.into(),
        });
    }

    /// A step that could not run at all is a failed check.
    pub fn failed(&mut self, name: impl Into<String>, why: impl Into<String>) {
        self.add(name, false, why);
    }

    #[must_use]
    pub fn ok(&self) -> bool {
        self.0.iter().all(|c| c.ok)
    }

    #[must_use]
    pub fn json(&self) -> Value {
        Value::Array(
            self.0
                .iter()
                .map(|c| json!({"name": c.name, "ok": c.ok, "said": c.said}))
                .collect(),
        )
    }
}

/// Open `path` (a song or a setlist) and wait until every song of it is
/// open: its `SESSION_SONGS` rows.
///
/// # Errors
///
/// The open failed or did not finish in time.
pub fn open(engine: &Engine, path: &Path) -> Result<Vec<Value>, String> {
    let mcp = &engine.mcp;
    mcp.run("SESSION_OPEN", &json!({"path": path.display().to_string()}))?;
    let began = Instant::now();
    loop {
        let opening = mcp.run("SESSION_OPENING", &json!({}))?;
        match opening["state"].as_str() {
            Some("failed") => return Err(format!("the open failed: {opening}")),
            Some("idle") => {
                let rows = mcp
                    .run("SESSION_SONGS", &json!({}))?
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if !rows.is_empty() && rows.iter().all(|r| r["state"] == json!("open")) {
                    return Ok(rows);
                }
            }
            _ => {}
        }
        if began.elapsed() > OPEN_WAIT {
            return Err(format!("not open after {OPEN_WAIT:?}: {opening}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Everything a fixture session is checked for, once it is open: its
/// click, that it plays, each `[[listen]]`, and each `[[chart]]` point
/// both asked of the chart and followed while playing.
pub fn session(engine: &Engine, fixture: &Fixture, out: &mut Checks) {
    let mcp = &engine.mcp;
    sections(mcp, "sections", &fixture.sections, out);
    click(mcp, out);
    match plays(mcp, 0.0) {
        Ok(said) => out.add("plays", true, said),
        Err(why) => out.failed("plays", why),
    }
    for l in &fixture.listen {
        let name = format!("heard {}", l.track);
        match lufs(mcp, &l.track, &l.from, &l.to) {
            Ok(Some(v)) => out.add(
                name,
                (l.lufs_min..=l.lufs_max).contains(&v),
                format!(
                    "{v:.1} LUFS over {}..{} (wanted {}..{})",
                    l.from, l.to, l.lufs_min, l.lufs_max
                ),
            ),
            Ok(None) => out.failed(name, "LISTEN gave no loudness: silence"),
            Err(why) => out.failed(name, why),
        }
    }
    for point in &fixture.chart {
        let name = format!("chart at {}s", point.at);
        match mcp.run("SESSION_CHART_AT", &json!({"at": point.at})) {
            Ok(answer) => {
                let wrong = mismatches(point, &answer);
                out.add(name, wrong.is_empty(), said_of(&answer, &wrong));
            }
            Err(why) => out.failed(name, why),
        }
    }
    for point in &fixture.chart {
        let name = format!("chart follows {}s", point.at);
        match follows(mcp, point) {
            Ok((ok, said)) => out.add(name, ok, said),
            Err(why) => out.failed(name, why),
        }
    }
}

/// The song's click: a track named for it, heard over the first bars.
fn click(mcp: &Mcp, out: &mut Checks) {
    let tracks = match mcp.run("SESSION_TRACKS", &json!({})) {
        Ok(t) => t,
        Err(why) => return out.failed("click", why),
    };
    let found = tracks.as_array().into_iter().flatten().find(|t| {
        t["name"]
            .as_str()
            .is_some_and(|n| n.to_lowercase().contains("click"))
    });
    let Some(track) = found else {
        let names: Vec<&str> = tracks
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["name"].as_str())
            .collect();
        return out.failed("click", format!("no click track among {names:?}"));
    };
    let guid = track["guid"].as_str().unwrap_or_default();
    let name = track["name"].as_str().unwrap_or_default();
    match lufs(mcp, guid, "0s", "4s") {
        Ok(Some(v)) if v > -50.0 => out.add("click", true, format!("{name}: {v:.1} LUFS")),
        Ok(v) => out.failed("click", format!("{name} is silent: {v:?} LUFS")),
        Err(why) => out.failed("click", why),
    }
}

/// Play from `from` seconds for a moment: the transport must run.
///
/// # Errors
///
/// It did not play, or the door failed.
pub fn plays(mcp: &Mcp, from: f64) -> Result<String, String> {
    play_from(mcp, from)?;
    std::thread::sleep(PLAY_FOR);
    let transport = mcp.run("SESSION_TRANSPORT", &json!({}));
    stop(mcp)?;
    let transport = transport?;
    let position = transport["position"].as_f64().unwrap_or(f64::NAN);
    if transport["playing"] == json!(true) && position > from + 0.1 {
        Ok(format!("from {from} s to {position:.2} s"))
    } else {
        Err(format!("the transport did not run on: {transport}"))
    }
}

/// How long the transport has to start or stop when asked.
const TRANSPORT_WAIT: Duration = Duration::from_secs(3);

/// Play from `at` once the transport is stopped, and wait until it says it
/// plays from there: a play asked while the last stop is still landing
/// can be lost, and what is checked is the song playing, not that race.
///
/// # Errors
///
/// It never played from `at`, or the door failed.
pub fn play_from(mcp: &Mcp, at: f64) -> Result<(), String> {
    stop(mcp)?;
    mcp.run("SESSION_PLAY_FROM", &json!({"at": at}))?;
    let began = Instant::now();
    loop {
        let t = mcp.run("SESSION_TRANSPORT", &json!({}))?;
        let position = t["position"].as_f64().unwrap_or(f64::NAN);
        if t["playing"] == json!(true) && position >= at - 0.01 {
            return Ok(());
        }
        if began.elapsed() > TRANSPORT_WAIT {
            return Err(format!("asked to play from {at} s, it did not: {t}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Stop a transport that is playing, and wait until it says it has. One
/// that is not playing is left alone: a stop is queued, and one asked of
/// a stopped transport can land after the next play and stop that.
///
/// # Errors
///
/// It did not stop, or the door failed.
pub fn stop(mcp: &Mcp) -> Result<(), String> {
    if mcp.run("SESSION_TRANSPORT", &json!({}))?["playing"] != json!(true) {
        return Ok(());
    }
    mcp.run("SESSION_STOP", &json!({}))?;
    let began = Instant::now();
    loop {
        let t = mcp.run("SESSION_TRANSPORT", &json!({}))?;
        if t["playing"] != json!(true) {
            return Ok(());
        }
        if began.elapsed() > TRANSPORT_WAIT {
            return Err(format!("asked to stop, it did not: {t}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `track`'s integrated loudness over `from`..`to`, `None` when silent.
fn lufs(mcp: &Mcp, track: &str, from: &str, to: &str) -> Result<Option<f64>, String> {
    let answer = mcp.run(
        "LISTEN",
        &json!({"track": track, "from": from, "to": to, "png": false}),
    )?;
    Ok(answer["figures"]["lufs"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > -120.0))
}

/// Play from the point, then ask where the chart is at the playhead: the
/// bar the point expects while the playhead is still inside it.
fn follows(mcp: &Mcp, point: &ChartPoint) -> Result<(bool, String), String> {
    let bar_end = mcp.run("SESSION_CHART_AT", &json!({"at": point.at}))?["end"].as_f64();
    play_from(mcp, point.at)?;
    std::thread::sleep(Duration::from_millis(300));
    let answer = mcp.run("SESSION_CHART_AT", &json!({}));
    stop(mcp)?;
    let answer = answer?;
    let at = answer["at"].as_f64().unwrap_or(f64::NAN);
    if at <= point.at {
        return Ok((false, format!("the playhead did not move: {answer}")));
    }
    if bar_end.is_some_and(|end| at >= end) {
        return Ok((
            false,
            format!("the playhead left the bar before it was read: {answer}"),
        ));
    }
    let wrong = mismatches(point, &answer);
    Ok((wrong.is_empty(), said_of(&answer, &wrong)))
}

/// The point's expected fields `answer` does not have.
fn mismatches(point: &ChartPoint, answer: &Value) -> Vec<String> {
    point
        .expect
        .iter()
        .filter(|(key, want)| &answer[key.as_str()] != want)
        .map(|(key, want)| format!("{key} {} (wanted {want})", answer[key.as_str()]))
        .collect()
}

fn said_of(answer: &Value, wrong: &[String]) -> String {
    let brief = format!(
        "at {} s: bar {} (written {}, pass {}) in {}",
        answer["at"], answer["bar"], answer["written"], answer["pass"], answer["section"]
    );
    if wrong.is_empty() {
        brief
    } else {
        format!("{brief}: {}", wrong.join(", "))
    }
}

/// The open song's section names, in order.
///
/// # Errors
///
/// The door failed.
pub fn section_names(mcp: &Mcp) -> Result<Vec<String>, String> {
    Ok(mcp
        .run("SESSION_SECTIONS", &json!({}))?
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["name"].as_str().map(str::to_owned))
        .collect())
}

/// Whether the open song's sections are `want`, in order (anything when
/// `want` is empty).
pub fn sections(mcp: &Mcp, name: impl Into<String>, want: &[String], out: &mut Checks) {
    if want.is_empty() {
        return;
    }
    match section_names(mcp) {
        Ok(have) => out.add(
            name,
            have == want,
            format!(
                "{have:?}{}",
                if have == want {
                    String::new()
                } else {
                    format!(" (wanted {want:?})")
                }
            ),
        ),
        Err(why) => out.failed(name, why),
    }
}

/// Make the fixture's other arrangements of `song` (open in the engine):
/// each a branch of its project file, forked by SESSION_CHECKPOINT, its
/// SECTION_INSERT run in it and saved, its sections checked; then the
/// song's own opened again, unchanged. Returns the song's project file.
///
/// # Errors
///
/// A step could not run.
pub fn arrangements(
    engine: &Engine,
    fixture: &Fixture,
    song: &Path,
    out: &mut Checks,
) -> Result<std::path::PathBuf, String> {
    let mcp = &engine.mcp;
    let file = song.with_extension("session");
    for arrangement in &fixture.arrangements {
        mcp.run("SESSION_CHECKPOINT", &json!({"name": arrangement.name}))?;
        open(engine, &file.join(&arrangement.name))?;
        mcp.run("SECTION_INSERT", &arrangement.insert)?;
        mcp.run("SESSION_SAVE", &json!({}))?;
        sections(
            mcp,
            format!("arrangement {}", arrangement.name),
            &arrangement.sections,
            out,
        );
        // Back to the song's own, for the next fork and the checks after.
        open(engine, &file)?;
        sections(mcp, "the song's own, after", &fixture.sections, out);
    }
    Ok(file)
}
