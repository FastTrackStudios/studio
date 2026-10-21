#!/usr/bin/env bash
# Sandbox test of scripts/release.sh: the repo's release/vX.Y.Z flow AND the
# per-app (signal, session) flow, end to end, on a synthetic repo and a local
# bare "origin". Nothing leaves the sandbox: cargo and gh are shims (gh keeps
# its releases as files). Run from anywhere:
#
#   scripts/release-sandbox.sh [SCRATCH_DIR]
#
# Needs git, git-cliff and jq on PATH. Exits non-zero if any check fails.
# shellcheck disable=SC2015 # `[ test ] && ok || bad` is the idiom here: ok never fails
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
     case "$*" in *"--json body"*) cat "$SHIM_REL/$3.body" ;; esac ;;
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
  printf '[package]\nname = "%s-desktop"\nversion = "0.0.2-alpha"\n\n[dependencies]\nserde = { version = "1" }\n' "$a" > "$W/apps/$a/desktop/Cargo.toml"
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

echo "=== the repo's own release still works (regression)"
card tc_A.txt "feat: card A before the cut (#2)"; dev
expect_ok "prepare (patch default)" R prepare
[ "$(git rev-parse --abbrev-ref HEAD)" = release/v0.1.1 ] && ok "on release/v0.1.1" || bad "branch is $(git rev-parse --abbrev-ref HEAD)"
[ "$(git_ver Cargo.toml)" = "" ] && [ "$(R version)" = 0.1.1 ] && ok "workspace version 0.1.1" || bad "workspace version $(R version)"
[ "$(git_ver apps/signal/desktop/Cargo.toml)" = 0.0.2-alpha ] && ok "a repo release leaves the app versions alone" || bad "app version moved"
expect_ok "release-pr" R pr; REL=$(git rev-parse HEAD)
expect_ok "guard passes" guard "$REL" release/v0.1.1
expect_fail "guard: dev -> main refused" guard "$REL" dev
expect_fail "guard: name/version mismatch" guard "$REL" release/v0.1.9
expect_out "repo PR body: iOS apps are not uploaded by a repo release" "$S/pr.body" 'NOT uploaded'
git push -q origin release/v0.1.1
ship "$REL" v0.1.1
[ "$(git rev-parse 'v0.1.1^{commit}')" = "$REL" ] && ok "v0.1.1 on the release tip" || bad "v0.1.1 target"
[ -f "$S/releases/v0.1.1.body" ] && ok "v0.1.1 GitHub Release" || bad "no v0.1.1 release"
[ "$(cat "$S/releases/v0.1.1.latest")" = true ] && ok "the repo's release is the latest" || bad "latest flag"
expect_no "repo publish made no app release" <(ls "$S/releases") 'signal-v|session-v'
[ "$(R backmerge-label 2>/dev/null)" != "" ] && ok "backmerge-label works" || bad "backmerge-label"
grep -q "backmerge/v0.1.1" "$S/gh.log" && ok "back-merge branch backmerge/v0.1.1" || bad "backmerge branch name"
card tc_B.txt "docs: card B (#3)"; dev

