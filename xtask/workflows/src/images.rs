//! `images` — studio's public image, pushed to GHCR from the release commit,
//! BEFORE the release tag exists (mirrors FastTrackStudios/task's
//! images.yml):
//!
//! ```text
//!   ghcr.io/fasttrackstudios/studio-web:<X.Y.Z>   (+ sha-<short>, latest)
//!
//!   release PR squash-merges -> push of `chore(release): vX.Y.Z` to main
//!     (or to release/v* for a hotfix release)
//!     -> this builds `.#studio-web-image` and pushes it
//!   just release-tag -> refuses until studio-web:X.Y.Z is on GHCR, public,
//!                       and labelled with the release commit
//!                       (scripts/ghcr-images.sh check)
//!     -> pushes vX.Y.Z -> Argo CD deploys the newest tag (deploy/chart/studio)
//! ```
//!
//! A push that does not change the version builds nothing. Versions are
//! CalVer (26.10.3): the image tag is the bare version, no `v`.
//!
//! workflow_dispatch with `tag`: build and push the image for an EXISTING
//! release tag from that tag's source (re-publishing a lost image, seeding
//! GHCR). Creates no git tag, never moves `latest`.
//!
//! Packages pushed with GITHUB_TOKEN are created PRIVATE: the org makes
//! `studio-web` public once, after its first push, or the cluster cannot
//! pull it (and the anonymous `ghcr-images.sh check` fails).

use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout, checkout_full};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "Images",
        yaml(
            r#"
push:
  # main: a release PR's squash commit. release/v*: a hotfix release.
  branches: [main, "release/v*"]
  # A release commit always changes Cargo.toml's version.
  paths: [Cargo.toml]
workflow_dispatch:
  inputs:
    tag:
      description: "Existing release tag to (re)publish the image for, e.g. v26.10.3"
      required: true
      type: string
"#,
        )?,
    )
    // One group PER RELEASE, never cancelled, like a release commit's CI
    // run: a shared group holds only one pending run and drops it for the
    // next, so a release queued behind another could vanish. Two publishes
    // still never build side by side on one runner: the pool takes them in
    // arrival order.
    .concurrency("images-${{ inputs.tag || github.sha }}", false)
    .permissions(yaml("contents: read\n")?)
    .job("plan", plan()?)
    .job("publish", publish()?))
}

/// Is this a release commit, and which tags does its image get? Hosted,
/// cheap: a push that is not a release says so and stops here.
fn plan() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("What to publish")
        // Never a fork: its release commit is not this repo's.
        .cond("${{ github.repository == 'FastTrackStudios/studio' }}")
        .timeout(5)
        .output("build", "${{ steps.plan.outputs.build }}")
        .output("sha", "${{ steps.plan.outputs.sha }}")
        .output("tags", "${{ steps.plan.outputs.tags }}")
        .step(checkout_full(None))
        .step(
            Step::run("Plan", PLAN)
                .id("plan")
                .env("EVENT", "${{ github.event_name }}")
                .env("INPUT_TAG", "${{ inputs.tag }}"),
        ))
}

