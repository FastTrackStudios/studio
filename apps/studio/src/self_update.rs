//! Desktop Studio updating itself (macOS and Linux): a check a little
//! after launch and every few hours after, which installs a newer signed
//! release in the background, the notice that says it is in, and
//! `STUDIO_UPDATE` (check, or install now) for agents and scripts.
//!
//! The work is `studio_shell::updates` (the feed, the signature, putting
//! the `.dmg`'s bundle or the `AppImage` in place). A development build, a
//! build that was not installed from a release, or `STUDIO_NO_AUTO_UPDATE=1`
//! checks only.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use architect::action::{ActionError, ActionFuture, ActionList, ActionMeta, Effect};
use dioxus::prelude::*;
use facet::Facet;
use serde_json::{Value, json};
use studio_shell::updates::{self, App, UpdateStatus};

/// How long after launch the first check waits: the window and the engine
/// come first.
const FIRST_CHECK: Duration = Duration::from_secs(45);

/// How often a running Studio checks again.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The running Studio.
fn app() -> App {
    App::new(fts_version::release_version(), fts_version::RELEASED)
}

/// The version this run installed: it runs from the next launch, so
/// there is nothing more to fetch until then.
static INSTALLED: OnceLock<String> = OnceLock::new();

fn remember_installed(version: &str) {
    let _ = INSTALLED.set(version.to_owned());
}

/// Whether the background check may install (it always may check).
fn automatic() -> bool {
    updates::background_install(std::env::var_os("STUDIO_NO_AUTO_UPDATE").as_deref())
}

/// Check (or install) once, as one span: what was found and what was done.
async fn update(feed: updates::Feed, install: bool, source: &'static str) -> UpdateStatus {
    let app = app();
    let mut span = crate::trace::Span::root("studio.update");
    span.set("update.source", source);
    span.set("update.mode", if install { "install" } else { "check" });
    span.set("update.current", app.version);
    span.set("update.installation", app.installation().kind());
    let mode = if install {
        updates::Mode::Install
    } else {
        updates::Mode::Check
    };
    let status = match INSTALLED.get() {
        Some(to) => UpdateStatus::Installed { to: to.clone() },
        None => app.update(feed, mode).await,
    };
    if let UpdateStatus::Installed { to } = &status {
        remember_installed(to);
    }
    span.set("update.outcome", status.kind());
    if let Some(newest) = status.newest() {
        span.set("update.newest", newest.to_owned());
    }
    if let UpdateStatus::Failed(why) = &status {
        span.fail(why.clone());
    }
    status
}

