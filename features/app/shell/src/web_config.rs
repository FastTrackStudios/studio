//! Where a web app finds Task and fts-auth: `/config.json`.
//!
//! The studio-web image writes `/config.json` when it starts, from
//! `TASK_URL` and `AUTH_URL` (`{ "task_url": …, "auth_url": … }`, either
//! left out when unset). Both web apps, Signal and Session, read it once at
//! boot. A field that is missing, empty or not an `http(s)` URL, and a file
//! that is missing or is not JSON (a static server answers an unknown path
//! with its `index.html`), fall back to the page's own origin: the setup
//! where Task is served behind the same host, as task-web dials its own
//! origin (task's `deploy/README.md`, "same origin"). One image serves
//! both setups.
//!
//! [`resolve`] is the whole rule, pure. The fetch (feature `web-config`,
//! wasm only) is a thin shell around it: `load_then(launch)` in the app's
//! `main`, then `current()` anywhere.

use facet::Facet;

/// Where this page's Task and fts-auth are: absolute `http(s)` URLs with
/// no trailing slash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebConfig {
    /// Task's base URL (`https://task.fasttrackstudio.app`).
    pub task_url: String,
    /// fts-auth's base URL, the OIDC issuer (`https://auth.fasttrackstudio.app`).
    pub auth_url: String,
}

impl WebConfig {
    /// Everything at `origin`: the same-origin setup.
    #[must_use]
    pub fn same_origin(origin: &str) -> Self {
        let origin = trim(origin);
        Self {
            task_url: origin.clone(),
            auth_url: origin,
        }
    }

    /// Task's vox WebSocket at `path` (`/vox`, `/org/<slug>/vox`):
    /// `https://…` becomes `wss://…`, `http://…` `ws://…`.
    #[must_use]
    pub fn task_ws(&self, path: &str) -> String {
        let base = self
            .task_url
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        format!("{base}/{}", path.trim_start_matches('/'))
    }
}

/// `/config.json`'s shape. Unknown fields are ignored, so the file can grow
/// without breaking an older page.
#[derive(Facet, Default)]
struct File {
    #[facet(default)]
    task_url: Option<String>,
    #[facet(default)]
    auth_url: Option<String>,
}

/// The URLs a page at `origin` (`https://studio.example`) dials, from
/// `/config.json`'s text (`None` when the fetch failed or found nothing).
#[must_use]
pub fn resolve(config: Option<&str>, origin: &str) -> WebConfig {
    let file: File = config
        .and_then(|text| facet_json::from_str(text).ok())
        .unwrap_or_default();
    let fallback = WebConfig::same_origin(origin);
    WebConfig {
        task_url: usable(file.task_url).unwrap_or(fallback.task_url),
        auth_url: usable(file.auth_url).unwrap_or(fallback.auth_url),
    }
}

/// A configured base URL, when it is one: trimmed, `http(s)`, an authority
/// that is a host (a domain name, IPv4, or `[IPv6]`) with an optional port
/// in 1..=65535 and no userinfo, then an optional path, with no query or
/// fragment (a share path is appended to it: `on_task`), and no trailing
/// slash.
fn usable(url: Option<String>) -> Option<String> {
    let url = trim(url.as_deref()?);
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    if rest.contains(['?', '#']) {
        return None;
    }
    let authority = rest.split('/').next().unwrap_or_default();
    authority_ok(authority).then_some(url)
}

/// `host[:port]` or `[v6][:port]`.
fn authority_ok(authority: &str) -> bool {
    let (host_ok, port) = if let Some(v6) = authority.strip_prefix('[') {
        let Some((inner, after)) = v6.split_once(']') else {
            return false;
        };
        let inner_ok = inner.contains(':')
            && inner
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || matches!(b, b':' | b'.'));
        let port = match after {
            "" => None,
            p => match p.strip_prefix(':') {
                Some(port) => Some(port),
                None => return false,
            },
        };
        (inner_ok, port)
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        (domain_ok(host), port)
    };
    host_ok && port.is_none_or(port_ok)
}

/// Dot-separated labels of letters, digits and inner hyphens.
fn domain_ok(host: &str) -> bool {
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// A TCP port: digits, 1..=65535.
fn port_ok(port: &str) -> bool {
    !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|p| p != 0)
}

fn trim(url: &str) -> String {
    url.trim().trim_end_matches('/').to_owned()
}

#[cfg(all(feature = "web-config", target_arch = "wasm32"))]
mod page {
    use super::{WebConfig, resolve};
    use std::sync::OnceLock;
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen_futures::JsFuture;

    static CURRENT: OnceLock<WebConfig> = OnceLock::new();

    /// Fetch `/config.json` (never from the HTTP cache: it is per
    /// deployment, not per build) and keep what it says. Call once, before
    /// the app launches; a failed fetch is the same-origin setup.
    pub async fn load() -> WebConfig {
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default();
        let text = fetch_text("/config.json").await;
        let config = resolve(text.as_deref(), &origin);
        tracing::info!(
            web_config.found = text.is_some(),
            web_config.task_same_origin =
                config.task_url == WebConfig::same_origin(&origin).task_url,
            web_config.auth_same_origin =
                config.auth_url == WebConfig::same_origin(&origin).auth_url,
            "web config"
        );
        CURRENT.get_or_init(|| config).clone()
    }

    /// [`load`], then `launch`: how a web app's `main` starts, so nothing
    /// in it dials before the config is known.
    pub fn load_then(launch: impl FnOnce() + 'static) {
        wasm_bindgen_futures::spawn_local(async move {
            load().await;
            launch();
        });
    }

