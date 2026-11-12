//! `bump-architect` — the Architect (and Keyflow) pin bump, by the bot.
//!
//! When Architect releases a stable tag, a PR moves every Architect pin to
//! it and Keyflow's to the newest keyflow release on that Architect
//! (scripts/bump-architect.py, exactly `just bump-architect vX.Y.Z`), and
//! merges once the FULL suite is green on it, if the bump was mechanical.
//!
//! - `prepare` (no secrets): resolves the tag (the input, or Architect's
//!   latest stable release; never a draft or a pre-release), runs the
//!   script, hands the diff on as an artifact. Script exit 3 (Architect
//!   moved a pin Studio follows, or a second architect source is left in
//!   the lock) still opens the PR, marked for a person. Exit 4 (no keyflow
//!   release on that Architect yet) opens nothing: the hourly run retries
//!   once keyflow is tagged.
//! - `open` (environment `pin-bump`): a short-lived fts-forge token for THIS
//!   repo only (contents, pull-requests, actions write). Pushes
//!   `bump/architect-<tag>`, opens or updates the PR, and dispatches `CI`
//!   (checks.yml, the full suite) on the branch. Main requires only `PR
//!   checks`, which compiles nothing, so auto-merge is NOT armed here: a
//!   bump is mechanical only once it builds and passes.
//! - `land` (environment `pin-bump`), when `CI` finishes on a bump branch:
//!   green, on the PR's current head, and the PR still carries the
//!   mechanical marker -> auto-merge (squash, pinned to that head), which
//!   still waits for `PR checks`. Red -> auto-merge off and a comment
//!   naming the failed jobs: the bump needs source changes, and stays open.
//!
//! Triggers: Architect's release dispatches it (`gh workflow run
//! bump-architect.yml -f tag=vX.Y.Z`, architect release.yml job
//! `dispatch`); an hourly run catches a missed dispatch and a keyflow
//! release that came later; by hand from the Actions tab.
//!
//! INERT until the owner sets, on this repo: the variable `ARCHITECT_BUMP`
//! = `on`; the variable `FORGE_APP_ID`; the environment `pin-bump`
//! (deployment branches: main only) with the secret `FORGE_APP_PRIVATE_KEY`.
//! Without the variable every job is skipped. Nothing here tags: a Studio
//! release stays `just release` (or a release queue, if one is added).

use crate::model::{Job, Step, UBUNTU, Workflow, yaml};
use crate::steps::{Tools, cargo_deps_restore, checkout, hosted_setup};
use anyhow::Result;

const ENABLED: &str = "vars.ARCHITECT_BUMP == 'on'";

/// The marker `land` requires on the PR: only `open` writes it, and only
/// for a bump the script called mechanical.
const MECHANICAL: &str = "<!-- bump:mechanical -->";

const WHICH_TAG: &str = r#"set -euo pipefail
tag="${INPUT_TAG:-}"
if [ -z "$tag" ]; then
  # `releases/latest` is the newest release that is neither a draft nor a
  # pre-release.
  tag="$(gh api repos/FastTrackStudios/architect/releases/latest --jq .tag_name)"
fi
if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "::error::'$tag' is not a stable release tag (vX.Y.Z): no bump"
  exit 1
fi
pre="$(gh api "repos/FastTrackStudios/architect/releases/tags/$tag" --jq '.prerelease or .draft')"
if [ "$pre" != false ]; then
  echo "::error::$tag is a draft or a pre-release on GitHub: no bump"
  exit 1
fi
echo "tag=$tag" >> "$GITHUB_OUTPUT"
"#;

const BUMP: &str = r####"set -uo pipefail
status=0
python3 scripts/bump-architect.py "$TAG" --keyflow auto --report "$RUNNER_TEMP/report.md" || status=$?
touch "$RUNNER_TEMP/report.md"
case "$status" in
  0|3) ;;
  4)
    echo "status=4" >> "$GITHUB_OUTPUT"
    echo "changed=false" >> "$GITHUB_OUTPUT"
    { echo "### Waiting on keyflow"; cat "$RUNNER_TEMP/report.md"; } >> "$GITHUB_STEP_SUMMARY"
    exit 0 ;;
  *)
    echo "::error::bump-architect refused $TAG (exit $status)"
    exit 1 ;;
esac
echo "status=$status" >> "$GITHUB_OUTPUT"
mkdir -p "$RUNNER_TEMP/bump"
git add -A
git diff --cached --binary > "$RUNNER_TEMP/bump/bump.patch"
cp "$RUNNER_TEMP/report.md" "$RUNNER_TEMP/bump/report.md"
if [ -s "$RUNNER_TEMP/bump/bump.patch" ]; then
  echo "changed=true" >> "$GITHUB_OUTPUT"
  git diff --cached --stat >> "$GITHUB_STEP_SUMMARY"
