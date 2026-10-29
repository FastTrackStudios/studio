#!/usr/bin/env bash
# The drum editing workflow, driven and timed: a full comp and a full edit
# of a song's drums, by the Action List (as a model over MCP runs it) and
# by the window's keys and mouse (computer use, in a loft-dev session), each
# step read back from the trace it leaves in the dev loop's feed.
#
# Usage: scripts/drum-lab.sh <command> [args]
#   copy SRC.session DIR   a lab copy of a song (the .session with its -wal;
#                          its Media linked), to edit without touching SRC
#   open DIR/X.session     open it in the engine (SESSION_OPEN), wait for it
#   actions                the workflow by actions: HITS_DETECT (all takes),
#                          COMP_MAIN (LAB_MAIN_TAKE, Teal), COMP_BEST,
#                          COMP_SEAMS, DRUMS_EDIT_LANES, DRUMS_REQUANTIZE,
#                          HITS_AUDIT — each step's time and trace
#   gui                    the workflow by the window, in the loft-dev session
#                          LAB_SESSION (drumlab): Edit mode (Alt+5), Ctrl+T
#                          (the best comp), then per fill (ZONES_LIST, at
#                          most LAB_FILLS) frame it, drag a time selection
#                          over it, T; per section (at most LAB_SECTIONS)
#                          frame it, drag, Q — every step checked through
#                          the Action List and screenshotted
#   report [SINCE]         every action and window action since SINCE
#                          (HH:MM:SS, UTC; default: the last 30 minutes):
#                          its time, its stages, the step recorded after it
#   trace ID               one trace as a tree
#   act ID [JSON]          run one action, show its result and its trace
#
# Needs the dev loop (`just dev`): the engine's Action List on :47811 and the
# feed (target/dev-telemetry/feed.jsonl, FTS_FEED). For `gui`: a loft-dev
# session with a Studio window in it, attached to the same engine, named
# by FTS_WINDOW_NAME=drumlab, maximized (`loft-input bind mod+f`), and a
# Loft checkout (LOFT_DIR, default ../../loft).
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
mcp="${FTS_MCP:-http://127.0.0.1:47811/mcp}"
feed="${FTS_FEED:-$repo/target/dev-telemetry/feed.jsonl}"
loft="${LOFT_DIR:-$(cd "$repo/../.." && pwd)/loft}"
session="${LAB_SESSION:-drumlab}"
shots="${LAB_SHOTS:-$repo/target/drum-lab}"
# The lab window's arrangement, maximized in a 1600×1000 session: where the
# ruler is, and the lanes' left and right edges (global logical px).
ruler_y="${LAB_RULER_Y:-222}"
lanes_left="${LAB_LANES_LEFT:-375}"
lanes_right="${LAB_LANES_RIGHT:-1565}"

# ── The Action List ──────────────────────────────────────────────────

# tool NAME JSON — one MCP tool call, its text.
tool() {
    local body
    body=$(jq -nc --arg t "$1" --argjson a "$2" '{jsonrpc:"2.0",id:1,method:"tools/call",params:{name:$t,arguments:$a}}')
    timeout 120 curl -s -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
        -X POST "$mcp" -d "$body" | jq -r '.result.content[]?.text // .error // .'
}

# run ID [JSON] — run an action, its result.
run() { tool run "$(jq -nc --arg id "$1" --argjson a "${2:-null}" 'if $a == null then {id:$id} else {id:$id,args:$a} end')"; }

now() { date -u +%H:%M:%S; }

# trace_of SINCE FILTER — the newest trace since SINCE holding a span FILTER
# (a jq predicate on a span) selects; waits for it up to a minute (the
# collector holds a trace 5 s before it decides, then batches).
trace_of() {
    local since="$1" filter="$2" id=""
    for _ in $(seq 1 30); do
        id=$(jq -r --arg s "$since" "select(.type==\"trace\" and .time[11:19] >= \$s) | select(.spans[] | $filter) | .trace.trace_id" "$feed" 2>/dev/null | tail -1)
        [ -n "$id" ] && break
        sleep 2
    done
    printf '%s' "$id"
}

# tree ID — a trace's spans as a tree, with their key fields.
tree() {
    jq -r --arg id "$1" 'select(.type=="trace" and (.trace.trace_id|startswith($id))) | .spans[]' "$feed" | jq -rs '
      (map({key: .span_id, value: .}) | from_entries) as $by
      | (map(.start_time) | min) as $t0
      | def depth(s): if s.parent_span_id == null or $by[s.parent_span_id] == null then 0 else 1 + depth($by[s.parent_span_id]) end;
      sort_by(.start_time)[]
      | "\("+\(.start_time - $t0)ms" | .[0:9])\("  " * depth(.))\(.name)  \(.duration) ms  \(.attributes | to_entries | map(select(.key | test("^(action\\.id|window\\.action|window\\.key_action|song\\.|set\\.|load\\.|steps\\.label)"))) | map("\(.key)=\(.value | tostring | .[0:40])") | join(" "))"'
}

# step ID [JSON] — run an action, print its time, its result's head and its
# trace's top stages.
step() {
    local id="$1" args="${2:-null}" t0 started took out trace
    t0=$(now); started=$(date +%s.%N)
    out=$(run "$id" "$args")
    took=$(echo "$(date +%s.%N) - $started" | bc)
    printf '\n== %s  %.2f s\n%s\n' "$id" "$took" "${out:0:300}"
    trace=$(trace_of "$t0" "(.attributes[\"action.id\"] // \"\") == \"$id\"")
    if [ -n "$trace" ]; then { tree "$trace" | head -40; } || true; else echo "(no trace: under 5 ms)"; fi
}

# ── The window, in the loft-dev session ──────────────────────────────

