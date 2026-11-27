//! Seam 3, the web build's boot test: a browser opens a fixture session in
//! the web build and plays it through the engine in the page.
//!
//! The fixture is `tests/it/web_boot/multitrack-click`: a project of a click,
//! a bass and keys. Its recordings are made here, never committed, as the
//! proxies a page plays (`Media/Proxy/<stem>.ogg` and their page indexes),
//! with the `files.json` a folder at a link is listed by. One server serves
//! the web build and the session; the page opens it from its address
//! (`/?session=/sessions/multitrack-click`, `src/engine_web.rs`), the test
//! presses Play as a person would (a browser starts audio only on a
//! gesture), and reads what the page's engine says (`#studio-engine`): the
//! session open, its playhead moving, and the worklet putting out sound.
//! Then it goes into the session as a person would (the top bar's Session
//! button) and opens its Arrangement and Mixer: each the desktop's panel
//! over the page's engine (`studio_panels::web_host::HostedView`), read
//! off what the panel says it shows (`data-hosted-*`) and the canvases its
//! widgets draw in, with the song playing on under them.
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

    // Into the session: its arrangement, drawn over the page's engine,
    // with the song's three tracks and their items.
    session.click("button.tbchart");
    let arrangement = session.wait_for(
        Duration::from_secs(30),
        "the arrangement over the page's engine",
        |s| {
            s.hosted("arrangement")
                .filter(|h| h.tracks >= 3 && h.canvases > 0)
        },
    );
    assert!(
        arrangement.items >= 3,
        "the arrangement shows {} items of the song's three recordings",
        arrangement.items
    );

    // Its mixer: every strip of the same song, in the same engine.
    session.click("button.sesstab[data-view='mixer']");
    let mixer = session.wait_for(
        Duration::from_secs(30),
        "the mixer over the page's engine",
        |s| s.hosted("mixer").filter(|h| h.canvases > 0),
    );
    assert_eq!(
        mixer.project, arrangement.project,
        "the mixer shows another song than the arrangement"
    );
    assert_eq!(mixer.tracks, arrangement.tracks);

    // The views are over the engine that plays: it plays on under them.
    let before: f64 = session.state("data-at").parse().unwrap_or(0.0);
    session.wait_for(
        Duration::from_secs(10),
        "the song to play on under the mixer",
        |s| (s.state("data-at").parse::<f64>().unwrap_or(0.0) > before + 0.5).then_some(()),
    );
}

/// What a hosted panel (`studio_panels::web_host::HostedView`) says it
/// shows, off its `data-hosted-*` attributes.
#[derive(Debug)]
struct Hosted {
    project: String,
    tracks: u64,
    items: u64,
    /// The canvases its widgets draw in.
    canvases: u64,
}

