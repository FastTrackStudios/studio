//! A room with a leader and a member, each its own engine, read only
//! through their MCP doors: Seam 2's base (studio#689, studio#709).
//!
//! ```sh
//! just test-room     # or: just test engine -E 'binary(room)'
//! ```
//!
//! Both engines are `daw-extension --engine`, sandboxed as the fixture
//! test sandboxes one. Each has a Studio folder of its own (its own copy
//! of the fixture sessions and two setlists of them, as each device has
//! its own media), and they share one Rooms folder (where the room is kept
//! and its leader is found). The leader makes the room, leads it and loads
//! the first setlist; the member joins by the room's link once and is
//! checked to follow the leader's setlist, song and playhead. Then they
//! edit it together (studio#710): a person's fader move on the leader
//! reaches the member, and an agent's on the member, through its MCP,
//! reaches the leader as the agent's undo step, with the agent in the
//! room; undone, it is undone on both. Then the leader loads the second
//! setlist and the member is checked to move to it on the same
//! connection, and then to a lone session. Through each of those the
//! member's lighting is checked to follow the room's session
//! (studio#720): the session it follows (`SESSION_FOLLOWED`, what the app
//! points Ignition's visualizer at) is the leader's, led from the member's
//! own engine. Then the leader leaves, and the member opens the same link
//! again (studio#714): nobody leads the room, so it opens the room's
//! setlist (the lone session, the last thing loaded) on the member's own,
//! to practise, and plays it with the leader stopped. Its checks go to
//! `target/<triple>/<profile>/room/room.json`.
//!
//! **Speed (studio#717):** the join, and each load the member follows, is
//! timed and held to its budget in `../fixtures/speed.toml`; a time over
//! it fails the test, and the times go in the record's `times`, which CI
//! reports.
//!
//! Then the same room on a network with no route out (studio#715): the
//! test runs again inside a user and network namespace of its own
//! (`unshare`), where the two engines have loopback and one local network
//! and nothing else (no relay, no DNS). Its checks go to
//! `room/room-no-route-out.json`.

#[path = "../unbreakable/budgets.rs"]
#[allow(
    dead_code,
    reason = "shared with tests/unbreakable, which uses all of it"
)]
mod budgets;
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
#[path = "../fixtures/speed.rs"]
mod speed;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use check::Checks;
use engine::Engine;
use mcp::Mcp;
use speed::Times;

/// The two setlists the leader loads, one after the other, by their
/// fixture sessions.
const FIRST: &[&str] = &["multitrack-click", "markers-only"];
const SECOND: &[&str] = &["chart-repeats", "multitrack-click"];
/// The lone session the leader loads last.
const LONE: &str = "markers-only";

/// How long a member has to follow what the leader did.
const FOLLOW_WAIT: Duration = Duration::from_secs(20);

/// How long an edit has to reach the other engine: "about a second"
/// (studio#689), with the MCP reads either side of it.
const EDIT_WAIT: Duration = Duration::from_millis(2000);

/// The agent of the room's test, and the undo step it makes.
const AGENT: &str = "Claude";
const AGENT_STEP: &str = "Agent: level the keys";

/// How far apart two playheads read one after the other may be, seconds:
/// the follower's own tolerance, the two MCP reads between them, and
/// the transport's poll.
const PLAYHEAD_TOLERANCE: f64 = 0.35;

fn engine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_daw-extension"))
}

/// A Studio folder: the fixture sessions staged in it, a setlist of each
/// set of them in its Setlists folder.
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

/// The first `volume_db` in what `v` holds (MIX_STATE's track).
fn volume_in(v: &Value) -> Option<f64> {
    match v {
        Value::Object(o) => o
            .get("volume_db")
            .and_then(Value::as_f64)
            .or_else(|| o.values().find_map(volume_in)),
        Value::Array(a) => a.iter().find_map(volume_in),
        _ => None,
    }
}

/// `track`'s fader on `mcp`'s engine, dB.
fn volume(mcp: &Mcp, track: &str) -> Result<Value, String> {
    let state = mcp.run("MIX_STATE", &json!({"track": track, "depth": 0}))?;
    Ok(volume_in(&state).map_or(Value::Null, |db| json!(db)))
}

/// Whether `v` reads `db`.
fn reads(v: &Value, db: f64) -> bool {
    v.as_f64().is_some_and(|got| (got - db).abs() < 0.05)
}

