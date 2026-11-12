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
//! Main requires only `PR checks` (this job); `Local CI` is an indicator,
//! not a gate. The full set (`CI`, checks.yml) runs after every merge to
//! main, on hotfix PRs into release/**, and by hand:
//!
//! ```text
//! gh workflow run checks.yml --ref <branch>     (posts Local CI when green)
//! ```
//!
//! One job here compiles: `studio-architect`, `cargo check` of Studio's
//! distribution of Architect's node for x86_64 Linux (apps/architect, the
//! release's `studio-architect` workflow builds it), and only when the PR
//! reaches it; and, when the PR reaches Studio's island's sheet, compiles
//! that sheet and puts it through Architect's `cleanSheet`. Not required.
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
        .job("pr-checks", pr_checks()?)
        .job("studio-architect", studio_architect()?),
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
        .step(Step::run("rail config", "cargo rail config validate --strict").cond(NOT_CANCELLED))
        // Every App token: one repository, its hop's permissions, its
        // environment's own key (scripts/check-token-mints.py's ALLOWED).
        .step(
            Step::run(
                "App token mints",
                "python3 scripts/check-token-mints.py --self-test && python3 scripts/check-token-mints.py",
            )
            .cond(NOT_CANCELLED),
        ))
}

/// The PR's changed files reach the node: a crate of this repo in its
/// tree (`cargo tree`, normal, build and its own dev edges), the manifest,
/// the lockfile, the toolchain, the cargo config or its build script.
const REACH: &str = r#"set -euo pipefail
changed="$(git diff --name-only HEAD^1 HEAD)"
dirs="$(cargo tree --locked -p studio-architect --no-default-features -e normal,build,dev --prefix none -f '{p}' \
  | sed -n "s|.* (\($GITHUB_WORKSPACE/[^)]*\))\$|\1|p" | sed "s|^$GITHUB_WORKSPACE/||" | sort -u)"
reach=false
while IFS= read -r f; do
  [ -n "$f" ] || continue
  case "$f" in
    Cargo.toml|Cargo.lock|rust-toolchain.toml|.cargo/*|scripts/studio-architect.sh) reach=true ;;
  esac
  while IFS= read -r d; do
    [ -n "$d" ] || continue
    case "$f" in "$d"/*) reach=true ;; esac
  done <<<"$dirs"
done <<<"$changed"
echo "reach=$reach" >> "$GITHUB_OUTPUT"
echo "studio-architect: $(wc -l <<<"$dirs") crates of this repo in the node's tree; this PR reaches it: $reach"
# Studio's island's sheet: its source, the classes it is compiled for, the
# check, the lockfile (which pins the sanitiser it is checked against), and
# Tailwind's version (the dev shell's, devenv.nix/devenv.lock; CI's pin in
# xtask/workflows/src/steps.rs): a Tailwind bump that breaks the sheet
# fails here, not at release.
island=false
while IFS= read -r f; do
  case "$f" in
    Cargo.toml|Cargo.lock|devenv.nix|devenv.lock|devenv.yaml|xtask/workflows/src/steps.rs|apps/architect/island/*|features/ui/transport-ui/*|scripts/island-sheet*) island=true ;;
  esac
done <<<"$changed"
echo "island=$island" >> "$GITHUB_OUTPUT"
echo "studio-architect: this PR reaches the island's sheet: $island"
"#;

/// Studio's distribution of Architect still builds for the hosted node:
/// `cargo check --all-targets` of its node for x86_64 Linux, exactly as the
/// release builds it (scripts/studio-architect.sh: `--no-default-features`,
/// `--cfg no_workspace_hack`). A free hosted runner with no target cache
/// (PRs never write the Actions cache): a cold check, 6m39s on its first
/// run (PR #407), so it runs only for a PR that reaches the node
/// ([`REACH`]) and nothing waits on it (main requires `PR checks` only).
fn studio_architect() -> Result<Job> {
    const REACHED: &str = "${{ steps.reach.outputs.reach == 'true' }}";
    const ISLAND: &str = "${{ steps.reach.outputs.island == 'true' }}";
    const EITHER: &str =
        "${{ steps.reach.outputs.reach == 'true' || steps.reach.outputs.island == 'true' }}";
    let setup = hosted_setup(
        "",
        "",
        // As checks.rs's hosted Gates job, which compiles on ubuntu-latest:
        // mold alone (.cargo/config.toml links x86_64 Linux with it) and the
        // CI cargo config. Not the apt header set: the node's tree has no
        // crate that needs a native header, and with the cached package set
        // gcc found no `ld` (PR #407, run 43119996719).
        Tools {
            mold: true,
            free_disk: true,
            ci_cargo_config: true,
            tailwind: true,
            ..Tools::default()
        },
    );
    Ok(Job::new(UBUNTU)
        .name("Studio Architect node (cargo check, Linux)")
        .timeout(45)
        .permissions(yaml("contents: read\n")?)
        .step(checkout().with("fetch-depth", 2))
        .step(cargo_deps_restore())
        // The image's rustup reads rust-toolchain.toml: the workspace pin.
        .step(Step::run("Does this PR reach the node?", REACH).id("reach"))
        .steps(setup.into_iter().map(|s| s.cond(EITHER)))
        .step(
            Step::run(
                "cargo check the node",
                "bash scripts/studio-architect.sh check",
            )
            .cond(REACHED),
        )
        // The island's sheet as a share page will hold it: compiled, then
        // put through the pinned Architect's `cleanSheet` (studio#445).
        // Refused, the island would draw unstyled; here the PR fails.
        .step(
            Step::run(
                "The island's sheet passes Architect's cleanSheet",
                "bash scripts/island-sheet.sh",
            )
            .cond(ISLAND),
        ))
}
