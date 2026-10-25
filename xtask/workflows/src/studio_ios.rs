//! `studio-ios` — build the Studio iPhone/iPad app (Signal and Session in one
//! app, apps/studio/desktop) and upload it to TestFlight, ONLY for a
//! release: the one studio tag `vX.Y.Z`. Not on a push to main, not on a PR
//! or a fork, not by path. The TestFlight marketing version
//! (CFBundleShortVersionString) IS the tag's version, and the tag is a
//! GitHub Release (docs/agents/release-workflow.md). Signal's and Session's
//! own App Store records get no new builds: Studio replaced them.
//!
//! ```text
//!   PRs --squash--> main           ships NOTHING
//!   `just release` pushes vYY.M.MICRO on a green commit (of main, or of a
//!   release/v* hotfix line) --> this workflow starts on the tag -->
//!   resolve (hosted: the tag's version stamped in, tag shape, app crate
//!   version == tag version, commit on main or release/v*, `CI passed`
//!   green, Release exists) --> testflight (airlock: build, sign, upload) --> record
//!   (hosted: marketing version + build number written into the Release
//!   notes; a re-run's edit re-reads and retries).
//! ```
//!
//! Triggers: a tag push (`just release`, pushed as the fts-forge App or
//! a person: a tag pushed with GITHUB_TOKEN starts no workflow), and
//! workflow_dispatch with the tag (a re-run). `dry_run` stops after
//! `resolve`, before anything is built or uploaded. No pull_request
//! trigger, so no fork or PR ever reaches the secrets or the runner.
//!
//! The upload runs on airlock (the Mac mini), on the org-level self-hosted
//! runner `studio-release-mac`: studio's own release runner there, with its
//! own warm checkout, so it never queues behind Task's jobs. The machine
//! holds its own credentials (~/.appstoreconnect + fts-build.keychain),
//! so this needs no GitHub secrets.
//!
//! Reuses the same parameterized deploy-testflight.sh the other products
//! use; only the DX_* env below differs. The build number (CFBundleVersion)
//! is the unix time of the upload, so it keeps climbing across re-runs; it
//! is recorded in the step summary and in the release notes.

use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{checkout, checkout_full, rig_models, stamp};
use anyhow::Result;

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "studio-ios",
        yaml(
            r#"
push:
  tags:
    - "v*"
workflow_dispatch:
  inputs:
    tag:
      description: "Release tag to ship to TestFlight (e.g. v26.10.3): must be on main or release/v*"
      required: true
      type: string
    dry_run:
      description: "Validate the tag and stop before building or uploading"
      required: false
      type: boolean
      default: false
"#,
        )?,
    )
    .permissions(yaml("contents: read\n")?)
    // Per tag, so two releases in a row are not collapsed into one pending
    // run (a concurrency group holds only one pending run); a re-run of the
    // same tag queues behind the first. `studio-release-mac` is a single
    // runner and takes one job at a time (this, then desktop's macOS .dmg).
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
        // Never a fork: its tag is not this repo's release.
        .cond("${{ github.repository == 'FastTrackStudios/studio' }}")
        .timeout(10)
        // contents: the GitHub Release is created here when the tag has
        // none yet. checks: ios-resolve reads the tagged commit's `Build +
        // test` check.
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
        // The tag's version into the checkout, so ios-resolve checks the
        // version the build will carry (a release commits none).
        .step(stamp("${{ env.TAG }}"))
        .step(
            Step::run(
                "Marketing version = tag version, tag on main or release/v*, green",
                "bash scripts/release.sh ios-resolve studio \"$TAG\"\n",
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
        )
        .step(
            Step::run(
                "Summary",
                r####"{
  echo "### Studio iOS TestFlight: $TAG"
  echo "- marketing version: \`$VERSION\` (the tag's)"
  echo "- commit: \`${SHA:0:12}\` (in main)"
  if [ "$DRY_RUN" = true ]; then
    echo "- **dry run**: stopped before the build and the upload"
  fi
} >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .env("VERSION", "${{ steps.resolve.outputs.version }}")
            .env("SHA", "${{ steps.resolve.outputs.sha }}")
            .env("DRY_RUN", "${{ inputs.dry_run }}"),
        ))
}

