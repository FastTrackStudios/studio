#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   tools/render_parity_actuals.sh [--root tests/frame/parity] [--all] [--scenario <id>] [--core10]

ROOT="tests/frame/parity"
SCENARIO_FILTER=""
RUN_ALL="false"

CORE10_SCENARIOS=(
  "primitives.fractional-geometry"
  "paint.gradient-matrix"
  "paint.blend-mode-stack"
  "effects.layer-background-blur"
  "strokes.caps-joins-dashes"
  "masks.vector-text-image-mask"
  "images.fill-mode-matrix"
  "text.mixed-style-runs"
  "layout.auto-layout-hug-fill"
  "regression.kitchen-sink"
)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT="${2:-}"
      shift 2
      ;;
    --scenario)
      SCENARIO_FILTER="${2:-}"
      shift 2
      ;;
    --all)
      RUN_ALL="true"
      shift 1
      ;;
    --core10)
      RUN_ALL="false"
      shift 1
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

SCENARIOS_DIR="$ROOT/scenarios"
if [[ ! -d "$SCENARIOS_DIR" ]]; then
  echo "missing scenarios dir: $SCENARIOS_DIR" >&2
  exit 2
fi

mapfile -t SCENARIO_DIRS < <(find "$SCENARIOS_DIR" -mindepth 1 -maxdepth 1 -type d | LC_ALL=C sort)
if [[ -n "$SCENARIO_FILTER" ]]; then
  SCENARIO_DIRS=("$SCENARIOS_DIR/$SCENARIO_FILTER")
elif [[ "$RUN_ALL" != "true" ]]; then
  SCENARIO_DIRS=()
  for scenario_id in "${CORE10_SCENARIOS[@]}"; do
    SCENARIO_DIRS+=("$SCENARIOS_DIR/$scenario_id")
  done
fi

for scenario_dir in "${SCENARIO_DIRS[@]}"; do
  scenario_id="$(basename "$scenario_dir")"
  export_json="$scenario_dir/export/latest-bridge-export.json"
  actual_png="$scenario_dir/actual/frame.png"

  if [[ ! -f "$export_json" ]]; then
    echo "[$scenario_id] skip: missing $export_json"
    continue
  fi

  mkdir -p "$(dirname "$actual_png")"
  echo "[$scenario_id] rendering actual/frame.png"
  cargo run -q -p frame-ui --features anyrender --bin render_png -- \
    "$export_json" \
    --out "$actual_png"
done

echo "done rendering parity actual PNGs"
