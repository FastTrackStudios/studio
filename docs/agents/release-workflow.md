# Releases: dev first, main is production, release branches

studio has two long-lived branches:

- **`dev`**: the default branch and the integration branch. Everything
  lands here: the Captain's pushes, and agents' PRs (squash-merged).
  **Dev never freezes**, not even during a release.
- **`main`**: production. It only receives release merges from
  `release/vX.Y.Z` branches (cut from dev) and `hotfix/*`. A push to main
  deploys the sites, uploads the iOS apps to TestFlight and tags the
  release.

A release is a short-lived branch, `release/vX.Y.Z`, cut from dev's head.
CI is spent once per release, not once per push.

```
PRs --squash--> dev --cut--> release/vX.Y.Z --PR, merge commit--> main --> deploy + TestFlight + tag vX.Y.Z
                 ^                ^ fix PRs (squash) + release-refresh                 |
                 +----------- back-merge main -> dev (merge commit) <------------------+
```

## What runs where

| Event | Workflows | Runner |
|---|---|---|
| PR into `dev` (or into an open `release/v*`) | `pr-title` (a regex on the title, seconds) | GitHub-hosted |
| push to `dev` | nothing | none |
| release PR `release/vX.Y.Z` -> `main` (or `hotfix/*` -> `main`) | `checks` (the full `just ci` set + advisory clippy), `release` guard | THEBATTLESHIP (`session`), guard GitHub-hosted |
| push to `main` (the release merge) | `deploy` (session-web, signal-web), `session-ios` / `signal-ios` (path-filtered), `release` publish (tag `v<version>` + GitHub Release, then dispatches `reaper-integration` on the tag), `cache-devshell` (flake changes only) | `session`, `airlock`, GitHub-hosted |
| by hand | any of them: `gh workflow run <file> --ref <branch>` | |

The gate for work going into `dev` is local: at least
`cargo check --workspace --all-targets` in the dev shell on the commit
being pushed, or the full `just ci` (exactly what `checks.yml` runs, in
the same order). An agent's PR into dev carries that evidence in its
description: the head sha, `origin/dev`'s sha, the result and the host.

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

It never compares with dev's head. Until `dev` exists on origin the guard
only prints a notice (the bootstrap PR that adds it).

## The changelog

Each release's section is generated over a git range, the commits since
the last `v*` tag (`git cliff --tag vX.Y.Z --strip all <last>..HEAD`), and
written above the older sections, which stay as released. Never regenerate
the whole file with `git cliff -o`: with release branches it files work
under a release that never shipped it. Per-app releases (Signal and
Session builds, the plugins, the macOS and Linux installers) carry their
own `<app>-v*` tags and are not the repo's `v*` tag; cliff.toml's
`tag_pattern` ignores them.

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
  build stays installable.
- **The workflow itself**: set the default branch back to `main` and
  restore `pull_request:` (no branch filter) in `checks.yml`; `dev` can
  stay.

## Branches

The Captain's feature branches (`editing-engine`, ...) are cut from and
merged into `dev`. A branch that was cut from main before the switch and
is not behind it fast-forwards onto dev unchanged
(`git push origin <branch>:dev` after a local check).