else
  echo "changed=false" >> "$GITHUB_OUTPUT"
  echo "already on $TAG: nothing to do" >> "$GITHUB_STEP_SUMMARY"
fi
"####;

const OPEN: &str = r#"set -euo pipefail
git apply --index "$RUNNER_TEMP/bump/bump.patch"
branch="bump/architect-$TAG"
title="build(deps): architect $TAG"
export GIT_AUTHOR_NAME="fts-forge[bot]" GIT_COMMITTER_NAME="fts-forge[bot]"
export GIT_AUTHOR_EMAIL="345159799+fts-forge[bot]@users.noreply.github.com" GIT_COMMITTER_EMAIL="345159799+fts-forge[bot]@users.noreply.github.com"
git commit -qm "$title" -m "scripts/bump-architect.py $TAG --keyflow auto, run by bump-architect.yml."
git push -q --force "https://x-access-token:${GH_TOKEN}@github.com/${GITHUB_REPOSITORY}.git" "HEAD:refs/heads/$branch"
if [ "$STATUS" = 0 ]; then
  verdict="$MECHANICAL"$'\n'"Mechanical so far: only the pins, the comments that name them, \`Cargo.lock\`, add-only lines in \`.config/duplicate-deps.toml\` and the change file. The full suite (\`CI\`) is running on this branch; when it is green on this head, auto-merge goes on. When it fails, the bump needs source changes and waits for a person."
else
  verdict="**Needs a person** (it will not merge itself):"$'\n\n'"$(cat "$RUNNER_TEMP/bump/report.md")"
fi
body="Moves every Architect pin to [$TAG](https://github.com/FastTrackStudios/architect/releases/tag/$TAG), and Keyflow's to the newest keyflow release on it (\`just bump-architect $TAG\`)."$'\n\n'"$verdict"$'\n\n'"Opened by \`.github/workflows/bump-architect.yml\` ($GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID). It cuts no Studio release."
pr="$(gh pr list --head "$branch" --state open --json number --jq '.[0].number // empty')"
if [ -n "$pr" ]; then
  gh pr merge "$pr" --disable-auto 2>/dev/null || true
  gh pr edit "$pr" --title "$title" --body "$body"
else
  gh pr create --base main --head "$branch" --title "$title" --body "$body"
  pr="$(gh pr list --head "$branch" --state open --json number --jq '.[0].number')"
fi
gh workflow run checks.yml --ref "$branch"
echo "PR #$pr ($branch): $([ "$STATUS" = 0 ] && echo 'CI dispatched; merges itself when green' || echo 'needs a person')" >> "$GITHUB_STEP_SUMMARY"
"#;

const LAND: &str = r#"set -euo pipefail
read -r pr head body_has_marker < <(gh pr list -R "$GITHUB_REPOSITORY" --head "$BRANCH" --state open \
  --json number,headRefOid,author,body \
  --jq '.[] | select(.author.login == "app/fts-forge") | "\(.number) \(.headRefOid) \(.body | contains(env.MECHANICAL))"' | head -1) || true
[ -n "${pr:-}" ] || { echo "no open bot PR from $BRANCH"; exit 0; }
if [ "$CONCLUSION" = success ]; then
  if [ "$head" != "$HEAD_SHA" ]; then
    echo "CI ran on $HEAD_SHA, the PR's head is now $head: waiting for that run"; exit 0
  fi
  if [ "$body_has_marker" != true ]; then
    echo "PR #$pr is not marked mechanical: a person merges it"; exit 0
  fi
  gh pr merge "$pr" -R "$GITHUB_REPOSITORY" --auto --squash --match-head-commit "$HEAD_SHA"
  echo "PR #$pr: CI green on $HEAD_SHA, auto-merge on" >> "$GITHUB_STEP_SUMMARY"
else
  gh pr merge "$pr" -R "$GITHUB_REPOSITORY" --disable-auto 2>/dev/null || true
  failed="$(gh run view "$RUN_ID" -R "$GITHUB_REPOSITORY" --json jobs --jq '.jobs[] | select(.conclusion == "failure") | "- \(.name)"')"
  gh pr comment "$pr" -R "$GITHUB_REPOSITORY" --body "CI $CONCLUSION on this bump ($RUN_URL), so it is not mechanical: it will not merge itself, and waits for a person (a source change on this branch, or a keyflow release first)."$'\n\n'"Failed jobs:"$'\n'"${failed:-(none reported)}"
fi
"#;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "bump-architect",
        yaml(
            r#"workflow_dispatch:
  inputs:
    tag:
      description: Architect release tag (vX.Y.Z); empty = Architect's latest stable release
      required: false
      type: string
schedule:
- cron: "23 * * * *"
workflow_run:
  workflows: [CI]
  types: [completed]
  branches: ["bump/architect-*"]
"#,
        )?,
    )
    .permissions(yaml("{}")?)
    // One opener at a time; each CI completion handled on its own.
    .concurrency(
        "bump-architect-${{ github.event_name == 'workflow_run' && github.event.workflow_run.id || 'open' }}",
        false,
    )
    .job("prepare", prepare()?)
    .job("open", open()?)
    .job("land", land()?))
}

