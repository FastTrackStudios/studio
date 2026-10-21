#!/usr/bin/env bash
# Sandbox test of scripts/release.sh: the ONE studio release (release/vX.Y.Z, tag
# vX.Y.Z, one GitHub Release) that drives both iOS TestFlight uploads (Signal +
# Session), end to end, on a synthetic repo and a local bare "origin". Nothing
# leaves the sandbox: cargo and gh are shims (gh keeps its releases as files).
# Run from anywhere:
#
#   scripts/release-sandbox.sh [SCRATCH_DIR]
#
# Needs git, git-cliff and jq on PATH. Exits non-zero if any check fails.
# shellcheck disable=SC2015,SC2016 # `[ test ] && ok || bad` is the idiom here: ok never fails; the ERE patterns hold literal backticks
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
S="${1:-${TMPDIR:-/tmp}/release-sandbox}"
REPO=FastTrackStudios/studio
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=t@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=t@example.invalid
rm -rf "$S"; mkdir -p "$S/shim" "$S/releases"
export PATH="$S/shim:$PATH"
export GITHUB_REPOSITORY="$REPO"
pass=0; failn=0
ok()  { echo "PASS: $*"; pass=$((pass+1)); }
bad() { echo "FAIL: $*"; failn=$((failn+1)); }
expect_ok()   { local d="$1"; shift; if "$@" >"$S/last.out" 2>&1; then ok "$d"; else bad "$d"; cat "$S/last.out"; fi; }
expect_fail() { local d="$1"; shift; if "$@" >"$S/last.out" 2>&1; then bad "$d (unexpected pass)"; cat "$S/last.out"; else ok "$d"; grep -h '::error::\|release:' "$S/last.out" | head -2 | sed 's/^/      /'; fi; }
# expect_out <description> <file> <ERE>: the file matches
expect_out()  { if grep -qiE "$3" "$2"; then ok "$1"; else bad "$1 (no match for: $3)"; sed 's/^/      | /' "$2" | head -20; fi; }
expect_no()   { if grep -qiE "$3" "$2"; then bad "$1 (found: $3)"; else ok "$1"; fi; }

# ── shims ────────────────────────────────────────────────────────────────
cat > "$S/shim/cargo" <<'EOF'
#!/bin/sh
echo "[shim] cargo $*" >&2
EOF
cat > "$S/shim/gh" <<'EOF'
#!/bin/bash
# A tiny gh: releases are files under $SHIM_REL, calls are logged to $SHIM_LOG.
echo "[shim] gh $*" >> "$SHIM_LOG"
arg() { local k="$1"; shift; while [ $# -gt 0 ]; do [ "$1" = "$k" ] && { echo "$2"; return; }; shift; done; }
case "$1 $2" in
  "release view") [ -f "$SHIM_REL/$3.body" ] || exit 1
     case "$*" in *"--json body"*) b="$(cat "$SHIM_REL/$3.body")"; sleep "${SHIM_VIEW_DELAY:-0}"; printf '%s\n' "$b" ;; esac ;;
  "release create") tag="$3"; shift 3
     tgt="$(arg --target "$@")"; nf="$(arg --notes-file "$@")"
     cp "$nf" "$SHIM_REL/$tag.body"; arg --title "$@" > "$SHIM_REL/$tag.title"
     case " $* " in *" --latest=false "*) echo false > "$SHIM_REL/$tag.latest" ;; *) echo true > "$SHIM_REL/$tag.latest" ;; esac
     git rev-parse -q --verify "refs/tags/$tag" >/dev/null || { git tag "$tag" "$tgt" && git push -q origin "$tag"; } ;;
  "release edit") tag="$3"; shift 3; cp "$(arg --notes-file "$@")" "$SHIM_REL/$tag.body" ;;
  "pr list") exit 0 ;;
  "pr create") shift 2; cp "$(arg --body-file "$@")" "$SHIM_PRBODY"; echo "https://github.com/x/y/pull/9999" ;;
  "pr merge"|"workflow run") : ;;
