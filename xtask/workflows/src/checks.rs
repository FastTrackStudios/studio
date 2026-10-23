//! `checks` — the cheap gates on every PR, the full set after every merge.
//!
//! THE MODEL (Task's, 2026-10): the agents and people pushing PRs already
//! build and test before they push, so CI does not do it again on a billed
//! runner. The pre-push hook (`just install-hooks`) runs scripts/ci-local.sh
//! — fmt, change files, then the tests, the workspace check and the wasm
//! check for what `cargo rail plan` says the change affects, on the
//! machine's warm target dir — and posts a `local-ci` commit status on the
//! pushed commits. The `main` ruleset requires `local-ci` and `Build + test`.
//!
//! ```text
//!   PR -> main          the GATES only (free runner): lockfile, fmt, the
//!                       generated workflows + actionlint, change files, rail
//!                       config, the flow gate. Nothing compiles beyond the
//!                       workflows crate.
//!   PR -> release/**    a hotfix onto a release line (just release-hotfix):
//!                       the FULL set.
//!   RELEASE PR release/pending -> main (or -> release/v*)   the plan runs
//!                       the release guard from the BASE commit's
//!                       scripts/release.sh: a sound release PR skips every
//!                       job (`Build + test` still runs and passes); one the
//!                       guard refuses fails `Build + test`.
//!   push to main        the FULL set, once per merge: what catches two PRs
//!   (and release/**)    that pass alone and break together. main may go red
//!                       — releases are cut by hand, from a green commit
//!                       (`just release-tag` requires `Build + test`).
//!   by hand             `gh workflow run checks.yml --ref <branch>` runs the
//!                       full set; on main, `-f seed=true` also rebuilds the
//!                       tests job's snapshot from clean.
//! ```
//!
//! RUNNERS: only the big builds are paid for.
//!
//! ```text
//!   ubuntu-latest  plan, gates, wasm and the `Build + test` aggregator
//!                  (free: the repo is public)
//!   warp 8x        check: `cargo check --workspace` + advisory clippy, with
//!                  rust-cache on WarpBuild's cache
//!   warp 16x       tests: ~1460 crates and ~800 test binaries at full
//!                  codegen, on a SNAPSHOT runner whose saved VM holds the
//!                  warm target dir
//! ```
//!
//! No tailwind step: every sheet a crate `include_str!`s is committed, and
//! session-web's build.rs writes a placeholder for its generated one (dx
//! builds the real sheet). No Boundaries step: `just ci-boundaries` is a
//! manual check.
//!
//! The jobs run WITHOUT nix (see [`crate::steps`]): the pinned rustc from
//! dtolnay/rust-toolchain, the native headers from apt (cached), the pinned
//! tools as release binaries. Debuginfo is line-tables-only
//! (.cargo/ci-config.toml) and incremental is off.

use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{
    Cache, Toolchain, Tools, checkout, checkout_full, faces, footprint, hosted_setup, rust_cache,
    target_mark, target_sweep,
};
use anyhow::Result;

/// The features `just test` / `just ci` name: the app CLIs and the worklet
/// rigs sit behind features, and without them their tests never build (an
/// app crate has no lib to dev-depend on).
const TEST_FEATURES: &str =
    "session-desktop/cli,studio-desktop/cli,web-audio-worklet/guitar,web-audio-worklet/keys";

/// The workspace check's WarpBuild size. One constant, so resizing is a
/// one-line change (then `just workflows`).
const WARP_CHECK: RunsOn = RunsOn::Label("warp-ubuntu-latest-x64-8x");

pub fn workflow() -> Result<Workflow> {
    Ok(Workflow::new(
        "checks",
        yaml(
            r#"
pull_request:
  branches: [main, "release/**"]
push:
  branches: [main, "release/**"]
workflow_dispatch:
  inputs:
    seed:
      description: "On main: rebuild the tests job's WarpBuild snapshot from clean and save it."
      type: boolean
      required: false
      default: false
"#,
        )?,
    )
    // One group per PR, so a new push to a PR cancels its predecessor's run.
    // A PUSH gets a group of its own commit (the Zed way): every merge is
    // checked to the end, never cancelled by the next merge — its green
    // `Build + test` is what `just release-tag` requires.
    .concurrency(
        "checks-${{ github.event.pull_request.number || github.ref }}-${{ \
         github.event_name == 'push' && github.sha || 'run' }}",
        true,
    )
    .job("plan", plan()?)
    .job("gates", gates())
    .job("check", check())
    .job("wasm", wasm())
    .job("tests", tests())
    .job("build-and-test", build_and_test()))
}

