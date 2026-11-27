# Studio's recipes. `just` lists them in this order.
#
# Every recipe runs in the dev shell (scripts/dev-shell.sh enters it unless
# this checkout's shell is already loaded), so cargo is the pinned 1.99.1
# with the fleet's kache environment. The cargo on the default PATH is
# 1.98.1: its builds share no cache keys with anyone else's.
#
# Use these recipes rather than ad-hoc `cargo -p` sets: cargo unifies
# features per invocation, so every different package set compiles the
# shared crates a different way, under a different kache key.

set shell := ["bash", "scripts/dev-shell.sh", "-c"]
set script-interpreter := ["bash", "scripts/dev-shell.sh"]

# The expression editor's harness: `just editor <recipe>` (just/editor.just).
mod editor "just/editor.just"

# One build for working on the app: dx's `desktop-dev` profile for the host
# triple, as `dx serve` builds the window. The engine, the tests and the
# window then share one set of compiled crates.
host := if os() == "macos" { arch() + "-apple-darwin" } else { arch() + "-unknown-linux-gnu" }
dev_build := "--profile desktop-dev --target " + host
target_dir := env("CARGO_TARGET_DIR", justfile_directory() / "target")
dev_out := target_dir / host / "desktop-dev"

# The engine's package and program. Today it is the `daw` extension
# (`daw-extension --engine`); when the engine binary moves into apps/studio
# these two lines change, and nothing else.
engine_package := "daw-extension"
engine_bin := "daw-extension"
# What `just studio` and `just engine` export to: the local collector that
# devenv-fleet's telemetry runs (otelcol on 4318 → OpenObserve on 5080). An
# OTEL_* already in the environment wins. Only these recipes export, so tests
# and checks never try to reach a collector.
otlp := "OTEL_EXPORTER_OTLP_ENDPOINT=${OTEL_EXPORTER_OTLP_ENDPOINT:-http://127.0.0.1:4318} OTEL_METRIC_EXPORT_INTERVAL=${OTEL_METRIC_EXPORT_INTERVAL:-5000}"

# List the recipes.
default:
    @just --list --unsorted

# ── Run ──────────────────────────────────────────────────────────────────

# Hot-patching off: it wraps rustc (RUSTC_WORKSPACE_WRAPPER), which kache passes through uncached;
# with it off, the fleet's dx (DX_NO_RUSTC_WRAPPER=1, devenv-fleet's dioxus) runs plain cargo under kache
# The desktop app (apps/studio, Blitz) under `dx serve`, with the engine it starts built first
studio *ARGS: _engine-build
    cd apps/studio && env {{otlp}} dx serve --platform desktop --renderer native --no-default-features --features native --hot-patch false {{dev_build}} {{ARGS}}

# The engine, headless: `run` (the default) starts it in the foreground; `status` and `stop` ask the running one
engine ACTION="run": _engine-build
    env {{otlp}} {{dev_out}}/{{engine_bin}} {{ if ACTION == "run" { "--engine" } else { "--" + ACTION } }}

# The web app under `dx serve` (its /studio/api goes to the Drive node on :8096, apps/studio/Dioxus.toml)
web *ARGS:
    cd apps/studio && dx serve --platform web --features web {{ARGS}}

# The release's web bundle, for the root of fasttrackstudio.app: target/web/studio-web-<version>.tar.gz (scripts/web/bundle.sh)
web-bundle:
    bash scripts/web/bundle.sh

# Starts nothing: `devenv up` runs OpenObserve and the collector (or attaches to the ones already running).
# Metrics, logs and traces from the app and the engine: is the local stack up, and where to look
telemetry:
    #!/usr/bin/env bash
    up() { curl -s --max-time 2 "$1" >/dev/null && echo up || echo DOWN; }
    echo "OpenObserve  http://127.0.0.1:5080   $(up http://127.0.0.1:5080/healthz)   (traces, metrics, logs; login in ~/.config/openobserve/credentials.json)"
    echo "collector    http://127.0.0.1:4318   $(curl -s --max-time 2 -o /dev/null -w '%{http_code}' -X POST http://127.0.0.1:4318/v1/traces -H 'content-type: application/json' -d '{}' | grep -qE '^(200|400)$' && echo up || echo DOWN)   (what \`just studio\` and \`just engine\` export to)"
    echo "If either is DOWN: \`devenv up openobserve otelcol\`"

