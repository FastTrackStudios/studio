#!/usr/bin/env bash
# Sandbox test of scripts/release.sh (tag-from-main): CalVer next versions,
# the image tag rule, `release` picking main's newest green commit and
# refusing what must not ship, the notes from the change files added since
# the previous tag, `stamp`, the tag guards, a hotfix line, and the
# TestFlight helpers, end to end on a synthetic repo and a local bare
# "origin". Nothing leaves the sandbox: gh is a shim (it keeps releases and
# PRs as files, and answers `CI passed` from a list of green commits).
# Run from anywhere:
#
#   scripts/release-sandbox.sh [SCRATCH_DIR]
#
# Needs git and awk. Exits non-zero if any check fails. Portable: it (and
# release.sh) must pass with GNU tools AND with nothing but macOS's own
# (BSD sed, bash 3.2) on PATH — CI runs both (checks.yml `Gates`,
# `Release tooling (macOS)`). Edit files in place with `sedi`, never
# `sed -i`, whose argument BSD sed reads as a backup suffix.
# shellcheck disable=SC2015,SC2016 # `[ test ] && ok || bad` is the idiom here: ok never fails; the ERE patterns hold literal backticks
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
S="${1:-${TMPDIR:-/tmp}/release-sandbox}"
REPO=FastTrackStudios/studio
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=t@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=t@example.invalid
export GIT_CONFIG_GLOBAL=/dev/null
rm -rf "$S"; mkdir -p "$S/shim" "$S/releases" "$S/prs"
export PATH="$S/shim:$PATH"
export GITHUB_REPOSITORY="$REPO"
unset GITHUB_STEP_SUMMARY GITHUB_OUTPUT GH_TOKEN RELEASE_MONTH SHIM_CHECK
pass=0; failn=0
ok()  { echo "PASS: $*"; pass=$((pass+1)); }
bad() { echo "FAIL: $*"; failn=$((failn+1)); }
# Every command runs with stdin from /dev/null: `release` asks before it
# pushes only on a terminal.
expect_ok()   { local d="$1"; shift; if "$@" >"$S/last.out" 2>&1 </dev/null; then ok "$d"; else bad "$d"; cat "$S/last.out"; fi; }
expect_fail() { local d="$1"; shift; if "$@" >"$S/last.out" 2>&1 </dev/null; then bad "$d (unexpected pass)"; cat "$S/last.out"; else ok "$d"; grep -h '::error::\|release:' "$S/last.out" | head -2 | sed 's/^/      /'; fi; }
expect_rc()   { local d="$1" want="$2"; shift 2; "$@" >"$S/last.out" 2>&1 </dev/null; local rc=$?; [ "$rc" = "$want" ] && ok "$d (exit $rc)" || { bad "$d (exit $rc, wanted $want)"; cat "$S/last.out"; }; }
expect_out()  { if grep -qiE "$3" "$2"; then ok "$1"; else bad "$1 (no match for: $3)"; sed 's/^/      | /' "$2" | head -30; fi; }
expect_no()   { if grep -qiE "$3" "$2"; then bad "$1 (found: $3)"; else ok "$1"; fi; }
# sed in place, one spelling for GNU and BSD (macOS) sed: BSD `sed -i`
# takes the next argument as a backup suffix. Through a temp file, so no
# backup is left behind.
sedi()        { local e="$1" f; shift; for f in "$@"; do sed "$e" "$f" > "$f.sedi" && mv "$f.sedi" "$f"; done; }
eq()          { [ "$2" = "$3" ] && ok "$1: $2" || bad "$1: got '$2', wanted '$3'"; }
lastpr()      { local f n=0; for f in "$S"/prs/*.meta; do [ -e "$f" ] && n=$((n+1)); done; echo "$n"; } # the newest PR's number

# ── the gh shim ──────────────────────────────────────────────────────────
# `CI passed` of a commit: $SHIM_CHECK if set (every commit), else green
# when the commit is listed in $SHIM_GREEN, else $SHIM_OTHER (running).
cat > "$S/shim/gh" <<'EOF'
#!/usr/bin/env bash
# A tiny gh: releases and PRs are files, calls are logged to $SHIM_LOG.
echo "[shim] gh $*" >> "$SHIM_LOG"
arg() { local k="$1"; shift; while [ $# -gt 0 ]; do [ "$1" = "$k" ] && { echo "$2"; return; }; shift; done; }
case "$1 $2" in
  "release view") [ -f "$SHIM_REL/$3.body" ] || exit 1
     case "$*" in *"--json body"*) b="$(cat "$SHIM_REL/$3.body")"; sleep "${SHIM_VIEW_DELAY:-0}"; printf '%s\n' "$b" ;; esac ;;
  "release create") tag="$3"; shift 3
     cp "$(arg --notes-file "$@")" "$SHIM_REL/$tag.body"; arg --title "$@" > "$SHIM_REL/$tag.title" ;;
  "release edit") tag="$3"; shift 3; cp "$(arg --notes-file "$@")" "$SHIM_REL/$tag.body" ;;
  "pr list") exit 0 ;;
  "pr create") shift 2; n=$(( $(ls "$SHIM_PRS" | grep -c meta) + 1 ))
     { echo "base=$(arg --base "$@")"; echo "head=$(arg --head "$@")"; echo "title=$(arg --title "$@")"; } > "$SHIM_PRS/$n.meta"
     f="$(arg --body-file "$@")"; if [ -n "$f" ]; then cp "$f" "$SHIM_PRS/$n.body"; else arg --body "$@" > "$SHIM_PRS/$n.body"; fi
     echo "https://github.com/x/y/pull/$n" ;;
  "api -X") case "$*" in *check-runs*)
       sha="$(printf '%s\n' "$*" | sed -n 's#.*commits/\([0-9a-f]*\)/check-runs.*#\1#p')"
       if [ -n "${SHIM_CHECK:-}" ]; then echo "$SHIM_CHECK"
       elif grep -qx "$sha" "$SHIM_GREEN" 2>/dev/null; then echo "completed success"
       else echo "${SHIM_OTHER:-in_progress null}"; fi ;; esac ;;
  "api --paginate") : ;;
esac
EOF
chmod +x "$S/shim/gh"
export SHIM_LOG="$S/gh.log" SHIM_REL="$S/releases" SHIM_PRS="$S/prs" SHIM_GREEN="$S/green"; : > "$SHIM_LOG"; : > "$SHIM_GREEN"
green() { local r; for r in "$@"; do git rev-parse "$r^{commit}" >> "$SHIM_GREEN"; done; } # mark commits' CI passed green

# ── a synthetic repo: the real script ────────────────────────────────────
W="$S/seed"; mkdir -p "$W/scripts" "$W/apps/studio/desktop" "$W/apps/session/desktop" "$W/crates/shared" "$W/.changes"
cp "$HERE/scripts/release.sh" "$W/scripts/"
printf 'Change files: cargo rail change add <crates> --bump <none|patch|minor|major> -m "<note>"\n' > "$W/.changes/README"
printf '[workspace]\nmembers = ["apps/*/desktop", "crates/shared"]\n\n[workspace.package]\nversion = "0.1.2"\nedition = "2021"\n' > "$W/Cargo.toml"
for a in studio session; do
  printf '[package]\nname = "%s-desktop"\nversion.workspace = true\n' "$a" > "$W/apps/$a/desktop/Cargo.toml"
done
printf '[package]\nname = "shared"\nversion = "0.1.0"\n' > "$W/crates/shared/Cargo.toml"
cat > "$W/Cargo.lock" <<'EOF'
# This file is automatically @generated by Cargo.
version = 4

[[package]]
name = "inventory"
version = "0.3.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "inventory"
version = "0.1.2"

[[package]]
name = "serde"
version = "0.1.2"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "session-desktop"
version = "0.1.2"
dependencies = [
 "inventory 0.1.2",
 "inventory 0.3.0",
 "serde",
]

[[package]]
name = "shared"
version = "0.1.0"

[[package]]
name = "studio-desktop"
version = "0.1.2"

[[patch.unused]]
name = "a-patch"
version = "1.0.0"
EOF
printf '# Changelog\n\n## [0.1.2] - 2026-10-21\n\n- the last committed section\n' > "$W/CHANGELOG.md"
git -C "$W" init -q -b main; git -C "$W" add -A; git -C "$W" commit -qm "initial import"
git -C "$W" tag -a v0.1.1 -m "studio 0.1.1"
echo "// 0.1.2" >> "$W/crates/shared/lib.rs"; git -C "$W" add -A; git -C "$W" commit -qm "chore(release): v0.1.2 (#3)"
git -C "$W" tag -a v0.1.2 -m "studio 0.1.2"
git -C "$W" tag v99.1.0-rc.1 -m "a suffixed tag never counts"
git init -q --bare -b main "$S/origin.git"
git -C "$W" push -q "$S/origin.git" main:main --tags
git clone -q "$S/origin.git" "$S/wc"; cd "$S/wc" || exit 1
R() { bash scripts/release.sh "$@"; }
# land <subject> <file> [<change file name> <front matter lines> <note>]: a squash-merged PR on <line> (default main)
land() {
  local subj="$1" f="$2" line="${LINE:-main}"
  git fetch -q origin; git switch -q --detach "origin/$line"
  mkdir -p "$(dirname "$f")"; echo "// $subj" >> "$f"; git add "$f"
  if [ $# -ge 5 ]; then printf -- '---\n%b---\n\n%b\n' "$4" "$5" > ".changes/$3.md"; git add ".changes/$3.md"; fi
  git commit -qm "$subj"; git push -q origin "HEAD:$line"; }
tagged() { git fetch -q --tags origin; git rev-parse -q --verify "refs/tags/$1^{commit}" 2>/dev/null; }

echo "=== CalVer: the next version"
export RELEASE_MONTH=26.10
eq "tags v0.1.1, v0.1.2 (and v99.1.0-rc.1) in October 2026" "$(R next 2>/dev/null)" 26.10.0
eq "the same in November" "$(RELEASE_MONTH=26.11 R next 2>/dev/null)" 26.11.0
eq "this month, from the real clock" "$(env -u RELEASE_MONTH bash scripts/release.sh next 2>/dev/null)" "$(date -u +%y).$((10#$(date -u +%m))).0"
expect_fail "RELEASE_MONTH must be YY.M" env RELEASE_MONTH=26.01 bash scripts/release.sh next

echo "=== the image tag: exactly the bare CalVer (argocd-image-updater's filter and order)"
eq "v26.10.4 -> 26.10.4" "$(R image-tag v26.10.4 2>/dev/null)" 26.10.4
eq "v26.12.10 -> 26.12.10" "$(R image-tag v26.12.10 2>/dev/null)" 26.12.10
for t in 26.10.4 v26.09.1 v26.10.04 v26.13.0 v26.0.1 v6.10.0 v26.10.4-rc.1 vfoo; do
  expect_fail "image-tag refuses '$t'" R image-tag "$t"
done

echo "=== change files since the last tag -> the draft notes"
land "feat(signal): offline drafts (#70)" crates/shared/a.rs added-drafts '"studio-desktop" = "minor"\n"shared" = "patch"\n' "Added offline drafts (signal)"
land "fix(session): reconnect backoff (#71)" crates/shared/b.rs fixed-backoff '"session-desktop" = "patch"\n' "Fixed reconnect backoff resetting after sleep (session)"
land "feat(shared)!: rename sync.peer (#72)" crates/shared/c.rs renamed-peer '"shared" = "major"\n' "Renamed config key \`sync.peer\` to \`sync.peers\` (session)\n\nMigration: run \`session migrate-config\`; no database migration."
land "ci: faster plan (#73)" .github/x.yml ci-plan '"shared" = "none"\n' "CI plans faster"
M_GOOD=$(git rev-parse origin/main)
land "docs: a typo (#74)" docs/typo.md
land "fix(signal): a late fix (#75)" crates/shared/late.rs late-fix '"studio-desktop" = "patch"\n' "Fixed a late bug (signal)"
M_HEAD=$(git rev-parse origin/main)
git switch -q --detach origin/main
expect_ok "plan" R plan; R plan > "$S/plan.out" 2>&1
expect_out "plan: next is v26.10.0" "$S/plan.out" 'next release: *v26\.10\.0'
expect_out "plan: five change files since v0.1.2" "$S/plan.out" 'change files since v0\.1\.2: *5'
expect_out "plan: the committed version is not the released one" "$S/plan.out" 'committed version: *0\.1\.2 \(not the released one'
R notes > "$S/n.md" 2>&1
expect_out "Breaking / Migration holds the major note" "$S/n.md" '^### Breaking / Migration'
expect_out "  ...with its Migration paragraph indented under it" "$S/n.md" '^  Migration: run `session migrate-config`'
expect_out "Added holds the minor note" "$S/n.md" '^- Added offline drafts \(signal\)$'
expect_out "Fixed and improved holds the patch notes" "$S/n.md" '^- Fixed reconnect backoff resetting after sleep \(session\)$'
expect_no  "the none note is skipped" "$S/n.md" 'CI plans faster'
[ "$(grep -n '^### ' "$S/n.md" | cut -d: -f2- | paste -sd'|' -)" = "### Breaking / Migration|### Added|### Fixed and improved" ] \
  && ok "groups in order: Breaking, Added, Fixed" || bad "group order: $(grep '^### ' "$S/n.md" | paste -sd'|' -)"
[ "$(grep -n '^- Fixed' "$S/n.md" | cut -d: -f2- | paste -sd'|' -)" = "- Fixed reconnect backoff resetting after sleep (session)|- Fixed a late bug (signal)" ] \
  && ok "entries in merge order" || bad "entry order: $(grep '^- Fixed' "$S/n.md" | paste -sd'|' -)"

echo "=== release: what it refuses"
expect_fail "no commit on main has a green CI passed" R release --yes
grep -q "has a green 'CI passed'" "$S/last.out" && ok "  ...saying so" || bad "  ...message"
SHIM_CHECK="completed failure" expect_fail "release <sha>: CI passed red" R release --yes "$M_HEAD"
grep -q "is 'completed failure', not green" "$S/last.out" && ok "  ...naming the check result" || bad "  ...message"
expect_fail "release <sha>: CI passed still running" R release --yes "$M_HEAD"
git switch -q -c feat/sta-9-thing origin/main; echo x > stray.txt; git add stray.txt; git commit -qm "feat: unmerged"; git push -q origin feat/sta-9-thing
STRAY=$(git rev-parse HEAD); green "$STRAY"
expect_fail "release <sha>: a commit on a feature branch (green, but not on main or release/*)" R release --yes "$STRAY"
grep -q 'is on neither origin/main nor an origin/release/v\* line' "$S/last.out" && ok "  ...for that reason" || bad "  ...refused for another reason"
expect_fail "release <sha>: not a commit" R release --yes 0123456789abcdef0123456789abcdef01234567
expect_fail "release: an unknown option" R release --force
eq "nothing was tagged" "$(git ls-remote --tags origin 'v26*' | wc -l | tr -d ' ')" 0

echo "=== release: main's newest green commit"
green "$M_GOOD"
SHIM_OTHER="completed cancelled" expect_ok "release --dry-run" R release --dry-run
cp "$S/last.out" "$S/dry.out"
expect_out "  ...picks the newest green commit, not main's head" "$S/dry.out" "^release v26\.10\.0: $(git rev-parse --short "$M_GOOD") ci: faster plan"
expect_out "  ...says why it skipped the newer ones" "$S/dry.out" "skipping $(git rev-parse --short "$M_HEAD") fix\(signal\): a late fix.*'completed cancelled'"
expect_out "  ...shows the notes" "$S/dry.out" '^- Added offline drafts'
expect_no  "  ...without the late fix, which is after it" "$S/dry.out" 'Fixed a late bug'
expect_out "  ...and tags nothing" "$S/dry.out" 'dry run: v26\.10\.0 not tagged'
eq "  ...nothing on origin" "$(git ls-remote --tags origin 'v26*' | wc -l | tr -d ' ')" 0
expect_ok "release (no terminal: no prompt)" R release
eq "v26.10.0 is on the newest green commit" "$(tagged v26.10.0)" "$M_GOOD"
eq "annotated" "$(git cat-file -t v26.10.0)" tag
eq "main's committed version did not move" "$(R version origin/main)" 0.1.2
eq "nothing was committed for it" "$(git rev-parse origin/main)" "$M_HEAD"
expect_fail "release again: nothing green since v26.10.0" R release --yes
expect_fail "release <the tagged sha>: already released" R release --yes "$M_GOOD"
grep -q 'already released as v26.10.0' "$S/last.out" && ok "  ...naming the tag" || bad "  ...message"
green "$M_GOOD~2"
expect_fail "release <an older sha>: behind v26.10.0" R release --yes "$M_GOOD~2"
grep -q 'is older than v26.10.0 on main' "$S/last.out" && ok "  ...saying so" || bad "  ...message"
green "$M_HEAD"
RELEASE_MONTH=26.9 expect_fail "release: a version not above the highest tag (26.9.0 after v26.10.0)" R release --yes
grep -q 'is not above the highest tag v26.10.0' "$S/last.out" && ok "  ...saying versions only increase" || bad "  ...message"
eq "  ...and tagged nothing" "$(git ls-remote --tags origin 'v26.9*' | wc -l | tr -d ' ')" 0

echo "=== the next release in the same month: notes since the previous tag only"
expect_ok "release" R release --yes
eq "v26.10.1 on main's head" "$(tagged v26.10.1)" "$M_HEAD"
R notes v26.10.1 > "$S/n1.md" 2>&1
expect_out "v26.10.1's notes: the late fix" "$S/n1.md" '^- Fixed a late bug \(signal\)$'
expect_no  "v26.10.1's notes: nothing v26.10.0 shipped" "$S/n1.md" 'offline drafts|reconnect backoff|sync\.peer'
R notes v26.10.0 > "$S/n0.md" 2>&1
expect_out "v26.10.0's notes are still its own" "$S/n0.md" '^- Added offline drafts'
expect_no  "  ...without the later fix" "$S/n0.md" 'Fixed a late bug'
eq "next after v26.10.0, v26.10.1 in October" "$(R next 2>/dev/null)" 26.10.2
land "chore: tooling only (#76)" crates/shared/t.rs tooling '"shared" = "none"\n' "Tooling"
R notes > "$S/nn.md" 2>&1
expect_out "draft with only a none change file: no user-visible changes" "$S/nn.md" '^No user-visible changes'

echo "=== stamp: the release version into a build's checkout (uncommitted)"
git switch -q --detach v26.10.1
expect_ok "stamp v26.10.1" R stamp v26.10.1
eq "  ...the workspace version" "$(R version)" 26.10.1
eq "  ...studio-desktop in Cargo.lock" "$(grep -A1 '^name = "studio-desktop"$' Cargo.lock | tail -1)" 'version = "26.10.1"'
eq "  ...session-desktop in Cargo.lock" "$(grep -A1 '^name = "session-desktop"$' Cargo.lock | tail -1)" 'version = "26.10.1"'
eq "  ...shared (its own version) did not move" "$(grep -A1 '^name = "shared"$' Cargo.lock | tail -1)" 'version = "0.1.0"'
eq "  ...serde (a registry crate) did not move" "$(grep -A1 '^name = "serde"$' Cargo.lock | tail -1)" 'version = "0.1.2"'
eq "  ...the inventory member (named like a registry crate)" "$(grep -A2 '^name = "inventory"$' Cargo.lock | grep -A1 -v source | grep '^version' | paste -sd' ' -)" 'version = "0.3.0" version = "26.10.1"'
eq "  ...its \"name version\" reference in a dependency list" "$(grep -c '^ "inventory 26.10.1",$' Cargo.lock)" 1
eq "  ...but not the registry crate's" "$(grep -c '^ "inventory 0.3.0",$' Cargo.lock)" 1
eq "  ...and no reference at the old version is left" "$(grep -c '"inventory 0.1.2"' Cargo.lock)" 0
eq "  ...nor did the unused patch" "$(grep -A1 '^name = "a-patch"$' Cargo.lock | tail -1)" 'version = "1.0.0"'
eq "  ...only Cargo.toml and Cargo.lock changed, uncommitted" "$(git status --porcelain | awk '{ print $2 }' | paste -sd' ' -)" "Cargo.lock Cargo.toml"
eq "  ...HEAD still carries the committed version" "$(R version HEAD)" 0.1.2
expect_ok "stamp again: a no-op" R stamp 26.10.1
expect_fail "stamp refuses a non-X.Y.Z" R stamp 26.10

echo "=== the tag guards (every tag-triggered job, after stamp)"
expect_ok "tag-guard on stamped v26.10.1" env GITHUB_REF=refs/tags/v26.10.1 bash scripts/release.sh tag-guard
expect_ok "tag-guard with GH_TOKEN: CI passed green" env GITHUB_REF=refs/tags/v26.10.1 GH_TOKEN=x bash scripts/release.sh tag-guard
SHIM_CHECK="completed failure" expect_fail "tag-guard with GH_TOKEN: red CI passed" env GITHUB_REF=refs/tags/v26.10.1 GH_TOKEN=x bash scripts/release.sh tag-guard
expect_fail "tag-guard: a branch ref" env GITHUB_REF=refs/heads/main bash scripts/release.sh tag-guard
expect_fail "tag-guard: the checkout is another tag's" env GITHUB_REF=refs/tags/v26.10.0 bash -c 'bash scripts/release.sh stamp v26.10.0 && bash scripts/release.sh tag-guard'
R stamp v26.10.1 2>/dev/null
export GITHUB_OUTPUT="$S/gho"; : > "$GITHUB_OUTPUT"
expect_ok "ios-resolve studio v26.10.1 (stamped)" R ios-resolve studio v26.10.1
expect_out "  marketing version output" "$GITHUB_OUTPUT" '^version=26\.10\.1$'
for t in v26.10 26.10.1 v26.10.1-rc.1 V26.10.1 studio-v26.10.1; do expect_fail "ios-resolve refuses '$t'" R ios-resolve studio "$t"; done
git checkout -q -- Cargo.toml Cargo.lock
expect_fail "tag-guard: not stamped (the committed version)" env GITHUB_REF=refs/tags/v26.10.1 bash scripts/release.sh tag-guard
grep -q "run 'release.sh stamp v26.10.1' first" "$S/last.out" && ok "  ...asking for the stamp" || bad "  ...message"
expect_fail "ios-resolve: not stamped" R ios-resolve studio v26.10.1
R stamp v26.10.1 2>/dev/null
sedi 's/^version.workspace = true/version = "0.0.2"/' apps/studio/desktop/Cargo.toml
expect_fail "ios-resolve: studio crate pinned off the workspace version" R ios-resolve studio v26.10.1
expect_fail "tag-guard: the same" env GITHUB_REF=refs/tags/v26.10.1 bash scripts/release.sh tag-guard
git checkout -q -- .
# a tag on a commit no release line has (the feature branch)
git tag v26.10.9 "$STRAY"; git switch -q --detach v26.10.9; R stamp v26.10.9 2>/dev/null
expect_fail "ios-resolve: a commit on neither main nor release/v*" R ios-resolve studio v26.10.9
grep -q 'is in neither origin/main nor an origin/release/v\* line' "$S/last.out" && ok "  ...for that reason" || bad "  ...refused for another reason"
expect_fail "tag-guard: the same stray tag" env GITHUB_REF=refs/tags/v26.10.9 bash scripts/release.sh tag-guard
git checkout -q -- .; git switch -q --detach v26.10.1; git tag -d v26.10.9 >/dev/null; unset GITHUB_OUTPUT

echo "=== publish + TestFlight record"
R stamp v26.10.1 2>/dev/null
expect_ok "publish v26.10.1 (the run's tag)" env GITHUB_REF=refs/tags/v26.10.1 bash scripts/release.sh publish
expect_out "release notes: the change files since v26.10.0" "$S/releases/v26.10.1.body" '^- Fixed a late bug'
expect_no  "release notes: nothing v26.10.0 shipped" "$S/releases/v26.10.1.body" 'offline drafts'
expect_out "release notes: on main" "$S/releases/v26.10.1.body" 'on main'
expect_out "release notes: the image" "$S/releases/v26.10.1.body" 'studio-web:26\.10\.1'
expect_out "release notes: the full diff" "$S/releases/v26.10.1.body" 'compare/v26\.10\.0\.\.\.v26\.10\.1'
expect_out "release notes: TestFlight block" "$S/releases/v26.10.1.body" '^- Studio .*pending'
eq "release title" "$(cat "$S/releases/v26.10.1.title")" "studio 26.10.1"
expect_ok "publish again: nothing to do" env GITHUB_REF=refs/tags/v26.10.1 bash scripts/release.sh publish
expect_ok "ios-release v26.10.0 (another tag, by argument)" R ios-release v26.10.0
expect_out "v26.10.0's Release: its own notes" "$S/releases/v26.10.0.body" '^### Breaking / Migration'
expect_fail "publish: no tag" R publish v26.10.8
export RECORD_SETTLE=0
expect_ok "ios-record studio" R ios-record studio v26.10.1 26.10.1 1760000001 https://github.com/x/y/actions/runs/1
expect_ok "ios-record studio again" R ios-record studio v26.10.1 26.10.1 1760000002
expect_out "notes: Studio's latest build" "$S/releases/v26.10.1.body" '^- Studio .*build 1760000002, '
expect_out "  ...naming the earlier one" "$S/releases/v26.10.1.body" '^- Studio .*earlier builds: 1760000001'
expect_fail "ios-record: marketing version != tag" R ios-record studio v26.10.1 26.10.2 1
git checkout -q -- .

echo "=== hotfix on release/v26.10.1, then main's next release"
land "feat(session): half-done mixer (#83)" crates/shared/e.rs added-mixer '"session-desktop" = "minor"\n' "Added the new mixer (session)"
land "fix(signal): crash on preset load (#84)" crates/shared/f.rs fixed-crash '"studio-desktop" = "patch"\n' "Fixed a crash when loading a preset (signal)"
FIX=$(git rev-parse origin/main)
land "feat(session): more unshippable work (#85)" crates/shared/g.rs added-more '"session-desktop" = "minor"\n' "Added more (session)"
git switch -q --detach origin/main
expect_fail "hotfix: a commit not on main is refused" R hotfix "$STRAY"
expect_ok "hotfix $FIX" R hotfix "$FIX"
HB="$(git rev-parse --abbrev-ref HEAD)"
eq "hotfix branch name" "$HB" "fix/no-issue-hotfix-$(git rev-parse --short "$FIX")"
eq "release/v26.10.1 created at the tag" "$(git rev-parse origin/release/v26.10.1)" "$(git rev-parse 'v26.10.1^{commit}')"
git log -1 --format=%B > "$S/pick.out"; expect_out "cherry-picked with -x" "$S/pick.out" "cherry picked from commit $FIX"
P=$(lastpr); expect_out "hotfix PR into release/v26.10.1" "$S/prs/$P.meta" '^base=release/v26\.10\.1$'
expect_out "hotfix PR title is the fix's" "$S/prs/$P.meta" '^title=fix\(signal\): crash on preset load$'
expect_out "hotfix PR body: then just release <sha>" "$S/prs/$P.body" 'just release <merge sha>'
# the hotfix PR squash-merged into the release line
git switch -q --detach origin/release/v26.10.1; git merge -q --squash "$HB"; git commit -qm "fix(signal): crash on preset load (#86)"; git push -q origin HEAD:release/v26.10.1
HOT=$(git rev-parse HEAD)
expect_fail "release <hotfix sha>: its CI passed not green yet" R release --yes "$HOT"
green "$HOT"
expect_ok "release <hotfix sha>" R release --yes "$HOT"
expect_out "  ...on the hotfix line" "$S/last.out" 'on release/v26\.10\.1'
eq "v26.10.2 (the next free number) on the hotfix commit" "$(tagged v26.10.2)" "$HOT"
git merge-base --is-ancestor v26.10.2 origin/main && bad "v26.10.2 is not on main" || ok "v26.10.2 is on the release line, not main"
R notes v26.10.2 > "$S/nh.md" 2>&1
expect_out "hotfix notes: the fix" "$S/nh.md" 'Fixed a crash when loading a preset'
expect_no  "hotfix notes: nothing unshipped from main" "$S/nh.md" 'mixer|Added more|Tooling'
git switch -q --detach v26.10.2; R stamp v26.10.2 2>/dev/null
expect_ok "tag-guard on v26.10.2 (release/v* line)" env GITHUB_REF=refs/tags/v26.10.2 GH_TOKEN=x bash scripts/release.sh tag-guard
expect_out "  ...naming the line" "$S/last.out" 'on release/v26\.10\.1'
git checkout -q -- .
eq "no forward-port PR (a release commits nothing)" "$(grep -l '^base=main$' "$S"/prs/*.meta 2>/dev/null | wc -l | tr -d ' ')" 0
# main's next release lists what the hotfix did not ship, and says it covers it
MAIN2=$(git rev-parse origin/main); green "$MAIN2"
expect_ok "release on main after the hotfix" R release --yes
eq "v26.10.3 on main" "$(tagged v26.10.3)" "$MAIN2"
R notes v26.10.3 > "$S/n3.md" 2>&1
expect_out "main's notes: include the hotfix" "$S/n3.md" '^Includes all changes up to v26\.10\.2\.$'
expect_out "main's notes: the mixer" "$S/n3.md" 'Added the new mixer'
expect_no  "main's notes: the hotfixed crash is not listed twice" "$S/n3.md" 'Fixed a crash'
R notes v26.10.2 > "$S/nh2.md" 2>&1
expect_out "the hotfix's notes, read after v26.10.3: still the fix" "$S/nh2.md" 'Fixed a crash when loading a preset'
expect_no  "  ...and no 'Includes' line from the later main release" "$S/nh2.md" '^Includes'
eq "next month, after v26.10.x" "$(RELEASE_MONTH=26.11 R next 2>/dev/null)" 26.11.0
expect_fail "release <old hotfix-line sha>: already released" R release --yes "$HOT"

echo "=== Release guard: there are no release PRs"
expect_ok "guard passes" R guard
grep -q 'no release PRs' "$S/last.out" && ok "  ...and says why" || bad "  ...message"
expect_rc "release-only: never a release PR" 1 R release-only

echo "=== an unreachable origin fails, never passes"
git remote set-url origin "$S/nowhere.git"
green HEAD
expect_fail "release with origin unreachable" R release --yes
expect_fail "tag-guard with origin unreachable" env GITHUB_REF=refs/tags/v26.10.3 bash -c 'git switch -q --detach v26.10.3 && bash scripts/release.sh stamp v26.10.3 && bash scripts/release.sh tag-guard'
git checkout -q -- .; git remote set-url origin "$S/origin.git"

echo; echo "=== RESULT: $pass passed, $failn failed"
[ "$failn" = 0 ]
