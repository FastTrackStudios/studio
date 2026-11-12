//! `bump-architect` — the Architect (and Keyflow) pin bump, by the bot.
//!
//! When Architect releases a stable tag, a PR moves every Architect pin to
//! it (scripts/bump-architect.py, exactly `just bump-architect vX.Y.Z`): a
//! PATCH release alone (Studio patches keyflow's www-spelled Architect pins
//! to its own tag), a MINOR together with the newest keyflow release on it.
//! It merges once the FULL suite is green on it, if the bump was mechanical.
//!
//! - `prepare` (no secrets): resolves the tag (the input, or Architect's
//!   latest stable release; never a draft or a pre-release), and stops
//!   when a person has held this release: a CLOSED, unmerged bot PR for
//!   its branch, or an open one labelled `hold` (THE way to hold one
//!   release; the bot never reopens or re-pushes it). Runs the script,
//!   stages ONLY the paths a bump may touch, and hands the diff on as an
//!   artifact. Exit 3 (needs a person) still opens the PR, unmarked; exit 4
//!   (waiting on keyflow) opens nothing, and the hourly run retries. When
//!   an open bot PR already carries exactly this diff, nothing happens: no
//!   push, no new CI run (no hourly churn).
//! - `open` (environment `pin-bump`): pushes `bump/architect-<tag>` with
//!   `--force-with-lease` against the head it saw (only over its own
//!   commit; a branch a person pushed to is left alone), opens or updates
//!   the PR, and dispatches `CI` (checks.yml, the full suite) on it: main
//!   requires only `PR checks`, which compiles nothing, so a bump is
//!   mechanical only once it builds and passes.
//! - `land` (environment `pin-bump`), when `CI` finishes on a bump branch:
//!   green, on the PR's current head (the bot's own commit), the PR still
//!   marked mechanical, not held, and every changed file on the allow-list
//!   -> merges it NOW (squash, pinned to that head; the squash commit
//!   carries `Release-Queue: bump`, the release queue's marker). Red ->
//!   one comment (edited in place on later runs) and the marker removed,
//!   so a later green run does not merge it: it waits for a person.
//!
//! Tokens (scripts/check-token-mints.py checks them):
//!   open  studio only: contents write (push the branch), pull-requests
//!         write (open the PR), actions write (dispatch CI: a run started
//!         with the workflow's own token would not fire `land`).
//!   land  studio only: contents write + pull-requests write (the merge
//!         must be the App's, or main's CI would not run on it).
//!   Everything else uses the workflow's own GITHUB_TOKEN.
//!
//! INERT until the owner sets, on this repo: the variable `ARCHITECT_BUMP`
//! = `on`; the variable `FORGE_APP_ID`; the environment `pin-bump`
//! (deployment branches: main only) with the environment variable
//! `BOT_ENVIRONMENT` = `pin-bump` and the secret `PIN_BUMP_APP_KEY` (the
//! fts-forge App's key, under a name nothing else uses: a job without its
//! environment gets nothing, not an org secret). Nothing here tags: the
//! release queue (release_queue.rs) does, behind its own variable.

use crate::bot::{app_token, environment_guard};
use crate::model::{Job, Step, UBUNTU, Workflow, pinned, yaml};
use anyhow::Result;

const ENABLED: &str = "vars.ARCHITECT_BUMP == 'on'";
const SECRET: &str = "PIN_BUMP_APP_KEY";

/// The marker `land` requires on the PR: only `open` writes it, only for
/// a bump the script called mechanical; `land` removes it on a red run.
const MECHANICAL: &str = "<!-- bump:mechanical -->";

/// The paths a bump may change (git pathspecs; `land` checks the PR's
/// files against the same list as a regex).
const PATHS: &str = "Cargo.toml '*/Cargo.toml' Cargo.lock .config/duplicate-deps.toml '.changes/studio-architect-on-architect-*.md'";
const PATHS_RE: &str = r"^(Cargo\.toml|.*/Cargo\.toml|Cargo\.lock|\.config/duplicate-deps\.toml|\.changes/studio-architect-on-architect-[0-9a-f]+\.md)$";

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

