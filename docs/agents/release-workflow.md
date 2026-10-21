# Releases: dev first, main is production, release branches

studio has two long-lived branches:

- **`dev`**: the default branch and the integration branch. Everything
  lands here: the Captain's pushes, and agents' PRs (squash-merged).
  **Dev never freezes**, not even during a release.
- **`main`**: production. It only receives release merges from
  `release/vX.Y.Z` / `release/<app>-vX.Y.Z` branches (cut from dev) and
  `hotfix/*`. A push to main deploys the sites and tags the release. It
  does **not** upload to TestFlight: that is only ever a per-app release
  tag (see "iOS apps: TestFlight is a release" below).

A release is a short-lived branch, `release/vX.Y.Z`, cut from dev's head.
CI is spent once per release, not once per push.

```
PRs --squash--> dev --cut--> release/vX.Y.Z --PR, merge commit--> main --> deploy + tag vX.Y.Z
                 ^                ^ fix PRs (squash) + release-refresh                 |
                 +----------- back-merge main -> dev (merge commit) <------------------+
```

## What runs where

| Event | Workflows | Runner |
|---|---|---|
| PR into `dev` (or into an open `release/v*`) | `pr-title` (a regex on the title, seconds) | GitHub-hosted |
| push to `dev` | nothing | none |
| release PR `release/vX.Y.Z` -> `main` (or `hotfix/*` -> `main`) | `checks` (the full `just ci` set + advisory clippy), `release` guard | THEBATTLESHIP (`session`), guard GitHub-hosted |
| push to `main` (the release merge) | `deploy` (session-web, signal-web), `release` publish (tag `v<version>` + GitHub Release, then dispatches `reaper-integration` on the tag; and, for a per-app release, tag `<app>-v<version>` + its GitHub Release, then dispatches `<app>-ios` on the tag), `cache-devshell` (flake changes only) | `session`, GitHub-hosted |
| tag `signal-vX.Y.Z` / `session-vX.Y.Z` (or `workflow_dispatch` with the `tag`) | `signal-ios` / `session-ios`: resolve (hosted) -> build + upload to TestFlight (`airlock`) -> record the build on the GitHub Release (hosted). The only thing that uploads TestFlight. | GitHub-hosted, `airlock` |
| by hand | any of them: `gh workflow run <file> --ref <branch>` | |

The gate for work going into `dev` is local: at least
`cargo check --workspace --all-targets` in the dev shell on the commit
being pushed, or the full `just ci` (exactly what `checks.yml` runs, in
the same order). An agent's PR into dev carries that evidence in its
description: the head sha, `origin/dev`'s sha, the result and the host.

## iOS apps: TestFlight is a release

Signal (`app.fasttrackstudio.signal`) and Session (`app.fasttrackstudio.session`,
the iPad/iPhone app) are released **per app**, at versions of their own, with the
same branch -> PR -> merge commit -> tag protocol as the repo's release:

- **TestFlight uploads only from a per-app release tag** (`signal-vX.Y.Z`,
  `session-vX.Y.Z`), never on a push to main, a path change, a PR or a fork.
- **The TestFlight marketing version (CFBundleShortVersionString) is the tag's
  version** (`signal-v0.1.3` -> `0.1.3`), and the app crate's `[package] version`
  (`apps/<app>/desktop/Cargo.toml`) must be that same version.
- **A GitHub Release is required** for the tag (title `Signal 0.1.3`, the app's
  changelog section as the notes). After the upload the workflow writes the
  marketing version and the build number (CFBundleVersion, the unix time of the
  upload, climbing across re-runs) into the release notes, so the release and the
  TestFlight build can be traced to each other. macOS and Linux installers attach
  to that same release.
- Versions start at **0.1.0** (the first release of an app is its baseline) and only
  the **patch** moves (0.1.9 -> 0.1.10, never rolling over, compared with `sort -V`).
  `minor` / `major` / an exact `X.Y.Z` only when the Captain or Commander calls it.

```bash
just release-plan signal            # next signal-v<version>, the unreleased changelog
just release-prepare signal         # cut release/signal-v<next>: crate version, Cargo.lock,
                                    #   apps/signal/CHANGELOG.md section, `chore(release): signal-v<next>`
just release-pr                     # push it, open it -> main
# merge with "Create a merge commit"; release.yml tags signal-v<next> on the branch tip,
# publishes the GitHub Release, and dispatches signal-ios on the tag
just release-backmerge              # backmerge/signal-v<next> -> dev
```

(`session` the same way.) An app's changelog lists the commits since its last
`<app>-v*` tag that touched its own tree (`apps/<app>`) or the shared `crates/` and
`features/`, without the other app's. A release with no such commit is refused.

What `<app>-ios` refuses (red job, nothing built): a tag that is not
`^<app>-v[0-9]+\.[0-9]+\.[0-9]+$`; a crate version that differs from the tag's; a
tag without its changelog section; a commit that is not in `main`. A
`workflow_dispatch` with `dry_run` runs those checks and stops before the build. A tag
pushed by hand starts the workflow too (it creates the Release if there is none).
`MARKETING_VER` has no default any more: `deploy-testflight.sh` refuses to run without
it.