/// Which jobs have work: a sound release PR none (a refused one fails
/// here); a PR into main the gates; everything else the full set. Plus
/// whether main's tests run should save the snapshot ([`DEPS_PATHS`]).
/// Bash and git only: no toolchain.
fn plan() -> Result<Job> {
    let mut job = Job::new(UBUNTU)
        .name("Plan")
        .timeout(10)
        .permissions(yaml("contents: read\n")?);
    for out in ["skip", "full", "deps"] {
        job = job.output(out, &format!("${{{{ steps.plan.outputs.{out} }}}}"));
    }
    Ok(job
        // Full history: the release guard reads the PR's base and head, and
        // `deps` diffs a push against the commit before it.
        .step(checkout_full(None))
        .step(
            Step::run("Plan", PLAN)
                .id("plan")
                .env("BASE_SHA", "${{ github.event.pull_request.base.sha }}")
                .env("BASE_REF", "${{ github.event.pull_request.base.ref }}")
                .env("HEAD_SHA", "${{ github.event.pull_request.head.sha }}")
                .env("HEAD_REF", "${{ github.event.pull_request.head.ref }}")
                .env(
                    "HEAD_REPO",
                    "${{ github.event.pull_request.head.repo.full_name }}",
                )
                .env("BEFORE", "${{ github.event.before }}")
                .env("DEPS_PATHS", DEPS_PATHS),
        ))
}

/// What a tests snapshot is built from besides the workspace's own code: the
/// lockfile, the cargo config, the toolchain pins (steps.rs's `RUST`, the
/// flake's). A push to main changing one of these saves a new snapshot; the
/// workspace's own crates rebuild incrementally from an older one.
const DEPS_PATHS: &str =
    "Cargo.lock .cargo xtask/workflows/src/steps.rs nix/modules/toolchain.nix rust-toolchain.toml";