/// A person's hold wins; an open bot PR is reused, never overwritten.
const PR_STATE: &str = r#"set -euo pipefail
branch="bump/architect-$TAG"
prs="$(gh pr list --head "$branch" --state all --json number,state,author,headRefOid,labels \
  --jq '[.[] | select(.author.login == "app/fts-forge")]')"
if [ "$(jq '[.[] | select(.state == "CLOSED")] | length' <<<"$prs")" != 0 ]; then
  echo "held: a closed bot PR for $branch exists (closing it is how a release is held)" | tee -a "$GITHUB_STEP_SUMMARY"
  echo "go=false" >> "$GITHUB_OUTPUT"; exit 0
fi
if [ "$(jq '[.[] | select(.state == "MERGED")] | length' <<<"$prs")" != 0 ]; then
  echo "$branch is merged already" | tee -a "$GITHUB_STEP_SUMMARY"
  echo "go=false" >> "$GITHUB_OUTPUT"; exit 0
fi
open="$(jq -c '[.[] | select(.state == "OPEN")][0] // {}' <<<"$prs")"
if jq -e '(.labels // []) | map(.name) | index("hold")' <<<"$open" >/dev/null; then
  echo "held: PR #$(jq .number <<<"$open") is labelled hold" | tee -a "$GITHUB_STEP_SUMMARY"
  echo "go=false" >> "$GITHUB_OUTPUT"; exit 0
fi
{
  echo "go=true"
  echo "branch=$branch"
  echo "pr=$(jq -r '.number // empty' <<<"$open")"
  echo "head=$(jq -r '.headRefOid // empty' <<<"$open")"
} >> "$GITHUB_OUTPUT"
"#;

const BUMP: &str = r####"set -uo pipefail
status=0
python3 scripts/bump-architect.py "$TAG" --keyflow auto --report "$RUNNER_TEMP/report.md" || status=$?
touch "$RUNNER_TEMP/report.md"
case "$status" in
  0|3) ;;
  4)
    echo "changed=false" >> "$GITHUB_OUTPUT"
    { echo "### Waiting on keyflow"; cat "$RUNNER_TEMP/report.md"; } >> "$GITHUB_STEP_SUMMARY"
    exit 0 ;;
  *)
    echo "::error::bump-architect refused $TAG (exit $status)"
    exit 1 ;;
esac
set -e
echo "status=$status" >> "$GITHUB_OUTPUT"
# Only the paths a bump may touch; anything else it changed is a bug.
eval "git add -- $PATHS"
if [ -n "$(git status --porcelain --untracked-files=all | grep -v '^[AM]  ' || true)" ]; then
  echo "::error::the bump changed paths outside its allow-list:"
  git status --porcelain --untracked-files=all
  exit 1
fi
mkdir -p "$RUNNER_TEMP/bump"
git diff --cached --binary > "$RUNNER_TEMP/bump/bump.patch"
cp "$RUNNER_TEMP/report.md" "$RUNNER_TEMP/bump/report.md"
if [ ! -s "$RUNNER_TEMP/bump/bump.patch" ]; then
  echo "changed=false" >> "$GITHUB_OUTPUT"
  echo "already on $TAG: nothing to do" >> "$GITHUB_STEP_SUMMARY"
  exit 0
fi
# An open bot PR with exactly these files: nothing to push, no new CI.
if [ -n "$PR_HEAD" ]; then
  git fetch -q --depth 1 origin "$PR_HEAD"
  if eval "git diff --cached --quiet $PR_HEAD -- $PATHS"; then
    echo "changed=false" >> "$GITHUB_OUTPUT"
    echo "PR #$PR already carries this bump: nothing to do" >> "$GITHUB_STEP_SUMMARY"
    exit 0
  fi
