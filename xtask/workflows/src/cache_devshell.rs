//! `cache-devshell` — seed fasttrackstudio.cachix.org with the dev shell
//! closures (the rust pin, the dx trio, cargo-rail, tracey, the native deps),
//! so a DEVELOPER's `nix develop` (and the self-hosted NixOS runners' `nix
//! develop .#ci`) substitutes them instead of building.
//!
//! No CI job depends on it any more: the GitHub-hosted checks jobs stopped
//! using nix (they install a pinned rustup toolchain, apt headers and release
//! binaries; see `crate::steps`), and the self-hosted jobs only gain speed
//! from a warm cache, never correctness. A red or skipped run here costs a
//! developer a slower first `nix develop`, nothing more.
//!
//! Needs the CACHIX_AUTH_TOKEN repo/org secret (a cachix auth token with
//! write access to the fasttrackstudio cache). Without it the workflow skips
//! pushing and merely reports what would be cached.
//!
//! Runs on a GitHub-HOSTED runner (.github/actions/hosted-nix installs nix
//! and cachix): the shells are almost entirely cache hits (cache.nixos.org +
//! this cache; a dry run builds nothing and fetches ~1.5 GiB), so it is not
//! heavy work. Recent runs took 1-6 minutes.

use crate::model::{Job, Step, UBUNTU, Workflow, yaml};
use crate::steps::checkout;
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "cache-devshell",
        yaml(
            r#"
workflow_dispatch:
push:
  branches: [main]
  paths:
    - "flake.nix"
    - "flake.lock"
    - "nix/**"
"#,
        )?,
    )
    .concurrency("cache-devshell", true)
    .job(
        "seed",
        Job::new(UBUNTU)
            .timeout(180)
            .step(checkout())
            .step(
                Step::action("./.github/actions/hosted-nix")
                    // With a token, cachix-action also pushes whatever this
                    // job builds.
                    .with("cachix-auth-token", "${{ secrets.CACHIX_AUTH_TOKEN }}")
                    .with("shell", ""),
            )
            .step(Step::run(
                "Build CI devshell closure",
                "\
nix build --accept-flake-config --no-link --print-out-paths \\
  .#devShells.x86_64-linux.ci \\
  .#devShells.x86_64-linux.default \\
  | tee devshell-paths.txt
",
            ))
            .step(
                Step::run(
                    "Push to cachix",
                    r#"if [ -z "${CACHIX_AUTH_TOKEN}" ]; then
  echo "CACHIX_AUTH_TOKEN not set — would push:"
  cat devshell-paths.txt
  exit 0
fi
xargs -a devshell-paths.txt cachix push fasttrackstudio
"#,
                )
                .env("CACHIX_AUTH_TOKEN", "${{ secrets.CACHIX_AUTH_TOKEN }}"),
            ),
    ))
}