esac
EOF
chmod +x "$S/shim/cargo" "$S/shim/gh"
export SHIM_LOG="$S/gh.log" SHIM_REL="$S/releases" SHIM_PRBODY="$S/pr.body"; : > "$SHIM_LOG"

# ── a synthetic repo: the real tooling, the real cliff.toml ──────────────
W="$S/seed"; mkdir -p "$W/scripts" "$W/apps/signal/desktop" "$W/apps/session/desktop" "$W/features/signal" "$W/features/session" "$W/crates/shared"
cp "$HERE/scripts/release.sh" "$W/scripts/"; cp "$HERE/cliff.toml" "$W/"
printf '[workspace.package]\nversion = "0.1.0"\n' > "$W/Cargo.toml"
for a in signal session; do
  printf '[package]\nname = "%s-desktop"\nversion.workspace = true\n\n[dependencies]\nserde = { version = "1" }\n' "$a" > "$W/apps/$a/desktop/Cargo.toml"
  echo "// $a" > "$W/features/$a/lib.rs"
done
echo "// shared" > "$W/crates/shared/lib.rs"; echo "# lock" > "$W/Cargo.lock"
git -C "$W" init -q -b main; git -C "$W" add -A; git -C "$W" commit -qm "initial import"
git init -q --bare -b main "$S/origin.git"
git -C "$W" push -q "$S/origin.git" main:main main:dev
git clone -q "$S/origin.git" "$S/wc"; cd "$S/wc" || exit 1
R() { bash scripts/release.sh "$@"; }
git_ver() { sed -n '/^\[package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' "$1" | head -1; }
guard() { BASE_SHA="$(git rev-parse origin/main)" HEAD_SHA="$1" HEAD_REF="$2" GITHUB_STEP_SUMMARY="$S/summary.md" bash scripts/release.sh guard; }
card() { # <file> <subject>: a commit on dev, wherever the checkout is
  local here; here="$(git rev-parse --abbrev-ref HEAD)"; [ "$here" = HEAD ] && here="$(git rev-parse HEAD)"
  git fetch -q origin; git switch -q --detach origin/dev
  mkdir -p "$(dirname "$1")"; echo "// $2" >> "$1"; git add "$1"; git commit -qm "$2"; git push -q origin HEAD:dev
  git switch -q "$here" 2>/dev/null || git switch -q --detach "$here"; }
ship() { # <sha> <tag>: merge commit on main, publish, back-merge, and the back-merge lands on dev
  git switch -q --detach origin/main; git merge -q --no-ff -m "Merge pull request #1 from $REPO/release/$2" "$1"; git push -q origin HEAD:main
  R publish >"$S/publish.out" 2>&1; R backmerge >"$S/backmerge.out" 2>&1; git fetch -q --tags origin; git switch -q --detach origin/dev
  git merge -q --no-ff -m "Merge main into dev after $2" "origin/backmerge/$2" && git push -q origin HEAD:dev; }
dev() { git fetch -q --tags origin; git switch -q --detach origin/dev; }

echo "=== one release: plan, prepare, the release PR"
card tc_A.txt "feat: card A before the cut (#2)"; dev
expect_ok "plan" R plan; R plan > "$S/plan.out" 2>&1
expect_out "plan: next is 0.1.1" "$S/plan.out" 'next release: *0\.1\.1'
expect_out "plan: one tag and Release for both apps" "$S/plan.out" 'one tag v0\.1\.1 and one GitHub Release for Signal \+ Session'
expect_out "plan: signal crate at the workspace version" "$S/plan.out" 'signal crate version: *0\.1\.0'
expect_no  "plan: crates agree with the workspace" "$S/plan.out" 'differs'
expect_fail "prepare signal: there is no per-app release any more" R prepare signal
expect_fail "prepare session: there is no per-app release any more" R prepare session
expect_fail "prepare 0.1.0.1: not X.Y.Z" R prepare 0.1.0.1
expect_ok "prepare (patch default)" R prepare
[ "$(git rev-parse --abbrev-ref HEAD)" = release/v0.1.1 ] && ok "on release/v0.1.1" || bad "branch is $(git rev-parse --abbrev-ref HEAD)"
[ "$(R version)" = 0.1.1 ] && ok "workspace version 0.1.1" || bad "workspace version $(R version)"
for a in signal session; do
  grep -q '^version.workspace = true' "apps/$a/desktop/Cargo.toml" && ok "$a crate still follows the workspace version" || bad "$a crate pinned"
