//! `PR` — what every PR gets (into main, or into the layer below on a gh
//! stack).
//!
//! ```text
//!   PR checks        the gates that compile nothing, on a free hosted runner
//!                    without nix: fmt, change files, the lockfile, the rail
//!                    config, the group rules (boundaries.toml), Ignition's
//!                    spec coverage, the release bots' tests
//!   Affected crates  what the change reaches, from `cargo rail plan`
//!                    against the PR's base: `cargo check` of those crates,
//!                    their tests (the tested set, scripts/boundaries.py),
//!                    and the wasm check when a browser build is among them
//!                    — and, on every PR, the fixture sessions played over
//!                    the engine's MCP door (daw-extension's `fixtures` test)
//!                    and a room led and followed by two engines (`room`)
//!   Architect pin    a PR that moves the Architect pin starts `CI` on its
//!                    branch (below)
//! ```
//!
//! The prek pre-push hook (prek.toml -> scripts/ci-local-hook.sh ->
//! scripts/ci-local.sh) runs the same plan on the author's warm target dir
//! first and posts the `Local CI` status, an indicator. The full set (`CI`,
//! checks.yml) runs after every merge to main, on hotfix PRs into
//! release/**, and by hand:
//!
//! ```text
//! gh workflow run checks.yml --ref <branch>     (posts Local CI when green)
//! ```
//!
//! Its own workflow, not a job of `CI`: a PR shows `PR checks` and `Local
//! CI`, not a row of skipped CI jobs (Task, 2026-10).
//!
//! A DRAFT PR runs nothing: both jobs are skipped (`NOT_DRAFT`), and a
//! skipped `PR checks` still reports, so nothing waits on it. Marking the PR
//! ready for review (`ready_for_review`) starts the run; every push after
//! that runs it again and cancels the run before it.

use crate::checks::{
    LOCKFILE, POOL, assets, ci_cargo_config, hosted_cargo_job, in_shell, target_dir,
};
use crate::model::{Job, Step, UBUNTU, Workflow, yaml};
use crate::steps::{Tools, cargo_deps_restore, checkout, hosted_setup};
use anyhow::Result;

/// From the change files on, a step runs even when fmt failed, so one run
/// reports every problem; the job still fails.
const NOT_CANCELLED: &str = "${{ !cancelled() }}";

/// Every job here: skipped while the PR is a draft. A job skipped by its
/// `if:` reports as skipped, which satisfies a required check; a workflow
/// that never triggered reports nothing and would leave `PR checks` pending.
const NOT_DRAFT: &str =
    "${{ github.event_name != 'pull_request' || github.event.pull_request.draft == false }}";

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
        .job("affected", affected()?)
        .job("architect-pin", architect_pin()?),
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
        .cond(NOT_DRAFT)
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
        // The group rules (boundaries.toml): cargo metadata only, seconds. The
        // promise (a `cargo tree` per member) runs in `CI`'s Gates and in
        // local CI when a manifest changed.
        .step(
            Step::run("Group rules (boundaries.toml)", "python3 scripts/boundaries.py check --edges")
                .cond(NOT_CANCELLED),
        )
        .step(
            Step::run(
                "Spec coverage (Ignition)",
                "python3 -I scripts/spec-coverage.py --scope lighting",
            )
            .cond(NOT_CANCELLED),
        )
        // Every App token: one repository, its hop's permissions, its
        // environment's own key (scripts/check-token-mints.py's ALLOWED).
        .step(
            Step::run(
                "Release bots: token mints, sums, facts, queue, bump dependents, www patch",
                "set -euo pipefail\npython3 scripts/check-token-mints.py --self-test\npython3 scripts/check-token-mints.py\nbash scripts/release-facts-test.sh\nbash scripts/release-sums-test.sh\nbash scripts/bump-verify-test.sh\npython3 scripts/bump-architect-deps-test.py\npython3 scripts/check-www-patch.py --self-test\npython3 scripts/check-www-patch.py\nbash scripts/lock-sources-test.sh\n",
            )
            .cond(NOT_CANCELLED),
        ))
}

