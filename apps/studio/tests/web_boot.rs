//! Seam 3, the web build's boot test: a browser opens a fixture session in
//! the web build and plays it through the engine in the page.
//!
//! The fixture is `tests/web_boot/multitrack-click`: a project of a click,
//! a bass and keys. Its recordings are made here, never committed, as the
//! proxies a page plays (`Media/Proxy/<stem>.ogg` and their page indexes),
//! with the `files.json` a folder at a link is listed by. One server serves
//! the web build and the session; the page opens it from its address
//! (`/?session=/sessions/multitrack-click`, `src/engine_web.rs`), the test
//! presses Play as a person would (a browser starts audio only on a
//! gesture), and reads what the page's engine says (`#studio-engine`): the
//! session open, its playhead moving, and the worklet putting out sound.
//!
//! It needs the web build (`STUDIO_WEB_DIST`, its `public` folder) and a
//! W3C WebDriver (`WEBDRIVER`: chromedriver or geckodriver, started here;
//! `CHROME_BIN` names the browser chromedriver drives). `just test-web-boot`
//! builds the one and finds the others; ignored otherwise.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The page's crash reports (`src/trace.rs` posts a `crash` span to its
/// node, here this test's server): what a failure quotes.
static CRASHES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The fixture's rate and length.
const RATE: u32 = 48_000;
const SECONDS: f32 = 16.0;

#[test]
#[ignore = "needs the web build and a WebDriver: `just test-web-boot`"]
fn the_web_build_opens_and_plays_a_fixture_session() {
    let dist = PathBuf::from(
        std::env::var_os("STUDIO_WEB_DIST")
            .expect("STUDIO_WEB_DIST: the web build's public folder"),
    );
    assert!(
        dist.join("index.html").is_file(),
        "{} has no index.html: build the web app first",
        dist.display()
    );
    let sessions = tempfile::tempdir().expect("a folder for the session");
    stage_session(&sessions.path().join("multitrack-click"));
    let site = Server::start(dist, sessions.path().to_path_buf());

    let driver = Driver::start();
    let session = driver.new_session();
    let page = format!("{}/?session=/sessions/multitrack-click", site.origin);
    session.navigate(&page);

    // The session open in the page's engine.
    let ready = session.wait_for(Duration::from_secs(90), "the session to open", |s| match s
        .state("data-state")
        .as_str()
    {
        "ready" => Some(Ok(())),
        "failed" => Some(Err(s.state("data-why"))),
        _ => None,
    });
    if let Err(why) = ready {
        panic!("the session did not open in the page: {why}");
    }

    // Play, pressed as a person would.
    session.click("button.npb[aria-label='Play']");

    // Its playhead moves and the worklet puts out sound: a second of it.
    session.wait_for(Duration::from_secs(30), "the session to be heard", |s| {
        let loud: u64 = s.state("data-loud-frames").parse().unwrap_or(0);
        let at: f64 = s.state("data-at").parse().unwrap_or(0.0);
        (loud >= u64::from(RATE) && at > 1.0).then_some(())
    });
    let peak: f32 = session.state("data-peak-ever").parse().unwrap_or(0.0);
    assert!(peak > 0.01, "the page played silence (peak {peak})");
    assert_eq!(
        session.state("data-playing"),
        "true",
        "the transport stopped"
    );
}

