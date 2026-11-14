#!/usr/bin/env bash
# Run null_suite.sh for many sections at once, every (section, mode) pair as
# its own job, JOBS (default 12) side by side:
#
#   features/rigs/orchestra/tests/null_all.sh "1st Violins" "Violas" …
#   MODES="probe legato" JOBS=16 features/rigs/orchestra/tests/null_all.sh "Solo Flute"
#
# Each pair writes its own target/orchestra-ab/sec/<tag>/<mode>/, so the jobs
# never share a file; the strings regression (5 sections × 10 modes) takes
# ~6 minutes this way instead of ~100 one after another. Env as null_suite.sh
# (SKIP_KONTAKT=1 to reuse the Kontakt renders). Prints one line per pair.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
jobs="${JOBS:-12}"
read -r -a modes <<< "${MODES:-probe legato leg2 marcato marc2 shorts pizz trem rebow attack}"
[ $# -gt 0 ] || { echo "usage: null_all.sh <Section>..." >&2; exit 1; }
# shellcheck disable=SC2016  # the job script expands its own variables
for s in "$@"; do
  for m in "${modes[@]}"; do printf '%s\t%s\n' "$s" "$m"; done
done | NULL_SUITE="$here/null_suite.sh" xargs -P "$jobs" -d '\n' -n1 bash -c '
  IFS=$'"'\t'"' read -r s m <<< "$0"
  out=$(timeout 2400 "$NULL_SUITE" "$s" "$m" 2>&1 | grep "^all" || true)
  printf "%-20s %-8s %s\n" "$s" "$m" "${out:-FAILED}"'
