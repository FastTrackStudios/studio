#!/usr/bin/env bash
# The local stack (docs/local-stack.md): `devenv up` runs these, each through
# this script:
#
#   engine   Studio's headless engine (`studio engine`), a process of the
#            stack: every window attaches to it (FTS_ENGINE=attach), so a
#            window's rebuild never takes the audio down. It opens
#            [studio] open.
#   studio   Studio: Studio Architect, the app (Architect with Studio's
#            extensions, Lighting and the DAW view linked in), `dx serve`.
#            It is the server too (STUDIO_SERVE=1: its node in its own
#            process, holding a copy of the test vault and its share link).
#   web      Studio Web: Architect's web app under `dx serve --platform
#            web` (`cargo xtask dev run web` in the Architect checkout),
#            which proxies the node's paths (/s /vox /org /node /_ext) to
#            Studio. A share link (/s/<code>/) is the web app's page
#            (Architect #646): opened on Studio Web's origin, it loads
#            /app/start.js and the app from that same origin.
#   studio-desktop-legacy   (off; `devenv up studio-desktop-legacy`) the old
#            Studio window, until it is merged into Studio Architect.
#
#   studio-dev.sh run engine|studio|web|studio-desktop-legacy
#   studio-dev.sh ready engine|studio|web   devenv's readiness probe
#   studio-dev.sh plan                which Architect and Keyflow, the overrides
#   studio-dev.sh status              what runs, from where, the URLs
#   studio-dev.sh url                 the share link
#   studio-dev.sh sync [watch]        the vault copy, again from the source
#   studio-dev.sh phone [off]         Studio Web on the tailnet over HTTPS
#   studio-dev.sh reset               remove the stack's state (stopped)
#
# Settings: local-stack.toml, this machine's local-stack.local.toml
# (gitignored; LOCAL_STACK_LOCAL names another) merged over it, and the
# LOCAL_STACK_* variables over both. A local Architect or Keyflow goes
# under the build as a [patch] in a generated cargo config (`--config`,
# under the state), never in Cargo.toml; every build puts Cargo.lock back.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
self="$root/scripts/studio-dev.sh"

ARCH_URL=https://github.com/FastTrackStudios/architect
KF_URL=https://github.com/FastTrackStudios/keyflow
devenv_state="$root/.devenv/state"
if [ "${DEVENV_ROOT:-}" = "$root" ] && [ -n "${DEVENV_STATE:-}" ]; then
    devenv_state="$DEVENV_STATE"
fi
# `STUDIO_DEV_STATE` puts a second stack of this checkout beside the first.
state="${STUDIO_DEV_STATE:-$devenv_state/studio-dev}"
mkdir -p "$state"
chmod 700 "$state"

say() { printf 'studio-dev: %s\n' "$*" >&2; }
die() { say "$*"; exit 1; }
py() { python3 "$root/scripts/local-stack.py" "$@"; }

# A setting: the variable VAR when set, else KEY, else the default given.
setting() {
    local var="$1" key="$2"
    if [ -n "${!var:-}" ]; then echo "${!var}"; else py get "$key" "${@:3}"; fi
}

node_port() { setting LOCAL_STACK_PORT_DESKTOP studio.node_port; }
web_port() { setting LOCAL_STACK_PORT_STUDIO_WEB studio.web_port; }
origin() { echo "http://127.0.0.1:$(web_port)"; }
host_triple() { rustc -vV | sed -n 's/^host: //p'; }

target_dir() {
    [ -n "${_target:-}" ] || _target="$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    echo "$_target"
}

main_checkout() { dirname "$(git rev-parse --path-format=absolute --git-common-dir)"; }