/// A release PR (release/pending) runs the release guard from the BASE
/// commit's copy of the script (never the PR's, which could exempt itself).
/// 0 = sound: version + changelog only, its content was checked when it
/// merged, skip everything. 3 = refused: fail, which fails `Build + test`.
/// 1 = not a release PR.
///
/// A fork PR into a release line would run the full set on the org's paid
/// runners with whatever its code does: refused. Into main it runs the free
/// gates like any PR.
const PLAN: &str = r####"set -u
out() { printf '%s\n' "$@" >> "$GITHUB_OUTPUT"; }
full=true
case "${GITHUB_EVENT_NAME:-}" in
  pull_request)
    rc=1
    if git show "${BASE_SHA}:scripts/release.sh" > "${RUNNER_TEMP}/release-base.sh" 2>/dev/null; then
      bash "${RUNNER_TEMP}/release-base.sh" release-only || rc=$?
      [ "$rc" = 0 ] || [ "$rc" = 1 ] || rc=3
    fi
    case "$rc" in
      0)
        { echo "### Release commit only"
          echo "This PR is one commit changing the version, CHANGELOG.md and consumed change files; its content was checked when it merged, so every job is skipped."
        } >> "$GITHUB_STEP_SUMMARY"
        echo "plan: release commit only (${HEAD_REF} -> ${BASE_REF}): nothing to run"
        out skip=true full=false deps=false
        exit 0 ;;
      3)
        echo "::error::${HEAD_REF} -> ${BASE_REF} is a release PR the release guard refuses (see above)"
        exit 1 ;;
    esac
    case "${BASE_REF}" in
      release/*)
        if [ "${HEAD_REPO}" != "${GITHUB_REPOSITORY}" ]; then
          echo "::error::fork PR into ${BASE_REF}: the full set runs on the org's paid runners; re-push it from a branch of this repo"
          exit 1
        fi
        echo "plan: PR into ${BASE_REF}: a hotfix line runs the full set" ;;
      *)
        full=false
        echo "plan: PR into ${BASE_REF}: the gates (the tests ran locally: the local-ci status)" ;;
    esac ;;
  *) echo "plan: ${GITHUB_EVENT_NAME:-?} on ${GITHUB_REF_NAME}: the full set" ;;
esac
deps=false
if [ "${GITHUB_EVENT_NAME:-}" = push ] && [ "${GITHUB_REF_NAME}" = main ]; then
  # shellcheck disable=SC2086 # DEPS_PATHS is a list of pathspecs
  if ! git cat-file -e "${BEFORE}^{commit}" 2>/dev/null \
    || ! git diff --quiet "${BEFORE}" "${GITHUB_SHA}" -- ${DEPS_PATHS}; then
    deps=true
    echo "plan: dependencies or toolchain changed: the tests job saves a new snapshot"
  fi
fi
out skip=false "full=${full}" "deps=${deps}"
"####;

/// The env every hosted cargo job sets. Incremental is off: a CI build is
/// never rebuilt in place, and incremental dirs would only fatten the cache.
fn hosted_cargo_job(job: Job) -> Job {
    job.env("CARGO_INCREMENTAL", "0")
}

/// From here on a step runs even when an earlier gate failed, so one run
/// reports fmt AND the workflows AND the flow gate instead of one at a time;
/// the job still fails.
const NOT_CANCELLED: &str = "${{ !cancelled() }}";

/// The jobs only a full run has.
const FULL: &str = "${{ needs.plan.outputs.full == 'true' }}";

/// REQUIRED (through `Build + test`) on every run: the checks no machine's
/// local run can vouch for or that cost nothing — manifests + lockfile, fmt,
/// the generated workflows, change files, the rail config, the flow gate.
/// Compiles only the workflows crate.
fn gates() -> Job {
    hosted_cargo_job(
        Job::new(UBUNTU)
            .name("Gates (fmt, manifests, workflows, change files, flows)")
            .needs(&["plan"])
            .cond("${{ needs.plan.outputs.skip != 'true' }}")
            .timeout(30),
    )
    .step(checkout())
    .steps(hosted_setup(
        "rustfmt",
        "",
        Tools {
            mold: true,
            just: true,
            cargo_rail: true,
            tracey: true,
            actionlint: true,
            ci_cargo_config: true,
            ..Tools::default()
        },
    ))
    .steps(rust_cache("gates", Cache::Github))
    .step(Step::run("Manifests + lockfile", LOCKFILE))
    .step(Step::run("cargo fmt --check", "cargo fmt --all --check").cond(NOT_CANCELLED))
    .steps(workflow_gates())
    .step(change_check())
    .step(Step::run("rail config", "cargo rail config validate --strict").cond(NOT_CANCELLED))
    // The workflow-spec verification standard (#48): every `flow.*` rule
    // with an r[impl] has an r[verify], or the build is red. Same recipe as
    // local (`just daw-flows`); tracey is the pinned release binary.
    // `--own-daemon`: the script runs its own tracey daemon under a short
    // state dir — the runner's HOME pushes tracey's default socket path past
    // the 108-byte Unix limit — and stops it after.
    .step(Step::run("Flow verification gate", "just daw-flows --own-daemon").cond(NOT_CANCELLED))
}

/// REQUIRED on full runs: `cargo check --workspace`. Clippy reports here
/// too, advisory: the lint debt is being paid down crate by crate, so it
/// reports (grouped by lint and crate, .github/scripts/clippy-report.py)
/// without failing. Same check-mode dependency metadata, so same job.
fn check() -> Job {
    hosted_cargo_job(
        Job::new(WARP_CHECK)
            .name("check (cargo check + clippy)")
            .needs(&["plan"])
            .cond(FULL)
            .timeout(90),
    )
    .step(checkout())
    .steps(hosted_setup(
        "clippy",
        "",
        Tools {
            apt: true,
            ci_cargo_config: true,
            free_disk: true,
            ..Tools::default()
        },
    ))
    // The "lint" key: the entries the old lint job saved still match.
    .steps(rust_cache("lint", Cache::Warp))
    .step(Step::run(
        "cargo check --workspace",
        "cargo check --workspace",
    ))
    .step(
        Step::run(
            "cargo clippy (advisory, does not block)",
            "\
cargo clippy --workspace --no-deps --all-targets --keep-going \\
  --message-format=json -- --cap-lints warn \\
  | python3 .github/scripts/clippy-report.py
",
        )
        .cond(NOT_CANCELLED)
        .continue_on_error(),
    )
    .step(footprint())
}

/// Not `--locked`: cargo writes the lockfile's [[patch.unused]] trailer in
/// HashMap order — one group per patched source URL — so the same inputs
/// serialise differently from one run to the next and `--locked` fails at
/// random (observed locally: four consecutive `cargo metadata` runs, three
/// orderings). Instead: resolve, then diff the committed lockfile against
/// the written one with the trailer compared as a sorted set.
const LOCKFILE: &str = r#"set -euo pipefail
committed="$RUNNER_TEMP/Cargo.lock.committed"
cp Cargo.lock "$committed"
cargo metadata --format-version 1 > /dev/null
packages() { sed '/^\[\[patch\.unused\]\]/,$d' "$1"; }
unused() {
  awk '/^\[\[patch\.unused\]\]/ { f = 1; next }
       f && /^name = /    { n = $3 }
       f && /^version = / { print n "@" $3 }' "$1" | sort
}
ok=1
diff -u <(packages "$committed") <(packages Cargo.lock) || ok=0
diff -u <(unused "$committed") <(unused Cargo.lock) || ok=0
if [ "$ok" = 0 ]; then
  echo "::error::Cargo.lock is out of date — run cargo metadata and commit the result"
  exit 1
fi
"#;

/// The workflows are generated (this crate): regenerating must change
/// nothing, so a hand edit to .github/workflows or a source change without
/// `just workflows` is red. Then actionlint over every workflow (it runs the
/// image's shellcheck over each `run:` script too). Two steps rather than
/// `just workflows-check` (which runs both), so each reports on its own.
/// The workflows check is the one gate that compiles (the xtask-workflows
/// crate, from the rust cache).
fn workflow_gates() -> [Step; 2] {
    [
        Step::run(
            "Workflows are generated (xtask-workflows check)",
            "cargo run -q -p xtask-workflows -- check",
        )
        .cond(NOT_CANCELLED),
        Step::run("actionlint", "actionlint -color").cond(NOT_CANCELLED),
    ]
}

/// Release intent (fleet flow v4): every crate a PR changes needs a change
/// file in .changes/ (`cargo rail change add <crates> --bump
/// <none|patch|minor|major> -m "<note>"`); the release builds the changelog
/// from them. PRs only: a push has no "change" to describe, and a release
/// PR never reaches this job. `--since <the PR's base
/// commit>` rather than `--merge-base`: the checkout is shallow (no
/// default-branch ref to find a merge-base with), and for a PR into a
/// release/v* line the merge-base with main would count the line's
/// earlier, already-consumed hotfixes.
fn change_check() -> Step {
    Step::run(
        "cargo rail change check",
        "\
git fetch --quiet --no-tags --depth=1 origin \"${BASE_SHA}\"
cargo rail change check --since \"${BASE_SHA}\"
",
    )
    .cond("${{ github.event_name == 'pull_request' }}")
    .env("BASE_SHA", "${{ github.event.pull_request.base.sha }}")
}

/// The browser builds, which the workspace check (native) never compiles.
/// REQUIRED part: the web bundles. ADVISORY part: the Signal browser remote
/// (red since STA-184: it pulls tokio/mio into wasm), which reports but
/// cannot fail the job.
fn wasm() -> Job {
    hosted_cargo_job(
        Job::new(UBUNTU)
            .name("wasm checks")
            .needs(&["plan"])
            .cond(FULL)
            .timeout(75),
    )
    // The wasm32 C builds (ring): the nix shell pointed these at an
    // unwrapped clang-18 and llvm-ar; on the image they are apt's clang and
    // llvm (steps::APT_PACKAGES).
    .env("CC_wasm32_unknown_unknown", "clang")
    .env("AR_wasm32_unknown_unknown", "llvm-ar")
    .step(checkout())
    .steps(hosted_setup(
        "",
        "wasm32-unknown-unknown",
        Tools {
            apt: true,
            just: true,
            ci_cargo_config: true,
            free_disk: true,
            ..Tools::default()
        },
    ))
    .steps(rust_cache("wasm", Cache::Github))
    // The three web bundles deploy.yml ships (STA-366). The workspace check
    // is native, so it never compiles them: a native-only dep (tokio `net`,
    // an engine, an X11/HID crate) leaking into their graph stayed hidden
    // until the deploy itself failed. Same targets and features as
    // nix/modules/packages/web-bundles.nix builds (`just web-wasm-check`).
    .step(Step::run("wasm check (web bundles)", "just web-wasm-check"))
    // The Signal browser remote: studio-desktop's web build. Advisory until
    // STA-184 makes it green; then drop continue-on-error and it is
    // required like the bundles.
    .step(
        Step::run(
            "wasm check (Signal browser remote, advisory)",
            "\
cargo check -p studio-desktop \\
  --target wasm32-unknown-unknown --no-default-features --features signal
",
        )
        .id("signal")
        .cond(NOT_CANCELLED)
        .continue_on_error(),
    )
    .step(
        Step::run(
            "Advisory result",
            "\
echo \"::warning::wasm check (Signal browser remote) failed — advisory (STA-184), does not block\"
echo \"### Signal browser remote wasm check failed (advisory, STA-184)\" >> \"$GITHUB_STEP_SUMMARY\"
",
        )
        .cond("${{ !cancelled() && steps.signal.outcome == 'failure' }}"),
    )
    .step(footprint())
}

/// REQUIRED on full runs: the whole suite, on a WarpBuild SNAPSHOT runner,
/// without nix: the hosted jobs' rustup/apt/release-binary setup plus
/// nextest and lavapipe (software Vulkan) for the GPU tests.
///
/// The warm target dir is the snapshot (`<label>;snapshot.key=<alias>`,
/// WarpBuilds/snapshot-save): the VM boots from the last snapshot saved under
/// [`WARPBUILD_SNAPSHOT`], or the base image if there is none. Only main
/// saves it (a PR never runs this job, and a release line never saves), and
/// only when it is worth the ~13 min of 16x a save costs ([`SAVE_SNAPSHOT`]):
/// the dependencies or toolchain changed, the snapshot is [`STALE_DAYS`] old
/// (or missing: WarpBuild expires one after 15 days unused), or a dispatch
/// asked for a clean one. Before a save the target is pruned to what that
/// build used. First run (2026-10-23): cold build 15 min, 10 949 tests
/// 2 min, on a 16x.
fn tests() -> Job {
    let warp = Toolchain::Hosted;
    Job::new(RunsOn::Owned(format!(
        "${{{{ {SEED} && '{WARPBUILD_RUNNER};snapshot.enabled=true' || \
         '{WARPBUILD_RUNNER};snapshot.key={WARPBUILD_SNAPSHOT}' }}}}"
    )))
    .name("tests")
    .needs(&["plan"])
    .cond(FULL)
    .timeout(120)
    .env("TEST_FEATURES", TEST_FEATURES)
    // No incremental caches: they grow without bound in a persistent target
    // (372 GB on one dev machine) and a CI build seldom rebuilds the same
    // crate twice. Dioxus and Zed run CI the same way.
    .env("CARGO_INCREMENTAL", "0")
    // No token left in .git/config: checkout only removes it in its post
    // step, after the snapshot has been taken.
    .step(checkout().with("persist-credentials", false))
    .steps(hosted_setup(
        "",
        "",
        Tools {
            apt: true,
            gpu: true,
            just: true,
            nextest: true,
            ci_cargo_config: true,
            ..Tools::default()
        },
    ))
    .environment(ASSETS_ENVIRONMENT.into())
    // ~/.cache/cargo-target: outside the checkout, so the pre-snapshot
    // `git clean` keeps it.
    .step(Step::run("Persistent target dir", &target_dir_script()).id("target"))
    .step(target_mark())
    .step(assets_token())
    .step(assets(warp))
    .step(faces(warp))
    .step(
        Step::run(
            "cargo nextest",
            &format!("bash -uo pipefail -s <<'EOF'\n{NEXTEST}EOF\n"),
        )
        .id("nextest")
        .env("TEST_LOG", "${{ runner.temp }}/nextest-ci.log"),
    )
    .step(target_sweep())
    .step(test_log("nextest-ci"))
    // Before the snapshot: nothing secret may be in it (WarpBuild's own
    // advice; a snapshot alias is readable by any run in the org). The
    // asset clones STAY: they are only as private as the org that owns
    // both. So do the checkout's ignored files — the asset links, the plugin
    // faces: build scripts watch some of them, and recreating them each boot
    // rebuilt mic-plugin and everything above (13 min of a warm build;
    // `just ci-churn` checks it). The target dir, ~/.cargo, rustup and the
    // apt packages stay: that is the point. The stamp dates the snapshot.
    .step(
        Step::run(
            "Clean up before the snapshot",
            "\
rm -rf \"$HOME/.ssh\" \"$HOME/.aws\"
git config --local --unset-all http.https://github.com/.extraheader || true
date +%s > \"$HOME/.cache/snapshot-stamp\"
du -sh \"$HOME/.cache/cargo-target\" 2>/dev/null || true
",
        )
        .cond(SAVE_SNAPSHOT),
    )
    .step(
        Step::uses("Save the WarpBuild snapshot", "WarpBuilds/snapshot-save@v1")
            .cond(SAVE_SNAPSHOT)
            .with("alias", WARPBUILD_SNAPSHOT)
            // A failed save on a run that set out to save is a failure.
            .with("fail-on-error", true)
            .with("wait-timeout-minutes", 60),
    )
}

/// The full nextest log as an artifact (see [`NEXTEST`] for why it is a file).
fn test_log(artifact: &str) -> Step {
    Step::uses("Test log", "actions/upload-artifact@v4")
        .cond("${{ always() }}")
        .with("name", artifact)
        .with("path", "${{ runner.temp }}/nextest-ci.log")
        .with("if-no-files-found", "ignore")
        .with("retention-days", 7)
}

/// The WarpBuild runner the tests job uses. One constant, so trying another
/// size is a one-line change (then `just workflows`).
const WARPBUILD_RUNNER: &str = "warp-ubuntu-latest-x64-16x";

/// The WarpBuild snapshot alias: the tests job boots from the VM snapshot
/// saved under it (toolchain, apt packages, ~/.cargo, the asset clones and
/// the warm target dir in ~/.cache/cargo-target).
const WARPBUILD_SNAPSHOT: &str = "studio-tests";

/// A dispatch on main asking for a clean snapshot: boots the base image and
/// saves.
const SEED: &str =
    "(github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && inputs.seed)";

/// How old a snapshot may get before main's next run saves a fresh one. The
/// workspace's own crates rebuild from an old one; this bounds how many.
const STALE_DAYS: u32 = 3;

/// See [`tests`]. Main's own runs only (a push or a dispatch), never a
/// release line's.
const SAVE_SNAPSHOT: &str = "${{ !cancelled() && github.ref == 'refs/heads/main' && \
     github.event_name != 'pull_request' && (needs.plan.outputs.deps == 'true' || \
     steps.target.outputs.stale == 'true' || \
     (github.event_name == 'workflow_dispatch' && inputs.seed)) }}";

/// Cargo never collects garbage: every lockfile and feature set leaves its
/// own units here until the disk is full and rustc dies mid-build (PR #184,
/// October 2026). [`target_sweep`] is the garbage collection; this is the
/// backstop: the incremental caches always go (the jobs build without them),
/// and below 30 GB free the whole dir does (100 GB wiped the warm snapshot
/// on every boot: a 16x boots with 186 GB free and the target is 87 GB).
///
/// `stale`: the snapshot this VM booted from is missing, cleared or older
/// than [`STALE_DAYS`] (the stamp is written just before a save).
fn target_dir_script() -> String {
    format!(
        r#"dir="${{CI_CACHE_DIR:-${{CACHE_DIRECTORY:-$HOME/.cache}}}}/cargo-target"
mkdir -p "$dir"
echo "CARGO_TARGET_DIR=$dir" >> "$GITHUB_ENV"
free_gb() {{ df -Pk "$dir" | awk 'NR==2 {{ print int($4 / 1048576) }}'; }}
rm -rf "$dir"/*/incremental "$dir"/*/*/incremental
if [ "$(free_gb)" -lt 30 ]; then
  echo "::warning::still $(free_gb) GB free: clearing the target dir (a cold build follows)"
  rm -rf "$dir" && mkdir -p "$dir"
