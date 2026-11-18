//! `release-queue` — a merged pin bump ships Studio without a person.
//!
//! Only a bump: `bump-architect.yml`'s `land` squash-merges a green,
//! mechanical Architect bump with the trailer `Release-Queue: bump` in its
//! commit message, authored by fts-forge[bot]. Those are free text (every
//! agent PR is fts-forge's), so a claimed bump counts only once
//! scripts/bump-verify.py confirms through the API that it is the merge of
//! a PR from `bump/architect-*` in this repo, opened by fts-forge[bot],
//! changing only allow-listed files. When `CI` (checks.yml, the
//! full suite) passes on a push to main, this looks at main's first-parent
//! commits since the last release tag for one such commit; if there is one,
//! it runs exactly `just release <sha>` (`release.sh release --yes <sha>`)
//! on that run's commit. Every merge that is not a bot bump ships only when
//! a person runs `just release` (or the queue fires for a later bump and
//! carries it along, as `just release` would).
//!
//! The release-workflow rules hold because it IS `release.sh release`: a
//! CalVer tag on a green main commit (`CI passed`, which requires the
//! pre-tag checks "Web image evaluates" and "Apple builds"), no release
//! commit, no release PR. A bump that declares or might carry a data
//! migration is never mechanical, so it never carries the trailer.
//!
//! Token (scripts/check-token-mints.py): studio only, `contents: write`,
//! for the one `git push` of the tag (the App is a bypass actor of the `v*`
//! tag ruleset; a tag pushed with GITHUB_TOKEN would start no release
//! workflow). It reaches git through GIT_CONFIG_* in that step's
//! environment, never a file; every read (CI status, tags) uses the
//! workflow's own GITHUB_TOKEN.
//!
//! INERT until the owner sets the repo variable `STUDIO_RELEASE_QUEUE` =
//! `on` (separately from `ARCHITECT_BUMP`) and the environment
//! `release-queue` (deployment branches: main only) holds the environment
//! variable `BOT_ENVIRONMENT` = `release-queue` and the secret
//! `RELEASE_QUEUE_APP_KEY`.

use crate::bot::{app_token, environment_guard};
use crate::model::{Job, Step, UBUNTU, Workflow, pinned, yaml};
use anyhow::Result;

const SECRET: &str = "RELEASE_QUEUE_APP_KEY";

/// Read-only: is there a bot bump on main since the last release?
/// (`release.sh bump-queued`, tested by scripts/release-facts-test.sh.)
const ASKED: &str = r#"set -euo pipefail
git fetch -q --tags --force origin main
echo "asked=false" >> "$GITHUB_OUTPUT"
claims="$(bash scripts/release.sh bump-queued "$SHA" || true)"
[ -n "$claims" ] || { echo "no bot bump on main since the last release: nothing to release" | tee -a "$GITHUB_STEP_SUMMARY"; exit 0; }
for c in $claims; do
  # The trailer is free text: the API says whether it is a real bot bump.
  if pr="$(python3 scripts/bump-verify.py "$c")"; then
    echo "asked by #$pr ($(git log -1 --format='%h %s' "$c")): releasing $SHA" | tee -a "$GITHUB_STEP_SUMMARY"
    echo "asked=true" >> "$GITHUB_OUTPUT"
    exit 0
  fi
  echo "$pr" | tee -a "$GITHUB_STEP_SUMMARY"
done
echo "no claimed bump is a verified bot bump: nothing to release" | tee -a "$GITHUB_STEP_SUMMARY"
"#;

const RELEASE: &str = r#"set -euo pipefail
auth="$(printf 'x-access-token:%s' "$FORGE_TOKEN" | base64 -w0)"
echo "::add-mask::$auth"
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="http.https://github.com/.extraheader" GIT_CONFIG_VALUE_0="AUTHORIZATION: basic $auth"
bash scripts/release.sh release --yes "$SHA"
"#;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "release-queue",
        yaml(
            "workflow_run:\n  workflows: [CI]\n  types: [completed]\n  branches: [main]\nworkflow_dispatch: null\n",
        )?,
    )
    .permissions(yaml("{}")?)
    // One at a time, in order: two green runs back to back must not race
    // for the same version.
    .concurrency("release-queue", false)
    .job("release", release()?))
}

fn release() -> Result<Job> {
    let asked = "${{ steps.asked.outputs.asked == 'true' }}";
    Ok(Job::new(UBUNTU)
        .name("Release a merged bump")
        .cond(
            "${{ vars.STUDIO_RELEASE_QUEUE == 'on' && (github.event_name == 'workflow_dispatch' || \
             (github.event.workflow_run.conclusion == 'success' && github.event.workflow_run.event == 'push' && \
             github.event.workflow_run.head_repository.full_name == github.repository)) }}",
        )
        .timeout(15)
        .environment("release-queue")
        .permissions(yaml("contents: read\nchecks: read\nactions: read\n")?)
        .step(environment_guard("release-queue", SECRET))
        .step(
            Step::action(pinned::CHECKOUT)
                .with("ref", "main")
                .with("fetch-depth", 0)
                .with("persist-credentials", false),
        )
        .step(
            Step::run("A verified bot bump since the last release?", ASKED)
                .id("asked")
                .env("GH_TOKEN", "${{ github.token }}")
                .env("SHA", "${{ github.event.workflow_run.head_sha || github.sha }}"),
        )
        .step(app_token(SECRET, "studio", &[("contents", "write")]).cond(asked))
        .step(
            Step::run("just release <sha>", RELEASE)
                .cond(asked)
                .env("GH_TOKEN", "${{ github.token }}")
                .env("FORGE_TOKEN", "${{ steps.forge.outputs.token }}")
                .env("SHA", "${{ github.event.workflow_run.head_sha || github.sha }}"),
        ))
}