/// The fixture has sections (Intro, Verse 1, Chorus 1) and no `.kf`: its
/// chart page shows the chart Keyflow generates from them, as the desktop's
/// pane and `SESSION_CHART` do (studio#742), and lights the bar the
/// playhead is in.
#[test]
#[ignore = "needs the web build and a WebDriver: `just test-web-boot`"]
fn the_chart_page_shows_a_generated_chart_and_follows_the_playhead() {
    let dist = PathBuf::from(
        std::env::var_os("STUDIO_WEB_DIST")
            .expect("STUDIO_WEB_DIST: the web build's public folder"),
    );
    let sessions = tempfile::tempdir().expect("a folder for the session");
    stage_session(&sessions.path().join("multitrack-click"));
    let site = Server::start(dist, sessions.path().to_path_buf());

    let driver = Driver::start();
    let session = driver.new_session();
    session.navigate(&format!(
        "{}/now/chart?session=/sessions/multitrack-click",
        site.origin
    ));

    // The generated chart engraved, a time on every bar.
    let bars = session.wait_for(Duration::from_secs(90), "the generated chart", |s| {
        if s.state("data-state") == "failed" {
            panic!(
                "the session did not open in the page: {}",
                s.state("data-why")
            );
        }
        let bars = s.bars();
        (!bars.is_empty()).then_some(bars)
    });
    let line = session.script("return document.querySelector('.chartnow')?.innerText || '';");
    assert!(
        line.contains("The generated chart"),
        "the page does not say the chart is generated: {line}"
    );
    // 120 bpm in 4/4, two seconds a bar: the Intro's two bars a count-in
    // (no measure numbers), then Verse 1's four (measures 1 to 4) and
    // Chorus 1's two (5 and 6), each bar at its time in the recording.
    let times: Vec<f64> = bars.iter().map(|(at, _)| *at).collect();
    let numbers: Vec<u32> = bars.iter().map(|(_, bar)| *bar).collect();
    assert_eq!(
        numbers,
        (1..=8).collect::<Vec<_>>(),
        "the chart's bars: {bars:?}"
    );
    for (i, at) in times.iter().enumerate() {
        let want = 2.0 * i as f64;
        assert!(
            (at - want).abs() < 0.05,
            "bar {} is at {at}s, not {want}s: {bars:?}",
            i + 1
        );
    }

    // Played, the lit bar follows the playhead past the first bars.
    session.click("button.npb[aria-label='Play']");
    session.wait_for(Duration::from_secs(30), "the chart to follow", |s| {
        let lit = s.script(
            "return document.querySelector('#nowchart .kf-bar[aria-current]')?.getAttribute('data-at') || '';",
        );
        let at: f64 = s.state("data-at").parse().unwrap_or(0.0);
        let lit: f64 = lit.parse().ok()?;
        (at > 4.5 && lit >= 4.0 && lit <= at).then_some(())
    });
}

/// A screen the app is laid out for, in CSS pixels.
#[derive(Clone, Copy, Debug)]
struct Screen {
    name: &'static str,
    width: u32,
    height: u32,
}

/// An iPhone 15's (and 14's, 13's) screen, upright.
const PHONE: Screen = Screen {
    name: "phone",
    width: 390,
    height: 844,
};

/// An iPad Air's, upright: app-shell's desktop form, with its sidebar.
const IPAD: Screen = Screen {
    name: "ipad",
    width: 820,
    height: 1180,
};

/// A built-in set with five songs, in the org a page starts in.
const SET: &str = "/set/main_2026-11-22";

/// studio#702 at phone width: a set built (searched, added to, a key and a
/// leader picked, a song moved), then the session's arrangement, mixer,
/// chart and overview, each checked for fit and kept as a screenshot.
#[test]
#[ignore = "needs the web build and a WebDriver: `just test-web-screens`"]
fn screens_at_phone_width() {
    screens(PHONE);
}

/// studio#702 on an iPad: the same, at its size.
#[test]
#[ignore = "needs the web build and a WebDriver: `just test-web-screens`"]
fn screens_on_an_ipad() {
    screens(IPAD);
}

fn screens(screen: Screen) {
    let dist = PathBuf::from(
        std::env::var_os("STUDIO_WEB_DIST")
            .expect("STUDIO_WEB_DIST: the web build's public folder"),
    );
    let sessions = tempfile::tempdir().expect("a folder for the session");
    stage_session(&sessions.path().join("multitrack-click"));
    let site = Server::start(dist, sessions.path().to_path_buf());
    let driver = Driver::start();
    let session = driver.new_session_at(Some(screen));
    let shots = Shots::new(screen);
    building_a_set(&session, &site.origin, &shots);
    the_sessions_views(&session, &site.origin, &shots);
}

