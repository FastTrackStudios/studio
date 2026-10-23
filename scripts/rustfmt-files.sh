#!/usr/bin/env bash
# Format exactly the given Rust files (prek's rustfmt hook passes the
# staged ones), each through rustfmt's stdin. `rustfmt <file>` would
# follow `mod` declarations into files that aren't staged; stdin never
# does. The edition is the owning crate's (the nearest Cargo.toml's
# `edition`, or the workspace's for `edition.workspace = true`), so the
# result is what `cargo fmt --check` expects.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

workspace_edition="$(sed -n 's/^edition *= *"\([0-9]*\)".*/\1/p' Cargo.toml | head -1)"

edition_of() { # file
  local dir
  dir="$(dirname "$1")"
  while [ "$dir" != "." ] && [ "$dir" != "/" ] && [ ! -f "$dir/Cargo.toml" ]; do
    dir="$(dirname "$dir")"
  done
  local own=""
  [ -f "$dir/Cargo.toml" ] && own="$(sed -n 's/^edition *= *"\([0-9]*\)".*/\1/p' "$dir/Cargo.toml" | head -1)"
  echo "${own:-${workspace_edition:-2024}}"
}

rc=0
for f in "$@"; do
  tmp="$(mktemp)"
  edition="$(edition_of "$f")"
  if rustfmt --edition "$edition" --emit stdout <"$f" >"$tmp"; then
    cmp -s "$f" "$tmp" || cat "$tmp" >"$f"
  else
    echo "rustfmt: could not format $f" >&2
    rc=1
  fi
  rm -f "$tmp"
done
exit "$rc"
