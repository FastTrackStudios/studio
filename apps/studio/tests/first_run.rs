//! Seam 1 on desktop for the first run (studio#699): a fresh install, with
//! no config, gets from signing in to an open setlist with no setup
//! screens, checked only through Studio's MCP door.
//!
//! The real Studio (`STUDIO_APP`, else this package's `Studio` binary) is
//! started as a first-time user's: an empty home folder (no Studio folder,
//! no Drive copies, no token, no MCP discovery file), its own instance
//! (`STUDIO_INSTANCE`), and a Drive node and account server of this test's
//! on loopback (`DRIVE_NODE`). The two stand in for Drive and auth at the
//! HTTP they speak: `/orgs`, signed out until a bearer the account server
//! issued comes with it, then one org for the account; `/oauth2/authorize`
//! and `/oauth2/token`; and the org's setlist file, found by `/search` and
//! read by `/fetch`. The system's browser is a script on `PATH` (`open`,
//! `xdg-open`) that writes down the page it was asked to open; the test is
//! then the person in it, following the account server back to Studio's
//! loopback port.
//!
//! Over MCP it checks:
//! - `STUDIO_ACCOUNT`: signed out, naming the account server;
//! - `STUDIO_SIGN_IN`: the account server opened in the browser, Studio
//!   back from it signed in and started again;
//! - after the restart, `STUDIO_ACCOUNT` signed in as the account, with
//!   its org, and the app on Home (`/`): no setup screen between;
//! - `STUDIO_FIRST_RUN`: `ready`, the org with its setlist and the route it
//!   opens at;
//! - `STUDIO_NAVIGATE` to it, then `STUDIO_ROUTE`: the setlist open, its
//!   songs read.
//!
//! It opens a window, so it needs a desktop session: `just test-first-run`
//! builds Studio and runs it; ignored otherwise.

#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The account the account server signs in, and what the node lists for it.
const WHO: &str = "ada@example.com";
const ORG: &str = "adult-jam";
/// The token the account server issues; the node takes nothing else.
const TOKEN: &str = "first-run-access-token";
const CODE: &str = "first-run-code";

/// The org's one setlist file, where its computer keeps it.
const LOCATION: &str = "loc-ada";
const SET_PATH: &str = "Setlists/Sunday.setlist";
const SET_NAME: &str = "Sunday";
const SONGS: [&str; 2] = ["Amazing Grace", "How Great Thou Art"];
const SET_TEXT: &str =
    "# org: adult-jam\n# Sunday morning\n= Amazing Grace\n= How Great Thou Art\n";

/// The instance this test's Studio is (its doors and engine its own).
const INSTANCE: &str = "first-run-test";

