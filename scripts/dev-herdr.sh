#!/usr/bin/env bash
# The whole Studio dev loop in one herdr workspace, "Studio Dev" (`just dev`):
#
# Tab "app":
#   engine     `studio engine --reopen`, a process of its own. Ctrl-C and ↑
#              Enter restart it; it opens what it had open.
#   ui         Studio's window under `dx serve`: a save rebuilds and
#              relaunches the window. It only attaches to an engine
#              (FTS_ENGINE=attach): it never stops one, it waits for the next
#              when the engine restarts, and the engine keeps playing while
#              the window rebuilds.
#   task       with `local`: a dev Task server, so nothing touches production.
#   telemetry  teley's terminal view (`teley-cli --local`): the engine's and
#              the window's traces, logs and metrics, live.
#   Laid out as engine over ui (over task) on the left, where little is
#   printed — with telemetry on, the terminals keep warnings and errors and
#   dx's status stays at the top — and telemetry on the right.
#
# Tab "telemetry":
#   collector  the OpenTelemetry Collector on 4318 (OTLP/HTTP's own port;
#              FTS_OTLP_PORT), where the engine and the window send. It hands
#              every signal to the terminal view (4319) and to teley's web
#              relay (8787), in teley's own room: the dashboard shows it, and
#              so does every coding agent's teley (a user-scope MCP server,
#              `bunx teley-cli mcp --host localhost:8787`, subscribing to
#              the same room — as many sessions as there are, nothing to
#              reconnect), so the agents read what you are watching.
#   feed       every signal as JSON lines in target/dev-telemetry/feed.jsonl
#              (a local teley on 4321, FTS_FEED_PORT): what an agent reads
#              (`scripts/drum-lab.sh report`).
#   web        teley's web dashboard and its ingest, served locally by its
#              worker (`wrangler dev`) from a checkout of teley (TELEY_DIR,
#              default ../../teley; clone github.com/logaretm/teley there).
#              Its address is printed: one room, the same every run —
#              teley's (~/.teley/session.json), which the agents' teley
#              subscribes to. It opens as a desktop app of its own
#              (scripts/teley-window.py: a WebKitGTK window, app id
#              `dev.fts.teley`, no browser around it), once: a later run
#              finds it still open and leaves it, and it reconnects to the
#              relay by itself.
#   Nothing is sent to teley.dev.
#
# Usage: scripts/dev-herdr.sh [local] [--fresh] [--no-telemetry] [--no-dashboard]
#   local          run a dev Task server too, and point Studio at it
#   --fresh        close an earlier "Studio Dev" workspace first
#   --no-telemetry leave the collector and teley out (the terminals then
#                  print everything)
#   --no-dashboard do not open the dashboard window (`o` in the links pane
#                  still does)
#
# Watch it: `herdr --session studio` (FTS_HERDR_SESSION picks another).
# The herdr server is started headless when it is not running. Closing the
# workspace stops everything in it.
#
# Local Task needs a dev home (FTS_DEV_HOME, default ../../fts-dev from the
# repo) with `task-server.sh` (the server's data root, media volume and
# sign-in), and optionally `accounts/studio-cody.json` (the dev account
# Studio signs in as) and `profiles/` (config dirs of its own, so your real
# Studio settings are never touched). Task's checkout: TASK_DIR, default
# ../../task.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dev_root="$(cd "$repo/../.." && pwd)"
session="${FTS_HERDR_SESSION:-studio}"
label="Studio Dev"
local_task=0
fresh=0
telemetry=1
dashboard=1
for arg in "$@"; do
    case "$arg" in
        local) local_task=1 ;;
        --fresh) fresh=1 ;;
        --no-telemetry) telemetry=0 ;;
        --no-dashboard) dashboard=0 ;;
        *) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
    esac
done

h() { herdr --session "$session" "$@"; }
need() { command -v "$1" >/dev/null || { echo "dev: needs $1 on PATH" >&2; exit 1; }; }
pane_of() { jq -r '.result.pane.pane_id'; }
need herdr
need jq

# The session's server, headless, when it is not up yet.
if ! h workspace list >/dev/null 2>&1; then
    (setsid nohup herdr --session "$session" server > /tmp/herdr-"$session".log 2>&1 < /dev/null &)
    for _ in $(seq 1 40); do h workspace list >/dev/null 2>&1 && break; sleep 0.25; done
    h workspace list >/dev/null 2>&1 || { echo "dev: herdr --session $session did not start (see /tmp/herdr-$session.log)" >&2; exit 1; }
    echo "dev: started herdr --session $session"
fi