echo "=== per-app: plan, prepare (the baseline), refusals"
expect_ok "plan signal" R plan signal; R plan signal > "$S/plan.out" 2>&1
expect_out "plan signal: first release is the baseline" "$S/plan.out" 'next signal release:.*0\.1\.0'
expect_fail "prepare: unknown app" R prepare studio
expect_fail "prepare signal minor: refused before a first tag" R prepare signal minor
expect_fail "prepare signal auto: refused" R prepare signal auto
expect_fail "prepare signal 0.1.0.1: not X.Y.Z" R prepare signal 0.1.0.1
expect_ok "prepare signal (baseline 0.1.0)" R prepare signal
[ "$(git rev-parse --abbrev-ref HEAD)" = release/signal-v0.1.0 ] && ok "on release/signal-v0.1.0" || bad "branch is $(git rev-parse --abbrev-ref HEAD)"
[ "$(git_ver apps/signal/desktop/Cargo.toml)" = 0.1.0 ] && ok "signal crate version 0.1.0" || bad "signal crate version $(git_ver apps/signal/desktop/Cargo.toml)"
[ "$(git_ver apps/session/desktop/Cargo.toml)" = 0.0.2-alpha ] && ok "session crate untouched" || bad "session crate moved"
[ "$(R version)" = 0.1.1 ] && ok "workspace version untouched" || bad "workspace version moved"
[ "$(git log -1 --format=%s)" = "chore(release): signal-v0.1.0" ] && ok "release commit subject" || bad "subject: $(git log -1 --format=%s)"
R notes signal 0.1.0 > "$S/n.md"; expect_out "signal changelog: baseline line" "$S/n.md" 'First tagged release of Signal'
git show --stat --format= HEAD > "$S/stat.out"
expect_out "release commit holds Cargo.toml, Cargo.lock, apps/signal/CHANGELOG.md" "$S/stat.out" 'apps/signal/CHANGELOG.md'
expect_no "release commit does not write the repo's CHANGELOG.md" "$S/stat.out" ' CHANGELOG.md'
card docs/late.md "docs: a doc card after the cut (#4)"
expect_ok "release-pr" R pr; SREL=$(git rev-parse HEAD)
grep "pr create" "$S/gh.log" | tail -1 | grep -q "Release signal-v0.1.0" && ok "PR title: Release signal-v0.1.0" || bad "PR title"
expect_out "PR body: tag, GitHub Release, TestFlight at the tag's version" "$S/pr.body" 'TestFlight upload, marketing version \*\*0\.1\.0\*\*|marketing version \*\*0\.1\.0\*\*'
git push -q origin release/signal-v0.1.0

echo "=== per-app: the release guard"
expect_ok "guard: signal release passes (dev moved on)" guard "$SREL" release/signal-v0.1.0
expect_fail "guard: wrong branch name for the version" guard "$SREL" release/signal-v0.1.5
expect_fail "guard: dev -> main refused" guard "$SREL" dev
expect_fail "guard: session ref for a signal release" guard "$SREL" release/session-v0.1.0
echo fix > tc_fix.txt; git add tc_fix.txt; git commit -qm "fix(signal): on the release branch (#5)"
expect_fail "guard: a fix landed after the release commit" guard "$(git rev-parse HEAD)" release/signal-v0.1.0
expect_ok "refresh" R refresh; git push -q origin release/signal-v0.1.0; SREL=$(git rev-parse HEAD)
expect_ok "guard: after refresh" guard "$SREL" release/signal-v0.1.0
# workspace version moved
git switch -q -c t/ws; sed -i 's/^version = "0.1.1"/version = "0.1.2"/' Cargo.toml; git commit -qam "chore(release): signal-v0.1.0"; WS=$(git rev-parse HEAD)
expect_fail "guard: an app release may not move the repo version" guard "$WS" release/signal-v0.1.0
git switch -q --detach "$SREL"
# the other app moved
git switch -q -c t/other; sed -i 's/^version = "0.0.2-alpha"/version = "0.1.0"/' apps/session/desktop/Cargo.toml; git commit -qam "chore(release): signal-v0.1.0"; OT=$(git rev-parse HEAD)
expect_fail "guard: an app release moves one app" guard "$OT" release/signal-v0.1.0
git switch -q --detach "$SREL"
# no changelog section
git switch -q -c t/nocl; git rm -q apps/signal/CHANGELOG.md; git commit -qm "chore(release): signal-v0.1.0"; NC=$(git rev-parse HEAD)
expect_fail "guard: changelog section missing" guard "$NC" release/signal-v0.1.0
git switch -q --detach "$SREL"
# version not moved from main's
git switch -q -c t/nomove; git checkout -q origin/main -- apps/signal/desktop/Cargo.toml; git commit -qm "chore(release): signal-v0.1.0"; NM=$(git rev-parse HEAD)
expect_fail "guard: crate version not moved (not X.Y.Z)" guard "$NM" release/signal-v0.1.0
git switch -q --detach "$SREL"