# A path from the settings: absolute, else from this checkout, else from
# its main checkout.
locate() {
    local path="$1" base
    [ -n "$path" ] || return 0
    case "$path" in /*) [ -e "$path" ] && echo "$path"; return 0 ;; esac
    for base in "$root" "$(main_checkout)"; do
        if [ -e "$base/$path" ]; then (cd "$base/$path" && pwd); return 0; fi
    done
}

describe() { git -C "$1" describe --always --dirty --tags 2>/dev/null || echo "?"; }

# ── Which Architect, which Keyflow ───────────────────────────────────────
overrides="$state/overrides.toml"

# A local checkout for `local` or a path; empty for `pinned`.
local_source() {
    local name="$1" value path var
    var="LOCAL_STACK_$(tr '[:lower:]' '[:upper:]' <<<"$name")"
    value="$(setting "$var" "choose.$name" pinned)"
    case "$value" in
        pinned) return 0 ;;
        local) path="$(locate "$(setting "${var}_PATH" "sources.$name")")" ;;
        *) path="$(locate "$value")" ;;
    esac
    [ -n "$path" ] && [ -f "$path/Cargo.toml" ] || die "$name=$value: no $name checkout there (local-stack.local.toml [sources] $name)"
    echo "$path"
}

# The overrides (empty: what Cargo.lock pins) and where Architect is.
arch_local="" kf_local="" arch_src=""
plan() {
    (
        flock 7
        local arch kf
        arch="$(local_source architect)"
        kf="$(local_source keyflow)"
        : >"$overrides.new"
        if [ -n "$arch" ]; then py patch "$ARCH_URL" "$arch" >>"$overrides.new"; fi
        if [ -n "$kf" ]; then py patch "$KF_URL" "$kf" >>"$overrides.new"; fi
        mv "$overrides.new" "$overrides"
        {
            printf 'arch_local=%q\n' "$arch"
            printf 'kf_local=%q\n' "$kf"
            # The Architect checkout Studio Web runs from: the local one,
            # else the pinned tag's (scripts/local-stack.sh keeps it).
            printf 'arch_src=%q\n' "${arch:-$("$root/scripts/local-stack.sh" _pinned-architect)}"
        } >"$state/plan.env"
    ) 7>"$state/plan.flock"
    # shellcheck source=/dev/null
    . "$state/plan.env"
}



# ── The vault ────────────────────────────────────────────────────────────
vault_id=studio
vault_source() { setting LOCAL_STACK_VAULT vault.source ''; }
vault_setlist() { setting LOCAL_STACK_SETLIST vault.setlist; }
share_code() { setting LOCAL_STACK_SHARE_CODE vault.share_code; }
vault_copy() { echo "$state/vault"; }

# A copy of the test vault with its share link, made once (`sync` copies
# it again). Studio opens it and its node serves it.
seed_vault() {
    local src dst out
    src="$(locate "$(vault_source)")"
    dst="$(vault_copy)"
    if [ -z "$src" ]; then
        say "NOTICE: no test vault at '$(vault_source)': Studio opens an empty vault, there is no share link"
        rm -f "$state/share"
        mkdir -p "$dst"
        return 0
    fi
    if [ ! -d "$dst" ]; then
        say "copying the test vault $src into $dst (once; just dev-sync copies it again)"
        mkdir -p "$dst.tmp"
        cp -a --reflink=auto "$src/." "$dst.tmp/"
        mv "$dst.tmp" "$dst"
    fi
    out="$(py seed-share "$dst" "$(vault_setlist)" "$(share_code)" "$vault_id")"
    echo "$(share_code) ${out#* }" >"$state/share"
}

# What changed in the source comes in, what was deleted there goes, the
# share note is written again; what Architect keeps in the vault
# (_architect/, .architect/) stays the copy's own.
sync_vault() {
    local src
    src="$(locate "$(vault_source)")"
    [ -n "$src" ] || die "no test vault at '$(vault_source)'"
    [ -d "$(vault_copy)" ] || { seed_vault; return; }
    rsync -a --delete --exclude=/_architect/ --exclude=/.architect/ "$src/" "$(vault_copy)/"
    say "synced $src -> $(vault_copy)"
    seed_vault
}

share_url() {
    local origin="${1:-$(origin)}" code page
    [ -f "$state/share" ] || return 0
    read -r code page <"$state/share"
    echo "$origin/s/$code/$page/"
}

# ── The engine and Studio ───────────────────────────────────────────────

# A program built from this tree, copied to bin/ (a stable path and
# mtime: what runs from there is not swapped under it by the next build)
# when it changed.
stable() {
    local built="$1" name
    name="$(basename "$built")"
    mkdir -p "$state/bin"
    if ! cmp -s "$built" "$state/bin/$name"; then
        cp --reflink=auto "$built" "$state/bin/$name.new"
        mv "$state/bin/$name.new" "$state/bin/$name"
    fi
    echo "$state/bin/$name"
}

# Studio's island (`just island`), copied beside the node as it ships
# (never linked: the node refuses a linked island).
island() {
    local built
    built="$(target_dir)/studio-island"
    just island-build >&2 || { say "the island's build failed: share pages keep Architect's own bar"; return 0; }
    [ -f "$built/architect.lock.json" ] || return 0
    rm -rf "$state/studio-island.new"
    mkdir -p "$state/studio-island.new"
    find "$built" -maxdepth 1 -type f -exec cp --reflink=auto {} "$state/studio-island.new/" \;
    rm -rf "$state/studio-island"
    mv "$state/studio-island.new" "$state/studio-island"
}

# The engine's profile: the XDG data/config home Studio's engine and every
# window of the stack share. The engine registers there
# ($XDG_CONFIG_HOME/fts/engines), so the stack's windows find it and never
# another Studio's (nor does that one find the stack's).
profile_dir() {
    local profile
    profile="$(setting LOCAL_STACK_PROFILE_DIR studio.profile '')"
    echo "${profile:-$state/profile}"
}

host_out() { echo "$(target_dir)/$(host_triple)/desktop-dev"; }

# A build of this tree for the dev loop (desktop-dev, the host triple),
# under the plan's overrides, below everything else, Cargo.lock as it was
# after.
dev_build() {
    local cfg=() held="$state/Cargo.lock.$$"
    [ -s "$overrides" ] && cfg=(--config "$overrides")
    cp Cargo.lock "$held"
    local rc=0
    nice -n 19 cargo build -q --profile desktop-dev --target "$(host_triple)" "${cfg[@]}" "$@" || rc=$?
    cmp -s Cargo.lock "$held" || cp "$held" Cargo.lock
    rm -f "$held"
    return "$rc"
}

# What a window of the stack runs with to reach the engine: attach to the
# stack's engine (the `engine` process), never spawn or replace one.
engine_env() {
    local profile
    profile="$(profile_dir)"
    printf '%s\n' \
        FTS_ENGINE=attach \
        FTS_STUDIO_BIN="$state/bin/studio" \
        FTS_MCP_PORT="$(setting LOCAL_STACK_MCP_PORT studio.engine_mcp_port)" \
        XDG_DATA_HOME="$profile/data" \
        XDG_CONFIG_HOME="$profile/config"
}

# The socket of the stack's running engine (its registration's `socket`).
engine_socket() {
    local f
    for f in "$(profile_dir)"/config/fts/engines/studio-*.json; do
        [ -f "$f" ] || continue
        python3 -c '
import json, os, sys
d = json.load(open(sys.argv[1]))
os.kill(d["pid"], 0)
print(d["socket"])' "$f" 2>/dev/null && return 0
    done
    return 1
}

# The stack's running engine, by its registration in the profile.
engine_up() {
    local f pid
    for f in "$(profile_dir)"/config/fts/engines/studio-*.json; do
        [ -f "$f" ] || continue
        pid="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pid"])' "$f" 2>/dev/null)" || continue
        kill -0 "$pid" 2>/dev/null && return 0
    done
    return 1
}

# The engine: Studio's headless engine (`studio engine`), a process of the
# stack, from a stable copy of the `studio` binary, so a rebuild of a window
# never takes it down: the window attaches to it again. When it is up it
# opens [studio] open (`studio open`, through its Action List).
run_engine() {
    plan
    local profile open studio_bin pid
    profile="$(profile_dir)"
    mkdir -p "$profile/data" "$profile/config"
    if engine_up; then
        die "an engine already runs in the profile $profile (another Studio, or a stack not stopped): stop it, or give the stack another [studio] profile"
    fi
    say "engine: building studio (the CLI that is the engine)"
    dev_build -p studio-desktop --features cli --bin studio
    studio_bin="$(stable "$(host_out)/studio")"
    open="$(setting LOCAL_STACK_OPEN studio.open '')"
    local env
    mapfile -t env < <(engine_env)
    env+=(RUST_LOG="${RUST_LOG:-info,vox_core=warn,schema_deser=off,architect::layer=warn}")
    say "engine: $studio_bin engine, profile $profile, its Action List on :$(setting LOCAL_STACK_MCP_PORT studio.engine_mcp_port)"
    env "${env[@]}" "$studio_bin" engine &
    pid=$!
    trap 'kill -TERM "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; exit 0' TERM INT
    if [ -n "$open" ]; then
        (
            for _ in $(seq 1 240); do engine_up && break; sleep 0.5; done
            if env "${env[@]}" "$studio_bin" open "$open" >&2; then
                say "engine: opened $open"
            else
                say "NOTICE: the engine did not open $open (studio open); open it from Studio"
            fi
        ) &
    fi
    wait "$pid"
}

# Studio: Studio Architect, the app (Architect with Studio's extensions,
# Lighting and the DAW view linked in), under `dx serve`. It is the server
# too (STUDIO_SERVE=1: its node in its own process, holding the vault copy
# and its share link, which Studio Web proxies), and it attaches to the
# stack's engine.
run_studio() {
    plan
    seed_vault
    local host out live_set ignition_bin lock_held="$state/Cargo.lock.held" web
    host="$(host_triple)"
    out="$(host_out)"
    web="$(web_port)"
    [ -n "$arch_local" ] || say "NOTICE: Architect is pinned: Lighting and STUDIO_SERVE need Architect's main (local-stack.local.toml [choose] architect = \"local\")"

    say "studio: building Lighting's renderer (ignition) and Studio's island"
    dev_build -p ignition
    ignition_bin="$(stable "$out/ignition")"
    island
    live_set="$(setting LOCAL_STACK_LIVE_SET studio.live_set '')"
    mkdir -p "$state/app" "$state/node"

    local env
    mapfile -t env < <(engine_env)
    env+=(
        # The server, in the app's process (Architect's node, its
        # variables as `serve` reads them): no issuer, no account, loopback
        # only; the web app beside it on Studio Web's origin.
        STUDIO_SERVE=1
        ARCHITECT_NAME=studio-dev
        ARCHITECT_BIND="127.0.0.1:$(node_port)"
        ARCHITECT_DATA_DIR="$state/node"
        ARCHITECT_VAULTS="$vault_id=$(vault_copy)"
        ARCHITECT_IROH=local
        ARCHITECT_WEB_APP=beside
        ARCHITECT_LANE_ORIGINS="http://127.0.0.1:$web,http://localhost:$web"
        # The app: its vault the same copy, its settings and secrets the
        # stack's, never the person's own app's; an instance of its own.
        ARCHITECT_VAULT="$(vault_copy)"
        ARCHITECT_CONFIG_DIR="$state/app"
        ARCHITECT_SECRET_STORE="file:$state/app/secrets"
        ARCHITECT_FORBID_KEYCHAIN=1
        STUDIO_INSTANCE=dev
        STUDIO_ISLAND_DIR="$state/studio-island"
        # Lighting (`ignition.*` settings; these when they are empty).
        IGNITION_ROOT="$root/features/lighting"
        IGNITION_BIN="$ignition_bin"
        RUST_LOG="${RUST_LOG:-info,vox_core=warn,schema_deser=off,architect::layer=warn}"
    )
    [ -n "$live_set" ] && env+=(IGNITION_LIVE_SET="$live_set")
    # Which engine the setlist's clock and Lighting's renderer follow: the
    # stack's, never the newest on the machine. Set, the app takes it and
    # skips its own engine link at launch.
    local socket
    if socket="$(engine_socket)"; then env+=(STUDIO_ENGINE_SOCKET="$socket"); fi

    say "studio: $root @ $(describe "$root"); architect ${arch_local:-pinned}; keyflow ${kf_local:-pinned}"
    say "  serving on 127.0.0.1:$(node_port) (Studio Web proxies it); the engine in $(profile_dir)"
    [ -f "$state/share" ] && say "  the share link: $(share_url)"
    local dx_args=(serve -p studio-architect --bin studio-architect --platform desktop --renderer native
        --profile desktop-dev --target "$host"
        --hot-patch false --interactive false --open true)
    # Studio Architect's features beyond its defaults ([studio] features):
    # `daw`, the DAW view and the engine link, until it is a default.
    local features
    features="$(setting LOCAL_STACK_STUDIO_FEATURES studio.features '')"
    [ -n "$features" ] && dx_args+=(--features "$features")
    # One token, joined with `=`: dx reads a separate `--…` value as a flag
    # of its own, and hands --cargo-args to cargo as a single argument.
    [ -s "$overrides" ] && dx_args+=("--cargo-args=--config=$overrides")
    # Cargo writes a local Architect's sources into Cargo.lock while dx
    # builds: the committed one comes back when this process stops.
    cp Cargo.lock "$lock_held"
    env "${env[@]}" nice -n 10 dx "${dx_args[@]}" &
    local pid=$!
    trap 'kill -TERM "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; cmp -s Cargo.lock "$lock_held" || cp "$lock_held" Cargo.lock; exit 0' TERM INT
    wait "$pid"
    cmp -s Cargo.lock "$lock_held" || cp "$lock_held" Cargo.lock
}

# TEMPORARY, until Studio desktop is merged into Studio Architect: the old
# Studio window (studio-desktop, the DAW view), attached to the stack's
# engine, opening [studio] open.
run_studio_desktop_legacy() {
    plan
    local open env
    dev_build -p studio-desktop --features cli,signal-keys-rig --bin studio-desktop
    open="$(setting LOCAL_STACK_OPEN studio.open '')"
    mapfile -t env < <(engine_env)
    env+=(RUST_LOG="${RUST_LOG:-info,vox_core=warn,schema_deser=off,architect::layer=warn}")
    [ -n "$open" ] && env+=(FTS_STUDIO_OPEN="$open")
    say "studio-desktop-legacy: the old Studio window, attached to the stack's engine${open:+, opening $open}"
    exec env "${env[@]}" "$(host_out)/studio-desktop"
}

# ── Studio Web ───────────────────────────────────────────────────────────

# Architect's web app under dx, as Architect's own dev runs it (`cargo
# xtask dev run web`: the dev page, its public folder, `dx serve
# --platform web` in apps/web). Its proxy (apps/web/Dioxus.toml) sends the
# node's paths to 127.0.0.1:7412, Studio's node port. The page
# says it is Studio Architect: dx is reached through a shim that puts
# Studio's /app/distribution.json into the public folder xtask just wrote.
# Studio's extensions are not in Architect's web build: a
# studio-architect-web package is the gap (docs/design/studio-in-architect.md).
run_web() {
    plan
    local shim="$state/web-shim" real json="$state/distribution.json"
    [ "$(node_port)" = 7412 ] || say "NOTICE: Studio serves on $(node_port), but Architect's web app proxies to 7412 (apps/web/Dioxus.toml): share links will not load"
    real="$(command -v dx)"
    mkdir -p "$shim"
    # Studio's distribution, as its binary describes it: written into the
    # web app's public folder (and dx's copy of it) once Studio's build
    # answers, whenever that is; until then the page names Architect's own.
    local app web_target
    app="$(target_dir)/$(host_triple)/desktop-dev/studio-architect"
    web_target="$(target_dir)/local-stack/architect/target"
    (
        for _ in $(seq 1 1800); do
            if [ -x "$app" ] && "$app" distribution >"$json.new" 2>/dev/null && [ -s "$json.new" ]; then
                mv "$json.new" "$json"
                for d in "$arch_src/apps/web/dev/public/app" "$web_target"/dx/architect-web/*/web/public/app; do
                    [ -d "$d" ] && cp "$json" "$d/distribution.json"
                done
                say "Studio Web: /app/distribution.json is Studio Architect's"
                exit 0
            fi
            sleep 5
        done
        say "NOTICE: no Studio Architect build answered: the page names Architect's own distribution"
    ) &
    cat >"$shim/dx" <<SH
