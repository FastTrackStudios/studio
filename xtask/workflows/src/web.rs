//! `web` — Studio's web bundle on the release tag: the web app built for
//! the root of fasttrackstudio.app (scripts/web/bundle.sh), attached to the
//! ONE GitHub Release of `studio-vX.Y.Z` as `studio-web-X.Y.Z.tar.gz` and
//! signed into its SHA256SUMS.
//!
//! ```text
//!   `just release` pushes studio-vX.Y.Z --> this, the installers (desktop)
//!   and the release publish start on the tag, in parallel --> resolve (the
//!   same checks as the installers': desktop::resolve) --> build
//!   (THEBATTLESHIP, `thebattleship-release`) uploads the tarball as a
//!   workflow artifact --> publish (hosted) attaches it to the Release
//!   --> sums signs it in.
//! ```
//!
//! The deploy is the VPS's (FastTrackStudios/infra): its `studio-web`
//! package names this asset by tag and hash, and the host's Caddy serves
//! it at fasttrackstudio.app. A release changes nothing live by itself.
//!
//! Triggers: a tag push, and workflow_dispatch with the desktop's modes
//! (publish, validate, build-only).

use crate::desktop::{NOT_VALIDATE, STAMPED, resolve};
use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout, stamp};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "web",
        yaml(
            r#"
push:
  tags: ["studio-v*"]
workflow_dispatch:
  inputs:
    tag:
      description: "Release tag (e.g. studio-v26.12.0) for publish/validate; any branch or sha for build-only"
      required: true
      type: string
    mode:
      description: "publish: attach to the tag's Release. validate: resolve only. build-only: build the bundle, no Release."
      required: false
      type: choice
      options: [publish, validate, build-only]
      default: publish
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    .concurrency("web-${{ inputs.tag || github.ref_name }}", false)
    .env("TAG", "${{ inputs.tag || github.ref_name }}")
    .env("MODE", "${{ inputs.mode || 'publish' }}")
    .job("resolve", resolve()?)
    .job("build", build())
    .job("publish", publish()?)
    .jobs(crate::sums::jobs(
        &["publish"],
        "inputs.mode != 'validate' && inputs.mode != 'build-only' && needs.publish.result == 'success'",
        "${{ needs.publish.outputs.digests }}",
    )?))
}

/// The release pool on THEBATTLESHIP: the web build in the dev shell, in a
/// target dir of its own in the pool's shared cache (a wasm release build
/// shares no fingerprints with the installers').
fn build() -> Job {
    Job::new(RunsOn::Labels(&["self-hosted", "thebattleship-release"]))
        .name("Web bundle")
        .needs(&["resolve"])
        .cond(NOT_VALIDATE)
        .timeout(60)
        .step(checkout().with("ref", "${{ needs.resolve.outputs.sha }}"))
        .step(Step::uses(
            "Shared runner tools",
            "./.github/actions/shared-tools",
        ))
        .step(stamp("${{ env.TAG }}").cond(STAMPED))
        .step(Step::run(
            "Own target dir",
            r#"base="${CI_CACHE_DIR:?not a shared runner (no CI_CACHE_DIR)}"
dir="$base/web/$RUNNER_NAME/target"
mkdir -p "$dir"
echo "target dir: $dir ($(du -sh "$dir" | cut -f1))"
echo "CARGO_TARGET_DIR=$dir" >> "$GITHUB_ENV"
"#,
        ))
        .step(
            Step::run(
                "Build the web bundle",
                "set -euo pipefail\nrm -rf target/web\nbash scripts/web/bundle.sh\n",
            )
            .env("VERSION", "${{ needs.resolve.outputs.version }}"),
        )
        .step(
            Step::uses("Keep the bundle", crate::model::pinned::UPLOAD_ARTIFACT)
                .with("name", "web")
                .with("path", "target/web/*.tar.gz\n")
                .with("if-no-files-found", "error")
                .with("retention-days", 14),
        )
}

/// Checks the bundle is there (named with the release's version) and
/// attaches it; its GitHub digest must be the file's own, which `sums`
/// signs (sums.rs).
fn publish() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Attach to the GitHub Release")
        .needs(&["resolve", "build"])
        .cond(
            "${{ inputs.mode != 'validate' && inputs.mode != 'build-only' && \
             needs.build.result == 'success' }}",
        )
        .timeout(15)
        .permissions(yaml("contents: write\n")?)
        .output("digests", "${{ steps.attach.outputs.digests }}")
        .step(
            Step::uses("The bundle", crate::model::pinned::DOWNLOAD_ARTIFACT)
                .with("name", "web")
                .with("path", "web"),
        )
        .step(
            Step::run(
                "Verify, then attach to the one Release",
                r####"set -euo pipefail
cd web
f="studio-web-$VERSION.tar.gz"
test -f "$f"
tar -tzf "$f" ./index.html >/dev/null
gh release upload "$TAG" --repo "$GITHUB_REPOSITORY" --clobber "$f"
want="sha256:$(sha256sum "$f" | cut -d' ' -f1)"
# The Release is a draft until `sums` publishes it: find it in the list.
got="$(gh api "repos/$GITHUB_REPOSITORY/releases?per_page=50" --jq ".[] | select(.tag_name == \"$TAG\") | .assets[] | select(.name == \"$f\") | .digest")"
[ "$got" = "$want" ] || { echo "::error::$f: GitHub's digest is '$got', the file is $want"; exit 1; }
{ echo "digests<<EOF"; sha256sum "$f"; echo "EOF"; } >> "$GITHUB_OUTPUT"
echo "### Attached \`$f\` to $TAG (\`$want\`)" >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .id("attach")
            .env("GH_TOKEN", "${{ github.token }}")
            .env("VERSION", "${{ needs.resolve.outputs.version }}"),
        ))
}
