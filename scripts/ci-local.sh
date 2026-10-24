#!/usr/bin/env bash
# Local CI: the expensive half of the PR gate, run on this machine's warm
# target dir instead of a billed runner. The same model as Task's.
#
#   scripts/ci-local.sh [BASE]        run the checks (`just ci-local`)
#   scripts/ci-local.sh --post SHA…   mark each commit `Local CI` on GitHub
#                                     once GitHub has it (the pre-push hook,
#                                     scripts/ci-local-hook.sh, runs this
#                                     detached, after the push)
#
# PRs into main run only the free `PR checks` (pr.yml: fmt, change files,
# the lockfile, the rail config) plus the `Local CI` status this posts. The
# rest of the PR gate runs HERE: the static guards (the generated
# workflows + actionlint, the flow gate), then the tests, `cargo check
# --workspace` and the wasm check for what the change affects: `cargo rail
# plan` from BASE (default: the merge-base with origin/main) through
# cargo's reverse-dependency graph, turned into gates by
# scripts/ci-rail-plan.py. The full set runs on WarpBuild after every merge
# to main (checks.yml), which is what catches two PRs that pass alone and
# break together.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

now() { date -u +%FT%TZ; }

if [ "${1:-}" = "--post" ]; then
  shift
  repo="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
  # How long the run took, from the pass record of the tree that ran it
  # (the hook posts right after, so HEAD is still that tree). GitHub caps a
  # status description at 140 characters.
  pf="$(git rev-parse --git-common-dir)/ci-local-pass/$(git rev-parse HEAD)"
  if [ -f "$pf" ] && [ -n "$(sed -n 2p "$pf")" ]; then
    desc="passed in $(sed -n 2p "$pf") on $(hostname -s): $(sed -n 3p "$pf")"
  else
    desc="rail-planned tests + check + wasm passed on $(hostname -s)"
  fi
  desc="${desc:0:140}"
  # Every layer's push re-posts the whole stack; post each commit once.
  # Keyed by the status name too, so a rename re-posts under the new one.
  posted="$(git rev-parse --git-common-dir)/ci-local-posted/Local-CI"
  mkdir -p "$posted"
  for sha in "$@"; do
    [ -f "$posted/$sha" ] && continue
    # The hook runs before the push lands: wait for GitHub to know the
    # commit (a status on an unknown sha is a 422).
    for _ in $(seq 1 60); do
      gh api "repos/$repo/commits/$sha" --silent 2>/dev/null && break
      sleep 5
    done
    if gh api -X POST "repos/$repo/statuses/$sha" --silent \
      -f state=success -f context="Local CI" -f description="$desc"; then
      now > "$posted/$sha"
      echo "$(now) Local CI: $sha"
    else
      echo "$(now) Local CI: posting to $sha FAILED"
    fi
  done
  exit 0
fi

features="session-desktop/cli,studio-desktop/cli,web-audio-worklet/guitar,web-audio-worklet/keys"

base="${1:-}"
if [ -z "$base" ]; then
  git fetch --quiet origin main || echo "ci-local: fetch failed; planning against the local origin/main" >&2
  base="$(git merge-base origin/main HEAD)"
fi
echo "ci-local: $(git rev-parse --short HEAD) against $(git rev-parse --short "$base")"
# The commit this run checks: a pass is recorded for it only if HEAD is
# still it at the end (a commit made mid-run was never checked).
started_at="$(git rev-parse HEAD)"

# Wall time per phase, for the summary line, the pass record and the
# `Local CI` status description ("passed in 6m12s on host: tests 2m05s, …").
start=$SECONDS
timings=()
dur() { if [ "$1" -ge 60 ]; then printf '%dm%02ds' $(($1 / 60)) $(($1 % 60)); else printf '%ds' "$1"; fi; }
phase() {
  local name="$1" t=$SECONDS
  shift
  "$@"
  timings+=("$name $(dur $((SECONDS - t)))")
}

# Remember the pass for this exact commit (only if the tree is clean, so the
# pass is the commit's): the pre-push hook then vouches without re-running.
# `gh stack submit` pushes each layer separately, and git holds the
# connection to GitHub open while the hook runs, so a long first run can
# outlast it — the push fails, and the retry finds this and goes through.
finish() {
  local total breakdown dirty pass
  total="$(dur $((SECONDS - start)))"
  breakdown="$(printf '%s, ' "${timings[@]}")"
  breakdown="${breakdown%, }"
  dirty="$(git status --porcelain --untracked-files=no)"
  if [ -n "${1:-}" ]; then
    echo "ci-local: BASE given, so no pass recorded (a narrower run doesn't vouch for the commit)"
  elif [ "$(git rev-parse HEAD)" != "$started_at" ]; then
    echo "ci-local: HEAD moved to $(git rev-parse --short HEAD) during the run, so no pass recorded (this run checked $(git rev-parse --short "$started_at"))"
  elif [ -n "$dirty" ]; then
    echo "ci-local: the tree changed, so no pass recorded for $(git rev-parse --short HEAD):"
    echo "$dirty"
  else
    # Line 1 when, line 2 the total, line 3 the per-phase breakdown (the
    # poster reads 2 and 3 into the status description).
    pass="$(git rev-parse --git-common-dir)/ci-local-pass"
    mkdir -p "$pass"
    printf '%s\n%s\n%s\n' "$(now)" "$total" "$breakdown" > "$pass/$(git rev-parse HEAD)"
    echo "ci-local: pass recorded for $(git rev-parse --short HEAD)"
  fi
  echo "ci-local: green in $total ($breakdown)"
}

