//! `release` — the guard on every PR into main or a release/v* hotfix line,
//! and the GitHub Release once a release tag is pushed. The fleet flow (v4)
//! is the `release-workflow` skill; studio specifics are in
//! docs/agents/release-workflow.md; the logic is scripts/release.sh, so a
//! human can run every step of it by hand (`just release-*`).
//!
//! ```text
//!   PR into main or release/**   "Release guard": cheap, on a hosted
//!                          runner. Only a release PR (head `release/pending`
//!                          in this repo) is checked; any other PR passes at
//!                          once. A release PR must be ONE commit
//!                          `chore(release): vYY.M.MICRO` directly on the
//!                          CURRENT head of its base (fetched now, not the
//!                          event's base.sha), at the computed next CalVer
//!                          version, touching only the version line(s), the
//!                          new CHANGELOG.md section and the change files it
//!                          consumed; both iOS app crates on the version.
//!                          checks.yml's plan runs the same guard, so a
//!                          refused release PR also fails `CI passed`.
//!   push tag v*            "GitHub Release": the tag guard (the tag is a
//!                          release commit on main or release/v*, with
//!                          `CI passed` green), then the ONE GitHub
//!                          Release for Signal + Session. A tag is pushed by
//!                          `just release-tag` as the fts-forge App or a
//!                          person: a tag pushed with GITHUB_TOKEN starts no
//!                          workflow.
//!   workflow_dispatch      the same, by hand, with the tag as the ref.
//! ```
//!
//! SECURITY: the guard runs the BASE commit's copy of scripts/release.sh,
//! never the PR's: a release PR that edits the script must not judge itself.
//!
//! The release does not wait for deploy.yml: the product is the apps and the
//! plugins, and the two web sites are one leg of it. A failed site deploy is
//! its own red run, not a missing Release.

use crate::model::{Job, Step, UBUNTU, Workflow, yaml};
use crate::steps::checkout_full;
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "release",
        yaml(
            r#"
# Every base, not just main and release/**: on a gh stack each layer's
# base is the layer below, and the atomic stack merge wants `Release
# guard` on every PR (it skips, which counts as passing, unless the PR
# is a release).
pull_request:
  types: [opened, synchronize, reopened, ready_for_review]
push:
  tags: ["v*"]
workflow_dispatch:
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    .job("guard", guard())
    .job("publish", publish()?))
}

/// Only release PRs (head `release/*`, e.g. `release/pending`) and PRs into
/// a `release/**` hotfix line. Every other PR skips it at once (no runner;
/// a skipped required check counts as passed).
fn guard() -> Job {
    Job::new(UBUNTU)
        .name("Release guard")
        .cond(
            "${{ github.event_name == 'pull_request' && \
             (startsWith(github.head_ref, 'release/') || startsWith(github.base_ref, 'release/')) }}",
        )
        .timeout(5)
        .step(checkout_full(Some("${{ github.event.pull_request.head.sha }}")))
        .step(
            Step::run(
                "Is this release PR sound? (the base commit's script)",
                r#"if ! git show "${BASE_SHA}:scripts/release.sh" > "${RUNNER_TEMP}/release-base.sh"; then
  echo "::notice::the base has no scripts/release.sh: nothing to guard"
  exit 0
fi
bash "${RUNNER_TEMP}/release-base.sh" guard
"#,
            )
            .env("BASE_SHA", "${{ github.event.pull_request.base.sha }}")
            .env("BASE_REF", "${{ github.event.pull_request.base.ref }}")
            .env("HEAD_SHA", "${{ github.event.pull_request.head.sha }}")
            .env("HEAD_REF", "${{ github.event.pull_request.head.ref }}")
            .env("HEAD_REPO", "${{ github.event.pull_request.head.repo.full_name }}"),
        )
}

fn publish() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("GitHub Release")
        .cond("${{ github.event_name != 'pull_request' && startsWith(github.ref, 'refs/tags/v') }}")
        .timeout(5)
        .permissions(yaml("contents: write\nchecks: read\n")?)
        .concurrency("release-publish", false)
        .step(checkout_full(Some("${{ github.sha }}")))
        .step(
            Step::run(
                "The tag is a green release commit on main or release/v*",
                "bash scripts/release.sh tag-guard",
            )
            .env("GH_TOKEN", "${{ github.token }}"),
        )
        .step(
            Step::run("Publish", "bash scripts/release.sh publish")
                .env("GH_TOKEN", "${{ github.token }}"),
        ))
}
