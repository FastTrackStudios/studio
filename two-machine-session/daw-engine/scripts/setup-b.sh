# THEBATTLESHIP: $1 = the engine's session id. Follows voyager's engine through the org.
ID=$1; U=ws://192.168.1.10:4142/vox; DOC="Songs/Drive To LA.session"
A=/run/media/Development/scratch/session-org-run/arch/target/debug/architect-desktop; S=/tmp/orgrun/ctl/architect-desktop.sock
IG=$HOME/session-clock-proof/Ignition; LS="/run/media/AudioHaven/Sr. Recital Showcase Project/Sr. Recital Showcase.als"
r() { timeout 60 $A actions --app $S run "$@"; }
j() { python3 -I -c 'import json,sys; print(json.dumps(dict(zip(sys.argv[1::2], sys.argv[2::2]))))' "$@"; }
r EXT_DEV --arg folder=$HOME/session-clock-proof/keyflow/features/extension | tr -d '\n ' | head -c 120; echo
r EXT_DEV --arg folder=$IG/features/lighting/pane | tr -d '\n ' | head -c 120; echo; sleep 6
for i in $(seq 30); do [ -f "/tmp/orgrun/b/vault/$DOC" ] && break; sleep 2; done; ls -la "/tmp/orgrun/b/vault/$DOC"
r KEYFLOW_SESSION_DOCUMENT --json "$(j path "$DOC")" | tr -d '\n' | head -c 300; echo
r KEYFLOW_SESSION_FOLLOW --json "$(j url $U id $ID)" | tr -d '\n' | head -c 300; echo
r EXT_VIEW_OPEN --json "{\"extension\":\"keyflow\",\"view\":\"chart\",\"state\":\"session:$ID\"}" | tr -d ' \n'; echo
r EXT_HOST_CALL --json "{\"id\":\"ignition\",\"request\":{\"op\":\"set_setting\",\"key\":\"ignition.live_set\",\"value\":\"$LS\"}}" | tr -d '\n ' | head -c 120; echo
r IGNITION_SESSION_SONG --json "$(python3 -I -c 'import json,sys; print(json.dumps({"doc":sys.argv[1],"lane":"dtla","live_set":sys.argv[2],"locator":"Drive To LA","cues":sys.argv[3],"tempo":False}))' "$ID" "$LS" "$IG/features/lighting/data/songs/jt-recital/drive-to-la.json")" | tr -d '\n' | head -c 300; echo
r IGNITION_CAMERA_PICK --arg camera=Wide | tr -d '\n ' | head -c 120; echo
r IGNITION_SESSION_ROOM --json "$(j url $U doc $ID song drive-to-la lane dtla)" | tr -d '\n' | head -c 300; echo
r EXT_VIEW_OPEN --json '{"extension":"ignition","view":"room","placement":"split"}' | tr -d ' \n'; echo
r EXT_VIEW_OPEN --json '{"extension":"ignition","view":"cues","placement":"split"}' | tr -d ' \n'; echo
r CAPTURE_CLOSE >/dev/null; r GUIDE_TOUR_SKIP >/dev/null
r WORKSPACE_DOCK_SET --arg side=left --arg shown=hidden >/dev/null; r WORKSPACE_DOCK_SET --arg side=right --arg shown=hidden >/dev/null
r KEYFLOW_STATE | tr -d '\n ' | head -c 400; echo