# A release PR (release/pending): the release guard CI runs, from the base's
# copy of the script. A sound one is the version + changelog only, whose
# content was checked when it merged: nothing to build.
if [ "$(git rev-parse --abbrev-ref HEAD)" = release/pending ]; then
  guard="$(mktemp)"
  git show "$base:scripts/release.sh" >"$guard"
  rc=0
  HEAD_SHA="$(git rev-parse HEAD)" HEAD_REF=release/pending BASE_REF=main \
    bash "$guard" release-only || rc=$?
  rm -f "$guard"
  case "$rc" in
    0)
      echo "ci-local: a sound release commit: nothing to build"
      timings+=("release guard $(dur $((SECONDS - start)))")
      finish "${1:-}"
      exit 0 ;;
    1) ;;
    *) echo "ci-local: the release guard refuses this release commit" >&2; exit 1 ;;
  esac
fi

# The static guards, seconds each: what pr.yml checks, plus the generated
# workflows (+ actionlint) and the flow gate, which CI's `Gates` job also
# runs after a merge.
checks() {
  echo "── fmt ──"
  cargo fmt --all --check
  echo "── change files ──"
  cargo rail change check --since "$base"
  echo "── rail config ──"
  cargo rail config validate --strict
  echo "── generated workflows + actionlint ──"
  just workflows-check
  echo "── flow verification gate ──"
  # The user's tracey daemon, not `--own-daemon`: that mode is for CI's
  # runners (whose HOME pushes the socket path past the Unix limit), and on
  # macOS tracey keeps its socket outside the XDG_STATE_HOME it sets.
  just daw-flows
}
phase checks checks

# Rail evidence for the base, if `just rail-evidence` recorded it on this
# machine: without it a changed file outside a crate's sources (docs, CI
# files) widens to the whole workspace. Evidence binds to the cargo
# environment, so it is recorded here, never borrowed from CI.
ev=()
evdir="$(git rev-parse --git-common-dir)/rail-evidence/$(git rev-parse "$base")"
for f in "$evdir"/*.json; do
  [ -f "$f" ] && ev+=(--evidence "$f")
done
[ "${#ev[@]}" -gt 0 ] && echo "ci-local: using rail evidence for the base (${#ev[@]} file(s))"

# Fail open, like CI: no honest plan means everything.
plan="$(mktemp)"
trap 'rm -f "$plan"' EXIT
gates=""
t=$SECONDS
if cargo rail plan --from "$base" --to HEAD ${ev[@]+"${ev[@]}"} --json >"$plan"; then
  gates="$(python3 scripts/ci-rail-plan.py "$plan")" || gates=""
fi
timings+=("plan $(dur $((SECONDS - t)))")
[ -n "$gates" ] || { echo "ci-local: no plan; running everything"; gates=$'cargo=true\ncheck=true\ntests=true\ntest_filter=\nwasm=true'; }
gate() { printf '%s\n' "$gates" | sed -n "s/^$1=//p" | head -1; }
echo "ci-local: check=$(gate check) tests=$(gate tests) wasm=$(gate wasm) filter=$(gate test_filter)"

# The test build is --workspace (one artifact set in the warm target dir; a
# `-p` subset changes feature unification) and only which tests RUN narrows.
# Its codegen is what `cargo check` then finds fresh.
tests() {
  just assets >/dev/null
  just faces >/dev/null
  local filter=() f
  f="$(gate test_filter)"
  [ -n "$f" ] && filter=(-E "$f" --no-tests=pass)
  cargo nextest run --workspace --no-fail-fast --features "$features" ${filter[@]+"${filter[@]}"}
}
if [ "$(gate tests)" = true ]; then
  phase tests tests
fi
if [ "$(gate check)" = true ] || [ "$(gate cargo)" = true ]; then
  phase check cargo check --workspace
fi
if [ "$(gate wasm)" = true ]; then
  phase wasm just web-wasm-check
fi

finish "${1:-}"