/// The fixture session staged into `dir`: its project, each track's
/// recording as a proxy with its page index, and the folder's `files.json`.
fn stage_session(dir: &Path) {
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/web_boot/multitrack-click");
    let proxies = dir.join("Media/Proxy");
    std::fs::create_dir_all(&proxies).expect("the session's folders");
    std::fs::copy(
        here.join("multitrack-click.RPP"),
        dir.join("multitrack-click.RPP"),
    )
    .expect("the project");
    for (stem, samples) in [
        ("click", click(120.0)),
        ("bass", tone(55.0, -12.0)),
        ("keys", tone(440.0, -18.0)),
    ] {
        let ogg = proxies.join(format!("{stem}.ogg"));
        let bytes = fts_sample::cache::encode_ogg_vorbis(&samples, 1, RATE, 0.4)
            .expect("the proxy encodes");
        std::fs::write(&ogg, bytes).expect("the proxy is written");
        fts_sample::cache::write_ogg_index(&ogg, 48_000).expect("the proxy's page index");
    }
    let mut files = Vec::new();
    list(dir, dir, &mut files);
    std::fs::write(
        dir.join("files.json"),
        serde_json::to_vec(&files).expect("the listing"),
    )
    .expect("the listing is written");
}

fn list(root: &Path, dir: &Path, out: &mut Vec<Value>) {
    for entry in std::fs::read_dir(dir).expect("the folder lists").flatten() {
        let path = entry.path();
        if path.is_dir() {
            list(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(json!({ "path": rel.to_string_lossy().replace('\\', "/"), "size": size }));
        }
    }
}

/// A sine at `hz`, `db` dBFS, for the length of the song.
fn tone(hz: f32, db: f32) -> Vec<f32> {
    let gain = 10f32.powf(db / 20.0);
    (0..frames())
        .map(|i| gain * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin())
        .collect()
}

/// A click on every beat at `bpm`: 20 ms of a decaying 1 kHz tick.
fn click(bpm: f32) -> Vec<f32> {
    let beat = (RATE as f32 * 60.0 / bpm) as usize;
    let tick = RATE as usize / 50;
    (0..frames())
        .map(|i| {
            let into = i % beat;
            if into >= tick {
                return 0.0;
            }
            let t = into as f32 / RATE as f32;
            0.5 * (1.0 - into as f32 / tick as f32) * (std::f32::consts::TAU * 1000.0 * t).sin()
        })
        .collect()
}

fn frames() -> usize {
    (RATE as f32 * SECONDS) as usize
}

/// A free port on loopback.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

/// The web build at the root and the sessions under `/sessions/`, served by
/// range, every other path the app's page (its router's).
struct Server {
    origin: String,
}

impl Server {
    fn start(dist: PathBuf, sessions: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the server's port");
        let origin = format!("http://{}", listener.local_addr().expect("its address"));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (dist, sessions) = (dist.clone(), sessions.clone());
                std::thread::spawn(move || {
                    let _ = serve(stream, &dist, &sessions);
                });
            }
        });
        Self { origin }
    }
}

fn serve(stream: TcpStream, dist: &Path, sessions: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let method = line.split_whitespace().next().unwrap_or("GET").to_owned();
    let target = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let (mut range, mut length) = (None, 0usize);
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("range") {
                range = parse_range(value.trim());
            } else if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    // A post is the page's telemetry: its crash reports kept, the rest let go.
    if method != "GET" {
        let mut body = vec![0u8; length];
        reader.read_exact(&mut body)?;
        let body = String::from_utf8_lossy(&body);
        if body.contains("crash.message")
            && let Ok(mut crashes) = CRASHES.lock()
        {
            crashes.push(body.into_owned());
        }
        let mut out = stream;
        return out.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
    }
    let path = percent_decode(target.split('?').next().unwrap_or("/"));
    let file = match path.strip_prefix("/sessions/") {
        Some(rest) => sessions.join(rest),
        None => {
            let asked = dist.join(path.trim_start_matches('/'));
            if asked.is_file() {
                asked
            } else {
                dist.join("index.html")
            }
        }
    };
    let mut out = stream;
    let Ok(bytes) = std::fs::read(&file) else {
        return out.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    };
    let kind = match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("ogg") => "audio/ogg",
        _ => "application/octet-stream",
    };
    let len = bytes.len() as u64;
    let (status, body, extra) = match range {
        Some((start, end)) if start < len => {
            let end = end.unwrap_or(len - 1).min(len - 1);
            (
                "206 Partial Content",
                &bytes[start as usize..=end as usize],
                format!("Content-Range: bytes {start}-{end}/{len}\r\n"),
            )
        }
        _ => ("200 OK", &bytes[..], String::new()),
    };
    write!(
        out,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{extra}Connection: close\r\n\r\n",
        body.len()
    )?;
    out.write_all(body)
}

