//! `checks` — the fast checks on every PR, the full set after every merge.
//!
//! WHEN THIS RUNS — fleet flow v4, trunk-based (the `release-workflow`
//! skill; docs/agents/release-workflow.md):
//!
//! ```text
//!   PR -> main          HERE, the AFFECTED set (rail plan against the PR's
//!                       base commit, see CHANGE SELECTION), plus `cargo
//!                       rail change check` (every changed crate has a
//!                       change file in .changes/). The required check is
//!                       `Build + test` (the aggregator at the end). PRs
//!                       squash-merge into main.
//!   PR -> release/**    a hotfix onto a release line (just release-hotfix):
//!                       the FULL set, never narrowed.
//!   RELEASE PR release/pending -> main (or -> release/v*)   one commit: the
//!                       CalVer bump + the changelog built from .changes/.
//!                       The plan runs the release guard from the BASE
//!                       commit's scripts/release.sh: a sound release PR
//!                       SKIPS every heavy job (`Build + test` still runs
//!                       and passes: its content was checked when it merged
//!                       into main); a release PR the guard refuses FAILS
//!                       `Build + test`.
//!   push to main        the FULL set, once per merge, and records rail
//!   (and release/**)    EVIDENCE for that commit (main only): it is the
//!                       base the next PRs plan against, and a green
//!                       `Build + test` on a release merge commit is what
//!                       `just release-tag` requires. Nothing ships from a
//!                       push: production changes only when a v* tag is
//!                       pushed.
//!   nightly             the FULL set on main.
//!   by hand             `gh workflow run checks.yml --ref <branch>` runs
//!                       everything; add `-f base=main` to check only what the
//!                       branch changed against main.
//! ```
//!
//! RUNNERS: light and parallel work on GitHub-HOSTED runners (public repo:
//! free, ~20 concurrent Linux jobs org-wide); only the heavy test build on
//! the self-hosted VM, whose warm target dir it needs:
//!
//! ```text
//!   ubuntu-latest  plan (cargo metadata + rail), lint (fmt, manifests,
//!                  the generated workflows, Boundaries, flow gate,
//!                  tailwind, `cargo check --workspace`, clippy: check
//!                  mode, no codegen), wasm (the wasm32 checks), and the
//!                  `Build + test` aggregator
//!   studio-tests   (self-hosted, NixOS) the nextest build + rail evidence:
//!                  ~1460 crates and 814 test binaries at full codegen, 20
//!                  min on the warm Mac Pro; cold on a 4-vCPU hosted runner
//!                  it would not fit the disk or the 10 GB cache budget.
//! ```
//!
//! The three build kinds share NO artifacts except build scripts and proc
//! macros (check-mode units are not the test build's units, and wasm32
//! units are another triple), so they run as separate jobs side by side.
//!
//! HOSTED jobs run WITHOUT nix (see [`crate::steps`]): free the image's disk,
//! install the pinned rustc with dtolnay/rust-toolchain, the native headers
//! from apt (cached), the pinned tools as release binaries, then
//! Swatinem/rust-cache with ONE shared key per build kind ([`rust_cache`]).
//! Debuginfo is line-tables-only (.cargo/ci-config.toml, as [profile.dev]
//! already sets) and incremental is off.
//!
//! The SELF-HOSTED tests job has nix and the toolchain on the runner
//! already, so there is no installer step and no rust-cache action —
//! `CARGO_TARGET_DIR` points at the runner instance's own cache dir
//! instead, which persists across runs (and across restarts/reboots). Its
//! steps run under `nix develop .#ci -c` ([`NIX_DEV`]). Everything the
//! workspace builds is in this repo (daw, processor, signal and patchbay
//! were folded in), so there are no sibling checkouts.
//!
//! CHANGE SELECTION: the `plan` job asks `cargo rail plan` (0.32) which
//! workspace packages a change affects, through cargo's resolved
//! reverse-dependency graph, and scripts/ci-rail-plan.py turns that into
//! gates plus a nextest filterset. Only which tests RUN narrows; the test
//! BUILD stays --workspace so the warm target dir keeps one artifact set.
//! Full runs (push to main, the nightly, a dispatch without `base`) record
//! rail's observed-input evidence for their commit (dep-info of every unit,
//! no extra compiling) into the Actions cache; a later plan against that
//! commit restores it, which is what lets a docs/fixture change narrow
//! instead of widening to the whole workspace. Workflow, Justfile, nix,
//! scripts or nextest-config changes (.config/rail.toml [plan.work.*]),
//! workspace scope, or any rail failure FAIL OPEN to the full set.

