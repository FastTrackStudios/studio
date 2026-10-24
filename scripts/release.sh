#!/usr/bin/env bash
# studio releases: ONE version, ONE changelog, ONE tag, ONE GitHub Release.
#
# Fleet release flow v4 (trunk-based; the fleet `release-workflow` skill is
# the source, docs/agents/release-workflow.md holds studio's specifics):
#
#   feat/... fix/... --PR, squash--> main     main is the only long-lived
#       each PR carries a cargo-rail           branch, and anything on it is
#       change file (.changes/*.md)            safe to ship. A push to main
#                                              ships NOTHING.
#   main --release PR (release/pending: ONE commit, version + changelog)-->
#       squash --> tag vYY.M.MICRO on that merge commit
#                                              The TAG ships: the two sites
#                                              (deploy), the Studio
#                                              TestFlight upload and the
#                                              one GitHub Release. (The
#                                              REAPER suites run locally.)
#
#   hotfix, when main can't ship: release/v<latest tag> (from the tag)
#       <-- PR of `cherry-pick -x` of fixes already on main
#       <-- release PR (release/pending) --> tag the next version there,
#       then a forward-port PR copies its changelog section into main.
#       release/* branches are never merged back.
#
# Production is the latest `v*` tag.
#
# The version lives in ONE place, `[workspace.package] version` in the
# root Cargo.toml. Every crate that says `version.workspace = true` follows
# it, and that includes the iOS app crate (apps/studio/desktop: Studio,
# Signal and Session in one app). There are no per-app tags, branches or
# changelogs. The `v<version>` tag drives the TestFlight upload
# (studio-ios.yml): its marketing version is the tag's version, and it
# writes its build number into the Release's notes. The plugins'
# tags are not covered here.
#
# Versions are CalVer YY.M.MICRO (no leading zeros: Cargo and Apple both
# reject them). The NEXT version is computed, never chosen: take the
# highest vX.Y.Z tag (tags with a `-` suffix don't count); if its YY.M is
# this month (UTC) the next is MICRO+1, otherwise YY.M.0 of this month.
# A version the line already carries counts as taken even untagged (a
# release that merged and was skipped, `prepare --skip-untagged`): the next
# is then above it. Hotfixes use the same rule, so every version is unique and ascending
# (App Store Connect requires it). The 0.1.x tags predate CalVer:
# 0.1.2 -> 26.M.0 is ascending.
#
# The changelog is built from the cargo-rail change files in .changes/
# (`cargo rail change add <crates> --bump <none|patch|minor|major> -m
# "<note>"`, one per PR): major -> "Breaking / Migration", minor ->
# "Added", patch -> "Fixed and improved", none -> no entry. The release
# commit deletes the files it consumed. With no change files at all (the
# history before v4) it falls back to git-cliff over the commit range.
# `cargo rail release` cannot do this yet: it refuses crates that inherit
# `version.workspace = true`.
#
#   scripts/release.sh version            the current version
#   scripts/release.sh next               the version the next release gets
#   scripts/release.sh plan               what is live, what main holds since,
#                                          the next version, the draft notes
#   scripts/release.sh prepare [X.Y.Z]    cut release/pending from origin/main:
#                                          ONE commit `chore(release): vX.Y.Z`
#                                          (version + CHANGELOG section from
#                                          .changes/, which it deletes). X.Y.Z
#                                          only pins what you expect: it must
#                                          be the computed next version.
#   scripts/release.sh prepare --hotfix [X.Y.Z]
#                                         the same, from origin/release/v<latest tag>
#   scripts/release.sh pr                 push release/pending (or a hotfix
#                                          branch) and open its PR
#   scripts/release.sh merge              re-run the guard against the CURRENT
#                                          base, then squash-merge the release
#                                          PR (pinned to its head commit)
#   scripts/release.sh tag [X.Y.Z]        after the release PR merged: tag
#                                          vX.Y.Z on its merge commit, only if
#                                          that commit's `CI passed` is green
#                                          and its images are on GHCR, public
#                                          and labelled with it (images.yml,
#                                          scripts/ghcr-images.sh) (this is
#                                          what ships); after a hotfix, open
#                                          the forward-port PR into main
#   scripts/release.sh hotfix <commit>... release/v<latest tag> from the tag
#                                          (if missing), a branch with the fixes
#                                          cherry-picked (-x) from main, its PR
#   scripts/release.sh forward-port X.Y.Z PR into main: the hotfix's changelog
#                                          section, its consumed change files
#                                          deleted
#   scripts/release.sh notes [VERSION]    one version's changelog section
#   scripts/release.sh guard              CI (release.yml): is this release PR
#                                          sound? (other PRs: exit 0)
#   scripts/release.sh release-only       CI (checks.yml plan): 0 = a sound
#                                          release PR, 1 = not a release PR,
#                                          3 = a release PR the guard refuses
#   scripts/release.sh tag-guard          CI (deploy, release): is this
#                                          tag a release on main or release/v*?
#   scripts/release.sh publish [TAG]      CI: the GitHub Release for the tag
#   scripts/release.sh stamp X.Y.Z        CI, first step of every release build:
#                                          write X.Y.Z into this checkout's
#                                          [workspace.package] version and the
#                                          workspace members' Cargo.lock
#                                          entries (uncommitted), so every
#                                          env!("CARGO_PKG_VERSION") is the
#                                          release's
#   scripts/release.sh ios-resolve APP TAG
#                                         CI (<app>-ios.yml): refuse a tag that
#                                          is not vX.Y.Z, whose app crate
#                                          version differs, that has no
#                                          changelog section or is not on main
#                                          or release/v*; print the marketing
#                                          version
#   scripts/release.sh ios-release TAG    CI: make sure the tag's GitHub Release
#                                          exists (idempotent, safe to run from
#                                          both iOS workflows at once)
#   scripts/release.sh ios-record APP TAG MARKETING BUILD [RUN_URL]
#                                         CI: after the TestFlight upload, write
#                                          the marketing version + build number
#                                          into the release notes (one line per
#                                          app; safe to run concurrently)
#
# See docs/agents/release-workflow.md for the whole procedure.
set -euo pipefail

# This script, absolute, before the cd: `merge` and `release-only` run its
# guard in a child process (CI runs a copy taken from the base commit).
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
REPO="${GITHUB_REPOSITORY:-FastTrackStudios/studio}"
MAIN_BRANCH="${MAIN_BRANCH:-main}"
# The ONE name every release PR's head has, into main and into a hotfix
# line alike: one release is in flight at a time, and the guard knows a
# release PR by this name alone.
RELEASE_BRANCH="release/pending"
CHANGES_DIR=".changes"
# The required check of checks.yml (the aggregator). A release is tagged,
# deployed and uploaded only from a commit where it is green.
BUILD_CHECK="${BUILD_CHECK:-CI passed}"
# studio-web's Helm chart: the release commit sets its version and
# appVersion, and appVersion is the image tag the chart at the tag renders
# (scripts/ghcr-images.sh), the one Argo CD pulls.
CHART="deploy/chart/studio/Chart.yaml"
# A release line cut from a tag before the chart existed (a hotfix of an
# older release) has none: it ships no studio-web, and its release commit
# bumps no chart.
has_chart() { git cat-file -e "${1:-HEAD}:$CHART" 2>/dev/null; } # [rev]
# A field of the chart at <rev> (`version`, `appVersion`), unquoted.
chart_field() { # <rev> <field>
    git show "$1:$CHART" 2>/dev/null | sed -n "s/^$2: *\"\{0,1\}\([^\"[:space:]]*\)\"\{0,1\}.*/\1/p" | head -1
}

die() { echo "release: $*" >&2; exit 1; }
note() { echo "release: $*" >&2; }

# git-cliff from PATH, else from nixpkgs (it is not in the dev shell, and
# adding it there would make every flake.lock bump an infra change). Only
# the fallback for a release with no change files uses it.
cliff() {
    if command -v git-cliff >/dev/null 2>&1; then
        git-cliff "$@"
    elif command -v nix >/dev/null 2>&1; then
        nix run --accept-flake-config nixpkgs#git-cliff -- "$@"
    else
        die "git-cliff not found (and no nix to fetch it)"
    fi
}

# `[workspace.package] version` of Cargo.toml at a revision (default: the
# working tree).
version_at() {
    local src
    if [ -n "${1:-}" ]; then src="$(git show "$1:Cargo.toml")"; else src="$(cat Cargo.toml)"; fi
    printf '%s\n' "$src" \
        | sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' \
        | head -1
}

is_semver() { [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; }

# YY.M.MICRO: two-digit year, month 1-12 and a counter, no zero padding
# (`26.01.0` is not a valid Cargo version).
is_calver() { [[ "$1" =~ ^[1-9][0-9]\.([1-9]|1[0-2])\.(0|[1-9][0-9]*)$ ]]; }

# The release commit's subject for a version, as an ERE (the squash merge
# appends " (#N)").
release_subject_re() { # <version>
    printf '^chore\\(release\\): v%s( \\(#[0-9]+\\))?$' "${1//./\\.}"
}

# true when $1 > $2 (both X.Y.Z)
gt() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]; }

# Release tags, newest first. versionsort.suffix=- sorts a `-rc.N` before
# its release; the grep then drops every suffixed tag anyway.
release_tags() { # [git tag args...]
    git -c versionsort.suffix=- tag --list 'v[0-9]*' --sort=-v:refname "$@" \
        | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' || true
}

