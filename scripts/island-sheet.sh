#!/usr/bin/env bash
# Studio's island's stylesheet (apps/architect/island/island.css, studio#445)
# held to the sanitiser the page holds it to: Architect's `cleanSheet`, from
# the architect checkout Cargo.lock pins (features/extensions/web/sanitize.js),
# with the island's own prefix. A sheet it refuses fails here, not as a
# console warning on a share page with the island drawn unstyled.
#
#   scripts/island-sheet.sh            compile island.css (tailwindcss) and check it
#   scripts/island-sheet.sh SHEET      check an already compiled sheet
#
# `just island` runs it on the sheet it built; CI's PR check
# (xtask/workflows/src/pr.rs, job studio-architect) compiles and checks it.
# Needs cargo (`cargo metadata --locked`), python3, node, and tailwindcss
# when compiling.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
meta="$(cargo metadata --locked --format-version 1)"
package_dir() {
    python3 -c 'import json,sys,os; print(next(os.path.dirname(p["manifest_path"]) for p in json.loads(sys.stdin.read())["packages"] if p["name"]==sys.argv[1]))' "$1" <<<"$meta"
}
ext="$(package_dir architect-extensions)"
sheet="${1:-}"
if [ -z "$sheet" ]; then
    # island.css imports Architect's theme from the architect-ui checkout
    # (`just _lumen-link`'s link: a git dep has no stable path).
    ln -sfn "$(package_dir architect-ui)" features/ui/.architect-ui
    sheet="$(mktemp --suffix .css)"
    trap 'rm -f "$sheet"' EXIT
    tailwindcss -i apps/architect/island/island.css -o "$sheet" --minify 2>/dev/null
fi
node scripts/island-sheet-check.mjs "$ext/web/sanitize.js" "$sheet"