done
[ "$(git log -1 --format=%s)" = "chore(release): v0.1.1" ] && ok "release commit subject" || bad "subject: $(git log -1 --format=%s)"
git show --stat --format= HEAD > "$S/stat.out"
expect_out "release commit holds Cargo.toml, Cargo.lock and the root CHANGELOG.md" "$S/stat.out" ' CHANGELOG.md'
expect_no  "no per-app changelog anywhere" <(git ls-files) 'apps/.*CHANGELOG'
R notes 0.1.1 > "$S/n.md"; expect_out "one changelog section lists card A" "$S/n.md" 'card A'
card docs/late.md "docs: a doc card after the cut (#4)"
expect_ok "release-pr" R pr; REL=$(git rev-parse HEAD)
expect_out "PR body: one tag, one Release" "$S/pr.body" 'tags `v0\.1\.1`'
expect_out "PR body: both TestFlight uploads" "$S/pr.body" '`signal-ios`.*`session-ios`|BOTH iOS TestFlight'
expect_out "PR body: marketing version" "$S/pr.body" 'marketing version \*\*0\.1\.1\*\*'
git push -q origin release/v0.1.1

echo "=== the release guard"
expect_ok "guard passes (dev moved on)" guard "$REL" release/v0.1.1
expect_fail "guard: dev -> main refused" guard "$REL" dev
expect_fail "guard: name/version mismatch" guard "$REL" release/v0.1.9
expect_fail "guard: a per-app release branch is refused" guard "$REL" release/signal-v0.1.1
expect_fail "guard: a per-app release branch is refused (session)" guard "$REL" release/session-v0.1.1
echo fix > tc_fix.txt; git add tc_fix.txt; git commit -qm "fix(signal): on the release branch (#5)"
expect_fail "guard: a fix landed after the release commit" guard "$(git rev-parse HEAD)" release/v0.1.1
expect_ok "refresh" R refresh; git push -q origin release/v0.1.1; REL=$(git rev-parse HEAD)
expect_ok "guard: after refresh" guard "$REL" release/v0.1.1
# an app crate pinned to another version
git switch -q -c t/pin; sed -i 's/^version.workspace = true/version = "0.0.2-alpha"/' apps/signal/desktop/Cargo.toml; git commit -qam "chore(release): v0.1.1"; PIN=$(git rev-parse HEAD)
expect_fail "guard: signal crate pinned to another version" guard "$PIN" release/v0.1.1
grep -q 'signal/desktop/Cargo.toml resolves to version .0.0.2-alpha. but the release is 0.1.1' "$S/last.out" && ok "  ...naming the crate and both versions" || bad "  ...message"
git switch -q --detach "$REL"
git switch -q -c t/pin2; sed -i 's/^version.workspace = true/version = "0.1.0"/' apps/session/desktop/Cargo.toml; git commit -qam "chore(release): v0.1.1"; PIN2=$(git rev-parse HEAD)
expect_fail "guard: session crate pinned to another version" guard "$PIN2" release/v0.1.1
git switch -q --detach "$REL"
# a crate that spells the same version out is fine; so is the table form
git switch -q -c t/lit; sed -i 's/^version.workspace = true/version = "0.1.1"/' apps/signal/desktop/Cargo.toml
sed -i 's/^version.workspace = true/version = { workspace = true }/' apps/session/desktop/Cargo.toml; git commit -qam "chore(release): v0.1.1"; LIT=$(git rev-parse HEAD)
expect_ok "guard: a literal equal version and the table form both resolve to 0.1.1" guard "$LIT" release/v0.1.1
git switch -q --detach "$REL"
# no changelog section
git switch -q -c t/nocl; git rm -q CHANGELOG.md; git commit -qm "chore(release): v0.1.1"; NC=$(git rev-parse HEAD)
expect_fail "guard: changelog section missing" guard "$NC" release/v0.1.1
git switch -q --detach "$REL"
# version not moved from main's
git switch -q -c t/nomove; git checkout -q origin/main -- Cargo.toml; git commit -qm "chore(release): v0.1.1"; NM=$(git rev-parse HEAD)
expect_fail "guard: version not moved past main's" guard "$NM" release/v0.1.1
git switch -q --detach "$REL"

