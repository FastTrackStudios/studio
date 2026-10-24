//! `PR` — the free backstop every PR gets (into main, or into the layer
//! below on a gh stack), and nothing else.
//!
//! The expensive half of the PR gate runs on the author's machine: the prek
//! pre-push hook (prek.toml -> scripts/ci-local-hook.sh -> scripts/
//! ci-local.sh) runs the static guards, the tests, `cargo check` and the
//! wasm check `cargo rail plan` says the change affects, and posts the
//! `Local CI` status. This workflow re-checks the cheap, compile-nothing
//! part on a free hosted runner, without nix, for a push that skipped the
//! hook: fmt, change files, the lockfile, the rail config.
//!
//! Main requires `PR checks` (this job), `Local CI` and `Release guard`
//! (release.yml). The full set (`CI`, checks.yml) runs after every merge to
//! main, on hotfix PRs into release/**, and by hand:
//!
//! ```text
//! gh workflow run checks.yml --ref <branch>     (posts Local CI when green)
//! ```
//!
//! Its own workflow, not a job of `CI`: a PR shows `PR checks` and `Local
//! CI`, not a row of skipped CI jobs (Task, 2026-10).

use crate::checks::LOCKFILE;
use crate::model::{Job, Step, UBUNTU, Workflow, yaml};
use crate::steps::{Tools, cargo_deps_restore, checkout, hosted_setup};
use anyhow::Result;

/// From the change files on, a step runs even when fmt failed, so one run
/// reports every problem; the job still fails.
const NOT_CANCELLED: &str = "${{ !cancelled() }}";

pub fn workflow() -> Result<Workflow> {
    Ok(
        // Every base, not just main: on a gh stack each layer's base is the
        // layer below, and GitHub's atomic stack merge requires `PR checks`
        // on every PR of the stack. Change files are checked against the
        // merge commit's first parent, so each layer against its own base.
        Workflow::new(
            "PR",
            yaml("pull_request:\n  types: [opened, synchronize, reopened, ready_for_review]\n")?,
        )
        .concurrency("pr-${{ github.event.pull_request.number }}", true)
        .job("pr-checks", pr_checks()?),
    )
}

fn pr_checks() -> Result<Job> {
    let setup = hosted_setup(
        "rustfmt",
        "",
        Tools {
            cargo_rail: true,
            ..Tools::default()
        },
    );
    Ok(Job::new(UBUNTU)
        .name("PR checks")
        .timeout(15)
        .permissions(yaml("contents: read\n")?)
        // The merge commit and its parents: the change check diffs against
        // the first parent (the base it merges into).
        .step(checkout().with("fetch-depth", 2))
        .steps(setup)
        // Main's cargo registry + git checkouts (CI's `deps-cache` job saves
        // them): without it the first cargo command spends most of a minute
        // fetching every git dependency.
        .step(cargo_deps_restore())
        .step(Step::run("fmt", "cargo fmt --all --check"))
        // Every crate the PR changes needs a cargo-rail change file
        // (.changes/), against the base this PR merges into.
        .step(
            Step::run(
                "Change files",
                "cargo rail change check --since \"$(git rev-parse HEAD^1)\"",
            )
            .cond(NOT_CANCELLED),
        )
        .step(Step::run("Manifests + lockfile", LOCKFILE).cond(NOT_CANCELLED))
        .step(Step::run("rail config", "cargo rail config validate --strict").cond(NOT_CANCELLED)))
}
