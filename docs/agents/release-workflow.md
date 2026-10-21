# Releases: dev first, main is finished features, a tag is the release

studio has two long-lived branches and one rule for shipping:

- **`dev`**: the default branch and the integration branch. Everything
  lands here: the Captain's pushes, and agents' PRs (squash-merged). Dev
  never freezes.
- **`main`**: **finished features**. Work gets here by a *promotion PR*
  from `dev` once it is verified. **A push to main ships nothing**: no
  deploy, no TestFlight, no installers.
- **A release is a `v*` tag on main.** The tag ships everything: both
  sites, both iOS apps (Signal and Session, one version, one TestFlight
  upload each), the REAPER suites, the desktop installers, and the one
  GitHub Release. **Production is the latest `v*` tag.**

**studio is one version.** Signal and Session are the same build, and a change
to one almost always affects the other, so they are one release: one
`[workspace.package] version`, one `vX.Y.Z` tag, one GitHub Release, one
`CHANGELOG.md` section. There are no per-app tags, branches or changelogs. The
version the repo is on is the version both apps are on.

```
PRs --squash--> dev --promotion PR (merge commit)--> main (finished features)
                 ^                                     |
                 |                    release PR (version + changelog), merge commit
                 |                                     v
                 +---- back-merge main -> dev <---- tag vX.Y.Z on main
                                                       |
                       sites . Signal + Session TestFlight . REAPER . installers . GitHub Release
```

CI is spent once per promotion, not once per push, and a release costs a
version bump.

## What runs where

| Event | Workflows | Runner |
|---|---|---|
| PR into `dev` | `pr-title` (a regex on the title, seconds) | GitHub-hosted |
| push to `dev` | nothing | none |
| **promotion PR** `dev` -> `main` (or `hotfix/*` -> `main`) | `checks` (the full `just ci` set + advisory clippy; check name `Build + test`), `release` guard | THEBATTLESHIP (`session`), guard GitHub-hosted |
| **release PR** `release/vX.Y.Z` -> `main` | `release` guard; `checks` plans it as a release commit and **skips** `Build + test` (a skipped job passes the required check) | GitHub-hosted |
| push to `main` | nothing (`cache-devshell` for flake changes only) | none |
| push of tag `vX.Y.Z` (or `workflow_dispatch` with the tag as its ref) | `deploy` (session-web, signal-web), `signal-ios` **and** `session-ios` (resolve -> build + upload to TestFlight on `airlock` -> record the build on the Release), `reaper-integration`, `desktop` (resolve -> macOS `.dmg` x2 on `airlock` and Linux AppImage x2 on `session` in parallel -> attach all four to the Release), `release` publish (the one GitHub Release) | `session`, `airlock`, GitHub-hosted |
| by hand | any of them: `gh workflow run <file> --ref <branch or tag>` | |

The gate for work going into `dev` is local: at least
`cargo check --workspace --all-targets` in the dev shell on the commit
being pushed, or the full `just ci` (exactly what `checks.yml` runs, in the
same order). An agent's PR into dev carries that evidence in its
description: the head sha, `origin/dev`'s sha, the result and the host.

The ruleset `main (release only)` makes a PR the only way into main: merge
commits only, and a green `Build + test` required.

`Build + test` (the required check) is manifests + lockfile, fmt, the flow
gate and `cargo check --workspace`. The wasm check and the nextest run live in
the **advisory** job `Advisory: wasm + tests`: it runs on every PR into main
and shows red or green, but is not in the ruleset. It is red today (STA-184);
when STA-184 makes it green its two steps move back into `Build + test` and
the advisory job goes away.

## Day to day

```bash
git switch dev && git pull            # once: track dev instead of main
cargo check --workspace --all-targets # (or just ci) before pushing
git push origin HEAD:dev              # or open a PR into dev
```

PR titles into dev must be conventional commits (`feat(editor): ...`,
`fix: ...`, `chore(deps)!: ...`), because a squash merge turns the title
into the commit subject and git-cliff turns that into the changelog line.
Merge PRs into dev with **squash**, and give `gh pr merge` a conventional
`--subject` when the title is not one. Commits pushed directly with prose
subjects are fine; they land under "Changes".

