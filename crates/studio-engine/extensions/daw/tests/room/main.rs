//! A room with a leader and a member, each its own engine, read only
//! through their MCP doors: Seam 2's base (studio#689, studio#709).
//!
//! ```sh
//! just test-room     # or: just test engine -E 'binary(room)'
//! ```
//!
//! Both engines are `daw-extension --engine`, sandboxed as the fixture
//! test sandboxes one, sharing one Studio folder: its Rooms folder (where
//! the room is kept and its leader is found) and the fixture sessions and
//! two setlists of them. The leader makes the room, leads it and loads the
//! first setlist; the member joins by the room's link once and is checked
//! to follow the leader's setlist, song and playhead; then the leader
//! loads the second setlist and the member is checked to move to it on
//! the same connection. Its checks go to
//! `target/<triple>/<profile>/room/room.json`.

#[path = "../fixtures/check.rs"]
#[allow(dead_code, reason = "shared with tests/fixtures, which uses all of it")]
mod check;
#[path = "../unbreakable/engine.rs"]
#[allow(
    dead_code,
    reason = "shared with tests/unbreakable, which uses all of it"
)]
mod engine;
#[path = "../fixtures/fixture.rs"]
#[allow(dead_code, reason = "shared with tests/fixtures, which uses all of it")]
mod fixture;
#[path = "../unbreakable/mcp.rs"]
mod mcp;
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
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use check::Checks;
use engine::Engine;
use mcp::Mcp;

/// The two setlists the leader loads, one after the other, by their
/// fixture sessions.
const FIRST: &[&str] = &["multitrack-click", "markers-only"];
const SECOND: &[&str] = &["chart-repeats", "multitrack-click"];

/// How long a member has to follow what the leader did.
const FOLLOW_WAIT: Duration = Duration::from_secs(20);

/// How far apart two playheads read one after the other may be, seconds:
/// the follower's own tolerance, the two MCP reads between them, and
/// the transport's poll.
const PLAYHEAD_TOLERANCE: f64 = 0.35;

fn engine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daw-extension"))
}

/// The shared Studio folder: the fixture sessions staged in it, a
/// setlist of each set of them in its Setlists folder.
fn stage(studio: &Path) -> Result<(PathBuf, PathBuf), String> {
    let mut staged: Vec<&str> = Vec::new();
    for name in FIRST.iter().chain(SECOND) {
        if !staged.contains(name) {
            fixture::Fixture::load(name)?.stage(studio)?;
            staged.push(name);
        }
    }
    let setlists = studio.join("Setlists");
    std::fs::create_dir_all(&setlists).map_err(|e| e.to_string())?;
    let write = |file: &str, names: &[&str]| -> Result<PathBuf, String> {
        let path = setlists.join(file);
        let lines: String = names.iter().map(|n| format!("../sessions/{n}\n")).collect();
        std::fs::write(&path, lines).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    };
    Ok((
        write("first.setlist", FIRST)?,
        write("second.setlist", SECOND)?,
    ))
}