_engine-build:
    cargo build {{dev_build}} -p {{engine_package}} --bin {{engine_bin}}

# ── Check and test ───────────────────────────────────────────────────────

# The whole workspace compiles, and the layout rules hold (boundaries.toml)
check:
    cargo check --workspace {{dev_build}}
    python3 scripts/boundaries.py check

# The browser builds, for wasm32: the web app, and Keyflow's chart editor as its web build takes it
check-web:
    cargo check -p studio-app --features web --target wasm32-unknown-unknown
    cargo check --target wasm32-unknown-unknown -p editor-keyflow -p keyflow -p keyflow-ui --no-default-features --features editor-keyflow/render,keyflow/text,keyflow/svg,keyflow/highlighting,keyflow-ui/wasm-graphics

# The layout rules (boundaries.toml): `check` (both), `check --edges` (seconds), `tighten`, `groups`
boundaries *ARGS="check":
    python3 scripts/boundaries.py {{ARGS}}

# A group's tests: core, connect, engine, app, tools or all. Extra arguments go to nextest (`just test engine -E 'test(mixer)'`)
[positional-arguments]
[script]
test GROUP="core" *ARGS:
    group="$1"; shift
    pkgs=()
    while read -r p; do pkgs+=(-p "$p"); done < <(python3 scripts/boundaries.py packages "$group" --tested)
    [ "${#pkgs[@]}" -gt 0 ] || { echo "no packages in group $group" >&2; exit 1; }
    cargo nextest run --cargo-profile desktop-dev --target {{host}} "${pkgs[@]}" --no-fail-fast "$@"

# What a person or agent does by hand on Unbreakable, timed against budgets (open, play, LISTEN, LISTEN_AB, MIX_RENDER),
# on the real engine binary, headless and isolated; then the same on a synthetic song. The song is skipped where it isn't
# (FTS_UNBREAKABLE names another, `off` skips it). Budgets: crates/studio-engine/extensions/daw/tests/unbreakable/budgets.toml;
# the record goes to target/, the trend to docs/perf/unbreakable.jsonl. Extra arguments go to nextest
test-unbreakable *ARGS:
    cargo nextest run --cargo-profile desktop-dev --target {{host}} -p {{engine_package}} --test unbreakable --run-ignored all --no-capture --no-fail-fast {{ARGS}}

# The fixture sessions and setlist (crates/studio-engine/extensions/daw/tests/fixtures), opened and played over MCP on
# the real engine binary, headless and isolated: the click, the chart following the playhead, each track heard, the
# setlist moved through. Pass or fail per session; records in target/. Extra arguments go to nextest
test-fixtures *ARGS:
    cargo nextest run --cargo-profile desktop-dev --target {{host}} -p {{engine_package}} --test fixtures --no-fail-fast {{ARGS}}

# A room with a leader and a member, each its own engine, over MCP (Seam 2's base, daw-extension's tests/room)
test-room *ARGS:
    cargo nextest run --cargo-profile desktop-dev --target {{host}} -p {{engine_package}} --test room --no-fail-fast {{ARGS}}