fi
stamp="$(cat "$HOME/.cache/snapshot-stamp" 2>/dev/null || echo 0)"
age=$(( ($(date +%s) - stamp) / 86400 ))
if [ ! -d "$dir/debug" ] || [ "$age" -ge {STALE_DAYS} ]; then
  echo "stale=true" >> "$GITHUB_OUTPUT"
  echo "snapshot: stale (target $( [ -d "$dir/debug" ] && echo warm || echo cold ), ${{age}} days old): main saves a fresh one"
fi
echo "target dir: $dir ($(du -sh "$dir" 2>/dev/null | cut -f1)), $(free_gb) GB free"
"#
    )
}
/// The large fixtures and the rigs' NAM captures / IRs live in the private
/// repos studio-assets, rig-models and mic-models (assets.txt); `just assets` clones them
/// into .assets/ and links each file into place, so the tests that need them
/// run instead of skipping.
///
/// WHO MAY FETCH: only main's runs (a push to main, a dispatch on main). They
/// run in the `ci-assets` environment, which only `main` may deploy from and
/// which holds the fts-forge App's id and key; `assets_token` mints from it
/// a token that can only READ the two asset repos, for this one job. A PR
/// never gets the environment, so neither the key nor a token is ever in a
/// job that runs a PR's code (this repo is public, and the PR's own Justfile
/// is what `just assets` runs). A PR run links the clones its runner already
/// holds (`ASSETS_OFFLINE=1`: no fetch); an asset added in a PR reaches CI
/// once main has merged it and a main run has fetched it.
///
/// The token reaches git as a `url.<token-url>.insteadOf` rewrite passed in
/// `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_0`/`GIT_CONFIG_VALUE_0`, so it is
/// scoped to this step's processes: nothing is written to the runner's
/// ~/.gitconfig or to the clones' .git/config.
///
/// actions/checkout's clean wipes the ignored .assets/ every run, so
/// .assets is a symlink into the runner instance's cache dir: the clean
/// removes the link, not the clones.
fn assets(tc: Toolchain) -> Step {
    Step::run(
        "Assets (studio-assets, rig-models, mic-models)",
        &format!(
            r#"store="${{CI_CACHE_DIR:-${{CACHE_DIRECTORY:-$HOME/.cache}}}}/assets"
mkdir -p "$store"
[ -L .assets ] || [ -e .assets ] || ln -s "$store" .assets
if [ -n "${{ASSETS_TOKEN}}" ]; then
  export GIT_CONFIG_COUNT=1
  export GIT_CONFIG_KEY_0="url.https://x-access-token:${{ASSETS_TOKEN}}@github.com/.insteadOf"
  export GIT_CONFIG_VALUE_0="https://github.com/"
  {fetch}
elif [ -d "$store/studio-assets/.git" ] && [ -d "$store/rig-models/.git" ] && [ -d "$store/mic-models/.git" ]; then
  echo "no asset credential on this run (only main's full runs fetch): linking the clones this runner holds"
  ASSETS_OFFLINE=1 {fetch}
else
  echo "::warning::no asset clones on this runner yet and no credential to fetch them (a main run fetches them): tests that need assets will fail"
fi
"#,
            fetch = tc.cmd("just assets")
        ),
    )
    .env("ASSETS_TOKEN", "${{ steps.assets-token.outputs.token }}")
}

