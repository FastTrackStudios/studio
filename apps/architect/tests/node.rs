//! The point of Studio Architect, on a real node: a setlist shared from a
//! vault draws its songs' charts as Keyflow's follow SVG on Studio's
//! node, where the stock Architect node shows the chart's Keyflow text
//! (FastTrackStudio/architect#547).
//!
//! Each test starts a node the way `<bin> serve` does
//! (`architect_desktop::node::main`: the distribution's `blocks`, then
//! `node::start_with`) on loopback, with a temp data dir and a temp vault
//! holding two songs with charts, a setlist of them, and its share link
//! as the note `SHARE_CREATE` writes (`architect_spaces::share_note`,
//! what `STUDIO_SETLISTS_SHARE` asks for: the setlist's note and its
//! songs'). The song's page is fetched over HTTP. Every wait is bounded;
//! the node is stopped at the end and dropped (its tasks aborted) on a
//! failure.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::arithmetic_side_effects,
    clippy::as_conversions
)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use architect_desktop::Distribution;
use architect_desktop::node::{self, Running, config::Config};
use architect_spaces::share::{code_from, share_note_path};
use architect_spaces::{Audience, Listen, Share, ShareAccess, ShareScope, share_note};
use setlists::chart::{SongFiles, import_song};
use setlists::layout::{Library, SongRef, setlist_note};
use setlists::setlist::Setlist;
use setlists::vault::Vault;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// Franz Gruber's tune (1818).
fn silent_night() -> String {
    let verse = "VS 12\nA A A A\nE E A A\nD D A A\n\n";
    format!(
        "Silent Night - Franz Gruber\n72bpm 3/4 #A\n\\downbeat = 0.5\n\n{}",
        verse.repeat(4)
    )
}

/// Isaac Watts' text (1719), on a timed lyric lane.
const JOY: &str = "Joy to the World - Isaac Watts\n100bpm 2/4 #D\n\\downbeat = 1\n\nVS 8\nD D A D\nD G A D\n[lyrics lrc]\n@1 Joy @1:2 to @2 the @2:2 world @3 the @3:2 Lord @4 is @4:2 come\n@5 Let @5:2 earth @6 re- @6:2 ceive @7 her @8 King\n";

/// A line of Silent Night's chart only its Keyflow text has.
const CHART_TEXT: &str = "72bpm 3/4 #A";

/// One second of a 440 Hz sine, 16-bit mono WAV.
fn sine_wav() -> Vec<u8> {
    let rate: u32 = 8_000;
    let mut wav = Vec::new();
    let data = rate * 2;
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    for n in 0..rate {
        let t = f64::from(n) / f64::from(rate);
        #[allow(clippy::cast_possible_truncation)]
        let v = ((t * 440.0 * std::f64::consts::TAU).sin() * 8000.0) as i16;
        wav.extend_from_slice(&v.to_le_bytes());
    }
    wav
}

/// The vault id the node holds the vault as.
const VAULT: &str = "main";

/// A vault with two songs and a setlist of them, shared by a link; its
/// share code.
fn shared_setlist(root: &Path, files: &Path) -> String {
    let put = |name: &str, bytes: &[u8]| {
        let path = files.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };
    let vault = Vault::at(root);
    let lib = Library::named("Christmas");
    let kf = put("silent.kf", silent_night().as_bytes());
    let wav = put("Silent Night.wav", &sine_wav());
    let silent = import_song(
        &vault,
        &lib,
        &SongFiles {
            chart: Some(&kf),
            media: &[&wav],
            ..SongFiles::default()
        },
        None,
    )
    .unwrap();
    let kf = put("joy.kf", JOY.as_bytes());
    let wav = put("Joy to the World.wav", &sine_wav());
    let joy = import_song(
        &vault,
        &lib,
        &SongFiles {
            chart: Some(&kf),
            media: &[&wav],
            ..SongFiles::default()
        },
        None,
    )
    .unwrap();
    let at = |s: &str| SongRef::new("Christmas", s, None);
    let mut set = Setlist::new("Tree Lighting", Vec::new());
    set.add(at("Joy to the World"), None, Some("Opening"))
        .unwrap();
    set.add(at("Silent Night"), None, Some("Opening")).unwrap();
    let known = [(at("Joy to the World"), joy), (at("Silent Night"), silent)];
    vault
        .write(&setlist_note(&set.title), set.note(&known).as_bytes())
        .unwrap();

    // The share STUDIO_SETLISTS_SHARE asks SHARE_CREATE for: the
    // setlist's note and its songs', for anyone with the link.
    let code = code_from(*b"studiotest");
    let share = Share {
        code: code.clone(),
        vault: VAULT.to_owned(),
        scope: ShareScope::Notes(vec![
            setlist_note(&set.title),
            lib.song_note("Joy to the World"),
            lib.song_note("Silent Night"),
        ]),
        title: Some(set.title.clone()),
        audience: Audience::Link,
        access: ShareAccess::Read,
        expires_at: None,
        revoked_at: None,
        created_at: 1_790_000_000,
        created_by: "studio-architect-test".to_owned(),
        domain: None,
        audiences: Vec::new(),
        // What SHARE_CREATE gives a new link (architect v0.12.2, #528).
        listen: Listen::Reference,
    };
    let path = root.join(share_note_path(&code));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, share_note(&share)).unwrap();
    code
}

