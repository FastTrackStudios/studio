# One recorded run: voyager's engine leads; THEBATTLESHIP follows. Writes /tmp/orgrun/daw/media.
M=/tmp/orgrun/daw/media4; A=/Volumes/dev-drive/wt/arch-app-870/target/debug/architect-desktop; S=$TMPDIR/fts/architect-desktop.sock
r() { timeout 30 $A actions --app $S run "$@"; }
B() { timeout 60 ssh -o BatchMode=yes cody@thebattleship.local bash -s; }
shot() { # $1 tag
  r APP_SHOT --arg path=$M/voyager-$1.png >/dev/null & p1=$!
  echo "export WAYLAND_DISPLAY=wayland-1 XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus; timeout 30 spectacle -b -n -f -o /tmp/orgrun/take4-thebattleship-$1.png" | B & p2=$!
  wait $p1 $p2; echo "== $1 $(date +%T)"; bash /tmp/orgrun/daw/read.sh | sed 's/"charts":\[\],//' | grep -oE '^[a-zA-Z ]+:|"time":[0-9.]+|"shown_seconds":[0-9.]+|"playing":(true|false)' | tr '\n' ' '; echo
}
echo 'export WAYLAND_DISPLAY=wayland-1 XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus; timeout 60 spectacle -b -n -f -o /tmp/orgrun/warm.png' | B; echo warmed
screencapture -x -V 40 $M/voyager-run.mov &
sleep 2;  r DAW_RUN --json '{"id":"SESSION_PLAY_FROM","args":{"at":0}}' >/dev/null; echo "play 0 at $(date +%T)"
sleep 7;  shot 1-play-from-0
sleep 1;  r DAW_RUN --json '{"id":"SESSION_SEEK","args":{"at":66}}' >/dev/null; echo "seek 66 at $(date +%T)"
sleep 3;  shot 2-seek-66s-interlude
sleep 3;  r DAW_RUN --json '{"id":"SESSION_SEEK","args":{"at":93}}' >/dev/null; echo "seek 93 at $(date +%T)"
sleep 3;  shot 3-seek-93s-prechorus-2
sleep 3;  r DAW_RUN --json '{"id":"SESSION_STOP","args":{}}' >/dev/null; echo "stop at $(date +%T)"
sleep 2;  shot 4-stopped
wait
timeout 60 scp -q 'cody@thebattleship.local:/tmp/orgrun/take4-thebattleship-*.png' $M/ && echo fetched