/// Main's full runs only: the job is in the `ci-assets` environment, whose
/// deployment branch policy allows `main` alone. Empty (no environment) on
/// every other run, PRs above all. See `assets`.
const ASSETS_ENVIRONMENT: &str = "${{ github.ref == 'refs/heads/main' && github.event_name != 'pull_request' && 'ci-assets' || '' }}";

/// A token from the fts-forge App that can read the contents of the three
/// asset repos and nothing else, valid for an hour. Main's full runs only
/// (see `assets`); skipped everywhere else, which leaves the token empty.
fn assets_token() -> Step {
    Step::uses(
        "Asset repo token (main runs only)",
        "actions/create-github-app-token@v2",
    )
    .id("assets-token")
    .cond("${{ github.ref == 'refs/heads/main' && github.event_name != 'pull_request' }}")
    .with("app-id", "${{ vars.FORGE_APP_ID }}")
    .with("private-key", "${{ secrets.FORGE_APP_PRIVATE_KEY }}")
    .with("owner", "FastTrackStudios")
    .with("repositories", "studio-assets,rig-models,mic-models")
    .with("permission-contents", "read")
}

/// The whole workspace, in two timed phases (`nextest run --no-run`, then
/// the run, which finds every unit fresh); the times go to the step summary
/// so runner sizes can be compared.
///
/// nextest's report goes to a FILE, then a digest to the step's log. On the
/// Mac Pro VM runners Task's nextest died with exit 110, "error writing to
/// output", at the first failing test's captured output: writing to the
/// runner's stdout pipe failed. A regular file cannot do that. The full log
/// is the job's test-log artifact.
const NEXTEST: &str = r####"t0=$(date +%s)
cargo nextest run --workspace --features "${TEST_FEATURES}" --no-run >"${TEST_LOG}" 2>&1
rc=$?
t1=$(date +%s)
# A build that succeeded read every unit the workspace uses: the target dir
# may be pruned to that set (target_sweep), whatever the tests say.
if [ "$rc" = 0 ]; then
  echo "prunable=true" >> "${GITHUB_OUTPUT}"
  cargo nextest run --workspace --no-fail-fast --features "${TEST_FEATURES}" >>"${TEST_LOG}" 2>&1
  rc=$?
