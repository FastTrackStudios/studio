#!/usr/bin/env bash
# What a Studio release workflow's `sums` job signs (xtask/workflows/src/
# sums.rs): ONLY digests it can vouch for, never whatever is on the (draft)
# Release when it runs.
#
#   scripts/release-sums.sh plan OWN ASSETS.json [PREVIOUS]   prints SHA256SUMS, or refuses
#
# OWN: "<sha256>  <name>" lines this workflow's build jobs reported (job
# outputs), plus release-facts.json as `release.sh facts <tag>` computes it
# from the tag. PREVIOUS: the SHA256SUMS already on the Release, passed in
# only after its signature verified with the org key and its trusted comment
# named this tag (several workflows attach to Studio's one Release, each
# signing after its own builds; the last has seen them all).
# ASSETS.json: the Release's assets as GitHub reports them, [{"name","digest"}].
#
# Refuses (exit 1, reasons on stderr) when an asset it vouches for (OWN or
# PREVIOUS) has another digest on the Release (replaced after upload), when
# one of OWN is missing, or when OWN and PREVIOUS disagree. Any other asset
# (another workflow's, not built yet, or one nobody built) is left OUT of the
# sums and named on stderr: it is never signed. `.sha256` sidecars are
# checked but not listed. `scripts/release-sums-test.sh` tests it.
set -euo pipefail

die() { echo "release-sums: $*" >&2; exit 1; }
if [ "${1:-}" != plan ] || [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then die "usage: release-sums.sh plan OWN ASSETS.json [PREVIOUS]"; fi
own="$2" assets="$3" previous="${4:-/dev/null}"

declare -A want=() mine=() have=()
add() { # <file> <is-own>
    local hex name
    while read -r hex name; do
        [ -n "${name:-}" ] || continue
        name="${name#\*}"
        [[ "$hex" =~ ^[0-9a-f]{64}$ ]] || die "'$hex' for $name is not a sha256"
        if [ -n "${want[$name]:-}" ] && [ "${want[$name]}" != "$hex" ]; then
            die "$name: this workflow reported $hex, the signed SHA256SUMS already says ${want[$name]}"
        fi
        want[$name]="$hex"
        if [ "$2" = 1 ]; then mine[$name]=1; fi
    done <"$1"
}
add "$previous" 0
add "$own" 1
[ "${#mine[@]}" -gt 0 ] || die "this workflow reported no asset"

while IFS=$'\t' read -r name digest; do
    case "$name" in SHA256SUMS | SHA256SUMS.minisig) continue ;; esac
    have[$name]="${digest#sha256:}"
done < <(jq -r '.[] | [.name, (.digest // "")] | @tsv' "$assets")

bad=0
for name in "${!want[@]}"; do
    if [ -z "${have[$name]+x}" ]; then
        if [ -n "${mine[$name]:-}" ]; then echo "release-sums: $name (this workflow's) is not on the Release" >&2; bad=1
        else echo "release-sums: $name (signed before) is gone from the Release" >&2; bad=1; fi
    elif [ "${have[$name]}" != "${want[$name]}" ]; then
        echo "release-sums: $name is ${have[$name]:-without a digest} on the Release, ${want[$name]} was built" >&2; bad=1
    fi
done
for name in "${!have[@]}"; do
    [ -n "${want[$name]+x}" ] || echo "release-sums: $name is on the Release but not vouched for here: left unsigned" >&2
done
[ "$bad" = 0 ] || die "refusing to sign"

for name in "${!want[@]}"; do
    case "$name" in *.sha256) continue ;; esac
    printf '%s  %s\n' "${want[$name]}" "$name"
done | LC_ALL=C sort -k2
