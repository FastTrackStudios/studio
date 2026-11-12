//! `studio-architect` — Studio's distribution of Architect (apps/architect,
//! docs/architect-distribution.md) on the release tag: its headless node
//! and its web bundle for x86_64 Linux, attached to the ONE GitHub Release
//! of `vX.Y.Z` in the shape Architect's own release attaches its app's
//! (architect `release.yml`, jobs `node` and `web`), under names that
//! cannot be taken for Architect's:
//!
//! ```text
//!   studio-architect-node-vX.Y.Z-x86_64-linux.tar.gz (+ .sha256)
//!       studio-architect-node-vX.Y.Z-x86_64-linux/studio-architect
//!       studio-architect-node-vX.Y.Z-x86_64-linux/studio-island/
//!           architect.json, architect.lock.json, studio_island.js,
//!           studio_island.*.wasm (the lock's files), island.css, BUILT_FROM
//!   studio-architect-web-vX.Y.Z.tar.gz (+ .sha256)
//!       studio-architect-web-vX.Y.Z/{index.html, docs/, app/ (+ distribution.json)}
//!   SHA256SUMS (+ SHA256SUMS.minisig, the org key)   sums.rs, over the whole Release
//! ```
//!
//! No image: the hosted node runs as a NixOS service from the tarball
//! (FastTrackStudios/infra wraps it). `studio-island/` must sit beside the
//! node's real binary: the node reads it from its own executable's folder
//! (`studio_architect::ISLAND_FOLDER`), and serves no island without it.
//!
//! ```text
//!   `just release` pushes vX.Y.Z --> resolve (desktop.rs's: the stamped
//!   version is the tag's, on main or release/v*, `CI passed` green, the
//!   Release exists) --> build (THEBATTLESHIP `thebattleship-release`, its
//!   own size-capped target dir in the pool's cache, warm across tags:
//!   scripts/studio-architect.sh release) --> publish (hosted, the only
//!   job with a write token) --> sums (signed, in release-signing)
//! ```
//!
//! workflow_dispatch: `publish` (default) re-runs a tag; `build-only`
//! builds any ref and keeps the tarballs as workflow artifacts, touching
//! no Release. Never a pull_request: PRs get `cargo check` of the node
//! (pr.rs `studio-architect`).

use crate::desktop::{NOT_VALIDATE, STAMPED, resolve};
use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout, stamp};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "studio-architect",
        yaml(
            r#"
push:
  tags: ["v*"]
workflow_dispatch:
  inputs:
    tag:
      description: "Release tag (e.g. v26.11.0) for publish; any branch or sha for build-only"
      required: true
      type: string
    mode:
      description: "publish: attach to the tag's Release. build-only: build, no Release."
      required: false
      type: choice
      options: [publish, build-only]
      default: publish
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    .concurrency(
        "studio-architect-${{ inputs.tag || github.ref_name }}",
        false,
    )
    .env("TAG", "${{ inputs.tag || github.ref_name }}")
    .env("MODE", "${{ inputs.mode || 'publish' }}")
    .job("resolve", resolve()?)
    .job("build", build())
    .job("publish", publish()?)
    .jobs(crate::sums::jobs(
        &["publish"],
        "inputs.mode != 'build-only' && needs.publish.result == 'success'",
    )?)
    .job("notify-infra", notify_infra()?))
}

/// Tell FastTrackStudios/infra that a stable release's node is up and its
/// SHA256SUMS signed: infra's `release-bump` workflow verifies the
/// signature itself, rewrites packages/architect-node/source.json and
/// opens its PR (infra#3). This job only wakes it: the token can start
/// workflows in infra and nothing else (`actions: write`, infra only), and
/// infra treats the inputs as untrusted. Infra also polls hourly, so a
/// missed dispatch costs at most an hour. Never a pre-release, a
/// build-only run or a dispatch from a branch (sums, the signed job, runs
/// only on a tag ref).
///
/// INERT until the owner sets the repo variable `RELEASE_DISPATCH` = `on`
/// and gives the environment `release-dispatch` (deployment: `v*` tags
/// only) the secret `FORGE_APP_PRIVATE_KEY` (and the repo the variable
/// `FORGE_APP_ID`). A failure here never fails the release.
fn notify_infra() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Wake infra's release bump")
        .needs(&["sums"])
        .cond("${{ vars.RELEASE_DISPATCH == 'on' && needs.sums.result == 'success' }}")
        .timeout(5)
        .continue_on_error()
        .environment("release-dispatch")
        .permissions(yaml("{}")?)
        .step(
            Step::uses("fts-forge token (infra, actions only)", "actions/create-github-app-token@v2")
                .id("forge")
                .with("app-id", "${{ vars.FORGE_APP_ID }}")
                .with("private-key", "${{ secrets.FORGE_APP_PRIVATE_KEY }}")
                .with("owner", "FastTrackStudios")
                .with("repositories", "infra")
                .with("permission-actions", "write"),
        )
        .step(
            Step::run(
                "gh workflow run release-bump.yml (infra)",
                r#"set -euo pipefail
if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "$TAG is not a stable release tag: infra is not told"; exit 0
fi
gh workflow run release-bump.yml -R FastTrackStudios/infra -f repo=FastTrackStudios/studio -f tag="$TAG"
echo "infra's release-bump started for studio $TAG" >> "$GITHUB_STEP_SUMMARY"
"#,
            )
            .env("GH_TOKEN", "${{ steps.forge.outputs.token }}"),
        ))
}

