//! The running app's version, for library code.
//!
//! The binary knows the release version (`fts_version::release_version()`,
//! from `FTS_RELEASE_VERSION` at build time) and sets it here first thing in
//! `main`; Session's window, the update check and the MCP servers read it
//! with [`current`]. Read at run time, not compiled in, so a release
//! recompiles the binary and the one tiny `fts-version` crate, never the
//! libraries that show the version.
//!
//! Unset (a test, a tool that never sets it), it is this crate's own
//! version: the committed workspace version, what a plain `cargo build`
//! reports anyway.

use std::sync::OnceLock;

static VERSION: OnceLock<&'static str> = OnceLock::new();

/// Set the running app's version. The first call wins; a later one with
/// another version is ignored (and says so in the returned `false`).
pub fn set(version: &'static str) -> bool {
    VERSION.set(version).is_ok() || VERSION.get() == Some(&version)
}

/// The running app's version (`X.Y.Z`): what the binary [`set`], else the
/// committed workspace version.
#[must_use]
pub fn current() -> &'static str {
    VERSION.get().copied().unwrap_or(env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_once_then_current_reports_it() {
        assert_eq!(current(), env!("CARGO_PKG_VERSION"));
        assert!(set("99.1.2"));
        assert_eq!(current(), "99.1.2");
        assert!(set("99.1.2"), "the same version again is fine");
        assert!(!set("99.1.3"), "a second, different version is refused");
        assert_eq!(current(), "99.1.2");
    }
}