/// `bytes=a-b` (or `a-`).
fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()))
}

fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = path.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A W3C WebDriver started here, stopped when dropped.
struct Driver {
    child: Child,
    base: String,
    gecko: bool,
}

impl Driver {
    fn start() -> Self {
        let program = std::env::var("WEBDRIVER").unwrap_or_else(|_| "chromedriver".into());
        let port = free_port();
        let gecko = program.contains("geckodriver");
        let mut command = Command::new(&program);
        if gecko {
            command.args(["--port", &port.to_string()]);
        } else {
            command.arg(format!("--port={port}"));
        }
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("{program} did not start: {e}"));
        let driver = Self {
            child,
            base: format!("127.0.0.1:{port}"),
            gecko,
        };
        let began = Instant::now();
        while http(&driver.base, "GET", "/status", None).is_err() {
            assert!(
                began.elapsed() < Duration::from_secs(20),
                "{program} never answered"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        driver
    }

    fn new_session(&self) -> Session<'_> {
        let browser = if self.gecko {
            json!({ "moz:firefoxOptions": { "args": ["-headless"] } })
        } else {
            let mut options = json!({
                "args": ["--headless=new", "--no-sandbox", "--window-size=1280,800",
                         "--autoplay-policy=no-user-gesture-required"]
            });
            if let Ok(bin) = std::env::var("CHROME_BIN") {
                options["binary"] = json!(bin);
            }
            json!({ "goog:chromeOptions": options, "goog:loggingPrefs": { "browser": "ALL" } })
        };
        let answer = http(
            &self.base,
            "POST",
            "/session",
            Some(&json!({ "capabilities": { "alwaysMatch": browser } })),
        )
        .expect("a browser session");
        let id = answer["value"]["sessionId"]
            .as_str()
            .unwrap_or_else(|| panic!("no session: {answer}"))
            .to_owned();
        Session { driver: self, id }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Session<'a> {
    driver: &'a Driver,
    id: String,
}

impl Session<'_> {
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Value {
        let path = format!("/session/{}{path}", self.id);
        let answer = http(&self.driver.base, method, &path, body)
            .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
        if answer["value"].get("error").is_some() {
            panic!("{method} {path}: {}", answer["value"]);
        }
        answer["value"].clone()
    }

    fn navigate(&self, url: &str) {
        // Every uncaught error the page has, with its stack, kept for a
        // failure to quote (chromedriver's CDP door; best effort).
        let catch = "window.__bootErrors = []; \
            const keep = x => window.__bootErrors.push(String((x && x.stack) || x)); \
            addEventListener('error', e => keep(e.error || e.message)); \
            addEventListener('unhandledrejection', e => keep(e.reason)); \
            const error = console.error; \
            console.error = (...a) => { keep(a.map(String).join(' ')); error(...a); };";
        let _ = http(
            &self.driver.base,
            "POST",
            &format!("/session/{}/goog/cdp/execute", self.id),
            Some(
                &json!({ "cmd": "Page.addScriptToEvaluateOnNewDocument", "params": { "source": catch } }),
            ),
        );
        self.call("POST", "/url", Some(&json!({ "url": url })));
    }

    /// An attribute of the page's engine state element (`#studio-engine`).
    fn state(&self, attribute: &str) -> String {
        let script = "const e = document.getElementById('studio-engine'); \
                      return e ? (e.getAttribute(arguments[0]) || '') : '';";
        self.call(
            "POST",
            "/execute/sync",
            Some(&json!({ "script": script, "args": [attribute] })),
        )
        .as_str()
        .unwrap_or_default()
        .to_owned()
    }