/// Wait for `ok` on what `read` says, up to `wait`; the last reading
/// either way.
fn until(
    wait: Duration,
    mut read: impl FnMut() -> Result<Value, String>,
    ok: impl Fn(&Value) -> bool,
) -> (bool, Value) {
    let began = Instant::now();
    loop {
        let got = read().unwrap_or_else(|e| json!({ "error": e }));
        if ok(&got) {
            return (true, got);
        }
        if began.elapsed() > wait {
            return (false, got);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The songs an engine has open, by title, once its open has settled.
fn open_titles(mcp: &Mcp) -> Result<Value, String> {
    let opening = mcp.run("SESSION_OPENING", &json!({}))?;
    if opening["state"] != json!("idle") {
        return Ok(json!({ "opening": opening }));
    }
    let rows = mcp.run("SESSION_SONGS", &json!({}))?;
    let rows = rows.as_array().cloned().unwrap_or_default();
    if rows.is_empty() || rows.iter().any(|r| r["state"] != json!("open")) {
        return Ok(json!({ "songs": rows }));
    }
    Ok(Value::Array(
        rows.iter().map(|r| r["title"].clone()).collect(),
    ))
}

/// The current song's number.
fn current(mcp: &Mcp) -> Result<Value, String> {
    let rows = mcp.run("SESSION_SONGS", &json!({}))?;
    Ok(rows
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["current"] == json!(true)))
        .map_or(Value::Null, |r| r["number"].clone()))
}

/// Run `id` and note it; an error fails the check `name`.
fn run(mcp: &Mcp, checks: &mut Checks, name: &str, id: &str, args: &Value) -> Option<Value> {
    match mcp.run(id, args) {
        Ok(v) => Some(v),
        Err(e) => {
            checks.failed(name, format!("{id}: {e}"));
            None
        }
    }
}

fn room(leader: &Engine, member: &Engine, first: &Path, second: &Path, checks: &mut Checks) {
    let (l, m) = (&leader.mcp, &member.mcp);

    // The organization's room, made once: one link.
    let Some(made) = run(
        l,
        checks,
        "made",
        "ROOM_CREATE",
        &json!({"name": "Adult Jam", "org": "rockstars"}),
    ) else {
        return;
    };
    let link = made["link"].as_str().unwrap_or_default().to_owned();
    checks.add(
        "made",
        link.starts_with("https://fasttrackstudio.app/join/adult-jam-")
            && made["org"] == json!("rockstars"),
        format!("{made}"),
    );

    // The leader leads it and loads the first setlist.
    if run(
        l,
        checks,
        "leads",
        "ROOM_LEAD",
        &json!({"room": link, "name": "Cody"}),
    )
    .is_none()
    {
        return;
    }
    if run(
        l,
        checks,
        "loads",
        "ROOM_LOAD",
        &json!({"path": first.display().to_string()}),
    )
    .is_none()
    {
        return;
    }
    let (ok, leader_first) = until(
        FOLLOW_WAIT,
        || open_titles(l),
        |v| v.as_array().is_some_and(|a| a.len() == FIRST.len()),
    );
    checks.add(
        "leader opens the first setlist",
        ok,
        format!("{leader_first}"),
    );
    if !ok {
        return;
    }
    let listed = l
        .run("ROOMS", &json!({"org": "rockstars"}))
        .unwrap_or_default();
    checks.add(
        "the room is live",
        listed[0]["link"] == json!(link) && listed[0]["live"]["leader"] == json!("Cody"),
        format!("{listed}"),
    );

    // The member joins once, by the link alone.
    if run(
        m,
        checks,
        "joins",
        "ROOM_JOIN",
        &json!({"link": link, "name": "Alex"}),
    )
    .is_none()
    {
        return;
    }
    let (ok, got) = until(FOLLOW_WAIT, || open_titles(m), |v| *v == leader_first);
    checks.add(
        "member follows the setlist",
        ok,
        format!("leader {leader_first}, member {got}"),
    );
    let (ok, got) = until(
        FOLLOW_WAIT,
        || l.run("ROOM", &json!({})),
        |v| v["people"] == json!(["Alex", "Cody"]),
    );
    checks.add(
        "the leader sees the member",
        ok,
        format!("{}", got["people"]),
    );

    // The leader changes song: the member does too.
    run(l, checks, "song 2", "SESSION_SONG", &json!({"to": "2"}));
    let (ok, got) = until(FOLLOW_WAIT, || current(m), |v| *v == json!(2));
    checks.add(
        "member follows the song",
        ok,
        format!("member on song {got}"),
    );

    // The leader plays: the member plays at the leader's playhead.
    if let Err(e) = check::play_from(l, 1.0) {
        checks.failed("member follows the playhead", e);
        return;
    }
    let (playing, _) = until(
        FOLLOW_WAIT,
        || m.run("SESSION_TRANSPORT", &json!({})),
        |v| v["playing"] == json!(true),
    );
    std::thread::sleep(Duration::from_secs(1));
    let (lt, mt) = (
        l.run("SESSION_TRANSPORT", &json!({})).unwrap_or_default(),
        m.run("SESSION_TRANSPORT", &json!({})).unwrap_or_default(),
    );
    let apart = (lt["position"].as_f64().unwrap_or(f64::NAN)
        - mt["position"].as_f64().unwrap_or(f64::NAN))
    .abs();
    checks.add(
        "member follows the playhead",
        playing && mt["playing"] == json!(true) && apart < PLAYHEAD_TOLERANCE,
        format!("leader {lt}, member {mt}: {apart:.3} s apart"),
    );
    if let Err(e) = check::stop(l) {
        checks.failed("member stops with the leader", e);
    }
    let (ok, got) = until(
        FOLLOW_WAIT,
        || m.run("SESSION_TRANSPORT", &json!({})),
        |v| v["playing"] == json!(false),
    );
    checks.add("member stops with the leader", ok, format!("{got}"));

    // Mid-rehearsal, another setlist: the member moves, on the same
    // connection.
    let before = m.run("ROOM", &json!({})).unwrap_or_default();
    if run(
        l,
        checks,
        "loads again",
        "ROOM_LOAD",
        &json!({"path": second.display().to_string()}),
    )
    .is_none()
    {
        return;
    }
    let (ok, leader_second) = until(
        FOLLOW_WAIT,
        || open_titles(l),
        |v| v.as_array().is_some_and(|a| a.len() == SECOND.len()) && *v != leader_first,
    );
    checks.add(
        "leader opens the second setlist",
        ok,
        format!("{leader_second}"),
    );
    let (ok, got) = until(FOLLOW_WAIT, || open_titles(m), |v| *v == leader_second);
    checks.add(
        "member moves to the second setlist",
        ok,
        format!("leader {leader_second}, member {got}"),
    );
    let after = m.run("ROOM", &json!({})).unwrap_or_default();
    checks.add(
        "without reconnecting",
        before["attaches"] == json!(1)
            && after["attaches"] == json!(1)
            && after["load"] == json!(2),
        format!(
            "attaches {} then {}, load {}",
            before["attaches"], after["attaches"], after["load"]
        ),
    );
}

/// The leader and a member in one room.
#[test]
fn a_member_follows_the_leader_through_two_setlists() {
    let studio = report::profile_dir()
        .join("room")
        .join("runs")
        .join("studio");
    let _ = std::fs::remove_dir_all(&studio);
    std::fs::create_dir_all(&studio).unwrap_or_else(|e| panic!("{}: {e}", studio.display()));
    let (first, second) = stage(&studio).unwrap_or_else(|e| panic!("staging: {e}"));
    let rooms = studio.join("Rooms");
    let env = [("FTS_ROOMS_DIR", rooms.as_os_str())];
    let spans = otlp::Receiver::start();
    let start = |who: &str| {
        let instance = format!("room-{who}-{}", std::process::id());
        Engine::start_with(&engine_bin(), &spans.endpoint, &instance, &env)
            .unwrap_or_else(|e| panic!("the {who}'s engine: {e}"))
    };
    let (mut leader, mut member) = (start("leader"), start("member"));
    let mut checks = Checks::default();
    room(&leader, &member, &first, &second, &mut checks);

    let tails = format!(
        "--- the leader's last lines ---\n{}\n--- the member's last lines ---\n{}",
        leader.tail(40),
        member.tail(40)
    );
    leader.stop();
    member.stop();
    let record = json!({
        "test": "room",
        "commit": report::commit(),
        "ok": checks.ok(),
        "checks": checks.json(),
    });
    let dir = report::profile_dir().join("room");
    let _ = std::fs::write(dir.join("room.json"), format!("{record:#}\n"));
    let mut out = format!(
        "\n=== room: {} ===\n",
        if checks.ok() { "pass" } else { "FAIL" }
    );
    for c in &checks.0 {
        out.push_str(&format!(
            "{} {:<36} {}\n",
            if c.ok { "ok  " } else { "FAIL" },
            c.name,
            c.said
        ));
    }
    eprintln!("{out}");
    assert!(checks.ok(), "the room failed:\n{out}\n{tails}");
}