#[test]
#[ignore = "opens Studio's window (a desktop session): `just test-first-run`"]
fn a_fresh_install_signs_in_sees_its_orgs_and_opens_a_setlist() {
    let sandbox = tempfile::tempdir().expect("a sandbox");
    let root = sandbox.path();
    let stand = Stand::start();
    let opened = root.join("opened.txt");
    let bin = fake_browser(&root.join("bin"), &opened);
    let mut studio = Studio::start(root, &stand, &bin, &opened);

    // Signed out: the node wants a sign-in through its account server.
    let mcp = studio.door(Duration::from_secs(120));
    let account = mcp
        .run("STUDIO_ACCOUNT", &json!({}))
        .expect("STUDIO_ACCOUNT");
    assert_eq!(account["signed_in"], false, "{account}");
    assert_eq!(account["signed_out"], true, "{account}");
    assert_eq!(account["issuer"], stand.origin, "{account}");
    // Nothing of the app's own is up yet: it says so at once.
    let early = mcp.run("STUDIO_ORGS", &json!({}));
    assert!(early.is_err(), "STUDIO_ORGS before signing in: {early:?}");

    // Sign in: the account server in the browser, then back to Studio.
    let signing = {
        let mcp = mcp.clone();
        std::thread::spawn(move || mcp.run("STUDIO_SIGN_IN", &json!({})))
    };
    let page = until(Duration::from_secs(60), "the browser to be opened", || {
        std::fs::read_to_string(&opened).ok().and_then(|t| {
            t.lines()
                .find(|l| l.contains("/oauth2/authorize"))
                .map(str::to_string)
        })
    });
    assert!(page.starts_with(&stand.origin), "the browser opened {page}");
    browse(&page);
    let signed = signing
        .join()
        .expect("the sign-in's thread")
        .expect("STUDIO_SIGN_IN");
    assert_eq!(signed["signed_in"], true, "{signed}");
    assert_eq!(signed["restarting"], true, "{signed}");

    // Started again, signed in, on Home: no setup screen between.
    let mcp = studio.door_after_restart(&mcp, Duration::from_secs(120));
    let account = until(Duration::from_secs(30), "Studio to be signed in", || {
        mcp.run("STUDIO_ACCOUNT", &json!({}))
            .ok()
            .filter(|a| a["signed_in"] == true)
    });
    assert_eq!(account["who"], WHO, "{account}");
    assert_eq!(account["orgs"], json!([ORG]), "{account}");
    let orgs = until(Duration::from_secs(30), "the app's pages", || {
        mcp.run("STUDIO_ORGS", &json!({})).ok()
    });
    assert_eq!(orgs["orgs"], json!([ORG]), "only the account's org: {orgs}");
    assert_eq!(orgs["here"], ORG, "{orgs}");
    let route = mcp.run("STUDIO_ROUTE", &json!({})).expect("STUDIO_ROUTE");
    assert_eq!(route["route"], "/", "a first run lands on Home: {route}");

    // Home offers the org's setlist, one tap from open.
    let first = until(Duration::from_secs(60), "Home to offer the setlist", || {
        mcp.run("STUDIO_FIRST_RUN", &json!({}))
            .ok()
            .filter(|f| f["step"] == "ready")
    });
    let org = &first["orgs"][0];
    assert_eq!(org["org"], ORG, "{first}");
    let offered = org["setlists"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["title"] == SET_NAME))
        .unwrap_or_else(|| panic!("no {SET_NAME} offered: {first}"));
    let to = offered["route"].as_str().expect("its route").to_string();

    // Opened: the page shows it, its songs read.
    mcp.run("STUDIO_NAVIGATE", &json!({ "path": to }))
        .expect("STUDIO_NAVIGATE");
    let shown = until(Duration::from_secs(60), "the setlist to open", || {
        mcp.run("STUDIO_ROUTE", &json!({})).ok().filter(|r| {
            r["shown"]["songs"]
                .as_array()
                .is_some_and(|s| !s.is_empty())
        })
    });
    assert_eq!(shown["route"], to, "{shown}");
    assert!(
        shown["shown"]["name"]
            .as_str()
            .is_some_and(|n| n.contains(SET_NAME)),
        "{shown}"
    );
    assert_eq!(shown["shown"]["songs"], json!(SONGS), "{shown}");

    studio.stop();
    let asked = stand.asked();
    assert!(
        asked.iter().any(|a| a.starts_with("POST /oauth2/token")),
        "the code was traded at the account server: {asked:?}"
    );
}

// ── the browser ─────────────────────────────────────────────────────────

/// A folder whose `open` and `xdg-open` write down what they were asked to
/// open, in `opened`: the system's browser, for Studio.
fn fake_browser(dir: &Path, opened: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("the browser's folder");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\n",
        opened.display()
    );
    for name in ["open", "xdg-open"] {
        let p = dir.join(name);
        std::fs::write(&p, &script).expect("the browser");
        let mut perm = std::fs::metadata(&p).expect("its mode").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
        std::fs::set_permissions(&p, perm).expect("made runnable");
    }
    dir.to_path_buf()
}

/// What a person's browser does with the account server's page: the
/// account server sends it back to Studio (a redirect), and it goes.
fn browse(page: &str) {
    let (status, head, _) = get(page);
    assert_eq!(status, 302, "the account server's page: {head}");
    let back = head
        .lines()
        .find_map(|l| {
            l.strip_prefix("Location: ")
                .or_else(|| l.strip_prefix("location: "))
        })
        .expect("where it sends the browser")
        .trim()
        .to_string();
    assert!(back.starts_with("http://127.0.0.1:"), "back to {back}");
    let (status, _, _) = get(&back);
    assert_eq!(status, 200, "Studio's loopback page");
}

/// A plain GET: status, head, body.
fn get(url: &str) -> (u16, String, Vec<u8>) {
    let rest = url.strip_prefix("http://").expect("an http URL");
    let (at, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_string()), |(a, p)| (a, format!("/{p}")));
    let mut s = TcpStream::connect(at).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(30))).ok();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: {at}\r\nConnection: close\r\n\r\n"
    )
    .expect("send");
    let mut reply = Vec::new();
    s.read_to_end(&mut reply).expect("read");
    let split = reply
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(reply.len());
    let head = String::from_utf8_lossy(&reply[..split]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (
        status,
        head,
        reply.get(split + 4..).unwrap_or_default().to_vec(),
    )
}

