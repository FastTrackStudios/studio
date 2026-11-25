# plugins: FastTrackStudio audio plugin UIs & CLAP/VST3 targets. `just` lists the recipes.

default:
    @just --list

# cargo check of every crate and target
check:
    cargo check --workspace --all-targets --locked

# Every test (nextest does not run doctests; `cargo test --doc` does)
test:
    cargo nextest run --workspace --locked

# What Studio's web builds reach, checked for the browser: the effect
# engines and their painted UI (no plugin shell, no native windowing).
wasm_ui_features := "comp-ui comp-viz delay-ui delay-viz eq-analyzer-ui eq-graph eq-graph-paint eq-ui modulation-ui modulation-viz reverb-ui reverb-viz saturate-ui saturate-viz"

wasm-check:
    cargo check --locked -p fts-audio-ui --target wasm32-unknown-unknown --features shader
    cargo check --locked -p plugin-ui --target wasm32-unknown-unknown --features "{{wasm_ui_features}}"

# architect-ui is a git dependency with no stable path on disk: link the
# resolved checkout where the FX editors' tailwind.css files import it.
_architect-ui-link:
    #!/usr/bin/env bash
    set -euo pipefail
    ui=$(cargo metadata --locked --format-version 1 2>/dev/null \
        | python3 -c 'import json,sys,os; print(next(os.path.dirname(p["manifest_path"]) for p in json.load(sys.stdin)["packages"] if p["name"]=="architect-ui"))')
    ln -sfn "$ui" crates/.architect-ui

# The FX editors' embedded Tailwind sheets (the editor include_str!s
# assets/tailwind.css). Rebuild after class changes in the editor or in
# architect-ui.
tailwind-eq: _architect-ui-link
    tailwindcss -i crates/plugin-ui/eq/tailwind.css -o crates/plugin-ui/eq/assets/tailwind.css --minify

tailwind-comp: _architect-ui-link
    tailwindcss -i crates/plugin-ui/comp/tailwind.css -o crates/plugin-ui/comp/assets/tailwind.css --minify

tailwind-limiter: _architect-ui-link
    tailwindcss -i crates/plugin-ui/comp/ui_limiter-tailwind.css -o crates/plugin-ui/comp/ui_limiter-assets/tailwind.css --minify

tailwind-trigger: _architect-ui-link
    tailwindcss -i crates/plugin-ui/trigger/tailwind.css -o crates/plugin-ui/trigger/assets/tailwind.css --minify

tailwind-eq-standalone: _architect-ui-link
    tailwindcss -i apps/plugins/eq-standalone/tailwind.css -o apps/plugins/eq-standalone/assets/tailwind.css --minify

# THE plugin suite — the one list every plugin recipe iterates. A name here
# is `<name>-plugin` as a cargo package and "FTS <Name>" as a bundle (see
# bundler.toml); adding a plugin means touching this line and that file.
fts_plugins := "eq comp reverb delay modulation level saturate gate limiter trigger meter pitch unison"

# Bundle every FTS plugin as .clap + .vst3 (target/bundled/, names from
# bundler.toml). Pass a subset to bundle only those:
#   just plugins-bundle "eq comp"
# Debug of a single plugin: cargo run -p fts-plugin-xtask -- bundle -p eq-plugin
#
# On macOS this uses nice-plug-xtask's `bundle-universal` instead of
# `bundle`: it builds both aarch64-apple-darwin and x86_64-apple-darwin and
# lipo's them into one universal .clap/.vst3 per plugin — no custom lipo
# scripting needed. Requires the x86_64-apple-darwin rustc target
# (`rustup target add x86_64-apple-darwin`).
plugins-bundle plugins=fts_plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    cmd=bundle
    [ "$(uname)" = "Darwin" ] && cmd=bundle-universal
    for p in {{plugins}}; do
        # `export` (the CLAP/VST3 entry points) is a feature of each plugin.
        feat=(); grep -q '^export *=' "apps/plugins/$p/Cargo.toml" && feat=(--features export)
        cargo run -q -p fts-plugin-xtask -- "$cmd" -p "$p-plugin" --release "${feat[@]}"
    done
    ls target/bundled/

