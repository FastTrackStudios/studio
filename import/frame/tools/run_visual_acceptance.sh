#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   tools/frame/run_visual_acceptance.sh <actual_dir> <expected_dir> [threshold_percent] [--out-json path] [--out-md path]
#
# Compares PNG files with matching relative paths in expected_dir against actual_dir
# using frame-ui visual_diff binary.

ACTUAL_DIR="${1:-}"
EXPECTED_DIR="${2:-}"
THRESHOLD="${3:-2.0}"
OUT_JSON=""
OUT_MD=""

shift 3 || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-json)
      OUT_JSON="${2:-}"
      shift 2
      ;;
    --out-md)
      OUT_MD="${2:-}"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$ACTUAL_DIR" || -z "$EXPECTED_DIR" ]]; then
  echo "usage: $0 <actual_dir> <expected_dir> [threshold_percent] [--out-json path] [--out-md path]" >&2
  exit 2
fi
if [[ ! -d "$ACTUAL_DIR" ]]; then
  echo "missing actual dir: $ACTUAL_DIR" >&2
  exit 2
fi
if [[ ! -d "$EXPECTED_DIR" ]]; then
  echo "missing expected dir: $EXPECTED_DIR" >&2
  exit 2
fi

TMP_REPORT="$(mktemp)"
trap 'rm -f "$TMP_REPORT"' EXIT
printf '[]' > "$TMP_REPORT"

TOTAL=0
PASSED=0
FAILED=0
MISSING=0

while IFS= read -r -d '' expected; do
  rel="${expected#${EXPECTED_DIR}/}"
  actual="${ACTUAL_DIR}/${rel}"
  TOTAL=$((TOTAL + 1))

  if [[ ! -f "$actual" ]]; then
    MISSING=$((MISSING + 1))
    jq --arg rel "$rel" '. += [{path:$rel, status:"missing_actual"}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
    mv "$TMP_REPORT.tmp" "$TMP_REPORT"
    continue
  fi

  if output=$(cargo run -q -p frame-ui --features anyrender --bin visual_diff -- "$actual" "$expected" "$THRESHOLD" 2>&1); then
    PASSED=$((PASSED + 1))
    jq --argjson item "$output" '. += [$item + {status:"pass"}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
    mv "$TMP_REPORT.tmp" "$TMP_REPORT"
  else
    FAILED=$((FAILED + 1))
    # Attempt to parse structured output if present, else capture stderr text.
    if echo "$output" | tail -n 1 | jq -e . >/dev/null 2>&1; then
      json_line="$(echo "$output" | tail -n 1)"
      jq --argjson item "$json_line" '. += [$item + {status:"fail"}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
      mv "$TMP_REPORT.tmp" "$TMP_REPORT"
    else
      jq --arg rel "$rel" --arg err "$output" '. += [{path:$rel, status:"fail", error:$err}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
      mv "$TMP_REPORT.tmp" "$TMP_REPORT"
    fi
  fi
done < <(find "$EXPECTED_DIR" -type f -name '*.png' -print0 | sort -z)

SUMMARY=$(jq -n \
  --arg actualDir "$ACTUAL_DIR" \
  --arg expectedDir "$EXPECTED_DIR" \
  --argjson threshold "$THRESHOLD" \
  --argjson total "$TOTAL" \
  --argjson passed "$PASSED" \
  --argjson failed "$FAILED" \
  --argjson missing "$MISSING" \
  --argjson results "$(cat "$TMP_REPORT")" \
  '{actualDir:$actualDir, expectedDir:$expectedDir, thresholdPercent:$threshold, total:$total, passed:$passed, failed:$failed, missingActual:$missing, results:$results}')

echo "$SUMMARY"

if [[ -n "$OUT_JSON" ]]; then
  mkdir -p "$(dirname "$OUT_JSON")"
  printf '%s\n' "$SUMMARY" > "$OUT_JSON"
fi

if [[ -n "$OUT_MD" ]]; then
  mkdir -p "$(dirname "$OUT_MD")"
  {
    echo "# Frame Visual Acceptance Report"
    echo
    echo "- Actual: \`$ACTUAL_DIR\`"
    echo "- Expected: \`$EXPECTED_DIR\`"
    echo "- Threshold: \`$THRESHOLD\`%"
    echo "- Total: \`$TOTAL\`"
    echo "- Passed: \`$PASSED\`"
    echo "- Failed: \`$FAILED\`"
    echo "- Missing actual: \`$MISSING\`"
    echo
    echo "## Failures"
    jq -r '
      [.results[]
        | select(.status == "fail" or .status == "missing_actual")
        | "- `\(.path // (.actual + " vs " + .expected))`: \(.status)\(if .diffPercent then ", diff=\(.diffPercent)%" else "" end)\(if .error then ", error=\(.error | gsub("\\n"; " "))" else "" end)"
      ] | if length == 0 then "- none" else .[] end
    ' <<<"$SUMMARY"
  } > "$OUT_MD"
fi

if [[ "$FAILED" -gt 0 || "$MISSING" -gt 0 ]]; then
  exit 1
fi
