//! A setlist downloaded ahead of a gig opens and plays with the network
//! off (studio#716), over MCP.
//!
//! 1. **Shared.** Two fixture sessions are staged as the setlist's source
//!    and made ready to share by `SESSION_PREPARE` (each recording's Ogg
//!    proxy), the first song's proxies then moved to the newer layout
//!    (`Media/Proxy/`). A share link serves them (`/s/<code>/_files.json`
//!    and each file by `Range`) as a Drive node serves one: each song's
//!    prepared `.session` is its project.
//! 2. **Downloaded.** `SETLIST_DOWNLOAD` brings the setlist down twice, as
//!    a desktop keeps it (`full`: the original recordings too) and as a
//!    phone or a browser keeps it (`proxies`: never an original), and
//!    `SETLIST_DOWNLOADING` says what is on the device.
//! 3. **Network off.** The link's server is stopped and the source deleted.
//!    On Linux the test then runs itself again in a network namespace of
//!    its own (`unshare`, as the room's no-internet test does): loopback
//!    and nothing else, no route out. A fresh engine opens each downloaded
//!    `.setlist`, moves through its songs, plays each, and LISTENs to its
//!    tracks: heard from the originals on the desktop's copy, from the
//!    proxies on the phone's.
//!
//! The records go to `fixtures/offline-setlist.json` (the download) and
//! `fixtures/offline-setlist-played.json` (the play with the network off).

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::check::{self, Checks};
use crate::fixture::Fixture;

/// The setlist's songs, in running order: fixture sessions with
/// recordings (a setlist of songs with nothing to download proves
/// nothing).
const SONGS: &[&str] = &["multitrack-click", "chart-repeats"];

/// The setlist's name, as the link titles it.
const NAME: &str = "Offline Gig";

/// The share link's code.
const CODE: &str = "offlinegig0000";

/// Set (to the run's folder) when this test runs itself again with the
/// network off.
const NETWORK_OFF: &str = "FTS_OFFLINE_SETLIST_RUN";

/// How long a download may take.
const DOWNLOAD_WAIT: Duration = Duration::from_secs(180);

/// The test: shared, downloaded, then played with the network off.
pub fn a_downloaded_setlist_plays_with_the_network_off(test: &str) {
    if let Some(run) = std::env::var_os(NETWORK_OFF) {
        played_offline(&PathBuf::from(run));
        return;
    }
    let run = crate::run_dir("offline-setlist");
    download(&run);
    if cfg!(target_os = "linux") {
        let exe = std::env::current_exe().expect("the test's own path");
        let status = std::process::Command::new("unshare")
            .args(["--user", "--map-root-user", "--net", "--", "sh", "-c"])
            .arg("set -e\nip link set lo up\nexec \"$@\"")
            .arg("sh")
            .arg(&exe)
            .args([test, "--exact", "--nocapture", "--test-threads", "1"])
            .env(NETWORK_OFF, &run)
            .status()
            .unwrap_or_else(|e| {
                panic!("unshare (util-linux, in the dev shell) to cut the network: {e}")
            });
        assert!(
            status.success(),
            "the downloaded setlist did not play with the network off ({status}); its checks \
             are above and in {}",
            crate::report::profile_dir()
                .join("fixtures")
                .join("offline-setlist-played.json")
                .display()
        );
    } else {
        // No network namespace here: the link's server is gone and its
        // files deleted, which is what the device would find offline.
        played_offline(&run);
    }
}