#!/usr/bin/env bash
# Written by scripts/studio-dev.sh: Studio's distribution.json into the
# public folder xtask prepared, then the real dx.
[ -d dev/public/app ] && [ -f "$json" ] && cp "$json" dev/public/app/distribution.json
exec "$real" "\$@"
SH
    chmod +x "$shim/dx"
    cd "$arch_src"
    local vars=(
        DEV_STACK_ROOT="$arch_src"
        DEV_STACK_STATE="$state/architect-dev"
        DEVENV_STATE="$state"
        DEV_STACK_PORT_WEB="$(web_port)"
        DEV_STACK_PORT_NODE="$(node_port)"
        PATH="$shim:$PATH"
    )
    # A local Architect's xtask builds into this checkout's target dir,
    # never into that checkout.
    # (the legacy stack's, so the two share it).
    [ -n "$arch_local" ] && vars+=(CARGO_TARGET_DIR="$(cd "$root" && target_dir)/local-stack/architect/target")
    say "Studio Web: $(origin)/app/ ($arch_src @ $(describe "$arch_src")), the node's paths to 127.0.0.1:$(node_port)"
    exec env "${vars[@]}" nice -n 10 cargo run -q -p xtask -- dev run web
}

# ── Status, phone, reset ─────────────────────────────────────────────────
up() { curl -s -o /dev/null --max-time 1 "$1" 2>/dev/null; }