fn testflight() -> Job {
    Job::new(RunsOn::Labels(&["self-hosted", "macos", "studio-release-mac"]))
        .name("Build, sign, upload")
        .needs(&["resolve"])
        .cond("${{ !inputs.dry_run }}")
        // Turn a missing runner into a red X within the hour. Without it a
        // job with no available runner sits QUEUED and is eventually
        // cancelled, which reads as "cancelled" rather than "failed" and
        // alerts nobody.
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
        // The commit `resolve` checked, not whatever the ref points at now.
        // clean:false keeps target/ between runs so cargo builds
        // incrementally.
        .step(
            checkout()
                .with("ref", "${{ needs.resolve.outputs.sha }}")
                .with("clean", false),
        )
        // The tag's version into the build (the app's CARGO_PKG_VERSION, its
        // update check); the next checkout overwrites it.
        .step(stamp("${{ env.TAG }}"))
        // The guitar rig's release build refuses to compile without its
        // captures (features/rigs/guitar/build.rs). They are in the private
        // rig-models repo, which airlock keeps one clone of beside its other
        // projects. The clone is fast-forwarded to origin/main first, so a
        // capture pushed to rig-models ships in the next build; a failed
        // fetch fails the job rather than building with stale captures.
        // https, so the fts-forge credential helper authenticates (airlock
        // has no GitHub ssh key). Inline, not a script in the tree: a re-run
        // of an older tag builds that tag's sources, which do not have one.
        .step(Step::run(
            "Rig captures (rig-models) linked into the tree",
            &rig_models("/Volumes/build-disk/development/rig-models", true),
        ))
        // CFBundleVersion has to climb with every upload: the unix time,
        // taken once, here, so the number built is the number recorded.
        .step(
            Step::run(
                "Build number",
                r#"build_no="$(date +%s)"
echo "build_no=$build_no" >> "$GITHUB_OUTPUT"
echo "Studio $TAG: marketing version ${{ needs.resolve.outputs.version }}, build $build_no"
"#,
            )
            .id("stamp"),
        )
        .step(upload())
        .step(
            Step::run(
                "Summary",
                r####"echo "### Studio iOS TestFlight" >> "$GITHUB_STEP_SUMMARY"
echo "\`$TAG\` (\`$(git rev-parse --short HEAD)\`): marketing version \`$VERSION\`, build \`$BUILD_NO\`: $RESULT." >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .cond("always()")
            .env("VERSION", "${{ needs.resolve.outputs.version }}")
            .env("BUILD_NO", "${{ steps.stamp.outputs.build_no }}")
            .env("RESULT", "${{ job.status }}"),
        )
}

/// The env block is the product identity for deploy-testflight.sh.
///
/// - MARKETING_VER: the tag's version, from `resolve`; deploy-testflight.sh
///   refuses to run without one. There is no default: a build is a
///   release's.
/// - The release Xcode xcode-select points at (27, on macOS 27) builds,
///   compiles the icon catalog, signs and uploads: App Store Connect refuses
///   a beta SDK (90534), and actool must match the macOS.
/// - Telemetry: baked at build time via option_env! in architect-telemetry:
///   a shipped app has nobody to set env vars for it, and the exporter is
///   built at process start — before sign-in — so a session token cannot be
///   the credential. Both empty => the app is telemetry-free, which is the
///   local-first default.
/// - DX_PACKAGE / DX_APP_DIR: the app crate (apps/studio/desktop): the front
///   screen, Signal's phone shell and Session's Blitz app, on one window.
/// - DX_FEATURES: the same set as deploy-testflight.sh's default: the guitar
///   and keys rigs plus the embedded drum and e-kit rigs and tone3000 (the
///   iPad drum grid and the Libraries page assume them), and Session
///   (`session-app`). NOT the crate defaults: `tts` pulls ort/ONNX Runtime,
///   which publishes no prebuilt binary for aarch64-apple-ios (ort-sys'
///   build.rs panics); the guide runs on synthesized cues.
/// - FRAME_PLUGINS: frame's faces ride in the bundle; frame is not checked
///   out beside the workspace on the runner, so say where it lives.
/// - DX_BUNDLE_ID must match apps/studio/desktop/Dioxus.toml.
/// - APP_DISPLAY_NAME: the home-screen name; without it the device shows the
///   crate name.
/// - PROFILE_NAME: its own profile, or each product's build revokes the last
///   one's.
fn upload() -> Step {
    Step::run(
        "Build, sign, and upload Studio to TestFlight",
        "\
set -euo pipefail
export PATH=\"/nix/var/nix/profiles/default/bin:$PATH\"
export NIX=/nix/var/nix/profiles/default/bin/nix
bash apps/studio/desktop/ios/deploy-testflight.sh
",
    )
    .env("KEYCHAIN", "fts-build.keychain")
    .env("KEYCHAIN_PW", "fts-build")
    .env("MARKETING_VER", "${{ needs.resolve.outputs.version }}")
    .env("BUILD_NO", "${{ steps.stamp.outputs.build_no }}")
    .env("TASK_OTLP_ENDPOINT", "${{ vars.TASK_OTLP_ENDPOINT }}")
    .env("TASK_OTLP_TOKEN", "${{ secrets.TASK_OTLP_TOKEN }}")
    .env("TASK_SENTRY_DSN", "${{ secrets.TASK_SENTRY_DSN }}")
    .env("DX_PACKAGE", "studio-desktop")
    .env("DX_APP_DIR", "apps/studio/desktop")
    .env(
        "DX_FEATURES",
        "--no-default-features --features signal-guitar,signal-keys-rig,signal-drums-rig,signal-ekit-rig,tone3000,session-app,doc-sync",
    )
    .env("FRAME_PLUGINS", "/Volumes/build-disk/development/frame/examples/plugins")
    .env("DX_BUNDLE_ID", "app.fasttrackstudio.studio")
    .env("APP_DISPLAY_NAME", "Studio")
    .env("PROFILE_NAME", "FTS Studio App Store")
    .env(
        "ICONS_DIR",
        "${{ github.workspace }}/apps/studio/desktop/ios/Assets.xcassets",
    )
}

fn record() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Record on the GitHub Release")
        .needs(&["resolve", "testflight"])
        .timeout(10)
        .permissions(yaml("contents: write\n")?)
        .step(checkout().with("ref", "${{ needs.resolve.outputs.sha }}"))
        // So the Release and the TestFlight build can be traced to each
        // other.
        .step(
            Step::run(
                "Marketing version + build number into the release notes",
                "bash scripts/release.sh ios-record studio \"$TAG\" \"$VERSION\" \"$BUILD_NO\" \"$RUN_URL\"\n",
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