# Seam 3 for the web: the web build opens a fixture session in a headless browser and plays it through the engine in
# the page (apps/studio/tests/it/web_boot.rs). Builds the web app first; WEBDRIVER (chromedriver or geckodriver) and
# CHROME_BIN when set, else nixpkgs' chromedriver and the Chrome it matches. Extra arguments go to nextest
[script]
test-web-boot *ARGS:
    set -euo pipefail
    target="${CARGO_TARGET_DIR:-$PWD/target}"
    (cd apps/studio && dx build --platform web --features web)
    export STUDIO_WEB_DIST="$target/dx/Studio/debug/web/public"
    if [ -z "${WEBDRIVER:-}" ]; then
        # Both unfree (Google's builds): fetched, not built.
        paths=$(NIXPKGS_ALLOW_UNFREE=1 nix build --impure --no-link --print-out-paths nixpkgs#chromedriver nixpkgs#google-chrome)
        export WEBDRIVER="$(echo "$paths" | grep chromedriver)/bin/chromedriver"
        export CHROME_BIN="$(echo "$paths" | grep google-chrome)/bin/google-chrome"
    fi
    cargo nextest run --cargo-profile desktop-dev --target {{host}} -p studio-app --test it -E 'test(/^web_boot::/)' --run-ignored only --no-fail-fast {{ARGS}}

# Seam 1 for the first run (studio#699): a fresh install's Studio signs in, sees its orgs and opens a setlist, all over
# its MCP, against a Drive node and account server of the test's (apps/studio/tests/it/first_run.rs). It opens Studio's
# window, so run it in a desktop session. STUDIO_APP names another Studio to test (a dx build); extra arguments go to nextest
test-first-run *ARGS:
    cargo nextest run --cargo-profile desktop-dev --target {{host}} -p studio-app --features native --test it -E 'test(/^first_run::/)' --run-ignored only --no-fail-fast {{ARGS}}

# Seam 1 for creating an organization (studio#697): signed in, Studio creates an org at the account server, owns it,
# lists it and is in it; a taken slug is refused, all over its MCP (apps/studio/tests/it/org_create.rs, on first_run's
# stand-ins). It opens Studio's window, so run it in a desktop session. STUDIO_APP names another Studio; extra arguments go to nextest
test-org-create *ARGS:
    cargo nextest run --cargo-profile desktop-dev --target {{host}} -p studio-app --features native --test it -E 'test(/^org_create::/)' --run-ignored only --no-fail-fast {{ARGS}}

# The PR gate on this machine: the static gates, then check and tests for what `cargo rail plan` says the change affects (the pre-push hook runs it)
ci-local BASE="":
    bash scripts/ci-local.sh {{BASE}}

# What CI runs after a merge to main, in its order, on this machine (`just ci tests` starts from that step)
[script]
ci FROM="gates":
    export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
    steps=(gates boundaries check web tests)
    start=0
    for i in "${!steps[@]}"; do [ "${steps[$i]}" = "{{FROM}}" ] && start=$i; done
    from() { local at=0 i; for i in "${!steps[@]}"; do [ "${steps[$i]}" = "$1" ] && at=$i; done; [ "$at" -ge "$start" ]; }
    # A full disk fails in the linker, as a bus error that reads like a
    # compiler fault: say so in the first second instead.
    free_gb=$(df -PBG "$CARGO_TARGET_DIR" 2>/dev/null | awk 'NR==2 {gsub(/G/,"",$4); print $4}')
    if [ -n "${free_gb:-}" ] && [ "$free_gb" -lt 25 ]; then
      echo "only ${free_gb}G free under $CARGO_TARGET_DIR; reclaim first (just sweep, just disk)" >&2
      exit 1
    fi
    if from gates; then
      echo "── gates: lockfile, duplicate versions, fmt, workflows, rail config, spec coverage ──"
      bash scripts/check-lockfile.sh
      python3 scripts/check-duplicate-deps.py
      cargo fmt --all --check
      just workflows-check
      cargo rail config validate --strict
      python3 -I scripts/spec-coverage.py --scope lighting >/dev/null
    fi
    if from boundaries; then echo "── boundaries ──"; python3 scripts/boundaries.py check; fi
    if from check; then echo "── cargo check --workspace ──"; cargo check --workspace; fi
    if from web; then echo "── wasm ──"; just check-web; fi
    if from tests; then echo "── tests (all groups) ──"; just assets; just test all; fi
    echo "every CI step passed locally"

# Record rail's observed-input evidence for HEAD, so `ci-local` against it narrows a docs or CI-file change
[script]
rail-evidence:
    dir="$(git rev-parse --git-common-dir)/rail-evidence/$(git rev-parse HEAD)"
    mkdir -p "$dir"
    cargo rail plan evidence --work cargo.build -o "$dir/cargo.build.json" -- check --workspace
    cargo rail plan evidence --work cargo.test -o "$dir/cargo.test.json" -- check --workspace --all-targets
    echo "rail evidence: $dir"

# Spec coverage: reqs / impl / verify per spec (`--scope lighting` is Ignition's gate; `--uncovered`, `--untested`)
spec-coverage *ARGS:
    python3 -I scripts/spec-coverage.py {{ARGS}}

# Crates Cargo.lock holds in more than one version: fails on a new one (`--write` after an intended change)
deps-dupes *ARGS:
    python3 scripts/check-duplicate-deps.py {{ARGS}}

# ── CI workflows ─────────────────────────────────────────────────────────
# .github/workflows is GENERATED from xtask/workflows (one Rust module per
# workflow, the reasons as comments there): edit the Rust, run `just
# workflows`, commit both.

# Write .github/workflows from xtask/workflows
workflows:
    cargo run -q -p xtask-workflows -- generate

# Fail if a workflow on disk is not what `just workflows` writes; then actionlint
workflows-check:
    cargo run -q -p xtask-workflows -- check
    actionlint

# ── Assets, faces, plugins ───────────────────────────────────────────────

# Fetch the large binaries kept out of git (assets.txt) into .assets/ and link them into place (ASSETS_OFFLINE=1: no network)
assets:
    bash scripts/assets.sh

# Make a worktree complete as the main checkout is (assets, faces); the post-checkout hook runs it
worktree-setup:
    bash scripts/worktree-setup.sh --force

# Generate the plugin faces (every `.frame`, the catalogs, rig-faces/faces.json) into plugin-faces/generated; not kept in git
faces *ONLY:
    cargo run -q --release -p plugin-faces -- crates/studio-connect/plugin-ui/plugin-faces/generated {{ if ONLY != "" { "--only " + ONLY } else { "" } }}

# The plugin bundles every plugin recipe iterates: cargo packages, named for
# their bundles in bundler.toml (adding a plugin means this line and that file).
plugins := "eq-plugin comp-plugin reverb-plugin delay-plugin modulation-plugin level-plugin saturate-plugin gate-plugin limiter-plugin trigger-plugin meter-plugin pitch-plugin unison-plugin mic-plugin guide-plugin nam-plugin signal-plugin signal-sampler-clap"

# Bundle plugins as .clap + .vst3 into target/bundled (all of them, or the packages named; universal binaries on macOS)
[script]
plugins-bundle *ONLY:
    cmd=bundle
    [ "$(uname)" = Darwin ] && cmd=bundle-universal
    for p in {{ if ONLY != "" { ONLY } else { plugins } }}; do
        # `export` (the CLAP/VST3 entry points) is a feature of the FX plugins.
        manifest="$(grep -rl --include=Cargo.toml "^name = \"$p\"" crates/studio-engine/plugins | head -1)"
        feat=(); grep -q '^export *=' "$manifest" && feat=(--features export)
        cargo run -q -p fts-plugin-xtask -- "$cmd" -p "$p" --release "${feat[@]}"
    done
    ls "{{target_dir}}/bundled/"