// ── the stand-in node and account server ────────────────────────────────

/// Drive's node and the account server, as their HTTP goes, on loopback.
struct Stand {
    /// `http://127.0.0.1:<port>`: the issuer, and the node's address.
    origin: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Stand {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let origin = format!("http://{}", listener.local_addr().expect("addr"));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (o, a) = (origin.clone(), asked.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (o, a) = (o.clone(), a.clone());
                std::thread::spawn(move || serve(stream, &o, &a));
            }
        });
        Self { origin, asked }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().map(|a| a.clone()).unwrap_or_default()
    }
}

/// One request: its line, its headers (lower-cased names) and body.
fn serve(stream: TcpStream, origin: &str, asked: &Mutex<Vec<String>>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_lowercase(), v.trim().to_string()));
        }
    }
    let length: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    if let Ok(mut a) = asked.lock() {
        a.push(format!("{method} {target}"));
    }
    let bearer = headers
        .iter()
        .find(|(k, _)| k == "authorization")
        .map(|(_, v)| v.as_str());
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let reply = answer(method, path, query, bearer, origin);
    let mut stream = reader.into_inner();
    let _ = stream.write_all(&reply);
}

/// What the node or the account server answers.
fn answer(method: &str, path: &str, query: &str, bearer: Option<&str>, origin: &str) -> Vec<u8> {
    let auth = json!({ "issuer": origin, "client_id": "drive" });
    let signed_in = bearer == Some(&format!("Bearer {TOKEN}"));
    match (method, path) {
        ("GET", "/orgs") if signed_in => json_reply(
            200,
            &json!({ "auth": auth, "signed_in": true, "who": WHO, "orgs": [ORG], "default": "" }),
        ),
        ("GET", "/orgs") => json_reply(
            200,
            &json!({ "auth": auth, "signed_in": false, "orgs": [], "default": "" }),
        ),
        // The account server: the person is signed in there already, and
        // it sends them straight back with a code.
        ("GET", "/oauth2/authorize") => {
            let redirect = param(query, "redirect_uri").unwrap_or_default();
            let state = param(query, "state").unwrap_or_default();
            let to = format!("{redirect}?code={CODE}&state={}", encode(&state));
            format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes()
        }
        ("POST", "/oauth2/token") => json_reply(
            200,
            &json!({
                "access_token": TOKEN, "refresh_token": "first-run-refresh",
                "token_type": "Bearer", "expires_in": 3600,
            }),
        ),
        // The org's index: its one setlist file.
        ("GET", "/search") if param(query, "exts").is_some_and(|e| e.split(',').any(|x| x == "setlist")) => {
            let results = if param(query, "space").as_deref() == Some(ORG) {
                json!([{
                    "location": LOCATION, "location_name": "Ada's Mac", "device": "dev-ada",
                    "device_name": "Ada's Mac", "online": true, "path": SET_PATH,
                    "name": "Sunday.setlist", "dir": false, "size": SET_TEXT.len(),
                }])
            } else {
                json!([])
            };
            json_reply(200, &json!({ "results": results }))
        }
        ("GET", "/search") => json_reply(200, &json!({ "results": [] })),
        ("GET", p) if p == format!("/fetch/{ORG}/{LOCATION}") && param(query, "path").as_deref() == Some(SET_PATH) => {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SET_TEXT}",
                SET_TEXT.len()
            )
            .into_bytes()
        }
        _ => json_reply(404, &json!({})),
    }
}

fn json_reply(status: u16, v: &Value) -> Vec<u8> {
    let body = v.to_string();
    format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        if status == 200 { "OK" } else { "Not Found" },
        body.len()
    )
    .into_bytes()
}

fn param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then(|| decode(v))
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(x);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

// ── Studio ──────────────────────────────────────────────────────────────

/// The Studio under test, in a process group of its own (it starts itself
/// again on signing in, and the new one is in the same group).
struct Studio {
    child: Child,
    home: PathBuf,
    config: PathBuf,
}

