#!/usr/bin/env bash
# Install FTS plugin bundles (.clap, .vst3) into the standard plugin folders,
# replacing any older copy of the same bundle, and say what went where.
#
# Two callers, one script:
#   - `just install` in a checkout: the bundles `just plugins-bundle` built
#     into target/bundled, the suite's names only (scripts/bundles.sh).
#   - the Linux release archive ships this file beside its bundles:
#     `./install.sh` from the unpacked folder installs every bundle in it.
#
# Usage: install.sh [--from DIR] [--system] [--dry-run] [BUNDLE ...]
#   --from DIR   where the bundles are (default: this script's folder when it
#                holds bundles, else target/bundled of the checkout)
#   --system     every user's folders instead of the current user's (needs
#                root; Linux /usr/lib/{clap,vst3}, macOS /Library/Audio/Plug-Ins)
#   --dry-run    print what would be installed, change nothing
#   BUNDLE       bundle names without extension ("FTS EQ"); default: every
#                .clap/.vst3 in DIR
#
# Folders (CLAP and VST3 specs' standard locations):
#   macOS  ~/Library/Audio/Plug-Ins/{CLAP,VST3}  (--system: /Library/Audio/Plug-Ins/…)
#   Linux  ~/.clap, ~/.vst3                     (--system: /usr/lib/clap, /usr/lib/vst3)
# Windows has its own script, install.ps1.
#
# A bundle is replaced whole (copied beside, then swapped in), so a host
# never sees half of one. Only the exact `<name>.clap` / `<name>.vst3` this
# installs is touched; a symlink left by `just plugins-link` is replaced by
# the copy.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
from=""
system=0
dry=0
names=()
while [ $# -gt 0 ]; do
    case "$1" in
        --from) from="$2"; shift 2 ;;
        --system) system=1; shift ;;
        --dry-run) dry=1; shift ;;
        -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "install.sh: unknown option $1" >&2; exit 2 ;;
        *) names+=("$1"); shift ;;
    esac
done

if [ -z "$from" ]; then
    # A glob loop, not compgen: non-interactive bash builds (NixOS's) lack it.
    from="$(cd "$here/.." && pwd)/target/bundled"
    for b in "$here"/*.clap "$here"/*.vst3; do
        if [ -e "$b" ]; then from="$here"; break; fi
    done
fi
[ -d "$from" ] || { echo "install.sh: no bundles at $from (run 'just plugins-bundle' first)" >&2; exit 1; }

case "$(uname)" in
    Darwin)
        if [ "$system" = 1 ]; then base="/Library/Audio/Plug-Ins"; else base="$HOME/Library/Audio/Plug-Ins"; fi
        clap_dir="$base/CLAP"; vst3_dir="$base/VST3" ;;
    Linux)
        if [ "$system" = 1 ]; then clap_dir="/usr/lib/clap"; vst3_dir="/usr/lib/vst3"
        else clap_dir="$HOME/.clap"; vst3_dir="$HOME/.vst3"; fi ;;
    *) echo "install.sh: $(uname) is not macOS or Linux; on Windows run install.ps1" >&2; exit 1 ;;
esac

# The bundles to install: the names given, else everything in $from.
bundles=()
if [ "${#names[@]}" -gt 0 ]; then
    for n in "${names[@]}"; do
        got=0
        for ext in clap vst3; do
            if [ -e "$from/$n.$ext" ]; then bundles+=("$from/$n.$ext"); got=1; fi
        done
        [ "$got" = 1 ] || { echo "install.sh: no $n.clap or $n.vst3 in $from (run 'just plugins-bundle')" >&2; exit 1; }
    done
else
    for b in "$from"/*.clap "$from"/*.vst3; do
        [ -e "$b" ] && bundles+=("$b")
    done
fi
[ "${#bundles[@]}" -gt 0 ] || { echo "install.sh: no .clap or .vst3 bundles in $from" >&2; exit 1; }

[ "$dry" = 1 ] || mkdir -p "$clap_dir" "$vst3_dir"
installed=0; replaced=0
for b in "${bundles[@]}"; do
    name="$(basename "$b")"
    case "$name" in *.clap) dir="$clap_dir" ;; *) dir="$vst3_dir" ;; esac
    dest="$dir/$name"
    what="install"
    if [ -e "$dest" ] || [ -L "$dest" ]; then what="replace"; fi
    if [ "$dry" = 1 ]; then
        printf '  %-7s %-22s -> %s (dry run)\n' "$what" "$name" "$dir"
        continue
    fi
    tmp="$dir/.$name.installing.$$"
    rm -rf "$tmp"
    # -R copies a bundle directory (macOS, Linux VST3) and a single-file
    # bundle (Linux CLAP) alike; -p keeps modes, so the binary stays executable.
    cp -Rp "$b" "$tmp"
    rm -rf "$dest"
    mv "$tmp" "$dest"
    printf '  %-7s %-22s -> %s\n' "$what" "$name" "$dir"
    installed=$((installed+1))
    [ "$what" = replace ] && replaced=$((replaced+1))
done
if [ "$dry" = 1 ]; then
    echo "dry run: ${#bundles[@]} bundle(s) from $from, nothing changed"
else
    echo "installed $installed bundle(s) from $from ($replaced replaced an older copy)"
    echo "  CLAP: $clap_dir"
    echo "  VST3: $vst3_dir"
    echo "Rescan plugins in your host to pick them up."
fi