/// `dist`'s node on loopback, as `<bin> serve` starts it, over `vault`.
async fn serve(dist: &Distribution, data: &Path, vault: &Path) -> Running {
    let env: HashMap<&str, String> = HashMap::from([
        ("ARCHITECT_BIND", "127.0.0.1:0".to_owned()),
        ("ARCHITECT_DATA_DIR", data.display().to_string()),
        ("ARCHITECT_VAULTS", format!("{VAULT}={}", vault.display())),
        ("ARCHITECT_IROH", "local".to_owned()),
        ("ARCHITECT_NAME", "studio-architect-test".to_owned()),
    ]);
    let config = Config::load(None, |k| env.get(k).cloned()).unwrap();
    let blocks = dist.blocks(&config.data_dir.join("linked"));
    let start = node::start_with_islands(config, blocks, dist.share_layers(), dist.islands());
    tokio::time::timeout(Duration::from_secs(60), start)
        .await
        .expect("the node started within a minute")
        .unwrap()
}

/// `GET path` from the node: its status and body.
async fn get(node: SocketAddr, path: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(node).await.unwrap();
    let req = format!("GET {path} HTTP/1.0\r\nHost: {node}\r\nAccept: text/html\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(30), stream.read_to_end(&mut bytes))
        .await
        .expect("the node answered within 30 s")
        .unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, body.to_owned())
}