li() { (cd "$loft" && timeout 60 nix run .#input -- "$1" -s "$session" "${@:2}" >/dev/null); }
shot() {
    mkdir -p "$shots"
    (cd "$loft" && timeout 60 nix run .#shot -- -s "$session" --max 1500 --out "$shots/$1.png" >/dev/null 2>&1) || true
    echo "   shot: $shots/$1.png"
}

# gui_step NAME FILTER KEYS… — press KEYS in the window; wait for the trace
# a span FILTER selects; print its tree.
gui_step() {
    local name="$1" filter="$2" t0 trace
    shift 2
    t0=$(now)
    li key "$@"
    trace=$(trace_of "$t0" "$filter")
    printf '\n== %s (keys: %s)\n' "$name" "$*"
    if [ -n "$trace" ]; then { tree "$trace" | head -40; } || true; else echo "(no trace)"; fi
    shot "$name"
}

# select_framed FROM TO — frame FROM..TO (s) and drag a time selection over
# the whole of it on the ruler; the selection the window made.
select_framed() {
    run SESSION_VIEW "$(jq -nc --argjson f "$1" --argjson t "$2" '{from:$f,to:$t}')" >/dev/null
    sleep 1
    li drag "$((lanes_left + 3))" "$ruler_y" "$((lanes_right - 3))" "$ruler_y"
    sleep 0.5
    run SESSION_EDIT_STATE | jq -c '{selection_start, selection_end}'
}

gui() {
    local fills="${LAB_FILLS:-4}" sections="${LAB_SECTIONS:-3}"
    run SESSION_WINDOW_TARGET '{"window":"drumlab"}' | jq -c .
    li click "$(((lanes_left + lanes_right) / 2))" 455
    li key alt+5
    sleep 1
    echo "mode: $(run SESSION_WINDOW)"
    gui_step comp-best '.name == "window.action"' Escape ctrl+t
    local n=0
    while IFS=$'\t' read -r start end note; do
        n=$((n + 1)); [ "$n" -le "$fills" ] || break
        echo; echo "-- fill $n at ${start}–${end} s: ${note:0:60}"
        echo "   selected: $(select_framed "$start" "$end")"
        gui_step "fill-$n-next-take" '.attributes["window.action"] == "CompCycle"' t
    done < <(run ZONES_LIST | jq -r '.[] | select(.kind == "Fill") | [.start, .end, .note] | @tsv')
    n=0
    while IFS=$'\t' read -r name from to; do
        n=$((n + 1)); [ "$n" -le "$sections" ] || break
        echo; echo "-- section $name at ${from}–${to} s"
        echo "   selected: $(select_framed "$from" "$to")"
        gui_step "section-$n-quantize" '.attributes["window.action"] == "QuantizeDrums"' q
    done < <(run SESSION_SECTIONS | jq -r '.[] | [.name, .start, .end] | @tsv')
}

actions() {
    step HITS_DETECT '{"all_takes":true}'
    step COMP_MAIN "$(jq -nc --arg t "${LAB_MAIN_TAKE:-Teal}" '{take:$t}')"
    step COMP_BEST
    step COMP_SEAMS
    step DRUMS_EDIT_LANES
    step DRUMS_REQUANTIZE '{"keep_manual":true}'
    step HITS_AUDIT
}

# report [SINCE] — each action or window action since SINCE, as a table.
report() {
    local since="${1:-$(date -u -d '30 minutes ago' +%H:%M:%S)}"
    jq -r --arg s "$since" '
      select(.type=="trace" and .time[11:19] >= $s) | .time[11:19] as $at | .spans as $sp
      | ($sp | map(select(.name=="window.action" and (.attributes["window.action"] // "") != "")) | sort_by(.start_time) | .[0]) as $gui
      | ($sp | map(select(.name=="action")) | sort_by(.start_time) | .[0]) as $act
      | select($act != null or $gui != null)
      | ($act // $gui) as $work
      | ($sp | map(select(.parent_span_id == $work.span_id and (.name | test("^[a-z]+\\.[a-z_]+$")) and .name != "view.run" and .name != "view.call" and .name != "song.step"))
             | sort_by(-.duration) | .[0:4] | map("\(.name) \(.duration)") | join(", ")) as $stages
      | ($sp | map(select(.name=="song.step")) | map(.duration) | max // 0) as $rec
      | [$at,
         (if $gui != null then "\($gui.attributes["window.action"]) → \($act.attributes["action.id"] // "-")" else $act.attributes["action.id"] end),
         (if $gui != null then "gui" else "act" end),
         (($gui // $act).duration), $rec, $stages] | @tsv' "$feed" \
      | (printf 'at\taction\tby\tms\trecord ms\tlongest stages (ms)\n'; cat) | column -t -s $'\t'
}

cmd="${1:-}"; shift || true
case "$cmd" in
copy)
    src="$1"; dir="$2"
    [ -e "$dir" ] && { echo "drum-lab: $dir exists" >&2; exit 1; }
    mkdir -p "$dir"
    cp --reflink=auto "$src" "$dir/"
    [ -f "$src-wal" ] && cp --reflink=auto "$src-wal" "$dir/"
    [ -d "$(dirname "$src")/Media" ] && ln -s "$(dirname "$src")/Media" "$dir/Media"
    echo "drum-lab: $dir/$(basename "$src")"
    ;;
open)
    run SESSION_OPEN "$(jq -nc --arg p "$1" '{path:$p}')"
    for _ in $(seq 1 90); do run SESSION_OPENING | grep -q '"state":"idle"' && break; sleep 2; done
    run SESSION_SONGS | jq -c '.[] | {title, state}'
    ;;
actions) actions ;;
gui) gui ;;
report) report "$@" ;;
trace) tree "$1" ;;
act) step "$1" "${2:-null}" ;;
*) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
