#!/usr/bin/env bash
# shellcheck disable=SC2015,SC2329 # ok() never fails; cleanup runs from the EXIT trap
# The release MECHANICS against a throwaway draft, never a real release
# (release-dry-run.yml runs it on PRs that touch them, and by hand):
#   draft -> uploads -> scripts/release-sums.sh -> sign -> publish
# with the cases that matter:
#   1. a workflow signs what it built, an injected asset is refused, then
#      left unsigned once removed;
#   2. an asset replaced after upload is refused;
#   3. a second workflow builds on the first one's VERIFIED sums;
#   4. publish: only the draft, never Latest (the name is not a stable tag);
#   5. a re-run on the published release: nothing re-published, Latest untouched.
# The draft is named for the run (dry-run-<id>), signed with an ephemeral
# key, and deleted with its tag at the end whatever happens.
# Needs gh (GH_TOKEN with contents: write), jq, minisign.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="${GITHUB_REPOSITORY:?}"
name="dry-run-${GITHUB_RUN_ID:-local$$}-${GITHUB_RUN_ATTEMPT:-1}"
[[ "$name" != v* ]] || { echo "refusing: $name looks like a release tag" >&2; exit 1; }
t="$(mktemp -d)"
fail=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*" >&2; fail=1; }
cleanup() {
    gh release delete "$name" --repo "$repo" --yes --cleanup-tag >/dev/null 2>&1 || true
    rm -rf "$t"
}
trap cleanup EXIT

minisign -G -W -p "$t/key.pub" -s "$t/key.sec" >/dev/null
pub="$(sed -n 2p "$t/key.pub")"

gh release create "$name" --repo "$repo" --draft --title "$name (dry run, deleted at the end)" \
    --notes "Dry run of the release mechanics (scripts/release-dry-run.sh). Not a release." \
    --target "${GITHUB_SHA:-$(git rev-parse HEAD)}" >/dev/null
id="$(gh api --paginate "repos/$repo/releases?per_page=100" --jq ".[] | select(.tag_name == \"$name\") | .id" | head -1)"
[ -n "$id" ] || { echo "no draft $name" >&2; exit 1; }
assets() { gh api "repos/$repo/releases/$id/assets?per_page=100" --jq '[.[] | {name, digest}]'; }
upload() { gh release upload "$name" --repo "$repo" --clobber "$@" >/dev/null; }
sign() { # <sums>
    minisign -S -s "$t/key.sec" -m "$1" -x "$1.minisig" -t "studio $name SHA256SUMS" </dev/null >/dev/null
    cp "$1" "$t/SHA256SUMS"; cp "$1.minisig" "$t/SHA256SUMS.minisig"
    upload "$t/SHA256SUMS" "$t/SHA256SUMS.minisig"
}

# Workflow A builds two files.
mkdir -p "$t/a" "$t/b"
echo node > "$t/a/node.tar.gz"; echo web > "$t/a/web.tar.gz"
upload "$t/a/node.tar.gz" "$t/a/web.tar.gz"
(cd "$t/a" && sha256sum node.tar.gz web.tar.gz) > "$t/own-a"

# 1. Someone injects an asset: signed? No. Left unsigned.
echo evil > "$t/evil.bin"; upload "$t/evil.bin"
assets > "$t/assets.json"
if out="$(bash "$here/release-sums.sh" plan "$t/own-a" "$t/assets.json" 2>"$t/err")" \
   && ! grep -q evil <<<"$out" && grep -q "evil.bin.*left unsigned" "$t/err"; then
    ok "an injected asset is never signed (named, left out)"
else no "an injected asset: $out $(cat "$t/err")"; fi
gh release delete-asset "$name" evil.bin --repo "$repo" --yes >/dev/null

# 2. A's own asset replaced after upload: refused.
echo tampered > "$t/a2"; cp "$t/a2" "$t/node.tar.gz"; upload "$t/node.tar.gz"
assets > "$t/assets.json"
if bash "$here/release-sums.sh" plan "$t/own-a" "$t/assets.json" >/dev/null 2>&1; then no "a replaced asset was signed"
else ok "an asset replaced after its upload is refused"; fi
upload "$t/a/node.tar.gz"

# A signs.
assets > "$t/assets.json"
bash "$here/release-sums.sh" plan "$t/own-a" "$t/assets.json" > "$t/sums-a" && sign "$t/sums-a" \
    && ok "workflow A: signed $(wc -l < "$t/sums-a") assets" || no "workflow A could not sign"

# 3. Workflow B builds one file, on A's verified sums.
echo dmg > "$t/b/Studio.dmg"; upload "$t/b/Studio.dmg"
(cd "$t/b" && sha256sum Studio.dmg) > "$t/own-b"
gh release download "$name" --repo "$repo" --dir "$t/prev" -p SHA256SUMS -p SHA256SUMS.minisig >/dev/null
if [ "$(sed -n 3p "$t/prev/SHA256SUMS.minisig")" = "trusted comment: studio $name SHA256SUMS" ] \
   && minisign -V -q -P "$pub" -m "$t/prev/SHA256SUMS" -x "$t/prev/SHA256SUMS.minisig" >/dev/null; then
    assets > "$t/assets.json"
    out="$(bash "$here/release-sums.sh" plan "$t/own-b" "$t/assets.json" "$t/prev/SHA256SUMS")" \
        && [ "$(wc -l <<<"$out")" = 3 ] && ok "workflow B: signs its own + A's verified sums (3 assets)" || no "workflow B: $out"
    printf '%s\n' "$out" > "$t/sums-b"; sign "$t/sums-b"
else no "A's sums do not verify"; fi

# 4. Publish: the draft, never Latest (not the highest stable tag).
highest="$(git tag --list 'v[0-9]*' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -1)"
draft="$(gh api "repos/$repo/releases/$id" --jq .draft)"
decision="$(bash "$here/release-publish.sh" decide "$draft" "$name" "${highest:-v0.0.0}")"
[ "$decision" = publish ] && ok "the draft is published, not Latest ($decision)" || no "decision on the draft: $decision"
latest_before="$(gh api "repos/$repo/releases/latest" --jq .tag_name 2>/dev/null || echo none)"
gh release edit "$name" --repo "$repo" --draft=false --latest=false >/dev/null
[ "$(gh api "repos/$repo/releases/latest" --jq .tag_name 2>/dev/null || echo none)" = "$latest_before" ] \
    && ok "Latest is still $latest_before" || no "Latest moved"

# 5. A re-run on the published release: leave it.
draft="$(gh api "repos/$repo/releases/$id" --jq .draft)"
decision="$(bash "$here/release-publish.sh" decide "$draft" "$name" "${highest:-v0.0.0}")"
[ "$decision" = leave ] && ok "a re-run leaves the published release and Latest alone" || no "re-run decision: $decision"

exit "$fail"
