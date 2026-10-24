# Releases: trunk-based main, change files, a CalVer tag is the release

studio runs the fleet's **release flow v4**. The rules (branch names, PR
titles and template, change files, versions, tags, CI gates, hotfixes,
rollback) live in ONE place, the fleet **`release-workflow` skill**
(`SKILL.md` + `references/`). This page covers only what is specific to
studio. If the two disagree, this page is stale: fix it.

In short:

- **`main` is the only long-lived branch.** Branch from `origin/main`
  (`feat/sta-123-slug`, `fix/gh-42-slug`, `chore/no-issue-slug`), open a PR
  into `main`, squash-merge it. There is no `dev`, no promotion PR and no
  back-merge.
- **Every PR that changes a crate carries a change file**:
  `cargo rail change add <crates> --bump <none|patch|minor|major> -m
  "<note>"` writes `.changes/<slug>.md` (see `.changes/README`). CI fails a
  PR whose changed crates have none. Nobody edits `CHANGELOG.md` outside a
  release.
- **A release is a `vYY.M.MICRO` tag.** The tag ships everything: both
  sites, the iOS app (Studio, one TestFlight upload) and the one GitHub
  Release. The REAPER integration suites are not in CI;
  they are checked locally (`just reaper-test`, `just reaper daw-test`). **Production is the
  latest `v*` tag**; a push to `main` ships nothing.

```
feat/sta-1-x ─PR (fast checks, change file)─squash─▶ main ─release/pending PR─▶ tag v26.10.3
fix/sta-2-y  ─┘                                       │ full set after every merge     │
                                                      │      sites . Signal + Session TestFlight
        hotfix: release/v26.10.3 (from the tag) ◀── cherry-pick -x    . GitHub Release
                └─ release/pending PR ─▶ tag v26.10.4 ─▶ forward-port PR (changelog) into main
```

## One version, one app

Signal and Session ship as one app, Studio, so there is one release: one
`[workspace.package] version`, one tag, one GitHub Release, one
`CHANGELOG.md` section. The app crates (`apps/studio/desktop`, and
`apps/session/desktop`, which Studio links) say `version.workspace =
true`. Change-file notes end with `(signal)` or `(session)` so the shared
changelog says which side of Studio changed, or `(studio)` for the app as
a whole (its front screen, packaging, the TestFlight build).

