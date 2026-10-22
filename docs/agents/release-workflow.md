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
  sites, both iOS apps (Signal and Session, one TestFlight upload each),
  the REAPER suites and the one GitHub Release. **Production is the
  latest `v*` tag**; a push to `main` ships nothing.

```
feat/sta-1-x ─PR (fast checks, change file)─squash─▶ main ─release/pending PR─▶ tag v26.10.3
fix/sta-2-y  ─┘                                       │ full set after every merge     │
                                                      │      sites . Signal + Session TestFlight
        hotfix: release/v26.10.3 (from the tag) ◀── cherry-pick -x    . REAPER . GitHub Release
                └─ release/pending PR ─▶ tag v26.10.4 ─▶ forward-port PR (changelog) into main
```

## One version for Signal and Session

Signal and Session are the same build, and a change to one almost always
affects the other, so they are one release: one `[workspace.package]
version`, one tag, one GitHub Release, one `CHANGELOG.md` section. Both app
crates (`apps/signal/desktop`, `apps/session/desktop`) say
`version.workspace = true`. Change-file notes end with `(signal)` or
`(session)` so the shared changelog says which app changed.

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
just release-tag             # tag the merge commit once its `Build + test` is green: THIS ships

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
| PR into `main` | `checks` (`Build + test`: the rail-selected set + `cargo rail change check`), `pr-title`, `release` guard (passes at once) | Mac Pro (`studio`, `studio-lint`, `studio-wasm`, `studio-tests`), hosted |
| release PR `release/pending` → `main` | `release` guard; `checks` runs the same guard from the base commit and **skips** the heavy jobs if it passes, fails `Build + test` if not | hosted + `studio` |
| PR into `release/**` (hotfix) | `checks` (the full set), `pr-title`, `release` guard | as above |
| push to `main` / `release/**` | `checks` full set (+ rail evidence on `main`) | Mac Pro |
| nightly | `checks` full set on `main` | Mac Pro |
| push of tag `vYY.M.MICRO` (or `workflow_dispatch` with the tag as ref) | `deploy` (session-web, signal-web), `signal-ios` **and** `session-ios`, `reaper-integration`, `release` publish | `studio-deploy`, `airlock`, `studio`, hosted |

`Build + test` is the one required check (the `main` ruleset). It is an
aggregator: it requires the `lint` job and, when the plan gave it work, the
web-bundle wasm check. Advisory (red or green, never blocking): clippy, the
Signal browser remote wasm check and the `Advisory: tests` nextest run, red
today (STA-184); when STA-184 makes them green they join the required set.
`main` may be red for code reasons; nothing is tagged until it is green.

## What the guards check

**Release guard** (`release.sh guard`, from the base commit's copy of the
script; any head other than `release/pending` in this repo passes):
the head is ONE commit `chore(release): vX.Y.Z` directly on the **current**
head of its base (`main` or `release/v*`); `X.Y.Z` is the computed next
version and untagged; the commit changes only the `[workspace.package]`
version line, the workspace members' version lines in `Cargo.lock`, adds
the `CHANGELOG.md` section and deletes consumed change files; no change
file is left pending; both iOS crates resolve to the version. A network
error fails it. `just release-merge` re-runs it right before merging, so a
release PR cut before another merge can't ship that merge unlisted.

**Tag guard** (`release.sh tag-guard`, first step of `deploy`, `release`
publish and `reaper` on a `v*` tag; `ios-resolve` in the iOS workflows):
the tag is `v` + the workspace version, both iOS crates are on it, it has a
changelog section, its commit is on `main` or a `release/v*` line, and
(where the job has a token) `Build + test` is green on it.

## iOS apps: TestFlight is a release

- **TestFlight uploads only from the release tag**, never on a push, a PR
  or a fork. One tag starts **both** `signal-ios` and `session-ios`.
- **Marketing version (CFBundleShortVersionString) = the tag's version**
  (`v26.10.3` → `26.10.3`) for both apps, and each app crate must resolve
  to it. The build number (CFBundleVersion) is the unix time of the upload.
- **One GitHub Release** per tag (title `studio 26.10.3`, the changelog
  section as notes). After each upload that app's workflow writes its
  marketing version, build number and run URL into the Release notes, one
  line per app; the two edit concurrently, so each re-reads and retries.
- `signal-ios` / `session-ios` refuse a tag that is not
  `^v[0-9]+\.[0-9]+\.[0-9]+$`, a crate or workspace version that differs, a
  missing changelog section, a commit on neither `main` nor `release/v*`, or
  a red `Build + test`. `workflow_dispatch` with `dry_run` stops after
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