# Symlink the built bundles into this machine's user plugin dirs, so a rebuild
# is live in REAPER (and anything else that scans them) without reinstalling.
#
# Points at `target/bundled` rather than copying it. The bundler rewrites each bundle in place at a stable path, so the link stays
# valid across rebuilds — `just plugins-bundle eq` and the next plugin scan
# sees the new binary.
#
#   just plugins-link              # every bundle currently in target/bundled
#   just plugins-bundle eq && ...  # rebuild one; the link already points at it
#
# Idempotent, and deliberately conservative: it replaces its own symlinks but
# refuses to delete a REAL directory of the same name, which would be a copy
# installed by hand or by a release. Remove those yourself first —
# a plugin folder is not somewhere to be clever with rm -rf.
plugins-link:
    #!/usr/bin/env bash
    set -euo pipefail
    src="$(pwd)/target/bundled"
    [ -d "$src" ] || { echo "no target/bundled — run 'just plugins-bundle' first" >&2; exit 1; }
    case "$(uname)" in
        Darwin) clap_dir="$HOME/Library/Audio/Plug-Ins/CLAP"; vst3_dir="$HOME/Library/Audio/Plug-Ins/VST3";;
        *)      clap_dir="$HOME/.clap"; vst3_dir="$HOME/.vst3";;
    esac
    mkdir -p "$clap_dir" "$vst3_dir"
    linked=0; skipped=0
    for bundle in "$src"/*.clap "$src"/*.vst3; do
        [ -e "$bundle" ] || continue
        name="$(basename "$bundle")"
        case "$name" in *.clap) dest_dir="$clap_dir";; *) dest_dir="$vst3_dir";; esac
        dest="$dest_dir/$name"
        if [ -e "$dest" ] && [ ! -L "$dest" ]; then
            printf "  SKIP  %-22s (a real copy is installed at %s)\n" "$name" "$dest_dir"
            skipped=$((skipped+1))
            continue
        fi
        rm -f "$dest"
        ln -s "$bundle" "$dest"
        printf "  link  %-22s -> %s\n" "$name" "$dest_dir"
        linked=$((linked+1))
    done
    echo "linked $linked bundle(s), skipped $skipped"

# Remove the symlinks `plugins-link` made, leaving any real installs alone.
plugins-unlink:
    #!/usr/bin/env bash
    set -euo pipefail
    src="$(pwd)/target/bundled"
    case "$(uname)" in
        Darwin) dirs=("$HOME/Library/Audio/Plug-Ins/CLAP" "$HOME/Library/Audio/Plug-Ins/VST3");;
        *)      dirs=("$HOME/.clap" "$HOME/.vst3");;
    esac
    for dir in "${dirs[@]}"; do
        [ -d "$dir" ] || continue
        for link in "$dir"/*.clap "$dir"/*.vst3; do
            [ -L "$link" ] || continue
            target="$(readlink "$link")"
            case "$target" in "$src"/*) rm -f "$link"; echo "  unlink $(basename "$link")";; esac
        done
    done

# Prove every bundle in target/bundled actually LOADS — dlopen it, run its
# entry point, and walk its factory (apps/plugins/verify/). A plugin that
# compiles, links, and exports the right symbol can still fail in a host: a
# missing dependency or a panicking init only shows up at load time, and
# `nm` cannot see either.
#
# Works on both platforms, including their different bundle shapes (Linux
# .clap is a bare shared object, macOS .clap is a directory) and different
# VST3 entry-point names (ModuleEntry vs bundleEntry). On macOS, pass an
# arch to run the universal binaries in one personality:
#   just plugins-verify              # native
#   just plugins-verify x86_64       # the Intel half, under Rosetta
plugins-verify arch="":
    #!/usr/bin/env bash
    set -euo pipefail
    [ -d target/bundled ] || { echo "no target/bundled — run 'just plugins-bundle' first" >&2; exit 1; }
    out="target/plugin-verify"; mkdir -p "$out"
    archflag=""
    [ -n "{{arch}}" ] && archflag="-arch {{arch}}"
    cc $archflag -o "$out/clap_load" apps/plugins/verify/clap_load.c -ldl
    cc $archflag -o "$out/vst3_load" apps/plugins/verify/vst3_load.c -ldl
    # The loadable binary inside a bundle, whatever shape the bundle is.
    binary_in() {
        if [ -d "$1" ]; then find "$1/Contents" -type f -perm -u+x ! -name "*.txt" ! -name "PkgInfo" | head -1
        else echo "$1"; fi
    }
    fail=0
    for b in target/bundled/*.clap target/bundled/*.vst3; do
        [ -e "$b" ] || continue
        case "$b" in *.clap) loader="$out/clap_load";; *) loader="$out/vst3_load";; esac
        bin="$(binary_in "$b")"
        if [ -z "$bin" ]; then printf "%-24s FAIL no binary in bundle\n" "$(basename "$b")"; fail=1; continue; fi
        result="$("$loader" "$bin" 2>&1 | tail -1)"
        printf "%-24s %s\n" "$(basename "$b")" "$result"
        case "$result" in OK*) ;; *) fail=1;; esac
    done
    [ "$fail" = 0 ] || { echo "FAILURES — some bundles do not load" >&2; exit 1; }
    echo "all bundles load"

# The models are the private FastTrackStudios/mic-models repository, read from
# $FTS_MIC_MODELS, else ~/.local/share/fts/mic-models. The default mic (LD-47K)
# is built into the plugin, so this is only needed for the other thirty-nine.
# Install or update the FTS Mic models for the plugin
mic-models-install:
    #!/usr/bin/env bash
    set -euo pipefail
    dest="${FTS_MIC_MODELS:-${XDG_DATA_HOME:-$HOME/.local/share}/fts/mic-models}"
    if [ -d "$dest/.git" ]; then
        git -C "$dest" pull --ff-only
    elif [ -e "$dest" ]; then
        echo "$dest exists and is not a mic-models checkout; leaving it alone" >&2
        exit 1
    else
        mkdir -p "$(dirname "$dest")"
        git clone git@github.com:FastTrackStudios/mic-models.git "$dest"
    fi

# Install the built plugins for the current user, replacing older copies
# (scripts/install.sh): macOS ~/Library/Audio/Plug-Ins/{CLAP,VST3}, Linux
# ~/.clap and ~/.vst3. Copies, unlike plugins-link; run `just plugins-bundle`
# first. Pass a subset as for plugins-bundle, or flags to install.sh:
#   just install                 # the suite
#   just install "eq comp"
#   just install "eq comp" --dry-run
install plugins=fts_plugins *flags:
    #!/usr/bin/env bash
    set -euo pipefail
    names=(); while IFS= read -r n; do names+=("$n"); done < <(bash scripts/bundles.sh {{plugins}})
    bash scripts/install.sh --from target/bundled {{flags}} "${names[@]}"

# The macOS installer: dist/FTS-Plugins-<version>-macos.pkg, the suite's
# universal bundles Developer-ID-signed, the pkg signed with the Developer
# ID Installer identity (scripts/macos/pkg.sh). Not notarized unless
# NOTARIZE=1. Local structure test without the identities: PKG_UNSIGNED=1.
plugins-pkg: plugins-bundle
    bash scripts/macos/pkg.sh {{fts_plugins}}

# The Linux / Windows release archive: dist/fts-plugins-<version>-<platform>
# .tar.gz or .zip with the suite's bundles and install.sh / install.ps1
# (scripts/package.sh), plus its dist/SHA256SUMS line.
plugins-package: plugins-bundle
    bash scripts/package.sh {{fts_plugins}}

# ── Releases (scripts/release.sh; the fleet release flow, as patchbay's) ──
# A release PR bumps the CalVer version and writes CHANGELOG.md from
# .changes/; the tag on its merge commit starts release.yml, which builds
# the installers and publishes the GitHub Release with them.

# What a release would ship now: the next version, the draft notes. Reads only.
release-plan *REF:
    bash scripts/release.sh plan {{REF}}

# Cut `release/pending` from origin/main: version + CHANGELOG.md section from
# .changes/ (deleting them), one commit `chore(release): v<next>`.
release-prepare *ARGS:
    bash scripts/release.sh prepare {{ARGS}}

# On release/pending: push it and open its PR.
release-pr:
    bash scripts/release.sh pr

# Re-run the release guard against the current base, then squash-merge the
# release PR.
release-merge:
    bash scripts/release.sh merge

# After the release PR merged: tag v<version> on its merge commit. THE TAG
# SHIPS (release.yml). Push as a person or the fts-forge App, not a workflow
# token (that starts no workflow).
release-tag *ARGS:
    bash scripts/release.sh tag {{ARGS}}

# Hotfix while main can't ship (see scripts/release.sh).
release-hotfix +COMMITS:
    bash scripts/release.sh hotfix {{COMMITS}}

# One version's section of CHANGELOG.md (default: the current version).
release-notes *ARGS:
    @bash scripts/release.sh notes {{ARGS}}