echo "=== per-app: merge, tag, GitHub Release, TestFlight dispatch"
ship "$SREL" signal-v0.1.0
[ "$(git rev-parse 'signal-v0.1.0^{commit}')" = "$SREL" ] && ok "signal-v0.1.0 on the release tip" || bad "tag target"
[ -f "$S/releases/signal-v0.1.0.body" ] && ok "GitHub Release signal-v0.1.0 exists" || bad "no signal release"
[ "$(cat "$S/releases/signal-v0.1.0.title")" = "Signal 0.1.0" ] && ok "release title: Signal 0.1.0" || bad "title: $(cat "$S/releases/signal-v0.1.0.title")"
[ "$(cat "$S/releases/signal-v0.1.0.latest")" = false ] && ok "an app release is never 'latest'" || bad "latest flag"
expect_out "release notes: the app's changelog section" "$S/releases/signal-v0.1.0.body" 'First tagged release of Signal'
expect_out "release notes: TestFlight block (pending)" "$S/releases/signal-v0.1.0.body" 'testflight:begin'
expect_out "publish dispatched signal-ios.yml on the tag" "$SHIM_LOG" 'workflow run signal-ios.yml .*--ref signal-v0.1.0 -f tag=signal-v0.1.0'
expect_no "publish did not touch session" <(ls "$S/releases") 'session-v'
expect_no "publish did not dispatch session-ios" "$SHIM_LOG" 'session-ios'
expect_no "no repo tag for an app release" <(git tag) '^v0\.1\.2'
grep -q "backmerge/signal-v0.1.0" "$SHIM_LOG" && ok "back-merge branch backmerge/signal-v0.1.0" || bad "backmerge branch name"
expect_ok "publish is idempotent" R publish
[ "$(grep -c 'release create signal-v0.1.0' "$SHIM_LOG")" = 1 ] && ok "no second release" || bad "second release created"
[ -e tc_fix.txt ] && ok "the branch fix reached dev" || bad "fix not on dev"

echo "=== per-app: record the TestFlight build on the release"
expect_ok "app-record" R app-record signal signal-v0.1.0 0.1.0 1760000001 https://github.com/x/y/actions/runs/1
expect_out "notes: marketing version" "$S/releases/signal-v0.1.0.body" 'Marketing version \*\*0\.1\.0\*\*'
expect_out "notes: build number" "$S/releases/signal-v0.1.0.body" 'build 1760000001'
expect_no "notes: no pending line left" "$S/releases/signal-v0.1.0.body" 'Pending:'
expect_out "notes: the changelog is still there" "$S/releases/signal-v0.1.0.body" 'First tagged release of Signal'
expect_ok "app-record again (a re-run, a new build)" R app-record signal signal-v0.1.0 0.1.0 1760000999
[ "$(grep -c 'testflight:begin' "$S/releases/signal-v0.1.0.body")" = 1 ] && ok "one TestFlight block" || bad "block duplicated"
[ "$(grep -c '^- build ' "$S/releases/signal-v0.1.0.body")" = 2 ] && ok "both builds listed" || bad "builds listed: $(grep -c '^- build ' "$S/releases/signal-v0.1.0.body")"
expect_fail "app-record: marketing version != tag version" R app-record signal signal-v0.1.0 0.1.1 1760000002
expect_fail "app-record: build number not a number" R app-record signal signal-v0.1.0 0.1.0 12ab
expect_fail "app-record: no release for the tag" R app-record signal signal-v0.9.9 0.9.9 1760000003

echo "=== per-app: the TestFlight gate (app-resolve)"
git fetch -q --tags origin; git switch -q --detach signal-v0.1.0
export GITHUB_OUTPUT="$S/gho"; : > "$GITHUB_OUTPUT"
expect_ok "resolve signal-v0.1.0" R app-resolve signal signal-v0.1.0
expect_out "resolve: marketing version output" "$GITHUB_OUTPUT" '^version=0\.1\.0$'
expect_out "resolve: sha output" "$GITHUB_OUTPUT" "^sha=$(git rev-parse HEAD)$"
for t in signal-v0.1 signal-0.1.0 v0.1.0 signal-v0.1.0-rc1 signal-v0.1.0.1 Signal-v0.1.0 session-v0.1.0 signal-v01.1.0x; do
  expect_fail "resolve refuses tag '$t'" R app-resolve signal "$t"