# An earlier one (`studio-dev` is what it was called before).
existing="$(h workspace list | jq -r --arg l "$label" '.result.workspaces[] | select(.label == $l or .label == "studio-dev") | .workspace_id')"
if [ -n "$existing" ]; then
    if [ "$fresh" = 0 ]; then
        echo "dev: \"$label\" is already running in herdr --session $session; --fresh replaces it"
        exit 0
    fi
    for w in $existing; do
        h workspace close "$w" >/dev/null
        echo "dev: closed the earlier workspace ($w)"
    done
fi

# Another dx serve of the app (an earlier run) would fight this one over
# every save; found by process name, never `pkill -f` (it matches this
# script's own command line).
for g in $(ps -eo pgid=,comm=,args= | awk '$2 ~ /dx-wrapped|^dx$/ && $0 ~ /-p studio-desktop/ {print $1}' | sort -u); do
    kill -- "-$g" 2>/dev/null && echo "dev: stopped an earlier dx serve of studio-desktop (group $g)"
done

# What the engine and the window share: the Task server, the account, the
# config dirs their discovery files live in. Production Task by default.
envs=(RUST_BACKTRACE=1)
if [ "$local_task" = 1 ]; then
    dev_home="${FTS_DEV_HOME:-$dev_root/fts-dev}"
    task_dir="${TASK_DIR:-$dev_root/task}"
    [ -x "$dev_home/task-server.sh" ] || { echo "dev: no $dev_home/task-server.sh (FTS_DEV_HOME)" >&2; exit 1; }
    [ -d "$task_dir" ] || { echo "dev: no Task checkout at $task_dir (TASK_DIR)" >&2; exit 1; }
    envs+=(FTS_TASK_SERVER=http://127.0.0.1:9090)
    [ -f "$dev_home/accounts/studio-cody.json" ] && envs+=(FTS_ACCOUNT_FILE="$dev_home/accounts/studio-cody.json")
    profile="$dev_home/profiles/studio"
    mkdir -p "$profile/config" "$profile/data" "$profile/cache"
    envs+=(XDG_CONFIG_HOME="$profile/config" XDG_DATA_HOME="$profile/data" XDG_CACHE_HOME="$profile/cache")
fi
envs+=(FTS_TASK_ORG="${FTS_TASK_ORG:-fasttrackaudio}")

# Telemetry. Teley takes OTLP at one URL per room, `/r/<room>` with nothing
# after it, so the collector names each signal's URL for each viewer. The
# terminal view's room is teley's own (the same every run, read from a
# moment's `--json` run); so is the relay's, with teley's receive token
# (~/.teley/session.json): the room `teley-cli mcp --host` subscribes to,
# so the dashboard and every agent share it. The first to subscribe with
# the token claims it. The apps send to the collector at the standard
# paths.
otlp_port="${FTS_OTLP_PORT:-4318}"
tui_port=4319
# The agents' feed: a local teley that keeps every signal as JSON lines in
# target/dev-telemetry/feed.jsonl — what a model reads traces from without
# the relay (whose room a dashboard tab with another token can claim).
feed_port="${FTS_FEED_PORT:-4321}"
web_port="${FTS_TELEY_WEB_PORT:-8787}"
tel="$repo/target/dev-telemetry"
teley_dir="${TELEY_DIR:-$dev_root/teley}"
web=0
if [ "$telemetry" = 1 ]; then
    need bunx
    room="$(timeout 60 bunx teley-cli --local --port 0 --json 2>/dev/null | head -1 | jq -r '.room_id // empty' || true)"
    otelcol="$(timeout 600 nix build --no-link --print-out-paths nixpkgs#opentelemetry-collector-contrib 2>/dev/null | head -1)/bin/otelcol-contrib"
    if [ -z "$room" ] || [ ! -x "$otelcol" ]; then
        echo "dev: no teley room or no collector; running without telemetry" >&2
        telemetry=0
    fi
fi
if [ "$telemetry" = 1 ]; then
    mkdir -p "$tel"
    # The background collector earlier runs started.
    if [ -f "$tel/otelcol.pid" ]; then
        kill "$(cat "$tel/otelcol.pid")" 2>/dev/null || true
        rm -f "$tel/otelcol.pid"
    fi
    if [ -d "$teley_dir/workers" ] && command -v pnpm >/dev/null; then
        web=1
        # teley's session, which the `--json` run above made if it was not
        # there.
        web_room="$(jq -r '.roomId // empty' ~/.teley/session.json 2>/dev/null)"
        web_token="$(jq -r '.receiveToken // empty' ~/.teley/session.json 2>/dev/null)"
        if [ -z "$web_room" ] || [ -z "$web_token" ]; then
            echo "dev: no teley session (~/.teley/session.json) — the web dashboard is left out" >&2
            web=0
        fi
    else
        echo "dev: no teley checkout at $teley_dir (TELEY_DIR) — the web dashboard is left out" >&2
    fi
    exporter() { # <name> <port> <room>
        printf '  otlp_http/%s:\n' "$1"
        for signal in traces logs metrics; do
            printf '    %s_endpoint: http://127.0.0.1:%s/r/%s\n' "$signal" "$2" "$3"
        done
        # Queued, so a viewer that is not up (the relay, restarting)
        # loses only its own copy: sent in step, its failure
        # failed the request and the apps saw 503s. Not retried: a viewer
        # wants now, not a backlog.
        printf '    retry_on_failure: {enabled: false}\n    sending_queue: {enabled: true, queue_size: 200}\n'
    }
    to="otlp_http/tui, otlp_http/feed"
    [ "$web" = 1 ] && to="$to, otlp_http/web"
    {
        printf 'receivers:\n  otlp:\n    protocols:\n      http: {endpoint: "127.0.0.1:%s"}\n' "$otlp_port"
        printf 'exporters:\n'
        exporter tui "$tui_port" "$room"
        exporter feed "$feed_port" "$room"
        [ "$web" = 1 ] && exporter web "$web_port" "$web_room"
        # Every call's rate and timings come from the apps themselves
        # (architect records `rpc.server.duration` per service and method),
        # in the Metrics view, whatever is kept of their traces. A quick
        # call is not even sent as a span (architect-telemetry's
        # `QuietCalls`: under 5 ms, clean and without a warning, it is the
        # metric alone), and a call made inside a span — a song read back,
        # a song opened — is that span's child, across the link. The traces
        # kept (`tail_sampling`, a trace at a time): any with an error, any
        # that took 5 ms or more. A kept trace's late spans (the waveforms
        # coming in behind a read) follow it through the sampled-decision
        # cache. Batched last: a viewer gets one request every two seconds,
        # not one per span — teley's web worker under `wrangler dev` falls
        # over under a request per call. The relay takes the metrics every
        # 15 s (`interval`: each series' latest, a window's worth at once):
        # at the apps' 5 s, broadcast a point at a time, `wrangler dev`'s
        # proxy fell over every twenty seconds. The terminal view keeps
        # every point.
        printf 'processors:\n  batch: {timeout: 2s, send_batch_size: 4096}\n'
        printf '  interval: {interval: 15s}\n'
        printf '  tail_sampling:\n    decision_wait: 5s\n    decision_cache: {sampled_cache_size: 100000}\n    policies:\n'
        printf '      - {name: errors, type: status_code, status_code: {status_codes: [ERROR]}}\n'
        printf '      - {name: slow, type: latency, latency: {threshold_ms: 5}}\n'
        printf 'service:\n  telemetry: {logs: {level: warn}, metrics: {level: none}}\n  pipelines:\n'
        printf '    traces: {receivers: [otlp], processors: [tail_sampling, batch], exporters: [%s]}\n' "$to"
        printf '    metrics: {receivers: [otlp], processors: [batch], exporters: [otlp_http/tui, otlp_http/feed]}\n'
        [ "$web" = 1 ] && printf '    metrics/web: {receivers: [otlp], processors: [interval, batch], exporters: [otlp_http/web]}\n'
        printf '    logs: {receivers: [otlp], processors: [batch], exporters: [%s]}\n' "$to"
    } > "$tel/otelcol.yaml"
    envs+=(OTEL_EXPORTER_OTLP_ENDPOINT="http://127.0.0.1:$otlp_port")
    # Metrics every five seconds, not the SDK's minute: the frame rate and
    # the audio's load read as a line over time, not a point a minute.
    envs+=(OTEL_METRIC_EXPORT_INTERVAL=5000)
fi
env_line="env $(printf '%q ' "${envs[@]}")"

# One engine per profile: another (an earlier run, `fts-dev/run.sh engine`)
# holds the audio device and the Action List's port. Each says where it is
# in fts/engines/; stopped with SIGTERM, which has it note what it had open
# for this run's `--reopen`.
# shellcheck disable=SC2016 # expanded by the inner sh, under envs
engines_dir="$(env "${envs[@]}" sh -c 'echo "${XDG_CONFIG_HOME:-$HOME/.config}"')/fts/engines"
for f in "$engines_dir"/studio-*.json; do
    [ -f "$f" ] || continue
    pid="$(jq -r '.pid // empty' "$f" 2>/dev/null)"
    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        continue
    fi
    kill "$pid" && echo "dev: stopped the engine already running (pid $pid)"
    for _ in $(seq 1 40); do kill -0 "$pid" 2>/dev/null || break; sleep 0.25; done
done

# A dev Task server already up (an earlier run, fts-dev's own) is used as
# it is: a second one would not get the port.
task_up=0
if [ "$local_task" = 1 ] && curl -s -o /dev/null --max-time 2 http://127.0.0.1:9090/; then
    task_up=1
    echo "dev: a Task server already answers on 127.0.0.1:9090; using it"
fi

# The `studio` CLI the engine runs: release-fast, the one feature set the
# app's recipes share (another recompiles every shared crate).
cli="$repo/target/release-fast/studio"
build_cli="cargo build --profile release-fast -p studio-desktop --features cli,signal-keys-rig --bin studio"

# ── Tab "telemetry", first: the collector is up before anything sends ──
ws="$(h workspace create --label "$label" --cwd "$repo" --no-focus)"
ws_id="$(jq -r '.result.workspace.workspace_id' <<<"$ws")"
first_tab="$(jq -r '.result.tab.tab_id' <<<"$ws")"
first_pane="$(jq -r '.result.root_pane.pane_id' <<<"$ws")"
collector_pane="" web_pane="" task_pane="" telemetry_pane=""
if [ "$telemetry" = 1 ]; then
    h tab rename "$first_tab" telemetry >/dev/null
    collector_pane="$first_pane"
    h pane rename "$collector_pane" collector >/dev/null
    h pane run "$collector_pane" "$otelcol --config $tel/otelcol.yaml"
    feed_pane="$(h pane split "$collector_pane" --direction down --ratio 0.85 --cwd "$repo" --no-focus | pane_of)"
    h pane rename "$feed_pane" feed >/dev/null
    h pane run "$feed_pane" "while true; do bunx teley-cli --local --port $feed_port --json >> $tel/feed.jsonl; echo 'feed: stopped; starting again'; sleep 2; done"
    if [ "$web" = 1 ]; then
        web_pane="$(h pane split "$collector_pane" --direction right --cwd "$teley_dir" --no-focus | pane_of)"
        h pane rename "$web_pane" web >/dev/null
        # Its dependencies and the dashboard's static build, the first time.
        # Started again if it stops: its dev server can fall over.
        h pane run "$web_pane" "{ [ -d node_modules ] || pnpm install; } && { [ -d web/.output/public ] || pnpm --dir web generate; } && cd workers && while true; do pnpm exec wrangler dev --port $web_port --log-level warn; echo 'web: stopped; starting again'; sleep 2; done"
    fi
    app="$(h tab create --workspace "$ws_id" --label app --cwd "$repo" --focus)"
    app_tab="$(jq -r '.result.tab.tab_id' <<<"$app")"
    engine_pane="$(jq -r '.result.root_pane.pane_id // empty' <<<"$app")"
    if [ -z "$engine_pane" ]; then
        engine_pane="$(h pane list --workspace "$ws_id" | jq -r --arg t "$app_tab" '.result.panes[] | select(.tab_id == $t) | .pane_id' | head -1)"
    fi
else
    app_tab="$first_tab"
    h tab rename "$app_tab" app >/dev/null
    engine_pane="$first_pane"
fi

# ── Tab "app": engine over ui (over task) | telemetry ──
if [ "$telemetry" = 1 ]; then
    telemetry_pane="$(h pane split "$engine_pane" --direction right --ratio 0.45 --cwd "$repo" --no-focus | pane_of)"
    h pane rename "$telemetry_pane" telemetry >/dev/null
fi
ui_pane="$(h pane split "$engine_pane" --direction down --cwd "$repo/apps/studio/desktop" --no-focus | pane_of)"
h pane rename "$engine_pane" engine >/dev/null
h pane rename "$ui_pane" ui >/dev/null
if [ "$local_task" = 1 ] && [ "$task_up" = 0 ]; then
    task_pane="$(h pane split "$ui_pane" --direction down --cwd "$task_dir" --no-focus | pane_of)"
    h pane rename "$task_pane" task >/dev/null
    # The server sends to the same collector as the engine and the window,
    # so a call to Task is one trace from Studio to the server and back.
    task_envs=()
    if [ "$telemetry" = 1 ]; then
        task_envs+=(OTEL_EXPORTER_OTLP_ENDPOINT="http://127.0.0.1:$otlp_port" OTEL_METRIC_EXPORT_INTERVAL=5000)
        # Only warnings and errors in its terminal (Task's TASK_LOG_CONSOLE,
        # once it has it; ignored before): the rest is the collector's.
        task_envs+=(TASK_LOG_CONSOLE=warn)
    fi
    h pane run "$task_pane" "nix develop -c cargo build -p task-server && env ${task_envs[*]:+$(printf '%q ' "${task_envs[@]}")}$dev_home/task-server.sh"
fi
if [ "$telemetry" = 1 ]; then
    h pane run "$telemetry_pane" "bunx teley-cli --local --port $tui_port"
    # The addresses, kept in sight under the terminal view: the dashboard as
    # a terminal hyperlink (click it), `o` opens it.
    links_pane="$(h pane split "$telemetry_pane" --direction down --ratio 0.82 --cwd "$repo" --no-focus | pane_of)"
    h pane rename "$links_pane" links >/dev/null
    links="$tel/links.sh"
    {
        printf '#!/usr/bin/env bash\n'
        printf 'url=%q\n' "${web_room:+http://localhost:$web_port/live/$web_room?token=$web_token}"
        cat <<'LINKS'
show() {
    clear
    if [ -n "$url" ]; then
        printf '\e[1mDashboard\e[0m  \e]8;;%s\e\\%s\e]8;;\e\\\n' "$url" "$url"
        printf '\e[2mo opens its window · r redraws\e[0m\n'
    else
        printf 'No dashboard (no teley checkout)\n'
    fi
LINKS
        printf '    printf %q\n' "\e[2mOTLP in: http://127.0.0.1:$otlp_port · terminal view :$tui_port · agents (MCP) via the relay :$web_port\e[0m\n"
        cat <<'LINKS'
}
show
while read -rsn1 key; do
    case "$key" in
        o) "$(dirname "$0")/dashboard.sh" ;;
    esac
    show
done
LINKS
    } > "$links"
    chmod +x "$links"
    h pane run "$links_pane" "$links"