echo "=== merge: ONE tag, ONE GitHub Release, BOTH TestFlight workflows dispatched"
ship "$REL" v0.1.1
[ "$(git rev-parse 'v0.1.1^{commit}')" = "$REL" ] && ok "v0.1.1 on the release tip" || bad "v0.1.1 target"
[ "$(git tag --list | paste -sd, -)" = v0.1.1 ] && ok "v0.1.1 is the only tag (no per-app tags)" || bad "tags: $(git tag --list | paste -sd, -)"
rels=("$S"/releases/*.body); [ "${#rels[@]}" = 1 ] && ok "exactly one GitHub Release" || bad "releases: ${rels[*]}"
[ "$(cat "$S/releases/v0.1.1.title")" = "studio 0.1.1" ] && ok "release title: studio 0.1.1" || bad "title: $(cat "$S/releases/v0.1.1.title")"
[ "$(cat "$S/releases/v0.1.1.latest")" = true ] && ok "it is the latest release" || bad "latest flag"
expect_out "release notes: the changelog section" "$S/releases/v0.1.1.body" 'card A'
expect_out "release notes: TestFlight block" "$S/releases/v0.1.1.body" 'testflight:begin'
expect_out "release notes: Signal line pending" "$S/releases/v0.1.1.body" '^- Signal \(`app.fasttrackstudio.signal`\): pending'
expect_out "release notes: Session line pending" "$S/releases/v0.1.1.body" '^- Session \(`app.fasttrackstudio.session`\): pending'
expect_out "release notes: marketing version = tag version" "$S/releases/v0.1.1.body" 'Marketing version \*\*0\.1\.1\*\*'
expect_out "publish dispatched reaper on the tag" "$SHIM_LOG" 'workflow run reaper.yml .*--ref v0.1.1'
expect_out "publish dispatched signal-ios.yml on the one tag" "$SHIM_LOG" 'workflow run signal-ios.yml .*--ref v0.1.1 -f tag=v0.1.1'
expect_out "publish dispatched session-ios.yml on the same tag" "$SHIM_LOG" 'workflow run session-ios.yml .*--ref v0.1.1 -f tag=v0.1.1'
grep -q "backmerge/v0.1.1" "$SHIM_LOG" && ok "back-merge branch backmerge/v0.1.1" || bad "backmerge branch name"
expect_ok "publish is idempotent" R publish
[ "$(grep -c 'release create v0.1.1' "$SHIM_LOG")" = 1 ] && ok "no second release" || bad "second release created"
[ "$(grep -c 'workflow run signal-ios.yml' "$SHIM_LOG")" = 1 ] && [ "$(grep -c 'workflow run session-ios.yml' "$SHIM_LOG")" = 1 ] && ok "no second dispatch" || bad "dispatched twice"
[ -e tc_fix.txt ] && ok "the branch fix reached dev" || bad "fix not on dev"
card tc_B.txt "docs: card B (#3)"

echo "=== the TestFlight gate (ios-resolve), for each app"
git fetch -q --tags origin; git switch -q --detach v0.1.1
export GITHUB_OUTPUT="$S/gho"; : > "$GITHUB_OUTPUT"
for a in signal session; do
  : > "$GITHUB_OUTPUT"
  expect_ok "resolve $a v0.1.1" R ios-resolve "$a" v0.1.1
  expect_out "  $a: marketing version output" "$GITHUB_OUTPUT" '^version=0\.1\.1$'
  expect_out "  $a: sha output" "$GITHUB_OUTPUT" "^sha=$(git rev-parse HEAD)$"
  expect_out "  $a: tag output" "$GITHUB_OUTPUT" '^tag=v0\.1\.1$'
done
for t in v0.1 0.1.1 v0.1.1-rc1 v0.1.1.1 V0.1.1 signal-v0.1.1 session-v0.1.1 v01.1.1x ""; do
  expect_fail "resolve refuses tag '$t'" R ios-resolve signal "$t"
done
expect_fail "resolve: unknown app" R ios-resolve studio v0.1.1
expect_fail "resolve: a tag whose version differs from the crates (v0.1.5 on the 0.1.1 commit)" R ios-resolve signal v0.1.5
grep -q "says version '0.1.1' but the tag is v0.1.5" "$S/last.out" && ok "  ...naming both versions" || bad "  ...message"
# a crate that stopped following the workspace
git switch -q -c t/rpin; sed -i 's/^version.workspace = true/version = "0.0.2-alpha"/' apps/session/desktop/Cargo.toml; git commit -qam "pin"
expect_fail "resolve: session crate pinned to another version than the tag" R ios-resolve session v0.1.1
grep -q "session/desktop/Cargo.toml says version '0.0.2-alpha'" "$S/last.out" && ok "  ...naming the crate" || bad "  ...message"
git switch -q --detach v0.1.1; git branch -qD t/rpin
# the crates say the tag's version outright, the workspace version differs
git switch -q -c t/ws; sed -i 's/^version = "0.1.1"/version = "0.1.2"/' Cargo.toml
sed -i 's/^version.workspace = true/version = "0.1.1"/' apps/signal/desktop/Cargo.toml; git commit -qam "ws"
expect_fail "resolve: workspace version != tag" R ios-resolve signal v0.1.1
grep -q "workspace.package. version is '0.1.2' but the tag is v0.1.1" "$S/last.out" && ok "  ...naming the workspace version" || bad "  ...message"
git switch -q --detach v0.1.1; git branch -qD t/ws
# a tag whose crates, workspace and changelog are all right, on a commit main does not have
git switch -q --detach origin/dev
sed -i 's/^version = "0.1.1"/version = "0.7.7"/' Cargo.toml
printf '## [0.7.7] - 2026-10-21\n\n- x\n' >> CHANGELOG.md
git commit -qam "chore(release): v0.7.7"; git tag v0.7.7 HEAD
expect_fail "resolve: a tag that is not in main is refused" R ios-resolve session v0.7.7
grep -q 'is not in origin/main' "$S/last.out" && ok "  ...for being outside main" || bad "  ...refused for another reason"
git tag -d v0.7.7 >/dev/null; git reset -q --hard
git switch -q --detach "$REL"; git rm -q --cached CHANGELOG.md; mv CHANGELOG.md "$S/cl.bak"
expect_fail "resolve: no changelog section" R ios-resolve signal v0.1.1
grep -q 'has no \[0.1.1\] section' "$S/last.out" && ok "  ...for the missing section" || bad "  ...refused for another reason"
mv "$S/cl.bak" CHANGELOG.md; git reset -q --hard "$REL"
git fetch -q origin; git switch -q --detach origin/main
expect_fail "resolve: main's tip has the right versions but is not on the tag" R ios-resolve signal v0.1.1

echo "=== a tag pushed by hand has no Release: ios-release creates it once, from either workflow"
git switch -q --detach v0.1.1
mkdir -p "$S/kept"; mv "$S/releases"/v0.1.1.* "$S/kept/"
n0=$(grep -c 'workflow run' "$SHIM_LOG")
expect_ok "ios-release v0.1.1 (signal-ios first)" R ios-release v0.1.1
expect_out "  the Release exists again, with the pending block" "$S/releases/v0.1.1.body" '^- Signal .*pending'
expect_ok "ios-release v0.1.1 (session-ios second)" R ios-release v0.1.1
[ "$(grep -c 'release create v0.1.1' "$SHIM_LOG")" = 2 ] && ok "  created once (a second ios-release finds it), not twice" || bad "  create count $(grep -c 'release create v0.1.1' "$SHIM_LOG")"
[ "$(grep -c 'workflow run' "$SHIM_LOG")" = "$n0" ] && ok "  and it dispatched nothing (the hand-pushed tag started both workflows itself)" || bad "  ios-release dispatched workflows"
expect_fail "ios-release: not a tag" R ios-release 0.1.1
rm -f "$S/releases"/v0.1.1.*; mv "$S/kept"/v0.1.1.* "$S/releases/"

echo "=== both workflows record into the same Release"
export RECORD_SETTLE=1
expect_ok "ios-record signal" R ios-record signal v0.1.1 0.1.1 1760000001 https://github.com/x/y/actions/runs/1
B="$S/releases/v0.1.1.body"
expect_out "notes: Signal line with its build and run" "$B" '^- Signal \(`app.fasttrackstudio.signal`\): build 1760000001, [0-9TZ:-]+, \[run\]\(https://github.com/x/y/actions/runs/1\)$'
expect_out "notes: Session still pending" "$B" '^- Session .*pending'
expect_ok "ios-record session" R ios-record session v0.1.1 0.1.1 1760000002 https://github.com/x/y/actions/runs/2
expect_out "notes: Session line" "$B" '^- Session \(`app.fasttrackstudio.session`\): build 1760000002, '
expect_out "notes: Signal line kept" "$B" '^- Signal \(`app.fasttrackstudio.signal`\): build 1760000001, '
expect_no  "notes: nothing pending any more" "$B" 'pending'
expect_out "notes: marketing version" "$B" 'Marketing version \*\*0\.1\.1\*\*'
expect_out "notes: the changelog is still there" "$B" 'card A'
[ "$(grep -c 'testflight:begin' "$B")" = 1 ] && ok "one TestFlight block" || bad "block duplicated"
[ "$(grep -c '^- \(Signal\|Session\) ' "$B")" = 2 ] && ok "one line per app" || bad "lines: $(grep -c '^- \(Signal\|Session\) ' "$B")"
expect_ok "ios-record signal again (a re-run, a new build)" R ios-record signal v0.1.1 0.1.1 1760000999
[ "$(grep -c '^- Signal ' "$B")" = 1 ] && ok "still one Signal line" || bad "Signal lines: $(grep -c '^- Signal ' "$B")"
expect_out "notes: the new build leads, the old one is listed" "$B" '^- Signal .*build 1760000999, .*\(earlier builds: 1760000001\)$'
expect_out "notes: Session untouched by Signal's re-run" "$B" '^- Session .*build 1760000002, '
expect_ok "ios-record signal, the same build twice" R ios-record signal v0.1.1 0.1.1 1760000999
[ "$(grep -c '1760000999' "$B")" = 1 ] && ok "no duplicate of the same build" || bad "build listed $(grep -c '1760000999' "$B") times"
expect_fail "ios-record: marketing version != tag version" R ios-record signal v0.1.1 0.1.2 1760000002
expect_fail "ios-record: build number not a number" R ios-record signal v0.1.1 0.1.1 12ab
expect_fail "ios-record: no release for the tag" R ios-record signal v0.9.9 0.9.9 1760000003
expect_fail "ios-record: unknown app" R ios-record studio v0.1.1 0.1.1 1760000004
expect_fail "ios-record: a per-app tag" R ios-record signal signal-v0.1.1 0.1.1 1760000004

echo "=== both record at once (each edit replaces the whole body)"
dev
card features/signal/c0.rs "fix(signal): card C (#12)"; dev
expect_ok "prepare (v0.1.2)" R prepare; git push -q origin release/v0.1.2; CREL=$(git rev-parse HEAD)
expect_ok "guard: v0.1.2" guard "$CREL" release/v0.1.2
ship "$CREL" v0.1.2
C="$S/releases/v0.1.2.body"
n0=$(grep -c 'release edit v0.1.2' "$SHIM_LOG")
export SHIM_VIEW_DELAY=1 RECORD_SETTLE=2
R ios-record signal v0.1.2 0.1.2 1760001111 https://github.com/x/y/actions/runs/3 > "$S/rec1.out" 2>&1 & p1=$!
R ios-record session v0.1.2 0.1.2 1760002222 https://github.com/x/y/actions/runs/4 > "$S/rec2.out" 2>&1 & p2=$!
wait $p1; r1=$?; wait $p2; r2=$?
unset SHIM_VIEW_DELAY; export RECORD_SETTLE=1
[ "$r1" = 0 ] && [ "$r2" = 0 ] && ok "both records exit 0" || { bad "exit codes $r1 $r2"; cat "$S/rec1.out" "$S/rec2.out"; }
expect_out "concurrent: Signal's line survived" "$C" '^- Signal .*build 1760001111, '
expect_out "concurrent: Session's line survived" "$C" '^- Session .*build 1760002222, '
expect_no  "concurrent: nothing pending" "$C" 'pending'
[ "$(grep -c 'testflight:begin' "$C")" = 1 ] && ok "concurrent: one TestFlight block" || bad "block count"
[ "$(( $(grep -c 'release edit v0.1.2' "$SHIM_LOG") - n0 ))" -ge 3 ] && ok "one run had to go round again after the other overwrote it" || bad "edits: $(( $(grep -c 'release edit v0.1.2' "$SHIM_LOG") - n0 ))"
cat "$S/rec1.out" "$S/rec2.out" | grep -q 'trying again' && ok "  ...and said so" || bad "  ...no retry message"
RECORD_TRIES=1 SHIM_VIEW_DELAY=0 expect_ok "a record whose line is already there needs no edit" R ios-record signal v0.1.2 0.1.2 1760001111

echo "=== 0.1.9 -> 0.1.10 -> 0.1.11 (the patch never rolls over)"
dev
card features/signal/a.rs "fix(signal): card for 0.1.9 (#9)"; dev
expect_ok "prepare 0.1.9 (explicit)" R prepare 0.1.9; git push -q origin release/v0.1.9; ship "$(git rev-parse HEAD)" v0.1.9
card features/session/b.rs "fix(session): card F (#10)"; dev
expect_ok "prepare" R prepare; [ "$(R version)" = 0.1.10 ] && ok "0.1.10 (not 0.2.0)" || bad "got $(R version)"
git push -q origin release/v0.1.10; ship "$(git rev-parse HEAD)" v0.1.10
card features/signal/c.rs "fix(signal): card G (#11)"; dev
expect_ok "prepare" R prepare; [ "$(R version)" = 0.1.11 ] && ok "0.1.11" || bad "got $(R version)"
[ "$(git tag --list 'v*' --sort=-v:refname | head -1)" = v0.1.10 ] && ok "newest tag by sort -V is v0.1.10" || bad "tag order"
for a in signal session; do
  [ "$(git show "HEAD:apps/$a/desktop/Cargo.toml" | grep -c '^version.workspace = true')" = 1 ] && ok "$a crate follows 0.1.11 with no edit of its own" || bad "$a crate changed"
done
R notes 0.1.11 > "$S/n.md"; expect_no "[0.1.11] repeats nothing of 0.1.10" "$S/n.md" 'card F'
expect_out "[0.1.11] has card G" "$S/n.md" 'card G'
expect_fail "prepare while a release/v0.1.10 exists would be a duplicate tag" R prepare 0.1.10
git switch -q --detach origin/dev
expect_fail "prepare refuses a version that is not greater" R prepare 0.1.3
[ "$(git tag --list | grep -vc '^v[0-9]')" = 0 ] && ok "every tag in the sandbox is a vX.Y.Z tag" || bad "stray tags: $(git tag --list | grep -v '^v[0-9]' | paste -sd, -)"

echo; echo "=== RESULT: $pass passed, $failn failed"
[ "$failn" = 0 ]