Verify a release: App Store Connect's build for the bundle id has marketing version ==
the Release's tag version and the build number written in its notes
(`ruby apps/<app>/desktop/ios/testflight-status.rb app.fasttrackstudio.<app>` on airlock).

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

## Cutting a release

In the dev shell, on a clean checkout:

```bash
just release-plan          # next version, unreleased changelog, git deps moved
just release-prepare       # cut release/v<next> from origin/dev: bump, Cargo.lock, CHANGELOG.md section, release commit
just release-pr            # push release/v<next>, open it -> main with the notes as the body
```

- Versions: **patch by default**, and the patch never rolls over
  (0.1.9 -> 0.1.10 -> ... -> 0.1.86). `just release-prepare minor|major|X.Y.Z`
  only when the Captain or Commander calls it. Versions compare
  semantically (`sort -V`), never as strings.
- Dev keeps taking work while the release is open. Anything that lands
  after the cut ships in the next release.
- `release-prepare` refuses until the previous release has been
  back-merged into dev.

On the release PR, `checks` and the `Release guard` must be green. Merge
with **Create a merge commit**, never squash or rebase. After the merge,
`release` publishes `vX.Y.Z` on the release branch tip (the merge's second
parent, same tree) and starts the REAPER suites on the tag; check that
`deploy` went green (it only does once the sites serve the new sha). Then:

```bash
just release-backmerge     # backmerge/vX.Y.Z -> dev, merge commit, auto-merge
```

If the back-merge conflicts (usually Cargo.lock next to a version line),
merge dev into `backmerge/vX.Y.Z` and resolve there, never on main.

## What the guard checks (release.yml, on every PR into main)

| Check | Fails when |
|---|---|
| head ref | not `release/v<its Cargo.toml version>` and not `hotfix/*` (so `dev -> main` is refused) |
| cut point | the parent of the branch's first `chore(release): vX.Y.Z` commit is not in `origin/dev` |
| version | not X.Y.Z, not greater than main's, or the tag exists |
| changelog | CHANGELOG.md at the head has no `[X.Y.Z]` section |
| head commit | is not `chore(release): vX.Y.Z` (a fix landed after it: `just release-refresh`) |
| main in head | main has commits the branch lacks (back-merge first, or merge main into the branch and refresh) |
| tree identity | merging would not reproduce the head's tree |

For `release/<app>-vX.Y.Z` the same checks run against the app: the head ref is
`release/<app>-v<the crate's version>`, the crate version is X.Y.Z, greater than main's
(its first release may start from a pre-release such as `0.0.2-alpha`), the tag
`<app>-vX.Y.Z` is absent, `apps/<app>/CHANGELOG.md` has the section, and the release
commit is `chore(release): <app>-vX.Y.Z`. It must also leave the repo's own version and
the other app's version alone: one release, one tag.

It never compares with dev's head. Until `dev` exists on origin the guard
only prints a notice (the bootstrap PR that adds it).

## The changelog

Each release's section is generated over a git range, the commits since
the last `v*` tag (`git cliff --tag vX.Y.Z --strip all <last>..HEAD`), and
written above the older sections, which stay as released. Never regenerate
the whole file with `git cliff -o`: with release branches it files work
under a release that never shipped it. Per-app releases (Signal and
Session, and their macOS and Linux installers) carry their own `<app>-v*`
tags and changelogs (`apps/<app>/CHANGELOG.md`) and are not the repo's `v*`
tag; cliff.toml's `tag_pattern` ignores them, and the app's range is
`<app>-v<last>..HEAD` narrowed to the app's paths. (The plugins' tags are not
covered yet.)

## Fixes during a release

- A release branch is open: PR the fix **into `release/vX.Y.Z`**
  (squash, conventional title), then on that branch `just release-refresh`
  and push. The fix reaches dev with the back-merge.
- None open: a hotfix.

```bash
git switch -c hotfix/<name> origin/main
# commit the fix
just release-hotfix        # patch bump + changelog + release commit (refuses while release/v<next> is open)
just release-pr            # PR -> main: checks + guard; merge commit
just release-backmerge     # main -> dev
```

## Rollback

- **Sites**: `deploy` tags every image `sha-<short>`. Point
  argocd-image-updater back at the previous release's `sha-...` tag (or
  revert the release merge on main with a hotfix PR, which redeploys the
  old tree). Nothing in studio's sites holds data, so an image swap is a
  full rollback.
- **TestFlight**: expire the build in App Store Connect; the previous
  build stays installable. To ship the previous version again, cut a new
  patch release of the app; a tag is never moved or reused.
- **The workflow itself**: set the default branch back to `main` and
  restore `pull_request:` (no branch filter) in `checks.yml`; `dev` can
  stay.

## Branches

The Captain's feature branches (`editing-engine`, ...) are cut from and
merged into `dev`. A branch that was cut from main before the switch and
is not behind it fast-forwards onto dev unchanged
(`git push origin <branch>:dev` after a local check).
