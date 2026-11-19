# Lighting (Ignition) — `just lighting <recipe>`. Recipes run from the
# checkout root; the runner finds the lighting data itself.

set positional-arguments

# The recital's Live set. Not in the repo; override with IGNITION_LIVE_SET.
live_set := env_var_or_default("IGNITION_LIVE_SET", "/run/media/Development/fts-dev/recital/show/Sr. Recital Showcase Project/Sr. Recital Showcase.als")

# The dev loop's build (kache keys): desktop-dev for the host triple.
_build := "--profile desktop-dev --target $(rustc -vV | sed -n 's/^host: //p')"

# Regenerate JT's recital show from the Live set (data/songs/jt-recital, data/shows/jt-recital.ig-show).
author-recital:
    python3 -I tools/author_recital.py "{{live_set}}"

# Load the night and print its running order; `just lighting check-recital --at 3122` reports a beat.
check-recital *ARGS:
    cargo run {{_build}} -p ignition -- --set "{{live_set}}" --check "$@"

# The recital in the LAB (Room 138): --clock studio follows the Studio app, --clock midi:<port> Ableton; --output drives the rig.
recital *ARGS:
    cargo run {{_build}} -p ignition -- --set "{{live_set}}" "$@"

# Studio Architect, Lighting linked in (Linux): open the pane with the action "Lighting: show the room".
architect *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    host="$(rustc -vV | sed -n 's/^host: //p')"
    nice -n 19 cargo build --profile desktop-dev --target "$host" -p ignition -p studio-architect
    target="$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    export IGNITION_ROOT="$PWD/features/lighting"
    export IGNITION_LIVE_SET="{{live_set}}"
    exec "$target/$host/desktop-dev/studio-architect" "$@"

# EXT_CLONE Lighting from this checkout into the running Studio Architect: it builds the extension's process copy from source and overrides the built-in under the same id `ignition` (rebuilt on save).
pane-dev:
    #!/usr/bin/env bash
    set -euo pipefail
    host="$(rustc -vV | sed -n 's/^host: //p')"
    target="$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    sa="$target/$host/desktop-dev/studio-architect"
    folder="$("$sa" dev-source ignition)"
    echo "Lighting's source: $folder (edit it there; each save rebuilds)"
    "$sa" actions run EXT_CLONE --arg id=ignition

# Back to the built-in Lighting (the development copy's folder stays).
pane-undev:
    #!/usr/bin/env bash
    set -euo pipefail
    host="$(rustc -vV | sed -n 's/^host: //p')"
    target="$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    sa="$target/$host/desktop-dev/studio-architect"
    "$sa" extensions undev "$("$sa" dev-source ignition)"

# Check Lighting's manifest the way the app loads it (EXT_CHECK).
pane-check:
    #!/usr/bin/env bash
    set -euo pipefail
    host="$(rustc -vV | sed -n 's/^host: //p')"
    target="$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    "$target/$host/desktop-dev/studio-architect" extensions check features/lighting/pane

# The visualizer on its own, e.g. `just lighting viz --venue data/venues/room138-cbu`.
viz *ARGS:
    cd features/lighting && cargo run {{_build}} -p ignition-viz --bin viz -- "$@"

# The lighting crates' tests (the domain; the visualizer's need a GPU).
test:
    cargo nextest run -p ignition-pane -p ignition-proto -p ignition-daw-proto -p ignition-colour -p ignition-rig -p ignition-show -p ignition-effects -p ignition-playback -p ignition-core -p ignition-dmx -p ignition-daw -p ignition-fixture

# The recital's light show as videos, one song at a time (the lit songs, or the slugs given):
# <recital>/videos/<slug>.mp4, 1920x1080 30 fps, the Wide camera, with the song's audio
# (recordings/<slug>.ogg, else refs/). Slow and offline: an hour or more for the night.
render-recital *ARGS:
    cargo build {{_build}} -p ignition
    IGNITION_BIN="${CARGO_TARGET_DIR:-$(cd ../.. && pwd)/target}/$(rustc -vV | sed -n 's/^host: //p')/desktop-dev/ignition" \
    IGNITION_FFMPEG="${IGNITION_FFMPEG:-$(nix build --no-link --print-out-paths nixpkgs#ffmpeg-headless | head -1)/bin/ffmpeg}" \
    IGNITION_LIVE_SET="{{live_set}}" python3 -I tools/render_recital.py render "$@"

# Re-mux the recital videos whose audio changed (a newer recordings/<slug>.ogg): the picture is kept.
remux-recital *ARGS:
    IGNITION_FFMPEG="${IGNITION_FFMPEG:-$(nix build --no-link --print-out-paths nixpkgs#ffmpeg-headless | head -1)/bin/ffmpeg}" \
    python3 -I tools/render_recital.py remux "$@"
