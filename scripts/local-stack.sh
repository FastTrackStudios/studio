#!/usr/bin/env bash
# The local stack (docs/local-stack.md): Studio's node, its issuer,
# the web app and the site on this machine, nothing from the network at
# run time. `devenv up` runs each process through this script; the just
# recipes (`just dev`, `just demo`, `just website`) choose what is local
# and start it.
#
#   local-stack.sh choose [architect=pinned|local|PATH] [keyflow=…]
#                         [desktop=on|off] [watch=on|off] [mode=dev|demo]
#                                         what the next `devenv up` runs
#   local-stack.sh plan                   resolve it, write the overrides, say it
#   local-stack.sh procs                  the devenv processes the choice starts
#   local-stack.sh run mock-oauth|auth|node|web|site|desktop
#                                         one process, as devenv.nix starts it
#   local-stack.sh status                 what runs, from where, the URLs
#   local-stack.sh phone [off]            the node on the tailnet over HTTPS
#   local-stack.sh smoke                  the stack, checked end to end
#   local-stack.sh reset                  remove the stack's state
#
# local-stack.local.env (untracked) holds a checkout's own LOCAL_STACK_*
# settings: LOCAL_STACK_OVERRIDES + LOCAL_STACK_ARCHITECT_PATH (a stack of
# local checkouts), LOCAL_STACK_VAULT, LOCAL_STACK_SETLIST,
# LOCAL_STACK_PORT_<NAME>, LOCAL_STACK_DESKTOP_RUN (the window's own start
# script), LOCAL_STACK_OPEN=on (the web app and the link in the browser).
#
# What is Architect's stays Architect's: mock-oauth, the auth server, the
# node's environment, its sign-in and the guard are `cargo xtask dev run`
# in an Architect checkout (the local one, or the tag Studio pins), given
# this stack's ports and state (DEV_STACK_ROOT, DEV_STACK_STATE,
# DEV_STACK_PORT_*) and this node's binary (DEV_STACK_BIN_NODE).
#
# A local Architect or Keyflow goes under this checkout's build as a
# [patch] in a generated cargo config (`--config`, under the state), never
# in Cargo.toml. Cargo writes the patched sources into Cargo.lock while it
# builds; the stack puts the committed Cargo.lock back after every build.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
self="$root/scripts/local-stack.sh"

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
    local architect=pinned keyflow=pinned desktop=off watch=on mode=dev arg key value
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
    c_architect=pinned c_keyflow=pinned c_desktop=off c_watch=on c_mode=dev
    # shellcheck source=/dev/null
    [ -f "$choice_file" ] && . "$choice_file"
    return 0
}