/// `page` of the share, once the node has built it with `marker` on it
/// (bounded).
async fn page(node: SocketAddr, page: &str, marker: &str, code: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let (status, body) = get(node, page).await;
        if status == 200 && body.contains(marker) {
            return body;
        }
        assert!(
            Instant::now() < deadline,
            "{page} was not built within 90 s: last {status}\n{}\nthe link's index: {:?}",
            &body[..body.len().min(2000)],
            get(node, &format!("/s/{code}/")).await,
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The page of `dist`'s node for the shared setlist's song.
async fn shared_song_on(dist: &Distribution) -> String {
    shared_page_on(
        dist,
        "libraries/christmas/silent-night/silent-night/",
        "Silent Night",
    )
    .await
}

/// The page at `rest` of the shared setlist on `dist`'s node, once it
/// holds `marker` (bounded).
async fn shared_page_on(dist: &Distribution, rest: &str, marker: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let (data, vault, files) = (
        dir.path().join("data"),
        dir.path().join("vault"),
        dir.path().join("files"),
    );
    for d in [&data, &vault, &files] {
        std::fs::create_dir_all(d).unwrap();
    }
    let code = shared_setlist(&vault, &files);
    let node = serve(dist, &data, &vault).await;
    let html = page(node.http, &format!("/s/{code}/{rest}"), marker, &code).await;
    node.stop().await;
    html
}

/// Every `name="…"` value in `html`, in document order.
fn values<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let needle = format!(" {name}=\"");
    html.match_indices(&needle)
        .filter_map(|(at, _)| {
            let rest = &html[at + needle.len()..];
            rest.find('"').map(|end| &rest[..end])
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn studios_node_draws_a_shared_chart_as_the_follow_svg() {
    let html = shared_song_on(&studio_architect::distribution()).await;
    let start = html
        .find("data-by=\"studio.setlists\"")
        .unwrap_or_else(|| panic!("Studio's renderer drew no block:\n{html}"));
    let chart = &html[start..];
    let svg = &chart[..chart
        .find("</svg>")
        .unwrap_or_else(|| panic!("the chart is not an SVG:\n{chart}"))];
    assert!(svg.contains("<svg"), "the chart is not an SVG:\n{svg}");
    assert!(!svg.contains("<img"), "the chart is an image");
    // Its bars and its beats, timed on the chart's own clock, ascending:
    // what the page player marks as the recording plays (a box each: it
    // marks one item per box).
    assert_eq!(values(svg, "data-follow-playhead"), ["", ""]);
    let bars = &svg[svg.find("class=\"kf-bars\"").expect("no bars box")..];
    let bars = &bars[..bars.find("class=\"kf-beats\"").expect("no beats box")];
    let at: Vec<f64> = values(bars, "data-at")
        .iter()
        .map(|v| v.parse().unwrap())
        .collect();
    assert!(
        at.len() >= 48,
        "{} timed bars, want the chart's 48",
        at.len()
    );
    assert!(
        at.windows(2).all(|w| w[0] <= w[1]),
        "bar times out of order: {at:?}"
    );
    assert!(
        !values(svg, "data-bar").is_empty(),
        "no data-bar on the chart's bars"
    );
    assert!(
        values(svg, "data-media")
            .iter()
            .any(|m| m.ends_with("Silent%20Night.wav")),
        "the chart is not tied to its recording"
    );
    assert!(
        !html.contains(CHART_TEXT),
        "the chart's Keyflow text is on the page too"
    );
    // Nor is the chart an engraved page (the song's `pages` PNGs).
    let chart_images = html
        .split("<img")
        .skip(1)
        .filter(|tag| tag[..tag.find('>').unwrap_or(tag.len())].contains(".chart-"))
        .count();
    assert_eq!(chart_images, 0, "the page shows the chart as a PNG");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stock_node_shows_the_same_chart_as_its_keyflow_text() {
    let html = shared_song_on(&architect_desktop::distribution::architect()).await;
    assert!(
        html.contains(CHART_TEXT),
        "the stock node did not show the chart's text:\n{html}"
    );
    assert!(!html.contains("studio.setlists"));
    assert_eq!(values(&html, "data-bar"), Vec::<&str>::new());
}

#[test]
fn it_is_studio_architect_with_studios_extensions_compiled_in() {
    let d = studio_architect::distribution().describe();
    assert_eq!(d["name"], "Studio Architect");
    assert_eq!(d["app"], "studio-architect");
    assert_eq!(d["folder"], "studio-architect");
    assert_eq!(d["bundle_id"], "app.fasttrackstudio.studio-architect");
    let ids: Vec<&str> = d["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    for id in ["studio-setlists", "studio-productions"] {
        assert!(ids.contains(&id), "{id} is not compiled in: {ids:?}");
    }
    // Still the Architect app: its first-party extensions are there.
    let stock = architect_desktop::distribution::architect().describe();
    for e in stock["extensions"].as_array().unwrap() {
        assert!(ids.contains(&e["id"].as_str().unwrap()), "{e} is missing");
    }
    assert_ne!(d["bundle_id"], stock["bundle_id"]);
}

// Studio's island (#445): the folder beside the binary, checked against
// its lock. The page's player reads `<script id="site-islands">` (the
// node writes it on a page with recordings, its `islands` list empty when
// it has none) and draws Architect's own bar and controls without one.

/// The islands the page offers its now-playing view: their ids.
fn islands_on(html: &str) -> Vec<String> {
    let open = "<script type=\"application/json\" id=\"site-islands\">";
    let Some(at) = html.find(open) else {
        return Vec::new();
    };
    let rest = &html[at + open.len()..];
    let json: serde_json::Value =
        serde_json::from_str(&rest[..rest.find("</script>").unwrap()].replace("<\\/", "</"))
            .unwrap();
    json["islands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_owned())
        .collect()
}

/// The setlist's page (a playlist: it has recordings) on `dist`'s node.
async fn setlist_page_on(dist: &Distribution) -> String {
    shared_page_on(dist, "setlists/tree-lighting/", "Silent Night").await
}

/// A built island folder as `build-web` writes it: its files and a lock
/// naming each with its digest, and the stylesheet beside them.
fn island_folder(dir: &Path) {
    use base64::Engine as _;
    use sha2::Digest as _;
    let files: [(&str, &[u8]); 3] = [
        ("architect.json", br#"{"module":"studio_island.js"}"#),
        ("studio_island.js", b"export const instantiate = () => {};"),
        ("studio_island.core.wasm", b"\0asm\x01\0\0\0"),
    ];
    let mut lock = serde_json::Map::new();
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
        let digest = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(bytes));
        lock.insert(name.to_owned(), format!("sha256-{digest}").into());
    }
    let lock = serde_json::json!({ "lockfileVersion": 1, "files": lock });
    std::fs::write(dir.join("architect.lock.json"), lock.to_string()).unwrap();
    std::fs::write(
        dir.join("island.css"),
        b".studio-island-progress{display:flex}",
    )
    .unwrap();
}

#[test]
fn a_folder_matching_its_lock_is_the_island() {
    let dir = tempfile::tempdir().unwrap();
    island_folder(dir.path());
    let island = studio_architect::island::at(dir.path()).unwrap();
    assert_eq!(island.id(), "studio.transport");
    assert!(island.integrity().unwrap().starts_with("sha256-"));
    assert!(island.stylesheet_integrity().is_some());
    assert!(island.file("studio_island.js").is_some());
    // Only what the lock names, the lock and the sheet are served.
    std::fs::write(dir.path().join("stray.js"), "alert(1)").unwrap();
    let island = studio_architect::island::at(dir.path()).unwrap();
    assert!(island.file("stray.js").is_none());
}

#[test]
fn a_folder_that_does_not_match_its_lock_is_refused_whole() {
    use studio_architect::island::{IslandError, at};
    let none = tempfile::tempdir().unwrap();
    assert!(matches!(
        at(&none.path().join("studio-island")),
        Err(IslandError::Absent(_))
    ));

    let missing = tempfile::tempdir().unwrap();
    island_folder(missing.path());
    std::fs::remove_file(missing.path().join("studio_island.core.wasm")).unwrap();
    assert!(
        matches!(at(missing.path()), Err(IslandError::Missing(f)) if f == "studio_island.core.wasm")
    );

    let altered = tempfile::tempdir().unwrap();
    island_folder(altered.path());
    std::fs::write(altered.path().join("studio_island.js"), "fetch('//evil')").unwrap();
    assert!(matches!(at(altered.path()), Err(IslandError::Altered(f)) if f == "studio_island.js"));

    let outside = tempfile::tempdir().unwrap();
    island_folder(outside.path());
    std::fs::write(
        outside.path().join("architect.lock.json"),
        r#"{"files":{"../../etc/passwd":"sha256-x"}}"#,
    )
    .unwrap();
    assert!(matches!(at(outside.path()), Err(IslandError::Path(_))));

    let garbled = tempfile::tempdir().unwrap();
    island_folder(garbled.path());
    std::fs::write(garbled.path().join("architect.lock.json"), "{").unwrap();
    assert!(matches!(at(garbled.path()), Err(IslandError::Lock(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_with_the_island_offers_it_to_the_now_playing_view() {
    let dir = tempfile::tempdir().unwrap();
    island_folder(dir.path());
    let dist = studio_architect::distribution_with(studio_architect::island::at(dir.path()).ok());
    let html = setlist_page_on(&dist).await;
    assert_eq!(islands_on(&html), ["studio.transport"], "{html}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_without_the_folder_offers_no_island() {
    // The test binary has no `studio-island/` beside it: as a node shipped
    // without one.
    assert!(studio_architect::island().is_none());
    let html = setlist_page_on(&studio_architect::distribution()).await;
    assert!(
        html.contains("id=\"site-islands\""),
        "not a page with recordings:\n{html}"
    );
    assert_eq!(islands_on(&html), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_whose_folder_was_altered_offers_no_island() {
    let dir = tempfile::tempdir().unwrap();
    island_folder(dir.path());
    std::fs::write(
        dir.path().join("studio_island.core.wasm"),
        b"\0asm\x01\0\0\0tampered",
    )
    .unwrap();
    let dist = studio_architect::distribution_with(studio_architect::island::at(dir.path()).ok());
    let html = setlist_page_on(&dist).await;
    assert!(
        html.contains("id=\"site-islands\""),
        "not a page with recordings:\n{html}"
    );
    assert_eq!(islands_on(&html), Vec::<String>::new());
}