use crate::model::{Job, RunsOn, Step, UBUNTU, Workflow, yaml};
use crate::steps::{
    NIX_DEV, RUST, Toolchain, Tools, checkout, checkout_full, ci_cargo_config, faces, footprint,
    hosted_setup, rust_cache, tailwind, target_mark, target_sweep,
};
use anyhow::Result;

/// The features `just test` / `just ci` name: the app CLIs and the worklet
/// rigs sit behind features, and without them their tests never build (an
/// app crate has no lib to dev-depend on).
const TEST_FEATURES: &str =
    "session-desktop/cli,studio-desktop/cli,web-audio-worklet/guitar,web-audio-worklet/keys";

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
    base:
      description: "Only check crates affected since this ref (e.g. main). Empty = everything."
      required: false
      default: ""
schedule:
  - cron: "47 10 * * *"
"#,
        )?,
    )
    // One group per PR, so a new push to a PR cancels its predecessor's run.
    // A PUSH (to main or a release/** line) gets a group of its own commit
    // (the Zed way): every merge is checked to the end, never cancelled by
    // the next merge — its green `Build + test` is the rail evidence the
    // next PRs plan against and what `just release-tag` requires on a
    // release merge commit. The nightly keeps a group of its own so it
    // never cancels (or is cancelled by) a dispatch on main.
    .concurrency(
        "checks-${{ github.event.pull_request.number || github.ref }}-${{ \
         github.event_name == 'schedule' && 'nightly' || \
         github.event_name == 'push' && github.sha || 'run' }}",
        true,
    )
    .job("plan", plan())
    .job("lint", lint())
    .job("wasm", wasm())
    .job("tests", tests())
    .job("tests-warpbuild", tests_warpbuild())
    .job("build-and-test", build_and_test()))
}

/// Which jobs have work: a sound release PR skips them all (a refused one
/// fails here); otherwise rail decides (or the run is full). Hosted: the
/// toolchain for `cargo metadata`, cargo-rail, and the image's python3.
fn plan() -> Job {
    let outputs = [
        "full",
        "cargo",
        "tests",
        "test_filter",
        "test_packages",
        "wasm",
        "skip",
        "record",
    ];
    let mut job = Job::new(UBUNTU)
        .name("Plan")
        .timeout(20)
        // Fork PRs never run here: the tests job runs on a self-hosted
        // runner, and fork code must not run on it. GitHub also withholds
        // secrets from fork PRs. Every other job needs this one, so they
        // skip with it, and `Build + test` fails the fork PR rather than
        // passing it unchecked. (push/schedule/workflow_dispatch have no
        // pull_request.)
        .cond(
            "${{ github.event_name != 'pull_request' || \
             github.event.pull_request.head.repo.full_name == github.repository }}",
        );
    for out in outputs {
        job = job.output(out, &format!("${{{{ steps.filter.outputs.{out} }}}}"));
    }
    job
        // Full history: the plan needs both diff endpoints' ancestry, and
        // the release guard the PR's base and head.
        .step(checkout_full(None))
        .steps(hosted_setup(
            "",
            "",
            Tools {
                cargo_rail: true,
                ..Tools::default()
            },
        ))
        // Which commit to plan against, decided before the evidence restore
        // (its cache key is that commit).
        .step(
            Step::run("Choose the comparison base", CHOOSE_BASE)
                .id("base")
                .env("DISPATCH_BASE", "${{ inputs.base }}")
                .env("BASE_SHA", "${{ github.event.pull_request.base.sha }}"),
        )
        .step(
            Step::uses("Restore rail evidence for the base", "actions/cache/restore@v4")
                .cond("${{ steps.base.outputs.sha != '' }}")
                .with("path", "${{ runner.temp }}/rail-evidence")
                .with(
                    "key",
                    "rail-evidence-${{ runner.os }}-${{ runner.arch }}-cargo.test-${{ steps.base.outputs.sha }}",
                ),
        )
        .step(
            Step::run("Plan affected crates", PLAN)
                .id("filter")
                .env("BASE", "${{ steps.base.outputs.sha }}")
                .env("BASE_SHA", "${{ github.event.pull_request.base.sha }}")
                .env("BASE_REF", "${{ github.event.pull_request.base.ref }}")
                .env("HEAD_SHA", "${{ github.event.pull_request.head.sha }}")
                .env("HEAD_REF", "${{ github.event.pull_request.head.ref }}")
                .env("HEAD_REPO", "${{ github.event.pull_request.head.repo.full_name }}")
                .env("EVIDENCE", "${{ runner.temp }}/rail-evidence/cargo.test.json"),
        )
}

