//! Where the web build is served from: its base path.
//!
//! The browser build is served at the root of an origin by the engine
//! (`--features embed-web`, `http://<host>:4040/`), and under `/signal/` in
//! studio-web (`dx build --base-path signal`, nix/modules/packages/
//! web-bundles.nix). dx rewrites its own asset URLs for the base path; the
//! URLs this crate writes itself (the keys worklet bundle, the
//! `/rigs/keys/{profile}` route) go through here so they hold under both.
//!
//! Pure string functions: the base comes from `dioxus::cli_config::base_path`
//! in the page ([`current`]) and is a parameter everywhere else, so the
//! rules are tested natively.

/// A base path as dx gives it (`"signal"`, `"/signal/"`, `""` or none), as
/// the prefix a path is put under: `""` at the root, else `"/signal"`.
#[must_use]
pub fn normalize(base: Option<&str>) -> String {
    let trimmed = base.unwrap_or_default().trim().trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("/{trimmed}")
    }
}

/// `path` (absolute, `/worklet/x.js`) under the base (`/signal/worklet/x.js`).
#[must_use]
pub fn under(base: &str, path: &str) -> String {
    format!("{base}/{}", path.trim_start_matches('/'))
}

/// The page's path with the base taken off (`/signal/rigs/keys/x` →
/// `/rigs/keys/x`); a path outside the base is returned as it is.
#[must_use]
pub fn strip<'a>(base: &str, pathname: &'a str) -> &'a str {
    if base.is_empty() {
        return pathname;
    }
    match pathname.strip_prefix(base) {
        Some("") => "/",
        Some(rest) if rest.starts_with('/') => rest,
        _ => pathname,
    }
}

/// This page's base, from the one dx built it with.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn current() -> String {
    normalize(dioxus::cli_config::base_path().as_deref())
}

/// `path` under this page's base (see [`under`]).
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn url(path: &str) -> String {
    under(&current(), path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_every_spelling_dx_uses() {
        assert_eq!(normalize(None), "");
        assert_eq!(normalize(Some("")), "");
        assert_eq!(normalize(Some("/")), "");
        assert_eq!(normalize(Some("signal")), "/signal");
        assert_eq!(normalize(Some("/signal/")), "/signal");
        assert_eq!(normalize(Some("apps/signal")), "/apps/signal");
    }

    #[test]
    fn puts_paths_under_the_base() {
        assert_eq!(under("", "/worklet/a.js"), "/worklet/a.js");
        assert_eq!(under("/signal", "/worklet/a.js"), "/signal/worklet/a.js");
        assert_eq!(under("/signal", "worklet/a.js"), "/signal/worklet/a.js");
    }

    #[test]
    fn strips_the_base_from_the_page_path() {
        assert_eq!(strip("", "/rigs/keys/x"), "/rigs/keys/x");
        assert_eq!(strip("/signal", "/signal/rigs/keys/x"), "/rigs/keys/x");
        assert_eq!(strip("/signal", "/signal"), "/");
        assert_eq!(strip("/signal", "/signal/"), "/");
        // Not under the base, or only sharing its first letters: untouched.
        assert_eq!(strip("/signal", "/rigs/keys/x"), "/rigs/keys/x");
        assert_eq!(strip("/signal", "/signals/x"), "/signals/x");
    }
}
