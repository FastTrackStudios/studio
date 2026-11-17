#!/usr/bin/env bash
# THE LEGACY LOCAL STACK (docs/local-stack.md, "The legacy stack"): Studio's
# headless node, Architect's issuer, the static web app, the front that
# puts node and web app on one origin, and the site, as separate
# processes (`devenv --profile legacy-stack up`, `just dev-legacy`). The
# local stack is now the engine, Studio and Studio Web (scripts/studio-dev.sh);
# this stays until nothing needs it.
#
#   local-stack.sh choose [architect=pinned|local|PATH] [keyflow=…]
#                         [watch=on|off] [mode=dev|demo]
#                                         what the next `devenv up` runs
#   local-stack.sh plan                   resolve it, write the overrides, say it
#   local-stack.sh procs                  the devenv processes the choice starts
#   local-stack.sh run mock-oauth|auth|node|web|front|site
#                                         one process, as devenv.nix starts it
#   local-stack.sh status                 what runs, from where, the URLs
#   local-stack.sh sync [watch]           the vault copies, again from the source
#   local-stack.sh phone [off]            the front on the tailnet over HTTPS
#   local-stack.sh smoke                  the stack, checked end to end
#   local-stack.sh reset                  remove the stack's state
#
# Settings: local-stack.toml, with this machine's local-stack.local.toml
# (gitignored; `LOCAL_STACK_LOCAL` names another) merged over it, and the
# LOCAL_STACK_* variables over both.
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

ARCH_URL=https://github.com/FastTrackStudios/architect
KF_URL=https://github.com/FastTrackStudios/keyflow
# devenv's state for THIS checkout: a shell of another repo (Architect's
# `just dev` runs this from its own) carries that repo's DEVENV_STATE.
devenv_state="$root/.devenv/state"
if [ "${DEVENV_ROOT:-}" = "$root" ] && [ -n "${DEVENV_STATE:-}" ]; then
    devenv_state="$DEVENV_STATE"
fi
# `LOCAL_STACK_STATE` puts a second stack of this checkout beside the first.
state="${LOCAL_STACK_STATE:-$devenv_state/local-stack}"
mkdir -p "$state"

say() { printf 'local-stack: %s\n' "$*" >&2; }
die() { say "$*"; exit 1; }
py() { python3 "$root/scripts/local-stack.py" "$@"; }
py_local() { echo "${LOCAL_STACK_LOCAL:-$root/local-stack.local.toml}"; }

# A setting: the variable VAR when set, else KEY (local-stack.local.toml
# over local-stack.toml), else DEFAULT.
setting() {
    local var="$1" key="$2"
    if [ -n "${!var:-}" ]; then echo "${!var}"; else py get "$key" "${@:3}"; fi
}

port() {
    local name="$1"
    setting "LOCAL_STACK_PORT_$(tr '[:lower:]' '[:upper:]' <<<"$name")" "ports.$name"
}

# The one origin a browser uses: the front (the node, with the web app
# beside it at /app/).
origin() { echo "http://127.0.0.1:$(port front)"; }

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
    local architect keyflow watch mode=dev arg key value
    architect="$(py get choose.architect pinned)"
    keyflow="$(py get choose.keyflow pinned)"
    watch="$(py get choose.watch on)"
    for arg in "$@"; do
        [ -n "$arg" ] || continue
        key="${arg%%=*}"
        value="${arg#*=}"
        [ "$key" != "$arg" ] || die "\`$arg\`: give key=value (architect=local, keyflow=pinned, watch=off)"
        case "$key" in
            architect) architect="$value" ;;
            keyflow) keyflow="$value" ;;
            watch) watch="$value" ;;
            mode) mode="$value" ;;
            *) die "unknown choice \`$key\` (architect, keyflow, watch, mode; the windows are Studio now: just dev)" ;;
        esac
    done
    case "$watch" in on | off) ;; *) die "\`$watch\`: on or off" ;; esac
    case "$mode" in dev | demo) ;; *) die "mode: dev or demo" ;; esac
    {
        printf 'c_architect=%q\n' "$architect"
        printf 'c_keyflow=%q\n' "$keyflow"
        printf 'c_watch=%q\n' "$watch"
        printf 'c_mode=%q\n' "$mode"
    } >"$choice_file"
}

