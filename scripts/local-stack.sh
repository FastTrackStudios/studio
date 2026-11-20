#!/usr/bin/env bash
# The local stack (docs/local-stack.md): `devenv up` runs each process
# through this script; the just recipes (`just dev`, `just demo`) choose
# what is local and start it. What runs:
#
#   mock-oauth  Architect's stand-in for GitHub/Google (the sign-in's buttons)
#   auth        Architect's auth server: the issuer everyone signs in at
#   architect   THE SERVER: plain Architect, `architect-desktop serve`, no
#               Studio code in it; the demo vault and its share links
#   web         studio-web (apps/web) under `dx serve`, behind the
#               front that joins it and the server's paths on one origin
#   desktop     Studio's desktop app (apps/desktop) under `dx serve`, a
#               client of the server; it starts its engine (`daw`) itself
#
#   local-stack.sh choose [architect=pinned|local|PATH] [keyflow=…]
#                         [desktop=on|off] [mode=dev|demo]
#                                         what the next `devenv up` runs
#   local-stack.sh plan                   resolve it, write the overrides, say it
#   local-stack.sh procs                  the devenv processes the choice starts
#   local-stack.sh run <process>          one process, as devenv.nix starts it
#   local-stack.sh status                 what runs, from where, the URLs
#   local-stack.sh tailnet [off]          studio-web on the tailnet over HTTPS
#   local-stack.sh smoke                  the stack, checked end to end
#   local-stack.sh reset                  remove the stack's state
#
# local-stack.local.env (untracked) holds a checkout's own LOCAL_STACK_*
# settings: LOCAL_STACK_OVERRIDES + LOCAL_STACK_ARCHITECT_PATH (a stack of
# local checkouts), LOCAL_STACK_VAULT, LOCAL_STACK_SETLIST,
# LOCAL_STACK_PORT_<NAME>, LOCAL_STACK_OPEN=on (the web client in the
# browser), LOCAL_STACK_LAN=on, LOCAL_STACK_TAILNET=on,
# LOCAL_STACK_DESKTOP_ENV_<NAME> (the window's environment),
# LOCAL_STACK_DESKTOP_AFTER (a script once the window answers).
#
# What is Architect's stays Architect's: mock-oauth, the auth server, the
# server, its sign-in and the guard are `cargo xtask dev run` in an
# Architect checkout (the local one, or the tag Studio pins), given this
# stack's ports and state (DEV_STACK_ROOT, DEV_STACK_STATE,
# DEV_STACK_PORT_*).
#
# A local Architect or Keyflow goes under this checkout's builds as a
# [patch] in a generated cargo config (`--config`, under the state), never
# in Cargo.toml. Cargo writes the patched sources into Cargo.lock while it
# builds; the stack puts the committed Cargo.lock back after.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# This checkout's own settings (local-stack.local.env, never committed):
# `NAME=value` lines of the LOCAL_STACK_* variables below, for a checkout
# that always runs the same way (a stack of local checkouts and its
# overrides, another vault, other ports, the window's own start script).
# The environment wins over the file.
if [ -f "$root/local-stack.local.env" ]; then
    while IFS='=' read -r _k _v; do
        case "$_k" in
            LOCAL_STACK_*) [ -n "${!_k:-}" ] || export "$_k=$_v" ;;
        esac
    done <"$root/local-stack.local.env"
    unset _k _v
fi

ARCH_URL=https://github.com/FastTrackStudios/architect
KF_URL=https://github.com/FastTrackStudios/keyflow
# devenv's state for THIS checkout: a shell of another repo (Architect's
# `just dev` runs this from its own) carries that repo's DEVENV_STATE.
devenv_state="$root/.devenv/state"
if [ "${DEVENV_ROOT:-}" = "$root" ] && [ -n "${DEVENV_STATE:-}" ]; then
    devenv_state="$DEVENV_STATE"
fi
state="$devenv_state/local-stack"
mkdir -p "$state"

say() { printf 'local-stack: %s\n' "$*" >&2; }
die() { say "$*"; exit 1; }
py() { python3 "$root/scripts/local-stack.py" "$@"; }

port() {
    local name="$1" var
    var="LOCAL_STACK_PORT_$(tr '[:lower:]' '[:upper:]' <<<"$name")"
    echo "${!var:-$(py get "ports.$name")}"
}

target_dir() {
    [ -n "${_target:-}" ] || _target="$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    echo "$_target"
}

# The main checkout of this worktree (or this checkout).
main_checkout() {
    dirname "$(git rev-parse --path-format=absolute --git-common-dir)"
}

