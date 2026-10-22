# Releases: dev first, main is finished features, a tag is the release

studio has two long-lived branches and one rule for shipping:

- **`dev`**: the integration branch. Everything lands here: the Captain's
  pushes, and agents' PRs (squash-merged). Dev never freezes. It is *not*
  the repo's default branch: `main` is (see "PR base" below).
- **`main`**: the default branch, and **finished features**. Work gets here
  by a *promotion PR* from `dev` once it is verified. **A push to main ships nothing**: no
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
| PR opened against `main` from a card branch | `pr-base` retargets it to `dev` (seconds, one comment) | GitHub-hosted |
| PR into `dev` | `pr-title` (a regex on the title, seconds) | GitHub-hosted |
| push to `dev` | nothing | none |
| **promotion PR** `dev` -> `main` (or `hotfix/*` -> `main`) | `checks` (the full `just ci` set + advisory clippy; check name `Build + test`), `release` guard | THEBATTLESHIP (`session`), guard GitHub-hosted |
| **release PR** `release/vX.Y.Z` -> `main` | `release` guard; `checks` plans it as a release commit and **skips** `Build + test` (a skipped job passes the required check) | GitHub-hosted |
| push to `main` | nothing (`cache-devshell` for flake changes only) | none |
| push of tag `vX.Y.Z` (or `workflow_dispatch` with the tag as its ref) | `deploy` (session-web, signal-web), `signal-ios` **and** `session-ios` (resolve -> build + upload to TestFlight on `airlock` -> record the build on the Release), `reaper-integration`, `release` publish (the one GitHub Release) | `session`, `airlock`, GitHub-hosted |
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

## PR base: main is the default, PRs go to dev

`main` is the default branch, so `gh pr create` and the web UI propose `main`
as the base. GitHub has no setting for a different default PR base, so
`.github/workflows/pr-base.yml` (`pull_request_target`: opened, reopened,
edited) retargets any PR whose base is `main` onto `dev` and says why in a
one-line comment. It leaves alone PRs whose head is `dev` (the promotion PR),
`release/v*`, `hotfix/*` or `backmerge/*` in this repo; those are the release
PRs, opened by hand or by `scripts/release.sh`.

- **Agents and scripts still pass `--base dev` explicitly.** The workflow is a
  safety net, not the plan.
- It never checks out or runs PR code (it is privileged, and it works for
  forks), and it needs `pull-requests: write` only.
- `pull_request_target` runs the workflow file from the base branch, so
  it only works once it is on `main`. Until then, pass `--base dev`.
- The retarget is made with the workflow's `GITHUB_TOKEN`, which starts no
  further workflows: `pr-title` runs on the next push or title edit, not on
  the retarget itself.
- A PR into `main` from any other head name is retargeted to `dev`: a
  hotfix branch must be named `hotfix/<what>`, a release `release/vX.Y.Z`.

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
(`ruby scripts/apple/testflight-status.rb app.fasttrackstudio.<app>` on airlock).

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
- **The workflow itself**: `main` stays the default branch (the Captain's
  decision, STA-308), so there is no default-branch switch to undo. Restore
  the `on: push: branches: [main]` triggers in `deploy.yml` (one commit),
  delete `pr-base.yml` if PRs should stop moving to `dev`, and change the
  `main (release only)` ruleset only with the Captain's yes.

## Branches

The Captain's feature branches (`editing-engine`, ...) are cut from and
merged into `dev`. A branch that was cut from main before the switch and
is not behind it fast-forwards onto dev unchanged
(`git push origin <branch>:dev` after a local check).
