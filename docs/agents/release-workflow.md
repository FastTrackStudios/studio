# Releases: dev first, main is production, release branches

studio has two long-lived branches:

- **`dev`**: the default branch and the integration branch. Everything
  lands here: the Captain's pushes, and agents' PRs (squash-merged).
  **Dev never freezes**, not even during a release.
- **`main`**: production. It only receives release merges from
  `release/vX.Y.Z` branches (cut from dev) and `hotfix/*`. A push to main
  deploys the sites and tags the release `vX.Y.Z`. Signal and Session are
  uploaded to TestFlight **from that tag**, not from the push (see "iOS
  apps: TestFlight is a release" below).

**studio is one version.** Signal and Session are the same build, and a change
to one almost always affects the other, so they are one release: one
`[workspace.package] version`, one `vX.Y.Z` tag, one GitHub Release, one
`CHANGELOG.md` section. There are no per-app tags, branches or changelogs. The
version the repo is on is the version both apps are on.

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
| push to `main` (the release merge) | `deploy` (session-web, signal-web), `release` publish (tag `v<version>` + the GitHub Release, then dispatches `reaper-integration`, `signal-ios` and `session-ios` on the tag), `cache-devshell` (flake changes only) | `session`, GitHub-hosted |
| tag `vX.Y.Z` (or `workflow_dispatch` with the `tag`) | `signal-ios` **and** `session-ios`, both on the same tag: resolve (hosted) -> build + upload to TestFlight (`airlock`) -> record the build on the one GitHub Release (hosted). The only things that upload TestFlight. | GitHub-hosted, `airlock` |
| by hand | any of them: `gh workflow run <file> --ref <branch>` | |

The gate for work going into `dev` is local: at least
`cargo check --workspace --all-targets` in the dev shell on the commit
being pushed, or the full `just ci` (exactly what `checks.yml` runs, in
the same order). An agent's PR into dev carries that evidence in its
description: the head sha, `origin/dev`'s sha, the result and the host.

## iOS apps: TestFlight is a release

Signal (`app.fasttrackstudio.signal`) and Session (`app.fasttrackstudio.session`,
the iPad/iPhone app) are released **together, at one version**, with the same
branch -> PR -> merge commit -> tag protocol as the rest of the repo:

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
- Versions start at **0.1.0** and only the **patch** moves (0.1.9 -> 0.1.10, never
  rolling over, compared with `sort -V`). `minor` / `major` / an exact `X.Y.Z` only
  when the Captain or Commander calls it.

```bash
just release-plan                   # next version, the unreleased changelog
just release-prepare                # cut release/v<next>: workspace version, Cargo.lock,
                                    #   CHANGELOG.md section, `chore(release): v<next>`
just release-pr                     # push it, open it -> main
# merge with "Create a merge commit"; release.yml tags v<next> on the branch tip,
# publishes the GitHub Release, and dispatches reaper, signal-ios and session-ios on the tag
just release-backmerge              # backmerge/v<next> -> dev
```

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
| iOS apps | `apps/signal/desktop` or `apps/session/desktop` resolves to a version other than the release's (they take the workspace version) |
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
under a release that never shipped it. One `CHANGELOG.md` section covers
Signal and Session (and their macOS and Linux installers) together; there are
no per-app changelogs. cliff.toml's `tag_pattern` counts only `vX.Y.Z` tags.
(The plugins' tags are not covered yet.)

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
- **TestFlight**: expire the build in App Store Connect (per app); the previous
  build stays installable. To ship the previous version again, cut a new
  patch release; a tag is never moved or reused.
- **The workflow itself**: set the default branch back to `main` and
  restore `pull_request:` (no branch filter) in `checks.yml`; `dev` can
  stay.

## Branches

The Captain's feature branches (`editing-engine`, ...) are cut from and
merged into `dev`. A branch that was cut from main before the switch and
is not behind it fast-forwards onto dev unchanged
(`git push origin <branch>:dev` after a local check).