# The highest release tag in the repo, on any line (main or a hotfix).
latest_tag() { release_tags | head -1; }

# The newest release tag reachable from a revision (the start of the range
# a changelog fallback or a "git deps moved" list covers).
last_tag() { release_tags --merged "${1:-HEAD}" | head -1; }

# This month as YY.M, UTC. RELEASE_MONTH=YY.M overrides it (the sandbox
# test; never set it for a real release, the guard would refuse it).
this_month() {
    if [ -n "${RELEASE_MONTH:-}" ]; then
        [[ "$RELEASE_MONTH" =~ ^[1-9][0-9]\.([1-9]|1[0-2])$ ]] || die "RELEASE_MONTH '$RELEASE_MONTH' is not YY.M"
        echo "$RELEASE_MONTH"; return 0
    fi
    echo "$(date -u +%y).$((10#$(date -u +%m)))"
}

# The CalVer next-version rule (see the header). Needs the tags fetched.
# With a line (a rev), the version it already carries is taken too: a
# release that merged and was never tagged is skipped, not reused.
next_version() { # [line rev]
    local tag v ym next cv
    tag="$(latest_tag)"; ym="$(this_month)"
    v="${tag#v}"
    if [ -n "$tag" ] && [ "${v%.*}" = "$ym" ]; then
        next="$ym.$(( ${v##*.} + 1 ))"
    else
        next="$ym.0"
    fi
    if [ -n "${1:-}" ]; then
        cv="$(version_at "$1")"
        if is_calver "$cv" && [ "${cv%.*}" = "$ym" ] && ! gt "$next" "$cv"; then
            next="$ym.$(( ${cv##*.} + 1 ))"
        fi
    fi
    echo "$next"
}

# ── the iOS app (Studio) ─────────────────────────────────────────────────
# Studio (Signal and Session in one app, apps/studio/desktop) is the one iOS
# app; Signal's and Session's own App Store records get no new builds. It is
# not released on its own: it is built from the one `v<version>` tag. Their crates follow the workspace version; the helpers below read
# that back so the guard and the TestFlight gate can prove it.
IOS_APPS="studio"

ios_check() {
    case "${1:-}" in studio) ;; *) die "unknown iOS app '${1:-}' (studio)" ;; esac
}
ios_manifest() { case "$1" in studio) echo apps/studio/desktop/Cargo.toml ;; esac; }
ios_title() { case "$1" in studio) echo Studio ;; esac; }
ios_bundle_id() { echo "app.fasttrackstudio.$1"; }

# The version an app crate resolves to at a revision (default: working
# tree): the workspace version when it says `version.workspace = true`, else
# its own literal `[package] version`. Empty when neither is there.
crate_version_at() { # <app> [rev]
    local f src pkg; f="$(ios_manifest "$1")"
    if [ -n "${2:-}" ]; then src="$(git show "$2:$f" 2>/dev/null || true)"; else src="$(cat "$f" 2>/dev/null || true)"; fi
    pkg="$(printf '%s\n' "$src" | sed -n '/^\[package\]/,/^\[/p')"
    if grep -Eq '^version(\.workspace *= *true| *= *\{ *workspace *= *true *\})' <<<"$pkg"; then
        version_at "${2:-}"
    else
        printf '%s\n' "$pkg" | sed -n 's/^version *= *"\([^"]*\)".*/\1/p' | head -1
    fi
}

# Git dependencies whose locked commit moved in a range — behaviour can
# arrive without a line of this repo's source changing (architect,
# keyflow, the vendor repo, …). Release notes carry the list.
git_deps_moved() { # <from> <to>
    [ -n "$1" ] || return 0
    git diff -U0 "$1" "$2" -- Cargo.lock \
        | grep -E '^\+source = "git\+' \
        | sed -E 's/^\+source = "git\+([^?#"]+).*/\1/' | sort -u || true
}

# `git rev-list --count --no-merges A..B`, tolerant of a missing A.
count_between() {
    if [ -n "${1:-}" ]; then git rev-list --count --no-merges "$1..$2"; else git rev-list --count --no-merges "$2"; fi 2>/dev/null || echo 0
}

# true when <rev>:CHANGELOG.md has a `## [<version>]` section. (A variable,
# not `git show | grep -q`: under pipefail grep -q exits at the first match,
# the writer dies of SIGPIPE, and that reads as "no match" on a big file.)
changelog_has() { # <rev> <version>
    local c; c="$(git show "$1:CHANGELOG.md" 2>/dev/null)" || return 1
    grep -qF "## [$2]" <<<"$(grep '^## \[' <<<"$c")"
}

# `git fetch` that FAILS loudly: a guard or a tag that cannot see origin
# must never read that as "nothing there".
fetch() { # <refspec>...
    git fetch --quiet --tags origin "$@" || die "cannot fetch from origin ($*): network or auth problem; refusing to guess"
}

# Every release line on origin: main and the hotfix branches release/v*.
fetch_lines() {
    fetch "+refs/heads/$MAIN_BRANCH:refs/remotes/origin/$MAIN_BRANCH" \
        "+refs/heads/release/v*:refs/remotes/origin/release/v*"
}

# The release line (main, or release/vX.Y.Z) whose history holds <sha>,
# main first. Empty when none does. Needs fetch_lines.
release_line_of() { # <sha>
    local b
    for b in "$MAIN_BRANCH" $(git for-each-ref --format='%(refname:lstrip=3)' 'refs/remotes/origin/release/v*'); do
        if git merge-base --is-ancestor "$1" "origin/$b" 2>/dev/null; then echo "$b"; return 0; fi
    done
    return 0
}

# Does origin have <branch>? 0 yes, 1 no; a network failure dies (ls-remote
# --exit-code says 2 for "no such ref", anything else is an error).
remote_has_branch() { # <branch>
    local rc=0
    git ls-remote --exit-code --heads origin "$1" >/dev/null 2>&1 || rc=$?
    case "$rc" in 0) return 0 ;; 2) return 1 ;; *) die "cannot reach origin to look for $1 (git ls-remote exit $rc)" ;; esac
}

# `CI passed` on <sha> must have run and be green: the latest run of the
# check (a re-run supersedes a failure). A network or API failure dies.
require_green() { # <sha>
    local out
    out="$(gh api -X GET "repos/$REPO/commits/$1/check-runs" -f check_name="$BUILD_CHECK" -f per_page=100 \
        --jq '.check_runs | sort_by(.started_at) | last | "\(.status) \(.conclusion)"')" \
        || die "cannot read the '$BUILD_CHECK' check of $1 (network or auth problem); refusing to guess"
    [ "$out" = "completed success" ] \
        || die "'$BUILD_CHECK' on $(git rev-parse --short "$1") is '${out:-missing}', not green: nothing ships from a red or unchecked commit (fix main, or wait for the run)"
}

# The release's images (ghcr.io/fasttrackstudios/studio-web) must be on
# GHCR, public, and built from <sha> before the tag exists: Argo CD deploys
# the newest tag within minutes, and pulls `:<version>` anonymously.
# images.yml pushes them when the release commit lands.
require_images() { # <sha> <version>
    local itag
    if ! has_chart "$1"; then
        note "no $CHART at $(git rev-parse --short "$1") (a line cut before studio-web): no image to wait for"
        return 0
    fi
    itag="$(bash scripts/ghcr-images.sh tag "$1")"
    [ "$itag" = "$2" ] || die "$CHART at $(git rev-parse --short "$1") has appVersion $itag, not $2: the chart would deploy the wrong image"
    bash scripts/ghcr-images.sh check "$itag" "$1" \
        || die "the images for v$2 are not on GHCR yet (or not public, or not from $(git rev-parse --short "$1")). Wait for the Images workflow (gh run list -w images.yml --commit $1), then re-run."
}

# Write <version> into the working tree's [workspace.package] version and
# the workspace members' entries in Cargo.lock: the release build's own
# checkout, never committed. Every crate with `version.workspace = true`
# (the app crates included) then builds as the release, so
# env!("CARGO_PKG_VERSION"), the iOS marketing version, the desktop app's
# version and the update check all agree with the tag. A member is a
# [[package]] with no `source` line whose version is the old workspace
# version; nothing else in the lockfile moves (no cargo re-lock, which
# would re-resolve registry picks). Idempotent.
cmd_stamp() { # <X.Y.Z | vX.Y.Z>
    local v="${1:-}" cur lock
    v="${v#v}"
    is_semver "$v" || die "usage: release.sh stamp X.Y.Z (got '${1:-}')"
    cur="$(version_at)"
    [ -n "$cur" ] || die "Cargo.toml has no [workspace.package] version"
    if [ "$cur" = "$v" ]; then
        note "stamp: the checkout is already $v"
        return 0
    fi
    sed "/^\[workspace\.package\]/,/^\[/s/^version *= *\"$cur\"/version = \"$v\"/" Cargo.toml > Cargo.toml.stamp \
        && mv Cargo.toml.stamp Cargo.toml
    [ "$(version_at)" = "$v" ] || die "stamp: failed to rewrite [workspace.package] version"
    lock="$(mktemp)"
    awk -v cur="$cur" -v nv="$v" '
        function flush(   i) {
            for (i = 1; i <= n; i++) {
                if (!src && buf[i] == "version = \"" cur "\"") buf[i] = "version = \"" nv "\""
                print buf[i]
            }
            n = 0; src = 0
        }
        /^\[\[package\]\]$/ || /^\[\[patch\.unused\]\]$/ || /^\[metadata\]$/ { flush() }
        { buf[++n] = $0; if ($0 ~ /^source = /) src = 1 }
        END { flush() }
    ' Cargo.lock > "$lock" && cat "$lock" > Cargo.lock
    rm -f "$lock"
    note "stamp: $cur -> $v in Cargo.toml and Cargo.lock (uncommitted: the committed version is not the released one)"
}

