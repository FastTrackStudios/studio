#!/usr/bin/env bash
# Local CI: the expensive half of the PR gate, run on this machine's warm
# target dir instead of a billed runner. The same model as Task's.
#
#   scripts/ci-local.sh [BASE]        run the checks (`just ci-local`)
#   scripts/ci-local.sh --post SHA…   mark each commit `local-ci` on GitHub
#                                     once GitHub has it (the pre-push hook
#                                     runs this detached, after the push)
#
# PRs into main run only the free hosted checks (checks.yml `pr-checks`:
# fmt, manifests, the generated workflows, change files, flow gate and
# rail config) plus the `local-ci` status this posts. The tests, the
# workspace check and the wasm check run HERE, for what the change affects:
# `cargo rail plan` from BASE (default: the merge-base with origin/main)
# through cargo's reverse-dependency graph, turned into gates by
# scripts/ci-rail-plan.py, the same script CI plans with. The full set runs
# on WarpBuild after every merge to main, which is what catches two PRs
# that pass alone and break together.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if [ "${1:-}" = "--post" ]; then
  shift
  repo="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
  desc="rail-planned check + tests + wasm passed on $(hostname -s) ($(uname -s)-$(uname -m))"
  for sha in "$@"; do
    # The hook runs before the push lands: wait for GitHub to know the
    # commit (a status on an unknown sha is a 422).
    for _ in $(seq 1 60); do
      gh api "repos/$repo/commits/$sha" --silent 2>/dev/null && break
      sleep 5
    done
    gh api -X POST "repos/$repo/statuses/$sha" --silent \
      -f state=success -f context=local-ci -f description="$desc" \
      && echo "$(date -u +%FT%TZ) local-ci: $sha" \
      || echo "$(date -u +%FT%TZ) local-ci: posting to $sha FAILED"
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

# A release PR (release/pending): the release guard CI's plan runs, from
# the base's copy of the script. A sound one is the version + changelog
# only, whose content was checked when it merged: nothing to build.
if [ "$(git rev-parse --abbrev-ref HEAD)" = release/pending ]; then
  guard="$(mktemp)"
  git show "$base:scripts/release.sh" >"$guard"
  rc=0
  HEAD_SHA="$(git rev-parse HEAD)" HEAD_REF=release/pending BASE_REF=main \
    bash "$guard" release-only || rc=$?
  rm -f "$guard"
  case "$rc" in
    0) echo "ci-local: a sound release commit: nothing to build"; exit 0 ;;
    1) ;;
    *) echo "ci-local: the release guard refuses this release commit" >&2; exit 1 ;;
  esac
fi

echo "── fmt ──"
cargo fmt --all --check
echo "── change files ──"
cargo rail change check --since "$base"

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
if cargo rail plan --from "$base" --to HEAD ${ev[@]+"${ev[@]}"} --json >"$plan"; then
  gates="$(python3 scripts/ci-rail-plan.py "$plan")" || gates=""
fi
[ -n "$gates" ] || { echo "ci-local: no plan; running everything"; gates=$'cargo=true\ncheck=true\ntests=true\ntest_filter=\nwasm=true'; }
gate() { printf '%s\n' "$gates" | sed -n "s/^$1=//p" | head -1; }
echo "ci-local: check=$(gate check) tests=$(gate tests) wasm=$(gate wasm) filter=$(gate test_filter)"

# The test build is --workspace (one artifact set in the warm target dir; a
# `-p` subset changes feature unification) and only which tests RUN narrows.
# Its codegen is what `cargo check` then finds fresh.
if [ "$(gate tests)" = true ]; then
  echo "── tests ──"
  just assets >/dev/null
  just faces >/dev/null
  filter=()
  f="$(gate test_filter)"
  [ -n "$f" ] && filter=(-E "$f" --no-tests=pass)
  cargo nextest run --workspace --no-fail-fast --features "$features" ${filter[@]+"${filter[@]}"}
fi
if [ "$(gate check)" = true ] || [ "$(gate cargo)" = true ]; then
  echo "── cargo check --workspace ──"
  cargo check --workspace
fi
if [ "$(gate wasm)" = true ]; then
  echo "── wasm ──"
  just web-wasm-check
fi
echo "ci-local: green"
