//! Desktop Studio updating itself (`studio_shell::updates`), against a
//! fake releases feed on 127.0.0.1 signed by a throwaway minisign key: a
//! signed newer release replaces the installed `AppImage` and moves the
//! `studio` link and the desktop entry to it; a release signed by another
//! key, signed for another tag, or whose download does not match its sums
//! changes nothing. Plus the honest pieces: which release is picked, what
//! is installed and whether it may update, the bundle swap.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::missing_panics_doc,
    // The test BLAKE2b: wrapping word arithmetic and the counter's halves.
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::format_collect,
    clippy::option_if_let_else,
    clippy::items_after_statements,
    clippy::missing_const_for_fn
)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use architect::release::{GithubAsset, GithubRelease};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signer as _, SigningKey};
use studio_shell::updates::{
    Block, Feed, Installation, LinuxLinks, Mode, Platform, Target, UpdateStatus,
    appimage_destination, desktop_entry_moved, install_block, pick, relink, run, swap_bundle,
    team_identifier,
};

const OLD: &[u8] = b"old studio appimage";
const NEW: &[u8] = b"new studio appimage, 26.12.1";
const LINUX: Platform = Platform::LinuxX86_64;

// ── A minisign signer ───────────────────────────────────────────────

/// A throwaway minisign key: signs like `minisign -S -t <comment>`.
struct TestKey {
    sk: SigningKey,
    keynum: [u8; 8],
}

impl TestKey {
    fn new(seed: u8) -> Self {
        Self {
            sk: SigningKey::from_bytes(&[seed; 32]),
            keynum: [seed; 8],
        }
    }

    /// The `.pub` file's base64 line.
    fn public(&self) -> String {
        let mut raw = b"Ed".to_vec();
        raw.extend_from_slice(&self.keynum);
        raw.extend_from_slice(self.sk.verifying_key().as_bytes());
        B64.encode(raw)
    }

    /// A `.minisig` over `msg` (prehashed, `ED`) with `comment`.
    fn sign(&self, msg: &[u8], comment: &str) -> String {
        let prehash = blake2b_512(msg);
        let sig = self.sk.sign(&prehash).to_bytes();
        let mut line = b"ED".to_vec();
        line.extend_from_slice(&self.keynum);
        line.extend_from_slice(&sig);
        let mut global = sig.to_vec();
        global.extend_from_slice(comment.as_bytes());
        let global = self.sk.sign(&global).to_bytes();
        format!(
            "untrusted comment: signature from minisign secret key\n{}\ntrusted comment: {comment}\n{}\n",
            B64.encode(line),
            B64.encode(global)
        )
    }
}

/// BLAKE2b-512 (RFC 7693), unkeyed: minisign's prehash. Written here
/// rather than taken from a crate so the lock gains no package; if it were
/// wrong, every good signature would fail to verify.
fn blake2b_512(msg: &[u8]) -> [u8; 64] {
    const IV: [u64; 8] = [
        0x6a09_e667_f3bc_c908,
        0xbb67_ae85_84ca_a73b,
        0x3c6e_f372_fe94_f82b,
        0xa54f_f53a_5f1d_36f1,
        0x510e_527f_ade6_82d1,
        0x9b05_688c_2b3e_6c1f,
        0x1f83_d9ab_fb41_bd6b,
        0x5be0_cd19_137e_2179,
    ];
    const SIGMA: [[usize; 16]; 12] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    ];
    // The RFC's names.
    #[allow(clippy::many_single_char_names)]
    fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(32);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(24);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(63);
    }
    let compress = |h: &mut [u64; 8], block: &[u8; 128], t: u128, last: bool| {
        let m: [u64; 16] = std::array::from_fn(|i| {
            u64::from_le_bytes(block[i * 8..i * 8 + 8].try_into().unwrap())
        });
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(h);
        v[8..].copy_from_slice(&IV);
        v[12] ^= t as u64;
        v[13] ^= (t >> 64) as u64;
        if last {
            v[14] = !v[14];
        }
        for s in &SIGMA {
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }
        for i in 0..8 {
            h[i] ^= v[i] ^ v[i + 8];
        }
    };
    let mut h = IV;
    h[0] ^= 0x0101_0000 ^ 64;
    let mut t: u128 = 0;
    let mut chunks = msg.chunks(128).peekable();
    if chunks.peek().is_none() {
        compress(&mut h, &[0; 128], 0, true);
    }
    while let Some(chunk) = chunks.next() {
        let mut block = [0u8; 128];
        block[..chunk.len()].copy_from_slice(chunk);
        t += chunk.len() as u128;
        compress(&mut h, &block, t, chunks.peek().is_none());
    }
    let mut out = [0u8; 64];
    for (i, w) in h.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&w.to_le_bytes());
    }
    out
}

