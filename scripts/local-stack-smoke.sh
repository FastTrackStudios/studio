#!/usr/bin/env bash
# The local stack, checked end to end (`just dev-smoke`): the stack comes
# up (started here, with the last choice and no watchers, when it is not
# running, and stopped again after), the share link answers, its setlist
# page lists the songs, the node serves Studio's island and every file its
# lock names, and a headless Firefox (scripts/local-stack-smoke.mjs) plays
# a song and sees the playhead move.
#
# SCREENSHOTS=<dir> keeps the browser's shots at 390 px.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
stack="$root/scripts/local-stack.sh"
node_port="${LOCAL_STACK_PORT_NODE:-$(python3 scripts/local-stack.py get ports.node)}"
fail() { echo "smoke: FAILED: $*" >&2; exit 1; }
ok() { echo "smoke: ok: $*" >&2; }

started=0
if ! "$stack" ready node; then
    echo "smoke: the stack is not running: starting it (the last choice, demo, no watchers)" >&2
    choice="$root/.devenv/state/local-stack/choice.env"
    a=pinned k=pinned
    if [ -f "$choice" ]; then
        # shellcheck source=/dev/null
        . "$choice"
        a="${c_architect:-pinned}" k="${c_keyflow:-pinned}"
    fi
    "$stack" choose mode=demo watch=off architect="$a" keyflow="$k"
    devenv up -d mock-oauth auth node >/dev/null
    started=1
    trap 'devenv processes down >/dev/null 2>&1 || true' EXIT
    t0=$(date +%s)
    until "$stack" ready node; do
        [ $(($(date +%s) - t0)) -lt 3600 ] || fail "the node did not answer within an hour (devenv processes logs node)"
        sleep 2
    done
    ok "the stack came up in $(($(date +%s) - t0)) s"
fi

url="$("$stack" url)"
html=""
for _ in $(seq 1 120); do
    html="$(curl -sf "$url" || true)"
    [ -n "$html" ] && break
    sleep 0.5
done
[ -n "$html" ] || fail "the share link $url does not answer"
ok "the share link answers: $url"

# The songs the setlist names, on its page.
setlist="$(python3 scripts/local-stack.py get vault.setlist)"
vault="$root/.devenv/state/local-stack/stack/node/data/vaults/demo"
songs=0
if [ -f "$vault/$setlist" ]; then
    # The page's text, its entities decoded (Rockin&#39; Around …).
    text="$(python3 -c 'import html,sys; print(html.unescape(sys.stdin.read()))' <<<"$html")"
    while IFS= read -r title; do
        grep -qF "$title" <<<"$text" || fail "the setlist page does not list $title"
        songs=$((songs + 1))
    done < <(python3 - "$vault/$setlist" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
fm = text.split("\n---", 1)[0]
inside = False
for line in fm.splitlines():
    if re.match(r"^songs:\s*$", line):
        inside = True
        continue
    if inside:
        if not line.startswith("- "):
            break
        m = re.search(r"\|([^\]]+)\]\]", line)
        if m:
            print(m.group(1).replace("''", "'"))
PY
    )
    ok "the setlist page lists its $songs songs"
else
    songs=1
fi

# Studio's island: named on the page, its lock and every file it names served.
islands="$(grep -o '<script type="application/json" id="site-islands">[^<]*' <<<"$html" | sed 's/^[^>]*>//' || true)"
folder="$(python3 -c 'import json,sys
d=json.loads(sys.argv[1] or "{}")
print(next((i["folder"] for i in d.get("islands",[]) if i.get("id")=="studio.transport"),""))' "$islands")"
[ -n "$folder" ] || fail "the page names no studio.transport island ($islands): is studio-island/ beside the node? (the node's log says why)"
origin="http://127.0.0.1:$node_port"
lock="$(curl -sf "$origin${folder}architect.lock.json")" || fail "the island's lock is not served at ${folder}architect.lock.json"
n=0
while IFS= read -r f; do
    curl -sf -o /dev/null "$origin$folder$f" || fail "the island's $f is not served"
    n=$((n + 1))
done < <(python3 -c 'import json,sys; print("\n".join(json.loads(sys.argv[1])["files"]))' "$lock")
ok "the island ($folder) and the $n files its lock names are served"

# A real browser plays a song.
node="$(command -v node || true)"
if [ -z "$node" ]; then
    SHARE_URL="$url" devenv --profile wasi shell -- env -u LD_LIBRARY_PATH node scripts/local-stack-smoke.mjs
else
    SHARE_URL="$url" env -u LD_LIBRARY_PATH node scripts/local-stack-smoke.mjs
fi
ok "a headless Firefox played a song: the playhead moved and the chart followed"
[ "$started" = 0 ] || echo "smoke: stopping the stack it started" >&2
