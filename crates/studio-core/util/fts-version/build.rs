//! The release version, from the environment into the crate.
//!
//! A release build exports `FTS_RELEASE_VERSION=X.Y.Z` (`scripts/release.sh
//! stamp` writes it into the job's environment; the bundle scripts export
//! it). Through a build script rather than `option_env!` in the crate: the
//! value is then build-script output, which cargo reruns on
//! (`rerun-if-env-changed`) and kache keys on, so a release recompiles this
//! crate and its dependents, and a build with the same version restores
//! them. Without the variable the crate reports its own Cargo version (the
//! committed workspace version), so a plain `cargo build` is unchanged.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FTS_RELEASE_VERSION");
    let committed = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let version = match std::env::var("FTS_RELEASE_VERSION") {
        Ok(v) if !v.trim().is_empty() => {
            let v = v.trim();
            let v = v.strip_prefix('v').unwrap_or(v);
            assert!(
                is_xyz(v),
                "FTS_RELEASE_VERSION must be X.Y.Z (or vX.Y.Z), got '{v}'"
            );
            v.to_owned()
        }
        _ => committed,
    };
    println!("cargo:rustc-env=FTS_APP_VERSION={version}");
}

/// Three dot-separated runs of ASCII digits.
fn is_xyz(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}
