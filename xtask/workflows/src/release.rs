//! `release` — the GitHub Release of a tag.
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
//! ```

use crate::model::{Job, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout_full, stamp};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "release",
        yaml(
            r#"
push:
  tags: ["v*"]
workflow_dispatch:
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    .job("publish", publish()?))
}

fn publish() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("GitHub Release")
        .cond("${{ startsWith(github.ref, 'refs/tags/v') }}")
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
