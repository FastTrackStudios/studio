#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   tools/run_svg_parity_scenarios.sh \
#     [--root tests/frame/parity] \
#     [--all] \
#     [--core10] \
#     [--scenario <scenario-id>] \
#     [--require-pass]
#
# Notes:
# - Strict pass requires byte-identical SVG files.
# - Canonical pass uses xmllint C14N when available (diagnostic only).

ROOT="tests/frame/parity"
SCENARIO_FILTER=""
RUN_ALL="false"
REQUIRE_PASS="false"

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
    --all)
      RUN_ALL="true"
      shift 1
      ;;
    --core10)
      RUN_ALL="false"
      shift 1
      ;;
    --scenario)
      SCENARIO_FILTER="${2:-}"
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
elif [[ "$RUN_ALL" != "true" ]]; then
  SCENARIO_DIRS=()
  for scenario_id in "${CORE10_SCENARIOS[@]}"; do
    SCENARIO_DIRS+=("$SCENARIOS_DIR/$scenario_id")
  done
fi

if [[ ${#SCENARIO_DIRS[@]} -eq 0 ]]; then
  echo "no scenarios found in $SCENARIOS_DIR" >&2
  exit 2
fi

HAS_XMLLINT="false"
if command -v xmllint >/dev/null 2>&1; then
  HAS_XMLLINT="true"
fi

SUMMARY_TMP="$(mktemp)"
trap 'rm -f "$SUMMARY_TMP"' EXIT
printf '[]' > "$SUMMARY_TMP"

TOTAL=0
STRICT_PASS=0
STRICT_FAIL=0
CANONICAL_PASS=0
CANONICAL_FAIL=0
SKIPPED=0

for scenario_dir in "${SCENARIO_DIRS[@]}"; do
  scenario_id="$(basename "$scenario_dir")"
  expected_svg="$scenario_dir/expected.svg"
  actual_svg="$scenario_dir/actual.svg"
  report_json="$scenario_dir/svg-report.json"
  TOTAL=$((TOTAL + 1))

  if [[ ! -f "$expected_svg" || ! -f "$actual_svg" ]]; then
    SKIPPED=$((SKIPPED + 1))
    jq -n \
      --arg scenarioId "$scenario_id" \
      --arg expectedSvg "$expected_svg" \
      --arg actualSvg "$actual_svg" \
      --arg status "skipped" \
      --arg reason "missing_svg" \
      '{scenarioId:$scenarioId,status:$status,reason:$reason,expectedSvg:$expectedSvg,actualSvg:$actualSvg}' > "$report_json"
    jq --arg id "$scenario_id" \
      --arg status "skipped" \
      --arg reason "missing_svg" \
      --arg report "$report_json" \
      '. += [{scenarioId:$id,status:$status,reason:$reason,report:$report}]' "$SUMMARY_TMP" > "$SUMMARY_TMP.tmp"
    mv "$SUMMARY_TMP.tmp" "$SUMMARY_TMP"
    echo "[$scenario_id] skipped (missing actual.svg or expected.svg)"
    continue
  fi

  strict_equal="false"
  if cmp -s "$actual_svg" "$expected_svg"; then
    strict_equal="true"
    STRICT_PASS=$((STRICT_PASS + 1))
  else
    STRICT_FAIL=$((STRICT_FAIL + 1))
  fi

  actual_sha="$(shasum -a 256 "$actual_svg" | awk '{print $1}')"
  expected_sha="$(shasum -a 256 "$expected_svg" | awk '{print $1}')"
  actual_bytes="$(wc -c < "$actual_svg" | tr -d ' ')"
  expected_bytes="$(wc -c < "$expected_svg" | tr -d ' ')"

  canonical_equal="null"
  canonical_note="xmllint_unavailable"
  if [[ "$HAS_XMLLINT" == "true" ]]; then
    tmp_a="$(mktemp)"
    tmp_e="$(mktemp)"
    trap 'rm -f "$SUMMARY_TMP" "$tmp_a" "$tmp_e"' EXIT
    if xmllint --noblanks --c14n "$actual_svg" > "$tmp_a" 2>/dev/null && xmllint --noblanks --c14n "$expected_svg" > "$tmp_e" 2>/dev/null; then
      canonical_note="ok"
      if cmp -s "$tmp_a" "$tmp_e"; then
        canonical_equal="true"
        CANONICAL_PASS=$((CANONICAL_PASS + 1))
      else
        canonical_equal="false"
        CANONICAL_FAIL=$((CANONICAL_FAIL + 1))
      fi
    else
      canonical_note="c14n_failed"
    fi
    rm -f "$tmp_a" "$tmp_e"
  fi

  jq -n \
    --arg scenarioId "$scenario_id" \
    --arg expectedSvg "$expected_svg" \
    --arg actualSvg "$actual_svg" \
    --argjson strictEqual "$strict_equal" \
    --argjson canonicalEqual "$canonical_equal" \
    --arg canonicalNote "$canonical_note" \
    --arg expectedSha256 "$expected_sha" \
    --arg actualSha256 "$actual_sha" \
    --argjson expectedBytes "$expected_bytes" \
    --argjson actualBytes "$actual_bytes" \
    '{
      scenarioId:$scenarioId,
      strictEqual:$strictEqual,
      canonicalEqual:$canonicalEqual,
      canonicalNote:$canonicalNote,
      expectedSvg:$expectedSvg,
      actualSvg:$actualSvg,
      expectedSha256:$expectedSha256,
      actualSha256:$actualSha256,
      expectedBytes:$expectedBytes,
      actualBytes:$actualBytes
    }' > "$report_json"

  jq --arg id "$scenario_id" \
    --arg report "$report_json" \
    --argjson strictEqual "$strict_equal" \
    --argjson canonicalEqual "$canonical_equal" \
    '. += [{scenarioId:$id,strictEqual:$strictEqual,canonicalEqual:$canonicalEqual,report:$report}]' "$SUMMARY_TMP" > "$SUMMARY_TMP.tmp"
  mv "$SUMMARY_TMP.tmp" "$SUMMARY_TMP"

  echo "[$scenario_id] strict=$strict_equal canonical=$canonical_equal"
done

SUMMARY_JSON="$REPORTS_DIR/svg-summary.json"
SUMMARY_MD="$REPORTS_DIR/svg-summary.md"

jq -n \
  --arg generatedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg root "$ROOT" \
  --arg mode "$(if [[ -n "$SCENARIO_FILTER" ]]; then echo "single"; elif [[ "$RUN_ALL" == "true" ]]; then echo "all"; else echo "core10"; fi)" \
  --arg hasXmllint "$HAS_XMLLINT" \
  --argjson total "$TOTAL" \
  --argjson strictPass "$STRICT_PASS" \
  --argjson strictFail "$STRICT_FAIL" \
  --argjson canonicalPass "$CANONICAL_PASS" \
  --argjson canonicalFail "$CANONICAL_FAIL" \
  --argjson skipped "$SKIPPED" \
  --argjson scenarios "$(cat "$SUMMARY_TMP")" \
  '{
    generatedAt:$generatedAt,
    root:$root,
    mode:$mode,
    hasXmllint:($hasXmllint=="true"),
    total:$total,
    strictPass:$strictPass,
    strictFail:$strictFail,
    canonicalPass:$canonicalPass,
    canonicalFail:$canonicalFail,
    skipped:$skipped,
    scenarios:$scenarios
  }' > "$SUMMARY_JSON"

{
  echo "# SVG Parity Summary"
  echo
  echo "- Generated: \`$(jq -r '.generatedAt' "$SUMMARY_JSON")\`"
  echo "- Root: \`$(jq -r '.root' "$SUMMARY_JSON")\`"
  echo "- Mode: \`$(jq -r '.mode' "$SUMMARY_JSON")\`"
  echo "- xmllint available: \`$(jq -r '.hasXmllint' "$SUMMARY_JSON")\`"
  echo "- Total: \`$(jq -r '.total' "$SUMMARY_JSON")\`"
  echo "- Strict pass: \`$(jq -r '.strictPass' "$SUMMARY_JSON")\`"
  echo "- Strict fail: \`$(jq -r '.strictFail' "$SUMMARY_JSON")\`"
  echo "- Canonical pass: \`$(jq -r '.canonicalPass' "$SUMMARY_JSON")\`"
  echo "- Canonical fail: \`$(jq -r '.canonicalFail' "$SUMMARY_JSON")\`"
  echo "- Skipped: \`$(jq -r '.skipped' "$SUMMARY_JSON")\`"
  echo
  echo "## Scenarios"
  jq -r '.scenarios[] | "- `\(.scenarioId)`: strict=\(.strictEqual // "n/a"), canonical=\(.canonicalEqual // "n/a")"' "$SUMMARY_JSON"
} > "$SUMMARY_MD"

echo "wrote:"
echo "  $SUMMARY_JSON"
echo "  $SUMMARY_MD"

if [[ "$REQUIRE_PASS" == "true" ]]; then
  if jq -e '.strictFail == 0 and .skipped == 0' "$SUMMARY_JSON" >/dev/null; then
    echo "all SVGs are strict-identical"
  else
    echo "one or more SVG strict comparisons failed/skipped (failing due to --require-pass)" >&2
    exit 1
  fi
fi