/// The open song's history labels, newest first.
fn history(mcp: &Mcp) -> Vec<String> {
    mcp.run("SESSION_HISTORY", &json!({"limit": 20}))
        .ok()
        .and_then(|h| h["moments"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m["label"].as_str().map(str::to_owned))
        .collect()
}

/// The leader and the member edit the open song together: a person's
/// fader move on the leader reaches the member, and an agent's on the
/// member (through AGENT_RUN, as Studio's MCP runs every call) reaches the
/// leader as the agent's undo step, the agent in the leader's room.
fn edits(l: &Mcp, m: &Mcp, checks: &mut Checks) {
    // Both edit the set together once the leader has shared it and the
    // member joined it.
    let (ok_l, got_l) = until(
        FOLLOW_WAIT,
        || l.run("ROOM", &json!({})),
        |v| v["edits"] == json!(1),
    );
    let (ok_m, got_m) = until(
        FOLLOW_WAIT,
        || m.run("ROOM", &json!({})),
        |v| v["edits"] == json!(1),
    );
    checks.add(
        "the set is edited together",
        ok_l && ok_m,
        format!(
            "leader edits {} ({}), member edits {} ({})",
            got_l["edits"], got_l["error"], got_m["edits"], got_m["error"]
        ),
    );
    if !(ok_l && ok_m) {
        return;
    }
    // On the first song, which has the stems.
    run(l, checks, "song 1", "SESSION_SONG", &json!({"to": "1"}));
    let (ok, got) = until(FOLLOW_WAIT, || current(m), |v| *v == json!(1));
    checks.add("member back on song 1", ok, format!("member on song {got}"));

    // A person's edit on the leader.
    let began = Instant::now();
    if run(
        l,
        checks,
        "a person's edit reaches the member",
        "TRACK_SET",
        &json!({"track": "Bass", "volume_db": -6.0}),
    )
    .is_some()
    {
        let (ok, got) = until(EDIT_WAIT, || volume(m, "Bass"), |v| reads(v, -6.0));
        checks.add(
            "a person's edit reaches the member",
            ok,
            format!(
                "the member's Bass at {got} dB, {:.2} s after the leader's move",
                began.elapsed().as_secs_f64()
            ),
        );
    }

    // An agent's edit on the member, as one undo step of its own.
    let before = volume(l, "Keys").unwrap_or_default();
    let began = Instant::now();
    let ran = run(
        m,
        checks,
        "an agent's edit reaches the leader",
        "AGENT_RUN",
        &json!({
            "id": "TRACK_SET",
            "args": {"track": "Keys", "volume_db": -9.0},
            "step": AGENT_STEP,
            "agent": AGENT,
        }),
    );
    let ended = m
        .run("AGENT_STEP_END", &json!({"step": AGENT_STEP}))
        .unwrap_or_default();
    let Some(ran) = ran else { return };
    checks.add(
        "the agent's edit is its undo step",
        ran["ok"] == json!(true)
            && ran["step"] == json!(AGENT_STEP)
            && ended["undo"] == json!(AGENT_STEP),
        format!("AGENT_RUN {ran}, AGENT_STEP_END {ended}"),
    );
    let (ok, got) = until(EDIT_WAIT, || volume(l, "Keys"), |v| reads(v, -9.0));
    checks.add(
        "an agent's edit reaches the leader",
        ok,
        format!(
            "the leader's Keys at {got} dB, {:.2} s after the agent's move",
            began.elapsed().as_secs_f64()
        ),
    );
    let (ok, got) = until(
        FOLLOW_WAIT,
        || Ok(json!(history(l))),
        |v| {
            v.as_array()
                .is_some_and(|a| a.iter().any(|x| *x == json!(AGENT_STEP)))
        },
    );
    checks.add(
        "the leader has the agent's undo step",
        ok,
        format!("the leader's history {got}"),
    );
    let (ok, got) = until(
        FOLLOW_WAIT,
        || l.run("ROOM", &json!({})),
        |v| {
            v["agents"] == json!([AGENT])
                && v["people"]
                    .as_array()
                    .is_some_and(|p| p.contains(&json!(AGENT)))
        },
    );
    checks.add(
        "the agent is in the room",
        ok,
        format!(
            "the leader's room: people {}, agents {}",
            got["people"], got["agents"]
        ),
    );

    // The agent's step undone where it was made: undone on the leader too.
    let undone = m
        .run("SESSION_UNDO", &json!({}))
        .unwrap_or_else(|e| json!(e));
    let (ok, got) = until(
        EDIT_WAIT.saturating_mul(2),
        || volume(l, "Keys"),
        |v| before.as_f64().is_some_and(|db| reads(v, db)),
    );
    checks.add(
        "the agent's step is undone everywhere",
        ok && undone.to_string().contains(AGENT_STEP),
        format!("SESSION_UNDO {undone}; the leader's Keys at {got} dB, {before} before"),
    );
}

/// The current song's number.
fn current(mcp: &Mcp) -> Result<Value, String> {
    let rows = mcp.run("SESSION_SONGS", &json!({}))?;
    Ok(rows
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["current"] == json!(true)))
        .map_or(Value::Null, |r| r["number"].clone()))
}

