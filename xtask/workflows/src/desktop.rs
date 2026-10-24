//! `desktop` — the Studio desktop installers on the release tag: a signed,
//! notarized macOS `.dmg` and a Linux AppImage, attached to the ONE GitHub
//! Release of `vX.Y.Z`, beside the TestFlight build `studio-ios` makes from
//! the same tag. scripts/install.sh installs them (README, "Install").
//!
//! ```text
//!   `just release` pushes vX.Y.Z --> this, Images, studio-ios and the
//!   release publish all start on the tag, in parallel --> resolve (hosted:
//!   the tag's version stamped in, tag shape, the app crate's version == the
//!   tag's, commit on main or release/v*, `CI passed` green, Release exists) --> macos (airlock) and linux
//!   (THEBATTLESHIP, `studio-release`) build in parallel and upload
//!   workflow artifacts --> publish (hosted) checks them and attaches them
//!   to the Release.
//! ```
//!
//! The signing identity and the notarization key never leave their machine:
//! `macos` runs on airlock (the Mac mini), on studio's own release runner
//! there, `studio-release-mac` (its own warm checkout, shared with the
//! TestFlight build), whose build keychain holds the Developer ID Application identity and whose
//! ~/.appstoreconnect holds the App Store Connect key notarytool uses, so
//! this reads no GitHub secret. Only the hosted `publish` job can write to
//! the Release; the jobs that touch the identity have a read-only token.
//!
//! Triggers: a tag push (`just release`, pushed as the fts-forge App or
//! a person: a tag pushed with GITHUB_TOKEN starts no workflow), and
//! workflow_dispatch:
//!
//! ```text
//!   mode publish     (default) the above, for an existing tag (a re-run)
//!   mode validate    resolve only: the tag is a sound release
//!   mode build-only  build, sign, notarize and smoke-test from ANY ref (a
//!                    branch, a sha), keep the installers as workflow
//!                    artifacts, touch no Release: how a change to the
//!                    installers is proven before there is a tag
//! ```
//!
//! Never a pull_request, so no fork or PR reaches the runners.

use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout, rig_models, stamp};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "desktop",
        yaml(
            r#"
push:
  tags: ["v*"]
workflow_dispatch:
  inputs:
    tag:
      description: "Release tag (e.g. v26.10.1) for publish/validate; any branch or sha for build-only"
      required: true
      type: string
    mode:
      description: "publish: attach to the tag's Release. validate: resolve only. build-only: build + notarize + smoke-test, no Release."
      required: false
      type: choice
      options: [publish, validate, build-only]
      default: publish
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    // Per tag: a re-run of the same tag queues behind the first instead of
    // cancelling it (artifacts and uploads are idempotent).
    .concurrency("desktop-${{ inputs.tag || github.ref_name }}", false)
    .env("TAG", "${{ inputs.tag || github.ref_name }}")
    .env("MODE", "${{ inputs.mode || 'publish' }}")
    .job("resolve", resolve()?)
    .job("macos", macos())
    .job("linux", linux())
    .job("publish", publish()?))
}

const NOT_VALIDATE: &str = "${{ inputs.mode != 'validate' }}";

/// A release build takes the tag's version (scripts/release.sh `stamp`); a
/// build-only run builds the ref with the version it commits.
const STAMPED: &str = "${{ env.MODE != 'build-only' }}";