done
expect_fail "resolve: crate version != tag version (signal-v0.1.5 on the 0.1.0 commit)" R app-resolve signal signal-v0.1.5
grep -q "says version '0.1.0' but the tag is signal-v0.1.5" "$S/last.out" && ok "  ...naming both versions" || bad "  ...message"
# a tag whose crate version, changelog and shape are all right, on a commit main does not have
git switch -q --detach origin/dev
sed -i 's/^version = "0.1.0"/version = "0.1.9"/' apps/signal/desktop/Cargo.toml
printf '## [0.1.9] - 2026-10-21\n\n- x\n' >> apps/signal/CHANGELOG.md
git commit -qam "chore(release): signal-v0.1.9"; git tag signal-v0.1.9 HEAD
expect_fail "resolve: a tag that is not in main is refused" R app-resolve signal signal-v0.1.9
grep -q 'is not in origin/main' "$S/last.out" && ok "  ...for being outside main" || bad "  ...refused for another reason"
git tag -d signal-v0.1.9 >/dev/null; git reset -q --hard
git switch -q --detach "$SREL"; git rm -q --cached apps/signal/CHANGELOG.md; mv apps/signal/CHANGELOG.md "$S/cl.bak"
expect_fail "resolve: no changelog section" R app-resolve signal signal-v0.1.0
grep -q 'no \[0.1.0\] section\|has no \[0.1.0\]' "$S/last.out" && ok "  ...for the missing section" || bad "  ...refused for another reason"
mv "$S/cl.bak" apps/signal/CHANGELOG.md; git reset -q --hard "$SREL"
git switch -q --detach origin/main
git fetch -q origin; git switch -q --detach origin/main
expect_fail "resolve: main's tip has the right crate version but is not on the tag" R app-resolve signal signal-v0.1.0

