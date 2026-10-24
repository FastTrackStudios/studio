#!/usr/bin/env bash
# studio releases: ONE version, ONE tag, ONE GitHub Release. A release is a
# CalVer tag pushed on a green commit of main; nothing is committed for it.
#
# Tag-from-main (2026-10, after v26.10.3; docs/agents/release-workflow.md
# holds studio's specifics, the fleet `release-workflow` skill the rest):
#
#   feat/... fix/... --PR, squash--> main     main is the only long-lived
#       each PR carries a cargo-rail           branch. A merge to main runs
#       change file (.changes/*.md)            CI (`CI passed`) and ships
#                                              NOTHING.
#   just release [<sha>]  --> tag vYY.M.MICRO on main's newest commit whose
#       `CI passed` is green (or on <sha>)
#                                              The TAG ships, and only the
#                                              tag: the studio-web image
#                                              (Images), the Studio
#                                              TestFlight upload, the
#                                              desktop installers and the
#                                              one GitHub Release. (Argo CD
#                                              deploys the image from
#                                              gitops; this repo deploys
#                                              nothing.)
#
#   hotfix, when main can't ship: release/v<latest tag> (from the tag)
#       <-- PR of `cherry-pick -x` of fixes already on main; once that
#       line's `CI passed` is green, `just release <sha>` tags it. Nothing
#       is forward-ported (a release commits nothing). release/* branches
#       are never merged back.
#
# Production is the latest `v*` tag.
#
# THE VERSION IS THE TAG. Releases commit no version: every release build
# runs `release.sh stamp X.Y.Z` first, which writes the tag's version into
# its own checkout's `[workspace.package] version` and Cargo.lock's
# workspace members (uncommitted), so every crate that says
# `version.workspace = true` (the iOS app crate apps/studio/desktop
# included) builds as the release. The version committed on main is
# whatever it last was (26.10.3, the last release commit): it is NOT the
# released version. There are no per-app tags, branches or changelogs.
#
# Versions are CalVer YY.M.MICRO (no leading zeros: Cargo, Apple and the
# cluster's image-updater filter all need that). The NEXT version is
# computed, never chosen: take the highest vX.Y.Z tag (tags with a `-`
# suffix don't count); if its YY.M is this month (UTC) the next is
# MICRO+1, otherwise YY.M.0 of this month. Hotfixes use the same rule, so
# every version is unique and ascending (App Store Connect requires it).
#
# Release notes come from the cargo-rail change files (`cargo rail change
# add <crates> --bump <none|patch|minor|major> -m "<note>"`, one per PR)
# ADDED since the previous tag: major -> "Breaking / Migration", minor ->
# "Added", patch -> "Fixed and improved", none -> no entry. They go into
# the GitHub Release; nothing is deleted or committed. A change file a
# hotfix line already shipped (cherry-picked onto a release/v* line and
# tagged there) is not listed again. CHANGELOG.md holds the releases up to
# 26.10.3 and is no longer written.
#
#   scripts/release.sh version [REV]      the version the tree (or REV) carries
#   scripts/release.sh next               the version the next release gets
#   scripts/release.sh plan               what is live, what main holds since,
#                                          the next version, the draft notes
#   scripts/release.sh release [--dry-run] [--yes] [SHA]
#                                         tag the next version on main's newest
#                                          commit with a green `CI passed` (or
#                                          on SHA: on main or release/v*, green)
#                                          and push it: THIS SHIPS
#   scripts/release.sh hotfix <commit>... release/v<latest tag> from the tag
#                                          (if missing), a branch with the fixes
#                                          cherry-picked (-x) from main, its PR
#   scripts/release.sh notes [TAG|REV]    the release notes of a tag, or the
#                                          draft for REV (default origin/main)
#   scripts/release.sh stamp X.Y.Z        CI, first step of every release build:
#                                          write X.Y.Z into this checkout's
#                                          [workspace.package] version and the
#                                          workspace members' Cargo.lock
#                                          entries (uncommitted)
#   scripts/release.sh image-tag TAG      the image tag of a release tag: the
#                                          bare CalVer (v26.10.4 -> 26.10.4);
#                                          refuses anything else
#   scripts/release.sh tag-guard          CI (every tag-triggered job, after
#                                          stamp): is this tag a release of a
#                                          green commit on main or release/v*?
#   scripts/release.sh publish [TAG]      CI: the GitHub Release for the tag
#   scripts/release.sh ios-resolve APP TAG
#                                         CI (studio-ios, desktop, after
#                                          stamp): refuse a tag that is not
#                                          vX.Y.Z, whose stamped app crate
#                                          version differs, or that is not on
#                                          main or release/v* and green; print
#                                          the marketing version
#   scripts/release.sh ios-release TAG    CI: make sure the tag's GitHub Release
#                                          exists (idempotent)
#   scripts/release.sh ios-record APP TAG MARKETING BUILD [RUN_URL]
#                                         CI: after the TestFlight upload, write
#                                          the marketing version + build number
#                                          into the release notes
#   scripts/release.sh guard              the `Release guard` check: there are
#                                          no release PRs any more; passes
#
# `release` pushes the tag as whoever runs it: a person, or the fts-forge
# App. A tag pushed with a workflow's GITHUB_TOKEN starts no workflow, so a
# workflow must never push it with that token.
#
# See docs/agents/release-workflow.md for the whole procedure.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
REPO="${GITHUB_REPOSITORY:-FastTrackStudios/studio}"
MAIN_BRANCH="${MAIN_BRANCH:-main}"
CHANGES_DIR=".changes"
# The required check of checks.yml (the aggregator). A release is tagged,
# built and uploaded only from a commit where it is green.
BUILD_CHECK="${BUILD_CHECK:-CI passed}"
# How far back `release` looks on main for a green commit.
SCAN_MAX="${RELEASE_SCAN_MAX:-60}"