fn token(permissions: &[(&str, &str)]) -> Step {
    let mut step = Step::uses(
        "fts-forge token (this repo only)",
        "actions/create-github-app-token@v2",
    )
    .id("forge")
    .with("app-id", "${{ vars.FORGE_APP_ID }}")
    .with("private-key", "${{ secrets.FORGE_APP_PRIVATE_KEY }}")
    .with("owner", "FastTrackStudios")
    .with("repositories", "studio");
    for (scope, level) in permissions {
        step = step.with(&format!("permission-{scope}"), *level);
    }
    step
}

fn prepare() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Prepare the bump")
        .cond(&format!(
            "${{{{ {ENABLED} && github.event_name != 'workflow_run' }}}}"
        ))
        .timeout(20)
        .permissions(yaml("contents: read\n")?)
        .output("tag", "${{ steps.tag.outputs.tag }}")
        .output("changed", "${{ steps.bump.outputs.changed }}")
        .output("status", "${{ steps.bump.outputs.status }}")
        .step(
            checkout()
                .with("ref", "main")
                .with("persist-credentials", false),
        )
        .step(
            Step::run("Which tag", WHICH_TAG)
                .id("tag")
                .env("GH_TOKEN", "${{ github.token }}")
                .env("INPUT_TAG", "${{ inputs.tag }}"),
        )
        // cargo resolves the lock; nothing compiles.
        .steps(hosted_setup("", "", Tools::default()))
        .step(cargo_deps_restore())
        .step(
            Step::run("just bump-architect", BUMP)
                .id("bump")
                // Read-only: GitHub's API (is a keyflow tag on keyflow's main?).
                .env("GH_TOKEN", "${{ github.token }}")
                .env("TAG", "${{ steps.tag.outputs.tag }}"),
        )
        .step(
            Step::action("actions/upload-artifact@v4")
                .cond("${{ steps.bump.outputs.changed == 'true' }}")
                .with("name", "bump")
                .with("path", "${{ runner.temp }}/bump")
                .with("retention-days", 7),
        ))
}

fn open() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Open the bump PR, run CI on it")
        .needs(&["prepare"])
        .cond("${{ needs.prepare.outputs.changed == 'true' }}")
        .timeout(10)
        .environment("pin-bump")
        .permissions(yaml("{}")?)
        .step(token(&[
            ("contents", "write"),
            ("pull-requests", "write"),
            ("actions", "write"),
        ]))
        .step(
            checkout()
                .with("ref", "main")
                .with("token", "${{ steps.forge.outputs.token }}")
                .with("persist-credentials", false),
        )
        .step(
            Step::action("actions/download-artifact@v4")
                .with("name", "bump")
                .with("path", "${{ runner.temp }}/bump"),
        )
        .step(
            Step::run("Push the branch, open the PR, dispatch CI", OPEN)
                .env("GH_TOKEN", "${{ steps.forge.outputs.token }}")
                .env("TAG", "${{ needs.prepare.outputs.tag }}")
                .env("STATUS", "${{ needs.prepare.outputs.status }}")
                .env("MECHANICAL", MECHANICAL),
        ))
}

fn land() -> Result<Job> {
    // Only CI runs of this repo's own bump branches (a fork's branch of the
    // same name never gets here), and never on anything but its head.
    let cond = format!(
        "${{{{ {ENABLED} && github.event_name == 'workflow_run' && \
         github.event.workflow_run.head_repository.full_name == github.repository && \
         startsWith(github.event.workflow_run.head_branch, 'bump/architect-') && \
         (github.event.workflow_run.conclusion == 'success' || github.event.workflow_run.conclusion == 'failure') }}}}"
    );
    Ok(Job::new(UBUNTU)
        .name("Merge a green mechanical bump")
        .cond(&cond)
        .timeout(5)
        .environment("pin-bump")
        .permissions(yaml("{}")?)
        .step(token(&[
            ("contents", "write"),
            ("pull-requests", "write"),
            ("actions", "read"),
        ]))
        .step(
            Step::run("Auto-merge when green, explain when red", LAND)
                .env("GH_TOKEN", "${{ steps.forge.outputs.token }}")
                .env("BRANCH", "${{ github.event.workflow_run.head_branch }}")
                .env("HEAD_SHA", "${{ github.event.workflow_run.head_sha }}")
                .env("CONCLUSION", "${{ github.event.workflow_run.conclusion }}")
                .env("RUN_ID", "${{ github.event.workflow_run.id }}")
                .env("RUN_URL", "${{ github.event.workflow_run.html_url }}")
                .env("MECHANICAL", MECHANICAL),
        ))
}
