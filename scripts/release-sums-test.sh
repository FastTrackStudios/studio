#!/usr/bin/env bash
# shellcheck disable=SC2015 # echo cannot fail: A && ok || fail is if-then-else here
# release-sums.sh, hermetic (`just release-sums-test`, PR checks).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
h() { printf "$1%.0s" {1..64}; }
a=$(h a) b=$(h b) c=$(h c) d=$(h d) f=$(h f) x=$(h e)
printf '%s  node.tar.gz\n%s  node.tar.gz.sha256\n%s  release-facts.json\n' "$a" "$b" "$f" > "$t/own"
printf '%s  Studio.dmg\n' "$d" > "$t/prev"
assets() { jq -n --argjson extra "${1:-[]}" '[{"name":"node.tar.gz","digest":"sha256:'"$a"'"},{"name":"node.tar.gz.sha256","digest":"sha256:'"$b"'"},{"name":"release-facts.json","digest":"sha256:'"$f"'"},{"name":"Studio.dmg","digest":"sha256:'"$d"'"},{"name":"SHA256SUMS","digest":"sha256:'"$c"'"}] + $extra'; }
fail=0
ok() { echo "ok   $1"; }
no() { echo "FAIL $1" >&2; fail=1; }
run() { bash "$here/release-sums.sh" plan "$t/own" "$t/a.json" "$@" 2>"$t/err"; }

assets > "$t/a.json"
out="$(run "$t/prev")" && [ "$out" = "$(printf '%s  Studio.dmg\n%s  node.tar.gz\n%s  release-facts.json' "$d" "$a" "$f")" ] \
    && ok "signs its own assets and the previously signed ones" || no "the good case: $out"
assets '[{"name":"AppImage","digest":"sha256:'"$x"'"}]' > "$t/a.json"
out="$(run "$t/prev")" && ! grep -q AppImage <<<"$out" && grep -q "AppImage.*left unsigned" "$t/err" \
    && ok "leaves an asset nobody here vouches for unsigned (another workflow's, or injected)" || no "an unvouched asset was signed"
assets | jq '(.[] | select(.name == "release-facts.json") | .digest) = "sha256:'"$c"'"' > "$t/a.json"
run "$t/prev" >/dev/null && no "replaced facts were signed" || ok "refuses its own asset replaced after upload (facts)"
assets | jq '(.[] | select(.name == "Studio.dmg") | .digest) = "sha256:'"$c"'"' > "$t/a.json"
run "$t/prev" >/dev/null && no "a replaced, previously signed asset was signed" || ok "refuses a previously signed asset replaced since"
assets | jq 'map(select(.name != "node.tar.gz"))' > "$t/a.json"
run "$t/prev" >/dev/null && no "a missing own asset was accepted" || ok "refuses when its own asset is missing"
printf '%s  node.tar.gz\n' "$c" > "$t/prev2"; assets > "$t/a.json"
run "$t/prev2" >/dev/null && no "a disagreement with the signed sums was accepted" || ok "refuses when it disagrees with the signed SHA256SUMS"
exit "$fail"
