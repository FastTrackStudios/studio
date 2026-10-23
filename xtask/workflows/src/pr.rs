//! `PR` — the free backstop every PR into main gets, and nothing else.
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

/// Every check below is skipped on a release commit.
const NOT_RELEASE: &str = "${{ !cancelled() && steps.release.outputs.only != 'true' }}";

pub fn workflow() -> Result<Workflow> {
    Ok(
        Workflow::new("PR", yaml("pull_request:\n  branches: [main]\n")?)
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
    )
    .into_iter()
    .map(|s| s.cond("${{ steps.release.outputs.only != 'true' }}"));
    Ok(Job::new(UBUNTU)
        .name("PR checks")
        .timeout(15)
        .permissions(yaml("contents: read\n")?)
        // The merge commit and its parents: the change check diffs against
        // the first parent (the base it merges into). A release PR needs
        // full history for the release-only test.
        .step(checkout().with(
            "fetch-depth",
            "${{ github.head_ref == 'release/pending' && 0 || 2 }}",
        ))
        // A release PR (release/pending: version + changelog only) was
        // checked commit by commit before it. The BASE commit's release.sh
        // decides, never the PR's own copy: a release/pending branch that
        // edited the script must not be able to skip the checks.
        .step(
            Step::run(
                "Release commit?",
                "\
git show \"$BASE_SHA:scripts/release.sh\" > \"$RUNNER_TEMP/release-base.sh\"
if bash \"$RUNNER_TEMP/release-base.sh\" release-only; then
  echo \"only=true\" >> \"$GITHUB_OUTPUT\"
  echo \"release commit only: nothing to check\"
fi
",
            )
            .id("release")
            .cond("${{ github.head_ref == 'release/pending' }}")
            .env("BASE_SHA", "${{ github.event.pull_request.base.sha }}")
            .env("BASE_REF", "${{ github.event.pull_request.base.ref }}")
            .env("HEAD_SHA", "${{ github.event.pull_request.head.sha }}")
            .env("HEAD_REF", "${{ github.head_ref }}")
            .env(
                "HEAD_REPO",
                "${{ github.event.pull_request.head.repo.full_name }}",
            ),
        )
        .steps(setup)
        // Main's cargo registry + git checkouts (CI's `deps-cache` job saves
        // them): without it the first cargo command spends most of a minute
        // fetching every git dependency.
        .step(cargo_deps_restore().cond("${{ steps.release.outputs.only != 'true' }}"))
        .step(Step::run("fmt", "cargo fmt --all --check").cond(NOT_RELEASE))
        // Every crate the PR changes needs a cargo-rail change file
        // (.changes/), against the base this PR merges into.
        .step(
            Step::run(
                "Change files",
                "cargo rail change check --since \"$(git rev-parse HEAD^1)\"",
            )
            .cond(NOT_RELEASE),
        )
        .step(Step::run("Manifests + lockfile", LOCKFILE).cond(NOT_RELEASE))
        .step(Step::run("rail config", "cargo rail config validate --strict").cond(NOT_RELEASE)))
}
