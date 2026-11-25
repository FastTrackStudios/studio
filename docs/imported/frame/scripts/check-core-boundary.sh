#!/usr/bin/env bash
# Frame's core (`frame`) must not depend on architect, vox, Dioxus or
# figcraft: plugin GUIs and non-architect hosts link only the core, and
# architect never depends on Frame (frame#5). Checks the normal dependency
# tree with every core feature on.
set -euo pipefail
tree=$(cargo tree -p frame -e normal --all-features --prefix none --format '{p}')
bad=$(grep -E '^(architect|vox|dioxus|figcraft|crdt)[a-z0-9_-]* ' <<<"$tree" | sort -u || true)
if [ -n "$bad" ]; then
  echo "frame (core) must not depend on these:" >&2
  echo "$bad" >&2
  exit 1
fi
echo "frame core: $(sort -u <<<"$tree" | wc -l) packages, none of architect/vox/dioxus/figcraft/crdt"
