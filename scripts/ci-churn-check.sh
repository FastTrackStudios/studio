#!/usr/bin/env bash
# Would a warm CI tests job rebuild something it didn't need to?
#
# CI's tests job boots from a WarpBuild snapshot holding main's last target
# dir and checkout, re-runs its prep (`just assets`, `just faces`) and
# builds the tests. Anything a build script watches that
# the prep touches — a re-made symlink, a rewritten sheet, a path that isn't
# there — recompiles that crate and everything above it, on every run (it
# cost 13 min of a warm PR build, 2026-10). This does what a boot does,
# twice: the second pass must compile nothing. Run it after changing a
# build script, a generator recipe or the CI prep.
set -euo pipefail

features="session-desktop/cli,studio-desktop/cli,web-audio-worklet/guitar,web-audio-worklet/keys"
logs="$(mktemp -d)"

prep() {
  ASSETS_OFFLINE="${ASSETS_OFFLINE:-}" just assets >/dev/null
  just faces >/dev/null
}

build() {
  cargo nextest run --workspace --no-run --features "$features" >"$logs/$1.log" 2>&1 \
    || { tail -n 40 "$logs/$1.log"; exit 1; }
  grep -E '^\s+Finished' "$logs/$1.log" | tail -n 1
}

echo "── pass 1: prep + build (cold the first time) ──"
prep
build pass1
echo "── pass 2: prep + build, as a CI boot would ──"
prep
build pass2

# (No mapfile: macOS's bash 3.2 runs this outside the nix shell.)
churned="$(grep -E '^\s+Compiling ' "$logs/pass2.log" | awk '{print $2}' || true)"
if [ -n "$churned" ]; then
  echo "churn: pass 2 recompiled $(printf '%s\n' "$churned" | wc -l | tr -d ' ') crate(s) with nothing changed:"
  printf '%s\n' "$churned" | sed 's/^/  /'
  echo "the first is the cause (a build script watching something the prep rewrites); the rest depend on it"
  exit 1
fi
echo "no churn: a warm CI build compiles only what a change touches"
