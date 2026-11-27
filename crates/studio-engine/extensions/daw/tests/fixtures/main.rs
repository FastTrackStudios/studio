//! The fixture sessions and setlist, opened and played over MCP: Seam 1's
//! base (studio#689, studio#691). Each session in `sessions/` is opened in
//! a private engine of its own, played with its click, its chart asked
//! where it is and followed while it plays, and its tracks listened to;
//! `setlists/fixtures.setlist` is opened and moved through, by hand and by
//! rolling on in Live.
//!
//! ```sh
//! just test-fixtures     # or: just test engine -E 'binary(fixtures)'
//! ```
//!
//! The engine is `daw-extension --engine`, sandboxed as the Unbreakable
//! test sandboxes it (`../unbreakable/engine.rs`) and driven only through
//! its MCP door. Each session is its own test, so nextest says pass or
//! fail per session; each writes its checks to
//! `target/<triple>/<profile>/fixtures/<name>.json`.
//!
//! **Adding a fixture:** a folder in `sessions/` with a song file (an
//! `.RPP` or a `.session`), a `.kf` chart beside it if it has one, and a
//! `fixture.toml` saying what recordings to generate and what to check
//! (`fixture.rs`); then its line in `sessions!` below. Recordings are
//! generated, never committed.

mod check;
#[path = "../unbreakable/engine.rs"]
#[allow(
    dead_code,
    reason = "shared with tests/unbreakable, which uses all of it"
)]
mod engine;
mod fixture;
#[path = "../unbreakable/mcp.rs"]
mod mcp;
mod new_session;
#[path = "../unbreakable/otlp.rs"]
#[allow(
    dead_code,
    reason = "shared with tests/unbreakable, which uses all of it"
)]
mod otlp;
#[path = "../unbreakable/report.rs"]
#[allow(
    dead_code,
    reason = "shared with tests/unbreakable, which uses all of it"
)]
mod report;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use check::Checks;
use engine::Engine;
use fixture::Fixture;

fn engine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daw-extension"))
}

/// A clean folder for `name`'s run.
fn run_dir(name: &str) -> PathBuf {
    let dir = report::profile_dir()
        .join("fixtures")
        .join("runs")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    dir
}

/// A private engine, its spans to a receiver of the test's own.
fn engine(name: &str) -> (Engine, otlp::Receiver) {
    let spans = otlp::Receiver::start();
    let instance = format!("fixture-{name}-{}", std::process::id());
    let engine = Engine::start(&engine_bin(), &spans.endpoint, &instance)
        .unwrap_or_else(|e| panic!("the engine: {e}"));
    (engine, spans)
}

/// Write `name`'s record, say its checks, and fail if one failed.
fn finish(name: &str, about: &str, mut engine: Engine, checks: &Checks) {
    let tail = engine.tail(60);
    engine.stop();
    let record = json!({
        "fixture": name,
        "about": about,
        "commit": report::commit(),
        "ok": checks.ok(),
        "checks": checks.json(),
    });
    let dir = report::profile_dir().join("fixtures");
    let file = dir.join(format!("{name}.json"));
    let written =
        std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, format!("{record:#}\n")));
    let mut out = format!(
        "\n=== {name}: {} — {about} ===\n",
        if checks.ok() { "pass" } else { "FAIL" }
    );
    for c in &checks.0 {
        out.push_str(&format!(
            "{} {:<24} {}\n",
            if c.ok { "ok  " } else { "FAIL" },
            c.name,
            c.said
        ));
    }
    match written {
        Ok(()) => out.push_str(&format!("record {}", file.display())),
        Err(e) => out.push_str(&format!("the record could not be written: {e}")),
    }
    eprintln!("{out}");
    assert!(
        checks.ok(),
        "{name} failed:\n  {}\n--- the engine's last lines ---\n{tail}",
        checks
            .0
            .iter()
            .filter(|c| !c.ok)
            .map(|c| format!("{}: {}", c.name, c.said))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// Open fixture `name` alone and check it.
fn play_session(name: &str) {
    let fixture = Fixture::load(name).unwrap_or_else(|e| panic!("{e}"));
    let song = fixture
        .stage(&run_dir(name))
        .unwrap_or_else(|e| panic!("staging {name}: {e}"));
    let (engine, _spans) = engine(name);
    let mut checks = Checks::default();
    match check::open(&engine, &song) {
        Ok(songs) => {
            checks.add("opens", true, format!("{} open", songs[0]["title"]));
            check::session(&engine, &fixture, &mut checks);
            if let Err(why) = check::arrangements(&engine, &fixture, &song, &mut checks) {
                checks.failed("arrangements", why);
            }
        }
        Err(why) => checks.failed("opens", why),
    }
    finish(name, &fixture.about, engine, &checks);
}

/// One test per fixture session: nextest's pass or fail is the
/// session's.
macro_rules! sessions {
    ($($test:ident => $name:literal),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                play_session($name);
            }
        )*
        const SESSIONS: &[&str] = &[$($name),*];
    };
}

