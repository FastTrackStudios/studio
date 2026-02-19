#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   tools/frame/run_parity_pipeline.sh <export_json> \
#     --out-dir <dir> \
#     [--actual-png-dir <dir> --expected-png-dir <dir> --threshold <percent>] \
#     [--require-pass]

EXPORT_JSON="${1:-}"
OUT_DIR=""
ACTUAL_PNG_DIR=""
EXPECTED_PNG_DIR=""
THRESHOLD="2.0"
REQUIRE_PASS="false"

shift 1 || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --actual-png-dir)
      ACTUAL_PNG_DIR="${2:-}"
      shift 2
      ;;
    --expected-png-dir)
      EXPECTED_PNG_DIR="${2:-}"
      shift 2
      ;;
    --threshold)
      THRESHOLD="${2:-}"
      shift 2
      ;;
    --require-pass)
      REQUIRE_PASS="true"
      shift 1
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$EXPORT_JSON" || -z "$OUT_DIR" ]]; then
  echo "usage: $0 <export_json> --out-dir <dir> [--actual-png-dir <dir> --expected-png-dir <dir> --threshold <percent>]" >&2
  exit 2
fi
if [[ ! -f "$EXPORT_JSON" ]]; then
  echo "missing export json: $EXPORT_JSON" >&2
  exit 2
fi

mkdir -p "$OUT_DIR"

RENDER_DIAG_JSON="$OUT_DIR/render-diagnostics.json"
IMPORT_DIAG_JSON="$OUT_DIR/import-diagnostics.json"
PERF_JSON="$OUT_DIR/load-profile.json"
VISUAL_JSON="$OUT_DIR/visual-acceptance.json"
VISUAL_MD="$OUT_DIR/visual-acceptance.md"
PARITY_JSON="$OUT_DIR/parity-report.json"
PARITY_MD="$OUT_DIR/parity-report.md"

tools/frame/capture_render_diagnostics.sh "$EXPORT_JSON" "$RENDER_DIAG_JSON"
tools/frame/capture_import_diagnostics.sh "$EXPORT_JSON" "$IMPORT_DIAG_JSON"
tools/frame/capture_load_profile.sh "$EXPORT_JSON" "$PERF_JSON" 3

VISUAL_ARGS=()
if [[ -n "$ACTUAL_PNG_DIR" || -n "$EXPECTED_PNG_DIR" ]]; then
  if [[ -z "$ACTUAL_PNG_DIR" || -z "$EXPECTED_PNG_DIR" ]]; then
    echo "both --actual-png-dir and --expected-png-dir are required together" >&2
    exit 2
  fi
  if tools/frame/run_visual_acceptance.sh "$ACTUAL_PNG_DIR" "$EXPECTED_PNG_DIR" "$THRESHOLD" --out-json "$VISUAL_JSON" --out-md "$VISUAL_MD"; then
    :
  else
    # Keep pipeline running so parity report still gets generated.
    echo "visual acceptance reported failures; continuing to parity report generation"
  fi
  VISUAL_ARGS=(--visual-json "$VISUAL_JSON")
fi

tools/frame/generate_parity_report.sh \
  "$EXPORT_JSON" \
  --perf-json "$PERF_JSON" \
  --render-diag-json "$RENDER_DIAG_JSON" \
  --import-diag-json "$IMPORT_DIAG_JSON" \
  "${VISUAL_ARGS[@]}" \
  --out-json "$PARITY_JSON" \
  --out-md "$PARITY_MD"

echo "wrote:"
  echo "  $RENDER_DIAG_JSON"
echo "  $IMPORT_DIAG_JSON"
echo "  $PERF_JSON"
[[ -f "$VISUAL_JSON" ]] && echo "  $VISUAL_JSON"
[[ -f "$VISUAL_MD" ]] && echo "  $VISUAL_MD"
echo "  $PARITY_JSON"
echo "  $PARITY_MD"

if [[ "$REQUIRE_PASS" == "true" ]]; then
  if jq -e '.gates.parityPass == true' "$PARITY_JSON" >/dev/null; then
    echo "parityPass=true"
  else
    echo "parityPass=false (failing due to --require-pass)" >&2
    exit 1
  fi
fi