die() { echo "release: $*" >&2; exit 1; }
note() { echo "release: $*" >&2; }

# `[workspace.package] version` of Cargo.toml at a revision (default: the
# working tree, which a release build has stamped).
version_at() {
    local src
    if [ -n "${1:-}" ]; then src="$(git show "$1:Cargo.toml")"; else src="$(cat Cargo.toml)"; fi
    printf '%s\n' "$src" \
        | sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' \
        | head -1
}

is_semver() { [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; }

# YY.M.MICRO: two-digit year, month 1-12 and a counter, no zero padding
# (`26.09.1` is not a valid Cargo version, and argocd-image-updater's
# filter `^[0-9]+\.[0-9]+\.[0-9]+$` with semver ordering needs exactly
# this shape to order the images).
is_calver() { [[ "$1" =~ ^[1-9][0-9]\.(1[0-2]|[1-9])\.(0|[1-9][0-9]*)$ ]]; }

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

# The newest release tag reachable from a revision: the previous release
# of a commit, where its notes start.
last_tag() { release_tags --merged "${1:-HEAD}" | head -1; }

# This month as YY.M, UTC. RELEASE_MONTH=YY.M overrides it (the sandbox
# test; never set it for a real release).
this_month() {
    if [ -n "${RELEASE_MONTH:-}" ]; then
        [[ "$RELEASE_MONTH" =~ ^[1-9][0-9]\.([1-9]|1[0-2])$ ]] || die "RELEASE_MONTH '$RELEASE_MONTH' is not YY.M"
        echo "$RELEASE_MONTH"; return 0
    fi
    echo "$(date -u +%y).$((10#$(date -u +%m)))"
}

# The CalVer next-version rule (see the header). Needs the tags fetched.
next_version() {
    local tag v ym
    tag="$(latest_tag)"; ym="$(this_month)"
    v="${tag#v}"
    if [ -n "$tag" ] && [ "${v%.*}" = "$ym" ]; then
        echo "$ym.$(( ${v##*.} + 1 ))"
    else
        echo "$ym.0"
    fi
}

# ── the iOS app (Studio) ─────────────────────────────────────────────────
# Studio (Signal and Session in one app, apps/studio/desktop) is the one iOS
# app; Signal's and Session's own App Store records get no new builds. It is
# built from the one `v<version>` tag. Its crate follows the workspace
# version; the helpers below read that back so the tag guard and the
# TestFlight gate can prove the stamped checkout carries the tag's version.
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

# `git fetch` that FAILS loudly: a guard or a release that cannot see
# origin must never read that as "nothing there".
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

# The latest run of `CI passed` on <sha> as "<status> <conclusion>" (a
# re-run supersedes a failure; "null null" when it never ran). A network or
# API failure dies.
check_state() { # <sha>
    local out
    out="$(gh api -X GET "repos/$REPO/commits/$1/check-runs" -f check_name="$BUILD_CHECK" -f per_page=100 \
        --jq '.check_runs | sort_by(.started_at) | last | "\(.status) \(.conclusion)"')" \
        || die "cannot read the '$BUILD_CHECK' check of $1 (network or auth problem); refusing to guess"
    echo "$out"
}

# `CI passed` on <sha> must have run and be green.
require_green() { # <sha>
    local out
    out="$(check_state "$1")"
    [ "$out" = "completed success" ] \
        || die "'$BUILD_CHECK' on $(git rev-parse --short "$1") is '${out:-missing}', not green: nothing ships from a red or unchecked commit (fix main, or wait for the run)"
}

# The newest first-parent commit of origin/<line> after <since> (a tag, or
# empty) whose `CI passed` is green; empty when none is. Says why it skips
# each newer one (still running, red, cancelled by a newer merge).
newest_green() { # <line> [since]
    local line="$1" since="${2:-}" c st n=0
    while read -r c; do
        [ -n "$c" ] || continue
        n=$((n + 1))
        [ "$n" -le "$SCAN_MAX" ] || { note "looked at $SCAN_MAX commits of $line: none green"; return 0; }
        st="$(check_state "$c")"
        if [ "$st" = "completed success" ]; then echo "$c"; return 0; fi
        note "skipping $(git log -1 --format='%h %s' "$c"): '$BUILD_CHECK' is '${st:-missing}'"
    done < <(if [ -n "$since" ]; then git rev-list --first-parent "origin/$line" "^$since"; else git rev-list --first-parent "origin/$line"; fi)
    return 0
}

cmd_version() { version_at "$@"; } # [rev]
cmd_next() {
    git fetch --quiet --tags origin 2>/dev/null || note "offline: next version from the local tags"
    next_version
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

# The change files a release of <sha> lists, oldest first (the order their
# PRs merged): those ADDED between <prev> (the previous tag; empty for the
# whole history) and <sha>, minus the ones a hotfix release on another line
# already shipped (a newer release tag not merged into <sha> whose tree has
# the file: it was cherry-picked there with its fix).
release_change_files() { # <prev> <sha> [below]
    local prev="$1" sha="$2" below="${3:-}" added shipped="" t
    if [ -n "$prev" ]; then
        added="$(git diff --no-renames --diff-filter=A --name-only "$prev" "$sha" -- "$CHANGES_DIR/" | grep -E '\.md$' || true)"
    else
        added="$(git ls-tree --name-only "$sha" "$CHANGES_DIR/" | grep -E '\.md$' || true)"
    fi
    [ -n "$added" ] || return 0
    for t in $(other_line_tags "$sha" "$below"); do
        [ -z "$prev" ] || gt "${t#v}" "${prev#v}" || continue
        shipped+="$(git ls-tree --name-only "$t" "$CHANGES_DIR/" | grep -E '\.md$' || true)"$'\n'
    done
    shipped="$(grep . <<<"$shipped" || true)"
    if [ -n "$shipped" ]; then
        added="$(grep -vxF -f <(printf '%s\n' "$shipped") <<<"$added" || true)"
        [ -n "$added" ] || return 0
    fi
    { if [ -n "$prev" ]; then git log --diff-filter=A --reverse --format= --name-only "$prev..$sha" -- "$CHANGES_DIR" 2>/dev/null || true
      else git log --diff-filter=A --reverse --format= --name-only "$sha" -- "$CHANGES_DIR" 2>/dev/null || true; fi
      printf '%s\n' "$added"; } \
        | awk 'NF && !seen[$0]++' | grep -Fx -f <(printf '%s\n' "$added") || true
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

# Release tags on ANOTHER line than <sha>: neither in its history nor
# after it (a hotfix tag on a release/v* line, seen from main), newest
# first. With <below> (the version being described), only the ones older
# than it: notes of a tag never change once later releases exist.
other_line_tags() { # <sha> [below]
    local t
    for t in $(release_tags --no-merged "$1" --no-contains "$1"); do
        [ -z "${2:-}" ] || gt "${2#v}" "${t#v}" || continue
        echo "$t"
    done
}

# Hotfix releases tagged on a release/v* line after <prev>: their fixes
# reached main first, so main's next notes say they cover them.
includes_line() { # <prev> <sha> [below]
    local hot
    hot="$(other_line_tags "$2" "${3:-}" | head -1)"
    [ -n "$hot" ] || return 0
    if [ -z "$1" ] || gt "${hot#v}" "${1#v}"; then
        echo "Includes all changes up to $hot."
    fi
}

# The release notes of <sha> since <prev>, grouped the way the CHANGELOG
# sections were (Breaking / Migration, Added, Fixed and improved; `none`
# lists nothing). Read from <sha>'s tree. <version>: the release they
# describe (empty for a draft).
render_notes() { # <prev> <sha> [version]
    local prev="$1" sha="$2" ver="${3:-}" files f b group title any=0 out incl
    files="$(release_change_files "$prev" "$sha" "$ver")"
    if [ -n "$files" ]; then
        while read -r f; do
            b="$(change_bump "$sha" "$f")"
            [ "$b" != bad ] || die "$f is not a change file (front matter: \"crate\" = \"none|patch|minor|major\"): fix it on main (cargo rail change add)"
        done <<<"$files"
    fi
    incl="$(includes_line "$prev" "$sha" "$ver")"
    if [ -n "$incl" ]; then echo "$incl"; echo; fi
    for group in major minor patch; do
        [ -n "$files" ] || break
        case "$group" in
            major) title="Breaking / Migration" ;;
            minor) title="Added" ;;
            patch) title="Fixed and improved" ;;
        esac
        out=""
        while read -r f; do
            [ "$(change_bump "$sha" "$f")" = "$group" ] || continue
            out+="$(change_entry "$sha" "$f")"$'\n'
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

# The notes of a tag (since the tag before it), or the draft for a
# revision (default origin/main) since its newest tag.
cmd_notes() { # [TAG|REV]
    local r="${1:-origin/$MAIN_BRANCH}" sha prev
    sha="$(git rev-parse -q --verify "$r^{commit}")" || die "'$r' is not a tag or commit"
    if git rev-parse -q --verify "refs/tags/$r" >/dev/null && [[ "$r" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        render_notes "$(last_tag "$sha^")" "$sha" "$r"
    else
        render_notes "$(last_tag "$sha")" "$sha"
    fi
}

cmd_plan() {
    git fetch --quiet --tags origin "+refs/heads/$MAIN_BRANCH:refs/remotes/origin/$MAIN_BRANCH" 2>/dev/null \
        || note "offline: planning from the local origin/$MAIN_BRANCH and tags"
    local ref="origin/$MAIN_BRANCH" tag mtag next
    tag="$(latest_tag)"; mtag="$(last_tag "$ref")"; next="$(next_version)"
    echo "production (latest v* tag):  ${tag:-none}"
    echo "on $MAIN_BRANCH since ${mtag:-the start}:     $(count_between "$mtag" "$ref") commits"
    echo "change files since ${mtag:-the start}:  $(release_change_files "$mtag" "$ref" | grep -c . || true)"
    echo "next release:                v$next (CalVer YY.M.MICRO from the highest tag and this month)"
    echo "                             just release tags it on $MAIN_BRANCH's newest commit with a green '$BUILD_CHECK'"
    echo "                             (just release --dry-run names that commit)"
    echo "committed version:           $(version_at "$ref") (not the released one: builds stamp the tag's)"
    echo
    echo "── draft notes ($ref) ──"
    render_notes "$mtag" "$ref"
    if [ -n "$mtag" ]; then
        local d; d="$(git_deps_moved "$mtag" "$ref")"
        echo "── git dependencies moved since $mtag ──"
        echo "${d:-none}"
    fi
}

# Tag the next version on a green commit and push it. THE TAG SHIPS: the
# image, TestFlight, the installers and the Release all start from it, in
# parallel; nothing here waits for any of them.
cmd_release() { # [--dry-run] [--yes] [SHA]
    local dry=0 yes=0 want="" sha line tag prev next ltag existing a
    while [ $# -gt 0 ]; do
        case "$1" in
            --dry-run|-n) dry=1 ;;
            --yes|-y) yes=1 ;;
            -*) die "unknown option '$1' (release [--dry-run] [--yes] [SHA])" ;;
            *) [ -z "$want" ] || die "one commit at most"; want="$1" ;;
        esac
        shift
    done
    fetch_lines
    tag="$(latest_tag)"
    if [ -n "$want" ]; then
        sha="$(git rev-parse -q --verify "$want^{commit}")" || die "'$want' is not a commit (fetched origin's main and release/v*)"
        line="$(release_line_of "$sha")"
        [ -n "$line" ] || die "$(git rev-parse --short "$sha") is on neither origin/$MAIN_BRANCH nor an origin/release/v* line: only a commit that reached one is released"
        require_green "$sha"
    else
        line="$MAIN_BRANCH"
        ltag="$(last_tag "origin/$line")"
        sha="$(newest_green "$line" "$ltag")"
        [ -n "$sha" ] || die "no commit on $MAIN_BRANCH since ${ltag:-the start} has a green '$BUILD_CHECK' (gh run list -b $MAIN_BRANCH -w checks.yml): wait for the run, or fix main"
    fi
    existing="$(git tag --points-at "$sha" | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -1 || true)"
    [ -z "$existing" ] || die "$(git rev-parse --short "$sha") is already released as $existing: nothing new to ship"
    # Never a commit older than its line's newest release: that would ship
    # a regression under a newer version.
    ltag="$(last_tag "origin/$line")"
    if [ -n "$ltag" ] && git merge-base --is-ancestor "$sha" "$ltag"; then
        die "$(git rev-parse --short "$sha") is older than $ltag on $line: a release only moves $line forward"
    fi
    prev="$(last_tag "$sha")"
    next="$(next_version)"
    is_calver "$next" || die "next version '$next' is not CalVer YY.M.MICRO"
    if [ -n "$tag" ]; then
        gt "$next" "${tag#v}" || die "the next version $next is not above the highest tag $tag (the clock, or RELEASE_MONTH?): versions only increase"
    fi
    git rev-parse -q --verify "refs/tags/v$next" >/dev/null && die "tag v$next already exists"

    echo "release v$next: $(git log -1 --format='%h %s' "$sha")"
    echo "  on $line, $(count_between "$prev" "$sha") commits since ${prev:-the start}; highest tag ${tag:-none}; '$BUILD_CHECK' green"
    echo
    echo "── release notes ──"
    render_notes "$prev" "$sha" "$next"
    if [ -n "$prev" ]; then
        local d; d="$(git_deps_moved "$prev" "$sha")"
        [ -z "$d" ] || { echo "── git dependencies moved since $prev ──"; echo "$d"; }
    fi
    if [ "$dry" = 1 ]; then note "dry run: v$next not tagged"; return 0; fi
    if [ "$yes" = 0 ] && [ -t 0 ]; then
        printf 'Push v%s? It ships the studio-web image, TestFlight, the installers and the Release. [y/N] ' "$next" >&2
        read -r a
        [ "$a" = y ] || [ "$a" = Y ] || die "not tagged"
    fi
    # An annotated tag needs a committer identity; a bot checkout may have none.
    local ident=()
    git config user.email >/dev/null 2>&1 || [ -n "${GIT_COMMITTER_EMAIL:-}" ] \
        || ident=(-c "user.name=fts-forge[bot]" -c "user.email=fts-forge[bot]@users.noreply.github.com")
    git ${ident[@]+"${ident[@]}"} tag -a "v$next" "$sha" -m "studio $next"
    git push --quiet origin "refs/tags/v$next"
    note "pushed v$next at $(git rev-parse --short "$sha") on $line: Images, studio-ios, desktop and the Release start from the tag"
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

# The image tag of a release tag: the bare CalVer, which is what
# argocd-image-updater orders (semver over `^[0-9]+\.[0-9]+\.[0-9]+$`).
# Anything else (a `v`, a leading zero, a suffix) is refused: it would
# sort wrong or be ignored by the cluster.
cmd_image_tag() { # <tag>
    local t="${1:-}" v
    v="${t#v}"
    [ "$t" != "$v" ] || die "'$t' is not a release tag (vYY.M.MICRO)"
    is_calver "$v" || die "'$t' is not vYY.M.MICRO CalVer (no leading zeros, month 1-12): its image tag would not match the cluster's ^[1-9][0-9]\\.(1[0-2]|[1-9])\\.(0|[1-9][0-9]*)\$"
    echo "$v"
}

# A hotfix: fixes already merged into main, cherry-picked (-x) onto
# release/v<latest tag>, as a PR into that branch (the full suite runs on
# it, and again once it merges). Then `just release <merged sha>`.
cmd_hotfix() {
    local tag line branch c first n title body
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
    for c in "$@"; do
        git cherry-pick -x "$c" >/dev/null \
            || die "cherry-pick of $c conflicts: resolve it, git cherry-pick --continue, pick the rest, push $branch and open its PR into $line"
    done
    n=$#
    if [ "$n" = 1 ]; then
        title="$(git log -1 --format=%s "$1" | sed -E 's/ \(#[0-9]+\)$//')"
    else
        title="fix: hotfix $n fixes onto $tag"
    fi
    note "$branch: $n fix(es) on $line"
    git push --quiet -u origin "HEAD:refs/heads/$branch"
    body="$(mktemp)"
    {
        echo "## Summary"
        echo "Hotfix onto \`$line\` (production line). Cherry-picked with \`-x\` from \`$MAIN_BRANCH\`, where each fix landed first:"
        echo
        git log --format='- %s' "origin/$line..HEAD"
        echo
        echo "The full suite runs on this PR, and again on \`$line\` once it merges. Then \`just release <merge sha>\` tags the next version on it (once its \`$BUILD_CHECK\` is green). Nothing is forward-ported: a release commits nothing."
        echo
        echo "## Issue"
        echo "Refs: see the picked commits"
        echo
        printf '## How tested\n%s\n\n## Risk and rollback\n%s\n\n## Change files\n%s\n\n## AI disclosure\nTool: none (scripts/release.sh). Extent: generated. Human reviewed: no.\n' \
            "The picked commits passed checks on $MAIN_BRANCH; the full suite runs here." \
            "Revert the merge on \`$line\` before it is tagged; after, release the next version." \
            "the picked commits' own change files"
    } > "$body"
    gh pr create --repo "$REPO" --base "$line" --head "$branch" --title "$title" --body-file "$body"
    rm -f "$body"
}

# The `Release guard` required check (release.yml) once judged release
# PRs. There are none now (a release is a tag), so it passes; it stays so
# main's ruleset keeps seeing the check it requires.
cmd_guard() {
    echo "release guard: releases are tags pushed by \`just release\` (no release PRs, nothing to check)"
}

# Kept for older workflow copies that still ask: no PR is a release PR.
cmd_release_only() { return 1; }

# CI, in every tag-triggered job, AFTER `stamp`: the tag that started this
# run must be vX.Y.Z on a commit of main or of a release/v* hotfix line,
# with `CI passed` green on it (with GH_TOKEN set). The test suite is not
# run again: that green check is the proof. Stops a stray `v*` tag (on a
# feature branch, on a red commit) from reaching production.
cmd_tag_guard() {
    local ref="${GITHUB_REF:-}" tag v ok=1 a cv line
    fail() { echo "::error::$*"; ok=0; }
    case "$ref" in
        refs/tags/v*) tag="${ref#refs/tags/}" ;;
        *) die "this runs from a v* tag (ref is '${ref:-none}'). Production changes only by tag: just release, or dispatch this workflow with the tag as its ref." ;;
    esac
    [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "tag $tag is not vX.Y.Z"
    v="$(version_at)"
    [ "$tag" = "v$v" ] || fail "tag $tag but this checkout's version is $v: run 'release.sh stamp $tag' first"
    for a in $IOS_APPS; do
        cv="$(crate_version_at "$a")"
        [ "$cv" = "$v" ] || fail "$(ios_manifest "$a") resolves to version '${cv:-none}' but the tag is $tag (the app takes [workspace.package] version)"
    done
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        [ "$(git rev-parse "$tag^{commit}")" = "$(git rev-parse HEAD)" ] || fail "this checkout ($(git rev-parse --short HEAD)) is not $tag"
    fi
    fetch_lines
    line="$(release_line_of HEAD)"
    [ -n "$line" ] || fail "$tag ($(git rev-parse --short HEAD)) is on neither $MAIN_BRANCH nor a release/v* line: releases are tagged there"
    [ "$ok" = 1 ] || exit 1
    if [ -n "${GH_TOKEN:-}" ]; then require_green "$(git rev-parse HEAD)"; else note "no GH_TOKEN: '$BUILD_CHECK' not checked here (just release checked it)"; fi
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



# CI, on the tag (and from the iOS and desktop workflows, and by hand). The
# tag already exists (`release` pushed it); this attaches the GitHub
# Release: the notes from the change files added since the previous tag,
# moved git deps and the TestFlight block. Idempotent. With no argument the
# tag is the ref of the run.
publish_release() { # [tag]
    local v tag="${1:-}" target prev notes line
    if [ -z "$tag" ]; then
        case "${GITHUB_REF:-}" in refs/tags/v*) tag="${GITHUB_REF#refs/tags/}" ;; esac
    fi
    [ -n "$tag" ] || die "publish needs a tag (an argument, or a run on refs/tags/v*)"
    v="${tag#v}"
    is_semver "$v" || die "version '$v' is not X.Y.Z"
    if gh release view "$tag" --repo "$REPO" >/dev/null 2>&1; then
        note "release $tag already exists — nothing to do"; return 0
    fi
    git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "no tag $tag: tag first (just release)"
    target="$(git rev-parse "$tag^{commit}")"
    prev="$(last_tag "$target^")"
    line="$(release_line_of "$target")"

    notes="$(mktemp)"
    {
        render_notes "$prev" "$target" "$v"
        local d; d="$(git_deps_moved "$prev" "$target")"
        if [ -n "$d" ]; then
            echo "### Git dependencies moved"
            # shellcheck disable=SC2086 # $d is a word list
            printf -- '- %s\n' $d
            echo
        fi
        echo "---"
        echo "Tag \`$tag\` at \`$(git rev-parse --short "$target")\` on ${line:-main} (image \`ghcr.io/fasttrackstudios/studio-web:$v\`)."
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


# CI, first job of studio-ios.yml and desktop.yml, in a checkout of the tag
# that `stamp` has written the tag's version into. TestFlight ships exactly
# the tag: refuse a tag that is not vX.Y.Z, an app crate (or workspace)
# whose stamped version is not the tag's, a commit that is on neither main
# nor a release/v* hotfix line. With GH_TOKEN set, `CI passed` must be
# green on it too. Prints the marketing version.
cmd_ios_resolve() { # <app> <tag>
    local app="${1:-}" tag="${2:-}" v cv wv sha re line
    ios_check "$app"
    re='^v[0-9]+\.[0-9]+\.[0-9]+$'
    [[ "$tag" =~ $re ]] || die "tag '$tag' does not match $re: TestFlight uploads only from a release tag"
    v="${tag#v}"
    cv="$(crate_version_at "$app")"
    [ "$cv" = "$v" ] \
        || die "$(ios_manifest "$app") resolves to version '${cv:-none}' but the tag is $tag ($v): the marketing version is the tag's, so the stamped crate must carry it (release.sh stamp $tag first; the crate takes version.workspace = true)"
    wv="$(version_at)"
    [ "$wv" = "$v" ] \
        || die "[workspace.package] version is '$wv' but the tag is $tag ($v): release.sh stamp $tag first"
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
    release) shift; cmd_release "$@" ;;
    hotfix) shift; cmd_hotfix "$@" ;;
    stamp) shift; cmd_stamp "$@" ;;
    image-tag) shift; cmd_image_tag "$@" ;;
    guard) shift; cmd_guard "$@" ;;
    release-only) shift; cmd_release_only "$@" ;;
    tag-guard) shift; cmd_tag_guard "$@" ;;
    publish) shift; cmd_publish "$@" ;;
    ios-resolve) shift; cmd_ios_resolve "$@" ;;
    ios-release) shift; cmd_ios_release "$@" ;;
    ios-record) shift; cmd_ios_record "$@" ;;
    *) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
