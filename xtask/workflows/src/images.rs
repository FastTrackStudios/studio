//! `images` — studio's public image, built and pushed to GHCR from the
//! release TAG:
//!
//! ```text
//!   ghcr.io/fasttrackstudios/studio-web:<X.Y.Z>   (+ sha-<short>, latest)
//!
//!   just release -> pushes vX.Y.Z on a green commit of main (or release/v*)
//!     -> this (beside studio-ios, desktop and the GitHub Release, all from
//!        the tag, in parallel): the tag guard, then `.#studio-web-image`
//!        built from the tag with the tag's version stamped in, pushed
//!     -> argocd-image-updater (starcommand) picks the highest X.Y.Z image
//!        and Argo CD rolls it out. This repo deploys nothing.
//! ```
//!
//! Nothing waits for this: `just release` does not, and the Release does
//! not. The test suite is not run again; the tag guard checks `CI passed`
//! is already green on the tagged commit.
//!
//! THE IMAGE TAG IS EXACTLY THE BARE CALVER (`26.10.4`: no `v`, no leading
//! zeros). The cluster's image-updater filters tags by
//! `^[0-9]+\.[0-9]+\.[0-9]+$` and orders them as semver, which orders CalVer
//! correctly only in that shape; `plan` refuses any other. `latest` and
//! `sha-<short>` ride along (the filter ignores them).
//!
//! workflow_dispatch with `tag`: build and push the image for an EXISTING
//! release tag from that tag's source (re-publishing a lost image). Never
//! moves `latest`.
//!
//! Packages pushed with GITHUB_TOKEN are created PRIVATE: the org made
//! `studio-web` public once, after its first push, or the cluster could not
//! pull it (and the anonymous `ghcr-images.sh check` would fail).

use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout, checkout_full, stamp};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "Images",
        yaml(
            r#"
push:
  tags: ["v*"]
workflow_dispatch:
  inputs:
    tag:
      description: "Existing release tag to (re)publish the image for, e.g. v26.10.4"
      required: true
      type: string
"#,
        )?,
    )
    // One group PER TAG, never cancelled: a release build runs to the end
    // (a shared group holds one pending run and would drop a queued
    // release). The `studio-release` pool takes them in arrival order.
    .concurrency("images-${{ inputs.tag || github.ref_name }}", false)
    .permissions(yaml("contents: read\n")?)
    .env("TAG", "${{ inputs.tag || github.ref_name }}")
    .job("plan", plan()?)
    .job("publish", publish()?))
}

/// Is the tag a release of a green commit, and which tags does its image
/// get? Hosted, cheap.
fn plan() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("What to publish")
        // Never a fork: its tag is not this repo's release.
        .cond("${{ github.repository == 'FastTrackStudios/studio' }}")
        .timeout(10)
        // checks: the tag guard reads the tagged commit's `CI passed`.
        .permissions(yaml("contents: read\nchecks: read\n")?)
        .output("sha", "${{ steps.plan.outputs.sha }}")
        .output("tags", "${{ steps.plan.outputs.tags }}")
        .step(Step::run("The image tag is the bare CalVer", CALVER))
        .step(checkout_full(Some("refs/tags/${{ env.TAG }}")))
        .step(stamp("${{ env.TAG }}"))
        .step(
            Step::run(
                "The tag is a green release on main or release/v*",
                // A dispatch runs on a branch ref: judge the tag it names.
                "GITHUB_REF=\"refs/tags/$TAG\" bash scripts/release.sh tag-guard\n",
            )
            .env("GH_TOKEN", "${{ github.token }}"),
        )
        .step(
            Step::run("Plan", PLAN)
                .id("plan")
                .env("EVENT", "${{ github.event_name }}"),
        ))
}

/// argocd-image-updater's contract (starcommand): exactly YY.M.MICRO, no
/// leading zeros. Inline, not the tag's script: a re-publish of an older
/// tag is judged by today's rule.
const CALVER: &str = r#"set -euo pipefail
v="${TAG#v}"
if [ "$v" = "$TAG" ] || [[ ! "$v" =~ ^[1-9][0-9]\.(1[0-2]|[1-9])\.(0|[1-9][0-9]*)$ ]]; then
  echo "::error::'$TAG' is not vYY.M.MICRO CalVer: its image tag '$v' would not match the cluster's ^[1-9][0-9]\.(1[0-2]|[1-9])\.(0|[1-9][0-9]*)\$ and would sort wrong or be ignored"
  exit 1
fi
echo "image tag: $v"
"#;

const PLAN: &str = r####"set -euo pipefail
sha="$(git rev-parse HEAD)"
v="${TAG#v}"
extra="latest"
# A re-publish never moves `latest` (a newer release may own it).
[ "$EVENT" != workflow_dispatch ] || extra=""
tags="$(xargs <<<"$v sha-$(git rev-parse --short "$sha") $extra")"
echo "publishing $sha as: $tags"
{
  echo "sha=$sha"
  echo "tags=$tags"
} >> "$GITHUB_OUTPUT"
echo "### studio-web for $TAG: \`$tags\` from \`$sha\`" >> "$GITHUB_STEP_SUMMARY"
"####;

/// THEBATTLESHIP's `studio-release` pool (Codys-Wright/nix-config
/// hosts/THEBATTLESHIP/github-runners.nix; x86_64 NixOS, the pool for
/// release builds, never checks, as desktop.yml's AppImage): the image is
/// Linux-only and a nix build, and the pool's nix store is warm. Only
/// trusted code reaches it: a tag push, or a dispatch.
fn publish() -> Result<Job> {
    Ok(Job::new(RunsOn::Labels(&["self-hosted", "studio-release"]))
        .name("Build + push to GHCR")
        .needs(&["plan"])
        .timeout(180)
        .permissions(yaml("contents: read\npackages: write\n")?)
        // Newer skopeo rejects some hosts' v1 registries.conf; the registry
        // is named explicitly everywhere, so an empty (valid) config is enough.
        .env("CONTAINERS_REGISTRIES_CONF", "/dev/null")
        .step(
            checkout()
                .with("ref", "${{ needs.plan.outputs.sha }}")
                .with("fetch-depth", 0)
                .with("persist-credentials", false),
        )
        // The tag's version into the source the image is built from (the
        // web bundles' CARGO_PKG_VERSION, the image's version label).
        .step(stamp("${{ env.TAG }}"))
        .step(
            Step::run("Build and push studio-web", PUSH)
                .env("TAGS", "${{ needs.plan.outputs.tags }}")
                .env("SHA", "${{ needs.plan.outputs.sha }}")
                .env("GHCR_USER", "${{ github.actor }}")
                .env("GHCR_TOKEN", "${{ secrets.GITHUB_TOKEN }}"),
        )
        // An anonymous pull and the revision label: what the cluster gets.
        // Fails (with the reason) if the package went private. Nothing
        // waits on it.
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
# The stamp left the tree dirty (no self.rev): the revision label is the
# tagged commit, passed in (nix/modules/crane.nix `fts.buildRev`, read
# only under --impure).
streamer="$(FTS_BUILD_REV="$SHA" nix build --impure --accept-flake-config .#studio-web-image --no-link --print-out-paths)"
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

const PULLABLE: &str = r#"set -euo pipefail
first="${TAGS%% *}"
bash scripts/ghcr-images.sh check "$first" "$SHA" \
  || { echo "::warning::not anonymously pullable yet: make ghcr.io/fasttrackstudios/studio-web PUBLIC (org package settings), then re-run this job"; exit 1; }
"#;