/// The set's page, then each step of building it a person takes there.
fn building_a_set(session: &Session<'_>, origin: &str, shots: &Shots) {
    session.navigate(&format!("{origin}{SET}"));
    let before = session.wait_for(Duration::from_secs(90), "the set's running order", |s| {
        let songs = s.songs();
        (songs.len() >= 5).then_some(songs)
    });
    session.fits(
        "the set's page",
        &[".erow", "button.tool[aria-label='Add songs']"],
    );
    shots.take(session, "setlist");

    // Search the library and add the first song found.
    session.click("button.tool[aria-label='Add songs']");
    let search = "input[aria-label='Search the library']";
    let first = session.wait_for(Duration::from_secs(20), "the library's songs", |s| {
        let t = s.script("return document.querySelector('.addsong .t1')?.textContent || '';");
        (!t.trim().is_empty()).then_some(t)
    });
    let query = first
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    session.type_into(search, &query);
    session.wait_for(Duration::from_secs(10), "the library searched", |s| {
        let value = s.script_with(
            "return document.querySelector(arguments[0])?.value || '';",
            json!([search]),
        );
        let rows: u64 = s
            .script("return String(document.querySelectorAll('.addsong').length);")
            .parse()
            .unwrap_or(0);
        (value == query && rows > 0).then_some(())
    });
    session.fits("adding songs", &[search, ".addsong"]);
    shots.take(session, "add-songs");
    session.click(".addsong");
    session.wait_for(Duration::from_secs(10), "the song added", |s| {
        (s.songs().len() == before.len() + 1).then_some(())
    });
    session.click(".bsheet .bhead button");
    session.wait_for(Duration::from_secs(10), "the sheet closed", |s| {
        (s.count(search) == 0).then_some(())
    });

    // The first song's key, moved.
    let key =
        "return document.querySelector('.erow:not(.esec):not(.eitem) .key2')?.innerText || '';";
    let was = session.script(key);
    session.click(".erow:not(.esec):not(.eitem) .key2");
    session.wait_for(Duration::from_secs(10), "the keys", |s| {
        (s.count(".keypick .kp") >= 12).then_some(())
    });
    session.fits("picking a key", &[".keypick .kp"]);
    session.taps("picking a key", &[".keypick .kp"]);
    shots.take(session, "key");
    session.click(".keypick .kp:not(.on):not(.orig)");
    session.wait_for(Duration::from_secs(10), "the key moved", |s| {
        let now = s.script(key);
        (!now.is_empty() && now != was).then_some(())
    });

    // Who leads it, then down one.
    let title = before[0].clone();
    let at = |s: &Session<'_>| s.rows().iter().position(|t| *t == title);
    let from = at(session).expect("the first song in the running order");
    session.click(".erow:not(.esec):not(.eitem) > button.mid");
    session.wait_for(Duration::from_secs(10), "the song's sheet", |s| {
        (s.count(".edrow select option") >= 2).then_some(())
    });
    let leader = session.script(
        "return document.querySelector('.edrow select option:nth-child(2)')?.textContent || '';",
    );
    session.click(".edrow select option:nth-child(2)");
    session.wait_for(Duration::from_secs(10), "the leader shown on its row", |s| {
        let row = s.script(
            "return document.querySelector('.erow:not(.esec):not(.eitem) .meta3')?.innerText || '';",
        );
        row.contains(leader.trim()).then_some(())
    });
    session.fits("a song in the set", &[".edmove button", ".edrow select"]);
    session.taps("a song in the set", &[".edmove button"]);
    shots.take(session, "entry");
    session.click("button[aria-label='Move down']");
    session.wait_for(Duration::from_secs(10), "the song moved down", |s| {
        (at(s) == Some(from + 1)).then_some(())
    });
}