procs() {
    load_choice
    if [ "$c_mode" = demo ]; then
        echo "mock-oauth auth node"
    elif [ "$c_desktop" = on ]; then
        echo "mock-oauth auth node web site desktop"
    else
        echo "mock-oauth auth node web site"
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
# scripts/studio-architect.sh uses for the web bundle.
pinned_architect() {
    local tag rev src sibling
    read -r tag rev <<<"$(py pinned "$ARCH_URL" architect-desktop)"
    src="$(target_dir)/studio-architect/architect-$rev"
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
profile="${LOCAL_STACK_PROFILE:-dev}"

# What a staged node was built with: the overrides (empty: what Cargo.lock
# pins). `demo` reuses a staged node only when it matches.
plan_id() {
    if [ -s "$overrides" ]; then sha256sum "$overrides" | cut -c1-16; else echo pinned; fi
}

# Run a cargo build of this checkout with the overrides, serialised, and
# Cargo.lock as committed before and after.
with_overrides() {
    if [ ! -s "$overrides" ]; then
        "$@"
        return
    fi
    (
        flock 9
        cp Cargo.lock "$state/Cargo.lock.held"
        # shellcheck disable=SC2329 # the EXIT trap runs it
        restore() { cmp -s Cargo.lock "$state/Cargo.lock.held" || cp "$state/Cargo.lock.held" Cargo.lock; }
        trap restore EXIT
        STUDIO_ARCHITECT_CARGO_CONFIG="$overrides" "$@"
    ) 9>"$state/cargo.flock"
}

node_build() {
    local out
    out="$(with_overrides env PROFILE="$profile" bash scripts/studio-architect.sh node)" || return 1
    sed -n 's/^node=//p' <<<"$out" >"$state/built-node"
}

island_build() {
    with_overrides just island-build >&2
}

# The node and its island, copied (never linked: the node refuses a linked
# island) into the folder the node runs from; a new stamp when either
# changed, which restarts the node.
stage() {
    (
        flock 8
        local bin island new="$state/bin.new"
        bin="$(cat "$state/built-node" 2>/dev/null)" || die "no node built yet"
        island="$(target_dir)/studio-island"
        rm -rf "$new"
        mkdir -p "$new"
        cp --reflink=auto "$bin" "$new/studio-architect"
        if [ -f "$island/architect.lock.json" ]; then
            mkdir -p "$new/studio-island"
            find "$island" -maxdepth 1 -type f -exec cp --reflink=auto {} "$new/studio-island/" \;
        fi
        if [ -d "$state/bin" ] && cmp -s "$new/studio-architect" "$state/bin/studio-architect" \
            && diff -rq "$new/studio-island" "$state/bin/studio-island" >/dev/null 2>&1; then
            rm -rf "$new"
            say "staged: unchanged, the node keeps running"
            return 0
        fi
        date +%s.%N >"$new/STAMP"
        plan_id >"$new/PLAN"
        rm -rf "$state/bin.old"
        [ -d "$state/bin" ] && mv "$state/bin" "$state/bin.old"
        mv "$new" "$state/bin"
        say "staged: $(du -sh "$state/bin/studio-architect" | cut -f1) node$( [ -d "$state/bin/studio-island" ] && echo ", its island")"
    ) 8>"$state/stage.flock"
}

rebuild() {
    local what="$1" t0
    t0=$(date +%s%N)
    case "$what" in
        node) node_build || { say "the node's build failed: the last good node keeps running"; return 0; } ;;
        island) island_build || { say "the island's build failed: the last good island stays"; return 0; } ;;
    esac
    stage
    say "$what rebuilt and staged in $((($(date +%s%N) - t0) / 1000000)) ms"
}

# ── The vault ────────────────────────────────────────────────────────────
vault_id=demo
node_vault() { echo "$state/stack/node/data/vaults/$vault_id"; }

# A copy of the test vault with its share link, made once. Without one,
# xtask seeds Architect's demo vault.
seed_vault() {
    local src dst setlist code out
    src="$(locate "${LOCAL_STACK_VAULT:-$(py get vault.source)}")"
    dst="$(node_vault)"
    # `LOCAL_STACK_SETLIST` names another vault's setlist (with
    # `LOCAL_STACK_VAULT`), `LOCAL_STACK_LISTEN` how far its link plays
    # (Reference, the default, or Full: the multitracks too).
    setlist="${LOCAL_STACK_SETLIST:-$(py get vault.setlist)}"
    code="$(share_code)"
    if [ -z "$src" ]; then
        say "NOTICE: no test vault at ${LOCAL_STACK_VAULT:-$(py get vault.source)}: the node holds Architect's demo seed (setlist Fall Show, /s/ksqx75iaaaaaaaae/)"
        echo "ksqx75iaaaaaaaae setlists/fall-show" >"$state/share"
        return 0
    fi
    if [ ! -d "$dst" ]; then
        mkdir -p "$(dirname "$dst")"
        chmod 700 "$state/stack" "$state/stack/node" "$(dirname "$(dirname "$dst")")" 2>/dev/null || true
        say "copying the test vault $src into $dst (once; just dev-reset drops it)"
        cp -a --reflink=auto "$src/." "$dst.tmp/" 2>/dev/null || { mkdir -p "$dst.tmp" && cp -a --reflink=auto "$src/." "$dst.tmp/"; }
        mv "$dst.tmp" "$dst"
    fi
    out="$(py seed-share "$dst" "$setlist" "$code" "$vault_id" "${LOCAL_STACK_LISTEN:-Reference}")"
    echo "$code ${out#* }" >"$state/share"
}

share_url() {
    local origin="${1:-http://127.0.0.1:$(port node)}" code page
    read -r code page <"$state/share"
    echo "$origin/s/$code/$page/"
}

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
        DEV_STACK_PORT_NODE="$(port node)"
        DEV_STACK_PORT_WEB="$(port web)"
        DEV_STACK_PORT_SITE="$(port site)"
        DEV_STACK_BIN_NODE="$state/node-wrapper"
        # An Architect whose app is its node (architect#825): the same, by its names.
        DEV_STACK_PORT_APP="$(port node)"
        DEV_STACK_BIN_APP="$state/node-wrapper"
    )
    [ -n "$issuer_target" ] && vars+=(CARGO_TARGET_DIR="$issuer_target")
    # The fixed dev cast only: never a mirror of production.
    exec env -u AUTH_ADMIN_TOKEN -u AUTH_ADMIN_TOKEN_FILE "${vars[@]}" cargo run -q -p xtask -- dev "$@"
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