#[test]
fn the_test_blake2b_matches_rfc_7693() {
    // RFC 7693 appendix A: BLAKE2b-512("abc").
    let hex: String = blake2b_512(b"abc")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        hex,
        "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1\
         7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
    );
}

fn hex_sha256(bytes: &[u8]) -> String {
    <sha2::Sha256 as sha2::Digest>::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ── The fake feed ───────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Fake {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

async fn serve_file(State(fake): State<Fake>, req: Request) -> Response {
    match fake.files.lock().unwrap().get(req.uri().path()) {
        Some(body) => body.clone().into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// What the newest release (`studio-v26.12.1`, a pre-release) carries.
struct Release {
    download: Vec<u8>,
    /// The sha256 its sums list for the download.
    listed: String,
    /// Who signs the sums, and for which tag.
    signer: TestKey,
    signed_for: &'static str,
}

impl Release {
    fn good() -> Self {
        Self {
            download: NEW.to_vec(),
            listed: hex_sha256(NEW),
            signer: TestKey::new(7),
            signed_for: "studio-v26.12.1",
        }
    }
}

/// A feed with an older stable release, the newest pre-release and a
/// pre-monorepo `v` tag that is higher but not Studio's (`studio-v`) tag.
async fn start(release: Release) -> Feed {
    let fake = Fake::default();
    let app = axum::Router::new()
        .fallback(serve_file)
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let tag = "studio-v26.12.1";
    let asset = LINUX.asset("26.12.1");
    let sums = format!(
        "{}  {}\n{}  {asset}\n",
        "b".repeat(64),
        Platform::MacosArm64.asset("26.12.1"),
        release.listed
    );
    let comment = format!("studio {} SHA256SUMS", release.signed_for);
    let sig = release.signer.sign(sums.as_bytes(), &comment);
    let a = |tag: &str, name: &str| {
        format!(r#"{{"name":"{name}","browser_download_url":"{base}/dl/{tag}/{name}"}}"#)
    };
    let list = format!(
        r#"[
          {{"tag_name":"v99.0.0","draft":false,"prerelease":false,"assets":[{}]}},
          {{"tag_name":"{tag}","draft":false,"prerelease":true,"html_url":"x","assets":[{},{},{}]}},
          {{"tag_name":"studio-v26.11.4","draft":false,"prerelease":false,"assets":[{},{}]}}
        ]"#,
        a("v99.0.0", &LINUX.asset("99.0.0")),
        a(tag, &asset),
        a(tag, "SHA256SUMS"),
        a(tag, "SHA256SUMS.minisig"),
        a("studio-v26.11.4", &LINUX.asset("26.11.4")),
        a("studio-v26.11.4", "SHA256SUMS"),
    );
    let mut files = fake.files.lock().unwrap();
    files.insert(
        "/repos/FastTrackStudios/studio/releases".into(),
        list.into_bytes(),
    );
    files.insert(format!("/dl/{tag}/{asset}"), release.download);
    files.insert(format!("/dl/{tag}/SHA256SUMS"), sums.into_bytes());
    files.insert(format!("/dl/{tag}/SHA256SUMS.minisig"), sig.into_bytes());
    drop(files);
    Feed {
        api: base,
        repo: "FastTrackStudios/studio".into(),
        prereleases: true,
    }
}

/// An `AppImage` installed as `scripts/install.sh` installs one: the
/// versioned file, the `studio` link to it, the desktop entry naming it.
struct Installed {
    dir: tempfile::TempDir,
    image: PathBuf,
    links: LinuxLinks,
}

impl Installed {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("share/studio");
        let bin = dir.path().join("bin");
        let apps = dir.path().join("applications");
        for d in [&home, &bin, &apps] {
            std::fs::create_dir_all(d).unwrap();
        }
        let image = home.join(LINUX.asset("26.11.4"));
        std::fs::write(&image, OLD).unwrap();
        let link = bin.join("studio");
        std::os::unix::fs::symlink(&image, &link).unwrap();
        let desktop = apps.join("app.fasttrackstudio.studio.desktop");
        std::fs::write(
            &desktop,
            format!(
                "[Desktop Entry]\nName=Studio\nExec={}\nTerminal=false\n",
                image.display()
            ),
        )
        .unwrap();
        Self {
            dir,
            image,
            links: LinuxLinks {
                bin: Some(link),
                desktop: Some(desktop),
            },
        }
    }

    fn home(&self) -> PathBuf {
        self.image.parent().unwrap().to_owned()
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.home())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Still exactly as installed: the old file alone, the links on it.
    fn assert_unchanged(&self) {
        assert_eq!(self.files(), vec![LINUX.asset("26.11.4")]);
        assert_eq!(std::fs::read(&self.image).unwrap(), OLD);
        assert_eq!(
            std::fs::read_link(self.links.bin.as_ref().unwrap()).unwrap(),
            self.image
        );
    }
}

async fn update(feed: &Feed, installed: &Installed, key: &str, mode: Mode) -> UpdateStatus {
    let installation = Installation::AppImage(installed.image.clone());
    let scratch = installed.dir.path().join("cache");
    run(
        feed,
        Target {
            current: "26.11.4",
            installation: &installation,
            platform: Some(LINUX),
            key: Some(key),
            released: true,
            scratch: &scratch,
            links: &installed.links,
        },
        mode,
    )
    .await
}

// ── Installing ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_signed_newer_release_replaces_the_appimage_and_moves_its_links() {
    let release = Release::good();
    let key = release.signer.public();
    let feed = start(release).await;
    let installed = Installed::new();

    let status = update(&feed, &installed, &key, Mode::Install).await;
    assert_eq!(
        status,
        UpdateStatus::Installed {
            to: "26.12.1".into()
        }
    );

    let new = installed.home().join(LINUX.asset("26.12.1"));
    assert_eq!(
        installed.files(),
        vec![LINUX.asset("26.12.1")],
        "the old AppImage is gone"
    );
    assert_eq!(std::fs::read(&new).unwrap(), NEW);
    use std::os::unix::fs::PermissionsExt as _;
    assert_eq!(
        std::fs::metadata(&new).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::read_link(installed.links.bin.as_ref().unwrap()).unwrap(),
        new
    );
    let entry = std::fs::read_to_string(installed.links.desktop.as_ref().unwrap()).unwrap();
    assert!(
        entry.contains(&format!("Exec={}\n", new.display())),
        "{entry}"
    );
    assert!(
        entry.contains("Terminal=false"),
        "the rest of the entry is kept: {entry}"
    );
}

#[tokio::test]
async fn a_check_reports_the_newest_prerelease_and_changes_nothing() {
    let release = Release::good();
    let key = release.signer.public();
    let feed = start(release).await;
    let installed = Installed::new();

    let status = update(&feed, &installed, &key, Mode::Check).await;
    assert_eq!(
        status,
        UpdateStatus::Available {
            newest: "26.12.1".into()
        }
    );
    installed.assert_unchanged();
}

#[tokio::test]
async fn sums_signed_by_another_key_change_nothing() {
    let release = Release {
        signer: TestKey::new(9),
        ..Release::good()
    };
    let feed = start(release).await;
    let installed = Installed::new();

    let status = update(&feed, &installed, &TestKey::new(7).public(), Mode::Install).await;
    assert!(
        matches!(&status, UpdateStatus::Failed(e) if e.contains("not signed")),
        "{status:?}"
    );
    installed.assert_unchanged();
}

#[tokio::test]
async fn a_signature_lifted_from_another_release_changes_nothing() {
    let release = Release {
        signed_for: "studio-v26.11.4",
        ..Release::good()
    };
    let key = release.signer.public();
    let feed = start(release).await;
    let installed = Installed::new();

    let status = update(&feed, &installed, &key, Mode::Install).await;
    assert!(
        matches!(&status, UpdateStatus::Failed(e) if e.contains("studio-v26.11.4")),
        "{status:?}"
    );
    installed.assert_unchanged();
}

#[tokio::test]
async fn a_download_that_does_not_match_its_signed_sum_changes_nothing() {
    let release = Release {
        download: b"swapped after signing".to_vec(),
        ..Release::good()
    };
    let key = release.signer.public();
    let feed = start(release).await;
    let installed = Installed::new();

    let status = update(&feed, &installed, &key, Mode::Install).await;
    assert!(
        matches!(&status, UpdateStatus::Failed(e) if e.contains("sha256")),
        "{status:?}"
    );
    installed.assert_unchanged();
}

#[tokio::test]
async fn the_running_version_is_current_and_a_development_build_never_installs() {
    let release = Release::good();
    let key = release.signer.public();
    let feed = start(release).await;
    let installed = Installed::new();
    let installation = Installation::AppImage(installed.image.clone());
    let scratch = installed.dir.path().join("cache");
    let target = |current, released| Target {
        current,
        installation: &installation,
        platform: Some(LINUX),
        key: Some(&key),
        released,
        scratch: &scratch,
        links: &installed.links,
    };

    let status = run(&feed, target("26.12.1", true), Mode::Install).await;
    assert_eq!(
        status,
        UpdateStatus::Current {
            newest: "26.12.1".into()
        }
    );
    let status = run(&feed, target("26.11.4", false), Mode::Install).await;
    assert_eq!(
        status,
        UpdateStatus::Blocked {
            newest: "26.12.1".into(),
            why: Block::NotRelease
        }
    );
    installed.assert_unchanged();
}

// ── The honest pieces ───────────────────────────────────────────────

fn rel(tag: &str, draft: bool, prerelease: bool, names: &[String]) -> GithubRelease {
    GithubRelease {
        tag_name: tag.into(),
        draft,
        prerelease,
        html_url: String::new(),
        assets: names
            .iter()
            .map(|n| GithubAsset {
                name: n.clone(),
                browser_download_url: format!("https://x/{tag}/{n}"),
            })
            .collect(),
    }
}

#[test]
fn the_newest_studio_release_with_this_platforms_asset_and_sums_is_picked() {
    let full = |v: &str| vec![LINUX.asset(v), "SHA256SUMS".into()];
    let releases = [
        rel("studio-v26.12.9", true, false, &full("26.12.9")),
        rel("studio-v26.12.3", false, false, &[LINUX.asset("26.12.3")]),
        rel(
            "studio-v26.12.2",
            false,
            false,
            &[Platform::MacosArm64.asset("26.12.2"), "SHA256SUMS".into()],
        ),
        rel("studio-v26.12.1", false, true, &full("26.12.1")),
        rel("v27.1.1", false, false, &full("27.1.1")),
        rel("studio-v26.11.10", false, false, &full("26.11.10")),
        rel("studio-v26.11.9", false, false, &full("26.11.9")),
    ];
    let with_pre = pick(&releases, LINUX, true).unwrap();
    assert_eq!(
        with_pre.tag, "studio-v26.12.1",
        "drafts, other platforms, unsummed and v-tags skipped"
    );
    assert_eq!(
        with_pre.binary_url,
        format!("https://x/studio-v26.12.1/{}", LINUX.asset("26.12.1"))
    );
    assert_eq!(with_pre.sig_url, None);
    let stable = pick(&releases, LINUX, false).unwrap();
    assert_eq!(stable.version, "26.11.10", "compared as numbers");
    assert_eq!(pick(&releases[..1], LINUX, true), None);
}

#[test]
fn what_is_installed_comes_from_the_program_and_appimage() {
    let exe = Path::new("/Applications/Studio.app/Contents/MacOS/Studio");
    assert_eq!(
        Installation::detect(exe, None),
        Installation::MacApp("/Applications/Studio.app".into())
    );
    let image = Path::new("/home/a/.local/share/studio/Studio-1.2.3-linux-x86_64.AppImage");
    assert_eq!(
        Installation::detect(Path::new("/tmp/.mount_Studio/usr/bin/Studio"), Some(image)),
        Installation::AppImage(image.into())
    );
    let bare = Path::new("/home/a/studio/target/desktop-dev/Studio");
    assert_eq!(
        Installation::detect(bare, Some(Path::new(""))),
        Installation::Unmanaged(bare.into())
    );
}

#[test]
fn only_a_release_build_of_a_release_install_with_a_key_may_update() {
    let mac = Platform::MacosArm64;
    let app = Installation::MacApp("/Applications/Studio.app".into());
    let image = Installation::AppImage("/x/Studio.AppImage".into());
    let bare = Installation::Unmanaged("/x/Studio".into());
    let key = Some("RW");
    assert_eq!(install_block(&app, Some(mac), key, true), None);
    assert_eq!(install_block(&image, Some(LINUX), key, true), None);
    assert_eq!(
        install_block(&app, None, key, true),
        Some(Block::NoPlatform)
    );
    assert_eq!(
        install_block(&app, Some(mac), None, true),
        Some(Block::NoKey)
    );
    assert_eq!(
        install_block(&app, Some(mac), key, false),
        Some(Block::NotRelease)
    );
    assert_eq!(
        install_block(&bare, Some(mac), key, true),
        Some(Block::Unmanaged)
    );
    assert_eq!(
        install_block(&image, Some(mac), key, true),
        Some(Block::WrongKind)
    );
    let moved =
        Installation::MacApp("/private/var/folders/x/T/AppTranslocation/ABC/d/Studio.app".into());
    assert_eq!(
        install_block(&moved, Some(mac), key, true),
        Some(Block::Translocated)
    );
}

#[test]
fn a_versioned_appimage_gets_the_new_name_any_other_is_replaced_in_place() {
    let dir = Path::new("/h/.local/share/studio");
    let asset = LINUX.asset("26.12.1");
    assert_eq!(
        appimage_destination(&dir.join(LINUX.asset("26.11.4")), &asset),
        dir.join(&asset)
    );
    let own = Path::new("/h/Apps/studio.AppImage");
    assert_eq!(appimage_destination(own, &asset), own);
}

#[test]
fn only_the_desktop_entrys_exec_lines_for_the_old_program_move() {
    let (old, new) = (
        Path::new("/a/Studio-1.AppImage"),
        Path::new("/a/Studio-2.AppImage"),
    );
    let text =
        "[Desktop Entry]\nExec=/a/Studio-1.AppImage %U\nTryExec=/a/Studio-1.AppImage\nIcon=x\n";
    assert_eq!(
        desktop_entry_moved(text, old, new).unwrap(),
        "[Desktop Entry]\nExec=/a/Studio-2.AppImage %U\nTryExec=/a/Studio-2.AppImage\nIcon=x\n"
    );
    assert_eq!(
        desktop_entry_moved("Exec=/a/Studio-1.AppImage.bak\n", old, new),
        None
    );
    assert_eq!(desktop_entry_moved("Exec=/elsewhere\n", old, new), None);
}

#[test]
fn a_link_to_something_else_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("other");
    std::fs::write(&other, b"x").unwrap();
    let link = dir.path().join("studio");
    std::os::unix::fs::symlink(&other, &link).unwrap();
    let links = LinuxLinks {
        bin: Some(link.clone()),
        desktop: Some(dir.path().join("missing.desktop")),
    };
    relink(&dir.path().join("old"), &dir.path().join("new"), &links).unwrap();
    assert_eq!(std::fs::read_link(&link).unwrap(), other);
}

#[test]
fn links_default_to_where_the_installer_puts_them() {
    let links = LinuxLinks::from_env(Some(Path::new("/h")), None, None);
    assert_eq!(
        links.bin.as_deref(),
        Some(Path::new("/h/.local/bin/studio"))
    );
    assert_eq!(
        links.desktop.as_deref(),
        Some(Path::new(
            "/h/.local/share/applications/app.fasttrackstudio.studio.desktop"
        ))
    );
    let links = LinuxLinks::from_env(
        Some(Path::new("/h")),
        Some(Path::new("/b")),
        Some(Path::new("/d")),
    );
    assert_eq!(links.bin.as_deref(), Some(Path::new("/b/studio")));
    assert_eq!(
        links.desktop.as_deref(),
        Some(Path::new(
            "/d/applications/app.fasttrackstudio.studio.desktop"
        ))
    );
}

#[test]
fn the_new_bundle_takes_the_old_ones_place() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Studio.app");
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    std::fs::write(bundle.join("Contents/v"), b"old").unwrap();
    let staging = tempfile::tempdir_in(dir.path()).unwrap();
    let new = staging.path().join("Studio.app");
    std::fs::create_dir_all(new.join("Contents")).unwrap();
    std::fs::write(new.join("Contents/v"), b"new").unwrap();

    swap_bundle(&new, &bundle, staging.path()).unwrap();
    assert_eq!(std::fs::read(bundle.join("Contents/v")).unwrap(), b"new");
    assert_eq!(
        std::fs::read(staging.path().join("previous.app/Contents/v")).unwrap(),
        b"old"
    );
}