const CHOOSE_BASE: &str = r#"set -u
base=""
case "${GITHUB_EVENT_NAME:-}" in
  pull_request) base="${BASE_SHA}" ;;
  workflow_dispatch)
    if [ -n "${DISPATCH_BASE}" ]; then
      git fetch --quiet origin "${DISPATCH_BASE}" || true
      base="$(git merge-base "origin/${DISPATCH_BASE}" HEAD 2>/dev/null || true)"
    fi ;;
esac
echo "sha=${base}" >> "$GITHUB_OUTPUT"
echo "base: ${base:-<none: full run>}"
"#;

/// The plan itself. Fail open: if it cannot compute an honest plan, it runs
/// everything. Full runs record evidence for the commit they check, so later
/// plans against it can narrow.
///
/// A release PR (release/pending) runs the release guard from the BASE
/// commit's copy of the script (never the PR's, which could exempt itself).
/// 0 = sound: version + changelog only, its content was checked when it
/// merged, skip the heavy jobs. 3 = refused: fail, which fails `Build +
/// test`. 1 = not a release PR: plan as usual. A hotfix onto a release line
/// ships on its own: never narrowed.
///
/// Evidence is only visible to other branches' PRs when saved on main
/// (cargo-rail.md), so release lines don't record it. rail reads
/// .config/rail.toml from the commits it compares: a base with the pre-0.32
/// config fails, which runs everything.
const PLAN: &str = r####"set -u

run_all() {
  echo "plan: $1 — running EVERYTHING"
  printf 'full=true\ncargo=true\ntests=true\ntest_filter=\nwasm=true\n' >> "$GITHUB_OUTPUT"
  exit 0
}
skip_all() {
  echo "plan: $1 — nothing to run"
  printf 'skip=true\nfull=false\ncargo=false\ntests=false\ntest_filter=\nwasm=false\n' >> "$GITHUB_OUTPUT"
  exit 0
}
record() { echo "record=true" >> "$GITHUB_OUTPUT"; }