/// The release pool, as the AppImage builds: the node (Studio's checkout,
/// `--no-default-features`, `--cfg no_workspace_hack`) into a target dir
/// of its own in the pool's shared cache (its fingerprints are no check's
/// and no other release build's), and the web bundle from an Architect
/// checkout at the commit Cargo.lock pins (cloned once per commit into the
/// same cache, its own target dir beside it).
fn build() -> Job {
    Job::new(RunsOn::Labels(&["self-hosted", "thebattleship-release"]))
        .name("Node + web bundle (x86_64 Linux)")
        .needs(&["resolve"])
        .cond(NOT_VALIDATE)
        .timeout(150)
        .step(checkout().with("ref", "${{ needs.resolve.outputs.sha }}"))
        .step(Step::uses("Shared runner tools", "./.github/actions/shared-tools"))
        .step(stamp("${{ env.TAG }}").cond(STAMPED))
        .step(Step::run(
            "Own target dir, size-capped",
            r#"base="${CI_CACHE_DIR:?not a shared runner (no CI_CACHE_DIR)}"
dir="$base/studio-architect/$RUNNER_NAME/target"
cap_gb="${TARGET_CAP_GB-120}"
mkdir -p "$dir"
used_gb="$(du -s --block-size=1G "$dir" | cut -f1)"
free_gb="$(df -BG --output=avail "$base" | tail -1 | tr -dc '0-9')"
echo "target dir: $dir (${used_gb} GB used, cap ${cap_gb} GB; ${free_gb} GB free on its disk)"
if [ "$used_gb" -gt "$cap_gb" ]; then
  echo "::warning::$dir is ${used_gb} GB, over the ${cap_gb} GB cap: emptying it (cold build)"
  rm -rf "$dir"; mkdir -p "$dir"
fi
if [ "$free_gb" -lt 60 ]; then
  echo "::error::under 60 GB free on the runner's disk ($base): clean it before building"
  exit 1
fi
echo "CARGO_TARGET_DIR=$dir" >> "$GITHUB_ENV"
echo "STUDIO_ARCHITECT_CACHE=$base/studio-architect/$RUNNER_NAME" >> "$GITHUB_ENV"
"#,
        ))
        // Studio's island (#445), into the same target dir, from this very
        // checkout: the release script ships it only when its BUILT_FROM
        // is this commit. Its toolchain is the `wasi` devenv profile
        // (devenv.nix: the `wasm32-wasip2` target and node), checked here
        // first so a runner without it fails on the cause. `npm ci` in
        // Architect's web tools fetches their packages (pinned by its
        // package-lock.json).
        .step(Step::run(
            "Studio's island (wasm32-wasip2, node >= 26)",
            r#"set -euo pipefail
# The wasi profile's shell expands these, not this one.
# shellcheck disable=SC2016
devenv --profile wasi shell -- bash -c '
  test -d "$(rustc --print sysroot)/lib/rustlib/wasm32-wasip2" || { echo "::error::no wasm32-wasip2 target in the wasi profile"; exit 1; }
  node -e "process.exit(Number(process.versions.node.split(\".\")[0]) >= 26 ? 0 : 1)" || { echo "::error::node $(node --version) is older than 26"; exit 1; }
  echo "rustc $(rustc --version), node $(node --version)"
'
devenv shell -- just island
"#,
        ))
        .step(Step::run(
            "Build the node and the web bundle",
            r#"set -euo pipefail
name="${TAG//\//-}"
out="$RUNNER_TEMP/studio-architect"
rm -rf "$out"
devenv shell -- bash scripts/studio-architect.sh release "$name" "$out"
"#,
        ))
        // The node as the infra repo wraps it (autoPatchelf on NixOS): what
        // it links, so a new dependency shows here before a deploy does.
        .step(Step::run(
            "What the node links (DT_NEEDED), smoke test",
            r####"set -euo pipefail
name="${TAG//\//-}"
out="$RUNNER_TEMP/studio-architect"
tmp="$(mktemp -d)"
tar -C "$tmp" -xzf "$out/studio-architect-node-$name-x86_64-linux.tar.gz"
bin="$tmp/studio-architect-node-$name-x86_64-linux/studio-architect"
"$bin" serve --help > /dev/null
"$bin" distribution > "$tmp/distribution.json"
island="$tmp/studio-architect-node-$name-x86_64-linux/studio-island"
test -f "$island/architect.lock.json"
test "$(cat "$island/BUILT_FROM")" = "$(git describe --always --dirty --abbrev=40)"
python3 -c 'import json,sys; sys.exit(0 if "island.css" in json.load(open(sys.argv[1]))["files"] else "island.css is not in the lock")' "$island/architect.lock.json"
{
  echo "### Studio Architect $TAG"
  echo '```'
  ls -l "$out"
  echo '```'
  echo "DT_NEEDED of \`studio-architect\`:"
  echo '```'
  readelf -d "$bin" | grep NEEDED || true
  echo "interpreter: $(readelf -l "$bin" | sed -n 's/.*Requesting program interpreter: \(.*\)]/\1/p')"
  echo '```'
  echo "compiled in: $(python3 -c 'import json,sys; print(", ".join(e["id"] for e in json.load(open(sys.argv[1]))["extensions"]))' "$tmp/distribution.json")"
} >> "$GITHUB_STEP_SUMMARY"
rm -rf "$tmp"
"####,
        ))
        .step(
            Step::run(
                "Bound the target dirs",
                r#"cache="${STUDIO_ARCHITECT_CACHE:-}"
[ -n "$cache" ] && [ -d "$cache" ] || exit 0
used_gb="$(du -s --block-size=1G "$cache" | cut -f1)"
if [ "$used_gb" -gt 120 ]; then
  echo "::warning::$cache is ${used_gb} GB after the build: dropping incremental/"
  rm -rf "$cache"/target/*/incremental "$cache"/architect-target/*/incremental
fi
echo "cache after: $(du -sh "$cache" | cut -f1)"
"#,
            )
            .cond("always()"),
        )
        .step(
            Step::uses("Keep the tarballs", "actions/upload-artifact@v4")
                .with("name", "studio-architect")
                .with("path", "${{ runner.temp }}/studio-architect/*.tar.gz*\n")
                .with("if-no-files-found", "error")
                .with("retention-days", 14),
        )
}