fi
echo "changed=true" >> "$GITHUB_OUTPUT"
git diff --cached --stat >> "$GITHUB_STEP_SUMMARY"
"####;

const OPEN: &str = r#"set -euo pipefail
git apply --index "$RUNNER_TEMP/bump/bump.patch"
title="build(deps): architect $TAG"
export GIT_AUTHOR_NAME="fts-forge[bot]" GIT_COMMITTER_NAME="fts-forge[bot]"
export GIT_AUTHOR_EMAIL="345159799+fts-forge[bot]@users.noreply.github.com" GIT_COMMITTER_EMAIL="345159799+fts-forge[bot]@users.noreply.github.com"
git commit -qm "$title" -m "scripts/bump-architect.py $TAG --keyflow auto, run by bump-architect.yml."
if [ -n "$PR_HEAD" ]; then
  # Only over the bot's own commit: a branch a person pushed to is theirs.
  author="$(gh api "repos/$GITHUB_REPOSITORY/commits/$PR_HEAD" --jq '.author.login // ""')"
  if [ "$author" != "fts-forge[bot]" ]; then
    echo "PR #$PR's head $PR_HEAD is not the bot's commit ($author): left alone" | tee -a "$GITHUB_STEP_SUMMARY"
    exit 0
  fi
fi
git push -q --force-with-lease="refs/heads/$BRANCH:$PR_HEAD" \
  "https://x-access-token:${GH_TOKEN}@github.com/${GITHUB_REPOSITORY}.git" "HEAD:refs/heads/$BRANCH"
if [ "$STATUS" = 0 ]; then
  verdict="$MECHANICAL"$'\n'"Mechanical so far: only the pins, the comments that name them, \`Cargo.lock\`, add-only lines in \`.config/duplicate-deps.toml\` and the change file. The full suite (\`CI\`) is running on this branch; when it is green on this head, the bot merges it. When it fails, the bump needs source changes and waits for a person."
else
  verdict="**Needs a person** (it will not merge itself):"$'\n\n'"$(cat "$RUNNER_TEMP/bump/report.md")"
fi
body="Moves every Architect pin to [$TAG](https://github.com/FastTrackStudios/architect/releases/tag/$TAG) (\`just bump-architect $TAG\`)."$'\n\n'"$verdict"$'\n\n'"To hold this release: close this PR, or label it \`hold\`. Opened by \`.github/workflows/bump-architect.yml\` ($GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID)."
if [ -n "$PR" ]; then
  gh pr edit "$PR" --title "$title" --body "$body"
else
  gh pr create --base main --head "$BRANCH" --title "$title" --body "$body"
fi
gh workflow run checks.yml --ref "$BRANCH"
echo "pushed $BRANCH; CI dispatched" >> "$GITHUB_STEP_SUMMARY"
"#;

/// Read-only (GITHUB_TOKEN): decides; on red, comments once and unmarks.
const JUDGE: &str = r#"set -euo pipefail
echo "merge=false" >> "$GITHUB_OUTPUT"
pr_json="$(gh pr list -R "$GITHUB_REPOSITORY" --head "$BRANCH" --state open --json number,headRefOid,author,body,labels \
  --jq '[.[] | select(.author.login == "app/fts-forge")][0] // empty')"