case "${GITHUB_EVENT_NAME:-}" in
  pull_request)
    rc=1
    if git show "${BASE_SHA}:scripts/release.sh" > "${RUNNER_TEMP}/release-base.sh" 2>/dev/null; then
      bash "${RUNNER_TEMP}/release-base.sh" release-only || rc=$?
      [ "$rc" = 0 ] || [ "$rc" = 1 ] || rc=3
    fi
    case "$rc" in
      0)
        {
          echo "### Release commit only"
          echo "This PR is one commit changing the version, CHANGELOG.md and consumed change files; its content was checked when it merged, so the heavy jobs are skipped."
        } >> "$GITHUB_STEP_SUMMARY"
        skip_all "release commit only (${HEAD_REF} -> ${BASE_REF})" ;;
      3)
        echo "::error::${HEAD_REF} -> ${BASE_REF} is a release PR the release guard refuses (see above)"
        exit 1 ;;
    esac
    case "${BASE_REF}" in
      release/*) run_all "PR into ${BASE_REF}: a hotfix line runs the full set" ;;
    esac ;;
  push)
    [ "${GITHUB_REF_NAME}" != main ] || record
    run_all "push to ${GITHUB_REF_NAME}: full set after every merge" ;;
  schedule)
    record; run_all "nightly (main)" ;;
  workflow_dispatch)
    [ -n "${BASE}" ] || { record; run_all "manual dispatch"; } ;;
  *) run_all "event ${GITHUB_EVENT_NAME:-?}" ;;
esac
[ -n "${BASE}" ] || run_all "no comparison base"

ev=()
if [ -f "${EVIDENCE}" ]; then
  ev=(--evidence "${EVIDENCE}")
  echo "plan: using rail evidence for ${BASE}"
else
  echo "plan: no rail evidence for ${BASE} (non-Rust changes will widen)"
fi
plan="${RUNNER_TEMP}/rail-plan.json"
cargo rail plan --from "${BASE}" --to HEAD "${ev[@]}" --json > "${plan}" \
  || run_all "cargo rail plan failed"
cargo rail plan --from "${BASE}" --to HEAD "${ev[@]}" --explain-work cargo.test || true

gates="$(python3 scripts/ci-rail-plan.py "${plan}")" \
  || run_all "plan parse failed"
[ -n "${gates}" ] || run_all "plan parse produced no gates"
echo "${gates}" >> "$GITHUB_OUTPUT"
echo "${gates}"
"####;

/// The env every hosted cargo job sets. Incremental is off: a CI build is
/// never rebuilt in place, and incremental dirs would only fatten the cache.
/// (Debuginfo is pinned line-tables-only in .cargo/ci-config.toml, which
/// replaced the `CARGO_PROFILE_DEV_DEBUG` env.)
fn hosted_cargo_job(job: Job) -> Job {
    job.env("CARGO_INCREMENTAL", "0")
}

/// From here on a step runs even when an earlier gate failed, so one run
/// reports fmt AND Boundaries AND check instead of one at a time; the job
/// still fails.
const NOT_CANCELLED: &str = "${{ !cancelled() }}";

/// The cargo steps are gated on the plan: a change that touches no crate
/// (docs, with evidence for the base) compiles nothing.
const CARGO_WANTED: &str = "${{ !cancelled() && needs.plan.outputs.cargo == 'true' }}";

/// REQUIRED (through `Build + test`): manifests + lockfile, fmt, the
/// generated workflows, the boundaries and flow gates, the tailwind sheet
/// and `cargo check --workspace`. Clippy reports here too, advisory.
fn lint() -> Job {
    hosted_cargo_job(
        Job::new(UBUNTU)
            .name("Lint (fmt + manifests + gates + check + clippy)")
            .needs(&["plan"])
            .cond("${{ needs.plan.outputs.skip != 'true' }}")
            .timeout(90),
    )
    .step(checkout())
    .steps(hosted_setup(
        "rustfmt,clippy",
        "",
        Tools {
            apt: true,
            just: true,
            cargo_rail: true,
            tracey: true,
            tailwind: true,
            actionlint: true,
            ci_cargo_config: true,
            free_disk: true,
            ..Tools::default()
        },
    ))
    .steps(rust_cache("lint"))
    .step(Step::run("Manifests + lockfile", LOCKFILE))
    // fmt is ungated: it compiles nothing, so change selection would save
    // nothing.
    .step(Step::run("cargo fmt --check", "cargo fmt --all --check"))
    .steps(workflow_gates())
    .step(change_check())
    .steps(boundaries())
    // The workflow-spec verification standard (#48): every `flow.*` rule
    // with an r[impl] has an r[verify], or the build is red. Same recipe as
    // local (`just daw-flows`); tracey is the pinned release binary. Cheap,
    // so it runs before any cargo step. `--own-daemon`: the script runs its
    // own tracey daemon under a short state dir — the runner's HOME pushes
    // tracey's default socket path past the 108-byte Unix limit — and stops
    // it after.
    .step(Step::run("Flow verification gate", "just daw-flows --own-daemon").cond(NOT_CANCELLED))
    .step(tailwind(Toolchain::Hosted))
    .step(Step::run("cargo check --workspace", "cargo check --workspace").cond(CARGO_WANTED))
    // Advisory only: the lint debt is being paid down crate by crate, so
    // this reports (grouped by lint and crate, .github/scripts/
    // clippy-report.py) without blocking the PR. Same check-mode dependency
    // metadata as `cargo check`, so it lives in this job.
    .step(
        Step::run(
            "cargo clippy (advisory, does not block)",
            "\
cargo clippy --workspace --no-deps --all-targets --keep-going \\
  --message-format=json -- --cap-lints warn \\
  | python3 .github/scripts/clippy-report.py
",
        )
        .cond(CARGO_WANTED)
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
/// from them. PRs only: a push or the nightly has no "change" to describe,
/// and a release PR never reaches this job. `--since <the PR's base
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

/// Crate-layout boundaries (architect ADR layout.md, STA-268): resolves
/// graphs and scans sources, compiles nothing of studio's. Layer table +
/// ratcheted baselines in boundaries.toml. architect's xtask, `cargo
/// install`ed by `just ci-boundaries` into ~/.cache/architect-xtask/<tag>
/// (4+ minutes cold). Keyed on the pinned tag and the rustc that built it
/// (it was the nix toolchain's, keyed on flake.lock; a binary built in the
/// nix shell would not even load outside it), so an entry never changes and
/// is saved once. Saved even when Boundaries reports violations: the binary
/// is fine.
fn boundaries() -> [Step; 5] {
    let path = "~/.cache/architect-xtask/${{ steps.xtask.outputs.tag }}";
    let key = format!(
        "architect-xtask-${{{{ runner.os }}}}-${{{{ steps.xtask.outputs.tag }}}}-rust-{RUST}"
    );
    [
        Step::run(
            "architect xtask tag",
            "echo \"tag=$(sed -n 's/^architect_xtask_tag := \"\\(.*\\)\"$/\\1/p' Justfile)\" >> \"$GITHUB_OUTPUT\"\n",
        )
        .id("xtask")
        .cond(NOT_CANCELLED),
        Step::uses("Restore architect xtask", "actions/cache/restore@v4")
            .id("xtask-cache")
            .cond("${{ !cancelled() && steps.xtask.outputs.tag != '' }}")
            .with("path", path)
            .with("key", key.as_str()),
        Step::run("Boundaries", "just ci-boundaries").cond(NOT_CANCELLED),
        Step::run(
            "architect xtask built?",
            "\
if [ -x \"$HOME/.cache/architect-xtask/${TAG}/bin/xtask\" ]; then
  echo \"yes=true\" >> \"$GITHUB_OUTPUT\"
fi
",
        )
        .id("xtask-built")
        .cond(
            "${{ !cancelled() && steps.xtask.outputs.tag != '' && \
             steps.xtask-cache.outputs.cache-hit != 'true' }}",
        )
        .env("TAG", "${{ steps.xtask.outputs.tag }}"),
        Step::uses("Save architect xtask", "actions/cache/save@v4")
            .cond("${{ !cancelled() && steps.xtask-built.outputs.yes == 'true' }}")
            .with("path", path)
            .with("key", key.as_str()),
    ]
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
            .cond("${{ needs.plan.outputs.skip != 'true' && needs.plan.outputs.wasm == 'true' }}")
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
            tailwind: true,
            ci_cargo_config: true,
            free_disk: true,
            ..Tools::default()
        },
    ))
    .steps(rust_cache("wasm"))
    .step(tailwind(Toolchain::Hosted))
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

/// REQUIRED (through `Build + test`, when the plan wants tests): the test
/// run. It was advisory while main carried ~100 nextest failures (STA-184,
/// the captain's call in STA-276 to release first and fix after); it is now
/// a required job like lint and wasm. It is skipped on a release-only PR.
///
/// SELF-HOSTED on purpose (the one heavy job): ~1460 crates at full codegen
/// into 814 test binaries took 20 min to build on the warm Mac Pro (run
/// 39654183163); the tests themselves ran in 4.5 min. Cold on a hosted
/// runner that build would cost well over an hour every PR and a target
/// dir larger than the whole 10 GB cache budget. The runner is a NixOS VM:
/// nix is its package manager, so the steps run in the ci dev shell.
fn tests() -> Job {
    let nix = Toolchain::Nix;
    Job::new(RunsOn::Labels(&["self-hosted", "studio-tests"]))
        .name("tests")
        .needs(&["plan"])
        .cond("${{ needs.plan.outputs.skip != 'true' && needs.plan.outputs.tests == 'true' }}")
        .timeout(90)
        .env("NIX_DEV", NIX_DEV)
        // Same list as `just test` / `just ci`; the evidence recording below
        // uses it too, so both builds are the same units.
        .env("TEST_FEATURES", TEST_FEATURES)
        // No incremental caches: they grow without bound in a persistent
        // target (372 GB on one dev machine) and a CI build seldom rebuilds
        // the same crate twice. Dioxus and Zed run CI the same way.
        .env("CARGO_INCREMENTAL", "0")
        .step(checkout())
        .environment(ASSETS_ENVIRONMENT.into())
        // The CI cargo config here too: its profile is what the hosted jobs
        // build with, and it builds frame and plugin-faces optimised for
        // their tests (.cargo/ci-config.toml says why).
        .step(ci_cargo_config())
        .step(Step::run("Persistent target dir", TARGET_DIR))
        .step(target_mark())
        .step(assets_token())
        .step(assets(nix))
        // The test run compiles session-web.
        .step(tailwind(nix))
        .step(faces(nix))
        // Full runs first record rail evidence for this commit: the recorder
        // wraps rustc for one `cargo test --no-run` (units already fresh
        // keep their dep-info, so this compiles nothing nextest wouldn't),
        // and the nextest run after it finds every unit fresh. Best effort:
        // a failure only means later plans against this commit widen. Not
        // `--locked`, for the [[patch.unused]] ordering reason in the lint
        // job.
        .step(
            Step::run(
                "Record rail evidence",
                &format!(
                    "\
mkdir -p \"${{EVIDENCE_DIR}}\"
{} \\
  --output \"${{EVIDENCE_DIR}}/cargo.test.json\" \\
  -- test --workspace --no-run --features \"${{TEST_FEATURES}}\" \\
  || echo \"::warning::rail evidence not recorded; later plans against this commit widen\"
",
                    nix.cmd("cargo rail plan evidence --work cargo.test")
                ),
            )
            .cond("${{ needs.plan.outputs.record == 'true' }}")
            .env("EVIDENCE_DIR", "${{ runner.temp }}/rail-evidence"),
        )
        .step(nextest(nix, "tests (self-hosted)"))
        .step(target_sweep())
        .step(
            Step::run(
                "Evidence key",
                "\
echo \"sha=$(git rev-parse HEAD)\" >> \"$GITHUB_OUTPUT\"
[ -f \"${RUNNER_TEMP}/rail-evidence/cargo.test.json\" ] && echo \"have=true\" >> \"$GITHUB_OUTPUT\" || true
",
            )
            .id("evidence")
            .cond("${{ always() && needs.plan.outputs.record == 'true' }}"),
        )
        // Saved even when tests are red: evidence describes the build, not
        // the results. Only saves on main (and the nightly's default-branch
        // scope) are visible to other branches' PRs.
        .step(
            Step::uses("Save rail evidence", "actions/cache/save@v4")
                .cond("${{ always() && steps.evidence.outputs.have == 'true' }}")
                .with("path", "${{ runner.temp }}/rail-evidence")
                .with(
                    "key",
                    "rail-evidence-${{ runner.os }}-${{ runner.arch }}-cargo.test-${{ steps.evidence.outputs.sha }}",
                ),
        )
        .step(test_log("nextest-ci"))
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

/// The nextest step both test jobs run: the plan's selection, a timed build
/// phase and a timed test phase, the digest in the log, the timings in the
/// step summary. The body runs under bash in the job's toolchain (inside the
/// dev shell on the NixOS host, which has no grep/tail guarantees on the
/// runner's PATH).
fn nextest(tc: Toolchain, label: &str) -> Step {
    Step::run(
        "cargo nextest",
        &format!("{} <<'EOF'\n{NEXTEST}EOF\n", tc.cmd("bash -uo pipefail -s")),
    )
    .id("nextest")
    .env("TEST_FILTER", "${{ needs.plan.outputs.test_filter }}")
    .env("TEST_PACKAGES", "${{ needs.plan.outputs.test_packages }}")
    .env("TEST_LOG", "${{ runner.temp }}/nextest-ci.log")
    .env("RUN_LABEL", label)
}

/// The WarpBuild runner the trial job uses. One constant, so trying another
/// size is a one-line change (then `just workflows`).
const WARPBUILD_RUNNER: &str = "warp-ubuntu-latest-x64-16x";

/// The WarpBuild snapshot alias: the trial job boots from the VM snapshot
/// saved under it (toolchain, apt packages, ~/.cargo and the warm target dir
/// in ~/.cache/cargo-target), so its build is incremental the way the Mac
/// Pro's is.
const WARPBUILD_SNAPSHOT: &str = "studio-tests";

/// TRIAL (never part of `Build + test`, `continue-on-error`): the same test
/// run as [`tests`] on a WarpBuild cloud runner, WITHOUT nix — the hosted
/// jobs' rustup/apt/release-binary setup plus nextest and lavapipe for the
/// GPU tests — to compare its build and test times (the step summary) with
/// the self-hosted Mac Pro VM.
///
/// Off until the `WARPBUILD_TRIAL` repository variable is `true`, so it
/// costs nothing (and queues nothing) before the WarpBuild GitHub App is
/// installed on the org.
///
/// The warm target dir is a WarpBuild SNAPSHOT runner
/// (`<label>;snapshot.key=<alias>`, WarpBuilds/snapshot-save): the VM boots
/// from the last snapshot saved under [`WARPBUILD_SNAPSHOT`] (or the base
/// image if there is none). Only main pushes and the nightly SAVE it (after
/// a cleanup, below), so a PR never writes the state the next run boots
/// from — the same rule as the hosted rust-cache. Snapshots expire after 15
/// days. No rail evidence here: the self-hosted job records it, and a second
/// writer would race it for the same cache key.
fn tests_warpbuild() -> Job {
    let warp = Toolchain::Hosted;
    Job::new(RunsOn::Owned(format!(
        "{WARPBUILD_RUNNER};snapshot.key={WARPBUILD_SNAPSHOT}"
    )))
    .name("tests (warpbuild trial)")
    .needs(&["plan"])
    .cond(
        "${{ vars.WARPBUILD_TRIAL == 'true' && needs.plan.outputs.skip != 'true' && \
         needs.plan.outputs.tests == 'true' }}",
    )
    .timeout(120)
    .continue_on_error()
    // Incremental off, as on the Mac Pro, so the timings compare like for
    // like.
    .env("CARGO_INCREMENTAL", "0")
    .env("TEST_FEATURES", TEST_FEATURES)
    .step(checkout())
    .steps(hosted_setup(
        "",
        "",
        Tools {
            apt: true,
            gpu: true,
            just: true,
            nextest: true,
            tailwind: true,
            ci_cargo_config: true,
            ..Tools::default()
        },
    ))
    // ~/.cache/cargo-target: outside the checkout, so the pre-snapshot
    // `git clean` keeps it.
    .environment(ASSETS_ENVIRONMENT.into())
    .step(Step::run("Persistent target dir", TARGET_DIR))
    .step(target_mark())
    .step(assets_token())
    .step(assets(warp))
    .step(tailwind(warp))
    .step(faces(warp))
    .step(nextest(warp, "tests (warpbuild trial)"))
    .step(target_sweep())
    .step(test_log("nextest-warpbuild"))
    // Before the snapshot: nothing secret may be in it (WarpBuild's own
    // advice; a snapshot alias is readable by any run in the org). The
    // asset clones STAY: PR runs get no credential (see `assets`), so the
    // snapshot is how a PR on this runner has them, and they are only as
    // private as the org that owns both. The checkout's ignored files go.
    // The target dir, ~/.cargo, rustup and the apt packages stay: that is
    // the point.
    .step(
        Step::run(
            "Clean up before the snapshot",
            "\
rm -rf \"$HOME/.ssh\" \"$HOME/.aws\"
git clean -ffdx
du -sh \"$HOME/.cache/cargo-target\" 2>/dev/null || true
",
        )
        .cond(SAVE_SNAPSHOT),
    )
    .step(
        Step::uses("Save the WarpBuild snapshot", "WarpBuilds/snapshot-save@v1")
            .cond(SAVE_SNAPSHOT)
            .with("alias", WARPBUILD_SNAPSHOT)
            .with("fail-on-error", false)
            .with("wait-timeout-minutes", 60),
    )
}

/// The trial's snapshot is saved by main's full runs only (a push or the
/// nightly), and even when tests are red: it holds the build, not results.
const SAVE_SNAPSHOT: &str = "${{ !cancelled() && github.ref == 'refs/heads/main' && \
     github.event_name != 'pull_request' }}";

/// On the runner's big disk ($CI_CACHE_DIR: the runner module's
/// per-instance dir), not /var/cache on the root disk, which a 580 GB target
/// dir filled until jobs died mid-test. Per instance, so instances never
/// share a cargo lock. The fallbacks are for a runner without one.
///
/// Cargo never collects garbage: every branch's lockfile and features leave
/// their own units here until the disk is full and rustc dies mid-build (PR
/// #184, October 2026). [`target_sweep`] is the garbage collection; this is
/// the backstop: the incremental caches always go (the jobs build without
/// them), and below 100 GB free the whole dir does.
const TARGET_DIR: &str = r#"dir="${CI_CACHE_DIR:-${CACHE_DIRECTORY:-$HOME/.cache}}/cargo-target"
mkdir -p "$dir"
echo "CARGO_TARGET_DIR=$dir" >> "$GITHUB_ENV"
free_gb() { df -Pk "$dir" | awk 'NR==2 { print int($4 / 1048576) }'; }
# The jobs build with CARGO_INCREMENTAL=0: any incremental cache here is left
# over from before, and only takes disk.
rm -rf "$dir"/*/incremental "$dir"/*/*/incremental
if [ "$(free_gb)" -lt 100 ]; then
  echo "::warning::still $(free_gb) GB free: clearing the target dir (a cold build follows)"
  rm -rf "$dir" && mkdir -p "$dir"