## Stage 1: promote (dev -> main)

```bash
just release-plan          # what is live, what main holds, what waits on dev
just release-promote       # opens the promotion PR dev -> main
```

- `checks` (the full workspace) and the `Release guard` run on it, and the
  **Reviewer** agent reads the diff and posts its review (blocking /
  should-fix / nit) on the PR before it merges.
- Merge with **Create a merge commit**, never squash or rebase. Nothing
  ships.
- `release-promote` refuses while main has content dev lacks (a release
  commit or a hotfix): `just release-backmerge` first.
- Promote whenever a feature set is verified. Features pile up on main
  until a release is cut.

## Stage 2: release (a tag on main)

```bash
just release-prepare       # cuts release/vX.Y.Z from origin/main: ONE commit, version + CHANGELOG.md
just release-pr            # push it, open release/vX.Y.Z -> main
# merge it with a merge commit (nothing ships yet), then:
just release-tag           # tag vX.Y.Z on main: THIS ships; then opens the back-merge
```

- Versions: **patch by default**, and the patch never rolls over
  (0.1.9 -> 0.1.10 -> ... -> 0.1.86). `just release-prepare minor|major|X.Y.Z`
  only when the Captain or Commander calls it. Versions compare
  semantically (`sort -V`), never as strings. Versions move only on main,
  never on dev.