cmd_version() { version_at "$@"; } # [rev]
cmd_next() {
    git fetch --quiet --tags origin "+refs/heads/$MAIN_BRANCH:refs/remotes/origin/$MAIN_BRANCH" 2>/dev/null \
        || note "offline: next version from the local tags and origin/$MAIN_BRANCH"
    next_version "origin/$MAIN_BRANCH"
}

cmd_notes() { # [VERSION]
    local v="${1:-$(version_at)}" f=CHANGELOG.md
    [ -f "$f" ] || { note "no $f"; return 1; }
    awk -v want="## [$v]" '
        index($0, want) == 1 { on = 1; next }
        on && /^## \[/ { exit }
        on { print }
    ' "$f" | sed -e '/./,$!d' # drop leading blank lines
}

# ── change files (.changes/*.md, written by `cargo rail change add`) ──────
#
#   ---
#   "studio-desktop" = "minor"
#   "session-desktop" = "patch"
#   ---
#
#   Added offline drafts (signal)
#
#   Migration: …        (major only)
#
# A file's bump is the highest of its crates'. .changes/README is not a
# change file (and is not `.md`: rail rejects any .md without the front
# matter).

# The pending change files at <rev>, oldest first (the order their PRs
# merged; ties and strays by name).
change_files_at() { # <rev>
    local all
    all="$(git ls-tree --name-only "$1" "$CHANGES_DIR/" 2>/dev/null | grep -E '\.md$' || true)"
    [ -n "$all" ] || return 0
    { git log --diff-filter=A --reverse --format= --name-only "$1" -- "$CHANGES_DIR" 2>/dev/null || true
      printf '%s\n' "$all"; } \
        | awk 'NF && !seen[$0]++' | grep -Fx -f <(printf '%s\n' "$all") || true
}

# major | minor | patch | none, or "bad" for a file that is not one.
change_bump() { # <rev> <file>
    git show "$1:$2" | awk '
        NR == 1 { if ($0 != "---") { bad = 1; exit } next }
        $0 == "---" { closed = 1; exit }
        /^[[:space:]]*$/ { next }
        {
            v = $0; sub(/^[^=]*=[[:space:]]*"/, "", v); sub(/".*$/, "", v)
            r = (v == "major") ? 3 : (v == "minor") ? 2 : (v == "patch") ? 1 : (v == "none") ? 0 : -1
            if (r < 0) bad = 1
            if (r > max) max = r
        }
        END {
            if (bad || !closed) { print "bad"; exit }
            split("none patch minor major", name, " "); print name[max + 1]
        }'
}

# The note: everything after the front matter, as one list item (later
# lines and paragraphs indented under it, so a Migration: paragraph stays
# with its entry).
change_entry() { # <rev> <file>
    git show "$1:$2" \
        | awk 'c >= 2 { print; next } $0 == "---" { c++ }' \
        | sed -e '/./,$!d' \
        | awk '{ l[NR] = $0 } END { n = NR; while (n > 0 && l[n] ~ /^[[:space:]]*$/) n--
                 for (i = 1; i <= n; i++) {
                   if (i == 1) print "- " l[i]; else if (l[i] == "") print ""; else print "  " l[i] } }'
}

# Write <version>'s section from the change files at <rev> to stdout.
# Returns 1 when <rev> has no change files (the caller falls back).
changes_section() { # <rev> <version> [includes-line]
    local rev="$1" v="$2" incl="${3:-}" files f b group title any=0 out
    files="$(change_files_at "$rev")"
    [ -n "$files" ] || return 1
    while read -r f; do
        b="$(change_bump "$rev" "$f")"
        [ "$b" != bad ] || die "$f is not a change file (front matter: \"crate\" = \"none|patch|minor|major\"): fix it or delete it (cargo rail change add)"
    done <<<"$files"
    echo "## [$v] - $(date -u +%F)"
    echo
    if [ -n "$incl" ]; then echo "$incl"; echo; fi
    for group in major minor patch; do
        case "$group" in
            major) title="Breaking / Migration" ;;
            minor) title="Added" ;;
            patch) title="Fixed and improved" ;;
        esac
        out=""
        while read -r f; do
            [ "$(change_bump "$rev" "$f")" = "$group" ] || continue
            out+="$(change_entry "$rev" "$f")"$'\n'
        done <<<"$files"
        [ -n "$out" ] || continue
        any=1
        echo "### $title"
        echo
        printf '%s\n' "$out"
    done
    if [ "$any" = 0 ]; then
        echo "No user-visible changes (internal changes only)."
        echo
    fi
}

# Put <section file> into CHANGELOG.md as <version>'s section (replacing
# it if present), above every older section, which stay as released.
changelog_write() { # <version> <section file>
    local v="$1" sec="$2" file=CHANGELOG.md
    if [ ! -f "$file" ]; then
        printf '# Changelog\n\nAll notable changes to this project are documented in this file.\n\n' > "$file"
    fi
    awk -v want="## [$v]" -v sec="$sec" '
        function emit(   n, i, l, buf) {
            n = 0; while ((getline l < sec) > 0) buf[++n] = l
            while (n > 0 && buf[n] == "") n--          # no trailing blanks
            for (i = 1; i <= n; i++) print buf[i]
            print ""; done = 1
        }
        index($0, want) == 1 { skip = 1; if (!done) emit(); next }
        skip && /^## \[/ { skip = 0 }
        skip { next }
        /^## \[/ && !done { emit() }
        { print }
        END { if (!done) emit() }
    ' "$file" > "$sec.new"
    mv "$sec.new" "$file"
    grep -q "^## \[$v\]" "$file" || die "$file has no [$v] section"
}

# Hotfix releases tagged on a release/v* line after <rev>'s newest tag:
# their fixes reached main first, so main's next section says it covers them.
includes_line() { # <rev>
    local mine hot
    mine="$(last_tag "$1")"
    hot="$(release_tags --no-merged "$1" | head -1)"
    [ -n "$hot" ] || return 0
    if [ -z "$mine" ] || gt "${hot#v}" "${mine#v}"; then
        echo "Includes all changes up to $hot."
    fi
}

cmd_plan() {
    git fetch --quiet --tags origin "+refs/heads/$MAIN_BRANCH:refs/remotes/origin/$MAIN_BRANCH" 2>/dev/null \
        || note "offline: planning from the local origin/$MAIN_BRANCH and tags"
    local ref="origin/$MAIN_BRANCH" cur tag mtag next a cv files sec
    cur="$(version_at "$ref")"; tag="$(latest_tag)"; mtag="$(last_tag "$ref")"
    next="$(next_version "$ref")"
    echo "production (latest v* tag):  ${tag:-none}"
    echo "version on $MAIN_BRANCH:              $cur"
    for a in $IOS_APPS; do
        cv="$(crate_version_at "$a" "$ref")"
        echo "$a crate version:    ${cv:-?}$([ "$cv" = "$cur" ] || echo '   <-- differs from the workspace version')"
    done
    echo "on $MAIN_BRANCH since ${mtag:-the start}:     $(count_between "$mtag" "$ref") commits"
    files="$(change_files_at "$ref")"
    echo "pending change files:        $(grep -c . <<<"$files" || true)"
    echo "next release:                $next (CalVer YY.M.MICRO from the highest tag and this month)"
    echo "                             one tag v$next and one GitHub Release; the Studio TestFlight upload comes from it"
    echo
    echo "── draft changelog ($ref) ──"
    sec="$(mktemp)"
    if changes_section "$ref" "$next" "$(includes_line "$ref")" > "$sec"; then
        cat "$sec"
    else
        echo "(no change files: the release would fall back to git-cliff over ${mtag:+$mtag..}$MAIN_BRANCH)"
        cliff --strip header --tag "v$next" ${mtag:+"$mtag..$ref"} || true
    fi
    rm -f "$sec"
    if [ -n "$mtag" ]; then
        local d; d="$(git_deps_moved "$mtag" "$ref")"
        echo "── git dependencies moved since $mtag ──"
        echo "${d:-none}"
    fi
}