/// The plan against the PR's base (the merge commit's first parent), as
/// scripts/ci-rail-plan.py's gates; rail failing, or a plan it can't read,
/// is the full set (fail open, never narrow on a guess). No rail evidence on
/// a PR: a change outside every crate's sources widens to the workspace.
const AFFECTED_PLAN: &str = r#"base="$(git rev-parse HEAD^1)"
plan="$RUNNER_TEMP/rail-plan.json"
gates=""
if cargo rail plan --from "$base" --to HEAD --json > "$plan"; then
  gates="$(python3 scripts/ci-rail-plan.py "$plan")" || gates=""
fi
[ -n "$gates" ] || { echo "plan: no plan; the full set"; gates=$'full=true\ncargo=true\ncheck=true\ncheck_packages=\ntests=true\ntest_filter=\ntest_packages=\nwasm=true'; }
cargo rail plan --from "$base" --to HEAD --explain 2>&1 | head -n 40 || true
printf '%s\n' "$gates" | tee -a "$GITHUB_OUTPUT"
"#;

/// `cargo check` of the packages the plan names (every one of them when it
/// names none: the whole workspace).
const AFFECTED_CHECK: &str = r#"pkgs=()
# A member can share its name with a registry crate (vendor/wasm-streams-0.4 is
# `wasm-streams` 0.4, beside 0.5 from crates.io): `-p name` is then ambiguous,
# so pass each member as `name@version`.
declare -A spec=()
while read -r n v; do spec[$n]="$n@$v"; done < <(cargo metadata --no-deps --format-version 1 --offline 2>/dev/null | python3 -c 'import json,sys; [print(p["name"], p["version"]) for p in json.load(sys.stdin)["packages"]]')
for p in ${CHECK_PACKAGES:-}; do pkgs+=(-p "${spec[$p]:-$p}"); done
if [ "${#pkgs[@]}" = 0 ]; then cargo check --workspace; else cargo check "${pkgs[@]}"; fi
"#;

/// The tested set (scripts/boundaries.py packages all --tested) narrowed to
/// the plan's selection; an empty selection is all of it.
const AFFECTED_TESTS: &str = r#"pkgs=()
# A member can share its name with a registry crate (vendor/wasm-streams-0.4 is
# `wasm-streams` 0.4, beside 0.5 from crates.io): `-p name` is then ambiguous,
# so pass each member as `name@version`.
declare -A spec=()
while read -r n v; do spec[$n]="$n@$v"; done < <(cargo metadata --no-deps --format-version 1 --offline 2>/dev/null | python3 -c 'import json,sys; [print(p["name"], p["version"]) for p in json.load(sys.stdin)["packages"]]')
sel=" ${TEST_PACKAGES:-} "
while read -r p; do
  if [ "$sel" = "  " ] || [[ "$sel" == *" $p "* ]]; then pkgs+=(-p "${spec[$p]:-$p}"); fi
done < <(python3 scripts/boundaries.py packages all --tested)
if [ "${#pkgs[@]}" = 0 ]; then echo "no tested package affected"; exit 0; fi
cargo nextest run "${pkgs[@]}" --no-fail-fast --no-tests=pass
"#;

/// The fixture sessions and setlist (daw-extension's `fixtures` test) and
/// a room with a leader and a member (its `room` test), whatever the plan
/// selected: real engines opened and played over MCP.
/// Skipped when the affected tests already ran daw-extension's.
const FIXTURES: &str = r#"sel=" ${TEST_PACKAGES:-} "
if [ "${TESTS_RAN:-}" = "true" ] && { [ "$sel" = "  " ] || [[ "$sel" == *" daw-extension "* ]]; }; then
  echo "daw-extension's tests ran above, the fixtures and the room with them"; exit 0
fi
cargo nextest run -p daw-extension --test fixtures --test room --no-fail-fast
"#;

