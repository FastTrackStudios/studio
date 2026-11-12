//! `patchbay-ios` — build Patchbay for iPhone and iPad (apps/patchbay
//! `patchbay-ios`: the Patchbay distribution of Architect, a controller of
//! the Patchbay engines on other machines) and upload it to TestFlight,
//! ONLY for a release: the one studio tag `vX.Y.Z`. `studio-ios` beside it,
//! by shape (resolve, testflight, record), and architect's `ios.yml` by
//! what it runs (`apps/patchbay/ios/deploy-testflight.sh`, then the
//! read-only `testflight-status.rb` until App Store Connect lists the
//! upload).
//!
//! ```text
//!   `just release` pushes vYY.M.MICRO --> this workflow starts on the tag
//!   --> resolve (hosted: the tag's version stamped in, tag shape, stamped
//!   version == tag version, commit on main or release/v*, `CI passed`
//!   green, Release exists) --> testflight (airlock, in the tag-only
//!   `release-signing` environment: build, sign, upload, wait for App Store
//!   Connect to list it) --> record (hosted: the build number into the
//!   Release notes' TestFlight block, Patchbay's line).
//! ```
//!
//! Triggers: a tag push, and workflow_dispatch with the tag. A dispatch
//! runs from the tag (`gh workflow run patchbay-ios.yml --ref vX.Y.Z -f
//! tag=vX.Y.Z`): `release-signing` deploys only from a `v*` tag, so a
//! dispatch from a branch stops at `testflight`. `dry_run` builds, signs
//! and validates with App Store Connect, and uploads nothing. No
//! pull_request trigger, so no fork or PR ever reaches the runner.
//!
//! The upload runs on airlock's `studio-release-mac`, as `studio-ios` does,
//! and needs no GitHub secret: the machine holds the App Store Connect key
//! (~/.appstoreconnect), the Apple Distribution identity (fts-build
//! keychain) and the `FTS Patchbay App Store` profile. The repository
//! variable `PATCHBAY_IOS_RELEASE=false` turns the build off (airlock down,
//! or before Patchbay's App Store Connect record exists: altool refuses
//! an upload for a bundle id with no app record). The build number
//! (CFBundleVersion) is the upload's unix time.

use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout, checkout_full, stamp};
use anyhow::Result;

/// Patchbay's bundle id: apps/patchbay/ios/Info.plist's, and
/// `scripts/release.sh`'s `ios_bundle_id patchbay`.
const BUNDLE_ID: &str = "app.fasttrackstudio.patchbay";

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "patchbay-ios",
        yaml(
            r#"
push:
  tags:
    - "v*"
workflow_dispatch:
  inputs:
    tag:
      description: "Release tag to ship to TestFlight (e.g. v26.11.2): run it from that tag (--ref)"
      required: true
      type: string
    dry_run:
      description: "Build, sign and validate with App Store Connect; do not upload"
      required: false
      type: boolean
      default: false
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    // Per tag; a re-run of the same tag queues behind the first.
    // `studio-release-mac` is one runner: it takes this, studio-ios and the
    // .dmg one at a time.
    .concurrency(
        "ios-testflight-${{ github.workflow }}-${{ inputs.tag || github.ref_name }}",
        false,
    )
    .env("TAG", "${{ inputs.tag || github.ref_name }}")
    .job("resolve", resolve()?)
    .job("testflight", testflight())
    .job("record", record()?))
}

fn resolve() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Resolve the tag")
        // Never a fork; and off while PATCHBAY_IOS_RELEASE is `false`.
        .cond("${{ github.repository == 'FastTrackStudios/studio' && vars.PATCHBAY_IOS_RELEASE != 'false' }}")
        .timeout(10)
        .permissions(yaml("contents: write\nchecks: read\n")?)
        .output("version", "${{ steps.resolve.outputs.version }}")
        .output("sha", "${{ steps.resolve.outputs.sha }}")
        .step(Step::run(
            "The tag is vX.Y.Z",
            r#"if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "::error::'$TAG' is not a release tag (^v[0-9]+\.[0-9]+\.[0-9]+$): TestFlight uploads only from the studio release tag"
  exit 1
fi
"#,
        ))
        .step(checkout_full(Some("refs/tags/${{ env.TAG }}")))
        .step(stamp("${{ env.TAG }}"))
        .step(
            Step::run(
                "Marketing version = tag version, tag on main or release/v*, green",
                "bash scripts/release.sh ios-resolve patchbay \"$TAG\"\n",
            )
            .id("resolve")
            .env("GH_TOKEN", "${{ github.token }}"),
        )
        .step(
            Step::run(
                "A GitHub Release exists for the tag",
                "bash scripts/release.sh ios-release \"$TAG\"\n",
            )
            .cond("${{ !inputs.dry_run }}")
            .env("GH_TOKEN", "${{ github.token }}"),
        ))
}

