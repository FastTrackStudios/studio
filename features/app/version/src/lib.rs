//! The version a shipped Studio reports: its `--version`, its about line,
//! its update check.
//!
//! Releases commit no version and no longer write one into the workspace
//! (that put the version into every crate's rustc invocation, so every
//! release recompiled all of them). The release version reaches the build
//! as `FTS_RELEASE_VERSION`, and this crate is the only one that reads it
//! (`build.rs`). Without it, the committed workspace version.
//!
//! Depend on it from binaries only. Library code reads
//! `app_shell::version::current()`, which the binary sets from
//! [`release_version`] at start.

#![no_std]

/// The release version this binary was built as (`X.Y.Z`).
pub const RELEASE_VERSION: &str = env!("FTS_APP_VERSION");

/// The release version this binary was built as (`X.Y.Z`).
#[must_use]
pub const fn release_version() -> &'static str {
    RELEASE_VERSION
}