status() {
    local n w
    n="http://127.0.0.1:$(node_port)"
    w="$(origin)"
    printf '  %-8s %-28s %s\n' engine "$(profile_dir)" "$(engine_up && echo 'running' || echo 'not running')"
    printf '  %-8s %-28s %s\n' studio "$n" "$(up "$n/health" && echo 'serving' || echo 'not serving')"
    printf '  %-8s %-28s %s\n' web "$w/app/" "$(up "$w/app/" && echo 'running' || echo 'not running')"
    echo
    [ -f "$state/share" ] && echo "  share link  $(share_url)"
    echo "  vault       $(vault_copy) (from $(vault_source))"
    echo "  state       $state"
}

tailnet_name() {
    tailscale status --self --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["Self"]["DNSName"].rstrip("."))'
}

phone() {
    command -v tailscale >/dev/null || die "no tailscale on this machine"
    local https name web out
    https="$(py get phone.https_port)"
    web="$(web_port)"
    if [ "${1:-}" = off ]; then
        tailscale serve --https="$https" off
        say "phone: off (https port $https no longer served)"
        return
    fi
    name="$(tailnet_name)"
    if tailscale serve status 2>/dev/null | grep -A1 "^https://$name:$https" | grep -q proxy \
        && ! tailscale serve status | grep -A1 "^https://$name:$https" | grep -q "127.0.0.1:$web"; then
        die "https://$name:$https already serves something else: set [phone] https_port (Tailscale serves 443, 8443, 10000)"
    fi
    up "$(origin)/app/" || die "Studio Web is not running (devenv up first)"
    if ! out="$(tailscale serve --bg --https="$https" "$(origin)" 2>&1)"; then
        grep -q 'Access denied' <<<"$out" && die "tailscale refused: once, as root: sudo tailscale set --operator=$USER"
        die "tailscale serve failed: $out"
    fi
    say "phone: Studio Web on this tailnet only (tailscale serve; never funnel). Open on the phone:"
    share_url "https://$name:$https"
}