/// Stage, prepare, share and download the setlist both ways; write what
/// the offline half needs into `run/offline.json`.
fn download(run: &Path) {
    let (engine, _spans) = crate::engine("offline-setlist");
    let mut checks = Checks::default();
    let source = run.join("source");
    let mut songs = Vec::new();
    for name in SONGS {
        let staged = Fixture::load(name).and_then(|f| f.stage(&source));
        match staged {
            Ok(song) => songs.push((*name, song)),
            Err(why) => checks.failed(format!("{name} staged"), why),
        }
    }
    for (name, song) in &songs {
        let folder = song.parent().unwrap_or(song);
        match engine.mcp.run(
            "SESSION_PREPARE",
            &json!({"path": folder.display().to_string()}),
        ) {
            Ok(rows) => {
                let row = rows
                    .as_array()
                    .and_then(|r| r.first())
                    .cloned()
                    .unwrap_or(Value::Null);
                let proxies = row["proxies_made"].as_u64().unwrap_or(0)
                    + row["proxies_current"].as_u64().unwrap_or(0);
                checks.add(
                    format!("{name} prepared"),
                    proxies > 0 && row["failed"].as_array().is_none_or(Vec::is_empty),
                    format!("{proxies} proxies: {row}"),
                );
            }
            Err(why) => checks.failed(format!("{name} prepared"), why),
        }
    }
    // The first song's proxies in the newer layout (`Media/Proxy/<rel>.ogg`,
    // song-file's `media`), the others where `SESSION_PREPARE` writes them
    // (`Media/Proxies/<stem>.ogg`): a phone's copy plays from either.
    if let Some((name, song)) = songs.first() {
        let moved = mirror_proxies(song.parent().unwrap_or(song));
        checks.add(
            format!("{name} in the mirrored layout"),
            matches!(moved, Ok(n) if n > 0),
            format!("{moved:?} proxy files moved to Media/Proxy/"),
        );
    }
    let link = ShareLink::serve(&source.join("sessions"), &songs);
    let url = link.url();
    let mut setlists = serde_json::Map::new();
    for (keep, folder) in [("full", "desktop"), ("proxies", "phone")] {
        let to = run.join(folder);
        match download_one(&engine.mcp, &url, &to, keep) {
            Ok(done) => {
                judge_download(keep, &done, &mut checks);
                if let Some(setlist) = done["setlist"].as_str() {
                    setlists.insert(keep.to_owned(), json!(setlist));
                }
            }
            Err(why) => checks.failed(format!("{keep} downloaded"), why),
        }
    }
    let served = link.stop();
    checks.add(
        "served over the link",
        served > 0,
        format!("{served} requests answered"),
    );
    // Nothing of the source is left to fall back on.
    let removed = std::fs::remove_dir_all(&source);
    checks.add(
        "source gone",
        removed.is_ok() && !source.exists(),
        format!("{} deleted", source.display()),
    );
    let plan = json!({"link": url, "setlists": setlists});
    let written = std::fs::write(run.join("offline.json"), format!("{plan:#}\n"));
    checks.add(
        "plan written",
        written.is_ok(),
        format!("{}", run.join("offline.json").display()),
    );
    crate::finish(
        "offline-setlist",
        "a setlist downloaded from its share link, as a desktop and as a phone keep it",
        engine,
        &checks,
    );
}