/// Into the session as a phone goes (the Now Playing bar opens the player,
/// the player the session), then each of its views.
fn the_sessions_views(session: &Session<'_>, origin: &str, shots: &Shots) {
    session.navigate(&format!("{origin}/?session=/sessions/multitrack-click"));
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
    session.click("button.npb[aria-label='Play']");
    session.wait_for(Duration::from_secs(30), "the session to play", |s| {
        (s.state("data-at").parse::<f64>().unwrap_or(0.0) > 0.5).then_some(())
    });
    session.click("button.npmain");
    session.click("button[data-open='arrangement']");
    session.wait_for(Duration::from_secs(30), "the arrangement", |s| {
        s.hosted("arrangement")
            .filter(|h| h.tracks >= 3 && h.canvases > 0)
    });
    let chrome = [".sesstab", "[data-hosted]", ".np .npb"];
    session.fits("the arrangement", &chrome);
    session.taps(
        "the session's tabs and transport",
        &[".sesstab", ".np .npb"],
    );
    session.drawn("the arrangement");
    shots.take(session, "arrangement");

    session.click("button.sesstab[data-view='mixer']");
    session.wait_for(Duration::from_secs(30), "the mixer", |s| {
        s.hosted("mixer").filter(|h| h.canvases > 0)
    });
    session.fits("the mixer", &chrome);
    session.drawn("the mixer");
    shots.take(session, "mixer");

    session.click("button.sesstab[data-view='chart']");
    session.wait_for(Duration::from_secs(60), "the chart", |s| {
        (!s.bars().is_empty()).then_some(())
    });
    session.fits("the chart", &[".sesstab", "#nowchart svg", ".np .npb"]);
    let drawn: Value = serde_json::from_str(&session.script(
        "const svg = document.querySelector('#nowchart svg'); \
         const r = svg.getBoundingClientRect(); \
         const own = svg.viewBox.baseVal.width || svg.width.baseVal.value; \
         return JSON.stringify({ scale: r.width / own, own, shown: r.width, \
           laid: Number(document.getElementById('nowchart').getAttribute('data-layout-width')) });",
    ))
    .expect("the chart's size");
    let scale = drawn["scale"].as_f64().unwrap_or(0.0);
    assert!(
        scale >= 0.8,
        "{}: the chart is drawn at {scale:.2} of its size ({drawn}): too small to read",
        screen_of(session)
    );
    shots.take(session, "chart");

    session.click("button.sesstab[data-view='overview']");
    session.wait_for(Duration::from_secs(10), "the overview", |s| {
        (s.count(".sessrow") > 0).then_some(())
    });
    session.fits("the overview", &[".sessrow", ".sesstab"]);
    shots.take(session, "overview");
}

/// The page's width, for a failure to say.
fn screen_of(session: &Session<'_>) -> String {
    session.script("return innerWidth + 'x' + innerHeight;")
}

/// Where the screenshots go: `STUDIO_WEB_SHOTS` (the recipe's
/// `target/web-shots`), else a folder in the temp dir.
struct Shots {
    dir: PathBuf,
    screen: Screen,
}

impl Shots {
    fn new(screen: Screen) -> Self {
        let dir = std::env::var_os("STUDIO_WEB_SHOTS").map_or_else(
            || std::env::temp_dir().join("studio-web-shots"),
            PathBuf::from,
        );
        std::fs::create_dir_all(&dir).expect("the screenshots' folder");
        Self { dir, screen }
    }

    /// The page as it is now, as `<screen>-<name>.png`.
    fn take(&self, session: &Session<'_>, name: &str) {
        use base64::Engine as _;
        let shot = session.call("GET", "/screenshot", None);
        let png = base64::engine::general_purpose::STANDARD
            .decode(shot.as_str().unwrap_or_default())
            .expect("a screenshot");
        let path = self.dir.join(format!("{}-{name}.png", self.screen.name));
        std::fs::write(&path, png).expect("the screenshot is written");
    }
}

