# Releases: trunk-based main, change files, a CalVer tag is the release

studio releases **from the tag**. Any commit on `main` whose `CI passed`
is green can be released by pushing a CalVer tag, and the tag alone
starts the release builds. There is no release PR and no release commit.
The fleet `release-workflow` skill covers the shared rules (branch names,
PR titles and template, change files, CI gates). This page covers studio
and wins where the two disagree (the skill still describes release PRs).

In short:

- **`main` is the only long-lived branch.** Branch from `origin/main`
  (`feat/sta-123-slug`, `fix/gh-42-slug`, `chore/no-issue-slug`), open a PR
  into `main`, squash-merge it. There is no `dev`, no promotion PR and no
  back-merge.
- **Every PR that changes a crate carries a change file**:
  `cargo rail change add <crates> --bump <none|patch|minor|major> -m
  "<note>"` writes `.changes/<slug>.md` (see `.changes/README`). CI fails a
  PR whose changed crates have none. That is unchanged.
- **A merge to `main` runs CI and nothing else.** It ships nothing.
- **A release is a `vYY.M.MICRO` tag**, pushed by `just release` on a green
  commit. The tag ships everything, from the tag alone and in parallel:
  the studio-web image, the Studio TestFlight upload, the desktop
  installers and the one GitHub Release. **Production is the latest `v*`
  tag.** This repo deploys nothing: Argo CD rolls out the image from
  gitops (Codys-Wright/starcommand), with argocd-image-updater picking the
  highest image. The REAPER integration suites are not in CI; check them
  locally (`just reaper-test`, `just reaper daw-test`).

```
feat/sta-1-x ─PR (fast checks, change file)─squash─▶ main ── CI (CI passed) after every merge
fix/sta-2-y  ─┘                                       │
                       just release ─▶ tag v26.10.4 on main's newest green commit
                                          │ (the tag alone, in parallel:)
                                          ├─ Images: ghcr.io/…/studio-web:26.10.4 ─▶ image-updater ─▶ Argo CD
                                          ├─ studio-ios: TestFlight
                                          ├─ desktop: .dmg + AppImage, attached to the Release
                                          └─ release: the GitHub Release (notes from .changes/)

hotfix: release/v26.10.4 (from the tag) ◀── cherry-pick -x PR ── CI passed ── just release <sha> ─▶ v26.10.5
```

## One version, one app, and the version is the tag

Signal and Session ship as one app, Studio, so there is one release: one
version, one tag, one GitHub Release. The app crates (`apps/studio/desktop`,
and `apps/session/desktop`, which Studio links) say `version.workspace =
true`. Change-file notes end with `(signal)` or `(session)` so the notes
say which side of Studio changed, or `(studio)` for the app as a whole.

**Releases commit no version.** Every release build runs
`scripts/release.sh stamp X.Y.Z` right after its checkout. It writes the
tag's version into that checkout's `[workspace.package] version` and the
workspace members' Cargo.lock entries, uncommitted, so
`env!("CARGO_PKG_VERSION")`, the iOS marketing version, the desktop app
version, the update check and the image's version label all see the
release. **The version committed on `main` (26.10.3, the last release
commit) is not the released version.** The latest `v*` tag is
(`just release-plan` shows both).

Versions are CalVer `YY.M.MICRO` (no leading zeros). The next version is
computed, not chosen: take the highest `vX.Y.Z` tag (suffixed tags don't
count); in the same month it's `MICRO + 1`, otherwise `YY.M.0`. The image
tag is exactly that bare version (`26.10.4`: no `v`, never `26.09.1`),
because argocd-image-updater filters on `^[0-9]+\.[0-9]+\.[0-9]+$` and
orders the tags as semver. `Images` refuses anything else.

## Commands

```bash
cargo rail change add <crates> --bump patch -m "Fixed … (signal)"   # in every PR that changes a crate
cargo rail plan --explain                                            # which tests CI will run for your branch

just release-plan            # latest tag, what main holds since, the next version, the draft notes
just release --dry-run       # the commit it would tag (main's newest green), the version, the notes; no tag
just release                 # tag + push (asks first on a terminal; --yes skips the question): THIS SHIPS
just release <sha>           # a given commit: on main or a release/v* line, with CI passed green
just release-notes [vX.Y.Z]  # a tag's notes (default: the draft for origin/main)

just release-hotfix <sha>…   # release/v<latest tag> + the main fixes cherry-picked (-x) + PR into it
just release-sandbox         # the release tooling's own end-to-end test (no network)
```