/// The only job with a write token: the four files are there, named with
/// the tag, their sidecars match, and GitHub's digest of each uploaded
/// file is its sha256 (what the sums job sums).
fn publish() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Attach to the GitHub Release")
        .needs(&["resolve", "build"])
        .cond("${{ inputs.mode != 'build-only' && needs.build.result == 'success' }}")
        .timeout(15)
        .permissions(yaml("contents: write\n")?)
        .step(
            Step::uses("The tarballs", "actions/download-artifact@v4")
                .with("name", "studio-architect")
                .with("path", "dist"),
        )
        .step(
            Step::run(
                "Verify, then attach to the one Release",
                r####"set -euo pipefail
cd dist
ls -l
files=("studio-architect-node-$TAG-x86_64-linux.tar.gz" "studio-architect-web-$TAG.tar.gz")
for f in "${files[@]}"; do
  test -f "$f"
  sha256sum -c "$f.sha256"
done
gh release upload "$TAG" --repo "$GITHUB_REPOSITORY" --clobber ./*.tar.gz ./*.tar.gz.sha256
for f in "${files[@]}"; do
  want="sha256:$(sha256sum "$f" | cut -d' ' -f1)"
  got="$(gh api "repos/$GITHUB_REPOSITORY/releases/tags/$TAG" --jq ".assets[] | select(.name == \"$f\") | .digest")"
  [ "$got" = "$want" ] || { echo "::error::$f: GitHub's digest is '$got', the file is $want"; exit 1; }
done
{
  echo "### Attached to $TAG"
  for f in "${files[@]}"; do echo "- \`$f\` ($(du -h "$f" | cut -f1))"; done
} >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .env("GH_TOKEN", "${{ github.token }}"),
        ))
}