fi
echo "target dir: $dir ($(du -sh "$dir" 2>/dev/null | cut -f1)), $(free_gb) GB free"
"#;

/// The large fixtures and the rigs' NAM captures / IRs live in the private
/// repos studio-assets, rig-models and mic-models (assets.txt); `just assets` clones them
/// into .assets/ and links each file into place, so the tests that need them
/// run instead of skipping.
///
/// WHO MAY FETCH: only main's full runs (a push to main, the nightly). They
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

/// What to build and run, from the plan job (scripts/ci-rail-plan.py):
///
/// - TEST_PACKAGES: the packages a PR affects, space-separated. Non-empty =
///   the BUILD narrows too: `-p A -p B …`, with `--features` cut down to the
///   TEST_FEATURES entries whose package is in the set (cargo refuses a
///   `pkg/feature` for a package it is not building). Empty — every full run
///   (push, nightly, dispatch without `base`, a release line) and every
///   fail-open path — = `--workspace` with every TEST_FEATURES entry, as
///   before.
/// - TEST_FILTER: the same set as a nextest filterset; empty = every test.
///
/// The accepted cost of a narrowed build: building a subset can unify
/// features differently from `--workspace`, so some dependencies are built
/// a second time with other features and that second artifact set also
/// lands in the warm target dir. (rail evidence is still recorded with
/// `--workspace`: it describes the full build.)
///
/// Build and tests are two timed phases (`nextest run --no-run`, then the
/// run, which finds every unit fresh); the times go to the step summary so
/// the runners can be compared.
///
/// nextest's report goes to a FILE, then a digest to the step's log. On the
/// Mac Pro VM runners Task's nextest died with exit 110, "error writing to
/// output", at the first failing test's captured output: writing to the
/// runner's stdout pipe failed. A regular file cannot do that. The full log
/// is the job's test-log artifact.
const NEXTEST: &str = r####"sel=(--workspace)
feats="${TEST_FEATURES}"
if [ -n "${TEST_PACKAGES:-}" ]; then
  sel=()
  feats=""
  for p in ${TEST_PACKAGES}; do sel+=(-p "$p"); done
  IFS=, read -ra all <<< "${TEST_FEATURES}"
  for f in "${all[@]}"; do
    case " ${TEST_PACKAGES} " in *" ${f%%/*} "*) feats="${feats:+${feats},}${f}" ;; esac
  done
