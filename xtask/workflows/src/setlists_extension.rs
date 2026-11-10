//! `setlists-extension` — Studio's setlists Architect extension
//! (`features/setlists`, id `studio-setlists`) released to the
//! FastTrackStudios extension store with Studio's own releases.
//!
//! ```text
//!   push tag v*            architect's reusable extension-release.yml:
//!                          plan (reads features/setlists/extension.toml);
//!                          when the store already lists that version,
//!                          nothing more (seconds). Otherwise: build the
//!                          `studio-setlists` program per platform, pack
//!                          one .archext, sign it in release-signing with
//!                          the org release key (the key every Architect
//!                          app trusts), attach it to the tag's GitHub
//!                          Release (release.yml's), and write the store's
//!                          index entry (extensions/index.toml in
//!                          architect).
//!   workflow_dispatch      the same again, run on a v* tag (`gh workflow
//!                          run setlists-extension.yml --ref v26.11.0`):
//!                          a retry. On a branch it does nothing (the
//!                          signing key admits tags only).
//! ```
//!
//! So shipping a new version of the extension is a version bump in its
//! extension.toml, released with the next v* tag; no tags of its own. No
//! FTS_REGISTRY_TOKEN here: the index step prints the entry and the
//! store's pull request is opened from it by hand.
//!
//! Hosted runners, not the NixOS pool: a program linked on NixOS loads
//! its libc from /nix/store and runs nowhere else. Ubuntu needs the GTK,
//! WebKit and tray headers the desktop crates' build scripts ask
//! pkg-config for (nothing of them is linked into the program: `ldd`
//! shows only libc, libm and libgcc_s), and mold (.cargo/config.toml's
//! linker for x86_64 Linux). rustup reads rust-toolchain.toml.

use crate::model::{Job, Workflow, yaml};
use anyhow::Result;

/// Architect's reusable workflow and the ref whose `cargo xtask ext-*`
/// pack and sign: one pin.
const ARCHITECT_REF: &str = "b224c39cba2bd21cb17d451436df5aa3f3400875";

/// The platforms the program is built for.
const PLATFORMS: &str = r#"["x86_64-unknown-linux-gnu","aarch64-apple-darwin"]"#;

/// GitHub-hosted runners, one per platform.
const RUNNERS: &str = r#"{"x86_64-unknown-linux-gnu":"ubuntu-24.04","aarch64-apple-darwin":"macos-15"}"#;

/// The build, once per platform, `TARGET` set.
const BUILD: &str = "\
if [ \"$(uname)\" = Linux ]; then
  sudo apt-get update -q
  sudo apt-get install -y --no-install-recommends mold pkg-config libssl-dev \
    libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
    libayatana-appindicator3-dev libdbus-1-dev libfontconfig1-dev libwayland-dev libxdo-dev
fi
cargo build --release --locked --target \"$TARGET\" -p setlists --features extension --bin studio-setlists
";

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "setlists-extension",
        yaml(
            r#"
push:
  tags: ["v*"]
workflow_dispatch:
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    .job("extension", extension()?))
}

fn extension() -> Result<Job> {
    Ok(Job::calling(&format!(
        "FastTrackStudios/architect/.github/workflows/extension-release.yml@{ARCHITECT_REF}"
    ))
    .name("Setlists extension")
    .cond("${{ startsWith(github.ref, 'refs/tags/v') }}")
    // The sign job uploads to the Release.
    .permissions(yaml("contents: write\n")?)
    .with("package", "features/setlists")
    .with("build", BUILD)
    .with("binary", "target/$TARGET/release/studio-setlists")
    .with("platforms", PLATFORMS)
    .with("runners", RUNNERS)
    .with("architect-ref", ARCHITECT_REF)
    .with("registry", "FastTrackStudios/architect")
    .with("registry-dir", "extensions")
    // An environment secret, passed by name: the called sign job runs in
    // release-signing (tags only) and sees it only when passed.
    .secret("EXT_SIGNING_KEY", "${{ secrets.FTS_RELEASE_MINISIGN_KEY }}"))
}