# Cut release/pending from origin/main (or, with --hotfix, from
# origin/release/v<latest tag>): ONE commit, `chore(release): v<next>`.
cmd_prepare() {
    local hotfix=0 skip=0 want="" line base cur next tag mtag skipped=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --hotfix) hotfix=1 ;;
            --skip-untagged) skip=1 ;;
            -*) die "unknown option '$1' (prepare [--hotfix] [--skip-untagged] [X.Y.Z])" ;;
            *) [ -z "$want" ] || die "one version at most"; want="$1" ;;
        esac
        shift
    done
    [ -z "$(git status --porcelain --untracked-files=no)" ] || die "working tree is dirty"
    fetch "+refs/heads/$MAIN_BRANCH:refs/remotes/origin/$MAIN_BRANCH"
    tag="$(latest_tag)"
    if [ "$hotfix" = 1 ]; then
        [ -n "$tag" ] || die "no release tag yet: nothing to hotfix"
        line="release/$tag"
        remote_has_branch "$line" || die "origin has no $line: just release-hotfix <commit> creates it with the fix"
        fetch "+refs/heads/$line:refs/remotes/origin/$line"
    else
        line="$MAIN_BRANCH"
    fi
    base="origin/$line"
    cur="$(version_at "$base")"
    [ -n "$cur" ] || die "$base has no [workspace.package] version"
    # A release commit already on the line but never tagged: finish that one,
    # or (--skip-untagged) leave it unshipped and release past it, its
    # changes and everything since in the next version.
    if ! git rev-parse -q --verify "refs/tags/v$cur" >/dev/null && changelog_has "$base" "$cur"; then
        [ "$skip" = 1 ] \
            || die "$base is at $cur, which has a changelog section but no tag v$cur: an earlier release merged and was not tagged. Finish it (just release-tag $cur), or release past it (just release-prepare --skip-untagged)"
        skipped="$cur"
        note "skipping $cur: merged, never tagged or shipped; its changes ship in the next version"
    fi
    remote_has_branch "$RELEASE_BRANCH" \
        && die "$RELEASE_BRANCH is already on origin: a release is in flight. Merge it (just release-merge, then just release-tag), or close its PR and delete the branch to re-cut"
    mtag="$(last_tag "$base")"
    if [ -n "$mtag" ] && [ "$(count_between "$mtag" "$base")" -eq 0 ]; then
        die "nothing to release: $line has no commits since $mtag"
    fi

    next="$(next_version "$base")"
    if [ -n "$want" ]; then
        want="${want#v}"
        is_calver "$want" || die "'$want' is not a CalVer version YY.M.MICRO (no leading zeros)"
        [ "$want" = "$next" ] || die "the next version is $next (highest tag ${tag:-none}, month $(this_month)), not $want: the guard would refuse it"
    fi
    if [ -n "$tag" ]; then gt "$next" "${tag#v}" || die "next version $next is not above the highest tag $tag (clock?)"; fi
    gt "$next" "$cur" || die "next version $next is not above $line's $cur"
    git rev-parse -q --verify "refs/tags/v$next" >/dev/null && die "tag v$next already exists"

    git switch --quiet -C "$RELEASE_BRANCH" "$base"
    note "cutting v$next (was $cur) on $RELEASE_BRANCH from $base $(git rev-parse --short "$base")"
    release_commit "$cur" "$next" "$skipped"
    cat >&2 <<EOT

Next:
  1. Read CHANGELOG.md's [$next] section; tidy the wording in this commit if
     needed (git commit --amend: it must stay ONE commit).
  2. just release-pr     pushes $RELEASE_BRANCH and opens its PR into $line.
  3. just release-merge  re-checks it against the CURRENT $line, then squash-merges.
     If $line moved first, the guard refuses: close the PR, delete the
     branch and cut again (it takes a minute).
  4. just release-tag    tags v$next on the merge commit once its '$BUILD_CHECK'
     is green. THE TAG SHIPS: both sites, the Studio TestFlight upload
     (marketing version $next), the Release.
EOT
}

# Bump the version (Cargo.toml: the one line both iOS crates follow),
# Cargo.lock, write the changelog section, delete the consumed change
# files, commit `chore(release): v<next>` on the current branch.
release_commit() { # <cur> <next> [skipped version]
    local cur="$1" next="$2" skipped="${3:-}" sec files from f lock
    sed -i.bak "/^\[workspace\.package\]/,/^\[/s/^version *= *\"$cur\"/version = \"$next\"/" Cargo.toml
    rm -f Cargo.toml.bak
    [ "$(version_at)" = "$next" ] || die "failed to rewrite [workspace.package] version"

    # Workspace members' entries in the lockfile follow; nothing else may
    # move. Rewritten in place, not by cargo: any re-lock (`cargo update
    # --workspace`, even `cargo metadata` after the bump) re-resolves the
    # whole graph and moves registry picks (windows-sys, socket2: seen on
    # the 26.10.1 cut), which the guard rightly refuses. A member is a
    # [[package]] with no `source` line.
    lock="$(mktemp)"
    awk -v cur="$cur" -v nv="$next" '
        function flush(   i) {
            for (i = 1; i <= n; i++) {
                if (!src && buf[i] == "version = \"" cur "\"") buf[i] = "version = \"" nv "\""
                print buf[i]
            }
            n = 0; src = 0
        }
        /^\[\[package\]\]$/ || /^\[\[patch\.unused\]\]$/ || /^\[metadata\]$/ { flush() }
        { buf[++n] = $0; if ($0 ~ /^source = /) src = 1 }
        END { flush() }
    ' Cargo.lock > "$lock" && cat "$lock" > Cargo.lock
    rm -f "$lock"
    cargo metadata --format-version 1 --offline >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null \
        || die "cargo cannot read the lockfile after the version bump"

    # The chart's version and appVersion: the image tag Argo pulls at the
    # tag. A line cut before the chart existed has none to bump.
    if [ -f "$CHART" ]; then
    sed -E "s/^version: *\"?$cur\"?[[:space:]]*$/version: $next/; s/^appVersion: *\"?$cur\"?[[:space:]]*$/appVersion: \"$next\"/" "$CHART" > "$CHART.new" \
        && mv "$CHART.new" "$CHART"
    { grep -qx "appVersion: \"$next\"" "$CHART" && grep -qx "version: $next" "$CHART"; } \
        || die "failed to rewrite $CHART version/appVersion ($cur -> $next): both must be the workspace version"
    else
        note "no $CHART on this line (cut before studio-web): no chart to bump"
    fi

    sec="$(mktemp)"
    files="$(change_files_at HEAD)"
    if changes_section HEAD "$next" "$(includes_line HEAD)" > "$sec"; then
        while read -r f; do git rm --quiet "$f"; done <<<"$files"
        note "changelog from $(grep -c . <<<"$files") change file(s), now deleted"
    else
        # Before v4 there were no change files: the section is the commit
        # range since the last tag, as git-cliff sees it.
        from="$(last_tag HEAD)"
        note "no change files: falling back to git-cliff over ${from:+$from..}HEAD"
        cliff --tag "v$next" --strip all ${from:+"$from..HEAD"} > "$sec"
        grep -q "^## \[$next\]" "$sec" \
            || die "git-cliff produced no [$next] section: nothing to release since ${from:-the first commit}?"
    fi
    if [ -n "$skipped" ]; then
        # The section's first line, then the note: only lines are added.
        awk -v v="$skipped" 'NR == 1 { print; print ""; print "_" v " merged but was never tagged or shipped: its changes ship in this release._"; next } { print }' \
            "$sec" > "$sec.n" && mv "$sec.n" "$sec"
    fi
    changelog_write "$next" "$sec"
    rm -f "$sec"

    git add Cargo.toml Cargo.lock CHANGELOG.md
    if [ -f "$CHART" ]; then git add "$CHART"; fi
    git commit --quiet -m "chore(release): v$next"
    # The guard's own lockfile rule, here, so a bad cut fails at once.
    local why
    why="$(lock_members_only HEAD~1 HEAD "$cur" "$next")" \
        || die "the release commit moved more than member versions in Cargo.lock ($why): git reset --hard HEAD~1"
    note "committed: $(git log -1 --format='%h %s')"
}

