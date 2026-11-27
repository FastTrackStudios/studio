//! The iOS app on the release tag: Studio for iPhone and iPad built from
//! `studio-vX.Y.Z`, uploaded to TestFlight and handed to its testers. Jobs
//! of the `desktop` workflow (desktop.rs), beside the installers: the same
//! tag, the same `resolve`, the same runner pool.
//!
//! ```text
//!   resolve (desktop.rs: the tag's version stamped in, tag shape, on main
//!   or release/*, `CI passed` green, the Release exists)
//!     --> testflight (airlock: build.sh device, sign for the App Store,
//!         upload; deploy-testflight.sh)
//!     --> testers (airlock: Apple's processing waited for, What to Test,
//!         the internal group has it, the external group gets it through
//!         Beta App Review; testflight-testers.rb)
//!     --> record (hosted: the marketing version and build number written
//!         into the Release notes; release.sh ios-record)
//! ```
//!
//! The marketing version (CFBundleShortVersionString) is the tag's; the
//! build number (CFBundleVersion) the unix time of the build, so every
//! upload's is higher than the last. A `build-only` run builds and signs
//! any commit on main or release/*, and App Store Connect validates the IPA
//! (`UPLOAD=0`): nothing is uploaded and no tester sees it. `validate` stops
//! before any of this.
//!
//! The credentials never leave airlock and none is a GitHub secret: the
//! `studio-release-mac` runner's build keychain (fts-build.keychain) holds
//! the Apple Distribution identity, and its ~/.appstoreconnect the App Store
//! Connect key (upload, the provisioning profile, the testers), as for the
//! notarized .dmg. Only `record`, on a hosted runner, writes to the Release.

use crate::desktop::{NOT_VALIDATE, STAMPED};
use crate::model::{Job, RunsOn, Step, UBUNTU, yaml};
use crate::steps::{checkout, rig_models, stamp};
use anyhow::Result;

/// The App Store record (apps/studio/Dioxus.toml's bundle identifier).
const BUNDLE_ID: &str = "app.fasttrackstudio.studio";

/// A release run, not `validate` or `build-only`: what reaches testers.
const SHIPS: &str = "inputs.mode != 'validate' && inputs.mode != 'build-only'";

pub fn jobs() -> Result<Vec<(&'static str, Job)>> {
    Ok(vec![
        ("testflight", testflight()),
        ("testers", testers()),
        ("record", record()?),
    ])
}

const fn airlock() -> RunsOn {
    RunsOn::Labels(&["self-hosted", "macos", "studio-release-mac"])
}

/// Build, sign and upload (or, build-only, validate). The engine is linked
/// into the app (studio#703), so this is a release build of all of it: a
/// cold one is long, hence the limit.
fn testflight() -> Job {
    Job::new(airlock())
        .name("iOS to TestFlight")
        .needs(&["resolve"])
        .cond(NOT_VALIDATE)
        .timeout(180)
        .output("build_no", "${{ steps.build_no.outputs.build_no }}")
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
        // The commit `resolve` checked; clean:false keeps target/ warm.
        .step(
            checkout()
                .with("ref", "${{ needs.resolve.outputs.sha }}")
                .with("clean", false),
        )
        .step(stamp("${{ env.TAG }}").cond(STAMPED))
        // The guitar rig's release build refuses to compile without its
        // captures, as for the .dmg.
        .step(Step::run(
            "Rig captures (rig-models) linked into the tree",
            &rig_models("/Volumes/build-disk/development/rig-models", true),
        ))
        // Taken once, here: the number built is the number recorded.
        .step(
            Step::run(
                "Build number",
                r#"build_no="$(date +%s)"
echo "build_no=$build_no" >> "$GITHUB_OUTPUT"
echo "Studio $TAG: marketing version $VERSION, build $build_no"
"#,
            )
            .id("build_no")
            .env("VERSION", "${{ needs.resolve.outputs.version }}"),
        )
        .step(Step::uses("devenv CLI", "./.github/actions/devenv"))
        .step(
            Step::run(
                "Build, sign for the App Store, upload",
                r#"set -euo pipefail
export PATH="/nix/var/nix/profiles/default/bin:$PATH"
if [ "$MODE" = build-only ]; then export UPLOAD=0; fi
bash apps/studio/ios/deploy-testflight.sh
"#,
            )
            .env("KEYCHAIN", "fts-build.keychain")
            .env("KEYCHAIN_PW", "fts-build")
            .env("MARKETING_VER", "${{ needs.resolve.outputs.version }}")
            .env("BUILD_NO", "${{ steps.build_no.outputs.build_no }}")
            .envs(crate::desktop::SHIPPED_COLLECTOR),
        )
        .step(
            Step::run(
                "Summary",
                r####"{
  echo "### iOS TestFlight: $TAG"
  if [ "$MODE" = build-only ]; then what="signed and validated, not uploaded"; else what="uploaded"; fi
  echo "\`$BUNDLE\` marketing version \`$VERSION\`, build \`$BUILD_NO\`: $what, $RESULT."
} >> "$GITHUB_STEP_SUMMARY"
"####,
            )
            .cond("always()")
            .env("BUNDLE", BUNDLE_ID)
            .env("VERSION", "${{ needs.resolve.outputs.version }}")
            .env("BUILD_NO", "${{ steps.build_no.outputs.build_no }}")
            .env("RESULT", "${{ job.status }}"),
        )
}