    /// What [`load`] found; the same-origin setup when it has not run.
    #[must_use]
    pub fn current() -> WebConfig {
        CURRENT.get().cloned().unwrap_or_else(|| {
            let origin = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_default();
            WebConfig::same_origin(&origin)
        })
    }

    async fn fetch_text(url: &str) -> Option<String> {
        let window = web_sys::window()?;
        let init = web_sys::RequestInit::new();
        init.set_cache(web_sys::RequestCache::NoStore);
        let resp = JsFuture::from(window.fetch_with_str_and_init(url, &init))
            .await
            .ok()?;
        let resp: web_sys::Response = resp.dyn_into().ok()?;
        if !resp.ok() {
            return None;
        }
        JsFuture::from(resp.text().ok()?).await.ok()?.as_string()
    }
}

#[cfg(all(feature = "web-config", target_arch = "wasm32"))]
pub use page::{current, load, load_then};

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "https://studio.example";

    #[test]
    fn no_file_is_the_same_origin() {
        let got = resolve(None, ORIGIN);
        assert_eq!(got, WebConfig::same_origin(ORIGIN));
        assert_eq!(got.task_url, "https://studio.example");
        assert_eq!(got.auth_url, "https://studio.example");
    }

    #[test]
    fn both_urls_from_the_file() {
        let got = resolve(
            Some(r#"{ "task_url": "https://task.example/", "auth_url": "https://auth.example" }"#),
            ORIGIN,
        );
        assert_eq!(got.task_url, "https://task.example");
        assert_eq!(got.auth_url, "https://auth.example");
    }

    #[test]
    fn each_missing_field_falls_back_on_its_own() {
        let got = resolve(Some(r#"{ "task_url": "https://task.example" }"#), ORIGIN);
        assert_eq!(got.task_url, "https://task.example");
        assert_eq!(got.auth_url, ORIGIN);
        let got = resolve(Some(r#"{ "auth_url": "http://auth.local:8081" }"#), ORIGIN);
        assert_eq!(got.task_url, ORIGIN);
        assert_eq!(got.auth_url, "http://auth.local:8081");
    }

    #[test]
    fn empty_or_unusable_fields_fall_back() {
        let got = resolve(Some(r#"{ "task_url": "", "auth_url": "  " }"#), ORIGIN);
        assert_eq!(got, WebConfig::same_origin(ORIGIN));
        let got = resolve(
            Some(r#"{ "task_url": "task.example", "auth_url": "javascript:alert(1)" }"#),
            ORIGIN,
        );
        assert_eq!(got, WebConfig::same_origin(ORIGIN));
        let got = resolve(Some(r#"{ "task_url": "https://" }"#), ORIGIN);
        assert_eq!(got.task_url, ORIGIN);
    }

    #[test]
    fn a_url_without_a_host_falls_back() {
        for bad in [
            "https://?x",
            "https://#frag",
            "http:///path",
            "https://:443",
            "https://user@task.example",
            "https://task example",
            "https://a..b",
            "https://-task.example",
        ] {
            let text = format!(r#"{{ "task_url": "{bad}" }}"#);
            assert_eq!(resolve(Some(&text), ORIGIN).task_url, ORIGIN, "{bad}");
        }
    }

    #[test]
    fn a_base_url_with_a_query_or_fragment_falls_back() {
        // A share path is appended to it: `…/sub?x=1/org/a/share/b` would
        // put the path in the query.
        for bad in [
            "https://task.example/sub?x=1",
            "https://task.example?x",
            "https://task.example/#frag",
        ] {
            let text = format!(r#"{{ "task_url": "{bad}" }}"#);
            assert_eq!(resolve(Some(&text), ORIGIN).task_url, ORIGIN, "{bad}");
        }
    }

    #[test]
    fn a_bad_port_falls_back() {
        for bad in [
            "https://task.example:abc",
            "https://task.example:99999",
            "https://task.example:0",
            "https://task.example:",
            "https://task.example:80:80",
            "http://[::1]8090",
            "http://[::1:8090",
        ] {
            let text = format!(r#"{{ "task_url": "{bad}" }}"#);
            assert_eq!(resolve(Some(&text), ORIGIN).task_url, ORIGIN, "{bad}");
        }
    }

    #[test]
    fn well_formed_base_urls_are_kept() {
        for good in [
            "https://task.example",
            "https://task.example/sub",
            "http://localhost:8090",
            "http://127.0.0.1:18080",
            "http://[::1]:8090",
            "https://[2001:db8::1]",
            "https://task.example:65535",
        ] {
            let text = format!(r#"{{ "task_url": "{good}" }}"#);
            assert_eq!(resolve(Some(&text), ORIGIN).task_url, good, "{good}");
        }
    }

    #[test]
    fn a_page_of_html_or_an_empty_object_is_no_config() {
        // A static server answers a missing /config.json with its index.html.
        let html = "<!doctype html><html><body>Studio</body></html>";
        assert_eq!(resolve(Some(html), ORIGIN), WebConfig::same_origin(ORIGIN));
        assert_eq!(resolve(Some("{}"), ORIGIN), WebConfig::same_origin(ORIGIN));
        assert_eq!(resolve(Some(""), ORIGIN), WebConfig::same_origin(ORIGIN));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let got = resolve(
            Some(r#"{ "task_url": "https://task.example", "something_new": 1 }"#),
            ORIGIN,
        );
        assert_eq!(got.task_url, "https://task.example");
    }

    #[test]
    fn task_ws_turns_the_scheme() {
        let config = resolve(Some(r#"{ "task_url": "https://task.example" }"#), ORIGIN);
        assert_eq!(config.task_ws("/vox"), "wss://task.example/vox");
        let local = WebConfig::same_origin("http://localhost:8090/");
        assert_eq!(local.task_ws("vox"), "ws://localhost:8090/vox");
    }
}