[ -n "$pr_json" ] || { echo "no open bot PR from $BRANCH"; exit 0; }
pr="$(jq -r .number <<<"$pr_json")"
head="$(jq -r .headRefOid <<<"$pr_json")"
if [ "$CONCLUSION" != success ]; then
  failed="$(gh run view "$RUN_ID" -R "$GITHUB_REPOSITORY" --json jobs --jq '.jobs[] | select(.conclusion == "failure") | "- \(.name)"')"
  note="<!-- bump:red -->"$'\n'"CI $CONCLUSION on this bump ($RUN_URL), so it is not mechanical: it will not merge itself, and waits for a person (a source change on this branch, or a keyflow release first)."$'\n\n'"Failed jobs:"$'\n'"${failed:-(none reported)}"
  # One comment, edited in place on later red runs.
  cid="$(gh api "repos/$GITHUB_REPOSITORY/issues/$pr/comments" --jq '[.[] | select(.body | startswith("<!-- bump:red -->"))][0].id // empty')"
  if [ -n "$cid" ]; then
    gh api -X PATCH "repos/$GITHUB_REPOSITORY/issues/comments/$cid" -f body="$note" >/dev/null
  else
    gh pr comment "$pr" -R "$GITHUB_REPOSITORY" --body "$note"
  fi
  # Unmarked: a later green run (after a person's fix) does not merge it.
  body="$(jq -r .body <<<"$pr_json")"
  gh pr edit "$pr" -R "$GITHUB_REPOSITORY" --body "${body//$MECHANICAL/}"
  exit 0
fi
[ "$head" = "$HEAD_SHA" ] || { echo "CI ran on $HEAD_SHA, the PR's head is $head: waiting for that run"; exit 0; }
jq -e --arg m "$MECHANICAL" '.body | contains($m)' <<<"$pr_json" >/dev/null \
  || { echo "PR #$pr is not marked mechanical: a person merges it"; exit 0; }
if jq -e '.labels | map(.name) | index("hold")' <<<"$pr_json" >/dev/null; then
  echo "PR #$pr is labelled hold"; exit 0
fi
author="$(gh api "repos/$GITHUB_REPOSITORY/commits/$HEAD_SHA" --jq '.author.login // ""')"
[ "$author" = "fts-forge[bot]" ] || { echo "the head is $author's commit, not the bot's: a person merges it"; exit 0; }
others="$(gh pr diff "$pr" -R "$GITHUB_REPOSITORY" --name-only | grep -vE "$PATHS_RE" || true)"
if [ -n "$others" ]; then
  echo "PR #$pr changes files outside the bump's allow-list: a person merges it:"$'\n'"$others"; exit 0
fi
{ echo "merge=true"; echo "pr=$pr"; } >> "$GITHUB_OUTPUT"
"#;

const MERGE: &str = r#"set -euo pipefail
gh pr merge "$PR" -R "$GITHUB_REPOSITORY" --squash --match-head-commit "$HEAD_SHA" \
  --body "Merged by bump-architect.yml: CI green on $HEAD_SHA ($RUN_URL)."$'\n\n'"Release-Queue: bump"
echo "merged #$PR ($HEAD_SHA)" >> "$GITHUB_STEP_SUMMARY"
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

