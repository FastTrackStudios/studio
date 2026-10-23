//! The update check against a fake releases feed on 127.0.0.1 (STA-394):
//! the statuses Settings shows for a newer release, the current one, a
//! release without this platform's binary, and an install refused by a
//! build that pins no key, which leaves the binary as it was.

#![cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use app_shell::updates::{App, Mode, REPO, UpdateStatus, run};
use architect::release::{Target, platform_asset};

const APP: App = App::new("session-desktop", "0.2.0");

/// Serve `releases` as the studio repo's release list; returns the API base.
async fn feed(releases: String) -> String {
    let list = format!("/repos/{REPO}/releases");
    // One fallback answers every path (as architect's own release test
    // does): the list at `list`, 404 elsewhere.
    let app = axum::Router::new().fallback(move |uri: axum::http::Uri| async move {
        if uri.path() == list {
            (axum::http::StatusCode::OK, releases)
        } else {
            (axum::http::StatusCode::NOT_FOUND, String::new())
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

/// One release `tag` carrying `assets` (download URLs are never fetched by
/// a check).
fn release(tag: &str, assets: &[&str]) -> String {
    let assets: Vec<String> = assets
        .iter()
        .map(|name| {
            format!(
                r#"{{"name":"{name}","browser_download_url":"http://127.0.0.1:9/{tag}/{name}"}}"#
            )
        })
        .collect();
    format!(
        r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"assets":[{}]}}"#,
        assets.join(",")
    )
}

fn list(releases: &[String]) -> String {
    format!("[{}]", releases.join(","))
}

async fn status(releases: String, mode: Mode, key: Option<&str>) -> (UpdateStatus, Vec<u8>) {
    let base = feed(releases).await;
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join(APP.name);
    std::fs::write(&exe, b"old binary").unwrap();
    let target = Target {
        current: APP.version,
        exe: &exe,
        key,
    };
    let got = run(&APP.updater().api_base(base), target, mode).await;
    (got, std::fs::read(&exe).unwrap())
}

fn ours() -> String {
    platform_asset(APP.name)
}

#[tokio::test]
async fn a_newer_release_is_available() {
    let feed = list(&[
        release("v0.3.0", &[&ours(), "SHA256SUMS", "SHA256SUMS.minisig"]),
        release("v0.2.0", &[&ours(), "SHA256SUMS"]),
    ]);
    let (got, _) = status(feed, Mode::Check, None).await;
    assert_eq!(
        got,
        UpdateStatus::Available {
            newest: "0.3.0".into()
        }
    );
    assert_eq!(got.to_string(), "Update available: v0.3.0.");
}

#[tokio::test]
async fn the_running_version_is_current() {
    let feed = list(&[release("v0.2.0", &[&ours(), "SHA256SUMS"])]);
    let (got, _) = status(feed, Mode::Check, None).await;
    assert_eq!(
        got,
        UpdateStatus::Current {
            newest: "0.2.0".into()
        }
    );
}

#[tokio::test]
async fn a_release_without_this_platforms_binary_is_no_release() {
    // What the studio repo's releases carry today: installers, no bare
    // binary and no SHA256SUMS.
    let feed = list(&[release(
        "v0.9.0",
        &[
            "Session-0.9.0-macos-arm64.dmg",
            "Session-0.9.0-linux-x86_64.AppImage",
        ],
    )]);
    let (got, _) = status(feed, Mode::Check, None).await;
    assert_eq!(got, UpdateStatus::NoRelease);
}

#[tokio::test]
async fn an_install_without_a_key_is_refused_and_changes_nothing() {
    let feed = list(&[release(
        "v0.3.0",
        &[&ours(), "SHA256SUMS", "SHA256SUMS.minisig"],
    )]);
    let (got, exe) = status(feed, Mode::Install, None).await;
    let UpdateStatus::Failed(why) = got else {
        panic!("expected a refusal, got {got:?}");
    };
    assert!(why.to_lowercase().contains("key"), "{why}");
    assert_eq!(exe, b"old binary");
}