const PLAN: &str = r####"set -euo pipefail
git fetch --quiet --tags origin
if [ "$EVENT" = workflow_dispatch ]; then
  [[ "$INPUT_TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "::error::tag '$INPUT_TAG' is not vX.Y.Z"; exit 1; }
  sha="$(git rev-parse --verify --quiet "refs/tags/$INPUT_TAG^{commit}")" \
    || { echo "::error::no tag $INPUT_TAG"; exit 1; }
  v="$(bash scripts/release.sh version "$sha")"
  [ "v$v" = "$INPUT_TAG" ] || { echo "::error::$INPUT_TAG names a commit with version $v"; exit 1; }
  extra=""
else
  sha="$GITHUB_SHA"
  v="$(bash scripts/release.sh version "$sha")"
  vp="$(bash scripts/release.sh version "$sha^1")"
  if [ "$v" = "$vp" ]; then
    echo "version unchanged ($v): not a release commit, nothing to publish"
    echo "build=false" >> "$GITHUB_OUTPUT"
    exit 0
  fi
  # Captured, then matched: `git show | grep -q` under pipefail can read an
  # early match as a failure (grep exits, git show dies of SIGPIPE).
  changelog="$(git show "$sha:CHANGELOG.md")"
  grep -q "^## \[$v\]" <<<"$changelog" \
    || { echo "::error::version $vp -> $v but CHANGELOG.md has no [$v] section: not a release commit"; exit 1; }
  ! git rev-parse -q --verify "refs/tags/v$v" >/dev/null \
    || { echo "::error::tag v$v already exists; a released version's image is never replaced"; exit 1; }
  extra="latest"
fi
# The tag the release (and the chart at its tag) pulls.
itag="$(bash scripts/ghcr-images.sh tag "$sha")"
if [ "$itag" != "$v" ]; then
  echo "::error::the image tag at $sha is $itag but the release is $v: the cluster would pull the wrong image"
  exit 1
fi
tags="$(xargs <<<"$itag sha-$(git rev-parse --short "$sha") $extra")"
echo "publishing $sha as: $tags"
{
  echo "build=true"
  echo "sha=$sha"
  echo "tags=$tags"
} >> "$GITHUB_OUTPUT"
echo "### studio-web for v$v: \`$tags\` from \`$sha\`" >> "$GITHUB_STEP_SUMMARY"
"####;

/// THEBATTLESHIP's `studio-release` pool (Codys-Wright/nix-config
/// hosts/THEBATTLESHIP/github-runners.nix; x86_64 NixOS, the pool for
/// release builds, never checks, as desktop.yml's AppImage): the image is
/// Linux-only and a nix build, and the pool's nix store is warm. Only
/// trusted code reaches it: a push to main or release/v*, or a dispatch.
/// `just release-tag` waits for this image, so a release takes as long as
/// this build.
fn publish() -> Result<Job> {
    Ok(Job::new(RunsOn::Labels(&["self-hosted", "studio-release"]))
        .name("Build + push to GHCR")
        .needs(&["plan"])
        .cond("${{ needs.plan.outputs.build == 'true' }}")
        .timeout(180)
        .permissions(yaml("contents: read\npackages: write\n")?)
        // Newer skopeo rejects some hosts' v1 registries.conf; the registry
        // is named explicitly everywhere, so an empty (valid) config is enough.
        .env("CONTAINERS_REGISTRIES_CONF", "/dev/null")
        // Full history and a clean tree: the image's revision label is the
        // flake's `self.rev`, which a dirty tree does not have.
        .step(
            checkout()
                .with("ref", "${{ needs.plan.outputs.sha }}")
                .with("fetch-depth", 0)
                .with("persist-credentials", false),
        )
        .step(
            Step::run("Build and push studio-web", PUSH)
                .env("TAGS", "${{ needs.plan.outputs.tags }}")
                .env("GHCR_USER", "${{ github.actor }}")
                .env("GHCR_TOKEN", "${{ secrets.GITHUB_TOKEN }}"),
        )
        // What `just release-tag` will check: an anonymous pull, and the
        // revision label. Fails (with the reason) while the package is still
        // private.
        .step(
            Step::run("Pullable without credentials, from this commit?", PULLABLE)
                .env("TAGS", "${{ needs.plan.outputs.tags }}")
                .env("SHA", "${{ needs.plan.outputs.sha }}"),
        ))
}

/// streamLayeredImage builds an EXECUTABLE that streams a docker-archive to
/// stdout: no tarball in the store. The other tags are copied from the
/// first: the same digest by construction, the image built once.
const PUSH: &str = r####"set -euo pipefail
export PATH="$PATH:/run/current-system/sw/bin"
export REGISTRY_AUTH_FILE="$RUNNER_TEMP/ghcr-auth.json"
skopeo() { nix run nixpkgs#skopeo -- "$@"; }
printf '%s' "$GHCR_TOKEN" | skopeo login ghcr.io -u "$GHCR_USER" --password-stdin
read -r -a tags <<<"$TAGS"
dest="docker://ghcr.io/fasttrackstudios/studio-web"
streamer="$(nix build --accept-flake-config .#studio-web-image --no-link --print-out-paths)"
"$streamer" | skopeo --insecure-policy copy docker-archive:/dev/stdin "$dest:${tags[0]}"
for tag in "${tags[@]:1}"; do
  skopeo --insecure-policy copy "$dest:${tags[0]}" "$dest:$tag"
done
rm -f "$REGISTRY_AUTH_FILE"
{
  echo "### Pushed ghcr.io/fasttrackstudios/studio-web"
  for tag in "${tags[@]}"; do echo "- \`$tag\`"; done
} >> "$GITHUB_STEP_SUMMARY"
"####;

const PULLABLE: &str = r####"set -euo pipefail
first="${TAGS%% *}"
bash scripts/ghcr-images.sh check "$first" "$SHA" \
  || { echo "::warning::not anonymously pullable yet: make ghcr.io/fasttrackstudios/studio-web PUBLIC (org package settings), then re-run this job"; exit 1; }
"####;