/// The open and join times (studio#717) the fixture and room tests wrote
/// this run, as a table in the job's summary: each record under the
/// target dir's `fixtures/` and `room/` whose commit is this run's. The
/// tests fail on a time over its budget; this only reports them.
const SPEED: &str = r###"python3 - <<'PY' || echo "::warning::the open and join times could not be read"
import glob, json, os
target = os.environ.get("CARGO_TARGET_DIR") or "target"
sha = os.environ["GITHUB_SHA"][:12]
rows = []
for f in sorted(glob.glob(f"{target}/**/fixtures/*.json", recursive=True)
                + glob.glob(f"{target}/**/room/room.json", recursive=True)):
    try:
        record = json.load(open(f))
    except (OSError, ValueError):
        continue
    if (record.get("commit") or {}).get("sha") != sha:
        continue
    test = record.get("fixture") or record.get("test") or os.path.basename(f)
    for t in record.get("times") or []:
        rows.append((t["measure"], test, t["what"], t["ms"], t.get("budget_ms"), t["verdict"]))
out = ["## Session open and room join", ""]
if rows:
    out += ["| measure | test | what | ms | budget ms | |", "|---|---|---|--:|--:|---|"]
    out += [f"| {m} | {t} | {w} | {ms:.0f} | {'-' if b is None else f'{b:.0f}'} | {v} |"
            for m, t, w, ms, b, v in sorted(rows)]
else:
    out.append(f"No times from this commit ({sha}) under {target}: the tests did not run, or failed before timing.")
text = "\n".join(out) + "\n"
print(text)
with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as s:
    s.write(text)
PY
"###;

/// The crates the PR reaches, compiled and tested: one job, one target dir
/// (the deps build once; the tests reuse the check's metadata). Each step
/// runs even when one before it failed, so one run reports the check AND the
/// tests. On a PR it runs on a GitHub-hosted runner, as every PR build here
/// does (`POOL`): the shared pool runs only protected refs' code.
fn affected() -> Result<Job> {
    const RAN: &str = "!cancelled() && steps.plan.outcome == 'success'";
    Ok(hosted_cargo_job(Job::new(POOL))
        .name("Affected crates")
        .cond(NOT_DRAFT)
        .timeout(180)
        .permissions(yaml("contents: read\n")?)
        // The merge commit and its parents: the plan diffs against the first.
        .step(checkout().with("fetch-depth", 2).with("persist-credentials", false))
        .step(
            Step::uses("Hosted setup", "./.github/actions/hosted-nix")
                .cond("${{ runner.environment == 'github-hosted' }}"),
        )
        .step(
            Step::uses("Shared runner tools", "./.github/actions/shared-tools")
                .cond("${{ runner.environment == 'self-hosted' }}"),
        )
        .step(ci_cargo_config())
        .step(target_dir("affected"))
        .step(Step::run("cargo rail plan (against the PR's base)", &in_shell(AFFECTED_PLAN)).id("plan"))
        .step(
            Step::run("cargo check (the affected crates)", &in_shell(AFFECTED_CHECK))
                .cond(&format!(
                    "${{{{ {RAN} && (steps.plan.outputs.check == 'true' || steps.plan.outputs.cargo == 'true') }}}}"
                ))
                .env("CHECK_PACKAGES", "${{ steps.plan.outputs.check_packages }}"),
        )
        // No asset token on a PR (its code is untrusted): the clones the
        // runner holds are linked, else the tests that need them skip.
        .step(
            assets()
                .env("ASSETS_TOKEN", "")
                .cond(&format!("${{{{ {RAN} && steps.plan.outputs.tests == 'true' }}}}")),
        )
        .step(
            Step::run("cargo nextest (the affected crates' tests)", &in_shell(AFFECTED_TESTS))
                .cond(&format!("${{{{ {RAN} && steps.plan.outputs.tests == 'true' }}}}"))
                .env("TEST_PACKAGES", "${{ steps.plan.outputs.test_packages }}"),
        )
        // Seam 1's base (studio#691) and Seam 2's (studio#709), on every PR
        // whatever it touches: the fixture sessions opened and played over
        // the engine's MCP door, and a room led and followed by two engines.
        .step(
            Step::run("Fixture sessions and a room over MCP (every PR)", &in_shell(FIXTURES))
                .cond(&format!("${{{{ {RAN} }}}}"))
                .env("TEST_PACKAGES", "${{ steps.plan.outputs.test_packages }}")
                .env("TESTS_RAN", "${{ steps.plan.outputs.tests }}"),
        )
        // What they timed, reported (studio#717): the times ran above,
        // in the affected tests or the step before.
        .step(
            Step::run("Session open and room join times", &in_shell(SPEED))
                .cond(&format!("${{{{ {RAN} }}}}")),
        )
        .step(
            Step::run("wasm check (just check-web)", &in_shell("just check-web\n"))
                .cond(&format!("${{{{ {RAN} && steps.plan.outputs.wasm == 'true' }}}}")),
        ))
}