fi
feat=()
[ -n "${feats}" ] && feat=(--features "${feats}")
filter=()
[ -n "${TEST_FILTER}" ] && filter=(-E "${TEST_FILTER}" --no-tests=pass)
echo "selection: ${sel[*]} ${feat[*]}"
t0=$(date +%s)
cargo nextest run "${sel[@]}" "${feat[@]}" --no-run >"${TEST_LOG}" 2>&1
rc=$?
t1=$(date +%s)
# A full build that succeeded read every unit the workspace uses: the target
# dir may be pruned to that set (target_sweep), whatever the tests say.
if [ "$rc" = 0 ] && [ -z "${TEST_PACKAGES:-}" ]; then
  echo "prunable=true" >> "${GITHUB_OUTPUT}"
fi
if [ "$rc" = 0 ]; then
  cargo nextest run "${sel[@]}" --no-fail-fast "${feat[@]}" "${filter[@]}" >>"${TEST_LOG}" 2>&1
  rc=$?
fi
t2=$(date +%s)
{ echo "### ${RUN_LABEL}: timing"
  echo "| phase | seconds |"
  echo "|---|---|"
  echo "| build (nextest --no-run) | $((t1 - t0)) |"
  echo "| tests | $((t2 - t1)) |"
  echo
  echo "selection: \`${sel[*]} ${feat[*]}\`; exit ${rc}"; } >> "${GITHUB_STEP_SUMMARY}"
{ grep -E '^ *(FAIL|TRY [0-9]+ (FAIL|TMT)|SIGSEGV|SIGABRT|LEAK|TIMEOUT)|Summary \[|tests were not run|^error' "${TEST_LOG}" | head -n 200
  echo "── tail of the nextest log (full log: the nextest-ci artifact) ──"; tail -n 60 "${TEST_LOG}"; } || true
