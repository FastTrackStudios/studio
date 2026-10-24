#!/usr/bin/env bash
# studio-web's start step (#220): the image's entrypoint, and what
# `just studio-web-preview` runs.
#
#   serve.sh <root> <sws config template>
#
# The root is in the nix store, read-only, and /config.json is the
# container's to write. So the served root is a small directory under
# $STUDIO_WEB_RUNTIME_DIR (default /run/studio-web, writable in the image):
# a symlink to each entry of <root> (static-web-server follows them) beside
# the config.json written from TASK_URL / AUTH_URL (write-config.sh). The
# template's `@root@` becomes that directory.
set -euo pipefail
root="${1:?usage: serve.sh <root> <sws template>}"
template="${2:?usage: serve.sh <root> <sws template>}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
run="${STUDIO_WEB_RUNTIME_DIR:-/run/studio-web}"
served="$run/root"

rm -rf "$served"
mkdir -p "$served"
for entry in "$root"/* "$root"/.[!.]*; do
  [ -e "$entry" ] || continue
  ln -s "$entry" "$served/$(basename "$entry")"
done
# The root's own config.json, if a build ever ships one, is replaced: the
# environment is the only runtime input.
rm -f "$served/config.json"
"${STUDIO_WEB_WRITE_CONFIG:-$here/write-config.sh}" > "$served/config.json"
sed "s|@root@|$served|g" "$template" > "$run/sws.toml"

echo "studio-web: serving $root, config $(cat "$served/config.json")" >&2
exec static-web-server --config-file "$run/sws.toml"