- A release PR is exactly one commit that changes only the version line
  (`Cargo.toml`, the workspace members' lines in `Cargo.lock`) and
  `CHANGELOG.md`. The guard proves it, so the heavy checks are skipped.
- If main moves before the release PR merges (another promotion), close
  the PR, delete the branch and cut again. It takes a minute.
- `release-tag` pushes the tag as the `fts-forge` App (or a person). A tag
  pushed with a workflow's `GITHUB_TOKEN` starts no workflow, which is why
  the tag is pushed from outside CI. Never move or delete a tag; a bad
  release is superseded by the next.
- After the tag, `release-tag` runs `release-backmerge`: a PR
  `backmerge/vX.Y.Z-<sha>` -> dev, merge commit, auto-merge, so dev gets
  the version and the changelog. If it conflicts (usually Cargo.lock next to
  a version line), merge dev into the backmerge branch and resolve there,
  never on main. After a plain promotion the back-merge is a no-op (the merge
  commit's tree is dev's).

## iOS apps: TestFlight is a release

Signal (`app.fasttrackstudio.signal`) and Session (`app.fasttrackstudio.session`,
the iPad/iPhone app) are released **together, at one version**:

- **One version.** Both app crates (`apps/signal/desktop`,
  `apps/session/desktop`) say `version.workspace = true`, so they take
  `[workspace.package] version` from the root `Cargo.toml` and cannot differ.
- **TestFlight uploads only from the release tag** `vX.Y.Z`, never on a push to
  main, a path change, a PR or a fork. One tag starts **both** `signal-ios` and
  `session-ios`.
- **The TestFlight marketing version (CFBundleShortVersionString) is the tag's
  version** (`v0.1.3` -> `0.1.3`) for both apps, and each app crate's version
  must be that same version.
- **One GitHub Release** for the tag (title `studio 0.1.3`, the changelog section
  as the notes). After each upload, that app's workflow writes the marketing
  version, the build number (CFBundleVersion, the unix time of the upload,
  climbing across re-runs) and its run URL into the Release notes, **one line per
  app**, so the Release and the TestFlight builds can be traced to each other.
  The two workflows edit the same notes at about the same time: each re-reads the
  notes, edits, and checks that its line survived (retrying if the other's edit
  overwrote it). macOS and Linux installers attach to that same Release.

What `signal-ios` / `session-ios` refuse (red job, nothing built): a tag that is not
`^v[0-9]+\.[0-9]+\.[0-9]+$`; an app crate (or the workspace) whose version differs
from the tag's; a tag without its changelog section; a commit that is not in `main`.
A `workflow_dispatch` with `dry_run` runs those checks and stops before the build. A
tag pushed by hand starts both workflows too (the first creates the Release if there
is none). `MARKETING_VER` has no default: `deploy-testflight.sh` refuses to run without
it.

Verify a release: App Store Connect's build for each bundle id has marketing version ==
the Release's tag version and the build number written in its notes
(`ruby apps/<app>/desktop/ios/testflight-status.rb app.fasttrackstudio.<app>` on airlock).

## What the guard checks (release.yml, on every PR into main)

| Head | Fails when |
|---|---|
| `dev` (promotion) | no commits main lacks; the version differs from main's; merging would not reproduce dev's tree (main has content dev lacks: back-merge first) |
| `release/vX.Y.Z` | the name does not match the version; the version is not greater than main's or the tag exists; no `[X.Y.Z]` changelog section; the head is not `chore(release): vX.Y.Z`; main is not in the head; it is not exactly one commit; it changes anything but the version line, `Cargo.lock` versions and `CHANGELOG.md`; either iOS crate does not resolve to the version |
| `hotfix/*` | the version moved; nothing main lacks; it conflicts with main |
| anything else | always (card branches target `dev`) |

Until `scripts/release.sh` exists on main, the guard only prints a notice
(the PR that introduced this flow). The tag side has its own guard
(`release.sh tag-guard`, run first by `deploy` and by `release` publish): the
tag is `v<Cargo version>`, both iOS crates are on it, it is in main's history,
and it has a changelog section.

## The changelog

Each release's section is generated over a git range, the commits on main
since the last `v*` tag (`git cliff --tag vX.Y.Z --strip all <last>..HEAD`),
and written above the older sections, which stay as released. Never
regenerate the whole file with `git cliff -o`. One `CHANGELOG.md` section
covers Signal and Session (and their macOS and Linux installers) together;
there are no per-app changelogs. cliff.toml's `tag_pattern` counts only
`vX.Y.Z` tags (the plugins' tags are not covered yet).

## Hotfix

```bash
just release-hotfix NAME   # hotfix/NAME from the latest tag (what production runs)
# fix, commit (fix(scope): ...), just ci
just release-pr            # PR hotfix/NAME -> main: checks + guard; merge commit
just release-prepare       # then the patch release as usual
just release-pr            # merge, then
just release-tag
```

Finished features promoted since the tag ship with the hotfix, because
promoted means verified.

## Rollback

- **Sites**: `deploy` tags every image `sha-<short>` and `vX.Y.Z`. Point
  argocd-image-updater back at the previous release's tag (or re-run
  `deploy` on the previous tag: `gh workflow run deploy.yml --ref vX.Y.Z`).
  Nothing in studio's sites holds data, so an image swap is a full rollback.
- **TestFlight**: expire the build in App Store Connect (per app); the previous
  build stays installable. To ship the previous version again, cut a new
  patch release; a tag is never moved or reused.
- **The workflow itself**: set the default branch back to `main`, restore
  the `on: push: branches: [main]` triggers in `deploy.yml` (one commit), and
  change the `main (release only)` ruleset only with the Captain's yes.

## Branches

The Captain's feature branches (`editing-engine`, ...) are cut from and
merged into `dev`. A branch that was cut from main before the switch and
is not behind it fast-forwards onto dev unchanged
(`git push origin <branch>:dev` after a local check).

## Desktop installers: the same tag, the same Release

The release's desktop apps are attached to the one GitHub Release of `vX.Y.Z`
by `.github/workflows/desktop.yml`, which starts on the tag beside the two iOS
workflows. Four files, all named with the release's version, each with a
`.sha256`:

| File | Built on | How |
|---|---|---|
| `Signal-X.Y.Z-macos-arm64.dmg`, `Session-X.Y.Z-macos-arm64.dmg` | `airlock` | `scripts/desktop/macos.sh <app>`: `cargo build --profile release-fast` in the dev shell, a bundle whose executable is the Mach-O itself, Developer ID signature with the hardened runtime (`scripts/desktop/macos.entitlements`), a signed `.dmg`, notarization (`notarytool`, must say `Accepted`), staple, and `spctl -a -vv` on the image and on the app inside it |
| `Signal-X.Y.Z-linux-x86_64.AppImage`, `Session-X.Y.Z-linux-x86_64.AppImage` | THEBATTLESHIP (`session` runner) | `scripts/desktop/linux.sh <app>`: the same build in the CI dev shell with its **own** target dir (size-capped), an AppDir with every needed library except glibc and the graphics stack, no `/nix/store` path left in any ELF, packed as the pinned type2 runtime plus a squashfs (what `appimagetool` does; nixpkgs no longer has it) |

**Why an AppImage for Linux.** One file, no root and no package manager, runs
on any recent distro, and `--version` can be smoke-tested without a display. A
`.deb` fits one family and needs dpkg; a tarball has no desktop entry or icon.
A binary built in a Nix shell points into `/nix/store`, so `linux.sh` bundles
its libraries, repoints the interpreter at `/lib64/ld-linux-x86-64.so.2`, and
**computes the glibc floor** (the newest `GLIBC_x.y` any bundled file needs).
It refuses to build over `MAX_GLIBC` (2.42, a ratchet) and writes the floor
next to the file, so a toolchain bump that raises it is seen here, not by a
user. The AppImage therefore runs on distros whose glibc is at least that
floor. Measured on the v0.1.0 build: **2.42** (WebKitGTK and libgpg-error from
the Nix toolchain's glibc), so Fedora 43+, Arch and other rolling distros;
Ubuntu 24.04 (2.39) and Debian 12/13 stop with `GLIBC_2.xx not found`. That is
a real limit of shipping from a Nix toolchain, written into the release notes
by the workflow, and the way to lower it is a follow-up (build against an older
glibc, or carry glibc in the AppImage). The
graphics stack (GL, EGL, the Vulkan loader, DRM, GBM) is taken from the user's
driver, not bundled.

**Secrets.** None in GitHub. The Developer ID identity is in airlock's build
keychain (`fts-build.keychain`) and the App Store Connect API key notarytool
uses is in `~/.appstoreconnect/config.env` on airlock (the TestFlight workflows
use the same key). The jobs that run there and on THEBATTLESHIP have a
**read-only** `GITHUB_TOKEN`; the hosted `publish` job is the only one that can
write to the Release, and it only receives the finished files as artifacts. The
workflow has no `pull_request` trigger, so no fork or PR reaches either runner.
(The fts-forge App key is readable on THEBATTLESHIP runners, an accepted risk;
no signing material is on them.)

**Dry run before a tag.** `gh workflow run desktop.yml --ref <branch> -f
tag=<branch or sha> -f mode=build-only` builds, signs, notarizes and smoke-tests
every installer from any ref and keeps them as workflow artifacts; the Release
is not touched. `-f mode=validate` only resolves the tag. (A workflow file has
to be on the default branch before it can be dispatched.)

**Verify a shipped release.**
- Both apps' TestFlight builds show marketing version = the tag
  (`testflight-status.rb <bundle id>` on airlock), with the build numbers in the
  Release notes.
- Download each `.dmg`: `spctl -a -vv -t open --context context:primary-signature
  <dmg>` and `xcrun stapler validate <dmg>` on airlock say accepted/valid.
- Each AppImage starts with no display: `scripts/desktop/linux-smoke.sh <file>
  <app> <version>` (on NixOS it goes through `appimage-run`;
  `SMOKE_PODMAN_IMAGE=docker.io/library/archlinux:latest` runs the payload in
  another distro).
- The `.sha256` files match.

**Disk.** airlock builds in its one shared `target/` on `/Volumes/build-disk`
(the job refuses to start under 50 GB free). THEBATTLESHIP builds in
`$CI_CACHE_DIR/cargo-target-desktop`, capped at 150 GB (emptied when over it
before a build; `incremental/` dropped when over it after), and refuses to
start under 100 GB free on that disk.