write_wrapper() {
    load_plan
    {
        echo '#!/usr/bin/env bash'
        echo "# Written by scripts/local-stack.sh: the staged node, with what xtask"
        echo "# would strip (it removes inherited ARCHITECT_*)."
        if [ -n "$arch_local" ] && [ -d "$arch_local/features/sites/assets" ]; then
            # A debug node reads the share pages' player and styles from
            # here, so an edit shows on the next build of the pages
            # (architect-sites `assets::DEV_ASSETS_VAR`); a node older
            # than that, or a release build, ignores it.
            printf 'export ARCHITECT_SITES_ASSETS_DIR=%q\n' "$arch_local/features/sites/assets"
        fi
        printf 'exec %q "$@"\n' "$state/bin/studio-architect"
    } >"$state/node-wrapper"
    chmod +x "$state/node-wrapper"
}

# Touch the share notes, so the node builds their pages again (an asset
# edit shows on reload).
touch_shares() {
    touch "$(node_vault)/_architect/shares/"*.md 2>/dev/null || true
    say "share pages: rebuilding (reload the page)"
}

watch_dirs() {
    # What the node is built from: this tree, and what is local.
    local d
    for d in apps crates features vendor Cargo.toml; do echo "$root/$d"; done
    [ -n "$arch_local" ] && echo "$arch_local"
    [ -n "$kf_local" ] && echo "$kf_local"
    return 0
}