fn testflight() -> Job {
    Job::new(RunsOn::Labels(&["self-hosted", "macos", "studio-release-mac"]))
        .name("Build, sign, upload")
        .needs(&["resolve"])
        // Deploys only from a `v*` tag (the environment's one policy).
        .environment("release-signing")
        // A missing runner turns red within the hour instead of sitting
        // queued until it is cancelled.
        .timeout(90)
        .output("build_no", "${{ steps.stamp.outputs.build_no }}")
        .step(Step::run(
            "Preflight — build volume mounted",
            r#"if [ ! -d /Volumes/build-disk ]; then
  echo "ERROR: /Volumes/build-disk is not mounted on this runner." >&2
  exit 1
fi
df -h /Volumes/build-disk | tail -1
"#,
        ))
        // The commit `resolve` checked; clean:false keeps target/ warm.
        .step(
            checkout()
                .with("ref", "${{ needs.resolve.outputs.sha }}")
                .with("clean", false),
        )
        .step(stamp("${{ env.TAG }}"))
        // CFBundleVersion has to climb with every upload: the unix time,
        // taken once, here, so the number built is the number recorded.
        .step(
            Step::run(
                "Build number",
                r#"build_no="$(date +%s)"
echo "build_no=$build_no" >> "$GITHUB_OUTPUT"
echo "Patchbay $TAG: marketing version ${{ needs.resolve.outputs.version }}, build $build_no"
"#,
            )
            .id("stamp"),
        )
        .step(Step::uses("devenv CLI", "./.github/actions/devenv"))
        .step(upload())
        .step(listed())
        .step(
            Step::run(
                "Summary",
                r####"echo "### Patchbay iOS TestFlight" >> "$GITHUB_STEP_SUMMARY"
echo "\`$TAG\` (\`$(git rev-parse --short HEAD)\`): marketing version \`$VERSION\`, build \`$BUILD_NO\`$([ "$DRY_RUN" = true ] && echo ' (dry run: not uploaded)'): $RESULT." >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .cond("always()")
            .env("VERSION", "${{ needs.resolve.outputs.version }}")
            .env("BUILD_NO", "${{ steps.stamp.outputs.build_no }}")
            .env("DRY_RUN", "${{ inputs.dry_run }}")
            .env("RESULT", "${{ job.status }}"),
        )
}

/// apps/patchbay/ios/deploy-testflight.sh: a release build for the phone
/// (`patchbay-ios`), signed with the profile's entitlements, validated,
/// then uploaded (`--dry-run`: not). The release Xcode xcode-select points
/// at builds, compiles the icon and signs: App Store Connect refuses a
/// beta SDK (90534).
fn upload() -> Step {
    Step::run(
        "Build, sign, and upload Patchbay to TestFlight",
        "\
set -euo pipefail
export PATH=\"/nix/var/nix/profiles/default/bin:$PATH\"
export TMPDIR=\"$RUNNER_TEMP\"
flags=()
[ \"$DRY_RUN\" = true ] && flags=(--dry-run)
bash apps/patchbay/ios/deploy-testflight.sh ${flags[@]+\"${flags[@]}\"}
",
    )
    .env("KEYCHAIN", "fts-build.keychain")
    .env("KEYCHAIN_PW", "fts-build")
    .env("MARKETING_VER", "${{ needs.resolve.outputs.version }}")
    .env("BUILD_NO", "${{ steps.stamp.outputs.build_no }}")
    .env("DRY_RUN", "${{ inputs.dry_run }}")
    .env("DX_BUNDLE_ID", BUNDLE_ID)
    .env("PROFILE_NAME", "FTS Patchbay App Store")
}

/// App Store Connect lists an upload once it starts processing it (a few
/// minutes): its state, with Apple's errors and warnings, into the
/// summary. The job does not wait for it to be VALID.
fn listed() -> Step {
    Step::run(
        "App Store Connect has the build",
        &format!(
            r#"set -uo pipefail
for _ in $(seq 1 30); do
  if out="$(ruby apps/patchbay/ios/testflight-status.rb {BUNDLE_ID} "$BUILD_NO")"; then
    echo "$out"
    {{ echo '```'; echo "$out"; echo '```'; }} >> "$GITHUB_STEP_SUMMARY"
    exit 0
  fi
  sleep 30
done
echo "::warning::build $BUILD_NO is not in App Store Connect after 15 minutes (apps/patchbay/ios/testflight-status.rb shows it later)"
"#
        ),
    )
    .cond("${{ !inputs.dry_run }}")
    .env("BUILD_NO", "${{ steps.stamp.outputs.build_no }}")
}

fn record() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Record on the GitHub Release")
        .needs(&["resolve", "testflight"])
        .cond("${{ !inputs.dry_run }}")
        .timeout(10)
        .permissions(yaml("contents: write\n")?)
        .step(checkout().with("ref", "${{ needs.resolve.outputs.sha }}"))
        .step(
            Step::run(
                "Marketing version + build number into the release notes",
                "bash scripts/release.sh ios-record patchbay \"$TAG\" \"$VERSION\" \"$BUILD_NO\" \"$RUN_URL\"\n",
            )
            .env("GH_TOKEN", "${{ github.token }}")
            .env("VERSION", "${{ needs.resolve.outputs.version }}")
            .env("BUILD_NO", "${{ needs.testflight.outputs.build_no }}")
            .env(
                "RUN_URL",
                "${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}",
            ),
        ))
}