    /// What the page shows and what went wrong in it, for a failure.
    fn page(&self) -> String {
        let script = "return JSON.stringify({ url: location.href, title: document.title, \
                      text: (document.body ? document.body.innerText : '').slice(0, 400), \
                      errors: (window.__bootErrors || []).slice(0, 8) });";
        let shown = self
            .call(
                "POST",
                "/execute/sync",
                Some(&json!({ "script": script, "args": [] })),
            )
            .as_str()
            .unwrap_or_default()
            .to_owned();
        // chromedriver's console log (geckodriver has none to give).
        let log = http(
            &self.driver.base,
            "POST",
            &format!("/session/{}/se/log", self.id),
            Some(&json!({ "type": "browser" })),
        )
        .ok()
        .and_then(|v| v["value"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .rev()
        .take(15)
        .filter_map(|e| e["message"].as_str().map(str::to_owned))
        .collect::<Vec<_>>()
        .join("\n  ");
        let crashes = CRASHES.lock().map(|c| c.join("\n  ")).unwrap_or_default();
        format!("{shown}\n console:\n  {log}\n crashes:\n  {crashes}")
    }

    /// Wait for `css` to be there, then click it.
    fn click(&self, css: &str) {
        let found = self.wait_for(Duration::from_secs(20), css, |s| {
            let path = format!("/session/{}/element", s.id);
            let answer = http(
                &s.driver.base,
                "POST",
                &path,
                Some(&json!({ "using": "css selector", "value": css })),
            )
            .ok()?;
            let element = answer["value"]
                .as_object()?
                .values()
                .next()?
                .as_str()?
                .to_owned();
            Some(element)
        });
        self.call("POST", &format!("/element/{found}/click"), Some(&json!({})));
    }

    /// Poll `ready` until it says something, at most `within`.
    fn wait_for<T>(&self, within: Duration, what: &str, ready: impl Fn(&Self) -> Option<T>) -> T {
        let began = Instant::now();
        loop {
            if let Some(done) = ready(self) {
                return done;
            }
            if began.elapsed() > within {
                panic!(
                    "waited {within:?} for {what}; the page's engine says state={} why={} at={} loud={}; the page: {}",
                    self.state("data-state"),
                    self.state("data-why"),
                    self.state("data-at"),
                    self.state("data-loud-frames"),
                    self.page(),
                );
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = http(
            &self.driver.base,
            "DELETE",
            &format!("/session/{}", self.id),
            None,
        );
    }
}

/// One JSON request to the driver on loopback.
fn http(host: &str, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let mut stream = TcpStream::connect(host).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| e.to_string())?;
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    // The answer by its length (or its last chunk): a driver may keep the
    // connection open whatever the request asked.
    let mut answer = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        answer.extend_from_slice(&buf[..n]);
        if complete(&answer) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&answer);
    let (head, rest) = text.split_once("\r\n\r\n").ok_or("no answer")?;
    let rest = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        unchunk(rest)
    } else {
        rest.to_owned()
    };
    serde_json::from_str(&rest).map_err(|e| format!("{e}: {rest}"))
}

/// Whether `answer` holds a whole HTTP response.
fn complete(answer: &[u8]) -> bool {
    let text = String::from_utf8_lossy(answer);
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return false;
    };
    let head = head.to_ascii_lowercase();
    if head.contains("transfer-encoding: chunked") {
        return body.ends_with("0\r\n\r\n");
    }
    head.lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|n| n.trim().parse::<usize>().ok())
        .is_some_and(|n| body.len() >= n)
}

fn unchunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if n == 0 || rest.len() < n {
            break;
        }
        out.push_str(&rest[..n]);
        body = rest[n..].trim_start_matches("\r\n");
    }
    out
}