# A path from local-stack.toml or the environment: absolute, else from
# this checkout, else from its main checkout.
locate() {
    local path="$1" base
    [ -n "$path" ] || return 0
    case "$path" in /*) [ -e "$path" ] && echo "$path"; return 0 ;; esac
    for base in "$root" "$(main_checkout)"; do
        if [ -e "$base/$path" ]; then (cd "$base/$path" && pwd); return 0; fi
    done
}

# ── The choice ───────────────────────────────────────────────────────────
choice_file="$state/choice.env"

choose() {
    local architect=pinned keyflow=pinned desktop=on watch=on mode=dev arg key value
    for arg in "$@"; do
        [ -n "$arg" ] || continue
        key="${arg%%=*}"
        value="${arg#*=}"
        [ "$key" != "$arg" ] || die "\`$arg\`: give key=value (architect=local, keyflow=pinned, desktop=on, watch=off)"
        case "$key" in
            architect) architect="$value" ;;
            keyflow) keyflow="$value" ;;
            desktop) desktop="$value" ;;
            watch) watch="$value" ;;
            mode) mode="$value" ;;
            *) die "unknown choice \`$key\` (architect, keyflow, desktop, watch, mode)" ;;
        esac
    done
    for v in "$desktop" "$watch"; do
        case "$v" in on | off) ;; *) die "\`$v\`: on or off" ;; esac
    done
    case "$mode" in dev | demo) ;; *) die "mode: dev or demo" ;; esac
    {
        printf 'c_architect=%q\n' "$architect"
        printf 'c_keyflow=%q\n' "$keyflow"
        printf 'c_desktop=%q\n' "$desktop"
        printf 'c_watch=%q\n' "$watch"
        printf 'c_mode=%q\n' "$mode"
    } >"$choice_file"
}

load_choice() {
    c_architect=pinned c_keyflow=pinned c_desktop=on c_watch=on c_mode=dev
    # shellcheck source=/dev/null
    [ -f "$choice_file" ] && . "$choice_file"
    return 0
}

procs() {
    load_choice
    if [ "$c_mode" = demo ] || [ "$c_desktop" != on ]; then
        echo "mock-oauth auth architect web"
    else
        echo "mock-oauth auth architect web desktop"
    fi
}

# ── The plan: which Architect, which Keyflow, from where ────────────────
overrides="$state/overrides.toml"

# A local checkout for `local` or a path; empty for `pinned`.
local_source() {
    local name="$1" value="$2" var path
    case "$value" in
        pinned) return 0 ;;
        local)
            var="LOCAL_STACK_$(tr '[:lower:]' '[:upper:]' <<<"$name")_PATH"
            path="$(locate "${!var:-$(py get "sources.$name")}")"
            if [ -z "$path" ] || [ ! -f "$path/Cargo.toml" ]; then
                say "NOTICE: $name=local, but there is no $name checkout at ${!var:-$(py get "sources.$name")} (from $root or $(main_checkout)): $name stays at the tag Studio pins"
                return 0
            fi
            echo "$path"
            ;;
        *)
            path="$(locate "$value")"
            [ -n "$path" ] && [ -f "$path/Cargo.toml" ] || die "$name=$value: no checkout there"
            echo "$path"
            ;;
    esac
}

describe() { git -C "$1" describe --always --dirty --tags 2>/dev/null || echo "?"; }

# The Architect checkout at the pinned commit: a clone sharing the objects
# of a local Architect checkout when it has that commit (nothing fetched,
# nothing written there), else one from GitHub, once. The same folder
# scripts/studio.sh uses for the web bundle.
pinned_architect() {
    local tag rev src sibling
    read -r tag rev <<<"$(py pinned "$ARCH_URL" architect-desktop)"
    src="$(target_dir)/studio/architect-$rev"
    if [ ! -d "$src/.git" ] || [ "$(git -C "$src" rev-parse HEAD 2>/dev/null)" != "$rev" ]; then
        rm -rf "$src"
        mkdir -p "$(dirname "$src")"
        sibling="$(locate "${LOCAL_STACK_ARCHITECT_PATH:-$(py get sources.architect)}")"
        if [ -n "$sibling" ] && git -C "$sibling" cat-file -e "$rev^{commit}" 2>/dev/null; then
            say "the pinned Architect ($tag): a clone of $sibling sharing its objects"
            git clone --quiet --shared --no-checkout "$sibling" "$src" >&2
            git -C "$src" -c advice.detachedHead=false checkout --quiet "$rev" >&2
        else
            say "the pinned Architect ($tag): cloning it from GitHub, once"
            git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$tag" "$ARCH_URL" "$src" >&2
        fi
    fi
    echo "$src"
}

# A cargo config written elsewhere (LOCAL_STACK_OVERRIDES: a stack of local
# checkouts, every repo patched at once) is the overrides as they are; the
# issuer and the web app are built from LOCAL_STACK_ARCHITECT_PATH, the
# Architect checkout that goes with it. Nothing is written into either.
plan_given() {
    local given="$1" arch
    [ -f "$given" ] || die "LOCAL_STACK_OVERRIDES=$given: no such file"
    arch="$(locate "${LOCAL_STACK_ARCHITECT_PATH:-}")"
    [ -n "$arch" ] && [ -f "$arch/Cargo.toml" ] \
        || die "LOCAL_STACK_OVERRIDES needs LOCAL_STACK_ARCHITECT_PATH: the Architect checkout the issuer and the web app are built from"
    cp "$given" "$overrides"
    {
        printf 'arch_src=%q\n' "$arch"
        printf 'arch_local=%q\n' "$arch"
        printf 'kf_local=%q\n' ""
        printf 'issuer_target=%q\n' "$(target_dir)/local-stack/architect/target"
        printf 'arch_desc=%q\n' "LOCAL $arch @ $(describe "$arch") (issuer, web app); the node's crates as the overrides say"
        printf 'kf_desc=%q\n' "as the overrides say ($given)"
        printf 'studio_desc=%q\n' "THE TREE $root @ $(describe "$root") ($(git branch --show-current 2>/dev/null || echo detached))"
    } >"$state/plan.env"
    banner
}

plan() {
    load_choice
    if [ -n "${LOCAL_STACK_OVERRIDES:-}" ]; then
        plan_given "$LOCAL_STACK_OVERRIDES"
        return
    fi
    local arch_local kf_local s_tag k_tag
    arch_local="$(local_source architect "$c_architect")"
    kf_local="$(local_source keyflow "$c_keyflow")"
    s_tag="$(py dep-tag "$root/Cargo.toml" "$ARCH_URL")"
    : >"$overrides.new"
    local arch_desc kf_desc arch_src issuer_target
    if [ -n "$arch_local" ]; then
        py patch "$ARCH_URL" "$arch_local" >>"$overrides.new"
        arch_desc="LOCAL $arch_local @ $(describe "$arch_local")"
        arch_src="$arch_local"
        # Its builds for the stack (xtask, the issuer, the web app) go to a
        # target dir in this checkout's, never into that checkout.
        issuer_target="$(target_dir)/local-stack/architect/target"
    else
        local tag rev
        read -r tag rev <<<"$(py pinned "$ARCH_URL" architect-desktop)"
        arch_desc="pinned $tag (${rev:0:12}), Cargo.lock"
        arch_src="$(pinned_architect)"
        issuer_target=""
    fi
    if [ -n "$kf_local" ]; then
        py patch "$KF_URL" "$kf_local" >>"$overrides.new"
        kf_desc="LOCAL $kf_local @ $(describe "$kf_local")"
        k_tag="$(py dep-tag "$kf_local/Cargo.toml" "$ARCH_URL")"
        # One Architect: Cargo builds one copy per source, and Studio's
        # Architect and Keyflow's must be the same one.
        if [ -z "$arch_local" ] && [ "$k_tag" != "$s_tag" ]; then
            die "Keyflow at $kf_local depends on Architect $k_tag and Studio on $s_tag: they must agree on one Architect.
  - architect=local: the stack patches your Architect checkout in for both (check it out at what both need), or
  - check Keyflow out at a commit on Architect $s_tag (the tag Studio pins: $(py pinned "$KF_URL" keyflow | cut -d' ' -f1)), or
  - move Studio to Architect $k_tag first (just fleet-bump in Architect)."
        fi
    else
        local tag rev
        read -r tag rev <<<"$(py pinned "$KF_URL" keyflow)"
        kf_desc="pinned $tag (${rev:0:12}), Cargo.lock"
        k_tag="$s_tag"
    fi
    mv "$overrides.new" "$overrides"
    {
        printf 'arch_src=%q\n' "$arch_src"
        printf 'arch_local=%q\n' "$arch_local"
        printf 'kf_local=%q\n' "$kf_local"
        printf 'issuer_target=%q\n' "$issuer_target"
        printf 'arch_desc=%q\n' "$arch_desc"
        printf 'kf_desc=%q\n' "$kf_desc"
        printf 'studio_desc=%q\n' "THE TREE $root @ $(describe "$root") ($(git branch --show-current 2>/dev/null || echo detached))"
    } >"$state/plan.env"
    banner
    if [ -n "$arch_local" ]; then
        say "Architect is local: Keyflow's Architect ($k_tag) is patched to $arch_local as well, so there is one Architect. A Keyflow that does not compile against it needs keyflow=local on a matching branch."
    fi
}

# Every process plans at its start (they start together): one at a time.
plan_locked() {
    ( flock 7; plan ) 7>"$state/plan.flock"
}

load_plan() {
    # shellcheck source=/dev/null
    [ -f "$state/plan.env" ] || die "no plan yet: start the stack (just dev / just demo)"
    # shellcheck source=/dev/null
    . "$state/plan.env"
}

share_code() { py get vault.share_code; }

banner() {
    load_choice
    local rule studio_desc arch_desc kf_desc arch_src
    # shellcheck source=/dev/null
    . "$state/plan.env"
    rule="$(printf '=%.0s' {1..72})"
    {
        echo "$rule"
        echo "  STUDIO'S LOCAL STACK ($c_mode; watch $c_watch; desktop $c_desktop)"
        echo "  studio     $studio_desc"
        echo "  architect  $arch_desc"
        echo "  keyflow    $kf_desc"
        [ -s "$overrides" ] && echo "  overrides  $overrides (cargo --config; never committed)"
        echo "  issuer     $arch_src (cargo xtask dev)"
        echo "  state      $state"
        echo "$rule"
    } >&2
}

# ── Builds ───────────────────────────────────────────────────────────────

# The host's triple: one build for working is dx's desktop-dev profile
# for it (AGENTS.md, "Build performance").
host_triple() { rustc -vV | sed -n 's/^host: //p'; }

# Run a cargo (or dx) build of this checkout with the overrides, with
# Cargo.lock as committed put back after it.
with_overrides() {
    if [ ! -s "$overrides" ]; then
        "$@"
        return
    fi
    (
        flock 9
        cp "$root/Cargo.lock" "$state/Cargo.lock.held"
        # shellcheck disable=SC2329 # the EXIT trap runs it
        restore() { cmp -s "$root/Cargo.lock" "$state/Cargo.lock.held" || cp "$state/Cargo.lock.held" "$root/Cargo.lock"; }
        trap restore EXIT
        "$@"
    ) 9>"$state/cargo.flock"
}

# The cargo arguments that put the overrides under a build.
cargo_config() {
    [ -s "$overrides" ] && printf -- '--config\n%s\n' "$overrides"
    return 0
}

# ── The vault ────────────────────────────────────────────────────────────
vault_id=demo
server_vault() { echo "$state/stack/node/data/vaults/$vault_id"; }

# The demo vault (local-stack.toml [vault]: its sources merged into a
# copy, made again when they change) and a share link per set.
seed_vault() {
    mkdir -p "$(dirname "$(server_vault)")"
    chmod 700 "$state/stack" "$state/stack/node" 2>/dev/null || true
    py seed "$(server_vault)" "$vault_id" >"$state/share.new"
    mv "$state/share.new" "$state/share"
}

# Every share link, on an origin (the web front's by default).
share_urls() {
    local origin="${1:-http://127.0.0.1:$(port web)}" code page
    while read -r code page; do echo "$origin/s/$code/$page/"; done <"$state/share"
}
share_url() { share_urls "$@" | head -1; }

# ── Processes ────────────────────────────────────────────────────────────

# `cargo xtask dev run <process>` in the Architect checkout, on this
# stack's ports and state.
xdev() {
    load_plan
    cd "$arch_src"
    local vars=(
        DEV_STACK_ROOT="$arch_src"
        DEV_STACK_STATE="$state/stack"
        DEVENV_STATE="$state"
        DEV_STACK_PORT_MOCK_OAUTH="$(port mock_oauth)"
        DEV_STACK_PORT_AUTH="$(port auth)"
        # The server is Architect's app with no window, on its port.
        DEV_STACK_PORT_NODE="$(port server)"
        DEV_STACK_PORT_APP="$(port server)"
        # Where a browser opens the web client (the front): the issuer's
        # redirects and the org lane's allowed origin.
        DEV_STACK_PORT_WEB="$(port web)"
    )
    [ -n "$issuer_target" ] && vars+=(CARGO_TARGET_DIR="$issuer_target")
    # The fixed dev cast only: never a mirror of production. No binary
    # of Studio's: the server is Architect's own.
    exec env -u AUTH_ADMIN_TOKEN -u AUTH_ADMIN_TOKEN_FILE -u DEV_STACK_BIN_APP -u DEV_STACK_BIN_NODE \
        "${vars[@]}" cargo run -q -p xtask -- dev "$@"
}

run_auth() {
    plan_locked
    mkdir -p "$state/stack"
    chmod 700 "$state/stack"
    # The demo seed, always: the stack never mirrors production.
    : >"$state/stack/use-demo-seed"
    chmod 600 "$state/stack/use-demo-seed"
    xdev run auth
}

# THE SERVER: plain Architect (`architect-desktop serve`, the app with no
# window), built from the Architect checkout the plan names. Nothing of
# Studio is compiled into it or installed on it: it stores the vault and
# its structure, signs people in through the issuer, enforces who may
# read what and serves share links' data. Studio is its clients.
run_architect() {
    plan_locked
    load_plan
    seed_vault
    ( build_architect ) || die "server: the build failed (above)"
    say "server: plain Architect from $arch_src (architect-desktop serve, no Studio code), on :$(port server)"
    (
        for _ in $(seq 1 1800); do
            if curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(port server)/health"; then
                say "server: up on http://127.0.0.1:$(port server). Share links (open them on the web client's origin):"
                share_urls >&2
                exit 0
            fi
            sleep 1
        done
    ) &
    xdev run app --headless
}

# The folder of a crate the build resolves (with the overrides: the
# sibling checkout's).
crate_dir() {
    local cfg=()
    mapfile -t cfg < <(cargo_config)
    with_overrides cargo metadata --format-version 1 "${cfg[@]}" 2>/dev/null \
        | python3 -c 'import json, os, sys
m = json.load(sys.stdin)
print(next(os.path.dirname(p["manifest_path"]) for p in m["packages"] if p["name"] == sys.argv[1]))' "$1"
}

# studio-web's dev page and public folder for `dx serve`: Studio's own
# page (apps/web/page.html), Architect's web app's sheet.
prepare_web() {
    local crate="$1" archweb="$2"
    python3 - "$crate" "$archweb" "$(port auth)" "$arch_src" <<'PY'
import json, os, re, shutil, sys
crate, archweb, auth, arch = sys.argv[1:5]
# Studio's own page (apps/web/page.html: its name, black, its splash),
# with Studio's icon in it; Architect's web app's sheet beside it.
icon = open(os.path.join(crate, "../../features/distribution/icon.svg"), encoding="utf-8").read()
page = open(os.path.join(crate, "page.html"), encoding="utf-8").read().replace("{{ICON}}", icon.strip())
page = page.replace('<html lang="en">', '<html lang="en" data-assets="/assets/app-dev/">')
open(os.path.join(crate, "index.html"), "w", encoding="utf-8").write(page)
pub = os.path.join(crate, "dev/public")
shutil.rmtree(pub, ignore_errors=True)
def put(rel, src=None, text=None):
    out = os.path.join(pub, rel)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    if text is not None:
        open(out, "w", encoding="utf-8").write(text)
    else:
        shutil.copyfile(src, out)
put("app/style.css", os.path.join(archweb, "app/style.css"))
put("app/icon.svg", text=icon)
# Signed in already as the stack's demo account (architect#991): the
# issuer approves it without its form, the app starts the sign-in itself.
put("app/config.json", text=json.dumps({"issuer": f"http://127.0.0.1:{auth}", "client_id": "architect", "auto_sign_in": True}))
# A share link's page (the server's, /s/<code>/) starts the app from
# /app/start.js, as the bundle does: here, dx's build of studio-web.
put("app/start.js", text="// Written by scripts/local-stack.sh: studio-web's dev start, for a share link's page.\n"
    "document.documentElement.dataset.assets = '/assets/app-dev/';\nimport('/wasm/studio-web.js');\n")
fav = os.path.join(arch, "apps/site/public/favicon.svg")
if os.path.isfile(fav):
    put("favicon.svg", fav)
for face in ["Inter-Variable.woff2", "Inter-OFL.txt", "luau-web/index.js", "luau-web/Luau.Web.Asyncify.js", "luau-web/LICENSE"]:
    src = os.path.join(archweb, "assets", face)
    if os.path.isfile(src):
        put("assets/app-dev/" + face, src)
PY
}

# STUDIO-WEB: Studio's web client (apps/web: Architect's web app as
# a library with Studio's views linked in, one wasm) under `dx serve`,
# rebuilt on a save (rsx hot-reloaded; no Rust hot-patching: its builds
# bypass kache, so every workspace crate would compile uncached), behind the front that
# puts it and the server's paths on one origin. LOCAL_STACK_LAN=on opens
# the front to the LAN; LOCAL_STACK_TAILNET=on serves it on this
# machine's tailnet name over HTTPS (`tailscale serve`, never funnel).
run_web() {
    plan_locked
    load_plan
    local crate="$root/apps/web" web dx server archweb bind=127.0.0.1 cfg=() pids=()
    web="$(port web)" dx="$(port web_dx)" server="$(port server)"
    archweb="$(crate_dir architect-web)"
    [ -n "$archweb" ] && [ -d "$archweb" ] || die "web: no architect-web crate resolved (cargo metadata)"
    prepare_web "$crate" "$archweb"
    [ "${LOCAL_STACK_LAN:-off}" = on ] && bind=0.0.0.0
    mapfile -t cfg < <(cargo_config)
    cleanup() {
        trap - TERM INT EXIT
        for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done
        wait 2>/dev/null || true
        [ -f "$state/Cargo.lock.web" ] && ! cmp -s Cargo.lock "$state/Cargo.lock.web" && cp "$state/Cargo.lock.web" Cargo.lock
        exit 0
    }
    trap cleanup TERM INT EXIT
    # The issuer through the front too, for a browser on another origin
    # (the tailnet's: `just phone`).
    # and, once `just phone` built it, the phone's build of the app for
    # that origin (iOS Safari cannot compile the dev build's module).
    LOCAL_STACK_FRONT_AUTH="$(port auth)" python3 "$root/scripts/local-stack-front.py" "http://127.0.0.1:$dx" "$web" "$server" "$bind" &
    pids+=($!)
    say "web: studio-web at http://127.0.0.1:$web/app/ (sign in as ada@local.test, password development-password)"
    if [ "$bind" = 0.0.0.0 ]; then
        say "web: on the LAN too: http://$(hostname -I 2>/dev/null | cut -d' ' -f1):$web/app/"
    fi
    if [ "${LOCAL_STACK_TAILNET:-off}" = on ]; then
        tailnet_on || say "web: not on the tailnet (above)"
    fi
    if [ "${LOCAL_STACK_OPEN:-off}" = on ] && command -v xdg-open >/dev/null; then
        (
            for _ in $(seq 1 1800); do
                curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$web/app/" && up "$server" && break
                sleep 1
            done
            xdg-open "http://127.0.0.1:$web/app/" >/dev/null 2>&1 || true
        ) &
    fi
    # Its build first, in this process's log (compiling, then served:
    # `ready web` answers only once dx serves the built module).
    ( build_web ) || die "web: the build failed (above)"
    cp Cargo.lock "$state/Cargo.lock.web"
    (
        cd "$crate"
        if [ "${#cfg[@]}" -gt 0 ]; then
            exec dx serve --platform web --addr 127.0.0.1 --port "$dx" --open false --hot-patch false \
                --cargo-args="${cfg[*]}"
        fi
        exec dx serve --platform web --addr 127.0.0.1 --port "$dx" --open false --hot-patch false
    ) &
    pids+=($!)
    wait -n "${pids[@]}"
    say "web: one of studio-web's processes ended"
}

# STUDIO DESKTOP: Studio's distribution of Architect (apps/desktop,
# studio) under `dx serve`, rebuilt on a save through kache (an rsx edit
# hot-reloads; no Rust hot-patching, whose builds bypass kache), an
# isolated dev instance signed in to the stack's
# issuer and syncing with the server like any other client. Its engine is
# the `daw` extension, a development copy (extensions/daw, rebuilt alone):
# the app starts the engine when none runs, and the engine outlives the
# window (extensions/daw/src/owner.rs), so a rebuild or a hot-patch
# reattaches. `just engine-stop` stops it. LOCAL_STACK_DESKTOP_ENV_<NAME>
# (local-stack.local.env) gives the window <NAME>: what the engine opens
# (FTS_DAW_OPEN), Ignition's renderer (IGNITION_BIN, IGNITION_ROOT, ...).
# LOCAL_STACK_DESKTOP_AFTER names a script run once the window answers,
# given its binary and control socket (a machine's own layout).
run_desktop() {
    plan_locked
    load_plan
    load_choice
    if [ "$c_desktop" != on ]; then
        say "desktop: off (just dev desktop=on)"
        return 0
    fi
    local triple out cfg=() vars=() line name k after
    triple="$(host_triple)"
    out="$(target_dir)/$triple/desktop-dev"
    mapfile -t cfg < <(cargo_config)
    # Its build first (the engine, then the window), in this process's log;
    # `ready desktop` answers only once the window's control socket does.
    ( build_desktop ) || die "desktop: the build failed (above)"
    for _ in $(seq 1 7200); do
        curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(port server)/health" && break
        sleep 1
    done
    # The window's issuer and the org's host are the stack's (Architect's
    # `dev env app`); its own node is not the server: a port, a folder and
    # an account of its own, hosting no org.
    while IFS= read -r line; do
        k="${line%%=*}"
        case "$k" in
            ARCHITECT_BIND | ARCHITECT_DATA_DIR | ARCHITECT_VAULTS | ARCHITECT_SESSION_FILE | ARCHITECT_HOST_ORG | \
                ARCHITECT_WEB_APP | ARCHITECT_LANE_ORIGINS | ARCHITECT_NAME | ARCHITECT_FILES_DIR | ARCHITECT_CONFIG_DIR | \
                ARCHITECT_SECRET_STORE | ARCHITECT_VAULT | ARCHITECT_VAULT_SEED) ;;
            *) vars+=("$line") ;;
        esac
    done < <(
        cd "$arch_src" && env DEV_STACK_ROOT="$arch_src" DEV_STACK_STATE="$state/stack" DEVENV_STATE="$state" \
            DEV_STACK_PORT_AUTH="$(port auth)" DEV_STACK_PORT_NODE="$(port server)" DEV_STACK_PORT_APP="$(port server)" \
            ${issuer_target:+CARGO_TARGET_DIR="$issuer_target"} cargo run -q -p xtask -- dev env app
    )
    mkdir -p "$state/desktop/vault" "$state/desktop/files"
    vars+=(
        ARCHITECT_NAME=studio-desktop
        ARCHITECT_BIND="127.0.0.1:$(port desktop)"
        ARCHITECT_DATA_DIR="$state/desktop/node"
        ARCHITECT_FILES_DIR="$state/desktop/files"
        ARCHITECT_DEV=studio-stack
        ARCHITECT_DEV_ROOT="$state/desktop"
        ARCHITECT_DEV_VAULT="$state/desktop/vault"
        ARCHITECT_DEV_EXTENSIONS="$root/extensions/daw"
    )
    while IFS='=' read -r name _; do
        case "$name" in
            LOCAL_STACK_DESKTOP_ENV_*) vars+=("${name#LOCAL_STACK_DESKTOP_ENV_}=${!name}") ;;
        esac
    done < <(env)
    after="${LOCAL_STACK_DESKTOP_AFTER:-}"
    if [ -n "$after" ]; then
        case "$after" in /*) ;; *) after="$(cd "$root/$(dirname "$after")" && pwd)/$(basename "$after")" ;; esac
        (
            bin="" sock=""
            for _ in $(seq 1 3600); do
                desktop_up && break
                sleep 2
            done
            bin="$(desktop_bin)"
            sock="$(desktop_socket)"
            say "desktop: up; running $after"
            ARCHITECT_DEV_ROOT="$state/desktop" LOCAL_STACK_DESKTOP_INSTANCE="$(desktop_instance)" \
                "$after" "$bin" "$sock" >&2 || say "desktop: $after failed"
        ) &
    fi
    say "desktop: Studio (studio) under dx serve, signed in to the stack's issuer, its server :$(port server)"
    cp Cargo.lock "$state/Cargo.lock.desktop"
    trap '[ -f "$state/Cargo.lock.desktop" ] && ! cmp -s Cargo.lock "$state/Cargo.lock.desktop" && cp "$state/Cargo.lock.desktop" Cargo.lock' EXIT
    cd "$root/apps/desktop"
    if [ "${#cfg[@]}" -gt 0 ]; then
        env "${vars[@]}" dx serve --platform desktop --renderer native --hot-patch false --profile desktop-dev \
            --target "$triple" --open false --cargo-args="${cfg[*]}"
    else
        env "${vars[@]}" dx serve --platform desktop --renderer native --hot-patch false --profile desktop-dev \
            --target "$triple" --open false
    fi
}

# ── Builds ───────────────────────────────────────────────────────────────
# Each process builds first, in its own log, then serves: `devenv up` (or
# `devenv processes attach`) shows it running and not ready, compiling,
# until it really answers (`ready <process>`). Not devenv tasks: a task
# runs inside `devenv up -d`'s two-minute start and a desktop bundle takes
# longer. The same flags as the serve after it, so that serve is an
# up-to-date check. Everything through kache (no hot-patching).
# `local-stack.sh build <process>` runs one alone.

# The Architect server: plain `architect-desktop`, no window.
build_architect() {
    plan_locked
    load_plan
    say "build: the Architect server (architect-desktop, no window) from $arch_src"
    (
        cd "$arch_src"
        [ -n "$issuer_target" ] && export CARGO_TARGET_DIR="$issuer_target"
        cargo build -p architect-desktop --no-default-features --features vox --bin architect-desktop
    )
}

# studio-web, as `dx serve` builds it.
build_web() {
    plan_locked
    load_plan
    local crate="$root/apps/web" archweb cfg=()
    archweb="$(crate_dir architect-web)"
    [ -n "$archweb" ] && [ -d "$archweb" ] || die "build: no architect-web crate resolved (cargo metadata)"
    prepare_web "$crate" "$archweb"
    mapfile -t cfg < <(cargo_config)
    say "build: studio-web (apps/web) for the browser"
    cd "$crate"
    if [ "${#cfg[@]}" -gt 0 ]; then
        with_overrides dx build --platform web --cargo-args="${cfg[*]}"
    else
        dx build --platform web
    fi
}

# Studio desktop and its engine (the `daw` extension's program, copied
# into its package), as `dx serve` builds them.
build_desktop() {
    plan_locked
    load_plan
    local triple out cfg=()
    triple="$(host_triple)"
    out="$(target_dir)/$triple/desktop-dev"
    mapfile -t cfg < <(cargo_config)
    say "build: the engine (daw-extension) into its package, extensions/daw"
    with_overrides cargo build --profile desktop-dev --target "$triple" --bin daw-extension "${cfg[@]}"
    cp --reflink=auto "$out/daw-extension" "$root/extensions/daw/daw-extension.new"
    mv "$root/extensions/daw/daw-extension.new" "$root/extensions/daw/daw-extension"
    say "build: Studio desktop (apps/desktop)"
    cd "$root/apps/desktop"
    if [ "${#cfg[@]}" -gt 0 ]; then
        with_overrides dx build --platform desktop --renderer native --profile desktop-dev --target "$triple" \
            --cargo-args="${cfg[*]}"
    else
        dx build --platform desktop --renderer native --profile desktop-dev --target "$triple"
    fi
}

# The window as it runs, from its dev instance's own record
# (`<ARCHITECT_DEV_ROOT>/studio/studio-stack/instance.json`, written by the
# app: its pid, its control socket, whether it runs). Not `studio dev path`,
# which names another instance's socket folder, nor dx's binary, which dx
# names with a hash (`studio-1f19ef17`).
desktop_instance() { echo "$state/desktop/studio/studio-stack/instance.json"; }
desktop_field() {
    python3 -c 'import json, sys
try:
    print(json.load(open(sys.argv[1])).get(sys.argv[2], ""))
except (OSError, ValueError):
    pass' "$(desktop_instance)" "$1" 2>/dev/null
}
desktop_bin() {
    local pid
    pid="$(desktop_field pid)"
    [ -n "$pid" ] && readlink "/proc/$pid/exe" 2>/dev/null
}
desktop_socket() { desktop_field socket; }
desktop_up() {
    local pid bin sock
    pid="$(desktop_field pid)"
    [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null || return 1
    bin="$(desktop_bin)" sock="$(desktop_socket)"
    [ -n "$bin" ] && [ -S "$sock" ] && "$bin" actions --app "$sock" run APP_STATUS >/dev/null 2>&1
}

# ── Status, tailnet, reset ───────────────────────────────────────────────
up() { curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$1/" 2>/dev/null; }

status() {
    [ -f "$state/plan.env" ] && banner
    local name p
    for name in mock_oauth auth server web web_dx desktop; do
        p="$(port "$name")"
        printf '  %-11s http://127.0.0.1:%-6s %s\n' "$name" "$p" "$(up "$p" && echo running || echo 'not running')"
    done
    echo "  studio-web  http://127.0.0.1:$(port web)/app/  (ada@local.test / development-password)"
    [ -f "$state/share" ] && share_urls | sed 's/^/  share link  /'
    [ -f "$state/tailnet" ] && echo "  tailnet     $(cat "$state/tailnet")/app/"
    return 0
}

tailnet_name() {
    tailscale status --self --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["Self"]["DNSName"].rstrip("."))'
}

# studio-web (the front: the app and the server's paths) on this
# machine's tailnet name over HTTPS: tailnet only, `tailscale serve`,
# never funnel. [tailnet] https_port in local-stack.toml.
tailnet_on() {
    command -v tailscale >/dev/null || { say "tailnet: no tailscale on this machine"; return 1; }
    local https name web out
    https="$(py get tailnet.https_port)"
    web="$(port web)"
    name="$(tailnet_name)" || { say "tailnet: tailscale is not up"; return 1; }
    # Never over something else already served on that port.
    if tailscale serve status 2>/dev/null | grep -A1 "^https://$name:$https" | grep -q proxy \
        && ! tailscale serve status | grep -A1 "^https://$name:$https" | grep -q "127.0.0.1:$web"; then
        say "tailnet: https://$name:$https already serves something else: set [tailnet] https_port (Tailscale serves 443, 8443, 10000)"
        return 1
    fi
    if ! out="$(tailscale serve --bg --https="$https" "http://127.0.0.1:$web" 2>&1)"; then
        if grep -q 'Access denied' <<<"$out"; then
            say "tailnet: tailscale refused: this user may not change what it serves. Once, as root: sudo tailscale set --operator=$USER"
        else
            say "tailnet: tailscale serve failed: $out"
        fi
        return 1
    fi
    echo "https://$name:$https" >"$state/tailnet"
    say "tailnet: studio-web on this tailnet: https://$name:$https/app/ (share links: $(share_url "https://$name:$https"))"
}

tailnet_off() {
    command -v tailscale >/dev/null || return 0
    tailscale serve --https="$(py get tailnet.https_port)" off
    rm -f "$state/tailnet"
    say "tailnet: off"
}

reset() {
    if up "$(port server)" || up "$(port auth)"; then die "the stack is running: stop devenv up first"; fi
    rm -rf "$state"
    say "removed $state"
}

cmd="${1:-}"
shift || true
case "$cmd" in
    choose) choose "$@" ;;
    procs) procs ;;
    plan) plan ;;
    run)
        case "${1:-}" in
            mock-oauth) plan_locked && xdev run mock-oauth ;;
            auth) run_auth ;;
            architect) run_architect ;;
            web) run_web ;;
            desktop) run_desktop ;;
            *) die "run: mock-oauth, auth, architect, web or desktop" ;;
        esac
        ;;
    build)
        case "${1:-}" in
            architect) build_architect ;;
            web) build_web ;;
            desktop) build_desktop ;;
            *) die "build: architect, web or desktop" ;;
        esac
        ;;
    status) status ;;
    ready)
        # devenv's readiness probe: the process answers on its port.
        case "${1:-}" in
            auth) curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(port auth)/health" ;;
            architect) curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(port server)/health" ;;
            mock-oauth) curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$(port mock_oauth)/" ;;
            # Built and served: dx serves the app's own module only once its
            # build is done (before, its "building" page).
            web) curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(port web)/wasm/studio-web.js" ;;
            # The window answers on its control socket.
            desktop) desktop_up ;;
            *) curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$(port "$1")/" ;;
        esac
        ;;
    tailnet)
        if [ "${1:-}" = off ]; then tailnet_off; else tailnet_on; fi
        ;;
    smoke) exec bash "$root/scripts/local-stack-smoke.sh" "$@" ;;
    reset) reset ;;
    url) share_urls "$@" ;;
    _xdev) xdev "$@" ;;
    *)
        sed -n '2,24p' "$0" >&2
        exit 64
        ;;
esac
