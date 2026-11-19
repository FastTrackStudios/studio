#!/usr/bin/env bash
# Signal's rigs in Studio, as extensions (docs/design/signal-extensions.md):
# builds Studio's distribution of Architect and the rig engine's program
# (the root workspace's `desktop-dev` profile for the host), installs the
# engine into its package, starts an isolated Studio (`studio-architect
# dev`) with the engine loaded as a development copy, and lays the panes
# out as the iPad does (the "Rig" arrangement, below).
#
#   extensions/rig-dev.sh                       # the guitar rig
#   RIG_LAYOUT=keys extensions/rig-dev.sh       # the keys rig
#   RIG_COPIES="switches grid" extensions/rig-dev.sh   # those views as
#       development copies (a process each) instead of the linked ones
#
# Extra arguments go to `dev` (--vault, --hide all, --run …); RIG_DEV_NAME
# names the instance (`studio-rig`).
#
# What the rig plays and through what is the environment's, as for any rig
# engine (signal:features/rigs/guitar/src/library.rs):
#   SIGNAL_RIG_DIR=<dir>      a library other than <config>/signal/rig
#   SIGNAL_RIG_DESIGN=1       no audio device, no MIDI, no DSP: the screens
#                             over a synthesised rig (and nothing is saved)
#   SIGNAL_RIG_SILENT=1       the master muted, everything still processed
#   STUDIO_RIG_AUTOSTART=0    the engine up, its audio closed until
#                             STUDIO_RIG_START
# CARGO_ARGS, when set, goes to cargo before `build` (local overrides:
# CARGO_ARGS="--config ../stack/overrides.toml").
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
host=$(rustc -vV | sed -n 's/^host: //p')
out=$root/target/$host/desktop-dev
layout=${RIG_LAYOUT:-guitar}
copies=${RIG_COPIES:-}
name=${RIG_DEV_NAME:-studio-rig}

declare -A program=(
  [rig]=rig-extension [switches]=studio-rig-switches [setlist]=studio-rig-setlist
  [browser]=studio-rig-browser [grid]=studio-rig-grid [macros]=studio-rig-macros
  [status]=studio-rig-status [audio-settings]=studio-audio-settings [keys]=studio-keys
)
declare -A folder=(
  [rig]=rig [switches]=rig-switches [setlist]=rig-setlist [browser]=rig-browser
  [grid]=rig-grid [macros]=rig-macros [status]=rig-status
  [audio-settings]=audio-settings [keys]=keys
)

build=(-p studio-architect) args=()
for p in rig $copies; do
  [ -n "${program[$p]:-}" ] || { echo "rig-dev: no extension '$p'" >&2; exit 2; }
  build+=(-p "${program[$p]}")
done
# shellcheck disable=SC2086 # CARGO_ARGS is words, on purpose
cargo ${CARGO_ARGS:-} build --manifest-path "$root/Cargo.toml" --profile desktop-dev --target "$host" "${build[@]}"
for p in rig $copies; do
  install -m 755 "$out/${program[$p]}" "$here/${folder[$p]}/${program[$p]}"
  args+=(--extension "$here/${folder[$p]}")
done

app=${ARCHITECT:-$out/studio-architect}
state=$(mktemp)
trap 'rm -f "$state"' EXIT
"$app" dev --name "$name" "${args[@]}" --collapse left "$@" > "$state"
sock=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["socket"])' "$state")
act() {
  local json=${2:-}
  [ -n "$json" ] || json='{}'
  "$app" actions --app "$sock" run "$1" --json "$json"
}

# The engine answers before the panes are asked for.
for _ in $(seq 60); do
  act STUDIO_RIG_STATE 2>/dev/null | grep -q '"up": true' && break
  sleep 0.5
done

# The "Rig" arrangement: the iPad's screen as panes.
#
#   guitar   status across the top; the sidebar (setlist) down the left;
#            right of it the macros on top, the DSP grid in the middle with
#            the FX row, Edit and the browser as its tabs, and the switches
#            at the foot with the tuner as their tab.
#   keys     the browser down the left; the mixer over the engine-and-layer
#            page; the switches at the foot with the piano, the macros,
#            the routing and the MIDI monitor as their tabs.
#
# When Studio's launcher has landed (feat/project-launcher), this is the
# layout to save through it (STUDIO_LAUNCHER_LAYOUT_SAVE as "Rig").
open() { act EXT_VIEW_OPEN "{\"extension\":\"$1\",\"view\":\"$2\",\"placement\":\"${3:-split}\"}" > /dev/null; }
focus() { act WORKSPACE_FOCUS "{\"pane\":\"$1\"}" > /dev/null; }
if [ "$layout" = keys ]; then
  open studio.keys browser tab
  open studio.keys mixer
  open studio.keys layers
  open studio.keys switches
  for view in piano macros routing midi; do
    focus studio.keys/switches
    open studio.keys "$view" tab
  done
  focus studio.keys/switches
else
  open studio.rig.status status tab
  open studio.rig.setlist setlist
  open studio.rig.macros macros
  open studio.rig.grid grid
  open studio.rig.switches switches
  for view in fx edit; do
    focus studio.rig.grid/grid
    open studio.rig.grid "$view" tab
  done
  focus studio.rig.grid/grid
  open studio.rig.browser browser tab
  focus studio.rig.switches/switches
  open studio.rig.status tuner tab
  focus studio.rig.grid/grid
  focus studio.rig.switches/switches
fi

act WORKSPACE_EXPORT | RIG_LAYOUT=$layout python3 -I -c '
import json, os, sys
d = json.load(sys.stdin)
def views(n):
    if "group" in n:
        return [t["view"] for t in n["group"]["tabs"]]
    return [v for c in n["split"]["children"] for v in views(c["node"])]
def find(n, view):
    if "group" in n:
        return n if view in views(n) else None
    for c in n["split"]["children"]:
        hit = find(c["node"], view)
        if hit:
            return hit
def split(axis, parts):
    parts = [(share, node) for share, node in parts if node]
    total = sum(share for share, _ in parts)
    return {"split": {"axis": axis, "children": [
        {"share": share / total, "node": node} for share, node in parts]}}
for w in d["windows"]:
    c = w.get("center")
    if not c:
        continue
    g = lambda view: find(c, view)
    if os.environ["RIG_LAYOUT"] == "keys":
        if not g("studio.keys/mixer"):
            continue
        work = split("column", [(0.46, g("studio.keys/mixer")), (0.30, g("studio.keys/layers")),
                                (0.24, g("studio.keys/switches"))])
        w["center"] = split("row", [(0.22, g("studio.keys/browser")), (0.78, work)])
    else:
        if not g("studio.rig.grid/grid"):
            continue
        work = split("column", [(0.14, g("studio.rig.macros/macros")), (0.48, g("studio.rig.grid/grid")),
                                (0.38, g("studio.rig.switches/switches"))])
        body = split("row", [(0.24, g("studio.rig.setlist/setlist")), (0.76, work)])
        w["center"] = split("column", [(0.09, g("studio.rig.status/status")), (0.91, body)])
print(json.dumps({"layout": d}))' > "$state"
"$app" actions --app "$sock" run WORKSPACE_APPLY --json - < "$state" > /dev/null
# The rig has the window: Architect's side and bottom docks folded away
# (the layout applied above opens them again).
for side in right bottom; do
  act WORKSPACE_DOCK_SET "{\"side\":\"$side\",\"shown\":\"hidden\"}" > /dev/null || true
done
echo "studio rig: socket $sock ($layout); '$app dev stop $name' stops it"