reset() {
    if up "http://127.0.0.1:$(node_port)/health" || engine_up; then die "the stack is running: stop devenv up first"; fi
    rm -rf "$state"
    say "removed $state"
}

cmd="${1:-}"
shift || true
case "$cmd" in
    run)
        case "${1:-}" in
            engine) run_engine ;;
            studio) run_studio ;;
            web) run_web ;;
            studio-desktop-legacy) run_studio_desktop_legacy ;;
            *) die "run: engine, studio, web or studio-desktop-legacy" ;;
        esac
        ;;
    ready)
        case "${1:-}" in
            engine) engine_up ;;
            studio) curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(node_port)/health" ;;
            web) curl -sf -o /dev/null --max-time 2 "$(origin)/app/" ;;
            *) exit 1 ;;
        esac
        ;;
    status) status ;;
    plan) plan && say "architect ${arch_local:-pinned}, keyflow ${kf_local:-pinned}; Studio Web from $arch_src; overrides $overrides" ;;
    url) share_url "$@" ;;
    sync)
        if [ "${1:-}" = watch ]; then
            src="$(locate "$(vault_source)")"
            [ -n "$src" ] || die "no test vault at '$(vault_source)'"
            sync_vault
            say "watching $src: every change is synced (Ctrl-C stops)"
            exec cargo watch -q --postpone --delay 1 -w "$src" -s "'$self' sync"
        fi
        sync_vault
        ;;
    phone) phone "$@" ;;
    reset) reset ;;
    *)
        sed -n '2,34p' "$0" >&2
        exit 64
        ;;
esac
