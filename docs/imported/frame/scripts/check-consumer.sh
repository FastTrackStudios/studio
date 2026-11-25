#!/usr/bin/env bash
# Build examples/consumer — an app pinning Frame by tag, as the README shows —
# against the commit under test: a copy whose Frame dependencies point at this
# checkout, so the check works before the tag exists and needs no access to
# the (private) repo. Fails when the consumer's [patch] tables drift from
# Frame's, since a consumer must repeat them.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
patches() { sed -n '/^\[patch\.crates-io\]/,/^$/p' "$1"; }
if ! diff <(patches "$root/Cargo.toml") <(patches "$root/examples/consumer/Cargo.toml"); then
  echo "examples/consumer's [patch.crates-io] differs from Frame's (above); repeat Frame's" >&2
  exit 1
fi
# A scratch copy whose Frame dependencies are this checkout (path deps): a
# [patch] of the git source would still make cargo fetch the repo, which CI
# can't read (it's private) and which has no such tag before the release.
work="$root/target/consumer-src"
rm -rf "$work" && mkdir -p "$work"
cp -r "$root/examples/consumer/src" "$work/"
sed -E \
  -e "s#^frame = \{ git = \"[^\"]+\", tag = \"[^\"]+\"#frame = { path = \"$root/crates/frame\"#" \
  -e "s#^frame-architect = \{ git = \"[^\"]+\", tag = \"[^\"]+\"#frame-architect = { path = \"$root/crates/frame-architect\"#" \
  "$root/examples/consumer/Cargo.toml" > "$work/Cargo.toml"
grep -q "path = \"$root/crates/frame\"" "$work/Cargo.toml" || { echo "couldn't point the consumer at this checkout" >&2; exit 1; }
# Start from Frame's lock, as a new consumer should: a fresh resolve picks
# newer crates than the pins were built with (smallvec past what facet-styx
# rc.7 compiles against, 2026-11). Cargo drops what the consumer doesn't use.
cp "$root/Cargo.lock" "$work/Cargo.lock"
cargo check --manifest-path "$work/Cargo.toml" --target-dir "${CARGO_TARGET_DIR:-$root/target}" "$@"
echo "consumer: builds against $(git -C "$root" rev-parse --short HEAD) with Frame's patches"