# A hotfix: fixes already merged into main, cherry-picked (-x) onto
# release/v<latest tag>, as a PR into that branch (the full suite runs on
# it). Then: just release-prepare --hotfix, release-pr, release-merge,
# release-tag; the tag opens the forward-port PR into main.
cmd_hotfix() {
    local tag line branch c first n title
    [ $# -gt 0 ] || die "usage: release.sh hotfix <commit on main>...   (fix it on main first, then pick it)"
    [ -z "$(git status --porcelain --untracked-files=no)" ] || die "working tree is dirty"
    fetch "+refs/heads/$MAIN_BRANCH:refs/remotes/origin/$MAIN_BRANCH"
    tag="$(latest_tag)"
    [ -n "$tag" ] || die "no release tag yet: nothing to hotfix"
    line="release/$tag"
    if ! remote_has_branch "$line"; then
        git push --quiet origin "$(git rev-parse "$tag^{commit}"):refs/heads/$line"
        note "created $line at $tag"
    fi
    fetch "+refs/heads/$line:refs/remotes/origin/$line"
    for c in "$@"; do
        git rev-parse -q --verify "$c^{commit}" >/dev/null || die "'$c' is not a commit"
        git merge-base --is-ancestor "$c" "origin/$MAIN_BRANCH" \
            || die "$c is not on origin/$MAIN_BRANCH: a fix lands on main first (with its change file), then is picked"
    done
    first="$(git rev-parse --short "$1^{commit}")"
    branch="fix/no-issue-hotfix-$first"
    git switch --quiet -C "$branch" "origin/$line"
    git config "branch.$branch.releaseBase" "$line"
    for c in "$@"; do
        git cherry-pick -x "$c" >/dev/null \
            || die "cherry-pick of $c conflicts: resolve it, git cherry-pick --continue, pick the rest, then just release-pr"
    done
    n=$#
    if [ "$n" = 1 ]; then
        title="$(git log -1 --format=%s "$1" | sed -E 's/ \(#[0-9]+\)$//')"
    else
        title="fix: hotfix $n fixes onto $tag"
    fi
    git config "branch.$branch.prTitle" "$title"
    note "$branch: $n fix(es) on $line"
    cmd_pr
}

# The PR body's template sections (conventions.md).
pr_template_tail() { # <how tested> <risk> <change files>
    printf '## How tested\n%s\n\n## Risk and rollback\n%s\n\n## Change files\n%s\n\n## AI disclosure\nTool: none (scripts/release.sh). Extent: generated. Human reviewed: no.\n' "$1" "$2" "$3"
}

# Push the current release/pending (or hotfix) branch and open its PR.
cmd_pr() {
    local ref v body base existing title line tag consumed
    ref="$(git rev-parse --abbrev-ref HEAD)"
    [ -z "$(git status --porcelain --untracked-files=no)" ] || die "working tree is dirty"
    fetch_lines
    case "$ref" in
        "$RELEASE_BRANCH")
            v="$(version_at)"; tag="v$v"
            [ "$(git log -1 --format=%s)" = "chore(release): $tag" ] \
                || die "the head is not the release commit for $tag (just release-prepare)"
            # The line it was cut from: the parent is that line's head.
            base=""
            for line in "$MAIN_BRANCH" $(git for-each-ref --format='%(refname:lstrip=3)' 'refs/remotes/origin/release/v*'); do
                [ "$(git rev-parse HEAD^)" = "$(git rev-parse "origin/$line")" ] && { base="$line"; break; }
            done
            [ -n "$base" ] || die "$RELEASE_BRANCH is not one commit on top of origin/main or an origin/release/v* head: the line moved since the cut. Cut again (just release-prepare)"
            title="chore(release): $tag" ;;
        fix/no-issue-hotfix-*)
            base="$(git config "branch.$ref.releaseBase" || true)"
            [ -n "$base" ] || die "$ref has no recorded release line (just release-hotfix makes it)"
            title="$(git config "branch.$ref.prTitle" || echo "fix: hotfix onto ${base#release/}")" ;;
        *) die "run this on $RELEASE_BRANCH or a hotfix branch (on '$ref'); just release-prepare / release-hotfix makes one" ;;
    esac
    git push --quiet -u origin "HEAD:refs/heads/$ref"
    existing="$(gh pr list --repo "$REPO" --head "$ref" --base "$base" --state open --json url --jq '.[0].url' 2>/dev/null || true)"
    if [ -n "$existing" ]; then
        note "pushed; the PR is already open: $existing"; return 0
    fi
    body="$(mktemp)"
    if [ "$ref" = "$RELEASE_BRANCH" ]; then
        # shellcheck disable=SC2016 # the backticks are markdown, not expansions
        consumed="$(git diff --diff-filter=D --name-only HEAD^ HEAD -- "$CHANGES_DIR" | sed 's/^/- `/; s/$/` (consumed)/')"
        {
            echo "## Summary"
            echo "Release **$tag** into \`$base\`: ONE commit, the CalVer version bump (\`Cargo.toml\`, the workspace members in \`Cargo.lock\`) and the \`CHANGELOG.md\` section built from the pending change files, which it deletes. Every app shares the version."
            echo
            echo "Merge with \`just release-merge\` (re-runs the guard against the current \`$base\`, then squash-merges). Merging ships nothing. **The tag does:** \`just release-tag\` tags \`$tag\` on the merge commit once its \`$BUILD_CHECK\` is green → both sites deploy, the Studio TestFlight upload (\`studio-ios\`, marketing version **$v**), the one GitHub Release."
            local d; d="$(git_deps_moved "$(last_tag HEAD^)" HEAD)"
            if [ -n "$d" ]; then
                echo
                echo "### Git dependencies moved"
                # shellcheck disable=SC2086 # $d is a word list
                printf -- '- %s\n' $d
            fi
            echo
            echo "### Changelog"
            cmd_notes "$v"
            echo
            echo "## Issue"
            echo "Refs: none (release)"
            echo
            pr_template_tail "The release guard (release.yml, and checks.yml's plan) proves this is one commit touching only the version, Cargo.lock member versions, CHANGELOG.md and consumed change files." \
                "Ships nothing until tagged. Before the tag: close the PR. After: release the next version (tags are never moved)." \
                "${consumed:-none (git-cliff fallback)}"
        } > "$body"
    else
        {
            echo "## Summary"
            echo "Hotfix onto \`$base\` (production line). Cherry-picked with \`-x\` from \`$MAIN_BRANCH\`, where each fix landed first:"
            echo
            git log --format='- %s' "origin/$base..HEAD"
            echo
            echo "The full suite runs on this PR. After the merge: \`just release-prepare --hotfix\`, \`just release-pr\`, \`just release-merge\`, \`just release-tag\` (which opens the forward-port PR into \`$MAIN_BRANCH\`)."
            echo
            echo "## Issue"
            echo "Refs: see the picked commits"
            echo
            pr_template_tail "The picked commits passed checks on $MAIN_BRANCH; the full suite runs here." \
                "Revert the merge on \`$base\` before it is tagged; after, release the next version." \
                "the picked commits' own change files"
        } > "$body"
    fi
    gh pr create --repo "$REPO" --base "$base" --head "$ref" --title "$title" --body-file "$body"
    rm -f "$body"
}

# Squash-merge the open release PR, after re-running the guard against the
# CURRENT head of its base: a release PR that merges after a newer change
# on its line would ship that change without a changelog line.
cmd_merge() {
    local info num head base
    info="$(gh pr list --repo "$REPO" --head "$RELEASE_BRANCH" --state open --json number,headRefOid,baseRefName \
        --jq '.[0] | "\(.number) \(.headRefOid) \(.baseRefName)"')" || die "cannot list PRs (network?)"
    [ -n "$info" ] && [ "$info" != "null null null" ] || die "no open $RELEASE_BRANCH PR (just release-pr)"
    read -r num head base <<<"$info"
    fetch "+refs/heads/$RELEASE_BRANCH:refs/remotes/origin/$RELEASE_BRANCH"
    BASE_REF="$base" HEAD_REF="$RELEASE_BRANCH" HEAD_SHA="$head" bash "$SELF" guard \
        || die "the guard refuses PR #$num against the current $base: close it, delete $RELEASE_BRANCH and cut again"
    gh pr merge "$num" --repo "$REPO" --squash --match-head-commit "$head"
    note "merged #$num into $base. Next: just release-tag (once '$BUILD_CHECK' on the merge commit is green)"
}

# Is the diff base..head ONLY a release commit's: Cargo.toml's one version
# line, the workspace members' version lines in Cargo.lock, the chart's
# version and appVersion lines, an added CHANGELOG.md section and deleted
# change files? (A version bump cannot change what the checks tested.)
release_diff_only() { # <base> <head>
    # No `... | grep -q` pipelines: under pipefail, grep -q exits at the first
    # match and the writer dies of SIGPIPE, which reads as "no match" on a
    # big diff. Capture into a variable, then test it.
    local base="$1" head="$2" st f out vb vh
    vb="$(version_at "$base")"; vh="$(version_at "$head")"
    while read -r st f; do
        case "$f" in
            Cargo.toml|Cargo.lock|CHANGELOG.md|"$CHART") [ "$st" = M ] || { echo "$f: status $st"; return 1; } ;;
            "$CHANGES_DIR"/*.md) [ "$st" = D ] || { echo "$f: a change file may only be deleted (consumed)"; return 1; } ;;
            *) echo "$f: not a release file"; return 1 ;;
        esac
    done < <(git diff --no-renames --name-status "$base" "$head")
    # Cargo.toml: exactly the [workspace.package] version line changes.
    out="$(git diff -U0 "$base" "$head" -- Cargo.toml | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' || true)"
    if [ -n "$out" ]; then
        [ "$vb" != "$vh" ] || { echo "Cargo.toml changed but the workspace version did not"; return 1; }
        [ "$out" = "$(printf -- '-version = "%s"\n+version = "%s"' "$vb" "$vh")" ] \
            || { echo "Cargo.toml: more than the [workspace.package] version line changed"; return 1; }
    fi
    lock_members_only "$base" "$head" "$vb" "$vh" || return 1
    # The chart (where the line has one): exactly its version and
    # appVersion lines, each from the old version to the new, nothing else.
    if has_chart "$base" || has_chart "$head"; then
        if ! has_chart "$base" || ! has_chart "$head"; then echo "$CHART: added or removed"; return 1; fi
        local want got
        want="$(printf '%s\n' "-appVersion: \"$vb\"" "-version: $vb" "+appVersion: \"$vh\"" "+version: $vh" | sort)"
        got="$(git diff -U0 "$base" "$head" -- "$CHART" | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' \
            | sed -E 's/^-(version|appVersion): *"?([^"[:space:]]*)"?[[:space:]]*$/-\1: \2/' \
            | sed -E 's/^-appVersion: (.*)$/-appVersion: "\1"/' | sort || true)"
        [ "$got" = "$want" ] || { echo "$CHART: the release changes exactly its version and appVersion lines, $vb -> $vh"; return 1; }
    fi
    # CHANGELOG.md: lines are only added (the new section), never removed.
    out="$(git diff -U0 "$base" "$head" -- CHANGELOG.md | grep -E '^-' | grep -vE '^---' || true)"
    [ -z "$out" ] || { echo "CHANGELOG.md: released sections changed (only the new section may be added)"; return 1; }
    return 0
}

# Cargo.lock: the only lines that move are `version = "<old>"` →
# `version = "<new>"` of workspace members (path packages: no `source`).
# cargo writes the [[patch.unused]] trailer in HashMap order, so it is
# compared as a set, not line by line.
lock_members_only() { # <base> <head> <old> <new>
    local b h d bad added
    b="$(mktemp)"; h="$(mktemp)"
    git show "$1:Cargo.lock" > "$b" 2>/dev/null || true
    git show "$2:Cargo.lock" > "$h" 2>/dev/null || true
    unused() { awk '/^\[\[patch\.unused\]\]/ { f = 1; next } f && /^name = / { n = $3 } f && /^version = / { print n "@" $3 }' "$1" | sort; }
    if [ "$(unused "$b")" != "$(unused "$h")" ]; then rm -f "$b" "$h"; echo "Cargo.lock: [[patch.unused]] changed"; return 1; fi
    sed -i.bak '/^\[\[patch\.unused\]\]/,$d' "$b" "$h"; rm -f "$b.bak" "$h.bak"
    d="$(git diff --no-index -U0 "$b" "$h" || true)"
    bad="$(printf '%s\n' "$d" | grep -E '^[+-]' | grep -vE '^(\+\+\+|---) ' \
        | grep -vxF -e "-version = \"$3\"" -e "+version = \"$4\"" || true)"
    if [ -n "$bad" ]; then rm -f "$b" "$h"; echo "Cargo.lock: lines other than member version lines changed:"; printf '%s\n' "$bad" | head -5; return 1; fi
    [ "$(grep -c '^-version' <<<"$d" || true)" = "$(grep -c '^+version' <<<"$d" || true)" ] \
        || { rm -f "$b" "$h"; echo "Cargo.lock: version lines added or removed"; return 1; }
    # The new-side line of every hunk must sit in a [[package]] with no
    # `source` (a workspace member, not a registry or git crate).
    added="$(printf '%s\n' "$d" | sed -nE 's/^@@ -[0-9,]+ \+([0-9]+)(,([0-9]+))? @@.*/\1 \3/p' | paste -sd';' -)"
    if [ -n "$added" ]; then
        bad="$(awk -v want="$added" '
            BEGIN { n = split(want, w, ";"); for (i = 1; i <= n; i++) { split(w[i], p, " "); c = (p[2] == "") ? 1 : p[2]; for (j = 0; j < c; j++) line[p[1] + j] = 1 } }
            function close_block() { if (hit && src) print name; hit = 0; src = 0; name = "" }
            /^\[\[package\]\]/ { close_block(); next }
            /^name = / { name = $3 }
            /^source = / { src = 1 }
            (NR in line) { hit = 1 }
            END { close_block() }' "$h")"
        if [ -n "$bad" ]; then rm -f "$b" "$h"; echo "Cargo.lock: non-member packages changed version: $bad"; return 1; fi
    fi
    rm -f "$b" "$h"
    return 0
}