fn prepare() -> Result<Job> {
    let go = "${{ steps.state.outputs.go == 'true' }}";
    Ok(Job::new(UBUNTU)
        .name("Prepare the bump")
        .cond(&format!(
            "${{{{ {ENABLED} && github.event_name != 'workflow_run' }}}}"
        ))
        .timeout(20)
        .permissions(yaml("contents: read\npull-requests: read\n")?)
        .output("tag", "${{ steps.tag.outputs.tag }}")
        .output("branch", "${{ steps.state.outputs.branch }}")
        .output("pr", "${{ steps.state.outputs.pr }}")
        .output("head", "${{ steps.state.outputs.head }}")
        .output("changed", "${{ steps.bump.outputs.changed }}")
        .output("status", "${{ steps.bump.outputs.status }}")
        .step(
            Step::action(pinned::CHECKOUT)
                .with("ref", "main")
                .with("persist-credentials", false),
        )
        .step(
            Step::run("Which tag", WHICH_TAG)
                .id("tag")
                .env("GH_TOKEN", "${{ github.token }}")
                .env("INPUT_TAG", "${{ inputs.tag }}"),
        )
        .step(
            Step::run("Held, merged or open already?", PR_STATE)
                .id("state")
                .env("GH_TOKEN", "${{ github.token }}")
                .env("TAG", "${{ steps.tag.outputs.tag }}"),
        )
        // cargo resolves the lock; nothing compiles.
        .step(Step::uses("Rust 1.99.1", pinned::RUST_TOOLCHAIN).cond(go))
        .step(
            Step::uses("Restore cargo's dependencies", pinned::CACHE_RESTORE)
                .cond(go)
                .with(
                    "path",
                    "~/.cargo/registry/index\n~/.cargo/registry/cache\n~/.cargo/git/db\n",
                )
                .with(
                    "key",
                    "cargo-deps-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}",
                )
                .with("restore-keys", "cargo-deps-${{ runner.os }}-"),
        )
        .step(
            Step::run("just bump-architect", BUMP)
                .id("bump")
                .cond(go)
                // Read-only: GitHub's API (tags on main, release facts).
                .env("GH_TOKEN", "${{ github.token }}")
                .env("TAG", "${{ steps.tag.outputs.tag }}")
                .env("PATHS", PATHS)
                .env("PR", "${{ steps.state.outputs.pr }}")
                .env("PR_HEAD", "${{ steps.state.outputs.head }}"),
        )
        .step(
            Step::action(pinned::UPLOAD_ARTIFACT)
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
        .permissions(yaml("contents: read\n")?)
        .step(environment_guard("pin-bump", SECRET))
        .step(
            Step::action(pinned::CHECKOUT)
                .with("ref", "main")
                .with("persist-credentials", false),
        )
        .step(
            Step::action(pinned::DOWNLOAD_ARTIFACT)
                .with("name", "bump")
                .with("path", "${{ runner.temp }}/bump"),
        )
        .step(app_token(
            SECRET,
            "studio",
            &[
                ("contents", "write"),
                ("pull-requests", "write"),
                ("actions", "write"),
            ],
        ))
        .step(
            Step::run("Push the branch, open the PR, dispatch CI", OPEN)
                .env("GH_TOKEN", "${{ steps.forge.outputs.token }}")
                .env("TAG", "${{ needs.prepare.outputs.tag }}")
                .env("BRANCH", "${{ needs.prepare.outputs.branch }}")
                .env("PR", "${{ needs.prepare.outputs.pr }}")
                .env("PR_HEAD", "${{ needs.prepare.outputs.head }}")
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
    let run = |step: Step| {
        step.env("BRANCH", "${{ github.event.workflow_run.head_branch }}")
            .env("HEAD_SHA", "${{ github.event.workflow_run.head_sha }}")
            .env("RUN_URL", "${{ github.event.workflow_run.html_url }}")
    };
    let merge = "${{ steps.judge.outputs.merge == 'true' }}";
    Ok(Job::new(UBUNTU)
        .name("Merge a green mechanical bump")
        .cond(&cond)
        .timeout(10)
        .environment("pin-bump")
        .permissions(yaml(
            "contents: read\npull-requests: write\nactions: read\n",
        )?)
        .step(environment_guard("pin-bump", SECRET))
        .step(run(Step::run(
            "Green, marked, the bot's, on the allow-list?",
            JUDGE,
        )
        .id("judge")
        .env("GH_TOKEN", "${{ github.token }}")
        .env("CONCLUSION", "${{ github.event.workflow_run.conclusion }}")
        .env("RUN_ID", "${{ github.event.workflow_run.id }}")
        .env("MECHANICAL", MECHANICAL)
        .env("PATHS_RE", PATHS_RE)))
        .step(
            app_token(
                SECRET,
                "studio",
                &[("contents", "write"), ("pull-requests", "write")],
            )
            .cond(merge),
        )
        .step(run(Step::run("Merge it (squash, on that head)", MERGE)
            .cond(merge)
            .env("GH_TOKEN", "${{ steps.forge.outputs.token }}")
            .env("PR", "${{ steps.judge.outputs.pr }}"))))
}