Versions are CalVer `YY.M.MICRO` (no leading zeros). The next version is
computed, not chosen: the highest `vX.Y.Z` tag (suffixed tags don't count);
same month → `MICRO + 1`, otherwise `YY.M.0`. The flow-v2 releases were
`v0.1.0`–`v0.1.2`; the first v4 release is `26.M.0` (ascending, so App
Store Connect accepts it).

## Commands

```bash
cargo rail change add <crates> --bump patch -m "Fixed … (signal)"   # in every PR that changes a crate
cargo rail plan --explain                                            # which tests CI will run for your branch

just release-plan            # latest tag, what main holds since, the next version, the draft changelog
just release-prepare         # release/pending from origin/main: ONE commit `chore(release): vYY.M.MICRO`
just release-pr              # push it, open its PR into main
just release-merge           # re-run the guard against the CURRENT main, then squash-merge
just release-tag             # tag the merge commit once its `CI passed` is green: THIS ships

just release-prepare --skip-untagged  # main carries a release that merged but was never tagged: release past it
                             # (its changes ship in the next version; the section says so)

just release-hotfix <sha>…   # release/v<latest tag> + the main fixes cherry-picked (-x) + PR into it
just release-prepare --hotfix  # then release-pr, release-merge, release-tag (opens the forward-port PR)
just release-sandbox         # the release tooling's own end-to-end test (no network)
```

`release-prepare` builds the section from `.changes/*.md` (major →
Breaking / Migration, minor → Added, patch → Fixed and improved, none →
nothing), deletes the files it consumed, bumps `Cargo.toml` and the
workspace members in `Cargo.lock`. With no change files at all it falls
back to git-cliff over the commits since the last tag (history before v4).
Tidy wording by amending the one commit. `cargo rail release` can't do
this for studio (it refuses `version.workspace = true` crates); `release.sh`
fills that gap until it can.

`release-tag` pushes the tag as the `fts-forge` App (or a person): a tag
pushed with a workflow's `GITHUB_TOKEN` starts no workflow. Never move or
delete a tag; a bad release is superseded by the next.

## What runs where

| Event | Workflows | Runner |
|---|---|---|
| PR into `main` | `PR` (`PR checks`: fmt, change files, the lockfile, the rail config — compiles nothing) + the `Local CI` status (the prek pre-push hook: the static guards, then the rail-selected tests + `cargo check` + wasm, on the pusher's machine) + `Release guard` | `PR checks` on GitHub's free runners |
| release PR `release/pending` → `main` | `Release guard`; `PR checks` runs the same guard from the base commit and skips its checks if it passes; `ci-local` runs it too and posts `Local CI` without building | hosted |
| PR into `release/**` (hotfix) | `CI` (the full set), `Release guard` | as below |
| push to `main` / `release/v*` | `CI` full set: `Gates`, `Check` (+ advisory clippy), `Wasm`, `Tests`, aggregated as `CI passed`; on `main` also `Dependency cache` (the cargo downloads `PR checks` restore) | gates, wasm, plan, aggregator free; `Check` on a WarpBuild 8x; `Tests` on a 16x snapshot runner |
| Sunday (cron) / `gh workflow run checks.yml --ref main -f seed=true` | `CI` full set as a **seed**: `Tests` boots from WarpBuild's base image and saves a fresh snapshot | as above |
| `gh workflow run checks.yml --ref <branch>` | `CI` full set; green, it posts `Local CI` on the branch's head (for a push the hook couldn't vouch for) | as above |
| push of tag `vYY.M.MICRO` (or `workflow_dispatch` with the tag as ref) | `deploy` (session-web, signal-web), `studio-ios`, `release` publish | `studio-deploy` (LAN registry), `airlock`, hosted |

The workflow files are generated from `xtask/workflows` (one Rust module per
workflow, the reasons as comments there): edit the Rust, run `just
workflows`, commit both; `Gates` and `just ci-local` fail on drift (`just
workflows-check`) and run actionlint.

The `CI` jobs run without nix: the pinned rustc (dtolnay/rust-toolchain),
apt headers (cached), and pinned release binaries for just, cargo-rail,
tracey and actionlint. Gates, check and wasm restore one rust-cache per
build kind that only `main` saves (GitHub's cache on the free runners,
WarpBuild's on the 8x). `Tests` boots from a WarpBuild snapshot (warm
target dir, pruned to what the seed's build used) and builds the whole
workspace. **Only a seed saves the snapshot**, from a runner booted from
the base image: Task saved from a restored runner, and the snapshot of a
snapshot booted runners that never picked up a job (2026-10-23).
Snapshots expire after 15 days unused; the weekly seed keeps one alive.
There is no nightly, no tailwind step (the sheets are committed; dx builds
session-web's) and no Boundaries step (`just ci-boundaries` is manual).
`cache-devshell` still seeds fasttrackstudio.cachix.org, for developers'
`nix develop`; no CI job depends on it. The REAPER integration suites are not in
CI (they need a licensed REAPER): run `just reaper-test` and `just reaper
daw-test` locally before a change to the REAPER adapters lands.

`PR checks`, `Local CI` and `Release guard` are the required checks (the
`main` ruleset). `CI passed` is the aggregate of a full run: it requires
`Gates`, `Check`, `Wasm` and `Tests`, and `just release-tag` and the tag
guards require it green on the release commit. Advisory (red or green,
never blocking): clippy. `Local CI` is as honest as whoever posts it — anyone who
can push could post one — which is the bargain (2026-10): the full set
after every merge catches what a local run missed. `main` may be red for
code reasons; nothing is tagged until `CI passed` is green.

## What the guards check

**Release guard** (`release.sh guard`, from the base commit's copy of the
script; any head other than `release/pending` in this repo passes):
the head is ONE commit `chore(release): vX.Y.Z` directly on the **current**
head of its base (`main` or `release/v*`); `X.Y.Z` is the computed next
version and untagged; the commit changes only the `[workspace.package]`
version line, the workspace members' version lines in `Cargo.lock`, adds
the `CHANGELOG.md` section and deletes consumed change files; no change
file is left pending; the iOS app crate resolves to the version. A network
error fails it. `just release-merge` re-runs it right before merging, so a
release PR cut before another merge can't ship that merge unlisted.

**Tag guard** (`release.sh tag-guard`, first step of `deploy`, `release`
publish and `reaper` on a `v*` tag; `ios-resolve` in the iOS workflows):
the tag is `v` + the workspace version, the iOS app crate is on it, it has a
changelog section, its commit is on `main` or a `release/v*` line, and
(where the job has a token) `CI passed` is green on it.

## The iOS app: TestFlight is a release

Studio (`apps/studio/desktop`, App Store record "Studio - by
FastTrackStudio", `app.fasttrackstudio.studio`) is the one iOS app:
Signal and Session behind one front screen. Signal's and Session's own
records (`app.fasttrackstudio.signal`, `.session`) get no new builds.

- **TestFlight uploads only from the release tag**, never on a push, a PR
  or a fork. The tag starts `studio-ios`.
- **Marketing version (CFBundleShortVersionString) = the tag's version**
  (`v26.10.3` → `26.10.3`), and the app crate must resolve to it. The
  build number (CFBundleVersion) is the unix time of the upload.
- **One GitHub Release** per tag (title `studio 26.10.3`, the changelog
  section as notes). After the upload the workflow writes its marketing
  version, build number and run URL into the Release notes; a re-run
  editing at the same time re-reads and retries.
- `studio-ios` refuses a tag that is not
  `^v[0-9]+\.[0-9]+\.[0-9]+$`, a crate or workspace version that differs, a
  missing changelog section, a commit on neither `main` nor `release/v*`, or
  a red `CI passed`. `workflow_dispatch` with `dry_run` stops after
  those checks.

Verify a release: App Store Connect's build for each bundle id has the
Release's version and the build number written in its notes
(`ruby apps/<app>/desktop/ios/testflight-status.rb app.fasttrackstudio.<app>`
on airlock).

## Hotfix

1. Fix on `main` first (a `fix(scope): …` PR with its change file).
2. If everything on `main` since the last tag is shippable: cut a normal
   release.
3. Otherwise `just release-hotfix <merged sha>…`: it creates
   `release/v<latest tag>` from the tag if missing, cherry-picks the fixes
   (and their change files) with `-x` onto
   `fix/no-issue-hotfix-<sha>`, and opens a PR into that line (full set).
   Merge it, then `just release-prepare --hotfix`, `release-pr`,
   `release-merge`, `release-tag`. The version is the next free number.
4. `release-tag` then opens `chore/no-issue-forward-port-v<version>` into
   `main`: the hotfix's changelog section, and the consumed change files
   deleted there. `main`'s next section starts "Includes all changes up to
   v<hotfix>." `release/*` is never merged back.

## Rollback

- **Sites**: re-run `deploy` on the previous tag
  (`gh workflow run deploy.yml --ref v26.10.2`), or point
  argocd-image-updater at the previous release's image tag. Nothing in
  studio's sites holds data, so an image swap is a full rollback. A
  rollback is a production deploy: get the Captain's yes/no first.
- **TestFlight**: expire the build in App Store Connect (per app); the
  previous build stays installable. To ship the previous code again, fix
  forward and release a new version; a tag is never moved or reused.
