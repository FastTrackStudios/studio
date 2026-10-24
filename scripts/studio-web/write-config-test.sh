#!/usr/bin/env bash
# The config writer's cases (`just studio-web-config-test`, and the
# `studio-web-config-test` flake check). Needs bash and jq.
# shellcheck disable=SC2016 # the `$(id)` case is literal on purpose: it must come out as text
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
w="$here/write-config.sh"
pass=0 fail=0
case_() { # <name> <want> [VAR=value...]
  local name="$1" want="$2" got; shift 2
  got="$(env -u TASK_URL -u AUTH_URL "$@" bash "$w")"
  if [ "$got" = "$want" ]; then pass=$((pass + 1)); else
    fail=$((fail + 1)); echo "FAIL: $name: got '$got', wanted '$want'"
  fi
}
case_ "neither set: same origin" '{}'
case_ "both empty: same origin" '{}' TASK_URL= AUTH_URL=
case_ "task only" '{"task_url":"https://task.example"}' TASK_URL=https://task.example
case_ "auth only" '{"auth_url":"https://auth.example"}' AUTH_URL=https://auth.example
case_ "both" '{"task_url":"https://task.example","auth_url":"https://auth.example"}' \
  TASK_URL=https://task.example AUTH_URL=https://auth.example
case_ "escaped, not interpreted" '{"task_url":"https://x.example/\"},{\"a\":\"$(id)"}' \
  'TASK_URL=https://x.example/"},{"a":"$(id)'
echo "write-config: $pass passed, $fail failed"
[ "$fail" = 0 ]
