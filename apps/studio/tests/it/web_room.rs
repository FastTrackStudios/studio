//! Seam 2 with a browser member (studio#711, studio#689): a desktop leads
//! a room, and the web build in a browser joins it by its link, over the
//! internet, and follows the leader. The desktop's MCP observes the
//! browser in the room.
//!
//! The leader is a desktop engine (`daw-extension --engine`), sandboxed as
//! the room harness sandboxes one (`tests/room` in daw-extension), read
//! only over its MCP door. It makes a room, leads it and loads a setlist
//! of two songs from a Studio folder made here. The page is the web
//! build, served with that Studio folder beside it (`/sessions/studio`,
//! standing in for the organization's Drive: `?studio=`). It opens the
//! room's link (`/join/<room>`), and its person presses Join. From then on
//! it gets no other instruction. The test checks, in order:
//!
//! 1. The page is a member and opens the leader's setlist (`#studio-room`),
//!    and the leader's `ROOM` lists the browser, having opened that load.
//! 2. The page follows the leader's change of song.
//! 3. It plays at the leader's playhead, read from both sides, and stops
//!    with the leader.
//! 4. The leader loads another setlist, and the page moves to it on the
//!    same connection (`attaches` stays 1).
//! 5. A second page whose copy of the room's file has a pass of its own
//!    making is refused by the leader's door: its page says why, the
//!    leader's `ROOM` counts it in `refused`, and it is never in the room.
//!
//! The page reaches the leader through n0's relays (a page has no direct
//! path to a device), so the test needs the internet. It needs the web
//! build (`STUDIO_WEB_DIST`), a W3C WebDriver (`WEBDRIVER`, `CHROME_BIN`)
//! and the engine (`STUDIO_ENGINE_BIN`). `just test-web-room` builds the
//! engine and the web app and finds the rest; ignored otherwise.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::web_boot::{self, Driver, RATE, Server, Session};

/// The two songs of the Studio folder, and the setlists of them the
/// leader loads one after the other.
const SONGS: [&str; 2] = ["one", "two"];
const FIRST: &[&str] = &["one", "two"];
const SECOND: &[&str] = &["two"];

/// How long the page has to follow what the leader did: over a relay.
const FOLLOW_WAIT: Duration = Duration::from_secs(45);

/// How far apart the leader's playhead and the page's may read, one after
/// the other: the follower's own tolerance, the relay, the two reads.
const PLAYHEAD_TOLERANCE: f64 = 0.75;