/// The session an engine's chart and lighting follow
/// (`SESSION_FOLLOWED`): its file's name, and whether its transport is
/// led from that engine (a `url`, the lighting's clock).
fn followed(mcp: &Mcp) -> Result<Value, String> {
    let session = mcp.run("SESSION_FOLLOWED", &json!({}))?;
    let file = session["path"]
        .as_str()
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let led = session["url"].as_str().is_some_and(|u| !u.is_empty());
    Ok(json!({ "file": file, "led": led }))
}

/// The leader's session goes from `before` to another, and the member's
/// lighting follows it there: check `name`. The leader's, for the next.
fn lighting_follows(
    (l, m): (&Mcp, &Mcp),
    before: &Value,
    checks: &mut Checks,
    name: &str,
) -> Value {
    let (moved, leader) = until(
        FOLLOW_WAIT,
        || followed(l),
        |v| v["file"].is_string() && v["led"] == json!(true) && v != before,
    );
    let (ok, member) = until(FOLLOW_WAIT, || followed(m), |v| moved && *v == leader);
    checks.add(
        name,
        moved && ok,
        format!("leader {leader}, member {member}"),
    );
    leader
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

fn room(
    leader: &Engine,
    member: &Engine,
    leader_studio: &Path,
    (first, second): (&Path, &Path),
    checks: &mut Checks,
    times: &mut Times,
) {
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
    let began = Instant::now();
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
    if ok {
        let what = "joined by the link, the setlist open";
        times.judge(checks, "join time", "join", what, began.elapsed());
    }
    let lit = lighting_follows(
        (l, m),
        &Value::Null,
        checks,
        "member's lighting follows the session",
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
    let lit = lighting_follows((l, m), &lit, checks, "member's lighting follows the song");

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

    // Editing it together.
    edits(l, m, checks);

    // Mid-rehearsal, another setlist: the member moves, on the same
    // connection.
    let before = m.run("ROOM", &json!({})).unwrap_or_default();
    let began = Instant::now();
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
    if ok {
        let what = "the second setlist open on the member";
        let took = began.elapsed();
        times.judge(checks, "follow time (setlist)", "follow_load", what, took);
    }
    let lit = lighting_follows(
        (l, m),
        &lit,
        checks,
        "member's lighting moves to the second setlist",
    );

    // Then a lone session.
    let lone = leader_studio.join("sessions").join(LONE);
    let args = json!({"path": lone.display().to_string()});
    let began = Instant::now();
    if run(l, checks, "loads a session", "ROOM_LOAD", &args).is_none() {
        return;
    }
    let (ok, leader_lone) = until(
        FOLLOW_WAIT,
        || open_titles(l),
        |v| v.as_array().is_some_and(|a| a.len() == 1),
    );
    checks.add("leader opens a lone session", ok, format!("{leader_lone}"));
    let (ok, got) = until(FOLLOW_WAIT, || open_titles(m), |v| *v == leader_lone);
    checks.add(
        "member moves to the lone session",
        ok,
        format!("leader {leader_lone}, member {got}"),
    );
    if ok {
        let what = "the lone session open on the member";
        let took = began.elapsed();
        times.judge(checks, "follow time (session)", "follow_load", what, took);
    }
    lighting_follows(
        (l, m),
        &lit,
        checks,
        "member's lighting moves to the lone session",
    );
    match check::play_from(l, 0.5) {
        Ok(()) => {
            let (ok, got) = until(
                FOLLOW_WAIT,
                || m.run("SESSION_TRANSPORT", &json!({})),
                |v| v["playing"] == json!(true),
            );
            checks.add("member plays the lone session", ok, format!("{got}"));
            let _ = check::stop(l);
        }
        Err(e) => checks.failed("member plays the lone session", e),
    }

    let after = m.run("ROOM", &json!({})).unwrap_or_default();
    checks.add(
        "without reconnecting",
        before["attaches"] == json!(1)
            && after["attaches"] == json!(1)
            && after["load"] == json!(3),
        format!(
            "attaches {} then {}, load {}",
            before["attaches"], after["attaches"], after["load"]
        ),
    );
}

/// The leader leaves; the member, on a setlist of its own, opens the
/// room's link again: nobody leads it, so its setlist (the last thing the
/// leader loaded, the lone session) opens on the member's engine alone,
/// and plays there with the leader's stopped (studio#714).
fn practice(leader: &Engine, member: &Engine, member_first: &Path, checks: &mut Checks) {
    let (l, m) = (&leader.mcp, &member.mcp);
    let Some(rooms) = run(l, checks, "practice", "ROOMS", &json!({})) else {
        return;
    };
    let link = rooms[0]["link"].as_str().unwrap_or_default().to_owned();
    if run(l, checks, "practice", "ROOM_LEAVE", &json!({})).is_none()
        || run(m, checks, "practice", "ROOM_LEAVE", &json!({})).is_none()
    {
        return;
    }
    let rooms = l.run("ROOMS", &json!({})).unwrap_or_default();
    checks.add(
        "the room keeps its setlist when nobody leads",
        rooms[0]["live"].is_null()
            && rooms[0]["setlist"]["kind"] == json!("session")
            && rooms[0]["setlist"]["title"] == json!(LONE),
        format!("{rooms}"),
    );
    // Somewhere else first: the member's own copy of the first setlist.
    let args = json!({"path": member_first.display().to_string()});
    if run(m, checks, "practice", "SESSION_OPEN", &args).is_none() {
        return;
    }
    let (ok, got) = until(
        FOLLOW_WAIT,
        || open_titles(m),
        |v| v.as_array().is_some_and(|a| a.len() == FIRST.len()),
    );
    checks.add("member opens a setlist of its own", ok, format!("{got}"));
    if !ok {
        return;
    }
    let Some(joined) = run(
        m,
        checks,
        "practice",
        "ROOM_JOIN",
        &json!({"link": link, "name": "Alex"}),
    ) else {
        return;
    };
    checks.add(
        "the link opens the room's setlist to practise",
        joined["role"] == json!("practice") && joined["loaded"]["title"] == json!(LONE),
        format!("{joined}"),
    );
    // The leader's engine still has the lone session open, as it left it.
    let lone = open_titles(l).unwrap_or_default();
    let (ok, got) = until(
        FOLLOW_WAIT,
        || open_titles(m),
        |v| v.as_array().is_some_and(|a| a.len() == 1) && *v == lone,
    );
    checks.add(
        "member practises the room's setlist",
        ok,
        format!("leader {lone}, member {got}"),
    );
    let alone = m.run("ROOM", &json!({})).unwrap_or_default();
    checks.add(
        "practising is in no room",
        alone["room"].is_null(),
        format!("{alone}"),
    );
    match check::play_from(m, 0.5) {
        Ok(()) => {
            let lt = l.run("SESSION_TRANSPORT", &json!({})).unwrap_or_default();
            checks.add(
                "member plays it alone",
                lt["playing"] != json!(true),
                format!("leader {lt}"),
            );
            let _ = check::stop(m);
        }
        Err(e) => checks.failed("member plays it alone", e),
    }
}

/// Play the room (`room`) with a leader and a member, each its own
/// engine, after the checks already in `checks`; the record goes to
/// `<profile>/room/<record>.json`.
fn play_the_room(record: &str, mut checks: Checks) {
    let runs = report::profile_dir().join("room").join("runs").join(record);
    let _ = std::fs::remove_dir_all(&runs);
    // Each device's own Studio folder, with the same sessions in it.
    let folder = |who: &str| {
        let studio = runs.join(who).join("Studio");
        std::fs::create_dir_all(&studio).unwrap_or_else(|e| panic!("{}: {e}", studio.display()));
        let staged = stage(&studio).unwrap_or_else(|e| panic!("staging the {who}'s: {e}"));
        (studio, staged)
    };
    let (leader_studio, (first, second)) = folder("leader");
    let (member_studio, (member_first, _)) = folder("member");
    // One Rooms folder, the organization's: the member's is the leader's.
    let rooms = leader_studio.join("Rooms");
    std::fs::create_dir_all(&rooms).unwrap_or_else(|e| panic!("{}: {e}", rooms.display()));
    let member_rooms = member_studio.join("Rooms");
    std::os::unix::fs::symlink(&rooms, &member_rooms)
        .unwrap_or_else(|e| panic!("{}: {e}", member_rooms.display()));
    let spans = otlp::Receiver::start();
    let start = |who: &str, rooms: &Path| {
        let instance = format!("{record}-{who}-{}", std::process::id());
        let env = [("FTS_ROOMS_DIR", rooms.as_os_str())];
        Engine::start_with(&engine_bin(), &spans.endpoint, &instance, &env)
            .unwrap_or_else(|e| panic!("the {who}'s engine: {e}"))
    };
    let (mut leader, mut member) = (start("leader", &rooms), start("member", &member_rooms));
    let mut times = Times::new("join");
    if checks.ok() {
        room(
            &leader,
            &member,
            &leader_studio,
            (&first, &second),
            &mut checks,
            &mut times,
        );
    }
    if checks.ok() {
        practice(&leader, &member, &member_first, &mut checks);
    }

    let tails = format!(
        "--- the leader's last lines ---\n{}\n--- the member's last lines ---\n{}",
        leader.tail(40),
        member.tail(40)
    );
    leader.stop();
    member.stop();
    let json = json!({
        "test": record,
        "commit": report::commit(),
        "ok": checks.ok(),
        "checks": checks.json(),
        "times": times.json(),
    });
    let dir = report::profile_dir().join("room");
    let _ = std::fs::write(dir.join(format!("{record}.json")), format!("{json:#}\n"));
    let mut out = format!(
        "\n=== {record}: {} ===\n",
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
    assert!(checks.ok(), "{record} failed:\n{out}\n{tails}");
}

/// The leader and a member in one room.
#[test]
fn a_member_follows_the_leader_through_each_load() {
    play_the_room("room", Checks::default());
}

/// Set when this test binary runs inside the namespace
/// [`a_room_plays_on_a_network_with_no_route_out`] makes.
const NO_ROUTE_OUT: &str = "FTS_ROOM_NO_ROUTE_OUT";

/// The network inside that namespace: loopback, and one local network
/// (`lan0`, a dummy interface) with no route anywhere else, so there is
/// no internet, no relay and no DNS.
const LOCAL_NETWORK: &str = "set -e
ip link set lo up
ip link add lan0 type dummy
ip addr add 10.77.0.1/24 dev lan0
ip link set lan0 multicast on up
exec \"$@\"";

/// What there is to reach outside the local network: nothing, if each
/// of these fails.
fn no_route_out() -> (bool, String) {
    let tcp = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([1, 1, 1, 1], 443)),
        Duration::from_secs(2),
    );
    let udp = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect(std::net::SocketAddr::from(([8, 8, 8, 8], 53))));
    let said = format!(
        "1.1.1.1:443 over TCP: {}; 8.8.8.8:53 over UDP: {}",
        tcp.as_ref()
            .map_or_else(ToString::to_string, |_| "reached".to_owned()),
        udp.as_ref()
            .map_or_else(ToString::to_string, |()| "routed".to_owned()),
    );
    (tcp.is_err() && udp.is_err(), said)
}

