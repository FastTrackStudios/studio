# voyager: load the extensions, wait for the engine's session, the handshake (studio#522)
S=$TMPDIR/fts/architect-desktop.sock; A=/Volumes/dev-drive/wt/arch-app-870/target/debug/architect-desktop; P=/tmp/orgrun/daw/pkg
r() { timeout 60 $A actions --app $S run "$@"; }
r EXT_DEV --arg folder=/Volumes/dev-drive/wt/keyflow-chart-session/features/extension | tr -d '\n ' | head -c 160; echo; sleep 4
for e in daw arrangement mixer; do for t in 1 2 3; do o=$(r EXT_DEV --arg folder=$P/$e | tr -d '\n '); echo "$e try $t: ${o:0:160}"; case $o in *'"loaded":true'*|*'"loaded":"'*) break;; esac; sleep 3; done; done
for i in $(seq 60); do s=$(r DAW_STATE 2>/dev/null | python3 -I -c 'import json,sys; s=json.load(sys.stdin).get("session") or {}; print(s.get("url") or "", s.get("id") or "", s.get("path") or "")' 2>/dev/null); [ -n "${s%% *}" ] && break; sleep 1; done
echo "SESSION $s"; set -- $s; URL=$1 ID=$2; echo "$URL $ID" > /tmp/orgrun/daw/session
r EXT_SERVICE_USE --json '{"service":"fts.session","provider":"daw"}' | tr -d '\n ' | head -c 200; echo
r KEYFLOW_SESSION_FOLLOW --json "{\"url\":\"$URL\",\"id\":\"$ID\",\"chart_path\":\"/tmp/orgrun/daw/charts/02 Drive To LA.kf\"}" | tr -d '\n' | head -c 300; echo
r EXT_VIEW_OPEN --json '{"extension":"studio.arrangement","view":"arrangement"}' | tr -d ' \n'; echo
r EXT_VIEW_OPEN --json '{"extension":"studio.mixer","view":"mixer","placement":"split"}' | tr -d ' \n'; echo
r EXT_VIEW_OPEN --json "{\"extension\":\"keyflow\",\"view\":\"chart\",\"state\":\"session:$ID\",\"placement\":\"split\"}" | tr -d ' \n'; echo
r CAPTURE_CLOSE >/dev/null; r GUIDE_TOUR_SKIP >/dev/null
r WORKSPACE_DOCK_SET --arg side=left --arg shown=hidden >/dev/null; r WORKSPACE_DOCK_SET --arg side=right --arg shown=hidden >/dev/null
r DAW_STATE | tr -d '\n' | head -c 600; echo
