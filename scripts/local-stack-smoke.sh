#!/usr/bin/env bash
# The local stack, checked end to end (`just dev-smoke`): the stack comes
# up (started here, with the last choice and no watchers, when it is not
# running, and stopped again after), the share link answers with the web
# app beside it, the node serves Studio's island, its claims and every
# file its lock names, and a headless Firefox (scripts/local-stack-smoke.mjs)
# opens the link as a guest, sees the playlist list the set's songs, plays
# one and sees its chart follow.
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
    devenv up -d mock-oauth auth node web >/dev/null
    started=1
    trap 'devenv processes down >/dev/null 2>&1 || true' EXIT
    t0=$(date +%s)
    until "$stack" ready node; do
        [ $(($(date +%s) - t0)) -lt 3600 ] || fail "the node did not answer within an hour (devenv processes logs node)"
        sleep 2
    done
    ok "the stack came up in $(($(date +%s) - t0)) s"
fi

# A link's page is the node's and loads the web app from its own origin:
# the link is opened on the web process's port, the stack's front.
web_port="${LOCAL_STACK_PORT_WEB:-$(python3 scripts/local-stack.py get ports.web)}"
origin="http://127.0.0.1:$node_port"
url="$("$stack" url "http://127.0.0.1:$web_port")"
html=""
t0=$(date +%s)
until html="$(curl -sf "$url")" && curl -sf -o /dev/null "http://127.0.0.1:$web_port/app/start.js"; do
    [ $(($(date +%s) - t0)) -lt 3600 ] || fail "the share link $url does not answer with the web app beside it (devenv processes logs web)"
    sleep 2
done
grep -q 'name="architect-share-shell"' <<<"$html" || fail "$url is not the web app's page for a link"
ok "the share link answers with the web app: $url"

# The songs the setlist names: the browser looks for each in the playlist.
setlist="$(python3 scripts/local-stack.py get vault.setlist)"
vault="$root/.devenv/state/local-stack/stack/node/data/vaults/demo"
songs=""
if [ -f "$vault/$setlist" ]; then
    songs="$(python3 - "$vault/$setlist" <<'PY'
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
    )"
fi

# Studio's island: named on the page, its lock and every file it names served.
islands="$(curl -sf "$origin/_ext/islands.json" || true)"
folder="$(python3 -c 'import json,sys
d=json.loads(sys.argv[1] or "{}")
print(next((i["folder"] for i in d.get("islands",[]) if i.get("id")=="studio.transport"),""))' "$islands")"
[ -n "$folder" ] || fail "the node lists no studio.transport island ($islands): is studio-island/ beside the node? (the node's log says why)"
claims="$(python3 -c 'import json,sys
d=json.loads(sys.argv[1])
print(" ".join(sorted(e["view"] for i in d["islands"] if i.get("id")=="studio.transport" for e in i.get("embeds",[]))))' "$islands")"
[ "$claims" = "setlist.playlist song" ] || fail "the island claims \`$claims\`, not the setlist and song notes: the web app would mount none of its views"
lock="$(curl -sf "$origin${folder}architect.lock.json")" || fail "the island's lock is not served at ${folder}architect.lock.json"
n=0
while IFS= read -r f; do
    curl -sf -o /dev/null "$origin$folder$f" || fail "the island's $f is not served"
    n=$((n + 1))
done < <(python3 -c 'import json,sys; print("\n".join(json.loads(sys.argv[1])["files"]))' "$lock")
ok "the island ($folder) claims setlist and song notes, and the $n files its lock names are served"

# A real browser plays a song.
node="$(command -v node || true)"
if [ -z "$node" ]; then
    SHARE_URL="$url" SONGS="$songs" devenv --profile wasi shell -- env -u LD_LIBRARY_PATH node scripts/local-stack-smoke.mjs
else
    SHARE_URL="$url" SONGS="$songs" env -u LD_LIBRARY_PATH node scripts/local-stack-smoke.mjs
fi
ok "a headless Firefox opened the link as a guest, played a song from the set and saw its chart follow"
[ "$started" = 0 ] || echo "smoke: stopping the stack it started" >&2