sessions! {
    arrangements => "arrangements",
    chart_repeats => "chart-repeats",
    markers_only => "markers-only",
    multitrack_click => "multitrack-click",
    same_name => "same-name",
}

#[test]
fn every_fixture_session_has_its_test() {
    let mut listed: Vec<&str> = SESSIONS.to_vec();
    listed.sort_unstable();
    assert_eq!(
        fixture::all(),
        listed,
        "a folder in tests/fixtures/sessions needs its line in `sessions!`"
    );
}

/// The fixture setlist, opened and moved through: by `SESSION_SONG`, then
/// by rolling on at a song's end in Live. A place may name one of its
/// song's arrangements (`…/song.session/<name>`): that place plays that
/// arrangement.
#[test]
fn setlist_moves_from_session_to_session() {
    let run = run_dir("setlist");
    let setlist = Path::new("setlists/fixtures.setlist");
    let places = setlist_places(&fixture::root().join(setlist));
    let (engine, _spans) = engine("setlist");
    let mut checks = Checks::default();
    if let Err(why) = stage_setlist(&engine, &run, setlist, &places, &mut checks)
        .and_then(|staged| walk_setlist(&engine, &staged, &places, &mut checks))
    {
        checks.failed("setlist", why);
    }
    finish(
        "setlist",
        "a setlist moves from session to session",
        engine,
        &checks,
    );
}

/// One place of a setlist: the fixture session it names, and the
/// arrangement when it names one.
struct Place {
    fixture: String,
    arrangement: Option<String>,
}

/// The places a fixture setlist names: `../sessions/<fixture>`, or
/// `../sessions/<fixture>/<song>.session/<arrangement>`.
fn setlist_places(file: &Path) -> Vec<Place> {
    std::fs::read_to_string(file)
        .unwrap_or_else(|e| panic!("{}: {e}", file.display()))
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let parts: Vec<&str> = line.split('/').collect();
            let at = parts
                .iter()
                .position(|p| *p == "sessions")
                .unwrap_or_else(|| panic!("{line:?}: not under ../sessions/"));
            let fixture = parts
                .get(at + 1)
                .unwrap_or_else(|| panic!("{line:?}: no session"));
            let arrangement = parts
                .iter()
                .position(|p| p.ends_with(".session"))
                .and_then(|i| parts.get(i + 1))
                .map(|a| (*a).to_owned());
            Place {
                fixture: (*fixture).to_owned(),
                arrangement,
            }
        })
        .collect()
}