/// The build to the testers once Apple has processed it. Its own job, so
/// it can be re-run alone (after the owner fills in the Beta App Review
/// contact, say) without building again.
fn testers() -> Job {
    Job::new(airlock())
        .name("TestFlight testers")
        .needs(&["resolve", "testflight"])
        .cond(&format!(
            "${{{{ {SHIPS} && needs.testflight.result == 'success' }}}}"
        ))
        // Processing is 5-15 minutes, seldom an hour.
        .timeout(75)
        .step(
            checkout()
                .with("ref", "${{ needs.resolve.outputs.sha }}")
                .with("clean", false)
                .with("persist-credentials", false),
        )
        .step(
            Step::run(
                "Processed, then to the internal and external testers",
                r#"set -euo pipefail
set -a
# shellcheck disable=SC1091
source "$HOME/.appstoreconnect/config.env"
set +a
ruby apps/studio/ios/testflight-testers.rb "$BUNDLE" "$BUILD_NO" "$VERSION" \
  "$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/releases/tag/$TAG"
"#,
            )
            .env("BUNDLE", BUNDLE_ID)
            .env("VERSION", "${{ needs.resolve.outputs.version }}")
            .env("BUILD_NO", "${{ needs.testflight.outputs.build_no }}")
            .env("WAIT_MIN", "60"),
        )
}

/// So the Release and the TestFlight build name each other: the build
/// number on the Release, as soon as the upload is done.
fn record() -> Result<Job> {
    Ok(Job::new(UBUNTU)
        .name("Record the TestFlight build on the Release")
        .needs(&["resolve", "testflight"])
        .cond(&format!("${{{{ {SHIPS} && needs.testflight.result == 'success' }}}}"))
        .timeout(10)
        .permissions(yaml("contents: write\n")?)
        .step(checkout().with("ref", "${{ needs.resolve.outputs.sha }}"))
        .step(
            Step::run(
                "Marketing version and build number into the release notes",
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

#[cfg(test)]
mod tests {
    use anyhow::{Context, Result, ensure};
    use serde_yaml::Value;

    fn job(name: &str) -> Result<Value> {
        let rendered = serde_yaml::to_value(crate::desktop::workflow()?)?;
        rendered
            .get("jobs")
            .and_then(|j| j.get(name))
            .cloned()
            .with_context(|| format!("desktop has no job {name}"))
    }

    fn cond(job: &Value) -> &str {
        job.get("if").and_then(Value::as_str).unwrap_or_default()
    }

    /// Only a release reaches testers or writes to the Release: a
    /// `build-only` or `validate` run never does.
    #[test]
    fn only_a_release_reaches_testers_or_the_release() -> Result<()> {
        for name in ["testers", "record"] {
            let job = job(name)?;
            let cond = cond(&job);
            ensure!(
                cond.contains("inputs.mode != 'validate'")
                    && cond.contains("inputs.mode != 'build-only'"),
                "{name} runs on a validate or build-only run: {cond}"
            );
            ensure!(
                cond.contains("needs.testflight.result == 'success'"),
                "{name}: {cond}"
            );
        }
        Ok(())
    }

    /// A build-only run builds and signs, and App Store Connect only
    /// validates: the upload step turns the upload off for it.
    #[test]
    fn build_only_validates_and_uploads_nothing() -> Result<()> {
        let job = job("testflight")?;
        ensure!(
            cond(&job) == "${{ inputs.mode != 'validate' }}",
            "testflight runs on: {}",
            cond(&job)
        );
        let steps = job
            .get("steps")
            .and_then(Value::as_sequence)
            .context("steps")?;
        let upload = steps
            .iter()
            .find(|s| {
                s.get("run")
                    .and_then(Value::as_str)
                    .is_some_and(|r| r.contains("deploy-testflight.sh"))
            })
            .context("no step runs deploy-testflight.sh")?;
        let run = upload
            .get("run")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ensure!(
            run.contains(r#"if [ "$MODE" = build-only ]; then export UPLOAD=0; fi"#),
            "build-only would upload: {run}"
        );
        Ok(())
    }
}