# CI, on every PR into main or release/** (release.yml), run from the
# BASE commit's copy of this script, never the PR's. Only a release PR
# (head release/pending, in this repo) is checked; any other PR exits 0.
cmd_guard() {
    local head="${HEAD_SHA:?}" ref="${HEAD_REF:?}" base_ref="${BASE_REF:?}" head_repo="${HEAD_REPO:-$REPO}"
    local v vb cur next subject ok=1 tag a cv pending why
    if [ "$ref" != "$RELEASE_BRANCH" ] || [ "$head_repo" != "$REPO" ]; then
        echo "release guard: not a release PR ($head_repo:$ref → $base_ref): nothing to check"
        return 0
    fi
    fail() { echo "::error::$*"; ok=0; }

    case "$base_ref" in
        "$MAIN_BRANCH"|release/v*) ;;
        *) fail "a release PR goes into $MAIN_BRANCH or a release/v* hotfix line, not '$base_ref'" ;;
    esac
    # The CURRENT head of the base, fetched now (not the event's base.sha):
    # a release PR cut before another merge would ship that merge unlisted.
    fetch "+refs/heads/$base_ref:refs/remotes/origin/$base_ref"
    cur="$(git rev-parse "origin/$base_ref")"
    git cat-file -e "$head^{commit}" 2>/dev/null || die "head $head is not in this checkout (fetch the PR head first)"

    v="$(version_at "$head")"; vb="$(version_at "$cur")"; tag="v$v"
    next="$(next_version "$cur")"
    subject="$(git log -1 --format=%s "$head")"

    [ "$(git rev-list --parents -n 1 "$head" | awk '{ print NF }')" = 2 ] || fail "the head must be one ordinary commit (not a merge)"
    [ "$(git rev-parse "$head^" 2>/dev/null)" = "$cur" ] \
        || fail "the release commit is not on top of the CURRENT $base_ref ($(git rev-parse --short "$cur")): $base_ref moved since the cut, or it holds more than one commit. Close the PR, delete $RELEASE_BRANCH and cut again (just release-prepare)"
    [ "$subject" = "chore(release): $tag" ] || fail "the head commit must be 'chore(release): $tag' (it is '$subject')"
    is_calver "$v" || fail "version '$v' is not CalVer YY.M.MICRO"
    [ "$v" = "$next" ] || fail "version $v is not the next version $next (highest tag $(latest_tag), month $(this_month))"
    git rev-parse -q --verify "refs/tags/$tag" >/dev/null && fail "tag $tag already exists"
    changelog_has "$head" "$v" || fail "CHANGELOG.md at the head has no [$v] section"
    why="$(release_diff_only "$cur" "$head")" \
        || fail "the release commit may change only the version line (Cargo.toml, Cargo.lock members), the chart's version and appVersion ($CHART), add the CHANGELOG.md section and delete consumed change files: $why"
    if has_chart "$head"; then
        a="$(chart_field "$head" appVersion)"
        [ "$a" = "$v" ] || fail "$CHART at the head has appVersion '${a:-none}', not $v: the chart at the tag would deploy the wrong image"
        a="$(chart_field "$head" version)"
        [ "$a" = "$v" ] || fail "$CHART at the head has version '${a:-none}', not $v: the chart's version follows the release"
    fi
    pending="$(change_files_at "$head" | paste -sd' ' -)"
    [ -z "$pending" ] || fail "change files left unconsumed at the head: $pending"
    # One version for the whole studio: the app crate follows the
    # workspace version (version.workspace = true), so they cannot differ.
    for a in $IOS_APPS; do
        cv="$(crate_version_at "$a" "$head")"
        [ "$cv" = "$v" ] \
            || fail "$(ios_manifest "$a") resolves to version '${cv:-none}' but the release is $v: the app takes [workspace.package] version (version.workspace = true)"
    done

    {
        echo "### Release guard: $RELEASE_BRANCH → $base_ref"
        echo "- version: $vb → $v (next by the CalVer rule: $next; every app shares it)"
        echo "- head: \`$(git rev-parse --short "$head")\` $subject; base now \`$(git rev-parse --short "$cur")\`"
        local d; d="$(git_deps_moved "$(last_tag "$cur")" "$head" | paste -sd, -)"
        echo "- git deps moved since last release: ${d:-none}"
        echo "- merging ships nothing; production changes only when the tag is pushed (sites, studio-ios TestFlight, the Release)"
    } >> "${GITHUB_STEP_SUMMARY:-/dev/stderr}"
    [ "$ok" = 1 ] || exit 1
    echo "release guard: ok ($tag → $base_ref)"
}

# CI (checks.yml plan, from the base commit's copy): 0 = a sound release
# PR (its content was checked on its line; skip the heavy jobs), 1 = not a
# release PR (plan as usual), 3 = a release PR the guard refuses (fail the
# required check). The guard runs in its own process so its `set -e` holds.
cmd_release_only() {
    : "${HEAD_SHA:?}" "${BASE_REF:?}"
    local ref="${HEAD_REF:?}" head_repo="${HEAD_REPO:-$REPO}"
    [ "$ref" = "$RELEASE_BRANCH" ] && [ "$head_repo" = "$REPO" ] || return 1
    if bash "$SELF" guard >&2; then return 0; fi
    exit 3
}

# Find the merge commit of <version>'s release PR on any release line:
# the first-parent commit whose subject is the release subject and which
# carries that version and its changelog section.
release_commit_of() { # <version>
    local v="$1" line c re
    re="$(release_subject_re "$v")"
    for line in "$MAIN_BRANCH" $(git for-each-ref --format='%(refname:lstrip=3)' 'refs/remotes/origin/release/v*'); do
        while read -r c; do
            [[ "$(git log -1 --format=%s "$c")" =~ $re ]] || continue
            [ "$(version_at "$c")" = "$v" ] && changelog_has "$c" "$v" && { echo "$c $line"; return 0; }
        done < <(git rev-list --first-parent -n 400 "origin/$line")
    done
    return 0
}

