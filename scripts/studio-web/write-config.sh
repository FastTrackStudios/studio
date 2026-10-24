#!/usr/bin/env bash
# studio-web's /config.json, from the container's environment (#219, #220):
#
#   TASK_URL=https://task.example AUTH_URL=https://auth.example write-config.sh
#   {"task_url":"https://task.example","auth_url":"https://auth.example"}
#
# A variable that is unset or empty is left out, and the apps then dial
# their own origin for it (features/app/shell/src/web_config.rs): with
# neither set this writes `{}`, the same-origin setup. Values are written as
# given (JSON-escaped by jq); the apps ignore one that is not an http(s) URL.
set -euo pipefail
jq -cn --arg task "${TASK_URL:-}" --arg auth "${AUTH_URL:-}" '
  {}
  + (if $task != "" then { task_url: $task } else {} end)
  + (if $auth != "" then { auth_url: $auth } else {} end)'
