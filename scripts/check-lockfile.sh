#!/usr/bin/env bash
# The committed Cargo.lock is what the manifests resolve to (`just ci`, CI's
# Gates and PR checks).
#
# Not `cargo --locked`: cargo writes the lockfile's [[patch.unused]]
# trailer in HashMap order (one group per patched source URL), so the same
# inputs serialise differently from one run to the next and `--locked` fails
# at random. Instead: resolve, then diff the committed lockfile against the
# written one, with the trailer compared as a sorted set. The committed copy
# is put back either way.
set -euo pipefail
cd "$(dirname "$0")/.."
committed="$(mktemp)"
trap 'cp "$committed" Cargo.lock; rm -f "$committed"' EXIT
cp Cargo.lock "$committed"
cargo metadata --format-version 1 >/dev/null
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
    echo "::error::Cargo.lock is out of date: run cargo metadata and commit the result"
    exit 1
fi
echo "Cargo.lock: up to date"