/// The fixture session staged into `dir`: its project, each track's
/// recording as a proxy with its page index, and the folder's `files.json`.
fn stage_session(dir: &Path) {
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/web_boot/multitrack-click");
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
        self.new_session_at(None)
    }

    /// A browser whose page is `screen` (a touch screen, as a phone's and
    /// an iPad's are), else a desktop's window.
    fn new_session_at(&self, screen: Option<Screen>) -> Session<'_> {
        let browser = if self.gecko {
            json!({ "moz:firefoxOptions": { "args": ["-headless"] } })
        } else {
            let mut options = json!({
                "args": ["--headless=new", "--no-sandbox", "--window-size=1280,800",
                         "--autoplay-policy=no-user-gesture-required"]
            });
            if let Some(screen) = screen {
                options["mobileEmulation"] = json!({ "deviceMetrics": {
                    "width": screen.width, "height": screen.height,
                    "pixelRatio": 2.0, "touch": true,
                } });
            }
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
        let session = Session { driver: self, id };
        // geckodriver has no emulation: the window at the screen's size.
        if let (true, Some(screen)) = (self.gecko, screen) {
            session.call(
                "POST",
                "/window/rect",
                Some(&json!({ "width": screen.width, "height": screen.height })),
            );
        }
        session
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

    /// What `script` returns, as text.
    fn script(&self, script: &str) -> String {
        self.script_with(script, json!([]))
    }

    /// What `script` returns given `args`, as text.
    fn script_with(&self, script: &str, args: Value) -> String {
        self.call(
            "POST",
            "/execute/sync",
            Some(&json!({ "script": script, "args": args })),
        )
        .as_str()
        .unwrap_or_default()
        .to_owned()
    }

    /// How many elements `css` matches.
    fn count(&self, css: &str) -> usize {
        self.script_with(
            "return String(document.querySelectorAll(arguments[0]).length);",
            json!([css]),
        )
        .parse()
        .unwrap_or(0)
    }

    /// The set's songs as its page lists them, in order.
    fn songs(&self) -> Vec<String> {
        self.texts(".erow:not(.esec):not(.eitem) > button.mid .t1")
    }

    /// Every row of the set's running order (songs, sections, items).
    fn rows(&self) -> Vec<String> {
        self.texts(".erow > button.mid .t1")
    }

    fn texts(&self, css: &str) -> Vec<String> {
        let listed = self.script_with(
            "return JSON.stringify([...document.querySelectorAll(arguments[0])]                .filter(e => e.getClientRects().length)                .map(e => e.textContent.trim()));",
            json!([css]),
        );
        serde_json::from_str(&listed).unwrap_or_default()
    }

    /// What does not fit on `where` at this screen's width: the page
    /// scrolling sideways, or any of `parts` (each must be there) reaching
    /// past either edge.
    fn fits(&self, place: &str, parts: &[&str]) {
        let found = self.script_with(
            "const w = innerWidth, out = [];              const doc = document.scrollingElement || document.documentElement;              if (doc.scrollWidth > w + 1) out.push(`the page scrolls sideways (${doc.scrollWidth} > ${w})`);              for (const css of arguments[0]) {                const all = [...document.querySelectorAll(css)].filter(e => e.getClientRects().length && e.offsetWidth > 0);                if (!all.length) out.push(`no ${css}`);                for (const e of all) { const r = e.getBoundingClientRect();                  if (r.left < -1 || r.right > w + 1)                    out.push(`${css} at ${Math.round(r.left)}..${Math.round(r.right)} of ${w}: ${(e.innerText || '').trim().slice(0, 40)}`); }              }              return JSON.stringify(out.slice(0, 12));",
            json!([parts]),
        );
        let wrong: Vec<String> = serde_json::from_str(&found).unwrap_or_default();
        assert!(
            wrong.is_empty(),
            "{place} does not fit {}:\n  {}",
            screen_of(self),
            wrong.join("\n  ")
        );
    }

    /// Whether each of `parts` is a finger's size (44 pt, at least 40 for a
    /// tab in a pill) where the screen is touched: a phone's form, or a
    /// coarse pointer.
    fn taps(&self, place: &str, parts: &[&str]) {
        let found = self.script_with(
            "const touch = matchMedia('(pointer: coarse)').matches || innerWidth < 600;              if (!touch) return '[]';              const out = [];              for (const css of arguments[0]) for (const e of document.querySelectorAll(css)) {                const r = e.getBoundingClientRect();                if (!e.getClientRects().length || r.width === 0) continue;                if (r.height < 40 || r.width < 40)                  out.push(`${css} is ${Math.round(r.width)}x${Math.round(r.height)}: ${(e.innerText || e.getAttribute('aria-label') || '').trim().slice(0, 30)}`);              }              return JSON.stringify(out.slice(0, 12));",
            json!([parts]),
        );
        let small: Vec<String> = serde_json::from_str(&found).unwrap_or_default();
        assert!(
            small.is_empty(),
            "{place}: too small to tap at {}:\n  {}",
            screen_of(self),
            small.join("\n  ")
        );
    }

    /// Whether the hosted panel on show draws in canvases that are inside
    /// the screen, and of a size to see.
    fn drawn(&self, place: &str) {
        let found = self.script(
            "const w = innerWidth, h = innerHeight;              const cs = [...document.querySelectorAll('[data-hosted] canvas')]                .map(c => c.getBoundingClientRect()).filter(r => r.width > 0 && r.height > 0);              const area = cs.reduce((a, r) => a + Math.max(0, Math.min(r.right, w) - Math.max(r.left, 0))                * Math.max(0, Math.min(r.bottom, h) - Math.max(r.top, 0)), 0);              return JSON.stringify({ canvases: cs.length, share: area / (w * h) });",
        );
        let drawn: Value = serde_json::from_str(&found).unwrap_or_default();
        let share = drawn["share"].as_f64().unwrap_or(0.0);
        assert!(
            share >= 0.2,
            "{place} at {} draws on {:.0}% of the screen ({drawn})",
            screen_of(self),
            share * 100.0
        );
    }

    /// The chart page's bars as it times them: each `kf-bar`'s time
    /// (seconds) and its bar, in playing order.
    fn bars(&self) -> Vec<(f64, u32)> {
        let listed = self.script(
            "return JSON.stringify([...document.querySelectorAll('#nowchart .kf-bar')] \
               .map(b => [parseFloat(b.getAttribute('data-at')), parseInt(b.getAttribute('data-bar'))]));",
        );
        serde_json::from_str(&listed).unwrap_or_default()
    }

    /// The hosted panel showing `which` (`arrangement`, `mixer`), once it
    /// is open on a song.
    fn hosted(&self, which: &str) -> Option<Hosted> {
        let script = "const e = document.querySelector(`[data-hosted='${arguments[0]}']`); \
                      if (!e) return null; \
                      return { project: e.getAttribute('data-hosted-project') || '', \
                               tracks: Number(e.getAttribute('data-hosted-tracks') || 0), \
                               items: Number(e.getAttribute('data-hosted-items') || 0), \
                               canvases: e.querySelectorAll('canvas').length };";
        let v = self.call(
            "POST",
            "/execute/sync",
            Some(&json!({ "script": script, "args": [which] })),
        );
        let project = v.get("project")?.as_str()?.to_owned();
        (!project.is_empty()).then(|| Hosted {
            project,
            tracks: v["tracks"].as_u64().unwrap_or(0),
            items: v["items"].as_u64().unwrap_or(0),
            canvases: v["canvases"].as_u64().unwrap_or(0),
        })
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

    /// Wait for `css` to be there, then type `text` into it.
    fn type_into(&self, css: &str, text: &str) {
        let found = self.find(css);
        self.call(
            "POST",
            &format!("/element/{found}/value"),
            Some(&json!({ "text": text })),
        );
    }

    /// Wait for `css` to be there, then click it.
    fn click(&self, css: &str) {
        let found = self.find(css);
        self.call("POST", &format!("/element/{found}/click"), Some(&json!({})));
    }

    /// The first element `css` matches, once there is one.
    fn find(&self, css: &str) -> String {
        self.wait_for(Duration::from_secs(20), css, |s| {
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
        })
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