/// The background check. It says once, as the app's toast, that a newer
/// Studio is installed (or, when this one cannot install it, available).
/// Mounted once, in the main window; draws nothing.
#[component]
pub fn UpdateNotice() -> Element {
    let app_state = crate::state::use_app();
    use_future(move || async move {
        let block = app().block();
        // A development build hears nothing about releases.
        if block == Some(updates::Block::NotRelease) {
            return;
        }
        let install = automatic() && block.is_none();
        crate::state::sleep_ms(millis(FIRST_CHECK)).await;
        let mut said = None::<String>;
        loop {
            let status = update(updates::Feed::github(), install, "background").await;
            match &status {
                // What is installed runs from the next launch: nothing
                // more to check until then.
                UpdateStatus::Installed { .. } => return app_state.say(status.to_string()),
                UpdateStatus::Available { newest } | UpdateStatus::Blocked { newest, .. }
                    if said.as_deref() != Some(newest.as_str()) =>
                {
                    said = Some(newest.clone());
                    app_state.say(status.to_string());
                }
                _ => {}
            }
            crate::state::sleep_ms(millis(EVERY)).await;
        }
    });
    rsx! {}
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

static UPDATE: ActionMeta = ActionMeta {
    id: "STUDIO_UPDATE",
    trait_name: "",
    method_name: "",
    display_name: "Update Studio",
    description: "Checks Studio's releases (studio-v* tags on GitHub, pre-releases included) for one newer than this Studio. With `install` true, downloads it, verifies SHA256SUMS against the org's minisign key and the download against those sums (on macOS also the bundle's code signature and team), and puts it in place of the installed .app or AppImage; it runs from the next launch. Answers `status` (current, no-release, available, blocked, installed or failed), `current`, `newest`, `installation` (mac-app, appimage or unmanaged), `blocked` (why this Studio cannot install, if it cannot) and `message`.",
    category: "App",
    group: "Studio",
    shortcut: "",
    toggleable: false,
    undo: false,
};

#[derive(Facet)]
struct UpdateArgs {
    #[facet(default)]
    install: Option<bool>,
}

/// What `STUDIO_UPDATE` answers.
fn answer(app: App, status: &UpdateStatus) -> Value {
    json!({
        "status": status.kind(),
        "current": app.version,
        "newest": status.newest(),
        "installation": app.installation().kind(),
        "blocked": app.block().map(|b| b.to_string()),
        "message": status.to_string(),
    })
}

/// Put `STUDIO_UPDATE` on the list, on Studio's releases.
pub fn install_actions(list: &ActionList) {
    install_actions_with(list, updates::Feed::github());
}

/// Put `STUDIO_UPDATE` on the list, on `feed`'s releases.
fn install_actions_with(list: &ActionList, feed: updates::Feed) {
    let id = UPDATE.id;
    let handler = Arc::new(move |args: &[u8]| -> Result<ActionFuture, ActionError> {
        let args = match args.trim_ascii() {
            b"" | b"null" => b"{}".as_slice(),
            a => a,
        };
        let a: UpdateArgs = facet_json::from_slice(args).map_err(|e| ActionError::BadArgs {
            id: id.to_owned(),
            message: e.to_string(),
            expected: None,
        })?;
        let install = a.install.unwrap_or(false);
        let feed = feed.clone();
        Ok(Box::pin(async move {
            let status = update(feed, install, "action").await;
            serde_json::to_vec(&answer(app(), &status)).map_err(|e| ActionError::Failed {
                id: id.to_owned(),
                message: e.to_string(),
            })
        }) as ActionFuture)
    });
    if let Err(e) = list.register_dynamic_async(
        &UPDATE,
        Effect::Mutating,
        Some(UpdateArgs::SHAPE),
        None,
        handler,
    ) {
        tracing::warn!(action.id = id, error = %e, "an action could not be registered");
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};

    use super::*;

    /// A releases feed on 127.0.0.1 that answers every request with
    /// `list`: one newer pre-release with this platform's asset and sums.
    fn feed_with_a_newer_release() -> updates::Feed {
        let platform = updates::Platform::current().expect("a desktop platform");
        let asset = platform.asset("99.1.1");
        let list = format!(
            r#"[{{"tag_name":"studio-v99.1.1","draft":false,"prerelease":true,"assets":[
                 {{"name":"{asset}","browser_download_url":"http://127.0.0.1:1/{asset}"}},
                 {{"name":"SHA256SUMS","browser_download_url":"http://127.0.0.1:1/SHA256SUMS"}}]}}]"#
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut conn in listener.incoming().flatten() {
                let mut buf = [0u8; 4096];
                let _ = conn.read(&mut buf);
                let _ = write!(
                    conn,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{list}",
                    list.len()
                );
            }
        });
        updates::Feed {
            api,
            repo: updates::REPO.to_owned(),
            prereleases: true,
        }
    }

    fn call(list: &ActionList, args: &[u8]) -> Value {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let bytes = runtime
            .block_on(list.run_json_async("STUDIO_UPDATE", args))
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn studio_update_finds_a_newer_release_on_the_action_list() {
        let list = ActionList::new();
        install_actions_with(&list, feed_with_a_newer_release());
        for args in [&b""[..], b"{}", b"null"] {
            let got = call(&list, args);
            assert_eq!(got["status"], "available", "{got}");
            assert_eq!(got["newest"], "99.1.1");
            assert_eq!(got["current"], fts_version::release_version());
        }
    }

    #[test]
    fn once_a_release_is_installed_studio_update_says_so_and_fetches_nothing_again() {
        let list = ActionList::new();
        // A feed that would offer something else: it is never asked.
        install_actions_with(&list, feed_with_a_newer_release());
        remember_installed("99.0.5");
        for args in [&b""[..], br#"{"install":true}"#] {
            let got = call(&list, args);
            assert_eq!(got["status"], "installed", "{got}");
            assert_eq!(got["newest"], "99.0.5");
        }
    }

    #[test]
    fn studio_update_asked_to_install_from_a_test_build_says_why_not() {
        let list = ActionList::new();
        install_actions_with(&list, feed_with_a_newer_release());
        let got = call(&list, br#"{"install":true}"#);
        // A test binary is neither a release build nor an installed .app or
        // AppImage: it reports the release and changes nothing.
        assert_eq!(got["status"], "blocked", "{got}");
        assert_eq!(got["newest"], "99.1.1");
        assert_eq!(got["installation"], "unmanaged");
        assert!(
            got["blocked"].as_str().is_some_and(|b| !b.is_empty()),
            "{got}"
        );
    }
}
