# Reads voyager's engine (PLAYHEAD_STATE) and both machines' KEYFLOW_SESSION_STATE, one after the other.
A=/Volumes/dev-drive/wt/arch-app-870/target/debug/architect-desktop; S=$TMPDIR/fts/architect-desktop.sock
echo "voyager engine: $(timeout 20 $A actions --app $S run PLAYHEAD_STATE | tr -d '\n ' | head -c 400)"
echo "voyager chart:  $(timeout 20 $A actions --app $S run KEYFLOW_SESSION_STATE | tr -d '\n ' | head -c 700)"
echo 'A=/run/media/Development/scratch/session-org-run/arch/target/debug/architect-desktop; S=/tmp/orgrun/ctl/architect-desktop.sock; echo "B chart:        $(timeout 20 $A actions --app $S run KEYFLOW_SESSION_STATE | tr -d "\n " | head -c 700)"' | timeout 30 ssh -o BatchMode=yes cody@thebattleship.local bash -s