run_node() {
    plan_locked
    seed_vault
    write_wrapper
    local pids=()
    cleanup() {
        trap - TERM INT EXIT
        for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done
        wait 2>/dev/null || true
        [ -f "$state/Cargo.lock.held" ] && ! cmp -s Cargo.lock "$state/Cargo.lock.held" && cp "$state/Cargo.lock.held" Cargo.lock
        exit 0
    }
    trap cleanup TERM INT EXIT
    load_choice
    load_plan

    if [ "$c_mode" = demo ] && [ -x "$state/bin/studio-architect" ] \
        && [ "$(cat "$state/bin/PLAN" 2>/dev/null)" = "$(plan_id)" ]; then
        say "demo: the last staged node ($(date -d @"$(cut -d. -f1 "$state/bin/STAMP")" '+%F %T')), no build; just dev rebuilds it"
    else
        local t0
        t0=$(date +%s)
        say "building the node (profile $profile) and its island"
        node_build || die "the node's build failed"
        if [ -f "$root/apps/architect/island/extension.toml" ]; then
            island_build || say "the island's build failed: the node runs without it (the share page keeps Architect's own bar)"
        fi
        stage
        say "built and staged in $(($(date +%s) - t0)) s"
    fi

    # The node, restarted whenever a new build is staged.
    (
        trap 'kill -TERM "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; exit 0' TERM INT
        stamp="" pid=""
        while :; do
            now="$(cat "$state/bin/STAMP" 2>/dev/null || true)"
            if [ -n "$now" ] && [ "$now" != "$stamp" ]; then
                if [ -n "$pid" ]; then
                    kill -TERM "$pid" 2>/dev/null || true
                    wait "$pid" 2>/dev/null || true
                    say "node: restarting on the new build"
                fi
                stamp="$now"
                # The stack's node is `node`, or (architect#825) the app with no window.
                if grep -q '=> "app"' "$arch_src/xtask/src/dev/mod.rs" 2>/dev/null; then
                    "$self" _xdev run app --headless &
                else
                    "$self" _xdev run node &
                fi
                pid=$!
                (
                    for _ in $(seq 1 240); do
                        if curl -sf -o /dev/null "http://127.0.0.1:$(port node)/health"; then
                            say "node: up. The share link: $(share_url)"
                            exit 0
                        fi
                        sleep 0.25
                    done
                ) &
            elif [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
                say "node: exited; waiting for a new build"
                pid=""
            fi
            sleep 0.5
        done
    ) &
    pids+=($!)

    if [ "$c_watch" != on ]; then
        say "watch off: edits do not rebuild (just dev watches)"
        wait
        return
    fi
    local dirs=() d
    while IFS= read -r d; do dirs+=(-w "$d"); done < <(watch_dirs)
    # Rust: a build (cargo decides what changed), staged only when the
    # binary did. The island's sources and the share pages' assets have
    # loops of their own.
    cargo watch -q --postpone --delay 1 "${dirs[@]}" \
        -i '**/apps/architect/island/**' -i '**/features/sites/assets/**' -i '*.md' -i '**/Cargo.lock' \
        -s "'$self' rebuild node" &
    pids+=($!)
    cargo watch -q --postpone --delay 1 -w "$root/apps/architect/island" -w "$root/features/ui/transport-ui" \
        -s "'$self' rebuild island" &
    pids+=($!)
    if [ -n "$arch_local" ] && [ -d "$arch_local/features/sites/assets" ]; then
        cargo watch -q --postpone --delay 0.3 -w "$arch_local/features/sites/assets" -s "'$self' _touch-shares" &
        pids+=($!)
    fi
    say "watching: $(watch_dirs | tr '\n' ' ')(+ the island$( [ -n "$arch_local" ] && echo ", Architect's share-page assets"))"
    wait
}

# The web app: Architect's web bundle with this distribution's
# /app/distribution.json, its issuer the local one.
run_web() {
    plan_locked
    load_plan
    local web auth bindgen want out="$state/web" json="$state/distribution.json"
    web="$(port web)" auth="$(port auth)"
    for _ in $(seq 1 3600); do [ -x "$state/bin/studio-architect" ] && break; sleep 1; done
    "$state/bin/studio-architect" distribution >"$json"
    want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/[^0-9.]/,""); print; exit}' "$arch_src/Cargo.lock")
    bindgen="$(command -v wasm-bindgen || true)"
    if [ -z "$bindgen" ] || ! "$bindgen" --version | grep -q "$want"; then
        bindgen="$state/tools/bin/wasm-bindgen"
        "$bindgen" --version 2>/dev/null | grep -q "$want" \
            || cargo install -q wasm-bindgen-cli --version "$want" --locked --root "$state/tools"
    fi
    # Studio's own build of the web app (apps/architect/web: Architect's
    # app with Studio's engine in the page), when this tree has it and the
    # Architect it is built against has what it needs (`provide_engine`);
    # else Architect's own wasm, as before.
    local studio_wasm="" cfg=()
    [ -s "$overrides" ] && cfg=(--config "$overrides")
    if [ -f "$root/apps/architect/web/Cargo.toml" ] && grep -q 'pub fn provide_engine' "$arch_src/apps/web/src/provided.rs" 2>/dev/null; then
        say "web: building Studio's web app (apps/architect/web, the engine in the page)"
        # SIMD, as `just web-daw` builds the engine's page: its painting
        # and its mix pick their level at compile time and run their scalar
        # fallback without it.
        if RUSTFLAGS="-C target-feature=+simd128" with_overrides cargo build --profile wasm-release -p studio-architect-web --lib --target wasm32-unknown-unknown "${cfg[@]}" >&2; then
            mkdir -p "$state/web-wasm"
            cp --reflink=auto "$(target_dir)/wasm32-unknown-unknown/wasm-release/studio_architect_web.wasm" "$state/web-wasm/architect_web.wasm"
            studio_wasm="$state/web-wasm/architect_web.wasm"
        else
            say "web: Studio's web app did not build: Architect's own wasm instead (no engine in the page)"
        fi
    fi
    if [ "${LOCAL_STACK_WEB:-bundle}" = app ]; then
        # The app alone, no site and no docs (no bun, no Architect CLI): what
        # a share link and /app/ need, in a fraction of the build.
        [ -n "$studio_wasm" ] || die "LOCAL_STACK_WEB=app needs Studio's web app (apps/architect/web) built"
        say "web: the app alone (LOCAL_STACK_WEB=app): no site, no docs"
        rm -rf "$out"
        mkdir -p "$out/app" "$out/assets/app-local"
        cp "$arch_src"/apps/web/app/* "$out/app/"
        cp "$json" "$out/app/distribution.json"
        "$bindgen" --target web --no-typescript --out-dir "$out/assets/app-local" "$studio_wasm"
        # As `just web-daw`: smaller, and what a release would serve.
        if command -v wasm-opt >/dev/null; then
            wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
                --enable-sign-ext --enable-mutable-globals --enable-reference-types \
                --enable-multivalue --enable-simd "$out/assets/app-local/architect_web_bg.wasm" \
                -o "$out/assets/app-local/architect_web_bg.wasm" >&2 || say "web: wasm-opt failed: the module is served as built"
        fi
        cp "$arch_src/apps/web/assets/Inter-Variable.woff2" "$arch_src/apps/web/assets/Inter-OFL.txt" "$out/assets/app-local/"
        cp -r "$arch_src/apps/web/assets/luau-web" "$out/assets/app-local/"
        {
            echo "// Written by scripts/local-stack.sh (LOCAL_STACK_WEB=app), as Architect's"
            echo "// scripts/web-bundle.sh writes it."
            echo "import init from '/assets/app-local/architect_web.js';"
            echo "document.documentElement.dataset.assets = '/assets/app-local/';"
            echo "init({ module_or_path: '/assets/app-local/architect_web_bg.wasm' });"
        } >"$out/app/start.js"
    else
        say "web: building Architect's web bundle with Studio Architect's distribution.json"
        (
            cd "$arch_src"
            [ -n "$issuer_target" ] && export CARGO_TARGET_DIR="$issuer_target"
            [ -z "$studio_wasm" ] || export ARCHITECT_WEB_WASM="$studio_wasm" ARCHITECT_WEB_LOCK="$root/Cargo.lock"
            ARCHITECT_DISTRIBUTION="$json" ARCHITECT_WEB_URL="http://127.0.0.1:$web" WASM_BINDGEN="$bindgen" \
                bash scripts/web-bundle.sh "$out" >&2
        )
    fi
    printf '{"issuer":"http://127.0.0.1:%s","client_id":"architect"}' "$auth" >"$out/app/config.json"
    # The page's policy lets it talk to itself and to https; the stack's
    # issuer is plain http on loopback, so the sign-in's token exchange is
    # refused ("NetworkError") unless the page names it. This copy only.
    sed -i "s|connect-src 'self' https:|connect-src 'self' https: http://127.0.0.1:$auth|" "$out/app/index.html"
    say "web: http://127.0.0.1:$web/app/ (sign in as ada@local.test, password development-password)"
    # One origin for the bundle and the node, as the hosted proxy joins
    # them: a share link's page is the node's and loads the app from its
    # own origin, so on the bundle's port alone no link opens.
    say "web: share links open here: $(share_url "http://127.0.0.1:$web")"
    # `LOCAL_STACK_OPEN=on`: the app and the share link in the browser,
    # once both answer.
    if [ "${LOCAL_STACK_OPEN:-off}" = on ] && command -v xdg-open >/dev/null; then
        (
            for _ in $(seq 1 600); do
                up "$web" && up "$(port node)" && [ -f "$state/share" ] && break
                sleep 1
            done
            xdg-open "http://127.0.0.1:$web/app/" >/dev/null 2>&1 || true
            sleep 1
            xdg-open "$(share_url "http://127.0.0.1:$web")" >/dev/null 2>&1 || true
        ) &
    fi
    exec python3 "$root/scripts/local-stack-front.py" "$out" "$web" "$(port node)"
}

# The window. `LOCAL_STACK_DESKTOP_RUN` names a script of the machine's own
# that builds (`--build-only`, under the overrides) and then runs it
# (`--foreground`: until the window closes or the script is told to stop),
# on whatever vault and session that machine works on. Without one: the
# window signed in to the stack's issuer and node, when chosen
# (`just dev desktop=on`); a bare `devenv up` then starts no window.
run_desktop() {
    plan_locked
    load_plan
    load_choice
    local own="${LOCAL_STACK_DESKTOP_RUN:-}"
    if [ -n "$own" ]; then
        case "$own" in /*) ;; *) own="$(cd "$root/$(dirname "$own")" 2>/dev/null && pwd)/$(basename "$own")" ;; esac
        [ -n "$own" ] && [ -x "$own" ] || die "LOCAL_STACK_DESKTOP_RUN=$LOCAL_STACK_DESKTOP_RUN: no such program"
        say "desktop: $own (this machine's own window, vault and session)"
        with_overrides "$own" --build-only >&2
        exec "$own" --foreground
    fi
    if [ "$c_desktop" != on ]; then
        say "desktop: off (just dev desktop=on, or LOCAL_STACK_DESKTOP_RUN)"
        return 0
    fi
    for _ in $(seq 1 7200); do
        curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(port node)/health" && break
        sleep 1
    done
    local vars=() line cfg=() bin
    while IFS= read -r line; do vars+=("$line"); done < <(
        cd "$arch_src" && env DEV_STACK_ROOT="$arch_src" DEV_STACK_STATE="$state/stack" DEVENV_STATE="$state" \
            DEV_STACK_PORT_AUTH="$(port auth)" DEV_STACK_PORT_NODE="$(port node)" \
            ${issuer_target:+CARGO_TARGET_DIR="$issuer_target"} cargo run -q -p xtask -- dev env app
    )
    [ -s "$overrides" ] && cfg=(--config "$overrides")
    # Built under the overrides (and Cargo.lock put back), then run apart
    # from them, so the node's rebuilds do not wait on the window.
    with_overrides cargo build --profile desktop-dev -p studio-architect "${cfg[@]}"
    bin="$(target_dir)/desktop-dev/studio-architect"
    say "desktop: Studio Architect (the window), its issuer and org node the stack's"
    exec env "${vars[@]}" "$bin"
}

# ── Status, phone, reset ─────────────────────────────────────────────────
up() { curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$1/" 2>/dev/null; }

status() {
    [ -f "$state/plan.env" ] && banner
    local name p
    for name in mock_oauth auth node web site; do
        p="$(port "$name")"
        printf '  %-11s http://127.0.0.1:%-6s %s\n' "$name" "$p" "$(up "$p" && echo running || echo 'not running')"
    done
    [ -f "$state/share" ] && echo "  share link  $(share_url)"
    echo "  web app     http://127.0.0.1:$(port web)/app/  (ada@local.test / development-password)"
    echo "  site        http://127.0.0.1:$(port site)/"
}

tailnet_name() {
    tailscale status --self --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["Self"]["DNSName"].rstrip("."))'
}

phone() {
    command -v tailscale >/dev/null || die "no tailscale on this machine"
    local https name node
    https="$(py get phone.https_port)"
    node="$(port node)"
    if [ "${1:-}" = off ]; then
        tailscale serve --https="$https" off
        say "phone: off (https port $https no longer served)"
        return
    fi
    name="$(tailnet_name)"
    # Never over something else already served on that port.
    if tailscale serve status 2>/dev/null | grep -A1 "^https://$name:$https" | grep -q proxy \
        && ! tailscale serve status | grep -A1 "^https://$name:$https" | grep -q "127.0.0.1:$node"; then
        die "https://$name:$https already serves something else: set [phone] https_port (Tailscale serves 443, 8443, 10000)"
    fi
    up "$node" || die "the node is not running (just dev or just demo first)"
    # Tailnet only: serve, never funnel.
    local out
    if ! out="$(tailscale serve --bg --https="$https" "http://127.0.0.1:$node" 2>&1)"; then
        if grep -q 'Access denied' <<<"$out"; then
            die "tailscale refused: this user may not change what it serves. Once, as root: sudo tailscale set --operator=$USER (then just phone again)"
        fi
        die "tailscale serve failed: $out"
    fi
    say "phone: on this tailnet only, with a ts.net certificate (tailscale serve; never funnel). Open on the phone:"
    share_url "https://$name:$https"
    say "turn it off: just phone off"
}

reset() {
    if up "$(port node)" || up "$(port auth)"; then die "the stack is running: stop devenv up first"; fi
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
            node) run_node ;;
            web) run_web ;;
            site) plan_locked && xdev run site ;;
            desktop) run_desktop ;;
            *) die "run: mock-oauth, auth, node, web, site or desktop" ;;
        esac
        ;;
    rebuild) load_choice && load_plan && rebuild "$@" ;;
    status) status ;;
    ready)
        # devenv's readiness probe: the process answers on its port.
        case "${1:-}" in
            auth | node) curl -sf -o /dev/null --max-time 2 "http://127.0.0.1:$(port "$1")/health" ;;
            mock-oauth) curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$(port mock_oauth)/" ;;
            *) curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$(port "$1")/" ;;
        esac
        ;;
    phone) phone "$@" ;;
    smoke) exec bash "$root/scripts/local-stack-smoke.sh" "$@" ;;
    reset) reset ;;
    url) share_url "$@" ;;
    _xdev) xdev "$@" ;;
    _touch-shares) touch_shares ;;
    *)
        sed -n '2,18p' "$0" >&2
        exit 64
        ;;
esac