/// The same checks TestFlight makes (release.sh ios-resolve): the tag is
/// vX.Y.Z, the stamped studio-desktop's version is the tag's, the commit is
/// on main or release/v*, `CI passed` is green. A
/// build-only run checks none of that and builds the ref as it is.
fn resolve() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Resolve the tag")
        // Never a fork: its tag is not this repo's release.
        .cond("${{ github.repository == 'FastTrackStudios/studio' }}")
        .timeout(10)
        // The GitHub Release is created here when the tag has none yet.
        .permissions(yaml("contents: write\n")?)
        .output("version", "${{ steps.resolve.outputs.version || steps.ref.outputs.version }}")
        .output("sha", "${{ steps.resolve.outputs.sha || steps.ref.outputs.sha }}")
        .step(
            Step::run(
                "The tag is vX.Y.Z",
                r#"if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "::error::'$TAG' is not a release tag (^v[0-9]+\.[0-9]+\.[0-9]+$): installers are attached only to the studio release tag"
  exit 1
fi
"#,
            )
            .cond("${{ env.MODE != 'build-only' }}"),
        )
        .step(
            checkout()
                .with(
                    "ref",
                    "${{ env.MODE == 'build-only' && env.TAG || format('refs/tags/{0}', env.TAG) }}",
                )
                .with("fetch-depth", 0),
        )
        .step(stamp("${{ env.TAG }}").cond(STAMPED))
        .step(
            Step::run(
                "Version = tag version, on main or release/v*, CI passed",
                "bash scripts/release.sh ios-resolve studio \"$TAG\"\n",
            )
            .id("resolve")
            .cond("${{ env.MODE != 'build-only' }}")
            .env("GH_TOKEN", "${{ github.token }}"),
        )
        .step(
            Step::run(
                "Build-only ref",
                r#"echo "version=$(bash scripts/release.sh version)" >> "$GITHUB_OUTPUT"
echo "sha=$(git rev-parse HEAD)" >> "$GITHUB_OUTPUT"
"#,
            )
            .id("ref")
            .cond("${{ env.MODE == 'build-only' }}"),
        )
        .step(
            Step::run("A GitHub Release exists for the tag", "bash scripts/release.sh ios-release \"$TAG\"\n")
                .cond("${{ env.MODE == 'publish' }}")
                .env("GH_TOKEN", "${{ github.token }}"),
        )
        .step(
            Step::run(
                "Summary",
                r####"{
  echo "### Studio desktop installers: $TAG ($MODE)"
  echo "- version: \`$VERSION\`"
  echo "- commit: \`${SHA:0:12}\`"
  if [ "$MODE" = validate ]; then echo "- **validate**: stopped before any build"; fi
  if [ "$MODE" = build-only ]; then echo "- **build-only**: nothing will be attached to a Release"; fi
} >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .env("VERSION", "${{ steps.resolve.outputs.version || steps.ref.outputs.version }}")
            .env("SHA", "${{ steps.resolve.outputs.sha || steps.ref.outputs.sha }}"),
        ))
}