/// The PR moves the Architect pin: a line of the root manifest that names
/// FastTrackStudios/architect changed (a rev, a tag, the `www.` patch).
const PIN_MOVED: &str = r#"set -euo pipefail
moved="$(git diff HEAD^1 HEAD -- Cargo.toml | grep -E '^[+-][^+-].*FastTrackStudios/architect' || true)"
if [ -n "$moved" ]; then
  echo "moved=true" >> "$GITHUB_OUTPUT"
  echo "architect-pin: this PR moves the Architect pin:"
  echo "$moved"
else
  echo "moved=false" >> "$GITHUB_OUTPUT"
  echo "architect-pin: the Architect pin stays where it is"
fi
"#;

/// Start `CI` (checks.yml, the full set main runs) on the PR's branch.
const RUN_CI: &str = r####"set -euo pipefail
gh workflow run checks.yml --repo "$GITHUB_REPOSITORY" --ref "$HEAD_REF"
{
  echo "### The Architect pin moves: main's full set runs on this branch"
  echo
  echo "CI (checks.yml) was started on $HEAD_REF: its jobs report on this PR's head commit."
} >> "$GITHUB_STEP_SUMMARY"
"####;

/// A PR that moves the Architect pin runs what main runs, before it merges.
///
/// A PR's own jobs compile nothing but the node (`studio`, `cargo
/// check`), while main runs the duplicate-version gate, every crate's test
/// targets and the core test set. A pin move reaches 27 of the workspace's
/// 52 crates, and studio#519's did break main on all three: an Architect
/// function's arguments in two test targets, five node tests against
/// Architect's #646, two new duplicate versions (fixed in studio#532).
///
/// So when the PR moves the pin, this job starts `CI` itself on the PR's
/// branch: a `workflow_dispatch` run, the full set on THEBATTLESHIP's warm
/// pool (checks.yml), whose jobs report on the PR's head commit. Not
/// required, like everything here but `PR checks`; the PR is merged once
/// its `CI passed` is green. Not for a PR from a fork (no token to start a
/// run with; the pool runs only this repo's branches).
fn architect_pin() -> Result<Job> {
    const MOVED: &str = "${{ steps.pin.outputs.moved == 'true' }}";
    Ok(Job::new(UBUNTU)
        .name("Architect pin: main's full set")
        .cond(
            "${{ (github.event_name != 'pull_request' || github.event.pull_request.draft == false) \
             && github.event.pull_request.head.repo.full_name == github.repository }}",
        )
        .timeout(5)
        .permissions(yaml("contents: read\nactions: write\n")?)
        .step(checkout().with("fetch-depth", 2))
        .step(Step::run("Does this PR move the Architect pin?", PIN_MOVED).id("pin"))
        .step(
            Step::run("Start CI on this branch", RUN_CI)
                .cond(MOVED)
                .env("GH_TOKEN", "${{ github.token }}")
                .env("HEAD_REF", "${{ github.head_ref }}"),
        ))
}