#[test]
#[ignore = "needs the web build, a WebDriver, the engine and the internet: `just test-web-room`"]
fn a_browser_joins_a_desktop_hosted_room_and_follows_the_leader() {
    let dist = PathBuf::from(
        std::env::var_os("STUDIO_WEB_DIST")
            .expect("STUDIO_WEB_DIST: the web build's public folder"),
    );
    let engine_bin = PathBuf::from(
        std::env::var_os("STUDIO_ENGINE_BIN")
            .expect("STUDIO_ENGINE_BIN: the engine (daw-extension)"),
    );
    let site = tempfile::tempdir().expect("a folder for the site");
    let studio = site.path().join("studio");
    let (first, second) = stage_studio(&studio);

    let leader = Leader::start(&engine_bin, &studio.join("Rooms"));
    let made = leader.run(
        "ROOM_CREATE",
        &json!({"name": "Adult Jam", "org": "fixtures"}),
    );
    let id = made["id"].as_str().expect("the room's id").to_owned();
    leader.run("ROOM_LEAD", &json!({"room": id, "name": "Leader"}));
    leader.run("ROOM_LOAD", &json!({"path": first}));
    leader.settled();
    let _key = KeyCleanup(id.clone());

    let web = Server::start(dist, site.path().to_path_buf());
    let driver = Driver::start();
    let page = driver.new_session();
    page.navigate(&format!("{}/join/{id}?studio=/sessions/studio", web.origin));
    page.wait_for(Duration::from_secs(60), "the room's page", |p| {
        (p.script_with(
            "return String(!!document.getElementById('room-join'));",
            json!([]),
        ) == "true")
            .then_some(())
    });
    page.click("#room-join");

    // 1. A member, with the leader's setlist open; the leader sees it.
    room_wait(
        &page,
        FOLLOW_WAIT,
        "the page to open the leader's setlist",
        |p| room(p, "data-state") == "member" && room(p, "data-opened") == "1",
    );
    assert_eq!(room(&page, "data-loaded"), "first");
    let seen = until(
        FOLLOW_WAIT,
        || leader.room(),
        |r| browser_opened(r) == Some(1),
    );
    assert_eq!(
        browser_opened(&seen),
        Some(1),
        "the leader's ROOM lists the browser with the setlist open: {seen}"
    );
    assert_eq!(seen["people"], json!(["Browser", "Leader"]), "{seen}");

    // 2. The leader changes song: the page does too.
    leader.run("SESSION_SONG", &json!({"to": "2"}));
    room_wait(&page, FOLLOW_WAIT, "the page to follow the song", |p| {
        room(p, "data-here-song") == "2"
    });

    // 3. The leader plays: the page plays at its playhead, and stops.
    leader.run("SESSION_PLAY_FROM", &json!({"at": 1.0}));
    room_wait(
        &page,
        FOLLOW_WAIT,
        "the page to play with the leader",
        |p| {
            p.state("data-playing") == "true"
                && p.state("data-at").parse::<f64>().is_ok_and(|at| at > 1.0)
        },
    );
    std::thread::sleep(Duration::from_secs(2));
    let theirs = leader.run("SESSION_TRANSPORT", &json!({}))["position"]
        .as_f64()
        .unwrap_or(f64::NAN);
    let ours: f64 = page.state("data-at").parse().unwrap_or(f64::NAN);
    let apart = (theirs - ours).abs();
    assert!(
        apart < PLAYHEAD_TOLERANCE,
        "the page plays {apart:.3} s from the leader (leader {theirs:.3}, page {ours:.3})"
    );
    leader.run("SESSION_STOP", &json!({}));
    room_wait(
        &page,
        FOLLOW_WAIT,
        "the page to stop with the leader",
        |p| p.state("data-playing") == "false",
    );

    // 4. Another setlist, mid-rehearsal: the page moves, on one connection.
    leader.run("ROOM_LOAD", &json!({"path": second}));
    room_wait(
        &page,
        FOLLOW_WAIT,
        "the page to open the second setlist",
        |p| room(p, "data-opened") == "2",
    );
    assert_eq!(room(&page, "data-loaded"), "second");
    assert_eq!(room(&page, "data-attaches"), "1", "the page attached again");
    let seen = until(
        FOLLOW_WAIT,
        || leader.room(),
        |r| browser_opened(r) == Some(2),
    );
    assert_eq!(browser_opened(&seen), Some(2), "{seen}");
    assert_eq!(seen["refused"], json!(0), "the member was refused: {seen}");

    // 5. A pass of its own making: the leader's door refuses it.
    forge_room(&studio, &site.path().join("forged"), &id);
    let forger = driver.new_session();
    forger.navigate(&format!("{}/join/{id}?studio=/sessions/forged", web.origin));
    forger.wait_for(Duration::from_secs(60), "the forged room's page", |p| {
        (p.script_with(
            "return String(!!document.getElementById('room-join'));",
            json!([]),
        ) == "true")
            .then_some(())
    });
    forger.click("#room-join");
    room_wait(
        &forger,
        FOLLOW_WAIT,
        "the leader to refuse a forged pass",
        |p| room(p, "data-why").contains("permission denied"),
    );
    let seen = until(
        FOLLOW_WAIT,
        || leader.room(),
        |r| r["refused"].as_u64().is_some_and(|n| n > 0),
    );
    assert!(
        seen["refused"].as_u64().is_some_and(|n| n > 0),
        "the leader's door counted no refusal: {seen}"
    );
    assert_eq!(
        seen["people"],
        json!(["Browser", "Leader"]),
        "a refused page is in the room: {seen}"
    );
    assert_ne!(room(&forger, "data-opened"), "2");
    assert_ne!(
        room(&forger, "data-state"),
        "member",
        "a refused page says it is in the room"
    );
    // The member is still in, and still on one connection.
    assert_eq!(room(&page, "data-attaches"), "1");
}