/// The same room on a network with no route out (studio#715): this test
/// runs itself again inside a user and network namespace of its own
/// (`unshare`), where the leader's and the member's engines have only a
/// local network between them. They still make, lead, join and follow.
/// Linux only: elsewhere there is no network namespace to make.
#[test]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "needs a Linux network namespace (unshare)"
)]
fn a_room_plays_on_a_network_with_no_route_out() {
    const NAME: &str = "a_room_plays_on_a_network_with_no_route_out";
    if std::env::var_os(NO_ROUTE_OUT).is_none() {
        let exe = std::env::current_exe().expect("the test's own path");
        let status = std::process::Command::new("unshare")
            .args(["--user", "--map-root-user", "--net", "--", "sh", "-c"])
            .arg(LOCAL_NETWORK)
            .arg("sh")
            .arg(&exe)
            .args([NAME, "--exact", "--nocapture", "--test-threads", "1"])
            .env(NO_ROUTE_OUT, "1")
            .status()
            .unwrap_or_else(|e| {
                panic!("unshare (util-linux, in the dev shell) to make a network of its own: {e}")
            });
        assert!(
            status.success(),
            "the room on a network with no route out failed ({status}); its checks are above \
             and in {}",
            report::profile_dir()
                .join("room")
                .join("room-no-route-out.json")
                .display()
        );
        return;
    }
    let mut checks = Checks::default();
    let (ok, said) = no_route_out();
    checks.add("no route out", ok, said);
    play_the_room("room-no-route-out", checks);
}