/// Move every proxy (and its page index) under `folder`'s
/// `Media/Proxies/` into the mirrored layout, `Media/Proxy/`. How many
/// files moved.
fn mirror_proxies(folder: &Path) -> Result<usize, String> {
    let old = folder.join("Media").join("Proxies");
    let new = folder.join("Media").join("Proxy");
    std::fs::create_dir_all(&new).map_err(|e| e.to_string())?;
    let mut moved = 0;
    for entry in std::fs::read_dir(&old)
        .map_err(|e| format!("{}: {e}", old.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.is_file() {
            let to = new.join(entry.file_name());
            std::fs::rename(&path, &to).map_err(|e| format!("{}: {e}", path.display()))?;
            moved += 1;
        }
    }
    Ok(moved)
}

/// `SETLIST_DOWNLOAD` one way, waited on: `SETLIST_DOWNLOADING` once
/// done.
fn download_one(mcp: &crate::mcp::Mcp, link: &str, to: &Path, keep: &str) -> Result<Value, String> {
    mcp.run(
        "SETLIST_DOWNLOAD",
        &json!({"from": link, "to": to.display().to_string(), "keep": keep}),
    )?;
    let began = Instant::now();
    loop {
        let state = mcp.run("SETLIST_DOWNLOADING", &json!({}))?;
        match state["state"].as_str() {
            Some("done") => return Ok(state),
            Some("failed") => return Err(format!("the download failed: {state}")),
            _ => {}
        }
        if began.elapsed() > DOWNLOAD_WAIT {
            return Err(format!("not downloaded after {DOWNLOAD_WAIT:?}: {state}"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// What a download kept, checked against what it was asked to keep.
fn judge_download(keep: &str, done: &Value, checks: &mut Checks) {
    let songs = done["downloaded"].as_array().cloned().unwrap_or_default();
    checks.add(
        format!("{keep}: every song"),
        songs.len() == SONGS.len() && done["setlist"].as_str().is_some(),
        format!(
            "{} of {} songs, setlist {}",
            songs.len(),
            SONGS.len(),
            done["setlist"]
        ),
    );
    for song in &songs {
        let title = song["title"].as_str().unwrap_or_default();
        let recordings = song["recordings"].as_u64().unwrap_or(0);
        let proxies = song["proxies"].as_u64().unwrap_or(0);
        let (ok, wanted) = match keep {
            "full" => (recordings > 0 && proxies > 0, "its recordings and proxies"),
            _ => (recordings == 0 && proxies > 0, "its proxies, no recording"),
        };
        checks.add(
            format!("{keep}: {title} kept"),
            ok,
            format!(
                "{recordings} recordings, {proxies} proxies, {} files, {} bytes (wanted {wanted})",
                song["files"], song["bytes"]
            ),
        );
    }
}

/// With the network off: each downloaded setlist opened from disk, its
/// songs moved through, played and listened to.
fn played_offline(run: &Path) {
    let mut checks = Checks::default();
    let plan: Value = std::fs::read_to_string(run.join("offline.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    if cfg!(target_os = "linux") {
        let (ok, said) = no_route_out();
        checks.add("no route out", ok, said);
    }
    let link = plan["link"].as_str().unwrap_or_default();
    let answers = link_answers(link);
    checks.add(
        "the link is gone",
        answers.is_err(),
        answers.map_or_else(|e| e, |_| format!("{link} still answers")),
    );
    let (engine, _spans) = crate::engine("offline-setlist-played");
    for keep in ["full", "proxies"] {
        let Some(setlist) = plan["setlists"][keep].as_str() else {
            checks.failed(format!("{keep}: opens"), "the download wrote no setlist");
            continue;
        };
        if let Err(why) = play_setlist(&engine, Path::new(setlist), keep, &mut checks) {
            checks.failed(format!("{keep}: setlist"), why);
        }
    }
    crate::finish(
        "offline-setlist-played",
        "a downloaded setlist opens and plays with the network off",
        engine,
        &checks,
    );
}

fn play_setlist(
    engine: &crate::engine::Engine,
    setlist: &Path,
    keep: &str,
    checks: &mut Checks,
) -> Result<(), String> {
    let mcp = &engine.mcp;
    let songs = check::open(engine, setlist)?;
    checks.add(
        format!("{keep}: opens"),
        songs.len() == SONGS.len(),
        format!("{} of {} songs open", songs.len(), SONGS.len()),
    );
    for (to, name) in (1..=SONGS.len()).zip(SONGS) {
        if to > 1 {
            mcp.run("SESSION_SONG", &json!({"to": "next"}))?;
        }
        match check::plays(mcp, 0.0) {
            Ok(said) => checks.add(format!("{keep}: song {to} plays"), true, said),
            Err(why) => checks.failed(format!("{keep}: song {to} plays"), why),
        }
        let fixture = Fixture::load(name)?;
        let listen: Vec<(String, String, String, f64, f64)> = if fixture.listen.is_empty() {
            vec![("Bass".into(), "0s".into(), "4s".into(), -60.0, 0.0)]
        } else {
            fixture
                .listen
                .iter()
                .map(|l| {
                    (
                        l.track.clone(),
                        l.from.clone(),
                        l.to.clone(),
                        l.lufs_min,
                        l.lufs_max,
                    )
                })
                .collect()
        };
        for (track, from, until, min, max) in listen {
            let label = format!("{keep}: {name} {track} heard");
            match check::lufs(mcp, &track, &from, &until) {
                Ok(Some(v)) => checks.add(
                    label,
                    (min..=max).contains(&v),
                    format!("{v:.1} LUFS over {from}..{until} (wanted {min}..{max})"),
                ),
                Ok(None) => checks.failed(label, "LISTEN gave no loudness: silence"),
                Err(why) => checks.failed(label, why),
            }
        }
    }
    Ok(())
}

/// What there is to reach off this machine: nothing, if each fails.
fn no_route_out() -> (bool, String) {
    let tcp = TcpStream::connect_timeout(
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

/// Whether `url`'s listing answers: a plain HTTP GET of
/// `<link>_files.json`.
fn link_answers(url: &str) -> Result<String, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("{url}: not http"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let addr: std::net::SocketAddr = host.parse().map_err(|e| format!("{host}: {e}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|e| format!("{host}: {e}"))?;
    write!(
        stream,
        "GET /{path}_files.json HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| e.to_string())?;
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .map_err(|e| e.to_string())?;
    Ok(answer.lines().next().unwrap_or_default().to_owned())
}

/// A share link serving the staged songs, as a node serves one: its
/// listing at `/s/<code>/_files.json` and each file at its own URL, by
/// `Range` when asked.
struct ShareLink {
    addr: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    served: Arc<std::sync::atomic::AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ShareLink {
    /// Serve each of `songs` (its fixture name and its song file) from
    /// `root`, the folder they were staged in.
    fn serve(root: &Path, songs: &[(&str, PathBuf)]) -> Self {
        let listing = listing(root, songs);
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port for the share link");
        let addr = listener.local_addr().expect("the link's address");
        let stop = Arc::new(AtomicBool::new(false));
        let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let root = root.to_path_buf();
        let thread = {
            let stop = Arc::clone(&stop);
            let served = Arc::clone(&served);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let listing = listing.clone();
                    let root = root.clone();
                    let served = Arc::clone(&served);
                    std::thread::spawn(move || {
                        if answer(stream, &root, &listing).is_ok() {
                            served.fetch_add(1, Ordering::SeqCst);
                        }
                    });
                }
            })
        };
        Self {
            addr,
            stop,
            served,
            thread: Some(thread),
        }
    }

    fn url(&self) -> String {
        format!("http://{}/s/{CODE}/", self.addr)
    }

    /// Stop serving: the link no longer answers. How many requests it
    /// answered.
    fn stop(mut self) -> usize {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.served.load(Ordering::SeqCst)
    }
}

/// The link's listing: one note per song attaching its prepared
/// `.session` (its recordings named relative to its folder), and every
/// file under each song's folder but the session's log, with its size and
/// URL.
fn listing(root: &Path, songs: &[(&str, PathBuf)]) -> Value {
    let mut notes = Vec::new();
    let mut files = Vec::new();
    for (name, song) in songs {
        let folder = song.parent().unwrap_or(song);
        let mut under = Vec::new();
        walk(folder, &mut under);
        let (mut session, mut rpp) = (None, None);
        for file in under {
            let rel = file
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let lower = rel.to_lowercase();
            // A `.session`'s log (`-wal`, `-shm`, `-tshm`) is the engine's
            // own, never served (`song_files::folder`).
            if lower.contains(".session-") {
                continue;
            }
            if lower.ends_with(".session") {
                session = Some(rel.clone());
            } else if lower.ends_with(".rpp") {
                rpp = Some(rel.clone());
            }
            let size = std::fs::metadata(&file).map_or(0, |m| m.len());
            files.push(json!({
                "path": rel,
                "size": size,
                "url": format!("/s/{CODE}/f/{}", rel.replace(' ', "%20")),
                "layer": "full",
            }));
        }
        // The prepared `.session`, as a node serves a song, else the `.RPP`.
        let project = session.or(rpp);
        notes.push(json!({
            "path": format!("Songs/{name}.md"),
            "title": name,
            "type": "song",
            "files": project.into_iter().collect::<Vec<_>>(),
        }));
    }
    json!({
        "link": format!("/s/{CODE}/"),
        "title": NAME,
        "listen": "full",
        "notes": notes,
        "files": files,
    })
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Answer one request on `stream`, then close it.
fn answer(stream: TcpStream, root: &Path, listing: &Value) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let mut range = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("range")
        {
            range = parse_range(value.trim());
        }
    }
    let mut stream = stream;
    let files = format!("/s/{CODE}/f/");
    if path == format!("/s/{CODE}/_files.json") {
        let body = listing.to_string();
        return respond(
            &mut stream,
            "200 OK",
            "application/json",
            None,
            body.as_bytes(),
        );
    }
    let Some(rel) = path.strip_prefix(&files) else {
        return respond(
            &mut stream,
            "404 Not Found",
            "text/plain",
            None,
            b"not here",
        );
    };
    let rel = rel.replace("%20", " ");
    if rel.split('/').any(|p| p == "..") {
        return respond(&mut stream, "403 Forbidden", "text/plain", None, b"no");
    }
    let Ok(bytes) = std::fs::read(root.join(&rel)) else {
        return respond(
            &mut stream,
            "404 Not Found",
            "text/plain",
            None,
            b"not here",
        );
    };
    match range {
        Some((start, end)) if start < bytes.len() as u64 => {
            let last = end.unwrap_or(u64::MAX).min(bytes.len() as u64 - 1);
            let (from, to) = (
                usize::try_from(start).unwrap_or(usize::MAX),
                usize::try_from(last).unwrap_or(usize::MAX),
            );
            let span = format!("bytes {start}-{last}/{}", bytes.len());
            respond(
                &mut stream,
                "206 Partial Content",
                "application/octet-stream",
                Some(&span),
                &bytes[from..=to],
            )
        }
        _ => respond(
            &mut stream,
            "200 OK",
            "application/octet-stream",
            None,
            &bytes,
        ),
    }
}

/// `bytes=<start>-<end>` (the end optional).
fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let spec = value.strip_prefix("bytes=")?;
    let (start, end) = spec.split_once('-')?;
    Some((start.trim().parse().ok()?, end.trim().parse().ok()))
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    kind: &str,
    range: Option<&str>,
    body: &[u8],
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(range) = range {
        head.push_str(&format!("Content-Range: {range}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