# After the release PR merged: tag v<version> on its merge commit, once
# that commit's `CI passed` is green. The TAG is the release: it starts
# the deploy, the TestFlight upload and the GitHub Release. Pushing as a GitHub App (fts-forge) or a person starts the
# workflows; a tag pushed with a workflow's GITHUB_TOKEN would not.
cmd_tag() {
    local v="${1:-}" target line found prev pending l
    fetch_lines
    if [ -z "$v" ]; then
        # The one untagged release on any line.
        for l in "$MAIN_BRANCH" $(git for-each-ref --format='%(refname:lstrip=3)' 'refs/remotes/origin/release/v*'); do
            local lv; lv="$(version_at "origin/$l")"
            git rev-parse -q --verify "refs/tags/v$lv" >/dev/null && continue
            changelog_has "origin/$l" "$lv" || continue
            [ -z "$v" ] || [ "$v" = "$lv" ] || die "untagged releases on several lines ($v, $lv): say which (just release-tag X.Y.Z)"
            v="$lv"
        done
        [ -n "$v" ] || die "no merged, untagged release on $MAIN_BRANCH or release/v*: has the release PR merged?"
    fi
    v="${v#v}"
    is_semver "$v" || die "version '$v' is not X.Y.Z"
    if git rev-parse -q --verify "refs/tags/v$v" >/dev/null; then
        die "tag v$v already exists ($(git rev-parse --short "v$v^{commit}")). Never move a tag; a bad release is superseded by the next."
    fi
    found="$(release_commit_of "$v")"
    [ -n "$found" ] || die "no 'chore(release): v$v' merge commit carrying version $v on $MAIN_BRANCH or release/v*: has the release PR merged?"
    read -r target line <<<"$found"
    # A change file at the release commit means the line moved under the
    # release PR: that change ships in this tag with no changelog line.
    pending="$(change_files_at "$target" | paste -sd' ' -)"
    [ -z "$pending" ] || [ "${RELEASE_ALLOW_UNLISTED:-}" = 1 ] \
        || die "$line moved under the release PR: $pending merged before it and would ship in v$v unlisted. Add their notes to the [$v] section in a follow-up and re-run with RELEASE_ALLOW_UNLISTED=1, or release the next version"
    prev="$(last_tag "$target^")"
    if [ -n "$prev" ]; then gt "$v" "${prev#v}" || die "v$v is not greater than the previous release $prev"; fi
    require_green "$target"
    require_images "$target" "$v"
    note "tagging v$v at $(git rev-parse --short "$target") on $line (previous: ${prev:-none}). This ships: sites, Studio TestFlight, the Release."
    # An annotated tag needs a committer identity; a bot checkout may have none.
    local ident=()
    git config user.email >/dev/null 2>&1 || [ -n "${GIT_COMMITTER_EMAIL:-}" ] \
        || ident=(-c "user.name=fts-forge[bot]" -c "user.email=fts-forge[bot]@users.noreply.github.com")
    git ${ident[@]+"${ident[@]}"} tag -a "v$v" "$target" -m "studio $v"
    git push --quiet origin "refs/tags/v$v"
    note "pushed v$v: deploy, studio-ios and the Release start from the tag"
    if [ "$line" != "$MAIN_BRANCH" ]; then cmd_forward_port "$v"; fi
}

# After a hotfix release on release/v*: a PR into main that inserts its
# changelog section and deletes the change files it consumed (they were
# cherry-picked from main, where they are still pending), so main's next
# release does not list them twice.
cmd_forward_port() { # <version>
    local v="${1:-}" sec consumed branch here f
    v="${v#v}"
    is_semver "$v" || die "usage: release.sh forward-port X.Y.Z"
    git rev-parse -q --verify "refs/tags/v$v" >/dev/null || die "no tag v$v"
    fetch "+refs/heads/$MAIN_BRANCH:refs/remotes/origin/$MAIN_BRANCH"
    git merge-base --is-ancestor "v$v" "origin/$MAIN_BRANCH" && die "v$v is on $MAIN_BRANCH already: nothing to forward-port"
    changelog_has "origin/$MAIN_BRANCH" "$v" && { note "$MAIN_BRANCH already has the [$v] section"; return 0; }
    sec="$(mktemp)"
    git show "v$v:CHANGELOG.md" | awk -v want="## [$v]" '
        index($0, want) == 1 { on = 1; print; next }
        on && /^## \[/ { exit }
        on { print }' > "$sec"
    [ -s "$sec" ] || die "v$v's CHANGELOG.md has no [$v] section"
    consumed="$(git diff --no-renames --diff-filter=D --name-only "v$v^" "v$v" -- "$CHANGES_DIR")"
    here="$(git rev-parse --abbrev-ref HEAD)"
    branch="chore/no-issue-forward-port-v${v//./-}"
    git switch --quiet -C "$branch" "origin/$MAIN_BRANCH"
    changelog_write "$v" "$sec"
    rm -f "$sec"
    git add CHANGELOG.md
    while read -r f; do
        [ -n "$f" ] || continue
        if git cat-file -e "HEAD:$f" 2>/dev/null; then git rm --quiet "$f"; fi
    done <<<"$consumed"
    git commit --quiet -m "chore(release): forward-port the v$v changelog to $MAIN_BRANCH"
    git push --quiet -u origin "HEAD:refs/heads/$branch"
    # shellcheck disable=SC2016 # the backticks are markdown, not expansions
    gh pr create --repo "$REPO" --base "$MAIN_BRANCH" --head "$branch" \
        --title "chore(release): forward-port the v$v changelog" \
        --body "$(printf '## Summary\nHotfix v%s shipped from `release/*`. This copies its CHANGELOG.md section into `%s` and deletes the change files it consumed there, so the next release does not list them twice. `release/*` is never merged back.\n\n## Issue\nRefs: none (release)\n\n## Change files\nnone needed (deletes consumed ones)\n\n## AI disclosure\nTool: none (scripts/release.sh). Extent: generated. Human reviewed: no.\n' "$v" "$MAIN_BRANCH")" \
        || note "could not open the PR: open $branch -> $MAIN_BRANCH by hand"
    git switch --quiet "$here" 2>/dev/null || true
}

# CI, first step of the deploy (and of release publish on a
# tag): the tag that started this run must be a release commit on main or
# on a release/v* hotfix line. Stops a stray `v*` tag (on a feature branch,
# on a commit with another version) from reaching production. With GH_TOKEN
# set it also requires `CI passed` green on the commit.
cmd_tag_guard() {
    local ref="${GITHUB_REF:-}" tag v ok=1 a cv line
    fail() { echo "::error::$*"; ok=0; }
    case "$ref" in
        refs/tags/v*) tag="${ref#refs/tags/}" ;;
        *) die "this runs from a v* tag (ref is '${ref:-none}'). Production changes only by tag: just release-tag, or dispatch this workflow with the tag as its ref." ;;
    esac
    [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "tag $tag is not vX.Y.Z"
    v="$(version_at HEAD)"
    [ "$tag" = "v$v" ] || fail "tag $tag but Cargo.toml says $v"
    for a in $IOS_APPS; do
        cv="$(crate_version_at "$a")"
        [ "$cv" = "$v" ] || fail "$(ios_manifest "$a") resolves to version '${cv:-none}' but the tag is $tag"
    done
    fetch_lines
    line="$(release_line_of HEAD)"
    [ -n "$line" ] || fail "$tag ($(git rev-parse --short HEAD)) is on neither $MAIN_BRANCH nor a release/v* line: releases are tagged there"
    grep -q "^## \[$v\]" CHANGELOG.md 2>/dev/null || fail "CHANGELOG.md has no [$v] section: $tag is not a release commit"
    [ "$ok" = 1 ] || exit 1
    if [ -n "${GH_TOKEN:-}" ]; then require_green "$(git rev-parse HEAD)"; else note "no GH_TOKEN: '$BUILD_CHECK' not checked here (just release-tag checked it)"; fi
    echo "tag guard: ok ($tag at $(git rev-parse --short HEAD), on $line)"
}


# The block both TestFlight uploads fill in, kept at the end of the
# release notes between these markers: one line per app.
TF_BEGIN='<!-- testflight:begin -->'
TF_END='<!-- testflight:end -->'

# The app's line in the block of a release body (empty when there is none).
tf_app_line() { # <body> <app>
    printf '%s\n' "$1" | sed -n "\\#$TF_BEGIN#,\\#$TF_END#p" | grep -F -- "- $(ios_title "$2") (" | head -1 || true
}

tf_pending_line() { # <app>
    echo "- $(ios_title "$1") (\`$(ios_bundle_id "$1")\`): pending — \`$1-ios\` builds this tag and writes its build number here"
}

# The whole block: this app's line is <newline> (when given), every other
# app's is the one already in <body>, else pending.
tf_render() { # <tag> <version> <body> [<app> <newline>]
    local a l
    echo "$TF_BEGIN"
    echo "### iOS TestFlight"
    echo "Marketing version **$2** (CFBundleShortVersionString) == this release, \`$1\`, for Studio. One line per app, its latest upload."
    for a in $IOS_APPS; do
        if [ "$a" = "${4:-}" ]; then
            echo "${5:-}"
        else
            l="$(tf_app_line "$3" "$a")"
            echo "${l:-$(tf_pending_line "$a")}"
        fi
    done
    echo "$TF_END"
}