`just release` fetches `main` and the `release/v*` lines. Without a sha it
walks `main` back from its tip (first parent, down to the previous tag)
and takes the newest commit whose latest `CI passed` run succeeded. It says
why it skipped each newer one (still running, red, cancelled by a newer
merge). It refuses:

- a commit without a green `CI passed`;
- a commit on neither `main` nor a `release/v*` line;
- a commit that is already tagged, or older than its line's newest release;
- a next version that is not above the highest tag (the clock).

Then it prints the notes and pushes an annotated tag. It waits on nothing
after the push: the image doesn't exist yet, because the tag builds it.

Run it as a person, or as the `fts-forge` App. A tag pushed with a
workflow's `GITHUB_TOKEN` starts no workflow. Never move or delete a tag; a
bad release is superseded by the next.

**Release notes** are built from the change files **added** since the
previous tag (`git diff --diff-filter=A --name-only <prev>..<sha> --
.changes/`), read at the tagged commit and grouped: major → Breaking /
Migration, minor → Added, patch → Fixed and improved, none → nothing. A
change file that a hotfix release on another line already shipped is not
listed again, and the notes say "Includes all changes up to v…". The notes
go into the GitHub Release body. Nothing is deleted or committed at
release time, and change files stay in `.changes/` as the record.
`CHANGELOG.md` ends at 26.10.3 and is no longer written. To fix a note,
edit the change file on main before releasing.

## What runs where

