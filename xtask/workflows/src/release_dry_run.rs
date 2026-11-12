//! `release-dry-run` — the release MECHANICS against a throwaway draft
//! (scripts/release-dry-run.sh): draft, uploads, scripts/release-sums.sh,
//! signing with an ephemeral key, publishing, on a release named
//! `dry-run-<run id>` that is never a `v*` tag, never Latest, and is deleted
//! with its tag at the end. The cases: an injected asset never signed, a
//! replaced asset refused, a second workflow building on the first one's
//! verified sums, publishing only the draft, a re-run leaving it alone.
//!
//! On every PR that touches those mechanics (hosted runner, the workflow's
//! own token; a fork's PR gets no write token and is skipped), and by hand.

use crate::model::{Job, Step, UBUNTU, Workflow, pinned, yaml};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "release-dry-run",
        yaml(
            "pull_request:\n  paths:\n  - scripts/release.sh\n  - scripts/release-sums.sh\n  - scripts/release-publish.sh\n  - scripts/release-dry-run.sh\n  - xtask/workflows/src/sums.rs\n  - xtask/workflows/src/release_dry_run.rs\nworkflow_dispatch: null\n",
        )?,
    )
    .permissions(yaml("{}")?)
    .concurrency(
        "release-dry-run-${{ github.event.pull_request.number || github.run_id }}",
        true,
    )
    .job("dry-run", dry_run()?))
}

fn dry_run() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Release mechanics on a throwaway draft")
        .cond("${{ github.event_name == 'workflow_dispatch' || github.event.pull_request.head.repo.full_name == github.repository }}")
        .timeout(10)
        .permissions(yaml("contents: write\n")?)
        .step(
            Step::action(pinned::CHECKOUT)
                .with("fetch-depth", 0)
                .with("persist-credentials", false),
        )
        .step(Step::run("minisign (pinned)", crate::sums::INSTALL_MINISIGN))
        .step(
            Step::run("scripts/release-dry-run.sh", "bash scripts/release-dry-run.sh\n")
                .env("GH_TOKEN", "${{ github.token }}"),
        ))
}