impl Studio {
    fn start(root: &Path, stand: &Stand, bin: &Path, opened: &Path) -> Self {
        let app = std::env::var_os("STUDIO_APP").map_or_else(
            || PathBuf::from(env!("CARGO_BIN_EXE_Studio")),
            PathBuf::from,
        );
        let home = root.join("home");
        let config = home.join(".config");
        let runtime = root.join("run");
        for d in [&home, &config, &runtime] {
            std::fs::create_dir_all(d).expect("a folder of the sandbox");
        }
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let node = stand.origin.replacen("http://", "ws://", 1) + "/sync";
        let mut cmd = Command::new(app);
        cmd.env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("STUDIO_INSTANCE", INSTANCE)
            .env("DRIVE_NODE", node)
            .env("PATH", path)
            .env("FIRST_RUN_OPENED", opened);
        // A first-time user's: nothing of the person running the test.
        for k in [
            "STUDIO_HOME",
            "STUDIO_DATA_DIR",
            "DRIVE_AGENT_DIR",
            "DRIVE_CLIENT_DIR",
            "FTS_MCP",
            "FTS_MCP_PORT",
            "FTS_MCP_PRIVATE",
            "FTS_DAW_ENGINE_DIR",
            "BROWSER",
        ] {
            cmd.env_remove(k);
        }
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let child = cmd.spawn().expect("Studio started");
        Self {
            child,
            home,
            config,
        }
    }

    /// Where the discovery file of this instance's MCP door is.
    fn discovery(&self) -> Option<PathBuf> {
        let name = format!("studio-{INSTANCE}-mcp.json");
        [
            self.config.join("fts").join(&name),
            self.home
                .join("Library/Application Support/fts")
                .join(&name),
        ]
        .into_iter()
        .find(|p| p.is_file())
    }

    fn read_door(&self) -> Option<Mcp> {
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(self.discovery()?).ok()?).ok()?;
        Mcp::new(v["url"].as_str()?, v["token"].as_str()).ok()
    }

    /// The door, once Studio answers on it.
    fn door(&mut self, wait: Duration) -> Mcp {
        until(wait, "Studio's MCP door", || {
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!("Studio stopped before its door opened: {status}");
            }
            self.read_door().filter(Mcp::ping)
        })
    }

    /// The new Studio's door, once it has replaced `old`.
    fn door_after_restart(&self, old: &Mcp, wait: Duration) -> Mcp {
        until(wait, "Studio to start again", || {
            self.read_door().filter(|m| m.url != old.url && m.ping())
        })
    }

    /// Every process of the group: the Studio started and the one it
    /// started again.
    fn stop(&mut self) {
        let group = format!("-{}", self.child.id());
        let _ = Command::new("kill").args(["-TERM", "--", &group]).status();
        let _ = self.child.wait();
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        let group = format!("-{}", self.child.id());
        let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
    }
}

/// `f` until it gives something, or fail after `wait`.
fn until<T>(wait: Duration, what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(start.elapsed() < wait, "waited {wait:?} for {what}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

// ── the door ────────────────────────────────────────────────────────────

/// Studio's MCP door, as an agent reaches it: JSON-RPC over HTTP on
/// loopback with the token from its discovery file, every action through
/// the `run` tool.
#[derive(Clone, Debug)]
struct Mcp {
    url: String,
    at: String,
    path: String,
    token: Option<String>,
}

impl Mcp {
    fn new(url: &str, token: Option<&str>) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("not an http URL: {url}"))?;
        let (at, path) = rest.split_once('/').unwrap_or((rest, "mcp"));
        Ok(Self {
            url: url.to_string(),
            at: at.to_owned(),
            path: format!("/{path}"),
            token: token.map(str::to_string),
        })
    }

    fn ping(&self) -> bool {
        self.rpc("ping", &json!({})).is_ok()
    }

    fn run(&self, id: &str, args: &Value) -> Result<Value, String> {
        let result = self.rpc(
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

    fn rpc(&self, method: &str, params: &Value) -> Result<Value, String> {
        let body =
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        let mut stream = TcpStream::connect(&self.at).map_err(|e| format!("{}: {e}", self.at))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10 * 60)))
            .map_err(|e| e.to_string())?;
        let auth = self
            .token
            .as_ref()
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let head = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.path,
            self.at,
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|()| stream.write_all(body.as_bytes()))
            .map_err(|e| format!("{method}: {e}"))?;
        let mut reply = Vec::new();
        stream
            .read_to_end(&mut reply)
            .map_err(|e| format!("{method}: {e}"))?;
        let split = reply
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| format!("{method}: no HTTP reply"))?;
        let status = String::from_utf8_lossy(&reply[..split]);
        if !status.starts_with("HTTP/1.1 200") {
            return Err(format!(
                "{method}: {}",
                status.lines().next().unwrap_or_default()
            ));
        }
        let answer: Value = serde_json::from_slice(&reply[split + 4..])
            .map_err(|e| format!("{method}: the reply is not JSON: {e}"))?;
        if let Some(error) = answer.get("error") {
            return Err(format!("{method}: {error}"));
        }
        Ok(answer["result"].clone())
    }
}