/// airlock's `studio-release-mac`: build, Developer-ID sign, notarize,
/// staple (scripts/desktop/macos.sh). A missing runner becomes a red X within the time limit, not a
/// job that waits for hours.
fn macos() -> Job {
    Job::new(RunsOn::Labels(&["self-hosted", "macos", "studio-release-mac"]))
        .name("macOS .dmg")
        .needs(&["resolve"])
        .cond(NOT_VALIDATE)
        .timeout(150)
        .step(Step::run(
            "Preflight — build volume mounted, space free",
            r#"if [ ! -d /Volumes/build-disk ]; then
  echo "ERROR: /Volumes/build-disk is not mounted on this runner." >&2
  exit 1
fi
df -h /Volumes/build-disk | tail -1
avail_kb="$(df -k /Volumes/build-disk | awk 'NR==2 {print $4}')"
if [ "$avail_kb" -lt $((50 * 1024 * 1024)) ]; then
  echo "ERROR: under 50 GB free on /Volumes/build-disk: sweep target/ before building" >&2
  exit 1
fi
"#,
        ))
        // The commit `resolve` checked. clean:false keeps target/ between
        // runs, so cargo builds incrementally (studio-ios builds here too).
        .step(
            checkout()
                .with("ref", "${{ needs.resolve.outputs.sha }}")
                .with("clean", false),
        )
        .step(stamp("${{ env.TAG }}").cond(STAMPED))
        // The guitar rig's release build refuses to compile without its
        // captures (features/rigs/guitar/build.rs): airlock's rig-models
        // clone, fast-forwarded first (as studio-ios does).
        .step(Step::run(
            "Rig captures (rig-models) linked into the tree",
            &rig_models("/Volumes/build-disk/development/rig-models", true),
        ))
        .step(
            Step::run(
                "Build, sign, notarize, staple",
                r#"set -euo pipefail
export PATH="/nix/var/nix/profiles/default/bin:$PATH"
export NIX=/nix/var/nix/profiles/default/bin/nix
BUILD_NO="$(date +%s)"
export BUILD_NO
rm -rf target/desktop
bash scripts/desktop/macos.sh
ls -l target/desktop
"#,
            )
            .env("KEYCHAIN", "fts-build.keychain")
            .env("KEYCHAIN_PW", "fts-build")
            .env("VERSION", "${{ needs.resolve.outputs.version }}"),
        )
        .step(
            Step::uses("Keep the .dmg", "actions/upload-artifact@v4")
                .with("name", "desktop-macos")
                .with("path", "target/desktop/*.dmg\n")
                .with("if-no-files-found", "error")
                .with("retention-days", 14),
        )
        .step(
            Step::run(
                "Summary",
                r####"{
  echo "### macOS installer: $TAG"
  echo "version \`$VERSION\`, $RESULT."
  for f in target/desktop/*.dmg; do [ -f "$f" ] && echo "- \`$(basename "$f")\` ($(du -h "$f" | cut -f1))"; done
} >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .cond("always()")
            .env("VERSION", "${{ needs.resolve.outputs.version }}")
            .env("RESULT", "${{ job.status }}"),
        )
}

/// THEBATTLESHIP's `studio-release` pool (x86_64 NixOS; the pool for
/// release builds, never checks): the AppImage (scripts/desktop/linux.sh),
/// smoke-tested headless.
fn linux() -> Job {
    Job::new(RunsOn::Labels(&["self-hosted", "studio-release"]))
        .name("Linux AppImage")
        .needs(&["resolve"])
        .cond(NOT_VALIDATE)
        .timeout(120)
        .step(checkout().with("ref", "${{ needs.resolve.outputs.sha }}"))
        .step(stamp("${{ env.TAG }}").cond(STAMPED))
        // Its OWN target dir in the release pool's shared cache, so whichever
        // instance claims the job builds warm, with a size cap (a release
        // build has other fingerprints than any check's). Over the
        // cap before the build it is emptied (a cold build); over it after,
        // only `incremental/` is dropped.
        .step(Step::run(
            "Own target dir, size-capped",
            r#"base="${STUDIO_CACHE_DIR:?not a studio pool runner (no STUDIO_CACHE_DIR)}"
dir="$base/desktop-linux"
cap_gb="${TARGET_CAP_GB-150}"
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
echo "TARGET_CAP_GB=$cap_gb" >> "$GITHUB_ENV"
"#,
        ))
        // THEBATTLESHIP keeps one rig-models clone for CI beside the
        // runners' work dirs. Linked, not fetched: this runner has no
        // credential for the private repo.
        .step(Step::run(
            "Rig captures (rig-models) linked into the tree",
            &rig_models("/run/media/Development/ci-assets/rig-models", false),
        ))
        .step(
            Step::run(
                "Build the AppImage",
                "set -euo pipefail\nrm -rf target/desktop\nbash scripts/desktop/linux.sh\nls -l target/desktop\n",
            )
            .env("VERSION", "${{ needs.resolve.outputs.version }}"),
        )
        // --version with no display: the file starts on a machine without
        // the app's libraries (scripts/desktop/linux-smoke.sh: appimage-run,
        // NixOS's FHS sandbox, or the payload through the host's loader where
        // the runner has no user namespaces). The HARNESS is the workflow's
        // own commit's, the AppImage the tag's: a `publish` dispatch from main
        // re-runs a tag whose own copy of the test could not run here.
        .step(
            Step::run("Smoke test, headless", SMOKE)
                .env("VERSION", "${{ needs.resolve.outputs.version }}"),
        )
        .step(
            Step::run(
                "Bound the target dir",
                r#"dir="${CARGO_TARGET_DIR:-}"
[ -n "$dir" ] && [ -d "$dir" ] || exit 0
used_gb="$(du -s --block-size=1G "$dir" | cut -f1)"
if [ "$used_gb" -gt "${TARGET_CAP_GB-150}" ]; then
  echo "::warning::$dir is ${used_gb} GB after the build: dropping incremental/"
  rm -rf "$dir"/*/incremental
fi
echo "target dir after: $(du -sh "$dir" | cut -f1)"
"#,
            )
            .cond("always()"),
        )
        .step(
            Step::uses("Keep the AppImage", "actions/upload-artifact@v4")
                .with("name", "desktop-linux")
                .with(
                    "path",
                    "target/desktop/*.AppImage\ntarget/desktop/*-glibc-floor.txt\n",
                )
                .with("if-no-files-found", "error")
                .with("retention-days", 14),
        )
        .step(
            Step::run(
                "Summary",
                r####"{
  echo "### Linux installer: $TAG"
  echo "version \`$VERSION\`, $RESULT."
  for f in target/desktop/*.AppImage; do [ -f "$f" ] && echo "- \`$(basename "$f")\` ($(du -h "$f" | cut -f1))"; done
  for f in target/desktop/*-glibc-floor.txt; do [ -f "$f" ] && echo "- needs glibc >= $(cat "$f")"; done
} >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .cond("always()")
            .env("VERSION", "${{ needs.resolve.outputs.version }}")
            .env("RESULT", "${{ job.status }}"),
        )
}

const SMOKE: &str = r#"set -euo pipefail
export PATH="$PATH:/run/current-system/sw/bin"
smoke="$RUNNER_TEMP/linux-smoke.sh"
git fetch --quiet --depth 1 origin "$GITHUB_WORKFLOW_SHA" 2>/dev/null || true
if git show "$GITHUB_WORKFLOW_SHA:scripts/desktop/linux-smoke.sh" > "$smoke" 2>/dev/null; then
  echo "smoke test from the workflow's commit ${GITHUB_WORKFLOW_SHA:0:12}"
else
  cp scripts/desktop/linux-smoke.sh "$smoke"
  echo "smoke test from the checked-out tree"
fi
bash "$smoke" "target/desktop/Studio-$VERSION-linux-x86_64.AppImage" "$VERSION"
"#;

/// The only job with a write token: checks both installers are there (named
/// with the release's version) and attaches them. No .sha256 files: GitHub
/// computes each asset's sha256 on upload and publishes it as the asset's
/// `digest`, which scripts/install.sh checks; this job checks it matches the
/// file it uploaded.
fn publish() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Attach to the GitHub Release")
        .needs(&["resolve", "macos", "linux"])
        .cond(
            "${{ inputs.mode != 'validate' && inputs.mode != 'build-only' && \
             needs.macos.result == 'success' && needs.linux.result == 'success' }}",
        )
        .timeout(15)
        .permissions(yaml("contents: write\n")?)
        .step(
            Step::uses("The installers", "actions/download-artifact@v4")
                .with("path", "installers")
                .with("merge-multiple", true),
        )
        .step(
            Step::run(
                "Verify, then attach to the one Release",
                r####"set -euo pipefail
cd installers
ls -l
test -f "Studio-$VERSION-macos-arm64.dmg"
test -f "Studio-$VERSION-linux-x86_64.AppImage"
gh release upload "$TAG" --repo "$GITHUB_REPOSITORY" --clobber ./*.dmg ./*.AppImage
# GitHub's digest for each uploaded asset must be the file's own sha256.
for f in ./*.dmg ./*.AppImage; do
  name="$(basename "$f")"
  want="sha256:$(sha256sum "$f" | cut -d' ' -f1)"
  got="$(gh api "repos/$GITHUB_REPOSITORY/releases/tags/$TAG" --jq ".assets[] | select(.name == \"$name\") | .digest")"
  [ "$got" = "$want" ] || { echo "::error::$name: GitHub's digest is '$got', the file is $want"; exit 1; }
done
{
  echo "### Attached to $TAG"
  # shellcheck disable=SC2016 # the backticks are markdown, for jq's output
  gh release view "$TAG" --repo "$GITHUB_REPOSITORY" --json assets --jq '.assets[] | "- `\(.name)` (\(.size) bytes, `\(.digest)`)"'
} >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .env("GH_TOKEN", "${{ github.token }}")
            .env("VERSION", "${{ needs.resolve.outputs.version }}"),
        ))
}