# CI, on the tag (and from the iOS workflows, and by hand). The tag already
# exists (cmd_tag pushed it); this attaches the GitHub Release with the
# changelog section, moved git deps and the TestFlight block. Idempotent.
# With no argument the tag is the ref of the run, else HEAD's workspace
# version. studio-ios asks for the Release of a tag too, and may be
# first.
publish_release() { # [tag]
    local v tag="${1:-}" target prev notes line
    if [ -z "$tag" ]; then
        case "${GITHUB_REF:-}" in refs/tags/v*) tag="${GITHUB_REF#refs/tags/}" ;; esac
    fi
    if [ -n "$tag" ]; then v="${tag#v}"; else v="$(version_at HEAD)"; tag="v$v"; fi
    is_semver "$v" || die "version '$v' is not X.Y.Z"
    if [ "$(version_at HEAD)" != "$v" ] && [ -z "${1:-}" ]; then
        die "tag $tag but Cargo.toml says $(version_at HEAD)"
    fi
    if gh release view "$tag" --repo "$REPO" >/dev/null 2>&1; then
        note "release $tag already exists — nothing to do"; return 0
    fi
    if ! grep -q "^## \[$v\]" CHANGELOG.md 2>/dev/null; then
        note "CHANGELOG.md has no [$v] section — not a release commit, nothing to publish"; return 0
    fi
    git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "no tag $tag: tag first (just release-tag)"
    target="$(git rev-parse "$tag^{commit}")"
    prev="$(last_tag "$target^")"
    line="$(release_line_of "$target")"

    notes="$(mktemp)"
    {
        cmd_notes "$v" || true
        local d; d="$(git_deps_moved "$prev" "$target")"
        if [ -n "$d" ]; then
            echo
            echo "### Git dependencies moved"
            # shellcheck disable=SC2086 # $d is a word list
            printf -- '- %s\n' $d
        fi
        echo
        echo "---"
        echo "Tag \`$tag\` at \`$(git rev-parse --short "$target")\` on ${line:-main} (site images \`sha-$(git rev-parse --short "$target")\`)."
        [ -n "$prev" ] && echo "Full diff: https://github.com/$REPO/compare/$prev...$tag"
        echo
        tf_render "$tag" "$v" ""
    } > "$notes"
    [ -s "$notes" ] || die "no release notes for $v"

    if ! gh release create "$tag" --repo "$REPO" --verify-tag \
        --title "studio $v" --notes-file "$notes" --latest; then
        gh release view "$tag" --repo "$REPO" >/dev/null 2>&1 \
            || die "could not create the GitHub Release for $tag"
        note "release $tag was created meanwhile — nothing to do"; rm -f "$notes"; release_dedupe "$tag"; return 0
    fi
    rm -f "$notes"
    release_dedupe "$tag"
    note "published https://github.com/$REPO/releases/tag/$tag"
}

# `publish` and both ios-release steps start together on a tag, and
# `gh release view` + `gh release create` is not atomic: GitHub lets three
# creators each make a Release for one tag (v0.1.1 got three, and editing any
# of them then fails with 422 "tag_name already_exists"). Whoever created one
# keeps the lowest id and deletes the rest (the tag stays); concurrent callers
# converge on the same survivor.
release_dedupe() { # <tag>
    local tag="$1" ids keep id
    ids="$(gh api --paginate "repos/$REPO/releases" --jq ".[] | select(.tag_name == \"$tag\") | .id" 2>/dev/null | sort -n)" || return 0
    keep="$(head -1 <<<"$ids")"
    [ -n "$keep" ] || return 0
    for id in $ids; do
        [ "$id" = "$keep" ] && continue
        gh api -X DELETE "repos/$REPO/releases/$id" >/dev/null 2>&1 \
            && note "removed the duplicate Release $id of $tag (kept $keep)"
    done
    return 0
}

cmd_publish() { publish_release "$@"; }


# CI, first job of <app>-ios.yml, run in a checkout of the tag. TestFlight
# ships exactly the tag: refuse a tag that is not vX.Y.Z, an app crate
# (or the workspace) whose version is not the tag's, a tag without its
# changelog section, a commit that is on neither main nor a release/v*
# hotfix line. With GH_TOKEN set, `CI passed` must be green on it too.
# Prints the marketing version.
cmd_ios_resolve() { # <app> <tag>
    local app="${1:-}" tag="${2:-}" v cv wv sha re line
    ios_check "$app"
    re='^v[0-9]+\.[0-9]+\.[0-9]+$'
    [[ "$tag" =~ $re ]] || die "tag '$tag' does not match $re: TestFlight uploads only from a release tag"
    v="${tag#v}"
    cv="$(crate_version_at "$app")"
    [ "$cv" = "$v" ] \
        || die "$(ios_manifest "$app") says version '${cv:-none}' but the tag is $tag ($v): the marketing version is the tag's, so the crate must carry it (version.workspace = true, released with just release-prepare)"
    wv="$(version_at)"
    [ "$wv" = "$v" ] \
        || die "[workspace.package] version is '$wv' but the tag is $tag ($v): one version for the whole studio"
    grep -q "^## \[$v\]" CHANGELOG.md 2>/dev/null \
        || die "CHANGELOG.md has no [$v] section: $tag is not a release commit"
    sha="$(git rev-parse HEAD)"
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        [ "$(git rev-parse "$tag^{commit}")" = "$sha" ] || die "this checkout ($sha) is not $tag"
    fi
    fetch_lines
    line="$(release_line_of "$sha")"
    [ -n "$line" ] \
        || die "$tag ($(git rev-parse --short "$sha")) is in neither origin/$MAIN_BRANCH nor an origin/release/v* line: only a release that reached one goes to TestFlight"
    if [ -n "${GH_TOKEN:-}" ]; then require_green "$sha"; fi
    note "$tag: marketing version $v, commit $(git rev-parse --short "$sha") on $line, bundle $(ios_bundle_id "$app")"
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
        { echo "version=$v"; echo "sha=$sha"; echo "tag=$tag"; } >> "$GITHUB_OUTPUT"
    fi
    echo "$v"
}

cmd_ios_release() { # <tag>
    local tag="${1:-}"
    [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "usage: ios-release vX.Y.Z"
    publish_release "$tag"
}

# The app's line once its build is uploaded; earlier builds of the same
# version (a re-run) stay listed at its end.
ios_record_line() { # <app> <build> <run-url> <prior line>
    local app="$1" build="$2" run="$3" prior="$4" cur old list="" line
    cur="$(sed -n 's/^.*: build \([0-9][0-9]*\),.*$/\1/p' <<<"$prior")"
    old="$(sed -n 's/^.*(earlier builds: \([0-9, ]*\))$/\1/p' <<<"$prior")"
    if [ "$cur" = "$build" ]; then echo "$prior"; return 0; fi
    [ -z "$cur" ] || list="$cur"
    [ -z "$old" ] || list="${list:+$list, }$old"
    line="- $(ios_title "$app") (\`$(ios_bundle_id "$app")\`): build $build, $(date -u +%FT%TZ)${run:+, [run]($run)}"
    [ -z "$list" ] || line="$line (earlier builds: $list)"
    echo "$line"
}

# CI, last job of <app>-ios.yml: write what was uploaded into the release
# notes, so the GitHub Release and the TestFlight build name each other.
# More than one run can record into the SAME Release at about the same time
# (a re-run beside the first), and `gh release edit` replaces the whole
# body, so each run re-reads the
# notes before editing and, after a short settle, checks its own line
# survived; if the other run's edit overwrote it, it goes round again.
cmd_ios_record() { # <app> <tag> <marketing> <build> [run-url]
    local app="${1:-}" tag="${2:-}" mv="${3:-}" build="${4:-}" run="${5:-}"
    local body prior line keep out tries=0 max="${RECORD_TRIES:-6}"
    ios_check "$app"
    [ "$tag" = "v$mv" ] || die "marketing version '$mv' is not the version of tag '$tag'"
    [[ "$build" =~ ^[0-9]+$ ]] || die "build number '$build' is not a number"
    release_dedupe "$tag"   # an edit of a tag with several Releases is a 422
    while :; do
        tries=$((tries + 1))
        body="$(gh release view "$tag" --repo "$REPO" --json body --jq .body)" \
            || die "no GitHub Release for $tag (TestFlight needs one: ios-release)"
        prior="$(tf_app_line "$body" "$app")"
        if ! grep -Fq -- "): build $build," <<<"$prior"; then
            line="$(ios_record_line "$app" "$build" "$run" "$prior")"
            keep="$(printf '%s\n' "$body" | sed "\\#$TF_BEGIN#,\\#$TF_END#d" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')"
            out="$(mktemp)"
            {
                printf '%s\n\n' "$keep"
                tf_render "$tag" "$mv" "$body" "$app" "$line"
            } > "$out"
            gh release edit "$tag" --repo "$REPO" --notes-file "$out"
            rm -f "$out"
        fi
        sleep "${RECORD_SETTLE:-4}"
        body="$(gh release view "$tag" --repo "$REPO" --json body --jq .body)" \
            || die "no GitHub Release for $tag"
        if grep -Fq -- "): build $build," <<<"$(tf_app_line "$body" "$app")"; then
            note "recorded on $tag: $app marketing $mv, build $build"
            return 0
        fi
        [ "$tries" -lt "$max" ] || die "could not record $app build $build on $tag after $tries tries: the notes kept being overwritten"
        note "the notes of $tag changed under $app's edit; trying again ($tries/$max)"
        sleep "$((RANDOM % 3 + 1))"
    done
}

case "${1:-}" in
    version) shift; cmd_version "$@" ;;
    next) shift; cmd_next "$@" ;;
    notes) shift; cmd_notes "$@" ;;
    plan) shift; cmd_plan "$@" ;;
    prepare) shift; cmd_prepare "$@" ;;
    pr) shift; cmd_pr "$@" ;;
    merge) shift; cmd_merge "$@" ;;
    tag) shift; cmd_tag "$@" ;;
    hotfix) shift; cmd_hotfix "$@" ;;
    forward-port) shift; cmd_forward_port "$@" ;;
    guard) shift; cmd_guard "$@" ;;
    release-only) shift; cmd_release_only "$@" ;;
    tag-guard) shift; cmd_tag_guard "$@" ;;
    publish) shift; cmd_publish "$@" ;;
    stamp) shift; cmd_stamp "$@" ;;
    ios-resolve) shift; cmd_ios_resolve "$@" ;;
    ios-release) shift; cmd_ios_release "$@" ;;
    ios-record) shift; cmd_ios_record "$@" ;;
    *) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
