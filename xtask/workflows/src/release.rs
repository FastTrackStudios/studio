//! `release` — the GitHub Release of a tag, and the `Release guard` check.
//! The flow is tag-from-main (docs/agents/release-workflow.md); the logic
//! is scripts/release.sh, so a person can run every step by hand.
//!
//! ```text
//!   push tag v*            "GitHub Release": stamp the tag's version, the
//!                          tag guard (vX.Y.Z on a commit of main or
//!                          release/v*, `CI passed` already green: the suite
//!                          is not run again), then the ONE GitHub Release
//!                          with the notes from the change files added since
//!                          the previous tag. It waits for nothing: the
//!                          image, TestFlight and the installers start from
//!                          the same tag in parallel and attach or record
//!                          into the Release as they finish. The tag is
//!                          pushed by `just release` as a person or the
//!                          fts-forge App: a tag pushed with GITHUB_TOKEN
//!                          starts no workflow.
//!   workflow_dispatch      the same, by hand, with the tag as the ref.
//!   any PR                 "Release guard": a REQUIRED check of main's
//!                          ruleset. It judged release PRs; there are none
//!                          now (a release is a tag), so it passes at once
//!                          and says why. Kept so the ruleset still sees the
//!                          check it requires.
//! ```

use crate::model::{Job, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout_full, stamp};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "release",
        yaml(
            r#"
# Every base, not just main and release/**: on a gh stack each layer's
# base is the layer below, and the atomic stack merge wants `Release
# guard` on every PR.
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

/// Passes on every PR, saying why: no checkout, nothing to judge.
fn guard() -> Job {
    Job::new(UBUNTU)
        .name("Release guard")
        .cond("${{ github.event_name == 'pull_request' }}")
        .timeout(5)
        .step(Step::run(
            "No release PRs: releases are tags",
            r####"{
  echo "### Release guard: nothing to judge"
  echo "Releases are tags pushed by \`just release\` on a green commit of main (or release/v*); no PR is a release PR. This check stays because main's ruleset requires it."
} >> "$GITHUB_STEP_SUMMARY"
echo "release guard: releases are tags (just release); no release PRs, nothing to check"
"####,
        ))
}

fn publish() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("GitHub Release")
        .cond("${{ github.event_name != 'pull_request' && startsWith(github.ref, 'refs/tags/v') }}")
        .timeout(5)
        .permissions(yaml("contents: write\nchecks: read\n")?)
        .concurrency("release-publish", false)
        .step(checkout_full(Some("${{ github.sha }}")))
        .step(stamp("${{ github.ref_name }}"))
        .step(
            Step::run(
                "The tag is a green commit on main or release/v*",
                "bash scripts/release.sh tag-guard",
            )
            .env("GH_TOKEN", "${{ github.token }}"),
        )
        .step(
            Step::run("Publish", "bash scripts/release.sh publish")
                .env("GH_TOKEN", "${{ github.token }}"),
        ))
}
