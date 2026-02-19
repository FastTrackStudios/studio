#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   tools/run_parity_scenarios.sh \
#     [--root tests/frame/parity] \
#     [--scenario <scenario-id>] \
#     [--threshold <percent>] \
#     [--require-pass]

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="tests/frame/parity"
SCENARIO_FILTER=""
THRESHOLD="0.0"
REQUIRE_PASS="false"

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

SCENARIOS_DIR="$ROOT/scenarios"
REPORTS_DIR="$ROOT/reports"
mkdir -p "$REPORTS_DIR"

if [[ ! -d "$SCENARIOS_DIR" ]]; then
  echo "missing scenarios dir: $SCENARIOS_DIR" >&2
  exit 2
fi

mapfile -t SCENARIO_DIRS < <(find "$SCENARIOS_DIR" -mindepth 1 -maxdepth 1 -type d | LC_ALL=C sort)

if [[ -n "$SCENARIO_FILTER" ]]; then
  SCENARIO_DIRS=("$SCENARIOS_DIR/$SCENARIO_FILTER")
fi

if [[ ${#SCENARIO_DIRS[@]} -eq 0 ]]; then
  echo "no scenarios found in $SCENARIOS_DIR" >&2
  exit 2
fi

SUMMARY_TMP="$(mktemp)"
trap 'rm -f "$SUMMARY_TMP"' EXIT
printf '[]' > "$SUMMARY_TMP"

TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0

for scenario_dir in "${SCENARIO_DIRS[@]}"; do
  scenario_id="$(basename "$scenario_dir")"
  export_json="$scenario_dir/export/latest-bridge-export.json"
  actual_dir="$scenario_dir/actual"
  expected_dir="$scenario_dir/expected"
  out_dir="$REPORTS_DIR/$scenario_id"

  TOTAL=$((TOTAL + 1))

  if [[ ! -f "$export_json" ]]; then
    echo "[$scenario_id] skip: missing $export_json"
    SKIPPED=$((SKIPPED + 1))
    jq --arg id "$scenario_id" --arg reason "missing_export_json" \
      '. += [{scenarioId:$id, status:"skipped", reason:$reason}]' "$SUMMARY_TMP" > "$SUMMARY_TMP.tmp"
    mv "$SUMMARY_TMP.tmp" "$SUMMARY_TMP"
    continue
  fi

  mkdir -p "$out_dir"
  echo "[$scenario_id] running parity pipeline"

  pipeline_args=("$export_json" "--out-dir" "$out_dir" "--threshold" "$THRESHOLD")
  if [[ -d "$actual_dir" && -d "$expected_dir" ]]; then
    pipeline_args+=("--actual-png-dir" "$actual_dir" "--expected-png-dir" "$expected_dir")
  else
    echo "[$scenario_id] note: missing actual/expected dir pair, running non-visual diagnostics only"
  fi

  if "$SCRIPT_DIR/run_parity_pipeline.sh" "${pipeline_args[@]}"; then
    if jq -e '.gates.parityPass == true' "$out_dir/parity-report.json" >/dev/null 2>&1; then
      PASSED=$((PASSED + 1))
      status="pass"
    else
      FAILED=$((FAILED + 1))
      status="fail"
    fi
  else
    FAILED=$((FAILED + 1))
    status="fail"
  fi

  jq --arg id "$scenario_id" \
    --arg status "$status" \
    --arg report "$out_dir/parity-report.json" \
    '. += [{scenarioId:$id, status:$status, parityReport:$report}]' "$SUMMARY_TMP" > "$SUMMARY_TMP.tmp"
  mv "$SUMMARY_TMP.tmp" "$SUMMARY_TMP"
done

SUMMARY_JSON="$REPORTS_DIR/summary.json"
SUMMARY_MD="$REPORTS_DIR/summary.md"

jq -n \
  --arg generatedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg root "$ROOT" \
  --arg threshold "$THRESHOLD" \
  --argjson total "$TOTAL" \
  --argjson passed "$PASSED" \
  --argjson failed "$FAILED" \
  --argjson skipped "$SKIPPED" \
  --argjson scenarios "$(cat "$SUMMARY_TMP")" \
  '{generatedAt:$generatedAt, root:$root, thresholdPercent:($threshold|tonumber), total:$total, passed:$passed, failed:$failed, skipped:$skipped, scenarios:$scenarios}' > "$SUMMARY_JSON"

{
  echo "# Parity Scenario Summary"
  echo
  echo "- Generated: \`$(jq -r '.generatedAt' "$SUMMARY_JSON")\`"
  echo "- Root: \`$(jq -r '.root' "$SUMMARY_JSON")\`"
  echo "- Threshold: \`$(jq -r '.thresholdPercent' "$SUMMARY_JSON")\`%"
  echo "- Total: \`$(jq -r '.total' "$SUMMARY_JSON")\`"
  echo "- Passed: \`$(jq -r '.passed' "$SUMMARY_JSON")\`"
  echo "- Failed: \`$(jq -r '.failed' "$SUMMARY_JSON")\`"
  echo "- Skipped: \`$(jq -r '.skipped' "$SUMMARY_JSON")\`"
  echo
  echo "## Scenarios"
  jq -r '.scenarios[] | "- `\(.scenarioId)`: `\(.status)`"' "$SUMMARY_JSON"
} > "$SUMMARY_MD"

echo "wrote:"
echo "  $SUMMARY_JSON"
echo "  $SUMMARY_MD"

if [[ "$REQUIRE_PASS" == "true" ]]; then
  if jq -e '.failed == 0 and .skipped == 0' "$SUMMARY_JSON" >/dev/null; then
    echo "all scenarios passed"
  else
    echo "one or more scenarios failed/skipped (failing due to --require-pass)" >&2
    exit 1
  fi
fi