| Event | Workflows | Runner |
|---|---|---|
| PR into `main` | `PR` (`PR checks`: fmt, change files, the lockfile, the rail config; compiles nothing), the `Local CI` status (the prek pre-push hook: the static guards, then the rail-selected tests, `cargo check` and wasm, on the pusher's machine; an indicator, not required) | `PR checks` on GitHub's free runners |
| PR into `release/**` (hotfix) | `CI` (the full set) | as below |
| push to `main` | `CI`, narrowed: `Rail plan` (`cargo rail plan` from the newest green commit with rail evidence; the full set without one) gates `Check` (+ advisory clippy; records the evidence), `Wasm` and `Tests` (the core test set, filtered to the packages reached); with `Gates`, `Release tooling (macOS)`, aggregated as `CI passed`; also `Dependency cache`. A newer push cancels the older run | Rail plan, Check, Tests, Wasm on THEBATTLESHIP's `studio-main`; the rest on GitHub's free runners |
| push to `release/v*` | `CI` full set | as above |
| `gh workflow run checks.yml --ref <branch>` | `CI` full set; when green it posts `Local CI` on the branch's head | as above |
| push of tag `vYY.M.MICRO` (or `workflow_dispatch` with the tag) | all from the tag, in parallel, each in its own never-cancelled group. Each starts with `stamp` and the tag guard, and none re-runs the tests. **`Images`**: `.#studio-web-image` pushed to `ghcr.io/fasttrackstudios/studio-web` as `X.Y.Z`, `sha-<short>`, `latest`, labelled with the commit. **`studio-ios`**: the TestFlight upload. **`desktop`**: the signed, notarized macOS `.dmg` and the Linux AppImage, attached to the Release as they finish (`gh workflow run desktop.yml -f tag=<ref> -f mode=build-only` proves them from any branch). **`release`**: the GitHub Release with the notes, published straight away | Images on THEBATTLESHIP's `studio-release`; TestFlight and the `.dmg` on airlock's `studio-release-mac`; the AppImage on `studio-release`; resolve, guard and publish jobs hosted |

**Pools.** `studio-release` (THEBATTLESHIP) and `studio-release-mac`
(airlock) build only release artifacts: the image, the AppImage, the
`.dmg`, TestFlight. Checks never run there. They run on `studio-main`,
which keeps the warm dev cache, three runners sharing one cache capped at
200 GB (Codys-Wright/nix-config hosts/THEBATTLESHIP/github-runners.nix),
in the `.#ci` dev shell. Release jobs are never cancelled, and a push to
`main` cancels the older `main` run.

The workflow files are generated from `xtask/workflows` (one Rust module per
workflow, the reasons as comments there): edit the Rust, run `just
workflows`, commit both; `Gates` and `just ci-local` fail on drift (`just
workflows-check`) and run actionlint.

The `CI` jobs run without nix: the pinned rustc (dtolnay/rust-toolchain),
apt headers (cached), and pinned release binaries for just, cargo-rail,
actionlint. There is no nightly, no tailwind step (the sheets are
committed; dx builds session-web's) and no Boundaries step (`just
ci-boundaries` is manual). `cache-devshell` still seeds
fasttrackstudio.cachix.org, for developers' `nix develop`; no CI job
depends on it.

`PR checks` is the only required check (the `main` ruleset). `Local CI`
says a change's tests passed on a warm machine; it is an indicator, not a
gate. `CI passed` is the aggregate of a run: it requires
`Gates` and `Rail plan`, and `Check`, `Wasm` and `Tests` green or skipped by
the plan, and `just release` and the tag guard
require it green on the released commit. Advisory (red or green, never
blocking): clippy. `Local CI` is only as honest as whoever posts it, since
anyone who can push could post one. That is the bargain (2026-10): the
rail-planned run after every merge (everything since the last green
commit) catches what a local run missed. `main` may be
red for code reasons; nothing is released until a commit's `CI passed` is
green.

## What the guards check

**Tag guard** (`release.sh tag-guard` after `stamp`, in `Images` and the
`release` publish; `ios-resolve` in `studio-ios` and `desktop`):

- the tag is `vX.Y.Z`, and the stamped checkout and the iOS app crate carry
  its version;
- the checkout is the tagged commit, on `main` or a `release/v*` line;
- where the job has a token, `CI passed` is green on it. The test suite is
  not run again.

`Images` also refuses a tag whose bare version is not
`^[1-9][0-9]\.(1[0-2]|[1-9])\.(0|[1-9][0-9]*)$` (`release.sh image-tag`
holds the same rule). A network error fails every guard, so none of them
can pass by accident.

## The iOS app: TestFlight is a release

Studio (`apps/studio/desktop`, App Store record "Studio - by
FastTrackStudio", `app.fasttrackstudio.studio`) is the one iOS app:
Signal and Session behind one front screen. Signal's and Session's own
records (`app.fasttrackstudio.signal`, `.session`) get no new builds.

- **TestFlight uploads only from the release tag**, never on a push, a PR
  or a fork. The tag starts `studio-ios`.
- **Marketing version (CFBundleShortVersionString) = the tag's version**
  (`v26.10.4` → `26.10.4`), stamped into the checkout, and the app crate
  must resolve to it. The build number (CFBundleVersion) is the unix time
  of the upload.
- **One GitHub Release** per tag (title `studio 26.10.4`, the notes from
  the change files). After the upload the workflow writes its marketing
  version, build number and run URL into the Release notes; a re-run
  editing at the same time re-reads and retries.
- `studio-ios` refuses a tag that is not `^v[0-9]+\.[0-9]+\.[0-9]+$`, a
  stamped crate or workspace version that differs, a commit on neither
  `main` nor `release/v*`, or a red `CI passed`. `workflow_dispatch` with
  `dry_run` stops after those checks.

Verify a release: App Store Connect's build for each bundle id has the
Release's version and the build number written in its notes
(`ruby apps/<app>/desktop/ios/testflight-status.rb app.fasttrackstudio.<app>`
on airlock).

## Hotfix

1. Fix on `main` first (a `fix(scope): …` PR with its change file).
2. If everything on `main` since the last tag is shippable: `just release`
   once that commit is green.
3. Otherwise `just release-hotfix <merged sha>…`. It creates
   `release/v<latest tag>` from the tag if missing, cherry-picks the fixes
   (and their change files) with `-x` onto `fix/no-issue-hotfix-<sha>`, and
   opens a PR into that line (the full set runs on it). Merge it, wait for
   `CI passed` on the line's new head, then `just release <that sha>`. The
   version is the next free number.
4. Nothing is forward-ported: a release commits nothing. `main`'s next
   release skips the change files the hotfix shipped and says "Includes
   all changes up to v<hotfix>." `release/*` is never merged back.

## Rollback

- **studio-web**: this repo deploys nothing. Roll back in the cluster:
  pin the previous release's image in starcommand's gitops (or revert
  argocd-image-updater's write-back commit), or roll back the Argo CD
  Application. Nothing in studio-web holds data, so an image swap is a
  full rollback. A rollback is a production change: get the Captain's
  yes or no first. The old session-web / signal-web sites (#224) keep
  serving their last image until they are retired.
- **TestFlight**: expire the build in App Store Connect (per app); the
  previous build stays installable. To ship the previous code again, fix
  forward and release a new version; a tag is never moved or reused.
- **Desktop installers**: delete the bad assets from the GitHub Release,
  or release the next version.