# Symlink the bundles in target/bundled into this machine's CLAP and VST3 folders (never replaces a real install)
[script]
plugins-link:
    src="{{target_dir}}/bundled"
    [ -d "$src" ] || { echo "no $src: run 'just plugins-bundle' first" >&2; exit 1; }
    case "$(uname)" in
        Darwin) clap_dir="$HOME/Library/Audio/Plug-Ins/CLAP"; vst3_dir="$HOME/Library/Audio/Plug-Ins/VST3";;
        *)      clap_dir="$HOME/.clap"; vst3_dir="$HOME/.vst3";;
    esac
    mkdir -p "$clap_dir" "$vst3_dir"
    for bundle in "$src"/*.clap "$src"/*.vst3; do
        [ -e "$bundle" ] || continue
        name="$(basename "$bundle")"
        case "$name" in *.clap) dest="$clap_dir/$name";; *) dest="$vst3_dir/$name";; esac
        if [ -e "$dest" ] && [ ! -L "$dest" ]; then
            printf "  SKIP  %-22s (a real copy is installed there)\n" "$name"; continue
        fi
        rm -f "$dest"; ln -s "$bundle" "$dest"
        printf "  link  %-22s -> %s\n" "$name" "$(dirname "$dest")"
    done

# Remove the symlinks `plugins-link` made, leaving real installs alone
[script]
plugins-unlink:
    src="{{target_dir}}/bundled"
    case "$(uname)" in
        Darwin) dirs=("$HOME/Library/Audio/Plug-Ins/CLAP" "$HOME/Library/Audio/Plug-Ins/VST3");;
        *)      dirs=("$HOME/.clap" "$HOME/.vst3");;
    esac
    for dir in "${dirs[@]}"; do
        [ -d "$dir" ] || continue
        for link in "$dir"/*.clap "$dir"/*.vst3; do
            [ -L "$link" ] || continue
            case "$(readlink "$link")" in "$src"/*) rm -f "$link"; echo "  unlink $(basename "$link")";; esac
        done
    done

# Load every bundle in target/bundled (dlopen, entry point, factory): a bundle that links can still fail in a host. ARCH: x86_64 for the Intel half on macOS
[script]
plugins-verify ARCH="":
    src="{{target_dir}}/bundled"
    [ -d "$src" ] || { echo "no $src: run 'just plugins-bundle' first" >&2; exit 1; }
    out="{{target_dir}}/plugin-verify"; mkdir -p "$out"
    archflag=(); [ -n "{{ARCH}}" ] && archflag=(-arch "{{ARCH}}")
    cc "${archflag[@]}" -o "$out/clap_load" crates/studio-engine/plugins/verify/clap_load.c -ldl
    cc "${archflag[@]}" -o "$out/vst3_load" crates/studio-engine/plugins/verify/vst3_load.c -ldl
    binary_in() {
        if [ -d "$1" ]; then find "$1/Contents" -type f -perm -u+x ! -name "*.txt" ! -name PkgInfo | head -1
        else echo "$1"; fi
    }
    fail=0
    for b in "$src"/*.clap "$src"/*.vst3; do
        [ -e "$b" ] || continue
        case "$b" in *.clap) loader="$out/clap_load";; *) loader="$out/vst3_load";; esac
        bin="$(binary_in "$b")"
        if [ -z "$bin" ]; then printf "%-24s FAIL no binary in bundle\n" "$(basename "$b")"; fail=1; continue; fi
        result="$("$loader" "$bin" 2>&1 | tail -1)"
        printf "%-24s %s\n" "$(basename "$b")" "$result"
        case "$result" in OK*) ;; *) fail=1;; esac
    done
    [ "$fail" = 0 ] || { echo "some bundles do not load" >&2; exit 1; }

# Install or update the FTS Mic models (the private mic-models repo) into $FTS_MIC_MODELS or ~/.local/share/fts/mic-models
[script]
mic-models-install:
    dest="${FTS_MIC_MODELS:-${XDG_DATA_HOME:-$HOME/.local/share}/fts/mic-models}"
    if [ -d "$dest/.git" ]; then git -C "$dest" pull --ff-only
    elif [ -e "$dest" ]; then echo "$dest exists and is not a mic-models checkout; leaving it alone" >&2; exit 1
    else mkdir -p "$(dirname "$dest")"; git clone git@github.com:FastTrackStudios/mic-models.git "$dest"; fi

# Regenerate the golden session (crates/studio-engine/adapters/dynamic-template/fixtures/golden); a test fails while it is stale
daw-template:
    cargo run -p dynamic-template --bin golden-session -- crates/studio-engine/adapters/dynamic-template/fixtures/golden

# Prepare songs once (organize, build from the chart, click and guide) into `Song.session` beside each `.RPP`
[positional-arguments]
[script]
prepare +PROJECTS:
    cargo build {{dev_build}} -p studio-engine --bin prepare
    for p in "$@"; do "{{dev_out}}/prepare" "$p"; done

# ── Releases (the fleet's release flow; scripts/release.sh) ─────────────
# One release for the repo: a `studio-vYY.M.MICRO` tag on a green commit
# of main (or a release/studio-v* hotfix line). The tag alone ships; nothing
# is committed for a release.

# The version this tree carries (NOT the released one, which is the latest studio-v* tag)
version:
    @bash scripts/release.sh version

# What is live, what main holds since, the next version and the draft notes from .changes/
release-plan:
    bash scripts/release.sh plan

# Tag the next version on main's newest green commit (or on SHA) and push it: THIS SHIPS (`--dry-run` tags nothing)
release *ARGS:
    bash scripts/release.sh release {{ARGS}}

# A hotfix while main can't ship: the given main commits cherry-picked onto release/<latest tag>, with a PR into it
release-hotfix +COMMITS:
    bash scripts/release.sh hotfix {{COMMITS}}

# A tag's release notes (default: the draft for origin/main)
release-notes *ARGS:
    @bash scripts/release.sh notes {{ARGS}}

# The release tooling's own end-to-end test (a synthetic repo and a local origin)
release-sandbox:
    bash scripts/release-sandbox.sh

# The release bots' own tests: token mints, sums, facts, the bump's checks, the www [patch] guard
release-bots-test:
    python3 scripts/check-token-mints.py --self-test
    python3 scripts/check-token-mints.py
    bash scripts/release-sums-test.sh
    bash scripts/release-facts-test.sh
    bash scripts/bump-verify-test.sh
    python3 scripts/bump-architect-deps-test.py
    python3 scripts/check-www-patch.py --self-test
    python3 scripts/check-www-patch.py

# Move the architect pin to TAG (e.g. v0.15.1): the manifests, Cargo.lock, the change file (what the bump bot runs)
bump-architect TAG *ARGS:
    python3 scripts/bump-architect.py "{{TAG}}" {{ARGS}}

# ── Hooks and housekeeping ───────────────────────────────────────────────

# Install the git hooks (prek.toml): rustfmt on commit, `ci-local` on push, `worktree-setup` after `git worktree add`
install-hooks:
    git config --unset core.hooksPath || true
    prek install --force

# Move prek (nix/modules/prek.nix) to its newest release, or VERSION
[script]
prek-bump VERSION="":
    v="{{VERSION}}"
    [ -n "$v" ] || v="$(curl -fsSL https://api.github.com/repos/j178/prek/releases/latest | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p')"
    [ -n "$v" ] || { echo "could not read prek's latest release" >&2; exit 1; }
    f=nix/modules/prek.nix
    sed -i.bak "s/version = \"[^\"]*\";/version = \"$v\";/" "$f"
    for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-apple-darwin aarch64-apple-darwin; do
        sum="$(curl -fsSL "https://github.com/j178/prek/releases/download/v$v/prek-$t.tar.gz.sha256" | cut -d' ' -f1)"
        [[ "$sum" =~ ^[0-9a-f]{64}$ ]] || { echo "no sha256 for $t at v$v" >&2; exit 1; }
        sed -i.bak "s|target = \"$t\"; sha256 = \"[0-9a-f]*\"|target = \"$t\"; sha256 = \"$sum\"|" "$f"
    done
    rm -f "$f.bak"
    echo "prek -> $v; run just install-hooks"

# Reclaim artifacts in this worktree's target dir untouched for DAYS days
sweep DAYS="7":
    cargo sweep --time {{DAYS}}

# Each worktree's target dir, largest first
disk:
    git worktree list --porcelain | awk '/^worktree /{print $2"/target"}' | xargs -r du -sh 2>/dev/null | sort -rh

# Build or refresh the repo knowledge graph (graphify, local, no LLM)
graph:
    graphify update . --force

# Serve the knowledge graph over MCP on stdio (.mcp.json)
graph-serve:
    graphify-mcp --transport stdio --graph graphify-out/graph.json
