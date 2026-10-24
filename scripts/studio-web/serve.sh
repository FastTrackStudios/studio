#!/usr/bin/env bash
# studio-web's start step (#220): the image's entrypoint, and what
# `just studio-web-preview` runs.
#
#   serve.sh <root> <sws config>
#
# Writes <root>/config/config.json from TASK_URL / AUTH_URL
# (write-config.sh), then runs static-web-server, whose config answers
# `/config.json` from that file (a rewrite). The tree itself is read-only;
# `config/` is the one writable directory in it (in the image, a tmpfs or
# emptyDir mounts there). It has to be inside the root: static-web-server
# refuses a file, or a symlink, that resolves outside it.
set -euo pipefail
root="${1:?usage: serve.sh <root> <sws config>}"
conf="${2:?usage: serve.sh <root> <sws config>}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

mkdir -p "$root/config"
"${STUDIO_WEB_WRITE_CONFIG:-$here/write-config.sh}" > "$root/config/config.json.new"
mv "$root/config/config.json.new" "$root/config/config.json"

echo "studio-web: serving $root, config $(cat "$root/config/config.json")" >&2
exec static-web-server --config-file "$conf"
