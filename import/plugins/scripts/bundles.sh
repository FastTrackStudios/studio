#!/usr/bin/env bash
# The bundle names ("FTS EQ") of plugins by their short name ("eq"), one per
# line, from bundler.toml's `[<name>-plugin] name = "…"`. Fails on a name
# bundler.toml does not know, so a typo cannot quietly drop a plugin from
# an installer.
#
#   scripts/bundles.sh eq comp     ->  FTS EQ
#                                      FTS Comp
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ $# -gt 0 ] || { echo "bundles.sh: name at least one plugin (eq, comp, …)" >&2; exit 2; }
for p in "$@"; do
    n="$(awk -v want="[$p-plugin]" '
        $0 == want { on = 1; next }
        /^\[/ { on = 0 }
        on && /^name *=/ { sub(/^name *= *"/, ""); sub(/".*$/, ""); print; exit }
    ' "$root/bundler.toml")"
    [ -n "$n" ] || { echo "bundles.sh: bundler.toml has no [$p-plugin] name" >&2; exit 1; }
    printf '%s\n' "$n"
done