# The last choice; before any, local-stack(.local).toml's [choose].
load_choice() {
    c_architect="$(py get choose.architect pinned)"
    c_keyflow="$(py get choose.keyflow pinned)"
    c_watch="$(py get choose.watch on)"
    c_mode=dev
    # shellcheck source=/dev/null
    [ -f "$choice_file" ] && . "$choice_file"
    return 0
}

procs() {
    load_choice
    if [ "$c_mode" = demo ]; then
        echo "mock-oauth auth node legacy-web front"
    else
        echo "mock-oauth auth node legacy-web front site"
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
            path="$(locate "$(setting "$var" "sources.$name")")"
            if [ -z "$path" ] || [ ! -f "$path/Cargo.toml" ]; then
                say "NOTICE: $name=local, but there is no $name checkout at $(setting "$var" "sources.$name") (from $root or $(main_checkout)): $name stays at the tag Studio pins"
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
        sibling="$(locate "$(setting LOCAL_STACK_ARCHITECT_PATH sources.architect)")"
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

plan() {
    load_choice
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

share_code() { setting LOCAL_STACK_SHARE_CODE vault.share_code; }
vault_source() { setting LOCAL_STACK_VAULT vault.source ''; }
vault_setlist() { setting LOCAL_STACK_SETLIST vault.setlist; }

banner() {
    load_choice
    local rule studio_desc arch_desc kf_desc arch_src
    # shellcheck source=/dev/null
    . "$state/plan.env"
    rule="$(printf '=%.0s' {1..72})"
    {
        echo "$rule"
        echo "  STUDIO'S LEGACY LOCAL STACK ($c_mode; watch $c_watch)"
        echo "  studio     $studio_desc"
        echo "  architect  $arch_desc"
        echo "  keyflow    $kf_desc"
        [ -s "$overrides" ] && echo "  overrides  $overrides (cargo --config; never committed)"
        echo "  issuer     $arch_src (cargo xtask dev)"
        echo "  vault      $(vault_source) ($(vault_setlist))"
        [ -f "$(py_local)" ] && echo "  settings   $(py_local) over local-stack.toml"
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
    just island-build >&2
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
    src="$(locate "$(vault_source)")"
    dst="$(node_vault)"
    setlist="$(vault_setlist)"
    code="$(share_code)"
    if [ -z "$src" ]; then
        say "NOTICE: no test vault at $(vault_source): the node holds Architect's demo seed (setlist Fall Show, /s/ksqx75iaaaaaaaae/)"
        echo "ksqx75iaaaaaaaae setlists/fall-show" >"$state/share"
        return 0
    fi
    if [ ! -d "$dst" ]; then
        mkdir -p "$(dirname "$dst")"
        chmod 700 "$state/stack" "$state/stack/node" "$(dirname "$(dirname "$dst")")" 2>/dev/null || true
        say "copying the test vault $src into $dst (once; just dev-sync copies it again, just dev-reset drops it)"
        cp -a --reflink=auto "$src/." "$dst.tmp/" 2>/dev/null || { mkdir -p "$dst.tmp" && cp -a --reflink=auto "$src/." "$dst.tmp/"; }
        mv "$dst.tmp" "$dst"
    fi
    out="$(py seed-share "$dst" "$setlist" "$code" "$vault_id")"
    echo "$code ${out#* }" >"$state/share"
}

# The share link, on the front (or ORIGIN).
share_url() {
    local origin="${1:-$(origin)}" code page
    read -r code page <"$state/share"
    echo "$origin/s/$code/$page/"
}

# The copies, again from the source: what changed there comes in, what
# was deleted there goes, the share note is written again. What Architect
# keeps in a vault (_architect/, .architect/) stays the copy's own.
sync_vaults() {
    local src dst
    src="$(locate "$(vault_source)")"
    [ -n "$src" ] || die "no test vault at $(vault_source)"
    dst="$(node_vault)"
    if [ -d "$dst" ]; then
        rsync -a --delete --exclude=/_architect/ --exclude=/.architect/ --exclude=/.obsidian/workspace* "$src/" "$dst/"
        say "synced $src -> $dst"
    fi
    [ -d "$(node_vault)" ] && seed_vault
    return 0
}

sync_cmd() {
    if [ "${1:-}" = watch ]; then
        local src
        src="$(locate "$(vault_source)")"
        [ -n "$src" ] || die "no test vault at $(vault_source)"
        sync_vaults
        say "watching $src: every change is synced (Ctrl-C stops)"
        exec cargo watch -q --postpone --delay 1 -w "$src" -s "'$self' sync"
    fi
    sync_vaults
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
        # The web app's origin, to the issuer (its redirect, CORS) and the
        # node (its lane origins): the front, where the browser has it.
        DEV_STACK_PORT_WEB="$(port front)"
        DEV_STACK_PORT_SITE="$(port site)"
        DEV_STACK_BIN_NODE="$state/node-wrapper"
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
        # A share link opens the web app (#646): the front serves it at
        # /app/ on the node's origin. xtask says so too, from Architect's
        # main on; this covers an Architect from before.
        echo 'export ARCHITECT_WEB_APP=beside'
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
                "$self" _xdev run node &
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
    say "web: building Architect's web bundle with Studio Architect's distribution.json"
    (
        cd "$arch_src"
        [ -n "$issuer_target" ] && export CARGO_TARGET_DIR="$issuer_target"
        ARCHITECT_DISTRIBUTION="$json" ARCHITECT_WEB_URL="$(origin)" WASM_BINDGEN="$bindgen" \
            bash scripts/web-bundle.sh "$out" >&2
    )
    printf '{"issuer":"http://127.0.0.1:%s","client_id":"architect"}' "$auth" >"$out/app/config.json"
    say "web: $(origin)/app/ through the front (sign in as ada@local.test, password development-password)"
    exec static-web-server --config-file "$arch_src/apps/web/sws.toml" --root "$out" --host 127.0.0.1 --port "$web"
}

# The front: the node and the web app on one origin, as production's web
# front end has them. A share link (/s/<code>/) is the web app's page
# (Architect #646), loading /app/start.js and the bundle's /assets/app-*
# from its own origin; the sign-in comes back to /auth/callback. Those go
# to the web app, everything else (and its WebSockets) to the node.
# Caddy, as nothing in Architect's dev stack is a front of its own there
# (dx serve's proxy does it in the other direction).
run_front() {
    local cfg="$state/Caddyfile" auth
    auth="http://127.0.0.1:$(port auth)"
    cat >"$cfg" <<CADDY
# Written by scripts/local-stack.sh (run front); every start writes it again.
{
	admin off
	auto_https off
	persist_config off
}
http://127.0.0.1:$(port front), http://localhost:$(port front) {
	@webapp path /app /app/* /assets/app-* /auth/callback /auth/callback/
	handle @webapp {
		reverse_proxy 127.0.0.1:$(port web) {
			# The bundle's CSP names production's issuer: here, the stack's.
			header_down Content-Security-Policy "https://auth.fasttrackstudio.app" "$auth"
		}
	}
	handle {
		reverse_proxy 127.0.0.1:$(port node)
	}
}
CADDY
    say "front: $(origin) (/app/ the web app, the rest the node)"
    exec caddy run --config "$cfg" --adapter caddyfile
}

# ── Status, phone, reset ─────────────────────────────────────────────────
up() { curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$1/" 2>/dev/null; }

status() {
    [ -f "$state/plan.env" ] && banner
    local name p
    for name in mock_oauth auth node web front site; do
        p="$(port "$name")"
        printf '  %-11s http://127.0.0.1:%-6s %s\n' "$name" "$p" "$(up "$p" && echo running || echo 'not running')"
    done
    [ -f "$state/share" ] && echo "  share link  $(share_url)"
    echo "  web app     $(origin)/app/  (ada@local.test / development-password)"
    echo "  site        http://127.0.0.1:$(port site)/"
}

tailnet_name() {
    tailscale status --self --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["Self"]["DNSName"].rstrip("."))'
}

phone() {
    command -v tailscale >/dev/null || die "no tailscale on this machine"
    local https name node
    https="$(py get phone.https_port)"
    # The front: the share page needs the web app beside the node.
    node="$(port front)"
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
    up "$node" || die "the front is not running (just dev or just demo first)"
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
    if up "$(port node)" || up "$(port auth)" || up "$(port front)"; then die "the stack is running: stop devenv up first"; fi
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
            web | legacy-web) run_web ;;
            front) run_front ;;
            site) plan_locked && xdev run site ;;
            *) die "run: mock-oauth, auth, node, web, front or site" ;;
        esac
        ;;
    rebuild) load_choice && load_plan && rebuild "$@" ;;
    status) status ;;
    sync) sync_cmd "$@" ;;
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
    _pinned-architect) pinned_architect ;;
    _touch-shares) touch_shares ;;
    *)
        sed -n '2,25p' "$0" >&2
        exit 64
        ;;
esac