fi
t2=$(date +%s)
{ echo "### tests: timing"
  echo "| phase | seconds |"
  echo "|---|---|"
  echo "| build (nextest --no-run) | $((t1 - t0)) |"
  echo "| tests | $((t2 - t1)) |"
  echo
  echo "exit ${rc}"; } >> "${GITHUB_STEP_SUMMARY}"
{ grep -E '^ *(FAIL|TRY [0-9]+ (FAIL|TMT)|SIGSEGV|SIGABRT|LEAK|TIMEOUT)|Summary \[|tests were not run|^error' "${TEST_LOG}" | head -n 200
  echo "── tail of the nextest log (full log: the nextest-ci artifact) ──"; tail -n 60 "${TEST_LOG}"; } || true
exit "$rc"
"####;

/// The REQUIRED check: its name is what the `main` ruleset (25572127)
/// requires, and what `just release-tag` and the tag guards require green on
/// a release. A required check that is skipped counts as passing, so this
/// never skips: it fails unless the plan and the gates succeeded and, on a
/// full run, check, wasm and tests too. The advisory work (clippy, the
/// Signal browser remote) cannot fail it.
fn build_and_test() -> Job {
    Job::new(UBUNTU)
        .name("Build + test")
        .needs(&["plan", "gates", "check", "wasm", "tests"])
        .cond("${{ always() }}")
        .timeout(5)
        .step(
            Step::run("Every required job succeeded", AGGREGATE)
                .env("PLAN", "${{ needs.plan.result }}")
                .env("SKIP", "${{ needs.plan.outputs.skip }}")
                .env("FULL", "${{ needs.plan.outputs.full }}")
                .env("GATES", "${{ needs.gates.result }}")
                .env("CHECK", "${{ needs.check.result }}")
                .env("WASM", "${{ needs.wasm.result }}")
                .env("TESTS", "${{ needs.tests.result }}"),
        )
}

const AGGREGATE: &str = r#"set -u
fail=0
bad() { echo "::error::$1"; fail=1; }
[ "$PLAN" = success ] || { bad "plan: $PLAN"; exit 1; }
if [ "$SKIP" = "true" ]; then
  echo "plan: nothing to run (release commit)"; exit 0
fi
need() { # name result
  [ "$2" = success ] || bad "$1: $2"
  echo "$1: $2"
}
need gates "$GATES"
if [ "$FULL" = "true" ]; then
  need check "$CHECK"
  need wasm "$WASM"
  need tests "$TESTS"
else
  echo "check, wasm, tests: not run on a PR into main (the local-ci status covers them)"
fi
exit "$fail"
"#;