/// An attribute of the page's room state (`#studio-room`).
fn room(page: &Session<'_>, attribute: &str) -> String {
    page.script_with(
        "const e = document.getElementById('studio-room'); \
         return e ? (e.getAttribute(arguments[0]) || '') : '';",
        json!([attribute]),
    )
}

/// Wait for `ok` on the page, quoting its room state if it never comes.
fn room_wait(page: &Session<'_>, within: Duration, what: &str, ok: impl Fn(&Session<'_>) -> bool) {
    let began = Instant::now();
    while !ok(page) {
        assert!(
            began.elapsed() < within,
            "waited {within:?} for {what}; the page's room: state={} why={} loaded={} load={} opened={} attaches={} people={} song={} here-song={}; its engine: state={} why={} at={} playing={}; the page: {}",
            room(page, "data-state"),
            room(page, "data-why"),
            room(page, "data-loaded"),
            room(page, "data-load"),
            room(page, "data-opened"),
            room(page, "data-attaches"),
            room(page, "data-people"),
            room(page, "data-song"),
            room(page, "data-here-song"),
            page.state("data-state"),
            page.state("data-why"),
            page.state("data-at"),
            page.state("data-playing"),
            page.page(),
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Wait for `ok` on what `read` says, up to `wait`; the last reading
/// either way.
fn until(wait: Duration, read: impl Fn() -> Value, ok: impl Fn(&Value) -> bool) -> Value {
    let began = Instant::now();
    loop {
        let got = read();
        if ok(&got) || began.elapsed() > wait {
            return got;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The load the leader's `ROOM` says the browser has opened.
fn browser_opened(room: &Value) -> Option<u64> {
    room["here"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == json!("Browser"))
        .and_then(|p| p["opened"].as_u64())
}

/// A Studio folder in `studio`: two songs, each with its recordings (the
/// leader's engine plays them) and their proxies with the `files.json` a
/// folder at a link is listed by (the page plays those), two setlists of
/// them, and an empty Rooms folder. The two setlists' paths.
fn stage_studio(studio: &Path) -> (PathBuf, PathBuf) {
    for (n, name) in SONGS.iter().enumerate() {
        let dir = studio.join("sessions").join(name);
        let proxies = dir.join("Media/Proxy");
        std::fs::create_dir_all(&proxies).expect("the song's folders");
        std::fs::write(dir.join(format!("{name}.RPP")), project()).expect("the project");
        let keys_hz = if n == 0 { 440.0 } else { 330.0 };
        for (stem, samples) in [
            ("click", web_boot::click(120.0)),
            ("keys", web_boot::tone(keys_hz, -18.0)),
        ] {
            std::fs::write(dir.join(format!("Media/{stem}.wav")), wav(&samples))
                .expect("the recording");
            let ogg = proxies.join(format!("{stem}.ogg"));
            let bytes = fts_sample::cache::encode_ogg_vorbis(&samples, 1, RATE, 0.4)
                .expect("the proxy encodes");
            std::fs::write(&ogg, bytes).expect("the proxy is written");
            fts_sample::cache::write_ogg_index(&ogg, u64::from(RATE))
                .expect("the proxy's page index");
        }
        let mut files = Vec::new();
        web_boot::list(&dir, &dir, &mut files);
        std::fs::write(
            dir.join("files.json"),
            serde_json::to_vec(&files).expect("the listing"),
        )
        .expect("the listing is written");
    }
    let setlists = studio.join("Setlists");
    std::fs::create_dir_all(&setlists).expect("the Setlists folder");
    std::fs::create_dir_all(studio.join("Rooms")).expect("the Rooms folder");
    let write = |file: &str, names: &[&str]| {
        let path = setlists.join(file);
        let lines: String = names.iter().map(|n| format!("../sessions/{n}\n")).collect();
        std::fs::write(&path, lines).expect("the setlist");
        path
    };
    (
        write("first.setlist", FIRST),
        write("second.setlist", SECOND),
    )
}

/// A song's project: a click and keys, sixteen seconds, nothing on them.
fn project() -> String {
    let track = |name: &str, file: &str| {
        format!(
            "  <TRACK\n    NAME \"{name}\"\n    <ITEM\n      POSITION 0\n      LENGTH 16\n      NAME {file}\n      <SOURCE WAVE\n        FILE \"Media/{file}\"\n      >\n    >\n  >\n"
        )
    };
    format!(
        "<REAPER_PROJECT 0.1 \"7.0/fixture\" 1700000000\n  SAMPLERATE 48000 0 0\n  TEMPO 120 4 4 0\n{}{}>\n",
        track("Click", "click.wav"),
        track("Keys", "keys.wav"),
    )
}

/// `samples` as a mono 16-bit WAV at [`RATE`].
fn wav(samples: &[f32]) -> Vec<u8> {
    let data = u32::try_from(samples.len() * 2).expect("a short recording");
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        #[allow(clippy::cast_possible_truncation, reason = "clamped to i16's range")]
        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// A copy of the room's file in a Studio folder of its own (`forged`),
/// with a pass of its own making: what someone with the link and the
/// leader's endpoint, but not the organization's folder, could have.
fn forge_room(studio: &Path, forged: &Path, id: &str) {
    let file = format!("Rooms/{id}.room");
    let mut room: Value =
        serde_json::from_slice(&std::fs::read(studio.join(&file)).expect("the room's file"))
            .expect("the room's file is JSON");
    room["live"]["pass"] = json!("0".repeat(64));
    std::fs::create_dir_all(forged.join("Rooms")).expect("the forged Rooms folder");
    std::fs::write(
        forged.join(&file),
        serde_json::to_vec(&room).expect("the forged room"),
    )
    .expect("the forged room is written");
}

/// The leader: a desktop engine with its own sandbox, read over its MCP
/// door; stopped when dropped.
struct Leader {
    child: Child,
    /// `host:port` of its MCP door.
    door: String,
    log: PathBuf,
    _sandbox: tempfile::TempDir,
}

impl Leader {
    fn start(bin: &Path, rooms: &Path) -> Self {
        let sandbox = tempfile::Builder::new()
            .prefix("room-")
            .tempdir_in("/tmp")
            .expect("a sandbox");
        let root = sandbox.path();
        let (run, data, engine_dir, engines) = (
            root.join("run"),
            root.join("data"),
            root.join("engine"),
            root.join("engines"),
        );
        for dir in [&run, &data, &engine_dir, &engines] {
            std::fs::create_dir_all(dir).expect("the sandbox's folders");
        }
        let log = root.join("engine.out");
        let out = std::fs::File::create(&log).expect("the engine's log");
        let err = out.try_clone().expect("the engine's log");
        let mut command = Command::new(bin);
        command.arg("--engine");
        for var in [
            "FTS_MCP_TOKEN",
            "FTS_MCP",
            "FTS_DAW_OPEN",
            "STUDIO_ENGINE",
            "STUDIO_EXT_DEV",
            "OTEL_EXPORTER_OTLP_ENDPOINT",
        ] {
            command.env_remove(var);
        }
        let child = command
            .env("FTS_DAW_ENGINE_DIR", &engine_dir)
            .env("XDG_RUNTIME_DIR", &run)
            .env("XDG_DATA_HOME", &data)
            .env("FTS_ENGINES_DIR", &engines)
            .env("FTS_ROOMS_DIR", rooms)
            .env("STUDIO_INSTANCE", "web-room-leader")
            .env("FTS_MCP_PORT", "0")
            .env("FTS_MCP_PRIVATE", "1")
            .env("FTS_AUDIO_DEVICE", "null")
            .env("FTS_RENDER_WORKER", "process")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .unwrap_or_else(|e| panic!("{} did not start: {e}", bin.display()));
        let registry = engines.join(format!("studio-{}.json", child.id()));
        let mut leader = Self {
            child,
            door: String::new(),
            log,
            _sandbox: sandbox,
        };
        let began = Instant::now();
        loop {
            if let Ok(Some(status)) = leader.child.try_wait() {
                panic!("the engine exited ({status}):\n{}", leader.tail());
            }
            if let Some(door) = door_in(&registry) {
                leader.door = door;
                if leader.call("ping", &json!({})).is_ok() {
                    return leader;
                }
            }
            assert!(
                began.elapsed() < Duration::from_secs(120),
                "the engine served no MCP door:\n{}",
                leader.tail()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// One JSON-RPC call to its door, as an agent makes it: its `result`.
    fn call(&self, method: &str, params: &Value) -> Result<Value, String> {
        use std::io::Write as _;
        let body =
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        let mut stream =
            std::net::TcpStream::connect(&self.door).map_err(|e| format!("{}: {e}", self.door))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(300)))
            .map_err(|e| e.to_string())?;
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.door,
            body.len()
        )
        .map_err(|e| format!("{method}: {e}"))?;
        let mut reply = Vec::new();
        stream
            .read_to_end(&mut reply)
            .map_err(|e| format!("{method}: {e}"))?;
        let split = reply
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| format!("{method}: no HTTP reply"))?;
        let (head, rest) = reply.split_at(split);
        let status = String::from_utf8_lossy(head);
        if !status.starts_with("HTTP/1.1 200") {
            return Err(format!(
                "{method}: {}",
                status.lines().next().unwrap_or_default()
            ));
        }
        let answer: Value = serde_json::from_slice(rest.get(4..).unwrap_or_default())
            .map_err(|e| format!("{method}: the reply is not JSON: {e}"))?;
        if let Some(error) = answer.get("error") {
            return Err(format!("{method}: {error}"));
        }
        Ok(answer["result"].clone())
    }

    /// Run action `id` with `args`: its answer. A failed action fails the
    /// test, with the engine's log.
    fn run(&self, id: &str, args: &Value) -> Value {
        self.try_run(id, args)
            .unwrap_or_else(|e| panic!("{e}\nthe engine's log:\n{}", self.tail()))
    }

    fn try_run(&self, id: &str, args: &Value) -> Result<Value, String> {
        let result = self.call(
            "tools/call",
            &json!({"name": "run", "arguments": {"id": id, "args": args}}),
        )?;
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        if result["isError"].as_bool() == Some(true) {
            return Err(format!("{id}: {text}"));
        }
        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.clone());
        }
        Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
    }

    /// Its `ROOM`, or what went wrong reading it.
    fn room(&self) -> Value {
        self.try_run("ROOM", &json!({}))
            .unwrap_or_else(|e| json!({ "error": e }))
    }

    /// Its open settled: every song of what it loaded open.
    fn settled(&self) {
        let began = Instant::now();
        loop {
            let opening = self.run("SESSION_OPENING", &json!({}));
            let songs = self.run("SESSION_SONGS", &json!({}));
            let open = songs.as_array().is_some_and(|rows| {
                !rows.is_empty() && rows.iter().all(|r| r["state"] == json!("open"))
            });
            if opening["state"] == json!("idle") && open {
                return;
            }
            assert!(
                began.elapsed() < Duration::from_secs(60),
                "the leader's open did not settle: {opening} {songs}\n{}",
                self.tail()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The end of its log.
    fn tail(&self) -> String {
        let mut text = String::new();
        let _ = std::fs::File::open(&self.log).and_then(|mut f| f.read_to_string(&mut text));
        let lines: Vec<&str> = text.lines().collect();
        lines
            .get(lines.len().saturating_sub(40)..)
            .unwrap_or_default()
            .join("\n")
    }
}

impl Drop for Leader {
    fn drop(&mut self) {
        let _ = self.try_run("ROOM_LEAVE", &json!({}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The `host:port` of the MCP door an engine's registry file names, once
/// it is bound.
fn door_in(registry: &Path) -> Option<String> {
    let found: Value = serde_json::from_slice(&std::fs::read(registry).ok()?).ok()?;
    let url = found["mcp"].as_str()?;
    let at = url.strip_prefix("http://")?.split('/').next()?;
    (!at.ends_with(":0")).then(|| at.to_owned())
}

/// The endpoint key the leader kept for the room, removed when the test
/// ends. On Linux it is in the sandbox (`XDG_DATA_HOME`); on macOS in the
/// real data folder.
struct KeyCleanup(String);

impl Drop for KeyCleanup {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(home) = std::env::var_os("HOME") {
            let _ = std::fs::remove_file(
                PathBuf::from(home)
                    .join("Library/Application Support/fasttrackstudio/rooms")
                    .join(format!("{}.key", self.0)),
            );
        }
    }
}