exit "$rc"
"####;

/// The REQUIRED check: its name is what the `main` ruleset (25572127)
/// requires, unchanged from the single-job days, and what `just
/// release-tag` and the tag guards require green on a release. A required
/// check that is skipped counts as passing, so this never skips: it fails
/// unless every REQUIRED job the plan asked for succeeded (lint; wasm and
/// tests when the plan gave them work). A job may only be skipped when the
/// plan said it had no work. The advisory work (clippy, the Signal browser
/// remote) cannot fail it.
fn build_and_test() -> Job {
    Job::new(UBUNTU)
        .name("Build + test")
        .needs(&["plan", "lint", "wasm", "tests"])
        .cond("${{ always() }}")
        .timeout(5)
        .step(
            Step::run("Every required job succeeded", AGGREGATE)
                .env("EVENT", "${{ github.event_name }}")
                .env(
                    "FORK",
                    "${{ github.event_name == 'pull_request' && \
                     github.event.pull_request.head.repo.full_name != github.repository }}",
                )
                .env("PLAN", "${{ needs.plan.result }}")
                .env("SKIP", "${{ needs.plan.outputs.skip }}")
                .env("LINT", "${{ needs.lint.result }}")
                .env("WASM", "${{ needs.wasm.result }}")
                .env("WANT_WASM", "${{ needs.plan.outputs.wasm }}")
                .env("TESTS", "${{ needs.tests.result }}")
                .env("WANT_TESTS", "${{ needs.plan.outputs.tests }}"),
        )
}

const AGGREGATE: &str = r#"set -u
fail=0
bad() { echo "::error::$1"; fail=1; }
if [ "$FORK" = "true" ]; then
  bad "fork PR: not run (the tests job is self-hosted); a maintainer must re-push it from a branch of this repo"
  exit 1
fi
[ "$PLAN" = success ] || bad "plan: $PLAN"
if [ "$SKIP" = "true" ]; then
  echo "plan: nothing to run (release commit)"; exit "$fail"
fi
check() { # name result wanted
  if [ "$3" = "true" ]; then
    [ "$2" = success ] || bad "$1: $2"
  else
    case "$2" in success|skipped) ;; *) bad "$1: $2" ;; esac
  fi
  echo "$1: $2 (wanted=$3)"
}
check lint "$LINT" true
check wasm "$WASM" "$WANT_WASM"
check tests "$TESTS" "$WANT_TESTS"
exit "$fail"
"#;