#[test]
fn a_failed_swap_puts_the_old_bundle_back() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Studio.app");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(bundle.join("v"), b"old").unwrap();
    let staging = tempfile::tempdir_in(dir.path()).unwrap();

    assert!(swap_bundle(&staging.path().join("missing.app"), &bundle, staging.path()).is_err());
    assert_eq!(std::fs::read(bundle.join("v")).unwrap(), b"old");
}

#[test]
fn the_team_is_read_from_codesign() {
    let shown = "Executable=/A/Studio.app/Contents/MacOS/Studio\nIdentifier=app.fasttrackstudio.studio\nTeamIdentifier=ABCDE12345\n";
    assert_eq!(team_identifier(shown), Some("ABCDE12345"));
    assert_eq!(team_identifier("TeamIdentifier=not set\n"), None);
    assert_eq!(team_identifier("Signature=adhoc\n"), None);
}

// ── macOS: a real .dmg ──────────────────────────────────────────────

/// A `Studio.app` whose program answers `--version` with `says`, ad-hoc
/// signed, in `dir`.
#[cfg(target_os = "macos")]
fn fake_bundle(dir: &Path, says: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let app = dir.join("Studio.app");
    let macos = app.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::write(
        app.join("Contents/Info.plist"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
         <key>CFBundleIdentifier</key><string>app.fasttrackstudio.studio.test</string>\
         <key>CFBundleExecutable</key><string>Studio</string>\
         <key>CFBundlePackageType</key><string>APPL</string></dict></plist>\n",
    )
    .unwrap();
    let program = macos.join("Studio");
    std::fs::write(&program, format!("#!/bin/sh\necho '{says}'\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let signed = std::process::Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        signed.status.success(),
        "{}",
        String::from_utf8_lossy(&signed.stderr)
    );
    app
}

/// A feed whose `studio-v26.12.1` carries `dmg` as this Mac's asset,
/// its sums signed by `key`.
#[cfg(target_os = "macos")]
async fn start_mac(dmg: Vec<u8>, key: &TestKey) -> Feed {
    let fake = Fake::default();
    let app = axum::Router::new()
        .fallback(serve_file)
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let tag = "studio-v26.12.1";
    let asset = Platform::MacosArm64.asset("26.12.1");
    let sums = format!("{}  {asset}\n", hex_sha256(&dmg));
    let sig = key.sign(sums.as_bytes(), &format!("studio {tag} SHA256SUMS"));
    let a =
        |name: &str| format!(r#"{{"name":"{name}","browser_download_url":"{base}/dl/{name}"}}"#);
    let list = format!(
        r#"[{{"tag_name":"{tag}","draft":false,"prerelease":true,"assets":[{},{},{}]}}]"#,
        a(&asset),
        a("SHA256SUMS"),
        a("SHA256SUMS.minisig")
    );
    let mut files = fake.files.lock().unwrap();
    files.insert(
        "/repos/FastTrackStudios/studio/releases".into(),
        list.into_bytes(),
    );
    files.insert(format!("/dl/{asset}"), dmg);
    files.insert("/dl/SHA256SUMS".into(), sums.into_bytes());
    files.insert("/dl/SHA256SUMS.minisig".into(), sig.into_bytes());
    drop(files);
    Feed {
        api: base,
        repo: "FastTrackStudios/studio".into(),
        prereleases: true,
    }
}

/// A `.dmg` holding a `Studio.app` that says `says`.
#[cfg(target_os = "macos")]
fn fake_dmg(dir: &Path, says: &str) -> Vec<u8> {
    let src = dir.join("dmg-src");
    std::fs::create_dir_all(&src).unwrap();
    fake_bundle(&src, says);
    let dmg = dir.join("Studio.dmg");
    let made = std::process::Command::new("/usr/bin/hdiutil")
        .args([
            "create",
            "-quiet",
            "-fs",
            "HFS+",
            "-format",
            "UDZO",
            "-volname",
            "Studio",
            "-srcfolder",
        ])
        .arg(&src)
        .arg(&dmg)
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    std::fs::read(&dmg).unwrap()
}

#[cfg(target_os = "macos")]
async fn update_mac(feed: &Feed, key: &TestKey, apps: &Path, cache: &Path) -> UpdateStatus {
    let installation = Installation::MacApp(apps.join("Studio.app"));
    run(
        feed,
        Target {
            current: "26.11.4",
            installation: &installation,
            platform: Some(Platform::MacosArm64),
            key: Some(&key.public()),
            released: true,
            scratch: cache,
            links: &LinuxLinks::default(),
        },
        Mode::Install,
    )
    .await
}

#[cfg(target_os = "macos")]
fn version_of(bundle: &Path) -> String {
    let out = std::process::Command::new(bundle.join("Contents/MacOS/Studio"))
        .arg("--version")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[cfg(target_os = "macos")]
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_signed_newer_dmg_replaces_the_installed_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let apps = dir.path().join("Applications");
    std::fs::create_dir_all(&apps).unwrap();
    fake_bundle(&apps, "studio 26.11.4");
    let key = TestKey::new(7);
    let feed = start_mac(fake_dmg(dir.path(), "studio 26.12.1"), &key).await;
    let cache = dir.path().join("cache");

    let status = update_mac(&feed, &key, &apps, &cache).await;
    assert_eq!(
        status,
        UpdateStatus::Installed {
            to: "26.12.1".into()
        }
    );
    assert_eq!(version_of(&apps.join("Studio.app")), "studio 26.12.1");
    assert_eq!(
        entries(&apps),
        vec!["Studio.app"],
        "no staging copy or old bundle left"
    );
    assert!(
        entries(&cache).is_empty(),
        "the .dmg and its mount point are gone: {:?}",
        entries(&cache)
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_dmg_whose_studio_reports_another_version_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let apps = dir.path().join("Applications");
    std::fs::create_dir_all(&apps).unwrap();
    fake_bundle(&apps, "studio 26.11.4");
    let key = TestKey::new(7);
    let feed = start_mac(fake_dmg(dir.path(), "studio 26.10.1"), &key).await;
    let cache = dir.path().join("cache");

    let status = update_mac(&feed, &key, &apps, &cache).await;
    assert!(
        matches!(&status, UpdateStatus::Failed(e) if e.contains("studio 26.10.1")),
        "{status:?}"
    );
    assert_eq!(version_of(&apps.join("Studio.app")), "studio 26.11.4");
    assert_eq!(entries(&apps), vec!["Studio.app"]);
}

#[test]
fn studio_installs_in_the_background_unless_told_not_to() {
    use std::ffi::OsStr;
    use studio_shell::updates::background_install;
    assert!(background_install(None));
    assert!(background_install(Some(OsStr::new(""))));
    assert!(background_install(Some(OsStr::new("0"))));
    assert!(!background_install(Some(OsStr::new("1"))));
    assert!(!background_install(Some(OsStr::new("yes"))));
}