/// Stage every session the setlist names, make the arrangements it
/// places, then the setlist beside them. Returns the staged setlist.
fn stage_setlist(
    engine: &Engine,
    run: &Path,
    setlist: &Path,
    places: &[Place],
    checks: &mut Checks,
) -> Result<PathBuf, String> {
    let mut staged: Vec<&str> = Vec::new();
    for place in places {
        if staged.contains(&place.fixture.as_str()) {
            continue;
        }
        staged.push(&place.fixture);
        let fixture = Fixture::load(&place.fixture)?;
        let song = fixture.stage(run)?;
        if !fixture.arrangements.is_empty() {
            check::open(engine, &song)?;
            check::arrangements(engine, &fixture, &song, checks)?;
        }
    }
    let file = run.join(setlist);
    std::fs::create_dir_all(run.join("setlists")).map_err(|e| e.to_string())?;
    std::fs::copy(fixture::root().join(setlist), &file).map_err(|e| e.to_string())?;
    Ok(file)
}

/// The sections place `place` must have, when its fixture says.
fn sections_of(place: &Place) -> Result<Vec<String>, String> {
    let fixture = Fixture::load(&place.fixture)?;
    Ok(match &place.arrangement {
        None => fixture.sections,
        Some(name) => {
            fixture
                .arrangements
                .into_iter()
                .find(|a| &a.name == name)
                .ok_or_else(|| format!("{} has no arrangement {name:?}", place.fixture))?
                .sections
        }
    })
}

fn walk_setlist(
    engine: &Engine,
    setlist: &Path,
    places: &[Place],
    checks: &mut Checks,
) -> Result<(), String> {
    let mcp = &engine.mcp;
    let count = places.len();
    let songs = check::open(engine, setlist)?;
    checks.add(
        "opens",
        songs.len() == count,
        format!("{} of {count} songs: {}", songs.len(), titles(&songs)),
    );
    for (to, place) in (1..=count).zip(places) {
        if to > 1 {
            mcp.run("SESSION_SONG", &json!({"to": "next"}))?;
        }
        let now = current(mcp)?;
        let transport = mcp.run("SESSION_TRANSPORT", &json!({}))?;
        checks.add(
            format!("song {to}"),
            now["number"] == json!(to) && transport["song"] == now["project"],
            format!(
                "current {} ({}), transport on {}",
                now["number"], now["title"], transport["song"]
            ),
        );
        let name = match &place.arrangement {
            Some(a) => format!("song {to} is {} {a}", place.fixture),
            None => format!("song {to} is {}", place.fixture),
        };
        check::sections(mcp, name, &sections_of(place)?, checks);
        match check::plays(mcp, 0.0) {
            Ok(said) => checks.add(format!("song {to} plays"), true, said),
            Err(why) => checks.failed(format!("song {to} plays"), why),
        }
    }
    // Live: the first song played into its end rolls on to the second.
    mcp.run("SESSION_SONG", &json!({"to": "1"}))?;
    mcp.run("SESSION_LIVE", &json!({"on": true}))?;
    let sections = mcp.run("SESSION_SECTIONS", &json!({}))?;
    let end = sections
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["end"].as_f64())
        .reduce(f64::max)
        .ok_or("song 1 has no sections")?;
    let from = (end - 1.0).max(0.0);
    check::play_from(mcp, from)?;
    let began = std::time::Instant::now();
    let mut now = current(mcp)?;
    while now["number"] != json!(2) && began.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
        now = current(mcp)?;
    }
    let transport = mcp.run("SESSION_TRANSPORT", &json!({}))?;
    check::stop(mcp)?;
    checks.add(
        "rolls on",
        now["number"] == json!(2) && transport["playing"] == json!(true),
        format!(
            "played from {from:.1} s of song 1 ({end:.1} s long): now song {} ({}), {}",
            now["number"],
            now["title"],
            if transport["playing"] == json!(true) {
                "playing"
            } else {
                "stopped"
            }
        ),
    );
    Ok(())
}

/// The current song's `SESSION_SONGS` row.
fn current(mcp: &mcp::Mcp) -> Result<Value, String> {
    mcp.run("SESSION_SONGS", &json!({}))?
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["current"] == json!(true)).cloned())
        .ok_or_else(|| "no current song".to_owned())
}

fn titles(songs: &[Value]) -> String {
    songs
        .iter()
        .filter_map(|s| s["title"].as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