echo "=== per-app: next releases, path scoping, session independent"
dev
expect_fail "prepare signal: nothing touched Signal since the tag" R prepare signal
git switch -q --detach origin/dev
card features/session/only_session.rs "feat(session): a Session-only card (#6)"; dev
expect_fail "prepare signal: a Session-only card is not a Signal release" R prepare signal
card features/signal/sig.rs "feat(signal): a Signal card (#7)"; card crates/shared/shared.rs "fix: a shared-crate card (#8)"; dev
expect_ok "prepare signal (patch)" R prepare signal
[ "$(R version)" = 0.1.1 ] && ok "workspace untouched" || bad "workspace moved"
[ "$(git_ver apps/signal/desktop/Cargo.toml)" = 0.1.1 ] && ok "signal 0.1.0 -> 0.1.1" || bad "signal $(git_ver apps/signal/desktop/Cargo.toml)"
R notes signal 0.1.1 > "$S/n.md"
expect_out "[0.1.1] lists the Signal card" "$S/n.md" 'A Signal card'
expect_out "[0.1.1] lists a shared-crate card (the app is built from it)" "$S/n.md" 'shared-crate card'
expect_no "[0.1.1] does not list the Session card" "$S/n.md" 'Session-only'
expect_no "[0.1.1] does not repeat the branch fix of 0.1.0" "$S/n.md" 'on the release branch'
expect_no "[0.1.1] has no repo release section (the repo's v0.1.1 tag is inside the range)" "$S/n.md" '^## '
grep -c '^## \[' apps/signal/CHANGELOG.md > "$S/cnt"; [ "$(cat "$S/cnt")" = 2 ] && ok "apps/signal/CHANGELOG.md: [0.1.1] above [0.1.0]" || bad "sections: $(cat "$S/cnt")"
grep -n '^## \[' apps/signal/CHANGELOG.md | head -2 | tr '\n' ' ' > "$S/order"; expect_out "newest section first" "$S/order" '\[0\.1\.1\].*\[0\.1\.0\]'
git switch -q --detach origin/dev
expect_ok "plan signal after a tag" R plan signal
R plan signal > "$S/plan.out" 2>&1; expect_out "plan: last tag" "$S/plan.out" 'last signal tag: *signal-v0\.1\.0'
expect_out "plan: next is 0.1.1" "$S/plan.out" 'next signal release: *0\.1\.1'
expect_ok "prepare session (its own baseline)" R prepare session
[ "$(git_ver apps/session/desktop/Cargo.toml)" = 0.1.0 ] && ok "session 0.0.2-alpha -> 0.1.0" || bad "session $(git_ver apps/session/desktop/Cargo.toml)"
[ "$(git_ver apps/signal/desktop/Cargo.toml)" = 0.1.0 ] && ok "signal untouched by a session release" || bad "signal moved"
expect_ok "release-pr (session)" R pr; MREL=$(git rev-parse HEAD); git push -q origin release/session-v0.1.0
expect_ok "guard: session release" guard "$MREL" release/session-v0.1.0
expect_fail "guard: session release under a signal name" guard "$MREL" release/signal-v0.1.0
ship "$MREL" session-v0.1.0
[ -f "$S/releases/session-v0.1.0.body" ] && ok "GitHub Release session-v0.1.0" || bad "no session release"
expect_out "session-ios dispatched on its tag" "$SHIM_LOG" 'workflow run session-ios.yml .*-f tag=session-v0.1.0'
[ "$(grep -c 'release create signal-v' "$SHIM_LOG")" = 1 ] && ok "signal not re-released by session's merge" || bad "signal released again"
git fetch -q --tags origin; git switch -q --detach session-v0.1.0
: > "$GITHUB_OUTPUT"; expect_ok "resolve session-v0.1.0" R app-resolve session session-v0.1.0
expect_fail "resolve: signal tag handed to the session workflow" R app-resolve session signal-v0.1.0

echo "=== per-app: 0.1.9 -> 0.1.10 -> 0.1.11 (the patch never rolls over)"
dev
card features/signal/a.rs "fix(signal): card for 0.1.9 (#9)"; dev
expect_ok "prepare signal 0.1.9 (explicit)" R prepare signal 0.1.9; git push -q origin release/signal-v0.1.9; ship "$(git rev-parse HEAD)" signal-v0.1.9
card features/signal/b.rs "fix(signal): card F (#10)"; dev
expect_ok "prepare signal" R prepare signal; [ "$(git_ver apps/signal/desktop/Cargo.toml)" = 0.1.10 ] && ok "0.1.10 (not 0.2.0)" || bad "got $(git_ver apps/signal/desktop/Cargo.toml)"
git push -q origin release/signal-v0.1.10; ship "$(git rev-parse HEAD)" signal-v0.1.10
card features/signal/c.rs "fix(signal): card G (#11)"; dev
expect_ok "prepare signal" R prepare signal; [ "$(git_ver apps/signal/desktop/Cargo.toml)" = 0.1.11 ] && ok "0.1.11" || bad "got $(git_ver apps/signal/desktop/Cargo.toml)"
[ "$(git tag --list 'signal-v*' --sort=-v:refname | head -1)" = signal-v0.1.10 ] && ok "newest signal tag by sort -V is 0.1.10" || bad "tag order"
R notes signal 0.1.11 > "$S/n.md"; expect_no "[0.1.11] repeats nothing of 0.1.10" "$S/n.md" 'card F'
expect_out "[0.1.11] has card G" "$S/n.md" 'card G'
expect_fail "prepare while release/signal-v0.1.11 would be a duplicate tag" R prepare signal 0.1.10
git switch -q --detach origin/dev
expect_fail "prepare refuses a version that is not greater" R prepare signal 0.1.3

echo; echo "=== RESULT: $pass passed, $failn failed"
[ "$failn" = 0 ]