fi
# The dashboard as a desktop app of its own: scripts/teley-window.py, one
# WebKitGTK window (no tabs, no address bar, app id `dev.fts.teley`) under
# nix-shell, its storage kept in target/dev-telemetry. It outlives this
# script. One at a time: while the one opened earlier is up, it is left
# alone. No nix-shell, the browser.
if [ "$web" = 1 ]; then
    dash="$tel/dashboard.sh"
    {
        printf '#!/usr/bin/env bash\n'
        printf 'url=%q\n' "http://localhost:$web_port/live/$web_room?token=$web_token"
        printf 'tel=%q\n' "$tel"
        printf 'window=%q\n' "$repo/scripts/teley-window.py"
        cat <<'DASH'
pid_file="$tel/dashboard.pid"
if [ -f "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    exit 0
fi
if command -v nix-shell >/dev/null; then
    mkdir -p "$tel/dashboard-webkit"
    setsid nix-shell -I nixpkgs=flake:nixpkgs \
        -p 'python3.withPackages (p: [ p.pygobject3 ])' gtk4 webkitgtk_6_0 \
        gobject-introspection glib-networking \
        --run "exec python3 $(printf %q "$window") $(printf %q "$url") $(printf %q "$tel/dashboard-webkit")" \
        > "$tel/dashboard.log" 2>&1 &
    echo $! > "$pid_file"
else
    xdg-open "$url" >/dev/null 2>&1 &
fi
DASH
    } > "$dash"
    chmod +x "$dash"
    [ "$dashboard" = 1 ] && "$dash"
fi
# Rebuilt (incrementally) before each start: restarting the engine after an
# engine-side change is Ctrl-C and ↑ Enter.
h pane run "$engine_pane" "nix develop -c sh -c '$build_cli' && $env_line $cli engine --reopen"
# The window waits for the engine; FTS_STUDIO_BIN makes any running engine
# its own, so a rebuilt window never replaces it.
h pane run "$ui_pane" "$env_line FTS_ENGINE=attach FTS_STUDIO_BIN=$cli nix develop -c sh -c 'just signal-tailwind >/dev/null 2>&1; exec dx serve --platform desktop -p studio-desktop --bin studio-desktop --features signal-keys-rig --hot-patch false --interactive false'"

echo "dev: \"$label\" in herdr --session $session"
echo "dev:   app: engine ($engine_pane), ui ($ui_pane)${task_pane:+, task ($task_pane)}${telemetry_pane:+, telemetry ($telemetry_pane)}"
[ -n "$collector_pane" ] && echo "dev:   telemetry: collector ($collector_pane)${web_pane:+, web ($web_pane)}"
[ "$web" = 1 ] && echo "dev: dashboard: http://localhost:$web_port/live/$web_room?token=$web_token"
echo "dev: watch it with: herdr --session $session"
